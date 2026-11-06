//! Plots: `plot` and `list_plot` (and their logarithmic forms) built as `graphics` terms and rendered by
//! `mc-graphics`.
//!
//! A plot evaluates to `image("path")`, where `path` is a PNG file in the system's temporary folder
//! (`mathcas-plots/`), with the same plot as SVG and as vector PDF next to it (same name, `.svg` and `.pdf`); front
//! ends show the PNG (the Jupyter kernel displays it inline) and `export_data` copies the PDF. Functions are sampled
//! adaptively (64 even points, refined where the curve bends or stops) through `mc_numfn::compile_expr` (compiled
//! when it pays); a value that is not a finite real breaks the curve, and so does a jump larger than the visible
//! range (a pole). The y range is robust: the 1% and 99% quantiles of the sampled values, widened to the full range
//! when no value lies far outside them. Labels and legend entries take the graphics labels' formatting (`$...$`
//! math with scripts and Greek letters).
#![warn(missing_docs)]

mod anim;
mod charts;
mod curves;
mod fields;
mod figure;
mod more;
mod plots3d;
mod sidecar;

use figure::{Axis, Corner, Dash, Extra, Filling, Look, Rgb, Scale, Series};
use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::Format;
use mc_numfn::{NumFn, Options, Var};
use std::sync::atomic::{AtomicU64, Ordering};

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "plot",
        "plot(f, [x, a, b]): a PNG image of f for x from a to b (plot([f1, f2, ...], [x, a, b]) for several curves), with SVG and PDF copies next to it; evaluates to image(path)",
    ),
    (
        "list_plot",
        "list_plot(data): a PNG image of the points [y1, y2, ...] (at x = 1, 2, ...) or [[x1, y1], ...]; joined: true draws a line; SVG and PDF copies next to it; evaluates to image(path)",
    ),
    ("log_plot", "log_plot(f, [x, a, b]): plot with a logarithmic y axis (values that are not positive are left out)"),
    ("log_log_plot", "log_log_plot(f, [x, a, b]): plot with logarithmic x and y axes (0 < a < b)"),
    ("list_log_plot", "list_log_plot(data): list_plot with a logarithmic y axis"),
    ("list_log_log_plot", "list_log_log_plot(data): list_plot with logarithmic x and y axes"),
    ("log_linear_plot", "log_linear_plot(f, [x, a, b]): plot with a logarithmic x axis (0 < a < b)"),
    ("list_log_linear_plot", "list_log_linear_plot(data): list_plot with a logarithmic x axis"),
    (
        "x_scale",
        "option of every plot: x_scale: linear | log | signed_log | signed_log(c) (sign(v) log10(1 + |v|/c), default c = 1; linear near 0, logarithmic far from it, both signs)",
    ),
    ("y_scale", "option of every plot: y_scale: linear | log | signed_log | signed_log(c), as x_scale"),
    (
        "list_line_plot",
        "list_line_plot(data): list_plot with the points joined by lines; list_line_plot([data1, data2, ...]) several lines",
    ),
    (
        "parametric_plot",
        "parametric_plot([fx, fy], [t, a, b]): the curve (fx, fy) for t from a to b, equal units on both axes; parametric_plot([[fx, fy], [gx, gy], ...], [t, a, b]) several",
    ),
    (
        "polar_plot",
        "polar_plot(r, [theta, a, b]): the curve at distance r from the origin at angle theta, for theta from a to b; polar_plot([r1, r2, ...], ...) several",
    ),
    (
        "density_plot",
        "density_plot(f, [x, a, b], [y, c, d]): f as colours over the rectangle, with a colour bar (options color_function, color_bar, plot_points)",
    ),
    (
        "contour_plot",
        "contour_plot(f, [x, a, b], [y, c, d]): the regions between levels of f in colours with the level lines (options contours, color_function, color_bar, plot_points)",
    ),
    (
        "list_density_plot",
        "list_density_plot(matrix): the values of a matrix (rows from the bottom up) as colours, interpolated between the points (options data_range, color_function, color_bar)",
    ),
    (
        "histogram",
        "histogram(data): bars counting the values in bins of equal width, chosen automatically (options bins, normalized); histogram([data1, data2, ...]) several, overlaid",
    ),
    (
        "bar_chart",
        "bar_chart([v1, v2, ...]): a bar of height vk at k; bar_chart([[...], [...], ...]) groups of bars side by side (option labels)",
    ),
    (
        "filling",
        "option of plot, list_line_plot and joined list plots: filling: axis | bottom | top | c (to the line y = c) | [i, j] (between curves i and j), filled in each curve's colour, faint",
    ),
    (
        "color_function",
        "option of density_plot, contour_plot, list_density_plot: color_function: a gradient name (viridis, thermal, cool_warm, ocean, earth, rainbow, gray) or a list of colours",
    ),
    ("plot_points", "option of density_plot and contour_plot: plot_points: n, the function sampled on an n x n grid (default 100 and 80)"),
    ("contours", "option of contour_plot: contours: n (n levels evenly between the least and the greatest value, default 10) or a list of levels"),
    ("data_range", "option of list_density_plot: data_range: [[x0, x1], [y0, y1]], the coordinates of the matrix's first and last columns and rows"),
    (
        "bins",
        "option of histogram: bins: n (about n bins of a round width), [w] (width w), [lo, hi, w] (from lo to hi), automatic",
    ),
    ("normalized", "option of histogram: normalized: true scales the bars so that their area is 1 (a probability density)"),
    ("labels", "option of bar_chart: labels: [\"a\", \"b\", ...], the labels under the bars or groups"),
    (
        "plot3d",
        "plot3d(f, [x, a, b], [y, c, d]): the surface z = f(x, y) in perspective, lit, with mesh lines and axes; plot3d([f1, f2, ...], ...) several (options mesh, plot_points, color_function, plot_style, plot_range, view_point, box_ratios)",
    ),
    (
        "parametric_plot3d",
        "parametric_plot3d([fx, fy, fz], [u, a, b], [v, c, d]): a parametric surface; parametric_plot3d([fx, fy, fz], [u, a, b]): a space curve",
    ),
    ("spherical_plot3d", "spherical_plot3d(r, [theta, a, b], [phi, c, d]): the surface at distance r from the origin at polar angle theta and azimuth phi"),
    ("revolution_plot3d", "revolution_plot3d(f, [t, a, b]): the surface swept by the curve z = f(t), at distance t from the z axis, turned once about it"),
    ("list_plot3d", "list_plot3d(matrix): the surface of a matrix of heights (rows along y); list_plot3d([[x, y, z], ...]): a surface through scattered points"),
    ("list_point_plot3d", "list_point_plot3d([[x, y, z], ...]): points in space, with axes"),
    ("list_line_plot3d", "list_line_plot3d([[x, y, z], ...]): a line through points in space, with axes"),
    (
        "list_contour_plot",
        "list_contour_plot(matrix): contours of a matrix of values (rows from the bottom up); list_contour_plot([[x, y, f], ...]): of scattered values, interpolated (options contours, color_function, color_bar, data_range)",
    ),
    (
        "export_animation",
        "export_animation(\"file.gif\", [g1, g2, ...]): the graphics (or plots) as the frames of an animated GIF, looping (option frame_rate: frames per second, default 10); gives the path",
    ),
    ("re_im_plot", "re_im_plot(f, [x, a, b]): the real and the imaginary part of f plotted together"),
    ("abs_arg_plot", "abs_arg_plot(f, [x, a, b]): the modulus and the argument of f plotted together"),
    (
        "discrete_plot",
        "discrete_plot(f, [n, a, b]): the values of f at n = a, a + 1, ..., b as points with stems to the axis; [n, a, b, step] or [n, [values]] for other points; discrete_plot([f1, f2], ...) several",
    ),
    ("list_polar_plot", "list_polar_plot([r1, r2, ...]): the radii at equally spaced angles from 0; list_polar_plot([[theta, r], ...]): at the given angles"),
    (
        "complex_plot",
        "complex_plot(f, [z, z0, z1]): f over the rectangle with corners z0 and z1 by domain colouring: the hue is the argument (red for positive reals), dark near zeros, light near poles",
    ),
    ("complex_plot3d", "complex_plot3d(f, [z, z0, z1]): the modulus of f over the rectangle as a surface, coloured by the argument"),
    (
        "array_plot",
        "array_plot(m): a matrix as cells, row 1 at the top: numbers from 0 (white) to 1 (black), colours as they are, true black (options color_function, mesh)",
    ),
    ("matrix_plot", "matrix_plot(m): a matrix as cells coloured from blue (negative) through white to red (positive), with row and column numbers"),
    ("pie_chart", "pie_chart([v1, v2, ...]): a disk cut into sectors in proportion to the values, in the plot colours (options labels / chart_labels, legend)"),
    (
        "color_data",
        "color_data(name): a colour scheme as a function of t in [0, 1]; color_data(name, t) its colour at t; color_data(k, i): the i-th plot colour; color_data(name, \"ColorList\"): the scheme's colours",
    ),
    ("line_thickness", "option of plot and list plots: line_thickness: [w1, w2, ...], the width of each curve in points (cycled; default 2)"),
    ("mesh", "option of 3D plots: mesh: none | n (n mesh lines each way; default 15)"),
    ("plot_style", "option of every plot: plot_style: directives, or a list of them (one per curve or surface, cycled): colours, thick, dashed, ..."),
    ("plot_legends", "option of every plot: plot_legends: [\"a\", \"b\", ...] (labels or expressions), automatic for the plotted expressions; also line_legend([...]) and placed([...], pos)"),
    ("plot_labels", "option of plot: plot_labels: [l1, l2, ...], a label at the right end of each curve, in its colour"),
    ("x_label", "option of every plot: x_label: \"text\", the label under the x axis ($...$ for math: \"$E_\\\\gamma$ (GeV)\")"),
    ("y_label", "option of every plot: y_label: \"text\", the label beside the y axis (read upwards)"),
    (
        "colors",
        "option of every plot: colors: [\"#rrggbb\", ...], the colour of each curve or data set in order (the default cycle after the list)",
    ),
    (
        "line_style",
        "option of plot and joined list plots: line_style: [solid | dashed | dotted | dot_dashed, ...], one per curve in order (solid after the list)",
    ),
];

