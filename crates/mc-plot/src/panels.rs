#![allow(clippy::too_many_arguments, clippy::needless_range_loop, clippy::type_complexity)]
//! Plots of multivariate data and other charts drawn as pictures of their own:
//!
//! - the pairwise plots (`pairwise_list_plot`, `pairwise_density_histogram`, `pairwise_smooth_density_histogram`,
//!   `pairwise_probability_plot`, `pairwise_quantile_plot`): a grid of panels, one per pair of variables, with each
//!   variable's histogram on the diagonal;
//! - `parallel_axis_plot` (each row a polyline across parallel axes, each axis scaled to its column's range) and
//!   `radial_axis_plot` (each row a closed polygon over axes radiating from a centre);
//! - `ternary_list_plot` (compositions of three parts in an equilateral triangle), `stacked_list_plot` (data sets
//!   stacked on one another, the bands filled), `list_fit_plot` (points with their least-squares line or
//!   polynomial), `point_value_plot` (points coloured by their values);
//! - `paired_histogram`, `paired_smooth_histogram` (two distributions back to back), `smooth_density_histogram` (a
//!   two-dimensional kernel density estimate), `distribution_chart` (violins: mirrored density estimates);
//! - `candlestick_chart`, `renko_chart` (prices over time; dates as `[y, m, d]` lists or numbers);
//! - `dendrogram` and `clustering_tree` (agglomerative clustering with average linkage), `feature_space_plot` (data
//!   projected on its first two principal components), `list_curve_path_plot` (points joined in nearest-neighbour
//!   order);
//! - `chromaticity_plot`: the CIE 1931 chromaticity diagram with the gamuts of colour spaces: the spectral locus of
//!   the CIE 1931 standard observer (CIE 015 tables), the inside coloured by its sRGB look.
//!
//! Kernel density estimates use Gaussian kernels with Silverman's rule-of-thumb bandwidth (Silverman, *Density
//! Estimation for Statistics and Data Analysis*, 1986, eq. 3.31).
use crate::chartdata::{self, Node};
use crate::charts3::{bandwidth, empty, kde, list, look_of, picture, quantile, rgb, rule};
use crate::figure::{Extra, Look};
use crate::finish;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::Rgba;
use mc_graphics::gradient::Gradient;

type Body = for<'a, 'b, 'h> fn(&mut NativeCx<'a, 'b, 'h>, &[T<'h>], Option<T<'h>>) -> Res<Reply<'h>>;

struct P(&'static str, Body);

impl Native for P {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (a, o) = crate::more::split(h, args);
        if a.is_empty() {
            return Ok(Reply::Decline);
        }
        cx.tick(1)?;
        // an undefined name as the data: unevaluated in effect (a failure naming the argument), as for any value
        // that is not data
        if let Some(&f) = a.first() {
            // also `name -> labels`
            let f = match crate::values::call_of(h, f) {
                Some((n, &[x, _])) if n == mc_api::names::RULE => x,
                _ => f,
            };
            if matches!(h.view(f), View::Sym(_)) {
                return Ok(crate::bad(h, self.0, 1));
            }
        }
        (self.1)(cx, &a, o)
    }
}

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let all: [(&'static str, Body); 21] = [
        ("pairwise_list_plot", |cx, a, o| pairwise(cx, a, o, Pair::List)),
        ("pairwise_density_histogram", |cx, a, o| pairwise(cx, a, o, Pair::Density)),
        ("pairwise_smooth_density_histogram", |cx, a, o| pairwise(cx, a, o, Pair::Smooth)),
        ("pairwise_probability_plot", |cx, a, o| pairwise(cx, a, o, Pair::Probability)),
        ("pairwise_quantile_plot", |cx, a, o| pairwise(cx, a, o, Pair::Quantile)),
        ("parallel_axis_plot", parallel_axis_plot),
        ("radial_axis_plot", radial_axis_plot),
        ("ternary_list_plot", ternary_list_plot),
        ("stacked_list_plot", stacked_list_plot),
        ("list_fit_plot", list_fit_plot),
        ("point_value_plot", point_value_plot),
        ("paired_histogram", paired_histogram),
        ("paired_smooth_histogram", paired_smooth_histogram),
        ("smooth_density_histogram", smooth_density_histogram),
        ("distribution_chart", distribution_chart),
        ("candlestick_chart", candlestick_chart),
        ("renko_chart", renko_chart),
        ("dendrogram", |cx, a, o| dendrogram(cx, a, o, false)),
        ("clustering_tree", |cx, a, o| dendrogram(cx, a, o, true)),
        ("feature_space_plot", feature_space_plot),
        ("list_curve_path_plot", list_curve_path_plot),
    ];
    for (name, f) in all {
        k.register(h.sym(name), Attributes::default(), Box::new(P(name, f)));
    }
    k.register(h.sym("chromaticity_plot"), Attributes::default(), Box::new(P("chromaticity_plot", chromaticity_plot)));
}

fn seg<'h>(h: &'h Heap, a: [f64; 2], b: [f64; 2]) -> T<'h> {
    h.call(h.sym("line"), &[list(h, &[h.reals(&a), h.reals(&b)])])
}

fn text<'h>(h: &'h Heap, s: &str, at: [f64; 2]) -> T<'h> {
    h.call(h.sym("text"), &[h.string(s), h.reals(&at)])
}

fn text_off<'h>(h: &'h Heap, s: &str, at: [f64; 2], off: [f64; 2]) -> T<'h> {
    h.call(h.sym("text"), &[h.string(s), h.reals(&at), h.reals(&off)])
}

fn color<'h>(h: &'h Heap, c: Rgba) -> T<'h> {
    mc_graphics_kernel::rgb_term(h, c)
}

