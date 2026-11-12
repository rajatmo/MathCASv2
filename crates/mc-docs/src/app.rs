//! The documentation window: navigation (back, forward, home), the search box with incremental results, find in
//! page, the theme, and "Open as notebook".
use crate::catalogue::Catalogue;
use crate::library::Library;
use crate::page::Page;
use crate::search::{GuideText, Rank, Searchable, Searcher};
use crate::selftest::SelfTest;
use crate::view::{Colours, Draw, Find, Images, Loc, MAX_WIDTH};
use eframe::egui::{self, Frame, Key, Margin, Modifiers, RichText, ScrollArea, ViewportCommand};
use mc_notebook::theme::{Palette, ThemeChoice};
use mc_ui::icons::{self, Icon};
use std::collections::HashMap;
use std::path::PathBuf;

/// The most results a search lists.
const RESULTS: usize = 200;

/// The documentation window's single-instance file in the settings folder (F1 in the notebook reuses an open
/// documentation window: decision NBDOCS.1).
pub const INSTANCE: &str = "docs-instance.json";

/// The documentation window's state.
pub struct DocsApp {
    lib: Library,
    cat: Catalogue,
    pages: HashMap<String, Page>,
    /// the topic guides (round 21)
    guides: Vec<crate::guide::Guide>,
    searcher: Searcher,
    /// how long the last search took (milliseconds), for the self-test log
    pub search_ms: f64,
    /// visited places and the one shown
    history: Vec<Loc>,
    at: usize,
    /// the search box's text, and whether to focus it on the next frame
    query: String,
    focus_search: bool,
    /// the results of the current search and the highlighted one
    results: Vec<(String, Rank)>,
    /// the guides matching the query (indices into `guides`), listed above the functions; `selected` counts them first
    guide_hits: Vec<usize>,
    /// what the guide search looks through
    guide_text: Vec<GuideText>,
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
    /// the example input copied last and when (its button shows a check mark for two seconds)
    copied: Option<(String, std::time::Instant)>,
    /// the copy buttons of the last frame (self-test)
    pub copy_rects: Vec<(egui::Rect, String)>,
    /// the part of the page a place `name#anchor` points at, and for how many more frames to scroll it into view
    /// (the groups it opens animate open)
    target: Option<crate::page::Target>,
    reveal: u8,
    /// the typeset values of the last frame (self-test) and the one selected (a click; Ctrl+C copies it)
    pub value_rects: Vec<(egui::Rect, String)>,
    selected_value: Option<String>,
    /// the clipboard (made on the first copy)
    clipboard: Option<mc_ui::clipboard::Clipboard>,
    /// page outputs drawn as the notebook draws them
    outputs: mc_notebook::outputs::OutputPainter,
    /// places other launches (`mc-docs <name>`, F1 in the notebook) sent to this window
    instance: Option<mc_notebook::instance::Server>,
    /// the links of the last frame (screen rectangles, targets) and the one under the pointer
    pub link_rects: Vec<(egui::Rect, String)>,
    pub hovered: Option<String>,
    /// input events the self-test injects into the next frame
    pub events: Vec<egui::Event>,
}

