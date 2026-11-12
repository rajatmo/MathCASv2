//! The layout engine: rows to fragments (width, height, depth, painted items, caret stops) by TeX's math rules
//! with the OpenType MATH constants (the approach of the OpenType MATH specification and MathML Core).
//!
//! Internally lengths are ems of the base size with y up from the baseline; [`Engine::run`] converts to output
//! units with y down at the end.
use super::lines::{break_lines, LineSpan, PieceInfo, CLASS_ANY};
use super::metrics::{FontRole, Metrics};
use super::{is_vector_delimiter, Draw, ItemBox, Layout, Line, LayoutOptions, RowGeom, Stop};
use crate::ops::{is_binary, is_close, is_open, is_relation, is_word, pair_at};
use crate::tree::{BigOp, Item, Node, Row, RowPath, Side, Step, MAX_NESTING};
use mc_mathfont::{font, MathFont, Stretch};
use std::cell::RefCell;

/// Row index of symbols the layout adds itself (operators of big operators, `=` of limits, `d` of integrals).
const NO_ROW: u32 = u32::MAX;
/// A thin space (3 mu).
const THIN: f32 = 1.0 / 6.0;
/// Continuation lines are indented by this many ems per bracket level (two em spaces, design 08, section 3.2).
const INDENT_PER_LEVEL: f32 = 2.0;
/// Least distance between the baselines of two lines, in ems.
const BASELINE_SKIP: f32 = 1.2;
/// Gap between the bottom of a line and the top of the next, in ems.
const LINE_GAP: f32 = 0.15;
/// Horizontal padding of a fraction on each side, in ems.
const FRACTION_PAD: f32 = 0.12;
/// Space between grid columns and between grid rows, in ems.
const GRID_COLUMN_GAP: f32 = 0.8;
const GRID_ROW_GAP: f32 = 0.25;
/// Space between a value and a label beside it (a quad, as between a formula and text), and below or above it.
const LABEL_GAP_X: f32 = 1.0;
const LABEL_GAP_Y: f32 = 0.3;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Style {
    Display,
    Text,
    Script,
    ScriptScript,
}

#[derive(Clone, Copy, Debug)]
struct Ctx {
    style: Style,
    cramped: bool,
    /// the width to wrap this row at (ems)
    max_width: Option<f32>,
}

impl Ctx {
    fn with(self, style: Style, cramped: bool) -> Ctx {
        Ctx { style, cramped, max_width: None }
    }
    fn sup(self) -> Ctx {
        let s = if self.style <= Style::Text { Style::Script } else { Style::ScriptScript };
        self.with(s, self.cramped)
    }
    fn sub(self) -> Ctx {
        self.sup().with(self.sup().style, true)
    }
    /// Fraction slots stay at full size at the first level (inputs and outputs draw fractions at text size) and
    /// shrink inside scripts.
    fn num(self) -> Ctx {
        let s = match self.style {
            Style::Display | Style::Text => Style::Text,
            _ => Style::ScriptScript,
        };
        self.with(s, self.cramped)
    }
    fn den(self) -> Ctx {
        let n = self.num();
        n.with(n.style, true)
    }
    fn cramp(self) -> Ctx {
        self.with(self.style, true)
    }
    fn display(self) -> bool {
        self.style == Style::Display
    }
    fn script(self) -> bool {
        self.style >= Style::Script
    }
}

/// A painted element, internal coordinates.
#[derive(Clone, Debug)]
enum D {
    Char { c: char, role: FontRole, size: f32, x: f32, y: f32, row: u32, index: u32, w: f32, sy: f32 },
    Ph { x: f32, y: f32, size: f32, row: u32, index: u32 },
    Glyph { glyph: u16, size: f32, x: f32, y: f32 },
    /// lower-left corner
    Rule { x: f32, y: f32, w: f32, h: f32 },
    /// a bracket drawn as strokes, lower-left corner of its box
    Delim { c: char, x: f32, y: f32, w: f32, h: f32, t: f32, row: u32, index: u32 },
    Graphic { path: String, x: f32, y: f32, w: f32, h: f32 },
}

/// The distance from one line's baseline to the next's (ems at scale `s`): the previous line's depth `prev_d`, the
/// next line's height `h`. Lines of plain text are exactly as far apart as the painter's text lines, taller ones
/// apart by a gap.
pub(super) fn line_skip(m: &dyn Metrics, prev_d: f32, h: f32, s: f32) -> f32 {
    let (a, d) = m.line(FontRole::Text);
    let (strut_h, strut_d) = (a * s, d * s);
    match m.line_pitch() {
        Some(p) if prev_d + h <= (strut_h + strut_d) * 1.001 => p * s,
        Some(p) => (prev_d + LINE_GAP * s + h).max(p * s),
        None => (prev_d + LINE_GAP * s + h).max(BASELINE_SKIP * s),
    }
}

/// A caret stop, internal coordinates.
#[derive(Clone, Copy, Debug)]
struct StopRec {
    row: u32,
    offset: u32,
    x: f32,
    y: f32,
    asc: f32,
    desc: f32,
}

/// An item's box, internal coordinates: its left edge and baseline, width, height above and depth below.
#[derive(Clone, Copy, Debug)]
struct InkRec {
    row: u32,
    index: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    d: f32,
}

/// A laid-out piece.
#[derive(Clone, Debug, Default)]
struct Frag {
    w: f32,
    h: f32,
    d: f32,
    items: Vec<D>,
    stops: Vec<StopRec>,
    ink: Vec<InkRec>,
}

impl Frag {
    fn kern(w: f32) -> Frag {
        Frag { w, ..Default::default() }
    }

    /// Copies `other`'s contents translated by (dx, dy), without changing the dimensions.
    fn place(&mut self, other: &Frag, dx: f32, dy: f32) {
        for it in &other.items {
            self.items.push(match it.clone() {
                D::Char { c, role, size, x, y, row, index, w, sy } => D::Char { c, role, size, x: x + dx, y: y + dy, row, index, w, sy },
                D::Ph { x, y, size, row, index } => D::Ph { x: x + dx, y: y + dy, size, row, index },
                D::Glyph { glyph, size, x, y } => D::Glyph { glyph, size, x: x + dx, y: y + dy },
                D::Rule { x, y, w, h } => D::Rule { x: x + dx, y: y + dy, w, h },
                D::Delim { c, x, y, w, h, t, row, index } => D::Delim { c, x: x + dx, y: y + dy, w, h, t, row, index },
                D::Graphic { path, x, y, w, h } => D::Graphic { path, x: x + dx, y: y + dy, w, h },
            });
        }
        for s in &other.stops {
            self.stops.push(StopRec { x: s.x + dx, y: s.y + dy, ..*s });
        }
        for b in &other.ink {
            self.ink.push(InkRec { x: b.x + dx, y: b.y + dy, ..*b });
        }
    }

