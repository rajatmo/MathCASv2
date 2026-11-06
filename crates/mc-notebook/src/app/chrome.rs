//! The chrome (design 10): menus, toolbar, status bar.
use super::*;

impl App {
    pub(super) fn menu_item(&self, ui: &mut egui::Ui, label: &str, cmd: Command, fire: &mut Option<Command>) {
        self.menu_item_with(ui, egui::Button::new(label), cmd, fire);
    }

    /// A menu item with a check mark (or the room for one), the icon set's check.
    pub(super) fn check_menu_item(&self, ui: &mut egui::Ui, on: bool, label: &str, cmd: Command, fire: &mut Option<Command>) {
        self.menu_item_with(ui, egui::Button::image_and_text(check_mark(ui, on), label), cmd, fire);
    }

    fn menu_item_with(&self, ui: &mut egui::Ui, b: egui::Button<'_>, cmd: Command, fire: &mut Option<Command>) {
        let mut b = b;
        if let Some(k) = self.keys.chord_of(&cmd) {
            b = b.shortcut_text(k);
        }
        let r = ui.add(b);
        // a menu opened from the keyboard: its first item takes the focus (arrows then move it)
        if self.menu_focus.replace(false) {
            r.request_focus();
        }
        focus_ring(ui, &r);
        if r.clicked() {
            *fire = Some(cmd);
            ui.close();
        }
    }

    pub(super) fn style_menu(&self, ui: &mut egui::Ui, fire: &mut Option<Command>) {
        submenu(ui, "Style", |ui| {
            for s in Style::ALL.into_iter().filter(|s| !s.generated()) {
                self.menu_item(ui, s.info().name, Command::SetStyle(s), fire);
            }
            ui.separator();
            self.menu_item(ui, "Other...", Command::App(AppAction::OtherStyle), fire);
        });
    }

