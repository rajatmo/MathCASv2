//! Dialogs (design 10.5), the theme and the window title.
use super::*;

impl App {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.preferences(ctx);
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
                    // the keyboard: Enter is Yes, Esc is No (Tab moves between the buttons)
                    if ui.button("Yes").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        answer = Some(InitAnswer::Yes);
                    }
                    if ui.button("No").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
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
                    // the keyboard: Enter aborts, Esc continues
                    if ui.button("Abort").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        fire = Some(true);
                    }
                    if ui.button("Continue").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
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
                if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Escape)) {
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
        let system_dark = ctx.system_theme().is_some_and(|t| t == egui::Theme::Dark);
        let palette = self.settings.theme.palette(system_dark);
        if self.applied.as_ref() != Some(&palette) {
            palette.apply(ctx);
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
