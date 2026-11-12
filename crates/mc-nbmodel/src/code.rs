//! Code cells edited as trees (design 11, section 3.3): every Input and Code cell is a box tree ([`Cell::content`]),
//! edited by one [`mc_edit::Editor`] while it has the caret, with or without boxes. Each editing command is one
//! transaction, recorded in the notebook's undo history as an [`crate::notebook::Op::Edit`] (so cell operations and
//! edits interleave in one order); the cell's text is the tree's canonical text, derived after every change; undo
//! and redo put back the exact selection in the tree.
//!
//! The notebook's selection of a code cell stays a byte range of its canonical text, derived from the tree's
//! selection after every command, so Find, completion, help and the key scripts read meaningful positions; when
//! something other than the editor sets it (Find, a click on text, a restored session), the tree's selection is
//! read back from it.
use crate::cell::{Cell, CellId};
use crate::command::{Command, Effects, Motion};
use crate::env::{Clip, Env};
use crate::notebook::{Notebook, Run, Selection};
use crate::style::ReturnKey;
use mc_boxes::{Row, TextMap};
use mc_edit::{Command as E, Context, Editor, Geometry, Logical, Position, Selection as TreeSel, Status};

/// The editor of the code cell with the caret.
#[derive(Clone, Debug)]
pub(crate) struct CodeEdit {
    /// the cell
    pub cell: CellId,
    /// its tree, selection and marks
    pub ed: Editor,
    /// the cell's revision the editor's tree is (a different one: the text changed outside the editor)
    rev: u64,
    /// the notebook's byte selection last derived from the tree's
    bytes: (usize, usize),
}

/// The tree selection a byte selection of the canonical text stands for.
fn tree_selection(map: &TextMap, anchor: usize, pos: usize) -> TreeSel {
    if anchor == pos {
        let (p, k) = map.caret_at(pos);
        return TreeSel::caret(Position::new(&p, k));
    }
    match map.find(anchor.min(pos)..anchor.max(pos)) {
        Some((p, r)) if anchor <= pos => TreeSel::items(&p, r),
        Some((p, r)) => TreeSel { anchor: Position::new(&p, r.end), head: Position::new(&p, r.start) },
        None => {
            let (p, k) = map.caret_at(pos);
            TreeSel::caret(Position::new(&p, k))
        }
    }
}

/// The byte selection of the canonical text a tree selection stands for: the selected items' bytes (the anchor's
/// end first), or the caret's byte.
fn byte_selection(root: &Row, map: &TextMap, sel: &TreeSel) -> (usize, usize) {
    match sel.range(root) {
        Some((p, r)) => {
            let (a, b) = (map.offset(&p, r.start).unwrap_or(0), map.offset(&p, r.end).unwrap_or(0));
            if sel.forward() {
                (a, b)
            } else {
                (b, a)
            }
        }
        None => {
            let o = map.offset(&sel.head.path, sel.head.offset).unwrap_or(0);
            (o, o)
        }
    }
}

/// Whether an editor command can change the tree.
fn edits(c: &E) -> bool {
    !matches!(
        c,
        E::Move(..)
            | E::SetCaret(_)
            | E::ExtendTo(_)
            | E::Select(_)
            | E::SelectWord(_)
            | E::SelectAll
            | E::ExpandSelection
            | E::CheckBalance
    )
}

/// The box flavour of the clipboard as the notebook's boxes: any other box another program wrote becomes its linear
/// text.
fn clip_boxes(canonical: &str) -> String {
    mc_boxes::to_text(&mc_boxes::restrict(&mc_boxes::from_text(canonical)))
}

impl Notebook {
    /// Whether cell `c` is edited as a tree (every Input and Code cell not showing its source).
    pub fn edits_tree(&self, c: &Cell) -> bool {
        c.holds_tree()
    }

    /// The code cell with the caret and its editor (drawing the caret, the selection and the alias mark).
    pub fn code_editor(&self) -> Option<(CellId, &Editor)> {
        self.code.as_ref().map(|e| (e.cell, &e.ed))
    }