impl DocsApp {
    /// The window at `start` (home when `None`), with the pages of `lib`.
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        lib: Library,
        start: Option<String>,
        selftest: Option<SelfTest>,
    ) -> DocsApp {
        let defs = mc_ui::fonts::definitions(mc_ui::fonts::Source::from_env());
        cc.egui_ctx.set_fonts(defs.clone());
        let outputs = mc_notebook::outputs::OutputPainter::new(&cc.egui_ctx, &defs);
        let cat = Catalogue::new();
        let pages: HashMap<String, Page> = lib
            .names()
            .into_iter()
            .filter_map(|n| lib.page_text(&n).map(|t| (n.clone(), Page::read(&n, &t).0)))
            .collect();
        // every page's example inputs, to count how much each function is used
        let inputs: Vec<String> = pages
            .values()
            .map(|p| {
                p.inputs()
                    .filter_map(|(_, b)| match b {
                        crate::page::Block::Input { source, .. } => Some(source.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        let uses = crate::search::uses(cat.entries.iter().map(|e| e.name), &inputs);
        let items: Vec<Searchable> = cat
            .entries
            .iter()
            .map(|e| {
                let mut words = e.summary.to_lowercase();
                if let Some(p) = pages.get(e.name) {
                    words.push('\n');
                    words.push_str(&p.words().to_lowercase());
                }
                Searchable {
                    name: e.name.to_string(),
                    words,
                    documented: pages.contains_key(e.name),
                    uses: uses.get(e.name).copied().unwrap_or(0),
                }
            })
            .collect();
        let guides = crate::guide::all(&lib);
        let guide_text = guides.iter().map(|g| g.search_text()).collect();
        let theme = mc_notebook::settings::Settings::load().theme;
        // the self-test's window never takes the place of the user's
        let instance = if selftest.is_none() { mc_notebook::instance::Server::start_as(&cc.egui_ctx, INSTANCE) } else { None };
        let mut colours = Colours::default();
        colours.functions = cat.entries.iter().map(|e| e.name.to_string()).collect();
        let mut app = DocsApp {
            lib,
            cat,
            pages,
            guides,
            searcher: Searcher::new(items),
            search_ms: 0.0,
            history: vec![Loc::Home],
            at: 0,
            query: String::new(),
            focus_search: true,
            results: vec![],
            guide_hits: vec![],
            guide_text,
            selected: 0,
            find: None,
            theme,
            applied: None,
            images: Images::default(),
            colours,
            status: None,
            expand: false,
            scroll_by: 0.0,
            offset: 0.0,
            selftest,
            gpu: cc.wgpu_render_state.as_ref().map(|r| r.adapter.get_info()),
            frames: 0,
            title: String::new(),
            frame_ms: 0.0,
            instance,
            copied: None,
            copy_rects: vec![],
            target: None,
            reveal: 0,
            value_rects: vec![],
            selected_value: None,
            clipboard: None,
            outputs,
            link_rects: vec![],
            hovered: None,
            events: vec![],
        };
        if let Some(place) = start {
            app.go_place(&place);
        }
        app
    }

    /// The part of the page a place pointed at (self-test).
    pub fn target(&self) -> Option<crate::page::Target> {
        self.target
    }

    /// Go to a place as the command line and F1 name it: `name`, `guide:<area>`, `search:<text>`, `index`, or
    /// `name#anchor`, a part of a page (an option's example, a group; [`Page::target`]): the group opens and the
    /// part scrolls into view, marked.
    pub fn go_place(&mut self, place: &str) {
        let (at, anchor) = match place.split_once('#') {
            Some((name, a)) if !name.is_empty() && !name.contains(':') => (name, Some(a)),
            _ => (place, None),
        };
        let loc = Loc::parse(at);
        self.go(loc.clone());
        self.target = match (&loc, anchor) {
            (Loc::Page(n), Some(a)) => self.pages.get(n).and_then(|p| p.target(a)),
            _ => None,
        };
        self.reveal = if self.target.is_some() { 12 } else { 0 };
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

    /// Take places sent by other launches (the self-test's windows do not unless asked).
    pub fn start_instance(&mut self, ctx: &egui::Context) {
        if self.instance.is_none() {
            self.instance = mc_notebook::instance::Server::start_as(ctx, INSTANCE);
        }
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
        self.target = None;
        self.reveal = 0;
        self.selected_value = None;
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
            self.guide_hits.clear();
            if matches!(self.here(), Loc::Search(_)) {
                self.back();
            }
            return;
        }
        let start = std::time::Instant::now();
        self.results = self.searcher.search(&q, RESULTS);
        self.guide_hits = crate::search::search_guides(&self.guide_text, &q);
        self.search_ms = start.elapsed().as_secs_f64() * 1000.0;
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
        let guides = self.guide_hits.len();
        if self.selected < guides {
            let slug = self.guides[self.guide_hits[self.selected]].slug.clone();
            self.go(Loc::Topic(slug));
        } else if let Some((name, _)) = self.results.get(self.selected - guides).cloned() {
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
            Loc::Search(_) => format!(
                ", {} results, first {:?}, search {:.2} ms, guides {:?}",
                self.results.len(),
                self.results.iter().take(3).map(|(n, r)| format!("{n} ({r:?})")).collect::<Vec<_>>(),
                self.search_ms,
                self.guide_hits.iter().map(|&k| self.guides[k].slug.as_str()).collect::<Vec<_>>()
            ),
            _ => String::new(),
        };
        let hovered = self.hovered.as_ref().map_or(String::new(), |h| format!(", link under the pointer `{h}`"));
        format!(
            "at {}{results}{find}, {} links{hovered}, last frame {:.1} ms",
            self.here().key(),
            self.link_rects.len(),
            self.frame_ms
        )
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
    fn open_as_notebook(&mut self, name: &str, file: &str) {
        let dir = std::env::temp_dir().join("mathcas-docs").join(format!("{name}-{}", std::process::id()));
        let result = self
            .lib
            .copy_page(file, &dir)
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
            let nav = |ui: &mut egui::Ui, icon: Icon, tip: &str, enabled: bool| {
                let tint = if enabled { p.text } else { p.faint };
                let b = egui::Button::image(icons::image(ui.ctx(), icon, 16.0, tint)).frame(false).min_size(egui::vec2(26.0, 24.0));
                ui.add_enabled(enabled, b).on_hover_text(tip).on_disabled_hover_text(tip).clicked()
            };
            if nav(ui, Icon::ArrowLeft, "Back (Alt+Left)", self.at > 0) {
                self.back();
            }
            if nav(ui, Icon::ArrowRight, "Forward (Alt+Right)", self.at + 1 < self.history.len()) {
                self.forward();
            }
            if nav(ui, Icon::Home, "Home (Alt+Home)", true) {
                self.go(Loc::Home);
            }
            let index = ui.add(egui::Button::new(RichText::new("Index").size(13.0).color(p.text)).frame(false));
            if index.on_hover_text("Every function by area and topic").clicked() {
                self.go(Loc::Index);
            }
            ui.add_space(8.0);
            ui.add(icons::image(ui.ctx(), Icon::Search, 15.0, p.weak));
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
            let listed = self.results.len() + self.guide_hits.len();
            if searching && listed > 0 {
                if down {
                    self.selected = (self.selected + 1).min(listed - 1);
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
                let (label, icon, next) = match self.theme {
                    ThemeChoice::System => ("Theme: system", Icon::ThemeSystem, ThemeChoice::Light),
                    ThemeChoice::Light => ("Theme: light", Icon::ThemeLight, ThemeChoice::Dark),
                    ThemeChoice::Dark => ("Theme: dark", Icon::ThemeDark, ThemeChoice::System),
                };
                let img = icons::image(ui.ctx(), icon, 15.0, p.yellow);
                if ui.add(egui::Button::image_and_text(img, RichText::new(label).size(12.5)).frame(false)).clicked() {
                    self.theme = next;
                    // kept in the settings the notebook shares, so the next window opens in it (read fresh, so
                    // a notebook's own changes since start-up are kept)
                    let mut settings = mc_notebook::settings::Settings::load();
                    settings.theme = next;
                    settings.save();
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
            let color = ui.visuals().text_color();
            let icon = |ui: &mut egui::Ui, i: Icon, tip: &str| {
                ui.add(egui::Button::image(icons::image(ui.ctx(), i, 14.0, color)).min_size(egui::vec2(26.0, 22.0)))
                    .on_hover_text(tip)
            };
            if icon(ui, Icon::ArrowDown, "Next (Enter, F3)").clicked() {
                step = Some(true);
            }
            if icon(ui, Icon::ArrowUp, "Previous (Shift+Enter, Shift+F3)").clicked() {
                step = Some(false);
            }
            if icon(ui, Icon::Close, "Close (Esc)").clicked() || (edit.has_focus() && ui.input(|i| i.key_pressed(Key::Escape))) {
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
            guides: &self.guides,
            images: &mut self.images,
            colours: &mut self.colours,
            find,
            count: 0,
            go: None,
            open_notebook: false,
            selected: self.selected,
            expand: self.expand,
            link_rects: vec![],
            hovered: None,
            copy: None,
            copied: self.copied.as_ref().filter(|(_, t)| t.elapsed().as_secs_f32() < 2.0).map(|(s, _)| s.clone()),
            copy_rects: vec![],
            outputs: &mut self.outputs,
            target: self.target,
            reveal_target: self.reveal > 0,
            value_rects: vec![],
            selected_value: self.selected_value.clone(),
            select_value: None,
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
                        Loc::Search(q) => draw.search(ui, q, &self.guide_hits, &self.results),
                        Loc::Index => draw.index(ui),
                        Loc::Topic(s) => draw.topic(ui, s),
                    }
                });
            });
        });
        self.offset = out.state.clamped_offset().y;
        let (count, reveal, go, open) =
            (draw.count, draw.find.as_ref().is_some_and(|f| f.2), draw.go, draw.open_notebook);
        self.link_rects = draw.link_rects;
        self.hovered = draw.hovered;
        self.copy_rects = draw.copy_rects;
        self.value_rects = draw.value_rects;
        self.reveal = self.reveal.saturating_sub(1);
        if let Some(sel) = draw.select_value {
            self.selected_value = sel;
        }
        if let Some(src) = draw.copy {
            self.copy_example(ui.ctx(), &src);
        }
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
            match &here {
                Loc::Page(n) => self.open_as_notebook(n, n),
                Loc::Topic(s) => {
                    if let Some(g) = self.guides.iter().find(|g| g.slug == *s) {
                        let stem = g.stem.clone();
                        self.open_as_notebook(&stem, &format!("guides/{stem}"));
                    }
                }
                _ => {}
            }
        }
    }

    /// Put an example input on the clipboard (the copy button of an input, round 19).
    pub fn copy_example(&mut self, ctx: &egui::Context, source: &str) {
        let clip = self.clipboard.get_or_insert_with(|| mc_ui::clipboard::Clipboard::for_window(ctx));
        clip.set(mc_ui::clipboard::Contents { text: Some(source.to_string()), ..Default::default() });
        self.copied = Some((source.to_string(), std::time::Instant::now()));
        ctx.request_repaint_after(std::time::Duration::from_millis(2100));
    }

    /// The clipboard's text (self-test).
    pub fn clipboard_text(&mut self, ctx: &egui::Context) -> Option<String> {
        let clip = self.clipboard.get_or_insert_with(|| mc_ui::clipboard::Clipboard::for_window(ctx));
        clip.get().text
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let place = match self.here() {
            Loc::Home => String::new(),
            Loc::Guide(a) => format!("{} - ", a.title()),
            Loc::Page(n) => format!("{n} - "),
            Loc::Search(q) => format!("Search: {q} - "),
            Loc::Index => "Index - ".into(),
            Loc::Topic(s) => format!("{} - ", self.guides.iter().find(|g| g.slug == *s).map_or(s.as_str(), |g| g.title.as_str())),
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
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        for e in self.events.drain(..) {
            if let egui::Event::PointerButton { modifiers, pressed: true, .. } = &e {
                raw.events.push(egui::Event::ModifiersChanged(*modifiers));
            }
            raw.events.push(e);
        }
    }

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
        // places sent by other launches: shown here, the window brought to the front
        let sent = self.instance.as_ref().map(|i| i.take()).unwrap_or_default();
        if !sent.is_empty() {
            for place in sent {
                let place = place.to_string_lossy().to_string();
                if !place.is_empty() {
                    self.go_place(&place);
                }
            }
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
        if let Some(mut st) = self.selftest.take() {
            st.step(self, &ctx);
            self.selftest = Some(st);
        }
        let p = self.update_theme(&ctx);
        self.outputs.begin_frame();
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
