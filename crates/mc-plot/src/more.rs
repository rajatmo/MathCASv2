//! More plots: `discrete_plot` (values at integers, with stems), `list_polar_plot`, `re_im_plot` and
//! `abs_arg_plot` (real and imaginary parts, modulus and argument), `complex_plot` (domain colouring: hue the
//! argument, lightness the modulus) and `complex_plot3d` (the modulus as a surface coloured by the argument),
//! `array_plot` and `matrix_plot` (a matrix as coloured cells), `pie_chart`, and `color_data` (colour schemes by
//! name and the indexed plot colours).
use crate::figure::{Extra, Look, Series};
use crate::{bad, finish, finish_graphic, plot_options, real, short_name};
use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::{colour, Colour, Rgba};
use mc_graphics::gradient::Gradient;
use mc_numfn::{Options, Var};

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let a = Attributes::default;
    crate::held::register(k, h, "re_im_plot", Box::new(Parts { name: "re_im_plot", parts: ["re", "im_part"] }));
    crate::held::register(k, h, "abs_arg_plot", Box::new(Parts { name: "abs_arg_plot", parts: ["abs", "arg"] }));
    crate::held::register(k, h, "discrete_plot", Box::new(DiscretePlot));
    k.register(h.sym("list_polar_plot"), a(), Box::new(ListPolarPlot));
    crate::held::register(k, h, "complex_plot", Box::new(ComplexPlot { three: false }));
    crate::held::register(k, h, "complex_plot3d", Box::new(ComplexPlot { three: true }));
    k.register(h.sym("array_plot"), a(), Box::new(ArrayPlot { matrix: false }));
    k.register(h.sym("matrix_plot"), a(), Box::new(ArrayPlot { matrix: true }));
    k.register(h.sym("pie_chart"), a(), Box::new(PieChart));
    k.register(h.sym("color_data"), a(), Box::new(ColorData));
}

fn is_options(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::App { head, .. } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options"))
}

/// The arguments and the options term (if the last argument is one).
pub(crate) fn split<'h>(h: &'h Heap, args: &[T<'h>]) -> (Vec<T<'h>>, Option<T<'h>>) {
    match args.split_last() {
        Some((&o, rest)) if is_options(h, o) => (rest.to_vec(), Some(o)),
        _ => (args.to_vec(), None),
    }
}

/// `re_im_plot(f, [x, a, b])` and `abs_arg_plot`: a plot of two parts of the complex values of f.
struct Parts {
    name: &'static str,
    parts: [&'static str; 2],
}

impl Native for Parts {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (a, o) = split(h, args);
        let [f, spec] = a.as_slice() else { return Ok(Reply::Decline) };
        if cx.as_list(*spec).is_none_or(|l| l.len() != 3) {
            return Ok(bad(h, self.name, 2));
        }
        let fs: Vec<T<'h>> = cx.as_list(*f).map(|l| l.to_vec()).unwrap_or_else(|| vec![*f]);
        let mut curves = vec![];
        for f in fs {
            for p in self.parts {
                curves.push(h.call(h.sym(p), &[f]));
            }
        }
        let mut new = vec![cx.list(&curves), *spec];
        new.extend(o);
        Ok(Reply::Again(h.call(h.sym("plot"), &new)))
    }
}

/// A sampled function's variable and values: `[n, a, b]`, `[n, a, b, step]` or `[n, [v1, v2, ...]]`.
pub(crate) fn discrete_values<'h>(cx: &mut NativeCx<'_, '_, 'h>, spec: T<'h>) -> Res<Option<(Sym, Vec<f64>)>> {
    let h = cx.heap();
    let Some(items) = cx.as_list(spec) else { return Ok(None) };
    let Some(&v) = items.first() else { return Ok(None) };
    let View::Sym(n) = h.view(v) else { return Ok(None) };
    let vals = match items {
        [_, l] if cx.as_list(*l).is_some() => {
            let mut out = vec![];
            for &e in cx.as_list(*l).unwrap_or(&[]) {
                match real(cx, e)? {
                    Some(x) => out.push(x),
                    None => return Ok(None),
                }
            }
            out
        }
        [_, b] => match real(cx, *b)? {
            Some(b) => (1..=(b.floor() as i64).max(1)).map(|k| k as f64).collect(),
            None => return Ok(None),
        },
        [_, a, b, rest @ ..] if rest.len() <= 1 => {
            let (Some(a), Some(b)) = (real(cx, *a)?, real(cx, *b)?) else { return Ok(None) };
            let d = match rest.first() {
                Some(&d) => match real(cx, d)? {
                    Some(d) if d > 0.0 => d,
                    _ => return Ok(None),
                },
                None => 1.0,
            };
            if b < a || (b - a) / d > 10_000.0 {
                return Ok(None);
            }
            let m = ((b - a) / d + 1e-9).floor() as usize;
            (0..=m).map(|k| a + d * k as f64).collect()
        }
        _ => return Ok(None),
    };
    Ok(Some((n, vals)))
}

/// `discrete_plot(f, [n, a, b])`: the values of f at n = a, a + 1, ..., b as points with stems to the axis.
struct DiscretePlot;