    /// Keeps the editor in step with the selection: one for the code cell with the caret, made again when the cell
    /// changed outside it; a byte selection set from outside is read into the tree.
    pub(crate) fn sync_code(&mut self) {
        let Selection::Caret { cell, anchor, pos } = self.sel else {
            self.code = None;
            return;
        };
        let Some(c) = self.cell(cell).filter(|c| self.edits_tree(c)) else {
            self.code = None;
            return;
        };
        let rev = c.rev;
        let fresh = self.code.as_ref().is_none_or(|e| e.cell != cell || e.rev != rev);
        if fresh {
            let root = c.content.as_deref().cloned().unwrap_or_else(|| c.read_tree());
            self.code = Some(CodeEdit { cell, ed: Editor::new(root), rev, bytes: (usize::MAX, usize::MAX) });
        }
        let restored = self.tree_sel.take().filter(|(id, _)| *id == cell).map(|(_, s)| s);
        let Some(e) = self.code.as_mut() else { return };
        if let Some(s) = restored {
            // undo and redo: the exact selection
            e.ed.set_selection(s);
            let (_, map) = mc_boxes::to_text_mapped(e.ed.root());
            e.bytes = byte_selection(e.ed.root(), &map, e.ed.selection());
            self.sel = Selection::Caret { cell, anchor: e.bytes.0, pos: e.bytes.1 };
        } else if e.bytes != (anchor, pos) {
            let (_, map) = mc_boxes::to_text_mapped(e.ed.root());
            e.ed.set_selection(tree_selection(&map, anchor, pos));
            e.bytes = (anchor, pos);
        }
    }

    /// Runs an editor command in the code cell with the caret: the change goes into the cell (one undo step with the
    /// command) and the notebook's byte selection follows the tree's.
    fn run_code(&mut self, cmd: &E, env: &mut dyn Env) -> mc_edit::Outcome {
        let Some(mut e) = self.code.take() else {
            return mc_edit::Outcome { tx: None, status: Status::Beep };
        };
        let id = e.cell;
        let layout = self.cell(id).cloned().and_then(|c| env.layout(&c));
        let cx = Context { geometry: layout.as_deref().map(|l| l as &dyn Geometry), auto_close: self.auto_close };
        let out = e.ed.apply(cmd, &cx);
        if let Some(tx) = &out.tx {
            self.set_tree(id, e.ed.root(), tx.clone());
            e.rev = self.cell(id).map_or(0, |c| c.rev);
        }
        let (_, map) = mc_boxes::to_text_mapped(e.ed.root());
        e.bytes = byte_selection(e.ed.root(), &map, e.ed.selection());
        self.sel = Selection::Caret { cell: id, anchor: e.bytes.0, pos: e.bytes.1 };
        self.code = Some(e);
        out
    }

    /// The x of the caret in the code cell's layout (or its column without one), for Up and Down across cells.
    fn code_caret_x(&mut self, env: &mut dyn Env) -> Option<f32> {
        let e = self.code.as_ref()?;
        let c = self.cell(e.cell)?.clone();
        let layout = env.layout(&c);
        let root = e.ed.root().clone();
        let caret = e.ed.caret().clone();
        let logical = Logical(&root);
        let g: &dyn Geometry = match &layout {
            Some(l) => &**l,
            None => &logical,
        };
        g.locate(&caret).map(|(x, _)| x)
    }

    /// Up or Down from the insertion point `g` into code cell `j`: its first or last visual line at the goal x.
    fn enter_code_cell(&mut self, j: usize, up: bool, env: &mut dyn Env) {
        let c = self.cells[j].clone();
        let id = c.id;
        let root = c.content.as_deref().cloned().unwrap_or_else(|| c.read_tree());
        let layout = env.layout(&c);
        let logical = Logical(&root);
        let g: &dyn Geometry = match &layout {
            Some(l) => &**l,
            None => &logical,
        };
        let line = if up { g.line_count().saturating_sub(1) } else { 0 };
        let p = g.nearest(line, self.goal_x.unwrap_or(0.0)).unwrap_or_else(|| Position::root(if up { root.len() } else { 0 }));
        let (_, map) = mc_boxes::to_text_mapped(&root);
        let b = map.offset(&p.path, p.offset).unwrap_or(0);
        self.select(Selection::Caret { cell: id, anchor: b, pos: b });
        self.sync_code();
        if let Some(e) = self.code.as_mut() {
            e.ed.set_selection(TreeSel::caret(p));
        }
    }