/// Register the natives with `k`.
pub fn install(k: &mut Kernel, h: &Heap) {
    let lin = Scale::default();
    let ylog = Scale {
        x: Axis::Linear,
        y: Axis::Log,
    };
    let xlog = Scale {
        x: Axis::Log,
        y: Axis::Linear,
    };
    let both = Scale {
        x: Axis::Log,
        y: Axis::Log,
    };
    k.register(
        h.sym("plot"),
        Attributes::default(),
        Box::new(Plot {
            name: "plot",
            scale: lin,
        }),
    );
    k.register(
        h.sym("log_plot"),
        Attributes::default(),
        Box::new(Plot {
            name: "log_plot",
            scale: ylog,
        }),
    );
    k.register(
        h.sym("log_log_plot"),
        Attributes::default(),
        Box::new(Plot {
            name: "log_log_plot",
            scale: both,
        }),
    );
    k.register(
        h.sym("list_plot"),
        Attributes::default(),
        Box::new(ListPlot {
            name: "list_plot",
            scale: lin,
        }),
    );
    k.register(
        h.sym("list_log_plot"),
        Attributes::default(),
        Box::new(ListPlot {
            name: "list_log_plot",
            scale: ylog,
        }),
    );
    k.register(
        h.sym("list_log_log_plot"),
        Attributes::default(),
        Box::new(ListPlot {
            name: "list_log_log_plot",
            scale: both,
        }),
    );
    k.register(
        h.sym("log_linear_plot"),
        Attributes::default(),
        Box::new(Plot {
            name: "log_linear_plot",
            scale: xlog,
        }),
    );
    k.register(
        h.sym("list_log_linear_plot"),
        Attributes::default(),
        Box::new(ListPlot {
            name: "list_log_linear_plot",
            scale: xlog,
        }),
    );
    k.register(h.sym("list_line_plot"), Attributes::default(), Box::new(ListPlot { name: "list_line_plot", scale: lin }));
    curves::install(k, h);
    fields::install(k, h);
    charts::install(k, h);
    plots3d::install(k, h);
    anim::install(k, h);
    more::install(k, h);
}

pub(crate) fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

pub(crate) fn bad<'h>(h: &'h Heap, f: &str, k: i64) -> Reply<'h> {
    Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(f)), h.int(k)]))
}

/// A machine real for a number or a closed numeric expression (through `n`).
pub(crate) fn real<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let direct = |t: T<'h>| match h.view(t) {
        View::Int(i) => Some(i as f64),
        View::Real(x) => Some(x),
        _ => None,
    };
    if let Some(x) = direct(t) {
        return Ok(Some(x).filter(|x| x.is_finite()));
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(direct(v).filter(|x| x.is_finite()))
}

/// The display hint for front ends: the PNG file an image term `image("path")` names and its size in pixels
/// (width, height), read from the file's header. `None` when `t` is not an image term or the file is not a
/// readable PNG; the value is then shown as text.
pub fn image_of<'h>(h: &'h Heap, t: T<'h>) -> Option<(String, u32, u32)> {
    let View::App { head, args } = h.view(t) else { return None };
    let (View::Sym(s), [path]) = (h.view(head), args) else { return None };
    let View::Str(path) = h.view(*path) else { return None };
    if h.sym_name(s) != "image" {
        return None;
    }
    let (w, ht) = png_size(std::path::Path::new(path))?;
    Some((path.to_string(), w, ht))
}

