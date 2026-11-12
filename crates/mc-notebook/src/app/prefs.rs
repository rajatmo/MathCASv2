//! The preferences dialog (design 10.5, M7): theme, magnification, fonts, editing, output and file settings, each
//! applied at once and saved. Every control is an egui widget, so the dialog is reachable with Tab and named for
//! screen readers.
use super::*;

impl App {
    /// The fonts of the setting (system fonts where good ones exist, or only the bundled set).
    pub(super) fn apply_fonts(&mut self, ctx: &egui::Context) {
        let source = if self.settings.bundled_fonts { fonts::Source::Bundled } else { fonts::Source::from_env() };
        let defs = fonts::definitions(source);
        // typeset cells measure with the same fonts (their layouts are keyed by the font set's generation)
        self.glyphs.borrow_mut().set_fonts(fonts::FontSet::new(&defs));
        ctx.set_fonts(defs);
        self.doc.view.clear();
    }

    pub(super) fn preferences(&mut self, ctx: &egui::Context) {
        if !self.prefs {
            return;
        }
        let mut changed = false;
        let mut fonts_changed = false;
        let mut zoom = None;
        let s = &mut self.settings;
        let shown = dialog::show(ctx, "prefs", "Preferences", 470.0, true, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            section(ui, "Theme", |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    for (c, name) in [(ThemeChoice::System, "System"), (ThemeChoice::Light, "One Light"), (ThemeChoice::Dark, "One Dark")] {
                        changed |= ui.radio_value(&mut s.theme, c, name).changed();
                    }
                });
            });
            section(ui, "Magnification", |ui| {
                let current = s.magnification;
                egui::ComboBox::from_id_salt("prefs-zoom")
                    .icon(icons::combo_marker)
                    .selected_text(format!("{:.0} %", current * 100.0))
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for z in ZOOMS {
                            if ui.selectable_label((current - z).abs() < 0.01, format!("{:.0} %", z * 100.0)).clicked() {
                                zoom = Some(z);
                            }
                        }
                    });
            });
            section(ui, "Fonts", |ui| {
                fonts_changed |= ui.radio_value(&mut s.bundled_fonts, false, "System fonts where available").changed();
                fonts_changed |=
                    ui.radio_value(&mut s.bundled_fonts, true, "Bundled fonts only (the same on every system)").changed();
            });
            section(ui, "Editing", |ui| {
                changed |= ui.checkbox(&mut s.auto_close, "Close brackets and quotes automatically").changed();
                changed |= ui.checkbox(&mut s.caret_blink, "Blinking caret").changed();
            });
            section(ui, "Outputs", |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    changed |= ui.radio_value(&mut s.outputs_right, false, "On the left").changed();
                    changed |= ui.radio_value(&mut s.outputs_right, true, "On the right").changed();
                });
            });
            section(ui, "Files", |ui| {
                changed |= ui.checkbox(&mut s.save_without_outputs, "Save without outputs").changed();
                changed |= ui.checkbox(&mut s.restore_session, "Reopen notebooks at start").changed();
                changed |= ui.checkbox(&mut s.show_toolbar, "Show the toolbar").changed();
            });
            let mut done = false;
            dialog::buttons(ui, |ui| {
                done = dialog::primary(ui, "Close").clicked();
            });
            done
        });
        if shown.inner || shown.closed || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.prefs_close = true;
        }
        if let Some(z) = zoom {
            self.set_zoom(z);
        }
        if fonts_changed {
            self.apply_fonts(ctx);
        }
        if changed || fonts_changed {
            self.settings.save();
        }
        if std::mem::take(&mut self.prefs_close) {
            self.prefs = false;
        }
    }
}

/// The width of the preferences' label column.
const LABEL_WIDTH: f32 = 120.0;

/// One row of the preferences: the label on the left, level with the first line of the controls on the right
/// (labels of several-line groups stay at the top instead of floating between the options).
fn section(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_top(|ui| {
        let h = ui.spacing().interact_size.y;
        let weak = ui.visuals().weak_text_color();
        ui.allocate_ui_with_layout(vec2(LABEL_WIDTH, h), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_size(vec2(LABEL_WIDTH, h));
            ui.label(RichText::new(label).color(weak));
        });
        ui.vertical(|ui| add(ui));
    });
    ui.add_space(6.0);
}
