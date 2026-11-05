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
use mc_nbmodel::tabs::{Answer, Ask, Closed, Tabs};
use mc_nbmodel::{
    AppAction, Bracket, Cell, Clip, Command, Effects, Env, InitAnswer, KernelState, Notebook, Selection, Style,
};
use std::ops::Range;
use std::path::{Path, PathBuf};

/// The application icon (PNG, 256 px): the window's icon and the About dialog's.
pub const ICON_PNG: &[u8] = include_bytes!("../../../assets/app/icon_256.png");
/// The application icon as RGBA pixels.
pub fn icon_data() -> Option<egui::IconData> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(ICON_PNG));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(egui::IconData { rgba, width: info.width, height: info.height })
}

/// Magnifications offered (design 4.10: 50 % - 300 %).
const ZOOMS: [f32; 9] = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

/// The largest texture egui makes (its glyph atlas): see `raw_input_hook`.
const MAX_TEXTURE_SIDE: usize = 4096;
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

/// One notebook of the window (a tab): the document, its canvas state and its own kernel (started lazily on the
/// tab's first evaluation, shut down when the tab closes).
pub struct Doc {
    pub(crate) nb: Notebook,
    pub(crate) view: View,
    pub(crate) kernel: KernelProcess,
    pub(crate) scroll_pending: bool,
    pub(crate) scroll_by: f32,
    /// the scroll offset of the last frame
    pub(crate) offset: f32,
    completion: Option<Completion>,
    /// when to open the completion popup by itself
    auto_complete_at: Option<f64>,
    /// a scroll offset to restore (session) once the canvas shows
    pub(crate) restore_offset: Option<f32>,
}

impl Doc {
    /// A tab holding `nb`, from the file `path` if any.
    pub fn new(nb: Notebook, path: Option<&Path>, ctx: &egui::Context, palette: Palette, zoom: f32) -> Doc {
        let mut nb = nb;
        nb.mark_saved();
        let mut view = View::new(palette);
        view.set_magnification(zoom);
        let mut kernel = KernelProcess::new(default_kernel(), ctx.clone());
        kernel.dir = path.and_then(|p| p.parent().map(Path::to_path_buf));
        kernel.origin = path.map(|p| p.display().to_string());
        Doc {
            nb,
            view,
            kernel,
            // a notebook opens at its top, as in the reference
            scroll_pending: false,
            scroll_by: 0.0,
            offset: 0.0,
            completion: None,
            auto_complete_at: None,
            restore_offset: None,
        }
    }

    /// Nothing worth keeping: no cells and no changes (a fresh Untitled tab is replaced by the next file opened).
    fn is_blank(&self) -> bool {
        self.nb.cells().is_empty() && !self.nb.is_modified()
    }
}

/// The application: a window with tabs.
pub struct App {
    /// the active tab's notebook (its slot in `tabs` is empty while it is here)
    pub(crate) doc: Doc,
    /// the tabs in order; `doc` of every tab but the active one holds its notebook
    pub(crate) tabs: Tabs<Option<Doc>>,
    pub(crate) ctx: egui::Context,
    keys: KeyTable,
    clip: Clipboard,
    pub(crate) settings: Settings,
    applied: Option<Palette>,
    pub(crate) status: String,
    last_click: (f64, egui::Pos2, u32),
    drag: Option<Drag>,
    title: String,
    // dialogs
    other_style: Option<String>,
    ask_init: bool,
    interrupt: bool,
    confirmed_close: bool,
    kernel_log: bool,
    keyboard_help: bool,
    about: bool,
    error: Option<String>,
    selftest: Option<SelfTest>,
    /// the find bar's text, when it is open
    find: Option<String>,
    find_focus: bool,
    /// the tab bar: a tab being dragged (index, pointer x where the drag started), the strip's scroll offset
    pub(crate) tab_drag: Option<(usize, f32)>,
    pub(crate) tab_scroll: f32,
    pub(crate) tab_scroll_to_active: bool,
    /// files sent by other launches of the program (single instance)
    instance: Option<crate::instance::Server>,
    /// the text cursor hides while typing and shows again when the pointer moves
    pointer_hidden: bool,
    /// the text being composed with an input method (shown at the caret, not yet in the cell)
    preedit: String,
    /// the insertion point's "+" menu is open at this point (screen coordinates)
    plus_menu: Option<egui::Pos2>,
    /// the window's inner size and outer position of the last frame (remembered in the settings)
    window_rect: Option<(egui::Pos2, egui::Vec2)>,
    pub(crate) icon: Option<egui::TextureHandle>,
    /// the duration of the last frame's work, for the status bar's probe and the self-test
    pub frame_ms: f64,
    /// the GPU the window draws with, and the frames shown so far (the second clears the start's crash mark)
    gpu: Option<eframe::wgpu::AdapterInfo>,
    frames_shown: u64,
    /// Ctrl+wheel turns not yet taken as a magnification step
    wheel_zoom: f32,
}

