//! The notebook window: chrome (menus, toolbar, status bar, dialogs, design 10), input routing into model commands,
//! the canvas, files, settings and the kernel link.
use crate::clip::Clipboard;
use crate::fonts;
use crate::input;
use crate::kernel::{default_kernel, KernelProcess};
use crate::selftest::SelfTest;
use crate::settings::Settings;
use crate::theme::{Palette, ThemeChoice};
use crate::view::{Hit, View};
use eframe::egui::{
    self, vec2, Align, Color32, Frame, Key, Layout, Margin, RichText, ScrollArea, Sense, ViewportCommand,
};
use mc_nbmodel::keys::{Context, KeyTable};
use mc_nbmodel::{
    AppAction, Bracket, Cell, Clip, Command, Effects, Env, InitAnswer, KernelState, Notebook, Selection, Style,
};
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Magnifications offered (design 4.10: 50 % - 300 %).
const ZOOMS: [f32; 9] = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
/// Two clicks closer than this (seconds) count as a double click.
const DOUBLE_CLICK: f64 = 0.4;

/// The model's view of the window: layout from the canvas, the clipboard, the time.
struct AppEnv<'a> {
    view: &'a mut View,
    ctx: &'a egui::Context,
    clip: &'a mut Clipboard,
}

impl Env for AppEnv<'_> {
    fn lines(&mut self, cell: &Cell) -> Vec<Range<usize>> {
        self.view.lines(self.ctx, cell)
    }
    fn x_of(&mut self, cell: &Cell, pos: usize) -> f32 {
        self.view.x_of(self.ctx, cell, pos)
    }
    fn pos_at(&mut self, cell: &Cell, line: Range<usize>, x: f32) -> usize {
        self.view.pos_at(self.ctx, cell, line, x)
    }
    fn now(&self) -> f64 {
        self.ctx.input(|i| i.time)
    }
    fn set_clipboard(&mut self, clip: Clip) {
        self.clip.set(clip);
    }
    fn clipboard(&mut self) -> Clip {
        self.clip.get()
    }
}

/// A mouse drag in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// extending a text selection
    Text,
    /// selecting brackets
    Brackets,
    /// pressed on a selected bracket: moving the cells once the pointer moves far enough
    Cells(egui::Pos2),
}

/// What waits for the answer to "save changes?".
#[derive(Clone, Debug, PartialEq)]
enum Pending {
    Close,
}

/// Completion delay after two typed identifier characters (design 3.10, guess 20).
const COMPLETION_DELAY: f64 = 0.3;

/// The completion popup (design 3.10).
struct Completion {
    cell: mc_nbmodel::CellId,
    /// where the name being completed starts
    start: usize,
    /// candidates from the kernel and the keywords: (name, kind)
    names: Vec<(String, String)>,
    selected: usize,
}

