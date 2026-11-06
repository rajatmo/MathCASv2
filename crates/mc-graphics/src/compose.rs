//! Pictures made of pictures: `graphics_row`, `graphics_column`, `graphics_grid`, `grid`, `row` (of graphics), `framed`, `labeled` and `legended` evaluate to one `graphics` term that holds the parts as insets
//! (so the result shows, exports and combines like any graphic), and `show` combines plots.
//!
//! Plots evaluate to `image("path")`; the graphics term each plot was drawn from is remembered for the session
//! ([`remember`]), so plots can be combined with `show` and laid out with the functions above.
use crate::label::{typeset, Label};
use crate::num::number;
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use std::collections::VecDeque;
use std::sync::Mutex;

/// The graphics terms of recent plots, printed, by the path of their PNG file.
static MEMO: Mutex<VecDeque<(String, String)>> = Mutex::new(VecDeque::new());
/// The most plots remembered, and the most text in all.
const MEMO_ENTRIES: usize = 256;
const MEMO_BYTES: usize = 64 << 20;

/// Remember the graphics term (printed) a plot's PNG file at `path` was drawn from.
pub fn remember(path: &str, text: String) {
    if text.len() > MEMO_BYTES / 4 {
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

/// Remember the graphic a plot's PNG file at `path` was drawn from.
pub fn remember_graphic(path: &str, g: crate::model::Graphic) {
    let Ok(mut m) = GRAPHICS.lock() else { return };
    m.retain(|(p, _)| p != path);
    m.push_back((path.to_string(), std::sync::Arc::new(g)));
    while m.len() > GRAPHICS_ENTRIES {
        m.pop_front();
    }
}

/// The graphic of the plot drawn to `path`, if it is remembered.
pub fn recall_graphic(path: &str) -> Option<std::sync::Arc<crate::model::Graphic>> {
    let m = GRAPHICS.lock().ok()?;
    m.iter().rev().find(|(p, _)| p == path).map(|e| e.1.clone())
}

/// The path of an `image("path")` term.
pub fn image_path<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> Option<&'h str> {
    let View::App { head, args: &[p] } = h.view(t) else { return None };
    match (h.view(head), h.view(p)) {
        (View::Sym(s), View::Str(path)) if h.sym_name(s) == "image" => Some(path),
        _ => None,
    }
}

/// Register the natives.
pub fn install(k: &mut Kernel, h: &Heap) {
    for (name, kind) in [
        ("graphics_row", Kind::Row),
        ("graphics_column", Kind::Column),
        ("graphics_grid", Kind::Grid),
        ("grid", Kind::Table),
        ("row", Kind::Row),
        ("framed", Kind::Framed),
        ("labeled", Kind::Labeled),
        ("legended", Kind::Legended),
    ] {
        k.register(h.sym(name), Attributes::default(), Box::new(Compose { name, kind }));
    }
}

/// `image("path")` of a remembered plot as its graphics term (evaluated); anything else as it is.
pub fn resolve<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let View::App { head, args: &[p] } = h.view(t) else { return Ok(t) };
    let (View::Sym(s), View::Str(path)) = (h.view(head), h.view(p)) else { return Ok(t) };
    if h.sym_name(s) != "image" {
        return Ok(t);
    }
    let Some(text) = recall(path) else { return Ok(t) };
    match mc_syntax::read(h, &text) {
        Ok(v) if v.len() == 1 => cx.eval(v[0]),
        _ => Ok(t),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Row,
    Column,
    Grid,
    Table,
    Framed,
    Labeled,
    Legended,
}

struct Compose {
    name: &'static str,
    kind: Kind,
}

/// A part of a layout: a graphic with its natural page size, or an expression shown as a label.
enum Cell<'h> {
    Picture(T<'h>, (f64, f64)),
    Label(T<'h>, (f64, f64)),
}

/// Text size of labels in layouts.
const TEXT: f64 = 13.0;
/// Space between parts.
const GAP: f64 = 10.0;
/// The widest a layout of pictures is made, in points (wider ones are scaled down).
const MAX_W: f64 = 960.0;

impl<'h> Cell<'h> {
    fn size(&self) -> (f64, f64) {
        match self {
            Cell::Picture(_, s) | Cell::Label(_, s) => *s,
        }
    }
}

fn cell<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Cell<'h>> {
    let h = cx.heap();
    // a plot whose graphic is remembered: drawn from it (its term is not read again)
    if let Some(g) = image_path(h, t).and_then(recall_graphic) {
        if let Ok(p) = crate::page::plan(&g, &|| false) {
            return Ok(Cell::Picture(t, (p.width as f64, p.height as f64)));
        }
    }
    // column([a, b, ...]): laid out as a one-column grid
    if let View::App { head, args: &[items] } = h.view(t) {
        if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "column") {
            if let Some(items) = cx.as_list(items) {
                let rows: Vec<T<'h>> = items.iter().map(|&e| h.call(h.sym(names::LIST), &[e])).collect();
                let g = h.call(h.sym("grid"), &[h.call(h.sym(names::LIST), &rows)]);
                let g = cx.eval(g)?;
                if crate::is_graphics(h, g) {
                    return cell(cx, g);
                }
            }
        }
    }
    let t = resolve(cx, t)?;
    if crate::is_graphics(h, t) {
        if let Ok(s) = crate::page_size(h, t) {
            return Ok(Cell::Picture(t, (s.0 as f64, s.1 as f64)));
        }
    }
    // measured at its own font size when styled with one, as it is drawn
    let ts = typeset(&Label::of(h, t), TEXT as f32);
    Ok(Cell::Label(t, (ts.width as f64, ts.height as f64)))
}

