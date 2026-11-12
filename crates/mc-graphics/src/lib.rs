//! Graphics: the inert `graphics(content, options)` term, its primitives, directives and options, colours, and
//! three renderers behind one drawing interface: raster PNG (at a scale factor for high-DPI displays), SVG, and
//! vector PDF with compressed streams and embedded subset fonts.
//!
//! ```text
//! graphics([red, disk([0, 0], 1), black, thickness(0.01), line([[-1, -1], [1, 1]]),
//!           text("$\\alpha_1^2$", [0, 1.2])], axes: true, plot_label: "a disk")
//! ```
//!
//! - **Primitives** (2D): `point`, `line`, `polygon`, `rectangle`, `circle`, `disk`, `arrow`, `bezier_curve`,
//!   `text(expr, position)`; positions are `[x, y]` or `scaled([u, v])` (a fraction of the plot range). A primitive
//!   with symbolic coordinates stays as it is; with numbers it is checked (`failure(argument, f, k)`).
//! - **Directives** apply to what follows them in their list (and nested lists): colours (`rgb_color`, `hue`,
//!   `gray_level`, the named colours `red`, ...), `opacity`, `thickness` / `absolute_thickness`, `point_size` /
//!   `absolute_point_size`, `dashing` / `absolute_dashing`, `edge_form`, `face_form`, `font_size`. Relative sizes
//!   are fractions of the plot area's width, absolute ones points.
//! - **Options**: `plot_range`, `plot_range_padding`, `axes`, `frame`, `axes_label`, `frame_label`, `plot_label`,
//!   `aspect_ratio`, `image_size`, `background`, `ticks`, `grid_lines`, `legend`, `legend_position`, `font_size`.
//! - `raster(cells, rect)` draws a grid of coloured cells (images, density plots); colour gradients ([`gradient`])
//!   colour numbers, and the `color_bar` option shows a gradient's scale beside the plot.
//! - `show(g1, g2, ...)` combines graphics; `color_convert`, `blend`, `lighter`, `darker`, `gradient_color`
//!   compute colours.
//!
//! Labels (tick labels, axis and frame labels, legends, `text`) are typeset with the bundled math font: strings
//! with a small TeX subset between `$` signs, `subscript`, `superscript`, and any expression in its typeset form
//! ([`label`]).
//!
//! The API for other crates: [`render`] (and [`render_formats`] for several formats from one reading),
//! [`is_graphics`], [`page_size`], and the colour and model types.
//!
//! This crate reads terms through the store contract (`mc-api`) only and has no evaluator, so front ends (the
//! notebook) use the model, the colours, the themes and the 3D viewer without the kernel. The natives that make
//! and check graphics terms (`graphics`, `show`, the primitives and directives, the colour functions, the
//! transformations and the layouts of pictures) are `mc-graphics-kernel`'s (split in round 34).
#![warn(missing_docs)]

pub mod color;
pub mod gradient;
pub mod label;
pub mod memo;
pub mod model;
pub mod num;
pub mod page;
mod pdf;
mod raster;
pub mod surface;
mod svg;
pub mod theme;
pub mod three;
pub mod ticks;
pub mod view3d;

pub use svg::ROOT_CLASS as SVG_ROOT_CLASS;

use mc_api::names;
use mc_api::{Store, View, T};
use std::fmt;

/// A graphic that could not be read or rendered: a failure tag and a description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// the tag: `not_graphics` (content that is no primitive or directive), `not_numeric` (a coordinate that is
    /// not a number), `option`, `unknown_option`, `too_large`, `argument`, `interrupted`
    pub tag: String,
    /// what was wrong, as text
    pub detail: String,
}

impl Failure {
    /// A failure with `tag` and `detail`.
    pub fn new(tag: &str, detail: impl Into<String>) -> Failure {
        Failure { tag: tag.to_string(), detail: detail.into() }
    }

    /// The failure as a term: `failure(tag, "detail")`.
    pub fn term<'h, S: Store + ?Sized>(&self, h: &'h S) -> T<'h> {
        let tag = h.sym_term(h.sym(&self.tag));
        h.call(h.sym(names::FAILURE), &[tag, h.string(&self.detail)])
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.tag, self.detail)
    }
}

impl std::error::Error for Failure {}

