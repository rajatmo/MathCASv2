//! Painting typeset content (design 4.7, 4.10, 9.6; decisions NB5.4, NBUI.2): box layouts from `mc_boxes` drawn in
//! the cell's own text font. Letters, digits, operators and every other character the cell's font (its family's
//! chain before the math font) has are measured with that font's own glyph extents and drawn by egui exactly as the
//! cell's plain text is, in runs, so typing a structure key changes nothing about how the other characters look.
//! The bundled math font draws only what the text font lacks: stretched delimiters, radical signs, big operators,
//! placeholders and symbols the text font has no glyph for.
//!
//! Glyphs are rasterised at the device pixel size (with four horizontal sub-pixel phases) into one texture atlas of
//! our own, uploaded piecewise when a glyph is first drawn and never per frame. The atlas starts at 1024 x 1024
//! (4 MB) and grows once to 2048 x 2048 (16 MB); when that is full it is cleared at the start of the next frame
//! and refilled with what is drawn (so magnification steps and theme changes cannot make it grow without bound,
//! the round 9 lesson of egui's own atlas). Each frame the visible glyphs of a layout become one mesh.
use crate::theme::Palette;
use eframe::egui::{self, pos2, vec2, Color32, ColorImage, Mesh, Painter, Pos2, Rect, TextureHandle, TextureOptions};
use eframe::epaint::text::{LayoutJob, TextFormat};
use mc_boxes::layout::{metrics::math_char, CharMetrics, Draw, FontRole, Layout, LayoutOptions, Metrics};
use mc_boxes::Row;
use mc_mathfont::{font, rasterize_shifted, Glyph};
use mc_nbmodel::CellId;
use mc_ui::fonts::{Face, FaceFonts, FontSet};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

/// Sub-pixel phases of glyph origins.
const PHASES: f32 = 4.0;
/// The atlas sides: the first and the largest.
const ATLAS_START: usize = 1024;
const ATLAS_MAX: usize = 2048;
/// A glyph bitmap side above this is rasterised smaller and drawn scaled (huge stretched delimiters).
const GLYPH_MAX: usize = 512;

/// Where a glyph is in the atlas.
#[derive(Clone, Copy, Debug)]
struct Slot {
    /// top-left in atlas pixels
    x: usize,
    y: usize,
    /// size in pixels (0 for glyphs without ink)
    w: usize,
    h: usize,
    /// offset of the bitmap from the glyph origin's pixel: left, and up to the top
    left: i32,
    top: i32,
    /// the bitmap was drawn smaller than asked: draw it scaled by this
    scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    glyph: Glyph,
    /// pixels per em in quarter pixels
    px: u32,
    phase: u8,
    dark: bool,
}

/// The atlas of math-font glyphs.
pub struct Glyphs {
    ctx: egui::Context,
    tex: Option<TextureHandle>,
    side: usize,
    slots: HashMap<Key, Slot>,
    /// shelf packing: (top, height, next x) of each shelf
    shelves: Vec<(usize, usize, usize)>,
    next_y: usize,
    /// full this frame: clear before the next
    full: bool,
    /// the window's text fonts (typeset cells measure their characters with them) and how often they changed
    fonts: Arc<FontSet>,
    font_gen: u32,
}

impl Glyphs {
    /// An empty atlas (the texture is made when the first glyph is drawn).
    pub fn new(ctx: &egui::Context) -> Glyphs {
        Glyphs {
            ctx: ctx.clone(),
            tex: None,
            side: ATLAS_START,
            slots: HashMap::new(),
            shelves: vec![],
            next_y: 0,
            full: false,
            fonts: Arc::new(FontSet::new(&egui::FontDefinitions::empty())),
            font_gen: 0,
        }
    }

    /// The window's fonts changed (typeset layouts made with the old ones are stale).
    pub fn set_fonts(&mut self, fonts: FontSet) {
        self.fonts = Arc::new(fonts);
        self.font_gen = self.font_gen.wrapping_add(1);
    }

    /// The text fonts of `face`, and the generation of the window's fonts (part of every typeset layout's key).
    pub fn face(&self, face: Face) -> (Arc<FaceFonts>, u32) {
        (self.fonts.face(face), self.font_gen)
    }

