//! Files of a plot that are drawn when they are first needed.
//!
//! A plot is shown from its PNG and SVG files; its PDF and the print twins (the figure laid out on a page of
//! publication size) are only read when the plot is exported to a file (`mc-io`). Drawing them with every plot cost
//! more than the picture itself (a 3D surface's PDF takes longer than its PNG and SVG together), so the plot leaves
//! them here with the graphic they are drawn from, and [`ensure`] draws and writes them the first time a reader
//! asks. At most [`MAX_PENDING`] plots wait, and at most about [`MAX_BYTES`] of graphics: past that the oldest are
//! written then, so nothing is lost; [`forget`] drops the files of a plot that was removed.
use crate::{model::Graphic, render_model, Format};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// One file to draw later: the extension it gets next to the PNG (`pdf`, `print.svg`, ...), the page size it is
/// laid out on (`None`: the graphic's own) and its format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Later {
    /// the extension of the file next to the PNG
    pub ext: &'static str,
    /// the page size in points the figure is laid out on, if not its own
    pub size: Option<(f64, f64)>,
    /// the format
    pub format: Format,
}

/// A plot whose files wait: its PNG's path, the graphic and the files.
type Waiting = (PathBuf, Arc<Graphic>, Vec<Later>);

/// Plots whose files wait, oldest first.
static PENDING: Mutex<VecDeque<Waiting>> = Mutex::new(VecDeque::new());
/// The most plots that wait.
pub const MAX_PENDING: usize = 64;
/// The most bytes (about) of graphics that wait.
pub const MAX_BYTES: usize = 64 << 20;

fn pending() -> std::sync::MutexGuard<'static, VecDeque<Waiting>> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Leave the files `later` of the plot whose PNG is at `png` to be drawn from `g` when first needed.
pub fn defer(png: &Path, g: Arc<Graphic>, later: Vec<Later>) {
    if later.is_empty() {
        return;
    }
    let overflow: Vec<_> = {
        let mut p = pending();
        p.retain(|(q, ..)| q != png);
        p.push_back((png.to_path_buf(), g, later));
        let mut out = vec![];
        let mut total: usize = p.iter().map(|e| crate::memo::weigh(&e.1)).sum();
        while p.len() > MAX_PENDING || (total > MAX_BYTES && p.len() > 1) {
            match p.pop_front() {
                Some(e) => {
                    total -= crate::memo::weigh(&e.1);
                    out.push(e);
                }
                None => break,
            }
        }
        out
    };
    for (png, g, later) in overflow {
        write(&png, &g, &later);
    }
}

/// Draw and write the waiting files of the plot whose PNG is at `png`, if any wait (a reader of those files calls
/// this first).
pub fn ensure(png: &Path) {
    let entry = {
        let mut p = pending();
        p.iter().position(|(q, ..)| q == png).and_then(|i| p.remove(i))
    };
    if let Some((png, g, later)) = entry {
        write(&png, &g, &later);
    }
}

/// Drop the waiting files of the plot whose PNG is at `png` (its files were removed).
pub fn forget(png: &Path) {
    pending().retain(|(q, ..)| q != png);
}

/// Whether files of the plot whose PNG is at `png` wait.
pub fn is_pending(png: &Path) -> bool {
    pending().iter().any(|(q, ..)| q == png)
}

fn write(png: &Path, g: &Graphic, later: &[Later]) {
    // the PNG is gone (its session ended, or a sweep): nothing to draw for
    if !png.is_file() {
        return;
    }
    for l in later {
        let made = match l.size {
            Some((w, h)) => {
                let mut sized = g.clone();
                sized.opts.image_size = (Some(w), Some(h));
                render_model(&sized, &[l.format], &|| false)
            }
            None => render_model(g, &[l.format], &|| false),
        };
        if let Ok(mut files) = made {
            let _ = std::fs::write(png.with_extension(l.ext), files.remove(0));
        }
    }
}
