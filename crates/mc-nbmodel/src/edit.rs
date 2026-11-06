//! Applying commands (design 3): typing, motion, selection, clipboard, cell operations, styles, grouping and
//! evaluation requests. Every change goes through the primitives of [`crate::notebook`], so undo, the selection
//! and the groups stay consistent.
use crate::cell::{Cell, CellId, OutputForm};
use crate::command::{Bracket, Command, Effects, Motion};
use crate::env::{Clip, Env};
use crate::notebook::{Notebook, Run, Selection};
use crate::style::{FontRole, ReturnKey, Style};
use crate::text;

/// The indentation Tab inserts (decision NB-core.5: spaces, so files and the painter agree on widths).
pub const INDENT: &str = "    ";

/// The alias start marker typed by Esc (design 3.9).
pub const ALIAS_MARK: char = '⋮';

impl Notebook {
    /// Apply a command. The window repaints after every command; [`Effects`] says what else to do.
    pub fn apply(&mut self, cmd: Command, env: &mut dyn Env) -> Effects {
        let mut fx = Effects { changed: true, ..Effects::default() };
        let before = (self.sel.clone(), self.structure_rev());
        if !matches!(cmd, Command::Move(Motion::Up | Motion::Down, _)) {
            self.goal_x = None;
        }
        let modified = self.is_modified();
        if !matches!(cmd, Command::Insert(_) | Command::Backspace) {
            self.closers.clear();
        }
        self.sync_boxed();
        match cmd {
            Command::Undo => fx.beep = !self.undo(),
            Command::Redo => fx.beep = !self.redo(),
            cmd => {
                self.begin_unit();
                let run = self.dispatch(cmd, env, &mut fx);
                self.end_unit(run);
            }
        }
        self.sync_boxed();
        fx.modified = self.is_modified() && (!modified || self.structure_rev() != before.1 || fx.modified);
        fx.scroll = self.sel != before.0;
        fx
    }

    fn dispatch(&mut self, cmd: Command, env: &mut dyn Env, fx: &mut Effects) -> Option<Run> {
        use Command as C;
        if let Some(run) = self.box_dispatch(&cmd, env, fx) {
            return run;
        }
        match cmd {
            C::Insert(t) => return self.type_text(&t, env, fx),
            C::Enter => return self.enter(env, fx),
            C::Tab => self.tab(env, fx),
            C::Outdent => self.outdent(env, fx),
            C::ToggleComment => fx.beep = !self.toggle_comment(),
            C::Alias => return self.alias(env, fx),
            C::ExtendSelection => self.extend_selection(),
            C::CheckBalance => fx.beep = !self.check_balance(),
            C::Backspace | C::Delete | C::DeleteWordBack | C::DeleteWordForward => return self.delete(&cmd, env, fx),
            C::Move(m, extend) => self.motion(m, extend, env),
            C::SelectAll => self.select_all(),
            C::Undo | C::Redo => unreachable!("handled by apply"),
            C::Cut => {
                if self.copy(env, false) {
                    self.delete(&C::Delete, env, fx);
                }
            }
            C::Copy => fx.beep = !self.copy(env, false),
            C::CopyPlain => fx.beep = !self.copy(env, true),
            C::Paste => self.paste(env, fx),
            C::Evaluate => self.evaluate(false, fx),
            C::EvaluateNext => self.evaluate(true, fx),
            C::EvaluateNotebook => {
                let ids = self.cells.iter().filter(|c| c.evaluatable()).map(|c| c.id).collect();
                self.enqueue(ids, fx);
            }
            C::EvaluateInitialization => {
                let ids = self.initialization_cells();
                self.push_queue(ids);
            }
            C::InitAnswer(a) => self.answer_init(a),
            C::Abort => self.abort(),
            C::Unqueue => {
                let ids = self.selected_cells();
                self.unqueue(&ids);
            }
            C::QuitKernel => self.quit_kernel(),
            C::SetStyle(s) => self.set_style(s),
            C::Divide => fx.beep = !self.divide(),
            C::Merge => fx.beep = !self.merge(),
            C::GroupTogether => self.mark_together(true),
            C::Ungroup => self.mark_together(false),
            C::SetManualGrouping(on) => {
                if on {
                    self.header.set("grouping", Some("manual".into()));
                } else {
                    self.header.remove("grouping");
                }
                self.structure_changed();
            }
            C::ToggleGroup => fx.beep = !self.toggle_group(),
            C::OpenAll => self.open_close_all(false),
            C::CloseAll => self.open_close_all(true),
            C::ToggleInitialization => {
                self.toggle_prop(|c| c.props.initialization = Some(!c.initialization()));
            }
            C::ToggleEditable => self.toggle_prop(|c| c.props.editable = Some(!c.editable())),
            C::ToggleEvaluatable => self.toggle_prop(|c| c.props.evaluatable = Some(!c.evaluatable())),
            C::ToggleDeletable => self.toggle_prop(|c| c.props.deletable = Some(!c.deletable())),
            C::ShowExpression => fx.beep = !self.show_expression(),
            C::InputFromAbove => fx.beep = !self.copy_from_above(false),
            C::OutputFromAbove => fx.beep = !self.copy_from_above(true),
            C::SameStyleBelow => fx.beep = !self.same_style_below(),
            C::DeleteAllOutput => {
                let mut i = 0;
                while i < self.cells.len() {
                    if self.cells[i].generated() {
                        let mut j = i;
                        while j < self.cells.len() && self.cells[j].generated() {
                            j += 1;
                        }
                        self.remove_cells(i..j, true);
                    } else {
                        i += 1;
                    }
                }
            }
            C::NewCell(style) => {
                let at = match self.sel {
                    Selection::Gap(g) => g,
                    _ => match self.current_cell().and_then(|id| self.index_of(id)) {
                        Some(i) => self.outputs_end(i),
                        None => self.cells.len(),
                    },
                };
                self.create_cell(at, style, "");
            }
            C::ClickText(id, pos, extend) => self.click_text(id, pos, extend),
            C::SelectWord(id, pos) => {
                if let Some(c) = self.cell(id) {
                    // in an output shown as one unit a double-click selects all of it
                    let (a, b) =
                        if env.atomic(c) { (0, c.text.len()) } else { text::word_at(&c.text, pos.min(c.text.len())) };
                    self.select(Selection::Caret { cell: id, anchor: a, pos: b });
                }
            }
            C::SelectLine(id, pos) => {
                if let Some(c) = self.cell(id) {
                    let pos = pos.min(c.text.len());
                    let (a, b) = if env.atomic(c) {
                        (0, c.text.len())
                    } else {
                        (text::line_start(&c.text, pos), text::line_end(&c.text, pos))
                    };
                    self.select(Selection::Caret { cell: id, anchor: a, pos: b });
                }
            }
            C::ClickGap(g) => {
                self.gap_style = None;
                self.select(Selection::Gap(g));
            }
            C::ClickBracket(b, shift, ctrl) => self.click_bracket(b, shift, ctrl),
            C::DoubleClickBracket(b) => {
                if let Bracket::Group(s, e) = b {
                    if let Some(g) = self.groups.list.iter().position(|g| g.start == s && g.end == e) {
                        self.toggle_group_at(g, 0);
                    }
                }
            }
            C::MoveCells(to, copy) => fx.beep = !self.move_cells(to, copy),
            C::ResizeGraphic(id, size) => fx.beep = !self.resize_graphic(id, size),
            // without a typeset cell at the caret (handled before): nothing to act on
            C::Structure(_) | C::ExitStructure | C::SwitchScript => fx.beep = true,
            C::ToTypeset => {
                let outputs = self.set_output_form(OutputForm::Typeset);
                fx.beep = !(self.inputs_to_typeset() | outputs);
            }
            C::ToLinear => {
                let outputs = self.set_output_form(OutputForm::Linear);
                fx.beep = !(self.inputs_to_linear() | outputs);
            }
            C::OutputForm(f) => fx.beep = !self.set_output_form(f),
            C::ClickBox(id, path, offset, extend) => self.click_box(id, &path, offset, extend),
            C::Nop => {}
            C::App(a) => fx.app = Some(a),
        }
        None
    }

