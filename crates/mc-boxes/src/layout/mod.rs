//! Box layout (design 08, sections 4.7 and 4.8): TeX's math rules (TeXbook Appendix G) driven by the OpenType
//! MATH constants of the bundled font, over a [`Metrics`] trait for the painter's text font, with line breaking
//! of long rows.
//!
//! [`layout`] turns a [`Row`] into a [`Layout`]: positioned characters, math-font glyphs and rules ([`Draw`]),
//! the lines of the root row, and caret geometry for every row of the tree ([`Layout::caret`], [`Layout::hit`]).
//! Coordinates are in the units of [`LayoutOptions::size`] (the font size, say pixels), x to the right and y down
//! from the top of the layout; character positions are baselines.
mod engine;
mod lines;
pub mod metrics;

pub use metrics::{role_of, CharMetrics, FontRole, MathFontMetrics, Metrics, MonoMetrics};

use crate::tree::{Row, RowPath, Step};
use std::fmt::Write;

/// How to lay out a row.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutOptions {
    /// the font size: one em in output units
    pub size: f32,
    /// display style (outputs, display formulas): big operators with limits above and below and in their large
    /// size, a derivative as `∂f/∂x`; otherwise inline style (inputs)
    pub display: bool,
    /// the width to wrap at, in output units (`None`: never wrap except at new lines)
    pub max_width: Option<f32>,
    /// indent continuation lines by the bracket depth at the break (inputs); outputs start them at the left
    pub indent_continuations: bool,
    /// thin spaces around binary operators, `=`, arrows and comparisons, and two-character operators drawn as one
    /// glyph (`->` as `→`); off for Code cells
    pub operator_spacing: bool,
    /// traditional form: single-letter names in italics
    pub traditional: bool,
    /// a fraction of two numbers next to other items is drawn at script size (design 08, section 13, item 22)
    pub small_inline_rationals: bool,
    /// brackets grown around tall content are drawn as strokes ([`Draw::Delim`], [`delimiter_paths`]) instead of
    /// font glyphs, so tall brackets keep the weight of the text around them; painters that cannot stroke paths
    /// (plot labels) turn it off and get the math font's glyphs
    pub vector_delimiters: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            size: 1.0,
            display: false,
            max_width: None,
            indent_continuations: true,
            operator_spacing: true,
            traditional: false,
            small_inline_rationals: true,
            vector_delimiters: true,
        }
    }
}

impl LayoutOptions {
    /// Options for an input cell (inline style, wrapping with indented continuations).
    pub fn input(size: f32, max_width: Option<f32>) -> Self {
        LayoutOptions { size, max_width, ..Default::default() }
    }

    /// Options for an output cell (display style for big operators, continuations at the left edge).
    pub fn output(size: f32, max_width: Option<f32>) -> Self {
        LayoutOptions { size, max_width, display: true, indent_continuations: false, ..Default::default() }
    }
}

/// One painted element. Positions are in output units from the layout's top-left corner, y down.
#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    /// a character of the text or the math font, its baseline origin at (x, y), drawn at font size `size`
    Char {
        /// the character (already mapped to its display form: `→` for `->`, italic letters in traditional form)
        c: char,
        /// which font
        role: FontRole,
        /// font size in output units
        size: f32,
        /// origin x
        x: f32,
        /// baseline y
        y: f32,
        /// the row it belongs to (index into [`Layout::rows`]) and its first item index there, for colouring;
        /// `u32::MAX` for symbols the layout adds itself (`∑`, `=` and `→` of limits, `d` of integrals, `∂`)
        row: u32,
        /// item index in that row
        index: u32,
        /// its advance in output units (where the next character of the same run starts)
        advance: f32,
        /// vertical stretch about the baseline (1 for ordinary text; more for a bracket grown around tall content,
        /// drawn as the same glyph made taller)
        stretch: f32,
    },
    /// a placeholder `□` (an empty slot), drawn in the math font at `size`
    Placeholder {
        /// origin x
        x: f32,
        /// baseline y
        y: f32,
        /// font size
        size: f32,
        /// the row and item index of the placeholder
        row: u32,
        /// item index in that row
        index: u32,
    },
    /// a math-font glyph by id (size variants and assembly pieces of stretched delimiters, radical signs, big
    /// operators), its baseline origin at (x, y), drawn at font size `size`; paint it from
    /// [`mc_mathfont::rasterize`] or [`mc_mathfont::MathFont::outline`]
    Glyph {
        /// glyph id in [`mc_mathfont::font`]
        glyph: u16,
        /// font size in output units
        size: f32,
        /// origin x
        x: f32,
        /// baseline y
        y: f32,
    },
    /// a filled rectangle (fraction bars, radical overbars) with its top-left corner at (x, y)
    Rule {
        /// left
        x: f32,
        /// top
        y: f32,
        /// width
        w: f32,
        /// height
        h: f32,
    },
    /// a bracket grown around tall content, drawn as strokes: its box has its top-left corner at (x, y); paint the
    /// polylines of [`delimiter_paths`] with a stroke of width `stroke`
    Delim {
        /// the bracket: one of the characters [`is_vector_delimiter`] accepts
        c: char,
        /// left
        x: f32,
        /// top
        y: f32,
        /// width (the bracket's advance)
        w: f32,
        /// height
        h: f32,
        /// stroke width
        stroke: f32,
        /// the row it belongs to and its item index there (`u32::MAX` when the layout added it), for colouring
        row: u32,
        /// item index in that row
        index: u32,
    },
    /// a graphic's frame with its top-left corner at (x, y)
    Graphic {
        /// the image path from the box
        path: String,
        /// left
        x: f32,
        /// top
        y: f32,
        /// width
        w: f32,
        /// height
        h: f32,
    },
}

/// One line of the root row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    /// top of the line
    pub top: f32,
    /// its baseline
    pub baseline: f32,
    /// bottom of the line
    pub bottom: f32,
    /// first item of the root row on this line
    pub start: usize,
    /// one past the last item on this line
    pub end: usize,
    /// left indentation of the line (where its first item starts)
    pub indent: f32,
    /// where the line ends: the right edge of its last item
    pub right: f32,
}

