//! The caret inside typeset output (round 11, decision NB6.4): an Output cell shown typeset is read-only boxes the
//! caret walks through like typeset input (arrows enter boxes, Shift extends keeping the anchor, `Ctrl+.` grows the
//! selection, a click puts the caret between items), so a part of an output can be selected and copied.
//!
//! The model's selection stays bytes of the printed text (what copying, Find and every other command see); the
//! read-only editor here holds the box caret, and the [`mc_typeset::SourceMap`] of the typeset output translates
//! between the two: a selection made in boxes is written as the bytes its items stand for, and a selection made in
//! bytes (Find, a click before the output) puts the box caret on the items those bytes cover.
use crate::cell::CellId;
use crate::command::{Command, Motion};
use crate::env::{Clip, Env};
use crate::notebook::{Notebook, Selection};
use crate::style::Style;
use mc_boxes::{Row, RowPath};
use mc_typeset::Typeset;
use mc_edit::{Command as E, Context, Editor, Position, Selection as TreeSel, Status};
use std::ops::Range;
use std::sync::Arc;

/// The read-only box caret of the typeset output with the caret.
#[derive(Clone, Debug)]
pub(crate) struct OutBox {
    /// the Output cell
    pub cell: CellId,
    /// its revision the boxes were made for
    rev: u64,
    /// the output typeset, with its source map
    pub typeset: Arc<Typeset>,
    /// the caret and selection in its boxes (never edited)
    pub ed: Editor,
    /// the bytes (anchor, pos) last written to the model's selection: a different selection was made in bytes
    bytes: (usize, usize),
}

impl Notebook {
    /// The typeset output with the caret and its box caret (for drawing the caret and the selection).
    pub fn output_editor(&self) -> Option<(CellId, &Editor)> {
        self.outbox.as_ref().map(|o| (o.cell, &o.ed))
    }

    /// Keep the output's box caret in step with the selection: one for an Output cell shown typeset with the caret,
    /// none elsewhere.
    pub(crate) fn sync_out(&mut self, env: &mut dyn Env) {
        let Selection::Caret { cell, anchor, pos } = self.sel else {
            self.outbox = None;
            return;
        };
        let Some(c) = self.cell(cell) else {
            self.outbox = None;
            return;
        };
        if c.style != Style::Output || c.expr_view || !env.atomic(c) {
            self.outbox = None;
            return;
        }
        let current = self.outbox.as_ref().is_some_and(|o| o.cell == cell && o.rev == c.rev);
        if !current {
            let typeset = Arc::new(mc_typeset::math_text(&c.text));
            let ed = Editor::new(typeset.row.clone());
            self.outbox = Some(OutBox { cell, rev: c.rev, typeset, ed, bytes: (usize::MAX, usize::MAX) });
        }
        let Some(o) = self.outbox.as_mut() else { return };
        if o.bytes != (anchor, pos) {
            // a selection made in bytes: the box caret goes to the items they cover
            if anchor == pos {
                let (p, k) = o.typeset.map.caret_at(pos);
                o.ed.set_selection(TreeSel::caret(Position::new(&p, k)));
            } else if let Some((p, r)) = o.typeset.map.find(anchor.min(pos)..anchor.max(pos)) {
                o.ed.set_selection(TreeSel::items(&p, r));
            }
            o.bytes = (anchor, pos);
        }
    }

    /// Write the box caret or selection to the model's selection as bytes of the printed text.
    fn write_out(&mut self) {
        let Some(o) = self.outbox.as_mut() else { return };
        let (anchor, pos) = match o.ed.range() {
            Some((path, r)) => {
                let span = o.typeset.map.span(&path, r).unwrap_or(0..0);
                // the caret's end of the selection
                if o.ed.selection().forward() {
                    (span.start, span.end)
                } else {
                    (span.end, span.start)
                }
            }
            None => {
                let k = o.typeset.map.byte_at(&o.ed.caret().path, o.ed.caret().offset).unwrap_or(0);
                (k, k)
            }
        };
        o.bytes = (anchor, pos);
        self.sel = Selection::Caret { cell: o.cell, anchor, pos };
    }

    /// The selected part of the output with the caret: its row path, item range and the items.
    fn out_selection(&self) -> Option<(RowPath, Range<usize>, Row)> {
        let o = self.outbox.as_ref()?;
        let (p, r) = o.ed.range()?;
        let row = o.ed.root().at(&p)?;
        let items = Row(row.0.get(r.clone())?.to_vec());
        Some((p, r, items))
    }

    /// The input text of the selected part of the output: the printed text it stands for when that is one stretch,
    /// else its linear text with the output spellings turned back.
    pub(crate) fn out_selected_text(&self) -> Option<String> {
        let o = self.outbox.as_ref()?;
        let (p, r, items) = self.out_selection()?;
        let printed = &self.cell(o.cell)?.text;
        if o.typeset.map.contiguous(&p, r.clone(), printed) {
            let s = o.typeset.map.span(&p, r)?;
            return printed.get(s).map(str::to_string);
        }
        Some(mc_typeset::output_input_text(&items))
    }