fn sym_named(h: &Heap, t: T<'_>) -> Option<String> {
    match h.view(t) {
        View::Sym(s) => Some(h.sym_name(s).to_string()),
        View::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// Multivariate data: groups of rows of numbers with the variables' names.
struct Table {
    groups: Vec<(Option<String>, Vec<Vec<f64>>)>,
    names: Vec<String>,
}

/// Rows of a node: each item a row (a list of entries, or an association).
fn rows_of(n: &Node<'_>) -> Option<Vec<Vec<Option<f64>>>> {
    let items = n.items();
    if items.is_empty() || !items.iter().all(|r| r.is_list() && r.items().iter().all(|e| !e.is_list())) {
        return None;
    }
    Some(items.iter().map(|r| r.items().iter().map(Node::num).collect()).collect())
}

fn read_table<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Table>> {
    let h = cx.heap();
    let mut t = t;
    let mut names: Vec<String> = vec![];
    if let Some((n, &[x, l])) = crate::values::call_of(h, t) {
        if n == mc_api::names::RULE {
            if let Some(ls) = cx.as_list(l) {
                names = ls.iter().map(|&s| sym_named(h, s).unwrap_or_else(|| mc_syntax::Printer::new(h).expr(s))).collect();
                t = x;
            }
        }
    }
    if let Some((n, &[x])) = crate::values::call_of(h, t) {
        if n == "dataset" {
            t = x;
        }
    }
    let node = chartdata::read(cx, t)?;
    let mut raw: Vec<(Option<String>, Vec<Vec<Option<f64>>>, &Node<'h>)> = vec![];
    if let Some(r) = rows_of(&node) {
        raw.push((None, r, &node));
    } else {
        for g in node.items() {
            if let Some(r) = rows_of(g) {
                raw.push((g.label_text(h), r, g));
            }
        }
    }
    if raw.is_empty() {
        return Ok(None);
    }
    // the columns that are numbers in every row of every group
    let width = raw.iter().flat_map(|g| g.1.iter().map(|r| r.len())).min().unwrap_or(0);
    let keep: Vec<usize> = (0..width).filter(|&j| raw.iter().all(|g| g.1.iter().all(|r| r[j].is_some()))).collect();
    if keep.is_empty() {
        return Ok(None);
    }
    if names.is_empty() {
        let first = raw[0].2.items().first();
        names = keep.iter().map(|&j| first.and_then(|r| r.items().get(j)).and_then(|e| e.label_text(h)).unwrap_or_else(|| format!("x{}", j + 1))).collect();
    } else if names.len() != keep.len() {
        names.resize(keep.len(), String::new());
    }
    let groups = raw.into_iter().map(|(n, rows, _)| (n, rows.iter().map(|r| keep.iter().map(|&j| r[j].unwrap_or(f64::NAN)).collect()).collect())).collect();
    Ok(Some(Table { groups, names }))
}

fn span(v: impl Iterator<Item = f64>) -> (f64, f64) {
    let (a, b) = v.filter(|x| x.is_finite()).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
    if !a.is_finite() {
        (0.0, 1.0)
    } else if b > a {
        (a, b)
    } else {
        (a - 0.5, a + 0.5)
    }
}

// ---- pairwise plots

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pair {
    List,
    Density,
    Smooth,
    Probability,
    Quantile,
}

/// The empirical distribution function of sorted data at x.
fn ecdf(sorted: &[f64], x: f64) -> f64 {
    sorted.partition_point(|&v| v <= x) as f64 / sorted.len().max(1) as f64
}

fn pairwise<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>, kind: Pair) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who: &'static str = match kind {
        Pair::List => "pairwise_list_plot",
        Pair::Density => "pairwise_density_histogram",
        Pair::Smooth => "pairwise_smooth_density_histogram",
        Pair::Probability => "pairwise_probability_plot",
        Pair::Quantile => "pairwise_quantile_plot",
    };
    let look = look_of(cx, o, Look::default(), who);
    let Some(tb) = read_table(cx, a[0])? else { return empty(cx, &look, who) };
    let d = tb.names.len().min(12);
    let cols: Vec<Vec<Vec<f64>>> = tb.groups.iter().map(|g| (0..d).map(|j| g.1.iter().map(|r| r[j]).collect()).collect()).collect();
    let ranges: Vec<(f64, f64)> = (0..d).map(|j| span(cols.iter().flat_map(|g| g[j].iter().copied()))).collect();
    let grad = look.gradient.clone().unwrap_or_else(Gradient::default_gradient);
    let (m, inner) = (0.06, 0.88);
    let mut prims = vec![];
    for i in 0..d {
        for j in 0..d {
            let (ox, oy) = (j as f64 + m, (d - 1 - i) as f64 + m);
            let map = |x: f64, y: f64, rx: (f64, f64), ry: (f64, f64)| [ox + inner * (0.06 + 0.88 * (x - rx.0) / (rx.1 - rx.0)), oy + inner * (0.06 + 0.88 * (y - ry.0) / (ry.1 - ry.0))];
            // the panel's frame (chrome)
            let fr = [[ox, oy], [ox + inner, oy], [ox + inner, oy + inner], [ox, oy + inner], [ox, oy]];
            prims.push(list(h, &[h.call(h.sym("absolute_thickness"), &[h.real(0.6)]), h.call(h.sym("line"), &[list(h, &fr.iter().map(|p| h.reals(p)).collect::<Vec<_>>())])]));
            if i == j {
                // the variable's histogram (each group in its colour), its name above
                let (lo, hi) = ranges[j];
                let nb = 12;
                let mut counts: Vec<Vec<f64>> = vec![];
                for g in &cols {
                    let mut c = vec![0.0; nb];
                    for &v in &g[j] {
                        if v.is_finite() {
                            c[(((v - lo) / (hi - lo) * nb as f64) as usize).min(nb - 1)] += 1.0;
                        }
                    }
                    counts.push(c);
                }
                let top = counts.iter().flatten().copied().fold(1.0f64, f64::max);
                for (gk, c) in counts.iter().enumerate() {
                    let mut part = vec![color(h, rgb(look.color(gk))), h.call(h.sym("opacity"), &[h.real(if cols.len() > 1 { 0.5 } else { 0.85 })])];
                    for (b, &n) in c.iter().enumerate() {
                        if n > 0.0 {
                            let x0 = ox + inner * b as f64 / nb as f64;
                            part.push(h.call(h.sym("rectangle"), &[h.reals(&[x0, oy]), h.reals(&[x0 + inner / nb as f64, oy + inner * 0.85 * n / top])]));
                        }
                    }
                    prims.push(list(h, &part));
                }
                prims.push(text(h, &tb.names[j], [ox + inner / 2.0, oy + inner * 0.93]));
                continue;
            }
            let (rx, ry) = (ranges[j], ranges[i]);
            match kind {
                Pair::List | Pair::Probability | Pair::Quantile => {
                    for (gk, g) in cols.iter().enumerate() {
                        let pts: Vec<[f64; 2]> = match kind {
                            Pair::List => g[j].iter().zip(&g[i]).filter(|(x, y)| x.is_finite() && y.is_finite()).map(|(&x, &y)| map(x, y, rx, ry)).collect(),
                            _ => {
                                let mut xs: Vec<f64> = g[j].iter().copied().filter(|v| v.is_finite()).collect();
                                let mut ys: Vec<f64> = g[i].iter().copied().filter(|v| v.is_finite()).collect();
                                xs.sort_by(f64::total_cmp);
                                ys.sort_by(f64::total_cmp);
                                if xs.is_empty() || ys.is_empty() {
                                    continue;
                                }
                                let n = xs.len().max(ys.len()).min(200);
                                (0..n)
                                    .map(|k| {
                                        let q = (k as f64 + 0.5) / n as f64;
                                        if kind == Pair::Quantile {
                                            map(quantile(&xs, q), quantile(&ys, q), rx, ry)
                                        } else {
                                            // P-P: the two distribution functions at the pooled quantile
                                            let t = quantile(&xs, q).min(quantile(&ys, q)) + (quantile(&xs, q) - quantile(&ys, q)).abs() / 2.0;
                                            [ox + inner * ecdf(&xs, t), oy + inner * ecdf(&ys, t)]
                                        }
                                    })
                                    .collect()
                            }
                        };
                        if pts.is_empty() {
                            continue;
                        }
                        let ps = (3.2 / (pts.len() as f64 / 100.0).sqrt().max(1.0)).max(1.2);
                        prims.push(list(h, &[color(h, rgb(look.color(gk))), h.call(h.sym("absolute_point_size"), &[h.real(ps)]), h.call(h.sym("point"), &[list(h, &pts.iter().map(|p| h.reals(p)).collect::<Vec<_>>())])]));
                    }
                    if kind != Pair::List {
                        prims.push(list(h, &[h.call(h.sym("opacity"), &[h.real(0.4)]), h.call(h.sym("dashing"), &[h.sym_term(h.sym("dashed"))]), seg(h, [ox, oy], [ox + inner, oy + inner])]));
                    }
                }
                Pair::Density | Pair::Smooth => {
                    let n = if kind == Pair::Density { 10 } else { 32 };
                    let mut grid = vec![0.0; n * n];
                    let pts: Vec<(f64, f64)> = cols.iter().flat_map(|g| g[j].iter().zip(&g[i]).map(|(&x, &y)| (x, y))).filter(|(x, y)| x.is_finite() && y.is_finite()).collect();
                    if kind == Pair::Density {
                        for &(x, y) in &pts {
                            let a = (((x - rx.0) / (rx.1 - rx.0) * n as f64) as usize).min(n - 1);
                            let b = (((y - ry.0) / (ry.1 - ry.0) * n as f64) as usize).min(n - 1);
                            grid[b * n + a] += 1.0;
                        }
                    } else {
                        let mut xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
                        let mut ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
                        xs.sort_by(f64::total_cmp);
                        ys.sort_by(f64::total_cmp);
                        if xs.len() < 2 {
                            continue;
                        }
                        let (bx, by) = (bandwidth(&xs), bandwidth(&ys));
                        for b in 0..n {
                            for a2 in 0..n {
                                let x = rx.0 + (rx.1 - rx.0) * (a2 as f64 + 0.5) / n as f64;
                                let y = ry.0 + (ry.1 - ry.0) * (b as f64 + 0.5) / n as f64;
                                grid[b * n + a2] = pts.iter().map(|&(px, py)| (-0.5 * (((x - px) / bx).powi(2) + ((y - py) / by).powi(2))).exp()).sum();
                            }
                        }
                    }
                    let top = grid.iter().copied().fold(1e-300f64, f64::max);
                    let cells: Vec<Rgba> = grid.iter().map(|&v| if v <= 0.0 && kind == Pair::Density { Rgba { a: 0.0, ..Rgba::rgb(1.0, 1.0, 1.0) } } else { grad.at(v / top) }).collect();
                    prims.push(crate::charts2::raster_of(h, &cells, n, n, (ox, ox + inner, oy, oy + inner)));
                }
            }
        }
    }
    let w = (130.0 * d as f64).clamp(260.0, 720.0);
    picture(cx, prims, vec![rule(h, "aspect_ratio", h.real(1.0)), rule(h, "image_size", h.real(w)), rule(h, "plot_range", list(h, &[h.reals(&[0.0, d as f64]), h.reals(&[0.0, d as f64])]))], &look, who)
}

// ---- parallel and radial axes