/// A caret position's geometry (design 11, section 3.1): its x and baseline, and the extent of a text line at the
/// position's size (the scale of its row) above and below the baseline, however tall the line around it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    /// x of the caret
    pub x: f32,
    /// baseline of the row at that position
    pub baseline: f32,
    /// top of the caret: the baseline less the text line's ascent at the row's size
    pub top: f32,
    /// bottom of the caret: the baseline plus the text line's descent at the row's size
    pub bottom: f32,
}

impl Stop {
    /// The text line's ascent at this position.
    pub fn ascent(&self) -> f32 {
        self.baseline - self.top
    }

    /// The text line's descent at this position.
    pub fn descent(&self) -> f32 {
        self.bottom - self.baseline
    }
}

/// The box of one item of a row as drawn: its ink extent, at least a text line of the row's size (selection
/// rectangles and hit tests use it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItemBox {
    /// left edge
    pub x: f32,
    /// width
    pub w: f32,
    /// top
    pub top: f32,
    /// bottom
    pub bottom: f32,
}

/// The caret geometry of one row of the tree.
#[derive(Clone, Debug, PartialEq)]
pub struct RowGeom {
    /// where the row is in the tree
    pub path: RowPath,
    /// one stop per caret offset `0..=len`
    pub stops: Vec<Stop>,
    /// one box per item `0..len`
    pub ink: Vec<ItemBox>,
}

/// A laid-out row.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Layout {
    /// total width
    pub width: f32,
    /// total height
    pub height: f32,
    /// baseline of the first line
    pub baseline: f32,
    /// the painted elements
    pub items: Vec<Draw>,
    /// the lines of the root row (at least one)
    pub lines: Vec<Line>,
    /// caret geometry of every row, the root first
    pub rows: Vec<RowGeom>,
}

impl Layout {
    /// The caret at `offset` of the row at `path`.
    pub fn caret(&self, path: &[Step], offset: usize) -> Option<Stop> {
        self.rows.iter().find(|r| r.path == path)?.stops.get(offset).copied()
    }

    /// The box of item `index` of the row at `path`.
    pub fn item_box(&self, path: &[Step], index: usize) -> Option<ItemBox> {
        self.rows.iter().find(|r| r.path == path)?.ink.get(index).copied()
    }

    /// The rectangles that show items `range` of the row at `path` selected, one per visual line they are on: the
    /// union of the items' boxes on that line (x, top, width, height).
    pub fn selection_boxes(&self, path: &[Step], range: std::ops::Range<usize>) -> Vec<ItemBox> {
        let Some(r) = self.rows.iter().find(|r| r.path == path) else { return vec![] };
        let mut out: Vec<(usize, ItemBox)> = vec![];
        for k in range {
            let Some(b) = r.ink.get(k).copied() else { continue };
            let line = self.line_at((b.top + b.bottom) / 2.0);
            match out.last_mut() {
                Some((l, u)) if *l == line => {
                    let right = (u.x + u.w).max(b.x + b.w);
                    u.x = u.x.min(b.x);
                    u.w = right - u.x;
                    u.top = u.top.min(b.top);
                    u.bottom = u.bottom.max(b.bottom);
                }
                _ => out.push((line, b)),
            }
        }
        out.into_iter().map(|(_, b)| b).collect()
    }

    /// The caret position nearest to a point (design 11, section 3.4): the item box holding the point or nearest to
    /// it (the deepest row's on ties, so a click on a numerator lands in it), then the nearest stop of that row; a
    /// tree without items falls back to the nearest stop.
    pub fn hit(&self, x: f32, y: f32) -> (RowPath, usize) {
        let mut best: Option<(f32, usize, usize)> = None;
        for (ri, r) in self.rows.iter().enumerate() {
            for b in &r.ink {
                let dx = if x < b.x { b.x - x } else if x > b.x + b.w { x - b.x - b.w } else { 0.0 };
                let dy = if y < b.top { b.top - y } else if y > b.bottom { y - b.bottom } else { 0.0 };
                // among boxes at the same distance (a base's end and its exponent's start) the one centred nearest
                let d = dx + 4.0 * dy + 0.01 * (y - (b.top + b.bottom) / 2.0).abs();
                let better = match best {
                    None => true,
                    Some((bd, depth, _)) => d < bd - 1e-4 || ((d - bd).abs() <= 1e-4 && r.path.len() > depth),
                };
                if better {
                    best = Some((d, r.path.len(), ri));
                }
            }
        }
        match best {
            Some((_, _, ri)) => self.nearest_stop(x, y, Some(ri)),
            None => self.nearest_stop(x, y, None),
        }
    }

    /// The stop nearest to a point, in row `only` (an index into [`Layout::rows`]) or in any row: vertical distance
    /// weighs more (a point is on a line before it is near a stop); among stops at the same place the one centred
    /// nearest, then the deepest row's, wins.
    fn nearest_stop(&self, x: f32, y: f32, only: Option<usize>) -> (RowPath, usize) {
        let mut best: Option<(f32, usize, &RowPath, usize)> = None;
        for (ri, r) in self.rows.iter().enumerate() {
            if only.is_some_and(|o| o != ri) {
                continue;
            }
            for (k, s) in r.stops.iter().enumerate() {
                let dy = if y < s.top {
                    s.top - y
                } else if y > s.bottom {
                    y - s.bottom
                } else {
                    0.0
                };
                let centre = (y - (s.top + s.bottom) / 2.0).abs();
                let d = (x - s.x).abs() + 4.0 * dy + 0.01 * centre;
                let better = match best {
                    None => true,
                    Some((bd, depth, _, _)) => d < bd - 1e-4 || ((d - bd).abs() <= 1e-4 && r.path.len() > depth),
                };
                if better {
                    best = Some((d, r.path.len(), &r.path, k));
                }
            }
        }
        best.map_or((Vec::new(), 0), |(_, _, p, k)| (p.clone(), k))
    }

    /// The line holding vertical position `y` (a baseline or an edge of something drawn on that line): lines are
    /// stacked without overlapping, so it is the first whose bottom is at or below `y`.
    pub fn line_at(&self, y: f32) -> usize {
        self.lines.partition_point(|ln| ln.bottom + 0.01 < y).min(self.lines.len().saturating_sub(1))
    }

