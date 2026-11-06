//! The protocol server: [`serve`] answers requests read from `input` until `shutdown` or the end of the input.
//!
//! Two threads: a reader parses request lines and handles `interrupt` at once (raising the stop flag of the
//! running evaluation, or marking a queued one cancelled); every other request goes, in order, to the thread that
//! owns the engine, which evaluates and writes all responses. Evaluations are therefore sequential, and a
//! `complete` sent during a long evaluation is answered after it. A `shutdown` with `interrupt` is also handled by
//! the reader: it interrupts the running evaluation and cancels the queued ones, so that quitting a busy kernel is
//! prompt.
use crate::cell::{run_cell, run_each, EachEvent, ErrorReport, Status};
use crate::protocol::{
    parse_request, Body, DisplayKind, ModuleInfo, NameInfo, Request, Response, Values, CAPABILITIES, PROTOCOL,
};
use mc_bench::{Engine, StopFlag};
use mc_engine::{guard, CoreEngine, EachOptions, EngineConfig, Origin, Report};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

/// What the reader hands to the engine thread.
enum Message {
    Request(Request),
    /// a line that is not a request: its id (if one could be found) and the reason
    Bad(Value, String),
}

/// Which evaluations an `interrupt` can reach. Shared by the reader and the engine thread.
#[derive(Default)]
struct Evaluations {
    /// the id of the running evaluation
    running: Option<Value>,
    /// ids of `eval` requests read but not started, in order
    queued: Vec<Value>,
    /// queued ids an `interrupt` cancelled
    cancelled: Vec<Value>,
}

type Shared = Arc<Mutex<Evaluations>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Evaluations> {
    // the critical sections cannot panic halfway; a poisoned lock still holds consistent data
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `interrupt {id}`: raise the stop flag if `id` is running (or no id is given and something runs), else cancel
/// the queued evaluation with that id; an unknown id is ignored.
fn interrupt(shared: &Shared, stop: &StopFlag, id: Option<Value>) {
    let mut ev = lock(shared);
    match id {
        None if ev.running.is_some() => stop.raise(),
        None => {}
        Some(id) if ev.running.as_ref() == Some(&id) => stop.raise(),
        Some(id) if ev.queued.contains(&id) => ev.cancelled.push(id),
        Some(_) => {}
    }
}

/// `shutdown` with `interrupt`: interrupt the running evaluation and cancel every queued one.
fn stop_all(shared: &Shared, stop: &StopFlag) {
    let mut ev = lock(shared);
    if ev.running.is_some() {
        stop.raise();
    }
    let queued = ev.queued.clone();
    ev.cancelled.extend(queued);
}

/// Read request lines until the end of the input or a `shutdown`.
fn read_requests(input: impl BufRead, tx: mpsc::Sender<Message>, shared: Shared, stop: StopFlag) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let message = match parse_request(&line) {
            Ok(Request::Interrupt { id }) => {
                interrupt(&shared, &stop, id);
                continue;
            }
            Ok(r) => {
                match &r {
                    Request::Eval { id, .. } => lock(&shared).queued.push(id.clone()),
                    Request::Shutdown { interrupt: true, .. } => stop_all(&shared, &stop),
                    _ => {}
                }
                Message::Request(r)
            }
            Err((id, why)) => Message::Bad(id, why),
        };
        let shutdown = matches!(message, Message::Request(Request::Shutdown { .. }));
        if tx.send(message).is_err() || shutdown {
            break;
        }
    }
    // dropping the sender ends the engine thread's loop once the requests read so far are answered
}

/// Writes response lines, flushing each so that the client sees output as it is produced.
struct Writer<'w> {
    out: &'w mut dyn Write,
}

impl Writer<'_> {
    fn send(&mut self, id: &Value, cell: &Option<String>, body: Body) -> io::Result<()> {
        let r = Response { id: id.clone(), cell: cell.clone(), body };
        writeln!(self.out, "{}", r.to_line())?;
        self.out.flush()
    }

    fn done(&mut self, id: &Value, cell: &Option<String>, status: Status, start: Instant) -> io::Result<()> {
        self.done_numbered(id, cell, status, start, None)
    }

    /// `done`, with the first number used and the line counter after a numbered evaluation.
    fn done_numbered(
        &mut self,
        id: &Value,
        cell: &Option<String>,
        status: Status,
        start: Instant,
        numbers: Option<(u64, u64)>,
    ) -> io::Result<()> {
        let millis = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
        let (first, next) = (numbers.map(|n| n.0), numbers.map(|n| n.1));
        self.send(id, cell, Body::Done { status, millis, first, next })
    }
}