fn is_options(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::App { head, .. } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options"))
}

/// The value of option `key` in an options term.
fn option<'h>(h: &'h Heap, o: Option<T<'h>>, key: &str) -> Option<T<'h>> {
    let View::App { args, .. } = h.view(o?) else { return None };
    args.iter().find_map(|&r| match h.view(r) {
        View::App { args: &[k, v], .. } if matches!(h.view(k), View::Sym(s) if h.sym_name(s) == key) => Some(v),
        _ => None,
    })
}

fn sym_is(h: &Heap, t: T<'_>, name: &str) -> bool {
    matches!(h.view(t), View::Sym(s) if h.sym_name(s) == name)
}

/// Builds terms.
struct B<'h> {
    h: &'h Heap,
}

impl<'h> B<'h> {
    fn call(&self, f: &str, a: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(f), a)
    }
    fn list(&self, a: &[T<'h>]) -> T<'h> {
        self.call(names::LIST, a)
    }
    fn sym(&self, s: &str) -> T<'h> {
        self.h.sym_term(self.h.sym(s))
    }
    fn rule(&self, k: &str, v: T<'h>) -> T<'h> {
        self.call(names::RULE, &[self.sym(k), v])
    }
    fn pt(&self, x: f64, y: f64) -> T<'h> {
        self.h.reals(&[x, y])
    }
    /// A part at the rectangle (x, y) .. (x + w, y + h) of the page (y up).
    fn place(&self, c: &Cell<'h>, x: f64, y: f64, w: f64, ht: f64) -> T<'h> {
        let mid = self.pt(x + w / 2.0, y + ht / 2.0);
        match c {
            Cell::Picture(t, _) => self.call("inset", &[*t, mid, self.pt(0.0, 0.0), self.h.real(w)]),
            Cell::Label(t, _) => {
                let mut v = vec![self.call("font_size", &[self.h.real(TEXT)])];
                // `style(x, ..., colour, ...)`: drawn in that colour (the label reads the size itself)
                if let View::App { head, args } = self.h.view(*t) {
                    if sym_is(self.h, head, "style") && args.len() > 1 {
                        v.extend(args[1..].iter().find_map(|&a| colour_in(self.h, a)));
                    }
                }
                v.push(self.call("text", &[*t, mid]));
                self.list(&v)
            }
        }
    }
    /// The graphic of `content` on a page `w` x `ht` points, one plot unit a point.
    fn page(&self, content: Vec<T<'h>>, w: f64, ht: f64, background: Option<T<'h>>) -> T<'h> {
        let mut o = vec![
            self.rule("plot_range", self.list(&[self.h.reals(&[0.0, w]), self.h.reals(&[0.0, ht])])),
            self.rule("plot_range_padding", self.sym("none")),
            self.rule("aspect_ratio", self.sym("automatic")),
            self.rule("image_size", self.h.real(w)),
            self.rule("image_padding", self.sym("none")),
            self.rule("plot_range_clipping", self.sym("false")),
        ];
        if let Some(b) = background {
            o.push(self.rule("background", b));
        }
        self.call("graphics", &[self.list(&content), self.call("options", &o)])
    }
    fn rect_line(&self, x0: f64, y0: f64, x1: f64, y1: f64, color: T<'h>) -> T<'h> {
        let ring = vec![self.pt(x0, y0), self.pt(x1, y0), self.pt(x1, y1), self.pt(x0, y1), self.pt(x0, y0)];
        self.list(&[color, self.call("absolute_thickness", &[self.h.real(0.75)]), self.call("line", &[self.list(&ring)])])
    }
}

impl Native for Compose {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (args, opts) = match args.split_last() {
            Some((&o, rest)) if is_options(h, o) => (rest, Some(o)),
            _ => (args, None),
        };
        let b = B { h };
        let gray = || crate::natives::rgb_term(h, crate::color::Rgba::rgb(0.45, 0.45, 0.45));
        match self.kind {
            Kind::Row | Kind::Column | Kind::Grid | Kind::Table => {
                // the parts as rows of cells
                let Some(&first) = args.first() else { return Ok(Reply::Decline) };
                let Some(items) = cx.as_list(first) else { return Ok(Reply::Decline) };
                let rows: Vec<Vec<T<'h>>> = match self.kind {
                    Kind::Row => vec![items.to_vec()],
                    Kind::Column => items.iter().map(|&e| vec![e]).collect(),
                    _ => items.iter().map(|&r| cx.as_list(r).map(|l| l.to_vec()).unwrap_or_else(|| vec![r])).collect(),
                };
                if rows.iter().all(|r| r.is_empty()) {
                    return Ok(Reply::Decline);
                }
                let mut cells: Vec<Vec<Cell<'h>>> = Vec::with_capacity(rows.len());
                let mut pictures = false;
                for r in rows {
                    let mut out = Vec::with_capacity(r.len());
                    for e in r {
                        cx.tick(1)?;
                        let c = cell(cx, e)?;
                        pictures |= matches!(c, Cell::Picture(..));
                        out.push(c);
                    }
                    cells.push(out);
                }
                // `row` of expressions alone stays as it is (a text layout)
                if !pictures && self.name == "row" {
                    return Ok(Reply::Decline);
                }
                let ncol = cells.iter().map(|r| r.len()).max().unwrap_or(0);
                let mut widths = vec![0.0f64; ncol];
                let mut heights = vec![0.0f64; cells.len()];
                for (i, r) in cells.iter().enumerate() {
                    for (j, c) in r.iter().enumerate() {
                        let (w, ht) = c.size();
                        widths[j] = widths[j].max(w);
                        heights[i] = heights[i].max(ht);
                    }
                }
                let pad = if pictures { GAP } else { 6.0 };
                // frame: true (around), all (every cell)
                let frame = option(h, opts, "frame").or_else(|| option(h, opts, "dividers"));
                // the margin around the parts: text without a frame is fitted closely (a framed column inset)
                let edge = if pictures || frame.is_some() { pad } else { 2.0 };
                let total_w: f64 = widths.iter().sum::<f64>() + pad * (ncol as f64 - 1.0) + 2.0 * edge;
                let scale = if pictures && total_w > MAX_W { MAX_W / total_w } else { 1.0 };
                let (widths, heights): (Vec<f64>, Vec<f64>) =
                    (widths.iter().map(|w| w * scale).collect(), heights.iter().map(|x| x * scale).collect());
                let w = widths.iter().sum::<f64>() + pad * (ncol as f64 - 1.0) + 2.0 * edge;
                let ht = heights.iter().sum::<f64>() + pad * (heights.len() as f64 - 1.0) + 2.0 * edge;
                let mut content = vec![];
                let mut y = ht - edge;
                for (i, r) in cells.iter().enumerate() {
                    let mut x = edge;
                    for (j, c) in r.iter().enumerate() {
                        let (cw, ch) = (c.size().0 * scale, c.size().1 * scale);
                        // centred in its slot
                        let (sx, sy) = (x + (widths[j] - cw) / 2.0, y - heights[i] + (heights[i] - ch) / 2.0);
                        content.push(b.place(c, sx, sy, cw, ch));
                        x += widths[j] + pad;
                    }
                    y -= heights[i] + pad;
                }
                if let Some(f) = frame {
                    if sym_is(h, f, "all") {
                        let mut x = pad / 2.0;
                        let xs: Vec<f64> = std::iter::once(x).chain(widths.iter().map(|w| { x += w + pad; x })).collect();
                        let mut y = ht - pad / 2.0;
                        let ys: Vec<f64> = std::iter::once(y).chain(heights.iter().map(|hh| { y -= hh + pad; y })).collect();
                        let mut segs = vec![];
                        for &x in &xs {
                            segs.push(b.list(&[b.pt(x, ys[ys.len() - 1]), b.pt(x, ys[0])]));
                        }
                        for &y in &ys {
                            segs.push(b.list(&[b.pt(xs[0], y), b.pt(xs[xs.len() - 1], y)]));
                        }
                        content.push(b.list(&[gray(), b.call("absolute_thickness", &[h.real(0.75)]), b.call("line", &[b.list(&segs)])]));
                    } else if cx.is_true(f) {
                        content.push(b.rect_line(pad / 2.0, pad / 2.0, w - pad / 2.0, ht - pad / 2.0, gray()));
                    }
                }
                Ok(Reply::Again(b.page(content, w, ht, option(h, opts, "background"))))
            }
            Kind::Framed => {
                let [x] = args else { return Ok(Reply::Decline) };
                let c = cell(cx, *x)?;
                let (cw, ch) = c.size();
                let pad = if matches!(c, Cell::Picture(..)) { 4.0 } else { 5.0 };
                let (w, ht) = (cw + 2.0 * pad, ch + 2.0 * pad);
                let mut content = vec![];
                if let Some(bg) = option(h, opts, "background") {
                    content.push(b.list(&[bg, b.call("rectangle", &[b.pt(0.0, 0.0), b.pt(w, ht)])]));
                }
                content.push(b.place(&c, pad, pad, cw, ch));
                let color = option(h, opts, "frame_style").filter(|&s| !matches!(crate::color::colour(h, s), crate::color::Colour::Bad)).unwrap_or_else(gray);
                content.push(b.rect_line(0.5, 0.5, w - 0.5, ht - 0.5, color));
                Ok(Reply::Again(b.page(content, w, ht, None)))
            }
            Kind::Labeled => {
                let (x, l, pos) = match args {
                    [x, l] => (*x, *l, None),
                    [x, l, p] => (*x, *l, Some(*p)),
                    _ => return Ok(Reply::Decline),
                };
                // a list labelled by a list: each part with its label
                if let (Some(xs), Some(ls)) = (cx.as_list(x), cx.as_list(l)) {
                    if xs.len() == ls.len() && !xs.is_empty() {
                        let parts: Vec<T<'h>> = xs.iter().zip(ls).map(|(&a, &m)| {
                            let mut v = vec![a, m];
                            v.extend(pos);
                            h.call(h.sym("labeled"), &v)
                        }).collect();
                        return Ok(Reply::Again(b.call("row", &[b.list(&parts)])));
                    }
                }
                let (c, lc) = (cell(cx, x)?, cell(cx, l)?);
                // only pictures and text are laid out with their labels: anything else (a function or data for a
                // plot, a value to label in a chart) stays labelled, for the plots that take it
                if !matches!(c, Cell::Picture(..)) && !matches!(h.view(x), View::Str(_)) {
                    return Ok(Reply::Decline);
                }
                let side = pos.and_then(|p| match h.view(p) {
                    View::Sym(s) => Some(h.sym_name(s)),
                    _ => None,
                });
                Ok(Reply::Again(labeled_layout(&b, &c, &lc, side.as_deref())))
            }
            Kind::Legended => {
                let [x, l] = args else { return Ok(Reply::Decline) };
                let c = cell(cx, *x)?;
                // only a picture gets a legend drawn: anything else (a function or data for a plot) stays as it is
                if !matches!(c, Cell::Picture(..)) {
                    return Ok(Reply::Decline);
                }
                let (colors, labels) = legend_entries(h, *l, &|t| cx.as_list(t));
                let entries: Vec<Cell<'h>> = labels.iter().map(|&e| cell(cx, e)).collect::<Res<_>>()?;
                Ok(Reply::Again(legended_layout(&b, &c, &colors, &entries)))
            }
        }
    }
}

