//! Editing a box tree (design 08, sections 4.1 to 4.3 and 3.9): the caret and selection in boxes, typing,
//! deleting, moving, the structure commands with their operand rules, placeholders and `Tab`, `Ctrl+Space`,
//! `Ctrl+5`, grids, aliases and templates.
//!
//! The caret is a row path and an offset. A selection has an anchor, a caret position of its own that stays where
//! the selection started while the caret moves; what is selected is the range of items of the deepest row holding
//! both ends, a box counting whole when an end is inside it (extending out of a slot selects the whole box, and
//! extending back into it selects from the anchor again). Commands return whether they did
//! something, so the caller can move across cells, beep or fall back (Tab without placeholders indents).
use crate::aliases::{self, Alias};
use crate::canonical::{from_text, to_text};
use crate::linear::linear_text;
use crate::ops::{is_close, is_open, is_word, pair_around};
use crate::tree::{write_notation, Item, Node, Row, RowPath, Step, MAX_NESTING};
use std::ops::Range;

/// A caret position: the row and the offset in it (`0..=len`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Caret {
    /// the row
    pub path: RowPath,
    /// the offset in the row
    pub offset: usize,
}

/// The box keys (decision NBUI.5: a cell has exactly these three boxes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Structure {
    /// `Ctrl+/`: a fraction of the term before the caret (or the selection) over a new denominator
    Fraction,
    /// `Ctrl+6` (`Ctrl+^`): a power of the term before the caret
    Superscript,
    /// `Ctrl+2`: a square root of the selection (or an empty one)
    Radical,
}

/// What `Esc` did (design 08, section 3.9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AliasOutcome {
    /// an alias was started: letters typed now are its name
    Started,
    /// the alias was found and replaced by its character or template
    Replaced,
    /// no alias has that name: the text stays, marked as unknown until edited
    Unknown,
    /// `Esc Esc` with nothing between: the alias was dropped
    Cancelled,
}

/// A pending or unknown alias, for drawing: the start marker `⋮` goes at `start`, the name is `start..end` of
/// the row at `path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AliasMark {
    /// the row
    pub path: RowPath,
    /// where the marker is drawn
    pub start: usize,
    /// end of the name typed so far
    pub end: usize,
    /// the name was looked up and not found (error colour)
    pub unknown: bool,
}

/// A box tree being edited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Editor {
    root: Row,
    caret: Caret,
    /// the other end of the selection (it stays where the selection started)
    anchor: Option<Caret>,
    /// a pending alias: its row and start offset
    alias: Option<(RowPath, usize)>,
    /// the last alias looked up without success
    unknown: Option<AliasMark>,
}

impl Editor {
    /// An editor over `root` with the caret at its end.
    pub fn new(root: Row) -> Editor {
        let offset = root.len();
        Editor { root, caret: Caret { path: vec![], offset }, ..Default::default() }
    }

    /// An editor over canonical box text.
    pub fn from_text(text: &str) -> Editor {
        Editor::new(from_text(text))
    }

    /// The tree.
    pub fn root(&self) -> &Row {
        &self.root
    }

    /// Replaces the tree; the caret goes to its end.
    pub fn set_root(&mut self, root: Row) {
        *self = Editor::new(root);
    }

    /// The canonical text of the tree (design 08, section 4.4).
    pub fn text(&self) -> String {
        to_text(&self.root)
    }

    /// The linear input text of the tree (design 08, section 4.6).
    pub fn linear(&self) -> String {
        linear_text(&self.root)
    }

    /// The caret.
    pub fn caret(&self) -> &Caret {
        &self.caret
    }

    /// Moves the caret (clamped to a valid position: the deepest valid prefix of the path, the offset at most the
    /// row's length) and clears the selection.
    pub fn set_caret(&mut self, path: &[Step], offset: usize) {
        let mut p: RowPath = path.to_vec();
        while self.root.at(&p).is_none() {
            p.pop();
        }
        let len = self.row().len();
        let len = self.root.at(&p).map_or(len, |r| r.len());
        self.caret = Caret { path: p, offset: offset.min(len) };
        self.anchor = None;
    }

    /// The selection: the row holding both of its ends and the selected item range (`None` when nothing is
    /// selected). An end inside a box of that row selects the whole box.
    pub fn selection(&self) -> Option<(RowPath, Range<usize>)> {
        let a = self.anchor.as_ref()?;
        let c = &self.caret;
        let d = a.path.iter().zip(&c.path).take_while(|(x, y)| x == y).count();
        let common = &c.path[..d];
        let len = self.root.at(common)?.len();
        let span = |k: &Caret| match k.path.get(d) {
            Some(s) => (s.item.min(len), (s.item + 1).min(len)),
            None => (k.offset.min(len), k.offset.min(len)),
        };
        let (a0, a1) = span(a);
        let (c0, c1) = span(c);
        let (lo, hi) = (a0.min(c0), a1.max(c1));
        (lo < hi).then(|| (common.to_vec(), lo..hi))
    }

    /// The anchor of the selection, if one was started (it may select nothing yet).
    pub fn anchor(&self) -> Option<&Caret> {
        self.anchor.as_ref()
    }