    /// Set or clear the `size` of a graphic's output (a resize ends, or a double-click restores the natural size).
    fn resize_graphic(&mut self, id: CellId, size: Option<(u32, u32)>) -> bool {
        if self.cell(id).and_then(|c| c.graphic()).is_none() {
            return false;
        }
        let value = size.map(|(w, h)| mc_nbfile::images::size_value(w.max(1), h.max(1)));
        self.update_cell(id, true, |c| match value {
            Some(v) => c.out_extra.set("size", Some(v)),
            None => c.out_extra.remove("size"),
        });
        true
    }

    /// Show the selected Output cells (with a caret: its cell) in `form` (NB5.5). `false` when no output is
    /// selected.
    fn set_output_form(&mut self, form: OutputForm) -> bool {
        let selected = self.selected_cells();
        let targets: Vec<CellId> = self
            .cells
            .iter()
            .filter(|c| c.style == Style::Output && c.graphic().is_none() && selected.contains(&c.id))
            .map(|c| c.id)
            .collect();
        for &id in &targets {
            self.update_cell(id, true, |c| c.out_extra.set("form", Some(form.name().to_string())));
        }
        !targets.is_empty()
    }

    // ----- typing -----

    /// A new cell of `style` holding `text` at `at`, the caret at its end; groups closed around it open.
    fn create_cell(&mut self, at: usize, style: Style, content: &str) -> CellId {
        let c = self.new_cell(style, content);
        let id = c.id;
        self.insert_cells(at, vec![c], true);
        let i = self.index_of(id).expect("inserted");
        let heads: Vec<usize> = self.groups.containing(i).iter().map(|&g| self.groups.list[g].start).collect();
        if heads.iter().any(|&h| self.cells[h].closed.is_some()) {
            for h in heads {
                let hid = self.cells[h].id;
                self.update_cell(hid, true, |c| c.closed = None);
            }
        }
        self.gap_style = None;
        self.select(Selection::Caret { cell: id, anchor: content.len(), pos: content.len() });
        id
    }

    /// The cell and range an edit applies to: the caret's cell, or for an Output cell a new Input copy after the
    /// output's group (design 2.8: "edit duplicates"); `None` for cells that cannot be edited.
    fn edit_target(&mut self) -> Option<(CellId, usize, usize)> {
        let Selection::Caret { cell, anchor, pos } = self.sel else { return None };
        let i = self.index_of(cell)?;
        let c = &self.cells[i];
        if !c.editable() {
            return None;
        }
        let (a, b) = (anchor.min(pos), anchor.max(pos));
        if c.style.info().ret != ReturnKey::Duplicate || c.expr_view {
            return Some((cell, a, b));
        }
        let mut end = i + 1;
        while end < self.cells.len() && self.cells[end].generated() {
            end += 1;
        }
        let mut copy = self.new_cell(Style::Input, &c.text.clone());
        copy.props.typeset = self.cells[i].props.typeset;
        let id = copy.id;
        self.insert_cells(end, vec![copy], true);
        self.select(Selection::Caret { cell: id, anchor: a, pos: b });
        Some((id, a, b))
    }

    /// Replace `a..b` of a cell's text by `s`; the caret goes after it.
    fn replace_range(&mut self, id: CellId, a: usize, b: usize, s: &str) {
        let Some(c) = self.cell(id) else { return };
        let mut t = c.text.clone();
        t.replace_range(a..b, s);
        self.set_text(id, t, true);
        self.select(Selection::Caret { cell: id, anchor: a + s.len(), pos: a + s.len() });
    }

    fn type_text(&mut self, t: &str, env: &mut dyn Env, fx: &mut Effects) -> Option<Run> {
        if t.is_empty() {
            return None;
        }
        match self.sel.clone() {
            Selection::Gap(g) => {
                let style = self.gap_style();
                let t = if style.info().font == FontRole::Prose { smart_quotes("", t) } else { t.to_string() };
                // an opener typed at an insertion point makes a cell holding the pair (NB5.1)
                let pair = match t.as_str() {
                    "(" => Some("()"),
                    "[" => Some("[]"),
                    "{" => Some("{}"),
                    "\"" if style.is_code() => Some("\"\""),
                    _ => None,
                };
                if let Some(pair) = pair.filter(|_| self.auto_close) {
                    let id = self.create_cell(g, style, pair);
                    self.closers.push((id, 1));
                    self.select(Selection::Caret { cell: id, anchor: 1, pos: 1 });
                    return Some(Run::Typing(id, false, env.now()));
                }
                let id = self.create_cell(g, style, &t);
                Some(Run::Typing(id, t.ends_with(char::is_whitespace), env.now()))
            }
            Selection::Cells { .. } => None,
            Selection::Caret { .. } => {
                let Some((id, a, b)) = self.edit_target() else {
                    fx.beep = true;
                    return None;
                };
                if self.auto_close {
                    if let Some(space) = self.auto_close_typed(id, a, b, t) {
                        return Some(Run::Typing(id, space, env.now()));
                    }
                }
                let c = self.cell(id).expect("target exists");
                let t = if c.style.info().font == FontRole::Prose && !c.expr_view {
                    smart_quotes(&c.text[..a], t)
                } else {
                    t.to_string()
                };
                self.shift_closers(id, a, b, t.len());
                self.replace_range(id, a, b, &t);
                Some(Run::Typing(id, t.ends_with(char::is_whitespace), env.now()))
            }
        }
    }