/// The entries of a legend: a list of labels, or line_legend / swatch_legend / point_legend([colours], [labels]);
/// colours and labels.
fn legend_entries<'h>(h: &'h Heap, l: T<'h>, as_list: &dyn Fn(T<'h>) -> Option<&'h [T<'h>]>) -> (Vec<T<'h>>, Vec<T<'h>>) {
    match as_list(l) {
        Some(ls) => (vec![], ls.to_vec()),
        None => match h.view(l) {
            View::App { args: la, .. } => {
                let lists: Vec<&[T<'h>]> = la.iter().filter_map(|&a| as_list(a)).collect();
                match lists.as_slice() {
                    [cs, ls] => (cs.to_vec(), ls.to_vec()),
                    [ls] => (vec![], ls.to_vec()),
                    _ => (vec![], vec![l]),
                }
            }
            _ => (vec![], vec![l]),
        },
    }
}

/// `c` with its label `lc` beside it (`side`: top/above, left/before, right/after, else below).
fn labeled_layout<'h>(b: &B<'h>, c: &Cell<'h>, lc: &Cell<'h>, side: Option<&str>) -> T<'h> {
    let ((cw, ch), (lw, lh)) = (c.size(), lc.size());
    let (w, ht);
    let content = match side {
        Some("top" | "above") => {
            (w, ht) = (cw.max(lw), ch + lh + GAP / 2.0);
            vec![b.place(c, (w - cw) / 2.0, 0.0, cw, ch), b.place(lc, (w - lw) / 2.0, ch + GAP / 2.0, lw, lh)]
        }
        Some("left" | "before") => {
            (w, ht) = (cw + lw + GAP / 2.0, ch.max(lh));
            vec![b.place(lc, 0.0, (ht - lh) / 2.0, lw, lh), b.place(c, lw + GAP / 2.0, (ht - ch) / 2.0, cw, ch)]
        }
        Some("right" | "after") => {
            (w, ht) = (cw + lw + GAP / 2.0, ch.max(lh));
            vec![b.place(c, 0.0, (ht - ch) / 2.0, cw, ch), b.place(lc, cw + GAP / 2.0, (ht - lh) / 2.0, lw, lh)]
        }
        _ => {
            (w, ht) = (cw.max(lw), ch + lh + GAP / 2.0);
            vec![b.place(c, (w - cw) / 2.0, lh + GAP / 2.0, cw, ch), b.place(lc, (w - lw) / 2.0, 0.0, lw, lh)]
        }
    };
    b.page(content, w, ht, None)
}

