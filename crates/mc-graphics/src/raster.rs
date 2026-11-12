//! The raster surface: tiny-skia paints the page at a scale factor (pixels per point) into a pixmap, encoded as
//! PNG. Glyphs are filled from the math font's outlines.
use crate::color::Rgba;
use crate::surface::{Cap, Path, Pen, Role, Seg, Surface, Text};
use crate::theme::Theme;
use mc_mathfont::PathCmd;
use std::collections::HashMap;
use tiny_skia::{
    FillRule, FilterQuality, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, StrokeDash, Transform,
};

/// The largest pixmap, in pixels (4 bytes each).
pub const MAX_PIXELS: u64 = 40_000_000;

/// A page being painted into pixels.
pub struct Raster {
    pm: Pixmap,
    scale: f32,
    /// the clip rectangles in force, and the mask of their intersection
    clips: Vec<[f32; 4]>,
    mask: Option<Mask>,
    /// glyph outlines in ems (y up) by glyph id
    outlines: HashMap<u16, Vec<PathCmd>>,
    /// chrome drawn in a theme's colours (`None`: as the graphic gives them)
    theme: Option<Theme>,
    role: Role,
}

impl Raster {
    /// A transparent page of `width` x `height` points at `scale` pixels per point; `None` when it would be empty
    /// or larger than [`MAX_PIXELS`].
    pub fn new(width: f32, height: f32, scale: f32) -> Option<Raster> {
        let (w, h) = ((width * scale).ceil().max(1.0), (height * scale).ceil().max(1.0));
        if !(w.is_finite() && h.is_finite()) || (w as u64) * (h as u64) > MAX_PIXELS {
            return None;
        }
        Some(Raster { pm: Pixmap::new(w as u32, h as u32)?, scale, clips: vec![], mask: None, outlines: HashMap::new(), theme: None, role: Role::Data })
    }

    /// Draw chrome in `theme`'s colours from now on.
    pub fn with_theme(mut self, theme: Option<Theme>) -> Raster {
        self.theme = theme;
        self
    }

    /// The colour to draw `c` in for the current role (`None`: not drawn).
    fn ink(&self, c: Rgba) -> Option<Rgba> {
        match &self.theme {
            Some(t) => t.color(self.role, c),
            None => Some(c),
        }
    }

    /// The PNG bytes of the page.
    pub fn png(&self) -> Option<Vec<u8>> {
        self.pm.encode_png().ok()
    }

    /// The page's pixels: width, height and straight (not premultiplied) RGBA bytes, the top row first.
    pub fn rgba(&self) -> (u32, u32, Vec<u8>) {
        let mut out = Vec::with_capacity(self.pm.data().len());
        for p in self.pm.pixels() {
            let c = p.demultiply();
            out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        (self.pm.width(), self.pm.height(), out)
    }

    fn transform(&self) -> Transform {
        Transform::from_scale(self.scale, self.scale)
    }

    fn rebuild_mask(&mut self) {
        let Some(first) = self.clips.first().copied() else {
            self.mask = None;
            return;
        };
        // the intersection of the rectangles
        let r = self.clips.iter().skip(1).fold(first, |a, b| {
            let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
            let (x1, y1) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
            [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
        });
        let Some(mut m) = Mask::new(self.pm.width(), self.pm.height()) else { return };
        if let Some(rect) = tiny_skia::Rect::from_xywh(r[0], r[1], r[2], r[3]) {
            m.fill_path(&PathBuilder::from_rect(rect), FillRule::Winding, false, self.transform());
        }
        self.mask = Some(m);
    }
}

/// A tiny-skia path of a page path.
fn skia(path: &Path) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for s in &path.segs {
        match *s {
            Seg::Move(x, y) => pb.move_to(x, y),
            Seg::Line(x, y) => pb.line_to(x, y),
            Seg::Cubic(a, b, c, d, x, y) => pb.cubic_to(a, b, c, d, x, y),
            Seg::Close => pb.close(),
        }
    }
    pb.finish()
}

fn paint(c: Rgba) -> Paint<'static> {
    let [r, g, b, a] = c.bytes();
    let mut p = Paint::default();
    p.set_color_rgba8(r, g, b, a);
    p.anti_alias = true;
    p
}

impl Surface for Raster {
    fn role(&mut self, role: Role) {
        self.role = role;
    }

    fn fill(&mut self, path: &Path, color: Rgba) {
        let Some(color) = self.ink(color) else { return };
        if let Some(p) = skia(path) {
            let ts = self.transform();
            self.pm.fill_path(&p, &paint(color), FillRule::Winding, ts, self.mask.as_ref());
        }
    }