    /// Automatic closing (decision NB5.1): typing `t` over `a..b` of cell `id` when it is an opening bracket, a
    /// quote, or a closer that was inserted automatically. `Some(ends with space)` when it was handled here.
    ///
    /// - `(`, `[`, `{` insert their closer, the caret between, except inside strings and comments of code and
    ///   right before a letter, digit or `_` (typing a bracket before existing text is usually meant to wrap it);
    /// - `"` (code cells only; prose has smart quotes) pairs outside strings and comments when the next character
    ///   is white space, a closer, `,`, `;` or the end;
    /// - with a selection an opener (or `"` in code) wraps it and the selection stays on the wrapped text;
    /// - a typed closer right before a closer inserted automatically steps over it.
    fn auto_close_typed(&mut self, id: CellId, a: usize, b: usize, t: &str) -> Option<bool> {
        let mut it = t.chars();
        let (Some(ch), None) = (it.next(), it.next()) else { return None };
        let c = self.cell(id)?;
        let code = c.style.is_code() && !c.expr_view;
        let s = c.text.clone();
        let closer = match ch {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' if code => Some('"'),
            _ => None,
        };
        // step over an automatic closer
        if a == b && s[a..].starts_with(ch) && self.closers.contains(&(id, a)) {
            self.closers.retain(|&k| k != (id, a));
            self.select(Selection::Caret { cell: id, anchor: a + 1, pos: a + 1 });
            return Some(false);
        }
        let close = closer?;
        if a != b {
            let mut w = String::with_capacity(b - a + 2);
            w.push(ch);
            w.push_str(&s[a..b]);
            w.push(close);
            self.shift_closers(id, a, b, w.len());
            self.replace_range(id, a, b, &w);
            self.select(Selection::Caret { cell: id, anchor: a + 1, pos: b + 1 });
            return Some(false);
        }
        let place = if code { text::lexical_at(&s, a) } else { text::Lexical::Code };
        let next = s[a..].chars().next();
        let pairs = place == text::Lexical::Code
            && match ch {
                '"' => next.is_none_or(|n| n.is_whitespace() || ")]},;".contains(n)),
                _ => next.is_none_or(|n| !(n.is_alphanumeric() || n == '_')),
            };
        if !pairs {
            return None;
        }
        self.shift_closers(id, a, b, 2);
        let mut pair = String::new();
        pair.push(ch);
        pair.push(close);
        self.replace_range(id, a, b, &pair);
        self.closers.push((id, a + 1));
        self.select(Selection::Caret { cell: id, anchor: a + 1, pos: a + 1 });
        Some(false)
    }

    /// Keep the automatic closers of cell `id` in place when `a..b` is replaced by `len` bytes; closers inside the
    /// replaced range are forgotten.
    fn shift_closers(&mut self, id: CellId, a: usize, b: usize, len: usize) {
        self.closers.retain(|&(c, p)| c != id || p < a || p >= b);
        for (c, p) in &mut self.closers {
            if *c == id && *p >= b {
                *p = *p + len - (b - a);
            }
        }
    }

    fn enter(&mut self, env: &mut dyn Env, fx: &mut Effects) -> Option<Run> {
        let Selection::Caret { cell, .. } = self.sel else { return None };
        let c = self.cell(cell)?;
        match c.style.info().ret {
            ReturnKey::NewCell if !c.expr_view => {
                let Some((id, a, b)) = self.edit_target() else {
                    fx.beep = true;
                    return None;
                };
                let c = self.cell(id).expect("target");
                let (style, head, tail) = (c.style, c.text[..a].to_string(), c.text[b..].to_string());
                self.set_text(id, head, true);
                let i = self.index_of(id).expect("target");
                let new = self.create_cell(i + 1, style, &tail);
                self.select(Selection::Caret { cell: new, anchor: 0, pos: 0 });
                None
            }
            _ => self.type_text("\n", env, fx),
        }
    }

    fn selected_lines(&self) -> Option<(CellId, Vec<usize>)> {
        let Selection::Caret { cell, anchor, pos } = self.sel else { return None };
        let c = self.cell(cell)?;
        let (a, b) = (anchor.min(pos), anchor.max(pos));
        let starts = text::lines(&c.text)
            .into_iter()
            .filter(|r| r.start == text::line_start(&c.text, a) || (r.start > a && r.start < b.max(a + 1)))
            .map(|r| r.start)
            .collect();
        Some((cell, starts))
    }

    fn tab(&mut self, env: &mut dyn Env, fx: &mut Effects) {
        let Selection::Caret { cell, anchor, pos } = self.sel else { return };
        let multi = self.cell(cell).is_some_and(|c| c.text[anchor.min(pos)..anchor.max(pos)].contains('\n'));
        if !multi {
            self.type_text(INDENT, env, fx);
            return;
        }
        let Some((id, starts)) = self.selected_lines() else { return };
        if !self.cell(id).is_some_and(|c| c.editable() && c.style.info().ret != ReturnKey::Duplicate) {
            fx.beep = true;
            return;
        }
        let mut t = self.cell(id).expect("selected").text.clone();
        for &s in starts.iter().rev() {
            t.insert_str(s, INDENT);
        }
        let added = INDENT.len() * starts.len();
        self.set_text(id, t, true);
        let (a, b) = (anchor.min(pos), anchor.max(pos));
        self.select(Selection::Caret { cell: id, anchor: a + INDENT.len(), pos: b + added });
    }

    fn outdent(&mut self, _env: &mut dyn Env, fx: &mut Effects) {
        let Some((id, starts)) = self.selected_lines() else { return };
        let Selection::Caret { anchor, pos, .. } = self.sel else { return };
        if !self.cell(id).is_some_and(|c| c.editable() && c.style.info().ret != ReturnKey::Duplicate) {
            fx.beep = true;
            return;
        }
        let mut t = self.cell(id).expect("selected").text.clone();
        let (mut a, mut b) = (anchor.min(pos), anchor.max(pos));
        for &s in starts.iter().rev() {
            let n = t[s..].chars().take(INDENT.len()).take_while(|&c| c == ' ').count();
            t.replace_range(s..s + n, "");
            let shift = |p: usize| if p > s { p - n.min(p - s) } else { p };
            (a, b) = (shift(a), shift(b));
        }
        self.set_text(id, t, true);
        self.select(Selection::Caret { cell: id, anchor: a, pos: b });
    }

