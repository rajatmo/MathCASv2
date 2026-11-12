//! The keyboard: events into model commands, the completion popup (design 3.10), the find bar.
use super::*;

impl App {
    /// Keyboard events into commands (unless a text field of a dialog has the keyboard).
    pub(super) fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.modal_open() || egui::Popup::is_any_open(ctx) {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        if events.iter().any(|e| matches!(e, egui::Event::Text(_))) {
            self.pointer_hidden = true;
        }
        // the character an Alt chord may also deliver as text (only that text is dropped, round 19: dropping the next
        // text whatever it was lost the first letter typed after Alt+1 where the system sends no text for the chord)
        let mut swallow_text: Option<String> = None;
        let mut copied = false;
        for e in events {
            if self.completion_key(ctx, &e) || self.hint_key(&e) || self.slider_key(ctx, &e) {
                continue;
            }
            if let Some(id) = self.doc.view.graphic {
                // a selected picture (design 6): Esc and arrows deselect it, Delete removes its output, Copy copies
                // it, typing does not reach the text behind it
                match &e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } => match key {
                        Key::Escape | Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                            self.doc.view.graphic = None;
                            continue;
                        }
                        Key::Delete | Key::Backspace => {
                            self.doc.view.graphic = None;
                            self.run(ctx, Command::ClickBracket(Bracket::Cell(id), false, false));
                            self.run(ctx, Command::Delete);
                            continue;
                        }
                        Key::C if modifiers.command && !modifiers.shift && !modifiers.alt => {
                            if !std::mem::replace(&mut copied, true) {
                                self.copy_graphic(id);
                            }
                            continue;
                        }
                        _ => {}
                    },
                    egui::Event::Copy => {
                        if !std::mem::replace(&mut copied, true) {
                            self.copy_graphic(id);
                        }
                        continue;
                    }
                    egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut => continue,
                    _ => {}
                }
            }
            if self.find.is_some() && matches!(e, egui::Event::Key { key: Key::Escape, pressed: true, .. }) {
                self.find = None;
                continue;
            }
            // Esc closes the kernel log and the keyboard help first (windows that stay open while typing)
            if (self.kernel_log || self.keyboard_help) && matches!(e, egui::Event::Key { key: Key::Escape, pressed: true, .. }) {
                self.kernel_log = false;
                self.keyboard_help = false;
                continue;
            }
            match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    swallow_text = None;
                    self.doc.auto_complete_at = None;
                    // the menus from the keyboard: F10 opens File, Alt+letter the menu of that letter
                    if let Some(id) = self.menu_for_key(key, modifiers) {
                        egui::Popup::open_id(ctx, id.with("popup"));
                        self.menu_focus.set(true);
                        swallow_text = modifiers.alt.then(|| key.name().to_lowercase());
                        continue;
                    }
                    let Some(chord) = input::chord(key, modifiers) else {
                        continue;
                    };
                    if let Some(cmd) = self.keys.lookup(chord, self.context()).cloned() {
                        // a character key with Alt may also deliver its character as text: it is not typed (egui
                        // already drops the text of Ctrl chords)
                        swallow_text = match chord.key {
                            mc_nbmodel::keys::Key::Char(c) if chord.mods.alt => Some(c.to_lowercase().to_string()),
                            _ => None,
                        };
                        self.run(ctx, cmd);
                    }
                }
                egui::Event::Text(t) => {
                    if std::mem::take(&mut swallow_text).is_some_and(|c| t.to_lowercase() == c) {
                        continue;
                    }
                    let word = t.chars().all(|c| c.is_alphanumeric() || c == '_');
                    self.run(ctx, Command::Insert(t));
                    if word && self.doc.completion.is_none() {
                        self.doc.auto_complete_at = Some(ctx.input(|i| i.time) + COMPLETION_DELAY);
                    }
                }
                // input methods (accented and CJK input): the composition shows at the caret until committed
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => self.preedit = text,
                egui::Event::Ime(egui::ImeEvent::Commit(t)) => {
                    self.preedit.clear();
                    if !t.is_empty() {
                        self.pointer_hidden = true;
                        self.run(ctx, Command::Insert(t));
                    }
                }
                egui::Event::Copy => self.run(ctx, Command::Copy),
                egui::Event::Cut => self.run(ctx, Command::Cut),
                egui::Event::Paste(t) => {
                    self.clip.pasted = Some(t);
                    self.run(ctx, Command::Paste);
                }
                _ => {}
            }
        }
    }

    /// The name being typed at the caret in a code cell: (cell, where, start, prefix). In typeset input the name is
    /// in the caret's row of boxes (`where` is that row; `start` an item offset), else in the text (`start` a byte).
    pub(super) fn word_at_caret(&self) -> Option<(mc_nbmodel::CellId, Option<mc_boxes::RowPath>, usize, String)> {
        let Selection::Caret { cell, anchor, pos } = *self.doc.nb.selection() else {
            return None;
        };
        let c = self.doc.nb.cell(cell)?;
        if mc_nbmodel::drawn_as_tree(c) {
            // no completion inside a comment or a string
            let (path, start, word) = self.doc.nb.name_before_caret()?;
            if c.style.is_code() {
                let row = c.tree();
                let r = row.at(&path)?;
                let end = start.min(r.len());
                let line_start = (0..end).rev().find(|&j| r.0[j] == mc_boxes::Item::Char('\n')).map_or(0, |j| j + 1);
                let line: String =
                    r.0[line_start..end].iter().map(|i| if let mc_boxes::Item::Char(ch) = i { *ch } else { ' ' }).collect();
                if in_comment_or_string(&line) {
                    return None;
                }
            }
            return Some((cell, Some(path), start, word));
        }
        if anchor != pos || !c.style.is_code() || c.expr_view {
            return None;
        }
        let before = &c.text[..pos];
        // no completion inside a comment or a string: the caret's line up to it decides
        if in_comment_or_string(&before[before.rfind('\n').map_or(0, |i| i + 1)..]) {
            return None;
        }
        let len: usize = before
            .chars()
            .rev()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '.')
            .map(char::len_utf8)
            .sum();
        Some((cell, None, pos - len, before[pos - len..].to_string()))
    }

    /// Keys for the slider last pressed: Left/Right (Down/Up) a step (a hundredth of the range when continuous),
    /// Shift ten steps, Home/End the ends, Esc lets go; `true` when used.
    fn slider_key(&mut self, ctx: &egui::Context, e: &egui::Event) -> bool {
        let Some((id, k)) = self.slider else { return false };
        let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { return false };
        let Some((m, values)) = self.doc.nb.manipulate_of(id) else {
            self.slider = None;
            return false;
        };
        let (Some(c), Some(&now)) = (m.controls.get(k), values.get(k)) else { return false };
        let step = c.step.unwrap_or((c.hi - c.lo) / 100.0) * if modifiers.shift { 10.0 } else { 1.0 };
        let v = match key {
            egui::Key::ArrowRight | egui::Key::ArrowUp => now + step,
            egui::Key::ArrowLeft | egui::Key::ArrowDown => now - step,
            egui::Key::Home => c.lo,
            egui::Key::End => c.hi,
            egui::Key::Escape => {
                self.slider = None;
                return true;
            }
            _ => return false,
        };
        let v = match c.step {
            Some(s) if s > 0.0 => c.lo + ((v - c.lo) / s).round() * s,
            _ => v,
        }
        .clamp(c.lo.min(c.hi), c.hi.max(c.lo));
        if v != now {
            self.run(ctx, Command::SetControl(id, k, v, true));
        }
        true
    }

    pub(super) fn start_completion(&mut self) {
        let Some((cell, path, start, _)) = self.word_at_caret() else {
            return;
        };
        let mut names: Vec<(String, String)> =
            mc_syntax::KEYWORDS.iter().map(|k| (k.to_string(), "keyword".to_string())).collect();
        if let Some(c) = self.doc.nb.cell(cell) {
            // the locals in scope at the name: in typeset input, at its place in the canonical text
            let at = match (&path, self.doc.nb.input_editor()) {
                (Some(p), Some((_, ed))) => mc_boxes::to_text_mapped(ed.root()).1.offset(p, start).unwrap_or(c.text.len()),
                _ => start,
            };
            names.extend(mc_syntax::names_in_scope(&c.text, at).into_iter().map(|l| (l.name, "local".to_string())));
        }
        // every documented function with its usage line (the help index), after the locals (a local of the same name
        // is what the name means here) and before the session's own names
        names.extend(mc_help::Index::embedded().entries().iter().map(|e| {
            (e.name.clone(), if e.is_function() { "function" } else { "constant" }.to_string())
        }));
        // the session's own definitions come from a running kernel (completion never starts one)
        if self.doc.kernel.is_running() {
            self.doc.kernel.request_names("");
        }
        self.doc.completion = Some(Completion { cell, path, start, names, selected: 0 });
    }

    /// Keys for the completion popup (design 3.1: it takes Up, Down, Enter, Tab and Esc); `true` when used.
    pub(super) fn completion_key(&mut self, ctx: &egui::Context, e: &egui::Event) -> bool {
        let Some(comp) = &self.doc.completion else {
            return false;
        };
        let egui::Event::Key { key, pressed: true, modifiers, .. } = e else {
            return false;
        };
        if modifiers.any() {
            return false;
        }
        let prefix = self.word_at_caret().map(|w| w.3).unwrap_or_default();
        let items = rank(&comp.names, &prefix);
        match key {
            Key::ArrowDown if !items.is_empty() => {
                let c = self.doc.completion.as_mut().expect("open");
                c.selected = (c.selected + 1) % items.len();
                true
            }
            Key::ArrowUp if !items.is_empty() => {
                let c = self.doc.completion.as_mut().expect("open");
                c.selected = (c.selected + items.len() - 1) % items.len();
                true
            }
            Key::Enter | Key::Tab if !items.is_empty() => {
                let (name, kind) = items[comp.selected.min(items.len() - 1)].clone();
                self.doc.completion = None;
                self.run(ctx, Command::ReplaceWord(name));
                // a function gets its opening bracket (and, with automatic closing, the closing one), unless a
                // bracket follows already
                let bracket_follows = self.code_caret().is_some_and(|c| c.text[c.pos..].starts_with('('));
                if kind == "function" && !bracket_follows {
                    self.run(ctx, Command::Insert("(".into()));
                }
                true
            }
            Key::Escape => {
                self.doc.completion = None;
                true
            }
            _ => false,
        }
    }

    /// Keep the popup in step with the caret, take the kernel's names, draw it.
    pub(super) fn completion_popup(&mut self, ctx: &egui::Context) {
        if let Some(at) = self.doc.auto_complete_at {
            let now = ctx.input(|i| i.time);
            if now >= at {
                self.doc.auto_complete_at = None;
                if self.word_at_caret().is_some_and(|w| w.3.chars().count() >= 2) {
                    self.start_completion();
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(at - now));
            }
        }
        if let Some(names) = self.doc.kernel.names() {
            self.doc.view.set_defined(Some(names.iter().map(|n| n.0.clone()).collect()));
            self.doc.view.set_functions(&names);
            if let Some(c) = &mut self.doc.completion {
                c.names.extend(names);
            }
        }
        let Some(comp) = &self.doc.completion else {
            return;
        };
        let word = self.word_at_caret();
        let Some((cell, _, start, prefix)) = word.filter(|w| w.0 == comp.cell && w.1 == comp.path && w.2 == comp.start) else {
            self.doc.completion = None;
            return;
        };
        let items = rank(&comp.names, &prefix);
        // the caret of a typeset cell is the editor's (any position gives it)
        let pos = start + prefix.len();
        let Some(r) = self.doc.view.caret_rect(ctx, &self.doc.nb, cell, pos) else {
            return;
        };
        let at = self.doc.view.origin + r.left_bottom().to_vec2() + vec2(0.0, 2.0);
        let selected = comp.selected.min(items.len().saturating_sub(1));
        let p = self.doc.view.palette.clone();
        let mut clicked = None;
        egui::Area::new(egui::Id::unique("mc-notebook-completion")).fixed_pos(at).order(egui::Order::Foreground).show(
            ctx,
            |ui| {
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    if items.is_empty() {
                        ui.label(RichText::new("no completions").color(p.weak));
                    }
                    let index = mc_help::Index::embedded();
                    for (k, (name, kind)) in items.iter().take(12).enumerate() {
                        // a documented name shows its one-line usage, others their kind (keyword, local, session)
                        let usage = match kind.as_str() {
                            "keyword" | "local" => None,
                            _ => index.get(name).map(|e| e.line()),
                        };
                        let detail = usage.unwrap_or_else(|| kind.clone());
                        let mut job = egui::text::LayoutJob::default();
                        let mono = egui::FontId::monospace(13.0);
                        job.append(
                            &format!("{name:<20} "),
                            0.0,
                            egui::TextFormat { font_id: mono, color: p.text, ..Default::default() },
                        );
                        let short: String = detail.chars().take(72).collect();
                        let short = if short.len() < detail.len() { format!("{short}...") } else { short };
                        job.append(
                            &short,
                            0.0,
                            egui::TextFormat { font_id: egui::FontId::proportional(12.0), color: p.weak, ..Default::default() },
                        );
                        if ui.selectable_label(k == selected, job).clicked() {
                            clicked = Some(k);
                        }
                    }
                });
            },
        );
        if let Some(k) = clicked {
            if let Some(c) = &mut self.doc.completion {
                c.selected = k;
            }
            let e = egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            self.completion_key(ctx, &e);
        }
    }

    /// Select the next (or previous) occurrence of the find text after (before) the selection, wrapping around.
    pub(super) fn find_next(&mut self, ctx: &egui::Context, forward: bool) {
        let Some(needle) = self.find.clone().filter(|n| !n.is_empty()) else {
            return;
        };
        let cells = self.doc.nb.cells();
        let n = cells.len();
        if n == 0 {
            return;
        }
        let (start_cell, start_pos) = match self.doc.nb.selection() {
            Selection::Caret { cell, anchor, pos } => {
                let i = self.doc.nb.index_of(*cell).unwrap_or(0);
                (i, if forward { *anchor.max(pos) } else { *anchor.min(pos) })
            }
            Selection::Cells { ids, .. } => (ids.first().and_then(|id| self.doc.nb.index_of(*id)).unwrap_or(0), 0),
            Selection::Gap(g) => {
                (if forward { (*g).min(n - 1) } else { g.saturating_sub(1) }, if forward { 0 } else { usize::MAX })
            }
        };
        // in a cell with boxes, where the box selection ends (the model's own caret sits at the text's end then)
        let start_pos = match (self.doc.nb.input_editor(), self.doc.nb.selection()) {
            (Some((id, ed)), Selection::Caret { cell, .. }) if id == *cell => {
                let (_, map) = mc_boxes::to_text_mapped(ed.root());
                let (path, k) = match ed.selection() {
                    Some((p, r)) => (p, if forward { r.end } else { r.start }),
                    None => (ed.caret().path.clone(), ed.caret().offset),
                };
                // the top row's item the position is in (after it, going forward)
                let top = match path.first() {
                    Some(s) => s.item + usize::from(forward),
                    None => k,
                };
                map.offset(&[], top).unwrap_or(start_pos)
            }
            _ => start_pos,
        };
        let mut found = None;
        for k in 0..=n {
            let i = if forward { (start_cell + k) % n } else { (start_cell + n - k % n) % n };
            // a cell with boxes is searched in its linear text (`x^2` finds a power box, NBUI.7)
            let boxed = cells[i].props.typeset && cells[i].style.is_code();
            let (lin, segs) = if boxed { mc_boxes::linear_segments(&cells[i].text) } else { (String::new(), vec![]) };
            let t: &str = if boxed { &lin } else { &cells[i].text };
            let to_lin = |pos: usize| -> usize {
                if !boxed {
                    return pos;
                }
                if forward {
                    segs.iter().find(|(_, c)| c.start >= pos).map_or(t.len(), |(l, _)| l.start)
                } else {
                    segs.iter().rev().find(|(_, c)| c.end <= pos).map_or(0, |(l, _)| l.end)
                }
            };
            let start_pos = if k == 0 { to_lin(start_pos) } else { start_pos };
            let hit = if k == 0 {
                if forward {
                    t.get(start_pos.min(t.len())..).and_then(|s| s.find(&needle)).map(|p| p + start_pos.min(t.len()))
                } else {
                    t.get(..start_pos.min(t.len())).and_then(|s| s.rfind(&needle))
                }
            } else if forward {
                t.find(&needle)
            } else {
                t.rfind(&needle)
            };
            if let Some(p) = hit {
                if self.doc.nb.is_visible(i) {
                    // in a cell with boxes: the canonical text of the items the linear match covers
                    let range = if boxed {
                        let last = p + needle.len().max(1) - 1;
                        let a = segs.iter().find(|(l, _)| l.contains(&p)).map_or(0, |(_, c)| c.start);
                        let b = segs.iter().find(|(l, _)| l.contains(&last)).map_or(cells[i].text.len(), |(_, c)| c.end);
                        (a, b)
                    } else {
                        (p, p + needle.len())
                    };
                    found = Some((cells[i].id, range));
                    break;
                }
            }
        }
        match found {
            Some((id, (a, b))) => {
                self.run(ctx, Command::SelectText(id, a, b));
                self.status.clear();
            }
            None => {
                self.status = format!("\u{201c}{needle}\u{201d} not found");
                beep();
            }
        }
    }

    pub(super) fn find_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let Some(mut text) = self.find.clone() else {
            return;
        };
        let mut action = None;
        ui.horizontal_centered(|ui| {
            ui.label("Find");
            let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(260.0));
            if std::mem::take(&mut self.find_focus) {
                edit.request_focus();
            }
            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                action = Some(!ui.input(|i| i.modifiers.shift));
                edit.request_focus();
            }
            let color = ui.visuals().text_color();
            let icon = |ui: &mut egui::Ui, i: Icon, tip: &str| {
                ui.add(egui::Button::image(icons::image(ui.ctx(), i, 14.0, color)).min_size(vec2(26.0, 22.0)))
                    .on_hover_text(tip)
            };
            if icon(ui, Icon::ArrowDown, "Next (Enter, F3)").clicked() {
                action = Some(true);
            }
            if icon(ui, Icon::ArrowUp, "Previous (Shift+Enter, Shift+F3)").clicked() {
                action = Some(false);
            }
            if icon(ui, Icon::Close, "Close (Esc)").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) && edit.has_focus() {
                self.find = None;
                return;
            }
            self.find = Some(text.clone());
        });
        if let Some(forward) = action {
            self.find_next(&ctx, forward);
        }
    }

    /// The menu a key opens: F10 the first, Alt+letter (without Ctrl) the one of that letter.
    fn menu_for_key(&self, key: Key, m: egui::Modifiers) -> Option<egui::Id> {
        if key == Key::F10 && !m.any() {
            return self.menu_ids.first().map(|(_, id)| *id);
        }
        if !m.alt || m.ctrl || m.shift || m.command {
            return None;
        }
        let letter = key.name().chars().next()?.to_ascii_lowercase();
        if key.name().len() != 1 {
            return None;
        }
        self.menu_ids.iter().find(|(c, _)| *c == letter).map(|(_, id)| *id)
    }

    pub(super) fn modal_open(&self) -> bool {
        self.other_style.is_some()
            || self.ask_init
            || self.interrupt
            || self.tabs.asked().is_some()
            || self.error.is_some()
            || self.about
            || self.prefs
    }
}

