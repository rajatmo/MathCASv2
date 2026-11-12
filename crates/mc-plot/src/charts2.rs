#![allow(clippy::type_complexity, clippy::needless_range_loop)]
//! More charts and pictures: `bubble_chart`, `box_whisker_chart`, `sector_chart`, `rectangle_chart`,
//! `paired_bar_chart`, `density_histogram`, `smooth_histogram`, `number_line_plot`, the fractals
//! `mandelbrot_set_plot` and `julia_set_plot`, `relief_plot`, `complex_array_plot`, `complex_list_plot`,
//! `list_step_plot`, the gauges (`angular_gauge`, `horizontal_gauge`, `vertical_gauge`), and `rasterize`.
use crate::figure::{Extra, Look};
use crate::more::split;
use crate::{bad, finish, finish_graphic, plot_options, real};
use mc_api::names;
use std::sync::atomic::Ordering;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::Rgba;
use mc_graphics::gradient::Gradient;

type Body = for<'a, 'b, 'h> fn(&mut NativeCx<'a, 'b, 'h>, &[T<'h>], Option<T<'h>>) -> Res<Reply<'h>>;

struct Chart(&'static str, Body);

impl Native for Chart {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (a, o) = split(h, args);
        cx.tick(1)?;
        // an undefined name as the data: unevaluated in effect (a failure naming the argument), as for any value
        // that is not data
        if let Some(&f) = a.first() {
            // also `name -> labels`
            let f = match crate::values::call_of(h, f) {
                Some((n, &[x, _])) if n == mc_api::names::RULE => x,
                _ => f,
            };
            if matches!(h.view(f), View::Sym(_)) && self.0 != "rasterize" {
                return Ok(crate::bad(h, self.0, 1));
            }
        }
        (self.1)(cx, &a, o)
    }
}

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let a = Attributes::default;
    let charts: [(&'static str, Body); 18] = [
        ("bubble_chart", crate::charts3::bubble_chart),
        ("box_whisker_chart", crate::charts3::box_whisker_chart),
        ("sector_chart", crate::charts3::sector_chart),
        ("rectangle_chart", crate::charts3::rectangle_chart),
        ("paired_bar_chart", crate::charts3::paired_bar_chart),
        ("density_histogram", density_histogram),
        ("smooth_histogram", crate::charts3::smooth_histogram),
        ("number_line_plot", number_line_plot),
        ("mandelbrot_set_plot", mandelbrot_set_plot),
        ("julia_set_plot", julia_set_plot),
        ("relief_plot", relief_plot),
        ("complex_array_plot", complex_array_plot),
        ("complex_list_plot", crate::charts3::complex_list_plot),
        ("list_step_plot", list_step_plot),
        ("angular_gauge", |cx, a, o| gauge(cx, a, o, Gauge::Angular)),
        ("horizontal_gauge", |cx, a, o| gauge(cx, a, o, Gauge::Horizontal)),
        ("vertical_gauge", |cx, a, o| gauge(cx, a, o, Gauge::Vertical)),
        ("rasterize", rasterize),
    ];
    for (name, f) in charts {
        k.register(h.sym(name), a(), Box::new(Chart(name, f)));
    }
}

fn list<'h>(h: &'h Heap, items: &[T<'h>]) -> T<'h> {
    h.call(h.sym(names::LIST), items)
}

fn rgb_of((r, g, b): (u8, u8, u8)) -> Rgba {
    Rgba::rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)
}

