//! The data sidecar of a plot: `plot-N.json` next to `plot-N.png`, with what a front end needs to map a pixel of
//! the PNG to data coordinates and to snap to the plotted points (hover read-outs in the notebook).
//!
//! Format, version 1 (one JSON object):
//!
//! ```text
//! {"version":1, "width":640, "height":400,
//!  "plot_area":[x0, y0, x1, y1],              PNG pixels at scale 1, y down: the frame's inner rectangle
//!  "x":{"min":a, "max":b, "scale":"linear"},   linear | log | signed_log (then also "c", "a", "k": below)
//!  "y":{"min":c, "max":d, "scale":"linear"},
//!  "series":[{"name":"sin", "color":"#rrggbb", "kind":"line" | "points",
//!             "points":[[x, y], ...]}]}        data coordinates, at most 20000 per entry (thinned evenly beyond)
//! ```
//!
//! `min` and `max` are data values at the left and right (bottom and top) edges of the plot area; a value `v` is
//! at pixel `x0 + (s(v) - s(min)) / (s(max) - s(min)) * (x1 - x0)` with `s` the axis's transform (`log10` for
//! `log`, `sign(v) log10(a + k |v|)` for `signed_log`, 0 at 0, with the threshold `c` and the constants `a`, `k`
//! given: `k = 10^n`, `n = max(0, ceil(-log10 c))`, `a = max(1, 4 n)`), and `y` likewise from the bottom edge `y1` upwards. Points
//! that are not finite are left out; a line broken at a pole or a gap is several entries with the same name and
//! colour.
//!
//! A picture that combines plots (`show`) carries the series of all of them ([`combined`]).
use crate::figure::{Axis, Scale};
use mc_core::{Heap, Store, View, T};
use mc_graphics::Layout;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::Mutex;

/// The most points one series entry carries.
pub(crate) const MAX_POINTS: usize = 20_000;

/// How a series is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    Line,
    Points,
}

/// One series entry: its name, colour, kind and points in data coordinates.
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) color: (u8, u8, u8),
    pub(crate) kind: Kind,
    pub(crate) points: Vec<(f64, f64)>,
}

/// A number as JSON: the shortest text that reads back to it (with an exponent when very large or small).
fn number(v: f64, out: &mut String) {
    if !v.is_finite() {
        out.push_str("null");
    } else if v == 0.0 || (1e-5..1e15).contains(&v.abs()) {
        let _ = write!(out, "{v}");
    } else {
        let _ = write!(out, "{v:e}");
    }
}

/// A string as JSON.
fn string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// One axis: its ends as data values and its scale.
fn axis(ax: Axis, (lo, hi): (f64, f64), out: &mut String) {
    out.push_str("{\"min\":");
    number(ax.from(lo), out);
    out.push_str(",\"max\":");
    number(ax.from(hi), out);
    match ax {
        Axis::Linear => out.push_str(",\"scale\":\"linear\"}"),
        Axis::Log => out.push_str(",\"scale\":\"log\"}"),
        Axis::SignedLog(c) => {
            out.push_str(",\"scale\":\"signed_log\",\"c\":");
            number(c, out);
            let (a, k) = Axis::signed_log_constants(c);
            out.push_str(",\"a\":");
            number(a, out);
            out.push_str(",\"k\":");
            number(k, out);
            out.push('}');
        }
    }
}

/// The points of an entry, thinned evenly to at most [`MAX_POINTS`] (the first and last kept).
fn thinned(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let n = points.len();
    if n <= MAX_POINTS {
        return points.to_vec();
    }
    (0..MAX_POINTS).map(|i| points[i * (n - 1) / (MAX_POINTS - 1)]).collect()
}