    pub(super) fn menus(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut fire: Option<Command> = None;
        let mut local: Option<&'static str> = None;
        let mut tab_pick: Option<usize> = None;
        let mut menu_ids = vec![];
        egui::MenuBar::new().ui(ui, |ui| {
            let r = ui.menu_button("File", |ui| {
                self.menu_item(ui, "New", Command::App(AppAction::New), &mut fire);
                self.menu_item(ui, "Open...", Command::App(AppAction::Open), &mut fire);
                submenu(ui, "Open recent", |ui| {
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
                submenu(ui, "Export", |ui| {
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
            menu_ids.push(('f', r.response.id));
            let r = ui.menu_button("Edit", |ui| {
                self.menu_item(ui, "Undo", Command::Undo, &mut fire);
                self.menu_item(ui, "Redo", Command::Redo, &mut fire);
                ui.separator();
                self.menu_item(ui, "Cut", Command::Cut, &mut fire);
                self.menu_item(ui, "Copy", Command::Copy, &mut fire);
                submenu(ui, "Copy as", |ui| {
                    self.menu_item(ui, "Plain text", Command::CopyPlain, &mut fire);
                    self.menu_item(ui, "LaTeX", Command::CopyLatex, &mut fire);
                });
                self.menu_item(ui, "Paste", Command::Paste, &mut fire);
                self.menu_item(ui, "Clear", Command::Delete, &mut fire);
                ui.separator();
                self.menu_item(ui, "Select all", Command::SelectAll, &mut fire);
                self.menu_item(ui, "Extend selection", Command::ExtendSelection, &mut fire);
                ui.separator();
                self.menu_item(ui, "Find...", Command::App(AppAction::Find), &mut fire);
                self.menu_item(ui, "Find next", Command::App(AppAction::FindNext(true)), &mut fire);
                ui.separator();
                if check_item(ui, &mut self.settings.auto_close, "Close brackets automatically") {
                    self.settings.save();
                }
                self.menu_item(ui, "Preferences...", Command::App(AppAction::Preferences), &mut fire);
            });
            menu_ids.push(('e', r.response.id));
            let r = ui.menu_button("Insert", |ui| {
                self.menu_item(ui, "Input from above", Command::InputFromAbove, &mut fire);
                self.menu_item(ui, "Output from above", Command::OutputFromAbove, &mut fire);
                self.menu_item(ui, "Cell with same style", Command::SameStyleBelow, &mut fire);
                ui.separator();
                self.menu_item(ui, "New Input cell", Command::NewCell(Style::Input), &mut fire);
                self.menu_item(ui, "New Text cell", Command::NewCell(Style::Text), &mut fire);
            });
            menu_ids.push(('i', r.response.id));
            let r = ui.menu_button("Format", |ui| {
                self.style_menu(ui, &mut fire);
                submenu(ui, "Output form", |ui| {
                    if check_item(ui, &mut self.settings.typeset_output, "Typeset output by default") {
                        self.settings.save();
                        self.doc.view.typeset_outputs = self.settings.typeset_output;
                        self.doc.view.clear();
                    }
                    ui.separator();
                    self.menu_item(ui, "Typeset", Command::ToTypeset, &mut fire);
                    self.menu_item(ui, "Linear text", Command::ToLinear, &mut fire);
                    self.menu_item(ui, "Traditional", Command::OutputForm(mc_nbmodel::OutputForm::Traditional), &mut fire);
                });
                submenu(ui, "Theme", |ui| {
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
                submenu(ui, "Magnification", |ui| {
                    for z in ZOOMS {
                        if ui.radio((self.doc.view.magnification() - z).abs() < 0.01, format!("{:.0} %", z * 100.0)).clicked() {
                            self.set_zoom(z);
                            ui.close();
                        }
                    }
                });
            });
            menu_ids.push(('o', r.response.id));
            let r = ui.menu_button("Cell", |ui| {
                submenu(ui, "Cell properties", |ui| {
                    let c = self.doc.nb.current_cell().and_then(|id| self.doc.nb.cell(id)).cloned();
                    let props: [(&str, Option<bool>, Command); 4] = [
                        ("Editable", c.as_ref().map(|c| c.editable()), Command::ToggleEditable),
                        ("Evaluatable", c.as_ref().map(|c| c.evaluatable()), Command::ToggleEvaluatable),
                        ("Deletable", c.as_ref().map(|c| c.deletable()), Command::ToggleDeletable),
                        ("Initialization cell", c.as_ref().map(|c| c.initialization()), Command::ToggleInitialization),
                    ];
                    for (name, on, cmd) in props {
                        self.check_menu_item(ui, on.unwrap_or(false), name, cmd, &mut fire);
                    }
                });
                submenu(ui, "Grouping", |ui| {
                    self.menu_item(ui, "Group together", Command::GroupTogether, &mut fire);
                    self.menu_item(ui, "Ungroup", Command::Ungroup, &mut fire);
                    ui.separator();
                    let manual = self.doc.nb.manual_grouping();
                    self.check_menu_item(ui, !manual, "Automatic grouping", Command::SetManualGrouping(false), &mut fire);
                    self.check_menu_item(ui, manual, "Manual grouping", Command::SetManualGrouping(true), &mut fire);
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
            menu_ids.push(('c', r.response.id));
            let r = ui.menu_button("Evaluation", |ui| {
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
            menu_ids.push(('v', r.response.id));
            let r = ui.menu_button("Window", |ui| {
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
                    // a modified notebook: the icon set's dot after its title
                    let tint = if modified { ui.visuals().weak_text_color() } else { Color32::TRANSPARENT };
                    let dot = icons::image(ui.ctx(), Icon::Dot, 10.0, tint);
                    let mut b = egui::Button::selectable(k == active, (title, dot));
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
            menu_ids.push(('w', r.response.id));
            let r = ui.menu_button("Help", |ui| {
                if ui.button("Documentation").clicked() {
                    local = Some("docs");
                    ui.close();
                }
                self.menu_item(ui, "Find selected function", Command::App(AppAction::Help), &mut fire);
                ui.separator();
                if ui.button("Keyboard shortcuts").clicked() {
                    self.keyboard_help = true;
                    ui.close();
                }
                if ui.button("About").clicked() {
                    self.about = true;
                    ui.close();
                }
            });
            menu_ids.push(('h', r.response.id));
        });
        self.menu_ids = menu_ids;
        if let Some(k) = tab_pick {
            self.activate(k);
        }
        match local {
            Some("exit") => ctx.send_viewport_cmd(ViewportCommand::Close),
            Some("export") => self.export_script(),
            Some("docs") => self.open_docs(None),
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

    pub(super) fn kernel_indicator(&self) -> (Color32, String) {
        let p = &self.doc.view.palette;
        match self.doc.nb.kernel_state() {
            KernelState::NotStarted => (p.gutter, "Kernel: not started".into()),
            KernelState::Idle => (p.green, "Kernel: idle".into()),
            KernelState::Busy => (p.yellow, format!("Kernel: busy ({} queued)", self.doc.nb.queue_len())),
            KernelState::Stopped => (p.red, "Kernel: stopped".into()),
        }
    }

    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
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
            let tool = |ui: &mut egui::Ui, icon: Icon, icon_color: Color32, text: &str, enabled: bool, tip: &str| {
                let ic = if enabled { icon_color } else { p.faint };
                let b = egui::Button::image_and_text(icons::image(ui.ctx(), icon, TOOL_ICON, ic), text)
                    .frame_when_inactive(false)
                    .min_size(vec2(0.0, 26.0));
                ui.add_enabled(enabled, b).on_hover_text(tip).on_disabled_hover_text(tip)
            };
            if tool(ui, Icon::Evaluate, p.green, "Evaluate", true, "Evaluate the selected cells (Shift+Enter)").clicked() {
                fire = Some(Command::Evaluate);
            }
            let busy = self.doc.nb.kernel_state() == KernelState::Busy;
            if tool(ui, Icon::Abort, p.red, "Abort", busy, "Abort the evaluation (Alt+.)").clicked() {
                fire = Some(Command::Abort);
            }
            toolbar_separator(ui, &p);
            // the kernel indicator: a flat button with the state's dot, a menu on click
            let (color, text) = self.kernel_indicator();
            let b = egui::Button::image_and_text(
                icons::image(ui.ctx(), Icon::Kernel, TOOL_ICON, color),
                RichText::new(text.clone()).color(p.weak),
            )
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
                ui.add(icons::image(ui.ctx(), Icon::Zoom, TOOL_ICON, p.weak));
                let (icon, name) = match self.settings.theme {
                    ThemeChoice::System => (Icon::ThemeSystem, "System"),
                    ThemeChoice::Light => (Icon::ThemeLight, "Light"),
                    ThemeChoice::Dark => (Icon::ThemeDark, "Dark"),
                };
                if tool(ui, icon, p.yellow, name, true, "Theme: System, One Light, One Dark (click to change)")
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

    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let weak = self.doc.view.palette.weak;
        // the file name first, from the right, so the items on the left get the remaining width and are truncated
        // instead of running into it
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let name = match self.path() {
                Some(p) => p.display().to_string(),
                None => self.tabs.current().map_or("Untitled".to_string(), |t| t.title()),
            };
            let m = if self.doc.nb.is_modified() { " (modified)" } else { "" };
            // at most half the bar, so a long path is shortened instead of covering the items on the left
            let max = (ui.available_width() * 0.5).max(80.0);
            ui.scope(|ui| {
                ui.set_max_width(max);
                ui.add(egui::Label::new(RichText::new(format!("{name}{m}")).color(weak).small()).truncate());
            });
            ui.add_space(12.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| self.status_items(ui, weak));
        });
    }

    /// The left part of the status bar: kernel, last time, current cell, message.
    pub(super) fn status_items(&mut self, ui: &mut egui::Ui, weak: egui::Color32) {
        ui.horizontal_centered(|ui| {
            let (color, text) = self.kernel_indicator();
            ui.add(icons::image(ui.ctx(), Icon::Kernel, 12.0, color));
            let engine =
                if self.doc.nb.engine().is_empty() { String::new() } else { format!(" · {}", self.doc.nb.engine()) };
            ui.label(RichText::new(format!("{text}{engine}")).color(weak).small());
            if let Some(ms) = self.doc.nb.last_millis() {
                ui.label(RichText::new(format!("· last {ms} ms")).color(weak).small());
            }
            if let Some(c) = self.doc.nb.current_cell().and_then(|id| self.doc.nb.cell(id)) {
                let mut s = format!("· {}", c.style.info().name);
                if let Some(n) = c.label.filter(|_| c.style == Style::Output) {
                    s.push_str(&format!(" out({n})"));
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
            // a live region for screen readers: evaluation finished, kernel stopped (design 9.7)
            let announce = self.announce.clone();
            ui.ctx().accesskit_node_builder(egui::Id::unique("mc-notebook-live"), |node| {
                node.set_role(egui::accesskit::Role::Status);
                node.set_live(egui::accesskit::Live::Polite);
                node.set_label(announce);
            });
        });
    }
}

/// The size of the toolbar's icons (points).
const TOOL_ICON: f32 = 15.0;

/// A submenu: its label and the icon set's chevron on the right.
fn submenu<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) {
    let chevron = icons::image(ui.ctx(), Icon::ChevronRight, 12.0, ui.visuals().text_color());
    let _ = egui::containers::menu::SubMenuButton::from_button(egui::Button::new(label).right_text(chevron)).ui(ui, add);
}

/// The check mark of a menu item, or the same room without one.
pub(super) fn check_mark(ui: &egui::Ui, on: bool) -> egui::Image<'static> {
    let color = if on { ui.visuals().text_color() } else { Color32::TRANSPARENT };
    icons::image(ui.ctx(), Icon::Check, 13.0, color)
}