/// Data sets of rows: one matrix, or several (each a group).
fn row_sets(n: &Node<'_>) -> Vec<Vec<Vec<f64>>> {
    let row = |r: &Node<'_>| -> Vec<f64> { r.items().iter().map(|e| e.num().unwrap_or(f64::NAN)).collect() };
    if let Some(v) = n.numbers() {
        return vec![vec![v]];
    }
    if n.items().iter().all(|r| r.numbers().is_some() || !r.is_list()) {
        return vec![n.items().iter().filter(|r| r.is_list()).map(row).collect()];
    }
    n.items()
        .iter()
        .map(|g| if let Some(v) = g.numbers() { vec![v] } else { g.items().iter().filter(|r| r.is_list()).map(row).collect() })
        .filter(|g: &Vec<Vec<f64>>| !g.is_empty())
        .collect()
}

fn parallel_axis_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "parallel_axis_plot");
    let node = chartdata::read(cx, a[0])?;
    let sets = row_sets(&node);
    let d = sets.iter().flatten().map(|r| r.len()).max().unwrap_or(0);
    if d == 0 {
        return empty(cx, &look, "parallel_axis_plot");
    }
    let ranges: Vec<(f64, f64)> = (0..d).map(|j| span(sets.iter().flatten().filter_map(|r| r.get(j).copied()))).collect();
    let mut prims = vec![];
    for j in 0..d {
        let x = j as f64;
        prims.push(list(h, &[h.call(h.sym("absolute_thickness"), &[h.real(0.8)]), seg(h, [x, 0.0], [x, 1.0])]));
        for (t, v) in [(0.0, ranges[j].0), (1.0, ranges[j].1)] {
            prims.push(text_off(h, &mc_graphics::num::short(v), [x, t], [0.0, if t == 0.0 { 1.6 } else { -1.6 }]));
        }
    }
    for (g, rows) in sets.iter().enumerate() {
        let mut part = vec![color(h, rgb(look.color(g))), h.call(h.sym("absolute_thickness"), &[h.real(1.4)]), h.call(h.sym("opacity"), &[h.real(0.85)])];
        for r in rows {
            let pts: Vec<T<'h>> = r.iter().enumerate().filter(|(_, v)| v.is_finite()).map(|(j, &v)| h.reals(&[j as f64, (v - ranges[j].0) / (ranges[j].1 - ranges[j].0)])).collect();
            part.push(h.call(h.sym("line"), &[list(h, &pts)]));
        }
        prims.push(list(h, &part));
    }
    picture(cx, prims, vec![rule(h, "aspect_ratio", h.real(0.6)), rule(h, "plot_range_padding", h.real(0.08))], &look, "parallel_axis_plot")
}

fn radial_axis_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "radial_axis_plot");
    let node = chartdata::read(cx, a[0])?;
    let sets: Vec<Vec<f64>> = row_sets(&node).into_iter().flatten().collect();
    let d = sets.iter().map(|r| r.len()).max().unwrap_or(0);
    if d < 2 {
        return empty(cx, &look, "radial_axis_plot");
    }
    let tops: Vec<f64> = (0..d).map(|j| sets.iter().filter_map(|r| r.get(j).copied()).filter(|v| v.is_finite()).fold(0.0f64, |a, b| a.max(b.abs())).max(1e-300)).collect();
    let dir = |j: usize| {
        let t = std::f64::consts::FRAC_PI_2 - std::f64::consts::TAU * j as f64 / d as f64;
        [t.cos(), t.sin()]
    };
    let mut prims = vec![];
    let mut grid = vec![h.call(h.sym("opacity"), &[h.real(0.35)]), h.call(h.sym("absolute_thickness"), &[h.real(0.6)])];
    for ring in [0.25, 0.5, 0.75, 1.0] {
        let pts: Vec<T<'h>> = (0..=d).map(|j| h.reals(&[ring * dir(j % d)[0], ring * dir(j % d)[1]])).collect();
        grid.push(h.call(h.sym("line"), &[list(h, &pts)]));
    }
    for j in 0..d {
        grid.push(seg(h, [0.0, 0.0], dir(j)));
    }
    prims.push(list(h, &grid));
    for j in 0..d {
        let p = dir(j);
        prims.push(text(h, &mc_graphics::num::short(tops[j]), [1.12 * p[0], 1.12 * p[1]]));
    }
    for (g, r) in sets.iter().enumerate() {
        let pts: Vec<T<'h>> = (0..d).map(|j| {
            let v = r.get(j).copied().filter(|v| v.is_finite()).unwrap_or(0.0).abs() / tops[j];
            h.reals(&[v * dir(j)[0], v * dir(j)[1]])
        }).collect();
        let c = rgb(look.color(g));
        let mut closed = pts.clone();
        closed.push(pts[0]);
        prims.push(list(h, &[color(h, c), h.call(h.sym("opacity"), &[h.real(0.25)]), h.call(h.sym("polygon"), &[list(h, &pts)])]));
        prims.push(list(h, &[color(h, c), h.call(h.sym("absolute_thickness"), &[h.real(1.8)]), h.call(h.sym("line"), &[list(h, &closed)]), h.call(h.sym("absolute_point_size"), &[h.real(5.0)]), h.call(h.sym("point"), &[list(h, &pts)])]));
    }
    picture(cx, prims, vec![rule(h, "aspect_ratio", h.sym_term(h.sym("automatic"))), rule(h, "plot_range", list(h, &[h.reals(&[-1.3, 1.3]), h.reals(&[-1.3, 1.3])]))], &look, "radial_axis_plot")
}

// ---- ternary, stacked, fit and value plots

fn ternary_list_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "ternary_list_plot");
    let node = chartdata::read(cx, a[0])?;
    let groups = node.tuple_groups(3);
    let s3 = 3f64.sqrt() / 2.0;
    // corners: first part at the lower left, second at the lower right, third at the top
    let at = |t: &[f64]| -> Option<[f64; 2]> {
        let s = t[0] + t[1] + t[2];
        (s.abs() > 1e-300).then(|| {
            let (b, c) = (t[1] / s, t[2] / s);
            [b + c / 2.0, c * s3]
        })
    };
    let mut prims = vec![];
    let mut grid = vec![h.call(h.sym("opacity"), &[h.real(0.3)]), h.call(h.sym("absolute_thickness"), &[h.real(0.5)])];
    for k in 1..10 {
        let f = k as f64 / 10.0;
        grid.push(seg(h, [f, 0.0], [f + (1.0 - f) / 2.0, (1.0 - f) * s3]));
        grid.push(seg(h, [f, 0.0], [f / 2.0, f * s3]));
        grid.push(seg(h, [f / 2.0, f * s3], [1.0 - f / 2.0, f * s3]));
    }
    prims.push(list(h, &grid));
    prims.push(h.call(h.sym("line"), &[list(h, &[h.reals(&[0.0, 0.0]), h.reals(&[1.0, 0.0]), h.reals(&[0.5, s3]), h.reals(&[0.0, 0.0])])]));
    for (k, l) in [(0.0, [0.0, 0.0]), (1.0, [1.0, 0.0]), (2.0, [0.5, s3])] {
        let off = if k == 2.0 { [0.0, -1.5] } else { [if k == 0.0 { 1.2 } else { -1.2 }, 1.5] };
        prims.push(text_off(h, &format!("{}", k as usize + 1), l, off));
    }
    for (g, (_, items)) in groups.iter().enumerate() {
        let pts: Vec<T<'h>> = items.iter().filter_map(|(_, t)| at(t)).map(|p| h.reals(&p)).collect();
        prims.push(list(h, &[color(h, rgb(look.color(g))), h.call(h.sym("absolute_point_size"), &[h.real(5.0)]), h.call(h.sym("point"), &[list(h, &pts)])]));
    }
    picture(cx, prims, vec![rule(h, "aspect_ratio", h.sym_term(h.sym("automatic"))), rule(h, "plot_range_padding", h.real(0.08))], &look, "ternary_list_plot")
}

/// Data sets as lists of (x, y): values at 1, 2, ... or pairs.
fn xy_sets(n: &Node<'_>) -> Vec<Vec<(f64, f64)>> {
    let one = |s: &Node<'_>| -> Option<Vec<(f64, f64)>> {
        if let Some(v) = s.numbers() {
            if s.items().iter().all(|e| e.num().is_some() || !e.is_list()) {
                return Some(v.iter().enumerate().map(|(k, &y)| (k as f64 + 1.0, y)).collect());
            }
        }
        let pairs: Vec<(f64, f64)> = s.items().iter().filter_map(|e| e.tuple(2)).map(|t| (t[0], t[1])).collect();
        (!pairs.is_empty() && s.items().iter().all(|e| e.is_list() && e.items().iter().all(|x| !x.is_list()))).then_some(pairs)
    };
    if let Some(s) = one(n) {
        return vec![s];
    }
    n.items().iter().filter_map(one).collect()
}

