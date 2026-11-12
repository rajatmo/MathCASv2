//! Positions and selections in a box tree (design 11, section 3.2).
use mc_boxes::{Row, RowPath, Step};
use std::ops::Range;

/// A caret position: the row (a path from the root) and the offset in it (`0..=len`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Position {
    /// the row
    pub path: RowPath,
    /// the offset in the row
    pub offset: usize,
}

impl Position {
    /// A position in the row at `path`.
    pub fn new(path: &[Step], offset: usize) -> Position {
        Position { path: path.to_vec(), offset }
    }

    /// A position in the root row.
    pub fn root(offset: usize) -> Position {
        Position { path: vec![], offset }
    }

    /// The position made valid in `root`: the deepest valid prefix of the path, the offset at most the row's length.
    pub fn clamped(&self, root: &Row) -> Position {
        let mut p = self.path.clone();
        while root.at(&p).is_none() {
            p.pop();
        }
        let len = root.at(&p).map_or(0, |r| r.len());
        let offset = if p.len() == self.path.len() { self.offset.min(len) } else { self.path[p.len()].item.min(len) };
        Position { path: p, offset }
    }

    /// Whether the position is valid in `root`.
    pub fn valid(&self, root: &Row) -> bool {
        root.at(&self.path).is_some_and(|r| self.offset <= r.len())
    }
}

/// A selection: the anchor stays where it started while the head (the caret) moves. Ends in different rows select
/// the smallest run of items of their common row that holds both, a box counting whole when an end is inside it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Selection {
    /// where the selection started
    pub anchor: Position,
    /// the caret
    pub head: Position,
}

impl Selection {
    /// A caret at `p` (nothing selected).
    pub fn caret(p: Position) -> Selection {
        Selection { anchor: p.clone(), head: p }
    }

    /// Items `range` of the row at `path`, the caret at the range's end.
    pub fn items(path: &[Step], range: Range<usize>) -> Selection {
        Selection { anchor: Position::new(path, range.start), head: Position::new(path, range.end) }
    }

    /// The selected row and item range; `None` when nothing is selected.
    pub fn range(&self, root: &Row) -> Option<(RowPath, Range<usize>)> {
        let (a, c) = (&self.anchor, &self.head);
        let d = a.path.iter().zip(&c.path).take_while(|(x, y)| x == y).count();
        let common = &c.path[..d];
        let len = root.at(common)?.len();
        let span = |k: &Position| match k.path.get(d) {
            Some(s) => (s.item.min(len), (s.item + 1).min(len)),
            None => (k.offset.min(len), k.offset.min(len)),
        };
        let (a0, a1) = span(a);
        let (c0, c1) = span(c);
        let (lo, hi) = (a0.min(c0), a1.max(c1));
        (lo < hi).then(|| (common.to_vec(), lo..hi))
    }

    /// Whether the head is after the anchor in reading order (the selection was made forwards).
    pub fn forward(&self) -> bool {
        let d = self.anchor.path.iter().zip(&self.head.path).take_while(|(x, y)| x == y).count();
        let at = |k: &Position| k.path.get(d).map_or((k.offset, 0), |s| (s.item, 1));
        let (h, a) = (at(&self.head), at(&self.anchor));
        h.0 > a.0 || (h.0 == a.0 && h.1 > a.1)
    }

    /// The selection made valid in `root`.
    pub fn clamped(&self, root: &Row) -> Selection {
        Selection { anchor: self.anchor.clamped(root), head: self.head.clamped(root) }
    }
}