/// The width and height of the PNG file at `path`, from its signature and `IHDR` chunk.
pub fn png_size(path: &std::path::Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    png_header_size(&head)
}

/// The size in the first 24 bytes of a PNG file: the signature, then the `IHDR` chunk's length, type, width and
/// height (big-endian).
fn png_header_size(head: &[u8; 24]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if head[..8] != SIGNATURE || &head[12..16] != b"IHDR" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]);
    Some((be(16), be(20))).filter(|&(w, h)| w > 0 && h > 0)
}

/// The image term for a written PNG file.
fn image<'h>(h: &'h Heap, path: &str) -> T<'h> {
    h.call(h.sym("image"), &[h.string(path)])
}

/// The formats written for every plot, in the order of [`EXTENSIONS`].
const FORMATS: [Format; 3] = [Format::Png { scale: 1.0 }, Format::Svg, Format::Pdf];
/// The file extensions of [`FORMATS`].
const EXTENSIONS: [&str; 3] = ["png", "svg", "pdf"];

/// Build the plot's graphics term, render it to a fresh PNG file with the SVG and the vector PDF next to it (same
/// name: `export_data("f.pdf", plot)` copies the PDF), and give the PNG's image term (or a failure).
pub(crate) fn finish<'h>(cx: &mut NativeCx<'_, '_, 'h>, series: &[Series], look: &Look, extra: Extra<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    // writing a file is an effect: in program order, never speculatively
    cx.effect()?;
    if !look.ignored.is_empty() {
        let who = if look.who.is_empty() { "plot" } else { look.who };
        let text = format!("options not used: {}", look.ignored.join(", "));
        cx.emit(h.call(h.sym("message"), &[h.sym_term(h.sym(who)), h.string(&text)]))?;
    }
    // `plot_legends: automatic`: the series' names
    let auto;
    let look = if look.legend.first().is_some_and(|l| l == AUTO_LEGEND) {
        let mut l = look.clone();
        l.legend = series.iter().map(|s| s.name.clone()).collect();
        auto = l;
        &auto
    } else {
        look
    };
    let g = figure::figure(h, series, look, &extra);
    let stop = cx.stop_flag().clone();
    let rendered = mc_graphics::render_with_layout(h, g, &FORMATS, &|| stop.load(Ordering::Relaxed));
    let (files, layout) = match rendered {
        Ok(r) => r,
        Err(f) => {
            // an interrupt ends the evaluation as interrupts do; other failures are the value
            cx.tick(1)?;
            return Ok(Reply::Done(f.term(h)));
        }
    };
    let mut entries = entries(series, look, &layout);
    entries.extend(extra.entries);
    let json = sidecar::json(&layout, look.scale, &entries);
    Ok(Reply::Done(match write_files(&files, &json) {
        Ok(path) => {
            // remembered, so that show(...) and layouts can use the plot as a graphic
            mc_graphics::compose::remember(&path, mc_syntax::Printer::new(h).expr(g));
            image(h, &path)
        }
        Err(e) => failure(h, "io", &[h.string(&e)]),
    }))
}

/// A plot built as a graphics term `g` (2D or 3D) of its own: a message naming the options not used, then the
/// term rendered to a fresh PNG with SVG and PDF siblings and a data sidecar without series; the PNG's image
/// term (or a failure).
pub(crate) fn finish_graphic<'h>(cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>, who: &str, ignored: &[String]) -> Res<Reply<'h>> {
    let h = cx.heap();
    cx.effect()?;
    if !ignored.is_empty() {
        let text = format!("options not used: {}", ignored.join(", "));
        cx.emit(h.call(h.sym("message"), &[h.sym_term(h.sym(who)), h.string(&text)]))?;
    }
    let stop = cx.stop_flag().clone();
    let (files, layout) = match mc_graphics::render_with_layout(h, g, &FORMATS, &|| stop.load(Ordering::Relaxed)) {
        Ok(r) => r,
        Err(f) => {
            cx.tick(1)?;
            return Ok(Reply::Done(f.term(h)));
        }
    };
    let json = sidecar::json(&layout, Scale::default(), &[]);
    Ok(Reply::Done(match write_files(&files, &json) {
        Ok(path) => {
            mc_graphics::compose::remember(&path, mc_syntax::Printer::new(h).expr(g));
            image(h, &path)
        }
        Err(e) => failure(h, "io", &[h.string(&e)]),
    }))
}

/// The data sidecar's entries of a plot: each series in data coordinates, a line split where it breaks.
fn entries(series: &[Series], look: &Look, layout: &mc_graphics::Layout) -> Vec<sidecar::Entry> {
    let sc = look.scale;
    let data = |(x, y): (f64, f64)| (sc.x.from(x), sc.y.from(y));
    let span = if look.data { f64::INFINITY } else { layout.range[1].1 - layout.range[1].0 };
    let mut out = vec![];
    for (k, s) in series.iter().enumerate() {
        // the legend's text as a tooltip shows it (`$...$` math as plain characters)
        let name = look.legend.get(k).map(|l| mc_graphics::label::plain_text(l)).unwrap_or_else(|| s.name.clone());
        let color = look.color(k);
        let pieces = if look.is_joined(k) {
            // as drawn: a lone point between breaks is not part of the line
            figure::runs(&s.points, span).into_iter().filter(|r| r.len() > 1).collect()
        } else {
            vec![s.points.iter().filter_map(|&(x, y)| y.map(|y| (x, y))).collect()]
        };
        let kind = if look.is_joined(k) { sidecar::Kind::Line } else { sidecar::Kind::Points };
        for p in pieces {
            let points: Vec<(f64, f64)> = p.into_iter().map(data).filter(|p| p.0.is_finite() && p.1.is_finite()).collect();
            if !points.is_empty() {
                out.push(sidecar::Entry { name: name.clone(), color, kind, points });
            }
        }
    }
    out
}

/// A short name for an expression: its printed form, cut to 40 characters.
pub(crate) fn short_name<'h>(h: &'h Heap, t: T<'h>) -> String {
    let s = mc_syntax::Printer::new(h).expr(t);
    if s.chars().count() <= 40 {
        s
    } else {
        s.chars().take(39).chain(std::iter::once('…')).collect()
    }
}

