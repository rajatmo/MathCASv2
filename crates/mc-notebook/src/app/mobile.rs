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
/// A notebook screen lower than this (points) shows no top app bar: the on-screen keyboard of a phone held sideways
/// leaves about half of that.
const LOW: f32 = 260.0;
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
/// The keys that make typeset boxes, at the end of the row (after the arrows and tab).
const BOXES: [(&str, mc_nbmodel::Structure); 3] = [
    ("a/b", mc_nbmodel::Structure::Fraction),
    ("x\u{207f}", mc_nbmodel::Structure::Superscript),
    ("\u{221a}", mc_nbmodel::Structure::Radical),
];

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
    /// a cell of the notebook (a long press on it)
    Cell(mc_nbmodel::CellId),
}

/// The line that shows for a few seconds after a cell was deleted, with its way back.
#[derive(Clone, Debug)]
struct Snack {
    text: &'static str,
    /// how many undo steps bring back what was deleted
    undo: usize,
    /// until when it shows (seconds of the frame clock)
    until: f64,
    /// the notebook's structure when it came up: once cells come or go otherwise, Undo would undo something else
    rev: u64,
    /// it came up in this frame
    fresh: bool,
}

/// How long (seconds) the line after a deletion shows.
const SNACK_TIME: f64 = 6.0;

/// Where the controls of the layout were drawn in this frame, by label (the self-test taps them by name).
fn targets() -> egui::Id {
    egui::Id::unique("m-targets")
}

fn mark(ui: &egui::Ui, label: &str, rect: Rect) {
    ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<Vec<(String, Rect)>>(targets()).push((label.to_string(), rect)));
}

/// The state of the mobile layout.
#[derive(Default)]
pub(super) struct Mobile {
    screen: Screen,
    /// when the screen was entered (seconds of the frame clock): it fades in from then
    entered: f64,
    /// the sheet that shows (also while it slides away)
    sheet: Option<Sheet>,
    /// the sheet is up, or on its way up (`false`: it slides away, then `sheet` is `None`)
    sheet_up: bool,
    /// the height of the sheet when it was last drawn (points): how far it slides
    sheet_height: f32,
    /// what the bar at the bottom of the notebook last showed (it is drawn while it slides away)
    bottom: Option<Bottom>,
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
    /// a long press in the text of a cell: Cut, Copy, Paste and Select all show above the keys until the next key
    edit_tools: bool,
    /// the notebook screen is low (the on-screen keyboard of a phone held sideways is up): what is not needed to
    /// type gives way
    pub(super) low: bool,
    /// the cell a long press came down on, when lifting the finger in place opens its sheet
    held: Option<mc_nbmodel::CellId>,
    /// the line after a deletion
    snack: Option<Snack>,
}

impl Mobile {
    /// Bring up the sheet.
    fn show_sheet(&mut self, sheet: Sheet) {
        self.sheet = Some(sheet);
        self.sheet_up = true;
    }

    /// Whether a sheet or a question of the layout lies over the screen.
    pub(super) fn covered(&self) -> bool {
        self.sheet.is_some() || self.rename.is_some() || self.delete.is_some()
    }

    /// The sheet is gone at once (the screen changes).
    fn drop_sheet(&mut self) {
        self.sheet = None;
        self.sheet_up = false;
    }
}

/// What the bar at the bottom of the notebook is for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Bottom {
    /// the keys, while a cell is edited
    Keys,
    /// what can be done with the selected result or picture
    Actions(mc_nbmodel::CellId),
}

/// How long (seconds) a screen takes to fade in, the sheet to slide, and a bar or a floating button to come and go.
const SCREEN_TIME: f64 = 0.22;
const SHEET_TIME: f32 = 0.26;
const BAR_TIME: f32 = 0.16;

/// The time an animation takes: `seconds`, or none in a self-test (its screenshots show the screen at rest).
fn time(ctx: &egui::Context, seconds: f32) -> f32 {
    if ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("m-still"))).unwrap_or(false) {
        0.0
    } else {
        seconds
    }
}

/// Fast at first, then coming to rest (what enters the screen).
fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
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