    /// Moves every line right so that it ends at `width` (right-justified lines, the notebook's outputs); a line
    /// wider than `width` stays where it is. Everything follows its line: the painted items, the caret stops of
    /// every row, the lines' `indent` and `right`. The layout's width becomes at least `width`.
    pub fn align_right(&mut self, width: f32) {
        let shift: Vec<f32> = self.lines.iter().map(|l| (width - l.right).max(0.0)).collect();
        if shift.iter().all(|&s| s == 0.0) {
            return;
        }
        let at = |lines: &[Line], y: f32| lines.partition_point(|ln| ln.bottom + 0.01 < y).min(lines.len().saturating_sub(1));
        for d in &mut self.items {
            let (x, y) = match d {
                Draw::Char { x, y, .. } | Draw::Placeholder { x, y, .. } | Draw::Glyph { x, y, .. } => (x, *y),
                // rules and frames are positioned by their top-left corner: their middle tells the line
                Draw::Rule { x, y, h, .. } | Draw::Graphic { x, y, h, .. } | Draw::Delim { x, y, h, .. } => {
                    (x, *y + *h / 2.0)
                }
            };
            *x += shift[at(&self.lines, y)];
        }
        for r in &mut self.rows {
            for s in &mut r.stops {
                s.x += shift[at(&self.lines, s.baseline)];
            }
            for b in &mut r.ink {
                b.x += shift[at(&self.lines, (b.top + b.bottom) / 2.0)];
            }
        }
        for (l, s) in self.lines.iter_mut().zip(&shift) {
            l.indent += s;
            l.right += s;
        }
        self.width = self.width.max(width);
    }

    /// Moves everything by (dx, dy): the painted items, the caret stops, the lines and the first baseline (the size
    /// stays; a painter lets a line's superscripts reach into the space above the layout this way).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        for d in &mut self.items {
            match d {
                Draw::Char { x, y, .. }
                | Draw::Placeholder { x, y, .. }
                | Draw::Glyph { x, y, .. }
                | Draw::Rule { x, y, .. }
                | Draw::Delim { x, y, .. }
                | Draw::Graphic { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
            }
        }
        for r in &mut self.rows {
            for s in &mut r.stops {
                s.x += dx;
                s.baseline += dy;
                s.top += dy;
                s.bottom += dy;
            }
            for b in &mut r.ink {
                b.x += dx;
                b.top += dy;
                b.bottom += dy;
            }
        }
        for l in &mut self.lines {
            l.top += dy;
            l.baseline += dy;
            l.bottom += dy;
            l.indent += dx;
            l.right += dx;
        }
        self.baseline += dy;
    }

    /// A text dump with every number rounded to 0.1 output unit, for snapshot tests.
    pub fn snapshot(&self) -> String {
        let f = |v: f32| {
            let r = (v * 10.0).round() / 10.0;
            if r == 0.0 {
                "0.0".to_string()
            } else {
                format!("{r:.1}")
            }
        };
        let mut s = String::new();
        let _ = writeln!(s, "box {} x {} baseline {}", f(self.width), f(self.height), f(self.baseline));
        for l in &self.lines {
            let _ = writeln!(
                s,
                "line {}..{} top {} baseline {} bottom {} indent {}",
                l.start,
                l.end,
                f(l.top),
                f(l.baseline),
                f(l.bottom),
                f(l.indent)
            );
        }
        for d in &self.items {
            let _ = match d {
                Draw::Char { c, role, size, x, y, .. } => {
                    let r = if *role == FontRole::Math { "math" } else { "text" };
                    writeln!(s, "char {c:?} {r} {} at {} {}", f(*size), f(*x), f(*y))
                }
                Draw::Placeholder { x, y, size, .. } => writeln!(s, "placeholder {} at {} {}", f(*size), f(*x), f(*y)),
                Draw::Glyph { glyph, size, x, y } => writeln!(s, "glyph {glyph} {} at {} {}", f(*size), f(*x), f(*y)),
                Draw::Rule { x, y, w, h } => writeln!(s, "rule {} {} {} x {}", f(*x), f(*y), f(*w), f(*h)),
                Draw::Delim { c, x, y, w, h, stroke, .. } => {
                    writeln!(s, "delim {c:?} {} {} {} x {} stroke {}", f(*x), f(*y), f(*w), f(*h), f(*stroke))
                }
                Draw::Graphic { path, x, y, w, h } => writeln!(s, "graphic {path:?} {} {} {} x {}", f(*x), f(*y), f(*w), f(*h)),
            };
        }
        for r in &self.rows {
            let path: Vec<String> = r.path.iter().map(|p| format!("{}.{}", p.item, p.slot)).collect();
            let xs: Vec<String> = r.stops.iter().map(|p| f(p.x)).collect();
            let _ = writeln!(s, "row [{}] stops {}", path.join(" "), xs.join(" "));
        }
        s
    }
}

/// Lays out a row.
pub fn layout(row: &Row, metrics: &dyn Metrics, opts: &LayoutOptions) -> Layout {
    engine::Engine::new(metrics, opts).run(row)
}

/// The layouts of the logical lines of a root row, kept between calls of [`layout_by_lines`]: an edit lays out again
/// only the lines it changed (a long cell types as fast as a short one), and the layout the last call composed is
/// changed in place where the row changed (round 37: composing every line again cost 3.5 ms a keystroke in a
/// 2,000-line cell). A line that fits on one line is laid out once for every bracket depth it starts at (round 38: an
/// opening bracket typed at the top of a long cell moves the lines after it right, it does not lay them out again).
#[derive(Clone, Debug, Default)]
pub struct LineCache {
    opts: Option<LayoutOptions>,
    /// by a hash of the line's items: the items, the depths the layout serves, the layout, the call it was last used in
    lines: std::collections::HashMap<u64, Vec<CachedLine>>,
    calls: u64,
    /// the layout the last call gave, and where each of its logical lines is in it
    last: Option<Composed>,
}

/// A logical line's items, the bracket depths its layout serves, its layout, and the call it was last used in.
type CachedLine = (Arc<[crate::tree::Item]>, Fit, Arc<Layout>, u64);

/// The bracket depths at which a logical line's layout is the line's.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fit {
    /// laid out at depth 0 on one line: every depth at which it still fits on one line, moved right by the indent of
    /// that depth (nothing else about it depends on the depth)
    Any(engine::RootFit),
    /// laid out at this depth (it takes more than one line there)
    At(usize),
}