/// A vector of machine reals.
fn reals<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<f64>>> {
    let Some(items) = cx.as_list(t).map(|l| l.to_vec()) else { return Ok(None) };
    let mut out = Vec::with_capacity(items.len());
    for (i, e) in items.into_iter().enumerate() {
        if i % 4096 == 4095 {
            cx.tick(1)?;
        }
        match real(cx, e)? {
            Some(v) => out.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// A matrix of machine reals (rows).
fn matrix<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<Vec<f64>>>> {
    let Some(rows) = cx.as_list(t).map(|l| l.to_vec()) else { return Ok(None) };
    let mut out = vec![];
    for r in rows {
        match reals(cx, r)? {
            Some(v) => out.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}


fn look_of<'h>(cx: &NativeCx<'_, '_, 'h>, o: Option<T<'h>>, base: Look<'h>, extra: &[&str], who: &'static str) -> Look<'h> {
    match o {
        Some(o) => plot_options(cx, o, base, extra, who).unwrap_or_default(),
        None => Look { who, ..base },
    }
}


const CHART_OPTS: &[&str] = &["chart_labels", "chart_style", "chart_element_function", "chart_layout", "bar_spacing", "chart_base_style", "labeling_function", "bubble_sizes", "bubble_scale", "sector_origin", "sector_spacing", "polar_axes", "polar_grid_lines", "gauge_labels", "gauge_markers", "gauge_style", "gauge_faces", "scale_divisions", "scale_ranges", "scale_range_style", "target_units", "spacings", "color_function", "max_iterations", "escape_radius", "plot_points"];

// ---- bubble chart


// ---- box and whisker chart


// ---- sector, rectangle and paired bar charts


fn picture<'h>(cx: &mut NativeCx<'_, '_, 'h>, prims: Vec<T<'h>>, opts: Vec<T<'h>>, who: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    let g = h.call(h.sym("graphics"), &[list(h, &prims), h.call(h.sym("options"), &opts)]);
    finish_graphic(cx, g, who, &[])
}

fn rule<'h>(h: &'h Heap, k: &str, v: T<'h>) -> T<'h> {
    h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v])
}




// ---- histograms

fn density_histogram<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, ..] = a else { return Ok(Reply::Decline) };
    let Some(m) = matrix(cx, *d)?.filter(|m| !m.is_empty() && m.iter().all(|r| r.len() == 2)) else { return Ok(bad(h, "density_histogram", 1)) };
    let n = ((m.len() as f64).sqrt().ceil() as usize).clamp(4, 30);
    let span = |k: usize| {
        let lo = m.iter().map(|r| r[k]).fold(f64::INFINITY, f64::min);
        let hi = m.iter().map(|r| r[k]).fold(f64::NEG_INFINITY, f64::max);
        (lo, if hi > lo { hi } else { lo + 1.0 })
    };
    let ((x0, x1), (y0, y1)) = (span(0), span(1));
    let mut counts = vec![0usize; n * n];
    for r in &m {
        let i = (((r[0] - x0) / (x1 - x0) * n as f64) as usize).min(n - 1);
        let j = (((r[1] - y0) / (y1 - y0) * n as f64) as usize).min(n - 1);
        counts[j * n + i] += 1;
    }
    let cmax = counts.iter().copied().max().unwrap_or(1).max(1) as f64;
    let look = look_of(cx, o, Look { exact: true, ..Look::default() }, CHART_OPTS, "density_histogram");
    let g = look.gradient.clone().unwrap_or_else(Gradient::default_gradient);
    let mut under = vec![h.call(h.sym("edge_form"), &[h.sym_term(h.sym("none"))])];
    let (dx, dy) = ((x1 - x0) / n as f64, (y1 - y0) / n as f64);
    for j in 0..n {
        for i in 0..n {
            let c = counts[j * n + i];
            if c == 0 {
                continue;
            }
            let col = g.at(c as f64 / cmax);
            under.push(list(h, &[mc_graphics_kernel::rgb_term(h, col), h.call(h.sym("rectangle"), &[h.reals(&[x0 + dx * i as f64, y0 + dy * j as f64]), h.reals(&[x0 + dx * (i + 1) as f64, y0 + dy * (j + 1) as f64])])]));
        }
    }
    let extra = Extra { under, bounds: Some([(x0, x1), (y0, y1)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}


// ---- number line

/// The intervals of [a, b] where `m(x) >= 0`, by sampling and bisection.
fn intervals_where<'h>(cx: &mut NativeCx<'_, '_, 'h>, m: T<'h>, x: Sym, a: f64, b: f64) -> Res<Vec<(f64, f64)>> {
    let mut f = mc_numfn::compile_expr(cx, m, &[mc_numfn::Var::scalar(x)], &mc_numfn::Options::default())?;
    let mut out = [0.0];
    let mut val = |cx: &mut NativeCx<'_, '_, 'h>, t: f64| -> Res<bool> { Ok(f.eval(cx, &[t], &mut out)? && out[0] >= 0.0) };
    let n = 2000;
    let mut res = vec![];
    let mut start: Option<f64> = None;
    let mut prev = val(cx, a)?;
    if prev {
        start = Some(a);
    }
    for k in 1..=n {
        let t = a + (b - a) * k as f64 / n as f64;
        let cur = val(cx, t)?;
        if cur != prev {
            let (mut lo, mut hi) = (t - (b - a) / n as f64, t);
            for _ in 0..50 {
                let mid = 0.5 * (lo + hi);
                if val(cx, mid)? == prev {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let e = 0.5 * (lo + hi);
            if prev {
                res.push((start.take().unwrap_or(a), e));
            } else {
                start = Some(e);
            }
        }
        prev = cur;
    }
    if let Some(s) = start {
        res.push((s, b));
    }
    Ok(res)
}

fn number_line_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (items, var, range) = match a {
        [d] => (*d, None, None),
        [d, v] if matches!(h.view(*v), View::Sym(_)) => (*d, Some(*v), None),
        [d, s] => match cx.as_list(*s) {
            Some(&[v, lo, hi]) => (*d, Some(v), Some((lo, hi))),
            _ => return Ok(bad(h, "number_line_plot", 2)),
        },
        _ => return Ok(Reply::Decline),
    };
    let rows: Vec<T<'h>> = match cx.as_list(items) {
        Some(l) if l.iter().any(|&e| cx.as_list(e).is_some()) => l.to_vec(),
        _ => vec![items],
    };
    // each row: points and intervals
    let mut drawn: Vec<(Vec<f64>, Vec<(f64, f64)>)> = vec![];
    let mut conds = vec![];
    for (k, &r) in rows.iter().enumerate() {
        let parts: Vec<T<'h>> = cx.as_list(r).map(|l| l.to_vec()).unwrap_or_else(|| vec![r]);
        let mut pts = vec![];
        let mut ivs = vec![];
        for p in parts {
            if let Some(v) = real(cx, p)? {
                pts.push(v);
                continue;
            }
            if let Some((n, args)) = mc_geom::region::call_parts(h, p) {
                if n == "interval" {
                    for &iv in args {
                        if let Some(&[lo, hi]) = cx.as_list(iv) {
                            let (lo, hi) = (real(cx, lo)?.unwrap_or(f64::NEG_INFINITY), real(cx, hi)?.unwrap_or(f64::INFINITY));
                            ivs.push((lo, hi));
                        }
                    }
                    continue;
                }
            }
            conds.push((k, p));
        }
        drawn.push((pts, ivs));
    }
    // the range: given, or around the numbers (default -10 .. 10 for conditions)
    let finite: Vec<f64> = drawn.iter().flat_map(|(p, i)| p.iter().copied().chain(i.iter().flat_map(|&(a, b)| [a, b]))).filter(|v| v.is_finite()).collect();
    let (mut lo, mut hi) = match range {
        Some((a, b)) => match (real(cx, a)?, real(cx, b)?) {
            (Some(a), Some(b)) => (a, b),
            _ => return Ok(bad(h, "number_line_plot", 2)),
        },
        None if finite.is_empty() => (-10.0, 10.0),
        None => {
            let a = finite.iter().copied().fold(f64::INFINITY, f64::min);
            let b = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let pad = ((b - a) * 0.15).max(1.0);
            (a - pad, b + pad)
        }
    };
    if !conds.is_empty() {
        let Some(View::Sym(x)) = var.map(|v| h.view(v)) else { return Ok(bad(h, "number_line_plot", 2)) };
        if range.is_none() && finite.is_empty() {
            (lo, hi) = (-10.0, 10.0);
        }
        for (k, c) in conds {
            let m = mc_geom::margin(h, c).unwrap_or_else(|| h.call(h.sym("if"), &[c, h.int(1), h.int(-1)]));
            let ivs = intervals_where(cx, m, x, lo, hi)?;
            for (a, b) in ivs {
                if b - a < (hi - lo) * 1e-6 {
                    drawn[k].0.push(0.5 * (a + b));
                } else {
                    drawn[k].1.push((a, b));
                }
            }
        }
    }
    let look = look_of(cx, o, Look::default(), CHART_OPTS, "number_line_plot");
    let mut prims = vec![];
    let nrows = drawn.len();
    for (k, (pts, ivs)) in drawn.iter().enumerate() {
        let y = (nrows - k) as f64;
        let col = mc_graphics_kernel::rgb_term(h, rgb_of(look.color(k)));
        let mut part = vec![col, h.call(h.sym("absolute_thickness"), &[h.real(3.0)]), h.call(h.sym("absolute_point_size"), &[h.real(8.0)])];
        for &(a, b) in ivs {
            let (a2, b2) = (a.max(lo), b.min(hi));
            part.push(h.call(h.sym("line"), &[list(h, &[h.reals(&[a2, y]), h.reals(&[b2, y])])]));
            for e in [a, b] {
                if e.is_finite() && e >= lo && e <= hi {
                    part.push(h.call(h.sym("point"), &[h.reals(&[e, y])]));
                }
            }
        }
        if !pts.is_empty() {
            part.push(h.call(h.sym("point"), &[list(h, &pts.iter().map(|&v| h.reals(&[v, y])).collect::<Vec<_>>())]));
        }
        prims.push(list(h, &part));
    }
    let opts = vec![
        rule(h, "axes", list(h, &[h.sym_term(h.sym("true")), h.sym_term(h.sym("false"))])),
        rule(h, "plot_range", list(h, &[h.reals(&[lo, hi]), h.reals(&[0.0, nrows as f64 + 1.0])])),
        rule(h, "aspect_ratio", h.real(0.12 * (nrows as f64 + 1.0))),
        rule(h, "axes_origin", h.reals(&[lo, 0.0])),
    ];
    picture(cx, prims, opts, "number_line_plot")
}

// ---- fractals, relief, complex arrays

pub(crate) fn raster_of<'h>(h: &'h Heap, cols: &[Rgba], nx: usize, ny: usize, (x0, x1, y0, y1): (f64, f64, f64, f64)) -> T<'h> {
    // rows from the bottom up
    let mut data = Vec::with_capacity(nx * ny * 4);
    for c in cols {
        data.extend_from_slice(&[c.r, c.g, c.b, c.a]);
    }
    h.call(h.sym("raster"), &[h.real_array(&[ny, nx, 4], &data), list(h, &[h.reals(&[x0, y0]), h.reals(&[x1, y1])])])
}

