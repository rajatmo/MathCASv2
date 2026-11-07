//! The notebook window: chrome (menus, toolbar, status bar, dialogs, design 10), input routing into model commands,
//! the canvas, files, settings and the kernel link.
use mc_ui::clipboard::{Clipboard, Contents, Picture};
use mc_ui::fonts;
use mc_ui::icons::{self, Icon};
use crate::images::Images;
use crate::typeset::Glyphs;
use crate::input;
use crate::kernel::{default_kernel, KernelProcess};
use crate::selftest::SelfTest;
use crate::settings::Settings;
use crate::theme::{Palette, ThemeChoice};
use crate::view::{Handle, Hit, View};
use eframe::egui::{
    self, vec2, Align, Color32, Frame, Key, Layout, Margin, RichText, ScrollArea, Sense, ViewportCommand,
};
use mc_nbmodel::keys::{Context, KeyTable};
use mc_nbmodel::tabs::{Answer, Ask, Closed, Tabs};
use mc_nbmodel::{
    AppAction, Bracket, Cell, Clip, Command, Effects, Env, InitAnswer, KernelState, Notebook, Selection, Style,
};
use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

mod canvas;
mod chrome;
mod commands;
mod dialogs;
mod files;
mod help;
mod keyboard;
mod prefs;
mod tabs;

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

/// Magnifications offered in the menus (design 4.10: 50 % - 300 %), rungs of the wheel's ladder (`zoom::LADDER`).
const ZOOMS: [f32; 10] = [0.5, 0.8, 0.9, 1.0, 1.1, 1.2, 1.5, 2.0, 2.5, 3.0];

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
        self.clip.set(Contents { text: clip.text, cells: clip.cells, boxes: clip.boxes });
    }
    fn clipboard(&mut self) -> Clip {
        let c = self.clip.get();
        Clip { text: c.text, cells: c.cells, boxes: c.boxes }
    }
    fn box_line(&mut self, cell: &Cell, path: &[mc_boxes::Step], offset: usize, up: bool) -> Option<(mc_boxes::RowPath, usize)> {
        self.view.box_line(self.ctx, cell, path, offset, up)
    }
    fn atomic(&mut self, cell: &Cell) -> bool {
        self.view.shows_typeset_output(cell)
    }
    fn box_line_edge(&mut self, cell: &Cell, offset: usize, end: bool) -> Option<usize> {
        self.view.box_line_edge(self.ctx, cell, offset, end)
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
    /// resizing a picture by a handle: where the drag started (screen) and the picture's size then (screen points)
    Resize(mc_nbmodel::CellId, Handle, egui::Pos2, egui::Vec2),
    /// moving the slider of control `k` of a `manipulate` output
    Control(mc_nbmodel::CellId, usize),
    /// turning a 3D picture: where the drag started (screen), the view then and the picture's size (screen points)
    Rotate(mc_nbmodel::CellId, egui::Pos2, mc_graphics::view3d::View3d, egui::Vec2),
}

/// Completion delay after two typed identifier characters (design 3.10, guess 20).
const COMPLETION_DELAY: f64 = 0.3;

/// The completion popup (design 3.10).
struct Completion {
    cell: mc_nbmodel::CellId,
    /// in typeset input: the row of boxes the name is in
    path: Option<mc_boxes::RowPath>,
    /// where the name being completed starts (a byte of the text, or an item of the row)
    start: usize,
    /// candidates from the kernel and the keywords: (name, kind)
    names: Vec<(String, String)>,
    selected: usize,
}

/// The candidates matching `prefix` (`mc_help::score`, as the language server ranks them): case-insensitive prefix
/// matches first, then names with a word starting with it, each by length then name; subsequence matches only when
/// there are none of those (with 700 library names they would bury the useful ones, decision NBDOCS.6).
pub fn rank(names: &[(String, String)], prefix: &str) -> Vec<(String, String)> {
    let mut scored: Vec<(u8, &(String, String))> =
        names.iter().filter_map(|e| mc_help::score(&e.0, prefix).map(|s| (s, e))).collect();
    if scored.iter().any(|(s, _)| *s < 2) {
        scored.retain(|(s, _)| *s < 2);
    }
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
    /// the point the canvas keeps in place while the magnification changes
    pub(crate) zoom_anchor: Option<crate::zoom::Anchor>,
}