/// Write rendered PNG, SVG and PDF bytes (in `FORMATS` order) and the data sidecar `json` as a fresh
/// `plot-<pid>-<n>.png` with its siblings (`.svg`, `.pdf`, `.json`) in the temporary folder; gives the PNG's path
/// with forward slashes (it prints without escapes on every system).
fn write_files(files: &[Vec<u8>], json: &str) -> Result<String, String> {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join("mathcas-plots");
    let path = dir.join(format!("plot-{}-{}.png", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // the siblings first: once the PNG exists, so do they
    std::fs::write(path.with_extension("json"), json).map_err(|e| e.to_string())?;
    for (bytes, ext) in files.iter().zip(EXTENSIONS).rev() {
        std::fs::write(path.with_extension(ext), bytes).map_err(|e| e.to_string())?;
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}

/// A `graphics(...)` value as files to show: rendered like a plot (PNG with SVG and PDF siblings); gives the PNG's
/// path and pixel size. `None` when `t` is not a graphics term or cannot be rendered (symbolic coordinates: it is
/// shown as a term).
pub fn render_to_files<'h>(h: &'h Heap, t: T<'h>, stop: &dyn Fn() -> bool) -> Option<(String, u32, u32)> {
    if !mc_graphics::is_graphics(h, t) {
        return None;
    }
    let (files, layout) = mc_graphics::render_with_layout(h, t, &FORMATS, stop).ok()?;
    let path = write_files(&files, &sidecar::json(&layout, Scale::default(), &[])).ok()?;
    let (w, ht) = png_size(std::path::Path::new(&path))?;
    Some((path, w, ht))
}

/// `plot(f, [x, a, b])` and its log-scale forms.
struct Plot {
    name: &'static str,
    scale: Scale,
}

impl Native for Plot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = self.name;
        let base = Look {
            scale: self.scale,
            joined: true,
            ..Look::default()
        };
        let (f, spec, look) = match args {
            [f, spec] => (f, spec, base),
            [f, spec, o] => match plot_options(cx, *o, base, CURVE_OPTIONS, name) {
                Some(look) => (f, spec, look),
                None => return Ok(bad(h, name, 3)),
            },
            _ => return Ok(Reply::Decline),
        };
        let scale = look.scale;
        let Some(&[xv, a, b]) = cx.as_list(*spec) else {
            return Ok(bad(h, name, 2));
        };
        let View::Sym(x) = h.view(xv) else {
            return Ok(bad(h, name, 2));
        };
        let (Some(a), Some(b)) = (real(cx, a)?, real(cx, b)?) else {
            return Ok(bad(h, name, 2));
        };
        // the sampled interval, on the x axis (logarithms on a log axis)
        let (Some(ua), Some(ub)) = (scale.x.to(a), scale.x.to(b)) else {
            return Ok(bad(h, name, 2));
        };
        if ua >= ub {
            return Ok(bad(h, name, 2));
        }
        let fs: Vec<T<'h>> = match cx.as_list(*f) {
            Some(fs) if !fs.is_empty() => fs.to_vec(),
            Some(_) => return Ok(bad(h, name, 1)),
            None => vec![*f],
        };
        let mut series = Vec::with_capacity(fs.len());
        for f in fs {
            let mut s = sample(cx, f, x, ua, ub, scale)?;
            s.name = short_name(h, f);
            series.push(s);
        }
        if let Some(Filling::Between(i, j)) = look.filling {
            if i.max(j) >= series.len() || i == j {
                return Ok(bad(h, name, 3));
            }
        }
        finish(cx, &series, &look, Extra::default())
    }
}

/// The curve of `f` in `x` on [a, b], sampled adaptively: `INITIAL` even points, then each interval is halved
/// while its midpoint lies more than `TOLERANCE` (as a fraction of the visible y range, about half a pixel) off the
/// chord, or while the value is a number at one end and not at the other (to find where it stops), up to `DEPTH`
/// halvings and `BUDGET` evaluations in all. `None` where the value is not a finite real.
/// The interval [a, b] and the points are on the axes (logarithms on log axes), so refinement adapts to the curve as
/// it is drawn.
fn sample<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    x: Sym,
    a: f64,
    b: f64,
    scale: Scale,
) -> Res<Series> {
    let fun = mc_numfn::compile_expr(cx, f, &[Var::scalar(x)], &Options::default())?;
    let mut s = Sampler {
        fun,
        evals: 0,
        scale,
    };
    let mut grid = Vec::with_capacity(INITIAL);
    for k in 0..INITIAL {
        let xk = a + (b - a) * k as f64 / (INITIAL - 1) as f64;
        grid.push((xk, s.at(cx, xk)?));
    }
    // the visible y range of the coarse curve sets the tolerance
    let span = {
        let (lo, hi) = figure::robust(grid.iter().filter_map(|p| p.1).collect());
        if hi > lo {
            hi - lo
        } else {
            1.0
        }
    };
    let mut pts = vec![grid[0]];
    for w in grid.windows(2) {
        s.refine(cx, w[0], w[1], span, DEPTH, &mut pts)?;
    }
    Ok(Series { points: pts, name: String::new() })
}

/// Even points before refinement.
const INITIAL: usize = 64;
/// Halvings of an initial interval at most.
const DEPTH: u32 = 10;
/// Evaluations per curve at most.
const BUDGET: u64 = 20_000;
/// Deviation from the chord, as a fraction of the visible y range, that asks for a halving.
const TOLERANCE: f64 = 0.0015;
/// A step between neighbouring samples after every halving, as a fraction of the visible y range, that is taken for
/// a jump (the curve is not drawn across it).
const JUMP: f64 = 0.05;

struct Sampler<'h> {
    fun: Box<dyn NumFn<'h, f64> + 'h>,
    evals: u64,
    scale: Scale,
}

impl<'h> Sampler<'h> {
    /// The curve at `u` on the x axis: the value on the y axis, `None` where there is none.
    fn at(&mut self, cx: &mut NativeCx<'_, '_, 'h>, u: f64) -> Res<Option<f64>> {
        self.evals += 1;
        if self.evals.is_multiple_of(64) {
            cx.tick(1)?;
        }
        let x = self.scale.x.from(u);
        let mut out = [0.0];
        let ok = self.fun.eval(cx, &[x], &mut out)?;
        Ok(if ok && out[0].is_finite() {
            self.scale.y.to(out[0])
        } else {
            None
        })
    }