    /// Esc (design 3.9): start an alias with the marker [`ALIAS_MARK`], or replace the marker and the name typed
    /// after it by the alias's characters. An unknown name stays marked (guess 11); templates need typeset input
    /// and stay marked too.
    fn alias(&mut self, env: &mut dyn Env, fx: &mut Effects) -> Option<Run> {
        if let Selection::Gap(_) = self.sel {
            // Esc at an insertion point starts a cell with the alias mark
            return self.type_text(&ALIAS_MARK.to_string(), env, fx);
        }
        let Selection::Caret { cell, anchor, pos } = self.sel else { return None };
        let c = self.cell(cell)?;
        let line_start = text::line_start(&c.text, pos);
        let open = c.text[line_start..pos].rfind(ALIAS_MARK).map(|k| line_start + k);
        match open {
            Some(m) if anchor == pos => {
                let name = c.text[m + ALIAS_MARK.len_utf8()..pos].to_string();
                match mc_boxes::aliases::lookup(&name) {
                    Some(mc_boxes::aliases::Alias::Template(t)) => {
                        if self.template_in_plain(m, pos, t) {
                            Some(Run::Typing(cell, false, env.now()))
                        } else {
                            fx.beep = true;
                            None
                        }
                    }
                    Some(mc_boxes::aliases::Alias::Text(r)) => {
                        self.sel = Selection::Caret { cell, anchor: m, pos };
                        let Some((id, a, b)) = self.edit_target() else {
                            fx.beep = true;
                            return None;
                        };
                        self.replace_range(id, a, b, r);
                        Some(Run::Typing(id, false, env.now()))
                    }
                    _ => {
                        fx.beep = true;
                        None
                    }
                }
            }
            // Esc with a text selection leaves it alone
            _ if anchor != pos => None,
            _ => self.type_text(&ALIAS_MARK.to_string(), env, fx),
        }
    }

    /// Ctrl+. (design 3.4): the next syntactic unit of input cells (mc-syntax's grammar), the whole content of
    /// other cells, then the cell itself; further presses keep the cell selected.
    fn extend_selection(&mut self) {
        let Selection::Caret { cell, anchor, pos } = self.sel else { return };
        let Some(c) = self.cell(cell) else { return };
        let (a, b) = (anchor.min(pos), anchor.max(pos));
        let next = if c.style.is_code() && !c.expr_view {
            mc_syntax::extend_selection(&c.text, a, b)
        } else {
            (!(a == 0 && b == c.text.len())).then_some(0..c.text.len())
        };
        match next {
            Some(r) => self.select(Selection::Caret { cell, anchor: r.start, pos: r.end }),
            None => self.select(Selection::Cells { ids: vec![cell], anchor: cell }),
        }
    }

    /// Ctrl+Shift+B: the smallest bracketed expression strictly around the selection; again: the next one out.
    fn check_balance(&mut self) -> bool {
        let Selection::Caret { cell, anchor, pos } = self.sel else { return false };
        let Some(c) = self.cell(cell) else { return false };
        let colouring = mc_syntax::colour(&c.text, None);
        match colouring.enclosing_brackets(anchor.min(pos), anchor.max(pos)) {
            Some(r) => {
                self.select(Selection::Caret { cell, anchor: r.start, pos: r.end });
                true
            }
            None => false,
        }
    }

    /// Comment the selected lines with `# `, or uncomment them when every non-blank one is a comment.
    fn toggle_comment(&mut self) -> bool {
        let Some((id, starts)) = self.selected_lines() else { return false };
        let Some(c) = self.cell(id) else { return false };
        if !c.style.is_code() || !c.editable() {
            return false;
        }
        let Selection::Caret { anchor, pos, .. } = self.sel else { return false };
        let mut t = c.text.clone();
        let line_of = |t: &str, s: usize| t[s..text::line_end(t, s)].to_string();
        let commented = starts
            .iter()
            .map(|&s| line_of(&t, s))
            .filter(|l| !l.trim().is_empty())
            .all(|l| l.trim_start().starts_with('#'));
        let (mut a, mut b) = (anchor.min(pos), anchor.max(pos));
        for &s in starts.iter().rev() {
            let line = line_of(&t, s);
            if commented {
                let indent = line.len() - line.trim_start().len();
                let rest = &line[indent..];
                let n = if rest.starts_with("# ") {
                    2
                } else if rest.starts_with('#') {
                    1
                } else {
                    0
                };
                let at = s + indent;
                t.replace_range(at..at + n, "");
                let shift = |p: usize| if p > at { p - n.min(p - at) } else { p };
                (a, b) = (shift(a), shift(b));
            } else if !line.trim().is_empty() {
                t.insert_str(s, "# ");
                let shift = |p: usize| if p >= s && !(p == s && p == a && a != b) { p + 2 } else { p };
                (a, b) = (shift(a), shift(b));
            }
        }
        self.set_text(id, t, true);
        self.select(Selection::Caret { cell: id, anchor: a, pos: b });
        true
    }

    fn delete(&mut self, cmd: &Command, env: &mut dyn Env, fx: &mut Effects) -> Option<Run> {
        match self.sel.clone() {
            Selection::Cells { ids, .. } => {
                let deletable: Vec<CellId> =
                    ids.into_iter().filter(|id| self.cell(*id).is_some_and(|c| c.deletable())).collect();
                if deletable.is_empty() {
                    fx.beep = true;
                    return None;
                }
                let first = deletable.iter().filter_map(|id| self.index_of(*id)).min().expect("cells exist");
                for id in deletable {
                    if let Some(i) = self.index_of(id) {
                        self.remove_cells(i..i + 1, true);
                    }
                }
                self.select(Selection::Gap(first));
                None
            }
            Selection::Gap(_) => None,
            Selection::Caret { cell, anchor, pos } => {
                let c = self.cell(cell)?;
                let s = &c.text;
                let pair = |o: char| match o {
                    '(' => Some(')'),
                    '[' => Some(']'),
                    '{' => Some('}'),
                    '"' => Some('"'),
                    _ => None,
                };
                // Backspace right after an opener whose automatic closer follows deletes both (NB5.1)
                let both = anchor == pos
                    && *cmd == Command::Backspace
                    && self.closers.contains(&(cell, pos))
                    && s[..pos].chars().next_back().and_then(pair).is_some_and(|cl| s[pos..].starts_with(cl));
                let (a, b) = if both {
                    (text::prev_char(s, pos), pos + 1)
                } else if anchor != pos {
                    (anchor.min(pos), anchor.max(pos))
                } else {
                    match cmd {
                        Command::Backspace => (text::prev_char(s, pos), pos),
                        Command::Delete => (pos, text::next_char(s, pos)),
                        Command::DeleteWordBack => (text::word_left(s, pos), pos),
                        _ => (pos, text::word_right(s, pos)),
                    }
                };
                if a == b {
                    // Backspace at a cell's start, Delete at its end: cells are never merged by deletion
                    return None;
                }
                self.sel = Selection::Caret { cell, anchor: a, pos: b };
                let Some((id, a, b)) = self.edit_target() else {
                    self.sel = Selection::Caret { cell, anchor, pos };
                    fx.beep = true;
                    return None;
                };
                if id == cell {
                    self.shift_closers(id, a, b, 0);
                } else {
                    self.closers.clear();
                }
                self.replace_range(id, a, b, "");
                Some(Run::Deleting(id, env.now()))
            }
        }
    }

    // ----- motion -----

    /// The visible cell before the insertion point `g`.
    fn visible_before(&self, g: usize) -> Option<usize> {
        (0..g.min(self.cells.len())).rev().find(|&j| self.is_visible(j))
    }

    /// The visible cell at or after `g`.
    fn visible_from(&self, g: usize) -> Option<usize> {
        (g..self.cells.len()).find(|&j| self.is_visible(j))
    }