impl Fit {
    /// How far right (size units) the layout is drawn for a line starting at bracket depth `depth`, if it is that
    /// line's layout: the first indent the line breaker gives at that depth (two ems a level, at most half the width),
    /// as long as the line still fits beside it.
    fn shift(self, depth: usize, opts: &LayoutOptions) -> Option<f32> {
        match self {
            Fit::At(d) => (d == depth).then_some(0.0),
            Fit::Any(_) if depth == 0 => Some(0.0),
            Fit::Any(f) => {
                let per = if opts.indent_continuations { engine::INDENT_PER_LEVEL } else { 0.0 };
                let level = if f.first_closes { depth - 1 } else { depth };
                let max = opts.max_width.map(|w| w / opts.size);
                let indent = (per * level as f32).min(max.unwrap_or(f32::INFINITY) * 0.5);
                if max.is_some_and(|m| f.reach > (m - indent).max(m * 0.5)) {
                    return None;
                }
                Some(indent * opts.size)
            }
        }
    }
}

/// A composed layout and its logical lines, in order.
#[derive(Clone, Debug)]
struct Composed {
    layout: Arc<Layout>,
    parts: Vec<Part>,
}

/// One logical line of a composed layout: what it is (its items, the depths its layout serves, their hash, its own
/// layout and how far right it is drawn) and where its pieces are in the composed layout.
#[derive(Clone, Debug)]
struct Part {
    items: Arc<[crate::tree::Item]>,
    fit: Fit,
    hash: u64,
    lay: Arc<Layout>,
    /// its horizontal offset
    sx: f32,
    /// its vertical offset
    dy: f32,
    /// its first root item
    off: usize,
    /// its painted items, root stops, root item boxes, other rows and lines
    at: Spans,
}

/// Where a logical line's pieces are in a composed layout (index ranges).
#[derive(Clone, Debug, Default)]
struct Spans {
    items: Range<usize>,
    stops: Range<usize>,
    ink: Range<usize>,
    rows: Range<usize>,
    lines: Range<usize>,
}

use std::ops::Range;
use std::sync::Arc;

