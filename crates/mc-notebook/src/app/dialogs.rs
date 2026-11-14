//! Dialogs (design 10.5), the theme and the window title.
use super::*;

impl App {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.preferences(ctx);
        self.pick_dialog(ctx);
        if let Some(mut filter) = self.other_style.clone() {
            let mut chosen = None;
            let mut close = false;
            let shown = dialog::show(ctx, "other-style", "Other style", 300.0, true, |ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut filter).hint_text("type to filter").desired_width(f32::INFINITY),
                );
                edit.request_focus();
                ui.add_space(6.0);
                let matches: Vec<Style> = Style::ALL
                    .into_iter()
                    .filter(|s| !s.generated() && s.info().name.to_lowercase().contains(&filter.to_lowercase()))
                    .collect();
                ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for s in &matches {
                        let b = egui::Button::selectable(false, s.info().name)
                            .min_size(egui::vec2(ui.available_width(), 24.0));
                        if ui.add(b).clicked() {
                            chosen = Some(*s);
                        }
                    }
                });
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    chosen = matches.first().copied();
                }
                dialog::buttons(ui, |ui| {
                    if dialog::button(ui, "Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        close = true;
                    }
                });
            });
            close |= shown.closed;
            self.other_style = if chosen.is_some() || close { None } else { Some(filter) };
            if let Some(s) = chosen {
                self.run(ctx, Command::SetStyle(s));
            }
        }
        if self.ask_init {
            let mut answer = None;
            let accent = self.doc.view.palette.accent;
            dialog::show(ctx, "init", "Initialization cells", dialog::QUESTION_WIDTH, false, |ui| {
                dialog::message(ui, Icon::Info, accent, "Evaluate the initialization cells first?");
                // the keyboard: Enter is Yes, Esc is No (Tab moves between the buttons)
                dialog::buttons(ui, |ui| {
                    if dialog::button(ui, "Always for this notebook").clicked() {
                        answer = Some(InitAnswer::Always);
                    }
                    if dialog::button(ui, "No").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        answer = Some(InitAnswer::No);
                    }
                    if dialog::primary(ui, "Yes").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        answer = Some(InitAnswer::Yes);
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
            let yellow = self.doc.view.palette.yellow;
            dialog::show(ctx, "interrupt", "Interrupt", dialog::QUESTION_WIDTH, false, |ui| {
                dialog::message(ui, Icon::Warning, yellow, "The evaluation is interrupted. Abort it, or let it continue?");
                // the keyboard: Enter aborts, Esc continues
                dialog::buttons(ui, |ui| {
                    if dialog::button(ui, "Continue").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        fire = Some(false);
                    }
                    if dialog::primary(ui, "Abort").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        fire = Some(true);
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
            let text = match &ask {
                Ask::Save(id) => {
                    let name =
                        self.tabs.index_of(*id).and_then(|i| self.tabs.get(i)).map_or(String::new(), |t| t.title());
                    format!("Do you want to save the changes to {name}?")
                }
                Ask::SaveAll(ids) => {
                    let names: Vec<String> = ids
                        .iter()
                        .filter_map(|id| self.tabs.index_of(*id).and_then(|i| self.tabs.get(i)).map(|t| t.title()))
                        .collect();
                    if names.len() == 1 {
                        format!("Do you want to save the changes to {}?", names[0])
                    } else {
                        format!("Do you want to save the changes to {} notebooks?\n\n{}", names.len(), names.join("\n"))
                    }
                }
            };
            let yellow = self.doc.view.palette.yellow;
            let weak = self.doc.view.palette.weak;
            let all = matches!(ask, Ask::SaveAll(ref v) if v.len() > 1);
            dialog::show(ctx, "unsaved", "Unsaved changes", dialog::QUESTION_WIDTH, false, |ui| {
                dialog::message(ui, Icon::Warning, yellow, &text);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(36.0);
                    ui.label(RichText::new("Your changes are lost if you don't save them.").color(weak));
                });
                // Enter saves, Esc cancels; "Don't save" only by a click or Tab
                dialog::buttons(ui, |ui| {
                    if dialog::button(ui, "Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        answer = Some(Answer::Cancel);
                    }
                    if dialog::button(ui, "Don't save").clicked() {
                        answer = Some(Answer::Discard);
                    }
                    if dialog::primary(ui, if all { "Save all" } else { "Save" }).clicked()
                        || ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        answer = Some(Answer::Saved);
                    }
                });
            });
            if let Some(a) = answer {
                self.answer_close(a);
            }
        }
        if let Some(e) = self.error.clone() {
            let red = self.doc.view.palette.red;
            let mut ok = false;
            dialog::show(ctx, "error", "MathCAS Notebook", dialog::QUESTION_WIDTH, false, |ui| {
                dialog::message(ui, Icon::Error, red, &e);
                dialog::buttons(ui, |ui| {
                    ok = dialog::primary(ui, "OK").clicked()
                        || ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Escape));
                });
            });
            if ok {
                self.error = None;
            }
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
            let weak = self.doc.view.palette.weak;
            let shown = dialog::show(ctx, "keys", "Keyboard shortcuts", 520.0, true, |ui| {
                ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
                    egui::Grid::new("keys").striped(true).num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                        for b in keys.bindings() {
                            ui.label(RichText::new(b.keys).monospace().color(weak));
                            ui.label(b.command.describe());
                            ui.end_row();
                        }
                    });
                });
                let mut done = false;
                dialog::buttons(ui, |ui| {
                    done = dialog::primary(ui, "Close").clicked() || ui.input(|i| i.key_pressed(Key::Escape));
                });
                done
            });
            if shown.closed || shown.inner {
                self.keyboard_help = false;
            }
        }
        if self.about {
            if self.icon.is_none() {
                if let Some(d) = icon_data() {
                    let img = egui::ColorImage::from_rgba_unmultiplied([d.width as usize, d.height as usize], &d.rgba);
                    self.icon = Some(ctx.load_texture("mc-notebook-icon", img, egui::TextureOptions::LINEAR));
                }
            }
            let kernel = format!(
                "{} ({})",
                default_kernel().display(),
                crate::about::words(&format!("{:?}", self.doc.nb.kernel_state()))
            );
            let info = crate::about::SystemInfo::gather(
                self.gpu.as_ref(),
                ctx.pixels_per_point(),
                self.doc.view.magnification(),
                &kernel,
            );
            let weak = self.doc.view.palette.weak;
            crate::about::show(ctx, &mut self.about, &mut self.about_tab, self.icon.as_ref(), weak, &info);
        }
    }

    pub(super) fn update_theme(&mut self, ctx: &egui::Context) {
        // (Android tells through its activity whether the system is dark, not through the window)
        let system_dark = mc_ui::platform::system_dark(ctx.input(|i| i.time))
            .unwrap_or_else(|| ctx.system_theme().is_some_and(|t| t == egui::Theme::Dark));
        let palette = self.settings.theme.palette(system_dark);
        if self.applied.as_ref() != Some(&palette) {
            palette.apply(ctx);
            // the system draws its clock and icons dark on a light page, light on a dark one
            mc_ui::platform::light_bars(!palette.dark);
            self.doc.view.palette = palette.clone();
            self.doc.view.clear();
            self.applied = Some(palette);
        }
    }

    pub(super) fn update_title(&mut self, ctx: &egui::Context) {
        let name = self.tabs.current().map_or("Untitled".to_string(), |t| t.title());
        let title = format!("{}{} - MathCAS Notebook", name, if self.doc.nb.is_modified() { " *" } else { "" });
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}