    /// The selected boxes of the output with the caret, or all of it (Copy as LaTeX).
    pub(crate) fn out_boxes(&self, cell: CellId) -> Option<Row> {
        let o = self.outbox.as_ref().filter(|o| o.cell == cell)?;
        Some(self.out_selection().map_or_else(|| o.ed.root().clone(), |(_, _, items)| items))
    }

    /// Run `cmd` on the typeset output with the caret. `None`: not handled here (the generic code runs).
    pub(crate) fn out_dispatch(&mut self, cmd: &Command, env: &mut dyn Env) -> Option<()> {
        use Command as C;
        // a click in an output's boxes: the caret goes there (the output gets the caret first)
        if let C::ClickBox(id, path, k, extend) = cmd {
            let output = self.cell(*id).is_some_and(|c| c.style == Style::Output);
            if !output {
                return None;
            }
            let same = self.outbox.as_ref().is_some_and(|o| o.cell == *id);
            if !same {
                self.select(Selection::Caret { cell: *id, anchor: 0, pos: 0 });
                self.sync_out(env);
            }
            let o = self.outbox.as_mut()?;
            let p = Position::new(path, *k);
            let c = if *extend && same { E::ExtendTo(p) } else { E::SetCaret(p) };
            o.ed.apply(&c, &Context::default());
            self.write_out();
            return Some(());
        }
        let o = self.outbox.as_mut()?;
        let id = o.cell;
        let cx = Context::default();
        // the output is read-only: its editor only moves and selects
        let go = |ed: &mut Editor, m: mc_edit::Motion, extend: bool| ed.apply(&E::Move(m, extend), &Context::default()).status == Status::Done;
        match cmd {
            C::Move(m, extend) => {
                let moved = match m {
                    Motion::Left | Motion::WordLeft => go(&mut o.ed, mc_edit::Motion::Left, *extend),
                    Motion::Right | Motion::WordRight => go(&mut o.ed, mc_edit::Motion::Right, *extend),
                    Motion::Home | Motion::End => {
                        let end = *m == Motion::End;
                        let caret = o.ed.caret().clone();
                        let edge = if caret.path.is_empty() {
                            let c = self.cell(id)?.clone();
                            env.box_line_edge(&c, caret.offset, end)
                        } else {
                            None
                        };
                        let o = self.outbox.as_mut()?;
                        match edge {
                            Some(k) if *extend => {
                                o.ed.apply(&E::ExtendTo(Position::root(k)), &cx);
                            }
                            Some(k) => {
                                o.ed.apply(&E::SetCaret(Position::root(k)), &cx);
                            }
                            None => {
                                go(&mut o.ed, if end { mc_edit::Motion::End } else { mc_edit::Motion::Home }, *extend);
                            }
                        }
                        true
                    }
                    // structural Up and Down inside the output; at its top and bottom the generic motion leaves it
                    Motion::Up if !extend => {
                        if !o.ed.slot_vertical(true) {
                            return None;
                        }
                        true
                    }
                    Motion::Down if !extend => {
                        if !o.ed.slot_vertical(false) {
                            return None;
                        }
                        true
                    }
                    // Shift+Up and Shift+Down: structurally where they can, else to the output's start or end, the
                    // anchor kept
                    Motion::Up | Motion::Down => {
                        let up = *m == Motion::Up;
                        let start = o.ed.selection().anchor.clone();
                        let to = if o.ed.slot_vertical(up) {
                            o.ed.caret().clone()
                        } else {
                            Position::root(if up { 0 } else { o.ed.root().len() })
                        };
                        o.ed.set_selection(TreeSel { anchor: start, head: to });
                        true
                    }
                    _ => return None,
                };
                if !moved {
                    if *extend {
                        return Some(());
                    }
                    // past either end: the insertion point before or after the output
                    let i = self.index_of(id)?;
                    let back = matches!(m, Motion::Left | Motion::WordLeft);
                    let g = if back { i } else { i + 1 };
                    self.outbox = None;
                    self.select(Selection::Gap(g));
                    return Some(());
                }
                self.write_out();
                Some(())
            }
            C::ExtendSelection => {
                if o.ed.apply(&E::ExpandSelection, &cx).status == Status::Done {
                    self.write_out();
                } else {
                    self.outbox = None;
                    self.select(Selection::Cells { ids: vec![id], anchor: id });
                }
                Some(())
            }
            C::SelectAll => {
                let n = o.ed.root().len();
                let all = o.ed.range().is_some_and(|(p, r)| p.is_empty() && r == (0..n));
                if all {
                    return None;
                }
                o.ed.apply(&E::SelectAll, &cx);
                self.write_out();
                Some(())
            }
            C::Copy | C::CopyPlain => {
                let text = self.out_selected_text()?;
                // a part with roots, fractions or powers also goes as boxes (round 13): the input boxes it
                // stands for, pasted as boxes into cells
                let part = if *cmd == C::Copy { self.out_selection() } else { None };
                let boxes = part
                    .map(|(_, _, items)| mc_typeset::output_input_boxes(&items))
                    .filter(mc_boxes::has_boxes)
                    .map(|r| mc_boxes::to_text(&r));
                env.set_clipboard(Clip { text: Some(text), cells: None, boxes });
                Some(())
            }
            _ => None,
        }
    }
}
