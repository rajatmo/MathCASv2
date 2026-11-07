//! Plots in space: `contour_plot3d` (level surfaces of f(x, y, z)), `region_plot3d` (the boundary of where a
//! condition holds), `list_contour_plot3d` (level surfaces of a 3D array), `density_plot3d` (values as coloured,
//! translucent points), `discrete_plot3d` (values at integer points with stems), `bar_chart3d` (bars as
//! cuboids) and `pie_chart3d` (drawn as the flat pie chart).
//!
//! Surfaces come from marching tetrahedra: each cell of a grid is cut into six tetrahedra along its diagonal, and
//! the level surface crosses each tetrahedron in a triangle or a quadrilateral whose corners are interpolated
//! linearly on its edges.
use crate::fields::spec;
use crate::more::split;
use crate::vectors::pass_options;
use crate::{bad, finish_graphic, real};
use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::Rgba;
use mc_graphics::gradient::Gradient;
use mc_numfn::{Options, Var};

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let a = Attributes::default;
    for (name, kind) in [
        ("contour_plot3d", Kind::Contour),
        ("region_plot3d", Kind::Region),
        ("density_plot3d", Kind::Density),
        ("list_contour_plot3d", Kind::ListContour),
        ("discrete_plot3d", Kind::Discrete),
        ("bar_chart3d", Kind::Bars),
        ("pie_chart3d", Kind::Pie),
    ] {
        k.register(h.sym(name), a(), Box::new(Space { name, kind }));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Contour,
    Region,
    Density,
    ListContour,
    Discrete,
    Bars,
    Pie,
}

struct Space {
    name: &'static str,
    kind: Kind,
}

fn list<'h>(h: &'h Heap, items: &[T<'h>]) -> T<'h> {
    h.call(h.sym(names::LIST), items)
}

fn rule<'h>(h: &'h Heap, k: &str, v: T<'h>) -> T<'h> {
    h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v])
}

const PALETTE: [(f64, f64, f64); 4] = [(0.95, 0.75, 0.45), (0.55, 0.7, 0.95), (0.6, 0.85, 0.6), (0.9, 0.6, 0.75)];

/// Values on a grid of `n[0] x n[1] x n[2]` points: `v[(k * n1 + j) * n0 + i]`.
struct Grid3 {
    lo: [f64; 3],
    hi: [f64; 3],
    n: [usize; 3],
    v: Vec<f64>,
}

impl Grid3 {
    fn point(&self, i: usize, j: usize, k: usize) -> [f64; 3] {
        let c = |d: usize, t: usize| self.lo[d] + (self.hi[d] - self.lo[d]) * t as f64 / (self.n[d] - 1) as f64;
        [c(0, i), c(1, j), c(2, k)]
    }
    fn at(&self, i: usize, j: usize, k: usize) -> f64 {
        self.v[(k * self.n[1] + j) * self.n[0] + i]
    }
    fn extent(&self) -> Option<(f64, f64)> {
        let (a, b) = self.v.iter().filter(|x| x.is_finite()).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| (a.min(x), b.max(x)));
        a.is_finite().then_some((a, b))
    }
}