    /// Appends `other` on the right, raised by `dy`.
    fn hcat(&mut self, other: &Frag, dy: f32) {
        let x = self.w;
        self.place(other, x, dy);
        self.w += other.w;
        self.h = self.h.max(other.h + dy);
        self.d = self.d.max(other.d - dy);
    }

    /// Puts `other` at (dx, dy) and grows the box to contain it.
    fn overlay(&mut self, other: &Frag, dx: f32, dy: f32) {
        self.place(other, dx, dy);
        self.w = self.w.max(dx + other.w);
        self.h = self.h.max(other.h + dy);
        self.d = self.d.max(other.d - dy);
    }

    fn raise(&mut self, dy: f32) {
        let mut out = Frag { w: self.w, h: self.h + dy, d: self.d - dy, ..Default::default() };
        out.place(self, 0.0, dy);
        *self = out;
    }
}

/// One item (or operator pair) of a row, laid out.
struct Piece {
    frag: Frag,
    /// the source character, for classes and bracket matching
    ch: Option<char>,
    first: usize,
    count: usize,
    class: Class,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Ord,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
    Space,
    Newline,
}

/// A line of the root row, internal coordinates.
struct RootLine {
    start: usize,
    end: usize,
    base: f32,
    h: f32,
    d: f32,
    indent: f32,
    /// the line's right end (its width, the indent included)
    right: f32,
}

/// A line of a row while it is assembled.
struct LineRec {
    span: LineSpan,
    first_item: usize,
    end_item: usize,
    frag: Frag,
}

pub(super) struct Engine<'m> {
    m: &'m dyn Metrics,
    o: &'m LayoutOptions,
    f: &'static MathFont,
    /// the path of every row laid out, indexed by row id
    rows: RefCell<Vec<RowPath>>,
    /// the root row's lines
    root_lines: RefCell<Vec<RootLine>>,
    /// the bracket depth at the start of the root row (a logical line continuing another's open brackets)
    initial_depth: usize,
}

