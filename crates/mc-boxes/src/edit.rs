//! Editing a box tree (design 08, sections 4.1 to 4.3 and 3.9): the caret and selection in boxes, typing,
//! deleting, moving, the structure commands with their operand rules, placeholders and `Tab`, `Ctrl+Space`,
//! `Ctrl+5`, grids, aliases and templates.
//!
//! The caret is a row path and an offset; a selection is a range of items in the caret's row (selections never
//! cross a box boundary: extending out of a slot selects the whole box). Commands return whether they did
//! something, so the caller can move across cells, beep or fall back (Tab without placeholders indents).
use crate::aliases::{self, Alias};
use crate::canonical::{from_text, to_text};
use crate::linear::linear_text;
use crate::ops::{is_close, is_open, is_word, pair_around};
use crate::tree::{write_notation, BigOp, Item, Node, Row, RowPath, Step, MAX_NESTING};
use std::ops::Range;

/// A caret position: the row and the offset in it (`0..=len`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Caret {
    /// the row
    pub path: RowPath,
    /// the offset in the row
    pub offset: usize,
}

/// The structure commands (design 08, section 4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Structure {
    /// `Ctrl+/`
    Fraction,
    /// `Ctrl+^` (`Ctrl+6`)
    Superscript,
    /// `Ctrl+_` (`Ctrl+-`)
    Subscript,
    /// `Ctrl+2`
    Radical,
    /// `Ctrl+7`
    Overscript,
    /// `Ctrl+Shift+,` (`Ctrl+=`)
    Underscript,
    /// `Ctrl+Enter`: a grid with a new row (in a grid: add a row)
    GridRow,
    /// `Ctrl+,`: a grid with a new column (in a grid: add a column)
    GridColumn,
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
    /// the other end of the selection, an offset in the caret's row
    anchor: Option<usize>,
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

    /// The selection: the caret's row and the selected item range (`None` when nothing is selected).
    pub fn selection(&self) -> Option<(RowPath, Range<usize>)> {
        let a = self.anchor?;
        let b = self.caret.offset;
        (a != b).then(|| (self.caret.path.clone(), a.min(b)..a.max(b)))
    }

    /// Selects items `range` of the row at `path` (the caret goes to the range's end).
    pub fn select(&mut self, path: &[Step], range: Range<usize>) {
        self.set_caret(path, range.end);
        let len = self.row().len();
        self.anchor = Some(range.start.min(len));
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
            if pos.path != caret.path.as_slice() {
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
        let edge = if forward { last.slot + 1 == n } else { last.slot == 0 };
        if edge {
            self.set_caret(&parent, last.item + usize::from(forward));
            return true;
        }
        if matches!(**b, Node::Grid { .. } | Node::BigOp { .. }) {
            // these are not dissolved: step to the neighbouring slot
            let k = if forward { last.slot + 1 } else { last.slot - 1 };
            let len = b.slot(k).map_or(0, |r| r.len());
            let mut p = parent;
            p.push(Step { item: last.item, slot: k });
            self.set_caret(&p, if forward { 0 } else { len });
            return true;
        }
        // remove the structure, keeping the slots' contents; the caret at the junction
        let junction_slot = if forward { last.slot + 1 } else { last.slot };
        let mut items = Vec::new();
        let mut caret_at = 0;
        for (k, s) in b.slots().into_iter().enumerate() {
            if k == junction_slot {
                caret_at = items.len();
            }
            items.extend(s.0.iter().filter(|i| !matches!(i, Item::Placeholder)).cloned());
        }
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
            let anchor = self.anchor.unwrap_or(k);
            if inside {
                let step = self.step_len(forward);
                self.caret.offset = if forward { k + step } else { k - step };
                self.anchor = Some(anchor);
                return true;
            }
            // leaving the row: select the whole box in the parent row
            let Some(last) = self.caret.path.pop() else { return false };
            self.caret.offset = if forward { last.item + 1 } else { last.item };
            self.anchor = Some(if forward { last.item } else { last.item + 1 });
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

    /// Home: the start of the caret's row.
    pub fn move_home(&mut self, extend: bool) {
        self.jump_to_row_edge(false, extend);
    }

    /// End: the end of the caret's row.
    pub fn move_end(&mut self, extend: bool) {
        self.jump_to_row_edge(true, extend);
    }

    fn jump_to_row_edge(&mut self, end: bool, extend: bool) {
        let k = self.caret.offset;
        if extend {
            self.anchor = Some(self.anchor.unwrap_or(k));
        } else {
            self.anchor = None;
        }
        self.caret.offset = if end { self.row().len() } else { 0 };
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
        let order = positions(&self.root);
        let here = self.ordinal(&order, true);
        let found = order.iter().filter(|(_, _, ph)| *ph).find(|(o, _, _)| *o > here).cloned();
        self.select_found(found)
    }

    /// `Shift+Tab`: selects the previous placeholder in reading order. False when there is none before the caret.
    pub fn prev_placeholder(&mut self) -> bool {
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
        if matches!(s, Structure::GridRow | Structure::GridColumn) && self.selection().is_none() && self.grow_grid(s) {
            return true;
        }
        let (range, from_selection) = match self.selection() {
            Some((_, r)) => (Some(r), true),
            None => (self.operand(s), false),
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
            Structure::Subscript => {
                // a subscript on a superscript box wraps the base in parentheses first
                let base = match operand.0.as_slice() {
                    [Item::Box(b)] if matches!(**b, Node::Superscript { .. }) && !from_selection => {
                        let mut v = vec![Item::Char('(')];
                        v.extend(operand.0);
                        v.push(Item::Char(')'));
                        Row(v)
                    }
                    _ => operand,
                };
                (Node::Subscript { base, index: ph() }, usize::from(has))
            }
            Structure::Overscript => (Node::Overscript { base: operand, over: ph() }, usize::from(has)),
            Structure::Underscript => (Node::Underscript { base: operand, under: ph() }, usize::from(has)),
            Structure::Radical => (Node::Radical { radicand: operand, index: None }, 0),
            Structure::GridRow => (Node::Grid { rows: vec![vec![operand], vec![ph()]] }, 1),
            Structure::GridColumn => (Node::Grid { rows: vec![vec![operand, ph()]] }, 1),
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

    /// In a grid cell: add a row below or a column to the right, caret in the new cell of the same column/row.
    fn grow_grid(&mut self, s: Structure) -> bool {
        let Some(last) = self.caret.path.last().copied() else { return false };
        let parent: RowPath = self.caret.path[..self.caret.path.len() - 1].to_vec();
        let Some(Item::Box(b)) = self.root.at_mut(&parent).and_then(|r| r.0.get_mut(last.item)) else { return false };
        let Node::Grid { rows } = &mut **b else { return false };
        let w = rows.first().map_or(1, |r| r.len().max(1));
        let (r, c) = (last.slot / w, last.slot % w);
        let slot = if s == Structure::GridRow {
            rows.insert(r + 1, vec![Row::placeholder(); w]);
            (r + 1) * w + c
        } else {
            for row in rows.iter_mut() {
                row.insert(c + 1, Row::placeholder());
            }
            r * (w + 1) + c + 1
        };
        let mut p = parent;
        p.push(Step { item: last.item, slot });
        self.select(&p, 0..1);
        true
    }

    /// The operand before the caret for a structure command: the last primary ending at the caret (a name or
    /// number token, a bracketed group with the name it calls, a string, a box); for scripts after an operator,
    /// that operator; for grids with no primary, the row segment back to an open bracket or separator.
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
        match s {
            Structure::Superscript | Structure::Subscript | Structure::Overscript | Structure::Underscript => match row.0[k - 1] {
                Item::Char(c) if !(c.is_whitespace() || is_open(c) || matches!(c, ',' | ';')) => Some(k - 1..k),
                _ => None,
            },
            Structure::GridRow | Structure::GridColumn => {
                let mut depth = 0usize;
                let mut start = 0;
                for j in (0..k).rev() {
                    match row.0[j] {
                        Item::Char(c) if is_close(c) => depth += 1,
                        Item::Char(c) if is_open(c) => {
                            if depth == 0 {
                                start = j + 1;
                                break;
                            }
                            depth -= 1;
                        }
                        Item::Char(',' | ';' | '\n') if depth == 0 => {
                            start = j + 1;
                            break;
                        }
                        _ => {}
                    }
                }
                (start < k).then_some(start..k)
            }
            _ => None,
        }
    }

    /// `Ctrl+5`: to the other script position: subscript to superscript and back (creating the missing one),
    /// under to over, a radical's radicand to its index, a big operator's lower limit to its upper. False (does
    /// nothing) when the innermost structure has no such slot.
    pub fn switch_script(&mut self) -> bool {
        let Some(last) = self.caret.path.last().copied() else { return false };
        let parent: RowPath = self.caret.path[..self.caret.path.len() - 1].to_vec();
        let Some(b) = self.root.box_of(&self.caret.path).cloned() else { return false };
        let goto = |ed: &mut Editor, path: RowPath| {
            let len = ed.root.at(&path).map_or(0, |r| r.len());
            ed.caret = Caret { path, offset: len };
            ed.anchor = None;
            true
        };
        let with = |p: &[Step], item: usize, slot: usize| {
            let mut v = p.to_vec();
            v.push(Step { item, slot });
            v
        };
        let replace = |ed: &mut Editor, n: Node| {
            if let Some(r) = ed.root.at_mut(&parent) {
                r.0[last.item] = Item::Box(Box::new(n));
            }
        };
        match (b, last.slot) {
            (Node::Subscript { .. }, 1) => {
                // the base of a superscript already: go to its exponent
                if let (Some(gp), Some(Node::Superscript { base, .. })) = (parent.last(), self.root.box_of(&parent)) {
                    if base.len() == 1 && gp.slot == 0 {
                        return goto(self, with(&parent[..parent.len() - 1], gp.item, 1));
                    }
                }
                let sub = self.root.at(&parent).and_then(|r| r.0.get(last.item)).cloned();
                let Some(sub) = sub else { return false };
                replace(self, Node::Superscript { base: Row(vec![sub]), exp: Row::placeholder() });
                goto(self, with(&parent, last.item, 1))
            }
            (Node::Superscript { base, exp }, 1) => {
                if let [Item::Box(inner)] = base.0.as_slice() {
                    if matches!(**inner, Node::Subscript { .. }) {
                        let mut p = with(&parent, last.item, 0);
                        p.push(Step { item: 0, slot: 1 });
                        return goto(self, p);
                    }
                }
                let sub = Node::Subscript { base, index: Row::placeholder() };
                replace(self, Node::Superscript { base: Row::boxed(sub), exp });
                let mut p = with(&parent, last.item, 0);
                p.push(Step { item: 0, slot: 1 });
                self.select(&p, 0..1);
                true
            }
            (Node::Underscript { base, under }, 1) => {
                replace(self, Node::Underoverscript { base, under, over: Row::placeholder() });
                let p = with(&parent, last.item, 2);
                self.select(&p, 0..1);
                true
            }
            (Node::Overscript { base, over }, 1) => {
                replace(self, Node::Underoverscript { base, under: Row::placeholder(), over });
                let p = with(&parent, last.item, 1);
                self.select(&p, 0..1);
                true
            }
            (Node::Underoverscript { .. }, k @ (1 | 2)) => goto(self, with(&parent, last.item, 3 - k)),
            (Node::Radical { radicand, index: None }, 0) => {
                replace(self, Node::Radical { radicand, index: Some(Row::placeholder()) });
                let p = with(&parent, last.item, 0);
                self.select(&p, 0..1);
                true
            }
            (Node::Radical { index: Some(_), .. }, k) => goto(self, with(&parent, last.item, 1 - k.min(1))),
            (Node::BigOp { op, lower: Some(_), upper: Some(_), .. }, k) => {
                // slots: integral lower 0, upper 1; others lower 1, upper 2
                let (lo, hi) = if op == BigOp::Integral { (0, 1) } else { (1, 2) };
                match k {
                    _ if k == lo => goto(self, with(&parent, last.item, hi)),
                    _ if k == hi => goto(self, with(&parent, last.item, lo)),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    // ---- selection growth ----

    /// `Ctrl+.`: grows the selection: the word at the caret, then the whole row, then the enclosing box, and so
    /// on outwards. False when the whole tree is already selected.
    pub fn expand_selection(&mut self) -> bool {
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
                self.anchor = Some(last.item);
                true
            }
            None => false,
        }
    }

    // ---- aliases ----

    /// `Esc` (design 08, section 3.9): starts an alias at the caret, or ends one: the name typed since the start
    /// is looked up and replaced by its character or template.
    pub fn escape(&mut self) -> AliasOutcome {
        match self.alias_mark() {
            Some(m) if !m.unknown => {
                self.alias = None;
                let row = self.row();
                let name: String = row.0[m.start..m.end].iter().filter_map(|i| if let Item::Char(c) = i { Some(*c) } else { None }).collect();
                if name.is_empty() {
                    return AliasOutcome::Cancelled;
                }
                match aliases::lookup(&name) {
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
