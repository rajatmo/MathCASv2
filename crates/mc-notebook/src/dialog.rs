//! The look of every dialog of the notebook (design 10.5): a modal card over a dimmed window, the title on the left
//! in bold with an optional close button, generous margins, and the buttons in a row on the right with the default
//! action filled in the accent colour. Dialogs that ask a question (unsaved changes, interrupt) and the settings
//! dialogs (preferences, about) share it, so they look alike in both themes.
use eframe::egui::{self, vec2, Align, Color32, Layout, Margin, Response, RichText, Stroke, Ui};
use mc_ui::icons::{self, Icon};

/// The width of a question dialog's text column.
pub const QUESTION_WIDTH: f32 = 380.0;

/// The height of a dialog button, and its least width (short labels such as OK get a comfortable target).
const BUTTON: egui::Vec2 = vec2(88.0, 30.0);

/// What a dialog reports besides its contents' value.
pub struct Shown<R> {
    /// the contents' value
    pub inner: R,
    /// the close button was clicked
    pub closed: bool,
}

/// Show a modal dialog `title` of the given content width. With `closable` the title row has a close button
/// (reported in [`Shown::closed`]); keys stay with the caller, which knows what Enter and Escape mean.
pub fn show<R>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    width: f32,
    closable: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> Shown<R> {
    let style = ctx.global_style();
    let dark = style.visuals.dark_mode;
    let backdrop = if dark { Color32::from_black_alpha(120) } else { Color32::from_black_alpha(70) };
    let mut frame = egui::Frame::window(&style).inner_margin(Margin { left: 22, right: 22, top: 16, bottom: 18 });
    // the phone layout: a Material dialog (large corners, wider margins, a larger title, no outline)
    let material = crate::layout::mobile();
    let backdrop = if material { crate::theme::Material::of(dark).scrim } else { backdrop };
    if material {
        frame = frame.inner_margin(Margin { left: 24, right: 24, top: 22, bottom: 18 }).corner_radius(28.0);
    }
    let mut closed = false;
    let id = egui::Id::unique(("mc-dialog", id));
    // in the middle of the part of the window that can be seen
    let area = egui::Modal::default_area(id).anchor(egui::Align2::CENTER_CENTER, covered_shift(ctx));
    let r = egui::Modal::new(id).area(area).backdrop_color(backdrop).frame(frame).show(ctx, |ui| {
        // a screen narrower than the dialog (a phone): the dialog takes what there is
        ui.set_width(width.min(ui.ctx().content_rect().width() - 76.0).max(160.0));
        ui.horizontal(|ui| {
            if material {
                ui.label(RichText::new(title).size(22.0));
            } else {
                ui.label(RichText::new(title).size(16.0).strong());
            }
            if closable {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let x = icons::image(ui.ctx(), Icon::Close, 14.0, ui.visuals().weak_text_color());
                    let b = egui::Button::image(x).frame_when_inactive(false).min_size(vec2(26.0, 26.0));
                    if ui.add(b).on_hover_text("Close (Esc)").clicked() {
                        closed = true;
                    }
                });
            }
        });
        ui.add_space(12.0);
        add(ui)
    });
    Shown { inner: r.inner, closed }
}

/// How far the middle of the visible part of the window is from the window's middle. On Android the system's bars
/// and the on-screen keyboard lie over the window's top and bottom; elsewhere nothing covers it.
fn covered_shift(ctx: &egui::Context) -> egui::Vec2 {
    #[cfg(target_os = "android")]
    {
        let (top, bottom) = mc_ui::android::bars();
        vec2(0.0, (top - bottom) / ctx.pixels_per_point() / 2.0)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = ctx;
        egui::Vec2::ZERO
    }
}

/// A question or a report: the icon of its kind on the left, the text wrapped beside it.
pub fn message(ui: &mut Ui, icon: Icon, color: Color32, text: &str) {
    ui.horizontal_top(|ui| {
        ui.add(icons::image(ui.ctx(), icon, 26.0, color));
        ui.add_space(10.0);
        ui.add(egui::Label::new(RichText::new(text).size(14.5)).wrap());
    });
}

/// The row of buttons at the bottom right. `add` places them from the right: the last button of the reading order
/// first (so a row reading "Save, Don't save, Cancel" is added as Cancel, Don't save, Save).
pub fn buttons(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.add_space(18.0);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        add(ui);
    });
}

/// A dialog button.
pub fn button(ui: &mut Ui, text: &str) -> Response {
    if crate::layout::mobile() {
        return text_button(ui, text);
    }
    ui.add(egui::Button::new(text).min_size(BUTTON))
}

/// A Material text button (the phone layout): the label in the primary colour, no face until it is pressed.
fn text_button(ui: &mut Ui, text: &str) -> Response {
    let primary = crate::theme::Material::of(ui.visuals().dark_mode).primary;
    let label = RichText::new(text).size(15.0).strong().color(primary);
    ui.add(egui::Button::new(label).frame_when_inactive(false).stroke(Stroke::NONE).min_size(vec2(64.0, 40.0)))
}

/// The default button of a dialog (Enter): filled with the accent colour.
pub fn primary(ui: &mut Ui, text: &str) -> Response {
    if crate::layout::mobile() {
        return text_button(ui, text);
    }
    let accent = ui.visuals().hyperlink_color;
    // white on the light theme's deep blue; the dark theme's accent is a light blue, which needs dark text
    let ink = if ui.visuals().dark_mode { Color32::from_rgb(0x1b, 0x1f, 0x27) } else { Color32::WHITE };
    let b = egui::Button::new(RichText::new(text).color(ink).strong())
        .fill(accent)
        .stroke(Stroke::new(1.0, accent))
        .min_size(BUTTON);
    ui.add(b)
}
