//! Two-dimensional input (design 4.1 to 4.4 and 4.6, decision NB5.3): an Input cell marked `typeset` holds
//! canonical box text (4.4) and is edited through an [`mc_boxes::Editor`] while it has the caret. Every edit writes
//! the editor's canonical text back to the cell (so undo, saving and evaluation see ordinary text: the canonical
//! form evaluates as displayed), and the editor is rebuilt from the text whenever they differ (after undo).
//!
//! A plain Input cell becomes typeset when a structure command or a template alias is used in it (its text is
//! read as canonical text, which keeps every character), or on `Ctrl+Shift+N` (its text is parsed and every
//! construct with a box form becomes the box); `Ctrl+Shift+I` turns a typeset cell back into linear text.
use crate::cell::{Cell, CellId};
use crate::command::{Command, Effects, Motion};
use crate::env::{Clip, Env};
use crate::notebook::{Notebook, Run, Selection};
use crate::style::Style;
use mc_boxes::{AliasOutcome, Editor, RowPath};

/// The editor of the typeset cell with the caret.
#[derive(Clone, Debug)]
pub(crate) struct BoxEdit {
    /// the cell
    pub cell: CellId,
    /// its tree, caret and selection
    pub ed: Editor,
}

/// Whether a cell is edited as boxes: an editable code cell marked `typeset`, not showing its source.
pub fn is_boxed(c: &Cell) -> bool {
    c.props.typeset && c.style.is_code() && !c.expr_view && c.editable() && !c.generated()
}

/// Whether a cell may become typeset input (a plain Input or Code cell).
fn can_box(c: &Cell) -> bool {
    matches!(c.style, Style::Input | Style::Code) && !c.expr_view && c.editable()
}

impl Notebook {
    /// The typeset cell with the caret and its editor (for drawing the caret, the selection and alias marks).
    pub fn box_editor(&self) -> Option<(CellId, &Editor)> {
        self.boxed.as_ref().map(|b| (b.cell, &b.ed))
    }

    /// Keep the editor in step with the selection and the cell's text: one for a typeset cell with the caret,
    /// rebuilt when the text changed under it (undo), none elsewhere.
    pub(crate) fn sync_boxed(&mut self) {
        let Selection::Caret { cell, .. } = self.sel else {
            self.boxed = None;
            return;
        };
        let Some(c) = self.cell(cell).filter(|c| is_boxed(c)) else {
            self.boxed = None;
            return;
        };
        let fresh = match &self.boxed {
            Some(b) => b.cell != cell || b.ed.text() != c.text,
            None => true,
        };
        if fresh {
            let mut ed = Editor::from_text(&c.text);
            // entered from before it (Right, Down, a click at its start): the caret at the start
            if matches!(self.sel, Selection::Caret { pos: 0, .. }) {
                ed.set_caret(&[], 0);
            }
            self.boxed = Some(BoxEdit { cell, ed });
        }
    }

    /// Write the editor's text to its cell (an undoable change); the model's caret stays at the text's end.
    fn commit_box(&mut self) -> bool {
        let Some(b) = &self.boxed else { return false };
        let (id, text) = (b.cell, b.ed.text());
        let changed = self.cell(id).is_some_and(|c| c.text != text);
        if changed {
            self.set_text(id, text.clone(), true);
        }
        self.sel = Selection::Caret { cell: id, anchor: text.len(), pos: text.len() };
        changed
    }

    /// Make the caret's plain Input cell typeset, keeping its text (canonical reading), with the editor's caret at
    /// the same character. `false` when the cell cannot be typeset.
    pub(crate) fn box_current(&mut self) -> bool {
        let Selection::Caret { cell, anchor, pos } = self.sel else { return false };
        let Some(c) = self.cell(cell) else { return false };
        if is_boxed(c) {
            return true;
        }
        if !can_box(c) {
            return false;
        }
        let chars = |p: usize| c.text[..p.min(c.text.len())].chars().count();
        let (a, p) = (chars(anchor), chars(pos));
        let mut ed = Editor::from_text(&c.text);
        if a == p {
            ed.set_caret(&[], p);
        } else {
            ed.select(&[], a.min(p)..a.max(p));
        }
        self.update_cell(cell, true, |c| c.props.typeset = true);
        self.boxed = Some(BoxEdit { cell, ed });
        self.commit_box();
        true
    }