    /// Selects items `range` of the row at `path` (the caret goes to the range's end).
    pub fn select(&mut self, path: &[Step], range: Range<usize>) {
        self.set_caret(path, range.end);
        let len = self.row().len();
        self.anchor_at(range.start.min(len));
    }

    /// Moves the caret to a position, keeping the selection's anchor (or starting one where the caret was): a
    /// Shift+click or a drag.
    pub fn extend_to(&mut self, path: &[Step], offset: usize) {
        let anchor = self.anchor.clone().unwrap_or_else(|| self.caret.clone());
        self.set_caret(path, offset);
        self.anchor = Some(anchor);
    }

    /// The anchor at `offset` of the caret's row.
    fn anchor_at(&mut self, offset: usize) {
        self.anchor = Some(Caret { path: self.caret.path.clone(), offset });
    }

    /// Brings a selection whose ends are in different rows into one row (the caret at the end it was on), before
    /// a command acts on it.
    fn settle(&mut self) {
        let Some(a) = self.anchor.clone() else { return };
        if a.path == self.caret.path {
            return;
        }
        let Some((p, r)) = self.selection() else {
            self.anchor = None;
            return;
        };
        let d = p.len();
        let at = |k: &Caret| k.path.get(d).map_or(k.offset, |s| s.item);
        let (cpos, apos) = (at(&self.caret), at(&a));
        let forward = cpos > apos || (cpos == apos && self.caret.path.len() > d);
        self.caret = Caret { path: p.clone(), offset: if forward { r.end } else { r.start } };
        self.anchor = Some(Caret { path: p, offset: if forward { r.start } else { r.end } });
    }

    /// Selects the whole tree.
    pub fn select_all(&mut self) {
        let n = self.root.len();
        self.select(&[], 0..n);
    }

    /// The notation of the tree with the caret as `▮` or the selection in `⟦ ⟧` (the key scripts' notation).
    pub fn notation(&self) -> String {
        let sel = self.selection();
        let caret = &self.caret;
        let mut s = String::new();
        write_notation(&self.root, &mut s, &mut |out, pos| {
            let row = sel.as_ref().map_or(caret.path.as_slice(), |(p, _)| p.as_slice());
            if pos.path != row {
                return;
            }
            match &sel {
                Some((_, r)) => {
                    if pos.offset == r.end && r.end > r.start {
                        out.push('⟧');
                    }
                    if pos.offset == r.start {
                        out.push('⟦');
                    }
                }
                None if pos.offset == caret.offset => out.push('▮'),
                None => {}
            }
        });
        s
    }

    fn row(&self) -> &Row {
        self.root.at(&self.caret.path).unwrap_or(&self.root)
    }

    fn row_mut(&mut self) -> &mut Row {
        let path = self.caret.path.clone();
        if self.root.at(&path).is_none() {
            self.caret = Caret::default();
            return &mut self.root;
        }
        self.root.at_mut(&path).expect("checked above")
    }

    /// Nesting depth of the caret's row (boxes above it).
    fn depth(&self) -> usize {
        self.caret.path.len()
    }

    // ---- typing and deleting ----

    /// Deletes the selection; true if there was one.
    fn delete_selection(&mut self) -> bool {
        self.settle();
        let Some((_, r)) = self.selection() else {
            self.anchor = None;
            return false;
        };
        self.anchor = None;
        self.caret.offset = r.start;
        let slot = !self.caret.path.is_empty();
        let row = self.row_mut();
        row.0.drain(r);
        if slot && row.is_empty() {
            row.0.push(Item::Placeholder);
        }
        true
    }

    /// Inserts items at the caret, replacing the selection or a slot's lone placeholder; the caret goes after them.
    /// Returns where the inserted items start (after a `*` put in front of them, see [`Editor::implicit_times`]).
    fn insert_items(&mut self, items: Vec<Item>) -> usize {
        self.delete_selection();
        self.unknown = None;
        let offset = self.caret.offset;
        let path = self.caret.path.clone();
        let row = self.row_mut();
        if row.is_placeholder() && !items.is_empty() {
            row.0.clear();
            let n = items.len();
            row.0.extend(items);
            self.caret.offset = n;
            return 0;
        }
        let at = offset.min(row.len());
        let n = items.len();
        row.0.splice(at..at, items);
        self.caret.offset = at + n;
        self.implicit_times(&path, at, at + n)
    }

    /// Implicit multiplication (a user decision for the box editor): where newly inserted items `start..end` of the
    /// row at `path` make a box touch an operand, a `*` goes between them, since the text would otherwise read as a
    /// call (`x((sqrt(Q)))` is `x(sqrt(Q))`, `((sqrt(Q)))(y)` calls the root) or not read at all (`((sqrt(Q)))x`).
    /// The caret keeps its place relative to what was typed. Returns the start of the inserted items afterwards.
    fn implicit_times(&mut self, path: &RowPath, start: usize, end: usize) -> usize {
        let Some(row) = self.root.at(path) else { return start };
        let at_end = needs_times(row, end);
        let at_start = needs_times(row, start);
        if at_end {
            // the caret stays before this `*` (right after what was typed)
            self.insert_times(path, end, false);
        }
        if at_start {
            self.insert_times(path, start, true);
            return start + 1;
        }
        start
    }