    /// The gap after cell `i` (past any hidden cells).
    fn gap_after(&self, i: usize) -> usize {
        self.normal_gap(i + 1)
    }

    fn caret_enterable(&self, i: usize) -> bool {
        self.cells[i].style != Style::Message && !self.cells[i].props.hidden
    }

    fn motion(&mut self, m: Motion, extend: bool, env: &mut dyn Env) {
        match self.sel.clone() {
            Selection::Caret { cell, anchor, pos } => self.caret_motion(cell, anchor, pos, m, extend, env),
            Selection::Gap(g) => match m {
                Motion::DocStart => self.select(Selection::Gap(0)),
                Motion::DocEnd => self.select(Selection::Gap(self.cells.len())),
                Motion::Down | Motion::Right | Motion::WordRight => {
                    let Some(j) = self.visible_from(g) else { return };
                    if extend {
                        let id = self.cells[j].id;
                        self.select(Selection::Cells { ids: vec![id], anchor: id });
                    } else if !self.caret_enterable(j) {
                        self.select(Selection::Gap(self.gap_after(j)));
                    } else {
                        let id = self.cells[j].id;
                        let pos = if m == Motion::Down {
                            let lines = env.lines(&self.cells[j]);
                            let x = self.goal_x.unwrap_or(0.0);
                            env.pos_at(&self.cells[j], lines[0].clone(), x)
                        } else {
                            0
                        };
                        self.select(Selection::Caret { cell: id, anchor: pos, pos });
                    }
                }
                Motion::Up | Motion::Left | Motion::WordLeft => {
                    let Some(j) = self.visible_before(g) else { return };
                    if extend {
                        let id = self.cells[j].id;
                        self.select(Selection::Cells { ids: vec![id], anchor: id });
                    } else if !self.caret_enterable(j) {
                        self.select(Selection::Gap(j));
                    } else {
                        let c = &self.cells[j];
                        let id = c.id;
                        let pos = if m == Motion::Up {
                            let lines = env.lines(c);
                            let x = self.goal_x.unwrap_or(0.0);
                            env.pos_at(c, lines.last().expect("a line").clone(), x)
                        } else {
                            c.text.len()
                        };
                        self.select(Selection::Caret { cell: id, anchor: pos, pos });
                    }
                }
                Motion::Home | Motion::End => {}
            },
            Selection::Cells { ids, anchor } => {
                let idx: Vec<usize> = ids.iter().filter_map(|id| self.index_of(*id)).collect();
                let (Some(&lo), Some(&hi)) = (idx.iter().min(), idx.iter().max()) else { return };
                match (m, extend) {
                    (Motion::DocStart, _) => self.select(Selection::Gap(0)),
                    (Motion::DocEnd, _) => self.select(Selection::Gap(self.cells.len())),
                    (Motion::Up | Motion::Down, true) => {
                        // extend the cell range by the next visible cell on the side away from the anchor
                        let a = self.index_of(anchor).unwrap_or(lo);
                        let (mut from, mut to) = (lo, hi);
                        if m == Motion::Down {
                            if a <= lo && hi > a || a == lo && hi == lo {
                                if let Some(j) = self.visible_from(hi + 1) {
                                    to = self.groups.after_hidden(j) - 1;
                                }
                            } else if let Some(j) = self.visible_from(lo + 1) {
                                from = j;
                            }
                        } else if a >= hi && lo < a || a == hi && hi == lo {
                            if let Some(j) = self.visible_before(lo) {
                                from = j;
                            }
                        } else if let Some(j) = self.visible_before(hi) {
                            to = self.groups.after_hidden(j) - 1;
                        }
                        let ids = self.cells[from..=to].iter().map(|c| c.id).collect();
                        self.select(Selection::Cells { ids, anchor });
                    }
                    (Motion::Up | Motion::Left | Motion::WordLeft, _) => self.select(Selection::Gap(lo)),
                    (Motion::Down | Motion::Right | Motion::WordRight, _) => {
                        self.select(Selection::Gap(self.gap_after(hi)))
                    }
                    _ => {}
                }
            }
        }
    }

    fn caret_motion(&mut self, cell: CellId, anchor: usize, pos: usize, m: Motion, extend: bool, env: &mut dyn Env) {
        let Some(i) = self.index_of(cell) else { return };
        let c = self.cells[i].clone();
        let s = &c.text;
        let set = |nb: &mut Notebook, p: usize| {
            let anchor = if extend { anchor } else { p };
            nb.select(Selection::Caret { cell, anchor, pos: p });
        };
        // an output shown as one unit (typeset) has two caret places: before it and after it
        let atomic = env.atomic(&c);
        match m {
            Motion::Left | Motion::Right if anchor != pos && !extend => {
                let p = if m == Motion::Left { anchor.min(pos) } else { anchor.max(pos) };
                set(self, p);
            }
            Motion::Left | Motion::WordLeft => {
                if pos > 0 && atomic {
                    set(self, 0);
                } else if pos > 0 {
                    set(self, if m == Motion::Left { text::prev_char(s, pos) } else { text::word_left(s, pos) });
                } else if !extend {
                    self.select(Selection::Gap(i));
                }
            }
            Motion::Right | Motion::WordRight => {
                if pos < s.len() && atomic {
                    set(self, s.len());
                } else if pos < s.len() {
                    set(self, if m == Motion::Right { text::next_char(s, pos) } else { text::word_right(s, pos) });
                } else if !extend {
                    self.select(Selection::Gap(self.gap_after(i)));
                }
            }
            Motion::Up | Motion::Down => {
                let lines = env.lines(&c);
                let k = lines.iter().position(|r| pos <= r.end).unwrap_or(lines.len() - 1);
                let x = *self.goal_x.get_or_insert_with(|| env.x_of(&c, pos));
                let goal = self.goal_x;
                if m == Motion::Up && k > 0 {
                    let p = env.pos_at(&c, lines[k - 1].clone(), x);
                    set(self, p);
                } else if m == Motion::Down && k + 1 < lines.len() {
                    let p = env.pos_at(&c, lines[k + 1].clone(), x);
                    set(self, p);
                } else if extend {
                    set(self, if m == Motion::Up { 0 } else { s.len() });
                } else if m == Motion::Up {
                    self.select(Selection::Gap(i));
                } else {
                    self.select(Selection::Gap(self.gap_after(i)));
                }
                self.goal_x = goal;
            }
            Motion::Home | Motion::End => {
                let lines = env.lines(&c);
                let r = lines.iter().find(|r| pos >= r.start && pos <= r.end).cloned().unwrap_or(0..s.len());
                set(self, if m == Motion::Home { r.start } else { r.end });
            }
            Motion::DocStart => self.select(Selection::Gap(0)),
            Motion::DocEnd => self.select(Selection::Gap(self.cells.len())),
        }
    }