    /// At the start of a frame: an atlas that filled up last frame starts again (larger once).
    pub fn begin_frame(&mut self) {
        if self.full {
            self.full = false;
            self.side = (self.side * 2).min(ATLAS_MAX);
            self.tex = None;
            self.slots.clear();
            self.shelves.clear();
            self.next_y = 0;
            self.ctx.request_repaint();
        }
    }

    /// The bytes of pixels the atlas holds (for the self-test's report).
    pub fn bytes(&self) -> usize {
        if self.tex.is_some() {
            self.side * self.side * 4
        } else {
            0
        }
    }

    fn texture(&mut self) -> &mut TextureHandle {
        let side = self.side;
        self.tex.get_or_insert_with(|| {
            let mut img = ColorImage::new([side, side], vec![Color32::TRANSPARENT; side * side]);
            // a white block at the corner for rules drawn in the same mesh
            for y in 0..4 {
                for x in 0..4 {
                    img.pixels[y * side + x] = Color32::WHITE;
                }
            }
            self.ctx.load_texture("mc-math-glyphs", img, TextureOptions::LINEAR)
        })
    }

    /// A place for a `w` x `h` bitmap (with a pixel of padding), shelf by shelf.
    fn place(&mut self, w: usize, h: usize) -> Option<(usize, usize)> {
        let (w, h) = (w + 1, h + 1);
        for s in &mut self.shelves {
            if h <= s.1 && s.1 <= h + h / 3 + 2 && s.2 + w <= self.side {
                let x = s.2;
                s.2 += w;
                return Some((x, s.0));
            }
        }
        // the first shelf starts below the white block
        let top = self.next_y.max(if self.shelves.is_empty() { 5 } else { 0 });
        if top + h > self.side || w > self.side {
            return None;
        }
        self.shelves.push((top, h, w));
        self.next_y = top + h;
        Some((0, top))
    }

    /// The slot of a glyph at `px` pixels per em and sub-pixel phase, rasterised and uploaded on first use.
    fn slot(&mut self, glyph: Glyph, px: f32, phase: u8, dark: bool) -> Option<Slot> {
        let key = Key { glyph, px: (px * 4.0).round() as u32, phase, dark };
        if let Some(s) = self.slots.get(&key) {
            return Some(*s);
        }
        if self.full {
            return None;
        }
        let f = font();
        let m = f.metrics(glyph);
        let tall = (m.height + m.depth).max(m.x_max - m.x_min) * px;
        let scale = if tall > GLYPH_MAX as f32 { tall / GLYPH_MAX as f32 } else { 1.0 };
        let bm = rasterize_shifted(f, glyph, px / scale, f32::from(phase) / PHASES);
        let mut slot = Slot { x: 0, y: 0, w: bm.width, h: bm.height, left: bm.left, top: bm.top, scale };
        if bm.width > 0 && bm.height > 0 {
            let Some((x, y)) = self.place(bm.width, bm.height) else {
                self.full = true;
                return None;
            };
            slot.x = x;
            slot.y = y;
            let gamma = if dark {
                egui::epaint::FontColorTransferFunction::DARK_MODE_DEFAULT
            } else {
                egui::epaint::FontColorTransferFunction::LIGHT_MODE_DEFAULT
            };
            let pixels = bm.alpha.iter().map(|&a| gamma.color_from_coverage(f32::from(a) / 255.0)).collect();
            let img = ColorImage::new([bm.width, bm.height], pixels);
            self.texture().set_partial([x, y], img, TextureOptions::LINEAR);
        }
        self.slots.insert(key, slot);
        Some(slot)
    }
}

/// The cell's text font as typeset content uses it: the face, its fonts' glyph extents, and egui's line metrics of
/// the face (so a typeset line is exactly as tall as a plain one, with its baseline at the same place).
#[derive(Clone, Debug)]
pub struct TextFont {
    /// the fonts of the cell's face before the math font
    pub fonts: Arc<FaceFonts>,
    /// egui's baseline in a row of the face, from the row's top, in ems
    pub ascent: f32,
    /// egui's row height of the face in ems
    pub row: f32,
}

