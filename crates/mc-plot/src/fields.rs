//! Functions of two variables as colours: `density_plot` (a smooth raster), `contour_plot` (filled regions between
//! levels and the level lines) and `list_density_plot` (a matrix of values), with a colour bar.
//!
//! The function is sampled on an n x n grid of the rectangle. A density plot draws a raster finer than the grid
//! (values interpolated bilinearly) so that it looks smooth; a contour plot fills, for each level c, the region
//! where f >= c in the colour of the band above c, over the band below (the region is built cell by cell: the
//! part of each grid cell where the bilinear surface is at least c, cut at the edges where it crosses c, one path
//! of non-overlapping pieces so it fills without seams), and draws the lines where f = c (marching squares, the
//! ambiguous saddle cells decided by the cell's mean).
use crate::figure::{Contours, Extra, Look};
use crate::{bad, finish, plot_options, real};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::Rgba;
use mc_graphics::gradient::Gradient;
use mc_numfn::{Options, Var};
use std::collections::HashMap;

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("density_plot"), Attributes::default(), Box::new(Field { contour: false }));
    k.register(h.sym("contour_plot"), Attributes::default(), Box::new(Field { contour: true }));
    k.register(h.sym("list_density_plot"), Attributes::default(), Box::new(ListDensity { contour: false }));
    k.register(h.sym("list_contour_plot"), Attributes::default(), Box::new(ListDensity { contour: true }));
}

/// The page of these plots: the plot area about square, the colour bar right of it.
const SIZE: (f64, f64) = (540.0, 440.0);
/// The finest raster a density plot draws, in cells per side.
const MAX_CELLS: usize = 320;
/// Contour lines: gray, thin, half transparent.
const LINE_GRAY: f64 = 0.15;

/// `density_plot` or `contour_plot` of a function.
struct Field {
    contour: bool,
}

/// Values on a grid: `v[j * nx + i]` at (xs[i], ys[j]); NaN where there is none.
pub(crate) struct Grid {
    xs: Vec<f64>,
    ys: Vec<f64>,
    v: Vec<f64>,
    /// where the function stops having values on grid edges between a point with a value and one without: the
    /// point (found by bisection) and the value there, by the edge's two point indices (lower first)
    pub(crate) cut: HashMap<(usize, usize), ((f64, f64), f64)>,
}

impl Grid {
    fn at(&self, i: usize, j: usize) -> f64 {
        self.v[j * self.xs.len() + i]
    }

    /// The least and greatest finite value.
    fn extent(&self) -> Option<(f64, f64)> {
        let (lo, hi) = self.v.iter().filter(|v| v.is_finite()).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        lo.is_finite().then_some((lo, hi))
    }

    /// The value at (x, y) inside the grid, bilinear between its four nearest points; where some of them have no
    /// value, the nearest point's value (NaN if it has none).
    fn interpolate(&self, x: f64, y: f64) -> f64 {
        let (nx, ny) = (self.xs.len(), self.ys.len());
        let locate = |v: &[f64], n: usize, z: f64| {
            let u = ((z - v[0]) / (v[n - 1] - v[0]) * (n - 1) as f64).clamp(0.0, (n - 1) as f64);
            let k = (u.floor() as usize).min(n - 2);
            (k, u - k as f64)
        };
        let (i, fx) = locate(&self.xs, nx, x);
        let (j, fy) = locate(&self.ys, ny, y);
        let (a, b, c, d) = (self.at(i, j), self.at(i + 1, j), self.at(i, j + 1), self.at(i + 1, j + 1));
        if a.is_finite() && b.is_finite() && c.is_finite() && d.is_finite() {
            let lo = a + (b - a) * fx;
            let hi = c + (d - c) * fx;
            return lo + (hi - lo) * fy;
        }
        let (ni, nj) = (if fx < 0.5 { i } else { i + 1 }, if fy < 0.5 { j } else { j + 1 });
        self.at(ni, nj)
    }
}

/// The ends of a range spec `[x, a, b]`: the variable and a < b.
fn spec<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<(Sym, f64, f64)>> {
    let h = cx.heap();
    let Some(&[v, a, b]) = cx.as_list(t) else { return Ok(None) };
    let View::Sym(v) = h.view(v) else { return Ok(None) };
    let (Some(a), Some(b)) = (real(cx, a)?, real(cx, b)?) else { return Ok(None) };
    Ok((a < b).then_some((v, a, b)))
}

