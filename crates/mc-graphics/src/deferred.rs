//! Files of a plot that are drawn when they are first needed.
//!
//! A plot is shown from its PNG and SVG files; its PDF and the print twins (the figure laid out on a page of
//! publication size) are only read when the plot is exported to a file (`mc-io`). Drawing them with every plot cost
//! more than the picture itself (a 3D surface's PDF takes longer than its PNG and SVG together), so the plot leaves
//! them here with the graphic they are drawn from, and [`ensure`] draws and writes them the first time a reader
//! asks. When the front end says which files it reads (the kernel protocol's `pictures`), all of a plot's files can
//! wait, its PNG and the texts beside it ([`defer_all`]) included, and [`ensure_only`] writes the ones a reader
//! names. At most [`MAX_PENDING`] plots wait, and at most about [`MAX_BYTES`] of graphics: past that the oldest are
//! written then, so nothing is lost; [`forget`] drops the files of a plot that was removed.
use crate::{model::Graphic, render_model, Format};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// One file to draw later: the extension it gets next to the PNG (`pdf`, `print.svg`, ...), the page size it is
/// laid out on (`None`: the graphic's own) and its format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Later {
    /// the extension of the file next to the PNG (`png` for the PNG itself)
    pub ext: &'static str,
    /// the page size in points the figure is laid out on, if not its own (a side `None`: worked out from the other)
    pub size: Option<(Option<f64>, Option<f64>)>,
    /// the format
    pub format: Format,
}

/// A plot whose files wait.
struct Waiting {
    /// the PNG's path (the other files share its name)
    png: PathBuf,
    /// the graphic the files are drawn from
    g: Arc<Graphic>,
    /// the files to draw
    later: Vec<Later>,
    /// the files already made (texts beside the picture), by extension
    ready: Vec<(&'static str, Vec<u8>)>,
    /// about how many bytes the graphic takes ([`crate::memo::weigh`], worked out once)
    bytes: usize,
}

impl Waiting {
    /// Whether the PNG itself waits here.
    fn holds_png(&self) -> bool {
        self.later.iter().any(|l| l.ext == "png")
    }