fn escape_picture<'h>(cx: &mut NativeCx<'_, '_, 'h>, o: Option<T<'h>>, rect: (f64, f64, f64, f64), who: &'static str, julia: Option<(f64, f64)>) -> Res<Reply<'h>> {
    let (x0, x1, y0, y1) = rect;
    let look = look_of(cx, o, Look { exact: true, equal: true, ..Look::default() }, CHART_OPTS, who);
    let n = look.plot_points.unwrap_or(400).clamp(50, 1200);
    let ny = ((n as f64) * (y1 - y0) / (x1 - x0)).round().max(10.0) as usize;
    let max_iter = 200;
    let g = look.gradient.clone().unwrap_or_else(|| Gradient::named("ocean").unwrap_or_else(Gradient::default_gradient));
    let mut cols = Vec::with_capacity(n * ny);
    for j in 0..ny {
        if j % 16 == 0 {
            cx.tick(1)?;
        }
        let y = y0 + (y1 - y0) * (j as f64 + 0.5) / ny as f64;
        for i in 0..n {
            let x = x0 + (x1 - x0) * (i as f64 + 0.5) / n as f64;
            let (mut zr, mut zi, cr, ci) = match julia {
                Some((cr, ci)) => (x, y, cr, ci),
                None => (0.0, 0.0, x, y),
            };
            let mut k = 0;
            while k < max_iter && zr * zr + zi * zi <= 4.0 {
                let t = zr * zr - zi * zi + cr;
                zi = 2.0 * zr * zi + ci;
                zr = t;
                k += 1;
            }
            cols.push(if k == max_iter {
                Rgba::rgb(0.0, 0.0, 0.0)
            } else {
                // smooth escape count
                let mu = k as f64 + 1.0 - ((zr * zr + zi * zi).sqrt().ln().ln() / std::f64::consts::LN_2);
                g.at((mu / 40.0).fract().abs())
            });
        }
    }
    let h = cx.heap();
    let r = raster_of(h, &cols, n, ny, rect);
    let extra = Extra { under: vec![r], bounds: Some([(x0, x1), (y0, y1)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

/// A complex number's real and imaginary parts.
fn complex_parts<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<(f64, f64)>> {
    let h = cx.heap();
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(match h.view(v) {
        View::Int(a) => Some((a as f64, 0.0)),
        View::Real(a) => Some((a, 0.0)),
        View::App { head, args: &[a, b] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::COMPLEX) => {
            mc_graphics::num::number(h, a).zip(mc_graphics::num::number(h, b))
        }
        _ => None,
    })
}

fn mandelbrot_set_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let rect = match a {
        [] => (-2.2, 0.8, -1.3, 1.3),
        [r] => {
            let Some(&[p, q]) = cx.as_list(*r) else { return Ok(bad(h, "mandelbrot_set_plot", 1)) };
            let (Some(p), Some(q)) = (complex_parts(cx, p)?, complex_parts(cx, q)?) else { return Ok(bad(h, "mandelbrot_set_plot", 1)) };
            (p.0.min(q.0), p.0.max(q.0), p.1.min(q.1), p.1.max(q.1))
        }
        _ => return Ok(Reply::Decline),
    };
    if !(rect.1 > rect.0 && rect.3 > rect.2) {
        return Ok(bad(h, "mandelbrot_set_plot", 1));
    }
    escape_picture(cx, o, rect, "mandelbrot_set_plot", None)
}

fn julia_set_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let c = match a {
        [c] | [c, _] => *c,
        _ => return Ok(Reply::Decline),
    };
    // julia_set_plot(f, z): only z^2 + c is drawn; its c is the value of f at z = 0
    let c = match a {
        [f, z] if matches!(h.view(*z), View::Sym(_)) => h.call(h.sym("rewrite"), &[*f, h.call(h.sym(names::RULE), &[*z, h.int(0)])]),
        _ => c,
    };
    let Some(c) = complex_parts(cx, c)? else { return Ok(bad(h, "julia_set_plot", 1)) };
    // the filled set lies in |z| <= R with R = max(2, |c|)
    let r = 2.0f64.max(c.0.hypot(c.1)) * 0.8 + 0.4;
    escape_picture(cx, o, (-r, r, -r * 0.8, r * 0.8), "julia_set_plot", Some(c))
}

fn relief_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d] = a else { return Ok(Reply::Decline) };
    let Some(m) = matrix(cx, *d)?.filter(|m| m.len() >= 2 && m[0].len() >= 2 && m.iter().all(|r| r.len() == m[0].len())) else { return Ok(bad(h, "relief_plot", 1)) };
    let (ny, nx) = (m.len(), m[0].len());
    let lo = m.iter().flatten().copied().fold(f64::INFINITY, f64::min);
    let hi = m.iter().flatten().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (hi - lo).max(1e-300);
    let look = look_of(cx, o, Look { exact: true, ..Look::default() }, CHART_OPTS, "relief_plot");
    let g = look.gradient.clone().unwrap_or_else(|| Gradient::named("earth").unwrap_or_else(Gradient::default_gradient));
    let mut cols = Vec::with_capacity(nx * ny);
    // light from the upper left; slopes in units of the value range per cell
    let at = |i: isize, j: isize| m[j.clamp(0, ny as isize - 1) as usize][i.clamp(0, nx as isize - 1) as usize];
    for j in 0..ny {
        for i in 0..nx {
            let (i, j) = (i as isize, j as isize);
            let gx = (at(i + 1, j) - at(i - 1, j)) / (2.0 * span) * nx as f64 * 0.5;
            let gy = (at(i, j + 1) - at(i, j - 1)) / (2.0 * span) * ny as f64 * 0.5;
            // the matrix's first row is the top of the picture: y grows downwards in it
            let nrm = (gx * gx + gy * gy + 1.0).sqrt();
            let shade = ((-gx * -0.6 + gy * -0.6 + 0.55) / nrm).clamp(0.0, 1.0);
            let base = g.at((at(i, j) - lo) / span);
            let s = 0.55 + 0.45 * shade;
            cols.push(Rgba::rgb(base.r * s, base.g * s, base.b * s));
        }
    }
    // raster rows from the bottom up: the matrix's last row first
    let mut flipped = Vec::with_capacity(cols.len());
    for j in (0..ny).rev() {
        flipped.extend_from_slice(&cols[j * nx..(j + 1) * nx]);
    }
    let r = raster_of(h, &flipped, nx, ny, (0.0, nx as f64, 0.0, ny as f64));
    let extra = Extra { under: vec![r], bounds: Some([(0.0, nx as f64), (0.0, ny as f64)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

fn complex_array_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d] = a else { return Ok(Reply::Decline) };
    let Some(rows) = cx.as_list(*d).map(|l| l.to_vec()) else { return Ok(bad(h, "complex_array_plot", 1)) };
    let mut vals = vec![];
    let mut nx = 0;
    for r in &rows {
        let Some(items) = cx.as_list(*r).map(|l| l.to_vec()) else { return Ok(bad(h, "complex_array_plot", 1)) };
        nx = nx.max(items.len());
        let mut row = vec![];
        for e in items {
            row.push(complex_parts(cx, e)?);
        }
        vals.push(row);
    }
    let ny = vals.len();
    if ny == 0 || nx == 0 {
        return Ok(bad(h, "complex_array_plot", 1));
    }
    let mut cols = vec![];
    for row in vals.iter().rev() {
        for i in 0..nx {
            cols.push(match row.get(i).copied().flatten() {
                Some((re, im)) => crate::more::domain_colour(re, im),
                None => Rgba::rgb(1.0, 1.0, 1.0),
            });
        }
    }
    let look = look_of(cx, o, Look { exact: true, equal: true, ..Look::default() }, CHART_OPTS, "complex_array_plot");
    let r = raster_of(h, &cols, nx, ny, (0.0, nx as f64, 0.0, ny as f64));
    let extra = Extra { under: vec![r], bounds: Some([(0.0, nx as f64), (0.0, ny as f64)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}


fn list_step_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, rest @ ..] = a else { return Ok(Reply::Decline) };
    // where the step sits: left (default: the value holds from its point to the next), right, center
    let side = rest.first().and_then(|&s| match h.view(s) {
        View::Sym(s) => Some(h.sym_name(s)),
        View::Str(s) => Some(s.to_lowercase()),
        _ => None,
    });
    // one data set as points: `[y1, ...]` (x = 1, 2, ...), `[[x1, y1], ...]`, an association (numeric keys give the
    // x values), a list of quantities; `none` and `missing(...)` entries are gaps (a point without a value)
    let to_pts = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<Vec<(f64, Option<f64>)>>> {
        let t = match crate::values::call_of(h, t) {
            Some((n, &[l, _])) if n == "quantity" => l,
            _ => t,
        };
        let t = if matches!(crate::values::call_of(h, t), Some((n, _)) if n == "assoc") {
            let keys = cx.eval(h.call(h.sym("keys"), &[t]))?;
            let vals = cx.eval(h.call(h.sym("values"), &[t]))?;
            let numeric = match cx.as_list(keys) {
                Some(ks) => {
                    let ks = ks.to_vec();
                    let mut all = !ks.is_empty();
                    for k in ks {
                        all &= real(cx, k)?.is_some();
                    }
                    all
                }
                None => false,
            };
            if numeric {
                cx.eval(h.call(h.sym("transpose"), &[cx.list(&[keys, vals])]))?
            } else {
                vals
            }
        } else {
            t
        };
        let Some(items) = cx.as_list(t).map(|l| l.to_vec()) else { return Ok(None) };
        let mut out = Vec::with_capacity(items.len());
        for (k, &it) in items.iter().enumerate() {
            let gap = |t: T<'h>| match h.view(t) {
                View::Sym(s) => matches!(h.sym_name(s).as_str(), "none" | "missing"),
                _ => matches!(crate::values::call_of(h, t), Some((n, _)) if n == "missing"),
            };
            if let Some(&[x, y]) = cx.as_list(it) {
                let Some(x) = real(cx, x)? else { return Ok(None) };
                if gap(y) {
                    out.push((x, None));
                    continue;
                }
                match crate::values::read(cx, y)?.v {
                    Some(y) => out.push((x, Some(y))),
                    None => return Ok(None),
                }
            } else if gap(it) {
                out.push((k as f64 + 1.0, None));
            } else {
                match crate::values::read(cx, it)?.v {
                    Some(y) => out.push((k as f64 + 1.0, Some(y))),
                    None => return Ok(None),
                }
            }
        }
        Ok(Some(out))
    };
    // wrappers around a data set (`legended`, `style`, `labeled`, `tooltip`, ...) are kept around its step lines
    let wrapper = |t: T<'h>| -> Option<(T<'h>, String, Vec<T<'h>>)> {
        let (n, args) = crate::values::call_of(h, t)?;
        (matches!(n.as_str(), "legended" | "labeled" | "callout" | "style" | "tooltip" | "highlighted") && !args.is_empty()).then(|| (args[0], n, args[1..].to_vec()))
    };
    let peel = |mut t: T<'h>| -> (T<'h>, Vec<(String, Vec<T<'h>>)>) {
        let mut layers = vec![];
        while let Some((inner, n, rest)) = wrapper(t) {
            layers.push((n, rest));
            t = inner;
        }
        (t, layers)
    };
    let (d0, outer) = peel(*d);
    let wrapped_sets = cx.as_list(d0).is_some_and(|l| l.iter().any(|&e| wrapper(e).is_some()));
    let sets = if wrapped_sets { cx.as_list(d0).map(|l| l.to_vec()) } else { crate::data_sets(cx, d0).filter(|v| !(v.len() == 1 && v[0] == d0)) };
    let (items, set_layers): (Vec<T<'h>>, Vec<Vec<(String, Vec<T<'h>>)>>) = match sets {
        Some(sets) => {
            let (mut its, mut lays) = (vec![], vec![]);
            for e in sets {
                let (core, ls) = peel(e);
                its.push(core);
                lays.push(outer.iter().cloned().chain(ls).collect());
            }
            (its, lays)
        }
        None => (vec![d0], vec![outer.clone()]),
    };
    let mut lines = vec![];
    for (set, layers) in items.iter().zip(&set_layers) {
        let Some(p) = to_pts(cx, *set)? else { return Ok(bad(h, "list_step_plot", 1)) };
        if p.is_empty() {
            continue;
        }
        let mut out = vec![];
        let n = p.len();
        let width = if n > 1 { p[n - 1].0 - p[n - 2].0 } else { 1.0 };
        for k in 0..n {
            let (x, y) = p[k];
            let next = if k + 1 < n { p[k + 1].0 } else { x + width };
            let prev = if k > 0 { p[k - 1].0 } else { x - width };
            let (a, b) = match side.as_deref() {
                Some("right") => (prev, x),
                Some("center") => (0.5 * (prev + x), 0.5 * (x + next)),
                _ => (x, next),
            };
            match y {
                Some(y) => {
                    out.push(h.reals(&[a, y]));
                    out.push(h.reals(&[b, y]));
                }
                None => out.push(list(h, &[h.real(a), h.sym_term(h.sym("none"))])),
            }
        }
        let mut t = list(h, &out);
        for (n, rest) in layers.iter().rev() {
            let mut a = vec![t];
            a.extend(rest.iter().copied());
            t = h.call(h.sym(n), &a);
        }
        lines.push(t);
    }
    let data = list(h, &lines);
    let mut args = vec![data];
    args.extend(o);
    Ok(Reply::Again(h.call(h.sym("list_line_plot"), &args)))
}

