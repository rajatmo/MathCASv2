//! The graphics model: a `graphics(content, options)` term read into a [`Graphic`], free of the heap, which the
//! painter ([`crate::page`]) draws on any surface.
//!
//! The content is a primitive, a directive or a (nested) list of them. A directive applies to what follows it in
//! its list and in the lists nested there; a nested list starts with the style its parent has at that point, and
//! its directives end with it. The reader walks the content with an explicit stack (no recursion on terms).
use crate::color::{colour, is_symbolic, Colour, Rgba};
use crate::gradient::{self, Gradient};
use crate::label::Label;
use crate::num::{number, short};
use crate::Failure;
use mc_api::names;
use mc_core::{Packed, Store, View, T};

/// A length: absolute in points (1/72 inch, a pixel at scale 1) or relative to the width of the plot area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    /// points
    Abs(f64),
    /// a fraction of the plot area's width
    Rel(f64),
}

impl Size {
    /// The length in points when the plot area is `width` points wide.
    pub fn at(self, width: f64) -> f64 {
        match self {
            Size::Abs(p) => p,
            Size::Rel(f) => f * width,
        }
    }
}

/// How faces (polygons, disks, rectangles) are filled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    /// in the current colour
    Current,
    /// not at all
    None,
    /// in this colour (opacity included)
    Color(Rgba),
}

/// How the edges of faces are drawn (none unless `edge_form` says).
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    /// colour, opacity included
    pub color: Rgba,
    /// line width
    pub thickness: Size,
    /// dash pattern (empty: solid)
    pub dashing: Vec<Size>,
}

/// The state the directives set.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// the current colour
    pub color: Rgba,
    /// multiplies the colour's opacity
    pub opacity: f64,
    /// line width
    pub thickness: Size,
    /// the diameter of points
    pub point_size: Size,
    /// dash pattern of lines (empty: solid)
    pub dashing: Vec<Size>,
    /// edges of faces
    pub edge: Option<Edge>,
    /// fill of faces
    pub face: Face,
    /// text size in points (`None`: the graphic's `font_size`)
    pub font_size: Option<f64>,
    /// arrowheads at the start and the end of arrows, lengths as fractions of the plot area's width (0: none;
    /// `None`: one at the end, sized by the line width)
    pub arrowheads: Option<[f64; 2]>,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            color: Rgba::BLACK,
            opacity: 1.0,
            thickness: Size::Abs(1.0),
            point_size: Size::Rel(0.012),
            dashing: Vec::new(),
            edge: None,
            face: Face::Current,
            font_size: None,
            arrowheads: None,
        }
    }
}

impl Style {
    /// The colour of lines, points and text.
    pub fn ink(&self) -> Rgba {
        self.color.faded(self.opacity)
    }

    /// The fill of faces (`None`: not filled).
    pub fn fill(&self) -> Option<Rgba> {
        match self.face {
            Face::Current => Some(self.ink()),
            Face::None => None,
            Face::Color(c) => Some(c),
        }
    }
}

/// A position: in plot coordinates, or `scaled([u, v])`: a fraction of the plot range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    /// x
    pub x: f64,
    /// y
    pub y: f64,
    /// a fraction of the plot range rather than a coordinate
    pub scaled: bool,
}

/// A graphics primitive with its coordinates read.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// `point`: one or more points
    Points(Vec<Pt>),
    /// `line`: one or more polylines
    Line(Vec<Vec<Pt>>),
    /// `polygon`: one or more polygons
    Polygon(Vec<Vec<Pt>>),
    /// `rectangle(p, q)`: opposite corners
    Rect(Pt, Pt),
    /// `circle` (outline) or `disk` (filled): centre, radii, and an arc's angles (radians, anticlockwise)
    Ellipse {
        /// centre
        c: Pt,
        /// x and y radius in plot coordinates
        r: (f64, f64),
        /// from and to angle, for an arc (circle) or a sector (disk)
        arc: Option<(f64, f64)>,
        /// a disk
        filled: bool,
    },
    /// `arrow`: polylines with a head at their end
    Arrow(Vec<Vec<Pt>>),
    /// `bezier_curve`: control points of a composite cubic curve
    Bezier(Vec<Pt>),
    /// `raster(cells, [p, q])`: a grid of coloured cells filling the rectangle with corners `from` and `to`
    Raster {
        /// cells per row
        cols: usize,
        /// rows
        rows: usize,
        /// straight (not premultiplied) RGBA bytes, the top row first
        rgba: Vec<u8>,
        /// the lower-left corner
        from: Pt,
        /// the upper-right corner
        to: Pt,
    },
    /// `inset(graphic, position, offset, width)`: a whole graphic drawn in a rectangle of the page
    Inset {
        /// the graphic
        graphic: Box<Graphic>,
        /// its page size at its natural scale (points)
        natural: (f64, f64),
        /// where it goes
        at: Pt,
        /// which point of its rectangle is at `at`, as for text
        offset: (f64, f64),
        /// its width in plot coordinates (`None`: its natural size in points)
        width: Option<f64>,
    },
    /// `text(expr, position, offset)`
    Text {
        /// the label
        label: Label,
        /// where
        at: Pt,
        /// which point of the label's box is at `at`: (-1, -1) bottom left, (0, 0) centre, (1, 1) top right
        offset: (f64, f64),
    },
}

/// A primitive and the index of its style in [`Graphic::styles`].
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// the primitive
    pub shape: Shape,
    /// its style
    pub style: usize,
}

/// The aspect ratio (height over width) of the plot area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aspect {
    /// one unit in x as long as one in y
    Auto,
    /// whatever fills the image size
    Full,
    /// this ratio
    Ratio(f64),
}

/// Ticks on one axis.
#[derive(Clone, Debug, PartialEq)]
pub enum Ticks {
    /// about six at nice numbers
    Auto,
    /// none
    None,
    /// these positions with these labels (`None`: a tick without a label)
    List(Vec<(f64, Option<Label>)>),
}

/// Grid lines across one axis.
#[derive(Clone, Debug, PartialEq)]
pub enum Grid {
    /// none
    None,
    /// at the ticks
    Auto,
    /// at these positions, each with a style (an index in [`Graphic::styles`]) or the default one
    List(Vec<(f64, Option<usize>)>),
}

/// A corner of the plot area.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Corner {
    /// top right
    #[default]
    TopRight,
    /// top left
    TopLeft,
    /// bottom left
    BottomLeft,
    /// bottom right
    BottomRight,
}

/// The sample drawn before a legend entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Marker {
    /// a short line
    Line,
    /// a point
    Point,
    /// a filled square
    Swatch,
    /// nothing
    None,
}

/// One entry of the legend.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendEntry {
    /// the text
    pub label: Label,
    /// the sample's style (an index in [`Graphic::styles`])
    pub style: usize,
    /// the sample
    pub marker: Marker,
}

/// The options of a graphic.
#[derive(Clone, Debug, PartialEq)]
pub struct Opts {
    /// x and y range, each end given or automatic
    pub plot_range: [[Option<f64>; 2]; 2],
    /// padding of automatic ranges, a fraction of their length (`None`: 2%)
    pub padding: Option<f64>,
    /// x and y axes
    pub axes: [bool; 2],
    /// a frame with ticks
    pub frame: bool,
    /// labels at the ends of the x and y axes
    pub axes_label: [Option<Label>; 2],
    /// labels below and left of the frame
    pub frame_label: [Option<Label>; 2],
    /// a title above
    pub plot_label: Option<Label>,
    /// the plot area's shape
    pub aspect: Aspect,
    /// width and height in points (`None`: automatic)
    pub image_size: (Option<f64>, Option<f64>),
    /// `None`: transparent
    pub background: Option<Rgba>,
    /// x and y ticks
    pub ticks: [Ticks; 2],
    /// grid lines at x and y positions
    pub grid: [Grid; 2],
    /// legend entries
    pub legend: Vec<LegendEntry>,
    /// where the legend goes
    pub legend_position: Corner,
    /// the size of tick labels, other labels and text without a `font_size` directive
    pub font_size: f64,
    /// a colour bar right of the plot area
    pub color_bar: Option<ColorBar>,
    /// a title over the legend's entries
    pub legend_title: Option<Label>,
    /// where the axes cross (`None`: at 0, or at the lower end of a range without 0)
    pub origin: Option<(f64, f64)>,
    /// clip the primitives to the plot area (`None`: when the plot range is given)
    pub clip: Option<bool>,
    /// options left out: names nothing reads, and values an option does not take (in the order given)
    pub ignored: Vec<String>,
}

