//! Two-dimensional input (design 4.1 to 4.4 and 4.6, decision NB5.3): an Input cell marked `typeset` holds
//! canonical box text (4.4) and is edited through an [`mc_boxes::Editor`] while it has the caret. Every edit writes
//! the editor's canonical text back to the cell (so undo, saving and evaluation see ordinary text: the canonical
//! form evaluates as displayed), and the editor is rebuilt from the text whenever they differ (after undo).
//!
//! One kind of cell (decision NBUI.5): there is no typeset mode to switch to. A code cell holds boxes exactly when one
//! of the three box keys made one (Ctrl+2 a square root, Ctrl+/ a fraction, Ctrl+6 a power); its text is then
//! canonical box text and the `typeset` mark says so (it is derived, never chosen). When the last box goes (Backspace
//! at the start of a slot unwraps it) the cell is plain text again. Either way it looks the same: the boxes and the
//! characters are drawn in the cell's own font.
use crate::cell::{Cell, CellId};
use crate::command::{Command, Effects, Motion};
use crate::env::{Clip, Env};
use crate::notebook::{Notebook, Run, Selection};
use crate::style::Style;
use mc_boxes::{AliasOutcome, Caret, Editor, Item, RowPath};

/// The editor of the typeset cell with the caret.
#[derive(Clone, Debug)]
pub(crate) struct BoxEdit {
    /// the cell
    pub cell: CellId,
    /// its tree, caret and selection
    pub ed: Editor,
    /// the text and caret before the command being run (kept for undo, NB6.5)
    before: Option<(String, Caret)>,
}