impl TextFont {
    /// The text font of `face` measured by egui at `size` points (the cell's size: egui rounds its row metrics to
    /// pixels, so they are taken at the size they are drawn at).
    pub fn new(ctx: &egui::Context, face: Face, fonts: Arc<FaceFonts>, size: f32) -> TextFont {
        let size = size.max(1.0);
        let (ascent, row) = ctx.fonts_mut(|f| {
            let g = f.layout_no_wrap("x".to_string(), face.font(size), Color32::WHITE);
            let base = g.rows.first().and_then(|r| r.row.glyphs.first()).map_or(0.8 * size, |gl| gl.pos.y);
            (base, g.size().y.max(base))
        });
        TextFont { fonts, ascent: ascent / size, row: row / size }
    }
}

/// Metrics for the layout (NBUI.2): the cell's text font for every character it has, its row metrics for the lines
/// and the axis of its minus sign; the math font for the rest and for the MATH constants a text font has not got.
/// Every character of the row is measured once up front (a font lookup per character drawn made the layout of a
/// 25,000-character output twice as slow).
pub struct CellMetrics {
    font: TextFont,
    axis: Option<f32>,
    chars: HashMap<char, (FontRole, CharMetrics)>,
}

impl CellMetrics {
    /// Metrics for laying out `row` in `font`.
    pub fn for_row(row: &Row, font: &TextFont) -> CellMetrics {
        let mut chars = HashMap::new();
        let mut stack = vec![row];
        // ASCII seen already: most characters of a long cell, skipped without a hash lookup
        let mut ascii = [false; 128];
        while let Some(r) = stack.pop() {
            for it in &r.0 {
                match it {
                    mc_boxes::Item::Char(c) if c.is_ascii() && std::mem::replace(&mut ascii[*c as usize], true) => {}
                    mc_boxes::Item::Char(c) if !chars.contains_key(c) => {
                        chars.insert(*c, measure(&font.fonts, *c));
                    }
                    mc_boxes::Item::Box(b) => stack.extend(b.slots()),
                    _ => {}
                }
            }
        }
        // the placeholder is the math font's
        chars.insert('□', (FontRole::Math, math_char('□')));
        CellMetrics { axis: font.fonts.axis(), font: font.clone(), chars }
    }

    fn get(&self, c: char) -> (FontRole, CharMetrics) {
        match self.chars.get(&c) {
            Some(m) => *m,
            // a symbol the layout adds itself (`∂`, `d`, `lim`, `→`, `=`): measured now
            None => measure(&self.font.fonts, c),
        }
    }
}

/// The font and extent of `c`: the text font's when it has a glyph, else the math font's; a character neither has
/// is left to egui's fallbacks at an average advance.
fn measure(fonts: &FaceFonts, c: char) -> (FontRole, CharMetrics) {
    if let Some(b) = fonts.glyph(c) {
        return (FontRole::Text, CharMetrics { advance: b.advance, ascent: b.ascent, descent: b.descent });
    }
    if font().glyph(c).is_some() {
        return (FontRole::Math, math_char(c));
    }
    (FontRole::Text, CharMetrics { advance: 0.6, ascent: 0.7, descent: 0.0 })
}

impl Metrics for CellMetrics {
    fn char(&self, c: char, role: FontRole) -> CharMetrics {
        match (role, self.get(c)) {
            (FontRole::Math, (FontRole::Text, _)) => math_char(c),
            (_, (_, m)) => m,
        }
    }

    fn line(&self, _role: FontRole) -> (f32, f32) {
        // a typeset line of plain characters is a row of the cell's plain text: same height, same baseline
        (self.font.ascent, (self.font.row - self.font.ascent).max(0.0))
    }

    fn role(&self, c: char) -> FontRole {
        self.get(c).0
    }

    fn axis_height(&self) -> Option<f32> {
        self.axis
    }

    fn line_pitch(&self) -> Option<f32> {
        Some(self.font.row)
    }

    fn italic_letters(&self) -> bool {
        // no math italics in cells: every letter stays in the cell's font
        false
    }
}

/// Lay out `row` in the cell's text font.
pub fn lay_out(row: &Row, opts: &LayoutOptions, font: &TextFont) -> Layout {
    let m = CellMetrics::for_row(row, font);
    mc_boxes::layout::layout(row, &m, opts)
}