/// The triangles of the surface where the grid's values equal `c` (flat: 9 numbers per triangle).
fn iso_surface(g: &Grid3, c: f64) -> Vec<f64> {
    const TETS: [[usize; 4]; 6] = [[0, 1, 3, 7], [0, 3, 2, 7], [0, 2, 6, 7], [0, 6, 4, 7], [0, 4, 5, 7], [0, 5, 1, 7]];
    let mut out = vec![];
    for k in 0..g.n[2] - 1 {
        for j in 0..g.n[1] - 1 {
            for i in 0..g.n[0] - 1 {
                let corner = |q: usize| (i + (q & 1), j + (q >> 1 & 1), k + (q >> 2 & 1));
                let vals: [f64; 8] = std::array::from_fn(|q| {
                    let (a, b, d) = corner(q);
                    g.at(a, b, d)
                });
                if vals.iter().any(|v| !v.is_finite()) {
                    continue;
                }
                if vals.iter().all(|&v| v >= c) || vals.iter().all(|&v| v < c) {
                    continue;
                }
                let pts: [[f64; 3]; 8] = std::array::from_fn(|q| {
                    let (a, b, d) = corner(q);
                    g.point(a, b, d)
                });
                for t in TETS {
                    let inside: Vec<usize> = t.iter().copied().filter(|&q| vals[q] >= c).collect();
                    let outside: Vec<usize> = t.iter().copied().filter(|&q| vals[q] < c).collect();
                    let cut = |a: usize, b: usize| -> [f64; 3] {
                        let s = (c - vals[a]) / (vals[b] - vals[a]);
                        std::array::from_fn(|d| pts[a][d] + s * (pts[b][d] - pts[a][d]))
                    };
                    match (inside.len(), outside.len()) {
                        (1, 3) => {
                            for p in [cut(inside[0], outside[0]), cut(inside[0], outside[1]), cut(inside[0], outside[2])] {
                                out.extend_from_slice(&p);
                            }
                        }
                        (3, 1) => {
                            for p in [cut(outside[0], inside[0]), cut(outside[0], inside[1]), cut(outside[0], inside[2])] {
                                out.extend_from_slice(&p);
                            }
                        }
                        (2, 2) => {
                            let (a, b, p, q) = (inside[0], inside[1], outside[0], outside[1]);
                            let quad = [cut(a, p), cut(a, q), cut(b, q), cut(b, p)];
                            for tri in [[0, 1, 2], [0, 2, 3]] {
                                for &v in &tri {
                                    out.extend_from_slice(&quad[v]);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    out
}

fn sample<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, vars: &[Sym; 3], lo: [f64; 3], hi: [f64; 3], n: usize) -> Res<Grid3> {
    let vs: Vec<Var> = vars.iter().map(|&s| Var::scalar(s)).collect();
    let mut fun = mc_numfn::compile_expr(cx, f, &vs, &Options::default())?;
    let mut g = Grid3 { lo, hi, n: [n; 3], v: Vec::with_capacity(n * n * n) };
    let mut out = [0.0];
    for k in 0..n {
        cx.tick(1)?;
        for j in 0..n {
            for i in 0..n {
                let p = g.point(i, j, k);
                g.v.push(if fun.eval(cx, &p, &mut out)? && out[0].is_finite() { out[0] } else { f64::NAN });
            }
        }
    }
    Ok(g)
}

fn ranges<'h>(cx: &mut NativeCx<'_, '_, 'h>, specs: &[T<'h>], name: &str) -> Res<Result<([Sym; 3], [f64; 3], [f64; 3]), Reply<'h>>> {
    let h = cx.heap();
    let mut vars = vec![];
    let (mut lo, mut hi) = ([0.0; 3], [0.0; 3]);
    for (d, s) in specs.iter().enumerate() {
        let Some((v, a, b)) = spec(cx, *s)? else { return Ok(Err(bad(h, name, d as i64 + 2))) };
        vars.push(v);
        lo[d] = a;
        hi[d] = b;
    }
    Ok(Ok(([vars[0], vars[1], vars[2]], lo, hi)))
}

fn surface_term<'h>(h: &'h Heap, tris: &[f64], (r, g, b): (f64, f64, f64), opacity: f64) -> Option<T<'h>> {
    let q = tris.len() / 9;
    if q == 0 {
        return None;
    }
    let mut part = vec![h.call(h.sym("rgb_color"), &[h.real(r), h.real(g), h.real(b)])];
    if opacity < 1.0 {
        part.push(h.call(h.sym("opacity"), &[h.real(opacity)]));
    }
    part.push(h.call(h.sym("edge_form"), &[h.sym_term(h.sym("none"))]));
    part.push(h.call(h.sym("polygon"), &[h.real_array(&[q, 3, 3], tris)]));
    Some(list(h, &part))
}

/// The levels of a `contours` option: a count or a list; `None` when not given.
fn contour_option<'h>(cx: &mut NativeCx<'_, '_, 'h>, o: Option<T<'h>>) -> Res<Option<Vec<f64>>> {
    let h = cx.heap();
    let Some(o) = o else { return Ok(None) };
    let View::App { args: rs, .. } = h.view(o) else { return Ok(None) };
    for &r in rs {
        if let View::App { args: &[k, v], .. } = h.view(r) {
            if matches!(h.view(k), View::Sym(s) if h.sym_name(s) == "contours") {
                if let View::Int(n) = h.view(v) {
                    return Ok(Some(vec![f64::NAN; n.clamp(1, 20) as usize]));
                }
                let mut out = vec![];
                for &x in cx.as_list(v).unwrap_or(&[]) {
                    if let Some(c) = real(cx, x)? {
                        out.push(c);
                    }
                }
                return Ok(Some(out));
            }
        }
    }
    Ok(None)
}

fn finish3<'h>(cx: &mut NativeCx<'_, '_, 'h>, prims: Vec<T<'h>>, lo: [f64; 3], hi: [f64; 3], o: Option<T<'h>>, name: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (passed, ignored) = pass_options(cx, o);
    let mut opts = vec![rule(h, "axes", h.sym_term(h.sym("true"))), rule(h, "plot_range", list(h, &(0..3).map(|d| h.reals(&[lo[d], hi[d]])).collect::<Vec<_>>()))];
    opts.extend(passed);
    let g = h.call(h.sym("graphics3d"), &[list(h, &prims), h.call(h.sym("options"), &opts)]);
    let ignored: Vec<String> = ignored.into_iter().filter(|k| !matches!(k.as_str(), "contours" | "plot_points" | "mesh" | "max_recursion" | "color_function" | "contour_style" | "plot_theme")).collect();
    finish_graphic(cx, g, name, &ignored)
}

impl Native for Space {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = self.name;
        let (a, o) = split(h, args);
        match (self.kind, a.as_slice()) {
            (Kind::Contour | Kind::Region | Kind::Density, [f, sx, sy, sz]) => {
                let (vars, lo, hi) = match ranges(cx, &[*sx, *sy, *sz], name)? {
                    Ok(r) => r,
                    Err(e) => return Ok(e),
                };
                let mut prims = vec![];
                match self.kind {
                    Kind::Density => {
                        let g = sample(cx, *f, &vars, lo, hi, 14)?;
                        let Some((a0, b0)) = g.extent() else { return Ok(bad(h, name, 1)) };
                        let grad = Gradient::default_gradient();
                        prims.push(h.call(h.sym("absolute_point_size"), &[h.real(6.0)]));
                        for k in 0..14 {
                            for j in 0..14 {
                                for i in 0..14 {
                                    let v = g.at(i, j, k);
                                    if !v.is_finite() {
                                        continue;
                                    }
                                    let t = if b0 > a0 { (v - a0) / (b0 - a0) } else { 0.5 };
                                    let c = grad.at(t);
                                    prims.push(list(h, &[mc_graphics::rgb_term(h, Rgba { a: 0.15 + 0.6 * t, ..c }), h.call(h.sym("point"), &[h.reals(&g.point(i, j, k))])]));
                                }
                            }
                        }
                    }
                    _ => {
                        let items: Vec<T<'h>> = cx.as_list(*f).map(|l| l.to_vec()).unwrap_or_else(|| vec![*f]);
                        let levels_given = contour_option(cx, o)?;
                        for (s, &item) in items.iter().enumerate() {
                            // an equation: its zero set; a condition: the boundary of its margin; a function: levels
                            let (g_expr, fixed): (T<'h>, Option<f64>) = match (self.kind, mc_geom::region::call_parts(h, item)) {
                                (Kind::Contour, Some((n, ab))) if n == "equal" && ab.len() == 2 => {
                                    (h.call(h.sym(names::ADD), &[ab[0], h.call(h.sym(names::MUL), &[h.int(-1), ab[1]])]), Some(0.0))
                                }
                                (Kind::Region, _) => (mc_geom::margin(h, item).unwrap_or_else(|| h.call(h.sym("if"), &[item, h.int(1), h.int(-1)])), Some(0.0)),
                                _ => (item, None),
                            };
                            let n = if self.kind == Kind::Region { 36 } else { 32 };
                            let g = sample(cx, g_expr, &vars, lo, hi, n)?;
                            let levels: Vec<f64> = match fixed {
                                Some(c) => vec![c],
                                None => {
                                    let Some((a0, b0)) = g.extent() else { continue };
                                    match &levels_given {
                                        Some(l) if l.iter().all(|x| x.is_finite()) => l.clone(),
                                        Some(l) => (1..=l.len()).map(|k| a0 + (b0 - a0) * k as f64 / (l.len() + 1) as f64).collect(),
                                        None => (1..=3).map(|k| a0 + (b0 - a0) * k as f64 / 4.0).collect(),
                                    }
                                }
                            };
                            let many = levels.len() > 1;
                            for (k, &c) in levels.iter().enumerate() {
                                cx.tick(1)?;
                                let tris = iso_surface(&g, c);
                                let col = PALETTE[(s + k) % PALETTE.len()];
                                if let Some(t) = surface_term(h, &tris, col, if many { 0.6 } else { 1.0 }) {
                                    prims.push(t);
                                }
                            }
                        }
                    }
                }
                if prims.is_empty() {
                    // nothing to draw: an empty box
                    prims.push(list(h, &[]));
                }
                finish3(cx, prims, lo, hi, o, name)
            }
            (Kind::ListContour, [d]) => {
                // a 3D array of values: rows along y, layers along z
                let Some(layers) = cx.as_list(*d).map(|l| l.to_vec()) else { return Ok(bad(h, name, 1)) };
                let mut v = vec![];
                let (mut ny, mut nx) = (0, 0);
                for &layer in &layers {
                    let Some(rows) = cx.as_list(layer).map(|l| l.to_vec()) else { return Ok(bad(h, name, 1)) };
                    ny = rows.len();
                    for &r in &rows {
                        let Some(xs) = cx.as_list(r).map(|l| l.to_vec()) else { return Ok(bad(h, name, 1)) };
                        nx = xs.len();
                        for x in xs {
                            v.push(real(cx, x)?.unwrap_or(f64::NAN));
                        }
                    }
                }
                let nz = layers.len();
                if nx < 2 || ny < 2 || nz < 2 || v.len() != nx * ny * nz {
                    return Ok(bad(h, name, 1));
                }
                let (lo, hi) = ([1.0, 1.0, 1.0], [nx as f64, ny as f64, nz as f64]);
                let g = Grid3 { lo, hi, n: [nx, ny, nz], v };
                let Some((a0, b0)) = g.extent() else { return Ok(bad(h, name, 1)) };
                let levels: Vec<f64> = match contour_option(cx, o)? {
                    Some(l) if l.iter().all(|x| x.is_finite()) => l,
                    Some(l) => (1..=l.len()).map(|k| a0 + (b0 - a0) * k as f64 / (l.len() + 1) as f64).collect(),
                    None => (1..=3).map(|k| a0 + (b0 - a0) * k as f64 / 4.0).collect(),
                };
                let many = levels.len() > 1;
                let mut prims = vec![];
                for (k, c) in levels.into_iter().enumerate() {
                    if let Some(t) = surface_term(h, &iso_surface(&g, c), PALETTE[k % PALETTE.len()], if many { 0.6 } else { 1.0 }) {
                        prims.push(t);
                    }
                }
                finish3(cx, prims, lo, hi, o, name)
            }
            (Kind::Discrete, [f, si, sj, ..]) => {
                let mut axes = vec![];
                for (d, s) in [si, sj].into_iter().enumerate() {
                    let Some(&[v, lo, hi]) = cx.as_list(*s) else { return Ok(bad(h, name, d as i64 + 2)) };
                    let (Some(lo), Some(hi)) = (real(cx, lo)?, real(cx, hi)?) else { return Ok(bad(h, name, d as i64 + 2)) };
                    let View::Sym(v) = h.view(v) else { return Ok(bad(h, name, d as i64 + 2)) };
                    if hi - lo > 200.0 {
                        return Ok(bad(h, name, d as i64 + 2));
                    }
                    axes.push((v, lo, hi));
                }
                let vs = [Var::scalar(axes[0].0), Var::scalar(axes[1].0)];
                let mut fun = mc_numfn::compile_expr(cx, *f, &vs, &Options::default())?;
                let mut pts = vec![];
                let mut stems = vec![];
                let mut out = [0.0];
                let (mut zlo, mut zhi) = (0.0f64, 0.0f64);
                let mut i = axes[0].1;
                while i <= axes[0].2 + 1e-9 {
                    let mut j = axes[1].1;
                    while j <= axes[1].2 + 1e-9 {
                        if fun.eval(cx, &[i, j], &mut out)? && out[0].is_finite() {
                            pts.push(h.reals(&[i, j, out[0]]));
                            stems.push(list(h, &[h.reals(&[i, j, 0.0]), h.reals(&[i, j, out[0]])]));
                            zlo = zlo.min(out[0]);
                            zhi = zhi.max(out[0]);
                        }
                        j += 1.0;
                    }
                    i += 1.0;
                }
                if pts.is_empty() {
                    return Ok(bad(h, name, 1));
                }
                let col = h.call(h.sym("rgb_color"), &[h.real(0.24), h.real(0.38), h.real(0.62)]);
                let prims = vec![list(h, &[col, h.call(h.sym("line"), &[list(h, &stems)]), h.call(h.sym("absolute_point_size"), &[h.real(6.0)]), h.call(h.sym("point"), &[list(h, &pts)])])];
                let pad = ((zhi - zlo) * 0.05).max(1e-9);
                finish3(cx, prims, [axes[0].1 - 0.5, axes[1].1 - 0.5, zlo - pad], [axes[0].2 + 0.5, axes[1].2 + 0.5, zhi + pad], o, name)
            }
            (Kind::Bars, [d]) => {
                // a list of heights (one row) or a matrix (rows along y)
                let rows: Vec<T<'h>> = match cx.as_list(*d) {
                    Some(l) if l.iter().all(|&r| cx.as_list(r).is_some()) => l.to_vec(),
                    Some(_) => vec![*d],
                    None => return Ok(bad(h, name, 1)),
                };
                let mut prims = vec![h.call(h.sym("edge_form"), &[h.call(h.sym("gray_level"), &[h.real(0.3)])])];
                let (mut zlo, mut zhi, mut nx) = (0.0f64, 0.0f64, 0usize);
                for (j, &r) in rows.iter().enumerate() {
                    let vals = cx.as_list(r).map(|l| l.to_vec()).unwrap_or_default();
                    nx = nx.max(vals.len());
                    let (cr, cg, cb) = PALETTE[j % PALETTE.len()];
                    let mut part = vec![h.call(h.sym("rgb_color"), &[h.real(cr), h.real(cg), h.real(cb)])];
                    for (i, &v) in vals.iter().enumerate() {
                        let Some(z) = real(cx, v)? else { continue };
                        zlo = zlo.min(z);
                        zhi = zhi.max(z);
                        let (x, y) = (i as f64 + 1.0, j as f64 + 1.0);
                        part.push(h.call(h.sym("cuboid"), &[h.reals(&[x - 0.35, y - 0.35, 0.0f64.min(z)]), h.reals(&[x + 0.35, y + 0.35, 0.0f64.max(z)])]));
                    }
                    prims.push(list(h, &part));
                }
                if nx == 0 {
                    return Ok(bad(h, name, 1));
                }
                finish3(cx, prims, [0.4, 0.4, zlo], [nx as f64 + 0.6, rows.len() as f64 + 0.6, if zhi > zlo { zhi } else { zlo + 1.0 }], o, name)
            }
            (Kind::Pie, [d]) => {
                let mut args = vec![*d];
                args.extend(o);
                Ok(Reply::Again(h.call(h.sym("pie_chart"), &args)))
            }
            _ => Ok(Reply::Decline),
        }
    }
}
