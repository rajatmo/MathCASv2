//! Painting typeset content (design 4.7, 9.6; decision NB5.4): box layouts from `mc_boxes` drawn with the bundled
//! math font. Everything typeset (letters and digits too) is measured and drawn with the math font, so the layout's
//! metrics are exactly what is painted and nothing depends on the system's fonts; the rare character the math font
//! lacks is measured and drawn with the cell's text font through egui.
//!
//! Glyphs are rasterised at the device pixel size (with four horizontal sub-pixel phases) into one texture atlas of
//! our own, uploaded piecewise when a glyph is first drawn and never per frame. The atlas starts at 1024 x 1024
//! (4 MB) and grows once to 2048 x 2048 (16 MB); when that is full it is cleared at the start of the next frame
//! and refilled with what is drawn (so magnification steps and theme changes cannot make it grow without bound,
//! the round 9 lesson of egui's own atlas). Each frame the visible glyphs of a layout become one mesh.
use crate::theme::Palette;
use eframe::egui::{self, pos2, vec2, Color32, ColorImage, Mesh, Painter, Pos2, Rect, TextureHandle, TextureOptions};
use mc_boxes::layout::{metrics::math_char, CharMetrics, Draw, FontRole, Layout, LayoutOptions, Metrics};
use mc_boxes::Row;
use mc_mathfont::{font, rasterize_shifted, Glyph};
use mc_nbmodel::CellId;
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
}