/// Lay out `row` in the cell's text font line by line, the lines an edit did not change taken from `cache` (code
/// cells: typing into a long cell lays out one line, design 11, section 5), the layout shared with the cache: let go
/// of it before the next call and that call changes it in place where the row changed (round 37).
pub fn lay_out_lines(row: &Row, opts: &LayoutOptions, font: &TextFont, cache: &mut mc_boxes::layout::LineCache) -> Arc<Layout> {
    let m = CellMetrics::for_row(row, font);
    mc_boxes::layout::layout_by_lines_shared(row, &m, opts, cache)
}

/// Let the first line's superscripts reach up to `above` (and the last line's subscripts down to `below`, layout
/// units) into the cell's margins instead of growing the cell: the first baseline stays where the cell's plain text
/// has it, so a superscript typed into a line moves nothing else (NBUI.3).
pub fn keep_baseline(lay: &mut Layout, font: &TextFont, size: f32, above: f32, below: f32) {
    let (over, under) = baseline_reach(lay, font, size, above, below);
    if over > 0.0 {
        lay.translate(0.0, -over);
        lay.height -= over;
    }
    lay.height -= under;
}

/// [`keep_baseline`] on a shared layout: it is copied only when it changes (a cell whose first line reaches up or
/// whose last line reaches down), so a long cell's layout stays the one its line cache changes in place.
pub fn keep_baseline_shared(lay: &mut Arc<Layout>, font: &TextFont, size: f32, above: f32, below: f32) {
    let (over, under) = baseline_reach(lay, font, size, above, below);
    if over > 0.0 || under > 0.0 {
        keep_baseline(Arc::make_mut(lay), font, size, above, below);
    }
}

/// How far the first line reaches above the cell's plain first baseline and the last line below its plain last
/// line, each within what may reach into the margins: what [`keep_baseline`] takes off.
fn baseline_reach(lay: &Layout, font: &TextFont, size: f32, above: f32, below: f32) -> (f32, f32) {
    let over = (lay.baseline - font.ascent * size).clamp(0.0, above.max(0.0));
    let under = lay.lines.last().map_or(0.0, |last| {
        let depth = lay.height - over - (last.baseline - over);
        (depth - (font.row - font.ascent) * size).clamp(0.0, below.max(0.0))
    });
    (over, under)
}

/// What a cached layout depends on: the cell's revision, the content width and the zoom (as bits), the generation
/// of the window's fonts.
pub type LayoutKey = (u64, u32, u32, u32);

/// A typeset output to lay out in the background.
pub struct Job {
    /// the cell
    pub id: CellId,
    /// what the layout is for
    pub key: LayoutKey,
    /// the printed output
    pub text: String,
    /// layout options
    pub opts: LayoutOptions,
    /// the output's text font
    pub font: TextFont,
    /// right-justify the lines at this width (outputs)
    pub right: Option<f32>,
    /// how far scripts may reach into the margins above and below (see [`keep_baseline`])
    pub reach: (f32, f32),
}

/// A finished background layout.
pub struct Done {
    /// the cell
    pub id: CellId,
    /// what the layout is for
    pub key: LayoutKey,
    /// the layout
    pub layout: Arc<Layout>,
}

/// Typesetting of large outputs on a worker thread (NB5.6): a 25,000-character output takes about 25 ms to
/// typeset and lay out, more than a frame; the window shows the output's linear text (or its previous layout, while
/// the width changes) until the result arrives. Only the newest job of a cell is done.
pub struct Background {
    tx: Sender<Job>,
    rx: Receiver<Done>,
}