/// A quick hash of a line's items (FNV-1a over the characters; boxes through their own hash).
fn line_hash(items: &[crate::tree::Item]) -> u64 {
    use crate::tree::Item;
    use std::hash::{Hash, Hasher};
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for it in items {
        let v = match it {
            Item::Char(c) => *c as u64,
            Item::Placeholder => 0x1_0000_0000,
            Item::Box(b) => {
                let mut s = std::collections::hash_map::DefaultHasher::new();
                b.hash(&mut s);
                s.finish()
            }
        };
        h = (h ^ v).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The x of a painted item.
fn draw_x(d: &mut Draw) -> &mut f32 {
    match d {
        Draw::Char { x, .. }
        | Draw::Placeholder { x, .. }
        | Draw::Glyph { x, .. }
        | Draw::Rule { x, .. }
        | Draw::Delim { x, .. }
        | Draw::Graphic { x, .. } => x,
    }
}

/// The layout of the logical line `part` starting at bracket depth `depth` (hash `h`), and how far right it is
/// drawn: from the cache, or laid out now (at depth 0 first: on one line there, it serves every depth it fits at).
fn line_layout(
    lines: &mut std::collections::HashMap<u64, Vec<CachedLine>>,
    part: &[crate::tree::Item],
    (depth, h, call): (usize, u64, u64),
    metrics: &dyn Metrics,
    opts: &LayoutOptions,
) -> (Arc<[crate::tree::Item]>, Fit, Arc<Layout>, f32) {
    let bucket = lines.entry(h).or_default();
    if let Some(e) = bucket.iter_mut().find(|e| e.1.shift(depth, opts).is_some() && *e.0 == *part) {
        e.3 = call;
        let sx = e.1.shift(depth, opts).unwrap_or(0.0);
        return (e.0.clone(), e.1, e.2.clone(), sx);
    }
    let items: Arc<[crate::tree::Item]> = bucket.iter().find(|e| *e.0 == *part).map_or_else(|| part.into(), |e| e.0.clone());
    if !bucket.iter().any(|e| matches!(e.1, Fit::Any(_) | Fit::At(0)) && *e.0 == *part) {
        let (lay, fit) = engine::Engine::new(metrics, opts).run_with_fit(&Row(part.to_vec()));
        let fit = fit.map_or(Fit::At(0), Fit::Any);
        let lay = Arc::new(lay);
        bucket.push((items.clone(), fit, lay.clone(), call));
        if let Some(sx) = fit.shift(depth, opts) {
            return (items, fit, lay, sx);
        }
    }
    // the bracket depth the line starts at indents it as in the whole row, and decides where it breaks
    let lay = Arc::new(engine::Engine::new(metrics, opts).starting_at_depth(depth).run(&Row(part.to_vec())));
    bucket.push((items.clone(), Fit::At(depth), lay.clone(), call));
    (items, Fit::At(depth), lay, 0.0)
}

/// The vertical offset of a logical line laid out as `lay` below the composed line `prev` (none: the first line).
fn line_offset(metrics: &dyn Metrics, size: f32, prev: Option<&Line>, lay: &Layout) -> f32 {
    match (prev, lay.lines.first()) {
        (Some(p), Some(f)) => {
            let skip = engine::line_skip(metrics, (p.bottom - p.baseline) / size, (f.baseline - f.top) / size, 1.0) * size;
            p.baseline + skip - f.baseline
        }
        _ => 0.0,
    }
}

/// The pieces logical lines add to a composed layout, and where the next ones go.
struct Pieces<'m> {
    metrics: &'m dyn Metrics,
    size: f32,
    items: Vec<Draw>,
    stops: Vec<Stop>,
    ink: Vec<ItemBox>,
    rows: Vec<RowGeom>,
    lines: Vec<Line>,
    /// where the next line's pieces start in the composed layout
    next: Spans,
    /// the first root item of the next line
    off: usize,
    /// the composed layout's last line so far
    prev: Option<Line>,
}

/// A logical line to compose: its items, the depths its layout serves, their hash, its layout, how far right it is
/// drawn.
type Placed = (Arc<[crate::tree::Item]>, Fit, u64, Arc<Layout>, f32);

impl<'m> Pieces<'m> {
    /// Nothing yet: the next pieces go at `at` (the starts), the next root item is `off`, the line above is `prev`.
    fn at(metrics: &'m dyn Metrics, size: f32, at: &Spans, off: usize, prev: Option<Line>) -> Pieces<'m> {
        let s = |r: &Range<usize>| r.start..r.start;
        let next = Spans { items: s(&at.items), stops: s(&at.stops), ink: s(&at.ink), rows: s(&at.rows), lines: s(&at.lines) };
        Pieces { metrics, size, items: vec![], stops: vec![], ink: vec![], rows: vec![], lines: vec![], next, off, prev }
    }

    /// Add a logical line (`last`: the row's last line, which has no line break of its own) below the pieces so far;
    /// gives its part.
    fn push(&mut self, (items, fit, hash, lay, sx): Placed, last: bool) -> Part {
        let dy = line_offset(self.metrics, self.size, self.prev.as_ref(), &lay);
        let (off, n) = (self.off, items.len());
        // the line's other rows follow the composed rows so far (the root row is row 0)
        let base_row = (self.next.rows.end as u32).saturating_sub(1);
        let map_row = |r: u32| if r == u32::MAX { r } else if r == 0 { 0 } else { base_row + r };
        let map_index = |r: u32, i: u32| if r == 0 { i + off as u32 } else { i };
        let (i0, s0, k0, r0, l0) = (self.items.len(), self.stops.len(), self.ink.len(), self.rows.len(), self.lines.len());
        for d in &lay.items {
            let mut d = d.clone();
            *draw_x(&mut d) += sx;
            match &mut d {
                Draw::Char { y, row, index, .. } | Draw::Placeholder { y, row, index, .. } | Draw::Delim { y, row, index, .. } => {
                    *y += dy;
                    *index = map_index(*row, *index);
                    *row = map_row(*row);
                }
                Draw::Glyph { y, .. } | Draw::Rule { y, .. } | Draw::Graphic { y, .. } => *y += dy,
            }
            self.items.push(d);
        }
        let shift_stop = |s: &Stop| Stop { x: s.x + sx, baseline: s.baseline + dy, top: s.top + dy, bottom: s.bottom + dy };
        let shift_box = |b: &ItemBox| ItemBox { x: b.x + sx, top: b.top + dy, bottom: b.bottom + dy, ..*b };
        for (r, g) in lay.rows.iter().enumerate() {
            if r == 0 {
                let k = g.stops.len().saturating_sub(1);
                // the stop after a line break is the next line's first
                let take = if last { k + 1 } else { k };
                self.stops.extend(g.stops.iter().take(take).map(shift_stop));
                self.ink.extend(g.ink.iter().map(shift_box));
            } else {
                let mut path = g.path.clone();
                if let Some(first) = path.first_mut() {
                    first.item += off;
                }
                self.rows.push(RowGeom { path, stops: g.stops.iter().map(shift_stop).collect(), ink: g.ink.iter().map(shift_box).collect() });
            }
        }
        // a line ending in a line break has an empty last line of its own: the next logical line is that line
        let lines = if last { &lay.lines[..] } else { &lay.lines[..lay.lines.len().saturating_sub(1).max(1)] };
        for l in lines {
            self.lines.push(Line {
                top: l.top + dy,
                baseline: l.baseline + dy,
                bottom: l.bottom + dy,
                start: l.start + off,
                end: (l.end + off).min(off + n),
                indent: l.indent + sx,
                right: l.right + sx,
            });
        }
        self.prev = self.lines.last().copied();
        let grow = |r: &mut Range<usize>, by: usize| {
            let at = r.end..r.end + by;
            r.end += by;
            at
        };
        let at = Spans {
            items: grow(&mut self.next.items, self.items.len() - i0),
            stops: grow(&mut self.next.stops, self.stops.len() - s0),
            ink: grow(&mut self.next.ink, self.ink.len() - k0),
            rows: grow(&mut self.next.rows, self.rows.len() - r0),
            lines: grow(&mut self.next.lines, self.lines.len() - l0),
        };
        self.off += n;
        Part { items, fit, hash, lay, sx, dy, off, at }
    }

    /// Put the pieces in place of `old` (ranges of `lay`).
    fn splice_into(self, lay: &mut Layout, old: &Spans) {
        lay.items.splice(old.items.clone(), self.items);
        lay.rows[0].stops.splice(old.stops.clone(), self.stops);
        lay.rows[0].ink.splice(old.ink.clone(), self.ink);
        lay.rows.splice(old.rows.clone(), self.rows);
        lay.lines.splice(old.lines.clone(), self.lines);
    }
}

/// Draw the composed line `p` of `out` again at its own `sx` (its bracket depth changed, its layout did not): every
/// x from its own layout, as [`Pieces::push`] places it.
fn move_part(out: &mut Layout, p: &Part) {
    let sx = p.sx;
    for (d, src) in out.items[p.at.items.clone()].iter_mut().zip(&p.lay.items) {
        let mut s = src.clone();
        *draw_x(d) = *draw_x(&mut s) + sx;
    }
    let root = &p.lay.rows[0];
    for (s, src) in out.rows[0].stops[p.at.stops.clone()].iter_mut().zip(&root.stops) {
        s.x = src.x + sx;
    }
    for (b, src) in out.rows[0].ink[p.at.ink.clone()].iter_mut().zip(&root.ink) {
        b.x = src.x + sx;
    }
    for (g, src) in out.rows[p.at.rows.clone()].iter_mut().zip(&p.lay.rows[1..]) {
        for (s, from) in g.stops.iter_mut().zip(&src.stops) {
            s.x = from.x + sx;
        }
        for (b, from) in g.ink.iter_mut().zip(&src.ink) {
            b.x = from.x + sx;
        }
    }
    for (l, src) in out.lines[p.at.lines.clone()].iter_mut().zip(&p.lay.lines) {
        l.indent = src.indent + sx;
        l.right = src.right + sx;
    }
}

/// Lays out a row line by line: each logical line (up to and with its line break) on its own, with the bracket
/// depth it starts at, the lines stacked as [`layout`] stacks them. The result is [`layout`]'s, except that a
/// bracket spanning lines is not grown for tall content on another line. Unchanged lines come from `cache`.
pub fn layout_by_lines(row: &Row, metrics: &dyn Metrics, opts: &LayoutOptions, cache: &mut LineCache) -> Layout {
    let lay = layout_by_lines_shared(row, metrics, opts, cache);
    // the cache keeps its own for the next call
    (*lay).clone()
}

/// [`layout_by_lines`], the layout shared with the cache. The next call changes it in place where the row changed,
/// when nothing else holds it then (drop it before): a keystroke in a long cell costs the lines it changed and
/// integer shifts of what follows them (their item numbers), not a composition of every line; lines whose bracket
/// depth changed are moved sideways, not laid out again. The lines after a change of height are composed again from
/// their own layouts, so the result is always what a composition from the start gives, number for number.
pub fn layout_by_lines_shared(row: &Row, metrics: &dyn Metrics, opts: &LayoutOptions, cache: &mut LineCache) -> Arc<Layout> {
    use crate::tree::Item;
    if cache.opts.as_ref() != Some(opts) {
        *cache = LineCache { opts: Some(opts.clone()), ..LineCache::default() };
    }
    cache.calls += 1;
    let call = cache.calls;
    // the logical lines: item ranges, each ending after its line break (the last one may be empty), with the
    // bracket depth each starts at
    let mut segs = vec![];
    let mut depths = vec![0usize];
    let mut start = 0;
    let mut depth = 0usize;
    for (i, it) in row.0.iter().enumerate() {
        match it {
            Item::Char('\n') => {
                segs.push(start..i + 1);
                depths.push(depth);
                start = i + 1;
            }
            Item::Char(c) if crate::ops::is_open(*c) => depth += 1,
            Item::Char(c) if crate::ops::is_close(*c) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    segs.push(start..row.len());
    if segs.len() == 1 {
        cache.last = None;
        return Arc::new(layout(row, metrics, opts));
    }
    let n = segs.len();
    let size = opts.size;
    // the lines the last layout shares with this row at its start (`prefix`) and at its end (`suffix`): the same
    // items, and a layout that serves the depth they start at now
    let same = |p: &Part, k: usize| *p.items == row.0[segs[k].clone()] && p.fit.shift(depths[k], opts).is_some();
    let (prefix, suffix) = match &cache.last {
        Some(c) => {
            let old = &c.parts;
            let mut a = 0;
            while a < old.len() && a < n && same(&old[a], a) {
                a += 1;
            }
            let mut z = 0;
            while z < old.len() - a && z < n - a && same(&old[old.len() - 1 - z], n - 1 - z) {
                z += 1;
            }
            (a, z)
        }
        None => (0, 0),
    };
    // the layout to change, the lines kept before and after the change, and the lines replaced
    let fresh = || Layout { rows: vec![RowGeom { path: vec![], stops: Vec::with_capacity(row.len() + 1), ink: Vec::with_capacity(row.len()) }], ..Layout::default() };
    let (mut out, mut parts, mut tail, replaced) = match cache.last.take() {
        Some(Composed { layout, mut parts }) if prefix + suffix > 0 => {
            // no copy when the caller let go of the last layout
            let lay = Arc::try_unwrap(layout).unwrap_or_else(|a| (*a).clone());
            let tail = parts.split_off(parts.len() - suffix);
            let replaced = parts.split_off(prefix);
            (lay, parts, tail, replaced)
        }
        _ => (fresh(), vec![], vec![], vec![]),
    };
    // how far right the kept lines after the change are drawn at the depth they start at now
    let tail_sx: Vec<f32> = tail.iter().enumerate().map(|(i, p)| p.fit.shift(depths[n - suffix + i], opts).unwrap_or(0.0)).collect();
    // where the replaced lines are: from the end of the kept lines before them to the start of those after them
    let total = Spans {
        items: 0..out.items.len(),
        stops: 0..out.rows[0].stops.len(),
        ink: 0..out.rows[0].ink.len(),
        rows: 1..out.rows.len(),
        lines: 0..out.lines.len(),
    };
    let span = |f: &dyn Fn(&Spans) -> &Range<usize>| {
        parts.last().map_or(f(&total).start, |p| f(&p.at).end)..tail.first().map_or(f(&total).end, |p| f(&p.at).start)
    };
    let old = Spans { items: span(&|s| &s.items), stops: span(&|s| &s.stops), ink: span(&|s| &s.ink), rows: span(&|s| &s.rows), lines: span(&|s| &s.lines) };
    let prev = old.lines.start.checked_sub(1).map(|i| out.lines[i]);
    let mut mid = Pieces::at(metrics, size, &old, segs.get(prefix).map_or(row.len(), |s| s.start), prev);
    // the new lines between: from the lines replaced (an edit of several lines keeps those it did not change), the
    // cache, or laid out now
    let reuse: std::collections::HashMap<u64, &Part> = replaced.iter().map(|p| (p.hash, p)).collect();
    let mut new_parts = Vec::with_capacity(n - prefix - suffix);
    for k in prefix..n - suffix {
        let part = &row.0[segs[k].clone()];
        let depth = depths[k];
        let h = line_hash(part);
        let found = reuse
            .get(&h)
            .filter(|p| *p.items == *part)
            .and_then(|p| Some((p.items.clone(), p.fit, p.lay.clone(), p.fit.shift(depth, opts)?)));
        let (items, fit, lay, sx) = match found {
            Some(f) => f,
            None => line_layout(&mut cache.lines, part, (depth, h, call), metrics, opts),
        };
        new_parts.push(mid.push((items, fit, h, lay, sx), k + 1 == n));
    }
    drop(reuse);
    // the lines after: they keep their place when the first of them does (bit for bit), and only their item numbers
    // move (and their x where their depth moved them); otherwise they are composed again below the new lines
    let keeps_place = tail.first().is_none_or(|p| line_offset(metrics, size, mid.prev.as_ref(), &p.lay).to_bits() == p.dy.to_bits());
    if keeps_place {
        let d = |new: &Range<usize>, old: &Range<usize>| new.end as isize - old.end as isize;
        let (di, ds, dk, dr, dl) = (
            d(&mid.next.items, &old.items),
            d(&mid.next.stops, &old.stops),
            d(&mid.next.ink, &old.ink),
            d(&mid.next.rows, &old.rows),
            d(&mid.next.lines, &old.lines),
        );
        let doff = tail.first().map_or(0, |p| mid.off as isize - p.off as isize);
        if doff != 0 || dr != 0 {
            for it in &mut out.items[old.items.end..] {
                if let Draw::Char { row, index, .. } | Draw::Placeholder { row, index, .. } | Draw::Delim { row, index, .. } = it {
                    if *row == 0 {
                        *index = (*index as isize + doff) as u32;
                    } else if *row != u32::MAX {
                        *row = (*row as isize + dr) as u32;
                    }
                }
            }
            for r in &mut out.rows[old.rows.end..] {
                if let Some(first) = r.path.first_mut() {
                    first.item = (first.item as isize + doff) as usize;
                }
            }
            for l in &mut out.lines[old.lines.end..] {
                l.start = (l.start as isize + doff) as usize;
                l.end = (l.end as isize + doff) as usize;
            }
        }
        let mv = |r: &Range<usize>, by: isize| (r.start as isize + by) as usize..(r.end as isize + by) as usize;
        for p in &mut tail {
            p.off = (p.off as isize + doff) as usize;
            p.at = Spans { items: mv(&p.at.items, di), stops: mv(&p.at.stops, ds), ink: mv(&p.at.ink, dk), rows: mv(&p.at.rows, dr), lines: mv(&p.at.lines, dl) };
        }
        mid.splice_into(&mut out, &old);
        for (p, sx) in tail.iter_mut().zip(&tail_sx) {
            if p.sx.to_bits() != sx.to_bits() {
                p.sx = *sx;
                move_part(&mut out, p);
            }
        }
    } else {
        let count = tail.len();
        for (i, (p, sx)) in std::mem::take(&mut tail).into_iter().zip(tail_sx).enumerate() {
            new_parts.push(mid.push((p.items, p.fit, p.hash, p.lay, sx), i + 1 == count));
        }
        let to_end = Spans {
            items: old.items.start..out.items.len(),
            stops: old.stops.start..out.rows[0].stops.len(),
            ink: old.ink.start..out.rows[0].ink.len(),
            rows: old.rows.start..out.rows.len(),
            lines: old.lines.start..out.lines.len(),
        };
        mid.splice_into(&mut out, &to_end);
    }
    parts.extend(new_parts);
    parts.extend(tail);
    // the size and the first baseline, as a composition from the start takes them
    out.width = parts.iter().fold(0.0f32, |w, p| w.max(p.lay.width + p.sx));
    out.height = parts.iter().fold(0.0f32, |h, p| h.max(p.lay.height + p.dy));
    out.baseline = parts.first().map_or(0.0, |p| p.lay.baseline);
    // lines not used in this call nor the one before go
    cache.lines.retain(|_, b| {
        b.retain(|e| e.3 + 1 >= call);
        !b.is_empty()
    });
    let lay = Arc::new(out);
    cache.last = Some(Composed { layout: lay.clone(), parts });
    lay
}

/// Whether `c` is a bracket that [`delimiter_paths`] can draw.
pub fn is_vector_delimiter(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '|' | '‖' | '⟨' | '⟩' | '⌈' | '⌉' | '⌊' | '⌋')
}

/// The centre lines of the bracket `c` drawn in the box with top-left corner (x, y), width `w` and height `h` (y
/// down), for a stroke of width `t`: polylines of points, curves flattened finely enough for any size. Painters
/// stroke each polyline with width `t`. Brackets that are not [`is_vector_delimiter`] give no lines.
///
/// The shapes follow the text font's brackets rather than a math font's: one stroke weight, parentheses that stay
/// straight through the middle when tall, square brackets with short arms, braces with a centre cusp.
pub fn delimiter_paths(c: char, x: f32, y: f32, w: f32, h: f32, t: f32) -> Vec<Vec<[f32; 2]>> {
    let open = matches!(c, '(' | '[' | '{' | '⟨' | '⌈' | '⌊');
    let base = match c {
        ')' => '(',
        ']' => '[',
        '}' => '{',
        '⟩' => '⟨',
        '⌉' => '⌈',
        '⌋' => '⌊',
        other => other,
    };
    // the inner box the strokes' centre lines stay in
    let margin = (0.18 * w).max(t);
    let (xl, xr) = (x + margin + t / 2.0, x + w - margin - t / 2.0);
    let (top, bot) = (y + t / 2.0, y + h - t / 2.0);
    let mid = (top + bot) / 2.0;
    let span = (xr - xl).max(t);
    let mut lines: Vec<Vec<[f32; 2]>> = match base {
        '(' => {
            // one cubic whose leftmost point reaches xl; the controls sit at a height tied to the width, so a tall
            // parenthesis curves near its ends and runs straight through the middle
            let ky = (2.6 * span).min(0.32 * (bot - top));
            let cx = (xl - 0.25 * xr) / 0.75;
            vec![cubic([xr, top], [cx, top + ky], [cx, bot - ky], [xr, bot])]
        }
        '[' => {
            let arm = xl + (0.85 * span).max(1.5 * t);
            vec![vec![[arm, top], [xl, top], [xl, bot], [arm, bot]]]
        }
        '{' => {
            let xm = (xl + xr) / 2.0;
            let a = (1.1 * span).min(0.22 * (bot - top));
            let mut p = cubic([xr, top], [xm, top], [xm, top], [xm, top + a]);
            p.push([xm, mid - a]);
            p.extend(cubic([xm, mid - a], [xm, mid], [xm, mid], [xl, mid]).into_iter().skip(1));
            p.extend(cubic([xl, mid], [xm, mid], [xm, mid], [xm, mid + a]).into_iter().skip(1));
            p.push([xm, bot - a]);
            p.extend(cubic([xm, bot - a], [xm, bot], [xm, bot], [xr, bot]).into_iter().skip(1));
            vec![p]
        }
        '|' => vec![vec![[x + w / 2.0, top], [x + w / 2.0, bot]]],
        '‖' => {
            let d = (0.14 * w).max(t);
            vec![vec![[x + w / 2.0 - d, top], [x + w / 2.0 - d, bot]], vec![[x + w / 2.0 + d, top], [x + w / 2.0 + d, bot]]]
        }
        '⟨' => vec![vec![[xr, top], [xl, mid], [xr, bot]]],
        '⌈' => {
            let arm = xl + (0.85 * span).max(1.5 * t);
            vec![vec![[arm, top], [xl, top], [xl, bot]]]
        }
        '⌊' => {
            let arm = xl + (0.85 * span).max(1.5 * t);
            vec![vec![[xl, top], [xl, bot], [arm, bot]]]
        }
        _ => vec![],
    };
    if !open && base != c {
        // a closing bracket is its opening one mirrored about the box's centre
        let cx2 = 2.0 * x + w;
        for l in &mut lines {
            for p in l.iter_mut() {
                p[0] = cx2 - p[0];
            }
        }
    }
    lines
}

/// The points of a cubic Bezier curve, flattened (both ends included).
fn cubic(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]) -> Vec<[f32; 2]> {
    const N: usize = 24;
    (0..=N)
        .map(|i| {
            let u = i as f32 / N as f32;
            let v = 1.0 - u;
            let (a, b, c, d) = (v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u);
            [a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::from_text;
    use crate::tree::Item;

    /// A long cell of short lines inside and outside brackets, with boxes.
    fn long_cell(lines: usize) -> Row {
        let mut t = String::new();
        for k in 0..lines {
            match k % 4 {
                0 => t.push_str(&format!("a{k} = f(x,\n")),
                1 => t.push_str("  ((1)/(2)) + y)\n"),
                2 => t.push_str(&format!("b{k} = ((x)^(2)) + ((sqrt(z)))\n")),
                _ => t.push_str("c = [q, [1, 2]]\n"),
            }
        }
        t.push_str("last");
        from_text(&t)
    }

    /// Round 38: an opening bracket typed at the top of a long cell changes the bracket depth of every line after it;
    /// those lines keep their own layouts (the same ones, not laid out again) and are drawn two ems further right,
    /// and the composed layout is still the one composed from the start, number for number.
    #[test]
    fn a_bracket_at_the_top_keeps_the_layouts_of_the_lines_after_it() {
        for opts in [LayoutOptions::input(13.0, Some(900.0)), LayoutOptions::input(13.0, None)] {
            let mut row = long_cell(200);
            let mut cache = LineCache::default();
            drop(layout_by_lines_shared(&row, &MonoMetrics, &opts, &mut cache));
            let before: Vec<Arc<Layout>> = cache.last.as_ref().unwrap().parts.iter().map(|p| p.lay.clone()).collect();
            let xs_before: Vec<f32> = cache.last.as_ref().unwrap().layout.lines.iter().map(|l| l.indent).collect();
            row.0.insert(0, Item::Char('('));
            let lay = layout_by_lines_shared(&row, &MonoMetrics, &opts, &mut cache);
            let parts = &cache.last.as_ref().unwrap().parts;
            // the first line changed; every other line's layout is the one it had
            assert_eq!(parts.len(), before.len());
            let kept = parts.iter().zip(&before).skip(1).filter(|(p, b)| Arc::ptr_eq(&p.lay, b)).count();
            assert_eq!(kept, parts.len() - 1);
            // each line after the first moved right by one level (two ems), as a layout of the whole row puts them
            for (k, (l, x0)) in lay.lines.iter().zip(&xs_before).enumerate().skip(1) {
                assert!((l.indent - x0 - 2.0 * 13.0).abs() < 1e-3, "line {k}: {} from {x0}", l.indent);
            }
            let whole = layout(&row, &MonoMetrics, &opts);
            assert_eq!(whole.lines.len(), lay.lines.len());
            for (a, b) in whole.lines.iter().zip(&lay.lines) {
                assert!((a.indent - b.indent).abs() < 0.01 && (a.right - b.right).abs() < 0.01 && a.start == b.start);
            }
            let from_start = layout_by_lines(&row, &MonoMetrics, &opts, &mut LineCache::default());
            assert!(*lay == from_start);
            // and back: the bracket deleted again
            drop(lay);
            row.0.remove(0);
            let lay = layout_by_lines_shared(&row, &MonoMetrics, &opts, &mut cache);
            assert!(*lay == layout_by_lines(&row, &MonoMetrics, &opts, &mut LineCache::default()));
            for (l, x0) in lay.lines.iter().zip(&xs_before) {
                assert!((l.indent - x0).abs() < 1e-6);
            }
        }
    }

    /// A line laid out at depth 0 and moved right by a depth's indent is the line laid out at that depth (by the
    /// engine itself): the same painted items, stops and lines within rounding, for lines that open, close (a line
    /// starting with a closer counts one level less), hold boxes, and lines near the width at which they wrap.
    #[test]
    fn a_line_moved_by_its_indent_is_the_line_laid_out_at_its_depth() {
        let opts = LayoutOptions::input(10.0, Some(160.0));
        let texts = ["x + 1\n", ") + y\n", "]]\n", "((1)/(2)) + f(((x)^(2)))\n", "abcdefghijk + lmn\n", "abcdefghijklmn\n", "q"];
        let mut served = 0;
        for t in texts {
            let row = from_text(t);
            let (zero, fit) = engine::Engine::new(&MonoMetrics, &opts).run_with_fit(&row);
            let Some(fit) = fit else { continue };
            for depth in 0..6 {
                let direct = engine::Engine::new(&MonoMetrics, &opts).starting_at_depth(depth).run(&row);
                let one_line = direct.lines.iter().filter(|l| l.start < l.end).count() <= 1;
                match Fit::Any(fit).shift(depth, &opts) {
                    Some(sx) => {
                        served += 1;
                        assert!(one_line, "{t:?} at {depth} wraps");
                        assert_eq!(zero.items.len(), direct.items.len());
                        for (a, b) in zero.items.iter().zip(&direct.items) {
                            let (mut a, mut b) = (a.clone(), b.clone());
                            assert!((*draw_x(&mut a) + sx - *draw_x(&mut b)).abs() < 1e-3, "{t:?} at {depth}");
                        }
                        for (k, (a, b)) in zero.rows.iter().zip(&direct.rows).enumerate() {
                            // the stop after a line break is on the next line (another line's first)
                            let n = if k == 0 && t.ends_with('\n') { a.stops.len() - 1 } else { a.stops.len() };
                            for (s, r) in a.stops.iter().zip(&b.stops).take(n) {
                                assert!((s.x + sx - r.x).abs() < 1e-3 && s.baseline == r.baseline);
                            }
                        }
                        for (a, b) in zero.lines.iter().zip(&direct.lines).filter(|(a, _)| a.start < a.end) {
                            assert!((a.indent + sx - b.indent).abs() < 1e-3 && (a.right + sx - b.right).abs() < 1e-3);
                        }
                    }
                    None => assert!(!one_line, "{t:?} at {depth} fits on one line"),
                }
            }
        }
        assert!(served > 20, "{served}");
    }
}
