//! Drawing: text runs (prose with code spans, coloured source), find-in-page marks, images, and the four kinds of
//! location (home, guide, function page, search results).
use crate::catalogue::{Area, Catalogue, Entry};
use mc_ui::fonts::Face;
use crate::inline::{spans, Span};
use crate::guide::{Guide, Part};
use crate::library::Library;
use crate::links::{code_links, Link};
use crate::page::{image_path, Block, Page, Target};
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
    /// the index: every function by area and topic
    Index,
    /// a topic guide, by its slug (round 21)
    Topic(String),
}

impl Loc {
    /// A stable key (scroll positions are kept per location).
    pub fn key(&self) -> String {
        match self {
            Loc::Home => "home".into(),
            Loc::Guide(a) => format!("guide:{}", a.key()),
            Loc::Page(n) => format!("page:{n}"),
            Loc::Search(_) => "search".into(),
            Loc::Index => "index".into(),
            Loc::Topic(s) => format!("topic:{s}"),
        }
    }

    /// The location a self-test or command line names: `home`, `index`, `guide:<area>`, `topic:<slug>`, `search:<text>`,
    /// else a page.
    pub fn parse(s: &str) -> Loc {
        if s == "home" {
            Loc::Home
        } else if s == "index" {
            Loc::Index
        } else if let Some(a) = s.strip_prefix("guide:").and_then(Area::from_key) {
            Loc::Guide(a)
        } else if let Some(t) = s.strip_prefix("topic:") {
            Loc::Topic(t.to_string())
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
    known: HashMap<String, (Tokens, Arc<Vec<Link>>)>,
    /// the names that are links (the catalogue's functions)
    pub functions: std::collections::HashSet<String>,
}

/// The classed tokens of a snippet (`mc_syntax::Colouring::tokens`).
type Tokens = Arc<Vec<(Range<usize>, TokenClass)>>;

/// `links` moved by `by` bytes.
fn shifted(links: &[Link], by: usize) -> Vec<Link> {
    links.iter().map(|(r, n)| (r.start + by..r.end + by, n.clone())).collect()
}

impl Colours {
    fn tokens(&mut self, src: &str) -> (Tokens, Arc<Vec<Link>>) {
        if let Some(t) = self.known.get(src) {
            return t.clone();
        }
        let t = Arc::new(mc_syntax::colour(src, None).tokens);
        let functions = &self.functions;
        let links = Arc::new(code_links(src, &t, &|n: &str| functions.contains(n)));
        self.known.insert(src.to_string(), (t.clone(), links.clone()));
        (t, links)
    }

    /// The links of `src` (byte ranges in `src`).
    pub fn links(&mut self, src: &str) -> Vec<Link> {
        self.tokens(src).1.to_vec()
    }

    /// Append `src` to `job` in the code font, coloured as source; the links of the appended text (in `job`'s
    /// bytes). With `link_colour` the linked names take that colour.
    pub fn append_code(
        &mut self,
        job: &mut LayoutJob,
        src: &str,
        p: &Palette,
        size: f32,
        bold: bool,
        link_colour: Option<Color32>,
    ) -> Vec<Link> {
        let (tokens, links) = self.tokens(src);
        let at = job.text.len();
        let face = if bold { Face::CodeBold } else { Face::Code };
        let base = TextFormat { font_id: face.font(size), color: p.text, ..TextFormat::default() };
        let mut cuts: Vec<usize> = vec![0, src.len()];
        for (r, _) in tokens.iter() {
            cuts.extend([r.start, r.end]);
        }
        cuts.retain(|&c| c <= src.len() && src.is_char_boundary(c));
        cuts.sort_unstable();
        cuts.dedup();
        for (r, _) in links.iter() {
            cuts.extend([r.start, r.end]);
        }
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
            if let Some(c) = link_colour.filter(|_| links.iter().any(|(r, _)| r.start <= w[0] && w[0] < r.end)) {
                f.color = c;
            }
            job.append(&src[w[0]..w[1]], 0.0, f);
        }
        shifted(&links, at)
    }

    /// Append printed output `text` to `job` in the code font and one colour; its links (in `job`'s bytes).
    pub fn append_output(&mut self, job: &mut LayoutJob, text: &str, size: f32, color: Color32) -> Vec<Link> {
        let (_, links) = self.tokens(text);
        let at = job.text.len();
        job.append(text, 0.0, TextFormat { font_id: Face::Code.font(size), color, ..TextFormat::default() });
        shifted(&links, at)
    }

    /// Append prose with code spans to `job`; the links of its code spans (names in the accent colour).
    pub fn append_prose(&mut self, job: &mut LayoutJob, text: &str, p: &Palette, size: f32, color: Color32) -> Vec<Link> {
        let format = TextFormat { font_id: Face::Prose.font(size), color, ..TextFormat::default() };
        let mut links = vec![];
        match spans(text) {
            Ok(parts) => {
                for s in parts {
                    match s {
                        Span::Text(t) => job.append(t, 0.0, format.clone()),
                        Span::Code(c) => links.extend(self.append_code(job, c, p, size - 1.0, false, Some(p.accent))),
                    }
                }
            }
            Err(_) => job.append(text, 0.0, format),
        }
        links
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

/// A picture ready to draw: its texture and its size in points (`None`: it could not be read).
type Shown = Option<(egui::TextureHandle, [usize; 2])>;

/// The pictures of the pages, as textures. A plot is a vector drawing (`.svgz`, gzip-compressed SVG, written by the
/// page tool), drawn in the window's theme at the display's pixels and at the size the drawing says; any other
/// picture (an image that is not a plot) is a PNG.
#[derive(Default)]
pub struct Images {
    /// by picture, theme and raster width: the texture and the picture's size in points
    loaded: HashMap<(String, u64, u32), Shown>,
    /// the natural size of each drawing seen (`None`: unreadable)
    sizes: HashMap<String, Option<[usize; 2]>>,
    raster: mc_notebook::images::SvgRaster,
}

impl Images {
    fn get(&mut self, ctx: &egui::Context, lib: &Library, rel: &str, p: &Palette) -> Option<(egui::TextureHandle, [usize; 2])> {
        if !rel.ends_with(".svgz") {
            return self.loaded.entry((rel.to_string(), 0, 0)).or_insert_with(|| png_texture(ctx, lib, rel)).clone();
        }
        let theme = p.plot_theme();
        let size = (*self.sizes.entry(rel.to_string()).or_insert_with(|| {
            let svg = crate::library::gunzip(lib.file(rel)?)?;
            let (w, h) = mc_nbfile::images::svg_size(&String::from_utf8_lossy(&svg))?;
            Some([w as usize, h as usize])
        }))?;
        // the drawing at the device pixels of its natural size
        let width = ((size[0] as f32 * ctx.pixels_per_point()).round() as u32).max(1);
        let key = (rel.to_string(), theme.key(), width);
        if let Some(hit) = self.loaded.get(&key) {
            return hit.clone();
        }
        // a theme switch or a move to another display: the textures of the old one go
        self.loaded.retain(|k, _| k.0 != rel);
        let made = lib
            .file(rel)
            .and_then(crate::library::gunzip)
            .and_then(|svg| self.raster.pixels(&svg, width, Some(&theme)).ok())
            .map(|(px, w, h)| {
                let image = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], &px);
                (ctx.load_texture(rel, image, egui::TextureOptions::LINEAR), size)
            });
        self.loaded.insert(key, made.clone());
        made
    }
}

/// The PNG `rel` of the library as a texture, with its size.
fn png_texture(ctx: &egui::Context, lib: &Library, rel: &str) -> Option<(egui::TextureHandle, [usize; 2])> {
    let image = decode_png(lib.file(rel)?)?;
    let size = image.size;
    Some((ctx.load_texture(rel, image, egui::TextureOptions::LINEAR), size))
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
    /// the topic guides
    pub guides: &'a [Guide],
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
    /// the links drawn this frame (screen rectangles and targets), for the self-test
    pub link_rects: Vec<(egui::Rect, String)>,
    /// the link under the pointer this frame
    pub hovered: Option<String>,
    /// an example input whose copy button was clicked (round 19)
    pub copy: Option<String>,
    /// the example input copied last, whose button shows a check mark for a moment
    pub copied: Option<String>,
    /// the copy buttons drawn this frame (screen rectangles and the inputs they copy), for the self-test
    pub copy_rects: Vec<(egui::Rect, String)>,
    /// values drawn as the notebook draws outputs
    pub outputs: &'a mut mc_notebook::outputs::OutputPainter,
    /// the part of the page a place points at (round 20), and whether to open its group and scroll to it
    pub target: Option<crate::page::Target>,
    pub reveal_target: bool,
    /// the typeset values drawn this frame (screen rectangles and their text), for the self-test
    pub value_rects: Vec<(egui::Rect, String)>,
    /// the typeset value selected (by its text), and a change of it this frame
    pub selected_value: Option<String>,
    pub select_value: Option<Option<String>>,
}

impl Draw<'_> {
    /// Show `job` as a selectable, wrapping label, with find matches marked and `links` live: underlined under the
    /// pointer with a hand cursor and the function's usage line as a tooltip; a click (also a middle click or
    /// Ctrl+click: the window has no tabs) opens the page.
    fn text(&mut self, ui: &mut egui::Ui, mut job: LayoutJob, links: Vec<Link>) -> egui::Response {
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
        let text = job.text.clone();
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let mut response = ui.add(egui::Label::new(galley.clone()).selectable(true));
        if !links.is_empty() {
            response = self.live_links(ui, response, &galley, &text, &links);
        }
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

    /// The links of a label: their rectangles on the screen, the one under the pointer underlined and opened by
    /// a click.
    fn live_links(
        &mut self,
        ui: &mut egui::Ui,
        response: egui::Response,
        galley: &egui::Galley,
        text: &str,
        links: &[Link],
    ) -> egui::Response {
        let origin = response.rect.min;
        let pointer = ui.ctx().pointer_hover_pos().filter(|_| response.hovered());
        let mut under: Option<(egui::Rect, String, Color32)> = None;
        for (range, name) in links {
            let (a, b) = (text[..range.start].chars().count(), text[..range.end].chars().count());
            let mut k = 0;
            for row in &galley.rows {
                let mut rect = egui::Rect::NOTHING;
                let mut colour = self.p.accent;
                for (j, g) in row.glyphs.iter().enumerate() {
                    if (a..b).contains(&(k + j)) {
                        let x = origin.x + row.pos.x + g.pos.x;
                        rect = rect.union(egui::Rect::from_min_max(
                            pos2(x, origin.y + row.min_y()),
                            pos2(x + g.advance_width, origin.y + row.max_y()),
                        ));
                        colour = galley.job.sections.iter().find(|s| s.byte_range.contains(&ByteIndex(range.start))).map_or(colour, |s| s.format.color);
                    }
                }
                k += row.glyphs.len() + usize::from(row.ends_with_newline);
                if rect.is_positive() {
                    self.link_rects.push((rect, name.clone()));
                    if pointer.is_some_and(|p| rect.contains(p)) {
                        under = Some((rect, name.clone(), colour));
                    }
                }
            }
        }
        let Some((rect, name, colour)) = under else { return response };
        let y = rect.bottom() - 1.5;
        ui.painter().line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, colour));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        self.hovered = Some(name.clone());
        // a click opens the page; Ctrl+click (a click with Ctrl) and a middle click too: the window has no tabs
        if response.clicked() || response.middle_clicked() {
            self.go = Some(Loc::Page(name.clone()));
        }
        match mc_help::Index::embedded().get(&name) {
            Some(e) => response.on_hover_text_at_pointer(e.line()),
            None => response,
        }
    }

    fn prose(&mut self, ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
        let mut job = LayoutJob::default();
        let links = self.colours.append_prose(&mut job, text, self.p, size, color);
        self.text(ui, job, links);
    }

    fn heading(&mut self, ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
        let mut job = LayoutJob::default();
        job.append(text, 0.0, TextFormat { font_id: Face::ProseBold.font(size), color, ..TextFormat::default() });
        self.text(ui, job, vec![]);
    }

    /// A function name as a link (accent when it has a page).
    fn name_link(&mut self, ui: &mut egui::Ui, name: &str, size: f32) {
        let has_page = self.pages.contains_key(name);
        let color = if has_page { self.p.accent } else { self.p.text };
        let r = ui.add(egui::Link::new(RichText::new(name).font(Face::Code.font(size)).color(color)));
        if r.clicked() {
            self.go = Some(Loc::Page(name.to_string()));
        }
        let rect = r.rect;
        if let Some(e) = mc_help::Index::embedded().get(name) {
            r.on_hover_text(e.line());
        }
        self.link_rects.push((rect, name.to_string()));
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
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let r = ui.link(
                RichText::new(format!("Index of all {} functions", self.cat.entries.len())).size(14.0).color(self.p.blue),
            );
            ui.add(mc_ui::icons::image(ui.ctx(), mc_ui::icons::Icon::ArrowRight, 13.0, self.p.blue));
            if r.clicked() {
                self.go = Some(Loc::Index);
            }
        });
        ui.add_space(8.0);
        self.guide_list(ui);
        // every area as a heading over a uniform grid of topic cards: the same width and height in every row
        let gap = 12.0;
        let width = ui.available_width();
        let columns = ((width + gap) / (260.0 + gap)).floor().clamp(1.0, 4.0) as usize;
        let card_w = ((width - gap * (columns as f32 - 1.0)) / columns as f32).floor();
        let card_h = 150.0;
        for area in Area::ALL {
            ui.add_space(14.0);
            let title =
                ui.add(egui::Link::new(RichText::new(area.title()).font(Face::ProseBold.font(20.0)).color(self.p.text)));
            if title.clicked() {
                self.go = Some(Loc::Guide(area));
            }
            self.prose(ui, area.blurb(), 13.5, self.p.weak);
            ui.add_space(8.0);
            let topics = self.cat.topics(area);
            for row in topics.chunks(columns) {
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (topic, entries) in row {
                        let size = egui::vec2(card_w, card_h);
                        ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
                            ui.set_min_size(size);
                            ui.set_max_size(size);
                            self.topic_card(ui, area, topic, entries, size);
                        });
                    }
                });
                ui.add_space(gap);
            }
        }
    }

    /// One card of the home grid: the topic, a few of its functions (those with pages first) and a link to all of
    /// them; always `size`, whatever the topic holds.
    fn topic_card(&mut self, ui: &mut egui::Ui, area: Area, topic: &str, entries: &[&Entry], size: egui::Vec2) {
        let p = self.p;
        let margin = 14.0;
        let inner = size - egui::vec2(2.0 * margin, 2.0 * margin);
        let mut names: Vec<&str> = entries.iter().map(|e| e.name).collect();
        names.sort_by_key(|n| (!self.pages.contains_key(*n), *n));
        // as many names as fit in three lines of the card (monospaced links, about 0.6 em per character)
        let per_line = (inner.x / (13.5 * 0.6)).floor().max(8.0) as usize;
        let (mut line, mut lines, mut shown) = (0usize, 1usize, Vec::new());
        for n in &names {
            let w = n.chars().count() + 1;
            if line + w > per_line {
                lines += 1;
                line = 0;
            }
            if lines > 3 {
                break;
            }
            line += w;
            shown.push(*n);
        }
        Frame::new()
            .fill(p.card)
            .stroke(Stroke::new(1.0, if p.dark { p.rule() } else { p.border }))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(margin as i8))
            .show(ui, |ui| {
                ui.set_min_size(inner);
                ui.set_max_size(inner);
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    let t = ui.add(egui::Link::new(RichText::new(topic).font(Face::ProseBold.font(16.0)).color(p.blue)));
                    if t.clicked() {
                        self.go = Some(Loc::Guide(area));
                    }
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for n in &shown {
                            self.name_link(ui, n, 13.5);
                        }
                    });
                    // the count sits on the card's last line
                    let rest = ui.available_height() - 18.0;
                    if rest > 0.0 {
                        ui.add_space(rest);
                    }
                    let all = ui.horizontal(|ui| {
                        let more = names.len() - shown.len();
                        let label = if more > 0 { format!("{} functions ({more} more)", names.len()) } else { format!("{} functions", names.len()) };
                        let r = ui.link(RichText::new(label).size(12.5).color(p.weak));
                        ui.add(mc_ui::icons::image(ui.ctx(), mc_ui::icons::Icon::ArrowRight, 12.0, p.weak));
                        r.clicked()
                    });
                    if all.inner {
                        self.go = Some(Loc::Guide(area));
                    }
                });
            });
    }

    // ----- topic guides -----

    /// The topic guides on the home page: a heading, then one row each (title as a link, what it covers).
    fn guide_list(&mut self, ui: &mut egui::Ui) {
        if self.guides.is_empty() {
            return;
        }
        ui.add_space(10.0);
        self.heading(ui, "Guides", 20.0, self.p.text);
        self.prose(
            ui,
            "Short tours of a topic with worked examples, common mistakes and the functions to reach for.",
            13.5,
            self.p.weak,
        );
        ui.add_space(6.0);
        let guides = self.guides;
        for g in guides {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(250.0, 18.0), Layout::left_to_right(Align::Min), |ui| {
                    ui.set_width(250.0);
                    let r = ui.add(egui::Link::new(RichText::new(&g.title).font(Face::ProseBold.font(15.0)).color(self.p.blue)));
                    if r.clicked() {
                        self.go = Some(Loc::Topic(g.slug.clone()));
                    }
                    self.link_rects.push((r.rect, format!("topic:{}", g.slug)));
                });
                ui.vertical(|ui| self.prose(ui, g.summary(), 13.5, self.p.weak));
            });
            ui.add_space(3.0);
        }
    }

    /// A topic guide: title, prose, bullets, and examples with the engine's outputs.
    pub fn topic(&mut self, ui: &mut egui::Ui, slug: &str) {
        ui.add_space(14.0);
        let Some(guide) = self.guides.iter().find(|g| g.slug == slug) else {
            self.heading(ui, "Guide", 26.0, self.p.text);
            self.prose(ui, &format!("There is no guide `{slug}`."), BODY, self.p.weak);
            return;
        };
        let folder = format!("guides/{}", guide.stem);
        ui.horizontal(|ui| {
            self.heading(ui, &guide.title, 26.0, self.p.text);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let open = egui::Button::image_and_text(
                    mc_ui::icons::image(ui.ctx(), mc_ui::icons::Icon::Notebook, 15.0, ui.visuals().text_color()),
                    "Open as notebook",
                );
                if ui.add(open).on_hover_text("A copy of this guide in the notebook, to run and change").clicked() {
                    self.open_notebook = true;
                }
                ui.label(RichText::new("Guide").size(12.0).color(self.p.faint));
            });
        });
        ui.add_space(6.0);
        for part in &guide.parts {
            match part {
                Part::Heading(1, t) => {
                    ui.add_space(16.0);
                    self.heading(ui, t, 20.0, self.p.blue);
                    ui.add_space(2.0);
                }
                Part::Heading(_, t) => {
                    ui.add_space(10.0);
                    self.heading(ui, t, 16.0, self.p.text);
                }
                Part::Text(t) => {
                    ui.add_space(5.0);
                    self.prose(ui, t, BODY, self.p.text);
                }
                Part::Item(level, t) => {
                    ui.horizontal_top(|ui| {
                        ui.add_space(f32::from(*level) * 22.0 + 4.0);
                        let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, BODY * 1.35), egui::Sense::hover());
                        let dot = egui::Rect::from_center_size(r.center(), egui::vec2(8.0, 8.0));
                        mc_ui::icons::paint(ui.painter(), mc_ui::icons::Icon::Dot, dot, self.p.blue);
                        ui.vertical(|ui| self.prose(ui, t, BODY, self.p.text));
                    });
                    ui.add_space(2.0);
                }
                Part::Input { source, output } => {
                    ui.add_space(5.0);
                    self.example_input(ui, source);
                    if !output.is_empty() {
                        self.outputs(ui, &folder, output);
                    }
                }
            }
        }
        if !guide.related.is_empty() {
            // the section's own text is not repeated: its names are links under its heading
            ui.horizontal_wrapped(|ui| {
                for n in &guide.related {
                    self.name_link(ui, n, CODE);
                    ui.add_space(6.0);
                }
            });
        }
        ui.add_space(30.0);
    }

    // ----- the index -----

    /// Every function of the system, by area and topic, as links (accent: with a page).
    pub fn index(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        self.heading(ui, "Index", 26.0, self.p.text);
        let documented = self.cat.entries.iter().filter(|e| self.pages.contains_key(e.name)).count();
        self.prose(
            ui,
            &format!(
                "All {} functions by area and topic; the {documented} in the accent colour have a page with examples. \
                 Hover a name for its usage line.",
                self.cat.entries.len()
            ),
            BODY,
            self.p.weak,
        );
        let cat = self.cat;
        for area in Area::ALL {
            ui.add_space(14.0);
            let r = ui.add(egui::Link::new(RichText::new(area.title()).font(Face::ProseBold.font(20.0)).color(self.p.blue)));
            if r.clicked() {
                self.go = Some(Loc::Guide(area));
            }
            for (topic, entries) in cat.topics(area) {
                ui.add_space(4.0);
                ui.label(RichText::new(topic).font(Face::ProseBold.font(14.5)).color(self.p.text));
                ui.horizontal_wrapped(|ui| {
                    for e in entries {
                        self.name_link(ui, e.name, 13.5);
                        ui.add_space(6.0);
                    }
                });
            }
        }
        ui.add_space(24.0);
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
    pub fn search(
        &mut self,
        ui: &mut egui::Ui,
        query: &str,
        guide_hits: &[usize],
        results: &[(String, crate::search::Rank)],
    ) {
        ui.add_space(14.0);
        self.heading(ui, &format!("Search: {query}"), 22.0, self.p.text);
        let guides = self.guides;
        if !guide_hits.is_empty() {
            ui.label(RichText::new("Guides").font(Face::ProseBold.font(14.0)).color(self.p.weak));
            ui.add_space(2.0);
            for (k, &gi) in guide_hits.iter().enumerate() {
                let g = &guides[gi];
                let fill = if k == self.selected { self.p.over_bg(self.p.accent, 0.12) } else { Color32::TRANSPARENT };
                let r = Frame::new()
                    .fill(fill)
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(vec2(250.0, 18.0), Layout::left_to_right(Align::Min), |ui| {
                                ui.set_width(250.0);
                                let r = ui.add(egui::Link::new(
                                    RichText::new(&g.title).font(Face::ProseBold.font(14.0)).color(self.p.blue),
                                ));
                                if r.clicked() {
                                    self.go = Some(Loc::Topic(g.slug.clone()));
                                }
                                self.link_rects.push((r.rect, format!("topic:{}", g.slug)));
                            });
                            ui.vertical(|ui| self.prose(ui, g.summary(), 13.5, self.p.weak));
                        });
                    });
                if k == self.selected {
                    r.response.scroll_to_me(None);
                }
            }
            ui.add_space(8.0);
            if !results.is_empty() {
                ui.label(RichText::new("Functions").font(Face::ProseBold.font(14.0)).color(self.p.weak));
                ui.add_space(2.0);
            }
        }
        let offset = guide_hits.len();
        if results.is_empty() {
            if !guide_hits.is_empty() {
                return;
            }
            self.prose(
                ui,
                "No function or guide matches. Try a shorter part of the name, or a word from what it does.",
                BODY,
                self.p.weak,
            );
            return;
        }
        let exact = results.iter().take_while(|(_, r)| !r.is_near()).count();
        let counted = match (exact, results.len() - exact) {
            (0, near) => format!("No exact match; {near} similar names and descriptions. Enter opens the highlighted one."),
            (n, 0) => format!("{n} functions. Enter opens the highlighted one."),
            (n, near) => format!("{n} functions, then {near} similar. Enter opens the highlighted one."),
        };
        self.prose(ui, &counted, 13.0, self.p.faint);
        ui.add_space(8.0);
        for (k, (name, rank)) in results.iter().enumerate() {
            if k == exact && rank.is_near() {
                // near misses (round 19): under their own heading, after every exact match
                ui.add_space(if k == 0 { 2.0 } else { 10.0 });
                ui.label(RichText::new("Similar").font(Face::ProseBold.font(14.0)).color(self.p.weak));
                ui.add_space(2.0);
            }
            let fill = if k + offset == self.selected { self.p.over_bg(self.p.accent, 0.12) } else { Color32::TRANSPARENT };
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
            if k + offset == self.selected {
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
        let target = self.target;
        let reveal = self.reveal_target;
        let to_detail = matches!(target, Some(Target::Detail(_))) && reveal;
        let to_group = |k: usize| reveal && matches!(target, Some(Target::Group(g) | Target::Input(g, _)) if g == k);
        egui::CollapsingHeader::new(RichText::new("Details").font(Face::ProseBold.font(18.0)).color(self.p.blue))
            .icon(mc_ui::icons::collapsing_marker)
            .id_salt(("details", name))
            .default_open(false)
            .open(if to_detail { Some(true) } else { open })
            .show(ui, |ui| {
                for (i, item) in page.details.iter().enumerate() {
                    let top = ui.cursor().top();
                    ui.horizontal_top(|ui| {
                        ui.add_space(f32::from(item.level) * 22.0);
                        // the bullet: the icon set's dot, on the first line's middle
                        let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, BODY * 1.35), egui::Sense::hover());
                        let dot = egui::Rect::from_center_size(r.center(), egui::vec2(8.0, 8.0));
                        mc_ui::icons::paint(ui.painter(), mc_ui::icons::Icon::Dot, dot, self.p.blue);
                        ui.vertical(|ui| self.prose(ui, &item.text, BODY, self.p.text));
                    });
                    if target == Some(Target::Detail(i)) {
                        let r = egui::Rect::from_min_max(pos2(ui.min_rect().left(), top), pos2(ui.min_rect().right(), ui.cursor().top()));
                        self.mark_target(ui, r);
                    }
                    ui.add_space(3.0);
                }
            });
        ui.add_space(6.0);
        egui::CollapsingHeader::new(RichText::new("Examples").font(Face::ProseBold.font(18.0)).color(self.p.blue))
            .icon(mc_ui::icons::collapsing_marker)
            .id_salt(("examples", name))
            .default_open(true)
            .open(if target.is_some() && reveal && !to_detail { Some(true) } else { open })
            .show(ui, |ui| {
                for (k, group) in page.examples.iter().enumerate() {
                    let title = format!("{}  ({})", group.title, group.inputs());
                    let header = egui::CollapsingHeader::new(
                        RichText::new(title).font(Face::ProseBold.font(15.5)).color(self.p.text),
                    )
                    .icon(mc_ui::icons::collapsing_marker)
                    .id_salt(("group", name, k))
                    .default_open(k == 0)
                    .open(if to_group(k) { Some(true) } else { open })
                    .show(ui, |ui| self.group(ui, name, k, &group.blocks));
                    if target == Some(Target::Group(k)) && reveal {
                        ui.scroll_to_rect(header.header_response.rect, Some(Align::TOP));
                    }
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
                    let links = self.colours.append_code(&mut job, &u.call, self.p, 15.0, true, None);
                    self.text(ui, job, links);
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
    fn group(&mut self, ui: &mut egui::Ui, name: &str, k: usize, blocks: &[Block]) {
        for (b, block) in blocks.iter().enumerate() {
            match block {
                Block::Text(t) => {
                    ui.add_space(6.0);
                    self.prose(ui, t, BODY, self.p.text);
                }
                Block::Input { source, output, .. } => {
                    ui.add_space(4.0);
                    let top = ui.cursor().top();
                    self.example_input(ui, source);
                    if !output.is_empty() {
                        self.outputs(ui, name, output);
                    }
                    if self.target == Some(Target::Input(k, b)) {
                        let r = egui::Rect::from_min_max(pos2(ui.min_rect().left(), top), pos2(ui.min_rect().right(), ui.cursor().top()));
                        self.mark_target(ui, r);
                    }
                }
            }
        }
        ui.add_space(8.0);
    }

    /// The part a place pointed at: a bar in the accent colour at its left, scrolled into view while revealing.
    fn mark_target(&mut self, ui: &mut egui::Ui, r: egui::Rect) {
        let bar = egui::Rect::from_min_max(pos2(r.left() - 10.0, r.top()), pos2(r.left() - 7.0, r.bottom()));
        ui.painter().rect_filled(bar, 1.5, self.p.accent);
        if self.reveal_target {
            ui.scroll_to_rect(r, Some(Align::Center));
        }
    }

    /// An example input with a copy button at the right of its first line (round 19): the input goes onto the
    /// clipboard as text, which pastes into a notebook cell as it reads here (at an insertion point it makes an
    /// Input cell). The button is faint until the pointer is over the input, and shows a check mark once it copied.
    fn example_input(&mut self, ui: &mut egui::Ui, source: &str) {
        const BUTTON: f32 = 22.0;
        let width = ui.available_width();
        let row = ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let text = ui
                .allocate_ui(vec2(width - BUTTON - 8.0, 0.0), |ui| {
                    let mut job = LayoutJob::default();
                    let links = self.colours.append_code(&mut job, source, self.p, CODE, true, None);
                    self.text(ui, job, links)
                })
                .inner;
            text.rect
        });
        let text = row.inner;
        let hot = ui.rect_contains_pointer(row.response.rect.expand2(vec2(BUTTON + 8.0, 2.0)));
        let done = self.copied.as_deref() == Some(source);
        let rect = egui::Rect::from_min_size(pos2(ui.max_rect().right() - BUTTON, text.top()), vec2(BUTTON, BUTTON));
        let tint = if done {
            self.p.green
        } else if hot {
            self.p.text
        } else {
            self.p.faint
        };
        let icon = if done { mc_ui::icons::Icon::Check } else { mc_ui::icons::Icon::Copy };
        let b = egui::Button::image(mc_ui::icons::image(ui.ctx(), icon, 15.0, tint)).frame(hot).min_size(vec2(BUTTON, BUTTON));
        let tip = if done { "Copied: paste it into a notebook" } else { "Copy this input" };
        if ui.put(rect, b).on_hover_text(tip).clicked() {
            self.copy = Some(source.to_string());
        }
        self.copy_rects.push((rect, source.to_string()));
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
                // values with roots, fractions or powers are drawn as the notebook draws outputs (round 19), the
                // rest as text with links
                if kind == OutputKind::Value && run.iter().any(|l| self.outputs.typesets(&l.text)) {
                    for l in run {
                        if self.outputs.typesets(&l.text) {
                            self.typeset_value(ui, &l.text);
                        } else {
                            let mut job = LayoutJob::default();
                            let links = self.colours.append_output(&mut job, &l.text, CODE, self.p.text);
                            self.text(ui, job, links);
                        }
                    }
                    continue;
                }
                let text = run.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
                let color = match kind {
                    OutputKind::Value | OutputKind::Print => self.p.text,
                    OutputKind::Message => self.p.dark_red,
                    OutputKind::Note => self.p.orange,
                    // embedded files are not shown
                    OutputKind::Data => continue,
                };
                let mut job = LayoutJob::default();
                let links = self.colours.append_output(&mut job, &text, CODE, color);
                self.text(ui, job, links);
            }
        });
        let rect = r.response.rect;
        let x = rect.left() + 3.0;
        ui.painter().line_segment([pos2(x, rect.top() + 3.0), pos2(x, rect.bottom() - 1.0)], Stroke::new(2.0, rule));
    }

    /// A value drawn typeset as in the notebook; find-in-page counts its matches in the printed text and frames it
    /// when it holds the current one.
    fn typeset_value(&mut self, ui: &mut egui::Ui, printed: &str) {
        let r = self.outputs.show(ui, printed, CODE, self.p.text, self.p);
        let mut current_here = false;
        if let Some((q, current, _)) = &self.find {
            if !q.is_empty() {
                let n = printed.to_ascii_lowercase().matches(q.as_str()).count();
                current_here = (self.count..self.count + n).contains(current);
                if n > 0 {
                    // the whole value marked, as find marks text (the current match stronger)
                    let (c, alpha) = if current_here { (self.p.orange, 110) } else { (self.p.yellow, 60) };
                    let fill = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha);
                    ui.painter().rect_filled(r.rect.expand(2.0), 3.0, fill);
                }
                self.count += n;
            }
        }
        if current_here {
            if let Some((_, _, reveal)) = &mut self.find {
                if *reveal {
                    ui.scroll_to_rect(r.rect, Some(Align::Center));
                    *reveal = false;
                }
            }
        }
        // selectable and copyable (round 20): a click selects the value (Ctrl+C or the context menu copies its
        // text, which reads back as the value), its copy button copies it too
        let id = ui.scope_id().with(("typeset value", self.value_rects.len()));
        let r = ui.interact(r.rect, id, Sense::click());
        self.value_rects.push((r.rect, printed.to_string()));
        let selected = self.selected_value.as_deref() == Some(printed);
        if selected {
            let sel = ui.visuals().selection.bg_fill;
            let fill = Color32::from_rgba_unmultiplied(sel.r(), sel.g(), sel.b(), 90);
            ui.painter().rect_filled(r.rect.expand(2.0), 3.0, fill);
            if ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy))) {
                self.copy = Some(printed.to_string());
            }
        }
        if r.clicked() {
            self.select_value = Some(Some(printed.to_string()));
        } else if selected && ui.input(|i| i.pointer.any_click()) && !r.hovered() {
            self.select_value = Some(None);
        }
        if r.hovered() || selected {
            const BUTTON: f32 = 20.0;
            let done = self.copied.as_deref() == Some(printed);
            let at = egui::Rect::from_min_size(pos2(r.rect.right() + 6.0, r.rect.top()), vec2(BUTTON, BUTTON));
            let icon = if done { mc_ui::icons::Icon::Check } else { mc_ui::icons::Icon::Copy };
            let tint = if done { self.p.green } else { self.p.text };
            let b = egui::Button::image(mc_ui::icons::image(ui.ctx(), icon, 14.0, tint)).frame(false).min_size(vec2(BUTTON, BUTTON));
            if ui.put(at, b).on_hover_text("Copy this value").clicked() {
                self.copy = Some(printed.to_string());
            }
        }
        let copy = &mut self.copy;
        r.on_hover_text(printed).context_menu(|ui| {
            if ui.button("Copy").clicked() {
                *copy = Some(printed.to_string());
                ui.close();
            }
        });
    }

    fn image(&mut self, ui: &mut egui::Ui, page: &str, rel: &str) {
        // a guide's images are in its folder, `guides/<stem>.files/`
        let rel = if page.starts_with("guides/") { format!("guides/{rel}") } else { rel.to_string() };
        match self.images.get(ui.ctx(), self.lib, &rel, self.p) {
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