    fn select_all(&mut self) {
        if let Selection::Caret { cell, anchor, pos } = self.sel {
            if let Some(c) = self.cell(cell) {
                let len = c.text.len();
                if !(anchor.min(pos) == 0 && anchor.max(pos) == len) {
                    self.select(Selection::Caret { cell, anchor: 0, pos: len });
                    return;
                }
            }
        }
        let ids: Vec<CellId> = self.cells.iter().map(|c| c.id).collect();
        if let Some(&first) = ids.first() {
            self.select(Selection::Cells { ids, anchor: first });
        }
    }

    fn click_text(&mut self, id: CellId, pos: usize, extend: bool) {
        match self.sel.clone() {
            Selection::Caret { cell, anchor, .. } if extend && cell == id => {
                self.select(Selection::Caret { cell, anchor, pos });
            }
            Selection::Caret { cell, .. } if extend => {
                // a selection dragged across cells becomes a selection of the cells (guess 21)
                if let (Some(a), Some(b)) = (self.index_of(cell), self.index_of(id)) {
                    let ids = self.cells[a.min(b)..=a.max(b)].iter().map(|c| c.id).collect();
                    self.select(Selection::Cells { ids, anchor: cell });
                }
            }
            _ => self.select(Selection::Caret { cell: id, anchor: pos, pos }),
        }
    }

    fn bracket_range(&self, b: Bracket) -> Option<(usize, usize)> {
        match b {
            Bracket::Cell(id) => self.index_of(id).map(|i| (i, i + 1)),
            Bracket::Group(s, e) => (s < e && e <= self.cells.len()).then_some((s, e)),
        }
    }

    fn click_bracket(&mut self, b: Bracket, shift: bool, ctrl: bool) {
        let Some((s, e)) = self.bracket_range(b) else { return };
        let clicked: Vec<CellId> = self.cells[s..e].iter().map(|c| c.id).collect();
        let current = self.sel.clone();
        let sel = match current {
            Selection::Cells { ids, anchor } if ctrl => {
                let mut ids = ids;
                if clicked.iter().all(|c| ids.contains(c)) {
                    ids.retain(|c| !clicked.contains(c));
                } else {
                    ids.extend(clicked.iter().filter(|c| !ids.contains(c)).copied().collect::<Vec<_>>());
                }
                ids.sort_by_key(|id| self.index_of(*id));
                match ids.first() {
                    None => Selection::Gap(s),
                    Some(&f) => Selection::Cells { anchor: if ids.contains(&anchor) { anchor } else { f }, ids },
                }
            }
            Selection::Cells { anchor, .. } | Selection::Caret { cell: anchor, .. } if shift => {
                let a = self.index_of(anchor).unwrap_or(s);
                let (lo, hi) = (a.min(s), (a + 1).max(e));
                Selection::Cells { ids: self.cells[lo..hi].iter().map(|c| c.id).collect(), anchor }
            }
            _ => Selection::Cells { anchor: clicked[0], ids: clicked },
        };
        self.select(sel);
    }

    // ----- clipboard -----

    /// Copy the selection; `plain` writes only the text flavour. `false` when nothing is selected.
    fn copy(&mut self, env: &mut dyn Env, plain: bool) -> bool {
        match self.sel.clone() {
            Selection::Caret { cell, anchor, pos } if anchor != pos => {
                let c = self.cell(cell).expect("selected");
                let t = c.text[anchor.min(pos)..anchor.max(pos)].to_string();
                env.set_clipboard(Clip { text: Some(t), cells: None });
                true
            }
            Selection::Cells { ids, .. } => {
                let text = ids
                    .iter()
                    .filter_map(|id| self.cell(*id))
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let cells = (!plain).then(|| self.fragment(&ids));
                env.set_clipboard(Clip { text: Some(text), cells });
                true
            }
            _ => false,
        }
    }

    fn paste(&mut self, env: &mut dyn Env, fx: &mut Effects) {
        let clip = env.clipboard();
        let fragment = clip.cells.clone().or_else(|| clip.text.clone().filter(|t| mc_nbfile::parse_cells(t).is_some()));
        match self.sel.clone() {
            Selection::Caret { .. } => {
                let t = clip.text.unwrap_or_default();
                if !t.is_empty() {
                    let Some((id, a, b)) = self.edit_target() else {
                        fx.beep = true;
                        return;
                    };
                    self.replace_range(id, a, b, &t);
                }
            }
            Selection::Gap(g) => self.paste_at(g, fragment, clip.text),
            Selection::Cells { ids, .. } => {
                // pasting over selected cells replaces them (guess 5)
                let first = ids.iter().filter_map(|id| self.index_of(*id)).min().unwrap_or(0);
                for id in ids {
                    if let Some(i) = self.index_of(id) {
                        self.remove_cells(i..i + 1, true);
                    }
                }
                self.paste_at(first, fragment, clip.text);
            }
        }
    }

    fn paste_at(&mut self, g: usize, fragment: Option<String>, text: Option<String>) {
        if let Some(cells) = fragment.and_then(|f| self.cells_of_fragment(&f)) {
            let n = cells.len();
            self.insert_cells(g, cells, true);
            self.select(Selection::Gap(g + n));
        } else if let Some(t) = text.filter(|t| !t.is_empty()) {
            // plain text at an insertion point: one Input cell holding it
            let t = t.strip_suffix('\n').unwrap_or(&t).to_string();
            self.create_cell(g, self.gap_style(), &t);
        }
    }

    // ----- evaluation -----

    fn evaluate(&mut self, next: bool, fx: &mut Effects) {
        match self.sel.clone() {
            Selection::Caret { cell, .. } => {
                let Some(i) = self.index_of(cell) else { return };
                if !self.cells[i].evaluatable() {
                    fx.beep = true;
                    return;
                }
                self.enqueue(vec![cell], fx);
                let after = self.after_outputs(i);
                self.select(after);
                if next {
                    let target = (i + 1..self.cells.len()).find(|&j| self.cells[j].evaluatable() && self.is_visible(j));
                    if let Some(j) = target {
                        let id = self.cells[j].id;
                        self.select(Selection::Caret { cell: id, anchor: 0, pos: 0 });
                    }
                }
            }
            Selection::Cells { ids, .. } => {
                let mut ids = ids;
                ids.sort_by_key(|id| self.index_of(*id));
                if !ids.iter().any(|id| self.cell(*id).is_some_and(|c| c.evaluatable())) {
                    fx.beep = true;
                }
                self.enqueue(ids, fx);
            }
            Selection::Gap(_) => fx.beep = true,
        }
    }

    // ----- styles and cell operations -----

    fn set_style(&mut self, style: Style) {
        if let Selection::Gap(_) = self.sel {
            self.gap_style = Some(style);
            return;
        }
        for id in self.selected_cells() {
            self.update_cell(id, true, |c| {
                if c.generated() && !style.generated() {
                    c.input_of = None;
                    c.failure = false;
                }
                c.style = style;
                if !style.info().label {
                    c.label = None;
                }
            });
        }
    }