    /// Inserts a `*` at `i` in the row at `path`, moving the caret along when it is after `i` (or at `i` and
    /// `shift_at` holds) or inside a box at or after `i`.
    fn insert_times(&mut self, path: &RowPath, i: usize, shift_at: bool) {
        let Some(row) = self.root.at_mut(path) else { return };
        // a space before the boundary stays before the `*`, so `((sqrt(Q))) x` becomes `((sqrt(Q))) *x`
        row.0.insert(i, Item::Char('*'));
        let d = path.len();
        if self.caret.path.len() > d && self.caret.path[..d] == path[..] {
            if self.caret.path[d].item >= i {
                self.caret.path[d].item += 1;
            }
        } else if self.caret.path == *path && (self.caret.offset > i || (shift_at && self.caret.offset == i)) {
            self.caret.offset += 1;
        }
        // a pending alias that starts at or after the `*` keeps its name after it (`Esc` after a box, then
        // letters: the name must not take the `*` in)
        if let Some((ap, start)) = &mut self.alias {
            if ap.len() > d && ap[..d] == path[..] {
                if ap[d].item >= i {
                    ap[d].item += 1;
                }
            } else if *ap == *path && *start >= i {
                *start += 1;
            }
        }
    }

    /// Types a character (`□` inserts a placeholder).
    pub fn insert_char(&mut self, c: char) {
        self.insert_items(vec![if c == '□' { Item::Placeholder } else { Item::Char(c) }]);
    }

    /// Types plain text.
    pub fn insert_text(&mut self, s: &str) {
        self.insert_items(Row::text(s).0);
    }

    /// Pastes canonical box text at the caret (boxes become boxes).
    pub fn paste(&mut self, canonical: &str) -> bool {
        let row = from_text(canonical);
        if self.depth() + row.nesting() > MAX_NESTING {
            return false;
        }
        self.insert_items(row.0);
        true
    }

    /// Inserts a template: canonical box text where `▮` marks the placeholder that gets the caret (selected).
    pub fn insert_template(&mut self, template: &str) -> bool {
        let mut row = from_text(template);
        if self.depth() + row.nesting() > MAX_NESTING {
            return false;
        }
        let mark = find_char(&row, '▮');
        if let Some((p, k)) = &mark {
            if let Some(r) = row.at_mut(p) {
                r.0[*k] = Item::Placeholder;
            }
        }
        let base_path = self.caret.path.clone();
        self.delete_selection();
        let n = row.len();
        let start = self.insert_items(row.0);
        match mark {
            Some((mut p, k)) => {
                if let Some(first) = p.first_mut() {
                    first.item += start;
                    let mut full = base_path;
                    full.extend(p);
                    self.select(&full, k..k + 1);
                } else {
                    self.select(&base_path, start + k..start + k + 1);
                }
            }
            None => self.caret.offset = start + n,
        }
        true
    }

    /// The canonical text of the selection (empty without one).
    pub fn selected_text(&self) -> String {
        match self.selection() {
            Some((p, r)) => self.root.at(&p).map_or(String::new(), |row| to_text(&Row(row.0[r].to_vec()))),
            None => String::new(),
        }
    }

    /// Cuts the selection: returns its canonical text.
    pub fn cut(&mut self) -> String {
        let t = self.selected_text();
        self.delete_selection();
        t
    }

    /// Backspace: deletes the selection, the character or operator pair before the caret, enters a box before it
    /// (deleting it if all its slots are empty), or at the start of a slot moves before the box (first slot) or
    /// removes the structure keeping its contents. False at the start of the tree.
    pub fn backspace(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if let Some((p, s)) = &self.alias {
            if *p == self.caret.path && *s == self.caret.offset {
                self.alias = None;
                return true;
            }
        }
        self.unknown = None;
        let row = self.row();
        let k = self.caret.offset.min(row.len());
        if k > 0 && !row.is_placeholder() {
            match &row.0[k - 1] {
                Item::Box(b) if b.slot_count() > 0 && !b.is_blank() => {
                    self.enter_box(k - 1, false);
                }
                Item::Char(_) => {
                    let (s, n) = pair_around(row, k - 1).filter(|(s, n)| s + n == k).unwrap_or((k - 1, 1));
                    self.remove(s..s + n);
                }
                _ => self.remove(k - 1..k),
            }
            return true;
        }
        self.at_slot_edge(false)
    }

    /// Delete: the mirror of [`Editor::backspace`]. False at the end of the tree.
    pub fn delete(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        self.unknown = None;
        let row = self.row();
        let k = self.caret.offset;
        if k < row.len() && !row.is_placeholder() {
            match &row.0[k] {
                Item::Box(b) if b.slot_count() > 0 && !b.is_blank() => {
                    self.enter_box(k, true);
                }
                Item::Char(_) => {
                    let (s, n) = pair_around(row, k).filter(|(s, _)| *s == k).unwrap_or((k, 1));
                    self.remove(s..s + n);
                }
                _ => self.remove(k..k + 1),
            }
            return true;
        }
        self.at_slot_edge(true)
    }

    fn remove(&mut self, r: Range<usize>) {
        let slot = !self.caret.path.is_empty();
        let start = r.start;
        let row = self.row_mut();
        row.0.drain(r);
        if slot && row.is_empty() {
            row.0.push(Item::Placeholder);
        }
        self.caret.offset = start;
    }