impl Native for DiscretePlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = "discrete_plot";
        let (a, o) = split(h, args);
        let [f, spec] = a.as_slice() else { return Ok(Reply::Decline) };
        let base = Look { joined: false, data: true, ..Look::default() };
        let look = match o {
            Some(o) => plot_options(cx, o, base, &["filling", "joined"], name).unwrap_or_default(),
            None => base,
        };
        let Some((n, vals)) = discrete_values(cx, *spec)? else { return Ok(bad(h, name, 2)) };
        let fs: Vec<T<'h>> = cx.as_list(*f).map(|l| l.to_vec()).unwrap_or_else(|| vec![*f]);
        let (fs, wraps) = crate::unwrap_all(h, &fs);
        let mut look = look;
        wraps.apply(cx, &mut look);
        let mut series = vec![];
        for &f in &fs {
            let mut fun = mc_numfn::compile_expr(cx, f, &[Var::scalar(n)], &Options::default())?;
            let mut pts = Vec::with_capacity(vals.len());
            let mut out = [0.0];
            for (i, &x) in vals.iter().enumerate() {
                if i % 256 == 0 {
                    cx.tick(1)?;
                }
                let ok = fun.eval(cx, &[x], &mut out)?;
                let mut y = (ok && out[0].is_finite()).then_some(out[0]);
                // functions of integers (prime(n), moebius_mu(n), sums to n): the exact integer substituted
                if y.is_none() && x.fract() == 0.0 && x.abs() < 1e15 {
                    let r = h.call(h.sym(mc_api::names::RULE), &[h.sym_term(n), h.int(x as i64)]);
                    let v = cx.eval(h.call(h.sym("n"), &[h.call(h.sym("rewrite"), &[f, r])]))?;
                    y = crate::real(cx, v)?;
                }
                pts.push((x, y));
            }
            series.push(Series { points: pts, name: short_name(h, f), bars: vec![] });
        }
        if series.iter().all(|s| s.points.iter().all(|p| p.1.is_none())) {
            return Ok(bad(h, name, 1));
        }
        // stems from the axis to each point, faint, in the series' colour
        let mut under = vec![];
        if look.filling.is_none() && look.fill_each.is_empty() {
            for (k, s) in series.iter().enumerate() {
                let segs: Vec<T<'h>> = s.points.iter().filter_map(|&(x, y)| y.map(|y| h.real_array(&[2, 2], &[x, 0.0, x, y]))).collect();
                if segs.is_empty() {
                    continue;
                }
                let (r, g, b) = look.color(k);
                let c = Rgba { r: r as f64 / 255.0, g: g as f64 / 255.0, b: b as f64 / 255.0, a: 0.6 };
                under.push(cx.list(&[mc_graphics_kernel::rgb_term(h, c), h.call(h.sym("absolute_thickness"), &[h.real(1.5)]), h.call(h.sym("line"), &[cx.list(&segs)])]));
            }
        }
        let xs: Vec<f64> = vals.clone();
        let bounds = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| (a.min(x), b.max(x)));
        let ys = series.iter().flat_map(|s| s.points.iter().filter_map(|p| p.1)).fold((0.0f64, 0.0f64), |(a, b), y| (a.min(y), b.max(y)));
        let extra = Extra { under, bounds: Some([bounds, ys]), entries: vec![] };
        finish(cx, &series, &look, extra)
    }
}

/// `list_polar_plot([r1, r2, ...])` (at angles 0, 2 pi/n, ...) or `list_polar_plot([[theta, r], ...])`.
struct ListPolarPlot;

impl Native for ListPolarPlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = "list_polar_plot";
        let (a, o) = split(h, args);
        let [d] = a.as_slice() else { return Ok(Reply::Decline) };
        let base = Look { joined: false, equal: true, origin_axes: true, data: true, ..Look::default() };
        let look = match o {
            Some(o) => plot_options(cx, o, base, &["joined"], name).unwrap_or_default(),
            None => base,
        };
        let mut look = look;
        // one data set or several (a list of lists none of which is an [theta, r] pair), each maybe wrapped
        let wrapped = |t: T<'h>| matches!(crate::values::call_of(h, t), Some((n, _)) if matches!(n.as_str(), "legended" | "labeled" | "callout" | "style" | "highlighted"));
        let d = if wrapped(*d) {
            let (v, w) = crate::unwrap_all(h, &[*d]);
            w.apply(cx, &mut look);
            v[0]
        } else {
            *d
        };
        let d = match cx.as_list(d) {
            Some(items) if items.iter().any(|&e| wrapped(e)) => {
                let (v, w) = crate::unwrap_all(h, items);
                w.apply(cx, &mut look);
                cx.list(&v)
            }
            _ => d,
        };
        let Some(sets) = crate::data_sets(cx, d) else { return Ok(bad(h, name, 1)) };
        let many = sets.len() > 1;
        let mut series = vec![];
        for (k, set) in sets.into_iter().enumerate() {
            let Some(items) = cx.as_list(set) else { return Ok(bad(h, name, 1)) };
            let n = items.len().max(1);
            let mut pts = Vec::with_capacity(items.len());
            for (i, &e) in items.iter().enumerate() {
                let (th, r) = match cx.as_list(e) {
                    Some(&[t, r]) => (real(cx, t)?, real(cx, r)?),
                    Some(_) => return Ok(bad(h, name, 1)),
                    None => (Some(2.0 * std::f64::consts::PI * i as f64 / n as f64), real(cx, e)?),
                };
                match (th, r) {
                    (Some(t), Some(r)) => pts.push((r * t.cos(), Some(r * t.sin()))),
                    _ => pts.push((0.0, None)),
                }
            }
            let name = if many { format!("data {}", k + 1) } else { "data".into() };
            series.push(Series { points: pts, name, bars: vec![] });
        }
        if series.iter().all(|s| s.points.iter().all(|p| p.1.is_none())) {
            return Ok(bad(h, name, 1));
        }
        finish(cx, &series, &look, Extra::default())
    }
}

