//! The notebook: cells, the derived groups, the selection, undo, and the primitives every change goes through.
use crate::cell::{Cell, CellId};
use crate::eval::EvalState;
use crate::group::{CellInfo, Groups};
use crate::style::Style;
use mc_nbfile::Attrs;
use std::collections::HashMap;

/// The selection (design 2.9). Exactly one is current.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    /// a caret (`anchor == pos`) or a text selection from `anchor` to `pos` in one cell's content
    Caret {
        /// the cell
        cell: CellId,
        /// where the selection started
        anchor: usize,
        /// where the caret is
        pos: usize,
    },
    /// whole cells, selected by their brackets, in document order
    Cells {
        /// the cells
        ids: Vec<CellId>,
        /// the bracket a Shift+click extends from
        anchor: CellId,
    },
    /// the insertion point before cell `i` (`i == len`: after the last cell)
    Gap(usize),
}

/// One recorded change.
#[derive(Clone, Debug)]
pub(crate) enum Op {
    /// a cell's text changed
    Text { cell: CellId, before: String, after: String },
    /// cells were inserted at `at`, after the cell `after`
    Insert { at: usize, after: Option<CellId>, cells: Vec<Cell> },
    /// cells were removed from `at`, which followed the cell `after`
    Remove { at: usize, after: Option<CellId>, cells: Vec<Cell> },
    /// a cell's style or attributes changed
    Replace { before: Box<Cell>, after: Box<Cell> },
}

/// What kind of run an undo unit belongs to, for merging typing into one unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Run {
    /// typing in a cell; whether the last typed character was white space; when
    Typing(CellId, bool, f64),
    /// deleting characters in a cell
    Deleting(CellId, f64),
}

#[derive(Clone, Debug)]
pub(crate) struct Unit {
    pub ops: Vec<Op>,
    pub sel_before: Selection,
    pub sel_after: Selection,
    pub run: Option<Run>,
}

/// Undo history: one per notebook, unlimited depth, not saved (design 3.5).
#[derive(Clone, Debug, Default)]
pub(crate) struct Undo {
    pub done: Vec<Unit>,
    pub undone: Vec<Unit>,
    /// the unit of the command being applied
    pub current: Option<Unit>,
    /// typing may continue the last unit
    pub open_run: bool,
}

/// A notebook document.
#[derive(Clone, Debug)]
pub struct Notebook {
    pub(crate) cells: Vec<Cell>,
    index: HashMap<CellId, usize>,
    /// the header attributes of the file (kept for saving)
    pub header: Attrs,
    pub(crate) groups: Groups,
    pub(crate) sel: Selection,
    /// the style the cell typing at the insertion point will create (Alt+digit at an insertion point)
    pub(crate) gap_style: Option<Style>,
    /// the horizontal position Up and Down keep
    pub(crate) goal_x: Option<f32>,
    pub(crate) undo: Undo,
    pub(crate) eval: EvalState,
    next_id: u64,
    rev: u64,
    structure_rev: u64,
    modified: bool,
}

impl Default for Notebook {
    fn default() -> Self {
        Notebook::new()
    }
}

impl Notebook {
    /// An empty notebook.
    pub fn new() -> Notebook {
        Notebook {
            cells: vec![],
            index: HashMap::new(),
            header: Attrs::new(),
            groups: Groups::default(),
            sel: Selection::Gap(0),
            gap_style: None,
            goal_x: None,
            undo: Undo::default(),
            eval: EvalState::default(),
            next_id: 1,
            rev: 1,
            structure_rev: 1,
            modified: false,
        }
    }

    /// A notebook holding `cells` (their ids are replaced by fresh ones), the insertion point after the last.
    pub fn with_cells(cells: Vec<Cell>) -> Notebook {
        let mut nb = Notebook::new();
        let mut map = HashMap::new();
        nb.cells = cells
            .into_iter()
            .map(|mut c| {
                let id = nb.fresh_id();
                map.insert(c.id, id);
                c.id = id;
                c.rev = nb.next_rev();
                c
            })
            .collect();
        for c in &mut nb.cells {
            c.input_of = c.input_of.and_then(|i| map.get(&i).copied());
        }
        nb.structure_changed();
        nb.sel = Selection::Gap(nb.cells.len());
        nb
    }

    // ----- reading -----

    /// The cells in order.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// The cell with this id.
    pub fn cell(&self, id: CellId) -> Option<&Cell> {
        self.index.get(&id).map(|&i| &self.cells[i])
    }

