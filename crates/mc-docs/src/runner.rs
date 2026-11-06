//! Running a page's inputs through the engine: [`update`] writes the outputs into the page (and its images into the
//! page's `.files` folder), [`check`] compares a fresh run with the stored outputs.
//!
//! Every page runs in a fresh engine, its inputs in order in one session. Unlike a notebook run, an input that ends
//! in an error does not stop the page: the following inputs still run (a "Possible issues" group may show an error
//! and go on). Each input has [`LIMIT_SECS`] before it is stopped.
use crate::page::{image_path, Page, Problem};
use mc_bench::StopFlag;
use mc_engine::{CoreEngine, Origin};
use mc_kernel::{run_cell_numbered, CellEvent, Status};
use mc_nbfile::{Notebook, OutputKind, OutputLine};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// An input taking longer than this (in a release build) is reported by [`check`]: examples must be quick.
pub const SLOW_SECS: f64 = 1.0;
/// An input is stopped after this long.
pub const LIMIT_SECS: u64 = 30;

/// The run of one input.
#[derive(Clone, Debug)]
pub struct Ran {
    /// the cell's position in the notebook
    pub cell: usize,
    /// the output lines, image paths already rewritten to the page's `.files` folder
    pub lines: Vec<OutputLine>,
    /// the images the run made: (the path in the page's lines, the file the run wrote)
    pub images: Vec<(String, PathBuf)>,
    /// how long it took
    pub secs: f64,
}

/// The output lines of a cell's events (as a notebook run writes them).
fn lines_of(events: &[CellEvent], status: Status) -> Vec<OutputLine> {
    let mut out = vec![];
    let mut push = |kind, text: &str| out.extend(text.split('\n').map(|t| OutputLine { kind, text: t.to_string() }));
    for e in events {
        match e {
            CellEvent::Output(s) => push(OutputKind::Print, s),
            CellEvent::Value(s) => push(OutputKind::Value, s),
            CellEvent::Failure(s) => push(OutputKind::Message, s),
            CellEvent::Error(r) => push(OutputKind::Message, &r.rendered),
        }
    }
    if status == Status::Interrupted {
        push(OutputKind::Message, "interrupted");
    }
    out
}

/// The working folder of page `name`'s run: a fresh folder under the system's temporary folder, so that files a
/// page writes and reads by relative names (`export_data("points.csv", ...)`) neither litter the repository nor
/// depend on what an earlier run left behind.
pub fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mathcas-docs-run-{}", std::process::id())).join(name)
}

/// Remove page `name`'s working folder (after its images have been read).
fn clean(name: &str) {
    let _ = std::fs::remove_dir_all(scratch(name));
}

/// Run every code cell of `nb` (page `name`, in folder `dir`), writing each cell's new output into `nb` as it goes
/// (so later cells' error positions are those of the rewritten page). `progress` hears `(k, n)` before input k.
///
/// The run's current directory is the page's [`scratch`] folder, emptied first; the previous one is restored at
/// the end. (The current directory is the process's: pages run one at a time.)
pub fn run(name: &str, nb: &mut Notebook, dir: &Path, progress: &mut dyn FnMut(usize, usize)) -> Vec<Ran> {
    let work = scratch(name);
    clean(name);
    let _ = std::fs::create_dir_all(&work);
    let before = std::env::current_dir().ok();
    let moved = std::env::set_current_dir(&work).is_ok();
    let out = run_in(name, nb, dir, &work, progress);
    if let (true, Some(b)) = (moved, before) {
        let _ = std::env::set_current_dir(b);
    }
    out
}

fn run_in(
    name: &str,
    nb: &mut Notebook,
    dir: &Path,
    work: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> Vec<Ran> {
    let mut engine = CoreEngine::new();
    mc_kernel::front_end::install(&mut engine);
    let origin = Origin::new(format!("{name}.mcn"), Some(dir.to_path_buf()));
    let stop = StopFlag::new();
    let code: Vec<usize> = (0..nb.cells.len()).filter(|&i| nb.cells[i].is_code()).collect();
    let mut out = vec![];
    for (k, &i) in code.iter().enumerate() {
        progress(k + 1, code.len());
        let src = nb.cells[i].source().unwrap_or_default();
        let line = nb.first_lines()[i];
        stop.clear();
        let (done, watch) = mpsc::channel::<()>();
        let flag = stop.clone();
        let watchdog = std::thread::spawn(move || {
            if watch.recv_timeout(Duration::from_secs(LIMIT_SECS)) == Err(mpsc::RecvTimeoutError::Timeout) {
                flag.raise();
            }
        });
        let start = Instant::now();
        let mut events = vec![];
        let status = run_cell_numbered(&mut engine, &src, &origin, line, &stop, &mut |e| events.push(e));
        let secs = start.elapsed().as_secs_f64();
        let _ = done.send(());
        let _ = watchdog.join();
        let mut lines = lines_of(&events, status);
        // images: from wherever the plot put them to the page's folder, named by the input's number
        let mut images = vec![];
        let mut j = 0;
        for l in &mut lines {
            let Some(path) = image_path(l).map(str::to_string) else { continue };
            j += 1;
            let file = if j == 1 { format!("{name}-{}.png", k + 1) } else { format!("{name}-{}-{j}.png", k + 1) };
            let rel = format!("{name}.files/{file}");
            l.text = format!("image(\"{rel}\")");
            // a relative path is relative to the run's working folder (an image read back from a file the page
            // wrote), else to the page's folder
            let made = if Path::new(&path).is_absolute() {
                PathBuf::from(&path)
            } else if work.join(&path).exists() {
                work.join(&path)
            } else {
                dir.join(&path)
            };
            images.push((rel, made));
        }
        nb.cells[i].set_output((!lines.is_empty()).then(|| lines.clone()));
        out.push(Ran { cell: i, lines, images, secs });
    }
    out
}

/// The pixel size of a PNG file.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder.read_info().ok()?;
    let info = reader.info();
    Some((info.width, info.height))
}

