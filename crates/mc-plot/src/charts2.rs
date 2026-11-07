//! More charts and pictures: `bubble_chart`, `box_whisker_chart`, `sector_chart`, `rectangle_chart`,
//! `paired_bar_chart`, `density_histogram`, `smooth_histogram`, `number_line_plot`, the fractals
//! `mandelbrot_set_plot` and `julia_set_plot`, `relief_plot`, `complex_array_plot`, `complex_list_plot`,
//! `list_step_plot`, the gauges (`angular_gauge`, `horizontal_gauge`, `vertical_gauge`), and `rasterize`.
use crate::figure::{Extra, Look};
use crate::more::split;
use crate::{bad, finish, finish_graphic, plot_options, real};
use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::Rgba;
use mc_graphics::gradient::Gradient;

type Body = for<'a, 'b, 'h> fn(&mut NativeCx<'a, 'b, 'h>, &[T<'h>], Option<T<'h>>) -> Res<Reply<'h>>;

struct Chart(Body);

impl Native for Chart {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (a, o) = split(h, args);
        cx.tick(1)?;
        (self.0)(cx, &a, o)
    }
}

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let a = Attributes::default;
    let charts: [(&'static str, Body); 18] = [
        ("bubble_chart", bubble_chart),
        ("box_whisker_chart", box_whisker_chart),
        ("sector_chart", sector_chart),
        ("rectangle_chart", rectangle_chart),
        ("paired_bar_chart", paired_bar_chart),
        ("density_histogram", density_histogram),
        ("smooth_histogram", smooth_histogram),
        ("number_line_plot", number_line_plot),
        ("mandelbrot_set_plot", mandelbrot_set_plot),
        ("julia_set_plot", julia_set_plot),
        ("relief_plot", relief_plot),
        ("complex_array_plot", complex_array_plot),
        ("complex_list_plot", complex_list_plot),
        ("list_step_plot", list_step_plot),
        ("angular_gauge", |cx, a, o| gauge(cx, a, o, Gauge::Angular)),
        ("horizontal_gauge", |cx, a, o| gauge(cx, a, o, Gauge::Horizontal)),
        ("vertical_gauge", |cx, a, o| gauge(cx, a, o, Gauge::Vertical)),
        ("rasterize", rasterize),
    ];
    for (name, f) in charts {
        k.register(h.sym(name), a(), Box::new(Chart(f)));
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

/// One data set or several: a list of numbers, or a list of lists of numbers.
fn sets<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<Vec<f64>>>> {
    if let Some(v) = reals(cx, t)? {
        return Ok(Some(vec![v]));
    }
    matrix(cx, t)
}

fn look_of<'h>(cx: &NativeCx<'_, '_, 'h>, o: Option<T<'h>>, base: Look<'h>, extra: &[&str], who: &'static str) -> Look<'h> {
    match o {
        Some(o) => plot_options(cx, o, base, extra, who).unwrap_or_default(),
        None => Look { who, ..base },
    }
}

/// The q-quantile with linear interpolation between order statistics.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let pos = q * (n - 1) as f64;
    let k = pos.floor() as usize;
    let f = pos - k as f64;
    if k + 1 >= n {
        sorted[n - 1]
    } else {
        sorted[k] + f * (sorted[k + 1] - sorted[k])
    }
}

const CHART_OPTS: &[&str] = &["chart_labels", "chart_style", "chart_element_function", "chart_layout", "bar_spacing", "chart_base_style", "labeling_function", "bubble_sizes", "bubble_scale", "sector_origin", "sector_spacing", "polar_axes", "polar_grid_lines", "gauge_labels", "gauge_markers", "gauge_style", "gauge_faces", "scale_divisions", "scale_ranges", "scale_range_style", "target_units", "spacings", "color_function", "max_iterations", "escape_radius", "plot_points"];

// ---- bubble chart

fn bubble_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d] = a else { return Ok(Reply::Decline) };
    // one set [[x, y, z], ...] or several
    let mut groups: Vec<Vec<Vec<f64>>> = vec![];
    match matrix(cx, *d)? {
        Some(m) if m.iter().all(|r| r.len() == 3) => groups.push(m),
        _ => {
            for &s in cx.as_list(*d).map(|l| l.to_vec()).unwrap_or_default().iter() {
                match matrix(cx, s)? {
                    Some(m) if m.iter().all(|r| r.len() == 3) => groups.push(m),
                    _ => return Ok(bad(h, "bubble_chart", 1)),
                }
            }
        }
    }
    let all: Vec<&Vec<f64>> = groups.iter().flatten().collect();
    if all.is_empty() {
        return Ok(bad(h, "bubble_chart", 1));
    }
    let span = |k: usize| {
        let lo = all.iter().map(|r| r[k]).fold(f64::INFINITY, f64::min);
        let hi = all.iter().map(|r| r[k]).fold(f64::NEG_INFINITY, f64::max);
        (lo, if hi > lo { hi } else { lo + 1.0 })
    };
    let ((x0, x1), (y0, y1)) = (span(0), span(1));
    let zmax = all.iter().map(|r| r[2].abs()).fold(0.0f64, f64::max).max(1e-300);
    let look = look_of(cx, o, Look::default(), CHART_OPTS, "bubble_chart");
    let (kx, ky) = (0.08 * (x1 - x0), 0.08 * (y1 - y0));
    let mut under = vec![];
    for (g, set) in groups.iter().enumerate() {
        let c = rgb_of(look.color(g));
        let mut part = vec![mc_graphics::rgb_term(h, c), h.call(h.sym("opacity"), &[h.real(0.7)]), h.call(h.sym("edge_form"), &[mc_graphics::rgb_term(h, Rgba::rgb(c.r * 0.7, c.g * 0.7, c.b * 0.7))])];
        for r in set {
            // the area in proportion to z
            let s = (r[2].abs() / zmax).sqrt().max(0.05);
            part.push(h.call(h.sym("disk"), &[h.reals(&[r[0], r[1]]), h.reals(&[kx * s, ky * s])]));
        }
        under.push(list(h, &part));
    }
    let extra = Extra { under, bounds: Some([(x0 - kx, x1 + kx), (y0 - ky, y1 + ky)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- box and whisker chart

fn box_whisker_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, ..] = a else { return Ok(Reply::Decline) };
    let Some(sets) = sets(cx, *d)? else { return Ok(bad(h, "box_whisker_chart", 1)) };
    let sets: Vec<Vec<f64>> = sets.into_iter().map(|mut s| {
        s.retain(|v| v.is_finite());
        s.sort_by(f64::total_cmp);
        s
    }).filter(|s| !s.is_empty()).collect();
    if sets.is_empty() {
        return Ok(bad(h, "box_whisker_chart", 1));
    }
    let mut look = look_of(cx, o, Look::default(), CHART_OPTS, "box_whisker_chart");
    let mut under = vec![];
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (k, s) in sets.iter().enumerate() {
        let x = k as f64 + 1.0;
        let (q1, med, q3) = (quantile(s, 0.25), quantile(s, 0.5), quantile(s, 0.75));
        let iqr = q3 - q1;
        let (fence_lo, fence_hi) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
        let wlo = s.iter().copied().find(|&v| v >= fence_lo).unwrap_or(q1);
        let whi = s.iter().rev().copied().find(|&v| v <= fence_hi).unwrap_or(q3);
        let out: Vec<f64> = s.iter().copied().filter(|&v| v < fence_lo || v > fence_hi).collect();
        lo = lo.min(s[0]);
        hi = hi.max(s[s.len() - 1]);
        let c = rgb_of(look.color(0));
        let w = 0.3;
        let dark = mc_graphics::rgb_term(h, Rgba::rgb(c.r * 0.6, c.g * 0.6, c.b * 0.6));
        under.push(list(h, &[
            mc_graphics::rgb_term(h, c),
            h.call(h.sym("edge_form"), &[dark]),
            h.call(h.sym("rectangle"), &[h.reals(&[x - w, q1]), h.reals(&[x + w, q3])]),
        ]));
        let mut lines = vec![dark, h.call(h.sym("absolute_thickness"), &[h.real(1.2)])];
        lines.push(h.call(h.sym("line"), &[list(h, &[h.reals(&[x, q3]), h.reals(&[x, whi])])]));
        lines.push(h.call(h.sym("line"), &[list(h, &[h.reals(&[x, q1]), h.reals(&[x, wlo])])]));
        lines.push(h.call(h.sym("line"), &[list(h, &[h.reals(&[x - w / 2.0, whi]), h.reals(&[x + w / 2.0, whi])])]));
        lines.push(h.call(h.sym("line"), &[list(h, &[h.reals(&[x - w / 2.0, wlo]), h.reals(&[x + w / 2.0, wlo])])]));
        lines.push(h.sym_term(h.sym("white")));
        lines.push(h.call(h.sym("absolute_thickness"), &[h.real(2.0)]));
        lines.push(h.call(h.sym("line"), &[list(h, &[h.reals(&[x - w, med]), h.reals(&[x + w, med])])]));
        under.push(list(h, &lines));
        if !out.is_empty() {
            let pts: Vec<T<'h>> = out.iter().map(|&v| h.reals(&[x, v])).collect();
            under.push(list(h, &[dark, h.call(h.sym("absolute_point_size"), &[h.real(4.0)]), h.call(h.sym("point"), &[list(h, &pts)])]));
        }
    }
    let n = sets.len() as f64;
    if look.x_ticks.is_none() {
        look.x_ticks = Some((1..=sets.len()).map(|k| (k as f64, k.to_string())).collect());
    }
    let pad = (hi - lo).abs().max(1e-9) * 0.08;
    let extra = Extra { under, bounds: Some([(0.4, n + 0.6), (lo - pad, hi + pad)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- sector, rectangle and paired bar charts

/// A filled sector of the disk of radius r about the origin from angle a to b, as a polygon.
fn sector<'h>(h: &'h Heap, r0: f64, r: f64, a: f64, b: f64) -> T<'h> {
    let n = ((b - a).abs() / 0.05).ceil().max(2.0) as usize;
    let mut pts = vec![];
    for k in 0..=n {
        let t = a + (b - a) * k as f64 / n as f64;
        pts.push(h.reals(&[r * t.cos(), r * t.sin()]));
    }
    for k in (0..=n).rev() {
        let t = a + (b - a) * k as f64 / n as f64;
        pts.push(h.reals(&[r0 * t.cos(), r0 * t.sin()]));
    }
    h.call(h.sym("polygon"), &[list(h, &pts)])
}

fn picture<'h>(cx: &mut NativeCx<'_, '_, 'h>, prims: Vec<T<'h>>, opts: Vec<T<'h>>, who: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    let g = h.call(h.sym("graphics"), &[list(h, &prims), h.call(h.sym("options"), &opts)]);
    finish_graphic(cx, g, who, &[])
}

fn rule<'h>(h: &'h Heap, k: &str, v: T<'h>) -> T<'h> {
    h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v])
}

