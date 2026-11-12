//! Running a page's inputs through the engine: [`update`] writes the outputs into the page (and its images into the
//! page's `.files` folder), [`check`] compares a fresh run with the stored outputs.
//!
//! Every page runs in a fresh engine, its inputs in order in one session. Unlike a notebook run, an input that ends
//! in an error does not stop the page: the following inputs still run (a "Possible issues" group may show an error
//! and go on). Each input has [`LIMIT_SECS`] before it is stopped.
use crate::page::{image_path, Page, Problem};
use mc_api::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Origin};
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
    /// the pictures the run made: (the path in the page's lines, the file the run wrote: the plot's PNG, whose SVG
    /// drawing is beside it when the picture is a drawing; the path ends in `.svgz` for a drawing, else `.png`)
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

/// The images of a page are copied after its engine is gone: the plot files of this process are kept until it exits
/// (the engines would remove them when the last one ends, R23-P1).
static PLOT_FILES: std::sync::OnceLock<mc_plot::SessionGuard> = std::sync::OnceLock::new();

fn run_in(
    name: &str,
    nb: &mut Notebook,
    dir: &Path,
    work: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> Vec<Ran> {
    PLOT_FILES.get_or_init(mc_plot::session_guard);
    // the same display mode as the kernel and the REPL, so pages print `-27 - 36*x^2`, not `-27 + -36*x^2`
    let mut engine = CoreEngine::with_config(EngineConfig { display: true, ..EngineConfig::default() });
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
            // a relative path is relative to the run's working folder (an image read back from a file the page
            // wrote), else to the page's folder
            let made = if Path::new(&path).is_absolute() {
                PathBuf::from(&path)
            } else if work.join(&path).exists() {
                work.join(&path)
            } else {
                dir.join(&path)
            };
            // a plot is stored as its drawing; an image that is not one (read back from a file) stays a PNG
            let ext = if drawing_of(&made).is_some() { "svgz" } else { "png" };
            let file = if j == 1 { format!("{name}-{}.{ext}", k + 1) } else { format!("{name}-{}-{j}.{ext}", k + 1) };
            let rel = format!("{name}.files/{file}");
            l.text = format!("image(\"{rel}\")");
            images.push((rel, made));
        }
        nb.cells[i].set_output((!lines.is_empty()).then(|| lines.clone()));
        out.push(Ran { cell: i, lines, images, secs });
    }
    out
}

/// The SVG drawing of the picture a run wrote at `made`: its SVG sibling (the plot's PNG is written beside it), or
/// the file itself when it is one; `None` for an image that is not a drawing.
fn drawing_of(made: &Path) -> Option<PathBuf> {
    let svg = if made.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) { made.to_path_buf() } else { made.with_extension("svg") };
    svg.is_file().then_some(svg)
}

/// The pixel size of a PNG file, from its header (the signature and the IHDR chunk).
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || bytes[..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    Some((be(&bytes[16..20]), be(&bytes[20..24])))
}

/// The size of a stored picture file `rel` read as `bytes`: a `.svgz` drawing's from its root element, a PNG's from
/// its header.
fn stored_size(rel: &str, bytes: &[u8]) -> Option<(u32, u32)> {
    if rel.ends_with(".svgz") {
        let svg = crate::library::gunzip(bytes)?;
        mc_nbfile::images::svg_size(&String::from_utf8_lossy(&svg))
    } else {
        png_size(bytes)
    }
}

/// What an update did.
#[derive(Clone, Debug, Default)]
pub struct Updated {
    /// the inputs run
    pub inputs: usize,
    /// whether the page's text changed
    pub changed: bool,
    /// the pictures written
    pub images: usize,
    /// of them, the ones kept as PNG files (images that are not drawings)
    pub rasters: usize,
    /// pictures removed because no output names them any more
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

/// `data` gzip-compressed, without a time stamp (the same bytes for the same drawing on every run).
fn gzip(data: &[u8]) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut enc = flate2::GzBuilder::new().mtime(0).write(Vec::new(), flate2::Compression::best());
    enc.write_all(data).ok()?;
    enc.finish().ok()
}

/// The second half of [`update`]: write the pictures into the page's `.files` folder and write the page.
fn write_back(name: &str, dir: &Path, path: &Path, text: &str, nb: &Notebook, ran: &[Ran]) -> Result<Updated, String> {
    let folder = dir.join(format!("{name}.files"));
    let mut done = Updated { inputs: ran.len(), ..Updated::default() };
    let mut keep = vec![];
    for r in ran {
        done.slowest = done.slowest.max(r.secs);
        for (rel, made) in &r.images {
            let target = dir.join(rel);
            std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
            // a plot's drawing, compressed (the same bytes for the same drawing); any other image as it is
            let bytes = if rel.ends_with(".svgz") {
                let svg = drawing_of(made).ok_or_else(|| format!("{}: no drawing", made.display()))?;
                let svg = std::fs::read(&svg).map_err(|e| format!("{}: {e}", svg.display()))?;
                gzip(&svg).ok_or_else(|| format!("{}: cannot compress", made.display()))?
            } else {
                done.rasters += 1;
                std::fs::read(made).map_err(|e| format!("{}: {e}", made.display()))?
            };
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
    compare_run(name, dir, &stored, progress, &mut problems, &mut checked);
    checked.problems = problems;
    checked
}

/// Run `stored` afresh (page or guide `name` in `dir`) and add a problem for every output, image or time that
/// differs; `checked` gets the number of inputs and the slowest.
fn compare_run(
    name: &str,
    dir: &Path,
    stored: &Notebook,
    progress: &mut dyn FnMut(usize, usize),
    problems: &mut Vec<Problem>,
    checked: &mut Checked,
) {
    let problem = |line: usize, message: String| Problem { line, message };
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
            let stored_size = std::fs::read(dir.join(rel)).ok().and_then(|b| stored_size(rel, &b));
            let new_size = if rel.ends_with(".svgz") {
                drawing_of(made).and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| mc_nbfile::images::svg_size(&t))
            } else {
                std::fs::read(made).ok().and_then(|b| png_size(&b))
            };
            match (stored_size, new_size) {
                (None, _) => {
                    problems.push(problem(marker(r.cell), format!("the image `{rel}` is missing or not a picture")))
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
}

/// Check guide `stem` in folder `dir` (a topic guide of `docs/guides/`): its structure ([`crate::guide::Guide`]),
/// that its related functions exist (`known`) and that a fresh run gives its stored outputs.
pub fn check_guide(stem: &str, dir: &Path, known: &dyn Fn(&str) -> bool, progress: &mut dyn FnMut(usize, usize)) -> Checked {
    let path = dir.join(format!("{stem}.mcn"));
    let mut checked = Checked::default();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            checked.problems.push(Problem { line: 0, message: format!("cannot read the guide: {e}") });
            return checked;
        }
    };
    let (_, mut problems) = crate::guide::Guide::read(stem, &text, known);
    let Ok(stored) = Notebook::parse(&text) else {
        checked.problems = problems;
        return checked;
    };
    compare_run(stem, dir, &stored, progress, &mut problems, &mut checked);
    checked.problems = problems;
    checked
}