    /// Backspace at the start (or Delete at the end) of a slot.
    fn at_slot_edge(&mut self, forward: bool) -> bool {
        let Some(last) = self.caret.path.last().copied() else { return false };
        let parent: RowPath = self.caret.path[..self.caret.path.len() - 1].to_vec();
        let Some(Item::Box(b)) = self.root.at(&parent).and_then(|r| r.0.get(last.item)) else { return false };
        let n = b.slot_count();
        if b.is_blank() {
            let in_slot = !parent.is_empty();
            if let Some(r) = self.root.at_mut(&parent) {
                r.0.remove(last.item);
                if in_slot && r.is_empty() {
                    r.0.push(Item::Placeholder);
                }
            }
            self.set_caret(&parent, last.item);
            return true;
        }
        // the box goes and its contents stay as linear text (decision NBUI.6): a square root leaves its radicand,
        // a fraction `num/den`, a power `base^exp` (parentheses where the text needs them); the caret where it was
        let _ = n;
        let (items, caret_at) = unwrap_box(b, last.slot, forward);
        let caret_at = caret_at.min(items.len());
        if let Some(r) = self.root.at_mut(&parent) {
            r.0.splice(last.item..last.item + 1, items);
        }
        self.set_caret(&parent, last.item + caret_at);
        true
    }

    /// Puts the caret inside the box at item `k`: at the start of its first slot or the end of its last.
    fn enter_box(&mut self, k: usize, first: bool) {
        let Some(Item::Box(b)) = self.row().0.get(k) else { return };
        let n = b.slot_count();
        if n == 0 {
            self.caret.offset = if first { k + 1 } else { k };
            return;
        }
        let slot = if first { 0 } else { n - 1 };
        let len = b.slot(slot).map_or(0, |r| r.len());
        let mut p = self.caret.path.clone();
        p.push(Step { item: k, slot });
        self.caret = Caret { path: p, offset: if first { 0 } else { len } };
        self.anchor = None;
    }

    // ---- moving ----

    /// Right arrow: steps over a character or operator pair, into a box, out of a slot to the next one or past
    /// the box. With `extend`, grows the selection (boxes as units; leaving a slot selects the whole box). False
    /// at the end of the tree.
    pub fn move_right(&mut self, extend: bool) -> bool {
        self.horizontal(true, extend)
    }

    /// Left arrow: the mirror of [`Editor::move_right`].
    pub fn move_left(&mut self, extend: bool) -> bool {
        self.horizontal(false, extend)
    }

    fn horizontal(&mut self, forward: bool, extend: bool) -> bool {
        self.unknown = None;
        if !extend {
            self.settle();
            if let Some((_, r)) = self.selection() {
                self.anchor = None;
                self.caret.offset = if forward { r.end } else { r.start };
                return true;
            }
            self.anchor = None;
        }
        let row = self.row();
        let len = row.len();
        let k = self.caret.offset;
        let inside = if forward { k < len } else { k > 0 };
        if extend {
            // the anchor stays; boxes are stepped over as units, except the one holding the anchor, which is
            // entered (extending back towards where the selection started)
            let anchor = self.anchor.clone().unwrap_or_else(|| self.caret.clone());
            if inside {
                let at = if forward { k } else { k - 1 };
                let d = self.caret.path.len();
                let holds_anchor =
                    anchor.path.len() > d && anchor.path[..d] == self.caret.path[..] && anchor.path[d].item == at;
                let enter = holds_anchor && matches!(&row.0[at], Item::Box(b) if b.slot_count() > 0);
                if enter {
                    let mut p = self.caret.path.clone();
                    p.push(Step { item: at, slot: anchor.path[d].slot });
                    let len = self.root.at(&p).map_or(0, |r| r.len());
                    self.caret = Caret { path: p, offset: if forward { 0 } else { len } };
                } else {
                    let step = self.step_len(forward);
                    self.caret.offset = if forward { k + step } else { k - step };
                }
                self.anchor = Some(anchor);
                return true;
            }
            // leaving the row: the caret goes past the box in the parent row (the box is then selected whole)
            let Some(last) = self.caret.path.pop() else { return false };
            self.caret.offset = if forward { last.item + 1 } else { last.item };
            self.anchor = Some(anchor);
            return true;
        }
        if inside {
            let at = if forward { k } else { k - 1 };
            if let Item::Box(b) = &row.0[at] {
                if b.slot_count() > 0 {
                    self.enter_box(at, forward);
                    return true;
                }
            }
            let step = self.step_len(forward);
            self.caret.offset = if forward { k + step } else { k - step };
            return true;
        }
        // at the edge of a slot: the next slot, or out of the box
        let Some(last) = self.caret.path.last().copied() else { return false };
        let parent: RowPath = self.caret.path[..self.caret.path.len() - 1].to_vec();
        let n = self.root.box_of(&self.caret.path).map_or(0, |b| b.slot_count());
        let next = if forward { last.slot + 1 } else { last.slot.wrapping_sub(1) };
        if next < n {
            let mut p = parent;
            p.push(Step { item: last.item, slot: next });
            let len = self.root.at(&p).map_or(0, |r| r.len());
            self.caret = Caret { path: p, offset: if forward { 0 } else { len } };
        } else {
            self.caret = Caret { path: parent, offset: if forward { last.item + 1 } else { last.item } };
        }
        true
    }