/// `c` with a legend at its right: an entry per label, with a swatch of its colour when there are colours.
fn legended_layout<'h>(b: &B<'h>, c: &Cell<'h>, colors: &[T<'h>], entries: &[Cell<'h>]) -> T<'h> {
    let (cw, ch) = c.size();
    let row_h = entries.iter().map(|e| e.size().1).fold(TEXT * 1.4, f64::max);
    let lw = entries.iter().map(|e| e.size().0).fold(0.0, f64::max) + 30.0;
    let lh = row_h * entries.len() as f64;
    let (w, ht) = (cw + GAP + lw, ch.max(lh));
    let mut content = vec![b.place(c, 0.0, (ht - ch) / 2.0, cw, ch)];
    let top = (ht + lh) / 2.0;
    for (k, e) in entries.iter().enumerate() {
        let y = top - row_h * (k as f64 + 0.5);
        let x = cw + GAP;
        if let Some(&col) = colors.get(k) {
            content.push(b.list(&[col, b.call("rectangle", &[b.pt(x, y - 5.0), b.pt(x + 20.0, y + 5.0)])]));
        }
        let (ew, eh) = e.size();
        content.push(b.place(e, x + 26.0, y - eh / 2.0, ew, eh));
    }
    b.page(content, w, ht, None)
}

/// The most nodes an expression shown inside a labelled layout may have (larger ones are shown as terms).
const SHOWN_NODES: usize = 4000;

/// Whether `t` has at most `limit` nodes.
fn small(h: &Heap, t: T<'_>, limit: usize) -> bool {
    let mut stack = vec![t];
    let mut seen = 0usize;
    while let Some(e) = stack.pop() {
        seen += 1;
        if seen > limit {
            return false;
        }
        // a packed array counts its elements (it is not unpacked to count them)
        if let Some(mc_core::Packed::Reals { dims, .. } | mc_core::Packed::Ints { dims, .. }) = h.packed(e) {
            seen = seen.saturating_add(dims.iter().product::<u64>() as usize);
            continue;
        }
        if let View::App { head, args } = h.view(e) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    seen <= limit
}

/// An expression laid out as a label: its typeset size.
fn text_cell<'h>(h: &'h Heap, t: T<'h>) -> Cell<'h> {
    let ts = typeset(&Label::of(h, t), TEXT as f32);
    Cell::Label(t, (ts.width as f64, ts.height as f64))
}

/// How a `labeled` or `legended` value around something that is not a picture is shown: the expression typeset
/// with its label (or legend) beside it, as a `graphics` term to render. The value itself stays as it is (plots
/// and charts read it); this is only its picture. `None` for anything else, or an expression too large to
/// typeset.
///
/// `labeled(x, label)`, `labeled(x, label, top | bottom | left | right | above | below | before | after)` and the
/// options `frame: true`, `frame_style: colour`, `label_style: colour` (or a directive or list holding one),
/// `background: colour`; `legended(x, labels)` and `legended(x, line_legend([colours], [labels]))`.
pub fn shown_layout<'h>(h: &'h Heap, t: T<'h>) -> Option<T<'h>> {
    let View::App { head, args } = h.view(t) else { return None };
    let View::Sym(f) = h.view(head) else { return None };
    let name = h.sym_name(f);
    if name != "labeled" && name != "legended" {
        return None;
    }
    let (args, opts) = match args.split_last() {
        Some((&o, rest)) if is_options(h, o) => (rest, Some(o)),
        _ => (args, None),
    };
    let x = *args.first()?;
    if crate::is_graphics(h, x) || image_path(h, x).is_some() || !args.iter().all(|&a| small(h, a, SHOWN_NODES)) {
        return None;
    }
    let as_list = |t: T<'h>| match h.view(t) {
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::LIST) => Some(args),
        _ => None,
    };
    let b = B { h };
    let c = text_cell(h, x);
    let g = if name == "labeled" {
        let (l, pos) = match args {
            [_, l] => (*l, None),
            [_, l, p] => (*l, Some(*p)),
            _ => return None,
        };
        // the label in its colour
        let mut lc = text_cell(h, l);
        if let (Some(col), Cell::Label(_, size)) = (option(h, opts, "label_style").and_then(|s| colour_in(h, s)), &lc) {
            lc = Cell::Label(b.call("style", &[l, col]), *size);
        }
        let side = pos.and_then(|p| match h.view(p) {
            View::Sym(s) => Some(h.sym_name(s)),
            _ => None,
        });
        labeled_layout(&b, &c, &lc, side.as_deref())
    } else {
        let [_, l] = args else { return None };
        let (colors, labels) = legend_entries(h, *l, &as_list);
        if !labels.iter().all(|&e| small(h, e, SHOWN_NODES)) {
            return None;
        }
        let entries: Vec<Cell<'h>> = labels.iter().map(|&e| text_cell(h, e)).collect();
        legended_layout(&b, &c, &colors, &entries)
    };
    let framed = option(h, opts, "frame").is_some_and(|f| sym_is(h, f, names::TRUE) || sym_is(h, f, "all"));
    let background = option(h, opts, "background").and_then(|v| colour_in(h, v));
    if !framed && background.is_none() {
        return Some(g);
    }
    // a frame (and a background) around the whole: the layout drawn as an inset of a page a little larger
    let (w, ht) = crate::page_size(h, g).ok()?;
    let (w, ht) = (w as f64, ht as f64);
    let pad = 4.0;
    let (fw, fh) = (w + 2.0 * pad, ht + 2.0 * pad);
    let mut content = vec![];
    if let Some(bg) = background {
        content.push(b.list(&[bg, b.call("rectangle", &[b.pt(0.0, 0.0), b.pt(fw, fh)])]));
    }
    content.push(b.place(&Cell::Picture(g, (w, ht)), pad, pad, w, ht));
    if framed {
        let gray = crate::natives::rgb_term(h, crate::color::Rgba::rgb(0.45, 0.45, 0.45));
        let color = option(h, opts, "frame_style").and_then(|s| colour_in(h, s)).unwrap_or(gray);
        content.push(b.rect_line(0.5, 0.5, fw - 0.5, fh - 0.5, color));
    }
    Some(b.page(content, fw, fh, None))
}