impl Background {
    /// Start the worker; it asks `ctx` to repaint when a result is ready.
    pub fn new(ctx: &egui::Context) -> Background {
        let (tx, jobs) = mpsc::channel::<Job>();
        let (done, rx) = mpsc::channel::<Done>();
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().name("mc-typeset".into()).spawn(move || {
            while let Ok(first) = jobs.recv() {
                // only the newest job per cell (a resize sends one per frame)
                let mut batch: Vec<Job> = vec![first];
                while let Ok(j) = jobs.try_recv() {
                    batch.retain(|b| b.id != j.id);
                    batch.push(j);
                }
                for j in batch {
                    let row = mc_typeset::math_text(&j.text).row;
                    let mut layout = lay_out(&row, &j.opts, &j.font);
                    keep_baseline(&mut layout, &j.font, j.opts.size, j.reach.0, j.reach.1);
                    if let Some(w) = j.right {
                        layout.align_right(w);
                    }
                    if done.send(Done { id: j.id, key: j.key, layout: Arc::new(layout) }).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            }
        });
        Background { tx, rx }
    }

    /// Ask for a layout.
    pub fn submit(&self, job: Job) {
        let _ = self.tx.send(job);
    }

    /// The layouts finished since the last call.
    pub fn results(&self) -> Vec<Done> {
        self.rx.try_iter().collect()
    }
}

/// How typeset content is drawn: its colour, the theme, the frame colour of graphics and the cell's text face.
pub struct Ink<'a> {
    /// the text colour
    pub color: Color32,
    /// dark theme (glyph coverage is mapped for light text on dark)
    pub dark: bool,
    /// frames of graphics
    pub border: Color32,
    /// the cell's text face (characters of the text font are drawn in it by egui)
    pub face: Face,
    /// the colour of a character by layout row and item (syntax colouring of typeset input), asked for the
    /// characters drawn only (round 37: a table of every character of a long cell cost 2.4 ms a keystroke); `None`
    /// from it, or no function, is `color`
    pub colours: Option<&'a dyn Fn(u32, u32) -> Option<Color32>>,
}

impl<'a> Ink<'a> {
    /// The ink of a palette for text of `color` in `face`.
    pub fn new(p: &Palette, color: Color32, face: Face) -> Ink<'a> {
        Ink { color, dark: p.dark, border: p.border_strong(), face, colours: None }
    }
}

/// A run of text-font characters that follow each other on one baseline at one size: drawn as one egui galley, so
/// the characters are placed and rasterised exactly as in plain text.
struct Run {
    /// left of the first character and its baseline (layout units)
    x: f32,
    y: f32,
    size: f32,
    /// where the next character of the run would start
    next: f32,
    job: LayoutJob,
    /// characters of one colour not in the job yet (one section per colour, not per character)
    pending: String,
    colour: Option<Color32>,
    font: egui::FontId,
}

impl Run {
    /// The pending characters as one section of the job.
    fn settle(&mut self) {
        if let Some(color) = self.colour.filter(|_| !self.pending.is_empty()) {
            let format = TextFormat { font_id: self.font.clone(), color, ..TextFormat::default() };
            self.job.append(&std::mem::take(&mut self.pending), 0.0, format);
        }
    }
}

