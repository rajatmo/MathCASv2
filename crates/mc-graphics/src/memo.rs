//! The plots of the session, remembered: the graphics term (printed) and the graphic read from it, by the path of
//! the plot's PNG file. Plots evaluate to `image("path")`; `show` and the layout functions of
//! `mc-graphics-kernel` combine them through these memos, and layouts and insets draw remembered graphics without
//! reading their terms again.
use mc_api::{Store, View, T};
use std::collections::VecDeque;
use std::sync::Mutex;

/// The graphics terms of recent plots, printed, by the path of their PNG file.
static MEMO: Mutex<VecDeque<(String, String)>> = Mutex::new(VecDeque::new());
/// The most plots remembered, and the most text in all.
const MEMO_ENTRIES: usize = 256;
const MEMO_BYTES: usize = 24 << 20;

/// The longest printed term remembered: a plot prints its term with at most this many bytes plus one
/// (`Printer::with_limit`), so a larger one is not printed in full only to be dropped here.
pub const TEXT_LIMIT: usize = MEMO_BYTES / 4;

/// Remember the graphics term (printed) a plot's PNG file at `path` was drawn from.
pub fn remember(path: &str, text: String) {
    if text.len() > TEXT_LIMIT {
        return;
    }
    let Ok(mut m) = MEMO.lock() else { return };
    m.retain(|(p, _)| p != path);
    m.push_back((path.to_string(), text));
    let mut total: usize = m.iter().map(|e| e.1.len()).sum();
    while m.len() > MEMO_ENTRIES || total > MEMO_BYTES {
        match m.pop_front() {
            Some(e) => total -= e.1.len(),
            None => break,
        }
    }
}

/// The printed graphics term of the plot drawn to `path`, if it is remembered.
pub fn recall(path: &str) -> Option<String> {
    let m = MEMO.lock().ok()?;
    m.iter().rev().find(|(p, _)| p == path).map(|e| e.1.clone())
}

/// The graphics of recent plots read (heap-free), by the path of their PNG file: layouts and insets of plots draw
/// them without reading their terms again.
static GRAPHICS: Mutex<VecDeque<(String, std::sync::Arc<crate::model::Graphic>)>> = Mutex::new(VecDeque::new());
/// The most plots whose graphics are remembered.
const GRAPHICS_ENTRIES: usize = 32;
/// The most bytes (about) of remembered graphics.
const GRAPHICS_BYTES: usize = 48 << 20;

/// Remember the graphic a plot's PNG file at `path` was drawn from.
pub fn remember_graphic(path: &str, g: crate::model::Graphic) {
    remember_shared(path, std::sync::Arc::new(g));
}

/// [`remember_graphic`] of a graphic that is also kept elsewhere (the files of the plot drawn later,
/// [`crate::deferred`]).
pub fn remember_shared(path: &str, g: std::sync::Arc<crate::model::Graphic>) {
    // a surface of a million triangles is not kept, and the kept ones are bounded in sum (ten surface plots in a
    // session were 700 MB)
    if weigh(&g) > GRAPHICS_BYTES / 4 {
        return;
    }
    let Ok(mut m) = GRAPHICS.lock() else { return };
    m.retain(|(p, _)| p != path);
    m.push_back((path.to_string(), g));
    let mut total: usize = m.iter().map(|e| weigh(&e.1)).sum();
    while m.len() > GRAPHICS_ENTRIES || total > GRAPHICS_BYTES {
        match m.pop_front() {
            Some(e) => total -= weigh(&e.1),
            None => break,
        }
    }
}

/// The bytes of a graphic, about: its points and raster cells.
pub(crate) fn weigh(g: &crate::model::Graphic) -> usize {
    use crate::model::Shape;
    const PT: usize = std::mem::size_of::<crate::model::Pt>();
    g.items
        .iter()
        .map(|it| {
            std::mem::size_of::<crate::model::Item>()
                + match &it.shape {
                    Shape::Points(p) => p.len() * PT,
                    Shape::Line(rs) | Shape::Polygon(rs) | Shape::Arrow(rs) => rs.iter().map(|r| r.len() * PT + 32).sum::<usize>(),
                    Shape::Bezier(p) => p.len() * PT,
                    Shape::Raster { rgba, .. } => rgba.len(),
                    Shape::Inset { graphic, .. } => weigh(graphic),
                    _ => 128,
                }
        })
        .sum()
}

/// The graphic of the plot drawn to `path`, if it is remembered.
pub fn recall_graphic(path: &str) -> Option<std::sync::Arc<crate::model::Graphic>> {
    let m = GRAPHICS.lock().ok()?;
    m.iter().rev().find(|(p, _)| p == path).map(|e| e.1.clone())
}

/// The contents of plots read back from the memo (by the structural hash of the content, the first argument of the
/// `graphics` term) with the path of the plot's PNG: a picture that combines plots (`show`) finds by its parts the
/// plots they came from, and with them what those plots know (their series, for hovering).
static LINKS: Mutex<Option<Links>> = Mutex::new(None);
/// The most contents linked.
const LINK_ENTRIES: usize = 1024;

#[derive(Default)]
struct Links {
    order: VecDeque<u64>,
    path: std::collections::HashMap<u64, String>,
}

/// Link the content of the plot drawn to `path`, by its structural hash.
pub fn link(hash: u64, path: &str) {
    let Ok(mut m) = LINKS.lock() else { return };
    let l = m.get_or_insert_with(Links::default);
    if l.path.insert(hash, path.to_string()).is_none() {
        l.order.push_back(hash);
    }
    while l.order.len() > LINK_ENTRIES {
        if let Some(old) = l.order.pop_front() {
            l.path.remove(&old);
        }
    }
}

/// The plot each content hash is linked to ([`link`]), if any, in the order given.
pub fn linked(hashes: &[u64]) -> Vec<Option<String>> {
    let Ok(m) = LINKS.lock() else { return vec![None; hashes.len()] };
    hashes.iter().map(|k| m.as_ref().and_then(|l| l.path.get(k).cloned())).collect()
}

/// The path of an `image("path")` term.
pub fn image_path<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> Option<&'h str> {
    let View::App { head, args: &[p] } = h.view(t) else { return None };
    match (h.view(head), h.view(p)) {
        (View::Sym(s), View::Str(path)) if h.sym_name(s) == "image" => Some(path),
        _ => None,
    }
}