impl Glyphs {
    /// An empty atlas (the texture is made when the first glyph is drawn).
    pub fn new(ctx: &egui::Context) -> Glyphs {
        Glyphs { ctx: ctx.clone(), tex: None, side: ATLAS_START, slots: HashMap::new(), shelves: vec![], next_y: 0, full: false }
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

/// Metrics for the layout: the math font for everything it has, the cell's text font (measured by egui, at 1 em)
/// for the rest. Every character of the row is measured once up front (a font lookup per character drawn made the
/// layout of a 25,000-character output twice as slow).
pub struct BoxMetrics {
    chars: HashMap<char, CharMetrics>,
}

impl BoxMetrics {
    /// Metrics for laying out `row`: the characters the math font lacks are measured with `measure` (advance of a
    /// character at 1 em).
    pub fn for_row(row: &Row, measure: &mut dyn FnMut(char) -> f32) -> BoxMetrics {
        let mut chars = HashMap::new();
        let mut stack = vec![row];
        while let Some(r) = stack.pop() {
            for it in &r.0 {
                match it {
                    mc_boxes::Item::Char(c) if !chars.contains_key(c) => {
                        let m = if font().glyph(*c).is_some() {
                            math_char(*c)
                        } else {
                            CharMetrics { advance: measure(*c), ascent: 0.75, descent: 0.25 }
                        };
                        chars.insert(*c, m);
                    }
                    mc_boxes::Item::Box(b) => stack.extend(b.slots()),
                    _ => {}
                }
            }
        }
        BoxMetrics { chars }
    }
}

impl Metrics for BoxMetrics {
    fn char(&self, c: char, _role: FontRole) -> CharMetrics {
        match self.chars.get(&c) {
            Some(m) => *m,
            None => math_char(c),
        }
    }

    fn line(&self, _role: FontRole) -> (f32, f32) {
        // the math font's own ascent and descent are tall (they cover its big operators): a text line is 0.8 / 0.25
        (0.8, 0.25)
    }
}

/// The advance (at 1 em) of a character in the cell's text font, measured by egui.
fn text_advance(ctx: &egui::Context, text_font: &egui::FontId, c: char) -> f32 {
    let f = egui::FontId::new(64.0, text_font.family.clone());
    ctx.fonts_mut(|fs| fs.glyph_width(&f, c)) / 64.0
}

/// Lay out `row` with the math font.
pub fn lay_out(ctx: &egui::Context, row: &Row, opts: &LayoutOptions, text_font: &egui::FontId) -> Layout {
    let m = BoxMetrics::for_row(row, &mut |c| text_advance(ctx, text_font, c));
    mc_boxes::layout::layout(row, &m, opts)
}

/// What a cached layout depends on: the cell's revision, the content width and the zoom (as bits).
pub type LayoutKey = (u64, u32, u32);

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
    /// advances (1 em) of the output's characters the math font lacks, measured with the text font
    pub fallback: HashMap<char, f32>,
}

/// A finished background layout.
pub struct Done {
    /// the cell
    pub id: CellId,
    /// what the layout is for
    pub key: LayoutKey,
    /// the layout
    pub layout: Arc<Layout>,
    /// the right end of each of its lines
    pub rights: Arc<Vec<f32>>,
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
                    let row = mc_boxes::typeset_output(&j.text).row;
                    let m = BoxMetrics::for_row(&row, &mut |c| j.fallback.get(&c).copied().unwrap_or(0.6));
                    let layout = mc_boxes::layout::layout(&row, &m, &j.opts);
                    let rights = Arc::new(line_rights(&layout));
                    if done.send(Done { id: j.id, key: j.key, layout: Arc::new(layout), rights }).is_err() {
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

/// The advances of the characters of `text` the math font lacks (for a background layout).
pub fn fallback_advances(ctx: &egui::Context, text: &str, text_font: &egui::FontId) -> HashMap<char, f32> {
    let mut out = HashMap::new();
    let f = font();
    for c in text.chars() {
        if !out.contains_key(&c) && f.glyph(c).is_none() {
            out.insert(c, text_advance(ctx, text_font, c));
        }
    }
    out
}

/// How typeset content is drawn: its colour, the theme, the frame colour of graphics and the cell's text font (for
/// characters the math font lacks).
pub struct Ink<'a> {
    /// the text colour
    pub color: Color32,
    /// dark theme (glyph coverage is mapped for light text on dark)
    pub dark: bool,
    /// frames of graphics
    pub border: Color32,
    /// the cell's text font
    pub text_font: &'a egui::FontId,
}

impl<'a> Ink<'a> {
    /// The ink of a palette for text of `color` in `text_font`.
    pub fn new(p: &Palette, color: Color32, text_font: &'a egui::FontId) -> Ink<'a> {
        Ink { color, dark: p.dark, border: p.border_strong(), text_font }
    }
}

/// Paint a layout with its top-left at `origin` (screen), the parts within `clip` only.
pub fn paint(painter: &Painter, glyphs: &mut Glyphs, l: &Layout, origin: Pos2, clip: Rect, ink: &Ink<'_>) {
    let (color, text_font) = (ink.color, ink.text_font);
    let ppp = painter.pixels_per_point();
    let f = font();
    let tex_id = glyphs.texture().id();
    let side = glyphs.side as f32;
    let mut mesh = Mesh::with_texture(tex_id);
    let white = Rect::from_min_max(pos2(1.0 / side, 1.0 / side), pos2(3.0 / side, 3.0 / side));
    let snap = |v: f32| (v * ppp).round() / ppp;
    let visible = |y: f32, size: f32| origin.y + y + size * 1.5 >= clip.top() && origin.y + y - size * 1.5 <= clip.bottom();
    let glyph_at = |mesh: &mut Mesh, glyphs: &mut Glyphs, g: Glyph, size: f32, x: f32, y: f32| {
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
    for d in &l.items {
        match d {
            Draw::Char { c, size, x, y, .. } => {
                if c.is_whitespace() || !visible(*y, *size) {
                    continue;
                }
                match f.glyph(*c) {
                    Some(g) => glyph_at(&mut mesh, glyphs, g, *size, *x, *y),
                    None => {
                        // a character the math font lacks: the text font, on the baseline
                        let font = egui::FontId::new(*size, text_font.family.clone());
                        let galley = painter.layout_no_wrap(c.to_string(), font, color);
                        let base = galley.rows.first().and_then(|r| r.row.glyphs.first()).map_or(*size * 0.8, |gl| gl.pos.y);
                        painter.galley(pos2(origin.x + x, snap(origin.y + y - base)), galley, color);
                    }
                }
            }
            Draw::Placeholder { x, y, size, .. } => {
                if visible(*y, *size) {
                    if let Some(g) = f.glyph('□') {
                        glyph_at(&mut mesh, glyphs, g, *size, *x, *y);
                    }
                }
            }
            Draw::Glyph { glyph, size, x, y } => {
                if visible(*y, *size * 4.0) {
                    glyph_at(&mut mesh, glyphs, *glyph, *size, *x, *y);
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
            Draw::Graphic { x, y, w, h, .. } => {
                let r = Rect::from_min_size(origin + vec2(*x, *y), vec2(*w, *h));
                painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, ink.border), egui::StrokeKind::Inside);
            }
        }
    }
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// The right end of each line of a layout (selection highlights and the caret after a typeset output).
pub fn line_rights(l: &Layout) -> Vec<f32> {
    let mut right: Vec<f32> = l.lines.iter().map(|ln| ln.indent).collect();
    // lines are in order: the first whose bottom is at or below y
    let line_of = |y: f32| l.lines.partition_point(|ln| ln.bottom + 0.01 < y).min(l.lines.len().saturating_sub(1));
    let f = font();
    let mut advance: HashMap<char, f32> = HashMap::new();
    for d in &l.items {
        let (x1, y) = match d {
            Draw::Char { x, y, size, c, .. } => (x + *advance.entry(*c).or_insert_with(|| math_char(*c).advance) * size, *y),
            Draw::Placeholder { x, y, size, .. } => (x + math_char('□').advance * size, *y),
            Draw::Glyph { x, y, glyph, size } => (x + f.metrics(*glyph).advance * size, *y),
            Draw::Rule { x, y, w, .. } | Draw::Graphic { x, y, w, .. } => (x + w, *y),
        };
        if let Some(r) = right.get_mut(line_of(y)) {
            *r = r.max(x1);
        }
    }
    right
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
        let text = polynomial();
        let font = egui::FontId::monospace(17.3);
        // the first run warms the font tables up
        let row = mc_boxes::typeset_output(&text).row;
        let _ = lay_out(&ctx, &row, &LayoutOptions::output(17.3, Some(900.0)), &font);
        let t = std::time::Instant::now();
        let row = mc_boxes::typeset_output(&text).row;
        let typeset = t.elapsed().as_secs_f64() * 1e3;
        let t = std::time::Instant::now();
        let l = lay_out(&ctx, &row, &LayoutOptions::output(17.3, Some(900.0)), &font);
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