/// A colour term in `t`: `t` itself when it is a colour, else the first colour among the parts of a list or a
/// `directive(...)`.
fn colour_in<'h>(h: &'h Heap, t: T<'h>) -> Option<T<'h>> {
    if matches!(crate::color::colour(h, t), crate::color::Colour::Value(_)) {
        return Some(t);
    }
    match h.view(t) {
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::LIST || h.sym_name(s) == "directive") => {
            args.iter().find_map(|&a| colour_in(h, a))
        }
        _ => None,
    }
}

/// The plot ranges of graphics terms combined: the union of the explicit `[[x0, x1], [y0, y1]]` ranges; `None`
/// unless every graphic has one.
pub(crate) fn union_range<'h>(h: &'h Heap, opts: &[Option<T<'h>>]) -> Option<[f64; 4]> {
    let mut r = [f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY];
    for &o in opts {
        let v = option(h, o, "plot_range")?;
        let View::App { args: &[x, y], .. } = h.view(v) else { return None };
        let pair = |t: T<'h>| -> Option<(f64, f64)> {
            match h.view(t) {
                View::App { args: &[a, b], .. } => Some((number(h, a)?, number(h, b)?)),
                _ => None,
            }
        };
        let ((x0, x1), (y0, y1)) = (pair(x)?, pair(y)?);
        r = [r[0].min(x0), r[1].max(x1), r[2].min(y0), r[3].max(y1)];
    }
    Some(r)
}