/// Whether the end of `line` (the text of a line up to the caret) is inside a comment (`#` to the end of the line)
/// or a string (`"..."`, with `\"` escapes; raw strings `r"..."` have none).
fn in_comment_or_string(line: &str) -> bool {
    let mut chars = line.chars().peekable();
    let mut in_str = false;
    let mut raw = false;
    let mut prev = ' ';
    while let Some(ch) = chars.next() {
        if in_str {
            if ch == '\\' && !raw {
                chars.next();
            } else if ch == '"' {
                in_str = false;
            }
        } else if ch == '#' {
            return true;
        } else if ch == '"' {
            in_str = true;
            raw = prev == 'r';
        }
        prev = ch;
    }
    in_str
}

#[cfg(test)]
mod comment_tests {
    use super::in_comment_or_string;

    #[test]
    fn comments_and_strings_stop_completion() {
        assert!(in_comment_or_string("# a comm"));
        assert!(in_comment_or_string("f(x) # note"));
        assert!(in_comment_or_string("say(\"plo"));
        assert!(in_comment_or_string("label(r\"$\\alp"));
        assert!(!in_comment_or_string("say(\"a\") + plo"));
        assert!(!in_comment_or_string("plo"));
        assert!(!in_comment_or_string("s = \"a#b\"; sq"));
    }
}