/// A colour bar: the gradient from the bottom to the top of the bar, the values at its ends, and a label.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorBar {
    /// the colours
    pub gradient: Gradient,
    /// the value at the bottom and at the top
    pub range: (f64, f64),
    /// a label beside the bar (read upwards)
    pub label: Option<Label>,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            plot_range: [[None; 2]; 2],
            padding: None,
            axes: [false; 2],
            frame: false,
            axes_label: [None, None],
            frame_label: [None, None],
            plot_label: None,
            aspect: Aspect::Auto,
            image_size: (None, None),
            background: Some(Rgba::WHITE),
            ticks: [Ticks::Auto, Ticks::Auto],
            grid: [Grid::None, Grid::None],
            legend: Vec::new(),
            legend_position: Corner::TopRight,
            font_size: 10.0,
            color_bar: None,
            legend_title: None,
            origin: None,
            clip: None,
            ignored: Vec::new(),
        }
    }
}

/// A graphic read from its term.
#[derive(Clone, Debug, PartialEq)]
pub struct Graphic {
    /// the primitives in drawing order
    pub items: Vec<Item>,
    /// the styles the items, grid lines and legend entries refer to (the first is the default)
    pub styles: Vec<Style>,
    /// the options
    pub opts: Opts,
}

/// Why a primitive or a directive could not be read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bad {
    /// a coordinate that is not a number yet: the term stays as it is
    Symbolic,
    /// a wrong argument (1-based)
    Arg(usize),
    /// the wrong number of arguments
    Arity,
}

/// The primitive heads.
pub const PRIMITIVES: [&str; 10] =
    ["point", "line", "polygon", "rectangle", "circle", "disk", "arrow", "bezier_curve", "text", "raster"];

/// The directive heads (besides the colours).
pub const DIRECTIVES: [&str; 12] = [
    "arrowheads",
    "opacity",
    "thickness",
    "absolute_thickness",
    "point_size",
    "absolute_point_size",
    "dashing",
    "absolute_dashing",
    "edge_form",
    "face_form",
    "font_size",
    "scaled",
];

/// The option names of `graphics` that change the picture.
pub const OPTIONS: [&str; 23] = [
    "legend_title",
    "color_bar",
    "plot_range",
    "plot_range_padding",
    "axes",
    "frame",
    "axes_label",
    "frame_label",
    "plot_label",
    "aspect_ratio",
    "image_size",
    "background",
    "ticks",
    "frame_ticks",
    "grid_lines",
    "legend",
    "legend_position",
    "font_size",
    "axes_origin",
    "plot_range_clipping",
    "epilog",
    "prolog",
    "plot_legends",
];

/// Options of styling or interaction the renderers take no notice of: accepted without remark.
pub const HARMLESS: &[&str] = &[
    "base_style",
    "label_style",
    "ticks_style",
    "frame_style",
    "axes_style",
    "grid_lines_style",
    "image_padding",
    "image_margins",
    "plot_region",
    "content_selectable",
    "rotate_label",
    "alignment_point",
    "baseline_position",
    "format_type",
    "method",
    "plot_theme",
    "coordinates_tool_options",
    "axes_edge",
    "frame_margins",
    "spherical_region",
    "view_angle",
    "view_vertical",
    "lighting",
    "rotation_action",
    "tooltip",
    "plot_range_padding_style",
];

/// Whether `graphics` knows the option `name` (reads it, or ignores it on purpose).
pub fn known_option(name: &str) -> bool {
    OPTIONS.contains(&name) || HARMLESS.contains(&name)
}