/// The sidecar's text for a page laid out as `layout`, whose plot coordinates are on the axes `scale`.
pub(crate) fn json(layout: &Layout, scale: Scale, entries: &[Entry]) -> String {
    let mut o = String::with_capacity(256 + entries.iter().map(|e| e.points.len().min(MAX_POINTS) * 24).sum::<usize>());
    let _ = write!(o, "{{\"version\":1,\"width\":{},\"height\":{},\"plot_area\":[", layout.width, layout.height);
    let [x, y, w, h] = layout.area;
    let _ = write!(o, "{x},{y},{},{}],\"x\":", x + w, y + h);
    axis(scale.x, layout.range[0], &mut o);
    o.push_str(",\"y\":");
    axis(scale.y, layout.range[1], &mut o);
    o.push_str(",\"series\":[");
    for (k, e) in entries.iter().enumerate() {
        if k > 0 {
            o.push(',');
        }
        o.push_str("{\"name\":");
        string(&e.name, &mut o);
        let (r, g, b) = e.color;
        let kind = if e.kind == Kind::Line { "line" } else { "points" };
        let _ = write!(o, ",\"color\":\"#{r:02x}{g:02x}{b:02x}\",\"kind\":\"{kind}\",\"points\":[");
        for (i, &(px, py)) in thinned(&e.points).iter().enumerate() {
            if i > 0 {
                o.push(',');
            }
            o.push('[');
            number(px, &mut o);
            o.push(',');
            number(py, &mut o);
            o.push(']');
        }
        o.push_str("]}");
    }
    o.push_str("]}\n");
    o
}

/// What the picture of a plot knows for hovering, by the path of its PNG: the axes and the series entries in plot
/// coordinates (the values on the axes: logarithms on log axes, as the graphic draws them).
struct Known {
    path: String,
    scale: Scale,
    marks: Marks,
    entries: Vec<Entry>,
}

/// How a plot marked its axes: what `show` works out again for the range of a combined picture
/// ([`crate::reaxis`]).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Marks {
    /// axes through the origin (else a frame)
    pub(crate) origin_axes: bool,
    /// ticks the plot was given (or date ticks), per axis: kept as they are
    pub(crate) own_ticks: [bool; 2],
    /// the grid lines asked for, per axis
    pub(crate) grid: [Ask; 2],
}

/// The grid lines asked for on one axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Ask {
    #[default]
    None,
    /// `automatic`: the plot's own for its range
    Auto,
    /// positions given
    Given,
}

/// The plots known for hovering, oldest first.
static KNOWN: Mutex<VecDeque<Known>> = Mutex::new(VecDeque::new());
/// The most plots known, and the most points in all.
const KNOWN_PLOTS: usize = 256;
const KNOWN_POINTS: usize = 4_000_000;

fn points_of(k: &Known) -> usize {
    k.entries.iter().map(|e| e.points.len()).sum()
}

/// Remember the series of the plot drawn to `path` (`entries` in data coordinates, on the axes `scale`), so a
/// picture that combines it with others (`show`) can carry them.
pub(crate) fn remember(path: &str, scale: Scale, marks: Marks, entries: &[Entry]) {
    let on_axes = |&(x, y): &(f64, f64)| Some((scale.x.to(x)?, scale.y.to(y)?));
    let entries: Vec<Entry> = entries
        .iter()
        .map(|e| Entry { name: e.name.clone(), color: e.color, kind: e.kind, points: thinned(&e.points).iter().filter_map(on_axes).collect() })
        .collect();
    let Ok(mut m) = KNOWN.lock() else { return };
    m.retain(|k| k.path != path);
    m.push_back(Known { path: path.to_string(), scale, marks, entries });
    let mut total: usize = m.iter().map(points_of).sum();
    while m.len() > KNOWN_PLOTS || total > KNOWN_POINTS {
        match m.pop_front() {
            Some(k) => total -= points_of(&k),
            None => break,
        }
    }
}

/// The axes and their marks of the plot drawn to `path`, if it is known.
pub(crate) fn marks_of(path: &str) -> Option<(Scale, Marks)> {
    let m = KNOWN.lock().ok()?;
    m.iter().rev().find(|k| k.path == path).map(|k| (k.scale, k.marks))
}