// ---- gauges

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gauge {
    Angular,
    Horizontal,
    Vertical,
}

fn gauge<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>, kind: Gauge) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = match kind {
        Gauge::Angular => "angular_gauge",
        Gauge::Horizontal => "horizontal_gauge",
        Gauge::Vertical => "vertical_gauge",
    };
    let (v, lo, hi) = match a {
        [v] => (*v, 0.0, 1.0),
        [v, r] => match cx.as_list(*r) {
            Some(&[lo, hi]) => match (crate::values::read(cx, lo)?.v, crate::values::read(cx, hi)?.v) {
                (Some(lo), Some(hi)) if hi > lo => (*v, lo, hi),
                (Some(lo), Some(hi)) if hi < lo => (*v, hi, lo),
                _ => return Ok(bad(h, who, 2)),
            },
            _ => return Ok(bad(h, who, 2)),
        },
        _ => return Ok(Reply::Decline),
    };
    // one value or several
    let vals: Vec<f64> = {
        let n = crate::chartdata::read(cx, v)?;
        match n.num() {
            Some(x) => vec![x],
            None => match n.numbers() {
                Some(vs) if !vs.is_empty() => vs,
                _ => return Ok(bad(h, who, 1)),
            },
        }
    };
    let look = look_of(cx, o, Look::default(), CHART_OPTS, who);
    let ticks = mc_graphics::ticks::positions(lo, hi, 6);
    let label = |v: f64| h.string(&mc_graphics::num::short(v));
    let mut prims = vec![];
    let gray = h.call(h.sym("gray_level"), &[h.real(0.35)]);
    let mut opts = vec![];
    match kind {
        Gauge::Angular => {
            // the scale runs clockwise over 270 degrees from the lower left
            let ang = |v: f64| (225.0 - 270.0 * (v - lo) / (hi - lo)).to_radians();
            prims.push(list(h, &[h.call(h.sym("gray_level"), &[h.real(0.93)]), h.call(h.sym("disk"), &[h.reals(&[0.0, 0.0]), h.real(1.05)])]));
            let mut arc = vec![];
            for k in 0..=90 {
                let t = ang(lo + (hi - lo) * k as f64 / 90.0);
                arc.push(h.reals(&[0.9 * t.cos(), 0.9 * t.sin()]));
            }
            prims.push(list(h, &[gray, h.call(h.sym("absolute_thickness"), &[h.real(1.5)]), h.call(h.sym("line"), &[list(h, &arc)])]));
            for &t in &ticks {
                let a = ang(t);
                prims.push(list(h, &[gray, h.call(h.sym("line"), &[list(h, &[h.reals(&[0.9 * a.cos(), 0.9 * a.sin()]), h.reals(&[0.8 * a.cos(), 0.8 * a.sin()])])])]));
                prims.push(h.call(h.sym("text"), &[label(t), h.reals(&[0.66 * a.cos(), 0.66 * a.sin()])]));
            }
            for (k, &x) in vals.iter().enumerate() {
                let a = ang(x.clamp(lo, hi));
                let col = mc_graphics_kernel::rgb_term(h, rgb_of(look.color(k)));
                prims.push(list(h, &[col, h.call(h.sym("absolute_thickness"), &[h.real(3.0)]), h.call(h.sym("line"), &[list(h, &[h.reals(&[0.0, 0.0]), h.reals(&[0.85 * a.cos(), 0.85 * a.sin()])])])]));
            }
            prims.push(list(h, &[gray, h.call(h.sym("disk"), &[h.reals(&[0.0, 0.0]), h.real(0.06)])]));
            opts.push(rule(h, "aspect_ratio", h.sym_term(h.sym("automatic"))));
            opts.push(rule(h, "plot_range", list(h, &[h.reals(&[-1.1, 1.1]), h.reals(&[-1.1, 1.1])])));
        }
        Gauge::Horizontal | Gauge::Vertical => {
            let vert = kind == Gauge::Vertical;
            let p = |s: f64, c: f64| if vert { h.reals(&[c, s]) } else { h.reals(&[s, c]) };
            let span = hi - lo;
            prims.push(list(h, &[h.call(h.sym("gray_level"), &[h.real(0.9)]), h.call(h.sym("rectangle"), &[p(lo, -0.04 * span), p(hi, 0.04 * span)])]));
            for &t in &ticks {
                prims.push(list(h, &[gray, h.call(h.sym("line"), &[list(h, &[p(t, -0.06 * span), p(t, -0.1 * span)])])]));
                prims.push(h.call(h.sym("text"), &[label(t), p(t, -0.18 * span)]));
            }
            for (k, &x) in vals.iter().enumerate() {
                let col = mc_graphics_kernel::rgb_term(h, rgb_of(look.color(k)));
                prims.push(list(h, &[col, h.call(h.sym("rectangle"), &[p(lo, -0.04 * span), p(x.clamp(lo, hi), 0.04 * span)])]));
                prims.push(list(h, &[col, h.call(h.sym("polygon"), &[list(h, &[p(x.clamp(lo, hi), 0.05 * span), p(x.clamp(lo, hi) - 0.03 * span, 0.12 * span), p(x.clamp(lo, hi) + 0.03 * span, 0.12 * span)])])]));
            }
            opts.push(rule(h, "aspect_ratio", h.sym_term(h.sym("automatic"))));
        }
    }
    picture(cx, prims, opts, who)
}

