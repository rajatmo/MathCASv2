//! Coverage bitmaps of glyphs, for painters that draw math-font glyphs by id (stretched delimiters and their
//! pieces) as textures. Rasterising at the exact pixel size keeps them crisp at any magnification.
use crate::{Glyph, MathFont, PathCmd};
use ab_glyph_rasterizer::{point, Point, Rasterizer};

/// A glyph's coverage mask.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlyphBitmap {
    /// width in pixels
    pub width: usize,
    /// height in pixels
    pub height: usize,
    /// pixels from the glyph origin to the bitmap's left edge (usually negative or small)
    pub left: i32,
    /// pixels from the glyph's baseline up to the bitmap's top edge
    pub top: i32,
    /// coverage 0..=255, row by row from the top
    pub alpha: Vec<u8>,
}

/// Largest bitmap side drawn, in pixels (a guard against absurd sizes).
const MAX_SIDE: usize = 4096;

/// Rasterises glyph `g` of `font` at `px_per_em` pixels per em. Empty for glyphs without outline.
pub fn rasterize(font: &MathFont, g: Glyph, px_per_em: f32) -> GlyphBitmap {
    rasterize_shifted(font, g, px_per_em, 0.0)
}

/// Rasterises glyph `g` with its origin `dx` pixels (0 to 1) right of a pixel edge: painters that place glyphs at
/// whole pixels keep the advance's fraction this way (sub-pixel positioning), so text keeps even spacing at small
/// sizes. `left` is still relative to the pixel edge left of the origin.
pub fn rasterize_shifted(font: &MathFont, g: Glyph, px_per_em: f32, dx: f32) -> GlyphBitmap {
    let m = font.metrics(g);
    let k = px_per_em.max(0.0);
    let dx = dx.clamp(0.0, 1.0);
    let left = (m.x_min * k + dx).floor() as i32 - 1;
    let top = (m.height * k).ceil() as i32 + 1;
    let right = (m.x_max * k + dx).ceil() as i32 + 1;
    let bottom = (-m.depth * k).floor() as i32 - 1;
    let width = ((right - left).max(0) as usize).min(MAX_SIDE);
    let height = ((top - bottom).max(0) as usize).min(MAX_SIDE);
    let outline = font.outline(g);
    if outline.is_empty() || width == 0 || height == 0 {
        return GlyphBitmap::default();
    }
    let mut r = Rasterizer::new(width, height);
    // ems (y up) to bitmap pixels (y down)
    let tr = |x: f32, y: f32| point(x * k + dx - left as f32, top as f32 - y * k);
    let mut start: Point = point(0.0, 0.0);
    let mut cur = start;
    for c in outline {
        match c {
            PathCmd::Move(x, y) => {
                if cur != start {
                    r.draw_line(cur, start);
                }
                start = tr(x, y);
                cur = start;
            }
            PathCmd::Line(x, y) => {
                let p = tr(x, y);
                r.draw_line(cur, p);
                cur = p;
            }
            PathCmd::Quad(x1, y1, x, y) => {
                let p = tr(x, y);
                r.draw_quad(cur, tr(x1, y1), p);
                cur = p;
            }
            PathCmd::Cubic(x1, y1, x2, y2, x, y) => {
                let p = tr(x, y);
                r.draw_cubic(cur, tr(x1, y1), tr(x2, y2), p);
                cur = p;
            }
            PathCmd::Close => {
                if cur != start {
                    r.draw_line(cur, start);
                }
                cur = start;
            }
        }
    }
    let mut alpha = vec![0u8; width * height];
    r.for_each_pixel_2d(|x, y, a| {
        alpha[y as usize * width + x as usize] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
    });
    GlyphBitmap { width, height, left, top, alpha }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font;

    #[test]
    fn rasterised_sum_has_ink_and_space_has_none() {
        let f = font();
        let b = rasterize(f, f.glyph('∑').unwrap(), 40.0);
        assert!(b.width > 10 && b.height > 10, "{} x {}", b.width, b.height);
        let ink = b.alpha.iter().filter(|&&a| a > 128).count();
        assert!(ink > 50, "{ink}");
        assert_eq!(rasterize(f, f.glyph(' ').unwrap(), 40.0), GlyphBitmap::default());
    }

    #[test]
    fn a_shifted_glyph_moves_its_ink_right() {
        let f = font();
        let g = f.glyph('l').unwrap();
        let ink_x = |b: &GlyphBitmap| {
            let (mut sum, mut total) = (0.0, 0.0);
            for (k, &a) in b.alpha.iter().enumerate() {
                sum += (k % b.width) as f32 * f32::from(a) + b.left as f32 * f32::from(a);
                total += f32::from(a);
            }
            sum / total
        };
        let a = rasterize(f, g, 30.0);
        let b = rasterize_shifted(f, g, 30.0, 0.5);
        let d = ink_x(&b) - ink_x(&a);
        assert!((d - 0.5).abs() < 0.1, "{d}");
    }
}