impl Native for Field {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = if self.contour { "contour_plot" } else { "density_plot" };
        let allowed: &[&str] = if self.contour {
            &["color_function", "color_bar", "plot_points", "contours"]
        } else {
            &["color_function", "color_bar", "plot_points"]
        };
        let base = Look { exact: true, size: Some(SIZE), ..Look::default() };
        let (f, sx, sy, look) = match args {
            [f, sx, sy] => (*f, *sx, *sy, base),
            [f, sx, sy, o] => match plot_options(cx, *o, base, allowed, name) {
                Some(look) => (*f, *sx, *sy, look),
                None => return Ok(bad(h, name, 4)),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some((x, a, b)) = spec(cx, sx)? else { return Ok(bad(h, name, 2)) };
        let Some((y, c, d)) = spec(cx, sy)? else { return Ok(bad(h, name, 3)) };
        if x == y {
            return Ok(bad(h, name, 3));
        }
        let n = look.plot_points.unwrap_or(if self.contour { 80 } else { 100 });
        // equations `lhs == rhs` (one or a list): the curves where they hold
        let equations: Option<Vec<T<'h>>> = if self.contour {
            let items: Vec<T<'h>> = cx.as_list(f).map(|l| l.to_vec()).unwrap_or_else(|| vec![f]);
            items.iter().map(|&e| equation(h, e)).collect::<Option<Vec<_>>>().map(|v| {
                v.into_iter()
                    .map(|(l, r)| h.call(h.sym(mc_api::names::ADD), &[l, h.call(h.sym(mc_api::names::MUL), &[h.int(-1), r])]))
                    .collect()
            })
        } else {
            None
        };
        let xs: Vec<f64> = (0..n).map(|i| a + (b - a) * i as f64 / (n - 1) as f64).collect();
        let ys: Vec<f64> = (0..n).map(|j| c + (d - c) * j as f64 / (n - 1) as f64).collect();
        let sample = |cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>| -> Res<Grid> {
            let mut fun = mc_numfn::compile_expr(cx, f, &[Var::scalar(x), Var::scalar(y)], &Options::default())?;
            let mut v = Vec::with_capacity(n * n);
            let mut out = [0.0];
            for &yj in &ys {
                cx.tick(1)?;
                for &xi in &xs {
                    let ok = fun.eval(cx, &[xi, yj], &mut out)?;
                    v.push(if ok && out[0].is_finite() { out[0] } else { f64::NAN });
                }
            }
            let mut g = Grid { xs: xs.clone(), ys: ys.clone(), v, cut: HashMap::new() };
            // where the values stop between two neighbouring points: located by bisection on the edge
            let (nx, ny) = (xs.len(), ys.len());
            for jj in 0..ny {
                cx.tick(1)?;
                for ii in 0..nx {
                    for (di, dj) in [(1usize, 0usize), (0, 1)] {
                        let (i2, j2) = (ii + di, jj + dj);
                        if i2 >= nx || j2 >= ny {
                            continue;
                        }
                        let (va, vb) = (g.at(ii, jj), g.at(i2, j2));
                        if va.is_finite() == vb.is_finite() {
                            continue;
                        }
                        // from the point with a value (a) towards the one without (b)
                        let (pa, pb, mut val) =
                            if va.is_finite() { ((xs[ii], ys[jj]), (xs[i2], ys[j2]), va) } else { ((xs[i2], ys[j2]), (xs[ii], ys[jj]), vb) };
                        let (mut lo, mut hi) = (0.0f64, 1.0f64);
                        for _ in 0..20 {
                            let t = 0.5 * (lo + hi);
                            let q = (pa.0 + (pb.0 - pa.0) * t, pa.1 + (pb.1 - pa.1) * t);
                            let ok = fun.eval(cx, &[q.0, q.1], &mut out)?;
                            if ok && out[0].is_finite() {
                                lo = t;
                                val = out[0];
                            } else {
                                hi = t;
                            }
                        }
                        let q = (pa.0 + (pb.0 - pa.0) * lo, pa.1 + (pb.1 - pa.1) * lo);
                        let (a, b) = (jj * nx + ii, j2 * nx + i2);
                        g.cut.insert((a.min(b), a.max(b)), (q, val));
                    }
                }
            }
            Ok(g)
        };
        if let Some(eqs) = equations {
            let mut grids = Vec::with_capacity(eqs.len());
            for e in eqs {
                grids.push(sample(cx, e)?);
            }
            return curves_of(cx, &grids, look);
        }
        let grid = sample(cx, f)?;
        let Some(extent) = grid.extent() else { return Ok(bad(h, name, 1)) };
        if self.contour {
            contours(cx, &grid, extent, look)
        } else {
            density(cx, &grid, extent, look)
        }
    }
}