/// The strength of the state layer of a control: pressed, under the pointer, or at rest. It comes and goes over a
/// tenth of a second.
fn state(resp: &egui::Response) -> f32 {
    let target = if resp.is_pointer_button_down_on() {
        0.12
    } else if resp.hovered() {
        0.08
    } else {
        0.0
    };
    resp.ctx.animate_value_with_time(resp.id.with("m-state"), target, 0.1)
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
    let text = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(16.0), ink);
    let pad = if fill.is_some() { 24.0 } else { 12.0 };
    let lead = if icon.is_some() { 26.0 } else { 0.0 };
    let width = text.size().x + 2.0 * pad + lead - if icon.is_some() { 8.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, TARGET.min(ui.available_height())), Sense::click());
    mark(ui, label, rect);
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

/// A chip: a small outlined control with a label, in the code font (`code`: a name the completion offers) or the
/// font of the other controls.
fn chip(ui: &mut egui::Ui, m: &Material, label: &str, code: bool) -> bool {
    let font = if code { FontId::monospace(16.0) } else { FontId::proportional(15.0) };
    let text = ui.painter().layout_no_wrap(label.to_string(), font, m.on_surface);
    let width = text.size().x + 32.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ui.available_height()), Sense::click());
    mark(ui, label, rect);
    let face = Rect::from_center_size(rect.center(), vec2(width, 32.0));
    ui.painter().rect(
        face,
        8.0,
        layer(m.on_surface_variant, state(&resp)),
        egui::Stroke::new(1.0, m.outline_variant),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(pos2(face.left() + 16.0, face.center().y - text.size().y / 2.0), text, m.on_surface);
    resp.clicked()
}

/// A floating action button with its right bottom corner at `corner`: an icon, and a label when given (the extended
/// form), on the primary container with a shadow. It rises into place when `shown` turns true and sinks away when
/// it turns false; `alpha` is the opacity of the screen it floats on.
#[allow(clippy::too_many_arguments)]
fn fab(
    ctx: &egui::Context,
    id: &str,
    corner: egui::Pos2,
    m: &Material,
    icon: Icon,
    label: Option<&str>,
    shown: bool,
    alpha: f32,
) -> bool {
    let k = ease(ctx.animate_bool_with_time(egui::Id::unique(("m-fab-shown", id)), shown, time(ctx, BAR_TIME)));
    if k <= 0.0 {
        return false;
    }
    let ink = m.on_primary_container;
    let text = label.map(|l| ctx.fonts_mut(|f| f.layout_no_wrap(l.to_string(), FontId::proportional(16.0), ink)));
    let width = text.as_ref().map_or(56.0, |t| 16.0 + 24.0 + 12.0 + t.size().x + 20.0);
    let at = corner - vec2(width, 56.0) + vec2(0.0, (1.0 - k) * 20.0);
    let mut clicked = false;
    egui::Area::new(egui::Id::unique(("m-fab", id))).fixed_pos(at).order(egui::Order::Middle).show(ctx, |ui| {
        ui.set_opacity(k * alpha);
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
        clicked = shown && resp.clicked();
    });
    clicked
}

