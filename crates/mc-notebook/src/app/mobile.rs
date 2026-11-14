//! The layout for phones and tablets (Android; anywhere with `MCNB_LAYOUT=mobile`): two screens and one sheet around
//! the same canvas, model and kernel as the desktop window.
//!
//! - **Home**: "New calculation", the notebooks of the notebooks folder (newest first), "Open a file".
//! - **Notebook**: a bar (back, the name, the kernel, more), the canvas from edge to edge without brackets, and at
//!   the bottom what the selection calls for: a row of keys with Run while a cell is edited, Copy / Share / Delete
//!   for a selected result or picture, else a round "+" for a new input.
//! - **The sheet** (more): the few commands that are not on the screen.
//!
//! There are no menus, toolbar, tabs, status bar or save questions: one notebook is open at a time, and every change
//! is written to its file within [`AUTOSAVE`] seconds and when the notebook is left. A notebook without a file gets
//! one (`Calculation N.mcn`) as soon as it has content.
use super::*;
use crate::theme::Material;
use crate::view::Metrics;
use eframe::egui::{pos2, FontId, Rect};
use std::time::SystemTime;

/// How long (seconds) after a change the notebook is written to its file.
const AUTOSAVE: f64 = 2.0;
/// The height of the top app bar (points).
const TOP: f32 = 64.0;
/// The height of the bars at the bottom (points).
const BAR: f32 = 56.0;
/// The height of the strip of completions above the keys (points).
const STRIP: f32 = 48.0;
/// The side of a button that is only an icon (points).
const TARGET: f32 = 48.0;
/// The widest the content gets (points): on a tablet or a phone held sideways it sits in the middle.
const COLUMN: f32 = 720.0;
/// The keys above the on-screen keyboard: what a phone's keyboard hides two levels deep.
const KEYS: [&str; 11] = ["(", ")", "[", "]", ",", "^", "*", "/", "+", "-", "="];

/// The screen that shows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Screen {
    #[default]
    Home,
    Notebook,
}

/// A notebook file on the Home screen.
#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    name: String,
    modified: Option<SystemTime>,
}

/// What the sheet is open for.
#[derive(Clone, Debug, PartialEq)]
enum Sheet {
    /// the commands of the screen
    More,
    /// a notebook of the Home list
    File(PathBuf),
}

/// The state of the mobile layout.
#[derive(Default)]
pub(super) struct Mobile {
    screen: Screen,
    sheet: Option<Sheet>,
    /// the notebooks of the Home list (read when Home is entered)
    files: Option<Vec<Entry>>,
    /// a rename in progress: the file and the name typed
    rename: Option<(PathBuf, String, bool)>,
    /// a file the user asked to delete (the question is open)
    delete: Option<PathBuf>,
    /// when the notebook was last written (seconds of the frame clock)
    saved_at: f64,
    /// a write failed: no more attempts until the notebook is left or saved by hand
    save_failed: bool,
    /// the system's document picker was opened: its answer is looked for until this time (seconds of the frame clock)
    picking_until: f64,
    /// the names the completion offers now (from `completion_popup`, drawn above the keys)
    pub(super) completions: Vec<String>,
}

/// How long ago `t` was, in words.
fn age(t: SystemTime, now: SystemTime) -> String {
    let s = now.duration_since(t).map_or(0, |d| d.as_secs());
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        86_400..=172_799 => "yesterday".into(),
        172_800..=1_209_599 => format!("{} days ago", s / 86_400),
        1_209_600..=5_183_999 => format!("{} weeks ago", s / 604_800),
        _ => format!("{} months ago", s / 2_592_000),
    }
}

/// The folder of the user's notebooks: the app's folder on Android, `MCNB_DOCUMENTS` or a folder in the temporary
/// folder elsewhere (the layout is tried out there).
fn notebooks_dir() -> PathBuf {
    mc_ui::dirs::documents_dir()
        .or_else(|| std::env::var_os("MCNB_DOCUMENTS").map(PathBuf::from))
        .unwrap_or_else(|| std::env::temp_dir().join("mathcas-notebooks"))
}

/// The first `Calculation N.mcn` that `taken` does not say exists.
fn free_name(taken: impl Fn(&str) -> bool) -> String {
    (1..).map(|n| format!("Calculation {n}.mcn")).find(|n| !taken(n)).expect("an unused number")
}

/// The notebooks of `dir`, newest first.
fn list(dir: &Path) -> Vec<Entry> {
    let mut files: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mcn")))
        .map(|path| Entry {
            name: path.file_stem().map_or_else(String::new, |n| n.to_string_lossy().to_string()),
            modified: std::fs::metadata(&path).and_then(|m| m.modified()).ok(),
            path,
        })
        .collect();
    files.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    files
}

// ---------------------------------------------------------------------------------------- Material components
//
// The controls of Material Design 3, drawn with the roles of `theme::Material`: a control that is pressed or under
// the pointer is tinted with its content colour (a state layer); buttons are fully rounded and 40 pt high inside a
// 48 pt target; what floats has 16 pt corners and a shadow.

/// `color` at `alpha`, to lay over whatever is behind.
fn layer(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (alpha * 255.0).round() as u8)
}

