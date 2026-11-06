//! The documentation window: navigation (back, forward, home), the search box with incremental results, find in
//! page, the theme, and "Open as notebook".
use crate::catalogue::Catalogue;
use crate::library::Library;
use crate::page::Page;
use crate::search::{search, Searchable};
use crate::selftest::SelfTest;
use crate::view::{Colours, Draw, Find, Images, Loc, MAX_WIDTH};
use eframe::egui::{self, Frame, Key, Margin, Modifiers, RichText, ScrollArea, ViewportCommand};
use mc_notebook::theme::{Palette, ThemeChoice};
use std::collections::HashMap;
use std::path::PathBuf;

/// The most results a search lists.
const RESULTS: usize = 200;

/// The documentation window's state.
pub struct DocsApp {
    lib: Library,
    cat: Catalogue,
    pages: HashMap<String, Page>,
    items: Vec<Searchable>,
    /// visited places and the one shown
    history: Vec<Loc>,
    at: usize,
    /// the search box's text, and whether to focus it on the next frame
    query: String,
    focus_search: bool,
    /// the results of the current search and the highlighted one
    results: Vec<String>,
    selected: usize,
    find: Option<Find>,
    theme: ThemeChoice,
    applied: Option<Palette>,
    images: Images,
    colours: Colours,
    /// a message in the top bar (an error of "Open as notebook")
    status: Option<String>,
    /// open every collapsible part (self-test)
    expand: bool,
    /// a scroll the self-test asked for, and the content's offset in the last frame
    scroll_by: f32,
    offset: f32,
    selftest: Option<SelfTest>,
    gpu: Option<eframe::wgpu::AdapterInfo>,
    frames: u64,
    title: String,
    /// the window's own work in the last frame (milliseconds), for the self-test log
    frame_ms: f64,
}