impl App {
    /// The window with the notebooks at `paths` (and the last session's tabs, when that setting is on).
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Vec<PathBuf>, selftest: Option<SelfTest>) -> App {
        let ctx = cc.egui_ctx.clone();
        fonts::install(&ctx);
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let settings = Settings::load();
        let palette = settings.theme.palette(ctx.system_theme().is_some_and(|t| t == egui::Theme::Dark));
        let zoom = settings.magnification;
        let instance = crate::instance::Server::start(&ctx);
        let mut app = App {
            doc: Doc::new(Notebook::new(), None, &ctx, palette, zoom),
            tabs: Tabs::new(),
            ctx: ctx.clone(),
            keys: KeyTable::default(),
            clip: Clipboard::default(),
            settings,
            applied: None,
            status: String::new(),
            last_click: (-1.0, egui::Pos2::ZERO, 0),
            drag: None,
            title: String::new(),
            other_style: None,
            ask_init: false,
            interrupt: false,
            confirmed_close: false,
            kernel_log: false,
            keyboard_help: false,
            about: false,
            error: None,
            selftest,
            find: None,
            find_focus: false,
            tab_drag: None,
            tab_scroll: 0.0,
            tab_scroll_to_active: true,
            instance,
            pointer_hidden: false,
            preedit: String::new(),
            plus_menu: None,
            window_rect: None,
            icon: None,
            frame_ms: 0.0,
            gpu: None,
            frames_shown: 0,
            wheel_zoom: 1.0,
        };
        app.tabs.add(None, None);
        // the self-test restores a session only from a folder of its own (MCNB_CONFIG_DIR)
        let own_config = std::env::var_os("MCNB_CONFIG_DIR").is_some();
        if app.settings.restore_session && (app.selftest.is_none() || own_config) {
            app.restore_session();
        }
        for p in paths {
            app.open(&p);
        }
        app
    }

    // ----- tabs -----

    /// Put the active notebook back into its tab (before the tab list changes).
    fn park(&mut self) {
        let blank = Doc::new(Notebook::new(), None, &self.ctx, self.doc.view.palette.clone(), 1.0);
        let d = std::mem::replace(&mut self.doc, blank);
        if let Some(t) = self.tabs.current_mut() {
            t.doc = Some(d);
        }
    }

    /// Take the active tab's notebook out of the list (after the tab list changed); an empty list gets a new
    /// Untitled tab.
    fn unpark(&mut self) {
        if self.tabs.is_empty() {
            self.tabs.add(None, None);
        }
        let d = self.tabs.current_mut().and_then(|t| t.doc.take());
        self.doc = match d {
            Some(d) => d,
            None => self.blank_doc(),
        };
        // the palette may have changed while the tab was in the background
        if let Some(p) = &self.applied {
            if &self.doc.view.palette != p {
                self.doc.view.palette = p.clone();
                self.doc.view.clear();
            }
        }
        self.tab_switched();
    }

    fn tab_switched(&mut self) {
        self.status.clear();
        self.doc.completion = None;
        self.doc.auto_complete_at = None;
        self.drag = None;
        self.tab_scroll_to_active = true;
        self.doc.view.blink_from = self.ctx.input(|i| i.time);
    }

    fn blank_doc(&self) -> Doc {
        Doc::new(Notebook::new(), None, &self.ctx, self.doc.view.palette.clone(), self.settings.magnification)
    }

    /// Change the tab list with the active notebook parked in it.
    pub(crate) fn with_tabs<R>(&mut self, f: impl FnOnce(&mut Tabs<Option<Doc>>) -> R) -> R {
        self.park();
        let r = f(&mut self.tabs);
        self.unpark();
        r
    }

    /// Make tab `i` active.
    pub fn activate(&mut self, i: usize) {
        if i != self.tabs.active() {
            self.with_tabs(|t| t.select(i));
        }
    }

    /// A new Untitled tab.
    pub fn new_tab(&mut self) {
        let d = self.blank_doc();
        self.with_tabs(|t| t.add(None, Some(d)));
    }

    /// The path of the active tab.
    pub fn path(&self) -> Option<PathBuf> {
        self.tabs.current().and_then(|t| t.path.clone())
    }

    /// Close the tabs with these ids (asking about unsaved changes first).
    pub fn close_tabs(&mut self, ids: &[u64]) {
        self.sync_modified();
        let ids = ids.to_vec();
        let closed = self.with_tabs(|t| t.close(&ids));
        self.closed(closed);
    }

    fn closed(&mut self, c: Closed<Option<Doc>>) {
        // the closed tabs' documents drop here: their kernels get `shutdown` with `interrupt`
        drop(c.docs);
        if let Some(Ask::Save(id)) = &c.ask {
            if let Some(i) = self.tabs.index_of(*id) {
                self.activate(i);
            }
        }
        if c.window {
            self.confirmed_close = true;
            self.ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    /// Answer the unsaved-changes question.
    fn answer_close(&mut self, a: Answer) {
        let a = match (a, self.tabs.asked().cloned()) {
            (Answer::Saved, Some(Ask::Save(id))) => {
                if let Some(i) = self.tabs.index_of(id) {
                    self.activate(i);
                }
                if self.save() {
                    Answer::Saved
                } else {
                    Answer::Cancel
                }
            }
            (Answer::Saved, Some(Ask::SaveAll(ids))) => {
                let mut ok = true;
                for id in ids {
                    if let Some(i) = self.tabs.index_of(id) {
                        self.activate(i);
                        ok = self.save();
                    }
                    if !ok {
                        break;
                    }
                }
                if ok {
                    Answer::Saved
                } else {
                    Answer::Cancel
                }
            }
            (a, _) => a,
        };
        let c = self.with_tabs(|t| t.answer(a));
        self.closed(c);
    }

    /// Reopen the last closed file.
    fn reopen_tab(&mut self) {
        if let Some(p) = self.tabs.reopen() {
            self.open(&p);
        }
    }

    fn sync_modified(&mut self) {
        let modified = self.doc.nb.is_modified();
        if let Some(t) = self.tabs.current_mut() {
            t.modified = modified;
        }
    }

    /// Keep the tabs' modified marks current, let background tabs talk to their kernels, take files sent by other
    /// launches.
    fn sync_tabs(&mut self) {
        self.sync_modified();
        let active = self.tabs.active();
        for (i, t) in self.tabs.iter_mut().enumerate() {
            if i != active {
                if let Some(d) = t.doc.as_mut() {
                    d.nb.pump(&mut d.kernel);
                    t.modified = d.nb.is_modified();
                }
            }
        }
        let sent = self.instance.as_ref().map(|s| s.take()).unwrap_or_default();
        for p in sent {
            self.open(&p);
            self.ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            self.ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    /// The kernel of tab `i` is busy (the tab shows a spinner).
    pub(crate) fn tab_busy(&self, i: usize) -> bool {
        let state = if i == self.tabs.active() {
            self.doc.nb.kernel_state()
        } else {
            match self.tabs.get(i).and_then(|t| t.doc.as_ref()) {
                Some(d) => d.nb.kernel_state(),
                None => return false,
            }
        };
        state == KernelState::Busy
    }

    // ----- session -----

    fn restore_session(&mut self) {
        let session = self.settings.session.clone();
        let mut active = None;
        for (k, s) in session.tabs.iter().enumerate() {
            if !s.path.exists() {
                continue;
            }
            self.open(&s.path);
            if self.path().as_deref() != Some(s.path.as_path()) {
                continue;
            }
            self.doc.restore_offset = Some(s.offset);
            if let Some((i, pos)) = s.caret {
                let target = self.doc.nb.cells().get(i).filter(|c| c.text.is_char_boundary(pos.min(c.text.len())));
                if let Some((id, pos)) = target.map(|c| (c.id, pos.min(c.text.len()))) {
                    let ctx = self.ctx.clone();
                    self.run(&ctx, Command::ClickText(id, pos, false));
                }
            }
            self.doc.scroll_pending = false;
            if k == session.active {
                active = Some(self.tabs.active());
            }
        }
        if let Some(i) = active {
            self.activate(i);
        }
    }

    /// The open file tabs, for the next start.
    fn session(&self) -> crate::settings::Session {
        let mut tabs = vec![];
        let mut active = 0;
        for (i, t) in self.tabs.iter().enumerate() {
            let Some(path) = t.path.clone() else { continue };
            let d = if i == self.tabs.active() { Some(&self.doc) } else { t.doc.as_ref() };
            let Some(d) = d else { continue };
            let caret = match d.nb.selection() {
                Selection::Caret { cell, pos, .. } => d.nb.index_of(*cell).map(|i| (i, *pos)),
                _ => None,
            };
            if i == self.tabs.active() {
                active = tabs.len();
            }
            tabs.push(crate::settings::SessionTab { path, offset: d.offset, caret });
        }
        crate::settings::Session { tabs, active }
    }

    // ----- files -----

    /// Open a notebook: activate its tab when it is open, else a new tab (or this one when it is a blank Untitled).
    pub fn open(&mut self, path: &Path) {
        if let Some(i) = self.tabs.find_path(path) {
            self.activate(i);
            return;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(format!("Cannot read {}: {e}", path.display()));
                return;
            }
        };
        match mc_nbfile::Notebook::read(&text) {
            Ok((f, warnings)) => {
                let zoom =
                    f.header.number::<f32>("magnification").map_or(self.settings.magnification, |m| m.clamp(0.5, 3.0));
                let doc = Doc::new(Notebook::from_file(&f), Some(path), &self.ctx, self.doc.view.palette.clone(), zoom);
                let path = path.to_path_buf();
                if self.doc.is_blank() && self.path().is_none() {
                    self.doc = doc;
                    let i = self.tabs.active();
                    self.tabs.set_path(i, path.clone());
                    self.tab_switched();
                } else {
                    self.with_tabs(|t| t.add(Some(path.clone()), Some(doc)));
                }
                self.status = match warnings.first() {
                    Some(w) => format!("{} warning(s); first: {w}", warnings.len()),
                    None => format!("Opened {}", file_name(&path)),
                };
                self.settings.add_recent(&path);
            }
            Err(e) => self.error = Some(format!("{} is not a notebook: {e}", path.display())),
        }
    }

    /// Replace the active tab's notebook (the old kernel session ends).
    pub fn set_notebook(&mut self, nb: Notebook, path: Option<PathBuf>) {
        let zoom = self.doc.view.zoom;
        self.doc = Doc::new(nb, path.as_deref(), &self.ctx, self.doc.view.palette.clone(), zoom);
        if let Some(t) = self.tabs.current_mut() {
            t.path = path;
            t.modified = false;
        }
        self.tab_switched();
    }

    fn save_to(&mut self, path: &Path) -> bool {
        let text = if self.settings.save_without_outputs {
            let mut copy = self.doc.nb.clone();
            copy.apply(Command::DeleteAllOutput, &mut mc_nbmodel::FakeEnv::default());
            let mut f = copy.to_file();
            f.cells.iter_mut().for_each(|c| c.attrs.remove("label"));
            f.print()
        } else {
            self.doc.nb.to_text()
        };
        let start = std::time::Instant::now();
        match mc_nbfile::write_atomic(path, &text) {
            Ok(()) => {
                self.doc.nb.mark_saved();
                if self.path().as_deref() != Some(path) {
                    self.doc.kernel.dir = path.parent().map(Path::to_path_buf);
                    self.doc.kernel.origin = Some(path.display().to_string());
                    if let Some(t) = self.tabs.current_mut() {
                        t.path = Some(path.to_path_buf());
                    }
                }
                self.sync_modified();
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
        match self.path() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let mut dialog = rfd::FileDialog::new().add_filter("MathCAS notebook", &["mcn"]);
        if let Some(p) = self.path() {
            if let Some(d) = p.parent() {
                dialog = dialog.set_directory(d);
            }
            if let Some(n) = p.file_name() {
                dialog = dialog.set_file_name(n.to_string_lossy());
            }
        } else {
            let name = self.tabs.current().map_or("Untitled".to_string(), |t| t.title());
            dialog = dialog.set_file_name(format!("{name}.mcn"));
        }
        match dialog.save_file() {
            Some(p) => self.save_to(&p),
            None => false,
        }
    }

    fn open_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("MathCAS notebook", &["mcn"]);
        if let Some(d) = self.path().and_then(|p| p.parent().map(Path::to_path_buf)) {
            dialog = dialog.set_directory(d);
        }
        if let Some(ps) = dialog.pick_files() {
            for p in ps {
                self.open(&p);
            }
        }
    }
    fn export_script(&mut self) {
        let dialog = rfd::FileDialog::new().add_filter("MathCAS script", &["mc"]);
        let Some(p) = dialog.save_file() else { return };
        let mut out = String::from("#! mathcas 2\n");
        for c in self.doc.nb.cells() {
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
        let mut env = AppEnv { view: &mut self.doc.view, ctx, clip: &mut self.clip };
        let fx = self.doc.nb.apply(cmd, &mut env);
        // the caret shows at once after every key and click, then blinks
        self.doc.view.blink_from = ctx.input(|i| i.time);
        self.effects(ctx, fx);
    }

    fn effects(&mut self, ctx: &egui::Context, fx: Effects) {
        if fx.scroll {
            self.doc.scroll_pending = true;
            self.doc.view.blink_from = ctx.input(|i| i.time);
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
            AppAction::New => self.new_tab(),
            AppAction::Open => self.open_dialog(),
            AppAction::Save => {
                self.save();
            }
            AppAction::SaveAs => {
                self.save_as();
            }
            AppAction::Close => {
                if let Some(id) = self.tabs.current().map(|t| t.id) {
                    self.close_tabs(&[id]);
                }
            }
            AppAction::NextTab => self.with_tabs(|t| t.next()),
            AppAction::PrevTab => self.with_tabs(|t| t.prev()),
            AppAction::NthTab(n) => self.with_tabs(|t| t.nth(usize::from(n))),
            AppAction::ReopenTab => self.reopen_tab(),
            AppAction::OtherStyle => self.other_style = Some(String::new()),
            AppAction::InterruptDialog => self.interrupt = true,
            AppAction::ScrollLines(n) => self.doc.scroll_by += n as f32 * 20.0 * self.doc.view.zoom,
            AppAction::ScrollPages(n) => self.doc.scroll_by += n as f32 * ctx.content_rect().height() * 0.85,
            AppAction::Zoom(d) => self.step_zoom(d),
            AppAction::FullScreen => {
                let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!full));
            }
            AppAction::Complete => self.start_completion(),
            AppAction::Find => {
                // Ctrl+E's "use the selection" is folded in: a text selection becomes the search text
                let selected = match self.doc.nb.selection() {
                    Selection::Caret { cell, anchor, pos } if anchor != pos => {
                        self.doc.nb.cell(*cell).map(|c| c.text[*anchor.min(pos)..*anchor.max(pos)].to_string())
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
        let i = ZOOMS.iter().position(|&z| z >= self.doc.view.magnification() - 0.01).unwrap_or(2) as i32;
        let j = (i + d).clamp(0, ZOOMS.len() as i32 - 1) as usize;
        self.set_zoom(ZOOMS[j]);
    }

    fn set_zoom(&mut self, z: f32) {
        self.doc.view.set_magnification(z.clamp(0.5, 3.0));
        self.settings.magnification = self.doc.view.magnification();
        self.settings.save();
        self.doc.scroll_pending = true;
    }

    fn context(&self) -> Context {
        mc_nbmodel::script::context(&self.doc.nb)
    }

    /// Keyboard events into commands (unless a text field of a dialog has the keyboard).
    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.modal_open() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        if events.iter().any(|e| matches!(e, egui::Event::Text(_))) {
            self.pointer_hidden = true;
        }
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
                    self.doc.auto_complete_at = None;
                    let Some(chord) = input::chord(key, modifiers) else {
                        continue;
                    };
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

    /// The name being typed at the caret in a code cell: (cell, start, prefix).
    fn word_at_caret(&self) -> Option<(mc_nbmodel::CellId, usize, String)> {
        let Selection::Caret { cell, anchor, pos } = *self.doc.nb.selection() else {
            return None;
        };
        let c = self.doc.nb.cell(cell)?;
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
        let Some((cell, start, _)) = self.word_at_caret() else {
            return;
        };
        let mut names: Vec<(String, String)> =
            mc_syntax::KEYWORDS.iter().map(|k| (k.to_string(), "keyword".to_string())).collect();
        if let Some(c) = self.doc.nb.cell(cell) {
            names.extend(mc_syntax::names_in_scope(&c.text, start).into_iter().map(|l| (l.name, "local".to_string())));
        }
        self.doc.kernel.request_names("");
        self.doc.completion = Some(Completion { cell, start, names, selected: 0 });
    }

    /// Keys for the completion popup (design 3.1: it takes Up, Down, Enter, Tab and Esc); `true` when used.
    fn completion_key(&mut self, ctx: &egui::Context, e: &egui::Event) -> bool {
        let Some(comp) = &self.doc.completion else {
            return false;
        };
        let egui::Event::Key { key, pressed: true, modifiers, .. } = e else {
            return false;
        };
        if modifiers.any() {
            return false;
        }
        let prefix = self.word_at_caret().map(|w| w.2).unwrap_or_default();
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
                let name = items[comp.selected.min(items.len() - 1)].0.clone();
                let (cell, start) = (comp.cell, comp.start);
                self.doc.completion = None;
                if let Selection::Caret { pos, .. } = *self.doc.nb.selection() {
                    self.run(ctx, Command::ClickText(cell, start, false));
                    self.run(ctx, Command::ClickText(cell, pos, true));
                    self.run(ctx, Command::Insert(name));
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
    fn completion_popup(&mut self, ctx: &egui::Context) {
        if let Some(at) = self.doc.auto_complete_at {
            let now = ctx.input(|i| i.time);
            if now >= at {
                self.doc.auto_complete_at = None;
                if self.word_at_caret().is_some_and(|w| w.2.chars().count() >= 2) {
                    self.start_completion();
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(at - now));
            }
        }
        if let Some(names) = self.doc.kernel.names() {
            self.doc.view.set_defined(Some(names.iter().map(|n| n.0.clone()).collect()));
            if let Some(c) = &mut self.doc.completion {
                c.names.extend(names);
            }
        }
        let Some(comp) = &self.doc.completion else {
            return;
        };
        let word = self.word_at_caret();
        let Some((cell, start, prefix)) = word.filter(|w| w.0 == comp.cell && w.1 == comp.start) else {
            self.doc.completion = None;
            return;
        };
        let items = rank(&comp.names, &prefix);
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
    fn find_next(&mut self, ctx: &egui::Context, forward: bool) {
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
                if self.doc.nb.is_visible(i) {
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
            || self.tabs.asked().is_some()
            || self.error.is_some()
            || self.about
    }

    // ----- the canvas -----

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut area = ScrollArea::vertical().auto_shrink([false, false]).id_salt("notebook");
        if let Some(off) = self.doc.restore_offset.take() {
            area = area.vertical_scroll_offset(off.max(0.0));
        } else if self.doc.scroll_by != 0.0 {
            area = area.vertical_scroll_offset((self.doc.offset + self.doc.scroll_by).max(0.0));
            self.doc.scroll_by = 0.0;
            // the caret shows at once after a scroll
            self.doc.view.blink_from = ctx.input(|i| i.time);
        }
        let out = area.show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            if (self.doc.view.width - width).abs() > 0.5 {
                self.doc.view.width = width;
            }
            self.doc.view.update_positions(&ctx, &self.doc.nb, viewport.min.y..viewport.max.y);
            let total = self.doc.view.total_height() + (viewport.height() * 0.4).max(80.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(width, total), Sense::click_and_drag());
            let origin = rect.min;
            self.mouse(&ctx, &resp, origin);
            self.auto_scroll(&ctx, ui.clip_rect());
            // a bracket under the pointer names its cell's style (design 2.5)
            let tip = match self.doc.view.hover {
                Some(Bracket::Cell(id)) if self.drag.is_none() => self.doc.nb.cell(id).map(|c| {
                    let mut t = c.style.info().name.to_string();
                    if let Some(n) = c.label {
                        t.push_str(&if c.style == Style::Output {
                            format!("  Out[{n}]")
                        } else {
                            format!("  In[{n}]")
                        });
                    }
                    t
                }),
                Some(Bracket::Group(..)) if self.drag.is_none() => {
                    Some("Group (double-click to open or close)".to_string())
                }
                _ => None,
            };
            let resp = match tip {
                Some(t) => resp.on_hover_text_at_pointer(t),
                None => resp,
            };
            resp.context_menu(|ui| self.context_menu(ui));
            let time = ctx.input(|i| i.time);
            let clip = ui.clip_rect();
            self.doc.view.caret = match self.doc.nb.selection() {
                Selection::Caret { cell, anchor, pos } if anchor == pos => Some((*cell, *pos)),
                _ => None,
            };
            self.doc.view.paint(&ctx, ui.painter(), &self.doc.nb, origin, clip, time);
            self.ime(&ctx, ui.painter(), rect);
            if std::mem::take(&mut self.doc.scroll_pending) {
                if let Some(r) = self.selection_rect(&ctx) {
                    ui.scroll_to_rect(r.translate(origin.to_vec2()).expand(8.0), None);
                }
            }
        });
        self.doc.offset = out.state.clamped_offset().y;
    }

    /// `x` (canvas coordinates) is over the content of cell `id`, not in its margins or label.
    fn over_content(&self, id: mc_nbmodel::CellId, x: f32) -> bool {
        let Some(c) = self.doc.nb.cell(id) else {
            return false;
        };
        let (l, r) = self.doc.view.content_x(c.style);
        x >= l - 4.0 && x <= r + 4.0
    }

    /// Tell the input method where the caret is, and draw the text being composed there (underlined).
    fn ime(&mut self, ctx: &egui::Context, painter: &egui::Painter, canvas: egui::Rect) {
        let Selection::Caret { cell, pos, .. } = *self.doc.nb.selection() else {
            self.preedit.clear();
            return;
        };
        if self.modal_open() {
            return;
        }
        let Some(r) = self.doc.view.caret_rect(ctx, &self.doc.nb, cell, pos) else {
            return;
        };
        let r = r.translate(self.doc.view.origin.to_vec2());
        ctx.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                purpose: egui::IMEPurpose::Normal,
                rect: canvas,
                cursor_rect: egui::Rect::from_min_size(r.min, vec2(1.0, r.height())),
                should_interrupt_composition: false,
            })
        });
        if self.preedit.is_empty() {
            return;
        }
        let font = self.doc.nb.cell(cell).map_or(crate::fonts::Face::Code.font(13.0), |c| {
            let lk = crate::view::look(c, &self.doc.view.palette);
            lk.face.font(lk.size * self.doc.view.zoom)
        });
        let p = &self.doc.view.palette;
        let g = painter.layout_no_wrap(self.preedit.clone(), font, p.text);
        let at = egui::pos2(r.left(), r.top());
        let bg = egui::Rect::from_min_size(at, vec2(g.size().x, r.height().max(g.size().y))).expand2(vec2(2.0, 0.0));
        painter.rect_filled(bg, 2.0, p.card);
        painter.galley(at, g.clone(), p.text);
        painter.hline(at.x..=at.x + g.size().x, bg.bottom() - 1.0, egui::Stroke::new(1.0, p.accent));
        painter.vline(at.x + g.size().x + 1.0, r.top()..=r.bottom(), egui::Stroke::new(2.0, p.cursor));
    }

    /// The insertion point's "+" menu (design 2.7): Input, Text, Other style.
    fn plus_popup(&mut self, ctx: &egui::Context) {
        let Some(at) = self.plus_menu else { return };
        let mut fire = None;
        let mut close = false;
        let area = egui::Area::new(egui::Id::unique("mc-notebook-plus"))
            .fixed_pos(at + vec2(4.0, 8.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(170.0);
                    for (label, cmd, key) in [
                        ("Input", Command::NewCell(Style::Input), "Alt+9"),
                        ("Text", Command::NewCell(Style::Text), "Alt+7"),
                        ("Other style...", Command::App(AppAction::OtherStyle), "Alt+0"),
                    ] {
                        if ui.add(egui::Button::new(label).shortcut_text(key).frame(false)).clicked() {
                            fire = Some(cmd);
                            close = true;
                        }
                    }
                });
            });
        if area.response.clicked_elsewhere() || ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        if close {
            self.plus_menu = None;
        }
        if let Some(c) = fire {
            self.run(ctx, c);
        }
    }

    /// Dragging a selection past the top or bottom edge scrolls, faster the further the pointer is.
    fn auto_scroll(&mut self, ctx: &egui::Context, clip: egui::Rect) {
        if !matches!(self.drag, Some(Drag::Text) | Some(Drag::Brackets) | Some(Drag::Cells(_))) {
            return;
        }
        let Some(p) = ctx.input(|i| if i.pointer.primary_down() { i.pointer.latest_pos() } else { None }) else {
            return;
        };
        let d = if p.y < clip.top() + 4.0 {
            p.y - (clip.top() + 4.0)
        } else if p.y > clip.bottom() - 4.0 {
            p.y - (clip.bottom() - 4.0)
        } else {
            0.0
        };
        if d != 0.0 {
            self.doc.scroll_by += (d * 0.35).clamp(-40.0, 40.0);
            ctx.request_repaint();
        }
    }

    /// The rectangle (canvas coordinates) the selection occupies.
    fn selection_rect(&mut self, ctx: &egui::Context) -> Option<egui::Rect> {
        match self.doc.nb.selection().clone() {
            Selection::Caret { cell, pos, .. } => self.doc.view.caret_rect(ctx, &self.doc.nb, cell, pos),
            Selection::Gap(g) => {
                let y = self.doc.view.gap_y(g);
                Some(egui::Rect::from_min_max(egui::pos2(0.0, y - 10.0), egui::pos2(10.0, y + 10.0)))
            }
            Selection::Cells { ids, .. } => {
                let i = self.doc.nb.index_of(*ids.first()?)?;
                let r = self.doc.view.cell_rect(i);
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
        let over = resp.hovered() || resp.dragged();
        let hit = if over { Some(self.doc.view.hit(ctx, &self.doc.nb, p)) } else { None };
        self.doc.view.hover = match hit {
            Some(Hit::Bracket(b)) if resp.hovered() => Some(b),
            _ => None,
        };
        self.doc.view.plus_hover = matches!(hit, Some(Hit::Plus(_))) && resp.hovered();
        if over {
            // the pointer per region (design of the polish package): I-beam over text, the arrow over brackets,
            // margins and the gap between cells, a hand on the "+" button, a closed hand while moving cells; hidden
            // while typing until the mouse moves
            let icon = match (self.drag, hit) {
                (Some(Drag::Text), _) => egui::CursorIcon::Text,
                (Some(Drag::Cells(_)), _) if self.doc.view.drop_gap.is_some() => egui::CursorIcon::Grabbing,
                (_, Some(Hit::Text(id, _))) if self.over_content(id, p.x) => egui::CursorIcon::Text,
                (_, Some(Hit::Plus(_))) => egui::CursorIcon::PointingHand,
                _ => egui::CursorIcon::Default,
            };
            let icon =
                if self.pointer_hidden && icon == egui::CursorIcon::Text { egui::CursorIcon::None } else { icon };
            ctx.set_cursor_icon(icon);
        }
        if secondary && resp.hovered() {
            // a right-click selects what it is on, unless that is already selected
            match self.doc.view.hit(ctx, &self.doc.nb, p) {
                Hit::Bracket(b) if !self.bracket_selected(b) => self.run(ctx, Command::ClickBracket(b, false, false)),
                Hit::Text(id, pos) => {
                    let inside = matches!(self.doc.nb.selection(), Selection::Caret { cell, anchor, pos: q } if *cell == id && anchor != q);
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
            match self.doc.view.hit(ctx, &self.doc.nb, p) {
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
                Hit::Plus(_) => {
                    self.plus_menu = Some(screen);
                    self.drag = None;
                }
            }
        } else if resp.dragged() {
            match self.drag {
                Some(Drag::Text) => {
                    if let Hit::Text(id, pos) = self.doc.view.hit(ctx, &self.doc.nb, p) {
                        if !matches!(self.doc.nb.selection(), Selection::Caret { cell, pos: q, .. } if *cell == id && *q == pos)
                        {
                            self.run(ctx, Command::ClickText(id, pos, true));
                        }
                    }
                }
                Some(Drag::Brackets) => {
                    if let Some(i) = self.cell_at_y(p.y) {
                        let id = self.doc.nb.cells()[i].id;
                        let b = Bracket::Cell(id);
                        if !self.bracket_selected(b) {
                            self.run(ctx, Command::ClickBracket(b, true, false));
                        }
                    }
                }
                Some(Drag::Cells(start)) if (screen - start).length() > 4.0 => {
                    self.doc.view.drop_gap = Some(self.gap_at_y(p.y));
                }
                _ => {}
            }
        }
        if released {
            if let (Some(Drag::Cells(_)), Some(g)) = (self.drag, self.doc.view.drop_gap.take()) {
                self.run(ctx, Command::MoveCells(g, mods.ctrl));
            } else if let Some(Drag::Cells(_)) = self.drag {
                // a plain click on a selected bracket selects just it
                if let Hit::Bracket(b) = self.doc.view.hit(ctx, &self.doc.nb, p) {
                    self.run(ctx, Command::ClickBracket(b, false, false));
                }
            }
            self.drag = None;
        }
    }

    fn bracket_selected(&self, b: Bracket) -> bool {
        let Selection::Cells { ids, .. } = self.doc.nb.selection() else {
            return false;
        };
        match b {
            Bracket::Cell(id) => ids.contains(&id),
            Bracket::Group(s, e) => (s..e).all(|i| self.doc.nb.cells().get(i).is_some_and(|c| ids.contains(&c.id))),
        }
    }

    fn cell_at_y(&self, y: f32) -> Option<usize> {
        (0..self.doc.nb.cells().len()).find(|&i| {
            self.doc.nb.is_visible(i) && {
                let r = self.doc.view.cell_rect(i);
                y >= r.top() && y < r.bottom()
            }
        })
    }

    /// The insertion point nearest to `y` (between visible cells).
    fn gap_at_y(&self, y: f32) -> usize {
        let n = self.doc.nb.cells().len();
        match self.cell_at_y(y) {
            Some(i) => {
                let r = self.doc.view.cell_rect(i);
                if y < r.center().y {
                    i
                } else {
                    let mut j = i + 1;
                    while j < n && !self.doc.nb.is_visible(j) {
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
        let cells = matches!(self.doc.nb.selection(), Selection::Cells { .. });
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
        let mut tab_pick: Option<usize> = None;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                self.menu_item(ui, "New", Command::App(AppAction::New), &mut fire);
                self.menu_item(ui, "Open...", Command::App(AppAction::Open), &mut fire);
                ui.menu_button("Open recent", |ui| {
                    for p in self.settings.recent.clone() {
                        if ui.button(p.display().to_string()).clicked() {
                            self.open(&p);
                            ui.close();
                        }
                    }
                });
                ui.separator();
                self.menu_item(ui, "Save", Command::App(AppAction::Save), &mut fire);
                self.menu_item(ui, "Save as...", Command::App(AppAction::SaveAs), &mut fire);
                if check_item(ui, &mut self.settings.save_without_outputs, "Save without outputs") {
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
                self.menu_item(ui, "Reopen closed notebook", Command::App(AppAction::ReopenTab), &mut fire);
                ui.separator();
                if ui.button("Exit").clicked() {
                    local = Some("exit");
                    ui.close();
                }
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
                        if ui.radio((self.doc.view.magnification() - z).abs() < 0.01, format!("{:.0} %", z * 100.0)).clicked() {
                            self.set_zoom(z);
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button("Cell", |ui| {
                ui.menu_button("Cell properties", |ui| {
                    let c = self.doc.nb.current_cell().and_then(|id| self.doc.nb.cell(id)).cloned();
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
                    let manual = self.doc.nb.manual_grouping();
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
                if check_item(ui, &mut self.settings.show_toolbar, "Show toolbar") {
                    self.settings.save();
                }
                self.menu_item(ui, "Full screen", Command::App(AppAction::FullScreen), &mut fire);
                ui.separator();
                if check_item(ui, &mut self.settings.caret_blink, "Blinking caret") {
                    self.settings.save();
                }
                if check_item(ui, &mut self.settings.restore_session, "Reopen notebooks at start") {
                    self.settings.save();
                }
                ui.separator();
                self.menu_item(ui, "Next notebook", Command::App(AppAction::NextTab), &mut fire);
                self.menu_item(ui, "Previous notebook", Command::App(AppAction::PrevTab), &mut fire);
                ui.separator();
                let active = self.tabs.active();
                let list: Vec<(String, bool)> = self.tabs.iter().map(|t| (t.title(), t.modified)).collect();
                for (k, (title, modified)) in list.into_iter().enumerate() {
                    let mut b =
                        egui::Button::selectable(k == active, if modified { format!("{title}  ●") } else { title });
                    if let Some(key) = [1, 2, 3, 4, 5, 6, 7]
                        .get(k)
                        .and_then(|n| self.keys.chord_of(&Command::App(AppAction::NthTab(*n))))
                    {
                        b = b.shortcut_text(key);
                    }
                    if ui.add(b).clicked() {
                        tab_pick = Some(k);
                        ui.close();
                    }
                }
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
        if let Some(k) = tab_pick {
            self.activate(k);
        }
        match local {
            Some("exit") => ctx.send_viewport_cmd(ViewportCommand::Close),
            Some("export") => self.export_script(),
            Some("quit") | Some("restart") => self.run(&ctx, Command::QuitKernel),
            Some("find-running") => {
                if let Some(id) = self.doc.nb.running_cell() {
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
        let p = &self.doc.view.palette;
        match self.doc.nb.kernel_state() {
            KernelState::NotStarted => (p.gutter, "Kernel: not started".into()),
            KernelState::Idle => (p.green, "Kernel: idle".into()),
            KernelState::Busy => (p.yellow, format!("Kernel: busy ({} queued)", self.doc.nb.queue_len())),
            KernelState::Stopped => (p.red, "Kernel: stopped".into()),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut fire = None;
        let p = self.doc.view.palette.clone();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let current = match self.doc.nb.selection() {
                Selection::Gap(_) => self.doc.nb.gap_style(),
                _ => self.doc.nb.current_cell().and_then(|id| self.doc.nb.cell(id)).map_or(Style::Input, |c| c.style),
            };
            egui::ComboBox::from_id_salt("style")
                .selected_text(current.info().name)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for s in Style::ALL.into_iter().filter(|s| !s.generated()) {
                        let mut b = egui::Button::selectable(s == current, s.info().name);
                        if let Some(k) = s.info().key {
                            b = b.shortcut_text(format!("Alt+{k}"));
                        }
                        if ui.add(b).clicked() {
                            fire = Some(Command::SetStyle(s));
                        }
                    }
                })
                .response
                .on_hover_text("Style of the selected cells (Alt+1 ... Alt+9, Alt+0 for others)");
            ui.add_space(4.0);
            toolbar_separator(ui, &p);
            let tool = |ui: &mut egui::Ui, glyph: &str, glyph_color: Color32, text: &str, enabled: bool, tip: &str| {
                let gc = if enabled { glyph_color } else { p.faint };
                let b = egui::Button::new((RichText::new(glyph).color(gc), RichText::new(text)))
                    .frame_when_inactive(false)
                    .min_size(vec2(0.0, 26.0));
                ui.add_enabled(enabled, b).on_hover_text(tip).on_disabled_hover_text(tip)
            };
            if tool(ui, "▶", p.green, "Evaluate", true, "Evaluate the selected cells (Shift+Enter)").clicked() {
                fire = Some(Command::Evaluate);
            }
            let busy = self.doc.nb.kernel_state() == KernelState::Busy;
            if tool(ui, "■", p.red, "Abort", busy, "Abort the evaluation (Alt+.)").clicked() {
                fire = Some(Command::Abort);
            }
            toolbar_separator(ui, &p);
            // the kernel indicator: a flat button with the state's dot, a menu on click
            let (color, text) = self.kernel_indicator();
            let b = egui::Button::new((RichText::new("●").color(color), RichText::new(text.clone()).color(p.weak)))
                .frame_when_inactive(false)
                .min_size(vec2(0.0, 26.0));
            let resp = ui.add(b).on_hover_text(format!("{text}: click for the kernel menu"));
            egui::Popup::menu(&resp).show(|ui| {
                let running = !matches!(self.doc.nb.kernel_state(), KernelState::NotStarted | KernelState::Stopped);
                if ui.add_enabled(running, egui::Button::new("Quit kernel")).clicked() {
                    fire = Some(Command::QuitKernel);
                }
                if ui.add_enabled(running, egui::Button::new("Restart kernel")).clicked() {
                    fire = Some(Command::QuitKernel);
                }
                ui.separator();
                if ui.button("Kernel log").clicked() {
                    self.kernel_log = true;
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                egui::ComboBox::from_id_salt("zoom")
                    .selected_text(format!("{:.0} %", self.doc.view.magnification() * 100.0))
                    .width(72.0)
                    .show_ui(ui, |ui| {
                        for z in ZOOMS {
                            if ui
                                .selectable_label((self.doc.view.magnification() - z).abs() < 0.01, format!("{:.0} %", z * 100.0))
                                .clicked()
                            {
                                self.set_zoom(z);
                            }
                        }
                    })
                    .response
                    .on_hover_text("Magnification of the notebook (Ctrl+wheel, Ctrl+Alt+= / Ctrl+Alt+-)");
                let (glyph, name) = match self.settings.theme {
                    ThemeChoice::System => ("◐", "System"),
                    ThemeChoice::Light => ("☀", "Light"),
                    ThemeChoice::Dark => ("☾", "Dark"),
                };
                if tool(ui, glyph, p.yellow, name, true, "Theme: System, One Light, One Dark (click to change)")
                    .clicked()
                {
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
        let weak = self.doc.view.palette.weak;
        // the file name first, from the right, so the items on the left get the remaining width and are truncated
        // instead of running into it
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let name = match self.path() {
                Some(p) => p.display().to_string(),
                None => self.tabs.current().map_or("Untitled".to_string(), |t| t.title()),
            };
            let m = if self.doc.nb.is_modified() { " (modified)" } else { "" };
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
            let engine =
                if self.doc.nb.engine().is_empty() { String::new() } else { format!(" · {}", self.doc.nb.engine()) };
            ui.label(RichText::new(format!("{text}{engine}")).color(weak).small());
            if let Some(ms) = self.doc.nb.last_millis() {
                ui.label(RichText::new(format!("· last {ms} ms")).color(weak).small());
            }
            if let Some(c) = self.doc.nb.current_cell().and_then(|id| self.doc.nb.cell(id)) {
                let mut s = format!("· {}", c.style.info().name);
                if let Some(n) = c.label {
                    s.push_str(&if c.style == Style::Output { format!(" Out[{n}]") } else { format!(" In[{n}]") });
                }
                if let Selection::Caret { pos, .. } = self.doc.nb.selection() {
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
        if let Some(ask) = self.tabs.asked().cloned() {
            let mut answer = None;
            let (title, text) = match &ask {
                Ask::Save(id) => {
                    let name =
                        self.tabs.index_of(*id).and_then(|i| self.tabs.get(i)).map_or(String::new(), |t| t.title());
                    ("Unsaved changes".to_string(), format!("Save the changes to {name}?"))
                }
                Ask::SaveAll(ids) => {
                    let names: Vec<String> = ids
                        .iter()
                        .filter_map(|id| self.tabs.index_of(*id).and_then(|i| self.tabs.get(i)).map(|t| t.title()))
                        .collect();
                    let text = if names.len() == 1 {
                        format!("Save the changes to {}?", names[0])
                    } else {
                        format!(
                            "Save the changes to {} notebooks?

{}",
                            names.len(),
                            names.join(
                                "
"
                            )
                        )
                    };
                    ("Unsaved changes".to_string(), text)
                }
            };
            center(egui::Window::new(title)).show(ctx, |ui| {
                ui.set_min_width(300.0);
                ui.label(text);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let all = matches!(ask, Ask::SaveAll(ref v) if v.len() > 1);
                    if ui.button(if all { "Save all" } else { "Save" }).clicked()
                        || ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        answer = Some(Answer::Saved);
                    }
                    if ui.button("Don't save").clicked() {
                        answer = Some(Answer::Discard);
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        answer = Some(Answer::Cancel);
                    }
                });
            });
            if let Some(a) = answer {
                self.answer_close(a);
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
            let log = self.doc.kernel.log.lock().map(|l| l.clone()).unwrap_or_default();
            egui::Window::new("Kernel log").open(&mut self.kernel_log).default_size([520.0, 300.0]).show(ctx, |ui| {
                ui.label(format!("Kernel: {}", self.doc.kernel.exe.display()));
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
            if self.icon.is_none() {
                if let Some(d) = icon_data() {
                    let img = egui::ColorImage::from_rgba_unmultiplied([d.width as usize, d.height as usize], &d.rgba);
                    self.icon = Some(ctx.load_texture("mc-notebook-icon", img, egui::TextureOptions::LINEAR));
                }
            }
            let icon = self.icon.clone();
            let weak = self.doc.view.palette.weak;
            center(egui::Window::new("About MathCAS Notebook").title_bar(true)).show(ctx, |ui| {
                ui.set_width(320.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);
                    if let Some(t) = &icon {
                        ui.add(egui::Image::new(t).fit_to_exact_size(vec2(72.0, 72.0)));
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("MathCAS Notebook").heading());
                    ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(weak));
                    ui.add_space(6.0);
                    ui.label("Notebooks for the MathCAS language.");
                    ui.label(RichText::new("Colours: Atom One Light and One Dark.").color(weak).small());
                    ui.add_space(10.0);
                    if ui.add(egui::Button::new("OK").min_size(vec2(80.0, 0.0))).clicked()
                        || ui.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Enter))
                    {
                        self.about = false;
                    }
                    ui.add_space(4.0);
                });
            });
        }
    }

    fn update_theme(&mut self, ctx: &egui::Context) {
        let system_dark = ctx.system_theme().is_some_and(|t| t == egui::Theme::Dark);
        let palette = self.settings.theme.palette(system_dark);
        if self.applied.as_ref() != Some(&palette) {
            palette.apply(ctx);
            self.doc.view.palette = palette.clone();
            self.doc.view.clear();
            self.applied = Some(palette);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let name = self.tabs.current().map_or("Untitled".to_string(), |t| t.title());
        let title = format!("{}{} - MathCAS Notebook", name, if self.doc.nb.is_modified() { " *" } else { "" });
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// The notebook (for the self-test).
    pub fn notebook(&self) -> &Notebook {
        &self.doc.nb
    }

    /// Replace the notebook without a path (for the self-test).
    pub fn load_notebook(&mut self, nb: Notebook) {
        self.set_notebook(nb, None);
    }

    /// Set the theme (for the self-test).
    pub fn set_theme(&mut self, c: ThemeChoice) {
        self.settings.theme = c;
    }

    /// The GPU the window draws with (`main` sets it after creating the window).
    pub fn set_gpu(&mut self, info: Option<eframe::wgpu::AdapterInfo>) {
        self.gpu = info;
    }

    /// The GPU the window draws with.
    pub fn gpu(&self) -> Option<&eframe::wgpu::AdapterInfo> {
        self.gpu.as_ref()
    }

    /// Set the magnification (for the self-test).
    pub fn set_magnification(&mut self, z: f32) {
        self.doc.view.set_magnification(z);
    }

    /// The tabs as `title`, `*title` (modified), `[title]` (active) (for the self-test).
    pub fn tabs_summary(&self) -> Vec<String> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let s = format!("{}{}", if t.modified { "*" } else { "" }, t.title());
                if i == self.tabs.active() {
                    format!("[{s}]")
                } else {
                    s
                }
            })
            .collect()
    }

    /// Show or hide the About dialog (for the self-test).
    pub fn show_about(&mut self, on: bool) {
        self.about = on;
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
        self.doc.scroll_by += dy;
    }

    /// Save to `path` (for the self-test).
    pub fn save_path(&mut self, path: &Path) -> bool {
        self.save_to(path)
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // egui's glyph atlas grows to the largest texture the GPU takes before it is cleared at 80 % full: at 8192
        // that is 256 MB in memory plus its copy on the GPU (an input storm grew the window by 600 MB). 4096 bounds it
        // at 64 MB; the window's own surface is not limited by this.
        raw.max_texture_side = Some(raw.max_texture_side.unwrap_or(MAX_TEXTURE_SIDE).min(MAX_TEXTURE_SIDE));
        if let Some(mut st) = self.selftest.take() {
            st.inject(self, ctx, raw);
            self.selftest = Some(st);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let start = std::time::Instant::now();
        // the second frame: the first one reached the screen, the graphics interface works
        self.frames_shown += 1;
        if self.frames_shown == 2 {
            if let Some(info) = &self.gpu {
                crate::gpu::started(info);
            }
        }
        let ctx = ui.ctx().clone();
        self.update_theme(&ctx);
        self.sync_tabs();
        let was = self.doc.nb.kernel_state();
        if self.doc.nb.pump(&mut self.doc.kernel) {
            self.doc.scroll_pending |= self.doc.nb.kernel_state() != KernelState::Busy;
        }
        match self.doc.nb.kernel_state() {
            // definitions may have changed: refresh the names that colour undefined globals (design 3.11)
            KernelState::Idle if was == KernelState::Busy => self.doc.kernel.request_names(""),
            KernelState::NotStarted | KernelState::Stopped if was != self.doc.nb.kernel_state() => {
                self.doc.view.set_defined(None)
            }
            _ => {}
        }
        // Ctrl+wheel magnifies the notebook (not the chrome)
        let zoom_delta = ctx.input(|i| if i.modifiers.ctrl { i.zoom_delta() } else { 1.0 });
        // in the menu's steps, one per notch: a continuous magnification makes every frame a new text size, and
        // each size fills the glyph atlas anew (a wheel storm grew the window by 600 MB)
        if zoom_delta != 1.0 && zoom_delta.is_finite() && zoom_delta > 0.0 {
            self.wheel_zoom *= zoom_delta;
            if self.wheel_zoom >= 1.08 || self.wheel_zoom <= 1.0 / 1.08 {
                self.step_zoom(if self.wheel_zoom > 1.0 { 1 } else { -1 });
                self.wheel_zoom = 1.0;
            }
        }
        if ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_pressed()) {
            self.pointer_hidden = false;
        }
        self.keyboard(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.confirmed_close {
            self.sync_modified();
            if self.tabs.iter().any(|t| t.modified) {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                if self.tabs.asked().is_none() {
                    let c = self.with_tabs(|t| t.close_window());
                    self.closed(c);
                }
            }
        }
        let p = self.doc.view.palette.clone();
        egui::Panel::top("menu")
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(6, 2)))
            .show(ui, |ui| self.menus(ui));
        if self.settings.show_toolbar {
            egui::Panel::top("toolbar")
                .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(6, 3)))
                .exact_size(34.0)
                .show(ui, |ui| self.toolbar(ui));
        }
        egui::Panel::top("tabs")
            .frame(Frame::new().fill(p.panel).inner_margin(Margin { left: 4, right: 4, top: 2, bottom: 0 }))
            .exact_size(crate::tabbar::HEIGHT + 2.0)
            .show(ui, |ui| self.tab_bar(ui));
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
        self.plus_popup(&ctx);
        self.dialogs(&ctx);
        self.update_title(&ctx);
        if (0..self.tabs.len()).any(|i| self.tab_busy(i)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        if self.settings.caret_blink && matches!(self.doc.nb.selection(), Selection::Caret { .. }) {
            // the caret blinks: a repaint when the phase changes (the insertion line is steady)
            let next = self.doc.view.next_blink(ctx.input(|i| i.time));
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(next + 0.005));
        }
        self.doc.view.blink = self.settings.caret_blink;
        // the window's place, remembered for the next start
        if let Some(r) = ctx.input(|i| i.viewport().inner_rect) {
            let outer = ctx.input(|i| i.viewport().outer_rect).unwrap_or(r);
            // a maximized, full-screen or minimized window keeps the last normal place (a minimized one sits at
            // -32000 on Windows and would reopen off the screen)
            let special = ctx.input(|i| {
                let v = i.viewport();
                v.maximized.unwrap_or(false) || v.fullscreen.unwrap_or(false) || v.minimized.unwrap_or(false)
            });
            if !special && r.width() >= 100.0 && r.height() >= 100.0 && outer.min.x > -10000.0 && outer.min.y > -10000.0 {
                self.window_rect = Some((outer.min, r.size()));
            }
        }
        self.frame_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(mut st) = self.selftest.take() {
            st.after_frame(self, &ctx);
            self.selftest = Some(st);
        }
    }

    fn on_exit(&mut self) {
        self.doc.kernel.stop();
        self.settings.session = self.session();
        if let Some((pos, size)) = self.window_rect {
            self.settings.window = Some([pos.x, pos.y, size.x, size.y]);
        }
        self.settings.save();
    }
}

/// A menu item with a check mark that toggles `on`; `true` when clicked. (egui's checkbox draws its box with the
/// widget outline that menus remove, so it would show no box at all.)
fn check_item(ui: &mut egui::Ui, on: &mut bool, label: &str) -> bool {
    let mark = if *on { "✔" } else { "  " };
    let clicked = ui.add(egui::Button::new((RichText::new(mark), label))).clicked();
    if clicked {
        *on = !*on;
        ui.close();
    }
    clicked
}

/// A thin vertical line between groups of toolbar items.
fn toolbar_separator(ui: &mut egui::Ui, p: &Palette) {
    let (r, _) = ui.allocate_exact_size(vec2(9.0, 20.0), Sense::hover());
    let x = ui.painter().round_to_pixel_center(r.center().x);
    ui.painter().vline(x, r.y_range(), egui::Stroke::new(1.0, p.border_strong()));
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