    fn divide(&mut self) -> bool {
        let Selection::Caret { cell, anchor, pos } = self.sel else { return false };
        let Some(c) = self.cell(cell) else { return false };
        if c.generated() || !c.editable() {
            return false;
        }
        let (a, b) = (anchor.min(pos), anchor.max(pos));
        let t = c.text.clone();
        let style = c.style;
        let trim = |s: &str| s.strip_suffix('\n').unwrap_or(s).to_string();
        let lead = |s: &str| s.strip_prefix('\n').unwrap_or(s).to_string();
        let pieces: Vec<String> = if a == b {
            vec![trim(&t[..a]), lead(&t[a..])]
        } else {
            vec![trim(&t[..a]), lead(&trim(&t[a..b])), lead(&t[b..])]
        };
        let keep: Vec<(usize, String)> = pieces.into_iter().enumerate().filter(|(_, p)| !p.is_empty()).collect();
        if keep.len() < 2 {
            return false;
        }
        let i = self.index_of(cell).expect("exists");
        self.set_text(cell, keep[0].1.clone(), true);
        self.update_cell(cell, true, |c| c.label = None);
        // the caret goes to the start of the piece after the caret, or of the selection's piece
        let mut focus_id = cell;
        for (at, (k, p)) in (i + 1..).zip(&keep[1..]) {
            let nc = self.new_cell(style, p);
            if *k >= 1 && focus_id == cell {
                focus_id = nc.id;
            }
            self.insert_cells(at, vec![nc], true);
        }
        self.select(Selection::Caret { cell: focus_id, anchor: 0, pos: 0 });
        true
    }

    fn merge(&mut self) -> bool {
        let Selection::Cells { ids, .. } = self.sel.clone() else { return false };
        let mut idx: Vec<usize> = ids.iter().filter_map(|id| self.index_of(*id)).collect();
        idx.sort_unstable();
        let keep: Vec<usize> = idx.iter().copied().filter(|&i| !self.cells[i].generated()).collect();
        if keep.len() < 2 {
            return false;
        }
        let first = self.cells[keep[0]].id;
        let joined = keep.iter().map(|&i| self.cells[i].text.as_str()).collect::<Vec<_>>().join("\n");
        let drop: Vec<CellId> = idx.iter().filter(|&&i| i != keep[0]).map(|&i| self.cells[i].id).collect();
        self.set_text(first, joined, true);
        for id in drop {
            if let Some(i) = self.index_of(id) {
                self.remove_cells(i..i + 1, true);
            }
        }
        self.select(Selection::Cells { ids: vec![first], anchor: first });
        true
    }

    fn mark_together(&mut self, on: bool) {
        let ids = self.selected_cells();
        let idx: Vec<usize> = ids.iter().filter_map(|id| self.index_of(*id)).collect();
        let (Some(&lo), Some(&hi)) = (idx.iter().min(), idx.iter().max()) else { return };
        let range: Vec<CellId> = if on {
            self.cells[lo..=hi].iter().map(|c| c.id).collect()
        } else {
            // ungrouping a cell ungroups the whole run it belongs to
            let (mut a, mut b) = (lo, hi);
            while a > 0 && self.cells[a - 1].props.together && self.cells[a].props.together {
                a -= 1;
            }
            while b + 1 < self.cells.len() && self.cells[b + 1].props.together && self.cells[b].props.together {
                b += 1;
            }
            self.cells[a..=b].iter().map(|c| c.id).collect()
        };
        if on && range.len() < 2 {
            return;
        }
        for id in range {
            self.update_cell(id, true, |c| c.props.together = on);
        }
    }

    /// Open or close group `g`; closing shows its `k`-th cell.
    fn toggle_group_at(&mut self, g: usize, k: usize) {
        let head = self.cells[self.groups.list[g].start].id;
        let closed = self.cell(head).and_then(|c| c.closed).is_some();
        self.update_cell(head, true, |c| c.closed = if closed { None } else { Some(k) });
    }

    fn toggle_group(&mut self) -> bool {
        let ids = self.selected_cells();
        let idx: Vec<usize> = ids.iter().filter_map(|id| self.index_of(*id)).collect();
        let Some(&i) = idx.iter().min() else { return false };
        let hi = idx.iter().copied().max().expect("non-empty");
        // a selected group bracket toggles that group, showing its head
        if let Some(g) = self.groups.list.iter().rposition(|g| g.start == i && g.end == hi + 1) {
            if idx.len() > 1 {
                self.toggle_group_at(g, 0);
                return true;
            }
        }
        let Some(&g) = self.groups.containing(i).first() else { return false };
        let k = i - self.groups.list[g].start;
        self.toggle_group_at(g, k);
        true
    }

    fn open_close_all(&mut self, close: bool) {
        let idx: Vec<usize> = self.selected_cells().iter().filter_map(|id| self.index_of(*id)).collect();
        let (lo, hi) = match (idx.iter().min(), idx.iter().max()) {
            (Some(&lo), Some(&hi)) if idx.len() > 1 => (lo, hi + 1),
            (Some(&i), _) => match self.groups.containing(i).first() {
                Some(&g) => (self.groups.list[g].start, self.groups.list[g].end),
                None => return,
            },
            _ => return,
        };
        let heads: Vec<CellId> =
            self.groups.list.iter().filter(|g| g.start >= lo && g.end <= hi).map(|g| self.cells[g.start].id).collect();
        for h in heads {
            self.update_cell(h, true, |c| c.closed = if close { Some(0) } else { None });
        }
        if let Selection::Caret { cell, .. } = self.sel {
            if let Some(i) = self.index_of(cell) {
                if !self.is_visible(i) {
                    let head = self.groups.containing(i).last().map(|&g| self.cells[self.groups.list[g].start].id);
                    if let Some(h) = head {
                        self.select(Selection::Cells { ids: vec![h], anchor: h });
                    }
                }
            }
        }
    }

    fn toggle_prop(&mut self, f: impl Fn(&mut Cell)) {
        for id in self.selected_cells() {
            if self.cell(id).is_some_and(|c| !c.generated()) {
                self.update_cell(id, true, &f);
            }
        }
    }

    fn show_expression(&mut self) -> bool {
        let Some(id) = self.current_cell() else { return false };
        let Some(c) = self.cell(id).cloned() else { return false };
        if c.generated() {
            return false;
        }
        if !c.expr_view {
            let fragment = self.fragment(&[id]);
            let source = fragment.strip_prefix(mc_nbfile::FRAGMENT).unwrap_or(&fragment).trim_matches('\n').to_string();
            self.set_text(id, source, true);
            self.update_cell(id, true, |c| c.expr_view = true);
        } else {
            let text = format!("{}\n{}\n", mc_nbfile::FRAGMENT, c.text);
            let parsed = self
                .cells_of_fragment(&text)
                .filter(|cells| cells.len() == 1 || cells.iter().skip(1).all(|c| c.generated()));
            let Some(mut cells) = parsed else { return false };
            let back = cells.remove(0);
            self.set_text(id, back.text.clone(), true);
            self.update_cell(id, true, |c| {
                let label = c.label;
                *c = Cell { id: c.id, text: c.text.clone(), rev: c.rev, eval: c.eval, ..back };
                c.label = label.or(c.label);
                c.expr_view = false;
            });
        }
        self.select(Selection::Cells { ids: vec![id], anchor: id });
        true
    }