/// The strength of the state layer of a control: pressed, under the pointer, or at rest.
fn state(resp: &egui::Response) -> f32 {
    if resp.is_pointer_button_down_on() {
        0.12
    } else if resp.hovered() {
        0.08
    } else {
        0.0
    }
}

/// An icon button: the icon in a 48 pt target, a round state layer behind it.
fn icon_button(ui: &mut egui::Ui, icon: Icon, tint: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TARGET, TARGET), Sense::click());
    ui.painter().circle_filled(rect.center(), 20.0, layer(tint, state(&resp)));
    icons::paint(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(24.0, 24.0)), tint);
    resp
}

/// The "more" icon button: three dots, one above the other.
fn more_button(ui: &mut egui::Ui, tint: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TARGET, TARGET), Sense::click());
    ui.painter().circle_filled(rect.center(), 20.0, layer(tint, state(&resp)));
    for dy in [-6.0, 0.0, 6.0] {
        ui.painter().circle_filled(rect.center() + vec2(0.0, dy), 2.0, tint);
    }
    resp
}

/// A button with a label, an icon before it when given: filled with `fill` (a filled or tonal button), or only
/// its label in `ink` when there is no fill (a text button). 40 pt high in a target of the row's height.
fn button(ui: &mut egui::Ui, icon: Option<Icon>, label: &str, fill: Option<Color32>, ink: Color32) -> bool {
    let text = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(15.0), ink);
    let pad = if fill.is_some() { 24.0 } else { 12.0 };
    let lead = if icon.is_some() { 26.0 } else { 0.0 };
    let width = text.size().x + 2.0 * pad + lead - if icon.is_some() { 8.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, TARGET.min(ui.available_height())), Sense::click());
    let face = Rect::from_center_size(rect.center(), vec2(width, 40.0));
    if let Some(f) = fill {
        ui.painter().rect_filled(face, 20.0, f);
    }
    ui.painter().rect_filled(face, 20.0, layer(ink, state(&resp)));
    let mut x = face.left() + pad - if icon.is_some() { 8.0 } else { 0.0 };
    if let Some(i) = icon {
        icons::paint(ui.painter(), i, Rect::from_center_size(pos2(x + 9.0, face.center().y), vec2(18.0, 18.0)), ink);
        x += lead;
    }
    ui.painter().galley(pos2(x, face.center().y - text.size().y / 2.0), text, ink);
    resp.clicked()
}

/// A chip: a small outlined control with a label in the code font (a name the completion offers).
fn chip(ui: &mut egui::Ui, m: &Material, label: &str) -> bool {
    let text = ui.painter().layout_no_wrap(label.to_string(), FontId::monospace(14.0), m.on_surface_variant);
    let width = text.size().x + 32.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ui.available_height()), Sense::click());
    let face = Rect::from_center_size(rect.center(), vec2(width, 32.0));
    ui.painter().rect(
        face,
        8.0,
        layer(m.on_surface_variant, state(&resp)),
        egui::Stroke::new(1.0, m.outline_variant),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(pos2(face.left() + 16.0, face.center().y - text.size().y / 2.0), text, m.on_surface_variant);
    resp.clicked()
}

/// A floating action button with its right bottom corner at `corner`: an icon, and a label when given (the extended
/// form), on the primary container with a shadow.
fn fab(ctx: &egui::Context, id: &str, corner: egui::Pos2, m: &Material, icon: Icon, label: Option<&str>) -> bool {
    let ink = m.on_primary_container;
    let text = label.map(|l| ctx.fonts_mut(|f| f.layout_no_wrap(l.to_string(), FontId::proportional(15.0), ink)));
    let width = text.as_ref().map_or(56.0, |t| 16.0 + 24.0 + 12.0 + t.size().x + 20.0);
    let at = corner - vec2(width, 56.0);
    let mut clicked = false;
    egui::Area::new(egui::Id::unique(("m-fab", id))).fixed_pos(at).order(egui::Order::Middle).show(ctx, |ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 56.0), Sense::click());
        // the shadow of a surface three levels up
        for (dy, grow, alpha) in [(1.0, 0.0, 34), (4.0, 2.0, 18), (7.0, 4.0, 8)] {
            let shadow = rect.translate(vec2(0.0, dy)).expand(grow);
            ui.painter().rect_filled(shadow, 16.0 + grow, Color32::from_black_alpha(alpha));
        }
        ui.painter().rect_filled(rect, 16.0, m.primary_container);
        ui.painter().rect_filled(rect, 16.0, layer(ink, state(&resp)));
        let x = if text.is_some() { rect.left() + 16.0 + 12.0 } else { rect.center().x };
        icons::paint(ui.painter(), icon, Rect::from_center_size(pos2(x, rect.center().y), vec2(24.0, 24.0)), ink);
        if let Some(t) = &text {
            ui.painter().galley(pos2(x + 12.0 + 12.0, rect.center().y - t.size().y / 2.0), t.clone(), ink);
        }
        clicked = resp.clicked();
    });
    clicked
}

/// A row of a sheet or a menu: a full-width target with its label.
fn sheet_row(ui: &mut egui::Ui, label: &str, color: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    ui.painter().rect_filled(rect, 0.0, layer(color, state(&resp)));
    let at = rect.left_center() + vec2(24.0, 0.0);
    ui.painter().text(at, egui::Align2::LEFT_CENTER, label, FontId::proportional(16.0), color);
    resp.clicked()
}