    /// `Ctrl+Shift+N` on input cells: parse the linear text of the selected plain Input cells and make every
    /// construct with a box form a box (4.6). `false` when there was nothing to convert or the text does not
    /// parse (nothing changes then).
    pub(crate) fn inputs_to_typeset(&mut self) -> bool {
        let ids: Vec<CellId> =
            self.selected_cells().into_iter().filter(|id| self.cell(*id).is_some_and(|c| can_box(c) && !c.props.typeset)).collect();
        let mut done = false;
        for id in ids {
            let Some(c) = self.cell(id) else { continue };
            let Ok(row) = mc_boxes::typeset_input(&c.text) else { continue };
            let text = mc_boxes::to_text(&row);
            self.set_text(id, text.clone(), true);
            self.update_cell(id, true, |c| c.props.typeset = true);
            if matches!(self.sel, Selection::Caret { cell, .. } if cell == id) {
                self.sel = Selection::Caret { cell: id, anchor: text.len(), pos: text.len() };
            }
            done = true;
        }
        self.sync_boxed();
        done
    }

    /// `Ctrl+Shift+I` on input cells: the selected typeset cells become linear text with minimal parentheses.
    pub(crate) fn inputs_to_linear(&mut self) -> bool {
        let ids: Vec<CellId> = self.selected_cells().into_iter().filter(|id| self.cell(*id).is_some_and(|c| c.props.typeset && c.style.is_code())).collect();
        for &id in &ids {
            let Some(c) = self.cell(id) else { continue };
            let text = mc_boxes::linear_text(&mc_boxes::from_text(&c.text));
            self.set_text(id, text.clone(), true);
            self.update_cell(id, true, |c| c.props.typeset = false);
            if matches!(self.sel, Selection::Caret { cell, .. } if cell == id) {
                self.sel = Selection::Caret { cell: id, anchor: text.len(), pos: text.len() };
            }
        }
        self.boxed = None;
        !ids.is_empty()
    }

    /// A template alias typed in a plain Input cell: the cell becomes typeset and the template goes in.
    pub(crate) fn template_in_plain(&mut self, mark: usize, pos: usize, template: &str) -> bool {
        let Selection::Caret { cell, .. } = self.sel else { return false };
        let Some(c) = self.cell(cell).filter(|c| can_box(c)) else { return false };
        let mut t = c.text.clone();
        t.replace_range(mark..pos, "");
        self.set_text(cell, t, true);
        self.sel = Selection::Caret { cell, anchor: mark, pos: mark };
        if !self.box_current() {
            return false;
        }
        let Some(b) = self.boxed.as_mut() else { return false };
        b.ed.insert_template(template);
        self.commit_box();
        true
    }

