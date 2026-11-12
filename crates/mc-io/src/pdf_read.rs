//! Reading PDF files through `hayro`: opening (with clear errors for encrypted and damaged files), page sizes,
//! rendering a page to a [`Raster`], and the text of a page.
//!
//! `hayro` is defensive about malformed input, but it is a large parser: every call into it runs under
//! `catch_unwind`, so a file that trips an internal assertion is reported as damaged instead of ending the session.

use crate::raster::{Raster, Samples, MAX_SAMPLES, MAX_SIDE};
use hayro::hayro_interpret::font::Glyph;
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask,
};
use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_syntax::page::Page;
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::kurbo::{Affine, BezPath, Point, Rect};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Why a PDF file could not be opened.
#[derive(Debug, PartialEq)]
pub enum OpenError {
    /// the file is password-protected (or uses encryption the reader does not support)
    Encrypted,
    /// the file is not a readable PDF
    Damaged,
}

/// Run `f`, turning a panic inside the PDF library into `None`.
pub fn guarded<R>(f: impl FnOnce() -> R) -> Option<R> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

/// Open the PDF file `bytes`.
pub fn open(bytes: Vec<u8>) -> Result<Pdf, OpenError> {
    if !bytes.windows(5).take(1024).any(|w| w == b"%PDF-") {
        return Err(OpenError::Damaged);
    }
    match guarded(|| Pdf::new(bytes)) {
        Some(Ok(pdf)) => Ok(pdf),
        Some(Err(LoadPdfError::Decryption(_))) => Err(OpenError::Encrypted),
        Some(Err(_)) | None => Err(OpenError::Damaged),
    }
}

/// The size of page `k` (0-based) in points, as displayed (rotation applied): width, height.
pub fn page_size(page: &Page<'_>) -> (f64, f64) {
    let (w, h) = page.render_dimensions();
    (w as f64, h as f64)
}

/// The pixel size of `page` rendered at `dpi`, or an error when it is empty or larger than [`MAX_SIDE`].
pub fn pixel_size(page: &Page<'_>, dpi: f64) -> Result<(u32, u32), String> {
    let (w, h) = page_size(page);
    let s = dpi / 72.0;
    let (pw, ph) = ((w * s).floor(), (h * s).floor());
    if pw < 1.0 || ph < 1.0 {
        return Err("the page is empty".into());
    }
    if pw > MAX_SIDE.min(u16::MAX as u32) as f64 || ph > MAX_SIDE.min(u16::MAX as u32) as f64 {
        return Err(format!("the page would be {pw} x {ph} pixels at {dpi} dpi (too large)"));
    }
    if pw * ph * 3.0 > MAX_SAMPLES as f64 {
        return Err(format!("the page would be {pw} x {ph} pixels at {dpi} dpi (too large)"));
    }
    Ok((pw as u32, ph as u32))
}

/// Render `page` at `dpi` on white: an 8-bit RGB raster.
pub fn render(page: &Page<'_>, dpi: f64) -> Result<Raster, String> {
    pixel_size(page, dpi)?;
    let s = (dpi / 72.0) as f32;
    let pixmap = guarded(|| {
        let cache = hayro::RenderCache::new();
        hayro::render(
            page,
            &cache,
            &InterpreterSettings::default(),
            &hayro::RenderSettings::default(),
            &hayro::PixmapSettings {
                x_scale: s,
                y_scale: s,
                bg_color: hayro::vello_cpu::color::palette::css::WHITE,
            },
        )
    })
    .ok_or("the page could not be rendered (damaged content)")?;
    let (w, h) = (pixmap.width() as u32, pixmap.height() as u32);
    // the background is opaque, so the premultiplied pixels are the colours: drop alpha
    let rgb: Vec<u8> =
        pixmap.data_as_u8_slice().as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    Ok(Raster { width: w, height: h, channels: 3, samples: Samples::U8(rgb), dpi: Some(dpi) })
}

/// The text of `page`: its glyphs in drawing order, with a space where a gap separates words and a line break
/// where the next glyph does not continue the current line (lines follow the text's own direction, so rotated
/// labels read as one line). Glyphs without a known Unicode value are skipped. `None` when the page's content
/// could not be interpreted.
pub fn text(page: &Page<'_>) -> Option<String> {
    guarded(|| {
        let cache = InterpreterCache::new();
        let (w, h) = page_size(page);
        let mut cx = Context::new(
            Affine::IDENTITY,
            Rect::new(0.0, 0.0, w.max(1.0), h.max(1.0)),
            &cache,
            page.xref(),
            InterpreterSettings::default(),
        );
        let mut dev = TextDevice::default();
        interpret_page(page, &mut cx, &mut dev);
        dev.out.trim_end().to_string()
    })
}

/// A device that keeps only text.
#[derive(Default)]
struct TextDevice {
    out: String,
    /// where the last glyph ended, its baseline direction (a unit vector) and its size, in user space
    last: Option<(Point, (f64, f64), f64)>,
}

impl TextDevice {
    fn glyph(&mut self, m: Affine, unicode: &str, advance: Option<f32>) {
        let c = m.as_coeffs();
        // glyph space is 1000 units per em
        let (xa, ya) = (c[0] * 1000.0, c[1] * 1000.0);
        let size_x = xa.hypot(ya);
        let size = (c[2] * 1000.0).hypot(c[3] * 1000.0).max(size_x).max(1e-6);
        let dir = if size_x > 0.0 { (xa / size_x, ya / size_x) } else { (1.0, 0.0) };
        let origin = m * Point::ZERO;
        if let Some((end, d, sz)) = self.last {
            let (dx, dy) = (origin.x - end.x, origin.y - end.y);
            let along = dx * d.0 + dy * d.1;
            let across = dx * d.1 - dy * d.0;
            let em = sz.max(size);
            if across.abs() > 0.5 * em || along < -em {
                self.out.truncate(self.out.trim_end_matches(' ').len());
                self.out.push('\n');
            } else if along > 0.2 * em && !self.out.ends_with([' ', '\n']) && !unicode.starts_with(' ') {
                self.out.push(' ');
            }
        }
        self.out.push_str(unicode);
        let adv = advance.map(|a| a as f64 / 1000.0).unwrap_or(0.5);
        let end = Point::new(origin.x + dir.0 * adv * size_x, origin.y + dir.1 * adv * size_x);
        self.last = Some((end, dir, size));
    }
}

impl<'a> Device<'a> for TextDevice {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for g in run.glyphs() {
            let Some(u) = g.as_unicode() else { continue };
            let s = match u {
                BfString::Char(c) => c.to_string(),
                BfString::String(s) => s,
            };
            // control characters (a glyph mapped to U+0000, say) carry no text
            let s: String = s.chars().filter(|c| !c.is_control()).collect();
            if s.is_empty() {
                continue;
            }
            let advance = match &**g {
                Glyph::Outline(o) => o.advance_width(),
                Glyph::Type3(_) => None,
            };
            self.glyph(props.transform * g.transform(), &s, advance);
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}