/// The gradient of a plot.
fn gradient(look: &Look) -> Gradient {
    look.gradient.clone().unwrap_or_else(Gradient::default_gradient)
}

/// The colour of value `v` on a scale from `lo` to `hi`.
fn shade(g: &Gradient, (lo, hi): (f64, f64), v: f64) -> Rgba {
    if hi > lo {
        g.at((v - lo) / (hi - lo))
    } else {
        g.at(0.5)
    }
}

/// A density plot of the grid: a raster of interpolated colours over the grid's rectangle.
fn density<'h>(cx: &mut NativeCx<'_, '_, 'h>, grid: &Grid, extent: (f64, f64), mut look: Look<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let g = gradient(&look);
    let (nx, ny) = (grid.xs.len(), grid.ys.len());
    let (cols, rows) = ((3 * nx).min(MAX_CELLS).max(nx), (3 * ny).min(MAX_CELLS).max(ny));
    let (x0, x1) = (grid.xs[0], grid.xs[nx - 1]);
    let (y0, y1) = (grid.ys[0], grid.ys[ny - 1]);
    let mut data = Vec::with_capacity(cols * rows * 4);
    for r in 0..rows {
        if r % 64 == 0 {
            cx.tick(1)?;
        }
        let y = y0 + (y1 - y0) * (r as f64 + 0.5) / rows as f64;
        for k in 0..cols {
            let x = x0 + (x1 - x0) * (k as f64 + 0.5) / cols as f64;
            let v = grid.interpolate(x, y);
            if v.is_finite() {
                let c = shade(&g, extent, v);
                data.extend_from_slice(&[c.r, c.g, c.b, 1.0]);
            } else {
                data.extend_from_slice(&[0.0, 0.0, 0.0, 0.0]);
            }
        }
    }
    let cells = h.real_array(&[rows, cols, 4], &data);
    let rect = h.call(h.sym(mc_api::names::LIST), &[h.reals(&[x0, y0]), h.reals(&[x1, y1])]);
    let raster = h.call(h.sym("raster"), &[cells, rect]);
    look.bar_range = Some(extent);
    let extra = Extra { under: vec![raster], bounds: Some([(x0, x1), (y0, y1)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

/// The levels of a contour plot strictly inside the values' extent, increasing.
fn levels(spec: &Option<Contours>, (lo, hi): (f64, f64)) -> Vec<f64> {
    let mut v = match spec {
        None => (1..=10).map(|k| lo + (hi - lo) * k as f64 / 11.0).collect(),
        Some(Contours::Count(n)) => (1..=*n).map(|k| lo + (hi - lo) * k as f64 / (*n + 1) as f64).collect(),
        Some(Contours::Values(v)) => v.clone(),
    };
    v.retain(|c| *c > lo && *c < hi);
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

/// A contour plot of the grid: bands of colour between the levels, and the level lines.
fn contours<'h>(cx: &mut NativeCx<'_, '_, 'h>, grid: &Grid, extent: (f64, f64), mut look: Look<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let g = gradient(&look);
    let ls = levels(&look.contours, extent);
    let list = |items: &[T<'h>]| h.call(h.sym(mc_api::names::LIST), items);
    let poly_term = |rings: &[Vec<(f64, f64)>]| {
        let rs: Vec<T<'h>> = rings
            .iter()
            .map(|r| {
                let flat: Vec<f64> = r.iter().flat_map(|&(x, y)| [x, y]).collect();
                h.real_array(&[r.len(), 2], &flat)
            })
            .collect();
        h.call(h.sym("polygon"), &[list(&rs)])
    };
    // band k lies between level k - 1 and level k (the ends of the extent outside); its colour is that of its middle
    let mut bounds = vec![extent.0];
    bounds.extend(&ls);
    bounds.push(extent.1);
    let band = |k: usize| shade(&g, extent, 0.5 * (bounds[k] + bounds[k + 1]));
    let mut under = vec![];
    for k in 0..=ls.len() {
        cx.tick(1)?;
        let c = if k == 0 { f64::NEG_INFINITY } else { ls[k - 1] };
        let rings = region(grid, c);
        if !rings.is_empty() {
            under.push(list(&[mc_graphics::rgb_term(h, band(k)), poly_term(&rings)]));
        }
    }
    let mut lines = vec![];
    for &c in &ls {
        cx.tick(1)?;
        for l in level_lines(grid, c) {
            let flat: Vec<f64> = l.iter().flat_map(|&(x, y)| [x, y]).collect();
            lines.push(h.real_array(&[l.len(), 2], &flat));
        }
    }
    if !lines.is_empty() {
        under.push(list(&[
            h.call(h.sym("gray_level"), &[h.real(LINE_GRAY), h.real(0.55)]),
            h.call(h.sym("absolute_thickness"), &[h.real(0.75)]),
            h.call(h.sym("line"), &[list(&lines)]),
        ]));
    }
    let (nx, ny) = (grid.xs.len(), grid.ys.len());
    look.bar_range = Some(extent);
    let extra = Extra { under, bounds: Some([(grid.xs[0], grid.xs[nx - 1]), (grid.ys[0], grid.ys[ny - 1])]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

/// `lhs == rhs` as its two sides.
fn equation<'h>(h: &'h Heap, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    match h.view(t) {
        View::App { head, args: &[l, r] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "equal") => Some((l, r)),
        _ => None,
    }
}

/// The curves where each grid's values are 0 (the equations of a contour plot), each in its colour, without
/// filled regions or a colour bar.
fn curves_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, grids: &[Grid], mut look: Look<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let list = |items: &[T<'h>]| h.call(h.sym(mc_api::names::LIST), items);
    let mut under = vec![];
    for (k, g) in grids.iter().enumerate() {
        cx.tick(1)?;
        let mut lines = vec![];
        for l in level_lines(g, 0.0) {
            let flat: Vec<f64> = l.iter().flat_map(|&(x, y)| [x, y]).collect();
            lines.push(h.real_array(&[l.len(), 2], &flat));
        }
        if !lines.is_empty() {
            let (r, gr, b) = look.color(k);
            let c = Rgba::rgb(r as f64 / 255.0, gr as f64 / 255.0, b as f64 / 255.0);
            let mut part = vec![mc_graphics::rgb_term(h, c), h.call(h.sym("absolute_thickness"), &[h.real(2.0)])];
            if !look.styles.is_empty() {
                part.push(look.styles[k % look.styles.len()]);
            }
            part.push(h.call(h.sym("line"), &[list(&lines)]));
            under.push(list(&part));
        }
    }
    let g = &grids[0];
    let (nx, ny) = (g.xs.len(), g.ys.len());
    look.bar_range = None;
    let extra = Extra { under, bounds: Some([(g.xs[0], g.xs[nx - 1]), (g.ys[0], g.ys[ny - 1])]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

/// The corners of cell (i, j) in order around it (anticlockwise in plot coordinates): grid indices.
fn corners(i: usize, j: usize) -> [(usize, usize); 4] {
    [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)]
}

/// Where the surface crosses `c` on the grid edge between points p and q: computed from the lower-indexed end, so
/// the two cells sharing the edge get the identical point.
fn crossing(grid: &Grid, p: (usize, usize), q: (usize, usize), c: f64) -> (f64, f64) {
    let (p, q) = if (p.1, p.0) <= (q.1, q.0) { (p, q) } else { (q, p) };
    let (vp, vq) = (grid.at(p.0, p.1), grid.at(q.0, q.1));
    let t = if vq != vp { ((c - vp) / (vq - vp)).clamp(0.0, 1.0) } else { 0.5 };
    let (xp, yp) = (grid.xs[p.0], grid.ys[p.1]);
    let (xq, yq) = (grid.xs[q.0], grid.ys[q.1]);
    (xp + (xq - xp) * t, yp + (yq - yp) * t)
}

/// The region where the surface is at least `c` (everywhere it has values for c = -inf), as rings that do not
/// overlap and all run anticlockwise: runs of whole cells as rectangles, cut cells as their inside part.
pub(crate) fn region(grid: &Grid, c: f64) -> Vec<Vec<(f64, f64)>> {
    let (nx, ny) = (grid.xs.len(), grid.ys.len());
    let mut rings = vec![];
    for j in 0..ny - 1 {
        let mut run: Option<usize> = None;
        let close = |run: &mut Option<usize>, end: usize, rings: &mut Vec<Vec<(f64, f64)>>| {
            if let Some(s) = run.take() {
                let (x0, x1, y0, y1) = (grid.xs[s], grid.xs[end], grid.ys[j], grid.ys[j + 1]);
                rings.push(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
            }
        };
        for i in 0..nx - 1 {
            let cs = corners(i, j);
            let vs = cs.map(|(a, b)| grid.at(a, b));
            if vs.iter().any(|v| !v.is_finite()) {
                close(&mut run, i, &mut rings);
                if let Some(r) = partial(grid, cs, c) {
                    rings.push(r);
                }
                continue;
            }
            let inside = vs.map(|v| v >= c);
            if inside.iter().all(|b| *b) {
                run.get_or_insert(i);
                continue;
            }
            close(&mut run, i, &mut rings);
            if !inside.iter().any(|b| *b) {
                continue;
            }
            let at = |k: usize| (grid.xs[cs[k].0], grid.ys[cs[k].1]);
            let cut = |k: usize| crossing(grid, cs[k], cs[(k + 1) % 4], c);
            let mean = vs.iter().sum::<f64>() / 4.0;
            let saddle = inside == [true, false, true, false] || inside == [false, true, false, true];
            if saddle && mean < c {
                // two separate corners
                for k in (0..4).filter(|&k| inside[k]) {
                    rings.push(vec![cut((k + 3) % 4), at(k), cut(k)]);
                }
                continue;
            }
            let mut ring = vec![];
            for k in 0..4 {
                if inside[k] {
                    ring.push(at(k));
                }
                if inside[k] != inside[(k + 1) % 4] {
                    ring.push(cut(k));
                }
            }
            rings.push(ring);
        }
        close(&mut run, nx - 1, &mut rings);
    }
    rings
}

/// The part of a cell with corners without values where the surface is at least `c`: the cell cut at the edge
/// of the function's domain (the points [`Grid::cut`] holds), then at the level c (linear between the points).
/// `None` when nothing is left or an edge point is not known.
fn partial(grid: &Grid, cs: [(usize, usize); 4], c: f64) -> Option<Vec<(f64, f64)>> {
    let nx = grid.xs.len();
    let mut poly: Vec<((f64, f64), f64)> = vec![];
    for k in 0..4 {
        let (p, q) = (cs[k], cs[(k + 1) % 4]);
        let vp = grid.at(p.0, p.1);
        if vp.is_finite() {
            poly.push(((grid.xs[p.0], grid.ys[p.1]), vp));
        }
        if vp.is_finite() != grid.at(q.0, q.1).is_finite() {
            let (a, b) = (p.1 * nx + p.0, q.1 * nx + q.0);
            poly.push(*grid.cut.get(&(a.min(b), a.max(b)))?);
        }
    }
    if poly.len() < 3 {
        return None;
    }
    if c == f64::NEG_INFINITY {
        return Some(poly.into_iter().map(|p| p.0).collect());
    }
    // keep where the value is at least c
    let mut out = vec![];
    for k in 0..poly.len() {
        let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
        if a.1 >= c {
            out.push(a.0);
        }
        if (a.1 >= c) != (b.1 >= c) {
            let t = (c - a.1) / (b.1 - a.1);
            out.push((a.0 .0 + (b.0 .0 - a.0 .0) * t, a.0 .1 + (b.0 .1 - a.0 .1) * t));
        }
    }
    (out.len() >= 3).then_some(out)
}

/// The lines where the surface equals `c`, joined from the segments of the cells (marching squares).
pub(crate) fn level_lines(grid: &Grid, c: f64) -> Vec<Vec<(f64, f64)>> {
    let (nx, ny) = (grid.xs.len(), grid.ys.len());
    // a crossing is named by its grid edge (the two point indices, lower first)
    type Key = (usize, usize);
    let key = |p: (usize, usize), q: (usize, usize)| {
        let (a, b) = (p.1 * nx + p.0, q.1 * nx + q.0);
        (a.min(b), a.max(b))
    };
    let mut segs: Vec<(Key, Key)> = vec![];
    let mut points: HashMap<Key, (f64, f64)> = HashMap::new();
    for j in 0..ny - 1 {
        for i in 0..nx - 1 {
            let cs = corners(i, j);
            let vs = cs.map(|(a, b)| grid.at(a, b));
            if vs.iter().any(|v| !v.is_finite()) {
                continue;
            }
            let inside = vs.map(|v| v >= c);
            let edges: Vec<usize> = (0..4).filter(|&k| inside[k] != inside[(k + 1) % 4]).collect();
            let mut add = |e: usize, f: usize| {
                let (ke, kf) = (key(cs[e], cs[(e + 1) % 4]), key(cs[f], cs[(f + 1) % 4]));
                points.entry(ke).or_insert_with(|| crossing(grid, cs[e], cs[(e + 1) % 4], c));
                points.entry(kf).or_insert_with(|| crossing(grid, cs[f], cs[(f + 1) % 4], c));
                segs.push((ke, kf));
            };
            match edges[..] {
                [e, f] => add(e, f),
                [_, _, _, _] => {
                    // a saddle: the mean decides which pair of opposite corners is joined
                    let mean = vs.iter().sum::<f64>() / 4.0;
                    // the corners whose crossings are cut off: around corner k lie edges k - 1 and k
                    let cut_off = if (mean >= c) == inside[0] { [1, 3] } else { [0, 2] };
                    for k in cut_off {
                        add((k + 3) % 4, k);
                    }
                }
                _ => {}
            }
        }
    }
    join(&segs, &points)
}

/// A crossing, named by its grid edge.
type Edge = (usize, usize);
/// A segment between two crossings.
type Segment = (Edge, Edge);

/// Segments joined into polylines at shared ends: open chains from their ends first, then closed loops.
fn join(segs: &[Segment], points: &HashMap<Edge, (f64, f64)>) -> Vec<Vec<(f64, f64)>> {
    let mut at: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (k, (a, b)) in segs.iter().enumerate() {
        at.entry(*a).or_default().push(k);
        at.entry(*b).or_default().push(k);
    }
    let mut used = vec![false; segs.len()];
    let mut lines = vec![];
    let mut starts: Vec<(usize, usize)> = at.iter().filter(|(_, v)| v.len() == 1).map(|(k, _)| *k).collect();
    starts.sort_unstable();
    let mut all: Vec<(usize, usize)> = at.keys().copied().collect();
    all.sort_unstable();
    starts.extend(all);
    for s in starts {
        let mut cur = s;
        let mut line = vec![];
        while let Some(&k) = at.get(&cur).and_then(|v| v.iter().find(|&&k| !used[k])) {
            used[k] = true;
            if line.is_empty() {
                line.push(points[&cur]);
            }
            let (a, b) = segs[k];
            cur = if a == cur { b } else { a };
            line.push(points[&cur]);
        }
        if line.len() > 1 {
            lines.push(line);
        }
    }
    lines
}

/// `list_density_plot(matrix)`: rows from the bottom up, at x = 1..columns, y = 1..rows unless `data_range` says.
struct ListDensity {
    contour: bool,
}

impl Native for ListDensity {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = if self.contour { "list_contour_plot" } else { "list_density_plot" };
        let base = Look { exact: true, size: Some(SIZE), ..Look::default() };
        let allowed: &[&str] =
            if self.contour { &["color_function", "color_bar", "data_range", "contours"] } else { &["color_function", "color_bar", "data_range"] };
        let (m, look) = match args {
            [m] => (*m, base),
            [m, o] => match plot_options(cx, *o, base, allowed, name) {
                Some(look) => (*m, look),
                None => return Ok(bad(h, name, 2)),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(rows) = cx.as_list(m) else { return Ok(bad(h, name, 1)) };
        // scattered values [[x, y, f], ...]: interpolated onto a grid
        if rows.len() > 3 && rows.iter().all(|&r| cx.as_list(r).is_some_and(|l| l.len() == 3)) {
            let mut pts = Vec::with_capacity(rows.len());
            for &r in rows {
                let l = cx.as_list(r).unwrap_or(&[]);
                let mut p = [0.0; 3];
                for (k, &e) in l.iter().enumerate() {
                    p[k] = real(cx, e)?.unwrap_or(f64::NAN);
                }
                pts.push(p);
            }
            let Some(grid) = scattered_grid(cx, &pts)? else { return Ok(bad(h, name, 1)) };
            let Some(extent) = grid.extent() else { return Ok(bad(h, name, 1)) };
            return if self.contour { contours(cx, &grid, extent, look) } else { density(cx, &grid, extent, look) };
        }
        let mut v = vec![];
        let mut cols = 0;
        for (j, &r) in rows.iter().enumerate() {
            cx.tick(1)?;
            let row: Vec<f64> = match h.packed(r) {
                Some(mc_api::Packed::Reals { data, dims }) if dims.len() == 1 => data.to_vec(),
                Some(mc_api::Packed::Ints { data, dims }) if dims.len() == 1 => data.iter().map(|&k| k as f64).collect(),
                _ => {
                    let Some(items) = cx.as_list(r) else { return Ok(bad(h, name, 1)) };
                    let mut out = Vec::with_capacity(items.len());
                    for &e in items {
                        out.push(match mc_graphics::num::number(h, e) {
                            Some(x) => x,
                            None => real(cx, e)?.unwrap_or(f64::NAN),
                        });
                    }
                    out
                }
            };
            if j == 0 {
                cols = row.len();
            }
            if row.len() != cols || cols == 0 || cols * rows.len() > 4_000_000 {
                return Ok(bad(h, name, 1));
            }
            v.extend(row);
        }
        if rows.len() < 2 || cols < 2 {
            return Ok(bad(h, name, 1));
        }
        let [[x0, x1], [y0, y1]] = look.data_range.unwrap_or([[1.0, cols as f64], [1.0, rows.len() as f64]]);
        let ny = rows.len();
        let xs = (0..cols).map(|i| x0 + (x1 - x0) * i as f64 / (cols - 1) as f64).collect();
        let ys = (0..ny).map(|j| y0 + (y1 - y0) * j as f64 / (ny - 1) as f64).collect();
        let grid = Grid { xs, ys, v, cut: HashMap::new() };
        let Some(extent) = grid.extent() else { return Ok(bad(h, name, 1)) };
        if self.contour {
            contours(cx, &grid, extent, look)
        } else {
            density(cx, &grid, extent, look)
        }
    }
}

/// Scattered values `[x, y, f]` on a 60 x 60 grid over their extent: at each grid point the
/// inverse-distance-weighted mean of the values (NaN farther than twice the typical spacing of the points from
/// all of them).
fn scattered_grid(cx: &mut NativeCx<'_, '_, '_>, pts: &[[f64; 3]]) -> Res<Option<Grid>> {
    let pts: Vec<[f64; 3]> = pts.iter().copied().filter(|p| p.iter().all(|v| v.is_finite())).collect();
    if pts.len() < 3 {
        return Ok(None);
    }
    let ext = |k: usize| pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p[k]), b.max(p[k])));
    let ((x0, x1), (y0, y1)) = (ext(0), ext(1));
    if x1 <= x0 || y1 <= y0 {
        return Ok(None);
    }
    let n = 60;
    let spacing = ((x1 - x0) * (y1 - y0) / pts.len() as f64).sqrt();
    let xs: Vec<f64> = (0..n).map(|i| x0 + (x1 - x0) * i as f64 / (n - 1) as f64).collect();
    let ys: Vec<f64> = (0..n).map(|j| y0 + (y1 - y0) * j as f64 / (n - 1) as f64).collect();
    let mut v = Vec::with_capacity(n * n);
    for &y in &ys {
        cx.tick(1)?;
        for &x in &xs {
            let (mut ws, mut zs, mut near) = (0.0, 0.0, f64::INFINITY);
            let mut exact = None;
            for q in &pts {
                let (dx, dy) = ((q[0] - x) / (x1 - x0), (q[1] - y) / (y1 - y0));
                let d2 = dx * dx + dy * dy;
                near = near.min(((q[0] - x).powi(2) + (q[1] - y).powi(2)).sqrt());
                if d2 < 1e-24 {
                    exact = Some(q[2]);
                    break;
                }
                let w = 1.0 / (d2 * d2);
                ws += w;
                zs += w * q[2];
            }
            v.push(match exact {
                Some(z) => z,
                None if near <= 2.0 * spacing => zs / ws,
                None => f64::NAN,
            });
        }
    }
    Ok(Some(Grid { xs, ys, v, cut: HashMap::new() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grid of x^2 + y^2 on [-1, 1]^2 with n points per side.
    /// Gap queue G40: a cell with a corner outside the domain is cut at the domain's edge, not dropped whole.
    #[test]
    fn cells_are_cut_at_the_edge_of_the_domain() {
        // corners (0,0) (1,0) (1,1) (0,1); the point (1,1) has no value; the domain ends at x = 0.5 on the top
        // edge and at y = 0.25 on the right edge
        let mut cut = HashMap::new();
        cut.insert((1, 3), ((1.0, 0.25), 1.0));
        cut.insert((2, 3), ((0.5, 1.0), 1.0));
        let g = Grid { xs: vec![0.0, 1.0], ys: vec![0.0, 1.0], v: vec![1.0, 1.0, 1.0, f64::NAN], cut };
        let rings = region(&g, f64::NEG_INFINITY);
        assert_eq!(rings, vec![vec![(0.0, 0.0), (1.0, 0.0), (1.0, 0.25), (0.5, 1.0), (0.0, 1.0)]]);
        // without the edge points nothing is drawn rather than a whole cell
        let g = Grid { cut: HashMap::new(), ..g };
        assert!(region(&g, f64::NEG_INFINITY).is_empty());
    }

    fn bowl(n: usize) -> Grid {
        let xs: Vec<f64> = (0..n).map(|i| -1.0 + 2.0 * i as f64 / (n - 1) as f64).collect();
        let v = xs.iter().flat_map(|&y| xs.iter().map(move |&x| x * x + y * y)).collect();
        Grid { xs: xs.clone(), ys: xs, v, cut: HashMap::new() }
    }

    /// The signed area of a ring.
    fn area(r: &[(f64, f64)]) -> f64 {
        (0..r.len()).map(|k| {
            let (a, b) = (r[k], r[(k + 1) % r.len()]);
            a.0 * b.1 - b.0 * a.1
        }).sum::<f64>() / 2.0
    }

    #[test]
    fn regions_are_anticlockwise_pieces_with_the_right_area() {
        let g = bowl(41);
        // f >= 0.25 is the square minus a disc of radius 0.5
        let rings = region(&g, 0.25);
        assert!(rings.iter().all(|r| area(r) >= -1e-12));
        let total: f64 = rings.iter().map(|r| area(r)).sum();
        let want = 4.0 - std::f64::consts::PI * 0.25;
        assert!((total - want).abs() < 0.01, "{total} {want}");
        // everything
        let all: f64 = region(&g, f64::NEG_INFINITY).iter().map(|r| area(r)).sum();
        assert!((all - 4.0).abs() < 1e-12);
    }

    #[test]
    fn level_lines_are_joined_into_closed_curves() {
        let g = bowl(41);
        let lines = level_lines(&g, 0.25);
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert!(l.len() > 20);
        assert_eq!(l.first(), l.last());
        assert!(l.iter().all(|&(x, y)| ((x * x + y * y).sqrt() - 0.5).abs() < 0.01));
        // levels inside the extent only
        assert_eq!(levels(&Some(Contours::Values(vec![3.0, -1.0, 0.5, 0.5])), (0.0, 2.0)), vec![0.5]);
        assert_eq!(levels(&None, (0.0, 11.0)).len(), 10);
    }

    #[test]
    fn saddles_are_decided_by_the_mean() {
        // x*y on [-1, 1]^2 with 2 points per side: one saddle cell, mean 0
        let g = Grid { xs: vec![-1.0, 1.0], ys: vec![-1.0, 1.0], v: vec![1.0, -1.0, -1.0, 1.0], cut: HashMap::new() };
        // at c = 0.5 the mean is below: two separate corners (two triangles, two lines)
        assert_eq!(region(&g, 0.5).len(), 2);
        assert_eq!(level_lines(&g, 0.5).len(), 2);
        // at c = -0.5 the mean is above: one piece
        assert_eq!(region(&g, -0.5).len(), 1);
        assert_eq!(level_lines(&g, -0.5).len(), 2);
        assert!((g.interpolate(0.0, 0.0)).abs() < 1e-12);
    }
}