fn stacked_list_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut look = look_of(cx, o, Look::default(), "stacked_list_plot");
    let node = chartdata::read(cx, a[0])?;
    let sets = xy_sets(&node);
    if sets.is_empty() {
        return empty(cx, &look, "stacked_list_plot");
    }
    let n = sets.iter().map(|s| s.len()).max().unwrap_or(0);
    let mut base = vec![0.0; n];
    let mut under = vec![];
    let (mut lo, mut hi, mut x0, mut x1) = (0.0f64, 0.0f64, f64::INFINITY, f64::NEG_INFINITY);
    for (g, s) in sets.iter().enumerate() {
        let top: Vec<(f64, f64)> = s.iter().enumerate().map(|(k, &(x, y))| (x, base[k] + if y.is_finite() { y } else { 0.0 })).collect();
        let mut poly: Vec<T<'h>> = top.iter().map(|&(x, y)| h.reals(&[x, y])).collect();
        for k in (0..s.len()).rev() {
            poly.push(h.reals(&[s[k].0, base[k]]));
        }
        let c = rgb(look.color(g));
        under.push(list(h, &[color(h, c), h.call(h.sym("opacity"), &[h.real(0.35)]), h.call(h.sym("polygon"), &[list(h, &poly)])]));
        under.push(list(h, &[color(h, c), h.call(h.sym("absolute_thickness"), &[h.real(2.0)]), h.call(h.sym("line"), &[list(h, &top.iter().map(|&(x, y)| h.reals(&[x, y])).collect::<Vec<_>>())])]));
        for (k, &(x, y)) in top.iter().enumerate() {
            base[k] = y;
            lo = lo.min(y);
            hi = hi.max(y);
            x0 = x0.min(x);
            x1 = x1.max(x);
        }
    }
    look.legend_swatches = (0..sets.len()).map(|g| look.color(g)).collect();
    let extra = Extra { under, bounds: Some([(x0, if x1 > x0 { x1 } else { x0 + 1.0 }), (lo, if hi > lo { hi } else { lo + 1.0 })]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

/// Least-squares polynomial coefficients (lowest first) of degree `deg` through the points.
fn polyfit(pts: &[(f64, f64)], deg: usize) -> Option<Vec<f64>> {
    let m = deg + 1;
    if pts.len() < m {
        return None;
    }
    // normal equations, solved by Gaussian elimination with partial pivoting
    let mut a = vec![vec![0.0; m + 1]; m];
    for &(x, y) in pts {
        let pw: Vec<f64> = (0..2 * m).map(|k| x.powi(k as i32)).collect();
        for r in 0..m {
            for c in 0..m {
                a[r][c] += pw[r + c];
            }
            a[r][m] += y * pw[r];
        }
    }
    for c in 0..m {
        let p = (c..m).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        a.swap(c, p);
        if a[c][c].abs() < 1e-300 {
            return None;
        }
        for r in 0..m {
            if r != c {
                let f = a[r][c] / a[c][c];
                for k in c..=m {
                    a[r][k] -= f * a[c][k];
                }
            }
        }
    }
    Some((0..m).map(|r| a[r][m] / a[r][r]).collect())
}

fn list_fit_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut look = look_of(cx, o, Look::default(), "list_fit_plot");
    let (data, labels) = match crate::values::call_of(h, a[0]) {
        Some((n, &[x, l])) if n == mc_api::names::RULE => (x, Some(l)),
        _ => (a[0], None),
    };
    let node = chartdata::read(cx, data)?;
    let sets = xy_sets(&node);
    if sets.iter().all(|s| s.is_empty()) {
        return empty(cx, &look, "list_fit_plot");
    }
    // the model: a degree (an integer) or a name; linear by default
    let deg = match a.get(1).map(|&m| h.view(m)) {
        Some(View::Int(k)) => k.clamp(0, 8) as usize,
        Some(View::Str(s)) if s.contains("Quadratic") => 2,
        Some(View::Str(s)) if s.contains("Cubic") => 3,
        _ => 1,
    };
    let mut under = vec![];
    let (rx, ry) = (span(sets.iter().flatten().map(|p| p.0)), span(sets.iter().flatten().map(|p| p.1)));
    for (g, s) in sets.iter().enumerate() {
        let c = rgb(look.color(g));
        under.push(list(h, &[color(h, c), h.call(h.sym("absolute_point_size"), &[h.real(4.5)]), h.call(h.sym("point"), &[list(h, &s.iter().map(|&(x, y)| h.reals(&[x, y])).collect::<Vec<_>>())])]));
        if let Some(cf) = polyfit(s, deg) {
            let (a0, b0) = span(s.iter().map(|p| p.0));
            let line: Vec<T<'h>> = (0..=100).map(|k| {
                let x = a0 + (b0 - a0) * k as f64 / 100.0;
                h.reals(&[x, cf.iter().rev().fold(0.0, |acc, &q| acc * x + q)])
            }).collect();
            under.push(list(h, &[color(h, c), h.call(h.sym("absolute_thickness"), &[h.real(2.0)]), h.call(h.sym("line"), &[list(h, &line)])]));
        }
    }
    if let Some(l) = labels {
        match cx.as_list(l).map(|v| v.to_vec()) {
            Some(v) if v.len() == 2 => {
                look.x_label.get_or_insert(sym_named(h, v[0]).unwrap_or_default());
                look.y_label.get_or_insert(sym_named(h, v[1]).unwrap_or_default());
            }
            _ => {
                look.y_label.get_or_insert(sym_named(h, l).unwrap_or_default());
            }
        }
    }
    let extra = Extra { under, bounds: Some([rx, ry]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

fn point_value_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut look = look_of(cx, o, Look::default(), "point_value_plot");
    // [p -> v, ...], [p, ...] -> [v, ...], assoc(p -> v, ...)
    let mut pairs: Vec<(T<'h>, T<'h>)> = vec![];
    let rule_parts = |t: T<'h>| match crate::values::call_of(h, t) {
        Some((n, &[p, v])) if n == mc_api::names::RULE => Some((p, v)),
        _ => None,
    };
    let d = a[0];
    if let Some((ps, vs)) = rule_parts(d) {
        let vs = crate::values::spread_quantities(cx, vs, 0);
        if let (Some(ps), Some(vs)) = (cx.as_list(ps), cx.as_list(vs)) {
            pairs = ps.iter().copied().zip(vs.iter().copied()).collect();
        }
    } else if let Some((n, rs)) = crate::values::call_of(h, d).filter(|(n, _)| n == "assoc") {
        let _ = n;
        pairs = rs.iter().filter_map(|&r| rule_parts(r)).collect();
    } else if let Some(items) = cx.as_list(d) {
        pairs = items.iter().filter_map(|&r| rule_parts(r)).collect();
    }
    let mut pts = vec![];
    for (p, v) in pairs {
        let (Some(pt), Some(val)) = (chartdata::read(cx, p)?.tuple(2), crate::values::read(cx, v)?.v) else { continue };
        pts.push((pt, val));
    }
    if pts.is_empty() {
        return empty(cx, &look, "point_value_plot");
    }
    let (lo, hi) = span(pts.iter().map(|p| p.1));
    let grad = look.gradient.clone().unwrap_or_else(Gradient::default_gradient);
    let mut under = vec![h.call(h.sym("absolute_point_size"), &[h.real(9.0)])];
    for (p, v) in &pts {
        under.push(list(h, &[color(h, grad.at((v - lo) / (hi - lo))), h.call(h.sym("point"), &[h.reals(p)])]));
    }
    look.bar_range = Some((lo, hi));
    look.gradient = Some(grad);
    let extra = Extra { under, bounds: Some([span(pts.iter().map(|p| p.0[0])), span(pts.iter().map(|p| p.0[1]))]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- paired and smooth histograms, violins

/// Heights of a histogram of `data` over `edges` in the given form (count, probability, pdf, cdf, sf, hf, chf).
fn heights(data: &[f64], edges: &[f64], kind: &str) -> Vec<f64> {
    let nb = edges.len() - 1;
    let mut c = vec![0.0; nb];
    for &v in data {
        if v >= edges[0] && v <= edges[nb] {
            let k = edges.partition_point(|&e| e <= v).saturating_sub(1).min(nb - 1);
            c[k] += 1.0;
        }
    }
    let n = data.len().max(1) as f64;
    let mut cum = 0.0;
    c.iter()
        .enumerate()
        .map(|(k, &x)| {
            let w = edges[k + 1] - edges[k];
            let before = cum;
            cum += x;
            match kind {
                "PROBABILITY" => x / n,
                "PDF" => x / n / w,
                "CDF" => cum / n,
                "SF" => 1.0 - before / n,
                "HF" => x / n / w / (1.0 - before / n).max(1e-12),
                "CHF" => -(1.0 - cum / n).max(1e-12).ln(),
                "COUNTDENSITY" => x / w,
                "CUMULATIVECOUNT" => cum,
                _ => x,
            }
        })
        .collect()
}

fn bins_for(all: &[f64], spec: Option<T<'_>>, h: &Heap) -> Vec<f64> {
    let (lo, hi) = span(all.iter().copied());
    let mut sorted = all.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = match spec.and_then(|s| mc_graphics::num::number(h, s)) {
        Some(k) if k >= 1.0 => (k as usize).min(500),
        _ => {
            let iqr = quantile(&sorted, 0.75) - quantile(&sorted, 0.25);
            let w = if iqr > 0.0 { 2.0 * iqr / (all.len() as f64).cbrt() } else { (hi - lo) / 10.0 };
            (((hi - lo) / w).ceil() as usize).clamp(1, 100)
        }
    };
    (0..=n).map(|k| lo + (hi - lo) * k as f64 / n as f64).collect()
}

fn kind_arg(h: &Heap, t: Option<&T<'_>>) -> String {
    t.and_then(|&k| sym_named(h, k)).map(|s| s.to_uppercase()).unwrap_or_else(|| "COUNT".into())
}

fn paired_histogram<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "paired_histogram");
    let [l, r, rest @ ..] = a else { return Ok(Reply::Decline) };
    let (l, r) = (chartdata::read(cx, *l)?.leaves(), chartdata::read(cx, *r)?.leaves());
    let all: Vec<f64> = l.iter().chain(&r).copied().collect();
    if all.is_empty() {
        return empty(cx, &look, "paired_histogram");
    }
    let edges = bins_for(&all, rest.first().copied(), h);
    let kind = kind_arg(h, rest.get(1));
    let (hl, hr) = (heights(&l, &edges, &kind), heights(&r, &edges, &kind));
    let m = hl.iter().chain(&hr).copied().fold(1e-300f64, f64::max);
    let mut under = vec![];
    for (side, hs, c) in [(-1.0, &hl, look.color(0)), (1.0, &hr, look.color(1))] {
        let mut part = vec![color(h, rgb(c)), h.call(h.sym("edge_form"), &[h.sym_term(h.sym("white"))])];
        for (k, &v) in hs.iter().enumerate() {
            if v != 0.0 {
                part.push(h.call(h.sym("rectangle"), &[h.reals(&[0.0, edges[k]]), h.reals(&[side * v, edges[k + 1]])]));
            }
        }
        under.push(list(h, &part));
    }
    let extra = Extra { under, bounds: Some([(-m * 1.05, m * 1.05), (edges[0], edges[edges.len() - 1])]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

/// A density estimate as a curve over [lo, hi] (201 points) in the given form.
fn smooth_curve(data: &[f64], bw: f64, lo: f64, hi: f64, kind: &str) -> Vec<(f64, f64)> {
    (0..=200)
        .map(|k| {
            let x = lo + (hi - lo) * k as f64 / 200.0;
            let (p, c) = kde(data, bw, x);
            (x, match kind {
                "CDF" => c,
                "SF" => 1.0 - c,
                "HF" => p / (1.0 - c).max(1e-12),
                "CHF" => -(1.0 - c).max(1e-300).ln(),
                _ => p,
            })
        })
        .collect()
}

fn paired_smooth_histogram<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "paired_smooth_histogram");
    let [l, r, rest @ ..] = a else { return Ok(Reply::Decline) };
    let (mut l, mut r) = (chartdata::read(cx, *l)?.leaves(), chartdata::read(cx, *r)?.leaves());
    if l.is_empty() || r.is_empty() {
        return empty(cx, &look, "paired_smooth_histogram");
    }
    l.sort_by(f64::total_cmp);
    r.sort_by(f64::total_cmp);
    let kind = kind_arg(h, rest.get(1));
    let kind = if kind == "COUNT" { "PDF".to_string() } else { kind };
    let (bl, br) = (bandwidth(&l), bandwidth(&r));
    let lo = (l[0] - 3.0 * bl).min(r[0] - 3.0 * br);
    let hi = (l[l.len() - 1] + 3.0 * bl).max(r[r.len() - 1] + 3.0 * br);
    let mut under = vec![];
    let mut m = 1e-300f64;
    for (side, d, bw, c) in [(-1.0, &l, bl, look.color(0)), (1.0, &r, br, look.color(1))] {
        let curve = smooth_curve(d, bw, lo, hi, &kind);
        m = curve.iter().map(|p| p.1).fold(m, f64::max);
        let mut poly: Vec<T<'h>> = curve.iter().map(|&(y, v)| h.reals(&[side * v, y])).collect();
        poly.push(h.reals(&[0.0, hi]));
        poly.push(h.reals(&[0.0, lo]));
        under.push(list(h, &[color(h, rgb(c)), h.call(h.sym("opacity"), &[h.real(0.5)]), h.call(h.sym("polygon"), &[list(h, &poly)])]));
        under.push(list(h, &[color(h, rgb(c)), h.call(h.sym("absolute_thickness"), &[h.real(1.6)]), h.call(h.sym("line"), &[list(h, &curve.iter().map(|&(y, v)| h.reals(&[side * v, y])).collect::<Vec<_>>())])]));
    }
    let extra = Extra { under, bounds: Some([(-m * 1.05, m * 1.05), (lo, hi)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

fn smooth_density_histogram<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "smooth_density_histogram");
    let node = chartdata::read(cx, a[0])?;
    let pts: Vec<(f64, f64)> = node.items().iter().filter_map(|e| e.tuple(2)).map(|t| (t[0], t[1])).collect();
    if pts.len() < 2 {
        return empty(cx, &look, "smooth_density_histogram");
    }
    let kind = kind_arg(h, a.get(2));
    let mut xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let mut ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    let (bx, by) = (bandwidth(&xs), bandwidth(&ys));
    let (x0, x1) = (xs[0] - 2.0 * bx, xs[xs.len() - 1] + 2.0 * bx);
    let (y0, y1) = (ys[0] - 2.0 * by, ys[ys.len() - 1] + 2.0 * by);
    let n = 60;
    let norm = pts.len() as f64 * bx * by * std::f64::consts::TAU;
    let mut m = vec![vec![0.0; n]; n];
    for j in 0..n {
        cx.tick(1)?;
        for i in 0..n {
            let (x, y) = (x0 + (x1 - x0) * i as f64 / (n - 1) as f64, y0 + (y1 - y0) * j as f64 / (n - 1) as f64);
            m[j][i] = match kind.as_str() {
                "CDF" | "SF" => {
                    let c = pts.iter().map(|&(px, py)| 0.25 * (1.0 + crate::charts3::erf((x - px) / bx / std::f64::consts::SQRT_2)) * (1.0 + crate::charts3::erf((y - py) / by / std::f64::consts::SQRT_2))).sum::<f64>() / pts.len() as f64;
                    if kind == "SF" { 1.0 - c } else { c }
                }
                _ => pts.iter().map(|&(px, py)| (-0.5 * (((x - px) / bx).powi(2) + ((y - py) / by).powi(2))).exp()).sum::<f64>() / norm,
            };
        }
    }
    let rows: Vec<T<'h>> = m.iter().map(|r| h.reals(r)).collect();
    let mut opts = vec![rule(h, "data_range", list(h, &[h.reals(&[x0, x1]), h.reals(&[y0, y1])]))];
    if let Some(o) = o {
        if let View::App { args, .. } = h.view(o) {
            opts.extend(args.iter().copied());
        }
    }
    let _ = look;
    Ok(Reply::Again(h.call(h.sym("list_density_plot"), &[list(h, &rows), h.call(h.sym("options"), &opts)])))
}

fn distribution_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut look = look_of(cx, o, Look::default(), "distribution_chart");
    let node = chartdata::read(cx, a[0])?;
    let sets: Vec<(Option<String>, Vec<f64>)> = node.data_sets().into_iter().map(|(n, v)| (n.label_text(h), v)).filter(|s| s.1.len() > 1).collect();
    if sets.is_empty() {
        return empty(cx, &look, "distribution_chart");
    }
    let mut under = vec![];
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut ticks = vec![];
    for (k, (name, s)) in sets.iter().enumerate() {
        let mut s = s.clone();
        s.sort_by(f64::total_cmp);
        let bw = bandwidth(&s);
        let (a0, b0) = (s[0] - 2.0 * bw, s[s.len() - 1] + 2.0 * bw);
        lo = lo.min(a0);
        hi = hi.max(b0);
        let curve = smooth_curve(&s, bw, a0, b0, "PDF");
        let top = curve.iter().map(|p| p.1).fold(1e-300f64, f64::max);
        let x = k as f64 + 1.0;
        let mut poly: Vec<T<'h>> = curve.iter().map(|&(y, v)| h.reals(&[x + 0.4 * v / top, y])).collect();
        poly.extend(curve.iter().rev().map(|&(y, v)| h.reals(&[x - 0.4 * v / top, y])));
        let c = rgb(look.color(k));
        under.push(list(h, &[color(h, c), h.call(h.sym("edge_form"), &[color(h, Rgba::rgb(c.r * 0.7, c.g * 0.7, c.b * 0.7))]), h.call(h.sym("polygon"), &[list(h, &poly)])]));
        let (q1, med, q3) = (quantile(&s, 0.25), quantile(&s, 0.5), quantile(&s, 0.75));
        under.push(list(h, &[h.sym_term(h.sym("white")), h.call(h.sym("absolute_thickness"), &[h.real(3.0)]), seg(h, [x, q1], [x, q3]), h.call(h.sym("absolute_point_size"), &[h.real(5.0)]), h.call(h.sym("point"), &[h.reals(&[x, med])])]));
        ticks.push((x, name.clone().unwrap_or_else(|| (k + 1).to_string())));
    }
    look.x_ticks.get_or_insert(ticks);
    let extra = Extra { under, bounds: Some([(0.4, sets.len() as f64 + 0.6), (lo, hi)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- financial charts

/// A date as a number of days (a `[y, m, d]` list by the proleptic Gregorian calendar, or a number as it is).
fn day_of(n: &Node<'_>) -> Option<f64> {
    if let Some(x) = n.num() {
        return Some(x);
    }
    let v: Vec<f64> = n.items().iter().map(Node::num).collect::<Option<_>>()?;
    let (y, m, d) = (*v.first()? as i64, v.get(1).copied().unwrap_or(1.0) as i64, v.get(2).copied().unwrap_or(1.0));
    // days from civil (H. Hinnant's algorithm)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe) as f64 + d - 1.0)
}

/// Series of (time, values): `[[date, v], ...]`, `[[date, [o, h, l, c]], ...]` or plain values at 1, 2, ...
fn timed(n: &Node<'_>, width: usize) -> Vec<(f64, Vec<f64>)> {
    let mut out = vec![];
    for (k, e) in n.items().iter().enumerate() {
        if let Some(x) = e.num() {
            if width == 1 {
                out.push((k as f64 + 1.0, vec![x]));
            }
            continue;
        }
        if let Some(t) = e.tuple(width) {
            out.push((k as f64 + 1.0, t));
            continue;
        }
        if let [d, v] = e.items() {
            let vals: Option<Vec<f64>> = if width == 1 { v.num().map(|x| vec![x]) } else { v.tuple(width) };
            if let (Some(t), Some(vals)) = (day_of(d), vals) {
                out.push((t, vals));
            }
        }
    }
    out
}

fn candlestick_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "candlestick_chart");
    let node = chartdata::read(cx, a[0])?;
    let mut s = timed(&node, 4);
    if s.is_empty() {
        return empty(cx, &look, "candlestick_chart");
    }
    s.sort_by(|a, b| a.0.total_cmp(&b.0));
    // evenly spaced by order (trading days)
    let (up, down) = (Rgba::rgb(0.18, 0.55, 0.34), Rgba::rgb(0.80, 0.25, 0.22));
    let mut under = vec![];
    for (k, (_, v)) in s.iter().enumerate() {
        let x = k as f64 + 1.0;
        let (op, hi, lo, cl) = (v[0], v[1], v[2], v[3]);
        let c = if cl >= op { up } else { down };
        under.push(list(h, &[color(h, c), seg(h, [x, lo], [x, hi]), h.call(h.sym("edge_form"), &[color(h, c)]), h.call(h.sym("rectangle"), &[h.reals(&[x - 0.3, op.min(cl)]), h.reals(&[x + 0.3, op.max(cl).max(op.min(cl) + 1e-9)])])]));
    }
    let (lo, hi) = span(s.iter().flat_map(|p| p.1.iter().copied()));
    let extra = Extra { under, bounds: Some([(0.4, s.len() as f64 + 0.6), (lo, hi)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

fn renko_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "renko_chart");
    let node = chartdata::read(cx, a[0])?;
    let mut s = timed(&node, 1);
    if s.len() < 2 {
        return empty(cx, &look, "renko_chart");
    }
    s.sort_by(|a, b| a.0.total_cmp(&b.0));
    let v: Vec<f64> = s.iter().map(|p| p.1[0]).collect();
    // the brick size: given, else 2% of the price range rounded
    let (lo, hi) = span(v.iter().copied());
    let box_size = a.get(1).and_then(|&b| mc_graphics::num::number(h, b)).filter(|b| *b > 0.0).unwrap_or_else(|| ((hi - lo) / 25.0).max(1e-12));
    let (up, down) = (Rgba::rgb(0.18, 0.55, 0.34), Rgba::rgb(0.80, 0.25, 0.22));
    let mut under = vec![];
    let mut level = v[0];
    let mut k = 0usize;
    for &p in &v[1..] {
        while p >= level + box_size {
            under.push(list(h, &[color(h, up), h.call(h.sym("edge_form"), &[color(h, Rgba::rgb(0.1, 0.35, 0.2))]), h.call(h.sym("rectangle"), &[h.reals(&[k as f64, level]), h.reals(&[k as f64 + 1.0, level + box_size])])]));
            level += box_size;
            k += 1;
        }
        while p <= level - box_size {
            under.push(list(h, &[color(h, down), h.call(h.sym("edge_form"), &[color(h, Rgba::rgb(0.5, 0.12, 0.1))]), h.call(h.sym("rectangle"), &[h.reals(&[k as f64, level - box_size]), h.reals(&[k as f64 + 1.0, level])])]));
            level -= box_size;
            k += 1;
        }
        if k > 5000 {
            break;
        }
    }
    let extra = Extra { under, bounds: Some([(0.0, k.max(1) as f64), (lo, hi)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- clustering

/// The items to cluster: numbers or vectors (Euclidean distance), or strings (edit distance), with their labels.
fn cluster_items<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<(Vec<String>, Vec<Vec<f64>>, Vec<String>)> {
    let h = cx.heap();
    let (t, labels) = match crate::values::call_of(h, t) {
        Some((n, &[x, l])) if n == mc_api::names::RULE => (x, cx.as_list(l).map(|v| v.to_vec())),
        _ => (t, None),
    };
    let items: Vec<T<'h>> = match crate::values::assoc_entries(cx, t)? {
        Some(e) => {
            let names: Vec<String> = e.iter().map(|p| p.0.clone()).collect();
            let vals: Vec<T<'h>> = e.iter().map(|p| p.1).collect();
            let (_, v, s) = cluster_values(cx, &vals)?;
            return Ok((names, v, s));
        }
        None => cx.as_list(t).map(|l| l.to_vec()).unwrap_or_default(),
    };
    let (mut names, v, s) = cluster_values(cx, &items)?;
    if let Some(ls) = labels {
        names = ls.iter().map(|&l| sym_named(h, l).unwrap_or_else(|| mc_syntax::Printer::new(h).expr(l))).collect();
    }
    Ok((names, v, s))
}

fn cluster_values<'h>(cx: &mut NativeCx<'_, '_, 'h>, items: &[T<'h>]) -> Res<(Vec<String>, Vec<Vec<f64>>, Vec<String>)> {
    let h = cx.heap();
    let mut names = vec![];
    let mut vecs = vec![];
    let mut strs = vec![];
    for &e in items {
        if let View::Str(s) = h.view(e) {
            strs.push(s.to_string());
            names.push(s.to_string());
            continue;
        }
        let n = chartdata::read(cx, e)?;
        let v = n.num().map(|x| vec![x]).or_else(|| n.numbers());
        if let Some(v) = v {
            names.push(n.label_text(h).unwrap_or_else(|| mc_syntax::Printer::new(h).expr(e)));
            vecs.push(v);
        }
    }
    Ok((names, vecs, strs))
}

fn edit_distance(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
        prev = cur;
    }
    prev[b.len()] as f64
}

/// Agglomerative clustering with average linkage: the merges (a, b, height) over clusters numbered as the items
/// first and then each merge in turn.
fn cluster(dist: &[Vec<f64>]) -> Vec<(usize, usize, f64)> {
    let n = dist.len();
    let mut d = dist.to_vec();
    let mut size = vec![1usize; n];
    let mut id: Vec<usize> = (0..n).collect();
    let mut alive: Vec<bool> = vec![true; n];
    let mut merges = vec![];
    for step in 0..n.saturating_sub(1) {
        let mut best = (usize::MAX, usize::MAX, f64::INFINITY);
        for i in 0..n {
            if !alive[i] {
                continue;
            }
            for j in i + 1..n {
                if alive[j] && d[i][j] < best.2 {
                    best = (i, j, d[i][j]);
                }
            }
        }
        let (i, j, hgt) = best;
        if i == usize::MAX {
            break;
        }
        merges.push((id[i], id[j], hgt));
        for k in 0..n {
            if alive[k] && k != i && k != j {
                let v = (d[i][k] * size[i] as f64 + d[j][k] * size[j] as f64) / (size[i] + size[j]) as f64;
                d[i][k] = v;
                d[k][i] = v;
            }
        }
        size[i] += size[j];
        alive[j] = false;
        id[i] = n + step;
    }
    merges
}

fn dendrogram<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>, tree: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = if tree { "clustering_tree" } else { "dendrogram" };
    let look = look_of(cx, o, Look::default(), who);
    let (names, vecs, strs) = cluster_items(cx, a[0])?;
    let n = if strs.is_empty() { vecs.len() } else { strs.len() };
    if !(2..=2000).contains(&n) {
        return empty(cx, &look, who);
    }
    let dist: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| if strs.is_empty() { vecs[i].iter().zip(&vecs[j]).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt() } else { edit_distance(&strs[i], &strs[j]) })
                .collect()
        })
        .collect();
    let merges = cluster(&dist);
    // leaf order: depth first from the root; x of a leaf its rank, of a cluster the mean of its children
    let mut kids: Vec<Option<(usize, usize, f64)>> = vec![None; n + merges.len()];
    for (k, &(p, q, ht)) in merges.iter().enumerate() {
        kids[n + k] = Some((p, q, ht));
    }
    let root = n + merges.len() - 1;
    let mut order = vec![];
    let mut stack = vec![root];
    while let Some(c) = stack.pop() {
        match kids[c] {
            Some((p, q, _)) => {
                stack.push(q);
                stack.push(p);
            }
            None => order.push(c),
        }
    }
    let mut x = vec![0.0; n + merges.len()];
    let mut y = vec![0.0; n + merges.len()];
    for (r, &leaf) in order.iter().enumerate() {
        x[leaf] = r as f64;
    }
    let top = merges.last().map(|m| m.2).unwrap_or(1.0).max(1e-300);
    let mut lines = vec![h.call(h.sym("absolute_thickness"), &[h.real(1.2)])];
    for (k, &(p, q, ht)) in merges.iter().enumerate() {
        let c = n + k;
        x[c] = (x[p] + x[q]) / 2.0;
        y[c] = ht / top;
        let pts = [[x[p], y[p]], [x[p], y[c]], [x[q], y[c]], [x[q], y[q]]];
        lines.push(h.call(h.sym("line"), &[list(h, &pts.iter().map(|p| h.reals(p)).collect::<Vec<_>>())]));
    }
    let mut prims = vec![list(h, &[color(h, rgb(look.color(0))), list(h, &lines)])];
    if tree {
        let pts: Vec<T<'h>> = (0..n + merges.len()).map(|c| h.reals(&[x[c], y[c]])).collect();
        prims.push(list(h, &[color(h, rgb(look.color(1))), h.call(h.sym("absolute_point_size"), &[h.real(5.0)]), h.call(h.sym("point"), &[list(h, &pts)])]));
    }
    if n <= 60 {
        for (k, name) in names.iter().enumerate().take(n) {
            prims.push(text_off(h, name, [x[k], 0.0], [0.0, 1.6]));
        }
    }
    picture(cx, prims, vec![rule(h, "aspect_ratio", h.real(0.6)), rule(h, "plot_range", list(h, &[h.reals(&[-0.6, n as f64 - 0.4]), h.reals(&[-0.12, 1.06])]))], &look, who)
}

/// The first `k` principal components of the rows (centred), by power iteration with deflation.
fn principal(rows: &[Vec<f64>], k: usize) -> Vec<Vec<f64>> {
    let d = rows.first().map_or(0, |r| r.len());
    let n = rows.len() as f64;
    let mean: Vec<f64> = (0..d).map(|j| rows.iter().map(|r| r[j]).sum::<f64>() / n).collect();
    let mut cov = vec![vec![0.0; d]; d];
    for r in rows {
        for i in 0..d {
            for j in 0..d {
                cov[i][j] += (r[i] - mean[i]) * (r[j] - mean[j]) / n;
            }
        }
    }
    let mut comps: Vec<Vec<f64>> = vec![];
    for c in 0..k.min(d) {
        let mut v: Vec<f64> = (0..d).map(|j| if j == c { 1.0 } else { 0.1 / (1 + j) as f64 }).collect();
        let mut lambda = 0.0;
        for _ in 0..300 {
            let mut w: Vec<f64> = (0..d).map(|i| (0..d).map(|j| cov[i][j] * v[j]).sum()).collect();
            for u in &comps {
                let dot: f64 = w.iter().zip(u).map(|(a, b)| a * b).sum();
                w.iter_mut().zip(u).for_each(|(a, b)| *a -= dot * b);
            }
            let norm = w.iter().map(|a| a * a).sum::<f64>().sqrt();
            if norm < 1e-300 {
                break;
            }
            lambda = norm;
            v = w.iter().map(|a| a / norm).collect();
        }
        let _ = lambda;
        comps.push(v);
    }
    rows.iter().map(|r| comps.iter().map(|u| r.iter().zip(&mean).zip(u).map(|((x, m), w)| (x - m) * w).sum()).collect()).collect()
}

/// Feature vectors of groups of data (numbers or numeric vectors), projected on their first `k` components.
pub(crate) fn projected<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, k: usize) -> Res<Vec<(Option<String>, Vec<Vec<f64>>)>> {
    let h = cx.heap();
    let node = chartdata::read(cx, t)?;
    let row = |e: &Node<'_>| e.num().map(|x| vec![x]).or_else(|| e.numbers());
    let mut groups: Vec<(Option<String>, Vec<Vec<f64>>)> = vec![];
    let direct: Vec<Vec<f64>> = node.items().iter().filter_map(row).collect();
    if direct.len() * 2 >= node.items().len() && !direct.is_empty() {
        groups.push((None, direct));
    } else {
        for g in node.items() {
            let rows: Vec<Vec<f64>> = g.items().iter().filter_map(row).collect();
            if !rows.is_empty() {
                groups.push((g.label_text(h), rows));
            }
        }
    }
    let d = groups.iter().flat_map(|g| g.1.iter().map(|r| r.len())).min().unwrap_or(0);
    if d == 0 {
        return Ok(vec![]);
    }
    let all: Vec<Vec<f64>> = groups.iter().flat_map(|g| g.1.iter().map(|r| r[..d].to_vec())).collect();
    let mut proj = principal(&all, k).into_iter();
    Ok(groups
        .into_iter()
        .map(|(n, rows)| {
            (n, rows.iter().map(|_| {
                let mut p = proj.next().unwrap_or_default();
                p.resize(k, 0.0);
                p
            }).collect())
        })
        .collect())
}

fn feature_space_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "feature_space_plot");
    let groups = projected(cx, a[0], 2)?;
    if groups.is_empty() {
        return empty(cx, &look, "feature_space_plot");
    }
    let mut under = vec![];
    for (g, (_, pts)) in groups.iter().enumerate() {
        let ps: Vec<T<'h>> = pts.iter().map(|p| h.reals(p)).collect();
        under.push(list(h, &[color(h, rgb(look.color(g))), h.call(h.sym("absolute_point_size"), &[h.real(5.0)]), h.call(h.sym("point"), &[list(h, &ps)])]));
    }
    let extra = Extra { under, bounds: Some([span(groups.iter().flat_map(|g| g.1.iter().map(|p| p[0]))), span(groups.iter().flat_map(|g| g.1.iter().map(|p| p[1])))]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

fn list_curve_path_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "list_curve_path_plot");
    let node = chartdata::read(cx, a[0])?;
    let pts: Vec<[f64; 2]> = node.items().iter().filter_map(|e| e.tuple(2)).map(|t| [t[0], t[1]]).collect();
    if pts.len() < 2 {
        return empty(cx, &look, "list_curve_path_plot");
    }
    // a path by nearest neighbours from the point furthest from the centroid
    let c = [pts.iter().map(|p| p[0]).sum::<f64>() / pts.len() as f64, pts.iter().map(|p| p[1]).sum::<f64>() / pts.len() as f64];
    let d2 = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2);
    let mut left: Vec<[f64; 2]> = pts.clone();
    let start = (0..left.len()).max_by(|&i, &j| d2(left[i], c).total_cmp(&d2(left[j], c))).unwrap_or(0);
    let mut path = vec![left.swap_remove(start)];
    while !left.is_empty() {
        let last = path[path.len() - 1];
        let k = (0..left.len()).min_by(|&i, &j| d2(left[i], last).total_cmp(&d2(left[j], last))).unwrap_or(0);
        path.push(left.swap_remove(k));
    }
    let under = vec![list(h, &[color(h, rgb(look.color(0))), h.call(h.sym("absolute_thickness"), &[h.real(2.0)]), h.call(h.sym("line"), &[list(h, &path.iter().map(|p| h.reals(p)).collect::<Vec<_>>())])])];
    let extra = Extra { under, bounds: Some([span(pts.iter().map(|p| p[0])), span(pts.iter().map(|p| p[1]))]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- chromaticity

/// The spectral locus of the CIE 1931 standard colorimetric observer (2 degrees): wavelength (nm) and chromaticity
/// (x, y), from the CIE colorimetry tables (CIE 015).
const LOCUS: [(f64, f64, f64); 37] = [
    (380.0, 0.1741, 0.0050), (390.0, 0.1738, 0.0049), (400.0, 0.1733, 0.0048), (410.0, 0.1726, 0.0048), (420.0, 0.1714, 0.0051),
    (430.0, 0.1689, 0.0069), (440.0, 0.1644, 0.0109), (450.0, 0.1566, 0.0177), (460.0, 0.1440, 0.0297), (470.0, 0.1241, 0.0578),
    (475.0, 0.1096, 0.0868), (480.0, 0.0913, 0.1327), (485.0, 0.0687, 0.2007), (490.0, 0.0454, 0.2950), (495.0, 0.0235, 0.4127),
    (500.0, 0.0082, 0.5384), (505.0, 0.0039, 0.6548), (510.0, 0.0139, 0.7502), (515.0, 0.0389, 0.8120), (520.0, 0.0743, 0.8338),
    (525.0, 0.1142, 0.8262), (530.0, 0.1547, 0.8059), (540.0, 0.2296, 0.7543), (550.0, 0.3016, 0.6923), (560.0, 0.3731, 0.6245),
    (570.0, 0.4441, 0.5547), (580.0, 0.5125, 0.4866), (590.0, 0.5752, 0.4242), (600.0, 0.6270, 0.3725), (610.0, 0.6658, 0.3340),
    (620.0, 0.6915, 0.3083), (630.0, 0.7079, 0.2920), (640.0, 0.7190, 0.2809), (650.0, 0.7260, 0.2740), (660.0, 0.7300, 0.2700),
    (680.0, 0.7334, 0.2666), (700.0, 0.7347, 0.2653),
];

/// Linear sRGB (D65) of CIE XYZ.
pub(crate) fn xyz_to_linear_srgb(x: f64, y: f64, z: f64) -> [f64; 3] {
    [3.2406 * x - 1.5372 * y - 0.4986 * z, -0.9689 * x + 1.8758 * y + 0.0415 * z, 0.0557 * x - 0.2040 * y + 1.0570 * z]
}

/// The sRGB gamma of a linear value in [0, 1].
pub(crate) fn gamma(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// The xy chromaticities of the primaries (red, green, blue) and white of named RGB spaces.
pub(crate) fn primaries(name: &str) -> Option<[[f64; 2]; 4]> {
    let d65 = [0.3127, 0.3290];
    let d50 = [0.3457, 0.3585];
    Some(match name.to_lowercase().replace([' ', '-', '_'], "").as_str() {
        "srgb" | "rgb" | "rec709" => [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06], d65],
        "adobergb" => [[0.64, 0.33], [0.21, 0.71], [0.15, 0.06], d65],
        "widegamutrgb" => [[0.7347, 0.2653], [0.1152, 0.8264], [0.1566, 0.0177], d50],
        "prophotorgb" => [[0.7347, 0.2653], [0.1596, 0.8404], [0.0366, 0.0001], d50],
        "applergb" => [[0.625, 0.34], [0.28, 0.595], [0.155, 0.07], d65],
        "dcip3" | "displayp3" | "p3" => [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060], d65],
        "rec2020" | "bt2020" => [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046], d65],
        "ntsc" => [[0.67, 0.33], [0.21, 0.71], [0.14, 0.08], [0.3101, 0.3162]],
        "cie" | "ciergb" => [[0.7347, 0.2653], [0.2738, 0.7174], [0.1666, 0.0089], [1.0 / 3.0, 1.0 / 3.0]],
        _ => return None,
    })
}

fn chromaticity_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let look = look_of(cx, o, Look::default(), "chromaticity_plot");
    // the spectral locus
    let locus: Vec<[f64; 2]> = LOCUS.iter().map(|&(_, x, y)| [x, y]).collect();
    // the inside, coloured (xyY with the brightest Y the sRGB gamut allows)
    let n = 120;
    let inside = |p: [f64; 2]| -> bool {
        let mut c = false;
        let m = locus.len();
        for i in 0..m {
            let (a2, b2) = (locus[i], locus[(i + 1) % m]);
            if (a2[1] > p[1]) != (b2[1] > p[1]) && p[0] < (b2[0] - a2[0]) * (p[1] - a2[1]) / (b2[1] - a2[1]) + a2[0] {
                c = !c;
            }
        }
        c
    };
    let (w, ht) = (0.8, 0.9);
    let mut cells = Vec::with_capacity(n * n);
    for j in 0..n {
        for i in 0..n {
            let p = [w * (i as f64 + 0.5) / n as f64, ht * (j as f64 + 0.5) / n as f64];
            if !inside(p) {
                cells.push(Rgba { a: 0.0, ..Rgba::rgb(1.0, 1.0, 1.0) });
                continue;
            }
            let (x, y) = (p[0], p[1]);
            let lin = xyz_to_linear_srgb(x / y, 1.0, (1.0 - x - y) / y);
            let m = lin.iter().copied().fold(1e-12f64, f64::max);
            cells.push(Rgba::rgb(gamma(lin[0].max(0.0) / m), gamma(lin[1].max(0.0) / m), gamma(lin[2].max(0.0) / m)));
        }
    }
    let mut prims = vec![crate::charts2::raster_of(h, &cells, n, n, (0.0, w, 0.0, ht))];
    let mut closed: Vec<T<'h>> = locus.iter().map(|p| h.reals(p)).collect();
    closed.push(h.reals(&locus[0]));
    prims.push(list(h, &[h.call(h.sym("absolute_thickness"), &[h.real(1.2)]), h.call(h.sym("line"), &[list(h, &closed)])]));
    for &(nm, x, y) in LOCUS.iter().filter(|p| [460.0, 480.0, 500.0, 520.0, 540.0, 560.0, 580.0, 600.0, 620.0].contains(&p.0)) {
        prims.push(text_off(h, &format!("{}", nm as i64), [x, y], [if nm < 520.0 { 1.4 } else { -1.4 }, 0.0]));
    }
    // the spaces asked for: names, or colours (points)
    let items: Vec<T<'h>> = a.first().map(|&s| cx.as_list(s).map(|l| l.to_vec()).unwrap_or_else(|| vec![s])).unwrap_or_default();
    let mut k = 0;
    for it in items {
        if let Some(name) = sym_named(h, it) {
            if let Some(p) = primaries(&name) {
                let tri: Vec<T<'h>> = [p[0], p[1], p[2], p[0]].iter().map(|q| h.reals(q)).collect();
                prims.push(list(h, &[color(h, rgb(look.color(k))), h.call(h.sym("absolute_thickness"), &[h.real(2.0)]), h.call(h.sym("line"), &[list(h, &tri)]), h.call(h.sym("absolute_point_size"), &[h.real(5.0)]), h.call(h.sym("point"), &[h.reals(&p[3])])]));
                k += 1;
            }
            continue;
        }
        if let mc_graphics::color::Colour::Value(c) = mc_graphics::color::colour(h, it) {
            let lin = |u: f64| if u <= 0.04045 { u / 12.92 } else { ((u + 0.055) / 1.055).powf(2.4) };
            let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
            let (x, y, z) = (0.4124 * r + 0.3576 * g + 0.1805 * b, 0.2126 * r + 0.7152 * g + 0.0722 * b, 0.0193 * r + 0.1192 * g + 0.9505 * b);
            let s = (x + y + z).max(1e-12);
            prims.push(list(h, &[h.call(h.sym("absolute_point_size"), &[h.real(7.0)]), h.call(h.sym("point"), &[h.reals(&[x / s, y / s])])]));
        }
    }
    picture(cx, prims, vec![rule(h, "frame", h.sym_term(h.sym("true"))), rule(h, "aspect_ratio", h.sym_term(h.sym("automatic"))), rule(h, "plot_range", list(h, &[h.reals(&[0.0, w]), h.reals(&[0.0, ht])]))], &look, "chromaticity_plot")
}
