//! Drawing: text runs (prose with code spans, coloured source), find-in-page marks, images, and the four kinds of
//! location (home, guide, function page, search results).
use crate::catalogue::{Area, Catalogue, Entry};
use mc_ui::fonts::Face;
use crate::inline::{spans, Span};
use crate::library::Library;
use crate::page::{image_path, Block, Page};
use eframe::egui::{self, pos2, vec2, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke};
use eframe::epaint::text::{ByteIndex, LayoutJob, LayoutSection, TextFormat};
use mc_nbfile::{OutputKind, OutputLine};
use mc_notebook::theme::Palette;
use mc_syntax::TokenClass;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

/// Text sizes in egui points.
const BODY: f32 = 15.0;
const CODE: f32 = 14.0;
/// How far outputs sit right of their input (the rule that marks them is drawn in this margin).
const OUTPUT_INDENT: i8 = 14;
/// The widest the content gets (wider windows centre it).
pub const MAX_WIDTH: f32 = 980.0;

/// A place in the documentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loc {
    /// the home page with the area tiles
    Home,
    /// every function of an area
    Guide(Area),
    /// a function's page (or its description when it has no page)
    Page(String),
    /// search results for a query
    Search(String),
}

impl Loc {
    /// A stable key (scroll positions are kept per location).
    pub fn key(&self) -> String {
        match self {
            Loc::Home => "home".into(),
            Loc::Guide(a) => format!("guide:{}", a.key()),
            Loc::Page(n) => format!("page:{n}"),
            Loc::Search(_) => "search".into(),
        }
    }

    /// The location a self-test or command line names: `home`, `guide:<area>`, `search:<text>`, else a page.
    pub fn parse(s: &str) -> Loc {
        if s == "home" {
            Loc::Home
        } else if let Some(a) = s.strip_prefix("guide:").and_then(Area::from_key) {
            Loc::Guide(a)
        } else if let Some(q) = s.strip_prefix("search:") {
            Loc::Search(q.to_string())
        } else {
            Loc::Page(s.strip_prefix("page:").unwrap_or(s).to_string())
        }
    }
}

/// The look of a token class (the notebook's roles, design 8.2): colour, italic, bold.
fn class_style(class: TokenClass, p: &Palette) -> (Color32, bool, bool) {
    use TokenClass::*;
    match class {
        Operator | Global => (p.text, false, false),
        Undefined => (p.blue, false, false),
        Local => (p.cyan, false, false),
        PatternVar => (p.cyan, true, false),
        Shadowing => (p.dark_red, false, false),
        Keyword => (p.purple, false, false),
        Constant | Number => (p.orange, false, false),
        String => (p.green, false, false),
        Comment | Placeholder => (p.faint, class == Comment, false),
        // usage lines are templates (`f(a, b, ...)`), not code: their syntax errors are not marked
        SyntaxError | UnmatchedBracket => (p.text, false, false),
    }
}

/// The token classes of code snippets, by source. Colouring parses on a thread of its own (`mc_syntax`), too slow
/// to repeat for every snippet of a page in every frame.
#[derive(Default)]
pub struct Colours {
    known: HashMap<String, Tokens>,
}

/// The classed tokens of a snippet (`mc_syntax::Colouring::tokens`).
type Tokens = Arc<Vec<(Range<usize>, TokenClass)>>;

impl Colours {
    fn tokens(&mut self, src: &str) -> Tokens {
        if let Some(t) = self.known.get(src) {
            return t.clone();
        }
        let t = Arc::new(mc_syntax::colour(src, None).tokens);
        self.known.insert(src.to_string(), t.clone());
        t
    }