/// The colour of a complex value by domain colouring: hue the argument (red at positive reals), lightness rising
/// with the modulus from black (zeros) through full colour (modulus 1) towards white (poles).
pub(crate) fn domain_colour(re: f64, im: f64) -> Rgba {
    let m = re.hypot(im);
    let hue = (im.atan2(re) / (2.0 * std::f64::consts::PI)).rem_euclid(1.0);
    let l = (2.0 / std::f64::consts::PI) * m.sqrt().atan();
    let l = 0.08 + 0.84 * l;
    // HSL with full saturation
    let c = Rgba::from_hsb(hue, 1.0, 1.0);
    if l <= 0.5 {
        let k = l / 0.5;
        Rgba::rgb(c.r * k, c.g * k, c.b * k)
    } else {
        let k = (l - 0.5) / 0.5;
        Rgba::rgb(c.r + (1.0 - c.r) * k, c.g + (1.0 - c.g) * k, c.b + (1.0 - c.b) * k)
    }
}

/// The value of `f` at the complex number `x + i y` (substituted for `z`), if it is a number.
fn complex_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, z: T<'h>, x: f64, y: f64) -> Res<Option<(f64, f64)>> {
    let h = cx.heap();
    let c = h.call(h.sym(names::COMPLEX), &[h.real(x), h.real(y)]);
    let r = h.call(h.sym(names::RULE), &[z, c]);
    let v = cx.eval(h.call(h.sym("n"), &[h.call(h.sym("rewrite"), &[f, r])]))?;
    Ok(match h.view(v) {
        View::Real(a) => Some((a, 0.0)),
        View::Int(a) => Some((a as f64, 0.0)),
        View::App { head, args: &[a, b] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::COMPLEX) => {
            match (mc_graphics::num::number(h, a), mc_graphics::num::number(h, b)) {
                (Some(a), Some(b)) => Some((a, b)),
                _ => None,
            }
        }
        _ => None,
    }
    .filter(|(a, b)| a.is_finite() && b.is_finite()))
}

/// `complex_plot(f, [z, zmin, zmax])`: f over the rectangle with corners zmin and zmax, by domain colouring;
/// `complex_plot3d`: its modulus as a surface coloured the same way.
struct ComplexPlot {
    three: bool,
}

impl Native for ComplexPlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = if self.three { "complex_plot3d" } else { "complex_plot" };
        let (a, o) = split(h, args);
        let [f, spec] = a.as_slice() else { return Ok(Reply::Decline) };
        let spec = &crate::complex_spec(cx, *spec)?;
        let Some(&[z, lo, hi]) = cx.as_list(*spec) else { return Ok(bad(h, name, 2)) };
        if !matches!(h.view(z), View::Sym(_)) {
            return Ok(bad(h, name, 2));
        }
        let (Some(lo), Some(hi)) = (complex_at(cx, lo, z, 0.0, 0.0)?, complex_at(cx, hi, z, 0.0, 0.0)?) else {
            return Ok(bad(h, name, 2));
        };
        let (x0, x1, y0, y1) = (lo.0.min(hi.0), lo.0.max(hi.0), lo.1.min(hi.1), lo.1.max(hi.1));
        if x1 <= x0 || y1 <= y0 {
            return Ok(bad(h, name, 2));
        }
        let n = if self.three { 41 } else { 120 };
        let mut vals = Vec::with_capacity(n * n);
        for j in 0..n {
            cx.tick(1)?;
            // rows from the top (y1) down
            let y = if self.three { y0 + (y1 - y0) * j as f64 / (n - 1) as f64 } else { y1 - (y1 - y0) * (j as f64 + 0.5) / n as f64 };
            for i in 0..n {
                let x = if self.three { x0 + (x1 - x0) * i as f64 / (n - 1) as f64 } else { x0 + (x1 - x0) * (i as f64 + 0.5) / n as f64 };
                vals.push((x, y, complex_at(cx, *f, z, x, y)?));
            }
        }
        if vals.iter().all(|v| v.2.is_none()) {
            return Ok(bad(h, name, 1));
        }
        if !self.three {
            let mut cells = Vec::with_capacity(n * n * 4);
            for &(_, _, v) in &vals {
                match v {
                    Some((a, b)) => {
                        let c = domain_colour(a, b);
                        cells.extend_from_slice(&[c.r, c.g, c.b, 1.0]);
                    }
                    None => cells.extend_from_slice(&[0.0, 0.0, 0.0, 0.0]),
                }
            }
            // raster rows run from the bottom up
            let mut rows = Vec::with_capacity(cells.len());
            for r in (0..n).rev() {
                rows.extend_from_slice(&cells[r * n * 4..(r + 1) * n * 4]);
            }
            let raster = h.call(h.sym("raster"), &[h.real_array(&[n, n, 4], &rows), cx.list(&[h.reals(&[x0, y0]), h.reals(&[x1, y1])])]);
            let base = Look { exact: true, equal: true, ..Look::default() };
            let look = match o {
                Some(o) => plot_options(cx, o, base, &[], name).unwrap_or_default(),
                None => base,
            };
            let extra = Extra { under: vec![raster], bounds: Some([(x0, x1), (y0, y1)]), entries: vec![] };
            return finish(cx, &[], &look, extra);
        }
        // the surface |f| over the grid, each quadrilateral coloured by the argument at its corners
        let at = |i: usize, j: usize| vals[j * n + i];
        let mut quads = vec![];
        let mut colors = vec![];
        let ms: Vec<f64> = vals.iter().filter_map(|v| v.2.map(|(a, b)| a.hypot(b))).collect();
        let (_, top) = crate::figure::robust(ms);
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let c = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
                if c.iter().any(|p| p.2.is_none()) {
                    continue;
                }
                for (x, y, v) in c {
                    let (a, b) = v.unwrap_or((0.0, 0.0));
                    quads.extend_from_slice(&[x, y, a.hypot(b).min(top * 1.5)]);
                    let col = Rgba::from_hsb((b.atan2(a) / (2.0 * std::f64::consts::PI)).rem_euclid(1.0), 0.75, 1.0);
                    colors.extend_from_slice(&[col.r, col.g, col.b]);
                }
            }
        }
        let q = quads.len() / 12;
        if q == 0 {
            return Ok(bad(h, name, 1));
        }
        let vc = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("vertex_colors")), h.real_array(&[q, 4, 3], &colors)]);
        let poly = h.call(h.sym("polygon"), &[h.real_array(&[q, 4, 3], &quads), h.call(h.sym("options"), &[vc])]);
        let rule = |k: &str, v: T<'h>| h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v]);
        let mut opts = vec![
            rule("axes", h.sym_term(h.sym("true"))),
            rule("box_ratios", h.reals(&[1.0, 1.0, 0.6])),
            rule("plot_range", cx.list(&[h.reals(&[x0, x1]), h.reals(&[y0, y1]), h.reals(&[0.0, top * 1.5])])),
        ];
        let mut ignored = vec![];
        if let Some(o) = o {
            if let View::App { args: rs, .. } = h.view(o) {
                for &r in rs {
                    if let View::App { args: &[k, _], .. } = h.view(r) {
                        if let View::Sym(s) = h.view(k) {
                            let key = h.sym_name(s);
                            if mc_graphics::three::OPTIONS.contains(&key.as_str()) || mc_graphics::model::known_option(&key) {
                                opts.push(r);
                            } else {
                                ignored.push(key);
                            }
                        }
                    }
                }
            }
        }
        let g = h.call(h.sym("graphics3d"), &[cx.list(&[poly]), h.call(h.sym("options"), &opts)]);
        finish_graphic(cx, g, name, &ignored)
    }
}