pub(crate) fn head_name<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    match h.view(t) {
        View::App { head, args } => match h.view(head) {
            View::Sym(s) => Some((h.sym_name(s), args)),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn list<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> Option<&'h [T<'h>]> {
    match head_name(h, t) {
        Some((n, args)) if n == names::LIST => Some(args),
        _ => None,
    }
}

pub(crate) fn sym_is<S: Store + ?Sized>(h: &S, t: T<'_>, name: &str) -> bool {
    matches!(h.view(t), View::Sym(s) if h.sym_name(s) == name)
}

/// A scalar: a number, or `Symbolic` for something that may become one.
fn scalar<S: Store + ?Sized>(h: &S, t: T<'_>, arg: usize) -> Result<f64, Bad> {
    match number(h, t) {
        Some(x) => Ok(x),
        None if is_symbolic(h, t) => Err(Bad::Symbolic),
        None => Err(Bad::Arg(arg)),
    }
}

/// A position `[x, y]` or `scaled([u, v])`.
pub fn point<S: Store + ?Sized>(h: &S, t: T<'_>, arg: usize) -> Result<Pt, Bad> {
    if let Some((n, args)) = head_name(h, t) {
        if n == "scaled" {
            let [p] = args else { return Err(Bad::Arg(arg)) };
            let p = point(h, *p, arg)?;
            return if p.scaled { Err(Bad::Arg(arg)) } else { Ok(Pt { scaled: true, ..p }) };
        }
    }
    match list(h, t) {
        Some(&[x, y]) => Ok(Pt { x: scalar(h, x, arg)?, y: scalar(h, y, arg)?, scaled: false }),
        Some(_) => Err(Bad::Arg(arg)),
        None if is_symbolic(h, t) => Err(Bad::Symbolic),
        None => Err(Bad::Arg(arg)),
    }
}

/// Whether `t` has the shape of one position (rather than a list of them).
fn is_position<S: Store + ?Sized>(h: &S, t: T<'_>) -> bool {
    match head_name(h, t) {
        Some((n, _)) if n == "scaled" => true,
        _ => match list(h, t) {
            Some(&[x, y]) => list(h, x).is_none() && list(h, y).is_none(),
            _ => false,
        },
    }
}

/// A list of positions; a packed `n x 2` array is read without looking at its elements.
pub fn points<S: Store + ?Sized>(h: &S, t: T<'_>, arg: usize) -> Result<Vec<Pt>, Bad> {
    match h.packed(t) {
        Some(Packed::Reals { dims, data }) if dims.len() == 2 && dims[1] == 2 => {
            if data.iter().any(|v| !v.is_finite()) {
                return Err(Bad::Arg(arg));
            }
            return Ok(data.as_chunks::<2>().0.iter().map(|p| Pt { x: p[0], y: p[1], scaled: false }).collect());
        }
        Some(Packed::Ints { dims, data }) if dims.len() == 2 && dims[1] == 2 => {
            return Ok(data.as_chunks::<2>().0.iter().map(|p| Pt { x: p[0] as f64, y: p[1] as f64, scaled: false }).collect());
        }
        _ => {}
    }
    match list(h, t) {
        Some(items) => items.iter().map(|&p| point(h, p, arg)).collect(),
        None if is_symbolic(h, t) => Err(Bad::Symbolic),
        None => Err(Bad::Arg(arg)),
    }
}

/// One list of positions, or a list of such lists.
fn runs<S: Store + ?Sized>(h: &S, t: T<'_>, arg: usize) -> Result<Vec<Vec<Pt>>, Bad> {
    if h.packed(t).is_some() {
        return match h.packed(t) {
            Some(Packed::Reals { dims, .. } | Packed::Ints { dims, .. }) if dims.len() == 3 && dims[2] == 2 => {
                list(h, t).ok_or(Bad::Arg(arg))?.iter().map(|&r| points(h, r, arg)).collect()
            }
            _ => Ok(vec![points(h, t, arg)?]),
        };
    }
    match list(h, t) {
        Some([]) => Ok(vec![]),
        Some(items) if is_position(h, items[0]) || list(h, items[0]).is_none() => Ok(vec![points(h, t, arg)?]),
        Some(items) => items.iter().map(|&r| points(h, r, arg)).collect(),
        None if is_symbolic(h, t) => Err(Bad::Symbolic),
        None => Err(Bad::Arg(arg)),
    }
}

/// The radii of `circle(c, r)`: a number or `[rx, ry]`, positive.
fn radii<S: Store + ?Sized>(h: &S, t: T<'_>, arg: usize) -> Result<(f64, f64), Bad> {
    let r = match list(h, t) {
        Some(&[a, b]) => (scalar(h, a, arg)?, scalar(h, b, arg)?),
        Some(_) => return Err(Bad::Arg(arg)),
        None => {
            let r = scalar(h, t, arg)?;
            (r, r)
        }
    };
    if r.0 >= 0.0 && r.1 >= 0.0 {
        Ok(r)
    } else {
        Err(Bad::Arg(arg))
    }
}

/// Read the primitive `name(args)`. `Ok(None)` when `name` is not a primitive.
pub fn shape<S: Store + ?Sized>(h: &S, name: &str, args: &[T<'_>]) -> Result<Option<Shape>, Bad> {
    let origin = Pt { x: 0.0, y: 0.0, scaled: false };
    // options of a primitive (`vertex_colors`, `spline_closed`, ...) are accepted and not used
    let args = match args.split_last() {
        Some((&o, rest)) if head_name(h, o).is_some_and(|(n, _)| n == "options") => rest,
        _ => args,
    };
    let s = match (name, args) {
        ("polygon", [r]) if is_rule(h, *r) => Shape::Polygon(with_holes(h, None, *r)?),
        ("polygon", [p, r]) if is_rule(h, *r) => Shape::Polygon(with_holes(h, points(h, *p, 1).ok(), *r)?),
        ("polygon", [p, faces]) => {
            // points and faces as lists of 1-based indices into them
            let pts = points(h, *p, 1)?;
            let faces = list(h, *faces).ok_or(Bad::Arg(2))?;
            let one = |f: T<'_>| -> Result<Vec<Pt>, Bad> {
                list(h, f)
                    .ok_or(Bad::Arg(2))?
                    .iter()
                    .map(|&i| match h.view(i) {
                        View::Int(k) if k >= 1 && (k as usize) <= pts.len() => Ok(pts[k as usize - 1]),
                        _ => Err(Bad::Arg(2)),
                    })
                    .collect()
            };
            let rings = if faces.first().is_some_and(|&f| list(h, f).is_some()) {
                faces.iter().map(|&f| one(f)).collect::<Result<_, _>>()?
            } else {
                vec![one(args[1])?]
            };
            Shape::Polygon(rings)
        }
        ("point", [p]) => Shape::Points(if is_position(h, *p) { vec![point(h, *p, 1)?] } else { points(h, *p, 1)? }),
        ("line", [p]) => Shape::Line(runs(h, *p, 1)?),
        ("polygon", [p]) => Shape::Polygon(runs(h, *p, 1)?),
        ("arrow", [p]) => Shape::Arrow(runs(h, *p, 1)?),
        ("bezier_curve", [p]) => Shape::Bezier(points(h, *p, 1)?),
        ("rectangle", []) => Shape::Rect(origin, Pt { x: 1.0, y: 1.0, scaled: false }),
        ("rectangle", [p]) => {
            let a = point(h, *p, 1)?;
            Shape::Rect(a, Pt { x: a.x + 1.0, y: a.y + 1.0, ..a })
        }
        ("rectangle", [p, q]) => {
            let (a, b) = (point(h, *p, 1)?, point(h, *q, 2)?);
            if a.scaled != b.scaled {
                return Err(Bad::Arg(2));
            }
            Shape::Rect(a, b)
        }
        // primitives shared with 3D graphics, in two dimensions
        ("triangle", []) => Shape::Polygon(vec![vec![origin, Pt { x: 1.0, y: 0.0, scaled: false }, Pt { x: 0.0, y: 1.0, scaled: false }]]),
        ("triangle", [p]) => Shape::Polygon(runs(h, *p, 1)?),
        ("ball", _) if args.len() <= 2 => return shape(h, "disk", args),
        ("ellipsoid", [c, r]) => return shape(h, "disk", &[*c, *r]),
        ("text", [e]) => Shape::Text { label: Label::of(h, *e), at: origin, offset: (0.0, 0.0) },
        ("circle" | "disk", _) if args.len() <= 3 => {
            let c = match args.first() {
                Some(&c) => point(h, c, 1)?,
                None => origin,
            };
            let r = match args.get(1) {
                Some(&r) => radii(h, r, 2)?,
                None => (1.0, 1.0),
            };
            let arc = match args.get(2) {
                Some(&a) => match list(h, a) {
                    Some(&[a0, a1]) => Some((scalar(h, a0, 3)?, scalar(h, a1, 3)?)),
                    _ => return Err(Bad::Arg(3)),
                },
                None => None,
            };
            Shape::Ellipse { c, r, arc, filled: name == "disk" }
        }
        ("text", [e, p, rest @ ..]) if rest.len() <= 1 => {
            let at = point(h, *p, 2)?;
            let offset = match rest.first() {
                Some(&o) => match list(h, o) {
                    Some(&[a, b]) => (scalar(h, a, 3)?, scalar(h, b, 3)?),
                    _ => return Err(Bad::Arg(3)),
                },
                None => (0.0, 0.0),
            };
            Shape::Text { label: Label::of(h, *e), at, offset }
        }
        ("raster", [c, rest @ ..]) if rest.len() <= 2 => {
            // a third argument [lo, hi] is the range of the values (gray from lo to hi)
            let span = rest.get(1).and_then(|&g| match list(h, g) {
                Some(&[a, b]) => Some((number(h, a)?, number(h, b)?)).filter(|(a, b)| b > a),
                _ => None,
            });
            let grad = match (rest.get(1), span) {
                (Some(_), Some(_)) => Gradient::named("gray").ok_or(Bad::Arg(3))?,
                (Some(&g), None) => gradient::read(h, g).ok_or(Bad::Arg(3))?,
                (None, _) => Gradient::named("gray").ok_or(Bad::Arg(3))?,
            };
            let (cols, rows, rgba) = match span {
                Some((a, b)) => scaled_cells(h, *c, &grad, a, b)?,
                None => cells(h, *c, &grad)?,
            };
            let (from, to) = match rest.first() {
                Some(&r) => match list(h, r) {
                    Some(&[p, q]) => {
                        let (a, b) = (point(h, p, 2)?, point(h, q, 2)?);
                        if a.scaled != b.scaled || a.x == b.x || a.y == b.y {
                            return Err(Bad::Arg(2));
                        }
                        let lo = Pt { x: a.x.min(b.x), y: a.y.min(b.y), scaled: a.scaled };
                        (lo, Pt { x: a.x.max(b.x), y: a.y.max(b.y), scaled: a.scaled })
                    }
                    _ if is_symbolic(h, r) => return Err(Bad::Symbolic),
                    _ => return Err(Bad::Arg(2)),
                },
                None => (origin, Pt { x: cols as f64, y: rows as f64, scaled: false }),
            };
            Shape::Raster { cols, rows, rgba, from, to }
        }
        _ if PRIMITIVES.contains(&name) => return Err(Bad::Arity),
        _ => return Ok(None),
    };
    Ok(Some(s))
}

fn is_rule<S: Store + ?Sized>(h: &S, t: T<'_>) -> bool {
    head_name(h, t).is_some_and(|(n, a)| n == names::RULE && a.len() == 2)
}

/// The rings of `outer -> [hole, ...]`: rings of positions, or of 1-based indices into `pts`; the outer ring
/// anticlockwise and the holes clockwise, so that filling cuts the holes out.
fn with_holes<S: Store + ?Sized>(h: &S, pts: Option<Vec<Pt>>, r: T<'_>) -> Result<Vec<Vec<Pt>>, Bad> {
    let Some((_, &[outer, holes])) = head_name(h, r) else { return Err(Bad::Arg(2)) };
    let ring = |t: T<'_>| -> Result<Vec<Pt>, Bad> {
        match (&pts, list(h, t)) {
            (Some(pts), Some(items)) if items.iter().all(|&i| matches!(h.view(i), View::Int(_))) => items
                .iter()
                .map(|&i| match h.view(i) {
                    View::Int(k) if k >= 1 && (k as usize) <= pts.len() => Ok(pts[k as usize - 1]),
                    _ => Err(Bad::Arg(2)),
                })
                .collect(),
            _ => points(h, t, 2),
        }
    };
    let area = |r: &[Pt]| -> f64 {
        (0..r.len())
            .map(|i| {
                let (a, b) = (r[i], r[(i + 1) % r.len()]);
                a.x * b.y - b.x * a.y
            })
            .sum()
    };
    let mut o = ring(outer)?;
    if area(&o) < 0.0 {
        o.reverse();
    }
    let mut rings = vec![o];
    for &hl in list(h, holes).ok_or(Bad::Arg(2))? {
        let mut q = ring(hl)?;
        if area(&q) > 0.0 {
            q.reverse();
        }
        rings.push(q);
    }
    Ok(rings)
}