    /// Run `cmd` in the typeset cell with the caret, if there is one and the command is an editing command.
    /// `None`: not handled here (the generic code runs); `Some(run)`: handled.
    pub(crate) fn box_dispatch(&mut self, cmd: &Command, env: &mut dyn Env, fx: &mut Effects) -> Option<Option<Run>> {
        use Command as C;
        // structure commands make a plain Input cell typeset first
        if matches!(cmd, C::Structure(_)) && self.boxed.is_none() && !self.box_current() {
            fx.beep = true;
            return Some(None);
        }
        let b = self.boxed.as_mut()?;
        let id = b.cell;
        let ed = &mut b.ed;
        let now = env.now();
        let mut run = None;
        match cmd {
            C::Insert(t) => {
                ed.insert_text(t);
                run = Some(Run::Typing(id, t.ends_with(char::is_whitespace), now));
            }
            C::Enter => ed.insert_char('\n'),
            C::Backspace | C::DeleteWordBack => {
                if !ed.backspace() {
                    return Some(None);
                }
                run = Some(Run::Deleting(id, now));
            }
            C::Delete | C::DeleteWordForward => {
                if !ed.delete() {
                    return Some(None);
                }
                run = Some(Run::Deleting(id, now));
            }
            C::Move(m, extend) => {
                let moved = match m {
                    Motion::Left | Motion::WordLeft => ed.move_left(*extend),
                    Motion::Right | Motion::WordRight => ed.move_right(*extend),
                    Motion::Home => {
                        ed.move_home(*extend);
                        true
                    }
                    Motion::End => {
                        ed.move_end(*extend);
                        true
                    }
                    Motion::Up | Motion::Down => {
                        let up = *m == Motion::Up;
                        let inside = if up { ed.move_up() } else { ed.move_down() };
                        inside || {
                            let caret = ed.caret().clone();
                            let c = self.cell(id).expect("boxed cell").clone();
                            match env.box_line(&c, &caret.path, caret.offset, up) {
                                Some((p, k)) => {
                                    let b = self.boxed.as_mut().expect("boxed");
                                    b.ed.set_caret(&p, k);
                                    true
                                }
                                None => false,
                            }
                        }
                    }
                    Motion::DocStart | Motion::DocEnd => return None,
                };
                if !moved {
                    // past the content: to the insertion point before or after the cell
                    let i = self.index_of(id)?;
                    let back = matches!(m, Motion::Left | Motion::WordLeft | Motion::Up);
                    if *extend {
                        return Some(None);
                    }
                    let g = if back { i } else { self.gap_after_cell(i) };
                    self.boxed = None;
                    self.select(Selection::Gap(g));
                    return Some(None);
                }
                return Some(None);
            }
            C::Tab => {
                if !ed.next_placeholder() {
                    fx.beep = true;
                }
                return Some(None);
            }
            C::Outdent => {
                if !ed.prev_placeholder() {
                    fx.beep = true;
                }
                return Some(None);
            }
            C::Alias => {
                if ed.escape() == AliasOutcome::Unknown {
                    fx.beep = true;
                }
            }
            C::ExtendSelection => {
                if !ed.expand_selection() {
                    self.boxed = None;
                    self.select(Selection::Cells { ids: vec![id], anchor: id });
                }
                return Some(None);
            }
            C::SelectAll => {
                let n = ed.root().len();
                let all = ed.selection().is_some_and(|(p, r)| p.is_empty() && r == (0..n));
                if all {
                    return None;
                }
                ed.select_all();
                return Some(None);
            }
            C::Copy | C::CopyPlain | C::Cut => {
                let t = ed.selected_text();
                if t.is_empty() {
                    fx.beep = true;
                    return Some(None);
                }
                // the text flavour is canonical text (pasting it back gives the boxes; it evaluates the same);
                // "copy as plain text" gives linear text
                let text = if *cmd == C::CopyPlain { mc_boxes::linear_text(&mc_boxes::from_text(&t)) } else { t };
                env.set_clipboard(Clip { text: Some(text), cells: None });
                if *cmd != C::Cut {
                    return Some(None);
                }
                ed.cut();
            }
            C::Paste => {
                let clip = env.clipboard();
                if clip.cells.is_some() {
                    return None;
                }
                let Some(t) = clip.text else { return Some(None) };
                let b = self.boxed.as_mut().expect("boxed");
                if !b.ed.paste(&t) {
                    fx.beep = true;
                }
            }
            C::Structure(s) => {
                if !ed.structure(*s) {
                    fx.beep = true;
                }
            }
            C::ExitStructure => {
                if !ed.exit_structure() {
                    fx.beep = true;
                }
                return Some(None);
            }
            C::SwitchScript => {
                if !ed.switch_script() {
                    fx.beep = true;
                }
            }
            C::ToggleComment | C::CheckBalance | C::Divide => {
                fx.beep = true;
                return Some(None);
            }
            _ => return None,
        }
        self.commit_box();
        Some(run)
    }

    /// The insertion point after cell `i` (past its outputs' group as for text cells).
    fn gap_after_cell(&self, i: usize) -> usize {
        let mut j = i + 1;
        while j < self.cells.len() && !self.is_visible(j) {
            j += 1;
        }
        j
    }

    /// A click in a typeset cell: the caret there (a cell without the caret gets it first).
    pub(crate) fn click_box(&mut self, id: CellId, path: &RowPath, offset: usize, extend: bool) {
        let same = self.boxed.as_ref().is_some_and(|b| b.cell == id);
        if !same {
            let Some(c) = self.cell(id).filter(|c| is_boxed(c)) else { return };
            let n = c.text.len();
            self.select(Selection::Caret { cell: id, anchor: n, pos: n });
            self.sync_boxed();
        }
        if let Some(b) = self.boxed.as_mut() {
            if extend && same {
                let caret = b.ed.caret().clone();
                if caret.path == *path {
                    let a = b.ed.selection().map_or(caret.offset, |(_, r)| if r.end == caret.offset { r.start } else { r.end });
                    b.ed.select(path, a.min(offset)..a.max(offset));
                    return;
                }
            }
            b.ed.set_caret(path, offset);
        }
    }
}
