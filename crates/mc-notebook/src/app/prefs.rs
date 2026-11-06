//! The preferences dialog (design 10.5, M7): theme, magnification, fonts, editing, output and file settings, each
//! applied at once and saved. Every control is an egui widget, so the dialog is reachable with Tab and named for
//! screen readers.
use super::*;

impl App {
    /// The fonts of the setting (system fonts where good ones exist, or only the bundled set).
    pub(super) fn apply_fonts(&mut self, ctx: &egui::Context) {
        let source = if self.settings.bundled_fonts { fonts::Source::Bundled } else { fonts::Source::from_env() };
        ctx.set_fonts(fonts::definitions(source));
        self.doc.view.clear();
    }

    /// Typeset outputs are laid out again (the default form or the matrix setting changed).
    fn retypeset(&mut self) {
        self.doc.view.typeset_outputs = self.settings.typeset_output;
        self.doc.view.matrices = self.settings.matrices;
        self.doc.nb.set_matrices(self.settings.matrices);
        self.doc.view.clear_boxes();
    }

    pub(super) fn preferences(&mut self, ctx: &egui::Context) {
        if !self.prefs {
            return;
        }
        let mut open = true;
        let mut changed = false;
        let mut fonts_changed = false;
        let mut outputs_changed = false;
        let mut zoom = None;
        let s = &mut self.settings;
        egui::Window::new("Preferences")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                egui::Grid::new("prefs").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Theme");
                    ui.horizontal(|ui| {
                        for (c, name) in [(ThemeChoice::System, "System"), (ThemeChoice::Light, "One Light"), (ThemeChoice::Dark, "One Dark")] {
                            changed |= ui.radio_value(&mut s.theme, c, name).changed();
                        }
                    });
                    ui.end_row();

                    ui.label("Magnification");
                    let current = s.magnification;
                    egui::ComboBox::from_id_salt("prefs-zoom").selected_text(format!("{:.0} %", current * 100.0)).show_ui(ui, |ui| {
                        for z in ZOOMS {
                            if ui.selectable_label((current - z).abs() < 0.01, format!("{:.0} %", z * 100.0)).clicked() {
                                zoom = Some(z);
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Fonts");
                    ui.vertical(|ui| {
                        fonts_changed |= ui.radio_value(&mut s.bundled_fonts, false, "System fonts where available").changed();
                        fonts_changed |= ui.radio_value(&mut s.bundled_fonts, true, "Bundled fonts only (the same on every system)").changed();
                    });
                    ui.end_row();

                    ui.label("Editing");
                    ui.vertical(|ui| {
                        changed |= ui.checkbox(&mut s.auto_close, "Close brackets and quotes automatically").changed();
                        changed |= ui.checkbox(&mut s.caret_blink, "Blinking caret").changed();
                    });
                    ui.end_row();

                    ui.label("Output");
                    ui.vertical(|ui| {
                        outputs_changed |= ui.checkbox(&mut s.typeset_output, "Typeset output by default").changed();
                        outputs_changed |= ui.checkbox(&mut s.matrices, "Lists of equal-length lists as matrices").changed();
                    });
                    ui.end_row();

                    ui.label("Files");
                    ui.vertical(|ui| {
                        changed |= ui.checkbox(&mut s.save_without_outputs, "Save without outputs").changed();
                        changed |= ui.checkbox(&mut s.restore_session, "Reopen notebooks at start").changed();
                        changed |= ui.checkbox(&mut s.show_toolbar, "Show the toolbar").changed();
                    });
                    ui.end_row();
                });
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        self.prefs_close = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.prefs_close = true;
        }
        if let Some(z) = zoom {
            self.set_zoom(z);
        }
        if fonts_changed {
            self.apply_fonts(ctx);
        }
        if outputs_changed {
            self.retypeset();
        }
        if changed || fonts_changed || outputs_changed {
            self.settings.save();
        }
        if !open || std::mem::take(&mut self.prefs_close) {
            self.prefs = false;
        }
    }
}