/// The most cells of a `raster`.
pub const MAX_CELLS: usize = 4_000_000;

/// The cells of a raster: a matrix (rows from the bottom up) of numbers in [0, 1] (coloured by `grad`), colours
/// (`[r, g, b]`, `[r, g, b, a]` or colour terms) or `none` (transparent); as columns, rows and RGBA bytes with the
/// top row first.
fn cells<S: Store + ?Sized>(h: &S, t: T<'_>, grad: &Gradient) -> Result<(usize, usize, Vec<u8>), Bad> {
    let value = |v: f64| grad.at(v).bytes();
    let quad = |c: &[f64]| Rgba { r: c[0], g: c[1], b: c[2], a: c.get(3).copied().unwrap_or(1.0) }.bytes();
    // packed arrays: numbers (rank 2) or colours (rank 3, last dimension 3 or 4)
    let packed = match h.packed(t) {
        Some(Packed::Reals { dims, data }) => Some((dims, data.to_vec())),
        Some(Packed::Ints { dims, data }) => Some((dims, data.iter().map(|&v| v as f64).collect())),
        None => None,
    };
    if let Some((dims, data)) = packed {
        let (rows, cols) = match *dims {
            [r, c] | [r, c, 3] | [r, c, 4] => (r as usize, c as usize),
            _ => return Err(Bad::Arg(1)),
        };
        if rows.saturating_mul(cols) > MAX_CELLS {
            return Err(Bad::Arg(1));
        }
        let per = data.len() / (rows * cols);
        let mut rgba = Vec::with_capacity(rows * cols * 4);
        for r in (0..rows).rev() {
            for c in 0..cols {
                let at = (r * cols + c) * per;
                let cell = &data[at..at + per];
                rgba.extend_from_slice(&if per == 1 { value(cell[0]) } else { quad(cell) });
            }
        }
        return Ok((cols, rows, rgba));
    }
    let rows_t = match list(h, t) {
        Some(r) if !r.is_empty() => r,
        None if is_symbolic(h, t) => return Err(Bad::Symbolic),
        _ => return Err(Bad::Arg(1)),
    };
    let mut rows: Vec<&[T<'_>]> = Vec::with_capacity(rows_t.len());
    for &r in rows_t {
        match list(h, r) {
            Some(cs) if !cs.is_empty() && rows.first().is_none_or(|f| f.len() == cs.len()) => rows.push(cs),
            _ => return Err(Bad::Arg(1)),
        }
    }
    let cols = rows[0].len();
    if rows.len().saturating_mul(cols) > MAX_CELLS {
        return Err(Bad::Arg(1));
    }
    let mut rgba = Vec::with_capacity(rows.len() * cols * 4);
    for r in rows.iter().rev() {
        for &c in r.iter() {
            let px = if let Some(v) = number(h, c) {
                value(v)
            } else if sym_is(h, c, "none") {
                [0, 0, 0, 0]
            } else if let Some(parts) = list(h, c) {
                let v: Vec<f64> = parts.iter().map(|&p| number(h, p)).collect::<Option<_>>().ok_or(Bad::Arg(1))?;
                if !matches!(v.len(), 3 | 4) {
                    return Err(Bad::Arg(1));
                }
                quad(&v)
            } else {
                match colour(h, c) {
                    Colour::Value(c) => c.bytes(),
                    Colour::Symbolic => return Err(Bad::Symbolic),
                    Colour::Bad if is_symbolic(h, c) => return Err(Bad::Symbolic),
                    Colour::Bad => return Err(Bad::Arg(1)),
                }
            };
            rgba.extend_from_slice(&px);
        }
    }
    Ok((cols, rows.len(), rgba))
}

/// The cells of a raster of numbers in [lo, hi] (each scaled to [0, 1]).
fn scaled_cells<S: Store + ?Sized>(h: &S, t: T<'_>, grad: &Gradient, lo: f64, hi: f64) -> Result<(usize, usize, Vec<u8>), Bad> {
    let rows_t = list(h, t).ok_or(Bad::Arg(1))?;
    let mut rows = vec![];
    for &r in rows_t {
        let vals: Vec<f64> = match h.packed(r) {
            Some(Packed::Reals { data, .. }) => data.to_vec(),
            Some(Packed::Ints { data, .. }) => data.iter().map(|&v| v as f64).collect(),
            None => list(h, r).ok_or(Bad::Arg(1))?.iter().map(|&e| number(h, e).unwrap_or(f64::NAN)).collect(),
        };
        rows.push(vals);
    }
    let cols = rows.first().map_or(0, |r| r.len());
    if cols == 0 || rows.iter().any(|r| r.len() != cols) || rows.len().saturating_mul(cols) > MAX_CELLS {
        return Err(Bad::Arg(1));
    }
    let mut rgba = Vec::with_capacity(rows.len() * cols * 4);
    for r in rows.iter().rev() {
        for &v in r {
            rgba.extend_from_slice(&if v.is_finite() { grad.at((v - lo) / (hi - lo)).bytes() } else { [0, 0, 0, 0] });
        }
    }
    Ok((cols, rows.len(), rgba))
}

/// Dash patterns by name, in points for a line 2 points wide.
fn named_dashes(name: &str) -> Option<Vec<Size>> {
    let p: &[f64] = match name {
        "solid" => &[],
        "dashed" => &[8.0, 5.0],
        "dotted" => &[0.5, 4.5],
        "dot_dashed" => &[8.0, 4.0, 0.5, 4.0],
        _ => return None,
    };
    Some(p.iter().map(|&v| Size::Abs(v)).collect())
}

/// The directives that are symbols (besides the colours): `thick`, `thin`, `dashed`, `dotted`, `dot_dashed`, and
/// the text styles `bold`, `italic`, `plain`, `underlined` (accepted; labels keep the math font's shapes).
/// `false` for any other symbol.
pub fn symbol_directive(name: &str, style: &mut Style) -> bool {
    match name {
        "thick" => style.thickness = Size::Abs(2.0),
        "thin" => style.thickness = Size::Abs(0.5),
        "dashed" | "dotted" | "dot_dashed" => style.dashing = named_dashes(name).unwrap_or_default(),
        "bold" | "italic" | "plain" | "underlined" => {}
        _ => return false,
    }
    true
}

/// The symbol directives, for the documentation and name lookup.
pub const SYMBOL_DIRECTIVES: [&str; 9] = ["thick", "thin", "dashed", "dotted", "dot_dashed", "bold", "italic", "plain", "underlined"];

/// A size given by name (`tiny`, `small`, `medium`, `large`) for the directive `kind`.
fn named_size<S: Store + ?Sized>(h: &S, t: T<'_>, kind: &str) -> Option<Size> {
    let View::Sym(s) = h.view(t) else { return None };
    let k = match h.sym_name(s).as_str() {
        "tiny" => 0,
        "small" => 1,
        "medium" => 2,
        "large" => 3,
        _ => return None,
    };
    Some(match kind {
        "thickness" | "absolute_thickness" => Size::Abs([0.25, 0.5, 1.0, 2.0][k]),
        "point_size" | "absolute_point_size" => Size::Abs([2.0, 3.0, 4.5, 6.5][k]),
        "dashing" | "absolute_dashing" => Size::Abs([1.0, 2.0, 4.0, 8.0][k]),
        "arrowheads" => Size::Rel([0.015, 0.025, 0.04, 0.06][k]),
        "font_size" => Size::Abs([7.0, 9.0, 12.0, 16.0][k]),
        _ => return None,
    })
}

/// A text option inside `directive(...)` or `style(...)`: `font_size -> n`, `font_color -> c`; others are not used.
fn text_rule<S: Store + ?Sized>(h: &S, k: T<'_>, v: T<'_>, style: &mut Style) {
    if sym_is(h, k, "font_size") {
        if let Some(n) = number(h, v).filter(|n| *n > 0.0 && *n <= 1000.0) {
            style.font_size = Some(n);
        }
    } else if sym_is(h, k, "font_color") {
        if let Colour::Value(c) = colour(h, v) {
            style.color = c;
        }
    }
}

/// A length argument: a non-negative number.
fn length<S: Store + ?Sized>(h: &S, t: T<'_>, arg: usize) -> Result<f64, Bad> {
    let v = scalar(h, t, arg)?;
    if v >= 0.0 {
        Ok(v)
    } else {
        Err(Bad::Arg(arg))
    }
}

/// Apply the directive `t` to `style`. `Ok(false)` when `t` is not a directive (nor a colour).
pub fn directive<S: Store + ?Sized>(h: &S, t: T<'_>, style: &mut Style) -> Result<bool, Bad> {
    if let View::Sym(s) = h.view(t) {
        return match colour(h, t) {
            Colour::Value(c) => {
                style.color = c;
                Ok(true)
            }
            _ => Ok(symbol_directive(&h.sym_name(s), style)),
        };
    }
    let Some((name, args)) = head_name(h, t) else { return Ok(false) };
    if name == "directive" {
        // several directives as one; what is not a directive (a font option, a number) styles text or nothing
        for &d in args {
            if let Some(v) = number(h, d) {
                if v > 0.0 && v <= 1000.0 {
                    style.font_size = Some(v);
                }
                continue;
            }
            if let Some((r, &[k, v])) = head_name(h, d).filter(|(n, _)| n == names::RULE) {
                let _ = r;
                text_rule(h, k, v, style);
                continue;
            }
            if let Some((_, rs)) = head_name(h, d).filter(|(n, _)| n == "options") {
                for &r in rs {
                    if let Some((_, &[k, v])) = head_name(h, r).filter(|(n, _)| n == names::RULE) {
                        text_rule(h, k, v, style);
                    }
                }
                continue;
            }
            match directive(h, d, style) {
                Ok(_) => {}
                Err(Bad::Symbolic) => return Err(Bad::Symbolic),
                Err(_) => {}
            }
        }
        return Ok(true);
    }
    match colour(h, t) {
        Colour::Value(c) => {
            style.color = c;
            return Ok(true);
        }
        Colour::Symbolic => return Err(Bad::Symbolic),
        Colour::Bad if crate::color::HEADS.contains(&name.as_str()) => return Err(Bad::Arity),
        Colour::Bad => {}
    }
    let kind = name.clone();
    let one = |rel: bool| -> Result<Size, Bad> {
        match args {
            [v] => match named_size(h, *v, &kind) {
                Some(s) => Ok(s),
                None => length(h, *v, 1).map(|v| if rel { Size::Rel(v) } else { Size::Abs(v) }),
            },
            _ => Err(Bad::Arity),
        }
    };
    match name.as_str() {
        "opacity" => match args {
            [a] => style.opacity = scalar(h, *a, 1)?.clamp(0.0, 1.0),
            [a, c] => {
                style.opacity = scalar(h, *a, 1)?.clamp(0.0, 1.0);
                match colour(h, *c) {
                    Colour::Value(c) => style.color = c,
                    Colour::Symbolic => return Err(Bad::Symbolic),
                    Colour::Bad => return Err(Bad::Arg(2)),
                }
            }
            _ => return Err(Bad::Arity),
        },
        "thickness" => style.thickness = one(true)?,
        "arrowheads" => {
            let [v, ..] = args else { return Err(Bad::Arity) };
            let size = |e: T<'_>| -> Result<f64, Bad> {
                match named_size(h, e, "arrowheads") {
                    Some(Size::Rel(s)) => Ok(s),
                    _ => scalar(h, e, 1).map(f64::abs),
                }
            };
            style.arrowheads = match list(h, *v) {
                Some(&[a, b]) if list(h, a).is_none() && list(h, b).is_none() => Some([size(a)?, size(b)?]),
                // [[s, position, shape], ...]: the sizes at the start and the end
                Some(heads) if !heads.is_empty() && heads.iter().all(|&e| list(h, e).is_some()) => {
                    let mut ends = [0.0, 0.0];
                    for &hd in heads {
                        let parts = list(h, hd).unwrap_or_default();
                        let s = parts.first().map(|&e| size(e)).transpose()?.unwrap_or(0.04);
                        let at = parts.get(1).and_then(|&p| number(h, p)).unwrap_or(1.0);
                        ends[usize::from(at > 0.5)] = s;
                    }
                    Some(ends)
                }
                Some(_) => return Err(Bad::Arg(1)),
                None if sym_is(h, *v, "automatic") => None,
                None => {
                    let s = size(*v)?;
                    // a negative size turns the head round: at the start
                    if number(h, *v).is_some_and(|x| x < 0.0) {
                        Some([s, 0.0])
                    } else {
                        Some([0.0, s])
                    }
                }
            };
        }
        "absolute_thickness" => style.thickness = one(false)?,
        "point_size" => style.point_size = one(true)?,
        "absolute_point_size" => style.point_size = one(false)?,
        "font_size" => {
            style.font_size = Some(match one(false)? {
                Size::Abs(v) if v > 0.0 => v,
                _ => return Err(Bad::Arg(1)),
            })
        }
        "dashing" | "absolute_dashing" => {
            // an offset and a cap style after the pattern are accepted and not used
            let [v, ..] = args else { return Err(Bad::Arity) };
            let rel = name == "dashing";
            let sized = |d: f64| if rel { Size::Rel(d) } else { Size::Abs(d) };
            style.dashing = match (h.view(*v), list(h, *v)) {
                (View::Sym(s), _) => match named_dashes(&h.sym_name(s)) {
                    Some(d) => d,
                    None => match named_size(h, *v, "dashing") {
                        Some(d) => vec![d, d],
                        None => return Err(Bad::Arg(1)),
                    },
                },
                _ if number(h, *v).is_some() => {
                    let d = length(h, *v, 1)?;
                    vec![sized(d), sized(d)]
                }
                (_, Some(items)) => items
                    .iter()
                    .map(|&d| length(h, d, 1).map(|d| if rel { Size::Rel(d) } else { Size::Abs(d) }))
                    .collect::<Result<_, _>>()?,
                _ => return Err(Bad::Arg(1)),
            };
        }
        "edge_form" => {
            style.edge = match args {
                [] => None,
                [v] if sym_is(h, *v, "none") => None,
                [v] => {
                    let mut s = Style::default();
                    if !directives(h, *v, &mut s)? {
                        return Err(Bad::Arg(1));
                    }
                    Some(Edge { color: s.ink(), thickness: s.thickness, dashing: s.dashing })
                }
                _ => return Err(Bad::Arity),
            }
        }
        "face_form" => {
            style.face = match args {
                [] => Face::None,
                [v] if sym_is(h, *v, "none") => Face::None,
                [v] => {
                    let mut s = Style { opacity: style.opacity, ..Style::default() };
                    if !directives(h, *v, &mut s)? {
                        return Err(Bad::Arg(1));
                    }
                    Face::Color(s.ink())
                }
                _ => return Err(Bad::Arity),
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Apply a directive or a list of directives; `Ok(false)` when something in it is not a directive.
fn directives<S: Store + ?Sized>(h: &S, t: T<'_>, style: &mut Style) -> Result<bool, Bad> {
    match list(h, t) {
        Some(items) => {
            for &d in items {
                if !directive(h, d, style)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        None => directive(h, t, style),
    }
}

/// A failure for a primitive or directive that cannot be drawn.
fn bad_item<S: Store + ?Sized>(h: &S, t: T<'_>, bad: Bad) -> Failure {
    let printed = mc_syntax::Printer::new(h).expr(t);
    match bad {
        Bad::Symbolic => Failure::new("not_numeric", printed),
        _ => Failure::new("not_graphics", printed),
    }
}

/// Polls `stop` every so many items of a loop over the input.
pub const POLL: usize = 4096;

/// Read a `graphics(content)` or `graphics(content, options(...))` term. `stop` is polled in loops over the input.
pub fn read<S: Store + ?Sized>(h: &S, t: T<'_>, stop: &dyn Fn() -> bool) -> Result<Graphic, Failure> {
    if crate::three::is_graphics3d(h, t) {
        return crate::three::read(h, t, stop);
    }
    let (content, opts) = match head_name(h, t) {
        Some((n, args)) if n == "graphics" => match args {
            [c] => (*c, None),
            [c, o] => (*c, Some(*o)),
            _ => return Err(Failure::new("not_graphics", "graphics takes content and options")),
        },
        _ => return Err(Failure::new("not_graphics", mc_syntax::Printer::new(h).expr(t))),
    };
    let mut g = Graphic { items: Vec::new(), styles: vec![Style::default()], opts: Opts::default() };
    // the content, with the options `prolog` (drawn first) and `epilog` (drawn last)
    let extra = |key: &str| -> Option<T<'_>> {
        let o = opts?;
        let (_, rs) = head_name(h, o).filter(|(n, _)| n == "options")?;
        rs.iter().find_map(|&r| match head_name(h, r) {
            Some((n, &[k, v])) if n == names::RULE && sym_is(h, k, key) => Some(v),
            _ => None,
        })
    };
    let seq: Vec<T<'_>> = extra("prolog").into_iter().chain([content]).chain(extra("epilog")).collect();
    // a stack of (remaining elements, the style index current in that list)
    let mut stack: Vec<(&[T<'_>], usize)> = vec![(&seq[..], 0)];
    let mut seen = 0usize;
    while let Some((rest, style)) = stack.last_mut() {
        let Some((&e, tail)) = rest.split_first() else {
            stack.pop();
            continue;
        };
        *rest = tail;
        let cur = *style;
        seen += 1;
        if seen.is_multiple_of(POLL) && stop() {
            return Err(Failure::new("interrupted", ""));
        }
        if let Some(items) = list(h, e) {
            stack.push((items, cur));
            continue;
        }
        if let Some((name, args)) = head_name(h, e) {
            if name == "graphics" || (TRANSPARENT.contains(&name.as_str()) && !args.is_empty()) {
                // a graphic inside a graphic: its content, its options ignored; a tooltip or annotation: what it
                // annotates
                stack.push((&args[..args.len().min(1)], cur));
                continue;
            }
            if name == "style" && !args.is_empty() {
                // the content in its own scope, styled by the rest
                let mut s = g.styles[cur].clone();
                styled(h, &args[1..], &mut s).map_err(|b| bad_item(h, e, b))?;
                g.styles.push(s);
                stack.push((&args[..1], g.styles.len() - 1));
                continue;
            }
            if name == "inset" && !args.is_empty() {
                g.items.push(Item { shape: inset(h, args, stop).map_err(|f| if f.tag == "interrupted" { f } else { bad_item(h, e, Bad::Arg(1)) })?, style: cur });
                continue;
            }
            if name == "text" && args.len() >= 2 {
                if let Some((_, inner)) = head_name(h, args[0]).filter(|(n, a)| n == "style" && !a.is_empty()) {
                    // a styled label: its colour and size for this text
                    let mut s = g.styles[cur].clone();
                    styled(h, &inner[1..], &mut s).map_err(|b| bad_item(h, e, b))?;
                    let Some(Shape::Text { at, offset, .. }) = shape(h, "text", args).map_err(|b| bad_item(h, e, b))? else {
                        continue;
                    };
                    g.styles.push(s);
                    let label = Label::of(h, inner[0]);
                    g.items.push(Item { shape: Shape::Text { label, at, offset }, style: g.styles.len() - 1 });
                    continue;
                }
            }
            if matches!(name.as_str(), "disk" | "circle") {
                // several centres: one primitive each
                if let Some(cs) = args.first().and_then(|&c| list(h, c)).filter(|_| several_centres(h, args)) {
                    for &c in cs {
                        let mut a = vec![c];
                        a.extend_from_slice(&args[1..]);
                        match shape(h, &name, &a) {
                            Ok(Some(shape)) => g.items.push(Item { shape, style: cur }),
                            Ok(None) => {}
                            Err(bad) => return Err(bad_item(h, e, bad)),
                        }
                    }
                    continue;
                }
            }
            match shape(h, &name, args) {
                Ok(Some(shape)) => {
                    g.items.push(Item { shape, style: cur });
                    continue;
                }
                Ok(None) => {}
                Err(bad) => return Err(bad_item(h, e, bad)),
            }
        }
        let mut s = g.styles[cur].clone();
        match directive(h, e, &mut s) {
            Ok(true) => {
                g.styles.push(s);
                if let Some(top) = stack.last_mut() {
                    top.1 = g.styles.len() - 1;
                }
            }
            Ok(false) => return Err(bad_item(h, e, Bad::Arg(1))),
            Err(bad) => return Err(bad_item(h, e, bad)),
        }
    }
    if let Some(o) = opts {
        g.opts = options(h, o, &mut g.styles)?;
    }
    Ok(g)
}

thread_local! {
    /// How deep insets are nested in the graphic being read.
    static INSET_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Insets nested deeper than this are left out.
const MAX_INSETS: usize = 8;

/// `inset(obj)`, `inset(obj, pos)`, `inset(obj, pos, offset)`, `inset(obj, pos, offset, width)`: a graphic (or a
/// label for anything else) at `pos` (default: the middle of the plot range).
fn inset<S: Store + ?Sized>(h: &S, args: &[T<'_>], stop: &dyn Fn() -> bool) -> Result<Shape, Failure> {
    let at = match args.get(1) {
        Some(&p) => point(h, p, 2).map_err(|_| Failure::new("argument", "inset position"))?,
        None => Pt { x: 0.5, y: 0.5, scaled: true },
    };
    let offset = match args.get(2).and_then(|&o| list(h, o)) {
        Some(&[a, b]) => (number(h, a).unwrap_or(0.0), number(h, b).unwrap_or(0.0)),
        _ => (0.0, 0.0),
    };
    let width = args.get(3).and_then(|&w| match list(h, w) {
        Some(&[w, _]) => number(h, w),
        _ => number(h, w),
    });
    let obj = args[0];
    if !crate::is_graphics(h, obj) {
        let inner = match head_name(h, obj) {
            Some((n, a)) if n == "style" && !a.is_empty() => a[0],
            _ => obj,
        };
        return Ok(Shape::Text { label: Label::of(h, inner), at, offset });
    }
    let depth = INSET_DEPTH.with(|d| d.get());
    if depth >= MAX_INSETS {
        return Err(Failure::new("too_large", "insets nested too deep"));
    }
    INSET_DEPTH.with(|d| d.set(depth + 1));
    let sub = read(h, obj, stop);
    INSET_DEPTH.with(|d| d.set(depth));
    let sub = sub?;
    let p = crate::page::plan(&sub, stop)?;
    Ok(Shape::Inset { graphic: Box::new(sub), natural: (p.width as f64, p.height as f64), at, offset, width })
}

/// Whether the first argument of `disk` or `circle` is a list of centres.
pub fn several_centres<S: Store + ?Sized>(h: &S, args: &[T<'_>]) -> bool {
    args.first().and_then(|&c| list(h, c)).is_some_and(|cs| cs.first().is_some_and(|&c| list(h, c).is_some()))
}

/// Wrappers drawn as their first argument (what they annotate).
pub const TRANSPARENT: [&str; 8] =
    ["tooltip", "annotation", "mouseover", "hyperlink", "status_area", "popup_window", "event_handler", "dynamic"];

/// The styles of `style(content, s1, s2, ...)`: directives, numbers (a font size), `font_size -> n`,
/// `font_color -> c`; other things (font families, weights) are not used.
pub fn styled<S: Store + ?Sized>(h: &S, specs: &[T<'_>], style: &mut Style) -> Result<(), Bad> {
    for &d in specs {
        if let Some(v) = number(h, d) {
            if v > 0.0 && v <= 1000.0 {
                style.font_size = Some(v);
            }
            continue;
        }
        if let Some(Size::Abs(v)) = named_size(h, d, "font_size") {
            style.font_size = Some(v);
            continue;
        }
        match head_name(h, d) {
            Some((n, &[k, v])) if n == names::RULE => text_rule(h, k, v, style),
            Some((n, rs)) if n == "options" => {
                for &r in rs {
                    if let Some((_, &[k, v])) = head_name(h, r).filter(|(n, _)| n == names::RULE) {
                        text_rule(h, k, v, style);
                    }
                }
            }
            _ => {
                if let Err(Bad::Symbolic) = directive(h, d, style) {
                    return Err(Bad::Symbolic);
                }
            }
        }
    }
    Ok(())
}

/// The rules of an `options(...)` term as (name, value) pairs.
pub fn rules<'h, S: Store + ?Sized>(h: &'h S, o: T<'h>) -> Result<Vec<(String, T<'h>)>, Failure> {
    let Some((n, args)) = head_name(h, o) else {
        return Err(Failure::new("option", mc_syntax::Printer::new(h).expr(o)));
    };
    if n != "options" {
        return Err(Failure::new("option", mc_syntax::Printer::new(h).expr(o)));
    }
    let mut out = Vec::with_capacity(args.len());
    for &r in args {
        match head_name(h, r) {
            Some((n, &[k, v])) if n == names::RULE => match h.view(k) {
                View::Sym(k) => out.push((h.sym_name(k), v)),
                _ => return Err(Failure::new("option", mc_syntax::Printer::new(h).expr(r))),
            },
            _ => return Err(Failure::new("option", mc_syntax::Printer::new(h).expr(r))),
        }
    }
    Ok(out)
}

/// Read the options; style lists in grid lines and legend entries are added to `styles`. An option nothing reads,
/// or a value an option does not take, is left out and named in [`Opts::ignored`] (harmless styling options are
/// left out silently).
pub fn options<S: Store + ?Sized>(h: &S, o: T<'_>, styles: &mut Vec<Style>) -> Result<Opts, Failure> {
    let mut opts = Opts::default();
    for (name, v) in rules(h, o)? {
        if HARMLESS.contains(&name.as_str()) || name == "epilog" || name == "prolog" {
            continue;
        }
        if !OPTIONS.contains(&name.as_str()) {
            opts.ignored.push(name);
            continue;
        }
        if option(h, &name, v, &mut opts, styles).is_none() {
            opts.ignored.push(format!("{name}: {}", mc_syntax::Printer::new(h).expr(v)));
        }
    }
    Ok(opts)
}

pub(crate) fn boolean<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<bool> {
    if sym_is(h, t, names::TRUE) {
        Some(true)
    } else if sym_is(h, t, names::FALSE) {
        Some(false)
    } else {
        None
    }
}

/// A label option value: `none` or a label.
pub(crate) fn label_opt<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<Label> {
    (!sym_is(h, t, "none")).then(|| Label::of(h, t))
}

/// A range `[a, b]` with numbers or `automatic` ends.
pub(crate) fn range<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<[Option<f64>; 2]> {
    if sym_is(h, t, "automatic") || sym_is(h, t, "all") {
        return Some([None, None]);
    }
    let &[a, b] = list(h, t)? else { return None };
    let end = |e: T<'_>| -> Option<Option<f64>> {
        if sym_is(h, e, "automatic") || sym_is(h, e, "all") {
            Some(None)
        } else {
            number(h, e).map(Some)
        }
    };
    let r = [end(a)?, end(b)?];
    match r {
        [Some(a), Some(b)] if a >= b => None,
        r => Some(r),
    }
}

/// One option; `None` for a value it does not take.
fn option<'h, S: Store + ?Sized>(h: &'h S, name: &str, v: T<'h>, o: &mut Opts, styles: &mut Vec<Style>) -> Option<()> {
    let pair = |v: T<'h>| -> Option<[T<'h>; 2]> {
        match list(h, v) {
            Some(&[a, b]) => Some([a, b]),
            _ => None,
        }
    };
    match name {
        "plot_range" => {
            o.plot_range = if sym_is(h, v, "full") {
                [[None, None], [None, None]]
            } else if let Some(r) = number(h, v) {
                (r > 0.0).then_some(())?;
                [[None, None], [Some(-r), Some(r)]]
            } else if let Some([x, y]) = pair(v).filter(|[x, _]| list(h, *x).is_some() || sym_is(h, *x, "automatic")) {
                [range(h, x)?, range(h, y)?]
            } else {
                [[None, None], range(h, v)?]
            }
        }
        "plot_range_padding" => {
            o.padding = if sym_is(h, v, "none") {
                Some(0.0)
            } else if sym_is(h, v, "automatic") || sym_is(h, v, "all") {
                None
            } else if let Some((n, &[f])) = head_name(h, v).filter(|(n, _)| n == "scaled") {
                let _ = n;
                Some(number(h, f).filter(|p| *p >= 0.0)?)
            } else {
                Some(number(h, v).filter(|p| *p >= 0.0)?)
            }
        }
        "axes_origin" => {
            o.origin = if sym_is(h, v, "automatic") {
                None
            } else {
                let [x, y] = pair(v)?;
                Some((number(h, y)?, number(h, x)?))
            }
        }
        "plot_range_clipping" => o.clip = Some(boolean(h, v)?),
        "legend_title" => o.legend_title = label_opt(h, v),
        "plot_legends" => {
            // a list of labels: a legend of line samples in the order of the content's colours is not known here,
            // so the entries are labels without samples
            let items = list(h, v)?;
            o.legend = items.iter().map(|&e| LegendEntry { label: Label::of(h, e), style: 0, marker: Marker::None }).collect();
        }
        "axes" => {
            o.axes = match boolean(h, v) {
                Some(b) => [b, b],
                None => {
                    let [x, y] = pair(v)?;
                    [boolean(h, x)?, boolean(h, y)?]
                }
            }
        }
        "frame" => {
            o.frame = match boolean(h, v) {
                Some(b) => b,
                // per side: [[left, right], [bottom, top]] or [bottom, left, top, right]: a frame when any side has one
                None if sym_is(h, v, "all") => true,
                None if sym_is(h, v, "none") => false,
                None => {
                    let mut any = false;
                    for &e in list(h, v)? {
                        match list(h, e) {
                            Some(sides) => any |= sides.iter().any(|&b| boolean(h, b) == Some(true)),
                            None => any |= boolean(h, e)?,
                        }
                    }
                    any
                }
            }
        }
        "axes_label" | "frame_label" => {
            let labels = match pair(v) {
                Some([x, y]) => [label_opt(h, x), label_opt(h, y)],
                None if name == "axes_label" => [None, label_opt(h, v)],
                None => [label_opt(h, v), None],
            };
            if name == "axes_label" {
                o.axes_label = labels;
            } else {
                o.frame_label = labels;
            }
        }
        "plot_label" => o.plot_label = label_opt(h, v),
        "aspect_ratio" => {
            o.aspect = if sym_is(h, v, "automatic") || sym_is(h, v, "all") {
                Aspect::Auto
            } else if sym_is(h, v, "full") {
                Aspect::Full
            } else {
                Aspect::Ratio(number(h, v).filter(|r| *r > 0.0)?)
            }
        }
        "image_size" => {
            let size = |t: T<'_>| -> Option<Option<f64>> {
                if sym_is(h, t, "automatic") || sym_is(h, t, "all") || sym_is(h, t, "full") {
                    Some(None)
                } else if let Some(w) = named_width(h, t) {
                    Some(Some(w))
                } else {
                    number(h, t).filter(|w| *w >= 1.0 && *w <= MAX_SIDE).map(Some)
                }
            };
            o.image_size = match pair(v) {
                Some([w, ht]) => (size(w)?, size(ht)?),
                None => (size(v)?, None),
            }
        }
        "background" => {
            o.background = if sym_is(h, v, "none") {
                None
            } else {
                match colour(h, v) {
                    Colour::Value(c) => Some(c),
                    _ => return None,
                }
            }
        }
        "ticks" | "frame_ticks" => {
            if let Some(b) = boolean(h, v) {
                o.ticks = if b { [Ticks::Auto, Ticks::Auto] } else { [Ticks::None, Ticks::None] };
                return Some(());
            }
            if sym_is(h, v, "all") {
                o.ticks = [Ticks::Auto, Ticks::Auto];
                return Some(());
            }
            // frame ticks per side, [[left, right], [bottom, top]]: the left and the bottom ones are drawn
            let v = match pair(v) {
                Some([a, b]) if name == "frame_ticks" && list(h, a).is_some_and(|l| l.len() == 2) && list(h, b).is_some_and(|l| l.len() == 2) => {
                    let (l, bt) = (list(h, a)?[0], list(h, b)?[0]);
                    o.ticks = [ticks(h, bt).unwrap_or(Ticks::Auto), ticks(h, l).unwrap_or(Ticks::Auto)];
                    return Some(());
                }
                _ => v,
            };
            // two specifications (`automatic`, `none` or lists) are one per axis; anything else is for both
            let spec = |e: T<'_>| sym_is(h, e, "automatic") || sym_is(h, e, "none") || list(h, e).is_some();
            o.ticks = match pair(v).filter(|[x, y]| spec(*x) && spec(*y)) {
                Some([x, y]) => [ticks(h, x)?, ticks(h, y)?],
                None => {
                    let t = ticks(h, v)?;
                    [t.clone(), t]
                }
            }
        }
        "grid_lines" => {
            o.grid = if sym_is(h, v, "none") || sym_is(h, v, names::FALSE) {
                [Grid::None, Grid::None]
            } else if sym_is(h, v, "automatic") || sym_is(h, v, "all") || sym_is(h, v, names::TRUE) {
                [Grid::Auto, Grid::Auto]
            } else {
                let [x, y] = pair(v)?;
                [grid(h, x, styles)?, grid(h, y, styles)?]
            }
        }
        "legend" => {
            o.legend = if sym_is(h, v, "none") { vec![] } else { legend(h, v, styles)? };
        }
        "legend_position" => {
            let View::Sym(s) = h.view(v) else { return None };
            o.legend_position = match h.sym_name(s).as_str() {
                "top_right" => Corner::TopRight,
                "top_left" => Corner::TopLeft,
                "bottom_left" => Corner::BottomLeft,
                "bottom_right" => Corner::BottomRight,
                _ => return None,
            }
        }
        "font_size" => o.font_size = number(h, v).filter(|s| *s > 0.0 && *s <= 1000.0)?,
        "color_bar" => {
            o.color_bar = if sym_is(h, v, "none") {
                None
            } else {
                let (g, r, l) = match *list(h, v)? {
                    [g, r] => (g, r, None),
                    [g, r, l] => (g, r, label_opt(h, l)),
                    _ => return None,
                };
                let &[a, b] = list(h, r)? else { return None };
                let (a, b) = (number(h, a)?, number(h, b)?);
                (a < b).then_some(())?;
                Some(ColorBar { gradient: gradient::read(h, g)?, range: (a, b), label: l })
            }
        }
        _ => return None,
    }
    Some(())
}

/// The largest image side in points.
pub const MAX_SIDE: f64 = 20_000.0;

/// The width of a named image size: `tiny`, `small`, `medium`, `large`.
pub(crate) fn named_width<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<f64> {
    let View::Sym(s) = h.view(t) else { return None };
    Some(match h.sym_name(s).as_str() {
        "tiny" => 100.0,
        "small" => 180.0,
        "medium" => 360.0,
        "large" => 576.0,
        _ => return None,
    })
}

/// Ticks of one axis: `automatic`, `none`, or a list of positions or `[position, label]` pairs.
fn ticks<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<Ticks> {
    if sym_is(h, t, "automatic") || sym_is(h, t, "all") || sym_is(h, t, names::TRUE) {
        return Some(Ticks::Auto);
    }
    if sym_is(h, t, "none") || sym_is(h, t, names::FALSE) {
        return Some(Ticks::None);
    }
    let items = list(h, t)?;
    let mut out = Vec::with_capacity(items.len());
    for &e in items {
        if let Some(u) = number(h, e) {
            out.push((u, Some(Label::plain(&short(u)))));
        } else {
            let &[u, l] = list(h, e)? else { return None };
            out.push((number(h, u)?, label_opt(h, l)));
        }
    }
    Some(Ticks::List(out))
}

/// Grid lines across one axis: `automatic`, `none`, or a list of positions or `[position, style]` pairs.
fn grid<S: Store + ?Sized>(h: &S, t: T<'_>, styles: &mut Vec<Style>) -> Option<Grid> {
    if sym_is(h, t, "automatic") || sym_is(h, t, "all") || sym_is(h, t, names::TRUE) {
        return Some(Grid::Auto);
    }
    if sym_is(h, t, "none") || sym_is(h, t, names::FALSE) {
        return Some(Grid::None);
    }
    let items = list(h, t)?;
    let mut out = Vec::with_capacity(items.len());
    for &e in items {
        if let Some(u) = number(h, e) {
            out.push((u, None));
        } else {
            let &[u, d] = list(h, e)? else { return None };
            let mut s = Style::default();
            directives(h, d, &mut s).ok().filter(|ok| *ok)?;
            styles.push(s);
            out.push((number(h, u)?, Some(styles.len() - 1)));
        }
    }
    Some(Grid::List(out))
}

/// Legend entries: each a label (no sample), `[style, label]` (a line sample) or `[style, label, marker]` with
/// `line`, `point`, `swatch` or `none`.
fn legend<S: Store + ?Sized>(h: &S, t: T<'_>, styles: &mut Vec<Style>) -> Option<Vec<LegendEntry>> {
    let items = list(h, t)?;
    let mut out = Vec::with_capacity(items.len());
    for &e in items {
        let Some(parts) = list(h, e) else {
            out.push(LegendEntry { label: Label::of(h, e), style: 0, marker: Marker::None });
            continue;
        };
        let (d, l, m) = match *parts {
            [d, l] => (d, l, Marker::Line),
            [d, l, m] => {
                let View::Sym(m) = h.view(m) else { return None };
                let marker = match h.sym_name(m).as_str() {
                    "line" => Marker::Line,
                    "point" => Marker::Point,
                    "swatch" => Marker::Swatch,
                    "none" => Marker::None,
                    _ => return None,
                };
                (d, l, marker)
            }
            _ => return None,
        };
        let mut s = Style::default();
        directives(h, d, &mut s).ok().filter(|ok| *ok)?;
        styles.push(s);
        out.push(LegendEntry { label: Label::of(h, l), style: styles.len() - 1, marker: m });
    }
    Some(out)
}