impl App {
    /// The mobile layout's screens (in place of the desktop window's menus, toolbar, tabs, status bar and canvas).
    pub(super) fn mobile_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.doc.view.metrics != Metrics::mobile() {
            self.doc.view.metrics = Metrics::mobile();
            self.doc.view.clear();
        }
        // the system's Back (and Esc): the sheet, then the notebook, close
        let back = ctx.input(|i| i.key_pressed(Key::BrowserBack))
            || (!self.modal_open() && self.doc.completion.is_none() && ctx.input(|i| i.key_pressed(Key::Escape)));
        let on_notebook = self.mobile.screen == Screen::Notebook;
        if back && self.mobile.sheet.take().is_none() && on_notebook && !self.mobile_modal() {
            self.leave_notebook();
        }
        // a file the system hands over (the picker's answer, "Open with"): looked for in every frame, and a few
        // times a second while the picker is open, since its answer comes when no frame is due
        self.import_pending();
        if ctx.input(|i| i.time) < self.mobile.picking_until {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
        match self.mobile.screen {
            Screen::Home => self.home(ui),
            Screen::Notebook => self.notebook_screen(ui),
        }
        self.sheet(&ctx);
        self.rename_dialog(&ctx);
        self.delete_dialog(&ctx);
        self.autosave(&ctx);
    }

    /// Start a new calculation (for the self-test: what the Home screen's button does).
    pub fn mobile_calc(&mut self) {
        if self.mobile.screen == Screen::Notebook {
            self.leave_notebook();
        }
        self.new_calculation();
    }

    /// Open a notebook from Home (for the self-test).
    pub fn mobile_open(&mut self, path: &Path) {
        if self.mobile.screen == Screen::Notebook {
            self.leave_notebook();
        }
        self.open_notebook(path);
    }

    /// Back to Home (for the self-test).
    pub fn mobile_home(&mut self) {
        if self.mobile.screen == Screen::Notebook {
            self.leave_notebook();
        }
    }