// ---- rasterize

fn rasterize<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], _o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, rest @ ..] = a else { return Ok(Reply::Decline) };
    let is_image = crate::image_of(h, *x).is_some();
    let g = if is_image || mc_graphics::is_graphics(h, *x) {
        *x
    } else {
        // any other value: its text as a picture
        h.call(h.sym("graphics"), &[list(h, &[h.call(h.sym("text"), &[*x, h.reals(&[0.0, 0.0])])]), h.call(h.sym("options"), &[rule(h, "image_size", h.int(160))])])
    };
    match rest {
        [] => {}
        [p] => return raster_property(cx, *x, g, *p),
        _ => return Ok(crate::bad(h, "rasterize", 3)),
    }
    if is_image {
        return Ok(Reply::Done(*x));
    }
    finish_graphic(cx, g, "rasterize", &[])
}

/// The properties `rasterize(x, p)` gives (`"properties"` lists them).
const RASTER_PROPERTIES: [&str; 8] = ["Image", "Graphics", "Data", "RasterSize", "ImageSize", "BoundingBox", "Regions", "Properties"];

/// `rasterize(x, p)`: the property `p` of the picture of `x` (`g` the graphics drawn, or `x` itself when it is an
/// image), or a list of properties for a list. Sizes are in pixels at the picture's own resolution (one pixel a
/// point); a picture's baseline is its bottom edge (as for an image set in a line of text), so `"BoundingBox"` is
/// `[w, h, h]`; `"Regions"` gives `[tag, "Region"] -> [[left, top], [right, bottom]]` for each
/// `annotation(prim, tag, "Region")` of a graphic, in pixels with y up from the bottom edge.
fn raster_property<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, g: T<'h>, p: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    if let Some((n, items)) = crate::values::call_of(h, p) {
        if n.as_str() == names::LIST {
            let mut out = Vec::with_capacity(items.len());
            for &q in items {
                match raster_property(cx, x, g, q)? {
                    Reply::Done(v) => out.push(v),
                    other => return Ok(other),
                }
            }
            return Ok(Reply::Done(list(h, &out)));
        }
    }
    let name = match h.view(p) {
        View::Str(s) => s.to_string(),
        _ => return Ok(crate::bad(h, "rasterize", 2)),
    };
    let int = |v: f32| h.int(v.round() as i64);
    let unknown = || crate::failure(h, "unknown_property", &[h.string(&name), list(h, &RASTER_PROPERTIES.map(|s| h.string(s)))]);
    let image = crate::image_of(h, x);
    let stop = cx.stop_flag().clone();
    let stopped = || stop.load(Ordering::Relaxed);
    // the size in points (pixels at scale 1)
    let size = || -> Result<(f32, f32), mc_graphics::Failure> {
        match &image {
            Some((_, w, ht)) => Ok((*w as f32, *ht as f32)),
            None => mc_graphics::page_size(h, g),
        }
    };
    let v = match name.as_str() {
        "Image" | "Graphics" => return rasterize(cx, &[x], None),
        "Properties" => list(h, &RASTER_PROPERTIES.map(|s| h.string(s))),
        "RasterSize" | "ImageSize" => match size() {
            Ok((w, ht)) => list(h, &[int(w), int(ht)]),
            Err(f) => f.term(h),
        },
        "BoundingBox" => match size() {
            Ok((w, ht)) => list(h, &[int(w), int(ht), int(ht)]),
            Err(f) => f.term(h),
        },
        "Data" => {
            let pixels = match &image {
                Some((path, ..)) => std::fs::read(path)
                    .ok()
                    .and_then(|b| mc_graphics::decode_png(&b))
                    .ok_or_else(|| mc_graphics::Failure::new("io", format!("cannot read {path}"))),
                None => mc_graphics::render_rgba(h, g, 1.0, &stopped),
            };
            match pixels {
                Ok((w, _, rgba)) => {
                    let rows: Vec<T<'h>> = rgba
                        .chunks(4 * w as usize)
                        .map(|row| {
                            let px: Vec<T<'h>> = row.chunks(4).map(|c| list(h, &[h.int(c[0] as i64), h.int(c[1] as i64), h.int(c[2] as i64)])).collect();
                            list(h, &px)
                        })
                        .collect();
                    cx.tick(rows.len() as u64)?;
                    list(h, &rows)
                }
                Err(f) => {
                    cx.tick(1)?;
                    f.term(h)
                }
            }
        }
        "Regions" => {
            let mut parts = vec![];
            region_parts(h, if image.is_some() { h.call(h.sym(names::LIST), &[]) } else { x }, &mut parts);
            if !mc_graphics::is_graphics(h, x) && image.is_none() && any_region(h, x, 0) {
                // a layout (`column`, `grid`, ...) with annotated parts: where they lie is not known here
                crate::failure(h, "unsupported", &[h.string("regions of annotations outside graphics")])
            } else if parts.is_empty() {
                list(h, &[])
            } else {
                let height = match size() {
                    Ok((_, ht)) => ht,
                    Err(f) => return Ok(Reply::Done(f.term(h))),
                };
                let mut out = vec![];
                for (prim, tag, kind) in parts {
                    let part = h.call(h.sym("graphics"), &[prim]);
                    match mc_graphics::region_on_page(h, x, part, &stopped) {
                        Ok(Some([l, t, r, b])) => out.push(h.call(
                            h.sym(names::RULE),
                            &[list(h, &[tag, kind]), list(h, &[list(h, &[int(l), int(height - t)]), list(h, &[int(r), int(height - b)])])],
                        )),
                        Ok(None) => {}
                        Err(f) => {
                            cx.tick(1)?;
                            return Ok(Reply::Done(f.term(h)));
                        }
                    }
                }
                list(h, &out)
            }
        }
        _ => unknown(),
    };
    Ok(Reply::Done(v))
}