    /// About how many bytes the entry alone keeps: a graphic the memo of plots also holds
    /// ([`crate::memo::remember_shared`], bounded there) is not counted while it does.
    fn weight(&self) -> usize {
        let graphic = if Arc::strong_count(&self.g) > 1 { 0 } else { self.bytes };
        graphic + self.ready.iter().map(|r| r.1.len()).sum::<usize>()
    }
}

/// Plots whose files wait, oldest first.
static PENDING: Mutex<VecDeque<Waiting>> = Mutex::new(VecDeque::new());
/// The most plots that wait.
pub const MAX_PENDING: usize = 1024;
/// The most bytes (about) of graphics that wait.
pub const MAX_BYTES: usize = 64 << 20;

fn pending() -> std::sync::MutexGuard<'static, VecDeque<Waiting>> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Leave the files `later` of the plot whose PNG is at `png` to be drawn from `g` when first needed.
pub fn defer(png: &Path, g: Arc<Graphic>, later: Vec<Later>) {
    defer_all(png, g, later, vec![]);
}

/// Leave the files `later` of the plot whose PNG is at `png` to be drawn from `g`, and the files `ready` (made
/// already, by extension) to be written, when first needed. `later` may hold the PNG itself (extension `png`):
/// then nothing of the plot is on disk until a reader asks.
pub fn defer_all(png: &Path, g: Arc<Graphic>, later: Vec<Later>, ready: Vec<(&'static str, Vec<u8>)>) {
    if later.is_empty() && ready.is_empty() {
        return;
    }
    let overflow: Vec<_> = {
        let mut p = pending();
        p.retain(|w| w.png != png);
        let bytes = crate::memo::weigh(&g);
        p.push_back(Waiting { png: png.to_path_buf(), g, later, ready, bytes });
        let mut out = vec![];
        let mut total: usize = p.iter().map(Waiting::weight).sum();
        while p.len() > MAX_PENDING || (total > MAX_BYTES && p.len() > 1) {
            match p.pop_front() {
                Some(e) => {
                    total = total.saturating_sub(e.weight());
                    out.push(e);
                }
                None => break,
            }
        }
        out
    };
    for w in overflow {
        write(&w);
    }
}

/// Draw and write the waiting files of the plot whose PNG is at `png`, if any wait (a reader of those files calls
/// this first).
pub fn ensure(png: &Path) {
    let entry = {
        let mut p = pending();
        p.iter().position(|w| w.png == png).and_then(|i| p.remove(i))
    };
    if let Some(w) = entry {
        write(&w);
    }
}

/// Draw and write those waiting files of the plot whose PNG is at `png` whose extensions are in `exts` (`png`,
/// `svg`, `json`, ...); the others go on waiting.
pub fn ensure_only(png: &Path, exts: &[&str]) {
    let wanted = |e: &str| exts.contains(&e);
    let now = {
        let mut p = pending();
        let Some(i) = p.iter().position(|w| w.png == png) else { return };
        let w = &mut p[i];
        if !w.later.iter().any(|l| wanted(l.ext)) && !w.ready.iter().any(|r| wanted(r.0)) {
            return;
        }
        let (later_now, later_rest): (Vec<Later>, Vec<Later>) = w.later.iter().copied().partition(|l| wanted(l.ext));
        let (ready_now, ready_rest): (Vec<_>, Vec<_>) = std::mem::take(&mut w.ready).into_iter().partition(|r| wanted(r.0));
        // the PNG goes on waiting: the files written now may be written although it is not on disk yet
        let png_waits = later_rest.iter().any(|l| l.ext == "png");
        w.later = later_rest;
        w.ready = ready_rest;
        let now = Waiting { png: w.png.clone(), g: w.g.clone(), later: later_now, ready: ready_now, bytes: w.bytes };
        if w.later.is_empty() && w.ready.is_empty() {
            p.remove(i);
        }
        (now, png_waits)
    };
    let (w, png_waits) = now;
    write_files(&w, png_waits);
}

/// Drop the waiting files of the plot whose PNG is at `png` (its files were removed).
pub fn forget(png: &Path) {
    pending().retain(|w| w.png != png);
}

/// Drop the waiting files of every plot (a kernel's `reset`: the session that made them is gone); the number of
/// plots whose files were dropped.
pub fn forget_all() -> usize {
    let mut p = pending();
    let n = p.len();
    p.clear();
    n
}

/// Whether files of the plot whose PNG is at `png` wait.
pub fn is_pending(png: &Path) -> bool {
    pending().iter().any(|w| w.png == png)
}

/// Whether the PNG of the plot at `png` itself waits (it is not on disk yet, and is written when a reader asks).
pub fn png_is_pending(png: &Path) -> bool {
    pending().iter().any(|w| w.png == png && w.holds_png())
}

fn write(w: &Waiting) {
    write_files(w, false);
}

/// Write the files of `w`; `png_waits`: the PNG is not on disk because it waits in the queue.
fn write_files(w: &Waiting, png_waits: bool) {
    // the PNG is gone (its session ended, or a sweep): nothing to draw for
    if !w.holds_png() && !png_waits && !w.png.is_file() {
        return;
    }
    for (ext, bytes) in &w.ready {
        let _ = std::fs::write(w.png.with_extension(ext), bytes);
    }
    // the PNG last: once it exists, so do the files written with it
    let order = w.later.iter().filter(|l| l.ext != "png").chain(w.later.iter().filter(|l| l.ext == "png"));
    for l in order {
        let made = match l.size {
            Some(size) => {
                let mut sized = (*w.g).clone();
                sized.opts.image_size = size;
                render_model(&sized, &[l.format], &|| false)
            }
            None => render_model(&w.g, &[l.format], &|| false),
        };
        if let Ok(mut files) = made {
            let _ = std::fs::write(w.png.with_extension(l.ext), files.remove(0));
        }
    }
}