/// A row of a sheet or a menu: a full-width target with its label.
fn sheet_row(ui: &mut egui::Ui, label: &str, color: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    mark(ui, label, rect);
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
        ctx.data_mut(|d| d.remove::<Vec<(String, Rect)>>(targets()));
        // typing puts the tools of a long press away, and it is no longer the deletion that Undo would undo
        let typed = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(e, egui::Event::Text(_) | egui::Event::Ime(egui::ImeEvent::Commit(_)) | egui::Event::Key { pressed: true, .. })
            })
        });
        if typed {
            self.mobile.edit_tools = false;
            // (not the key that deleted: Backspace in an empty cell)
            self.mobile.snack.take_if(|s| !s.fresh);
        }
        if let Some(s) = &mut self.mobile.snack {
            s.fresh = false;
        }
        // the system's Back (and Esc): the sheet, then the notebook, close
        let back = ctx.input(|i| i.key_pressed(Key::BrowserBack))
            || (!self.modal_open() && self.doc.completion.is_none() && ctx.input(|i| i.key_pressed(Key::Escape)));
        let on_notebook = self.mobile.screen == Screen::Notebook;
        if back && self.mobile.sheet_up {
            self.mobile.sheet_up = false;
        } else if back && on_notebook && !self.mobile_modal() {
            self.leave_notebook();
        }
        // a long press is the notebook's to take; one on Home is nothing
        if !on_notebook {
            self.touch.take_long_press();
        }
        // the screen that was just entered fades in
        let alpha = self.screen_alpha(&ctx);
        if alpha < 1.0 {
            ui.set_opacity(alpha);
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

    /// How far the screen has faded in since it was entered (0 to 1); a frame follows while it is not there.
    fn screen_alpha(&self, ctx: &egui::Context) -> f32 {
        let still = self.selftest.is_some();
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("m-still"), still));
        if still {
            return 1.0;
        }
        let t = ((ctx.input(|i| i.time) - self.mobile.entered) / SCREEN_TIME) as f32;
        if t < 1.0 {
            ctx.request_repaint();
        }
        ease(t)
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

    /// What lies over or beside the notebook (for the self-test): the sheet that is up, whether the tools of a long
    /// press show above the keys, the line after a deletion, and how many cells the notebook has.
    pub fn mobile_overlays(&mut self) -> String {
        let sheet = match (&self.mobile.sheet, self.mobile.sheet_up) {
            (Some(Sheet::More), true) => "more",
            (Some(Sheet::File(_)), true) => "file",
            (Some(Sheet::Cell(_)), true) => "cell",
            _ => "none",
        };
        let editing = self.mobile_selection().0;
        let tools = editing && (self.mobile.edit_tools || self.text_selected());
        let snack = self.mobile.snack.as_ref().map_or("none", |s| s.text);
        // the caret: whether it lies in what shows of the notebook (its middle and most of its height: on the one
        // line a phone held sideways shows above its keyboard the line's edges are cut)
        let caret = match *self.doc.nb.selection() {
            Selection::Caret { cell, pos, .. } => {
                let ctx = self.ctx.clone();
                let r = self.doc.view.caret_rect(&ctx, &self.doc.nb, cell, pos);
                let seen = r.is_some_and(|r| {
                    let r = r.translate(self.doc.view.origin.to_vec2());
                    let inside = r.intersect(self.canvas_clip);
                    self.canvas_clip.contains(r.center()) && inside.height() >= 0.7 * r.height()
                });
                if seen { "shown" } else { "hidden" }
            }
            _ => "none",
        };
        format!("sheet {sheet}, tools {tools}, snack {snack}, cells {}, caret {caret}", self.doc.nb.cells().len())
    }

    /// The middle of the control of the mobile layout drawn last with `label` (for the self-test: it is tapped
    /// there as a finger taps it).
    pub fn mobile_target(&self, label: &str) -> Option<egui::Pos2> {
        let all = self.ctx.data(|d| d.get_temp::<Vec<(String, Rect)>>(targets())).unwrap_or_default();
        all.iter().rev().find(|(l, _)| l == label).map(|(_, r)| r.center())
    }

    /// Scroll so that cell `i` starts at the top of what shows of the notebook (for the self-test).
    pub fn show_cell(&mut self, i: usize) {
        if i < self.doc.nb.cells().len() {
            self.doc.restore_offset = Some(self.doc.view.cell_rect(i).top() - 4.0);
        }
    }

    /// A point of cell `i` on the screen (for the self-test): in its text at byte `at`, or the middle of what
    /// shows of the cell (a result, a picture).
    pub fn cell_point(&mut self, i: usize, at: Option<usize>) -> Option<egui::Pos2> {
        let id = self.doc.nb.cells().get(i)?.id;
        let origin = self.doc.view.origin.to_vec2();
        if let Some(pos) = at {
            let ctx = self.ctx.clone();
            let r = self.doc.view.caret_rect(&ctx, &self.doc.nb, id, pos)?;
            return Some(r.center() + origin + vec2(1.0, 0.0));
        }
        let r = match self.doc.view.graphic_rect(&self.doc.nb, i) {
            Some(r) => r,
            None => self.doc.view.cell_rect(i),
        };
        Some(r.translate(origin).intersect(self.canvas_clip).center())
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
                            self.mobile.show_sheet(Sheet::More);
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
                        ui.label(RichText::new("Notebooks").size(15.0).strong().color(m.primary));
                    });
                    ui.add_space(8.0);
                    if files.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            let none = "None yet. Start a calculation, or open a notebook file.";
                            ui.label(RichText::new(none).size(15.0).color(m.on_surface_variant));
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
                                ui.painter().layout_no_wrap(f.name.clone(), FontId::proportional(17.0), m.on_surface);
                            let clip = Rect::from_min_max(rect.min, pos2(rect.right() - 4.0, rect.bottom()));
                            let top = rect.center().y - if f.modified.is_some() { 22.0 } else { 10.0 };
                            ui.painter().with_clip_rect(clip).galley(pos2(x, top), name, m.on_surface);
                            if let Some(t) = f.modified {
                                let at = pos2(x, rect.center().y + 3.0);
                                let font = FontId::proportional(15.0);
                                ui.painter().text(at, egui::Align2::LEFT_TOP, age(t, now), font, m.on_surface_variant);
                            }
                            if resp.clicked() {
                                open = Some(f.path.clone());
                            }
                            if more_button(ui, m.on_surface_variant).clicked() {
                                self.mobile.show_sheet(Sheet::File(f.path.clone()));
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
        let alpha = self.screen_alpha(&ctx);
        let new = fab(&ctx, "new", corner, &m, Icon::Plus, Some("New calculation"), !self.modal_open(), alpha);
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
        self.mobile.entered = self.ctx.input(|i| i.time);
        self.mobile.drop_sheet();
        self.mobile.bottom = None;
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
        // the top app bar: back, the name, the kernel, the dots. Where the screen is low and the on-screen keyboard
        // is up (a phone held sideways) it gives its place to the text that is typed; the system's Back still
        // leaves, and the bar is back when the keyboard goes
        let low = ui.available_height() < LOW;
        self.mobile.low = low;
        egui::Panel::top("m-bar")
            .frame(Frame::new().fill(m.surface).inner_margin(Margin::symmetric(4, 0)))
            .show_separator_line(false)
            .exact_size(if low { 0.0 } else { TOP })
            .show(ui, |ui| {
                ui.set_clip_rect(ui.max_rect());
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    leave = icon_button(ui, Icon::ArrowLeft, m.on_surface).clicked();
                    ui.add_space(4.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if more_button(ui, m.on_surface_variant).clicked() {
                            self.mobile.show_sheet(Sheet::More);
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
        // the bar rises from the bottom edge and sinks back into it; while it sinks it shows what it showed last
        let wanted = if editing { Some(Bottom::Keys) } else { picked.map(Bottom::Actions) };
        if wanted.is_some() {
            self.mobile.bottom = wanted;
        }
        let up = ctx.animate_bool_with_time(egui::Id::unique("m-bottom-up"), wanted.is_some(), time(&ctx, BAR_TIME));
        let up = if wanted.is_some() { ease(up) } else { 1.0 - ease(1.0 - up) };
        if wanted.is_none() && up <= 0.0 {
            self.mobile.bottom = None;
        }
        let up = up.max(0.02);
        let frame = Frame::new().fill(m.surface_container).inner_margin(Margin::symmetric(8, 0));
        match self.mobile.bottom {
            Some(Bottom::Keys) => {
                // the keys lie on the keyboard; above them the completions, or Cut, Copy and Paste while text is
                // selected and after a long press
                // (a low screen has room for one row: the tools take the place of the keys until the next key is
                // typed, and the completions do not show: tab takes the first)
                let tools = self.mobile.edit_tools || self.text_selected();
                let completing = !tools && !low && !self.mobile.completions.is_empty();
                let keys = !(tools && low);
                let height = if (completing || tools) && keys { BAR + STRIP } else { BAR };
                egui::Panel::bottom("m-keys").frame(frame).show_separator_line(false).exact_size(height * up).show(
                    ui,
                    |ui| {
                        ui.set_clip_rect(ui.max_rect());
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if tools {
                            let room = if keys { STRIP } else { BAR };
                            ui.allocate_ui(vec2(ui.available_width(), room), |ui| self.edit_strip(ui));
                        }
                        if completing {
                            ui.allocate_ui(vec2(ui.available_width(), STRIP), |ui| self.completion_strip(ui));
                        }
                        if keys {
                            ui.allocate_ui(vec2(ui.available_width(), BAR), |ui| self.key_row(ui));
                        }
                    },
                );
            }
            Some(Bottom::Actions(id)) => {
                egui::Panel::bottom("m-actions").frame(frame).show_separator_line(false).exact_size(BAR * up).show(
                    ui,
                    |ui| {
                        ui.set_clip_rect(ui.max_rect());
                        ui.allocate_ui(vec2(ui.available_width(), BAR), |ui| self.action_row(ui, id));
                    },
                );
            }
            // (the place a bar takes among the screen's parts stays taken: what is drawn after it, the canvas, is
            // then the same widget with and without a bar, and a press that brings up the bar, a finger landing on
            // a picture, goes on as a drag)
            None => ui.skip_ahead_auto_ids(1),
        }
        if !editing {
            self.mobile.edit_tools = false;
        }
        let mut canvas = Rect::NOTHING;
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| {
            // the content column: the whole width of a phone, the middle of a wider screen
            let side = ((ui.available_width() - COLUMN) / 2.0).max(0.0);
            canvas = ui.available_rect_before_wrap().shrink2(vec2(side, 0.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(canvas), |ui| self.canvas(ui));
        });
        if let Some(at) = self.touch.take_long_press() {
            self.long_press(&ctx, at, canvas);
        }
        if self.touch.take_long_tap().is_some() {
            self.long_tap();
        }
        self.snackbar(&ctx, canvas);
        // a new input at the end
        let corner = pos2(ui.ctx().content_rect().right(), canvas.bottom()) - vec2(16.0, 16.0);
        let shown = !editing && picked.is_none() && !self.modal_open();
        let alpha = self.screen_alpha(&ctx);
        if fab(&ctx, "add", corner, &m, Icon::Plus, None, shown, alpha) {
            let end = self.doc.nb.cells().len();
            self.run(&ctx, Command::ClickGap(end));
            self.run(&ctx, Command::NewCell(Style::Input));
            self.doc.scroll_pending = true;
        }
        if leave {
            self.leave_notebook();
        }
    }

    /// A finger has rested at `at` (a long press; the button went down there in this frame, so the canvas has put
    /// the caret there or selected the result or picture): in the text of a cell that can be edited the word under
    /// it is selected, and Cut, Copy, Paste and Select all show above the keys. The finger may go on to drag: it
    /// then extends the selection, turns a 3D picture or resizes a picture by a handle.
    pub(super) fn long_press(&mut self, ctx: &egui::Context, at: egui::Pos2, canvas: Rect) {
        if !canvas.contains(at) || self.modal_open() {
            return;
        }
        // (the notebook's own coordinates: the page may be scrolled)
        let p = (at - self.doc.view.origin).to_pos2();
        // the cell whose sheet comes up if the finger is lifted in place: found now, since the page may move under
        // the resting finger (the keyboard goes when a result is selected)
        self.mobile.held = None;
        match self.doc.view.hit(ctx, &self.doc.nb, p) {
            crate::view::Hit::Text(id, pos) if self.edited(id) => {
                self.run(ctx, Command::SelectWord(id, pos));
                self.mobile.edit_tools = true;
            }
            crate::view::Hit::Boxes(id) if self.edited(id) => {
                self.run(ctx, Command::ExtendSelection);
                self.mobile.edit_tools = true;
            }
            crate::view::Hit::Text(id, _) | crate::view::Hit::Boxes(id) | crate::view::Hit::Graphic(id) => {
                self.mobile.held = Some(id);
            }
            _ => self.mobile.held = self.cell_at_y(p.y).map(|i| self.doc.nb.cells()[i].id),
        }
    }

    /// The finger of a long press at `at` was lifted where it rested: anywhere but in text that is edited (there
    /// the tools above the keys are the answer), the sheet of the cell under it comes up.
    pub(super) fn long_tap(&mut self) {
        let held = self.mobile.held.take();
        if self.modal_open() {
            return;
        }
        if let Some(id) = held.filter(|id| self.doc.nb.cell(*id).is_some()) {
            self.mobile.show_sheet(Sheet::Cell(id));
        }
    }

    /// Whether cell `id` is one the user writes (not a result) and may change.
    fn edited(&self, id: mc_nbmodel::CellId) -> bool {
        self.doc.nb.cell(id).is_some_and(|c| !c.generated() && c.editable())
    }

    /// Whether text is selected in the cell that is edited.
    fn text_selected(&self) -> bool {
        match self.doc.nb.selection() {
            Selection::Caret { anchor, pos, .. } if anchor != pos => true,
            Selection::Caret { .. } => self.doc.nb.any_editor().is_some_and(|(_, e)| e.selection().is_some()),
            _ => false,
        }
    }

    /// Cut, Copy, Paste and Select all for the cell that is edited, and the way to the cell's sheet.
    fn edit_strip(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let m = Material::of(self.doc.view.palette.dark);
        let selected = self.text_selected();
        let mut act = None;
        ui.horizontal_centered(|ui| {
            let bars = egui::scroll_area::ScrollBarVisibility::AlwaysHidden;
            ScrollArea::horizontal().id_salt("m-edit").scroll_bar_visibility(bars).show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for label in ["Cut", "Copy", "Paste", "Select all", "Cell"] {
                    if matches!(label, "Cut" | "Copy") && !selected {
                        continue;
                    }
                    if chip(ui, &m, label, false) {
                        act = Some(label);
                    }
                }
            });
        });
        match act {
            Some("Cut") => self.run(&ctx, Command::Cut),
            Some("Copy") => self.run(&ctx, Command::Copy),
            Some("Paste") => self.run(&ctx, Command::Paste),
            Some("Select all") => self.run(&ctx, Command::SelectAll),
            Some("Cell") => {
                if let Some(id) = self.doc.nb.current_cell() {
                    self.mobile.show_sheet(Sheet::Cell(id));
                }
            }
            _ => {}
        }
        // the tools stay while there is more to do with them
        if matches!(act, Some("Cut" | "Paste")) {
            self.mobile.edit_tools = false;
        } else if act.is_some() {
            self.mobile.edit_tools = true;
        }
    }

    /// A row of the sheet of cell `id` was tapped.
    fn cell_action(&mut self, ctx: &egui::Context, id: mc_nbmodel::CellId, what: &str) {
        let Some(i) = self.doc.nb.index_of(id) else { return };
        let cell = &self.doc.nb.cells()[i];
        let (text, picture) = (cell.text.clone(), cell.graphic().is_some());
        // the tools of the long press that led here have done their work
        self.mobile.edit_tools = false;
        match what {
            "evaluate" => {
                self.doc.view.graphic = None;
                self.run(ctx, Command::SelectText(id, text.len(), text.len()));
                self.run(ctx, Command::Evaluate);
            }
            "copy" if picture => self.copy_graphic(id),
            "copy" => self.clip.set(Contents { text: Some(text), ..Contents::default() }),
            "latex" => {
                self.run(ctx, Command::ClickText(id, 0, false));
                self.run(ctx, Command::CopyLatex);
            }
            "share-cell" if picture => self.share_picture(id),
            "share-cell" => mc_ui::platform::share_text(&text),
            "above" | "below" => {
                // below: after the results the input has
                let after = self.doc.nb.cells()[i + 1..].iter().take_while(|c| c.input_of == Some(id)).count();
                let gap = if what == "above" { i } else { i + 1 + after };
                self.doc.view.graphic = None;
                self.run(ctx, Command::ClickGap(gap));
                self.run(ctx, Command::NewCell(Style::Input));
                self.doc.scroll_pending = true;
            }
            "delete" => self.delete_cell(ctx, id),
            _ => {}
        }
    }

    /// Delete cell `id` (an input goes with its results) and offer the way back for a few seconds; a notebook
    /// left without cells gets an empty input.
    pub(super) fn delete_cell(&mut self, ctx: &egui::Context, id: mc_nbmodel::CellId) {
        let Some(i) = self.doc.nb.index_of(id) else { return };
        let cells = self.doc.nb.cells();
        let (generated, before) = (cells[i].generated(), cells.len());
        let last = if generated {
            id
        } else {
            cells[i + 1..].iter().take_while(|c| c.input_of == Some(id)).last().map_or(id, |c| c.id)
        };
        self.doc.view.graphic = None;
        self.run(ctx, Command::ClickBracket(Bracket::Cell(id), false, false));
        if last != id {
            self.run(ctx, Command::ClickBracket(Bracket::Cell(last), true, false));
        }
        self.run(ctx, Command::Delete);
        if self.doc.nb.cells().len() == before {
            // a cell that may not be deleted
            return;
        }
        let mut undo = 1;
        if self.doc.nb.cells().is_empty() {
            self.run(ctx, Command::NewCell(Style::Input));
            undo = 2;
        }
        self.mobile.edit_tools = false;
        let text = if generated { "Result deleted" } else { "Cell deleted" };
        let until = ctx.input(|i| i.time) + SNACK_TIME;
        self.mobile.snack = Some(Snack { text, undo, until, rev: self.doc.nb.structure_rev(), fresh: true });
    }

    /// Backspace in an empty cell that is edited removes the cell (the caret goes to the end of the cell before
    /// it); `false` when the key is not that. The only cell stays.
    pub(super) fn backspace_empty_cell(&mut self, ctx: &egui::Context) -> bool {
        let Selection::Caret { cell, .. } = *self.doc.nb.selection() else { return false };
        let Some(i) = self.doc.nb.index_of(cell) else { return false };
        let cells = self.doc.nb.cells();
        if !self.edited(cell) || !cells[i].text.is_empty() || cells.iter().filter(|c| !c.generated()).count() < 2 {
            return false;
        }
        let before = cells[..i].iter().rev().find(|c| !c.generated() && c.editable()).map(|c| (c.id, c.text.len()));
        self.delete_cell(ctx, cell);
        if let (Some((id, end)), true) = (before, self.doc.nb.cell(cell).is_none()) {
            self.run(ctx, Command::SelectText(id, end, end));
        }
        true
    }

    /// The line after a deletion: what went, and Undo; it goes after a few seconds, with the next key, and when
    /// cells come or go otherwise.
    fn snackbar(&mut self, ctx: &egui::Context, canvas: Rect) {
        let Some(snack) = self.mobile.snack.clone() else { return };
        let now = ctx.input(|i| i.time);
        if now >= snack.until || snack.rev != self.doc.nb.structure_rev() || self.mobile.screen != Screen::Notebook {
            self.mobile.snack = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(snack.until - now));
        let m = Material::of(self.doc.view.palette.dark);
        let width = (canvas.width() - 32.0).min(480.0);
        // above the round "+" when that shows (no bar at the bottom)
        let (editing, picked) = self.mobile_selection();
        let lift = if editing || picked.is_some() { 0.0 } else { 56.0 + 12.0 };
        let at = pos2(canvas.center().x - width / 2.0, canvas.bottom() - 16.0 - 48.0 - lift);
        let mut undo = false;
        egui::Area::new(egui::Id::unique("m-snack")).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 48.0), Sense::hover());
            ui.painter().rect_filled(rect.translate(vec2(0.0, 2.0)).expand(1.0), 5.0, Color32::from_black_alpha(40));
            // the page's colours the other way round: the line stands out on it in both themes
            let (ground, ink, accent) = (m.on_surface, m.surface, m.primary_container);
            ui.painter().rect_filled(rect, 4.0, ground);
            let font = FontId::proportional(15.0);
            let text_at = rect.left_center() + vec2(16.0, 0.0);
            ui.painter().text(text_at, egui::Align2::LEFT_CENTER, snack.text, font.clone(), ink);
            let label = ui.painter().layout_no_wrap("Undo".to_string(), font, accent);
            let target = Rect::from_min_max(pos2(rect.right() - label.size().x - 32.0, rect.top()), rect.max);
            let resp = ui.interact(target, egui::Id::unique("m-snack-undo"), Sense::click());
            mark(ui, "Undo", target);
            ui.painter().rect_filled(target.shrink2(vec2(4.0, 6.0)), 18.0, layer(accent, state(&resp)));
            ui.painter().galley(target.center() - label.size() / 2.0, label, accent);
            undo = resp.clicked();
        });
        if undo {
            for _ in 0..snack.undo {
                self.run(ctx, Command::Undo);
            }
            self.mobile.snack = None;
            self.doc.scroll_pending = true;
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
                            mark(ui, label, rect);
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
                        let left = icon_button(ui, Icon::ArrowLeft, m.on_surface_variant);
                        mark(ui, "left", left.rect);
                        if left.clicked() {
                            fire = Some(Command::Move(mc_nbmodel::Motion::Left, false));
                        }
                        let right = icon_button(ui, Icon::ArrowRight, m.on_surface_variant);
                        mark(ui, "right", right.rect);
                        if right.clicked() {
                            fire = Some(Command::Move(mc_nbmodel::Motion::Right, false));
                        }
                        tab = key(ui, "tab");
                        // the typeset boxes (on a desktop they are Ctrl chords): a fraction, a power, a square root
                        // around the selection or with empty places; the arrows and tab beside them go through and
                        // out of them
                        for (label, what) in BOXES {
                            if key(ui, label) {
                                fire = Some(Command::Structure(what));
                            }
                        }
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
                    if chip(ui, &m, name, true) {
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
            self.delete_cell(&ctx, id);
        }
    }

    /// Hand the picture of Output cell `id` to another app as a PNG.
    fn share_picture(&mut self, id: mc_nbmodel::CellId) {
        let Some(path) = self.doc.nb.cell(id).and_then(|c| c.graphic()) else { return };
        let path = mc_nbfile::images::resolve(&path, self.doc.view.base.as_deref());
        // a plot's PNG has its drawing beside it: the drawing is rasterised at a size that stays sharp on a large
        // screen (the PNG is as small as the plot is on the page)
        let drawing = path.with_extension("svg");
        let path = if drawing.is_file() { drawing } else { path };
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
        self.mobile.entered = self.ctx.input(|i| i.time);
        self.mobile.files = None;
        self.mobile.drop_sheet();
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
        // it slides up from the bottom edge and back down; the page behind it darkens with it
        let up = ctx.animate_bool_with_time(egui::Id::unique("m-sheet-up"), self.mobile.sheet_up, time(ctx, SHEET_TIME));
        let up = if self.mobile.sheet_up { ease(up) } else { 1.0 - ease(1.0 - up) };
        if !self.mobile.sheet_up && up <= 0.0 {
            self.mobile.sheet = None;
        }
        let Some(sheet) = self.mobile.sheet.clone() else { return };
        let p = self.doc.view.palette.clone();
        let m = Material::of(p.dark);
        // a Material bottom sheet: the width of the screen (640 pt at most), rising from its bottom edge, the top
        // corners rounded, a handle at the top; it reaches under the system's navigation bar
        let screen = ctx.content_rect();
        let width = screen.width().min(640.0);
        let id = egui::Id::unique("m-sheet");
        let (_, bottom) = mc_ui::platform::bars(ctx);
        let down = (1.0 - up) * self.mobile.sheet_height.max(120.0);
        let area =
            egui::Modal::default_area(id).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, down)).constrain(false);
        let corners = egui::CornerRadius { nw: 28, ne: 28, sw: 0, se: 0 };
        let frame = Frame::new().fill(m.surface_low).corner_radius(corners).inner_margin(Margin::ZERO);
        let mut close = false;
        let mut act: Option<&'static str> = None;
        let scrim = m.scrim.gamma_multiply(up);
        let r = egui::Modal::new(id).area(area).backdrop_color(scrim).frame(frame).show(ctx, |ui| {
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
                Sheet::Cell(id) => {
                    // what can be done with the cell: an input, a result, a picture
                    let cell = self.doc.nb.cell(*id);
                    let generated = cell.is_some_and(|c| c.generated());
                    let picture = cell.is_some_and(|c| c.graphic().is_some());
                    let rows: &[(&str, &str)] = match (generated, picture) {
                        (_, true) => &[("Copy picture", "copy"), ("Share", "share-cell")],
                        (true, false) => &[("Copy", "copy"), ("Copy as LaTeX", "latex"), ("Share", "share-cell")],
                        (false, false) => &[
                            ("Evaluate", "evaluate"),
                            ("Copy input", "copy"),
                            ("Insert input above", "above"),
                            ("Insert input below", "below"),
                        ],
                    };
                    for (label, what) in rows {
                        if sheet_row(ui, label, p.text) {
                            act = Some(*what);
                        }
                    }
                    if sheet_row(ui, if generated { "Delete result" } else { "Delete cell" }, p.red) {
                        act = Some("delete");
                    }
                }
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
        self.mobile.sheet_height = r.response.rect.height();
        // a sheet on its way down takes no more commands
        if !self.mobile.sheet_up {
            return;
        }
        let ctx = ctx.clone();
        match (act, &sheet) {
            (Some("all"), _) => self.run(&ctx, Command::EvaluateNotebook),
            (Some("clear"), _) => self.run(&ctx, Command::DeleteAllOutput),
            (Some("delete-cell"), _) => {
                if let Some(id) = self.doc.nb.current_cell() {
                    self.delete_cell(&ctx, id);
                }
            }
            (Some(what), Sheet::Cell(id)) => self.cell_action(&ctx, *id, what),
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
            self.mobile.sheet_up = false;
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