    /// Push the points of (p, q], refined; `p` is already pushed. Recursion depth is at most `DEPTH`.
    fn refine(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        p: (f64, Option<f64>),
        q: (f64, Option<f64>),
        span: f64,
        depth: u32,
        out: &mut Vec<(f64, Option<f64>)>,
    ) -> Res<()> {
        if depth > 0 && self.evals < BUDGET {
            let xm = 0.5 * (p.0 + q.0);
            let m = (xm, self.at(cx, xm)?);
            let split = match (p.1, m.1, q.1) {
                (Some(a), Some(c), Some(b)) => (c - 0.5 * (a + b)).abs() > TOLERANCE * span,
                (None, None, None) => false,
                _ => true,
            };
            if split {
                self.refine(cx, p, m, span, depth - 1, out)?;
                return self.refine(cx, m, q, span, depth - 1, out);
            }
            out.push(m);
        } else if let (0, Some(a), Some(b)) = (depth, p.1, q.1) {
            // halved as far as allowed and still a step: a jump, where the line breaks
            if (b - a).abs() > JUMP * span {
                out.push((0.5 * (p.0 + q.0), None));
            }
        }
        out.push(q);
        Ok(())
    }
}

/// `list_plot(data)`, `list_plot(data, joined: true)`, `list_line_plot(data)` and the log-scale forms; `data` may be
/// several data sets.
struct ListPlot {
    name: &'static str,
    scale: Scale,
}

/// The options of curves: scales, dash patterns, filling.
const CURVE_OPTIONS: &[&str] = &["x_scale", "y_scale", "line_style", "filling"];
/// The options of list plots: those of curves and `joined`.
const LIST_OPTIONS: &[&str] = &["x_scale", "y_scale", "line_style", "filling", "joined"];

/// The data sets of list plot data: several when it is a list of lists none of which is a pair of numbers'
/// shape (`[[1, 2, 3], [4, 5, 6]]`, `[[[0, 1], [1, 2]], [[0, 2], [1, 3]]]`), else one.
pub(crate) fn data_sets<'h>(cx: &NativeCx<'_, '_, 'h>, data: T<'h>) -> Option<Vec<T<'h>>> {
    let h = cx.heap();
    let items = cx.as_list(data)?;
    let several = !items.is_empty()
        && items.iter().all(|&it| cx.as_list(it).is_some())
        && items.iter().any(|&it| {
            let inner = cx.as_list(it).unwrap_or(&[]);
            inner.len() != 2 || inner.iter().any(|&e| cx.as_list(e).is_some())
        });
    let _ = h;
    Some(if several { items.to_vec() } else { vec![data] })
}

/// Data without labels: `data -> labels` is the data, an association its values, an item `v -> label` the value
/// v (in lists and in lists of data sets).
pub(crate) fn unlabelled<'h>(cx: &mut NativeCx<'_, '_, 'h>, data: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let head_is = |t: T<'h>, n: &str| matches!(h.view(t), View::App { head, .. } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == n));
    let mut d = data;
    if head_is(d, names::RULE) {
        if let View::App { args: &[l, _], .. } = h.view(d) {
            d = l;
        }
    }
    if head_is(d, "assoc") {
        d = cx.eval(h.call(h.sym("values"), &[d]))?;
    }
    let Some(items) = cx.as_list(d) else { return Ok(d) };
    if !items.iter().any(|&e| head_is(e, names::RULE) || head_is(e, "assoc") || cx.as_list(e).is_some_and(|l| l.iter().any(|&x| head_is(x, names::RULE)))) {
        return Ok(d);
    }
    let items = items.to_vec();
    let mut out = Vec::with_capacity(items.len());
    for e in items {
        out.push(if head_is(e, names::RULE) {
            match h.view(e) {
                View::App { args: &[l, _], .. } => l,
                _ => e,
            }
        } else if cx.as_list(e).is_some() || head_is(e, "assoc") {
            unlabelled(cx, e)?
        } else {
            e
        });
    }
    Ok(cx.list(&out))
}

/// Points on the axes; `None` where there is no value.
type Points = Vec<(f64, Option<f64>)>;

/// The points of one data set: `[y1, y2, ...]` (at x = 1, 2, ...) or `[[x1, y1], ...]`, on the axes of `sc`
/// (a point whose x has no place on a log axis is left out). `None` for data of another shape.
pub(crate) fn data_points<'h>(cx: &mut NativeCx<'_, '_, 'h>, data: T<'h>, sc: Scale) -> Res<Option<Points>> {
    let Some(items) = cx.as_list(data) else { return Ok(None) };
    let mut pts = Vec::with_capacity(items.len());
    for (i, &it) in items.iter().enumerate() {
        if i % 4096 == 0 {
            cx.tick(1)?;
        }
        let p = match cx.as_list(it) {
            Some(&[px, py]) => (real(cx, px)?, real(cx, py)?),
            Some(_) => return Ok(None),
            None => (Some(i as f64 + 1.0), real(cx, it)?),
        };
        match p {
            (Some(px), y) => {
                if let Some(u) = sc.x.to(px) {
                    pts.push((u, y.and_then(|y| sc.y.to(y))));
                }
            }
            (None, _) => return Ok(None),
        }
    }
    Ok(Some(pts))
}

impl Native for ListPlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = self.name;
        let base = Look {
            scale: self.scale,
            joined: name == "list_line_plot",
            data: true,
            ..Look::default()
        };
        let allowed = if name == "list_line_plot" { CURVE_OPTIONS } else { LIST_OPTIONS };
        let (data, look) = match args {
            [d] => (*d, base),
            [d, o] => match plot_options(cx, *o, base, allowed, name) {
                Some(look) => (*d, look),
                None => return Ok(bad(h, name, 2)),
            },
            _ => return Ok(Reply::Decline),
        };
        let data = unlabelled(cx, data)?;
        let Some(sets) = data_sets(cx, data) else {
            return Ok(bad(h, name, 1));
        };
        let many = sets.len() > 1;
        let mut series = Vec::with_capacity(sets.len());
        for (k, set) in sets.into_iter().enumerate() {
            let Some(points) = data_points(cx, set, look.scale)? else {
                return Ok(bad(h, name, 1));
            };
            let name = if many { format!("data {}", k + 1) } else { "data".into() };
            series.push(Series { points, name });
        }
        if series.iter().all(|s| s.points.iter().all(|p| p.1.is_none())) {
            return Ok(bad(h, name, 1));
        }
        if let Some(Filling::Between(i, j)) = look.filling {
            if i.max(j) >= series.len() || i == j {
                return Ok(bad(h, name, 2));
            }
        }
        finish(cx, &series, &look, Extra::default())
    }
}