/// `array_plot(m)`: a matrix as cells, row 1 at the top: numbers from 0 (white) to 1 (black) (other ranges
/// scaled), colours as they are, `true` black and `false` white; `matrix_plot(m)`: values coloured from blue
/// (negative) through white to red (positive), with row and column numbers.
struct ArrayPlot {
    matrix: bool,
}

impl Native for ArrayPlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = if self.matrix { "matrix_plot" } else { "array_plot" };
        let (a, o) = split(h, args);
        let [m] = a.as_slice() else { return Ok(Reply::Decline) };
        let Some(rows) = cx.as_list(*m).map(|r| r.to_vec()) else { return Ok(bad(h, name, 1)) };
        let mut cells: Vec<Vec<Result<f64, Option<Rgba>>>> = Vec::with_capacity(rows.len());
        for &r in &rows {
            cx.tick(1)?;
            let items: Vec<T<'h>> = cx.as_list(r).map(|l| l.to_vec()).unwrap_or_else(|| vec![r]);
            let mut row = Vec::with_capacity(items.len());
            for e in items {
                row.push(if cx.is_true(e) {
                    Ok(1.0)
                } else if cx.is_false(e) {
                    Ok(0.0)
                } else if let Colour::Value(c) = colour(h, e) {
                    Err(Some(c))
                } else {
                    match real(cx, e)? {
                        Some(v) => Ok(v),
                        None => Err(None),
                    }
                });
            }
            cells.push(row);
        }
        let cols = cells.iter().map(|r| r.len()).max().unwrap_or(0);
        if cols == 0 || cells.len() * cols > 4_000_000 {
            return Ok(bad(h, name, 1));
        }
        let nums: Vec<f64> = cells.iter().flatten().filter_map(|c| c.as_ref().ok().copied()).collect();
        let (lo, hi) = nums.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        let grad = o.and_then(|o| option(h, o, "color_function")).and_then(|g| {
            mc_graphics::gradient::read(h, g).or_else(|| match h.view(g) {
                View::Str(s) => Some(mc_graphics::gradient::scheme(s)),
                _ => None,
            })
        });
        let diverging = Gradient::named("cool_warm").unwrap_or_else(Gradient::default_gradient);
        let big = lo.abs().max(hi.abs()).max(f64::MIN_POSITIVE);
        let shade = |v: f64| -> Rgba {
            if let Some(g) = &grad {
                return g.at(if hi > lo { (v - lo) / (hi - lo) } else { 0.5 });
            }
            if self.matrix {
                diverging.at(0.5 + 0.5 * v / big)
            } else {
                // 0 white .. 1 black (values outside [0, 1] scaled to it)
                let t = if lo >= 0.0 && hi <= 1.0 { v } else if hi > lo { (v - lo) / (hi - lo) } else { 0.5 };
                Rgba::rgb(1.0 - t, 1.0 - t, 1.0 - t)
            }
        };
        let nr = cells.len();
        let mut data = Vec::with_capacity(nr * cols * 4);
        for row in cells.iter().rev() {
            for k in 0..cols {
                let c = match row.get(k) {
                    Some(Ok(v)) => shade(*v),
                    Some(Err(Some(c))) => *c,
                    _ => Rgba { a: 0.0, ..Rgba::WHITE },
                };
                data.extend_from_slice(&[c.r, c.g, c.b, c.a]);
            }
        }
        let raster = h.call(h.sym("raster"), &[h.real_array(&[nr, cols, 4], &data), cx.list(&[h.reals(&[0.0, 0.0]), h.reals(&[cols as f64, nr as f64])])]);
        let rule = |k: &str, v: T<'h>| h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v]);
        let none = h.sym_term(h.sym("none"));
        let ticks = if self.matrix {
            // row and column numbers at the cells' centres (a few when there are many)
            let marks = |n: usize, flip: bool| -> T<'h> {
                let step = (n as f64 / 6.0).ceil().max(1.0) as usize;
                let ts: Vec<T<'h>> = (1..=n)
                    .filter(|k| *k == 1 || k % step == 0)
                    .map(|k| {
                        let at = if flip { n as f64 - k as f64 + 0.5 } else { k as f64 - 0.5 };
                        cx.list(&[h.real(at), h.string(&k.to_string())])
                    })
                    .collect();
                cx.list(&ts)
            };
            cx.list(&[marks(cols, false), marks(nr, true)])
        } else {
            none
        };
        let size = (360.0f64).min(40.0 * cols.max(nr) as f64).max(120.0);
        let mut opts = vec![
            rule("frame", h.sym_term(h.sym("true"))),
            rule("ticks", ticks),
            rule("plot_range_padding", none),
            rule("aspect_ratio", h.sym_term(h.sym("automatic"))),
            rule("image_size", h.real(size)),
        ];
        let mut content = vec![raster];
        let mut ignored = vec![];
        if let Some(o) = o {
            if let View::App { args: rs, .. } = h.view(o) {
                for &r in rs {
                    let View::App { args: &[k, v], .. } = h.view(r) else { continue };
                    let View::Sym(s) = h.view(k) else { continue };
                    let key = h.sym_name(s);
                    match key.as_str() {
                        "color_function" => {}
                        "mesh" if !cx.is_false(v) && !matches!(h.view(v), View::Sym(n) if h.sym_name(n) == "none") => {
                            let mut segs = vec![];
                            for k in 0..=cols {
                                segs.push(h.real_array(&[2, 2], &[k as f64, 0.0, k as f64, nr as f64]));
                            }
                            for k in 0..=nr {
                                segs.push(h.real_array(&[2, 2], &[0.0, k as f64, cols as f64, k as f64]));
                            }
                            content.push(cx.list(&[h.call(h.sym("gray_level"), &[h.real(0.5)]), h.call(h.sym("absolute_thickness"), &[h.real(0.5)]), h.call(h.sym("line"), &[cx.list(&segs)])]));
                        }
                        "mesh" => {}
                        k if mc_graphics::model::known_option(k) => {
                            opts.retain(|q| !matches!(h.view(*q), View::App { args: &[n, _], .. } if matches!(h.view(n), View::Sym(s2) if h.sym_name(s2) == k)));
                            opts.push(r);
                        }
                        _ => ignored.push(key),
                    }
                }
            }
        }
        let g = h.call(h.sym("graphics"), &[cx.list(&content), h.call(h.sym("options"), &opts)]);
        finish_graphic(cx, g, name, &ignored)
    }
}