    /// How many items one step covers from the caret (an operator pair is one step).
    fn step_len(&self, forward: bool) -> usize {
        let row = self.row();
        let k = self.caret.offset;
        let at = if forward { k } else { k - 1 };
        match pair_around(row, at) {
            Some((s, n)) if forward && s == k => n,
            Some((s, n)) if !forward && s + n == k => n,
            _ => 1,
        }
    }

    /// Home: the start of the caret's line in its row (after the line break before it, else the row's start).
    pub fn move_home(&mut self, extend: bool) {
        self.jump_to_row_edge(false, extend);
    }

    /// End: the end of the caret's line in its row (before the next line break, else the row's end).
    pub fn move_end(&mut self, extend: bool) {
        self.jump_to_row_edge(true, extend);
    }

    fn jump_to_row_edge(&mut self, end: bool, extend: bool) {
        if extend {
            self.anchor = Some(self.anchor.clone().unwrap_or_else(|| self.caret.clone()));
        } else {
            self.anchor = None;
        }
        let row = self.row();
        let k = self.caret.offset.min(row.len());
        let is_break = |j: &usize| matches!(row.0[*j], Item::Char('\n'));
        self.caret.offset = if end {
            (k..row.len()).find(is_break).unwrap_or(row.len())
        } else {
            (0..k).rev().find(is_break).map_or(0, |j| j + 1)
        };
    }

    /// Up arrow inside structures: denominator to numerator, base to exponent, index to base, lower to upper grid
    /// row, under to base to over. False when no enclosing structure has a slot above (the caller then moves by
    /// lines with the layout).
    pub fn move_up(&mut self) -> bool {
        self.vertical(true)
    }

    /// Down arrow: the mirror of [`Editor::move_up`].
    pub fn move_down(&mut self) -> bool {
        self.vertical(false)
    }