    /// Append `src` to `job` in the code font, coloured as source.
    pub fn append_code(&mut self, job: &mut LayoutJob, src: &str, p: &Palette, size: f32, bold: bool) {
        let tokens = self.tokens(src);
        let face = if bold { Face::CodeBold } else { Face::Code };
        let base = TextFormat { font_id: face.font(size), color: p.text, ..TextFormat::default() };
        let mut cuts: Vec<usize> = vec![0, src.len()];
        for (r, _) in tokens.iter() {
            cuts.extend([r.start, r.end]);
        }
        cuts.retain(|&c| c <= src.len() && src.is_char_boundary(c));
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let mut f = base.clone();
            // the tokens are sorted and do not overlap: the one holding this piece, if any
            let at = tokens.partition_point(|(r, _)| r.end <= w[0]);
            if let Some((r, class)) = tokens.get(at).filter(|(r, _)| r.start <= w[0]) {
                debug_assert!(r.contains(&w[0]));
                let (color, italic, strong) = class_style(*class, p);
                f.color = color;
                f.italics = italic;
                if strong {
                    f.font_id = Face::CodeBold.font(size);
                }
            }
            job.append(&src[w[0]..w[1]], 0.0, f);
        }
    }

    /// Append prose with code spans to `job`.
    pub fn append_prose(&mut self, job: &mut LayoutJob, text: &str, p: &Palette, size: f32, color: Color32) {
        let format = TextFormat { font_id: Face::Prose.font(size), color, ..TextFormat::default() };
        match spans(text) {
            Ok(parts) => {
                for s in parts {
                    match s {
                        Span::Text(t) => job.append(t, 0.0, format.clone()),
                        Span::Code(c) => self.append_code(job, c, p, size - 1.0, false),
                    }
                }
            }
            Err(_) => job.append(text, 0.0, format),
        }
    }
}

/// Set the background of the parts of `job` inside `ranges`.
fn paint_ranges(job: &mut LayoutJob, ranges: &[(Range<usize>, Color32)]) {
    if ranges.is_empty() {
        return;
    }
    let mut out = Vec::with_capacity(job.sections.len() + 2 * ranges.len());
    for s in job.sections.drain(..) {
        let (a, b) = (s.byte_range.start.0, s.byte_range.end.0);
        let mut cuts = vec![a, b];
        for (r, _) in ranges {
            cuts.extend([r.start, r.end].into_iter().filter(|&c| c > a && c < b));
        }
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let mut t = LayoutSection {
                leading_space: 0.0,
                byte_range: ByteIndex(w[0])..ByteIndex(w[1]),
                format: s.format.clone(),
            };
            if w[0] == a {
                t.leading_space = s.leading_space;
            }
            if let Some((_, c)) = ranges.iter().find(|(r, _)| r.start <= w[0] && w[0] < r.end) {
                t.format.background = *c;
            }
            out.push(t);
        }
    }
    job.sections = out;
}

/// The decoded images of the pages, as textures.
#[derive(Default)]
pub struct Images {
    loaded: HashMap<String, Option<(egui::TextureHandle, [usize; 2])>>,
}

impl Images {
    fn get(&mut self, ctx: &egui::Context, lib: &Library, rel: &str) -> Option<(egui::TextureHandle, [usize; 2])> {
        self.loaded
            .entry(rel.to_string())
            .or_insert_with(|| {
                let image = decode_png(lib.file(rel)?)?;
                let size = image.size;
                Some((ctx.load_texture(rel, image, egui::TextureOptions::LINEAR), size))
            })
            .clone()
    }
}

/// A PNG file as an egui image.
pub fn decode_png(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data.as_chunks::<3>().0.iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            data.as_chunks::<2>().0.iter().flat_map(|c| [c[0], c[0], c[0], c[1]]).collect()
        }
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == w * h * 4).then(|| egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba))
}

/// Find in page: the query, and the matches counted while drawing.
#[derive(Clone, Debug, Default)]
pub struct Find {
    /// the text looked for
    pub query: String,
    /// the current match (0-based)
    pub current: usize,
    /// the matches of the last frame
    pub count: usize,
    /// scroll the current match into view on the next frame
    pub reveal: bool,
    /// focus the find box on the next frame
    pub focus: bool,
}

