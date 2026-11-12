//! What the editor asks of a layout (design 11, section 3.2): motions that need geometry (Up and Down, Home and End
//! on a wrapped line) take a [`Geometry`], implemented by [`mc_boxes::layout::Layout`]; without one the editor uses
//! [`Logical`], the root row's line breaks with one unit of width per item (key scripts and tests).
use crate::pos::Position;
use mc_boxes::layout::Layout;
use mc_boxes::{Item, Row};
use std::ops::Range;

/// The visual lines of a laid-out tree and where caret positions are on them.
pub trait Geometry {
    /// The x of a caret position and the index of the visual line it is on.
    fn locate(&self, p: &Position) -> Option<(f32, usize)>;
    /// The number of visual lines.
    fn line_count(&self) -> usize;
    /// The root items on visual line `k` (a line break ending it included).
    fn line_items(&self, k: usize) -> Option<Range<usize>>;
    /// The caret position on visual line `k` nearest to `x`, in any row.
    fn nearest(&self, k: usize, x: f32) -> Option<Position>;
}

impl Geometry for Layout {
    fn locate(&self, p: &Position) -> Option<(f32, usize)> {
        let s = self.caret(&p.path, p.offset)?;
        let k = self.lines.iter().position(|ln| s.baseline <= ln.bottom + 0.01).unwrap_or(self.lines.len().saturating_sub(1));
        Some((s.x, k))
    }

    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn line_items(&self, k: usize) -> Option<Range<usize>> {
        self.lines.get(k).map(|l| l.start..l.end)
    }

    fn nearest(&self, k: usize, x: f32) -> Option<Position> {
        let l = self.lines.get(k)?;
        let (path, offset) = self.hit(x, l.baseline);
        Some(Position { path, offset })
    }
}

/// The line breaks of the root row as lines, one unit of width per item (a box counts as one).
pub struct Logical<'a>(pub &'a Row);

impl Logical<'_> {
    fn ranges(&self) -> Vec<Range<usize>> {
        let mut out = vec![];
        let mut start = 0;
        for (i, it) in self.0 .0.iter().enumerate() {
            if *it == Item::Char('\n') {
                out.push(start..i + 1);
                start = i + 1;
            }
        }
        out.push(start..self.0.len());
        out
    }
}

impl Geometry for Logical<'_> {
    fn locate(&self, p: &Position) -> Option<(f32, usize)> {
        let top = p.path.first().map_or(p.offset, |s| s.item);
        let lines = self.ranges();
        let k = lines.iter().position(|r| top < r.end || r.end == self.0.len()).unwrap_or(lines.len() - 1);
        Some(((top - lines[k].start) as f32, k))
    }

    fn line_count(&self) -> usize {
        self.ranges().len()
    }

    fn line_items(&self, k: usize) -> Option<Range<usize>> {
        self.ranges().get(k).cloned()
    }

    fn nearest(&self, k: usize, x: f32) -> Option<Position> {
        let r = self.ranges().get(k).cloned()?;
        let n = r.end - r.start - usize::from(r.end > r.start && self.0 .0[r.end - 1] == Item::Char('\n'));
        let mut j = 0;
        while j < n && j as f32 + 0.5 <= x {
            j += 1;
        }
        Some(Position::root(r.start + j))
    }
}