    fn vertical(&mut self, up: bool) -> bool {
        self.anchor = None;
        for depth in (1..=self.caret.path.len()).rev() {
            let path = &self.caret.path[..depth];
            let Some(b) = self.root.box_of(path) else { continue };
            let slot = path[depth - 1].slot;
            let target = match (b, up) {
                (Node::Fraction { .. }, true) if slot == 1 => Some(0),
                (Node::Fraction { .. }, false) if slot == 0 => Some(1),
                (Node::Superscript { .. }, true) if slot == 0 => Some(1),
                (Node::Superscript { .. }, false) if slot == 1 => Some(0),
                (Node::Subscript { .. }, true) if slot == 1 => Some(0),
                (Node::Subscript { .. }, false) if slot == 0 => Some(1),
                (Node::Overscript { .. }, true) if slot == 0 => Some(1),
                (Node::Overscript { .. }, false) if slot == 1 => Some(0),
                (Node::Underscript { .. }, true) if slot == 1 => Some(0),
                (Node::Underscript { .. }, false) if slot == 0 => Some(1),
                (Node::Underoverscript { .. }, true) => match slot {
                    1 => Some(0),
                    0 => Some(2),
                    _ => None,
                },
                (Node::Underoverscript { .. }, false) => match slot {
                    2 => Some(0),
                    0 => Some(1),
                    _ => None,
                },
                (Node::Grid { rows }, _) => {
                    let w = rows.first().map_or(1, |r| r.len().max(1));
                    let (r, c) = (slot / w, slot % w);
                    match up {
                        true if r > 0 => Some((r - 1) * w + c),
                        false if r + 1 < rows.len() => Some((r + 1) * w + c),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(t) = target {
                let mut p = path[..depth - 1].to_vec();
                p.push(Step { item: path[depth - 1].item, slot: t });
                let len = self.root.at(&p).map_or(0, |r| r.len());
                let offset = self.caret.offset.min(len);
                self.caret = Caret { path: p, offset };
                return true;
            }
        }
        false
    }

    /// `Ctrl+Space`: the caret just after the innermost enclosing structure. False at top level.
    pub fn exit_structure(&mut self) -> bool {
        self.anchor = None;
        match self.caret.path.pop() {
            Some(last) => {
                self.caret.offset = last.item + 1;
                true
            }
            None => false,
        }
    }

    /// `Tab`: selects the next placeholder in reading order. False when there is none after the caret.
    pub fn next_placeholder(&mut self) -> bool {
        self.settle();
        let order = positions(&self.root);
        let here = self.ordinal(&order, true);
        let found = order.iter().filter(|(_, _, ph)| *ph).find(|(o, _, _)| *o > here).cloned();
        self.select_found(found)
    }

    /// `Shift+Tab`: selects the previous placeholder in reading order. False when there is none before the caret.
    pub fn prev_placeholder(&mut self) -> bool {
        self.settle();
        let order = positions(&self.root);
        let here = self.ordinal(&order, false);
        let found = order.iter().filter(|(_, _, ph)| *ph).rev().find(|(o, _, _)| *o < here).cloned();
        self.select_found(found)
    }

    fn select_found(&mut self, found: Option<(usize, Caret, bool)>) -> bool {
        match found {
            Some((_, c, _)) => {
                self.select(&c.path, c.offset..c.offset + 1);
                true
            }
            None => false,
        }
    }

    /// The caret's place in reading order: with a selection, its end (forward) or start (backward).
    fn ordinal(&self, order: &[(usize, Caret, bool)], forward: bool) -> usize {
        let offset = match self.selection() {
            Some((_, r)) if forward => r.end.saturating_sub(1).max(r.start),
            Some((_, r)) => r.start,
            None => self.caret.offset,
        };
        order.iter().find(|(_, c, _)| c.path == self.caret.path && c.offset == offset).map_or(0, |(o, _, _)| *o)
    }

    // ---- structures ----

    /// A structure command (design 08, section 4.2): the selection, or the operand before the caret, becomes the
    /// first slot and the caret goes to the next slot; with nothing to take, the caret goes into the first slot.
    /// In a grid cell, the grid commands add a row or a column. False if the tree would nest too deeply.
    pub fn structure(&mut self, s: Structure) -> bool {
        if self.depth() + 1 >= MAX_NESTING {
            return false;
        }
        self.unknown = None;
        self.settle();
        let range = match self.selection() {
            Some((_, r)) => Some(r),
            None => self.operand(s),
        };
        self.anchor = None;
        let k = self.caret.offset;
        let taken: Vec<Item> = match &range {
            Some(r) => self.row().0[r.clone()].to_vec(),
            None => Vec::new(),
        };
        let at = range.as_ref().map_or(k, |r| r.start);
        let has = !taken.is_empty();
        let operand = if has { Row(taken) } else { Row::placeholder() };
        let ph = Row::placeholder;
        // the new box and the slot the caret goes to
        let (node, slot) = match s {
            Structure::Fraction => (Node::Fraction { num: operand, den: ph() }, usize::from(has)),
            Structure::Superscript => (Node::Superscript { base: operand, exp: ph() }, usize::from(has)),
            Structure::Radical => (Node::Radical { radicand: operand, index: None }, 0),
        };
        let end = range.map_or(k, |r| r.end);
        let replacing_placeholder = self.row().is_placeholder() && !has;
        let row = self.row_mut();
        let at = if replacing_placeholder {
            row.0.clear();
            0
        } else {
            at
        };
        let end = if replacing_placeholder { 0 } else { end };
        row.0.splice(at..end, [Item::Box(Box::new(node))]);
        let row_path = self.caret.path.clone();
        let mut p = row_path.clone();
        p.push(Step { item: at, slot });
        let slot_row = self.root.at(&p).cloned().unwrap_or_default();
        // a radical over a selection leaves the caret at the end of the radicand
        let offset = if s == Structure::Radical && has { slot_row.len() } else { 0 };
        self.caret = Caret { path: p, offset };
        self.implicit_times(&row_path, at, at + 1);
        true
    }

    /// The operand before the caret for a box key: the last primary ending at the caret (a name or number token, a
    /// bracketed group with the name it calls, a string, a box); for a power after an operator, that operator.
    fn operand(&self, s: Structure) -> Option<Range<usize>> {
        if s == Structure::Radical {
            return None;
        }
        let row = self.row();
        let k = self.caret.offset.min(row.len());
        if row.is_placeholder() {
            return None;
        }
        let primary = primary_before(row, k);
        if primary.is_some() {
            return primary;
        }
        if k == 0 {
            return None;
        }
        match (s, &row.0[k - 1]) {
            (Structure::Superscript, Item::Char(c)) if !(c.is_whitespace() || is_open(*c) || matches!(c, ',' | ';')) => {
                Some(k - 1..k)
            }
            _ => None,
        }
    }

    // ---- selection growth ----

    /// `Ctrl+.`: grows the selection: the word at the caret, then the whole row, then the enclosing box, and so
    /// on outwards. False when the whole tree is already selected.
    pub fn expand_selection(&mut self) -> bool {
        self.settle();
        let row = self.row();
        let len = row.len();
        match self.selection() {
            None => {
                let k = self.caret.offset;
                let is_w = |j: usize| matches!(row.0.get(j), Some(Item::Char(c)) if is_word(*c));
                let (mut a, mut b) = (k, k);
                while a > 0 && is_w(a - 1) {
                    a -= 1;
                }
                while b < len && is_w(b) {
                    b += 1;
                }
                if a == b {
                    if k > 0 {
                        a = k - 1;
                    } else if k < len {
                        b = k + 1;
                    }
                }
                if a == b {
                    return self.lift_selection();
                }
                let p = self.caret.path.clone();
                self.select(&p, a..b);
                true
            }
            Some((p, r)) if r != (0..len) => {
                self.select(&p, 0..len);
                true
            }
            Some(_) => self.lift_selection(),
        }
    }

    fn lift_selection(&mut self) -> bool {
        match self.caret.path.pop() {
            Some(last) => {
                self.caret.offset = last.item + 1;
                self.anchor_at(last.item);
                true
            }
            None => false,
        }
    }

    // ---- aliases ----

    /// `Esc` (design 08, section 3.9): starts an alias at the caret, or ends one: the name typed since the start
    /// is looked up and replaced by its character or template.
    pub fn escape(&mut self) -> AliasOutcome {
        self.settle();
        match self.alias_mark() {
            Some(m) if !m.unknown => {
                self.alias = None;
                let row = self.row();
                let name: String = row.0[m.start..m.end].iter().filter_map(|i| if let Item::Char(c) = i { Some(*c) } else { None }).collect();
                if name.is_empty() {
                    return AliasOutcome::Cancelled;
                }
                match aliases::lookup_cell(&name) {
                    Some(Alias::Text(t)) => {
                        self.select(&m.path, m.start..m.end);
                        self.insert_text(t);
                        AliasOutcome::Replaced
                    }
                    Some(Alias::Template(t)) => {
                        self.select(&m.path, m.start..m.end);
                        self.delete_selection();
                        self.insert_template(t);
                        AliasOutcome::Replaced
                    }
                    None => {
                        self.unknown = Some(AliasMark { unknown: true, ..m });
                        AliasOutcome::Unknown
                    }
                }
            }
            _ => {
                self.delete_selection();
                self.unknown = None;
                self.alias = Some((self.caret.path.clone(), self.caret.offset));
                AliasOutcome::Started
            }
        }
    }

    /// The pending alias (while the caret stays in its row after its start, on characters only) or the last
    /// unknown one, for drawing the marker and colouring the name.
    pub fn alias_mark(&self) -> Option<AliasMark> {
        if let Some((p, s)) = &self.alias {
            let row = self.root.at(p)?;
            let ok = *p == self.caret.path
                && self.caret.offset >= *s
                && self.caret.offset <= row.len()
                && row.0[*s..self.caret.offset].iter().all(|i| matches!(i, Item::Char(c) if !c.is_whitespace() || *c == ' '));
            if ok {
                return Some(AliasMark { path: p.clone(), start: *s, end: self.caret.offset, unknown: false });
            }
        }
        self.unknown.clone()
    }
}

/// The start of the primary ending at `k` in `row`, if any.
/// Whether a `*` belongs at the boundary before item `j` of `row`: a box meets an operand across it (spaces and tabs
/// between them skipped). Left of a box: a box, a closing bracket, a prime, a string or an identifier (a number is
/// left alone: `2((sqrt(2)))` already multiplies). Right of a box: a box, an identifier or number, `(` or a string
/// (`[` after a box is an index).
fn needs_times(row: &Row, j: usize) -> bool {
    if j == 0 || j > row.len() {
        return false;
    }
    let blank = |it: &Item| matches!(it, Item::Char(' ' | '\t'));
    let Some(l) = (0..j).rev().find(|&i| !blank(&row.0[i])) else { return false };
    let Some(r) = (j..row.len()).find(|&i| !blank(&row.0[i])) else { return false };
    // only the boundary itself, or the spaces next to it, may separate them
    if !(l + 1 == j || r == j) {
        return false;
    }
    let left_box = matches!(row.0[l], Item::Box(_));
    let right_box = matches!(row.0[r], Item::Box(_));
    let left_end = match row.0[l] {
        Item::Box(_) => true,
        Item::Char(c) if is_close(c) || c == '\'' || c == '"' => true,
        Item::Char(c) if is_word(c) => {
            // an identifier, not a number: its first character is not a digit
            let s = word_start(row, l + 1);
            matches!(row.0[s], Item::Char(f) if !f.is_ascii_digit() && f != '.')
        }
        _ => false,
    };
    let right_start = match row.0[r] {
        Item::Box(_) => true,
        Item::Char(c) => is_word(c) || c == '(' || c == '"',
        Item::Placeholder => false,
    };
    (right_box && left_end) || (left_box && right_start)
}

fn primary_before(row: &Row, k: usize) -> Option<Range<usize>> {
    if k == 0 {
        return None;
    }
    let ch = |j: usize| match row.0.get(j) {
        Some(Item::Char(c)) => Some(*c),
        _ => None,
    };
    match &row.0[k - 1] {
        Item::Box(_) | Item::Placeholder => Some(k - 1..k),
        Item::Char(c) if is_close(*c) => {
            let open = matching_open(row, k - 1)?;
            // a call or an index: the name (or bracket group) it applies to
            let start = match open.checked_sub(1).and_then(ch) {
                Some(p) if is_word(p) || p == '\'' => word_start(row, open),
                _ => open,
            };
            Some(start..k)
        }
        Item::Char('"') => {
            let open = (0..k - 1).rev().find(|&j| ch(j) == Some('"') && (j == 0 || ch(j - 1) != Some('\\')))?;
            Some(open..k)
        }
        Item::Char(c) if is_word(*c) || *c == '\'' || *c == '.' => {
            let s = word_start(row, k);
            (s < k).then_some(s..k)
        }
        _ => None,
    }
}

/// The start of the last name or number token ending at `k` (`2x` gives `x`; `x1`, `1.5`, `f''` are one token).
fn word_start(row: &Row, k: usize) -> usize {
    let ch = |j: usize| match row.0.get(j) {
        Some(Item::Char(c)) => Some(*c),
        _ => None,
    };
    let mut run = k;
    while run > 0 && ch(run - 1).is_some_and(|c| is_word(c) || c == '.' || c == '\'') {
        run -= 1;
    }
    // tokenise the run forwards: a number is digits with an optional fraction; a name starts with a letter or `_`
    let mut starts = Vec::new();
    let mut j = run;
    while j < k {
        let c = ch(j).unwrap_or(' ');
        starts.push(j);
        if c.is_ascii_digit() {
            while j < k && ch(j).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
            }
            if ch(j) == Some('.') && ch(j + 1).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
                while j < k && ch(j).is_some_and(|c| c.is_ascii_digit()) {
                    j += 1;
                }
            }
        } else if is_word(c) {
            while j < k && ch(j).is_some_and(is_word) {
                j += 1;
            }
            while j < k && ch(j) == Some('\'') {
                j += 1;
            }
        } else {
            j += 1;
        }
    }
    starts.last().copied().unwrap_or(k)
}

/// The opening bracket matching the closing one at `close` (boxes count as single items).
fn matching_open(row: &Row, close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for j in (0..close).rev() {
        match row.0[j] {
            Item::Char(c) if is_close(c) => depth += 1,
            Item::Char(c) if is_open(c) => {
                if depth == 0 {
                    return Some(j);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Every caret position of a tree in reading order, with whether a placeholder sits there: (ordinal, position,
/// placeholder).
fn positions(root: &Row) -> Vec<(usize, Caret, bool)> {
    fn walk(r: &Row, path: &mut RowPath, out: &mut Vec<(usize, Caret, bool)>) {
        for i in 0..=r.len() {
            let ph = matches!(r.0.get(i), Some(Item::Placeholder));
            let n = out.len();
            out.push((n, Caret { path: path.clone(), offset: i }, ph));
            if let Some(Item::Box(b)) = r.0.get(i) {
                for k in 0..b.slot_count() {
                    path.push(Step { item: i, slot: k });
                    if let Some(s) = b.slot(k) {
                        walk(s, path, out);
                    }
                    path.pop();
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut Vec::new(), &mut out);
    out
}

/// The first occurrence of a character in a tree: its row path and index.
fn find_char(r: &Row, c: char) -> Option<(RowPath, usize)> {
    fn walk(r: &Row, c: char, path: &mut RowPath) -> Option<(RowPath, usize)> {
        for (i, it) in r.0.iter().enumerate() {
            match it {
                Item::Char(x) if *x == c => return Some((path.clone(), i)),
                Item::Box(b) => {
                    for k in 0..b.slot_count() {
                        path.push(Step { item: i, slot: k });
                        let found = b.slot(k).and_then(|s| walk(s, c, path));
                        path.pop();
                        if found.is_some() {
                            return found;
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(r, c, &mut Vec::new())
}

/// The linear items a box leaves when Backspace at the start of slot `slot` (or Delete at its end, `forward`) removes
/// it, and where the caret goes among them: a radicand alone; `num/den` and `base^exp` with parentheses around a slot
/// that would not read back as one operand.
fn unwrap_box(b: &Node, slot: usize, forward: bool) -> (Vec<Item>, usize) {
    let clean = |r: &Row| -> Vec<Item> { r.0.iter().filter(|i| !matches!(i, Item::Placeholder)).cloned().collect() };
    let (a, op, z, paren_a, paren_z) = match b {
        Node::Radical { radicand, index: None } => {
            let items = clean(radicand);
            let at = if forward { items.len() } else { 0 };
            return (items, at);
        }
        Node::Fraction { num, den } => (num, '/', den, needs_parens(num, false), needs_parens(den, true)),
        Node::Superscript { base, exp } => {
            let exp_power = matches!(exp.0.as_slice(), [Item::Box(x)] if matches!(**x, Node::Superscript { .. }));
            (base, '^', exp, needs_parens(base, true), needs_parens(exp, !exp_power))
        }
        other => {
            // any other box: its slots' contents one after the other
            let mut items = Vec::new();
            let mut at = 0;
            for (k, s) in other.slots().into_iter().enumerate() {
                if k == slot + usize::from(forward) {
                    at = items.len();
                }
                items.extend(clean(s));
            }
            return (items, at);
        }
    };
    let mut items = Vec::new();
    let wrap = |items: &mut Vec<Item>, r: &Row, p: bool| {
        if p {
            items.push(Item::Char('('));
        }
        items.extend(clean(r));
        if p {
            items.push(Item::Char(')'));
        }
    };
    wrap(&mut items, a, paren_a);
    let op_at = items.len();
    items.push(Item::Char(op));
    wrap(&mut items, z, paren_z);
    let at = match (slot, forward) {
        (0, false) => 0,
        (_, false) => op_at + 1,
        (0, true) => op_at,
        (_, true) => items.len(),
    };
    (items, at)
}

/// Whether a slot's contents need parentheses to stay one operand next to `/` or `^`: more than one item with an
/// operator, a space or a comma at its top level, or (`strict`, the side where a box would regroup) any box.
fn needs_parens(r: &Row, strict: bool) -> bool {
    let items: Vec<&Item> = r.0.iter().filter(|i| !matches!(i, Item::Placeholder)).collect();
    if items.len() <= 1 {
        return strict && matches!(items.first(), Some(Item::Box(_)));
    }
    let mut depth = 0i32;
    for i in &items {
        match i {
            Item::Char('(' | '[' | '{') => depth += 1,
            Item::Char(')' | ']' | '}') => depth -= 1,
            Item::Char(c) if depth == 0 && "+-*/^ ,=<>;:".contains(*c) => return true,
            Item::Box(_) if strict => return true,
            _ => {}
        }
    }
    false
}