/// How many carets of typeset cells are remembered for undo and redo.
const CARETS_KEPT: usize = 256;

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
        let n = c.text.len();
        if fresh {
            let same_cell = self.boxed.as_ref().is_some_and(|b| b.cell == cell);
            let mut ed = Editor::from_text(&c.text);
            // the text changed under the editor (undo, redo): the caret it had with that text (NB6.5)
            let remembered = same_cell
                .then(|| self.box_carets.iter().rev().find(|(id, t, _)| *id == cell && *t == c.text).map(|(_, _, k)| k.clone()))
                .flatten();
            let restored = remembered.is_some();
            if let Some(k) = remembered {
                ed.set_caret(&k.path, k.offset);
            }
            self.boxed = Some(BoxEdit { cell, ed, before: None });
            if restored {
                self.sel = Selection::Caret { cell, anchor: n, pos: n };
                return;
            }
        }
        // a caret or selection set in the text (entering from before the cell, Find, a restored session): the
        // editor takes the items it covers; the model's caret stays at the text's end
        if let Selection::Caret { anchor, pos, .. } = self.sel {
            if (anchor, pos) != (n, n) {
                let Some(b) = self.boxed.as_mut() else { return };
                let (_, map) = mc_boxes::to_text_mapped(b.ed.root());
                if anchor == pos {
                    let (p, k) = map.caret_at(pos);
                    b.ed.set_caret(&p, k);
                } else if let Some((p, r)) = map.find(anchor.min(pos)..anchor.max(pos)) {
                    b.ed.select(&p, r);
                }
                self.sel = Selection::Caret { cell, anchor: n, pos: n };
            }
        }
    }

    /// Write the editor's text to its cell (an undoable change); the model's caret stays at the text's end.
    fn commit_box(&mut self) -> bool {
        let Some(b) = self.boxed.as_mut() else { return false };
        let (id, text) = (b.cell, b.ed.text());
        let before = b.before.take();
        let after = b.ed.caret().clone();
        let changed = self.cell(id).is_some_and(|c| c.text != text);
        if changed {
            // the carets on either side of the edit, for undo and redo
            if let Some((t, k)) = before {
                self.box_carets.push((id, t, k));
            }
            self.box_carets.push((id, text.clone(), after));
            if self.box_carets.len() > CARETS_KEPT {
                self.box_carets.drain(..self.box_carets.len() - CARETS_KEPT);
            }
            self.set_text(id, text.clone(), true);
        }
        self.sel = Selection::Caret { cell: id, anchor: text.len(), pos: text.len() };
        // the last box went: the cell is plain text again, the caret on the same character
        if self.boxed.as_ref().is_some_and(|b| !mc_boxes::has_boxes(b.ed.root())) {
            self.unbox(id);
        }
        changed
    }

    /// The cell's boxes are gone: its text is plain, the `typeset` mark goes and the caret moves into the text.
    fn unbox(&mut self, id: CellId) {
        let Some(b) = self.boxed.take().filter(|b| b.cell == id) else { return };
        let row = b.ed.root();
        let byte = |k: usize| row.0[..k.min(row.0.len())].iter().map(|i| match i {
            Item::Char(c) => c.len_utf8(),
            _ => '□'.len_utf8(),
        }).sum::<usize>();
        let (pos, anchor) = match (b.ed.caret(), b.ed.selection()) {
            (_, Some((p, r))) if p.is_empty() => (byte(r.end), byte(r.start)),
            (c, _) => {
                let k = byte(c.offset);
                (k, k)
            }
        };
        self.update_cell(id, true, |c| c.props.typeset = false);
        self.sel = Selection::Caret { cell: id, anchor, pos };
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
        // the text read as canonical text keeps every character; only the three boxes are made from it
        let mut ed = Editor::new(mc_boxes::restrict(&mc_boxes::from_text(&c.text)));
        if a == p {
            ed.set_caret(&[], p);
        } else {
            ed.select(&[], a.min(p)..a.max(p));
        }
        let n = c.text.len();
        self.update_cell(cell, true, |c| c.props.typeset = true);
        self.boxed = Some(BoxEdit { cell, ed, before: None });
        // the box key that follows writes the text (a cell left without boxes goes back to plain text then)
        self.sel = Selection::Caret { cell, anchor: n, pos: n };
        true
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
        // a box key at an insertion point makes a code cell there first
        if let (C::Structure(_), Selection::Gap(g)) = (cmd, self.sel.clone()) {
            let style = self.gap_style();
            if style.is_code() {
                let id = self.create_cell(g, style, "");
                self.select(Selection::Caret { cell: id, anchor: 0, pos: 0 });
            }
        }
        // a box key in a plain code cell: its text becomes the editor's (every character kept) and the box goes in
        if matches!(cmd, C::Structure(_)) && self.boxed.is_none() && !self.box_current() {
            fx.beep = true;
            return Some(None);
        }
        let auto_close = self.auto_close;
        let b = self.boxed.as_mut()?;
        b.before = Some((b.ed.text(), b.ed.caret().clone()));
        let id = b.cell;
        let ed = &mut b.ed;
        let now = env.now();
        let mut run = None;
        match cmd {
            C::Insert(t) => {
                if !(auto_close && auto_pair(ed, t)) {
                    ed.insert_text(t);
                }
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
                    Motion::Home | Motion::End => {
                        let end = *m == Motion::End;
                        let caret = ed.caret().clone();
                        // on the root row, the visual line (wrapped content); else the line breaks of the row
                        let edge = if caret.path.is_empty() {
                            let c = self.cell(id).expect("boxed cell").clone();
                            env.box_line_edge(&c, caret.offset, end)
                        } else {
                            None
                        };
                        let ed = &mut self.boxed.as_mut().expect("boxed").ed;
                        match edge {
                            Some(k) if *extend => ed.extend_to(&[], k),
                            Some(k) => ed.set_caret(&[], k),
                            None if end => ed.move_end(*extend),
                            None => ed.move_home(*extend),
                        }
                        true
                    }
                    Motion::Up | Motion::Down => {
                        let up = *m == Motion::Up;
                        // Shift keeps the selection's anchor where it was
                        let start = ed.anchor().cloned().unwrap_or_else(|| ed.caret().clone());
                        let inside = if up { ed.move_up() } else { ed.move_down() };
                        let moved = inside || {
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
                        };
                        if *extend {
                            let ed = &mut self.boxed.as_mut().expect("boxed").ed;
                            // past the first or last line: to the start or the end of the cell
                            let to = if moved {
                                ed.caret().clone()
                            } else {
                                mc_boxes::Caret { path: vec![], offset: if up { 0 } else { ed.root().len() } }
                            };
                            ed.set_caret(&start.path, start.offset);
                            ed.extend_to(&to.path, to.offset);
                            return Some(None);
                        }
                        moved
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
                // the clipboard gets the plain text the boxes stand for (`x^2`, `(a+b)/c`, `sqrt(y)`)
                let text = mc_boxes::linear_text(&mc_boxes::from_text(&t));
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
            C::ReplaceWord(name) => {
                // the completion replaces the name before the caret in its row
                if ed.selection().is_none() {
                    let caret = ed.caret().clone();
                    let start = word_start(ed, &caret);
                    if start < caret.offset {
                        ed.select(&caret.path, start..caret.offset);
                    }
                }
                ed.insert_text(name);
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
                // Shift+click and drags keep the selection's anchor
                b.ed.extend_to(path, offset);
                return;
            }
            b.ed.set_caret(path, offset);
        }
    }

    /// The name being typed before the caret of the typeset cell with the caret (for completion): its row, where
    /// it starts in the row, and its text. `None` with a selection.
    pub fn box_word(&self) -> Option<(RowPath, usize, String)> {
        let b = self.boxed.as_ref()?;
        if b.ed.selection().is_some() {
            return None;
        }
        let caret = b.ed.caret();
        let start = word_start(&b.ed, caret);
        let row = b.ed.root().at(&caret.path)?;
        let word = row.0[start..caret.offset].iter().filter_map(|i| if let Item::Char(c) = i { Some(*c) } else { None }).collect();
        Some((caret.path.clone(), start, word))
    }
}

/// Where the name before `caret` starts in its row (letters, digits, `_` and `.`, as in plain cells).
fn word_start(ed: &Editor, caret: &Caret) -> usize {
    let Some(row) = ed.root().at(&caret.path) else { return caret.offset };
    let mut k = caret.offset.min(row.len());
    while k > 0 && matches!(row.0[k - 1], Item::Char(c) if c.is_alphanumeric() || c == '_' || c == '.') {
        k -= 1;
    }
    k
}

/// Automatic closing in a cell with boxes (as in plain cells, decision NB5.1): an opening bracket typed with no
/// selection and nothing word-like after the caret brings its closer, the caret between; a closer typed right before
/// the same closer steps over it. `true` when it was handled.
fn auto_pair(ed: &mut Editor, t: &str) -> bool {
    let mut it = t.chars();
    let (Some(ch), None) = (it.next(), it.next()) else { return false };
    if ed.selection().is_some() {
        return false;
    }
    let caret = ed.caret().clone();
    let next = ed.root().at(&caret.path).and_then(|r| r.0.get(caret.offset)).cloned();
    let next_char = match next {
        Some(Item::Char(c)) => Some(c),
        _ => None,
    };
    if matches!(ch, ')' | ']' | '}') && next_char == Some(ch) {
        ed.move_right(false);
        return true;
    }
    let close = match ch {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => return false,
    };
    if next_char.is_some_and(|n| n.is_alphanumeric() || n == '_') || matches!(next, Some(Item::Box(_))) {
        return false;
    }
    let mut pair = String::new();
    pair.push(ch);
    pair.push(close);
    ed.insert_text(&pair);
    ed.move_left(false);
    true
}