/// The origin of an `eval`: the named file, else `<cell NAME>` or `<input>` in the current directory.
fn origin(origin: Option<&str>, cell: Option<&str>) -> Origin {
    match (origin, cell) {
        (Some(path), _) => Origin::file(Path::new(path)),
        (None, Some(c)) => Origin::new(format!("<cell {c}>"), Some(PathBuf::new())),
        (None, None) => Origin::input(),
    }
}

/// Serve one session: write `hello`, then answer the requests read from `input` on `output`, until `shutdown`
/// or the end of the input (the requests read before it are still answered). Returns an error only when
/// `output` cannot be written.
pub fn serve(input: impl BufRead + Send + 'static, output: &mut dyn Write, config: EngineConfig) -> io::Result<()> {
    let mut engine = CoreEngine::with_config(config);
    let mut w = Writer { out: output };
    let hello = Body::Hello {
        protocol: PROTOCOL,
        version: env!("CARGO_PKG_VERSION").to_string(),
        engine: engine.name(),
        capabilities: CAPABILITIES.iter().map(|c| c.to_string()).collect(),
    };
    w.send(&Value::Null, &None, hello)?;
    let shared = Shared::default();
    let stop = StopFlag::new();
    let (tx, rx) = mpsc::channel();
    let reader = {
        let (shared, stop) = (shared.clone(), stop.clone());
        std::thread::Builder::new()
            .name("mc-kernel-reader".into())
            .spawn(move || read_requests(input, tx, shared, stop))?
    };
    for message in rx {
        let start = Instant::now();
        let request = match message {
            Message::Request(r) => r,
            Message::Bad(id, why) => {
                w.send(&id, &None, Body::Error(ErrorReport::plain(why)))?;
                w.done(&id, &None, Status::Error, start)?;
                continue;
            }
        };
        let id = request.id();
        // the last line of defence: a panic outside an evaluation's own guard (in the collector, completion,
        // printing a report) resets the session instead of ending the kernel (package robust)
        let handled = guard::catch(|| handle(&mut engine, &mut w, &shared, &stop, config, request, &id, start));
        match handled {
            Ok(Ok(Flow::Next)) => {}
            Ok(Ok(Flow::Shutdown)) => break,
            Ok(Err(e)) => return Err(e),
            Err(note) => {
                lock(&shared).running = None;
                engine = CoreEngine::with_config(config);
                let why = format!("internal error: {}; the session was reset", note.text());
                w.send(&id, &None, Body::Error(ErrorReport::plain(why)))?;
                w.done(&id, &None, Status::Error, start)?;
            }
        }
    }
    // after a shutdown the reader has stopped; at the end of the input it has too
    let _ = reader.join();
    Ok(())
}

/// What the loop does after a request.
enum Flow {
    Next,
    Shutdown,
}