/// An output format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    /// PNG at `scale` pixels per point (1 for the natural size, 2 for a high-DPI display; at most 16)
    Png {
        /// pixels per point
        scale: f32,
    },
    /// SVG, one user unit per point
    Svg,
    /// a one-page vector PDF, the page the graphic's size in points
    Pdf,
}

/// Whether `t` is a graphics term: `graphics(content)` or `graphics(content, options(...))`.
pub fn is_graphics<S: Store + ?Sized>(h: &S, t: T<'_>) -> bool {
    match h.view(t) {
        View::App { head, args } => {
            matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "graphics" | "graphics3d")) && matches!(args.len(), 1 | 2)
        }
        _ => false,
    }
}

/// The page size of a graphic in points (width, height): the PNG's size in pixels at scale 1.
pub fn page_size<S: Store + ?Sized>(h: &S, t: T<'_>) -> Result<(f32, f32), Failure> {
    let g = model::read(h, t, &|| false)?;
    let p = page::plan(&g, &|| false)?;
    Ok((p.width, p.height))
}

/// Where the primitives of `part` (a graphics term holding part of `whole`'s content) lie on `whole`'s page:
/// `[left, top, right, bottom]` in points, y down from the page's top-left corner (the PNG's pixels at scale 1);
/// `None` when `part` has no positioned primitive. Used by `rasterize(g, "Regions")`.
pub fn region_on_page<S: Store + ?Sized>(h: &S, whole: T<'_>, part: T<'_>, stop: &dyn Fn() -> bool) -> Result<Option<[f32; 4]>, Failure> {
    let g = model::read(h, whole, stop)?;
    let p = page::plan(&g, stop)?;
    let sub = model::read(h, part, stop)?;
    let [(x0, x1), (y0, y1)] = page::bounds(&sub.items, stop)?;
    if !(x0.is_finite() && x1.is_finite() && y0.is_finite() && y1.is_finite()) {
        return Ok(None);
    }
    let ((rx0, rx1), (ry0, ry1)) = (p.range[0], p.range[1]);
    let px = |x: f64| p.area[0] + ((x - rx0) / (rx1 - rx0)) as f32 * p.area[2];
    let py = |y: f64| p.area[1] + ((ry1 - y) / (ry1 - ry0)) as f32 * p.area[3];
    let clamp = |v: f32, hi: f32| v.clamp(0.0, hi);
    Ok(Some([clamp(px(x0), p.width), clamp(py(y1), p.height), clamp(px(x1), p.width), clamp(py(y0), p.height)]))
}

/// Render a graphics term to the bytes of a PNG, SVG or PDF file.
pub fn render<S: Store + ?Sized>(h: &S, t: T<'_>, format: Format) -> Result<Vec<u8>, Failure> {
    let mut v = render_formats(h, t, &[format], &|| false)?;
    Ok(v.pop().unwrap_or_default())
}

/// Render a graphics term to pixels at `scale` pixels per point: width, height and straight RGBA bytes, the top
/// row first (frames of animations).
pub fn render_rgba<S: Store + ?Sized>(h: &S, t: T<'_>, scale: f32, stop: &dyn Fn() -> bool) -> Result<(u32, u32, Vec<u8>), Failure> {
    render_rgba_themed(h, t, scale, None, stop)
}

/// [`render_rgba`] with the chrome in `theme`'s colours (see [`theme`]; `None`: the light look of the files).
pub fn render_rgba_themed<S: Store + ?Sized>(
    h: &S,
    t: T<'_>,
    scale: f32,
    theme: Option<&theme::Theme>,
    stop: &dyn Fn() -> bool,
) -> Result<(u32, u32, Vec<u8>), Failure> {
    if !(scale > 0.0 && scale <= 16.0) {
        return Err(Failure::new("argument", format!("scale {scale}")));
    }
    let g = model::read(h, t, stop)?;
    let p = page::plan(&g, stop)?;
    let mut r = raster::Raster::new(p.width, p.height, scale)
        .ok_or_else(|| Failure::new("too_large", format!("{} x {} points at scale {scale}", p.width, p.height)))?
        .with_theme(g.opts.theme.or(theme.copied()));
    page::paint(&g, &p, &mut r, stop)?;
    Ok(r.rgba())
}