    /// Run `cmd` in the code cell with the caret, if there is one and the command is an editing command (design 11).
    /// `None`: not handled here (the generic code runs); `Some(run)`: handled.
    pub(crate) fn code_dispatch(&mut self, cmd: &Command, env: &mut dyn Env, fx: &mut Effects) -> Option<Option<Run>> {
        use Command as C;
        match (cmd, self.sel.clone()) {
            // typing, a box key or Esc at an insertion point makes a code cell there first; the editor then types
            // (so an opener's automatic closer is the editor's)
            (C::Structure(_) | C::Alias | C::Insert(_), Selection::Gap(g)) => {
                if matches!(cmd, C::Insert(t) if t.is_empty()) {
                    return None;
                }
                let style = self.gap_style();
                if !style.is_code() {
                    if matches!(cmd, C::Structure(_)) {
                        fx.beep = true;
                        return Some(None);
                    }
                    return None;
                }
                self.create_cell(g, style, "");
            }
            // Up and Down from an insertion point into a code cell keep the x
            (C::Move(m @ (Motion::Up | Motion::Down), false), Selection::Gap(g)) => {
                let up = *m == Motion::Up;
                let j = if up {
                    (0..g.min(self.cells.len())).rev().find(|&j| self.is_visible(j))
                } else {
                    (g..self.cells.len()).find(|&j| self.is_visible(j))
                };
                let enter = j.filter(|&j| self.edits_tree(&self.cells[j]) && !self.cells[j].props.hidden)?;
                self.enter_code_cell(enter, up, env);
                return Some(None);
            }
            // a click in the boxes of a code cell without the caret gives it the caret first
            (C::ClickBox(id, ..), _) if self.code.as_ref().is_none_or(|e| e.cell != *id) => {
                let c = self.cell(*id).filter(|c| self.edits_tree(c))?;
                let n = c.text.len();
                self.select(Selection::Caret { cell: *id, anchor: n, pos: n });
            }
            _ => {}
        }
        self.sync_code();
        let id = self.code.as_ref()?.cell;
        let c = self.cell(id)?.clone();
        let now = env.now();
        let deleting = Some(Run::Deleting(id, now));
        let (ecmd, run): (E, Option<Run>) = match cmd {
            C::Insert(t) if t.is_empty() => return Some(None),
            C::Insert(t) => (E::Type(t.clone()), Some(Run::Typing(id, t.ends_with(char::is_whitespace), now))),
            C::Enter => {
                if c.style.info().ret == ReturnKey::NewCell {
                    return None;
                }
                (E::Newline, Some(Run::Typing(id, true, now)))
            }
            C::Tab => (E::Tab, None),
            C::Outdent => (E::Outdent, None),
            C::ToggleComment => (E::ToggleComment, None),
            C::Alias => (E::Alias, None),
            C::ExtendSelection => (E::ExpandSelection, None),
            C::CheckBalance => (E::CheckBalance, None),
            C::Backspace => (E::Backspace, deleting),
            C::Delete => (E::Delete, deleting),
            C::DeleteWordBack => (E::DeleteWordBack, deleting),
            C::DeleteWordForward => (E::DeleteWordForward, deleting),
            C::Move(m, extend) => {
                let m = match m {
                    Motion::Left => mc_edit::Motion::Left,
                    Motion::Right => mc_edit::Motion::Right,
                    Motion::WordLeft => mc_edit::Motion::WordLeft,
                    Motion::WordRight => mc_edit::Motion::WordRight,
                    Motion::Up => mc_edit::Motion::Up,
                    Motion::Down => mc_edit::Motion::Down,
                    Motion::Home => mc_edit::Motion::Home,
                    Motion::End => mc_edit::Motion::End,
                    Motion::DocStart | Motion::DocEnd => return None,
                };
                (E::Move(m, *extend), None)
            }
            C::SelectAll => (E::SelectAll, None),
            C::Copy | C::CopyPlain | C::Cut => {
                let items = self.code.as_ref().map(|e| e.ed.selected()).unwrap_or_default();
                if items.is_empty() {
                    fx.beep = true;
                    return Some(None);
                }
                // the plain text the boxes stand for for other programs, and the boxes themselves for pasting here
                env.set_clipboard(Clip::of_boxes(&items, *cmd == C::CopyPlain));
                if *cmd != C::Cut {
                    return Some(None);
                }
                (E::Delete, None)
            }
            C::ReplaceWord(name) => (E::ReplaceWord(name.clone()), None),
            C::Paste => {
                // boxes from the box flavour; any text (from another program, a cells fragment's text) stays text
                let clip = self.paste_clip(env);
                match (clip.boxes, clip.text) {
                    (Some(bx), _) => (E::Paste(clip_boxes(&bx)), None),
                    (None, Some(t)) if !t.is_empty() => {
                        (E::Insert(crate::text::untab(&crate::text::unix_lines(&t), crate::edit::INDENT)), None)
                    }
                    _ => return Some(None),
                }
            }
            C::Structure(s) => (E::Structure(*s), None),
            C::ExitStructure => (E::ExitStructure, None),
            C::ClickBox(_, path, offset, extend) => {
                let p = Position::new(path, *offset);
                (if *extend { E::ExtendTo(p) } else { E::SetCaret(p) }, None)
            }
            C::InputFromAbove | C::OutputFromAbove => {
                let output = *cmd == C::OutputFromAbove;
                let i = self.index_of(id)?;
                let wanted = |c: &Cell| if output { c.style == crate::Style::Output } else { c.style.is_code() };
                let Some(src) = self.cells[..i].iter().rev().find(|c| wanted(c)).cloned() else {
                    fx.beep = true;
                    return Some(None);
                };
                if !output && src.props.typeset {
                    (E::Paste(src.text.clone()), None)
                } else {
                    (E::Insert(src.text.clone()), None)
                }
            }
            _ => return None,
        };
        if edits(&ecmd) && !c.editable() {
            fx.beep = true;
            return Some(None);
        }
        let x = matches!(cmd, C::Move(Motion::Up | Motion::Down, false)).then(|| self.code_caret_x(env)).flatten();
        let out = self.run_code(&ecmd, env);
        match out.status {
            Status::Done => {}
            Status::Beep => fx.beep = true,
            Status::Before | Status::After => {
                if !matches!(cmd, C::Move(_, true)) {
                    // past the content: the insertion point before or after the cell
                    let i = self.index_of(id)?;
                    let g = if out.status == Status::Before { i } else { self.gap_after_code(i) };
                    if matches!(cmd, C::Move(Motion::Up | Motion::Down, _)) {
                        self.goal_x = x.or(self.goal_x);
                    }
                    self.code = None;
                    self.select(Selection::Gap(g));
                }
            }
            Status::Unhandled => match cmd {
                C::ExtendSelection => {
                    self.code = None;
                    self.select(Selection::Cells { ids: vec![id], anchor: id });
                }
                C::SelectAll => {
                    let ids: Vec<CellId> = self.cells.iter().map(|c| c.id).collect();
                    if let Some(&first) = ids.first() {
                        self.code = None;
                        self.select(Selection::Cells { ids, anchor: first });
                    }
                }
                _ => {}
            },
        }
        // typing and deleting merge into one undo step while they go on; anything else (a box gone, a paste) is a
        // step of its own
        let _ = run;
        Some(out.tx.and_then(|tx| match tx.kind {
            mc_edit::Kind::Typing { space } => Some(Run::Typing(id, space, now)),
            mc_edit::Kind::Deleting => Some(Run::Deleting(id, now)),
            mc_edit::Kind::Other => None,
        }))
    }