    /// The screen, the open notebook's file and what shows at the bottom (for the self-test).
    pub fn mobile_state(&self) -> String {
        match self.mobile.screen {
            Screen::Home => "home".to_string(),
            Screen::Notebook => {
                let file = self.path().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));
                let bottom = match self.mobile_selection() {
                    (true, _) => "keys",
                    (false, Some(_)) => "actions",
                    (false, None) => "add",
                };
                format!("notebook {} {bottom}", file.unwrap_or_else(|| "-".into()))
            }
        }
    }

    /// A bar shows at the bottom of the screen (the keys, or the actions of a selected result): the colour under
    /// Android's navigation bar follows it.
    #[cfg(target_os = "android")]
    pub(super) fn mobile_bottom_bar(&self) -> bool {
        self.mobile.screen == Screen::Notebook && self.mobile_selection() != (false, None)
    }

    /// A dialog of the mobile layout is open (they take the keys).
    pub(super) fn mobile_modal(&self) -> bool {
        self.mobile.sheet.is_some() || self.mobile.rename.is_some() || self.mobile.delete.is_some()
    }

    // ------------------------------------------------------------------------------------------------ Home

    fn home(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let m = Material::of(self.doc.view.palette.dark);
        // the top app bar: the title, and the dots for the rest
        egui::Panel::top("m-home-bar")
            .frame(Frame::new().fill(m.surface).inner_margin(Margin { left: 16, right: 4, top: 0, bottom: 0 }))
            .show_separator_line(false)
            .exact_size(TOP)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("MathCAS").size(22.0).color(m.on_surface));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if more_button(ui, m.on_surface_variant).clicked() {
                            self.mobile.sheet = Some(Sheet::More);
                        }
                    });
                });
            });
        let files = self.mobile.files.get_or_insert_with(|| list(&notebooks_dir())).clone();
        let mut open: Option<PathBuf> = None;
        let mut pick = false;
        let mut page = Rect::NOTHING;
        egui::CentralPanel::default().frame(Frame::new().fill(m.surface)).show(ui, |ui| {
            page = ui.available_rect_before_wrap();
            let side = ((ui.available_width() - COLUMN) / 2.0).max(0.0);
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let column = ui.available_rect_before_wrap().shrink2(vec2(side, 0.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(RichText::new("Notebooks").size(14.0).strong().color(m.primary));
                    });
                    ui.add_space(8.0);
                    if files.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            let none = "None yet. Start a calculation, or open a notebook file.";
                            ui.label(RichText::new(none).size(14.0).color(m.on_surface_variant));
                        });
                        ui.add_space(16.0);
                    }
                    let now = SystemTime::now();
                    for f in &files {
                        // a two-line list item: a leading circle, the name, when it was changed, the dots
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let w = ui.available_width() - TARGET - 4.0;
                            let (rect, resp) = ui.allocate_exact_size(vec2(w, 72.0), Sense::click());
                            let whole = rect.with_max_x(rect.right() + TARGET + 4.0);
                            ui.painter().rect_filled(whole, 0.0, layer(m.on_surface, state(&resp)));
                            let lead = rect.left_center() + vec2(16.0 + 20.0, 0.0);
                            ui.painter().circle_filled(lead, 20.0, m.primary_container);
                            let icon = Rect::from_center_size(lead, vec2(22.0, 22.0));
                            icons::paint(ui.painter(), Icon::Notebook, icon, m.on_primary_container);
                            let x = rect.left() + 72.0;
                            let name =
                                ui.painter().layout_no_wrap(f.name.clone(), FontId::proportional(16.0), m.on_surface);
                            let clip = Rect::from_min_max(rect.min, pos2(rect.right() - 4.0, rect.bottom()));
                            let top = rect.center().y - if f.modified.is_some() { 21.0 } else { 10.0 };
                            ui.painter().with_clip_rect(clip).galley(pos2(x, top), name, m.on_surface);
                            if let Some(t) = f.modified {
                                let at = pos2(x, rect.center().y + 3.0);
                                let font = FontId::proportional(14.0);
                                ui.painter().text(at, egui::Align2::LEFT_TOP, age(t, now), font, m.on_surface_variant);
                            }
                            if resp.clicked() {
                                open = Some(f.path.clone());
                            }
                            if more_button(ui, m.on_surface_variant).clicked() {
                                self.mobile.sheet = Some(Sheet::File(f.path.clone()));
                            }
                        });
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        pick = button(ui, None, "Open a file", Some(m.secondary_container), m.on_secondary_container);
                    });
                    // room for the floating button under the last row
                    ui.add_space(96.0);
                });
            });
        });
        // the main action floats at the bottom right
        let corner = page.right_bottom() - vec2(16.0, 16.0);
        let new = !self.modal_open() && fab(&ctx, "new", corner, &m, Icon::Plus, Some("New calculation"));
        if new {
            self.new_calculation();
        } else if let Some(path) = open {
            self.open_notebook(&path);
        } else if pick {
            self.open_other();
        }
    }

    /// A new notebook with an empty input cell, the caret in it.
    fn new_calculation(&mut self) {
        let ctx = self.ctx.clone();
        self.set_notebook(Notebook::new(), None);
        self.run(&ctx, Command::NewCell(Style::Input));
        self.enter_notebook();
    }

    /// Open the notebook at `path` (it replaces the one that was open; Home has none).
    pub(super) fn open_notebook(&mut self, path: &Path) {
        self.error = None;
        self.open(path);
        if self.path().as_deref() == Some(path) {
            self.enter_notebook();
        }
    }

    fn enter_notebook(&mut self) {
        self.mobile.screen = Screen::Notebook;
        self.mobile.sheet = None;
        self.mobile.save_failed = false;
        self.mobile.saved_at = self.ctx.input(|i| i.time);
    }

    /// "Open a file": a file from elsewhere is copied into the notebooks folder and opened.
    fn open_other(&mut self) {
        match mc_ui::platform::pick_document() {
            mc_ui::platform::Picked::Path(p) => self.import(&p),
            // the answer comes later (`import_pending`)
            mc_ui::platform::Picked::Later => self.mobile.picking_until = self.ctx.input(|i| i.time) + 300.0,
            mc_ui::platform::Picked::None => {}
        }
    }

    /// Files the system handed to the app (the document picker's answer, "Open with"): copied in and opened.
    fn import_pending(&mut self) {
        if let Some(p) = mc_ui::platform::take_incoming() {
            self.mobile.picking_until = 0.0;
            if self.mobile.screen == Screen::Notebook {
                self.leave_notebook();
            }
            self.import(&p);
        }
    }

    /// Copy the notebook at `from` into the notebooks folder (under a free name) and open it; a file already in the
    /// folder is opened where it is.
    fn import(&mut self, from: &Path) {
        let dir = notebooks_dir();
        let _ = std::fs::create_dir_all(&dir);
        if from.parent() == Some(dir.as_path()) {
            return self.open_notebook(from);
        }
        let stem = from.file_stem().map_or_else(|| "Notebook".to_string(), |s| s.to_string_lossy().to_string());
        let name = (0..)
            .map(|n| if n == 0 { format!("{stem}.mcn") } else { format!("{stem} ({n}).mcn") })
            .find(|n| !dir.join(n).exists())
            .expect("an unused name");
        let to = dir.join(name);
        match std::fs::copy(from, &to) {
            Ok(_) => {
                self.mobile.files = None;
                self.open_notebook(&to);
            }
            Err(e) => self.error = Some(format!("Cannot open {}: {e}", from.display())),
        }
    }

    // -------------------------------------------------------------------------------------------- Notebook

    fn notebook_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let p = self.doc.view.palette.clone();
        let title = match self.path() {
            Some(path) => path.file_stem().map_or_else(String::new, |n| n.to_string_lossy().to_string()),
            None => "New calculation".to_string(),
        };
        let busy = self.doc.nb.kernel_state() == KernelState::Busy;
        let mut leave = false;
        let m = Material::of(p.dark);
        // the top app bar: back, the name, the kernel, the dots
        egui::Panel::top("m-bar")
            .frame(Frame::new().fill(m.surface).inner_margin(Margin::symmetric(4, 0)))
            .show_separator_line(false)
            .exact_size(TOP)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    leave = icon_button(ui, Icon::ArrowLeft, m.on_surface).clicked();
                    ui.add_space(4.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if more_button(ui, m.on_surface_variant).clicked() {
                            self.mobile.sheet = Some(Sheet::More);
                        }
                        // the kernel: a dot, or the stop button while it works
                        if busy {
                            if icon_button(ui, Icon::Abort, m.error).clicked() {
                                self.run(&ctx, Command::Abort);
                            }
                        } else {
                            let (color, _) = self.kernel_indicator();
                            let (rect, _) = ui.allocate_exact_size(vec2(28.0, TARGET), Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4.5, color);
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let text = RichText::new(&title).size(22.0).color(m.on_surface);
                            let name = ui.add(egui::Label::new(text).truncate().sense(Sense::click()));
                            if name.clicked() {
                                if let Some(path) = self.path() {
                                    self.mobile.rename = Some((path, title.clone(), true));
                                }
                            }
                        });
                    });
                });
            });
        // what the selection calls for at the bottom
        let (editing, picked) = self.mobile_selection();
        if editing {
            // the keys lie on the keyboard, the completions above them
            let completing = !self.mobile.completions.is_empty();
            egui::Panel::bottom("m-keys")
                .frame(Frame::new().fill(m.surface_container).inner_margin(Margin::symmetric(8, 0)))
                .show_separator_line(false)
                .exact_size(if completing { BAR + STRIP } else { BAR })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if completing {
                        ui.allocate_ui(vec2(ui.available_width(), STRIP), |ui| self.completion_strip(ui));
                    }
                    ui.allocate_ui(vec2(ui.available_width(), BAR), |ui| self.key_row(ui));
                });
        } else if let Some(id) = picked {
            egui::Panel::bottom("m-actions")
                .frame(Frame::new().fill(m.surface_container).inner_margin(Margin::symmetric(8, 0)))
                .show_separator_line(false)
                .exact_size(BAR)
                .show(ui, |ui| self.action_row(ui, id));
        }
        let mut canvas = Rect::NOTHING;
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| {
            // the content column: the whole width of a phone, the middle of a wider screen
            let side = ((ui.available_width() - COLUMN) / 2.0).max(0.0);
            canvas = ui.available_rect_before_wrap().shrink2(vec2(side, 0.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(canvas), |ui| self.canvas(ui));
        });
        // a new input at the end
        if !editing && picked.is_none() && !self.modal_open() {
            let corner = pos2(ui.ctx().content_rect().right(), canvas.bottom()) - vec2(16.0, 16.0);
            if fab(&ctx, "add", corner, &m, Icon::Plus, None) {
                let end = self.doc.nb.cells().len();
                self.run(&ctx, Command::ClickGap(end));
                self.run(&ctx, Command::NewCell(Style::Input));
                self.doc.scroll_pending = true;
            }
        }
        if leave {
            self.leave_notebook();
        }
    }

    /// Whether a cell is being edited (the keys show), else the result or picture that is selected (its actions show).
    fn mobile_selection(&self) -> (bool, Option<mc_nbmodel::CellId>) {
        if let Some(id) = self.doc.view.graphic {
            return (false, Some(id));
        }
        match self.doc.nb.selection() {
            Selection::Caret { cell, .. } => match self.doc.nb.cell(*cell) {
                Some(c) if c.generated() => (false, Some(*cell)),
                Some(_) => (true, None),
                None => (false, None),
            },
            _ => (false, None),
        }
    }

    /// The row of keys above the on-screen keyboard: Run on the right, the keys scrolling sideways beside it.
    fn key_row(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let m = Material::of(self.doc.view.palette.dark);
        let mut fire: Option<Command> = None;
        let mut tab = false;
        ui.horizontal_centered(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // the main action: a filled button
                if button(ui, Some(Icon::Evaluate), "Run", Some(m.primary), m.on_primary) {
                    fire = Some(Command::Evaluate);
                }
                ui.add_space(4.0);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let bars = egui::scroll_area::ScrollBarVisibility::AlwaysHidden;
                    ScrollArea::horizontal().scroll_bar_visibility(bars).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        // a key: its character in the code font, a round state layer when pressed
                        let key = |ui: &mut egui::Ui, label: &str| {
                            let (rect, resp) = ui.allocate_exact_size(vec2(42.0, TARGET), Sense::click());
                            let face = Rect::from_center_size(rect.center(), vec2(40.0, 40.0));
                            ui.painter().rect_filled(face, 20.0, layer(m.on_surface, state(&resp)));
                            let font = FontId::monospace(if label.len() > 1 { 15.0 } else { 20.0 });
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, font, m.on_surface);
                            resp.clicked()
                        };
                        for k in KEYS {
                            if key(ui, k) {
                                fire = Some(Command::Insert(k.to_string()));
                            }
                        }
                        if icon_button(ui, Icon::ArrowLeft, m.on_surface_variant).clicked() {
                            fire = Some(Command::Move(mc_nbmodel::Motion::Left, false));
                        }
                        if icon_button(ui, Icon::ArrowRight, m.on_surface_variant).clicked() {
                            fire = Some(Command::Move(mc_nbmodel::Motion::Right, false));
                        }
                        tab = key(ui, "tab");
                    });
                });
            });
        });
        if tab {
            // the first completion when there is one, else the Tab key (the next placeholder, an indent)
            if !self.accept_completion(&ctx, 0) {
                fire = Some(Command::Tab);
            }
        }
        if let Some(c) = fire {
            self.run(&ctx, c);
        }
    }

    /// The names the completion offers, each a tap away.
    fn completion_strip(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let names = self.mobile.completions.clone();
        let m = Material::of(self.doc.view.palette.dark);
        let mut chosen = None;
        ui.horizontal_centered(|ui| {
            let bars = egui::scroll_area::ScrollBarVisibility::AlwaysHidden;
            ScrollArea::horizontal().id_salt("m-complete").scroll_bar_visibility(bars).show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for (k, name) in names.iter().enumerate() {
                    if chip(ui, &m, name) {
                        chosen = Some(k);
                    }
                }
            });
        });
        if let Some(k) = chosen {
            self.accept_completion(&ctx, k);
        }
    }

    /// Take completion `k` of the open completion (as Enter does in the desktop's list); `false` when none is open.
    fn accept_completion(&mut self, ctx: &egui::Context, k: usize) -> bool {
        let Some(c) = &mut self.doc.completion else { return false };
        c.selected = k;
        let enter = egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let used = self.completion_key(ctx, &enter);
        self.mobile.completions.clear();
        used
    }

    /// What can be done with the selected result or picture.
    fn action_row(&mut self, ui: &mut egui::Ui, id: mc_nbmodel::CellId) {
        let ctx = ui.ctx().clone();
        let m = Material::of(self.doc.view.palette.dark);
        let Some(cell) = self.doc.nb.cell(id) else { return };
        let picture = cell.graphic().is_some();
        let text = cell.text.clone();
        let (mut copy, mut latex, mut share, mut delete) = (false, false, false, false);
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_space(4.0);
            // sharing is what a selected result is mostly for: a tonal button; the rest are text buttons
            share = button(ui, None, "Share", Some(m.secondary_container), m.on_secondary_container);
            copy = button(ui, Some(Icon::Copy), "Copy", None, m.primary);
            if !picture {
                latex = button(ui, None, "LaTeX", None, m.primary);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                delete = button(ui, None, "Delete", None, m.error);
            });
        });
        if copy && picture {
            self.copy_graphic(id);
        } else if copy {
            self.clip.set(Contents { text: Some(text.clone()), ..Contents::default() });
        } else if latex {
            self.run(&ctx, Command::CopyLatex);
        } else if share && picture {
            self.share_picture(id);
        } else if share {
            mc_ui::platform::share_text(&text);
        } else if delete {
            self.doc.view.graphic = None;
            self.run(&ctx, Command::ClickBracket(mc_nbmodel::Bracket::Cell(id), false, false));
            self.run(&ctx, Command::Delete);
        }
    }

    /// Hand the picture of Output cell `id` to another app as a PNG.
    fn share_picture(&mut self, id: mc_nbmodel::CellId) {
        let Some(path) = self.doc.nb.cell(id).and_then(|c| c.graphic()) else { return };
        let path = mc_nbfile::images::resolve(&path, self.doc.view.base.as_deref());
        // a drawing is rasterised at a size that stays sharp on a large screen
        let png = match mc_nbfile::images::ImageFormat::of(&path) {
            Some(mc_nbfile::images::ImageFormat::Png) => std::fs::read(&path).map_err(|e| e.to_string()),
            _ => crate::images::pixels(&path, 1600)
                .and_then(|(rgba, w, h)| mc_ui::clipboard::encode_png(&rgba, w, h).ok_or_else(|| "no PNG".to_string())),
        };
        let stem = self.path().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()));
        let name = format!("{}-plot.png", stem.unwrap_or_else(|| "mathcas".into()));
        match png.and_then(|b| mc_ui::platform::share_bytes(&name, "image/png", &b)) {
            Ok(()) => {}
            Err(e) => self.error = Some(format!("Cannot share the picture: {e}")),
        }
    }

    /// Hand the notebook itself (`pdf`: as a PDF document) to another app.
    fn share_notebook(&mut self, pdf: bool) {
        self.save_now();
        let Some(path) = self.path() else { return };
        let stem = path.file_stem().map_or_else(|| "notebook".to_string(), |s| s.to_string_lossy().to_string());
        let result = if pdf {
            let base = self.doc.view.base.clone().unwrap_or_default();
            mc_nbexport::to_pdf(&self.doc.nb.to_file(), &base)
                .map_err(|e| e.to_string())
                .and_then(|b| mc_ui::platform::share_bytes(&format!("{stem}.pdf"), "application/pdf", &b))
        } else {
            std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|b| mc_ui::platform::share_bytes(&format!("{stem}.mcn"), "application/octet-stream", &b))
        };
        if let Err(e) = result {
            self.error = Some(format!("Cannot share {stem}: {e}"));
        }
    }

    /// Back to Home: the notebook is written and closed (its kernel ends); one without content leaves no file.
    fn leave_notebook(&mut self) {
        self.save_now();
        self.doc.view.graphic = None;
        self.set_notebook(Notebook::new(), None);
        self.mobile.screen = Screen::Home;
        self.mobile.files = None;
        self.mobile.sheet = None;
        self.mobile.completions.clear();
    }

    // ---------------------------------------------------------------------------------------------- saving

    /// Write the notebook when it changed and [`AUTOSAVE`] seconds have passed since the last write.
    fn autosave(&mut self, ctx: &egui::Context) {
        if self.mobile.screen != Screen::Notebook || !self.doc.nb.is_modified() || self.mobile.save_failed {
            return;
        }
        let now = ctx.input(|i| i.time);
        let due = self.mobile.saved_at + AUTOSAVE;
        if now >= due {
            self.save_now();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now));
        }
    }

    /// Write the notebook to its file now; one without a file gets `Calculation N.mcn` once it has content.
    fn save_now(&mut self) {
        self.mobile.saved_at = self.ctx.input(|i| i.time);
        if !self.doc.nb.is_modified() {
            return;
        }
        let path = match self.path() {
            Some(p) => p,
            None => {
                if self.doc.nb.cells().iter().all(|c| c.text.trim().is_empty()) {
                    return;
                }
                let dir = notebooks_dir();
                let _ = std::fs::create_dir_all(&dir);
                dir.join(free_name(|n| dir.join(n).exists()))
            }
        };
        let status = std::mem::take(&mut self.status);
        self.mobile.save_failed = !self.save_to(&path);
        self.status = status;
    }

    // ------------------------------------------------------------------------------------- sheet and dialogs

    /// The sheet: a card at the bottom of the screen with the commands that are not on it.
    fn sheet(&mut self, ctx: &egui::Context) {
        let Some(sheet) = self.mobile.sheet.clone() else { return };
        let p = self.doc.view.palette.clone();
        let m = Material::of(p.dark);
        // a Material bottom sheet: the width of the screen (640 pt at most), rising from its bottom edge, the top
        // corners rounded, a handle at the top; it reaches under the system's navigation bar
        let screen = ctx.content_rect();
        let width = screen.width().min(640.0);
        let id = egui::Id::unique("m-sheet");
        let (_, bottom) = mc_ui::platform::bars(ctx);
        let area = egui::Modal::default_area(id).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, 0.0));
        let corners = egui::CornerRadius { nw: 28, ne: 28, sw: 0, se: 0 };
        let frame = Frame::new().fill(m.surface_low).corner_radius(corners).inner_margin(Margin::ZERO);
        let mut close = false;
        let mut act: Option<&'static str> = None;
        let r = egui::Modal::new(id).area(area).backdrop_color(m.scrim).frame(frame).show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (grip, _) = ui.allocate_exact_size(vec2(width, 36.0), Sense::hover());
            let handle = Rect::from_center_size(grip.center(), vec2(32.0, 4.0));
            ui.painter().rect_filled(handle, 2.0, layer(m.on_surface_variant, 0.4));
            match &sheet {
                Sheet::More if self.mobile.screen == Screen::Notebook => {
                    for (label, what) in [
                        ("Evaluate all", "all"),
                        ("Clear results", "clear"),
                        ("Delete this cell", "delete-cell"),
                        ("Restart kernel", "restart"),
                        ("Share notebook", "share"),
                        ("Share as PDF", "share-pdf"),
                    ] {
                        if what == "delete-cell" && self.doc.nb.current_cell().is_none() {
                            continue;
                        }
                        if sheet_row(ui, label, p.text) {
                            act = Some(what);
                        }
                    }
                    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 17.0), Sense::hover());
                    ui.painter().hline(line.x_range(), line.center().y, egui::Stroke::new(1.0, m.outline_variant));
                }
                Sheet::More => {}
                Sheet::File(_) => {
                    for (label, what, color) in
                        [("Rename", "rename", p.text), ("Share", "share-file", p.text), ("Delete", "delete-file", p.red)]
                    {
                        if sheet_row(ui, label, color) {
                            act = Some(what);
                        }
                    }
                }
            }
            if sheet == Sheet::More {
                let theme = match self.settings.theme {
                    ThemeChoice::System => "Theme: system",
                    ThemeChoice::Light => "Theme: light",
                    ThemeChoice::Dark => "Theme: dark",
                };
                if sheet_row(ui, theme, p.text) {
                    act = Some("theme");
                }
                if self.mobile.screen == Screen::Notebook {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.add_space(24.0);
                        let size = format!("Text size: {:.0} %", self.doc.view.magnification() * 100.0);
                        let w = ui.available_width() - 2.0 * 64.0 - 16.0 - 16.0;
                        let (rect, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
                        let font = FontId::proportional(16.0);
                        ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, size, font, p.text);
                        // two tonal buttons
                        if button(ui, None, "\u{2212}", Some(m.secondary_container), m.on_secondary_container) {
                            act = Some("smaller");
                        }
                        if button(ui, None, "+", Some(m.secondary_container), m.on_secondary_container) {
                            act = Some("larger");
                        }
                    });
                }
                if sheet_row(ui, "About MathCAS", p.text) {
                    act = Some("about");
                }
            }
            // above the navigation bar the sheet lies under
            ui.add_space(12.0 + bottom);
        });
        close |= r.should_close();
        let ctx = ctx.clone();
        match (act, &sheet) {
            (Some("all"), _) => self.run(&ctx, Command::EvaluateNotebook),
            (Some("clear"), _) => self.run(&ctx, Command::DeleteAllOutput),
            (Some("delete-cell"), _) => {
                if let Some(id) = self.doc.nb.current_cell() {
                    self.doc.view.graphic = None;
                    self.run(&ctx, Command::ClickBracket(mc_nbmodel::Bracket::Cell(id), false, false));
                    self.run(&ctx, Command::Delete);
                }
            }
            (Some("restart"), _) => self.run(&ctx, Command::QuitKernel),
            (Some("share"), _) => self.share_notebook(false),
            (Some("share-pdf"), _) => self.share_notebook(true),
            (Some("theme"), _) => {
                self.settings.theme = match self.settings.theme {
                    ThemeChoice::System => ThemeChoice::Light,
                    ThemeChoice::Light => ThemeChoice::Dark,
                    ThemeChoice::Dark => ThemeChoice::System,
                };
                self.settings.save();
            }
            (Some("smaller"), _) => self.step_zoom(-1),
            (Some("larger"), _) => self.step_zoom(1),
            (Some("about"), _) => self.about = true,
            (Some("rename"), Sheet::File(path)) => {
                let name = path.file_stem().map_or_else(String::new, |n| n.to_string_lossy().to_string());
                self.mobile.rename = Some((path.clone(), name, true));
            }
            (Some("share-file"), Sheet::File(path)) => {
                let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().to_string());
                let shared = std::fs::read(path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| mc_ui::platform::share_bytes(&name, "application/octet-stream", &b));
                if let Err(e) = shared {
                    self.error = Some(format!("Cannot share {name}: {e}"));
                }
            }
            (Some("delete-file"), Sheet::File(path)) => self.mobile.delete = Some(path.clone()),
            _ => {}
        }
        // the theme and the text size are changed in steps: the sheet stays for the next one
        if close || act.is_some_and(|a| !matches!(a, "theme" | "smaller" | "larger")) {
            self.mobile.sheet = None;
        }
    }

    /// The name of a notebook, changed.
    fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some((path, mut name, focus)) = self.mobile.rename.take() else { return };
        let (mut done, mut cancel) = (false, false);
        let shown = dialog::show(ctx, "m-rename", "Rename", 320.0, true, |ui| {
            let edit = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
            if focus {
                edit.request_focus();
            }
            dialog::buttons(ui, |ui| {
                if dialog::primary(ui, "Rename").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    done = true;
                }
                if dialog::button(ui, "Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        let target = pick::file_name_of(&name, "mcn").map(|n| path.with_file_name(n));
        match target {
            Some(to) if done && to != path => {
                if to.exists() {
                    self.error = Some(format!("There is a notebook named {} already.", name.trim()));
                } else if self.path().as_deref() == Some(path.as_path()) {
                    // the open notebook: written under the new name, the old file goes
                    if self.save_to(&to) {
                        let _ = std::fs::remove_file(&path);
                    }
                } else if let Err(e) = std::fs::rename(&path, &to) {
                    self.error = Some(format!("Cannot rename {}: {e}", path.display()));
                }
                self.mobile.files = None;
            }
            _ if done || cancel || shown.closed => {}
            _ => self.mobile.rename = Some((path, name, false)),
        }
    }

    /// The question before a notebook's file is deleted.
    fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.mobile.delete.clone() else { return };
        let name = path.file_stem().map_or_else(String::new, |n| n.to_string_lossy().to_string());
        let red = self.doc.view.palette.red;
        let mut answer = None;
        dialog::show(ctx, "m-delete", "Delete notebook", dialog::QUESTION_WIDTH, false, |ui| {
            dialog::message(ui, Icon::Warning, red, &format!("Delete {name}? It cannot be brought back."));
            dialog::buttons(ui, |ui| {
                if dialog::primary(ui, "Delete").clicked() {
                    answer = Some(true);
                }
                if dialog::button(ui, "Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        if let Some(yes) = answer {
            if yes {
                if let Err(e) = std::fs::remove_file(&path) {
                    self.error = Some(format!("Cannot delete {}: {e}", path.display()));
                }
                self.mobile.files = None;
            }
            self.mobile.delete = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn ages() {
        let now = SystemTime::now();
        let ago = |s: u64| age(now - Duration::from_secs(s), now);
        assert_eq!(ago(5), "just now");
        assert_eq!(ago(125), "2 min ago");
        assert_eq!(ago(3 * 3600 + 5), "3 h ago");
        assert_eq!(ago(30 * 3600), "yesterday");
        assert_eq!(ago(4 * 86_400), "4 days ago");
        assert_eq!(ago(21 * 86_400), "3 weeks ago");
        assert_eq!(ago(100 * 86_400), "3 months ago");
        // a clock set back: not negative
        assert_eq!(age(now + Duration::from_secs(60), now), "just now");
    }

    #[test]
    fn names_and_lists() {
        assert_eq!(free_name(|_| false), "Calculation 1.mcn");
        assert_eq!(free_name(|n| n == "Calculation 1.mcn" || n == "Calculation 2.mcn"), "Calculation 3.mcn");
        let dir = std::env::temp_dir().join(format!("mcnb-mobile-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["a.mcn", "b.MCN", "c.txt"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        let mut names: Vec<String> = list(&dir).into_iter().map(|e| e.name).collect();
        names.sort();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
