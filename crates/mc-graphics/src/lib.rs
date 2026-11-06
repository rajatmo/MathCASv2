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
#![warn(missing_docs)]

pub mod color;
pub mod compose;
pub mod gradient;
pub mod label;
pub mod model;
mod natives;
pub mod num;
pub mod page;
mod pdf;
mod raster;
pub mod surface;
mod svg;
pub mod three;
pub mod ticks;

pub use natives::{install, rgb_term};

use mc_api::names;
use mc_core::{Store, View, T};
use std::fmt;

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "graphics",
        "graphics(content), graphics(content, options...): a two-dimensional graphic of primitives (point, line, polygon, rectangle, circle, disk, arrow, bezier_curve, text) and directives; inert, rendered to PNG, SVG or PDF",
    ),
    ("show", "show(g1, g2, ..., options...): one graphic with the content of every graphic gi; options given here come first, then each graphic's"),
    ("point", "point([x, y]), point([[x1, y1], ...]): a point or points, drawn as dots of the current point size"),
    ("line", "line([[x1, y1], ...]): a polyline; line([[...], [...]]) several"),
    ("polygon", "polygon([[x1, y1], ...]): a filled polygon (face_form, edge_form); polygon([[...], ...]) several"),
    ("rectangle", "rectangle([x0, y0], [x1, y1]): a filled rectangle with these opposite corners; rectangle([x, y]) the unit square there"),
    ("circle", "circle([x, y], r): a circle (circle(c, [rx, ry]) an ellipse, circle(c, r, [a0, a1]) an arc, angles in radians)"),
    ("disk", "disk([x, y], r): a filled disk (disk(c, [rx, ry]) an ellipse, disk(c, r, [a0, a1]) a sector)"),
    ("arrow", "arrow([[x1, y1], ..., [xn, yn]]): a polyline with an arrowhead at its end (arrowheads sets their size and ends)"),
    (
        "raster",
        "raster(cells), raster(cells, [[x0, y0], [x1, y1]]), raster(cells, rect, gradient): a grid of coloured cells, rows from the bottom up; a cell is a number in [0, 1] (gray, or a colour of the gradient), a colour or none",
    ),
    (
        "arrowheads",
        "arrowheads(s): directive: arrows get a head s times the plot width long at their end; arrowheads([s0, s1]) heads at the start and the end (0: none); automatic: the default",
    ),
    (
        "gradient_color",
        "gradient_color(g, t): the colour at t in [0, 1] of the gradient g: a name (viridis, thermal, cool_warm, ocean, earth, rainbow, gray) or a list of colours",
    ),
    ("bezier_curve", "bezier_curve([p0, p1, p2, p3, ...]): a composite cubic Bezier curve through p0, p3, p6, ... with the points between as control points"),
    ("text", "text(expr, [x, y]), text(expr, [x, y], [ox, oy]): a label at a position; the offset puts that point of its box there ([-1, 0]: left end, [0, 1]: top); strings take $...$ math"),
    ("scaled", "scaled([u, v]): a position as a fraction of the plot range, in any primitive"),
    ("rgb_color", "rgb_color(r, g, b), rgb_color(r, g, b, a), rgb_color(\"#rrggbb\"): a colour, components in [0, 1]"),
    ("hue", "hue(h), hue(h, s, b), hue(h, s, b, a): a colour by hue (cyclic, 0 red, 1/3 green, 2/3 blue), saturation and brightness"),
    ("gray_level", "gray_level(g), gray_level(g, a): a gray from 0 (black) to 1 (white)"),
    ("opacity", "opacity(a), opacity(a, color): directive: what follows is drawn with opacity a in [0, 1]"),
    ("thickness", "thickness(t): directive: lines t times the plot width wide"),
    ("absolute_thickness", "absolute_thickness(p): directive: lines p points wide"),
    ("point_size", "point_size(d): directive: points d times the plot width across"),
    ("absolute_point_size", "absolute_point_size(d): directive: points d points across"),
    ("dashing", "dashing([d1, d2, ...]): directive: dashes and gaps, fractions of the plot width; dashing(dashed | dotted | dot_dashed | solid)"),
    ("absolute_dashing", "absolute_dashing([d1, d2, ...]): directive: dashes and gaps in points"),
    ("edge_form", "edge_form(directives): directive: edges of polygons, rectangles and disks drawn in this style; edge_form() none (the default)"),
    ("face_form", "face_form(directives): directive: faces filled in this colour; face_form() not filled"),
    ("font_size", "font_size(p): directive: text p points high; also the option font_size: p of graphics (tick labels, labels, text)"),
    ("color_convert", "color_convert(c, rgb_color | hue | gray_level): the colour c in another colour model"),
    ("blend", "blend([c1, c2, ...]): the average colour; blend([c1, c2, ...], t): the colour a fraction t along c1 .. cn"),
    ("lighter", "lighter(c), lighter(c, f): c a fraction f (default 1/3) of the way to white"),
    ("darker", "darker(c), darker(c, f): c a fraction f (default 1/3) of the way to black"),
    ("black", "the colour black"),
    ("white", "the colour white"),
    ("gray", "the colour gray (gray_level(0.5))"),
    ("light_gray", "the colour light gray (gray_level(0.85))"),
    ("dark_gray", "the colour dark gray (gray_level(0.25))"),
    ("red", "the colour red"),
    ("green", "the colour green"),
    ("blue", "the colour blue"),
    ("cyan", "the colour cyan"),
    ("magenta", "the colour magenta"),
    ("yellow", "the colour yellow"),
    ("orange", "the colour orange"),
    ("purple", "the colour purple"),
    ("brown", "the colour brown"),
    ("pink", "the colour pink"),
    ("plot_range", "option of graphics: plot_range: [[x0, x1], [y0, y1]], [y0, y1], r (y from -r to r) or automatic; ends may be automatic"),
    ("plot_range_padding", "option of graphics: plot_range_padding: f, automatic ranges widened by f times their length on each side (default 0.02); none"),
    ("axes", "option of graphics: axes: true | false | [x, y], axes through the origin with ticks"),
    ("frame", "option of graphics: frame: true, a frame with ticks and labels on the left and bottom"),
    ("axes_label", "option of graphics: axes_label: [x_label, y_label] (a single label is the y axis's)"),
    ("frame_label", "option of graphics: frame_label: [bottom, left], labels beside the frame (the left one read upwards)"),
    ("plot_label", "option of graphics: plot_label: label, a title above the graphic"),
    ("aspect_ratio", "option of graphics: aspect_ratio: r (height over width of the plot area), automatic (equal units), full (fills image_size)"),
    ("image_size", "option of graphics: image_size: w or [w, h], in points (pixels at scale 1); default width 360"),
    ("background", "option of graphics: background: color, or none for transparent (default white)"),
    ("ticks", "option of graphics: ticks: automatic | none | [x_ticks, y_ticks], each automatic, none or a list of positions or [position, label]"),
    ("grid_lines", "option of graphics: grid_lines: none | automatic | [x_lines, y_lines], each automatic, none or a list of positions or [position, directives]"),
    (
        "legend",
        "option of graphics and plots: legend: [entry, ...]; in graphics an entry is a label, [directives, label] (a line sample) or [directives, label, line | point | swatch | none]; in plots a list of strings, one per curve",
    ),
    ("legend_position", "option of graphics and plots: legend_position: top_right (default) | top_left | bottom_left | bottom_right"),
    ("epilog", "option of graphics and plots: epilog: content, graphics drawn over everything else"),
    ("prolog", "option of graphics and plots: prolog: content, graphics drawn before everything else"),
    ("axes_origin", "option of graphics: axes_origin: [x, y], where the axes cross (automatic: at 0)"),
    ("frame_ticks", "option of graphics: frame_ticks: automatic | none | [[left, right], [bottom, top]], the frame's ticks (the left and bottom ones are drawn)"),
    ("plot_range_clipping", "option of graphics: plot_range_clipping: true | false, whether what lies outside the plot range is cut off"),
    (
        "graphics3d",
        "graphics3d(content), graphics3d(content, options...): a three-dimensional graphic of primitives (sphere, cuboid, cylinder, cone, tube, polygon, triangle, line, point, arrow, text) seen in perspective, lit, in a box; rendered to PNG, SVG or PDF",
    ),
    ("sphere", "sphere(), sphere([x, y, z]), sphere([x, y, z], r), sphere([c1, c2, ...], r): spheres in 3D graphics (radius 1 by default)"),
    ("ball", "ball([x, y, z], r): a solid ball in 3D graphics (drawn as its sphere)"),
    ("ellipsoid", "ellipsoid([x, y, z], [rx, ry, rz]): an axis-aligned ellipsoid in 3D graphics"),
    ("cuboid", "cuboid(), cuboid([x, y, z]), cuboid(pmin, pmax): an axis-aligned box in 3D graphics (the unit cube at a corner by default)"),
    ("cylinder", "cylinder(), cylinder([[x1, y1, z1], [x2, y2, z2]], r): a cylinder between two points with radius r (default 1)"),
    ("cone", "cone(), cone([[x1, y1, z1], [x2, y2, z2]], r): a cone with its base of radius r at the first point and its apex at the second"),
    ("tube", "tube([p1, p2, ...], r): a tube of radius r (default 0.05) along a path in 3D graphics"),
    ("triangle", "triangle([p1, p2, p3]): a triangle in 3D graphics; triangle([[...], ...]) several"),
    ("view_point", "option of graphics3d and 3D plots: view_point: [x, y, z] (in units of the box, default [1.3, -2.4, 2]) or front, back, left, right, above, below"),
    ("box_ratios", "option of graphics3d and 3D plots: box_ratios: [a, b, c], the side lengths of the box (automatic: the data's)"),
    ("boxed", "option of graphics3d: boxed: true | false, the edges of the box drawn (default true)"),
    ("graphics_row", "graphics_row([g1, g2, ...]): the graphics (or plots) side by side, as one graphic"),
    ("graphics_column", "graphics_column([g1, g2, ...]): the graphics (or plots) one above the other, as one graphic"),
    ("graphics_grid", "graphics_grid([[g11, g12, ...], ...]): the graphics (or plots) in rows and columns, as one graphic"),
    (
        "grid",
        "grid([[a, b, ...], ...]): the parts (graphics, plots, expressions typeset) in rows and columns, as one graphic; frame: true | all draws a frame or lines between the cells",
    ),
    ("row", "row([a, b, ...]): with graphics or plots among them, the parts side by side as one graphic"),
    ("framed", "framed(x): x (a graphic, a plot or an expression) in a frame (options background, frame_style)"),
    ("labeled", "labeled(x, label), labeled(x, label, top | bottom | left | right): x with a label beside it (below by default)"),
    ("legended", "legended(x, [labels]), legended(x, line_legend([colours], [labels])): x with a legend at its right"),
    ("inset", "inset(obj, [x, y]), inset(obj, pos, offset, width): a graphic, plot or expression placed inside a graphic"),
    ("line_legend", "line_legend([colours], [labels]): a legend of line samples (in plot_legends and legended)"),
    ("swatch_legend", "swatch_legend([colours], [labels]): a legend of colour swatches (in plot_legends and legended)"),
    ("thick", "directive: lines 2 points wide"),
    ("thin", "directive: lines half a point wide"),
    ("dashed", "directive: dashed lines (also a value of dashing and of line_style)"),
    ("dotted", "directive: dotted lines (also a value of dashing and of line_style)"),
    ("dot_dashed", "directive: dot-dashed lines (also a value of dashing and of line_style)"),
    ("bold", "a text style of style(...): accepted; labels keep the math font"),
    ("italic", "a text style of style(...): accepted; labels keep the math font"),
    ("plain", "a text style of style(...): accepted"),
    ("underlined", "a text style of style(...): accepted"),
    ("tiny", "a size: thickness(tiny), point_size(tiny), arrowheads(tiny), image_size: tiny (100 points), a font size"),
    ("small", "a size: thickness(small), point_size(small), arrowheads(small), image_size: small (180 points), a font size"),
    ("medium", "a size: thickness(medium), point_size(medium), image_size: medium (360 points), a font size"),
    ("large", "a size: thickness(large), point_size(large), arrowheads(large), image_size: large (576 points), a font size"),
    ("directive", "directive(d1, d2, ...): several directives as one (colours, thickness, dashing, opacity, a font size)"),
    (
        "style",
        "style(expr, s1, s2, ...): expr shown with styles: colours, a font size (a number), bold, italic, directives; inside graphics the styles apply to the content",
    ),
    (
        "color_bar",
        "option of graphics: color_bar: [gradient, [lo, hi]] or [gradient, [lo, hi], label], a bar of the gradient's colours with a scale from lo to hi right of the plot; none; in density and contour plots true | false",
    ),
];

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