/// Answer one request (not `interrupt`, which the reader handles).
#[allow(clippy::too_many_arguments)]
fn handle(
    engine: &mut CoreEngine,
    w: &mut Writer<'_>,
    shared: &Shared,
    stop: &StopFlag,
    config: EngineConfig,
    request: Request,
    id: &Value,
    start: Instant,
) -> io::Result<Flow> {
    let id = id.clone();
    match request {
        Request::Eval { cell, src, origin: file, line, values, numbered, .. } => {
            let cancelled = {
                let mut ev = lock(shared);
                if let Some(i) = ev.queued.iter().position(|q| *q == id) {
                    ev.queued.remove(i);
                }
                match ev.cancelled.iter().position(|q| *q == id) {
                    Some(i) => {
                        ev.cancelled.remove(i);
                        true
                    }
                    None => {
                        // under the lock: an interrupt for this id from now on reaches this evaluation
                        stop.clear();
                        ev.running = Some(id.clone());
                        false
                    }
                }
            };
            let first = engine.line();
            let status = if cancelled {
                Status::Interrupted
            } else {
                let origin = origin(file.as_deref(), cell.as_deref());
                let line = line.unwrap_or(1).max(1);
                let each = values == Some(Values::Each);
                let mut written = Ok(());
                let mut send = |body: Body| {
                    if written.is_ok() {
                        written = w.send(&id, &cell, body);
                    }
                };
                let status = if each || numbered {
                    let options = EachOptions { numbered };
                    // numbered with `values: "last"`: only the value of the last statement, if it is shown
                    let mut last = None;
                    let status = run_each(engine, &src, &origin, line, stop, options, &mut |e| match e {
                        EachEvent::Report(r @ Report::Value { .. }) if !each => last = Some(r),
                        e => send(each_body(e)),
                    });
                    let next = engine.line();
                    if let Some(r @ Report::Value { n, .. }) = last {
                        if n == Some(next - 1) {
                            send(each_body(EachEvent::Report(r)));
                        }
                    }
                    status
                } else {
                    run_cell(engine, &src, &origin, line, stop, &mut |e| send(e.into()))
                };
                lock(shared).running = None;
                written?;
                status
            };
            let numbers = numbered.then(|| (first, engine.line()));
            w.done_numbered(&id, &cell, status, start, numbers)?;
        }
        Request::Complete { prefix, .. } => {
            let names = engine.names().into_iter().filter(|n| n.starts_with(&prefix)).collect();
            w.send(&id, &None, Body::Completions { names })?;
            w.done(&id, &None, Status::Ok, start)?;
        }
        Request::Names { prefix, .. } => {
            let names = engine
                .name_kinds()
                .into_iter()
                .filter(|(n, _)| n.starts_with(&prefix))
                .map(|(name, kind)| NameInfo { name, kind: kind.as_str().to_string() })
                .collect();
            w.send(&id, &None, Body::Names { names })?;
            w.done(&id, &None, Status::Ok, start)?;
        }
        Request::Modules { .. } => {
            let modules = engine
                .modules()
                .map(|m| ModuleInfo {
                    name: m.name.clone(),
                    origin: m.origin.clone(),
                    public: m.public.iter().cloned().collect(),
                })
                .collect();
            w.send(&id, &None, Body::Modules { modules })?;
            w.done(&id, &None, Status::Ok, start)?;
        }
        Request::Reset { .. } => {
            *engine = CoreEngine::with_config(config);
            w.done(&id, &None, Status::Ok, start)?;
        }
        Request::Shutdown { .. } => {
            w.done(&id, &None, Status::Ok, start)?;
            return Ok(Flow::Shutdown);
        }
        // handled by the reader
        Request::Interrupt { .. } => {}
    }
    Ok(Flow::Next)
}

impl From<crate::cell::CellEvent> for Body {
    fn from(e: crate::cell::CellEvent) -> Body {
        use crate::cell::CellEvent;
        match e {
            CellEvent::Output(text) => Body::Output { text, n: None },
            CellEvent::Value(printed) => Body::Value { printed, n: None },
            CellEvent::Failure(printed) => Body::Failure { printed, n: None },
            CellEvent::Error(r) => Body::Error(r),
        }
    }
}

/// The SVG file an image value `image("path")` names and its size in CSS pixels; `None` when the value is not an
/// image value, the file is not an SVG drawing, or it cannot be read.
fn svg_of(printed: &str) -> Option<(String, u32, u32)> {
    use mc_nbfile::images::{image_path, ImageFormat};
    let path = image_path(printed)?;
    if ImageFormat::of(Path::new(&path))? != ImageFormat::Svg {
        return None;
    }
    let (w, h) = mc_nbfile::images::picture_size(Path::new(&path))?;
    Some((path, w, h))
}

/// The response for an event of a statement-by-statement evaluation: a value naming a readable PNG file becomes
/// a `display` (`kind: "image"`), one naming an SVG file a `display` with `kind: "svg"`.
fn each_body(e: EachEvent) -> Body {
    match e {
        EachEvent::Report(Report::Output { n, text }) => Body::Output { text, n },
        EachEvent::Report(Report::Message { n, tag, text }) => Body::Message { text, tag, n },
        EachEvent::Report(Report::Failure { n, printed }) => Body::Failure { printed, n },
        EachEvent::Report(Report::Value { n, printed, image: None }) => match svg_of(&printed) {
            Some((path, width, height)) => Body::Display { kind: DisplayKind::Svg, path, width, height, printed, n },
            None => Body::Value { printed, n },
        },
        EachEvent::Report(Report::Value { n, printed, image: Some(i) }) => {
            Body::Display { kind: DisplayKind::Image, path: i.path, width: i.width, height: i.height, printed, n }
        }
        EachEvent::Error(r) => Body::Error(r),
    }
}