/// The parts of a graphic's content looked at for plots: lists, at most this deep and this many.
const PART_DEPTH: usize = 3;
const PARTS: usize = 4096;

/// The axes and series of a picture that combines plots (`show(p1, p2, ...)`, a plot with other graphics): the
/// entries of every plot whose content is a part of the graphic `g` (linked by [`mc_graphics::memo::link`] when
/// the plot was read back), in the order of the content. The axes are the first plot's, as `show` draws them (its
/// ticks first); another plot's points stay where the picture draws them, read on those axes. `None` when no part
/// is a known plot.
pub(crate) fn combined<'h>(h: &'h Heap, g: T<'h>) -> Option<(Scale, Vec<Entry>)> {
    let View::App { args, .. } = h.view(g) else { return None };
    let &content = args.first()?;
    let list = h.sym_term(h.sym(mc_api::names::LIST));
    // level by level, so a plot's own parts are not looked at once the plot is found
    let mut level = vec![content];
    let mut paths: Vec<String> = vec![];
    let mut seen = 0;
    for depth in 0..=PART_DEPTH {
        if level.is_empty() || seen >= PARTS {
            break;
        }
        level.truncate(PARTS - seen);
        seen += level.len();
        let hashes: Vec<u64> = level.iter().map(|&t| h.structural_hash(t)).collect();
        let found = mc_graphics::memo::linked(&hashes);
        let mut next = vec![];
        for (&t, p) in level.iter().zip(found) {
            match p {
                Some(p) => paths.push(p),
                None if depth < PART_DEPTH && h.packed(t).is_none() => {
                    if let View::App { head, args } = h.view(t) {
                        if head == list {
                            next.extend_from_slice(args);
                        }
                    }
                }
                None => {}
            }
        }
        level = next;
    }
    let m = KNOWN.lock().ok()?;
    let known: Vec<&Known> = paths.iter().filter_map(|p| m.iter().rev().find(|k| &k.path == p)).collect();
    let scale = known.first()?.scale;
    let data = |&(u, v): &(f64, f64)| (scale.x.from(u), scale.y.from(v));
    let entries = known
        .iter()
        .flat_map(|k| k.entries.iter())
        .map(|e| Entry { name: e.name.clone(), color: e.color, kind: e.kind, points: e.points.iter().map(data).filter(|p| p.0.is_finite() && p.1.is_finite()).collect() })
        .filter(|e| !e.points.is_empty())
        .collect();
    Some((scale, entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sidecar_is_json_of_the_layout_and_points() {
        let layout = Layout { width: 640.0, height: 400.0, area: [50.0, 10.0, 580.0, 360.0], range: [(-1.0, 2.0), (0.0, 3.0)] };
        let scale = Scale { x: Axis::Linear, y: Axis::Log };
        let e = Entry { name: "a \"b\"".into(), color: (1, 2, 255), kind: Kind::Line, points: vec![(0.5, 1e-7), (1.0, 2.5e20)] };
        let j = json(&layout, scale, &[e]);
        assert_eq!(
            j,
            "{\"version\":1,\"width\":640,\"height\":400,\"plot_area\":[50,10,630,370],\
             \"x\":{\"min\":-1,\"max\":2,\"scale\":\"linear\"},\"y\":{\"min\":1,\"max\":1000,\"scale\":\"log\"},\
             \"series\":[{\"name\":\"a \\\"b\\\"\",\"color\":\"#0102ff\",\"kind\":\"line\",\"points\":[[0.5,1e-7],[1,2.5e20]]}]}\n"
        );
        let long: Vec<(f64, f64)> = (0..50_001).map(|i| (i as f64, 0.0)).collect();
        let t = thinned(&long);
        assert_eq!(t.len(), MAX_POINTS);
        assert_eq!((t[0].0, t[MAX_POINTS - 1].0), (0.0, 50_000.0));
        let mut s = String::new();
        string("\n", &mut s);
        assert_eq!(s, "\"\\u000a\"");
    }
}
