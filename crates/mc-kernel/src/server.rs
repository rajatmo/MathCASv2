//! The protocol server: [`serve`] answers requests read from `input` until `shutdown` or the end of the input.
//!
//! Two threads: a reader parses request lines and handles `interrupt` at once (raising the stop flag of the
//! running evaluation, or marking a queued one cancelled); every other request goes, in order, to the thread that
//! owns the engine, which evaluates and writes all responses. Evaluations are therefore sequential, and a
//! `complete` sent during a long evaluation is answered after it.
use crate::cell::{run_cell, ErrorReport, Status};
use crate::protocol::{parse_request, Body, ModuleInfo, Request, Response, PROTOCOL};
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig, Origin};
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
                if let Request::Eval { id, .. } = &r {
                    lock(&shared).queued.push(id.clone());
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
        let millis = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
        self.send(id, cell, Body::Done { status, millis })
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
    let hello =
        Body::Hello { protocol: PROTOCOL, version: env!("CARGO_PKG_VERSION").to_string(), engine: engine.name() };
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
        match request {
            Request::Eval { cell, src, origin: file, line, .. } => {
                let cancelled = {
                    let mut ev = lock(&shared);
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
                let status = if cancelled {
                    Status::Interrupted
                } else {
                    let origin = origin(file.as_deref(), cell.as_deref());
                    let mut written = Ok(());
                    let status = run_cell(&mut engine, &src, &origin, line.unwrap_or(1).max(1), &stop, &mut |e| {
                        if written.is_ok() {
                            written = w.send(&id, &cell, e.into());
                        }
                    });
                    lock(&shared).running = None;
                    written?;
                    status
                };
                w.done(&id, &cell, status, start)?;
            }
            Request::Complete { prefix, .. } => {
                let names = engine.names().into_iter().filter(|n| n.starts_with(&prefix)).collect();
                w.send(&id, &None, Body::Completions { names })?;
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
                engine = CoreEngine::with_config(config);
                w.done(&id, &None, Status::Ok, start)?;
            }
            Request::Shutdown { .. } => {
                w.done(&id, &None, Status::Ok, start)?;
                break;
            }
            // handled by the reader
            Request::Interrupt { .. } => {}
        }
    }
    // after a shutdown the reader has stopped; at the end of the input it has too
    let _ = reader.join();
    Ok(())
}

impl From<crate::cell::CellEvent> for Body {
    fn from(e: crate::cell::CellEvent) -> Body {
        use crate::cell::CellEvent;
        match e {
            CellEvent::Output(text) => Body::Output { text },
            CellEvent::Value(printed) => Body::Value { printed },
            CellEvent::Failure(printed) => Body::Failure { printed },
            CellEvent::Error(r) => Body::Error(r),
        }
    }
}