    fn copy_from_above(&mut self, output: bool) -> bool {
        let before = match self.sel {
            Selection::Gap(g) => g,
            _ => match self.current_cell().and_then(|id| self.index_of(id)) {
                Some(i) => i,
                None => return false,
            },
        };
        let wanted = |c: &Cell| if output { c.style == Style::Output } else { c.style.is_code() };
        let Some(src) = self.cells[..before].iter().rev().find(|c| wanted(c)).map(|c| c.text.clone()) else {
            return false;
        };
        match self.sel {
            Selection::Gap(g) => {
                self.create_cell(g, Style::Input, &src);
            }
            Selection::Caret { .. } => {
                let Some((id, a, b)) = self.edit_target() else { return false };
                self.replace_range(id, a, b, &src);
            }
            Selection::Cells { .. } => {
                let i = before;
                let at = self.outputs_end(i);
                self.create_cell(at, Style::Input, &src);
            }
        }
        true
    }

    fn same_style_below(&mut self) -> bool {
        let Some(id) = self.current_cell() else { return false };
        let Some(i) = self.index_of(id) else { return false };
        let style = if self.cells[i].generated() { Style::Input } else { self.cells[i].style };
        self.create_cell(i + 1, style, "");
        true
    }

    fn move_cells(&mut self, to: usize, copy: bool) -> bool {
        let Selection::Cells { ids, anchor } = self.sel.clone() else { return false };
        let mut idx: Vec<usize> = ids.iter().filter_map(|id| self.index_of(*id)).collect();
        idx.sort_unstable();
        let (Some(&lo), Some(&hi)) = (idx.first(), idx.last()) else { return false };
        if !copy && to > lo && to <= hi + 1 && idx.len() == hi - lo + 1 {
            return false;
        }
        let to = self.normal_gap(to);
        let cells: Vec<Cell> = idx.iter().map(|&i| self.cells[i].clone()).collect();
        if copy {
            let fresh: Vec<Cell> = cells.into_iter().map(|c| self.renumber(c)).collect();
            let new_ids: Vec<CellId> = fresh.iter().map(|c| c.id).collect();
            self.insert_cells(to, fresh, true);
            self.select(Selection::Cells { anchor: new_ids[0], ids: new_ids });
            return true;
        }
        let before = idx.iter().filter(|&&i| i < to).count();
        for &i in idx.iter().rev() {
            self.remove_cells(i..i + 1, true);
        }
        self.insert_cells(to - before, cells, true);
        self.select(Selection::Cells { ids, anchor });
        true
    }
}

/// Straight quotes typed in prose become curly ones by context (design 3.2): an opening quote at the start or after
/// white space or an opening bracket, a closing one otherwise.
fn smart_quotes(before: &str, typed: &str) -> String {
    let mut out = String::new();
    let mut prev = before.chars().next_back();
    for c in typed.chars() {
        let opening = prev.is_none_or(|p| p.is_whitespace() || "([{“‘".contains(p));
        let q = match c {
            '"' if opening => '“',
            '"' => '”',
            '\'' if opening => '‘',
            '\'' => '’',
            c => c,
        };
        out.push(q);
        prev = Some(q);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::smart_quotes;
    use crate::{Command, FakeEnv, Motion, Notebook, OutputForm, Selection};

    const TWO_OUTPUTS: &str = "#! mathcas 2 notebook\n\n#% cell label=1\nx/2\n#% output n=1\n#= x/2\n\n#% cell label=2\nsqrt(y)\n#% output n=2 form=linear\n#= sqrt(y)\n";

    #[test]
    fn quotes() {
        assert_eq!(smart_quotes("", "\"a\""), "“a”");
        assert_eq!(smart_quotes("it", "'s"), "’s");
        assert_eq!(smart_quotes("(", "'x'"), "‘x’");
    }

    #[test]
    fn output_forms_are_set_saved_and_undone() {
        let mut nb = Notebook::parse(TWO_OUTPUTS).unwrap();
        let mut env = FakeEnv::default();
        assert_eq!(nb.cells()[1].output_form(), None);
        assert_eq!(nb.cells()[3].output_form(), Some(OutputForm::Linear));
        // the caret in the first output
        let out = nb.cells()[1].id;
        nb.apply(Command::ClickText(out, 0, false), &mut env);
        let fx = nb.apply(Command::ToTypeset, &mut env);
        assert!(!fx.beep && fx.modified);
        assert_eq!(nb.cells()[1].output_form(), Some(OutputForm::Typeset));
        assert!(nb.to_text().contains("#% output n=1 form=typeset\n"), "{}", nb.to_text());
        nb.apply(Command::Undo, &mut env);
        assert_eq!(nb.cells()[1].output_form(), None);
        // a selection of cells changes every output in it
        nb.apply(Command::SelectAll, &mut env);
        nb.apply(Command::SelectAll, &mut env);
        nb.apply(Command::OutputForm(OutputForm::Traditional), &mut env);
        assert_eq!(nb.cells()[1].output_form(), Some(OutputForm::Traditional));
        assert_eq!(nb.cells()[3].output_form(), Some(OutputForm::Traditional));
        // nothing to change: a beep
        nb.apply(Command::ClickGap(0), &mut env);
        assert!(nb.apply(Command::OutputForm(OutputForm::Linear), &mut env).beep);
    }

    #[test]
    fn an_atomic_output_has_two_caret_places() {
        let mut nb = Notebook::parse(TWO_OUTPUTS).unwrap();
        let out = nb.cells()[1].id;
        let len = nb.cells()[1].text.len();
        let mut env = FakeEnv { atomic: vec![out], ..FakeEnv::default() };
        nb.apply(Command::ClickText(out, 0, false), &mut env);
        nb.apply(Command::Move(Motion::Right, false), &mut env);
        assert_eq!(nb.selection(), &Selection::Caret { cell: out, anchor: len, pos: len });
        nb.apply(Command::Move(Motion::Left, true), &mut env);
        assert_eq!(nb.selection(), &Selection::Caret { cell: out, anchor: len, pos: 0 });
        nb.apply(Command::SelectWord(out, 1), &mut env);
        assert_eq!(nb.selection(), &Selection::Caret { cell: out, anchor: 0, pos: len });
        nb.apply(Command::Copy, &mut env);
        assert_eq!(env.clip.text.as_deref(), Some("x/2"));
        // other cells move by characters
        let input = nb.cells()[0].id;
        nb.apply(Command::ClickText(input, 0, false), &mut env);
        nb.apply(Command::Move(Motion::Right, false), &mut env);
        assert_eq!(nb.selection(), &Selection::Caret { cell: input, anchor: 1, pos: 1 });
    }
}