/// The value of option `key` in an options term.
fn option<'h>(h: &'h Heap, o: T<'h>, key: &str) -> Option<T<'h>> {
    let View::App { args, .. } = h.view(o) else { return None };
    args.iter().find_map(|&r| match h.view(r) {
        View::App { args: &[k, v], .. } if matches!(h.view(k), View::Sym(s) if h.sym_name(s) == key) => Some(v),
        _ => None,
    })
}

/// `pie_chart([v1, v2, ...])`: a disk cut into sectors in proportion to the values, anticlockwise from the right,
/// in the plot colours. Values may be quantities (magnitudes), `legended(v, label)` (a legend entry for that sector)
/// or `labeled(v, label)` (a label beside it); values that are not non-negative reals have no sector. An association
/// gives each sector its key as a label; several lists (or an association of lists) are concentric rings, the first
/// innermost.
struct PieChart;

impl Native for PieChart {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = "pie_chart";
        let (a, o) = split(h, args);
        let [d] = a.as_slice() else { return Ok(Reply::Decline) };
        // the values of a data container (weighted_data, event_data, time_series)
        let d = &crate::chartdata::container_values(h, *d);
        let mut look = match o {
            Some(o) => plot_options(cx, o, Look::default(), &["labels"], name).unwrap_or_default(),
            None => Look::default(),
        };
        // an association: its keys label the sectors (or name the rings)
        let mut keys = vec![];
        let d = match crate::values::assoc_entries(cx, *d)? {
            Some(entries) => {
                let vals: Vec<T<'h>> = entries.iter().map(|e| e.1).collect();
                keys = entries.into_iter().map(|e| e.0).collect();
                cx.list(&vals)
            }
            None => *d,
        };
        let Some(items) = cx.as_list(d).map(|l| l.to_vec()) else { return Ok(bad(h, name, 1)) };
        let grouped = !items.is_empty() && items.iter().all(|&e| cx.as_list(e).is_some() || crate::values::assoc_entries(cx, e).ok().flatten().is_some());
        let groups: Vec<T<'h>> = if grouped { items } else { vec![d] };
        // per ring, per sector: the value, a legend entry, a label
        let mut rings: Vec<Vec<Sector>> = vec![];
        for g in groups {
            let g = match crate::values::assoc_entries(cx, g)? {
                Some(entries) => {
                    // rings given as associations: the first ring's keys label the sectors
                    if look.labels.is_empty() && rings.is_empty() {
                        look.labels = entries.iter().map(|e| e.0.clone()).collect();
                    }
                    let vals: Vec<T<'h>> = entries.iter().map(|e| e.1).collect();
                    cx.list(&vals)
                }
                None => g,
            };
            let Some(vs) = cx.as_list(g).map(|l| l.to_vec()) else { return Ok(bad(h, name, 1)) };
            let mut ring = vec![];
            for e in vs {
                let (v, legend, label) = match crate::values::call_of(h, e) {
                    Some((n, &[v, l, ..])) if n == "legended" => (v, Some(text_of(h, l)), None),
                    Some((n, &[v, l, ..])) if n == "labeled" || n == "callout" => (v, None, Some(text_of(h, l))),
                    Some((n, &[v, ..])) if n == "style" => (v, None, None),
                    _ => (e, None, None),
                };
                let r = crate::values::read(cx, v)?;
                ring.push((r.v.filter(|v| *v >= 0.0).unwrap_or(0.0), legend, label));
            }
            rings.push(ring);
        }
        if !keys.is_empty() && look.labels.is_empty() && !grouped {
            look.labels = keys.clone();
        }
        // `plot_legends: automatic`: the sectors by their keys
        if look.legend.first().is_some_and(|l| l == crate::AUTO_LEGEND) {
            look.legend = look.labels.clone();
            look.labels.clear();
        }
        let totals: Vec<f64> = rings.iter().map(|r| r.iter().map(|e| e.0).sum()).collect();
        if rings.is_empty() || totals.iter().all(|t| *t <= 0.0) {
            return Ok(bad(h, name, 1));
        }
        let mut content = vec![h.call(h.sym("edge_form"), &[cx.list(&[h.sym_term(h.sym("white")), h.call(h.sym("absolute_thickness"), &[h.real(1.5)])])])];
        let n = rings.len();
        // the outermost ring first: inner rings are drawn over it
        for (g, ring) in rings.iter().enumerate().rev() {
            if totals[g] <= 0.0 {
                continue;
            }
            let radius = (g + 1) as f64 / n as f64;
            let mut a0 = 0.0;
            for (k, (v, _, label)) in ring.iter().enumerate() {
                let a1 = a0 + 2.0 * std::f64::consts::PI * v / totals[g];
                if *v > 0.0 {
                    let (r, gg, b) = look.color(k);
                    content.push(mc_graphics_kernel::rgb_term(h, Rgba::rgb(r as f64 / 255.0, gg as f64 / 255.0, b as f64 / 255.0)));
                    content.push(h.call(h.sym("disk"), &[h.reals(&[0.0, 0.0]), h.real(radius), h.reals(&[a0, a1])]));
                    let l = label.clone().or_else(|| if g + 1 == n { look.labels.get(k).cloned() } else { None });
                    if let Some(l) = l {
                        let m = (a0 + a1) / 2.0;
                        content.push(cx.list(&[h.sym_term(h.sym("black")), h.call(h.sym("text"), &[h.string(&l), h.reals(&[1.18 * m.cos(), 1.18 * m.sin()])])]));
                    }
                }
                a0 = a1;
            }
        }
        // legend entries: the option's, else those of legended values
        if look.legend.is_empty() {
            let own: Vec<Option<String>> = rings.iter().flat_map(|r| r.iter().enumerate().map(|(k, e)| (k, e.1.clone()))).fold(vec![], |mut acc, (k, l)| {
                if acc.len() <= k {
                    acc.resize(k + 1, None);
                }
                if acc[k].is_none() {
                    acc[k] = l;
                }
                acc
            });
            if own.iter().any(Option::is_some) {
                look.legend = own.into_iter().map(Option::unwrap_or_default).collect();
            }
        }
        let rule = |k: &str, v: T<'h>| h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v]);
        let mut opts = vec![rule("image_size", h.real(360.0)), rule("font_size", h.real(13.0))];
        if !look.legend.is_empty() {
            let entries: Vec<T<'h>> = look
                .legend
                .iter()
                .enumerate()
                .filter(|(_, l)| !l.is_empty())
                .map(|(k, l)| {
                    let (r, g, b) = look.color(k);
                    cx.list(&[cx.list(&[mc_graphics_kernel::rgb_term(h, Rgba::rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0))]), h.string(l), h.sym_term(h.sym("swatch"))])
                })
                .collect();
            opts.push(rule("legend", cx.list(&entries)));
        }
        for (k, v) in &look.graphics {
            opts.push(rule(k, *v));
        }
        let g = h.call(h.sym("graphics"), &[cx.list(&content), h.call(h.sym("options"), &opts)]);
        finish_graphic(cx, g, name, &look.ignored)
    }
}