    fn stroke(&mut self, path: &Path, pen: &Pen) {
        let Some(color) = self.ink(pen.color) else { return };
        let Some(p) = skia(path) else { return };
        let stroke = Stroke {
            width: pen.width,
            line_cap: match pen.cap {
                Cap::Butt => LineCap::Butt,
                Cap::Round => LineCap::Round,
            },
            line_join: LineJoin::Round,
            dash: if pen.dash.is_empty() { None } else { StrokeDash::new(even(&pen.dash), 0.0) },
            ..Stroke::default()
        };
        let ts = self.transform();
        self.pm.stroke_path(&p, &paint(color), &stroke, ts, self.mask.as_ref());
    }

    fn dots(&mut self, centres: &[(f32, f32)], d: f32, color: Rgba) {
        let Some(color) = self.ink(color) else { return };
        // one path of every disc, filled once
        let mut pb = PathBuilder::new();
        let r = (d / 2.0).max(0.25);
        for &(x, y) in centres {
            pb.push_circle(x, y, r);
        }
        if let Some(p) = pb.finish() {
            let ts = self.transform();
            self.pm.fill_path(&p, &paint(color), FillRule::Winding, ts, self.mask.as_ref());
        }
    }

    fn text(&mut self, t: &Text<'_>) {
        let Some(color) = self.ink(t.color) else { return };
        let font = mc_mathfont::font();
        let mut pb = PathBuilder::new();
        for g in t.glyphs {
            let cmds = self.outlines.entry(g.id).or_insert_with(|| font.outline(g.id));
            // em units, y up, to the label's coordinates, then onto the page
            let at = |x: f32, y: f32| t.place(g.x + x * g.size, g.y - y * g.size);
            for c in cmds.iter() {
                match *c {
                    PathCmd::Move(x, y) => {
                        let (x, y) = at(x, y);
                        pb.move_to(x, y)
                    }
                    PathCmd::Line(x, y) => {
                        let (x, y) = at(x, y);
                        pb.line_to(x, y)
                    }
                    PathCmd::Quad(a, b, x, y) => {
                        let ((a, b), (x, y)) = (at(a, b), at(x, y));
                        pb.quad_to(a, b, x, y)
                    }
                    PathCmd::Cubic(a, b, c, d, x, y) => {
                        let ((a, b), (c, d), (x, y)) = (at(a, b), at(c, d), at(x, y));
                        pb.cubic_to(a, b, c, d, x, y)
                    }
                    PathCmd::Close => pb.close(),
                }
            }
        }
        for r in t.rules {
            let corners = [t.place(r[0], r[1]), t.place(r[0] + r[2], r[1]), t.place(r[0] + r[2], r[1] + r[3]), t.place(r[0], r[1] + r[3])];
            pb.move_to(corners[0].0, corners[0].1);
            for c in &corners[1..] {
                pb.line_to(c.0, c.1);
            }
            pb.close();
        }
        if let Some(p) = pb.finish() {
            let ts = self.transform();
            self.pm.fill_path(&p, &paint(color), FillRule::Winding, ts, self.mask.as_ref());
        }
    }

    fn image(&mut self, rect: [f32; 4], cols: usize, rows: usize, rgba: &[u8]) {
        let Some(cells) = pixmap(cols, rows, rgba) else { return };
        let s = self.scale;
        let ts = Transform::from_row(
            s * rect[2] / cols as f32,
            0.0,
            0.0,
            s * rect[3] / rows as f32,
            s * rect[0],
            s * rect[1],
        );
        let paint = PixmapPaint { quality: FilterQuality::Nearest, ..PixmapPaint::default() };
        self.pm.draw_pixmap(0, 0, cells.as_ref(), &paint, ts, self.mask.as_ref());
    }

    fn clip(&mut self, rect: [f32; 4]) {
        self.clips.push(rect);
        self.rebuild_mask();
    }

    fn unclip(&mut self) {
        self.clips.pop();
        self.rebuild_mask();
    }
}

/// A pixmap of cells given as straight RGBA bytes (premultiplied as tiny-skia stores them).
pub fn pixmap(cols: usize, rows: usize, rgba: &[u8]) -> Option<Pixmap> {
    if rgba.len() != cols * rows * 4 {
        return None;
    }
    let mut pm = Pixmap::new(u32::try_from(cols).ok()?, u32::try_from(rows).ok()?)?;
    for (out, c) in pm.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(rgba.as_chunks::<4>().0) {
        let a = c[3] as u32;
        let pre = |v: u8| ((v as u32 * a + 127) / 255) as u8;
        out.copy_from_slice(&[pre(c[0]), pre(c[1]), pre(c[2]), c[3]]);
    }
    Some(pm)
}

/// A dash pattern with an even number of entries (an odd one is repeated, as PDF and SVG read it), none of them
/// all zero.
pub fn even(dash: &[f32]) -> Vec<f32> {
    let mut v = dash.to_vec();
    if v.len() % 2 == 1 {
        v.extend_from_slice(dash);
    }
    if v.iter().all(|d| *d <= 0.0) {
        v.clear();
    }
    v
}