/// The plot options `options(k -> v, ...)` applied to `base`; `None` when `o` is not an options term. Every plot
/// takes `x_label`, `y_label`, `legend` / `plot_legends`, `legend_position`, `colors`, `plot_style`, `plot_range`,
/// `plot_labels`, `frame`, `axes` and the options of the graphic (`plot_label`, `ticks`, `frame_ticks`, `grid_lines`,
/// `axes_label`, `frame_label`, `image_size`, `aspect_ratio`, `background`, `epilog`, `prolog`, ...); `extra`
/// names the others it takes: `x_scale`, `y_scale`, `line_style`, `joined`, `filling`, `color_function`,
/// `color_bar`, `plot_points`, `contours`, `data_range`, `bins`, `normalized`, `labels`. An option the plot does not
/// use, or a value an option does not take, is left out and named in a message when the plot is drawn.
pub(crate) fn plot_options<'h>(cx: &NativeCx<'_, '_, 'h>, o: T<'h>, base: Look<'h>, extra: &[&str], who: &'static str) -> Option<Look<'h>> {
    let h = cx.heap();
    let View::App { head, args } = h.view(o) else {
        return None;
    };
    if head != h.sym_term(h.sym("options")) {
        return None;
    }
    let mut look = base;
    look.who = who;
    for &r in args {
        let View::App { args: &[k, v], .. } = h.view(r) else {
            look.ignored.push(mc_syntax::Printer::new(h).expr(r));
            continue;
        };
        let View::Sym(k) = h.view(k) else {
            look.ignored.push(mc_syntax::Printer::new(h).expr(r));
            continue;
        };
        let key = h.sym_name(k);
        match one_option(cx, &key, v, &mut look, extra) {
            Some(true) => {}
            Some(false) if SILENT.contains(&key.as_str()) => {}
            Some(false) => look.ignored.push(key),
            None => look.ignored.push(format!("{key}: {}", mc_syntax::Printer::new(h).expr(v))),
        }
    }
    Some(look)
}

/// Options of how a plot is computed, or of styling the renderer takes no notice of: accepted without remark.
const SILENT: &[&str] = &[
    "max_recursion",
    "working_precision",
    "performance_goal",
    "evaluated",
    "evaluation_monitor",
    "method",
    "plot_points",
    "max_plot_points",
    "plot_theme",
    "plot_highlighting",
    "image_padding",
    "label_style",
    "base_style",
    "ticks_style",
    "axes_style",
    "frame_style",
    "content_selectable",
    "rotate_label",
    "target_units",
];

/// Whether `t` is the symbol `name`.
fn is_sym(h: &Heap, t: T<'_>, name: &str) -> bool {
    matches!(h.view(t), View::Sym(s) if h.sym_name(s) == name)
}

/// The entries of a legend specification: a list, or the list inside `placed(...)`, `line_legend(...)`,
/// `swatch_legend(...)`, `point_legend(...)` (its last list argument).
fn legend_items<'h>(cx: &NativeCx<'_, '_, 'h>, v: T<'h>) -> Option<&'h [T<'h>]> {
    let h = cx.heap();
    if let Some(items) = cx.as_list(v) {
        return Some(items);
    }
    let View::App { head, args } = h.view(v) else { return None };
    let View::Sym(s) = h.view(head) else { return None };
    match h.sym_name(s).as_str() {
        "placed" => legend_items(cx, *args.first()?),
        "line_legend" | "swatch_legend" | "point_legend" | "bar_legend" => args.iter().rev().find_map(|&a| cx.as_list(a)),
        _ => None,
    }
}

/// The `legend_label` option inside `line_legend(..., legend_label: t)` (also inside `placed(...)`).
fn legend_label<'h>(cx: &NativeCx<'_, '_, 'h>, v: T<'h>) -> Option<T<'h>> {
    let h = cx.heap();
    let View::App { head, args } = h.view(v) else { return None };
    let View::Sym(s) = h.view(head) else { return None };
    match h.sym_name(s).as_str() {
        "placed" => legend_label(cx, *args.first()?),
        "line_legend" | "swatch_legend" | "point_legend" | "bar_legend" => args.iter().find_map(|&a| {
            let View::App { head, args: rs } = h.view(a) else { return None };
            if !is_sym(h, head, "options") {
                return None;
            }
            rs.iter().find_map(|&r| match h.view(r) {
                View::App { args: &[k, t], .. } if is_sym(h, k, "legend_label") => Some(t),
                _ => None,
            })
        }),
        _ => None,
    }
}

/// The marker of legend entries the plot fills in with its series' names.
pub(crate) const AUTO_LEGEND: &str = "\u{0}automatic";