/// A sector of a pie chart: its value, legend entry and label.
type Sector = (f64, Option<String>, Option<String>);

/// A label as text: a string as it is, anything else printed.
fn text_of<'h>(h: &'h Heap, t: T<'h>) -> String {
    match h.view(t) {
        View::Str(s) => s.to_string(),
        _ => mc_syntax::Printer::new(h).expr(t),
    }
}

/// `color_data(name)`: the colour scheme as a function of t in [0, 1]; `color_data(name, t)`: its colour at t;
/// `color_data(k)` with an integer: the k-th plot colour scheme as a function of the index (`color_data(k, i)`
/// the i-th colour); `color_data(name, "ColorList")` / `color_data(name, colors)`: the scheme's colours.
/// The named colours of CSS (W3C CSS Color Module Level 4), as `name rrggbb` pairs.
const CSS_COLORS: &str = "aliceblue f0f8ff antiquewhite faebd7 aqua 00ffff aquamarine 7fffd4 azure f0ffff beige f5f5dc bisque ffe4c4 black 000000 blanchedalmond ffebcd blue 0000ff blueviolet 8a2be2 brown a52a2a burlywood deb887 cadetblue 5f9ea0 chartreuse 7fff00 chocolate d2691e coral ff7f50 cornflowerblue 6495ed cornsilk fff8dc crimson dc143c cyan 00ffff darkblue 00008b darkcyan 008b8b darkgoldenrod b8860b darkgray a9a9a9 darkgreen 006400 darkkhaki bdb76b darkmagenta 8b008b darkolivegreen 556b2f darkorange ff8c00 darkorchid 9932cc darkred 8b0000 darksalmon e9967a darkseagreen 8fbc8f darkslateblue 483d8b darkslategray 2f4f4f darkturquoise 00ced1 darkviolet 9400d3 deeppink ff1493 deepskyblue 00bfff dimgray 696969 dodgerblue 1e90ff firebrick b22222 floralwhite fffaf0 forestgreen 228b22 fuchsia ff00ff gainsboro dcdcdc ghostwhite f8f8ff gold ffd700 goldenrod daa520 gray 808080 green 008000 greenyellow adff2f honeydew f0fff0 hotpink ff69b4 indianred cd5c5c indigo 4b0082 ivory fffff0 khaki f0e68c lavender e6e6fa lavenderblush fff0f5 lawngreen 7cfc00 lemonchiffon fffacd lightblue add8e6 lightcoral f08080 lightcyan e0ffff lightgoldenrodyellow fafad2 lightgray d3d3d3 lightgreen 90ee90 lightpink ffb6c1 lightsalmon ffa07a lightseagreen 20b2aa lightskyblue 87cefa lightslategray 778899 lightsteelblue b0c4de lightyellow ffffe0 lime 00ff00 limegreen 32cd32 linen faf0e6 magenta ff00ff maroon 800000 mediumaquamarine 66cdaa mediumblue 0000cd mediumorchid ba55d3 mediumpurple 9370db mediumseagreen 3cb371 mediumslateblue 7b68ee mediumspringgreen 00fa9a mediumturquoise 48d1cc mediumvioletred c71585 midnightblue 191970 mintcream f5fffa mistyrose ffe4e1 moccasin ffe4b5 navajowhite ffdead navy 000080 oldlace fdf5e6 olive 808000 olivedrab 6b8e23 orange ffa500 orangered ff4500 orchid da70d6 palegoldenrod eee8aa palegreen 98fb98 paleturquoise afeeee palevioletred db7093 papayawhip ffefd5 peachpuff ffdab9 peru cd853f pink ffc0cb plum dda0dd powderblue b0e0e6 purple 800080 red ff0000 rosybrown bc8f8f royalblue 4169e1 saddlebrown 8b4513 salmon fa8072 sandybrown f4a460 seagreen 2e8b57 seashell fff5ee sienna a0522d silver c0c0c0 skyblue 87ceeb slateblue 6a5acd slategray 708090 snow fffafa springgreen 00ff7f steelblue 4682b4 tan d2b48c teal 008080 thistle d8bfd8 tomato ff6347 turquoise 40e0d0 violet ee82ee wheat f5deb3 white ffffff whitesmoke f5f5f5 yellow ffff00 yellowgreen 9acd32";