/// Render a graphics term to the bytes of a PNG, SVG or PDF file.
pub fn render<S: Store + ?Sized>(h: &S, t: T<'_>, format: Format) -> Result<Vec<u8>, Failure> {
    let mut v = render_formats(h, t, &[format], &|| false)?;
    Ok(v.pop().unwrap_or_default())
}

/// Render a graphics term to pixels at `scale` pixels per point: width, height and straight RGBA bytes, the top
/// row first (frames of animations).
pub fn render_rgba<S: Store + ?Sized>(h: &S, t: T<'_>, scale: f32, stop: &dyn Fn() -> bool) -> Result<(u32, u32, Vec<u8>), Failure> {
    if !(scale > 0.0 && scale <= 16.0) {
        return Err(Failure::new("argument", format!("scale {scale}")));
    }
    let g = model::read(h, t, stop)?;
    let p = page::plan(&g, stop)?;
    let mut r = raster::Raster::new(p.width, p.height, scale)
        .ok_or_else(|| Failure::new("too_large", format!("{} x {} points at scale {scale}", p.width, p.height)))?;
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

/// [`render_formats`], also giving the page's [`Layout`] (the same one every format was painted with).
pub fn render_with_layout<S: Store + ?Sized>(
    h: &S,
    t: T<'_>,
    formats: &[Format],
    stop: &dyn Fn() -> bool,
) -> Result<(Vec<Vec<u8>>, Layout), Failure> {
    let g = model::read(h, t, stop)?;
    let p = page::plan(&g, stop)?;
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
                page::paint(&g, &p, &mut r, stop)?;
                r.png().ok_or_else(|| Failure::new("io", "PNG encoding failed"))?
            }
            Format::Svg => {
                let mut s = svg::Svg::new(p.width, p.height);
                page::paint(&g, &p, &mut s, stop)?;
                s.finish()
            }
            Format::Pdf => {
                let mut s = pdf::Pdf::new(p.width, p.height);
                page::paint(&g, &p, &mut s, stop)?;
                s.finish()
            }
        });
    }
    Ok((out, layout))
}