    /// The index of the cell with this id.
    pub fn index_of(&self, id: CellId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// The group tree.
    pub fn groups(&self) -> &Groups {
        &self.groups
    }

    /// Whether cell `i` is shown (not hidden in a closed group).
    pub fn is_visible(&self, i: usize) -> bool {
        self.groups.visible.get(i).copied().unwrap_or(false)
    }

    /// The selection.
    pub fn selection(&self) -> &Selection {
        &self.sel
    }

    /// The style the next cell typed at the insertion point gets.
    pub fn gap_style(&self) -> Style {
        self.gap_style.unwrap_or(Style::Input)
    }

    /// Changes whenever cells are inserted, removed, restyled, opened or closed (bracket and position caches key on
    /// it).
    pub fn structure_rev(&self) -> u64 {
        self.structure_rev
    }

    /// Whether the document changed since [`Notebook::mark_saved`].
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// The document was saved.
    pub fn mark_saved(&mut self) {
        self.modified = false;
    }

    /// The document changed outside the undo history (evaluation results).
    pub(crate) fn mark_modified(&mut self) {
        self.modified = true;
    }

    /// Whether the notebook-wide manual grouping mode is on.
    pub fn manual_grouping(&self) -> bool {
        self.header.get("grouping") == Some("manual")
    }

    /// The cell holding the caret or the first selected cell.
    pub fn current_cell(&self) -> Option<CellId> {
        match &self.sel {
            Selection::Caret { cell, .. } => Some(*cell),
            Selection::Cells { ids, .. } => ids.first().copied(),
            Selection::Gap(_) => None,
        }
    }

    /// The cells the selection covers: the caret's cell, the selected cells, none at an insertion point.
    pub fn selected_cells(&self) -> Vec<CellId> {
        match &self.sel {
            Selection::Caret { cell, .. } => vec![*cell],
            Selection::Cells { ids, .. } => ids.clone(),
            Selection::Gap(_) => vec![],
        }
    }

    /// The display number of numbered items (`1.`, `1.1.`), counted within the enclosing section (design 2.1).
    pub fn item_number(&self, i: usize) -> Option<String> {
        let level = self.cells[i].style.numbering_level()?;
        let mut counts = [0usize; 3];
        for c in &self.cells[..=i] {
            if c.style.resets_numbering() {
                counts = [0; 3];
            } else if let Some(l) = c.style.numbering_level() {
                counts[l - 1] += 1;
                counts[l..].iter_mut().for_each(|n| *n = 0);
            } else if matches!(c.style, Style::Item | Style::Text) {
                // an unnumbered item or text between numbered items ends their list
                counts = [0; 3];
            }
        }
        Some(counts[..level].iter().map(|n| format!("{n}.")).collect())
    }

    // ----- primitives -----

    pub(crate) fn fresh_id(&mut self) -> CellId {
        let id = CellId(self.next_id);
        self.next_id += 1;
        id
    }

    pub(crate) fn next_rev(&mut self) -> u64 {
        self.rev += 1;
        self.rev
    }

    /// A new cell with a fresh id.
    pub(crate) fn new_cell(&mut self, style: Style, text: &str) -> Cell {
        let id = self.fresh_id();
        let mut c = Cell::new(id, style, text);
        c.rev = self.next_rev();
        c
    }

    /// Recompute the index, the groups and the visibility after a structural change.
    pub(crate) fn structure_changed(&mut self) {
        self.index = self.cells.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        let infos: Vec<CellInfo> = self
            .cells
            .iter()
            .map(|c| CellInfo { style: c.style, together: c.props.together, closed: c.closed })
            .collect();
        self.groups = Groups::compute(&infos, self.manual_grouping());
        // a closed group whose head no longer heads a group forgets that it was closed
        for i in 0..self.cells.len() {
            if self.cells[i].closed.is_some() && self.groups.headed_by(i).is_none() {
                self.cells[i].closed = None;
            }
        }
        self.structure_rev += 1;
    }

    fn record(&mut self, op: Op, record: bool) {
        if record {
            if let Some(u) = self.undo.current.as_mut() {
                u.ops.push(op);
            }
        }
    }

    /// Insert cells at `at`; the insertion point moves with the cells after it.
    pub(crate) fn insert_cells(&mut self, at: usize, mut cells: Vec<Cell>, record: bool) {
        if cells.is_empty() {
            return;
        }
        let n = cells.len();
        for c in &mut cells {
            c.rev = self.next_rev();
        }
        let after = at.checked_sub(1).map(|i| self.cells[i].id);
        self.record(Op::Insert { at, after, cells: cells.clone() }, record);
        self.cells.splice(at..at, cells);
        if let Selection::Gap(g) = &mut self.sel {
            if *g >= at {
                *g += n;
            }
        }
        self.modified = true;
        self.structure_changed();
    }

    /// Remove the cells `range`; a selection in them becomes the insertion point where they were.
    pub(crate) fn remove_cells(&mut self, range: std::ops::Range<usize>, record: bool) -> Vec<Cell> {
        if range.is_empty() {
            return vec![];
        }
        let after = range.start.checked_sub(1).map(|i| self.cells[i].id);
        let (a, b) = (range.start, range.end);
        let removed: Vec<Cell> = self.cells.drain(range).collect();
        self.record(Op::Remove { at: a, after, cells: removed.clone() }, record);
        let gone = |id: &CellId| removed.iter().any(|c| c.id == *id);
        self.sel = match std::mem::replace(&mut self.sel, Selection::Gap(0)) {
            Selection::Gap(g) if g >= b => Selection::Gap(g - (b - a)),
            Selection::Gap(g) if g > a => Selection::Gap(a),
            Selection::Caret { cell, .. } if gone(&cell) => Selection::Gap(a),
            Selection::Cells { ids, anchor } => {
                let ids: Vec<_> = ids.into_iter().filter(|i| !gone(i)).collect();
                match ids.first() {
                    None => Selection::Gap(a),
                    Some(&first) => Selection::Cells { anchor: if gone(&anchor) { first } else { anchor }, ids },
                }
            }
            s => s,
        };
        self.eval.forget(&removed);
        self.modified = true;
        self.structure_changed();
        removed
    }

    /// Set a cell's text.
    pub(crate) fn set_text(&mut self, id: CellId, text: String, record: bool) {
        let Some(i) = self.index_of(id) else { return };
        if self.cells[i].text == text {
            return;
        }
        let before = std::mem::replace(&mut self.cells[i].text, text.clone());
        self.record(Op::Text { cell: id, before, after: text }, record);
        self.cells[i].rev = self.next_rev();
        self.modified = true;
    }

    /// Change a cell's style or attributes (`f` must not change its id or text).
    pub(crate) fn update_cell(&mut self, id: CellId, record: bool, f: impl FnOnce(&mut Cell)) {
        let Some(i) = self.index_of(id) else { return };
        let before = self.cells[i].clone();
        f(&mut self.cells[i]);
        if self.cells[i] == before {
            return;
        }
        self.cells[i].rev = self.next_rev();
        let after = self.cells[i].clone();
        self.record(Op::Replace { before: Box::new(before), after: Box::new(after) }, record);
        self.modified = true;
        self.structure_changed();
    }

    /// Mark a cell's look changed without changing the document (queue marks).
    pub(crate) fn retouch(&mut self, i: usize) {
        self.cells[i].rev = self.next_rev();
    }

    /// Set the selection, normalising an insertion point into a closed group to the one after it.
    pub(crate) fn select(&mut self, sel: Selection) {
        self.sel = match sel {
            Selection::Gap(g) => Selection::Gap(self.normal_gap(g)),
            Selection::Caret { cell, anchor, pos } => match self.cell(cell) {
                Some(c) => {
                    let fix = |p: usize| {
                        let mut p = p.min(c.text.len());
                        while !c.text.is_char_boundary(p) {
                            p -= 1;
                        }
                        p
                    };
                    Selection::Caret { cell, anchor: fix(anchor), pos: fix(pos) }
                }
                None => Selection::Gap(self.cells.len()),
            },
            s => s,
        };
    }

    /// The insertion point `g`, moved forward past hidden cells.
    pub(crate) fn normal_gap(&self, g: usize) -> usize {
        let mut g = g.min(self.cells.len());
        while g < self.cells.len() && !self.is_visible(g) {
            g += 1;
        }
        g
    }

    /// The end of the run of generated cells right after cell `i` (the input's output group).
    pub(crate) fn outputs_end(&self, i: usize) -> usize {
        let mut j = i + 1;
        while j < self.cells.len() && self.cells[j].generated() {
            j += 1;
        }
        j
    }

    // ----- undo -----

    pub(crate) fn begin_unit(&mut self) {
        self.undo.current =
            Some(Unit { ops: vec![], sel_before: self.sel.clone(), sel_after: self.sel.clone(), run: None });
    }

    /// End the command's unit; `run` says whether it was typing or deleting, which may merge it into the last unit.
    pub(crate) fn end_unit(&mut self, run: Option<Run>) {
        let Some(mut unit) = self.undo.current.take() else { return };
        if unit.ops.is_empty() {
            if run.is_none() {
                self.undo.open_run = false;
            }
            return;
        }
        unit.sel_after = self.sel.clone();
        unit.run = run;
        self.undo.undone.clear();
        let merged = match (self.undo.open_run, run, self.undo.done.last_mut()) {
            (true, Some(Run::Typing(c, space, t)), Some(prev)) => match prev.run {
                // a pause of 1 s or a word boundary after white space ends a typing run
                Some(Run::Typing(pc, pspace, pt)) if pc == c && t - pt < 1.0 && !(pspace && !space) => {
                    merge_text(prev, &unit.ops, unit.sel_after.clone(), run)
                }
                _ => false,
            },
            (true, Some(Run::Deleting(c, t)), Some(prev)) => match prev.run {
                Some(Run::Deleting(pc, pt)) if pc == c && t - pt < 1.0 => {
                    merge_text(prev, &unit.ops, unit.sel_after.clone(), run)
                }
                _ => false,
            },
            _ => false,
        };
        if !merged {
            self.undo.done.push(unit);
        }
        self.undo.open_run = run.is_some();
    }

    /// Undo the last unit; `false` when there is none.
    pub(crate) fn undo(&mut self) -> bool {
        let Some(unit) = self.undo.done.pop() else { return false };
        for op in unit.ops.iter().rev() {
            self.apply_op(op, true);
        }
        let sel = self.valid(unit.sel_before.clone());
        self.select(sel);
        self.undo.undone.push(unit);
        self.undo.open_run = false;
        true
    }

    /// Redo the last undone unit; `false` when there is none.
    pub(crate) fn redo(&mut self) -> bool {
        let Some(unit) = self.undo.undone.pop() else { return false };
        for op in &unit.ops {
            self.apply_op(op, false);
        }
        let sel = self.valid(unit.sel_after.clone());
        self.select(sel);
        self.undo.done.push(unit);
        self.undo.open_run = false;
        true
    }

    /// `sel` if the cells it names still exist, else the insertion point at the end.
    fn valid(&self, sel: Selection) -> Selection {
        match sel {
            Selection::Caret { cell, .. } if self.cell(cell).is_none() => Selection::Gap(self.cells.len()),
            Selection::Cells { ids, anchor } => {
                let ids: Vec<_> = ids.into_iter().filter(|i| self.cell(*i).is_some()).collect();
                match ids.first() {
                    None => Selection::Gap(self.cells.len()),
                    Some(&f) => Selection::Cells { anchor: if self.cell(anchor).is_some() { anchor } else { f }, ids },
                }
            }
            Selection::Gap(g) => Selection::Gap(g.min(self.cells.len())),
            s => s,
        }
    }

    /// Where cells recorded as following `after` (or at `at`) go back.
    fn position(&self, at: usize, after: Option<CellId>) -> usize {
        match after {
            None => 0,
            Some(id) => self.index_of(id).map_or(at.min(self.cells.len()), |i| i + 1),
        }
    }

    /// Apply `op` forwards, or its inverse when `inverse`, without recording it. Labels are not restored.
    fn apply_op(&mut self, op: &Op, inverse: bool) {
        match (op, inverse) {
            (Op::Text { cell, before, .. }, true) => self.set_text(*cell, before.clone(), false),
            (Op::Text { cell, after, .. }, false) => self.set_text(*cell, after.clone(), false),
            (Op::Insert { cells, .. }, true) | (Op::Remove { cells, .. }, false) => {
                for c in cells {
                    if let Some(i) = self.index_of(c.id) {
                        self.remove_cells(i..i + 1, false);
                    }
                }
            }
            (Op::Insert { at, after, cells }, false) | (Op::Remove { at, after, cells }, true) => {
                let at = self.position(*at, *after);
                self.insert_cells(at, cells.clone(), false);
            }
            (Op::Replace { before, after }, inverse) => {
                let target = if inverse { before } else { after };
                if let Some(i) = self.index_of(target.id) {
                    let label = self.cells[i].label;
                    let eval = self.cells[i].eval;
                    let text = std::mem::take(&mut self.cells[i].text);
                    self.cells[i] = (**target).clone();
                    self.cells[i].text = text;
                    self.cells[i].label = label;
                    self.cells[i].eval = eval;
                    self.retouch(i);
                    self.modified = true;
                    self.structure_changed();
                }
            }
        }
    }
}

/// Merge the text ops of a typing unit into the previous unit when both change the same cell.
fn merge_text(prev: &mut Unit, ops: &[Op], sel_after: Selection, run: Option<Run>) -> bool {
    let [Op::Text { cell, after, .. }] = ops else { return false };
    if prev.ops.len() != 1 {
        return false;
    }
    let Some(Op::Text { cell: pc, after: pafter, .. }) = prev.ops.last_mut() else { return false };
    if pc != cell {
        return false;
    }
    *pafter = after.clone();
    prev.sel_after = sel_after;
    prev.run = run;
    true
}