/// The pixels of a PNG file: width, height and straight RGBA bytes, the top row first.
pub fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let pm = tiny_skia::Pixmap::decode_png(bytes).ok()?;
    let mut out = Vec::with_capacity(pm.data().len());
    for p in pm.pixels() {
        let c = p.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Some((pm.width(), pm.height(), out))
}

/// Where a rendered graphic's plot area lies and what it shows: what a front end needs to map a pixel of the PNG
/// (at scale 1) to plot coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// the page width in points (PNG pixels at scale 1)
    pub width: f32,
    /// the page height in points
    pub height: f32,
    /// the plot area (x, y, w, h) in points, y down from the top-left corner of the page
    pub area: [f32; 4],
    /// the x and y plot range: plot coordinate `range[0].0` is at the area's left edge, `range[1].0` at its bottom
    pub range: [(f64, f64); 2],
}

/// Render a graphics term to several formats, reading it once; `stop` is polled in loops over the content (a
/// raised flag gives `Failure` `interrupted`).
pub fn render_formats<S: Store + ?Sized>(
    h: &S,
    t: T<'_>,
    formats: &[Format],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Vec<u8>>, Failure> {
    render_with_layout(h, t, formats, stop).map(|(files, _)| files)
}

/// [`render_with_layout`], also giving the graphic as read (heap-free), so that a caller that keeps it (layouts of
/// plots) does not read the term again.
pub fn render_read<S: Store + ?Sized>(
    h: &S,
    t: T<'_>,
    formats: &[Format],
    stop: &dyn Fn() -> bool,
) -> Result<(Vec<Vec<u8>>, Layout, model::Graphic), Failure> {
    let g = model::read(h, t, stop)?;
    let (files, layout) = render_graphic(&g, formats, stop)?;
    Ok((files, layout, g))
}

/// [`render_formats`], also giving the page's [`Layout`] (the same one every format was painted with).
pub fn render_with_layout<S: Store + ?Sized>(
    h: &S,
    t: T<'_>,
    formats: &[Format],
    stop: &dyn Fn() -> bool,
) -> Result<(Vec<Vec<u8>>, Layout), Failure> {
    let g = model::read(h, t, stop)?;
    render_graphic(&g, formats, stop)
}

/// Render a graphic already read (and perhaps changed: another page size) to several formats.
pub fn render_model(g: &model::Graphic, formats: &[Format], stop: &dyn Fn() -> bool) -> Result<Vec<Vec<u8>>, Failure> {
    render_graphic(g, formats, stop).map(|(files, _)| files)
}

/// Paint a planned page on a file's surface, in the graphic's own theme when it asks for one.
fn paint(g: &model::Graphic, p: &page::Plan, s: &mut dyn surface::Surface, stop: &dyn Fn() -> bool) -> Result<(), Failure> {
    match g.opts.theme {
        Some(t) => page::paint(g, p, &mut theme::Themed::new(s, t), stop),
        None => page::paint(g, p, s, stop),
    }
}

/// Render a graphic already read to several formats.
fn render_graphic(g: &model::Graphic, formats: &[Format], stop: &dyn Fn() -> bool) -> Result<(Vec<Vec<u8>>, Layout), Failure> {
    let p = page::plan(g, stop)?;
    let layout = Layout { width: p.width, height: p.height, area: p.area, range: p.range };
    let mut out = Vec::with_capacity(formats.len());
    for f in formats {
        out.push(match *f {
            Format::Png { scale } => {
                if !(scale > 0.0 && scale <= 16.0) {
                    return Err(Failure::new("argument", format!("scale {scale}")));
                }
                let mut r = raster::Raster::new(p.width, p.height, scale)
                    .ok_or_else(|| Failure::new("too_large", format!("{} x {} points at scale {scale}", p.width, p.height)))?;
                paint(g, &p, &mut r, stop)?;
                r.png().ok_or_else(|| Failure::new("io", "PNG encoding failed"))?
            }
            Format::Svg => {
                let mut s = svg::Svg::new(p.width, p.height);
                paint(g, &p, &mut s, stop)?;
                s.finish()
            }
            Format::Pdf => {
                let mut s = pdf::Pdf::new(p.width, p.height);
                paint(g, &p, &mut s, stop)?;
                s.finish()
            }
        });
    }
    Ok((out, layout))
}