fn sector_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d] = a else { return Ok(Reply::Decline) };
    let Some(m) = matrix(cx, *d)?.filter(|m| !m.is_empty() && m.iter().all(|r| r.len() == 2)) else { return Ok(bad(h, "sector_chart", 1)) };
    let total: f64 = m.iter().map(|r| r[0].abs()).sum();
    let rmax = m.iter().map(|r| r[1].abs()).fold(0.0f64, f64::max);
    if total <= 0.0 || rmax <= 0.0 {
        return Ok(bad(h, "sector_chart", 1));
    }
    let look = look_of(cx, o, Look::default(), CHART_OPTS, "sector_chart");
    let mut prims = vec![h.call(h.sym("edge_form"), &[h.sym_term(h.sym("white"))])];
    let mut ang = 0.0;
    for (k, r) in m.iter().enumerate() {
        let span = std::f64::consts::TAU * r[0].abs() / total;
        prims.push(list(h, &[mc_graphics::rgb_term(h, rgb_of(look.color(k))), sector(h, 0.0, r[1].abs() / rmax, ang, ang + span)]));
        ang += span;
    }
    picture(cx, prims, vec![rule(h, "aspect_ratio", h.sym_term(h.sym("automatic")))], "sector_chart")
}

fn rectangle_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d] = a else { return Ok(Reply::Decline) };
    let Some(m) = matrix(cx, *d)?.filter(|m| !m.is_empty() && m.iter().all(|r| r.len() == 2)) else { return Ok(bad(h, "rectangle_chart", 1)) };
    let look = look_of(cx, o, Look::default(), CHART_OPTS, "rectangle_chart");
    let mut x = 0.0;
    let mut under = vec![];
    let gap = m.iter().map(|r| r[0].abs()).sum::<f64>() * 0.01;
    let (mut lo, mut hi) = (0.0f64, 0.0f64);
    for r in m.iter() {
        let c = rgb_of(look.color(0));
        under.push(list(h, &[mc_graphics::rgb_term(h, c), h.call(h.sym("edge_form"), &[h.sym_term(h.sym("white"))]), h.call(h.sym("rectangle"), &[h.reals(&[x, 0.0]), h.reals(&[x + r[0].abs(), r[1]])])]));
        lo = lo.min(r[1]);
        hi = hi.max(r[1]);
        x += r[0].abs() + gap;
    }
    let extra = Extra { under, bounds: Some([(0.0, x), (lo, hi * 1.05)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

fn paired_bar_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [l, r] = a else { return Ok(Reply::Decline) };
    let (Some(l), Some(r)) = (reals(cx, *l)?, reals(cx, *r)?) else { return Ok(bad(h, "paired_bar_chart", 1)) };
    let look = look_of(cx, o, Look::default(), CHART_OPTS, "paired_bar_chart");
    let mut under = vec![];
    let n = l.len().max(r.len());
    let m = l.iter().chain(&r).map(|v| v.abs()).fold(0.0f64, f64::max).max(1e-300);
    for (side, vals, col) in [(-1.0, &l, look.color(0)), (1.0, &r, look.color(1))] {
        let mut part = vec![mc_graphics::rgb_term(h, rgb_of(col)), h.call(h.sym("edge_form"), &[h.sym_term(h.sym("white"))])];
        for (k, v) in vals.iter().enumerate() {
            let y = k as f64 + 1.0;
            part.push(h.call(h.sym("rectangle"), &[h.reals(&[0.0, y - 0.4]), h.reals(&[side * v.abs(), y + 0.4])]));
        }
        under.push(list(h, &part));
    }
    let extra = Extra { under, bounds: Some([(-m * 1.05, m * 1.05), (0.4, n as f64 + 0.6)]), entries: vec![] };
    finish(cx, &[], &look, extra)
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
            under.push(list(h, &[mc_graphics::rgb_term(h, col), h.call(h.sym("rectangle"), &[h.reals(&[x0 + dx * i as f64, y0 + dy * j as f64]), h.reals(&[x0 + dx * (i + 1) as f64, y0 + dy * (j + 1) as f64])])]));
        }
    }
    let extra = Extra { under, bounds: Some([(x0, x1), (y0, y1)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

fn smooth_histogram<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, ..] = a else { return Ok(Reply::Decline) };
    let Some(sets) = sets(cx, *d)? else { return Ok(bad(h, "smooth_histogram", 1)) };
    let mut curves = vec![];
    for s in sets {
        let n = s.len() as f64;
        if n < 2.0 {
            return Ok(bad(h, "smooth_histogram", 1));
        }
        let mean = s.iter().sum::<f64>() / n;
        let sd = (s.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        let mut sorted = s.clone();
        sorted.sort_by(f64::total_cmp);
        let iqr = quantile(&sorted, 0.75) - quantile(&sorted, 0.25);
        // Silverman's rule
        let bw = (0.9 * sd.min(iqr / 1.34).max(sd * 0.1) * n.powf(-0.2)).max(1e-12);
        let (lo, hi) = (sorted[0] - 3.0 * bw, sorted[sorted.len() - 1] + 3.0 * bw);
        let pts: Vec<T<'h>> = (0..=200)
            .map(|k| {
                let x = lo + (hi - lo) * k as f64 / 200.0;
                let y = s.iter().map(|v| (-0.5 * ((x - v) / bw).powi(2)).exp()).sum::<f64>() / (n * bw * (std::f64::consts::TAU).sqrt());
                h.reals(&[x, y])
            })
            .collect();
        curves.push(list(h, &pts));
    }
    let mut args = vec![list(h, &curves), rule(h, "filling", h.sym_term(h.sym("axis")))];
    if let Some(o) = o {
        if let View::App { args: rs, .. } = h.view(o) {
            args.extend(rs.iter().copied());
        }
    }
    let opts = h.call(h.sym("options"), &args[1..]);
    Ok(Reply::Again(h.call(h.sym("list_line_plot"), &[args[0], opts])))
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
        let col = mc_graphics::rgb_term(h, rgb_of(look.color(k)));
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

fn raster_of<'h>(h: &'h Heap, cols: &[Rgba], nx: usize, ny: usize, (x0, x1, y0, y1): (f64, f64, f64, f64)) -> T<'h> {
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
    let r = 2.0f64.max(c.0.hypot(c.1)).min(2.0) * 0.8 + 0.4;
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

fn complex_list_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d] = a else { return Ok(Reply::Decline) };
    let Some(items) = cx.as_list(*d).map(|l| l.to_vec()) else { return Ok(bad(h, "complex_list_plot", 1)) };
    // one list of numbers, or several
    let several = items.iter().all(|&e| cx.as_list(e).is_some());
    let groups: Vec<Vec<T<'h>>> = if several { items.iter().map(|&e| cx.as_list(e).map(|l| l.to_vec()).unwrap_or_default()).collect() } else { vec![items] };
    let mut sets = vec![];
    for g in groups {
        let mut pts = vec![];
        for e in g {
            if let Some((re, im)) = complex_parts(cx, e)? {
                pts.push(h.reals(&[re, im]));
            }
        }
        sets.push(list(h, &pts));
    }
    let data = if several { list(h, &sets) } else { sets[0] };
    let mut args = vec![data];
    args.extend(o);
    Ok(Reply::Again(h.call(h.sym("list_plot"), &args)))
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
    let to_pts = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<Vec<(f64, f64)>>> {
        if let Some(v) = reals(cx, t)? {
            return Ok(Some(v.iter().enumerate().map(|(k, &y)| (k as f64 + 1.0, y)).collect()));
        }
        Ok(matrix(cx, t)?.filter(|m| m.iter().all(|r| r.len() == 2)).map(|m| m.iter().map(|r| (r[0], r[1])).collect()))
    };
    let sets: Vec<Vec<(f64, f64)>> = match to_pts(cx, *d)? {
        Some(p) => vec![p],
        None => {
            let mut v = vec![];
            for &s in cx.as_list(*d).map(|l| l.to_vec()).unwrap_or_default().iter() {
                match to_pts(cx, s)? {
                    Some(p) => v.push(p),
                    None => return Ok(bad(h, "list_step_plot", 1)),
                }
            }
            v
        }
    };
    let mut lines = vec![];
    for p in sets {
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
            out.push(h.reals(&[a, y]));
            out.push(h.reals(&[b, y]));
        }
        lines.push(list(h, &out));
    }
    let mut args = vec![list(h, &lines)];
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
            Some(&[lo, hi]) => match (real(cx, lo)?, real(cx, hi)?) {
                (Some(lo), Some(hi)) if hi > lo => (*v, lo, hi),
                _ => return Ok(bad(h, who, 2)),
            },
            _ => return Ok(bad(h, who, 2)),
        },
        _ => return Ok(Reply::Decline),
    };
    // one value or several
    let vals: Vec<f64> = match reals(cx, v)? {
        Some(vs) => vs,
        None => match real(cx, v)? {
            Some(x) => vec![x],
            None => return Ok(bad(h, who, 1)),
        },
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
                let col = mc_graphics::rgb_term(h, rgb_of(look.color(k)));
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
                let col = mc_graphics::rgb_term(h, rgb_of(look.color(k)));
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
    let [x, ..] = a else { return Ok(Reply::Decline) };
    if crate::image_of(h, *x).is_some() {
        return Ok(Reply::Done(*x));
    }
    let g = if mc_graphics::is_graphics(h, *x) {
        *x
    } else {
        // any other value: its text as a picture
        h.call(h.sym("graphics"), &[list(h, &[h.call(h.sym("text"), &[*x, h.reals(&[0.0, 0.0])])]), h.call(h.sym("options"), &[rule(h, "image_size", h.int(160))])])
    };
    finish_graphic(cx, g, "rasterize", &[])
}