    /// Pastes canonical box text at the caret of the code cell with the caret (design 11), replacing its selection.
    /// `None` when the tree editor is not in use; `Some(false)` when the boxes would nest too deep.
    pub(crate) fn paste_tree(&mut self, canonical: &str, env: &mut dyn Env) -> Option<bool> {
        self.sync_code();
        self.code.as_ref()?;
        let out = self.run_code(&E::Paste(clip_boxes(canonical)), env);
        Some(out.status == Status::Done)
    }

    /// The insertion point after cell `i`, past cells hidden in closed groups.
    fn gap_after_code(&self, i: usize) -> usize {
        let mut j = i + 1;
        while j < self.cells.len() && !self.is_visible(j) {
            j += 1;
        }
        j
    }
}

/// What the window reads of an editor: the tree, the selection, the caret and the alias mark of a code cell's
/// editor or of the read-only box caret of a typeset output.
#[derive(Clone, Copy, Debug)]
pub struct EditorRef<'a>(pub &'a Editor);

impl EditorRef<'_> {
    /// The tree.
    pub fn root(&self) -> &Row {
        self.0.root()
    }

    /// The selected row and items.
    pub fn selection(&self) -> Option<(mc_boxes::RowPath, std::ops::Range<usize>)> {
        self.0.range()
    }

    /// The caret.
    pub fn caret(&self) -> Position {
        self.0.caret().clone()
    }

    /// The pending or unknown alias, for drawing.
    pub fn alias_mark(&self) -> Option<mc_edit::AliasMark> {
        self.0.alias_mark()
    }

    /// The notation with the caret or the selection marked (key scripts, self-tests).
    pub fn notation(&self) -> String {
        self.0.notation()
    }

    /// The linear text of the tree (what Copy gives).
    pub fn linear(&self) -> String {
        mc_boxes::linear_text(self.root())
    }
}

impl Notebook {
    /// The editor of the code cell with the caret.
    pub fn input_editor(&self) -> Option<(CellId, EditorRef<'_>)> {
        self.code_editor().map(|(id, e)| (id, EditorRef(e)))
    }

    /// The read-only box caret of the typeset output with the caret.
    pub fn output_caret(&self) -> Option<(CellId, EditorRef<'_>)> {
        self.output_editor().map(|(id, e)| (id, EditorRef(e)))
    }

    /// The editor of the cell with the caret, input or output.
    pub fn any_editor(&self) -> Option<(CellId, EditorRef<'_>)> {
        self.input_editor().or(self.output_caret())
    }

    /// The name being typed before the caret of the code cell with the caret: its row, where it starts in the row,
    /// and its text (completion). `None` with a selection.
    pub fn name_before_caret(&self) -> Option<(mc_boxes::RowPath, usize, String)> {
        self.code_editor()?.1.word_before_caret()
    }
}