impl Doc {
    /// A tab holding `nb`, from the file `path` if any.
    pub fn new(
        nb: Notebook,
        path: Option<&Path>,
        ctx: &egui::Context,
        images: &Rc<RefCell<Images>>,
        glyphs: &Rc<RefCell<Glyphs>>,
        palette: Palette,
        zoom: f32,
    ) -> Doc {
        let mut nb = nb;
        nb.mark_saved();
        let mut view = View::new(palette, images.clone(), glyphs.clone());
        view.base = path.and_then(|p| p.parent().map(Path::to_path_buf));
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
            zoom_anchor: None,
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
    /// what the live region last announced (evaluation finished, kernel stopped)
    announce: String,
    last_click: (f64, egui::Pos2, u32),
    drag: Option<Drag>,
    /// the slider last pressed: arrow keys move it (Left/Right a step, Shift ten, Home/End the ends) until another
    /// click
    slider: Option<(mc_nbmodel::CellId, usize)>,
    title: String,
    // dialogs
    other_style: Option<String>,
    ask_init: bool,
    interrupt: bool,
    confirmed_close: bool,
    kernel_log: bool,
    keyboard_help: bool,
    about: bool,
    about_tab: crate::about::Tab,
    /// the preferences dialog is open; its Close button or Esc was used
    prefs: bool,
    prefs_close: bool,
    /// the top-level menus' buttons (their popups open from the keyboard: F10, Alt+letter)
    menu_ids: Vec<(char, egui::Id)>,
    /// a menu was opened from the keyboard: its first item takes the focus
    menu_focus: std::cell::Cell<bool>,
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
    /// Ctrl+wheel and pinch input summed into magnification steps, and the steps of this frame not yet applied
    wheel: crate::zoom::WheelZoom,
    zoom_steps: i32,
    /// the pictures of every tab's Output cells, as textures
    pub(crate) images: Rc<RefCell<Images>>,
    /// the 3D pictures being turned, and their renders
    pub(crate) rot: Rc<RefCell<crate::rotate3d::Rotations>>,
    /// the selected 3D picture's screen rectangle (the wheel over it zooms it) and the pointer's last position
    wheel3d: Option<(mc_nbmodel::CellId, egui::Rect)>,
    pointer_at: Option<egui::Pos2>,
    /// wheel notches over the selected 3D picture not yet applied, and when its zoom is kept on the output
    zoom3d: f64,
    zoom3d_commit: Option<(mc_nbmodel::CellId, f64)>,
    /// the math-font glyphs of typeset content, as one texture atlas (shared by the tabs)
    pub(crate) glyphs: Rc<RefCell<Glyphs>>,
    /// the descriptions of plot pictures, for coordinates under the pointer
    plots: crate::plothover::Plots,
    /// help while writing code: the usage hint, the hovered name, the pages asked for
    pub(crate) help: help::HelpState,
    /// the canvas's visible rectangle in the last frame (screen coordinates)
    canvas_clip: egui::Rect,
}

impl App {
    /// The window with the notebooks at `paths` (and the last session's tabs, when that setting is on).
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Vec<PathBuf>, selftest: Option<SelfTest>) -> App {
        let ctx = cc.egui_ctx.clone();
        let defs = fonts::definitions(fonts::Source::from_env());
        let font_set = fonts::FontSet::new(&defs);
        ctx.set_fonts(defs);
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let settings = Settings::load();
        // the cache of pictures read out of opened notebooks: files unused for 30 days go (on a thread of its own)
        std::thread::spawn(|| mc_nbfile::embed::sweep_cache(&mc_nbfile::embed::cache_dir(), 30));
        let palette = settings.theme.palette(ctx.system_theme().is_some_and(|t| t == egui::Theme::Dark));
        let zoom = settings.magnification;
        let instance = crate::instance::Server::start(&ctx);
        let images = Rc::new(RefCell::new(Images::new(&ctx)));
        let glyphs = Rc::new(RefCell::new(Glyphs::new(&ctx)));
        glyphs.borrow_mut().set_fonts(font_set);
        let mut app = App {
            doc: Doc::new(Notebook::new(), None, &ctx, &images, &glyphs, palette, zoom),
            images,
            rot: Rc::new(RefCell::new(crate::rotate3d::Rotations::new(&ctx))),
            wheel3d: None,
            pointer_at: None,
            zoom3d: 0.0,
            zoom3d_commit: None,
            glyphs,
            plots: crate::plothover::Plots::default(),
            help: help::HelpState::default(),
            canvas_clip: egui::Rect::NOTHING,
            tabs: Tabs::new(),
            ctx: ctx.clone(),
            keys: KeyTable::default(),
            clip: Clipboard::for_window(&ctx),
            settings,
            applied: None,
            status: String::new(),
            announce: String::new(),
            last_click: (-1.0, egui::Pos2::ZERO, 0),
            drag: None,
            slider: None,
            title: String::new(),
            other_style: None,
            ask_init: false,
            interrupt: false,
            confirmed_close: false,
            kernel_log: false,
            keyboard_help: false,
            about: false,
            about_tab: crate::about::Tab::About,
            prefs: false,
            prefs_close: false,
            menu_ids: vec![],
            menu_focus: std::cell::Cell::new(false),
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
            wheel: crate::zoom::WheelZoom::default(),
            zoom_steps: 0,
        };
        if app.settings.bundled_fonts {
            app.apply_fonts(&ctx);
        }
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

    /// Show or hide the preferences dialog (for the self-test).
    pub fn show_preferences(&mut self, on: bool) {
        self.prefs = on;
    }

    /// The clipboard's text (for the self-test).
    pub fn clipboard_text(&mut self) -> Option<String> {
        self.clip.get().text
    }

    /// What the clipboard holds, every flavour (for the self-test).
    pub fn clipboard_contents(&mut self) -> Contents {
        self.clip.get()
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

    /// The views kept on 3D outputs and the render times so far (the self-test's `view3d` step).
    pub fn view3d_probe(&self) -> String {
        let views: Vec<String> = self
            .doc
            .nb
            .cells()
            .iter()
            .filter(|c| crate::rotate3d::VIEW_ATTRS.iter().any(|a| c.out_extra.has(a)))
            .map(|c| format!("[{}]", c.out_extra.to_string().trim()))
            .collect();
        let mut t = self.rot.borrow().times.clone();
        t.sort_by(f64::total_cmp);
        let med = t.get(t.len() / 2).copied().unwrap_or(0.0);
        let max = t.last().copied().unwrap_or(0.0);
        format!("{} renders: {} median {med:.1} ms max {max:.1} ms", views.join(" "), t.len())
    }

    /// The magnification, the scroll offset and what lies at window height `y`: the cell's index and the fraction
    /// of its height above `y` (the self-test's `anchor` step).
    pub fn zoom_probe(&self, y: f32) -> String {
        let canvas_y = self.doc.offset + (y - self.canvas_clip.top());
        let at = self.cell_at_y(canvas_y).map(|i| {
            let r = self.doc.view.cell_rect(i);
            (i, (canvas_y - r.top()) / r.height().max(1.0))
        });
        let at = at.map_or("nothing".to_string(), |(i, f)| format!("cell {i} at {f:.3}"));
        format!("zoom {:.2} offset {:.1} {at}", self.doc.view.magnification(), self.doc.offset)
    }

    /// What is at a window position (the self-test's `hit` step).
    pub fn hit_at(&mut self, at: egui::Pos2) -> String {
        let p = (at - self.doc.view.origin).to_pos2();
        let ctx = self.ctx.clone();
        format!("{:?}", self.doc.view.hit(&ctx, &self.doc.nb, p))
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
        // Ctrl+wheel and pinches are the notebook's magnification, in whole steps; egui never sees them (it would
        // smooth them over frames and scroll with what arrives after Ctrl is released)
        let time = raw.time.unwrap_or(0.0);
        self.zoom_steps += self.wheel.take(&mut raw.events, time);
        // the wheel over the selected 3D picture zooms the picture instead of scrolling the notebook
        for e in &raw.events {
            if let egui::Event::PointerMoved(p) = e {
                self.pointer_at = Some(*p);
            }
        }
        if let (Some((_, rect)), Some(p)) = (self.wheel3d, self.pointer_at) {
            if rect.contains(p) {
                let mut notches = 0.0;
                raw.events.retain(|e| match e {
                    egui::Event::MouseWheel { unit, delta, .. } => {
                        notches += f64::from(match unit {
                            egui::MouseWheelUnit::Point => delta.y / 50.0,
                            _ => delta.y,
                        });
                        false
                    }
                    _ => true,
                });
                self.zoom3d += notches;
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let start = std::time::Instant::now();
        self.images.borrow_mut().begin_frame();
        self.rot.borrow_mut().poll();
        self.doc.view.rot = Some(self.rot.clone());
        self.zoom_picture(&ui.ctx().clone());
        self.glyphs.borrow_mut().begin_frame();
        // the second frame: the first one reached the screen, the graphics interface works
        self.frames_shown += 1;
        if self.frames_shown == 2 {
            if let Some(info) = &self.gpu {
                mc_ui::gpu::started(info);
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
            KernelState::Idle if was == KernelState::Busy => {
                self.doc.kernel.request_names("");
                self.announce = "Evaluation finished".into();
            }
            KernelState::Stopped if was != KernelState::Stopped => {
                self.doc.view.set_defined(None);
                self.announce = "Kernel stopped".into();
            }
            KernelState::NotStarted | KernelState::Stopped if was != self.doc.nb.kernel_state() => {
                self.doc.view.set_defined(None)
            }
            _ => {}
        }
        // Ctrl+wheel magnifies the notebook (not the chrome): the frame's steps at once, one relayout, on the ladder
        // (a continuous magnification makes every frame a new text size, and each size fills the glyph atlas anew:
        // a wheel storm grew the window by 600 MB), the point under the pointer kept in place
        let steps = std::mem::take(&mut self.zoom_steps);
        if steps != 0 {
            let at = ctx.input(|i| i.pointer.hover_pos()).map(|p| p.y);
            self.zoom_at(crate::zoom::step(self.doc.view.magnification(), steps), at);
        }
        if ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_pressed()) {
            self.pointer_hidden = false;
        }
        self.keyboard(&ctx);
        // a picture stays selected while the caret is in its cell (any other click or key moves it away)
        if let Some(id) = self.doc.view.graphic {
            if !matches!(self.doc.nb.selection(), Selection::Caret { cell, .. } if *cell == id) {
                self.doc.view.graphic = None;
            }
        }
        if !matches!(self.drag, Some(Drag::Resize(..))) {
            self.doc.view.live_size = None;
            self.doc.view.live_anchor = None;
        }
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
        self.usage_hint(&ctx);
        self.plus_popup(&ctx);
        self.dialogs(&ctx);
        // the notebook takes the keyboard itself: egui's focus navigation must not leave a button focused (Tab,
        // the next-placeholder and indent key, moves egui's focus; a focused "+" tab button then took Enter and
        // opened tabs), so focus is kept only by the dialogs and the find bar
        if !self.modal_open() && self.find.is_none() && !egui::Popup::is_any_open(&ctx) {
            ctx.memory_mut(|m| {
                if let Some(f) = m.focused() {
                    m.surrender_focus(f);
                }
            });
        }
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
        self.doc.view.set_outputs_right(self.settings.outputs_right);
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
    let r = ui.add(egui::Button::image_and_text(chrome::check_mark(ui, *on), label));
    focus_ring(ui, &r);
    let clicked = r.clicked();
    if clicked {
        *on = !*on;
        ui.close();
    }
    clicked
}

/// A ring around a menu item that has the keyboard focus (menus draw no frame of their own around a focused item).
fn focus_ring(ui: &egui::Ui, r: &egui::Response) {
    if r.has_focus() {
        let c = ui.visuals().selection.stroke.color;
        ui.painter().rect_stroke(r.rect, 3.0, egui::Stroke::new(1.5, c), egui::StrokeKind::Inside);
    }
}

/// A thin vertical line between groups of toolbar items.
fn toolbar_separator(ui: &mut egui::Ui, p: &Palette) {
    let (r, _) = ui.allocate_exact_size(vec2(9.0, 20.0), Sense::hover());
    let x = ui.painter().round_to_pixel_center(r.center().x);
    ui.painter().vline(x, r.y_range(), egui::Stroke::new(1.0, p.border_strong()));
}

/// The name F1 looks up in `text` with the selection `anchor..pos`: the selected text, or the identifier around
/// the caret; `None` when that is not a name.
fn help_name(text: &str, anchor: usize, pos: usize) -> Option<String> {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let word = if anchor != pos {
        text.get(anchor.min(pos)..anchor.max(pos))?.trim().to_string()
    } else {
        let pos = pos.min(text.len());
        let start = text[..pos].char_indices().rev().take_while(|&(_, c)| ident(c)).last().map_or(pos, |(i, _)| i);
        let end = text[pos..].char_indices().find(|&(_, c)| !ident(c)).map_or(text.len(), |(i, _)| pos + i);
        text[start..end].to_string()
    };
    let ok = word.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') && word.chars().all(ident);
    ok.then_some(word)
}

/// The size a picture of size `from` (screen points) takes when handle `h` has moved by `d`: corner handles keep
/// the aspect ratio; side handles change one dimension with Shift held, else keep the ratio too (design 6, guess).
fn resize(from: egui::Vec2, d: egui::Vec2, h: Handle, shift: bool) -> egui::Vec2 {
    const MIN: f32 = 16.0;
    const MAX: f32 = 8000.0;
    let ratio = if from.y > 0.0 && from.x > 0.0 { from.x / from.y } else { 1.0 };
    // the handle's side grows when it moves outwards
    let (dx, dy) = (d.x * f32::from(h.x), d.y * f32::from(h.y));
    if shift && (h.x == 0 || h.y == 0) {
        return egui::vec2((from.x + dx).clamp(MIN, MAX), (from.y + dy).clamp(MIN, MAX));
    }
    let w = match (h.x, h.y) {
        (0, _) => (from.y + dy) * ratio,
        (_, 0) => from.x + dx,
        // a corner: whichever direction moved more (relative to the picture) decides
        _ => {
            if dx.abs() >= dy.abs() * ratio {
                from.x + dx
            } else {
                (from.y + dy) * ratio
            }
        }
    };
    let w = w.clamp(MIN, MAX);
    egui::vec2(w, (w / ratio).max(1.0))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f1_names() {
        let t = "y = n_integrate(sin(x), [x, 0, 1])";
        assert_eq!(help_name(t, 6, 6).as_deref(), Some("n_integrate"));
        assert_eq!(help_name(t, 4, 4).as_deref(), Some("n_integrate"));
        assert_eq!(help_name(t, 15, 15).as_deref(), Some("n_integrate"));
        assert_eq!(help_name(t, 17, 17).as_deref(), Some("sin"));
        assert_eq!(help_name(t, 16, 19).as_deref(), Some("sin"));
        assert_eq!(help_name(t, 2, 2), None);
        assert_eq!(help_name(t, 31, 31), None, "a number is not a name");
        assert_eq!(help_name(t, 0, 5), None, "a selection that is not one name");
        assert_eq!(help_name("", 0, 0), None);
        assert_eq!(help_name("αβ", 2, 2).as_deref(), Some("αβ"));
    }

    #[test]
    fn resizing_keeps_the_ratio_unless_a_side_moves_with_shift() {
        let from = egui::vec2(400.0, 200.0);
        let corner = Handle { x: 1, y: 1 };
        assert_eq!(resize(from, egui::vec2(100.0, 0.0), corner, false), egui::vec2(500.0, 250.0));
        assert_eq!(resize(from, egui::vec2(0.0, 100.0), corner, false), egui::vec2(600.0, 300.0));
        // the top-left corner grows when moved up and left
        assert_eq!(resize(from, egui::vec2(-40.0, 0.0), Handle { x: -1, y: -1 }, false), egui::vec2(440.0, 220.0));
        let right = Handle { x: 1, y: 0 };
        assert_eq!(resize(from, egui::vec2(-200.0, 9.0), right, false), egui::vec2(200.0, 100.0));
        assert_eq!(resize(from, egui::vec2(-200.0, 9.0), right, true), egui::vec2(200.0, 200.0));
        let bottom = Handle { x: 0, y: 1 };
        assert_eq!(resize(from, egui::vec2(0.0, 50.0), bottom, true), egui::vec2(400.0, 250.0));
        // never below the minimum
        assert_eq!(resize(from, egui::vec2(-1000.0, 0.0), right, false), egui::vec2(16.0, 8.0));
    }
}