/// Whether `t` holds an `annotation(_, _, "Region")` anywhere (a bounded look).
fn any_region<'h>(h: &'h Heap, t: T<'h>, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    let View::App { head, args } = h.view(t) else { return false };
    if let (View::Sym(s), [_, _, kind]) = (h.view(head), args) {
        if h.sym_name(s) == "annotation" && matches!(h.view(*kind), View::Str(k) if k == "Region") {
            return true;
        }
    }
    args.iter().any(|&a| any_region(h, a, depth + 1))
}

/// The `annotation(prim, tag, "Region")` parts of a graphic's content (through lists, `style` and nested
/// graphics), with their tag and kind.
fn region_parts<'h>(h: &'h Heap, t: T<'h>, out: &mut Vec<(T<'h>, T<'h>, T<'h>)>) {
    let Some((n, args)) = crate::values::call_of(h, t) else { return };
    match (n.as_str(), args) {
        ("annotation", [prim, tag, kind]) if matches!(h.view(*kind), View::Str(k) if k == "Region") => out.push((*prim, *tag, *kind)),
        ("annotation" | "style" | "graphics" | "tooltip", [first, ..]) => region_parts(h, *first, out),
        (l, items) if l == names::LIST => items.iter().for_each(|&i| region_parts(h, i, out)),
        _ => {}
    }
}