/// A CSS colour by name (case and spaces ignored).
fn css_color(name: &str) -> Option<Rgba> {
    let n = name.to_ascii_lowercase().replace([' ', '_', '-'], "").replace("grey", "gray");
    let mut it = CSS_COLORS.split(' ');
    while let (Some(k), Some(v)) = (it.next(), it.next()) {
        if k == n {
            let c = u32::from_str_radix(v, 16).ok()?;
            return Some(Rgba::rgb((c >> 16) as f64 / 255.0, (c >> 8 & 255) as f64 / 255.0, (c & 255) as f64 / 255.0));
        }
    }
    None
}

struct ColorData;

/// The colour of light of wavelength `l` nm (380-750): piecewise linear between the hues violet (380), blue (440),
/// cyan (490), green (550), yellow (580), red (645), fading to dark red at 750; clamped outside the range.
fn spectrum(l: f64) -> Rgba {
    const STOPS: [(f64, [f64; 3]); 7] = [
        (380.0, [0.45, 0.0, 0.6]),
        (440.0, [0.0, 0.0, 1.0]),
        (490.0, [0.0, 1.0, 1.0]),
        (550.0, [0.0, 1.0, 0.0]),
        (580.0, [1.0, 1.0, 0.0]),
        (645.0, [1.0, 0.0, 0.0]),
        (750.0, [0.4, 0.0, 0.0]),
    ];
    let l = l.clamp(380.0, 750.0);
    let i = STOPS.iter().rposition(|s| s.0 <= l).unwrap_or(0).min(STOPS.len() - 2);
    let (a, b) = (STOPS[i], STOPS[i + 1]);
    let t = (l - a.0) / (b.0 - a.0);
    let c = |k: usize| a.1[k] + t * (b.1[k] - a.1[k]);
    Rgba::rgb(c(0), c(1), c(2))
}