/// One option: `Some(true)` used, `Some(false)` not taken by this plot, `None` a value it does not take.
fn one_option<'h>(cx: &NativeCx<'_, '_, 'h>, key: &str, v: T<'h>, look: &mut Look<'h>, extra: &[&str]) -> Option<bool> {
    let h = cx.heap();
    let text = |t: T<'h>| match h.view(t) {
        View::Str(s) => Some(s.to_string()),
        _ => None,
    };
    let num = |t: T<'h>| mc_graphics::num::number(h, t);
    let is = |t: T<'h>, name: &str| is_sym(h, t, name);
    let takes = |k: &str| extra.contains(&k);
    match key {
        "x_label" => look.x_label = Some(text(v)?),
        "y_label" => look.y_label = Some(text(v)?),
        "legend" | "plot_legends" | "chart_legends" => {
            if is(v, "none") {
                look.legend.clear();
                return Some(true);
            }
            if is(v, "automatic") || text(v).is_some_and(|s| s == "Expressions") || is(v, "expressions") {
                look.legend = vec![AUTO_LEGEND.to_string()];
                look.legend_terms = vec![];
                return Some(true);
            }
            // a title over the entries: legend_label inside line_legend(...) or placed(...)
            if let Some(t) = legend_label(cx, v) {
                look.graphics.push(("legend_title".to_string(), t));
            }
            let items = legend_items(cx, v)?;
            look.legend = items.iter().map(|&e| text(e).unwrap_or_else(|| mc_syntax::Printer::new(h).expr(e))).collect();
            look.legend_terms = if items.iter().all(|&e| text(e).is_some()) { vec![] } else { items.to_vec() };
        }
        "legend_position" => look.legend_at = corner_option(h, v)?,
        "colors" => {
            // "#rrggbb" strings or colour values
            let byte = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
            let one = |e: T<'h>| match text(e) {
                Some(s) => hex_color(&s),
                None => match mc_graphics::color::colour(h, e) {
                    mc_graphics::color::Colour::Value(c) => Some((byte(c.r), byte(c.g), byte(c.b))),
                    _ => None,
                },
            };
            look.colors = match cx.as_list(v) {
                Some(items) => items.iter().map(|&e| one(e)).collect::<Option<_>>()?,
                None => vec![one(v)?],
            }
        }
        "line_thickness" => {
            // points, one per curve (cycled)
            let ws: Vec<f64> = match cx.as_list(v) {
                Some(items) => items.iter().map(|&e| num(e).filter(|w| *w > 0.0)).collect::<Option<_>>()?,
                None => vec![num(v).filter(|w| *w > 0.0)?],
            };
            look.thickness = ws;
        }
        "plot_style" | "chart_style" => {
            if is(v, "automatic") || is(v, "none") {
                return Some(true);
            }
            let entries: Vec<T<'h>> = match cx.as_list(v) {
                Some(items) if !items.is_empty() => items.to_vec(),
                Some(_) => return Some(true),
                None => vec![v],
            };
            let mut colors = Vec::with_capacity(entries.len());
            for (k, &e) in entries.iter().enumerate() {
                let mut st = mc_graphics::model::Style::default();
                let specs = cx.as_list(e).map(|l| l.to_vec()).unwrap_or_else(|| vec![e]);
                if mc_graphics::model::styled(h, &specs, &mut st).is_err() {
                    return None;
                }
                let c = st.color;
                let byte = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
                let own = st.color != mc_graphics::model::Style::default().color || specs.iter().any(|&s| is(s, "black"));
                colors.push(if own { (byte(c.r), byte(c.g), byte(c.b)) } else { look.color(k) });
            }
            look.colors = colors;
            look.styles = entries;
        }
        "plot_range" => {
            look.full = is(v, "all") || is(v, "full");
            look.range = match num(v) {
                Some(r) if r > 0.0 => [[None; 2], [Some(-r), Some(r)]],
                _ if is(v, "full") => [[None; 2]; 2],
                _ => range_option(cx, v)?,
            }
        }
        "plot_labels" => {
            if is(v, "none") {
                return Some(true);
            }
            look.curve_labels = match cx.as_list(v) {
                Some(items) => items.to_vec(),
                None if is(v, "automatic") || text(v).is_some_and(|s| s == "Expressions") => vec![],
                None => vec![v],
            };
        }
        "frame" => {
            look.origin_axes = !cx.is_true(v);
            look.bare = false;
        }
        "axes" => {
            if cx.is_false(v) && look.origin_axes {
                look.bare = true;
            }
        }
        "joined" if takes("joined") => match cx.as_list(v) {
            // per data set
            Some(items) => {
                look.joined_each = items.iter().map(|&e| cx.is_true(e)).collect();
                look.joined = false;
            }
            None => look.joined = cx.is_true(v),
        },
        "x_scale" if takes("x_scale") => look.scale.x = axis_option(h, v)?,
        "y_scale" if takes("y_scale") => look.scale.y = axis_option(h, v)?,
        "scaling_functions" if takes("y_scale") => {
            // "Log" / "Log10" for y, or [x, y]
            let one = |t: T<'h>| match text(t).as_deref() {
                Some("Log" | "Log10" | "Log2") => Some(Axis::Log),
                None if is(t, "none") || is(t, "automatic") => Some(Axis::Linear),
                _ => None,
            };
            match cx.as_list(v) {
                Some(&[x, y]) => {
                    look.scale.x = one(x)?;
                    look.scale.y = one(y)?;
                }
                _ => look.scale.y = one(v)?,
            }
        }
        "line_style" if takes("line_style") => look.dashes = cx.as_list(v)?.iter().map(|&e| dash_option(h, e)).collect::<Option<_>>()?,
        "filling" if takes("filling") => {
            if is(v, "none") || cx.is_false(v) {
                look.filling = None;
                return Some(true);
            }
            if let Some(f) = filling_value(cx, v) {
                look.filling = Some(f);
                return Some(true);
            }
            // rules: i -> axis | bottom | top | c | [j] | j, one or a list of them
            let rules: Vec<T<'h>> = match cx.as_list(v) {
                Some(items) => items.to_vec(),
                None => vec![v],
            };
            for r in rules {
                let View::App { head, args: &[i, w] } = h.view(r) else { return None };
                if head != h.sym_term(h.sym(names::RULE)) {
                    return None;
                }
                let View::Int(i) = h.view(i) else { return None };
                if i < 1 {
                    return None;
                }
                let i = i as usize - 1;
                // [target, style]: the style is not used
                let w = match cx.as_list(w) {
                    Some(&[t, s]) if cx.as_list(s).is_some() || !matches!(h.view(s), View::Int(_)) => t,
                    _ => w,
                };
                let target = match cx.as_list(w) {
                    Some(&[j]) => Some(j),
                    _ => None,
                };
                let f = match target.map(|j| h.view(j)) {
                    Some(View::Int(j)) if j >= 1 => Filling::Between(i, j as usize - 1),
                    Some(_) => return None,
                    None => filling_value(cx, w)?,
                };
                look.fill_each.push((i, f));
            }
        }
        "filling_style" if takes("filling") => {}
        "color_function" if takes("color_function") => {
            look.gradient = Some(match mc_graphics::gradient::read(h, v) {
                Some(g) => g,
                None => match text(v) {
                    Some(s) => mc_graphics::gradient::scheme(&s),
                    None => return Some(false),
                },
            })
        }
        "color_bar" if takes("color_bar") => {
            look.hide_color_bar = if is(v, "true") || is(v, "automatic") {
                false
            } else if is(v, "false") || is(v, "none") {
                true
            } else {
                return None;
            }
        }
        "plot_points" if takes("plot_points") => match h.view(v) {
            View::Int(n) if (2..=400).contains(&n) => look.plot_points = Some(n as usize),
            View::Int(n) if n > 400 => look.plot_points = Some(400),
            _ => return None,
        },
        "contours" if takes("contours") => {
            look.contours = Some(match h.view(v) {
                View::Int(n) if (1..=100).contains(&n) => figure::Contours::Count(n as usize),
                _ => {
                    let vs = cx.as_list(v)?.iter().map(|&e| num(e)).collect::<Option<Vec<f64>>>()?;
                    if vs.is_empty() || vs.len() > 100 {
                        return None;
                    }
                    figure::Contours::Values(vs)
                }
            })
        }
        "data_range" if takes("data_range") => {
            if is(v, "automatic") || is(v, "all") {
                return Some(true);
            }
            let &[x, y] = cx.as_list(v)? else { return None };
            let pair = |t: T<'h>| -> Option<[f64; 2]> {
                let &[a, b] = cx.as_list(t)? else { return None };
                let (a, b) = (num(a)?, num(b)?);
                (a < b).then_some([a, b])
            };
            look.data_range = Some([pair(x)?, pair(y)?]);
        }
        "bins" if takes("bins") => look.bins = bins_option(cx, v)?,
        "normalized" if takes("normalized") => {
            look.normalized = if is(v, "true") {
                true
            } else if is(v, "false") {
                false
            } else {
                return None;
            }
        }
        "labels" | "chart_labels" if takes("labels") => {
            let items = match legend_items(cx, v) {
                Some(items) => items,
                None if is(v, "automatic") || is(v, "none") => return Some(true),
                None => return None,
            };
            look.labels = items.iter().map(|&e| text(e).unwrap_or_else(|| mc_syntax::Printer::new(h).expr(e))).collect()
        }
        k if mc_graphics::model::known_option(k) => {
            if !mc_graphics::model::HARMLESS.contains(&k) {
                look.graphics.push((k.to_string(), v));
            }
        }
        _ => return Some(false),
    }
    Some(true)
}