/// What an update did.
#[derive(Clone, Debug, Default)]
pub struct Updated {
    /// the inputs run
    pub inputs: usize,
    /// whether the page's text changed
    pub changed: bool,
    /// the images written
    pub images: usize,
    /// images removed because no output names them any more
    pub removed: usize,
    /// the slowest input (seconds)
    pub slowest: f64,
}

/// Run page `name` in folder `dir` and write its outputs and images back.
pub fn update(name: &str, dir: &Path, progress: &mut dyn FnMut(usize, usize)) -> Result<Updated, String> {
    let path = dir.join(format!("{name}.mcn"));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut nb = Notebook::parse(&text).map_err(|e| format!("{name}.mcn:{}: {}", e.line, e.message))?;
    let ran = run(name, &mut nb, dir, progress);
    let result = write_back(name, dir, &path, &text, &nb, &ran);
    clean(name);
    result
}

/// The second half of [`update`]: copy the images into the page's `.files` folder and write the page.
fn write_back(name: &str, dir: &Path, path: &Path, text: &str, nb: &Notebook, ran: &[Ran]) -> Result<Updated, String> {
    let folder = dir.join(format!("{name}.files"));
    let mut done = Updated { inputs: ran.len(), ..Updated::default() };
    let mut keep = vec![];
    for r in ran {
        done.slowest = done.slowest.max(r.secs);
        for (rel, made) in &r.images {
            let target = dir.join(rel);
            std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
            let bytes = std::fs::read(made).map_err(|e| format!("{}: {e}", made.display()))?;
            if std::fs::read(&target).ok().as_deref() != Some(bytes.as_slice()) {
                std::fs::write(&target, &bytes).map_err(|e| format!("{}: {e}", target.display()))?;
            }
            done.images += 1;
            keep.push(target);
        }
    }
    if let Ok(entries) = std::fs::read_dir(&folder) {
        for e in entries.flatten() {
            if !keep.contains(&e.path()) && std::fs::remove_file(e.path()).is_ok() {
                done.removed += 1;
            }
        }
    }
    if keep.is_empty() {
        let _ = std::fs::remove_dir(&folder);
    }
    let new = nb.print();
    done.changed = new != text;
    mc_nbfile::write_atomic(path, &new).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(done)
}

/// The result of checking a page.
#[derive(Clone, Debug, Default)]
pub struct Checked {
    /// every problem: structure, catalogue, outputs
    pub problems: Vec<Problem>,
    /// the inputs run
    pub inputs: usize,
    /// the slowest input (seconds)
    pub slowest: f64,
}

/// Check page `name` in folder `dir`: its structure, the names it mentions (`known`: whether a function exists),
/// and that a fresh run gives its stored outputs.
pub fn check(name: &str, dir: &Path, known: &dyn Fn(&str) -> bool, progress: &mut dyn FnMut(usize, usize)) -> Checked {
    let path = dir.join(format!("{name}.mcn"));
    let mut checked = Checked::default();
    let problem = |line: usize, message: String| Problem { line, message };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            checked.problems.push(problem(0, format!("cannot read the page: {e}")));
            return checked;
        }
    };
    let (page, mut problems) = Page::read(name, &text);
    if !known(name) {
        problems.push(problem(0, format!("`{name}` is not a function of the catalogue")));
    }
    for s in &page.see_also {
        if !known(s) {
            problems.push(problem(0, format!("See also names `{s}`, which is not a function")));
        }
        if s == name {
            problems.push(problem(0, "See also names the page itself".into()));
        }
    }
    let Ok(stored) = Notebook::parse(&text) else {
        checked.problems = problems;
        return checked;
    };
    let mut fresh = stored.clone();
    let ran = run(name, &mut fresh, dir, progress);
    let marker = |i: usize| stored.first_lines()[i] as usize - 1;
    for r in &ran {
        checked.slowest = checked.slowest.max(r.secs);
        let old: Vec<OutputLine> = stored.cells[r.cell].output_lines().cloned().collect();
        if !crate::compare::same_output(&old, &r.lines) {
            let show = |ls: &[OutputLine]| {
                ls.iter().map(|l| format!("{} {}", l.kind.prefix(), l.text)).collect::<Vec<_>>().join(" / ")
            };
            problems.push(problem(
                marker(r.cell),
                format!("stored output [{}] differs from a fresh run [{}]", show(&old), show(&r.lines)),
            ));
        }
        for (rel, made) in &r.images {
            let stored_size = std::fs::read(dir.join(rel)).ok().and_then(|b| png_size(&b));
            let new_size = std::fs::read(made).ok().and_then(|b| png_size(&b));
            match (stored_size, new_size) {
                (None, _) => {
                    problems.push(problem(marker(r.cell), format!("the image `{rel}` is missing or not a PNG")))
                }
                (Some(a), Some(b)) if a != b => problems.push(problem(
                    marker(r.cell),
                    format!("the image `{rel}` is {}x{}, a fresh run makes {}x{}", a.0, a.1, b.0, b.1),
                )),
                _ => {}
            }
        }
        if !cfg!(debug_assertions) && r.secs > SLOW_SECS {
            problems.push(problem(marker(r.cell), format!("the input took {:.2} s (at most {SLOW_SECS} s)", r.secs)));
        }
    }
    clean(name);
    checked.inputs = ran.len();
    checked.problems = problems;
    checked
}