impl Native for ColorData {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let gradient = |t: T<'h>| -> Option<Gradient> {
            mc_graphics::gradient::read(h, t).or_else(|| match h.view(t) {
                View::Str(s) => Some(mc_graphics::gradient::scheme(s)),
                _ => None,
            })
        };
        let indexed = |k: i64| -> Rgba {
            let (r, g, b) = Look::default().color((k.max(1) - 1) as usize);
            Rgba::rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)
        };
        let word = |t: T<'h>| match h.view(t) {
            View::Str(w) => Some(w.to_string()),
            _ => None,
        };
        match (args, args.first().and_then(|&s| word(s)).as_deref(), args.get(1).and_then(|&t| word(t)).as_deref()) {
            ([], _, _) => {
                let w: Vec<T<'h>> = ["Gradients", "Indexed", "Named"].iter().map(|s| h.string(s)).collect();
                return Ok(Reply::Done(cx.list(&w)));
            }
            ([_], Some("Named"), _) => return Ok(Reply::Done(cx.list(&[h.string("HTML")]))),
            ([_], Some("Gradients"), _) => {
                let w: Vec<T<'h>> = mc_graphics::gradient::NAMED.iter().map(|g| h.string(g.0)).collect();
                return Ok(Reply::Done(cx.list(&w)));
            }
            ([_], Some("Properties"), _) => {
                let w: Vec<T<'h>> = ["ColorFunction", "ColorList", "Image", "Name", "Panel", "ParameterCount", "Range"].iter().map(|s| h.string(s)).collect();
                return Ok(Reply::Done(cx.list(&w)));
            }
            ([_, t], Some("HTML"), _) => {
                return Ok(match word(*t).and_then(|w| css_color(&w)) {
                    Some(c) => Reply::Done(mc_graphics_kernel::rgb_term(h, c)),
                    None if word(*t).is_some_and(|w| w == "Name") => Reply::Done(h.string("HTML")),
                    None if word(*t).is_some_and(|w| w == "ColorList") => {
                        let mut it = CSS_COLORS.split(' ');
                        let mut cs = vec![];
                        while let (Some(k), Some(_)) = (it.next(), it.next()) {
                            cs.extend(css_color(k).map(|c| mc_graphics_kernel::rgb_term(h, c)));
                        }
                        Reply::Done(cx.list(&cs))
                    }
                    None => Reply::Decline,
                });
            }
            ([_, _], Some("VisibleSpectrum"), Some("Range")) => return Ok(Reply::Done(cx.list(&[h.int(380), h.int(750)]))),
            ([_, t], Some("VisibleSpectrum"), None) => {
                return Ok(match mc_graphics::num::number(h, *t) {
                    Some(l) => Reply::Done(mc_graphics_kernel::rgb_term(h, spectrum(l))),
                    None => Reply::Decline,
                });
            }
            ([s, _], _, Some("Name")) => return Ok(Reply::Done(*s)),
            ([_, _], _, Some("ParameterCount")) => return Ok(Reply::Done(h.int(1))),
            ([s, _], _, Some("Range")) if gradient(*s).is_some() && !matches!(h.view(*s), View::Int(_)) => return Ok(Reply::Done(cx.list(&[h.int(0), h.int(1)]))),
            ([s, _], _, Some("ColorFunction")) => return Ok(Reply::Again(h.call(h.sym("color_data"), &[*s]))),
            ([s, _], _, Some(w @ ("Image" | "Panel"))) => {
                // the scheme as a bar of colours (a panel: framed, with its range)
                let cols: Vec<Rgba> = match h.view(*s) {
                    View::Int(_) => (1..=7).map(indexed).collect(),
                    _ => match gradient(*s) {
                        Some(g) => (0..64).map(|k| g.at(k as f64 / 63.0)).collect(),
                        None => return Ok(Reply::Decline),
                    },
                };
                let n = cols.len();
                let r = crate::charts2::raster_of(h, &cols, n, 1, (0.0, 1.0, 0.0, 0.12));
                let mut opts = vec![h.call(h.sym(names::RULE), &[h.sym_term(h.sym("image_size")), h.int(if w == "Panel" { 220 } else { 180 })]), h.call(h.sym(names::RULE), &[h.sym_term(h.sym("image_padding")), h.sym_term(h.sym("none"))])];
                if w == "Panel" {
                    opts.push(h.call(h.sym(names::RULE), &[h.sym_term(h.sym("frame")), h.sym_term(h.sym("true"))]));
                    opts.push(h.call(h.sym(names::RULE), &[h.sym_term(h.sym("plot_label")), *s]));
                }
                let g = h.call(h.sym("graphics"), &[cx.list(&[r]), h.call(h.sym("options"), &opts)]);
                return finish_graphic(cx, g, "color_data", &[]);
            }
            _ => {}
        }
        match args {
            [s] => {
                // a function: `t => color_data(s, t)`
                let src = match h.view(*s) {
                    View::Int(_) | View::Str(_) | View::Sym(_) => format!("t_color_data => color_data({}, t_color_data)", mc_syntax::Printer::new(h).expr(*s)),
                    _ => return Ok(Reply::Decline),
                };
                if matches!(h.view(*s), View::Sym(_)) && gradient(*s).is_none() {
                    return Ok(Reply::Decline);
                }
                match mc_syntax::read(h, &src) {
                    Ok(v) if v.len() == 1 => Ok(Reply::Again(v[0])),
                    _ => Ok(Reply::Decline),
                }
            }
            [s, t] => {
                let list_word = matches!(h.view(*t), View::Str(w) if w.to_ascii_lowercase().replace(' ', "") == "colorlist") || matches!(h.view(*t), View::Sym(w) if h.sym_name(w) == "colors");
                if let View::Int(k) = h.view(*s) {
                    if list_word {
                        let cs: Vec<T<'h>> = (1..=7).map(|i| mc_graphics_kernel::rgb_term(h, indexed(i))).collect();
                        return Ok(Reply::Done(cx.list(&cs)));
                    }
                    let _ = k;
                    return Ok(match h.view(*t) {
                        View::Int(i) => Reply::Done(mc_graphics_kernel::rgb_term(h, indexed(i))),
                        _ => Reply::Decline,
                    });
                }
                let Some(g) = gradient(*s) else { return Ok(Reply::Decline) };
                if list_word {
                    let cs: Vec<T<'h>> = g.stops().iter().map(|&c| mc_graphics_kernel::rgb_term(h, c)).collect();
                    return Ok(Reply::Done(cx.list(&cs)));
                }
                Ok(match real(cx, *t)? {
                    Some(v) => Reply::Done(mc_graphics_kernel::rgb_term(h, g.at(v))),
                    None => Reply::Decline,
                })
            }
            _ => Ok(Reply::Decline),
        }
    }
}