/// What a frame's drawing needs and what it reports back.
pub struct Draw<'a> {
    /// the palette
    pub p: &'a Palette,
    /// the pages and their images
    pub lib: &'a Library,
    /// the catalogue
    pub cat: &'a Catalogue,
    /// the parsed pages
    pub pages: &'a HashMap<String, Page>,
    /// the textures
    pub images: &'a mut Images,
    /// the colourings of code snippets
    pub colours: &'a mut Colours,
    /// the find query in ASCII lower case (empty: no find), the current match, whether to scroll to it
    pub find: Option<(String, usize, bool)>,
    /// matches counted so far this frame
    pub count: usize,
    /// a link that was clicked
    pub go: Option<Loc>,
    /// "Open as notebook" was clicked
    pub open_notebook: bool,
    /// the highlighted search result
    pub selected: usize,
    /// open every collapsible part
    pub expand: bool,
}

impl Draw<'_> {
    /// Show `job` as a selectable, wrapping label, with find matches marked.
    fn text(&mut self, ui: &mut egui::Ui, mut job: LayoutJob) -> egui::Response {
        let mut current_here = false;
        if let Some((q, current, _)) = &self.find {
            if !q.is_empty() {
                let lower = job.text.to_ascii_lowercase();
                let mut ranges = vec![];
                for (at, _) in lower.match_indices(q.as_str()) {
                    let is_current = self.count == *current;
                    current_here |= is_current;
                    let c = if is_current {
                        self.p.over_bg(self.p.orange, 0.55)
                    } else {
                        self.p.over_bg(self.p.yellow, 0.35)
                    };
                    ranges.push((at..at + q.len(), c));
                    self.count += 1;
                }
                paint_ranges(&mut job, &ranges);
            }
        }
        job.wrap.max_width = ui.available_width();
        let response = ui.add(egui::Label::new(job).selectable(true));
        if current_here {
            if let Some((_, _, reveal)) = &mut self.find {
                if *reveal {
                    ui.scroll_to_rect(response.rect, Some(Align::Center));
                    *reveal = false;
                }
            }
        }
        response
    }

    fn prose(&mut self, ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
        let mut job = LayoutJob::default();
        self.colours.append_prose(&mut job, text, self.p, size, color);
        self.text(ui, job);
    }

    fn heading(&mut self, ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
        let mut job = LayoutJob::default();
        job.append(text, 0.0, TextFormat { font_id: Face::ProseBold.font(size), color, ..TextFormat::default() });
        self.text(ui, job);
    }

    /// A function name as a link (accent when it has a page).
    fn name_link(&mut self, ui: &mut egui::Ui, name: &str, size: f32) {
        let has_page = self.pages.contains_key(name);
        let color = if has_page { self.p.accent } else { self.p.text };
        let r = ui.add(egui::Link::new(RichText::new(name).font(Face::Code.font(size)).color(color)));
        if r.clicked() {
            self.go = Some(Loc::Page(name.to_string()));
        }
        let entry = self.cat.get(name);
        if let Some(e) = entry {
            r.on_hover_text(e.summary);
        }
    }

    fn card<R>(p: &Palette, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        Frame::new()
            .fill(p.card)
            .stroke(Stroke::new(1.0, if p.dark { p.rule() } else { p.border }))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(14))
            .show(ui, add)
            .inner
    }

    // ----- home -----

    /// The home page: a title and one tile per area.
    pub fn home(&mut self, ui: &mut egui::Ui) {
        ui.add_space(18.0);
        self.heading(ui, "MathCAS Documentation", 28.0, self.p.text);
        self.prose(
            ui,
            "Every function of the system, sorted into five areas. Type in the search box above to find a function \
             by name or by what it does; `F1` jumps to the name typed there.",
            BODY,
            self.p.weak,
        );
        ui.add_space(14.0);
        let columns = ((ui.available_width() + 16.0) / 320.0).floor().clamp(1.0, 3.0) as usize;
        for row in Area::ALL.chunks(columns) {
            ui.columns(columns, |cols| {
                for (ui, &area) in cols.iter_mut().zip(row) {
                    self.tile(ui, area);
                }
            });
            ui.add_space(12.0);
        }
    }

    fn tile(&mut self, ui: &mut egui::Ui, area: Area) {
        let cat = self.cat;
        let topics = cat.topics(area);
        let total: usize = topics.iter().map(|(_, v)| v.len()).sum();
        let mut documented: Vec<&str> =
            topics.iter().flat_map(|(_, v)| v.iter()).map(|e| e.name).filter(|n| self.pages.contains_key(*n)).collect();
        documented.sort_unstable();
        let p = self.p;
        Self::card(p, ui, |ui| {
            ui.set_min_height(170.0);
            ui.set_width(ui.available_width());
            // the columns justify their children, which would spread wrapped text to both edges
            ui.with_layout(Layout::top_down(Align::Min), |ui| self.tile_body(ui, area, &topics, &documented, total));
        });
    }

    fn tile_body(
        &mut self,
        ui: &mut egui::Ui,
        area: Area,
        topics: &[(&str, Vec<&Entry>)],
        documented: &[&str],
        total: usize,
    ) {
        let title =
            ui.add(egui::Link::new(RichText::new(area.title()).font(Face::ProseBold.font(18.0)).color(self.p.blue)));
        if title.clicked() {
            self.go = Some(Loc::Guide(area));
        }
        self.prose(ui, area.blurb(), 13.5, self.p.weak);
        ui.add_space(6.0);
        if !documented.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for n in documented {
                    self.name_link(ui, n, 13.5);
                }
            });
        }
        let names: Vec<&str> = topics.iter().map(|(t, _)| *t).collect();
        self.prose(ui, &names.join(" · "), 12.5, self.p.faint);
        ui.add_space(4.0);
        let all = ui.horizontal(|ui| {
            let r = ui.link(RichText::new(format!("All {total} functions")).size(13.0));
            ui.add(mc_ui::icons::image(ui.ctx(), mc_ui::icons::Icon::ArrowRight, 13.0, self.p.blue));
            r.clicked()
        });
        if all.inner {
            self.go = Some(Loc::Guide(area));
        }
    }

    // ----- guides -----

    /// Every function of `area`, by topic, with its description and how many of the topic's functions have a page;
    /// at the end, the functions of the area without a page yet.
    pub fn guide(&mut self, ui: &mut egui::Ui, area: Area) {
        ui.add_space(14.0);
        self.heading(ui, area.title(), 26.0, self.p.text);
        self.prose(ui, area.blurb(), BODY, self.p.weak);
        let cat = self.cat;
        let topics = cat.topics(area);
        let mut without: Vec<&str> = vec![];
        for (topic, entries) in &topics {
            ui.add_space(14.0);
            let documented = entries.iter().filter(|e| self.pages.contains_key(e.name)).count();
            ui.horizontal(|ui| {
                self.heading(ui, topic, 18.0, self.p.blue);
                ui.label(
                    RichText::new(format!("{documented} of {} with a page", entries.len())).size(12.5).color(self.p.faint),
                );
            });
            ui.add_space(2.0);
            for e in entries {
                self.entry_row(ui, e);
                if !self.pages.contains_key(e.name) {
                    without.push(e.name);
                }
            }
        }
        if !without.is_empty() {
            ui.add_space(18.0);
            self.heading(ui, &format!("Without a page yet ({})", without.len()), 18.0, self.p.blue);
            self.prose(
                ui,
                "These functions show only their one-line description; the pages above have examples.",
                13.0,
                self.p.faint,
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for n in without {
                    self.name_link(ui, n, 13.5);
                    ui.add_space(4.0);
                }
            });
        }
        ui.add_space(24.0);
    }

    /// The documented functions before and after `name` in its area, in the guide's order (by topic, then name).
    pub fn neighbours(&self, name: &str) -> (Option<&'static str>, Option<&'static str>) {
        let Some(e) = self.cat.get(name) else { return (None, None) };
        let order: Vec<&'static str> = self
            .cat
            .topics(e.area)
            .into_iter()
            .flat_map(|(_, v)| v.into_iter().map(|e| e.name))
            .filter(|n| self.pages.contains_key(*n))
            .collect();
        match order.iter().position(|n| *n == name) {
            Some(i) => (i.checked_sub(1).map(|j| order[j]), order.get(i + 1).copied()),
            None => (None, None),
        }
    }

    /// The row under a page: the previous and next documented functions of its area, and the area's guide.
    fn area_navigation(&mut self, ui: &mut egui::Ui, name: &str) {
        let Some(area) = self.cat.get(name).map(|e| e.area) else { return };
        let (prev, next) = self.neighbours(name);
        ui.add_space(14.0);
        ui.separator();
        ui.horizontal(|ui| {
            if let Some(p) = prev {
                ui.add(mc_ui::icons::image(ui.ctx(), mc_ui::icons::Icon::ArrowLeft, 14.0, self.p.blue));
                if ui.link(RichText::new(p).font(Face::Code.font(CODE))).clicked() {
                    self.go = Some(Loc::Page(p.to_string()));
                }
            }
            ui.add_space(16.0);
            if ui.link(RichText::new(area.title()).size(13.5).color(self.p.blue)).clicked() {
                self.go = Some(Loc::Guide(area));
            }
            if let Some(n) = next {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // right to left: the arrow first, then the name before it
                    ui.add(mc_ui::icons::image(ui.ctx(), mc_ui::icons::Icon::ArrowRight, 14.0, self.p.blue));
                    if ui.link(RichText::new(n).font(Face::Code.font(CODE))).clicked() {
                        self.go = Some(Loc::Page(n.to_string()));
                    }
                });
            }
        });
    }

    fn entry_row(&mut self, ui: &mut egui::Ui, e: &Entry) {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(190.0, 18.0), Layout::left_to_right(Align::Min), |ui| {
                ui.set_width(190.0);
                self.name_link(ui, e.name, CODE);
            });
            ui.vertical(|ui| self.prose(ui, &summary_text(e), 13.5, self.p.weak));
        });
        ui.add_space(2.0);
    }

    // ----- search -----

    /// The results of a search.
    pub fn search(&mut self, ui: &mut egui::Ui, query: &str, results: &[String]) {
        ui.add_space(14.0);
        self.heading(ui, &format!("Search: {query}"), 22.0, self.p.text);
        if results.is_empty() {
            self.prose(
                ui,
                "No function matches. Try a shorter part of the name, or a word from what it does.",
                BODY,
                self.p.weak,
            );
            return;
        }
        self.prose(ui, &format!("{} functions. Enter opens the highlighted one.", results.len()), 13.0, self.p.faint);
        ui.add_space(8.0);
        for (k, name) in results.iter().enumerate() {
            let fill = if k == self.selected { self.p.over_bg(self.p.accent, 0.12) } else { Color32::TRANSPARENT };
            let r = Frame::new()
                .fill(fill)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match self.cat.get(name) {
                        Some(e) => self.entry_row(ui, e),
                        None => self.name_link(ui, name, CODE),
                    }
                });
            if k == self.selected {
                r.response.scroll_to_me(None);
            }
        }
    }

    // ----- function pages -----

    /// The page of function `name`: its page when it has one, else its description from the catalogue.
    pub fn page(&mut self, ui: &mut egui::Ui, name: &str) {
        ui.add_space(14.0);
        let pages = self.pages;
        ui.horizontal(|ui| {
            let mut job = LayoutJob::default();
            job.append(
                name,
                0.0,
                TextFormat { font_id: Face::CodeBold.font(26.0), color: self.p.text, ..TextFormat::default() },
            );
            ui.add(egui::Label::new(job).selectable(true));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let open = egui::Button::image_and_text(
                    mc_ui::icons::image(ui.ctx(), mc_ui::icons::Icon::Notebook, 15.0, ui.visuals().text_color()),
                    "Open as notebook",
                );
                if pages.contains_key(name)
                    && ui
                        .add(open)
                        .on_hover_text("A copy of this page in the notebook, to run and change")
                        .clicked()
                {
                    self.open_notebook = true;
                }
                if let Some(e) = self.cat.get(name) {
                    ui.label(RichText::new(format!("{} · {}", e.area.title(), e.topic)).size(12.0).color(self.p.faint));
                }
            });
        });
        ui.add_space(6.0);
        let Some(page) = pages.get(name) else {
            self.stub(ui, name);
            return;
        };
        self.usage(ui, page);
        let open = (self.expand || self.find.as_ref().is_some_and(|f| !f.0.is_empty())).then_some(true);
        ui.add_space(10.0);
        egui::CollapsingHeader::new(RichText::new("Details").font(Face::ProseBold.font(18.0)).color(self.p.blue))
            .icon(mc_ui::icons::collapsing_marker)
            .id_salt(("details", name))
            .default_open(false)
            .open(open)
            .show(ui, |ui| {
                for item in &page.details {
                    ui.horizontal_top(|ui| {
                        ui.add_space(f32::from(item.level) * 22.0);
                        // the bullet: the icon set's dot, on the first line's middle
                        let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, BODY * 1.35), egui::Sense::hover());
                        let dot = egui::Rect::from_center_size(r.center(), egui::vec2(8.0, 8.0));
                        mc_ui::icons::paint(ui.painter(), mc_ui::icons::Icon::Dot, dot, self.p.blue);
                        ui.vertical(|ui| self.prose(ui, &item.text, BODY, self.p.text));
                    });
                    ui.add_space(3.0);
                }
            });
        ui.add_space(6.0);
        egui::CollapsingHeader::new(RichText::new("Examples").font(Face::ProseBold.font(18.0)).color(self.p.blue))
            .icon(mc_ui::icons::collapsing_marker)
            .id_salt(("examples", name))
            .default_open(true)
            .open(open)
            .show(ui, |ui| {
                for (k, group) in page.examples.iter().enumerate() {
                    let title = format!("{}  ({})", group.title, group.inputs());
                    egui::CollapsingHeader::new(
                        RichText::new(title).font(Face::ProseBold.font(15.5)).color(self.p.text),
                    )
                    .icon(mc_ui::icons::collapsing_marker)
                    .id_salt(("group", name, k))
                    .default_open(k == 0)
                    .open(open)
                    .show(ui, |ui| self.group(ui, name, &group.blocks));
                }
            });
        if !page.see_also.is_empty() {
            ui.add_space(10.0);
            self.heading(ui, "See also", 18.0, self.p.blue);
            ui.horizontal_wrapped(|ui| {
                for n in &page.see_also {
                    self.name_link(ui, n, CODE);
                    ui.add_space(6.0);
                }
            });
        }
        self.area_navigation(ui, name);
        ui.add_space(30.0);
    }

    fn usage(&mut self, ui: &mut egui::Ui, page: &Page) {
        Frame::new()
            .fill(self.p.code_bg())
            .stroke(Stroke::new(1.0, if self.p.dark { self.p.rule() } else { self.p.border }))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (k, u) in page.usage.iter().enumerate() {
                    if k > 0 {
                        ui.add_space(4.0);
                        ui.separator();
                    }
                    let mut job = LayoutJob::default();
                    self.colours.append_code(&mut job, &u.call, self.p, 15.0, true);
                    self.text(ui, job);
                    ui.horizontal_top(|ui| {
                        ui.add_space(24.0);
                        ui.vertical(|ui| self.prose(ui, &u.text, BODY, self.p.text));
                    });
                }
            });
    }

    fn stub(&mut self, ui: &mut egui::Ui, name: &str) {
        let Some(e) = self.cat.get(name) else {
            self.prose(ui, &format!("There is no function `{name}`."), BODY, self.p.weak);
            return;
        };
        Frame::new()
            .fill(self.p.code_bg())
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.prose(ui, &summary_text(e), BODY, self.p.text);
            });
        ui.add_space(10.0);
        self.prose(
            ui,
            "This function has no page yet: the line above is its description from the library that defines it.",
            13.5,
            self.p.faint,
        );
    }

    /// An example group: its texts and inputs with their outputs. Inputs start at the group's content edge, in the
    /// bold code font with syntax colouring; each input's outputs follow, indented behind a thin rule.
    fn group(&mut self, ui: &mut egui::Ui, name: &str, blocks: &[Block]) {
        for b in blocks {
            match b {
                Block::Text(t) => {
                    ui.add_space(6.0);
                    self.prose(ui, t, BODY, self.p.text);
                }
                Block::Input { source, output, .. } => {
                    ui.add_space(4.0);
                    let mut job = LayoutJob::default();
                    self.colours.append_code(&mut job, source, self.p, CODE, true);
                    self.text(ui, job);
                    if !output.is_empty() {
                        self.outputs(ui, name, output);
                    }
                }
            }
        }
        ui.add_space(8.0);
    }

    /// The output lines of one input, as runs of the same kind (values, print lines, messages, notes; images
    /// drawn), indented behind a thin vertical rule so that they read apart from the input above.
    fn outputs(&mut self, ui: &mut egui::Ui, name: &str, lines: &[OutputLine]) {
        let rule = if self.p.dark { self.p.rule() } else { self.p.border };
        let r = Frame::new().inner_margin(Margin { left: OUTPUT_INDENT, right: 0, top: 1, bottom: 1 }).show(ui, |ui| {
            let mut k = 0;
            while k < lines.len() {
                let kind = lines[k].kind;
                let end = (k..lines.len()).find(|&j| lines[j].kind != kind).unwrap_or(lines.len());
                let run = &lines[k..end];
                k = end;
                ui.add_space(2.0);
                if kind == OutputKind::Value && run.len() == 1 {
                    if let Some(path) = image_path(&run[0]) {
                        let path = path.to_string();
                        self.image(ui, name, &path);
                        continue;
                    }
                }
                let text = run.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
                let color = match kind {
                    OutputKind::Value | OutputKind::Print => self.p.text,
                    OutputKind::Message => self.p.dark_red,
                    OutputKind::Note => self.p.orange,
                };
                let mut job = LayoutJob::default();
                job.append(&text, 0.0, TextFormat { font_id: Face::Code.font(CODE), color, ..TextFormat::default() });
                self.text(ui, job);
            }
        });
        let rect = r.response.rect;
        let x = rect.left() + 3.0;
        ui.painter().line_segment([pos2(x, rect.top() + 3.0), pos2(x, rect.bottom() - 1.0)], Stroke::new(2.0, rule));
    }

    fn image(&mut self, ui: &mut egui::Ui, _page: &str, rel: &str) {
        match self.images.get(ui.ctx(), self.lib, rel) {
            Some((tex, [w, h])) => {
                // as the notebook shows images: a pixel of the image per point, at most the column's width
                let natural = vec2(w as f32, h as f32);
                let scale = (ui.available_width() / natural.x).min(1.0);
                ui.add(egui::Image::new(&tex).fit_to_exact_size(natural * scale).sense(Sense::hover()));
            }
            None => {
                ui.label(RichText::new(format!("missing image {rel}")).color(self.p.dark_red));
            }
        }
    }
}

/// An entry's description as prose: the signature before the first `:` as code, the rest as text.
pub fn summary_text(e: &Entry) -> String {
    match e.summary.split_once("): ") {
        Some((call, rest)) if call.starts_with(e.name) && !call.contains('`') => format!("`{call})`: {rest}"),
        _ => e.summary.to_string(),
    }
}
