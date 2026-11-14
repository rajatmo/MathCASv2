//! Commands: applying a model command and acting on its effects, window actions, F1 documentation, copying
//! pictures, magnification.
use super::*;
use mc_graphics::view3d::View3d;

impl App {
    /// Apply a command to the notebook and act on its effects.
    pub fn run(&mut self, ctx: &egui::Context, cmd: Command) {
        self.doc.nb.set_auto_close(self.settings.auto_close);
        let mut env = AppEnv { view: &mut self.doc.view, ctx, clip: &mut self.clip };
        let fx = self.doc.nb.apply(cmd, &mut env);
        // the caret shows at once after every key and click, then blinks
        self.doc.view.blink_from = ctx.input(|i| i.time);
        self.effects(ctx, fx);
    }

    pub(super) fn effects(&mut self, ctx: &egui::Context, fx: Effects) {
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

    pub(super) fn app_action(&mut self, ctx: &egui::Context, a: AppAction) {
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
            AppAction::ToggleOutputAlignment => self.set_output_alignment(!self.settings.outputs_right),
            AppAction::FullScreen => {
                let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!full));
            }
            AppAction::Complete => self.start_completion(),
            AppAction::Preferences => self.prefs = true,
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
                let name = self.help_word();
                self.open_docs(name);
            }
        }
    }

    /// The function name F1 looks up: the selected name, the name at or before the caret, else the function whose
    /// call the caret is in; in a cell with boxes, read in its canonical text.
    pub(super) fn help_word(&self) -> Option<String> {
        if let Some(c) = self.code_caret() {
            let index = mc_help::Index::embedded();
            return help::help_target(&c.text, c.anchor, c.pos, &|n: &str| index.get(n).is_some());
        }
        let Selection::Caret { cell, anchor, pos } = *self.doc.nb.selection() else { return None };
        help_name(&self.doc.nb.cell(cell)?.text, anchor, pos)
    }

    /// Start the documentation app (`mc-docs [name]`): `MATHCAS_DOCS`, else `mc-docs` next to this program; a clear
    /// message when it is not there.
    pub(super) fn open_docs(&mut self, name: Option<String>) {
        self.help.docs_opened.push(name.clone().unwrap_or_default());
        // `MATHCAS_DOCS=-`: record the request only (self-tests)
        if std::env::var_os("MATHCAS_DOCS").is_some_and(|v| v == "-") {
            self.status = format!("Documentation: {}", name.unwrap_or_default());
            return;
        }
        if cfg!(target_os = "android") {
            self.status = "The documentation window is not part of the Android app yet".into();
            return;
        }
        let exe = std::env::var_os("MATHCAS_DOCS").map(PathBuf::from).filter(|p| p.is_file()).or_else(|| {
            let name = if cfg!(windows) { "mc-docs.exe" } else { "mc-docs" };
            std::env::current_exe().ok().map(|e| e.with_file_name(name)).filter(|p| p.is_file())
        });
        let Some(exe) = exe else {
            self.error = Some(
                "The documentation app (mc-docs) was not found next to the notebook program. Install it there, or \
                 set the environment variable MATHCAS_DOCS to its path."
                    .into(),
            );
            return;
        };
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(name.iter());
        match cmd.spawn() {
            Ok(_) => {
                self.status = match name {
                    Some(n) => format!("Opening the documentation of {n}"),
                    None => "Opening the documentation".into(),
                }
            }
            Err(e) => self.error = Some(format!("Cannot start {}: {e}", exe.display())),
        }
    }

    /// Copy the picture of Output cell `id`: the picture (PNG, and a bitmap for programs that take only that) and
    /// its `image(...)` text (design 6).
    pub(super) fn copy_graphic(&mut self, id: mc_nbmodel::CellId) {
        let Some(c) = self.doc.nb.cell(id) else { return };
        let Some(p) = c.graphic() else { return };
        let text = c.text.clone();
        let path = mc_nbfile::images::resolve(&p, self.doc.view.base.as_deref());
        let width = self
            .doc
            .nb
            .index_of(id)
            .and_then(|i| self.doc.view.graphic_rect(&self.doc.nb, i))
            .map_or(640, |r| (r.width() * self.ctx.pixels_per_point()).round() as u32);
        match crate::images::pixels(&path, width) {
            Ok((rgba, w, h)) => {
                let png = match mc_nbfile::images::ImageFormat::of(&path) {
                    Some(mc_nbfile::images::ImageFormat::Png) => std::fs::read(&path).ok(),
                    _ => mc_ui::clipboard::encode_png(&rgba, w, h),
                };
                self.clip.set_picture(&Picture { text: &text, png: png.as_deref(), rgba: &rgba, width: w, height: h });
                self.status = format!("Copied the graphic ({w} x {h} pixels)");
            }
            Err(e) => {
                self.clip.set(Contents { text: Some(text), ..Contents::default() });
                self.status = format!("Copied the graphic's text only: {e}");
            }
        }
    }

    /// `d` steps of the magnification ladder (0: back to 100 %), keeping the point under the pointer in place (the
    /// middle of the canvas when the pointer is elsewhere).
    pub(super) fn step_zoom(&mut self, d: i32) {
        let at = self.ctx.input(|i| i.pointer.hover_pos()).map(|p| p.y);
        self.zoom_at(crate::zoom::step(self.doc.view.magnification(), d), at);
    }

    /// Set the magnification (from the menus), keeping the middle of the canvas in place.
    /// Keep the content at screen height `y` where it is while the layout changes (magnification, alignment).
    fn hold_view(&mut self, y: f32) {
        let screen = y - self.canvas_clip.top();
        let canvas_y = self.doc.offset + screen;
        let time = self.ctx.input(|i| i.time);
        self.doc.zoom_anchor = self.cell_at_y(canvas_y).map(|cell| {
            let r = self.doc.view.cell_rect(cell);
            let frac = if r.height() > 0.0 { ((canvas_y - r.top()) / r.height()).clamp(0.0, 1.0) } else { 0.0 };
            crate::zoom::Anchor { cell, frac, screen, until: time + 1.0 }
        });
    }

    /// Outputs on the right edge (`true`) or the left (View > Output alignment, Ctrl+Shift+J): applied at once,
    /// saved in the preferences.
    pub(crate) fn set_output_alignment(&mut self, right: bool) {
        self.hold_view(self.canvas_clip.center().y);
        self.settings.outputs_right = right;
        self.doc.view.set_outputs_right(right);
        self.settings.save();
        self.status = format!("Outputs on the {}", if right { "right" } else { "left" });
    }

    pub(super) fn set_zoom(&mut self, z: f32) {
        self.zoom_at(z, None);
    }

    /// Set the magnification to `z`, keeping the content at screen height `at` (the canvas's middle when `None` or
    /// outside the canvas) where it is: the canvas holds that point for a short while, as cells measured at the
    /// new size move what lies above it.
    pub(super) fn zoom_at(&mut self, z: f32, at: Option<f32>) {
        let z = z.clamp(0.5, 3.0);
        let clip = self.canvas_clip;
        if (z - self.doc.view.magnification()).abs() < 1e-4 {
            return;
        }
        self.hold_view(at.filter(|y| clip.y_range().contains(*y)).unwrap_or(clip.center().y));
        self.doc.view.set_magnification(z);
        self.settings.magnification = self.doc.view.magnification();
        self.settings.save();
    }

    /// A 3D picture's view now (the one being turned, else the one kept on the output, else its own) and its own
    /// view; `None` for other cells.
    pub(super) fn view3d_of(&mut self, id: mc_nbmodel::CellId) -> Option<(View3d, View3d)> {
        let i = self.doc.nb.index_of(id)?;
        let path = self.doc.view.graphic_path(&self.doc.nb, i)?;
        let scene = self.rot.borrow_mut().scene(&path)?;
        let base = scene.view();
        if let Some((cell, v)) = self.doc.view.live_view {
            if cell == id {
                return Some((v, base));
            }
        }
        let c = &self.doc.nb.cells()[i];
        Some((crate::rotate3d::view_of(&c.out_extra, base).unwrap_or(base), base))
    }

    /// Keep view `v` on the 3D picture of cell `id` (an undo unit) and end the live view.
    pub(super) fn keep_view(&mut self, ctx: &egui::Context, id: mc_nbmodel::CellId, v: View3d) {
        if let Some((_, base)) = self.view3d_of(id) {
            self.run(ctx, Command::SetOutputAttrs(id, crate::rotate3d::attrs_of(&v, &base)));
            self.doc.view.graphic = Some(id);
        }
        self.doc.view.live_view = None;
    }

    /// Wheel notches over the selected 3D picture zoom it (`zoom_wheel`: 10 % a notch); the zoom is kept on the
    /// output once the wheel rests for 0.4 s.
    pub(super) fn zoom_picture(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|i| i.time);
        let notches = std::mem::take(&mut self.zoom3d);
        if notches != 0.0 {
            if let Some((id, _)) = self.wheel3d {
                if let Some((v, _)) = self.view3d_of(id) {
                    let zoom = mc_graphics::view3d::zoom_wheel(v.zoom, notches);
                    self.doc.view.live_view = Some((id, View3d { zoom, ..v }));
                    self.zoom3d_commit = Some((id, time + 0.4));
                    ctx.request_repaint_after(std::time::Duration::from_millis(450));
                }
            }
        }
        if let Some((id, at)) = self.zoom3d_commit {
            if time >= at && !matches!(self.drag, Some(Drag::Rotate(..))) {
                self.zoom3d_commit = None;
                if let Some((cell, v)) = self.doc.view.live_view {
                    if cell == id {
                        self.keep_view(ctx, id, v);
                    }
                }
            }
        }
    }

    pub(super) fn context(&self) -> Context {
        mc_nbmodel::script::context(&self.doc.nb)
    }
}