impl<'m> Engine<'m> {
    pub(super) fn new(m: &'m dyn Metrics, o: &'m LayoutOptions) -> Engine<'m> {
        Engine { m, o, f: font(), rows: RefCell::new(Vec::new()), root_lines: RefCell::new(Vec::new()), initial_depth: 0 }
    }

    /// The root row starts at bracket depth `d`.
    pub(super) fn starting_at_depth(mut self, d: usize) -> Engine<'m> {
        self.initial_depth = d;
        self
    }

    pub(super) fn run(self, row: &Row) -> Layout {
        let size = self.o.size;
        let ctx = Ctx {
            style: if self.o.display { Style::Display } else { Style::Text },
            cramped: false,
            max_width: self.o.max_width.map(|w| w / size),
        };
        let frag = self.row(row, &mut Vec::new(), ctx, 0);
        let top = frag.h;
        let ty = |y: f32| (top - y) * size;
        let items = frag
            .items
            .iter()
            .map(|d| match d.clone() {
                D::Char { c, role, size: s, x, y, row, index, w, sy } => {
                    Draw::Char { c, role, size: s * size, x: x * size, y: ty(y), row, index, advance: w * size, stretch: sy }
                }
                D::Ph { x, y, size: s, row, index } => Draw::Placeholder { x: x * size, y: ty(y), size: s * size, row, index },
                D::Glyph { glyph, size: s, x, y } => Draw::Glyph { glyph, size: s * size, x: x * size, y: ty(y) },
                D::Rule { x, y, w, h } => Draw::Rule { x: x * size, y: ty(y + h), w: w * size, h: h * size },
                D::Delim { c, x, y, w, h, t, row, index } => Draw::Delim {
                    c,
                    x: x * size,
                    y: ty(y + h),
                    w: w * size,
                    h: h * size,
                    stroke: t * size,
                    row,
                    index,
                },
                D::Graphic { path, x, y, w, h } => Draw::Graphic { path, x: x * size, y: ty(y + h), w: w * size, h: h * size },
            })
            .collect();
        let paths = self.rows.take();
        let mut rows: Vec<RowGeom> =
            paths.into_iter().map(|path| RowGeom { path, stops: Vec::new(), ink: Vec::new() }).collect();
        for b in &frag.ink {
            if let Some(r) = rows.get_mut(b.row as usize) {
                let k = b.index as usize;
                if r.ink.len() <= k {
                    r.ink.resize(k + 1, ItemBox::default());
                }
                r.ink[k] = ItemBox { x: b.x * size, w: b.w * size, top: ty(b.y + b.h), bottom: ty(b.y - b.d) };
            }
        }
        for s in &frag.stops {
            if let Some(r) = rows.get_mut(s.row as usize) {
                let k = s.offset as usize;
                if r.stops.len() <= k {
                    r.stops.resize(k + 1, Stop { x: 0.0, baseline: 0.0, top: 0.0, bottom: 0.0 });
                }
                r.stops[k] = Stop { x: s.x * size, baseline: ty(s.y), top: ty(s.y + s.asc), bottom: ty(s.y - s.desc) };
            }
        }
        let lines = self
            .root_lines
            .take()
            .into_iter()
            .map(|RootLine { start, end, base, h, d, indent, right }| Line {
                top: ty(base + h),
                baseline: ty(base),
                bottom: ty(base - d),
                start,
                end,
                indent: indent * size,
                right: right * size,
            })
            .collect();
        Layout { width: frag.w * size, height: (frag.h + frag.d) * size, baseline: top * size, items, lines, rows }
    }

    fn scale(&self, ctx: Ctx) -> f32 {
        let c = &self.f.constants;
        match ctx.style {
            Style::Display | Style::Text => 1.0,
            Style::Script => c.script_percent / 100.0,
            Style::ScriptScript => c.script_script_percent / 100.0,
        }
    }

    fn axis(&self, ctx: Ctx) -> f32 {
        self.m.axis_height().unwrap_or(self.f.constants.axis_height) * self.scale(ctx)
    }

    // ---- characters ----

    /// A character as drawn in `ctx`, attributed to item `index` of row `row`.
    fn char_frag(&self, c: char, ctx: Ctx, row: u32, index: u32) -> Frag {
        let s = self.scale(ctx);
        let space = match c {
            '\u{2009}' => Some(THIN),
            '\u{2005}' => Some(0.25),
            '\u{205f}' => Some(4.0 / 18.0),
            '\u{2004}' => Some(1.0 / 3.0),
            '\u{2002}' => Some(0.5),
            '\u{2003}' => Some(1.0),
            _ => None,
        };
        if let Some(w) = space {
            return Frag::kern(w * s);
        }
        let role = self.m.role(c);
        let m = self.m.char(c, role);
        if c.is_whitespace() {
            // kept as a character (no height): a painter can keep the text around it in one run, placed exactly as
            // the same text in a plain line
            return Frag {
                w: m.advance * s,
                h: 0.0,
                d: 0.0,
                items: vec![D::Char { c, role, size: s, x: 0.0, y: 0.0, row, index, w: m.advance * s, sy: 1.0 }],
                stops: vec![],
            ink: vec![],
            };
        }
        Frag {
            w: m.advance * s,
            h: m.ascent * s,
            d: m.descent * s,
            items: vec![D::Char { c, role, size: s, x: 0.0, y: 0.0, row, index, w: m.advance * s, sy: 1.0 }],
            stops: vec![],
            ink: vec![],
        }
    }

    /// A symbol the layout adds (not part of any row).
    fn symbol(&self, text: &str, ctx: Ctx) -> Frag {
        let mut f = Frag::default();
        for c in text.chars() {
            f.hcat(&self.char_frag(c, ctx, NO_ROW, NO_ROW), 0.0);
        }
        f
    }

    fn italic_of(&self, c: char, ctx: Ctx) -> f32 {
        if self.m.role(c) != FontRole::Math {
            return 0.0;
        }
        self.f.glyph(c).map_or(0.0, |g| self.f.italic_correction(g)) * self.scale(ctx)
    }

    // ---- rows ----

    fn register(&self, path: &[Step]) -> u32 {
        let mut rows = self.rows.borrow_mut();
        rows.push(path.to_vec());
        (rows.len() - 1) as u32
    }

    fn row(&self, row: &Row, path: &mut RowPath, ctx: Ctx, depth: usize) -> Frag {
        let id = self.register(path);
        let mut pieces = self.pieces(row, path, ctx, id, depth);
        self.stretch_brackets(&mut pieces, ctx);
        let spaces = self.spacing(&mut pieces, ctx);
        let (strut_h, strut_d) = {
            let (a, d) = self.m.line(FontRole::Text);
            let s = self.scale(ctx);
            (a * s, d * s)
        };
        // the root row may continue a line of another (a logical line laid out on its own, `layout_by_lines`)
        let start_depth = if path.is_empty() { self.initial_depth } else { 0 };
        let infos = self.infos(&pieces, &spaces, start_depth);
        let indent = if self.o.indent_continuations { INDENT_PER_LEVEL } else { 0.0 };
        let first = if start_depth > 0 { indent * infos.first().map_or(start_depth, |i| i.depth) as f32 } else { 0.0 };
        let spans = break_lines(&infos, ctx.max_width, indent, first);
        let mut lines: Vec<LineRec> = Vec::new();
        for span in spans {
            let mut f = Frag { h: strut_h, d: strut_d, ..Default::default() };
            f.w = span.indent;
            let first_item = pieces.get(span.start).map_or(row.len(), |p| p.first);
            for (k, p) in pieces.iter().enumerate().take(span.end).skip(span.start) {
                if k > span.start {
                    f.w += spaces[k];
                }
                // caret stops: before the piece, and inside an operator pair after it
                f.stops.push(StopRec { row: id, offset: p.first as u32, x: f.w, y: 0.0, asc: 0.0, desc: 0.0 });
                // each item's box: the piece's extent, at least a text line of the row's size (a space, a thin
                // operator), an operator pair's width shared among its items
                let share = p.frag.w / p.count.max(1) as f32;
                for j in 0..p.count {
                    f.ink.push(InkRec {
                        row: id,
                        index: (p.first + j) as u32,
                        x: f.w + share * j as f32,
                        y: 0.0,
                        w: share,
                        h: p.frag.h.max(strut_h),
                        d: p.frag.d.max(strut_d),
                    });
                }
                f.hcat(&p.frag, 0.0);
                for j in 1..p.count {
                    f.stops.push(StopRec { row: id, offset: (p.first + j) as u32, x: f.w, y: 0.0, asc: 0.0, desc: 0.0 });
                }
            }
            let end_item = pieces.get(span.end).map_or(row.len(), |p| p.first);
            lines.push(LineRec { span, first_item, end_item, frag: f });
        }
        if lines.is_empty() {
            lines.push(LineRec {
                span: LineSpan { start: 0, end: 0, indent: 0.0 },
                first_item: 0,
                end_item: 0,
                frag: Frag { h: strut_h, d: strut_d, ..Default::default() },
            });
        }
        // the end-of-row stop goes at the end of the last line
        if let Some(last) = lines.last_mut() {
            let x = last.frag.w;
            last.frag.stops.push(StopRec { row: id, offset: row.len() as u32, x, y: 0.0, asc: 0.0, desc: 0.0 });
        }
        // caret heights: one text line at the row's size (design 11, 3.1), however tall the line is
        for l in &mut lines {
            for s in l.frag.stops.iter_mut().filter(|s| s.row == id) {
                s.asc = strut_h;
                s.desc = strut_d;
            }
        }
        // stack the lines
        let mut out = Frag::default();
        let mut base = 0.0f32;
        let mut prev_d = 0.0f32;
        let mut root = Vec::new();
        for (k, l) in lines.iter().enumerate() {
            if k > 0 {
                let s = self.scale(ctx);
                let skip = line_skip(self.m, prev_d, l.frag.h, s);
                base -= skip;
            }
            out.place(&l.frag, 0.0, base);
            out.w = out.w.max(l.frag.w);
            if k == 0 {
                out.h = l.frag.h;
            }
            out.d = out.d.max(l.frag.d - base);
            prev_d = l.frag.d;
            root.push(RootLine {
                start: l.first_item,
                end: l.end_item,
                base,
                h: l.frag.h,
                d: l.frag.d,
                indent: l.span.indent,
                right: l.frag.w,
            });
        }
        if path.is_empty() {
            *self.root_lines.borrow_mut() = root;
        }
        out
    }

    /// The pieces of a row: characters (operator pairs merged), placeholders, boxes.
    fn pieces(&self, row: &Row, path: &mut RowPath, ctx: Ctx, id: u32, depth: usize) -> Vec<Piece> {
        let mut out = Vec::new();
        let mut k = 0;
        let small_ok = self.o.small_inline_rationals && row.len() > 1;
        while k < row.len() {
            let piece = match &row.0[k] {
                Item::Char('\n') => Piece { frag: Frag::default(), ch: Some('\n'), first: k, count: 1, class: Class::Newline },
                Item::Char(c) => {
                    if let Some((n, glyph)) = self.o.operator_spacing.then(|| pair_at(row, k)).flatten() {
                        let frag = self.char_frag(glyph, ctx, id, k as u32);
                        out.push(Piece { frag, ch: Some(glyph), first: k, count: n, class: Class::Rel });
                        k += n;
                        continue;
                    }
                    let shown = if self.o.traditional && self.m.italic_letters() { traditional_char(row, k, *c) } else { *c };
                    Piece { frag: self.char_frag(shown, ctx, id, k as u32), ch: Some(*c), first: k, count: 1, class: char_class(*c) }
                }
                Item::Placeholder => {
                    let s = self.scale(ctx);
                    let m = self.m.char('□', FontRole::Math);
                    let frag = Frag {
                        w: m.advance * s,
                        h: m.ascent * s,
                        d: m.descent * s,
                        items: vec![D::Ph { x: 0.0, y: 0.0, size: s, row: id, index: k as u32 }],
                        stops: vec![],
            ink: vec![],
                    };
                    Piece { frag, ch: None, first: k, count: 1, class: Class::Ord }
                }
                Item::Box(b) => {
                    let frag = if depth >= MAX_NESTING {
                        self.symbol("…", ctx)
                    } else {
                        let small = small_ok && is_small_rational(b);
                        self.node(b, k, path, ctx, depth + 1, small)
                    };
                    Piece { frag, ch: None, first: k, count: 1, class: Class::Ord }
                }
            };
            out.push(piece);
            k += 1;
        }
        out
    }

    /// Grows brackets around tall content (`(`, `[`, `{` and their partners), innermost pairs first.
    fn stretch_brackets(&self, pieces: &mut [Piece], ctx: Ctx) {
        let s = self.scale(ctx);
        let mut stack: Vec<usize> = Vec::new();
        let mut pairs = Vec::new();
        for (k, p) in pieces.iter().enumerate() {
            match p.ch {
                Some(c) if is_open(c) => stack.push(k),
                Some(c) if is_close(c) => {
                    if let Some(o) = stack.pop() {
                        pairs.push((o, k));
                    }
                }
                _ => {}
            }
        }
        // pairs are found in order of their closing bracket: inner pairs first
        for (o, c) in pairs {
            let h = pieces[o + 1..c].iter().map(|p| p.frag.h).fold(0.0, f32::max);
            let d = pieces[o + 1..c].iter().map(|p| p.frag.d).fold(0.0, f32::max);
            let (oh, od) = (pieces[o].frag.h, pieces[o].frag.d);
            // a bracket keeps its own glyph (the cell's font, nothing moves) unless the content clearly overhangs it:
            // a superscript or subscript reaching a little past the bracket does not grow it, a fraction does
            let slack = 0.3 * s;
            if h <= oh + slack && d <= od + slack {
                continue;
            }
            let a = self.axis(ctx);
            let delta = (h - a).max(d + a);
            let target = (2.0 * delta * 0.901).max(2.0 * delta - 0.5 * s);
            for k in [o, c] {
                if let Some(ch) = pieces[k].ch {
                    // a bracket of the text font grows as that same glyph, scaled (normal and tall brackets look
                    // alike); otherwise the math font's stretchy delimiter
                    let source = pieces[k].frag.items.iter().find_map(|it| match *it {
                        D::Char { role: FontRole::Text, row, index, .. } => Some((row, index)),
                        _ => None,
                    });
                    pieces[k].frag = match source {
                        Some((row, index)) => self.text_delim(ch, target, ctx, row, index),
                        None => self.delim(ch, target, ctx),
                    };
                }
            }
        }
    }

    /// Spacing before each piece: thin spaces around binary operators and relations (inputs and outputs, not in
    /// scripts), none next to typed spaces; a binary operator with nothing on its left is unary.
    fn spacing(&self, pieces: &mut [Piece], ctx: Ctx) -> Vec<f32> {
        let n = pieces.len();
        let mut sp = vec![0.0; n];
        if !self.o.operator_spacing || ctx.script() {
            return sp;
        }
        let s = self.scale(ctx);
        let mut prev: Option<Class> = None;
        for k in 0..n {
            let class = pieces[k].class;
            if class == Class::Bin && matches!(prev, None | Some(Class::Open | Class::Bin | Class::Rel | Class::Punct | Class::Newline)) {
                pieces[k].class = Class::Ord;
            }
            let class = pieces[k].class;
            if matches!(class, Class::Bin | Class::Rel) {
                let before_space = k == 0 || matches!(pieces[k - 1].class, Class::Space | Class::Newline);
                let after_space = k + 1 >= n || matches!(pieces[k + 1].class, Class::Space | Class::Newline);
                if !before_space {
                    sp[k] += THIN * s;
                }
                if !after_space {
                    sp[k + 1] += THIN * s;
                }
            }
            if class != Class::Space {
                prev = Some(class);
            }
        }
        sp
    }

    /// Break information for each piece.
    fn infos(&self, pieces: &[Piece], spaces: &[f32], start_depth: usize) -> Vec<PieceInfo> {
        let mut depth = start_depth;
        let mut out = Vec::with_capacity(pieces.len());
        for (k, p) in pieces.iter().enumerate() {
            if p.ch.is_some_and(is_close) {
                depth = depth.saturating_sub(1);
            }
            let prev = k.checked_sub(1).map(|j| &pieces[j]);
            let class = match prev {
                None => None,
                Some(q) => {
                    let word = |p: &Piece| p.ch.is_some_and(|c| is_word(c) || c == '.' || c == '\'');
                    if word(q) && word(p) {
                        None
                    } else if matches!(q.ch, Some(',' | ';')) {
                        Some(0)
                    } else if p.class == Class::Rel {
                        Some(1)
                    } else if p.class == Class::Bin || matches!(p.ch, Some('+' | '-')) {
                        Some(2)
                    } else if matches!(p.ch, Some('*' | '/')) {
                        Some(3)
                    } else if q.class == Class::Space {
                        Some(4)
                    } else if p.class == Class::Close || q.class == Class::Open {
                        None
                    } else {
                        Some(CLASS_ANY)
                    }
                }
            };
            out.push(PieceInfo { width: p.frag.w + spaces[k], space_before: spaces[k], newline: p.class == Class::Newline, class, depth });
            if p.ch.is_some_and(is_open) {
                depth += 1;
            }
        }
        out
    }

    // ---- boxes ----

    /// The row of slot `slot` of the box at item `item`, laid out.
    fn slot(&self, r: &Row, path: &mut RowPath, item: usize, slot: usize, ctx: Ctx, depth: usize) -> Frag {
        path.push(Step { item, slot });
        let f = self.row(r, path, ctx, depth);
        path.pop();
        f
    }

    fn node(&self, n: &Node, item: usize, path: &mut RowPath, ctx: Ctx, depth: usize, small: bool) -> Frag {
        let row_of = |k: usize| n.slot(k).cloned().unwrap_or_default();
        macro_rules! slot {
            ($k:expr, $c:expr) => {
                self.slot(&row_of($k), path, item, $k, $c, depth)
            };
        }
        match n {
            Node::Fraction { .. } => {
                let fctx = if small { ctx.sup().with(ctx.sup().style, false) } else { ctx };
                let width = ctx.max_width.map(|w| w - 2.0 * FRACTION_PAD);
                let num = slot!(0, Ctx { max_width: width, ..fctx.num() });
                let den = slot!(1, Ctx { max_width: width, ..fctx.den() });
                self.fraction(num, den, if small { fctx.with(Style::Text, false) } else { fctx })
            }
            Node::Superscript { base, .. } => {
                // a superscript on a subscript draws both scripts on the right of the subscript's base
                if let [Item::Box(inner)] = base.0.as_slice() {
                    if let Node::Subscript { base: b, index } = &**inner {
                        let base_ctx = ctx;
                        path.push(Step { item, slot: 0 });
                        let id = self.register(path);
                        path.push(Step { item: 0, slot: 0 });
                        let bf = self.row(b, path, base_ctx, depth + 1);
                        path.pop();
                        path.push(Step { item: 0, slot: 1 });
                        let sub = self.row(index, path, ctx.sub(), depth + 1);
                        path.pop();
                        path.pop();
                        let sup = slot!(1, ctx.sup());
                        let italic = single_char(b).map_or(0.0, |c| self.italic_of(c, ctx));
                        let mut f = self.scripts(bf, italic, single_char(b).is_some(), false, Some(sup), Some(sub), ctx);
                        // the base row (holding the subscript box) has stops before and after it, one text line tall
                        let (a, d) = self.m.line(FontRole::Text);
                        let s = self.scale(ctx);
                        let (h, d) = (a * s, d * s);
                        f.stops.push(StopRec { row: id, offset: 0, x: 0.0, y: 0.0, asc: h, desc: d });
                        f.stops.push(StopRec { row: id, offset: 1, x: f.w, y: 0.0, asc: h, desc: d });
                        f.ink.push(InkRec { row: id, index: 0, x: 0.0, y: 0.0, w: f.w, h: f.h, d: f.d });
                        return f;
                    }
                }
                let mut b = slot!(0, ctx);
                // a base that is not one operand is drawn in parentheses, as its linear text is written
                if crate::ops::compound_base(base) {
                    b = self.fenced(b, ctx);
                }
                let e = slot!(1, ctx.sup());
                let italic = single_char(base).map_or(0.0, |c| self.italic_of(c, ctx));
                self.scripts(b, italic, single_char(base).is_some(), false, Some(e), None, ctx)
            }
            Node::Subscript { base, .. } => {
                let b = slot!(0, ctx);
                let i = slot!(1, ctx.sub());
                let italic = single_char(base).map_or(0.0, |c| self.italic_of(c, ctx));
                self.scripts(b, italic, single_char(base).is_some(), false, None, Some(i), ctx)
            }
            Node::Radical { index, .. } => {
                let deg = index.as_ref().map(|_| slot!(0, ctx.with(Style::ScriptScript, false)));
                let body = slot!(usize::from(index.is_some()), ctx.cramp());
                self.radical(deg, body, ctx)
            }
            Node::Overscript { .. } => {
                let b = slot!(0, ctx);
                let o = slot!(1, ctx.sup());
                self.stack(b, Some(o), None, ctx)
            }
            Node::Underscript { .. } => {
                let b = slot!(0, ctx);
                let u = slot!(1, ctx.sub());
                self.stack(b, None, Some(u), ctx)
            }
            Node::Underoverscript { .. } => {
                let b = slot!(0, ctx);
                let u = slot!(1, ctx.sub());
                let o = slot!(2, ctx.sup());
                self.stack(b, Some(o), Some(u), ctx)
            }
            Node::Grid { rows } => {
                let w = rows.first().map_or(0, |r| r.len());
                let cell_ctx = ctx.num();
                let cells: Vec<Vec<Frag>> =
                    (0..rows.len()).map(|r| (0..w).map(|c| slot!(r * w + c, cell_ctx)).collect()).collect();
                self.grid(cells, ctx)
            }
            Node::BigOp { op, lower, upper, .. } => {
                let nslots = n.slot_count();
                let limits = usize::from(lower.is_some()) + usize::from(upper.is_some());
                // slot order: integral lower, upper, body, var; others var, lower, upper, body
                let (var_k, lim_k, body_k) =
                    if *op == BigOp::Integral { (nslots - 1, 0, nslots - 2) } else { (0, 1, nslots - 1) };
                let mut slot = |k: usize, c: Ctx| self.slot(&row_of(k), path, item, k, c, depth);
                self.big_op(*op, limits, var_k, lim_k, body_k, &mut slot, ctx)
            }
            Node::Labeled { label, side, .. } => {
                // the label at the size of text (not of a script: it is read as text); the value wraps in the width
                // the label leaves
                let lctx = ctx.with(ctx.style.max(Style::Text), false);
                let l = slot!(1, lctx);
                let s = self.scale(ctx);
                let width = ctx.max_width.map(|w| match side {
                    Side::Left | Side::Right => (w - l.w - LABEL_GAP_X * s).max(w * 0.4),
                    Side::Below | Side::Above => w,
                });
                let v = slot!(0, Ctx { max_width: width, ..ctx });
                let lines = 1 + label.0.iter().filter(|i| matches!(i, Item::Char('\n'))).count();
                self.labeled(v, l, *side, lines, ctx)
            }
            Node::Graphic { path: p } => {
                let s = self.scale(ctx);
                let (w, h) = self.m.graphic(p);
                let (w, h) = (w * s, h * s);
                Frag { w, h, d: 0.0, items: vec![D::Graphic { path: p.clone(), x: 0.0, y: 0.0, w, h }], stops: vec![], ink: vec![] }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn big_op(
        &self,
        op: BigOp,
        limits: usize,
        var_k: usize,
        lim_k: usize,
        body_k: usize,
        slot: &mut dyn FnMut(usize, Ctx) -> Frag,
        ctx: Ctx,
    ) -> Frag {
        let s = self.scale(ctx);
        let display = ctx.display();
        let mut out = match op {
            BigOp::Sum | BigOp::Product => {
                let sym = if op == BigOp::Sum { '∑' } else { '∏' };
                let (opf, italic) = self.big_symbol(sym, ctx);
                let sub_ctx = ctx.sub();
                let mut lower = slot(var_k, sub_ctx);
                let upper = if limits == 2 {
                    lower.hcat(&self.symbol("=", sub_ctx), 0.0);
                    lower.hcat(&slot(lim_k, sub_ctx), 0.0);
                    Some(slot(lim_k + 1, ctx.sup()))
                } else {
                    None
                };
                if display {
                    self.limits(opf, italic, upper, Some(lower), ctx)
                } else {
                    self.scripts(opf, italic, false, true, upper, Some(lower), ctx)
                }
            }
            BigOp::Integral => {
                let (opf, italic) = self.big_symbol('∫', ctx);
                if limits == 2 {
                    // integrals keep their limits at the side, the lower one tucked in by the italic correction
                    let lo = slot(lim_k, ctx.sub());
                    let hi = slot(lim_k + 1, ctx.sup());
                    self.scripts(opf, italic, false, true, Some(hi), Some(lo), ctx)
                } else {
                    opf
                }
            }
            BigOp::Limit => {
                let lim = self.symbol("lim", ctx);
                let sub_ctx = ctx.sub();
                let mut below = slot(var_k, sub_ctx);
                below.hcat(&self.symbol("→", sub_ctx), 0.0);
                below.hcat(&slot(lim_k, sub_ctx), 0.0);
                if display {
                    self.limits(lim, 0.0, None, Some(below), ctx)
                } else {
                    self.scripts(lim, 0.0, false, false, None, Some(below), ctx)
                }
            }
            BigOp::Derivative => {
                let d = "∂";
                if display {
                    let mut num = self.symbol(d, ctx.num());
                    num.hcat(&slot(body_k, ctx.num()), 0.0);
                    let mut den = self.symbol(d, ctx.den());
                    den.hcat(&slot(var_k, ctx.den()), 0.0);
                    return self.fraction(num, den, ctx);
                }
                let dsym = self.symbol(d, ctx);
                let v = slot(var_k, ctx.sub());
                self.scripts(dsym, 0.0, true, false, None, Some(v), ctx)
            }
        };
        out.hcat(&Frag::kern(THIN * s), 0.0);
        out.hcat(&slot(body_k, ctx), 0.0);
        if op == BigOp::Integral {
            out.hcat(&Frag::kern(THIN * s), 0.0);
            out.hcat(&self.symbol(if self.o.traditional { "ⅆ" } else { "d" }, ctx), 0.0);
            out.hcat(&slot(var_k, ctx), 0.0);
        }
        out
    }

    /// A big operator symbol, in its display size in display style, centred on the axis; with its italic
    /// correction.
    fn big_symbol(&self, c: char, ctx: Ctx) -> (Frag, f32) {
        let s = self.scale(ctx);
        let Some(mut g) = self.f.glyph(c) else { return (self.symbol(&c.to_string(), ctx), 0.0) };
        if ctx.display() {
            let min = self.f.constants.display_operator_min_height;
            let vs = self.f.variants(g, true);
            if let Some((v, _)) = vs.iter().find(|(v, adv)| *v != g && *adv >= min).or(vs.last()) {
                g = *v;
            }
        }
        let m = self.f.metrics(g);
        let mut frag = Frag {
            w: m.advance * s,
            h: m.height * s,
            d: m.depth * s,
            items: vec![D::Glyph { glyph: g, size: s, x: 0.0, y: 0.0 }],
            stops: vec![],
            ink: vec![],
        };
        let shift = (m.height - m.depth) * s / 2.0 - self.axis(ctx);
        frag.raise(-shift);
        (frag, self.f.italic_correction(g) * s)
    }

    /// Scripts on the right of a nucleus (rules 18a-f): superscript shifted up, subscript down, both kept apart.
    /// A big operator's scripts go at its advance, the subscript pulled left by the italic correction (MathML
    /// Core); other scripts push the superscript right by it.
    #[allow(clippy::too_many_arguments)]
    fn scripts(&self, base: Frag, italic: f32, is_char: bool, big: bool, sup: Option<Frag>, sub: Option<Frag>, ctx: Ctx) -> Frag {
        let s = self.scale(ctx);
        let c = &self.f.constants;
        let mut out = base.clone();
        let (x_sup, x_sub) = if big { (base.w, base.w - italic) } else { (base.w + italic, base.w) };
        let mut sup_shift = 0.0;
        let mut sub_shift = 0.0;
        if let Some(sp) = &sup {
            let mut u = if ctx.cramped { c.superscript_shift_up_cramped } else { c.superscript_shift_up } * s;
            if !is_char {
                u = u.max(base.h - c.superscript_baseline_drop_max * s);
            }
            sup_shift = u.max(sp.d + c.superscript_bottom_min * s);
        }
        if let Some(sb) = &sub {
            let mut v = c.subscript_shift_down * s;
            if !is_char {
                v = v.max(base.d + c.subscript_baseline_drop_min * s);
            }
            sub_shift = if sup.is_none() { v.max(sb.h - c.subscript_top_max * s) } else { v };
        }
        if let (Some(sp), Some(sb)) = (&sup, &sub) {
            let gap = (sup_shift - sp.d) - (sb.h - sub_shift);
            let min = c.sub_superscript_gap_min * s;
            if gap < min {
                let need = min - gap;
                let room = (c.superscript_bottom_max_with_subscript * s - (sup_shift - sp.d)).max(0.0);
                let up = need.min(room);
                sup_shift += up;
                sub_shift += need - up;
            }
        }
        let mut w = base.w;
        if let Some(sp) = &sup {
            out.overlay(sp, x_sup, sup_shift);
            w = w.max(x_sup + sp.w);
        }
        if let Some(sb) = &sub {
            out.overlay(sb, x_sub, -sub_shift);
            w = w.max(x_sub + sb.w);
        }
        out.w = w + c.space_after_script * s;
        out
    }

    /// Limits above and below a nucleus (rule 13a), centred.
    fn limits(&self, base: Frag, italic: f32, sup: Option<Frag>, sub: Option<Frag>, ctx: Ctx) -> Frag {
        let s = self.scale(ctx);
        let c = &self.f.constants;
        let w = base.w.max(sup.as_ref().map_or(0.0, |f| f.w)).max(sub.as_ref().map_or(0.0, |f| f.w));
        let mut out = Frag { w, ..Default::default() };
        out.overlay(&base, (w - base.w) / 2.0, 0.0);
        if let Some(sp) = &sup {
            let y = base.h + (c.upper_limit_gap_min * s + sp.d).max(c.upper_limit_baseline_rise_min * s);
            out.overlay(sp, (w - sp.w) / 2.0 + italic / 2.0, y);
        }
        if let Some(sb) = &sub {
            let y = -(base.d + (c.lower_limit_gap_min * s + sb.h).max(c.lower_limit_baseline_drop_min * s));
            out.overlay(sb, (w - sb.w) / 2.0 - italic / 2.0, y);
        }
        out.w = w;
        out
    }

    /// Something centred above and/or below a base (over- and underscripts).
    fn stack(&self, base: Frag, over: Option<Frag>, under: Option<Frag>, ctx: Ctx) -> Frag {
        let s = self.scale(ctx);
        let c = &self.f.constants;
        let w = base.w.max(over.as_ref().map_or(0.0, |f| f.w)).max(under.as_ref().map_or(0.0, |f| f.w));
        let mut out = Frag { w, ..Default::default() };
        out.overlay(&base, (w - base.w) / 2.0, 0.0);
        if let Some(o) = &over {
            out.overlay(o, (w - o.w) / 2.0, base.h + c.upper_limit_gap_min * s * 0.6 + o.d);
        }
        if let Some(u) = &under {
            out.overlay(u, (w - u.w) / 2.0, -(base.d + c.lower_limit_gap_min * s * 0.6 + u.h));
        }
        out
    }

    /// A value with its label (`lines` lines of text) at `side`: below or above it centred, before or after it on
    /// its baseline; a label of several lines beside the value is centred on the value's middle. The value keeps
    /// its baseline (the row's line stays where the value alone would put it).
    fn labeled(&self, v: Frag, l: Frag, side: Side, lines: usize, ctx: Ctx) -> Frag {
        let s = self.scale(ctx);
        let (gx, gy) = (LABEL_GAP_X * s, LABEL_GAP_Y * s);
        let beside = if lines > 1 { (v.h - v.d) / 2.0 - (l.h - l.d) / 2.0 } else { 0.0 };
        let mut out = Frag::default();
        match side {
            Side::Below | Side::Above => {
                let w = v.w.max(l.w);
                out.w = w;
                out.overlay(&v, (w - v.w) / 2.0, 0.0);
                let dy = if side == Side::Below { -(v.d + gy + l.h) } else { v.h + gy + l.d };
                out.overlay(&l, (w - l.w) / 2.0, dy);
            }
            Side::Left => {
                out.overlay(&l, 0.0, beside);
                out.overlay(&v, l.w + gx, 0.0);
            }
            Side::Right => {
                out.overlay(&v, 0.0, 0.0);
                out.overlay(&l, v.w + gx, beside);
            }
        }
        out
    }

    /// Numerator over denominator with a rule on the axis (rules 15a-e).
    fn fraction(&self, n: Frag, d: Frag, ctx: Ctx) -> Frag {
        let s = self.scale(ctx);
        let c = &self.f.constants;
        let disp = ctx.display();
        let a = self.axis(ctx);
        let t = c.fraction_rule_thickness * s;
        let mut u = if disp { c.fraction_numerator_display_style_shift_up } else { c.fraction_numerator_shift_up } * s;
        let mut v = if disp { c.fraction_denominator_display_style_shift_down } else { c.fraction_denominator_shift_down } * s;
        let gn = if disp { c.fraction_num_display_style_gap_min } else { c.fraction_numerator_gap_min } * s;
        let gd = if disp { c.fraction_denom_display_style_gap_min } else { c.fraction_denominator_gap_min } * s;
        u = u.max(a + t / 2.0 + gn + n.d);
        v = v.max(d.h + gd - (a - t / 2.0));
        let pad = FRACTION_PAD * s;
        let w = n.w.max(d.w);
        let mut f = Frag { w: w + 2.0 * pad, ..Default::default() };
        f.overlay(&n, pad + (w - n.w) / 2.0, u);
        f.overlay(&d, pad + (w - d.w) / 2.0, -v);
        f.items.push(D::Rule { x: pad, y: a - t / 2.0, w, h: t });
        f.h = f.h.max(a + t / 2.0);
        f.d = f.d.max(-(a - t / 2.0));
        f.w = w + 2.0 * pad;
        f
    }

    /// The radical sign grown to the body, the rule over it, the index in the notch (rules 11, 11a).
    fn radical(&self, degree: Option<Frag>, b: Frag, ctx: Ctx) -> Frag {
        let s = self.scale(ctx);
        let c = &self.f.constants;
        let t = c.radical_rule_thickness * s;
        let gap = if ctx.display() { c.radical_display_style_vertical_gap } else { c.radical_vertical_gap } * s;
        let target = b.h + b.d + gap + t;
        let mut sign = self.vstretch('√', target, ctx);
        let total = sign.h + sign.d;
        let gap = gap + ((total - target) / 2.0).max(0.0);
        let top = b.h + gap + t;
        let shift = top - sign.h;
        sign.raise(shift);
        let mut out = Frag::default();
        if let Some(df) = degree {
            let kb = c.radical_kern_before_degree * s;
            let ka = c.radical_kern_after_degree * s;
            let raise = -sign.d + total * c.radical_degree_bottom_raise_percent / 100.0;
            out.w = kb;
            out.hcat(&df, raise);
            out.w = (out.w + ka).max(0.0);
        }
        out.hcat(&sign, 0.0);
        out.items.push(D::Rule { x: out.w, y: top - t, w: b.w, h: t });
        out.hcat(&b, 0.0);
        out.h = out.h.max(top + c.radical_extra_ascender * s);
        out
    }

    /// Aligned cells, columns centred, the whole centred on the axis.
    fn grid(&self, cells: Vec<Vec<Frag>>, ctx: Ctx) -> Frag {
        let s = self.scale(ctx);
        let (strut_h, strut_d) = (0.84 * s, 0.36 * s);
        let ncols = cells.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut cw = vec![0.0f32; ncols];
        for r in &cells {
            for (j, f) in r.iter().enumerate() {
                cw[j] = cw[j].max(f.w);
            }
        }
        let mut xs = Vec::with_capacity(ncols);
        let mut x = 0.0;
        for (j, w) in cw.iter().enumerate() {
            if j > 0 {
                x += GRID_COLUMN_GAP * s;
            }
            xs.push(x);
            x += w;
        }
        let width = x;
        let rows: Vec<(f32, f32)> = cells
            .iter()
            .map(|r| (r.iter().map(|f| f.h).fold(strut_h, f32::max), r.iter().map(|f| f.d).fold(strut_d, f32::max)))
            .collect();
        let gap = GRID_ROW_GAP * s;
        let total: f32 = rows.iter().map(|(h, d)| h + d).sum::<f32>() + gap * rows.len().saturating_sub(1) as f32;
        let top = total / 2.0 + self.axis(ctx);
        let mut out = Frag { w: width, h: top, d: total - top, ..Default::default() };
        let mut y = top;
        for (i, r) in cells.iter().enumerate() {
            let (h, d) = rows[i];
            let base = y - h;
            for (j, f) in r.iter().enumerate() {
                out.place(f, xs[j] + (cw[j] - f.w) / 2.0, base);
            }
            y = base - d - gap;
        }
        out
    }

    /// `inner` between parentheses that are not items of any row (the automatic ones of a compound base): the
    /// bracket's own glyph, grown like typed brackets when the content is tall.
    fn fenced(&self, inner: Frag, ctx: Ctx) -> Frag {
        let s = self.scale(ctx);
        let plain = |c: char| self.char_frag(c, ctx, u32::MAX, u32::MAX);
        let open = plain('(');
        let slack = 0.3 * s;
        let tall = inner.h > open.h + slack || inner.d > open.d + slack;
        let side = |c: char| {
            if !tall {
                return plain(c);
            }
            let a = self.axis(ctx);
            let delta = (inner.h - a).max(inner.d + a);
            let target = (2.0 * delta * 0.901).max(2.0 * delta - 0.5 * s);
            if self.m.role(c) == FontRole::Text {
                self.text_delim(c, target, ctx, u32::MAX, u32::MAX)
            } else {
                self.delim(c, target, ctx)
            }
        };
        let mut out = side('(');
        out.hcat(&inner, 0.0);
        out.hcat(&side(')'), 0.0);
        out
    }

    /// A delimiter of at least `target` total height, centred on the axis.
    fn delim(&self, ch: char, target: f32, ctx: Ctx) -> Frag {
        if self.o.vector_delimiters && is_vector_delimiter(ch) {
            // the width the math font's grown bracket would take, drawn as strokes
            let st = self.vstretch(ch, target, ctx);
            return self.stroked_delim(ch, st.w, target, ctx, u32::MAX, u32::MAX);
        }
        let mut f = self.vstretch(ch, target, ctx);
        let shift = self.axis(ctx) - (f.h - f.d) / 2.0;
        f.raise(shift);
        f
    }

    /// The text font's glyph of `ch` stretched vertically so its height reaches `target`, centred on the axis; it keeps its place
    /// in the text (`row`, `index`) for carets and selection.
    fn text_delim(&self, ch: char, target: f32, ctx: Ctx, row: u32, index: u32) -> Frag {
        let s = self.scale(ctx);
        let m = self.m.char(ch, FontRole::Text);
        if self.o.vector_delimiters && is_vector_delimiter(ch) {
            // drawn as strokes in the bracket's own width, as tall as the content
            return self.stroked_delim(ch, m.advance * s, target, ctx, row, index);
        }
        // grown vertically only: the same width and strokes as the bracket at its normal size
        let k = (target / ((m.ascent + m.descent) * s)).max(1.0);
        let mut f = Frag {
            w: m.advance * s,
            h: m.ascent * s * k,
            d: m.descent * s * k,
            items: vec![D::Char { c: ch, role: FontRole::Text, size: s, x: 0.0, y: 0.0, row, index, w: m.advance * s, sy: k }],
            stops: vec![],
            ink: vec![],
        };
        let shift = self.axis(ctx) - (f.h - f.d) / 2.0;
        f.raise(shift);
        f
    }

    /// A bracket drawn as strokes ([`Draw::Delim`]): `w` wide, `target` tall, centred on the axis. The stroke is a
    /// little heavier than a fraction bar, the weight of the text's own brackets.
    fn stroked_delim(&self, ch: char, w: f32, target: f32, ctx: Ctx, row: u32, index: u32) -> Frag {
        let s = self.scale(ctx);
        let t = (1.25 * self.f.constants.fraction_rule_thickness * s).max(0.05 * s);
        let h = target.max(s);
        let a = self.axis(ctx);
        let bottom = a - h / 2.0;
        Frag {
            w,
            h: bottom + h,
            d: -bottom,
            items: vec![D::Delim { c: ch, x: 0.0, y: bottom, w, h, t, row, index }],
            stops: vec![],
            ink: vec![],
        }
    }

    /// The math-font glyph of `ch` grown vertically to at least `target`.
    fn vstretch(&self, ch: char, target: f32, ctx: Ctx) -> Frag {
        let s = self.scale(ctx);
        let g = self.f.glyph(ch).unwrap_or(0);
        let st: Stretch = self.f.stretch(g, target / s, true);
        Frag {
            w: st.advance * s,
            h: st.height * s,
            d: st.depth * s,
            items: st.pieces.iter().map(|p| D::Glyph { glyph: p.glyph, size: s, x: p.x * s, y: p.y * s }).collect(),
            stops: vec![],
            ink: vec![],
        }
    }
}

/// The class of a character for spacing and breaking.
fn char_class(c: char) -> Class {
    if c == ' ' || c == '\t' {
        Class::Space
    } else if is_open(c) {
        Class::Open
    } else if is_close(c) {
        Class::Close
    } else if matches!(c, ',' | ';') {
        Class::Punct
    } else if is_binary(c) {
        Class::Bin
    } else if is_relation(c) {
        Class::Rel
    } else {
        Class::Ord
    }
}

/// The only character of a row.
fn single_char(r: &Row) -> Option<char> {
    match r.0.as_slice() {
        [Item::Char(c)] => Some(*c),
        _ => None,
    }
}

/// A fraction of two integers (drawn small next to other items).
fn is_small_rational(n: &Node) -> bool {
    let digits = |r: &Row| r.plain().is_some_and(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()));
    matches!(n, Node::Fraction { num, den } if digits(num) && digits(den))
}

/// In traditional form a single-letter name is drawn in math italics.
fn traditional_char(row: &Row, k: usize, c: char) -> char {
    if !c.is_ascii_alphabetic() {
        return c;
    }
    let word = |j: Option<usize>| j.and_then(|j| row.0.get(j)).is_some_and(|i| matches!(i, Item::Char(c) if is_word(*c)));
    if word(k.checked_sub(1)) || word(Some(k + 1)) {
        return c;
    }
    let base = if c.is_ascii_lowercase() { 0x1D44E + (c as u32 - 'a' as u32) } else { 0x1D434 + (c as u32 - 'A' as u32) };
    // the italic h is the Planck constant's code point
    if c == 'h' {
        return 'ℎ';
    }
    char::from_u32(base).unwrap_or(c)
}