/// Paint a layout with its top-left at `origin` (screen), the parts within `clip` only.
pub fn paint(painter: &Painter, glyphs: &mut Glyphs, l: &Layout, origin: Pos2, clip: Rect, ink: &Ink<'_>) {
    let color = ink.color;
    let ppp = painter.pixels_per_point();
    let f = font();
    let tex_id = glyphs.texture().id();
    let side = glyphs.side as f32;
    let mut mesh = Mesh::with_texture(tex_id);
    let white = Rect::from_min_max(pos2(1.0 / side, 1.0 / side), pos2(3.0 / side, 3.0 / side));
    let snap = |v: f32| (v * ppp).round() / ppp;
    let visible = |y: f32, size: f32| origin.y + y + size * 1.5 >= clip.top() && origin.y + y - size * 1.5 <= clip.bottom();
    let glyph_at = |mesh: &mut Mesh, glyphs: &mut Glyphs, g: Glyph, size: f32, x: f32, y: f32, color: Color32| {
        let px = size * ppp;
        let dx = (origin.x + x) * ppp;
        let ix = dx.floor();
        let phase = (((dx - ix) * PHASES).floor() as u8).min(PHASES as u8 - 1);
        let iy = ((origin.y + y) * ppp).round();
        let Some(s) = glyphs.slot(g, px, phase, ink.dark) else { return };
        if s.w == 0 {
            return;
        }
        let k = s.scale;
        let min = pos2((ix + s.left as f32 * k) / ppp, (iy - s.top as f32 * k) / ppp);
        let r = Rect::from_min_size(min, vec2(s.w as f32 * k / ppp, s.h as f32 * k / ppp));
        let uv = Rect::from_min_size(pos2(s.x as f32 / side, s.y as f32 / side), vec2(s.w as f32 / side, s.h as f32 / side));
        mesh.add_rect_with_uv(r, uv, color);
    };
    let mut run: Option<Run> = None;
    let flush = |run: &mut Option<Run>| {
        let Some(mut r) = run.take() else { return };
        r.settle();
        let galley = painter.layout_job(r.job);
        // the galley's first baseline on the run's baseline, on a device pixel as plain text's is
        let base = galley.rows.first().and_then(|row| row.row.glyphs.first()).map_or(r.size * 0.8, |g| g.pos.y);
        painter.galley(pos2(snap(origin.x + r.x), snap(origin.y + r.y) - base), galley, color);
    };
    for d in &l.items {
        match d {
            Draw::Char { c, role, size, x, y, row, index, advance, stretch } => {
                if !visible(*y, *size * stretch.max(1.0)) {
                    continue;
                }
                // a space joins the run it continues (the text keeps its plain-line positions) and draws nothing
                if c.is_whitespace() && !run.as_ref().is_some_and(|r| r.y == *y && r.size == *size && (r.next - x).abs() <= 0.01 * size) {
                    continue;
                }
                let color = ink.colours.and_then(|f| f(*row, *index)).unwrap_or(color);
                if *stretch > 1.0001 && *role == FontRole::Text {
                    // a tall bracket: the glyph's mesh, scaled vertically about its baseline
                    flush(&mut run);
                    let mut buf = [0u8; 4];
                    let format = TextFormat { font_id: ink.face.font(*size), color, ..TextFormat::default() };
                    let galley = painter.layout_job(LayoutJob::single_section(c.encode_utf8(&mut buf).to_string(), format));
                    let base = galley.rows.first().and_then(|row| row.row.glyphs.first()).map_or(size * 0.8, |g| g.pos.y);
                    let (bx, by) = (origin.x + x, snap(origin.y + y));
                    // the galley's texture coordinates are in texels of the font atlas
                    let [tw, th] = painter.ctx().fonts(|f| f.font_image_size());
                    let (tw, th) = (tw.max(1) as f32, th.max(1) as f32);
                    for placed in &galley.rows {
                        let mut m = placed.row.visuals.mesh.clone();
                        for v in &mut m.vertices {
                            let (px, py) = (v.pos.x + placed.pos.x, v.pos.y + placed.pos.y);
                            v.pos = pos2(bx + px, by + (py - base) * stretch);
                            v.uv = pos2(v.uv.x / tw, v.uv.y / th);
                        }
                        painter.add(egui::Shape::mesh(m));
                    }
                    continue;
                }
                if *role == FontRole::Math {
                    if let Some(g) = f.glyph(*c) {
                        glyph_at(&mut mesh, glyphs, g, *size, *x, *y, color);
                        continue;
                    }
                }
                let joins = run.as_ref().is_some_and(|r| r.y == *y && r.size == *size && (r.next - x).abs() <= 0.01 * size);
                if !joins {
                    flush(&mut run);
                    run = Some(Run {
                        x: *x,
                        y: *y,
                        size: *size,
                        next: *x,
                        job: LayoutJob::default(),
                        pending: String::new(),
                        colour: None,
                        font: ink.face.font(*size),
                    });
                }
                if let Some(r) = run.as_mut() {
                    // characters of one colour make one section (a long line of code is a few sections, not one per
                    // character: the galley cache hashes and lays out less)
                    if r.colour != Some(color) {
                        r.settle();
                        r.colour = Some(color);
                    }
                    r.pending.push(*c);
                    r.next = x + advance;
                }
            }
            Draw::Placeholder { x, y, size, .. } => {
                if visible(*y, *size) {
                    if let Some(g) = f.glyph('□') {
                        glyph_at(&mut mesh, glyphs, g, *size, *x, *y, color);
                    }
                }
            }
            Draw::Glyph { glyph, size, x, y } => {
                if visible(*y, *size * 4.0) {
                    glyph_at(&mut mesh, glyphs, *glyph, *size, *x, *y, color);
                }
            }
            Draw::Rule { x, y, w, h } => {
                if !visible(*y, *h + 4.0) {
                    continue;
                }
                // whole device pixels, at least one
                let top = snap(origin.y + y);
                let hh = ((h * ppp).round().max(1.0)) / ppp;
                let r = Rect::from_min_size(pos2(snap(origin.x + x), top), vec2(snap(*w).max(1.0 / ppp), hh));
                mesh.add_rect_with_uv(r, white, color);
            }
            Draw::Delim { c, x, y, w, h, stroke, row, index } => {
                if !visible(*y + *h / 2.0, *h / 2.0 + 4.0) {
                    continue;
                }
                // a tall bracket as strokes, in its character's colour (inputs colour brackets by depth)
                let color = ink.colours.and_then(|f| f(*row, *index)).unwrap_or(color);
                for line in mc_boxes::layout::delimiter_paths(*c, origin.x + x, origin.y + y, *w, *h, *stroke) {
                    let pts: Vec<Pos2> = line.iter().map(|p| pos2(p[0], p[1])).collect();
                    painter.add(egui::Shape::line(pts, egui::Stroke::new(*stroke, color)));
                }
            }
            Draw::Graphic { x, y, w, h, .. } => {
                let r = Rect::from_min_size(origin + vec2(*x, *y), vec2(*w, *h));
                painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, ink.border), egui::StrokeKind::Inside);
            }
        }
    }
    flush(&mut run);
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// The right end of each line of a layout (selection highlights and the caret after a typeset output).
pub fn line_rights(l: &Layout) -> Vec<f32> {
    l.lines.iter().map(|ln| ln.right).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A printed polynomial of about 25,000 characters (the M5 performance case).
    fn polynomial() -> String {
        let mut terms = vec![];
        let mut k = 0u64;
        while terms.iter().map(|t: &String| t.len() + 3).sum::<usize>() < 25_000 {
            terms.push(format!("{}*x^{}*y^{}", (k * 7919) % 100_003 + 1, k, (k * 3) % 17));
            k += 1;
        }
        terms.join(" + ")
    }

    #[test]
    fn a_long_polynomial_typesets_and_lays_out() {
        let ctx = egui::Context::default();
        let defs = mc_ui::fonts::definitions(mc_ui::fonts::Source::Bundled);
        ctx.set_fonts(defs.clone());
        let set = mc_ui::fonts::FontSet::new(&defs);
        // egui has fonts once a frame ran
        let mut font = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            font = Some(TextFont::new(ui.ctx(), Face::Code, set.face(Face::Code), 17.3));
        });
        let font = font.expect("a frame ran");
        // a line of plain text is as tall as egui's row of the code font
        assert!(font.row > 1.0 && font.row < 1.4 && font.ascent > 0.6 && font.ascent < font.row, "{font:?}");
        let text = polynomial();
        // the first run warms the font tables up
        let row = mc_typeset::typeset_output(&text).row;
        let _ = lay_out(&row, &LayoutOptions::output(17.3, Some(900.0)), &font);
        let t = std::time::Instant::now();
        let row = mc_typeset::typeset_output(&text).row;
        let typeset = t.elapsed().as_secs_f64() * 1e3;
        let t = std::time::Instant::now();
        let l = lay_out(&row, &LayoutOptions::output(17.3, Some(900.0)), &font);
        let layout = t.elapsed().as_secs_f64() * 1e3;
        let t = std::time::Instant::now();
        let rights = line_rights(&l);
        let rights_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "{} characters: typeset {typeset:.1} ms, layout {layout:.1} ms ({} lines, {} items), line ends {rights_ms:.1} ms",
            text.len(),
            l.lines.len(),
            l.items.len()
        );
        assert!(l.lines.len() > 50 && rights.len() == l.lines.len());
        assert!(l.lines.iter().all(|ln| ln.bottom > ln.top));
        // every line fits the width
        assert!(rights.iter().all(|&r| r <= 900.5), "{rights:?}");
    }
}