/// The candidates matching `prefix`: case-insensitive prefix matches first, then subsequence matches, each by
/// length then name.
pub fn rank(names: &[(String, String)], prefix: &str) -> Vec<(String, String)> {
    let p = prefix.to_lowercase();
    let subsequence = |n: &str| {
        let mut it = n.chars();
        p.chars().all(|c| it.any(|d| d == c))
    };
    let mut scored: Vec<(u8, &(String, String))> = names
        .iter()
        .filter_map(|e| {
            let n = e.0.to_lowercase();
            if n.starts_with(&p) {
                Some((0, e))
            } else if subsequence(&n) {
                Some((1, e))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1 .0.len().cmp(&b.1 .0.len())).then(a.1 .0.cmp(&b.1 .0)));
    let mut out: Vec<(String, String)> = scored.into_iter().map(|(_, e)| e.clone()).collect();
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// The application.
pub struct App {
    nb: Notebook,
    path: Option<PathBuf>,
    view: View,
    kernel: KernelProcess,
    keys: KeyTable,
    clip: Clipboard,
    settings: Settings,
    applied: Option<Palette>,
    status: String,
    scroll_pending: bool,
    scroll_by: f32,
    /// the scroll offset of the last frame
    offset: f32,
    last_click: (f64, egui::Pos2, u32),
    drag: Option<Drag>,
    title: String,
    // dialogs
    other_style: Option<String>,
    ask_init: bool,
    interrupt: bool,
    confirm: Option<Pending>,
    confirmed_close: bool,
    kernel_log: bool,
    keyboard_help: bool,
    about: bool,
    error: Option<String>,
    selftest: Option<SelfTest>,
    completion: Option<Completion>,
    /// the find bar's text, when it is open
    find: Option<String>,
    find_focus: bool,
    /// when to open the completion popup by itself
    auto_complete_at: Option<f64>,
    /// the duration of the last frame's work, for the status bar's probe and the self-test
    pub frame_ms: f64,
}

impl App {
    /// The window with the notebook at `path` (or an empty one).
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>, selftest: Option<SelfTest>) -> App {
        let ctx = cc.egui_ctx.clone();
        fonts::install(&ctx);
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let settings = Settings::load();
        let palette = settings.theme.palette(ctx.system_theme().is_some_and(|t| t == egui::Theme::Dark));
        let mut view = View::new(palette);
        view.zoom = settings.magnification;
        let mut app = App {
            nb: Notebook::new(),
            path: None,
            view,
            kernel: KernelProcess::new(default_kernel(), ctx.clone()),
            keys: KeyTable::default(),
            clip: Clipboard::default(),
            settings,
            applied: None,
            status: String::new(),
            scroll_pending: true,
            scroll_by: 0.0,
            offset: 0.0,
            last_click: (-1.0, egui::Pos2::ZERO, 0),
            drag: None,
            title: String::new(),
            other_style: None,
            ask_init: false,
            interrupt: false,
            confirm: None,
            confirmed_close: false,
            kernel_log: false,
            keyboard_help: false,
            about: false,
            error: None,
            selftest,
            completion: None,
            find: None,
            find_focus: false,
            auto_complete_at: None,
            frame_ms: 0.0,
        };
        if let Some(p) = path {
            app.open(&p);
        }
        app
    }

    // ----- files -----

    /// Open a notebook in this window.
    pub fn open(&mut self, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(format!("Cannot read {}: {e}", path.display()));
                return;
            }
        };
        match mc_nbfile::Notebook::read(&text) {
            Ok((f, warnings)) => {
                self.set_notebook(Notebook::from_file(&f), Some(path.to_path_buf()));
                self.status = match warnings.first() {
                    Some(w) => format!("{} warning(s); first: {w}", warnings.len()),
                    None => format!("Opened {}", file_name(path)),
                };
                if let Some(m) = f.header.number::<f32>("magnification") {
                    self.view.zoom = m.clamp(0.5, 3.0);
                }
                self.settings.add_recent(path);
            }
            Err(e) => self.error = Some(format!("{} is not a notebook: {e}", path.display())),
        }
    }

    /// Replace the notebook (the old kernel session ends).
    pub fn set_notebook(&mut self, nb: Notebook, path: Option<PathBuf>) {
        let mut nb = nb;
        nb.apply(Command::QuitKernel, &mut mc_nbmodel::FakeEnv::default());
        self.kernel.stop();
        self.nb = Notebook::new();
        self.nb = nb;
        self.nb.mark_saved();
        self.kernel.dir = path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf));
        self.kernel.origin = path.as_ref().map(|p| p.display().to_string());
        self.path = path;
        self.view.clear();
        self.scroll_pending = true;
    }

    fn save_to(&mut self, path: &Path) -> bool {
        let text = if self.settings.save_without_outputs {
            let mut copy = self.nb.clone();
            copy.apply(Command::DeleteAllOutput, &mut mc_nbmodel::FakeEnv::default());
            let mut f = copy.to_file();
            f.cells.iter_mut().for_each(|c| c.attrs.remove("label"));
            f.print()
        } else {
            self.nb.to_text()
        };
        let start = std::time::Instant::now();
        match mc_nbfile::write_atomic(path, &text) {
            Ok(()) => {
                self.nb.mark_saved();
                if self.path.as_deref() != Some(path) {
                    self.kernel.dir = path.parent().map(Path::to_path_buf);
                    self.kernel.origin = Some(path.display().to_string());
                    self.path = Some(path.to_path_buf());
                }
                self.settings.add_recent(path);
                self.status = format!("Saved {} ({:.1} ms)", file_name(path), start.elapsed().as_secs_f64() * 1000.0);
                true
            }
            Err(e) => {
                self.error = Some(format!("Cannot save {}: {e}", path.display()));
                false
            }
        }
    }

    fn save(&mut self) -> bool {
        match self.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let mut dialog = rfd::FileDialog::new().add_filter("MathCAS notebook", &["mcn"]);
        if let Some(p) = &self.path {
            if let Some(d) = p.parent() {
                dialog = dialog.set_directory(d);
            }
            if let Some(n) = p.file_name() {
                dialog = dialog.set_file_name(n.to_string_lossy());
            }
        } else {
            dialog = dialog.set_file_name("Untitled.mcn");
        }
        match dialog.save_file() {
            Some(p) => self.save_to(&p),
            None => false,
        }
    }

    fn open_dialog(&mut self) {
        let dialog = rfd::FileDialog::new().add_filter("MathCAS notebook", &["mcn"]);
        if let Some(p) = dialog.pick_file() {
            self.open_path(p);
        }
    }

    /// Open `p` here when this window holds nothing worth keeping, else in a new window.
    fn open_path(&mut self, p: PathBuf) {
        if self.nb.cells().is_empty() && !self.nb.is_modified() {
            self.open(&p);
        } else {
            spawn_window(Some(&p));
        }
    }

    fn export_script(&mut self) {
        let dialog = rfd::FileDialog::new().add_filter("MathCAS script", &["mc"]);
        let Some(p) = dialog.save_file() else { return };
        let mut out = String::from("#! mathcas 2\n");
        for c in self.nb.cells() {
            if c.style.is_code() {
                out.push_str(&c.text);
                out.push_str("\n\n");
            } else if !c.generated() {
                for l in c.text.lines() {
                    out.push_str(&format!("# {l}\n"));
                }
                out.push('\n');
            }
        }
        if let Err(e) = mc_nbfile::write_atomic(&p, &out) {
            self.error = Some(format!("Cannot write {}: {e}", p.display()));
        }
    }

    // ----- commands -----

    /// Apply a command to the notebook and act on its effects.
    pub fn run(&mut self, ctx: &egui::Context, cmd: Command) {
        let mut env = AppEnv { view: &mut self.view, ctx, clip: &mut self.clip };
        let fx = self.nb.apply(cmd, &mut env);
        self.effects(ctx, fx);
    }

    fn effects(&mut self, ctx: &egui::Context, fx: Effects) {
        if fx.scroll {
            self.scroll_pending = true;
            self.view.blink_from = ctx.input(|i| i.time);
        }
        if fx.beep {
            beep();
        }
        if fx.ask_init {
            self.ask_init = true;
        }
        if let Some(a) = fx.app {
            self.app_action(ctx, a);
        }
    }

    fn app_action(&mut self, ctx: &egui::Context, a: AppAction) {
        match a {
            AppAction::New => spawn_window(None),
            AppAction::Open => self.open_dialog(),
            AppAction::Save => {
                self.save();
            }
            AppAction::SaveAs => {
                self.save_as();
            }
            AppAction::Close => ctx.send_viewport_cmd(ViewportCommand::Close),
            AppAction::OtherStyle => self.other_style = Some(String::new()),
            AppAction::InterruptDialog => self.interrupt = true,
            AppAction::ScrollLines(n) => self.scroll_by += n as f32 * 20.0 * self.view.zoom,
            AppAction::ScrollPages(n) => self.scroll_by += n as f32 * ctx.content_rect().height() * 0.85,
            AppAction::Zoom(d) => self.step_zoom(d),
            AppAction::FullScreen => {
                let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!full));
            }
            AppAction::Complete => self.start_completion(),
            AppAction::Find => {
                // Ctrl+E's "use the selection" is folded in: a text selection becomes the search text
                let selected = match self.nb.selection() {
                    Selection::Caret { cell, anchor, pos } if anchor != pos => {
                        self.nb.cell(*cell).map(|c| c.text[*anchor.min(pos)..*anchor.max(pos)].to_string())
                    }
                    _ => None,
                };
                self.find = Some(selected.or_else(|| self.find.clone()).unwrap_or_default());
                self.find_focus = true;
            }
            AppAction::FindNext(forward) => self.find_next(ctx, forward),
            AppAction::Help => {
                self.status = "Not available yet".into();
            }
        }
    }

    fn step_zoom(&mut self, d: i32) {
        let i = ZOOMS.iter().position(|&z| z >= self.view.zoom - 0.01).unwrap_or(2) as i32;
        let j = (i + d).clamp(0, ZOOMS.len() as i32 - 1) as usize;
        self.set_zoom(ZOOMS[j]);
    }

    fn set_zoom(&mut self, z: f32) {
        self.view.zoom = z.clamp(0.5, 3.0);
        self.settings.magnification = self.view.zoom;
        self.settings.save();
        self.scroll_pending = true;
    }

    fn context(&self) -> Context {
        mc_nbmodel::script::context(&self.nb)
    }

    /// Keyboard events into commands (unless a text field of a dialog has the keyboard).
    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.modal_open() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let mut swallow_text = false;
        for e in events {
            if self.completion_key(ctx, &e) {
                continue;
            }
            if self.find.is_some() && matches!(e, egui::Event::Key { key: Key::Escape, pressed: true, .. }) {
                self.find = None;
                continue;
            }
            match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    swallow_text = false;
                    self.auto_complete_at = None;
                    let Some(chord) = input::chord(key, modifiers) else { continue };
                    if let Some(cmd) = self.keys.lookup(chord, self.context()).cloned() {
                        // a character key with Alt may also deliver its character as text: it is not typed (egui
                        // already drops the text of Ctrl chords)
                        swallow_text = chord.mods.alt && matches!(chord.key, mc_nbmodel::keys::Key::Char(_));
                        self.run(ctx, cmd);
                    }
                }
                egui::Event::Text(t) => {
                    if std::mem::take(&mut swallow_text) {
                        continue;
                    }
                    let word = t.chars().all(|c| c.is_alphanumeric() || c == '_');
                    self.run(ctx, Command::Insert(t));
                    if word && self.completion.is_none() {
                        self.auto_complete_at = Some(ctx.input(|i| i.time) + COMPLETION_DELAY);
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

    /// The name being typed at the caret in a code cell: (cell, start, prefix).
    fn word_at_caret(&self) -> Option<(mc_nbmodel::CellId, usize, String)> {
        let Selection::Caret { cell, anchor, pos } = *self.nb.selection() else { return None };
        let c = self.nb.cell(cell)?;
        if anchor != pos || !c.style.is_code() || c.expr_view {
            return None;
        }
        let before = &c.text[..pos];
        let len: usize = before
            .chars()
            .rev()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '.')
            .map(char::len_utf8)
            .sum();
        Some((cell, pos - len, before[pos - len..].to_string()))
    }

    fn start_completion(&mut self) {
        let Some((cell, start, _)) = self.word_at_caret() else { return };
        let mut names: Vec<(String, String)> =
            mc_syntax::KEYWORDS.iter().map(|k| (k.to_string(), "keyword".to_string())).collect();
        if let Some(c) = self.nb.cell(cell) {
            names.extend(mc_syntax::names_in_scope(&c.text, start).into_iter().map(|l| (l.name, "local".to_string())));
        }
        self.kernel.request_names("");
        self.completion = Some(Completion { cell, start, names, selected: 0 });
    }

    /// Keys for the completion popup (design 3.1: it takes Up, Down, Enter, Tab and Esc); `true` when used.
    fn completion_key(&mut self, ctx: &egui::Context, e: &egui::Event) -> bool {
        let Some(comp) = &self.completion else { return false };
        let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { return false };
        if modifiers.any() {
            return false;
        }
        let prefix = self.word_at_caret().map(|w| w.2).unwrap_or_default();
        let items = rank(&comp.names, &prefix);
        match key {
            Key::ArrowDown if !items.is_empty() => {
                let c = self.completion.as_mut().expect("open");
                c.selected = (c.selected + 1) % items.len();
                true
            }
            Key::ArrowUp if !items.is_empty() => {
                let c = self.completion.as_mut().expect("open");
                c.selected = (c.selected + items.len() - 1) % items.len();
                true
            }
            Key::Enter | Key::Tab if !items.is_empty() => {
                let name = items[comp.selected.min(items.len() - 1)].0.clone();
                let (cell, start) = (comp.cell, comp.start);
                self.completion = None;
                if let Selection::Caret { pos, .. } = *self.nb.selection() {
                    self.run(ctx, Command::ClickText(cell, start, false));
                    self.run(ctx, Command::ClickText(cell, pos, true));
                    self.run(ctx, Command::Insert(name));
                }
                true
            }
            Key::Escape => {
                self.completion = None;
                true
            }
            _ => false,
        }
    }

    /// Keep the popup in step with the caret, take the kernel's names, draw it.
    fn completion_popup(&mut self, ctx: &egui::Context) {
        if let Some(at) = self.auto_complete_at {
            let now = ctx.input(|i| i.time);
            if now >= at {
                self.auto_complete_at = None;
                if self.word_at_caret().is_some_and(|w| w.2.chars().count() >= 2) {
                    self.start_completion();
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(at - now));
            }
        }
        if let Some(names) = self.kernel.names() {
            self.view.set_defined(Some(names.iter().map(|n| n.0.clone()).collect()));
            if let Some(c) = &mut self.completion {
                c.names.extend(names);
            }
        }
        let Some(comp) = &self.completion else { return };
        let word = self.word_at_caret();
        let Some((cell, start, prefix)) = word.filter(|w| w.0 == comp.cell && w.1 == comp.start) else {
            self.completion = None;
            return;
        };
        let items = rank(&comp.names, &prefix);
        let pos = start + prefix.len();
        let Some(r) = self.view.caret_rect(ctx, &self.nb, cell, pos) else { return };
        let at = self.view.origin + r.left_bottom().to_vec2() + vec2(0.0, 2.0);
        let selected = comp.selected.min(items.len().saturating_sub(1));
        let p = self.view.palette.clone();
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
                    for (k, (name, kind)) in items.iter().take(12).enumerate() {
                        let text = RichText::new(format!("{name:<24} {kind}")).monospace();
                        if ui.selectable_label(k == selected, text).clicked() {
                            clicked = Some(k);
                        }
                    }
                });
            },
        );
        if let Some(k) = clicked {
            if let Some(c) = &mut self.completion {
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
    fn find_next(&mut self, ctx: &egui::Context, forward: bool) {
        let Some(needle) = self.find.clone().filter(|n| !n.is_empty()) else { return };
        let cells = self.nb.cells();
        let n = cells.len();
        if n == 0 {
            return;
        }
        let (start_cell, start_pos) = match self.nb.selection() {
            Selection::Caret { cell, anchor, pos } => {
                let i = self.nb.index_of(*cell).unwrap_or(0);
                (i, if forward { *anchor.max(pos) } else { *anchor.min(pos) })
            }
            Selection::Cells { ids, .. } => (ids.first().and_then(|id| self.nb.index_of(*id)).unwrap_or(0), 0),
            Selection::Gap(g) => {
                (if forward { (*g).min(n - 1) } else { g.saturating_sub(1) }, if forward { 0 } else { usize::MAX })
            }
        };
        let mut found = None;
        for k in 0..=n {
            let i = if forward { (start_cell + k) % n } else { (start_cell + n - k % n) % n };
            let t = &cells[i].text;
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
                if self.nb.is_visible(i) {
                    found = Some((cells[i].id, p));
                    break;
                }
            }
        }
        match found {
            Some((id, p)) => {
                self.run(ctx, Command::ClickText(id, p, false));
                self.run(ctx, Command::ClickText(id, p + needle.len(), true));
                self.status.clear();
            }
            None => {
                self.status = format!("\u{201c}{needle}\u{201d} not found");
                beep();
            }
        }
    }

    fn find_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let Some(mut text) = self.find.clone() else { return };
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
            if ui.button("Next").on_hover_text("F3").clicked() {
                action = Some(true);
            }
            if ui.button("Previous").on_hover_text("Shift+F3").clicked() {
                action = Some(false);
            }
            if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) && edit.has_focus() {
                self.find = None;
                return;
            }
            self.find = Some(text.clone());
        });
        if let Some(forward) = action {
            self.find_next(&ctx, forward);
        }
    }

    fn modal_open(&self) -> bool {
        self.other_style.is_some()
            || self.ask_init
            || self.interrupt
            || self.confirm.is_some()
            || self.error.is_some()
            || self.about
    }

    // ----- the canvas -----

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut area = ScrollArea::vertical().auto_shrink([false, false]).id_salt("notebook");
        if self.scroll_by != 0.0 {
            area = area.vertical_scroll_offset((self.offset + self.scroll_by).max(0.0));
            self.scroll_by = 0.0;
        }
        let out = area.show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            if (self.view.width - width).abs() > 0.5 {
                self.view.width = width;
            }
            self.view.update_positions(&ctx, &self.nb, viewport.min.y..viewport.max.y);
            let total = self.view.total_height() + (viewport.height() * 0.4).max(80.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(width, total), Sense::click_and_drag());
            let origin = rect.min;
            self.mouse(&ctx, &resp, origin);
            resp.context_menu(|ui| self.context_menu(ui));
            let time = ctx.input(|i| i.time);
            let clip = ui.clip_rect();
            self.view.caret = match self.nb.selection() {
                Selection::Caret { cell, anchor, pos } if anchor == pos => Some((*cell, *pos)),
                _ => None,
            };
            self.view.paint(&ctx, ui.painter(), &self.nb, origin, clip, time);
            if std::mem::take(&mut self.scroll_pending) {
                if let Some(r) = self.selection_rect(&ctx) {
                    ui.scroll_to_rect(r.translate(origin.to_vec2()).expand(8.0), None);
                }
            }
        });
        self.offset = out.state.clamped_offset().y;
    }

    /// The rectangle (canvas coordinates) the selection occupies.
    fn selection_rect(&mut self, ctx: &egui::Context) -> Option<egui::Rect> {
        match self.nb.selection().clone() {
            Selection::Caret { cell, pos, .. } => self.view.caret_rect(ctx, &self.nb, cell, pos),
            Selection::Gap(g) => {
                let y = self.view.gap_y(g);
                Some(egui::Rect::from_min_max(egui::pos2(0.0, y - 10.0), egui::pos2(10.0, y + 10.0)))
            }
            Selection::Cells { ids, .. } => {
                let i = self.nb.index_of(*ids.first()?)?;
                let r = self.view.cell_rect(i);
                Some(r.intersect(egui::Rect::from_min_size(r.min, vec2(r.width(), 40.0))))
            }
        }
    }

    fn mouse(&mut self, ctx: &egui::Context, resp: &egui::Response, origin: egui::Pos2) {
        let (pointer, pressed, released, secondary, mods, time) = ctx.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.secondary_pressed(),
                i.modifiers,
                i.time,
            )
        });
        let Some(screen) = pointer else { return };
        let p = (screen - origin).to_pos2();
        if resp.hovered() {
            self.view.hover = match self.view.hit(ctx, &self.nb, p) {
                Hit::Bracket(b) => Some(b),
                _ => None,
            };
        } else {
            self.view.hover = None;
        }
        if secondary && resp.hovered() {
            // a right-click selects what it is on, unless that is already selected
            match self.view.hit(ctx, &self.nb, p) {
                Hit::Bracket(b) if !self.bracket_selected(b) => self.run(ctx, Command::ClickBracket(b, false, false)),
                Hit::Text(id, pos) => {
                    let inside = matches!(self.nb.selection(), Selection::Caret { cell, anchor, pos: q } if *cell == id && anchor != q);
                    if !inside {
                        self.run(ctx, Command::ClickText(id, pos, false));
                    }
                }
                _ => {}
            }
        }
        if pressed && resp.hovered() {
            let (t0, p0, n0) = self.last_click;
            let count = if time - t0 < DOUBLE_CLICK && (screen - p0).length() < 5.0 { n0 + 1 } else { 1 };
            self.last_click = (time, screen, count);
            match self.view.hit(ctx, &self.nb, p) {
                Hit::Text(id, pos) => {
                    let cmd = match count {
                        1 => Command::ClickText(id, pos, mods.shift),
                        2 => Command::SelectWord(id, pos),
                        _ => Command::SelectLine(id, pos),
                    };
                    self.run(ctx, cmd);
                    self.drag = Some(Drag::Text);
                }
                Hit::Bracket(b) => {
                    if count >= 2 {
                        self.run(ctx, Command::DoubleClickBracket(b));
                        self.drag = None;
                    } else if self.bracket_selected(b) && !mods.shift && !mods.ctrl {
                        self.drag = Some(Drag::Cells(screen));
                    } else {
                        self.run(ctx, Command::ClickBracket(b, mods.shift, mods.ctrl));
                        self.drag = Some(Drag::Brackets);
                    }
                }
                Hit::Gap(g) => {
                    self.run(ctx, Command::ClickGap(g));
                    self.drag = None;
                }
            }
        } else if resp.dragged() {
            match self.drag {
                Some(Drag::Text) => {
                    if let Hit::Text(id, pos) = self.view.hit(ctx, &self.nb, p) {
                        if !matches!(self.nb.selection(), Selection::Caret { cell, pos: q, .. } if *cell == id && *q == pos)
                        {
                            self.run(ctx, Command::ClickText(id, pos, true));
                        }
                    }
                }
                Some(Drag::Brackets) => {
                    if let Some(i) = self.cell_at_y(p.y) {
                        let id = self.nb.cells()[i].id;
                        let b = Bracket::Cell(id);
                        if !self.bracket_selected(b) {
                            self.run(ctx, Command::ClickBracket(b, true, false));
                        }
                    }
                }
                Some(Drag::Cells(start)) if (screen - start).length() > 4.0 => {
                    self.view.drop_gap = Some(self.gap_at_y(p.y));
                }
                _ => {}
            }
        }
        if released {
            if let (Some(Drag::Cells(_)), Some(g)) = (self.drag, self.view.drop_gap.take()) {
                self.run(ctx, Command::MoveCells(g, mods.ctrl));
            } else if let Some(Drag::Cells(_)) = self.drag {
                // a plain click on a selected bracket selects just it
                if let Hit::Bracket(b) = self.view.hit(ctx, &self.nb, p) {
                    self.run(ctx, Command::ClickBracket(b, false, false));
                }
            }
            self.drag = None;
        }
    }

    fn bracket_selected(&self, b: Bracket) -> bool {
        let Selection::Cells { ids, .. } = self.nb.selection() else { return false };
        match b {
            Bracket::Cell(id) => ids.contains(&id),
            Bracket::Group(s, e) => (s..e).all(|i| self.nb.cells().get(i).is_some_and(|c| ids.contains(&c.id))),
        }
    }

    fn cell_at_y(&self, y: f32) -> Option<usize> {
        (0..self.nb.cells().len()).find(|&i| {
            self.nb.is_visible(i) && {
                let r = self.view.cell_rect(i);
                y >= r.top() && y < r.bottom()
            }
        })
    }

    /// The insertion point nearest to `y` (between visible cells).
    fn gap_at_y(&self, y: f32) -> usize {
        let n = self.nb.cells().len();
        match self.cell_at_y(y) {
            Some(i) => {
                let r = self.view.cell_rect(i);
                if y < r.center().y {
                    i
                } else {
                    let mut j = i + 1;
                    while j < n && !self.nb.is_visible(j) {
                        j += 1;
                    }
                    j
                }
            }
            None => {
                if y < 10.0 {
                    0
                } else {
                    n
                }
            }
        }
    }

    fn context_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut fire = None;
        let cells = matches!(self.nb.selection(), Selection::Cells { .. });
        let items: Vec<(&str, Command)> = vec![
            ("Cut", Command::Cut),
            ("Copy", Command::Copy),
            ("Copy as plain text", Command::CopyPlain),
            ("Paste", Command::Paste),
            ("Evaluate cells", Command::Evaluate),
        ];
        for (label, cmd) in items {
            self.menu_item(ui, label, cmd, &mut fire);
        }
        ui.separator();
        self.style_menu(ui, &mut fire);
        self.menu_item(ui, "Divide cell", Command::Divide, &mut fire);
        if cells {
            self.menu_item(ui, "Merge cells", Command::Merge, &mut fire);
            self.menu_item(ui, "Group together", Command::GroupTogether, &mut fire);
            self.menu_item(ui, "Ungroup", Command::Ungroup, &mut fire);
            self.menu_item(ui, "Delete cells", Command::Delete, &mut fire);
        }
        self.menu_item(ui, "Open/close group", Command::ToggleGroup, &mut fire);
        self.menu_item(ui, "Show expression", Command::ShowExpression, &mut fire);
        if let Some(c) = fire {
            self.run(&ctx, c);
        }
    }

    // ----- chrome -----

    fn menu_item(&self, ui: &mut egui::Ui, label: &str, cmd: Command, fire: &mut Option<Command>) {
        let mut b = egui::Button::new(label);
        if let Some(k) = self.keys.chord_of(&cmd) {
            b = b.shortcut_text(k);
        }
        if ui.add(b).clicked() {
            *fire = Some(cmd);
            ui.close();
        }
    }

    fn style_menu(&self, ui: &mut egui::Ui, fire: &mut Option<Command>) {
        ui.menu_button("Style", |ui| {
            for s in Style::ALL.into_iter().filter(|s| !s.generated()) {
                self.menu_item(ui, s.info().name, Command::SetStyle(s), fire);
            }
            ui.separator();
            self.menu_item(ui, "Other...", Command::App(AppAction::OtherStyle), fire);
        });
    }

    fn menus(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut fire: Option<Command> = None;
        let mut local: Option<&'static str> = None;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                self.menu_item(ui, "New", Command::App(AppAction::New), &mut fire);
                self.menu_item(ui, "Open...", Command::App(AppAction::Open), &mut fire);
                ui.menu_button("Open recent", |ui| {
                    for p in self.settings.recent.clone() {
                        if ui.button(p.display().to_string()).clicked() {
                            self.open_path(p);
                            ui.close();
                        }
                    }
                });
                ui.separator();
                self.menu_item(ui, "Save", Command::App(AppAction::Save), &mut fire);
                self.menu_item(ui, "Save as...", Command::App(AppAction::SaveAs), &mut fire);
                if ui.checkbox(&mut self.settings.save_without_outputs, "Save without outputs").changed() {
                    self.settings.save();
                }
                ui.menu_button("Export", |ui| {
                    if ui.button("Script (.mc)...").clicked() {
                        local = Some("export");
                        ui.close();
                    }
                });
                ui.separator();
                self.menu_item(ui, "Close", Command::App(AppAction::Close), &mut fire);
            });
            ui.menu_button("Edit", |ui| {
                self.menu_item(ui, "Undo", Command::Undo, &mut fire);
                self.menu_item(ui, "Redo", Command::Redo, &mut fire);
                ui.separator();
                self.menu_item(ui, "Cut", Command::Cut, &mut fire);
                self.menu_item(ui, "Copy", Command::Copy, &mut fire);
                self.menu_item(ui, "Copy as plain text", Command::CopyPlain, &mut fire);
                self.menu_item(ui, "Paste", Command::Paste, &mut fire);
                self.menu_item(ui, "Clear", Command::Delete, &mut fire);
                ui.separator();
                self.menu_item(ui, "Select all", Command::SelectAll, &mut fire);
            });
            ui.menu_button("Insert", |ui| {
                self.menu_item(ui, "Input from above", Command::InputFromAbove, &mut fire);
                self.menu_item(ui, "Output from above", Command::OutputFromAbove, &mut fire);
                self.menu_item(ui, "Cell with same style", Command::SameStyleBelow, &mut fire);
                ui.separator();
                self.menu_item(ui, "New Input cell", Command::NewCell(Style::Input), &mut fire);
                self.menu_item(ui, "New Text cell", Command::NewCell(Style::Text), &mut fire);
            });
            ui.menu_button("Format", |ui| {
                self.style_menu(ui, &mut fire);
                ui.menu_button("Theme", |ui| {
                    for (c, name) in [
                        (ThemeChoice::System, "System"),
                        (ThemeChoice::Light, "One Light"),
                        (ThemeChoice::Dark, "One Dark"),
                    ] {
                        if ui.radio(self.settings.theme == c, name).clicked() {
                            self.settings.theme = c;
                            self.settings.save();
                            ui.close();
                        }
                    }
                });
                ui.menu_button("Magnification", |ui| {
                    for z in ZOOMS {
                        if ui.radio((self.view.zoom - z).abs() < 0.01, format!("{:.0} %", z * 100.0)).clicked() {
                            self.set_zoom(z);
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button("Cell", |ui| {
                ui.menu_button("Cell properties", |ui| {
                    let c = self.nb.current_cell().and_then(|id| self.nb.cell(id)).cloned();
                    let flag = |on: Option<bool>| if on.unwrap_or(false) { "✔ " } else { "    " };
                    let props: [(&str, Option<bool>, Command); 4] = [
                        ("Editable", c.as_ref().map(|c| c.editable()), Command::ToggleEditable),
                        ("Evaluatable", c.as_ref().map(|c| c.evaluatable()), Command::ToggleEvaluatable),
                        ("Deletable", c.as_ref().map(|c| c.deletable()), Command::ToggleDeletable),
                        ("Initialization cell", c.as_ref().map(|c| c.initialization()), Command::ToggleInitialization),
                    ];
                    for (name, on, cmd) in props {
                        self.menu_item(ui, &format!("{}{name}", flag(on)), cmd, &mut fire);
                    }
                });
                ui.menu_button("Grouping", |ui| {
                    self.menu_item(ui, "Group together", Command::GroupTogether, &mut fire);
                    self.menu_item(ui, "Ungroup", Command::Ungroup, &mut fire);
                    ui.separator();
                    let manual = self.nb.manual_grouping();
                    self.menu_item(
                        ui,
                        if manual { "    Automatic grouping" } else { "✔ Automatic grouping" },
                        Command::SetManualGrouping(false),
                        &mut fire,
                    );
                    self.menu_item(
                        ui,
                        if manual { "✔ Manual grouping" } else { "    Manual grouping" },
                        Command::SetManualGrouping(true),
                        &mut fire,
                    );
                    ui.separator();
                    self.menu_item(ui, "Open all subgroups", Command::OpenAll, &mut fire);
                    self.menu_item(ui, "Close all subgroups", Command::CloseAll, &mut fire);
                    self.menu_item(ui, "Open/close group", Command::ToggleGroup, &mut fire);
                });
                ui.separator();
                self.menu_item(ui, "Divide cell", Command::Divide, &mut fire);
                self.menu_item(ui, "Merge cells", Command::Merge, &mut fire);
                self.menu_item(ui, "Delete all output", Command::DeleteAllOutput, &mut fire);
                self.menu_item(ui, "Show expression", Command::ShowExpression, &mut fire);
            });
            ui.menu_button("Evaluation", |ui| {
                self.menu_item(ui, "Evaluate cells", Command::Evaluate, &mut fire);
                self.menu_item(ui, "Evaluate notebook", Command::EvaluateNotebook, &mut fire);
                self.menu_item(ui, "Evaluate initialization cells", Command::EvaluateInitialization, &mut fire);
                ui.separator();
                self.menu_item(ui, "Abort evaluation", Command::Abort, &mut fire);
                self.menu_item(ui, "Remove from queue", Command::Unqueue, &mut fire);
                self.menu_item(ui, "Interrupt...", Command::App(AppAction::InterruptDialog), &mut fire);
                if ui.button("Find evaluating cell").clicked() {
                    local = Some("find-running");
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit kernel").clicked() {
                    local = Some("quit");
                    ui.close();
                }
                if ui.button("Restart kernel").clicked() {
                    local = Some("restart");
                    ui.close();
                }
                if ui.button("Kernel log").clicked() {
                    self.kernel_log = true;
                    ui.close();
                }
            });
            ui.menu_button("Window", |ui| {
                if ui.checkbox(&mut self.settings.show_toolbar, "Show toolbar").changed() {
                    self.settings.save();
                }
                self.menu_item(ui, "Full screen", Command::App(AppAction::FullScreen), &mut fire);
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Keyboard shortcuts").clicked() {
                    self.keyboard_help = true;
                    ui.close();
                }
                if ui.button("About").clicked() {
                    self.about = true;
                    ui.close();
                }
            });
        });
        match local {
            Some("export") => self.export_script(),
            Some("quit") | Some("restart") => self.run(&ctx, Command::QuitKernel),
            Some("find-running") => {
                if let Some(id) = self.nb.running_cell() {
                    self.run(&ctx, Command::ClickBracket(Bracket::Cell(id), false, false));
                }
            }
            _ => {}
        }
        if let Some(c) = fire {
            self.run(&ctx, c);
        }
    }

    fn kernel_indicator(&self) -> (Color32, String) {
        let p = &self.view.palette;
        match self.nb.kernel_state() {
            KernelState::NotStarted => (p.gutter, "Kernel: not started".into()),
            KernelState::Idle => (p.green, "Kernel: idle".into()),
            KernelState::Busy => (p.yellow, format!("Kernel: busy ({} queued)", self.nb.queue_len())),
            KernelState::Stopped => (p.red, "Kernel: stopped".into()),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut fire = None;
        ui.horizontal_centered(|ui| {
            let current = match self.nb.selection() {
                Selection::Gap(_) => self.nb.gap_style(),
                _ => self.nb.current_cell().and_then(|id| self.nb.cell(id)).map_or(Style::Input, |c| c.style),
            };
            egui::ComboBox::from_id_salt("style").selected_text(current.info().name).width(140.0).show_ui(ui, |ui| {
                for s in Style::ALL.into_iter().filter(|s| !s.generated()) {
                    let label = match s.info().key {
                        Some(k) => format!("{}   Alt+{k}", s.info().name),
                        None => s.info().name.to_string(),
                    };
                    if ui.selectable_label(s == current, label).clicked() {
                        fire = Some(Command::SetStyle(s));
                    }
                }
            });
            ui.separator();
            if ui.button("▶ Evaluate").on_hover_text("Evaluate the selected cells (Shift+Enter)").clicked() {
                fire = Some(Command::Evaluate);
            }
            let busy = self.nb.kernel_state() == KernelState::Busy;
            if ui
                .add_enabled(busy, egui::Button::new("■ Abort"))
                .on_hover_text("Abort the evaluation (Alt+.)")
                .clicked()
            {
                fire = Some(Command::Abort);
            }
            ui.separator();
            let (color, text) = self.kernel_indicator();
            let (r, resp) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::click());
            ui.painter().circle_filled(r.center(), 5.0, color);
            let resp = resp.on_hover_text(format!("{text}\nClick: kernel menu"));
            egui::Popup::menu(&resp).show(|ui| {
                if ui.button("Quit kernel").clicked() {
                    fire = Some(Command::QuitKernel);
                }
                if ui.button("Kernel log").clicked() {
                    self.kernel_log = true;
                }
            });
            ui.label(RichText::new(text).color(self.view.palette.weak));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt("zoom")
                    .selected_text(format!("{:.0} %", self.view.zoom * 100.0))
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for z in ZOOMS {
                            if ui
                                .selectable_label((self.view.zoom - z).abs() < 0.01, format!("{:.0} %", z * 100.0))
                                .clicked()
                            {
                                self.set_zoom(z);
                            }
                        }
                    });
                let icon = match self.settings.theme {
                    ThemeChoice::System => "◐ System",
                    ThemeChoice::Light => "☀ Light",
                    ThemeChoice::Dark => "☾ Dark",
                };
                if ui.button(icon).on_hover_text("Theme: System, One Light, One Dark").clicked() {
                    self.settings.theme = match self.settings.theme {
                        ThemeChoice::System => ThemeChoice::Light,
                        ThemeChoice::Light => ThemeChoice::Dark,
                        ThemeChoice::Dark => ThemeChoice::System,
                    };
                    self.settings.save();
                }
            });
        });
        if let Some(c) = fire {
            self.run(&ctx, c);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let weak = self.view.palette.weak;
        // the file name first, from the right, so the items on the left get the remaining width and are truncated
        // instead of running into it
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let name = self.path.as_ref().map_or("Untitled".to_string(), |p| p.display().to_string());
            let m = if self.nb.is_modified() { " (modified)" } else { "" };
            ui.add(egui::Label::new(RichText::new(format!("{name}{m}")).color(weak).small()).truncate());
            ui.add_space(12.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| self.status_items(ui, weak));
        });
    }

    /// The left part of the status bar: kernel, last time, current cell, message.
    fn status_items(&mut self, ui: &mut egui::Ui, weak: egui::Color32) {
        ui.horizontal_centered(|ui| {
            let (color, text) = self.kernel_indicator();
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, color);
            let engine = if self.nb.engine().is_empty() { String::new() } else { format!(" · {}", self.nb.engine()) };
            ui.label(RichText::new(format!("{text}{engine}")).color(weak).small());
            if let Some(ms) = self.nb.last_millis() {
                ui.label(RichText::new(format!("· last {ms} ms")).color(weak).small());
            }
            if let Some(c) = self.nb.current_cell().and_then(|id| self.nb.cell(id)) {
                let mut s = format!("· {}", c.style.info().name);
                if let Some(n) = c.label {
                    s.push_str(&if c.style == Style::Output { format!(" Out[{n}]") } else { format!(" In[{n}]") });
                }
                if let Selection::Caret { pos, .. } = self.nb.selection() {
                    let before = &c.text[..(*pos).min(c.text.len())];
                    let line = before.matches('\n').count() + 1;
                    let col = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                    s.push_str(&format!(" · {line}:{col}"));
                }
                ui.label(RichText::new(s).color(weak).small());
            }
            if !self.status.is_empty() {
                ui.add(egui::Label::new(RichText::new(format!("· {}", self.status)).color(weak).small()).truncate());
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let center = |w: egui::Window<'static>| {
            w.collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        };
        if let Some(mut filter) = self.other_style.clone() {
            let mut chosen = None;
            let mut close = false;
            center(egui::Window::new("Other style")).show(ctx, |ui| {
                let edit = ui.add(egui::TextEdit::singleline(&mut filter).hint_text("type to filter"));
                edit.request_focus();
                let matches: Vec<Style> = Style::ALL
                    .into_iter()
                    .filter(|s| !s.generated() && s.info().name.to_lowercase().contains(&filter.to_lowercase()))
                    .collect();
                ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for s in &matches {
                        if ui.selectable_label(false, s.info().name).clicked() {
                            chosen = Some(*s);
                        }
                    }
                });
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    chosen = matches.first().copied();
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) || ui.button("Cancel").clicked() {
                    close = true;
                }
            });
            self.other_style = if chosen.is_some() || close { None } else { Some(filter) };
            if let Some(s) = chosen {
                self.run(ctx, Command::SetStyle(s));
            }
        }
        if self.ask_init {
            let mut answer = None;
            center(egui::Window::new("Initialization cells")).show(ctx, |ui| {
                ui.label("Evaluate the initialization cells first?");
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() {
                        answer = Some(InitAnswer::Yes);
                    }
                    if ui.button("No").clicked() {
                        answer = Some(InitAnswer::No);
                    }
                    if ui.button("Always for this notebook").clicked() {
                        answer = Some(InitAnswer::Always);
                    }
                });
            });
            if let Some(a) = answer {
                self.ask_init = false;
                self.run(ctx, Command::InitAnswer(a));
            }
        }
        if self.interrupt {
            let mut fire = None;
            center(egui::Window::new("Interrupt")).show(ctx, |ui| {
                ui.label("The evaluation is interrupted.");
                ui.horizontal(|ui| {
                    if ui.button("Abort").clicked() {
                        fire = Some(true);
                    }
                    if ui.button("Continue").clicked() {
                        fire = Some(false);
                    }
                });
            });
            if let Some(abort) = fire {
                self.interrupt = false;
                if abort {
                    self.run(ctx, Command::Abort);
                }
            }
        }
        if let Some(pending) = self.confirm.clone() {
            let mut answer = None;
            center(egui::Window::new("Unsaved changes")).show(ctx, |ui| {
                ui.label("Save the changes to this notebook?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        answer = Some(0);
                    }
                    if ui.button("Don't save").clicked() {
                        answer = Some(1);
                    }
                    if ui.button("Cancel").clicked() {
                        answer = Some(2);
                    }
                });
            });
            if let Some(a) = answer {
                self.confirm = None;
                let go = match a {
                    0 => self.save(),
                    1 => true,
                    _ => false,
                };
                if go {
                    match pending {
                        Pending::Close => {
                            self.confirmed_close = true;
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                    }
                }
            }
        }
        if let Some(e) = self.error.clone() {
            center(egui::Window::new("MathCAS")).show(ctx, |ui| {
                ui.label(e);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
        if self.kernel_log {
            let log = self.kernel.log.lock().map(|l| l.clone()).unwrap_or_default();
            egui::Window::new("Kernel log").open(&mut self.kernel_log).default_size([520.0, 300.0]).show(ctx, |ui| {
                ui.label(format!("Kernel: {}", self.kernel.exe.display()));
                ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    ui.label(RichText::new(if log.is_empty() { "(empty)" } else { &log }).monospace());
                });
            });
        }
        if self.keyboard_help {
            let keys = &self.keys;
            egui::Window::new("Keyboard shortcuts").open(&mut self.keyboard_help).default_size([520.0, 480.0]).show(
                ctx,
                |ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("keys").striped(true).show(ui, |ui| {
                            for b in keys.bindings() {
                                ui.label(RichText::new(b.keys).monospace());
                                ui.label(b.command.describe());
                                ui.end_row();
                            }
                        });
                    });
                },
            );
        }
        if self.about {
            center(egui::Window::new("About")).show(ctx, |ui| {
                ui.label(RichText::new("MathCAS Notebook").heading());
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Atom One Light and One Dark colours.");
                if ui.button("OK").clicked() {
                    self.about = false;
                }
            });
        }
    }

    fn update_theme(&mut self, ctx: &egui::Context) {
        let system_dark = ctx.system_theme().is_some_and(|t| t == egui::Theme::Dark);
        let palette = self.settings.theme.palette(system_dark);
        if self.applied.as_ref() != Some(&palette) {
            palette.apply(ctx);
            self.view.palette = palette.clone();
            self.view.clear();
            self.applied = Some(palette);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or("Untitled".into(), |n| n.to_string_lossy().to_string());
        let title = format!("{}{} - MathCAS Notebook", name, if self.nb.is_modified() { " *" } else { "" });
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// The notebook (for the self-test).
    pub fn notebook(&self) -> &Notebook {
        &self.nb
    }

    /// Replace the notebook without a path (for the self-test).
    pub fn load_notebook(&mut self, nb: Notebook) {
        self.set_notebook(nb, None);
    }

    /// Set the theme (for the self-test).
    pub fn set_theme(&mut self, c: ThemeChoice) {
        self.settings.theme = c;
    }

    /// Set the magnification (for the self-test).
    pub fn set_magnification(&mut self, z: f32) {
        self.view.zoom = z;
    }

    /// Close without asking to save (for the self-test).
    pub fn discard_changes(&mut self) {
        self.confirmed_close = true;
    }

    /// Answer the initialization question (for the self-test).
    pub fn answer_init(&mut self, a: InitAnswer, ctx: &egui::Context) {
        self.ask_init = false;
        self.run(ctx, Command::InitAnswer(a));
    }

    /// Scroll the notebook by `dy` points (for the self-test).
    pub fn scroll(&mut self, dy: f32) {
        self.scroll_by += dy;
    }

    /// Save to `path` (for the self-test).
    pub fn save_path(&mut self, path: &Path) -> bool {
        self.save_to(path)
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(mut st) = self.selftest.take() {
            st.inject(self, ctx, raw);
            self.selftest = Some(st);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let start = std::time::Instant::now();
        let ctx = ui.ctx().clone();
        self.update_theme(&ctx);
        let was = self.nb.kernel_state();
        if self.nb.pump(&mut self.kernel) {
            self.scroll_pending |= self.nb.kernel_state() != KernelState::Busy;
        }
        match self.nb.kernel_state() {
            // definitions may have changed: refresh the names that colour undefined globals (design 3.11)
            KernelState::Idle if was == KernelState::Busy => self.kernel.request_names(""),
            KernelState::NotStarted | KernelState::Stopped if was != self.nb.kernel_state() => {
                self.view.set_defined(None)
            }
            _ => {}
        }
        // Ctrl+wheel magnifies the notebook (not the chrome)
        let zoom_delta = ctx.input(|i| if i.modifiers.ctrl { i.zoom_delta() } else { 1.0 });
        if zoom_delta != 1.0 && zoom_delta.is_finite() {
            let z = (self.view.zoom * zoom_delta).clamp(0.5, 3.0);
            self.view.zoom = z;
        }
        self.keyboard(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && self.nb.is_modified() && !self.confirmed_close {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.confirm = Some(Pending::Close);
        }
        let p = self.view.palette.clone();
        egui::Panel::top("menu")
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(6, 2)))
            .show(ui, |ui| self.menus(ui));
        if self.settings.show_toolbar {
            egui::Panel::top("toolbar")
                .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(6, 3)))
                .exact_size(34.0)
                .show(ui, |ui| self.toolbar(ui));
        }
        egui::Panel::bottom("status")
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(8, 0)))
            .exact_size(24.0)
            .show(ui, |ui| self.status_bar(ui));
        if self.find.is_some() {
            egui::Panel::bottom("find")
                .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(8, 3)))
                .exact_size(32.0)
                .show(ui, |ui| self.find_bar(ui));
        }
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| self.canvas(ui));
        self.completion_popup(&ctx);
        self.dialogs(&ctx);
        self.update_title(&ctx);
        if self.nb.kernel_state() == KernelState::Busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        if matches!(self.nb.selection(), Selection::Caret { .. }) {
            // the caret blinks: two repaints a second
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        self.frame_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(mut st) = self.selftest.take() {
            st.after_frame(self, &ctx);
            self.selftest = Some(st);
        }
    }

    fn on_exit(&mut self) {
        self.kernel.stop();
        self.settings.save();
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().to_string())
}

/// Open another window (another process, with its own kernel).
pub fn spawn_window(path: Option<&Path>) {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(exe);
        if let Some(p) = path {
            cmd.arg(p);
        }
        let _ = cmd.spawn();
    }
}

/// The system's "asterisk" sound for a key that cannot act (design 3.2).
fn beep() {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        extern "system" {
            fn MessageBeep(kind: u32) -> i32;
        }
        // MB_ICONASTERISK
        unsafe {
            MessageBeep(0x40);
        }
    }
}
