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
    /// left indentation of the line
    pub indent: f32,
}

/// A caret position's geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    /// x of the caret
    pub x: f32,
    /// baseline of the row at that position
    pub baseline: f32,
    /// top of the caret
    pub top: f32,
    /// bottom of the caret
    pub bottom: f32,
}

/// The caret geometry of one row of the tree.
#[derive(Clone, Debug, PartialEq)]
pub struct RowGeom {
    /// where the row is in the tree
    pub path: RowPath,
    /// one stop per caret offset `0..=len`
    pub stops: Vec<Stop>,
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

    /// The caret position nearest to a point: the stop whose caret box is closest, preferring deeper rows on ties
    /// (so a click on a numerator lands in it, not beside the fraction).
    pub fn hit(&self, x: f32, y: f32) -> (RowPath, usize) {
        let mut best: Option<(f32, usize, &RowPath, usize)> = None;
        for r in &self.rows {
            for (k, s) in r.stops.iter().enumerate() {
                let dy = if y < s.top {
                    s.top - y
                } else if y > s.bottom {
                    y - s.bottom
                } else {
                    0.0
                };
                // vertical distance weighs more: a point is on a line before it is near a stop; among stops at the
                // same place (a base's end and its exponent's start) the one centred nearest wins
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