/// A filling written as a value: `axis`, `bottom`, `top`, a number, or `[i, j]` (between curves i and j).
fn filling_value<'h>(cx: &NativeCx<'_, '_, 'h>, v: T<'h>) -> Option<Filling> {
    let h = cx.heap();
    let is = |name: &str| is_sym(h, v, name);
    if is("axis") || is("true") || is("automatic") {
        return Some(Filling::Axis);
    }
    if is("bottom") {
        return Some(Filling::Bottom);
    }
    if is("top") {
        return Some(Filling::Top);
    }
    if let Some(c) = mc_graphics::num::number(h, v) {
        return Some(Filling::Value(c));
    }
    let &[i, j] = cx.as_list(v)? else { return None };
    let index = |t: T<'h>| match h.view(t) {
        View::Int(k) if k >= 1 => Some(k as usize - 1),
        _ => None,
    };
    Some(Filling::Between(index(i)?, index(j)?))
}

/// `plot_range:` `automatic` / `all`, `[y0, y1]`, or `[[x0, x1], [y0, y1]]`; an end may be `automatic`.
fn range_option<'h>(cx: &NativeCx<'_, '_, 'h>, v: T<'h>) -> Option<[[Option<f64>; 2]; 2]> {
    let h = cx.heap();
    let auto = |t: T<'h>| matches!(h.view(t), View::Sym(s) if matches!(h.sym_name(s).as_str(), "automatic" | "all"));
    if auto(v) {
        return Some([[None; 2]; 2]);
    }
    let pair = |t: T<'h>| -> Option<[Option<f64>; 2]> {
        if auto(t) {
            return Some([None; 2]);
        }
        let &[a, b] = cx.as_list(t)? else { return None };
        let end = |e: T<'h>| if auto(e) { Some(None) } else { mc_graphics::num::number(h, e).map(Some) };
        let r = [end(a)?, end(b)?];
        match r {
            [Some(a), Some(b)] if a >= b => None,
            r => Some(r),
        }
    };
    let &[a, b] = cx.as_list(v)? else { return None };
    if cx.as_list(a).is_some() || cx.as_list(b).is_some() {
        Some([pair(a)?, pair(b)?])
    } else {
        Some([[None; 2], pair(v)?])
    }
}

/// `bins:` `automatic`, `n` (a count), `[w]` (a width) or `[lo, hi, w]`.
fn bins_option<'h>(cx: &NativeCx<'_, '_, 'h>, v: T<'h>) -> Option<figure::Bins> {
    let h = cx.heap();
    let num = |t: T<'h>| mc_graphics::num::number(h, t);
    if matches!(h.view(v), View::Sym(s) if h.sym_name(s) == "automatic") {
        return Some(figure::Bins::Auto);
    }
    if let View::Int(n) = h.view(v) {
        return (1..=10_000).contains(&n).then_some(figure::Bins::Count(n as usize));
    }
    match *cx.as_list(v)? {
        [w] => num(w).filter(|w| *w > 0.0).map(figure::Bins::Width),
        [lo, hi, w] => {
            let (lo, hi, w) = (num(lo)?, num(hi)?, num(w)?);
            (lo < hi && w > 0.0).then_some(figure::Bins::Range(lo, hi, w))
        }
        _ => None,
    }
}

/// A colour written `#rrggbb` (hexadecimal red, green, blue).
pub(crate) fn hex_color(s: &str) -> Option<Rgb> {
    let d = s.strip_prefix('#').filter(|d| d.len() == 6 && d.is_ascii())?;
    let c = |i: usize| u8::from_str_radix(&d[i..i + 2], 16).ok();
    Some((c(0)?, c(2)?, c(4)?))
}

/// A dash pattern: `solid`, `dashed`, `dotted` or `dot_dashed`.
fn dash_option(h: &Heap, v: T<'_>) -> Option<Dash> {
    let View::Sym(s) = h.view(v) else { return None };
    match h.sym_name(s).as_str() {
        "solid" => Some(Dash::Solid),
        "dashed" => Some(Dash::Dashed),
        "dotted" => Some(Dash::Dotted),
        "dot_dashed" => Some(Dash::DotDashed),
        _ => None,
    }
}

/// A corner: `top_right`, `top_left`, `bottom_left` or `bottom_right`.
fn corner_option(h: &Heap, v: T<'_>) -> Option<Corner> {
    let View::Sym(s) = h.view(v) else { return None };
    match h.sym_name(s).as_str() {
        "top_right" => Some(Corner::TopRight),
        "top_left" => Some(Corner::TopLeft),
        "bottom_left" => Some(Corner::BottomLeft),
        "bottom_right" => Some(Corner::BottomRight),
        _ => None,
    }
}

/// An axis scale: `linear`, `log`, `signed_log` (c = 1) or `signed_log(c)` with a number c > 0.
fn axis_option(h: &Heap, v: T<'_>) -> Option<Axis> {
    let num = |t: T<'_>| match h.view(t) {
        View::Int(i) => Some(i as f64),
        View::Real(x) => Some(x),
        _ => None,
    };
    match h.view(v) {
        View::Sym(s) => match h.sym_name(s).as_str() {
            "linear" => Some(Axis::Linear),
            "log" => Some(Axis::Log),
            "signed_log" => Some(Axis::SignedLog(1.0)),
            _ => None,
        },
        View::App { head, args: &[c] } if head == h.sym_term(h.sym("signed_log")) => num(c)
            .filter(|c| *c > 0.0 && c.is_finite())
            .map(Axis::SignedLog),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors() {
        assert_eq!(hex_color("#560591"), Some((86, 5, 145)));
        assert_eq!(hex_color("#FFffff"), Some((255, 255, 255)));
        for bad in ["560591", "#56059", "#5605911", "#56059g", "#\u{e9}0591", ""] {
            assert_eq!(hex_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn png_header() {
        let mut head = [0u8; 24];
        head[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        head[8..12].copy_from_slice(&13u32.to_be_bytes());
        head[12..16].copy_from_slice(b"IHDR");
        head[16..20].copy_from_slice(&640u32.to_be_bytes());
        head[20..24].copy_from_slice(&400u32.to_be_bytes());
        assert_eq!(png_header_size(&head), Some((640, 400)));
        head[12] = b'X';
        assert_eq!(png_header_size(&head), None);
        assert_eq!(png_size(std::path::Path::new("no such file.png")), None);
    }
}