impl DocsApp {
    /// The window at `start` (home when `None`), with the pages of `lib`.
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        lib: Library,
        start: Option<Loc>,
        selftest: Option<SelfTest>,
    ) -> DocsApp {
        mc_ui::fonts::install(&cc.egui_ctx);
        let cat = Catalogue::new();
        let pages: HashMap<String, Page> = lib
            .names()
            .into_iter()
            .filter_map(|n| lib.page_text(&n).map(|t| (n.clone(), Page::read(&n, &t).0)))
            .collect();
        let items = cat
            .entries
            .iter()
            .map(|e| {
                let mut words = e.summary.to_lowercase();
                if let Some(p) = pages.get(e.name) {
                    words.push('\n');
                    words.push_str(&p.words().to_lowercase());
                }
                Searchable { name: e.name.to_string(), words }
            })
            .collect();
        let theme = mc_notebook::settings::Settings::load().theme;
        let mut app = DocsApp {
            lib,
            cat,
            pages,
            items,
            history: vec![Loc::Home],
            at: 0,
            query: String::new(),
            focus_search: true,
            results: vec![],
            selected: 0,
            find: None,
            theme,
            applied: None,
            images: Images::default(),
            colours: Colours::default(),
            status: None,
            expand: false,
            scroll_by: 0.0,
            offset: 0.0,
            selftest,
            gpu: cc.wgpu_render_state.as_ref().map(|r| r.adapter.get_info()),
            frames: 0,
            title: String::new(),
            frame_ms: 0.0,
        };
        if let Some(loc) = start {
            app.go(loc);
        }
        app
    }

    /// The place shown.
    pub fn here(&self) -> &Loc {
        &self.history[self.at]
    }

    /// Go to `loc` (a new step in the history; a search replaces a search).
    pub fn go(&mut self, loc: Loc) {
        if let Loc::Search(q) = &loc {
            self.type_search(&q.clone());
            return;
        }
        if *self.here() == loc {
            return;
        }
        self.history.truncate(self.at + 1);
        self.history.push(loc);
        self.at = self.history.len() - 1;
        self.left_page();
    }

    /// Back in the history.
    pub fn back(&mut self) {
        if self.at > 0 {
            self.at -= 1;
            self.left_page();
        }
    }

    /// Forward in the history.
    pub fn forward(&mut self) {
        if self.at + 1 < self.history.len() {
            self.at += 1;
            self.left_page();
        }
    }

    fn left_page(&mut self) {
        self.expand = false;
        if let Some(f) = &mut self.find {
            f.current = 0;
            f.reveal = true;
        }
    }

    /// Set the search text: the results are listed as it changes.
    pub fn type_search(&mut self, text: &str) {
        self.query = text.to_string();
        self.searched();
    }

    fn searched(&mut self) {
        let q = self.query.trim().to_string();
        self.selected = 0;
        if q.is_empty() {
            self.results.clear();
            if matches!(self.here(), Loc::Search(_)) {
                self.back();
            }
            return;
        }
        self.results = search(&self.items, &q, RESULTS).into_iter().map(|(n, _)| n).collect();
        let loc = Loc::Search(q);
        if matches!(self.here(), Loc::Search(_)) {
            self.history[self.at] = loc;
        } else {
            self.history.truncate(self.at + 1);
            self.history.push(loc);
            self.at = self.history.len() - 1;
        }
    }

    /// Open the highlighted search result.
    fn open_selected(&mut self) {
        if let Some(name) = self.results.get(self.selected).cloned() {
            self.go(Loc::Page(name));
        }
    }

    /// Find `text` in the page (empty: close the find bar).
    pub fn set_find(&mut self, text: &str) {
        if text.is_empty() {
            self.find = None;
        } else {
            self.find = Some(Find { query: text.to_string(), reveal: true, ..Find::default() });
        }
    }

    /// The next (or previous) match.
    pub fn find_next(&mut self, forward: bool) {
        if let Some(f) = &mut self.find {
            if f.count > 0 {
                f.current = if forward { (f.current + 1) % f.count } else { (f.current + f.count - 1) % f.count };
            }
            f.reveal = true;
        }
    }

    /// Open every collapsible part of the page (self-test).
    pub fn expand_all(&mut self, _ctx: &egui::Context) {
        self.expand = true;
    }

    /// Scroll the content by `dy` points (self-test).
    pub fn scroll_by(&mut self, dy: f32) {
        self.scroll_by += dy;
    }

    /// The theme.
    pub fn set_theme(&mut self, t: ThemeChoice) {
        self.theme = t;
    }

    /// A line for the self-test log: where the window is and what find counted.
    pub fn describe(&self) -> String {
        let find =
            self.find.as_ref().map_or(String::new(), |f| format!(", find `{}` {}/{}", f.query, f.current + 1, f.count));
        let results = match self.here() {
            Loc::Search(_) => format!(", {} results, first {:?}", self.results.len(), self.results.first()),
            _ => String::new(),
        };
        format!("at {}{results}{find}, last frame {:.1} ms", self.here().key(), self.frame_ms)
    }

    fn update_theme(&mut self, ctx: &egui::Context) -> Palette {
        let system_dark = ctx.system_theme().is_some_and(|t| t == egui::Theme::Dark);
        let palette = self.theme.palette(system_dark);
        if self.applied.as_ref() != Some(&palette) {
            palette.apply(ctx);
            self.applied = Some(palette.clone());
        }
        palette
    }

    /// Copy the page and its images into a temporary folder and open the copy in the notebook.
    fn open_as_notebook(&mut self, name: &str) {
        let dir = std::env::temp_dir().join("mathcas-docs").join(format!("{name}-{}", std::process::id()));
        let result = self
            .lib
            .copy_page(name, &dir)
            .and_then(|page| std::process::Command::new(notebook_program()).arg(page).spawn());
        self.status = match result {
            Ok(_) => Some(format!("Opened a copy of `{name}` in the notebook.")),
            Err(e) => Some(format!("Cannot open the notebook ({}): {e}", notebook_program().display())),
        };
    }

    fn keys(&mut self, ctx: &egui::Context) {
        let (back, forward, home, find, f1, f3, shift_f3) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::ALT, Key::ArrowLeft) || i.pointer.button_pressed(egui::PointerButton::Extra1),
                i.consume_key(Modifiers::ALT, Key::ArrowRight) || i.pointer.button_pressed(egui::PointerButton::Extra2),
                i.consume_key(Modifiers::ALT, Key::Home),
                i.consume_key(Modifiers::COMMAND, Key::F),
                i.consume_key(Modifiers::NONE, Key::F1),
                i.consume_key(Modifiers::NONE, Key::F3),
                i.consume_key(Modifiers::SHIFT, Key::F3),
            )
        });
        if back {
            self.back();
        }
        if forward {
            self.forward();
        }
        if home {
            self.go(Loc::Home);
        }
        if find {
            let f = self.find.get_or_insert_with(Find::default);
            f.focus = true;
        }
        if f1 {
            let q = self.query.trim().to_string();
            if self.cat.get(&q).is_some() {
                self.go(Loc::Page(q));
            } else {
                self.focus_search = true;
            }
        }
        if f3 || shift_f3 {
            self.find_next(f3);
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.horizontal_centered(|ui| {
            let nav = |ui: &mut egui::Ui, label: &str, tip: &str, enabled: bool| {
                ui.add_enabled(enabled, egui::Button::new(RichText::new(label).size(15.0)).frame(false))
                    .on_hover_text(tip)
                    .clicked()
            };
            if nav(ui, "◀", "Back (Alt+Left)", self.at > 0) {
                self.back();
            }
            if nav(ui, "▶", "Forward (Alt+Right)", self.at + 1 < self.history.len()) {
                self.forward();
            }
            if nav(ui, "⌂", "Home (Alt+Home)", true) {
                self.go(Loc::Home);
            }
            ui.add_space(8.0);
            // the result keys act before the box sees them
            let (down, up, enter, escape) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.key_pressed(Key::Enter),
                    i.key_pressed(Key::Escape),
                )
            });
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Search functions (F1)")
                    .desired_width(340.0)
                    .id_salt("search"),
            );
            if std::mem::take(&mut self.focus_search) {
                edit.request_focus();
            }
            if edit.changed() {
                self.searched();
            }
            let searching = matches!(self.here(), Loc::Search(_));
            if searching && !self.results.is_empty() {
                if down {
                    self.selected = (self.selected + 1).min(self.results.len() - 1);
                }
                if up {
                    self.selected = self.selected.saturating_sub(1);
                }
            }
            if edit.lost_focus() && enter && searching {
                self.open_selected();
            }
            if edit.has_focus() && escape {
                self.query.clear();
                self.searched();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (label, next) = match self.theme {
                    ThemeChoice::System => ("Theme: system", ThemeChoice::Light),
                    ThemeChoice::Light => ("Theme: light", ThemeChoice::Dark),
                    ThemeChoice::Dark => ("Theme: dark", ThemeChoice::System),
                };
                if ui.add(egui::Button::new(RichText::new(label).size(12.5)).frame(false)).clicked() {
                    self.theme = next;
                }
                if let Some(s) = &self.status {
                    ui.label(RichText::new(s).size(12.0).color(p.weak));
                }
            });
        });
    }

    fn find_bar(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &mut self.find else { return };
        let mut close = false;
        let mut step = None;
        ui.horizontal_centered(|ui| {
            ui.label("Find in page");
            let edit = ui.add(egui::TextEdit::singleline(&mut f.query).desired_width(260.0).id_salt("find"));
            if std::mem::take(&mut f.focus) {
                edit.request_focus();
            }
            if edit.changed() {
                f.current = 0;
                f.reveal = true;
            }
            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                step = Some(!ui.input(|i| i.modifiers.shift));
                edit.request_focus();
            }
            let counted = if f.query.is_empty() {
                String::new()
            } else if f.count == 0 {
                "no matches".into()
            } else {
                format!("{} of {}", f.current.min(f.count - 1) + 1, f.count)
            };
            ui.label(RichText::new(counted).size(12.5));
            if ui.button("Next").on_hover_text("Enter, F3").clicked() {
                step = Some(true);
            }
            if ui.button("Previous").on_hover_text("Shift+Enter, Shift+F3").clicked() {
                step = Some(false);
            }
            if ui.button("Close").clicked() || (edit.has_focus() && ui.input(|i| i.key_pressed(Key::Escape))) {
                close = true;
            }
        });
        if close {
            self.find = None;
        } else if let Some(forward) = step {
            self.find_next(forward);
        }
    }

    fn content(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let here = self.here().clone();
        let mut area = ScrollArea::vertical().auto_shrink([false, false]).id_salt(here.key());
        if self.scroll_by != 0.0 {
            area = area.vertical_scroll_offset((self.offset + std::mem::take(&mut self.scroll_by)).max(0.0));
        }
        let find = self.find.as_ref().map(|f| (f.query.to_ascii_lowercase(), f.current, f.reveal));
        let mut draw = Draw {
            p,
            lib: &self.lib,
            cat: &self.cat,
            pages: &self.pages,
            images: &mut self.images,
            colours: &mut self.colours,
            find,
            count: 0,
            go: None,
            open_notebook: false,
            selected: self.selected,
            expand: self.expand,
        };
        let out = area.show(ui, |ui| {
            // a column at most MAX_WIDTH wide, centred, with a margin on both sides
            let width = (ui.available_width() - 48.0).clamp(200.0, MAX_WIDTH);
            let side = ((ui.available_width() - width) / 2.0).max(0.0);
            ui.horizontal_top(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    match &here {
                        Loc::Home => draw.home(ui),
                        Loc::Guide(a) => draw.guide(ui, *a),
                        Loc::Page(n) => draw.page(ui, n),
                        Loc::Search(q) => draw.search(ui, q, &self.results),
                    }
                });
            });
        });
        self.offset = out.state.clamped_offset().y;
        let (count, reveal, go, open) =
            (draw.count, draw.find.as_ref().is_some_and(|f| f.2), draw.go, draw.open_notebook);
        if let Some(f) = &mut self.find {
            f.count = count;
            if count > 0 && f.current >= count {
                f.current = 0;
                f.reveal = true;
            } else {
                f.reveal = reveal && count > 0;
            }
        }
        if let Some(loc) = go {
            self.go(loc);
        }
        if open {
            if let Loc::Page(n) = &here {
                self.open_as_notebook(n);
            }
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let place = match self.here() {
            Loc::Home => String::new(),
            Loc::Guide(a) => format!("{} - ", a.title()),
            Loc::Page(n) => format!("{n} - "),
            Loc::Search(q) => format!("Search: {q} - "),
        };
        let title = format!("{place}MathCAS Documentation");
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

/// The notebook program: `MATHCAS_NOTEBOOK`, else `mc-notebook` next to this program.
fn notebook_program() -> PathBuf {
    if let Some(p) = std::env::var_os("MATHCAS_NOTEBOOK") {
        return PathBuf::from(p);
    }
    let name = format!("mc-notebook{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe().map(|e| e.with_file_name(&name)).unwrap_or_else(|_| PathBuf::from(name))
}

impl eframe::App for DocsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let start = std::time::Instant::now();
        // the second frame: the first one reached the screen, the graphics interface works
        self.frames += 1;
        if self.frames == 2 {
            if let Some(info) = &self.gpu {
                mc_ui::gpu::started(info);
            }
        }
        let ctx = ui.ctx().clone();
        if let Some(mut st) = self.selftest.take() {
            st.step(self, &ctx);
            self.selftest = Some(st);
        }
        let p = self.update_theme(&ctx);
        self.keys(&ctx);
        egui::Panel::top("bar")
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(8, 4)))
            .exact_size(38.0)
            .show(ui, |ui| self.top_bar(ui, &p));
        if self.find.is_some() {
            egui::Panel::bottom("find")
                .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(8, 3)))
                .exact_size(34.0)
                .show(ui, |ui| self.find_bar(ui));
        }
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| self.content(ui, &p));
        self.update_title(&ctx);
        self.frame_ms = start.elapsed().as_secs_f64() * 1000.0;
    }
}
