//! Vector fields: `vector_plot` (arrows on a grid, scaled and coloured by the field's magnitude),
//! `stream_plot` (streamlines spread evenly over the rectangle, each with an arrowhead in its middle, coloured by
//! magnitude), their list forms (`list_vector_plot`, `list_stream_plot`: a matrix of vectors, or
//! `[[x, y], [vx, vy]]` pairs), the complex forms (`complex_vector_plot`, `complex_stream_plot`: the field
//! (Re f, Im f)), and the 3D forms `vector_plot3d` and `stream_plot3d`.
//!
//! Streamlines are integrated (classical Runge-Kutta on the normalised field) both ways from seeds on a grid; a
//! line stops where it leaves the rectangle, where the field vanishes or has no value, where it closes on itself,
//! or where it comes into a cell of a coarse occupancy grid another line already crosses, so that lines keep
//! apart.
use crate::fields::spec;
use crate::figure::{Extra, Look};
use crate::more::split;
use crate::{bad, finish, finish_graphic, plot_options};
use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::Rgba;
use mc_graphics::gradient::Gradient;
use mc_numfn::{NumFn, Options, Var};

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let a = Attributes::default;
    for (name, kind, stream) in [
        ("vector_plot", Kind::Function, false),
        ("stream_plot", Kind::Function, true),
        ("list_vector_plot", Kind::List, false),
        ("list_stream_plot", Kind::List, true),
        ("complex_vector_plot", Kind::Complex, false),
        ("complex_stream_plot", Kind::Complex, true),
    ] {
        k.register(h.sym(name), a(), Box::new(FieldPlot { name, kind, stream }));
    }
    k.register(h.sym("vector_plot3d"), a(), Box::new(Field3 { name: "vector_plot3d", stream: false, list: false }));
    k.register(h.sym("stream_plot3d"), a(), Box::new(Field3 { name: "stream_plot3d", stream: true, list: false }));
    k.register(h.sym("list_vector_plot3d"), a(), Box::new(Field3 { name: "list_vector_plot3d", stream: false, list: true }));
    k.register(h.sym("list_stream_plot3d"), a(), Box::new(Field3 { name: "list_stream_plot3d", stream: true, list: true }));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Function,
    List,
    Complex,
}

fn list<'h>(h: &'h Heap, items: &[T<'h>]) -> T<'h> {
    h.call(h.sym(names::LIST), items)
}

/// A vector field to evaluate at points.
enum Source<'h> {
    /// compiled components in the coordinates
    Fun(Vec<Box<dyn NumFn<'h, f64> + 'h>>),
    /// values on a regular grid: `v[(k * ny + j) * nx + i]` component-wise, at the grid's points
    Grid { lo: Vec<f64>, step: Vec<f64>, n: Vec<usize>, v: Vec<Vec<f64>> },
    /// scattered samples, interpolated by inverse squared distance from the nearest few
    Scattered(Vec<(Vec<f64>, Vec<f64>)>),
}

impl<'h> Source<'h> {
    fn at(&mut self, cx: &mut NativeCx<'_, '_, 'h>, p: &[f64]) -> Res<Option<Vec<f64>>> {
        Ok(match self {
            Source::Fun(fs) => {
                let mut out = Vec::with_capacity(fs.len());
                let mut o = [0.0];
                for f in fs.iter_mut() {
                    if !f.eval(cx, p, &mut o)? || !o[0].is_finite() {
                        return Ok(None);
                    }
                    out.push(o[0]);
                }
                Some(out)
            }
            Source::Grid { lo, step, n, v } => {
                // multilinear interpolation
                let d = n.len();
                let mut base = vec![0usize; d];
                let mut frac = vec![0.0; d];
                for k in 0..d {
                    let u = (p[k] - lo[k]) / step[k];
                    if !(-1e-9..=(n[k] - 1) as f64 + 1e-9).contains(&u) {
                        return Ok(None);
                    }
                    let u = u.clamp(0.0, (n[k] - 1) as f64);
                    let b = (u.floor() as usize).min(n[k].saturating_sub(2));
                    base[k] = b;
                    frac[k] = if n[k] > 1 { u - b as f64 } else { 0.0 };
                }
                let comps = v.len();
                let mut out = vec![0.0; comps];
                for corner in 0..(1usize << d) {
                    let mut w = 1.0;
                    let mut idx = 0;
                    let mut mul = 1;
                    for k in 0..d {
                        let up = corner >> k & 1 == 1;
                        let i = (base[k] + usize::from(up)).min(n[k] - 1);
                        w *= if up { frac[k] } else { 1.0 - frac[k] };
                        idx += i * mul;
                        mul *= n[k];
                    }
                    if w == 0.0 {
                        continue;
                    }
                    for (c, o) in out.iter_mut().enumerate() {
                        *o += w * v[c][idx];
                    }
                }
                out.iter().all(|x| x.is_finite()).then_some(out)
            }
            Source::Scattered(pts) => {
                let mut near: Vec<(f64, usize)> = pts.iter().enumerate().map(|(i, (q, _))| (q.iter().zip(p).map(|(a, b)| (a - b).powi(2)).sum::<f64>(), i)).collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
                if near.is_empty() {
                    return Ok(None);
                }
                if near[0].0 < 1e-24 {
                    return Ok(Some(pts[near[0].1].1.clone()));
                }
                let mut out = vec![0.0; pts[0].1.len()];
                let mut wsum = 0.0;
                for &(d2, i) in near.iter().take(6) {
                    let w = 1.0 / d2;
                    wsum += w;
                    for (o, v) in out.iter_mut().zip(&pts[i].1) {
                        *o += w * v;
                    }
                }
                Some(out.into_iter().map(|x| x / wsum).collect())
            }
        })
    }
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// The colour of a magnitude between the least and greatest ones.
fn colour_of(g: &Gradient, (lo, hi): (f64, f64), m: f64) -> Rgba {
    let t = if hi > lo { ((m - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.5 };
    g.at(t)
}

fn default_gradient(look: &Look) -> Gradient {
    look.gradient.clone().unwrap_or_else(Gradient::default_gradient)
}

/// The numeric matrix or list of a data term as nested machine reals.
fn data<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, depth: usize) -> Res<Option<Vec<f64>>> {
    let h = cx.heap();
    let mut out = vec![];
    fn walk<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, depth: usize, out: &mut Vec<f64>) -> Res<bool> {
        if depth == 0 {
            return Ok(match crate::real(cx, t)? {
                Some(v) => {
                    out.push(v);
                    true
                }
                None => false,
            });
        }
        let Some(items) = cx.as_list(t) else { return Ok(false) };
        for &x in items {
            if !walk(cx, x, depth - 1, out)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    let _ = h;
    Ok(walk(cx, t, depth, &mut out)?.then_some(out))
}

/// The shape of nested lists (lengths along the first element).
fn shape<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Vec<usize> {
    let mut dims = vec![];
    let mut cur = t;
    while let Some(items) = cx.as_list(cur) {
        dims.push(items.len());
        let Some(&first) = items.first() else { break };
        cur = first;
    }
    dims
}

/// A list field: a d-dimensional array of d-vectors (`dims = [n_y, n_x, d]` in the plane: rows along y), or
/// pairs `[[x, y], [vx, vy]]`; with the coordinates' bounds.
#[allow(clippy::type_complexity)]
fn list_source<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, d: usize, range: Option<Vec<(f64, f64)>>) -> Res<Option<(Source<'h>, Vec<(f64, f64)>)>> {
    let dims = shape(cx, t);
    if dims.len() == 3 && dims[1] == 2 && dims[2] == d {
        // pairs of a point and a vector
        let Some(flat) = data(cx, t, 3)? else { return Ok(None) };
        let pts: Vec<(Vec<f64>, Vec<f64>)> = flat.chunks(2 * d).map(|c| (c[..d].to_vec(), c[d..].to_vec())).collect();
        let mut b = vec![(f64::INFINITY, f64::NEG_INFINITY); d];
        for (p, _) in &pts {
            for k in 0..d {
                b[k] = (b[k].0.min(p[k]), b[k].1.max(p[k]));
            }
        }
        // a degenerate range is widened, so the arrows have room
        let b = b.into_iter().map(|(lo, hi)| if hi > lo { (lo, hi) } else { (lo - 1.0, hi + 1.0) }).collect();
        return Ok(Some((Source::Scattered(pts), b)));
    }
    if dims.len() == d + 1 && dims[d] == d && dims[..d].iter().all(|&n| n >= 2) {
        let Some(flat) = data(cx, t, d + 1)? else { return Ok(None) };
        // the array's outermost index is the last coordinate (rows along y)
        let n: Vec<usize> = dims[..d].iter().rev().copied().collect();
        let total: usize = n.iter().product();
        let mut v = vec![vec![0.0; total]; d];
        for (idx, chunk) in flat.chunks(d).enumerate() {
            for c in 0..d {
                v[c][idx] = chunk[c];
            }
        }
        let b: Vec<(f64, f64)> = match range {
            Some(r) if r.len() == d => r,
            _ => n.iter().map(|&k| (1.0, k as f64)).collect(),
        };
        let lo: Vec<f64> = b.iter().map(|x| x.0).collect();
        let step: Vec<f64> = b.iter().zip(&n).map(|(x, &k)| (x.1 - x.0) / (k - 1) as f64).collect();
        return Ok(Some((Source::Grid { lo, step, n, v }, b)));
    }
    Ok(None)
}

/// `[[x0, x1], [y0, y1], ...]` of a `data_range` option.
fn data_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, o: Option<T<'h>>) -> Res<Option<Vec<(f64, f64)>>> {
    let h = cx.heap();
    let Some(o) = o else { return Ok(None) };
    let Some((_, rules)) = mc_geom::region::call_parts(h, o) else { return Ok(None) };
    for &r in rules {
        if let Some((n, &[k, v])) = mc_geom::region::call_parts(h, r) {
            if n == names::RULE && matches!(h.view(k), View::Sym(s) if h.sym_name(s) == "data_range") {
                let mut out = vec![];
                for &row in cx.as_list(v).unwrap_or(&[]) {
                    let Some(&[a, b]) = cx.as_list(row) else { return Ok(None) };
                    let (Some(a), Some(b)) = (crate::real(cx, a)?, crate::real(cx, b)?) else { return Ok(None) };
                    out.push((a, b));
                }
                return Ok(Some(out));
            }
        }
    }
    Ok(None)
}

/// The components of a field compiled in the coordinates.
fn compile_field<'h>(cx: &mut NativeCx<'_, '_, 'h>, comps: &[T<'h>], vars: &[Sym]) -> Res<Source<'h>> {
    let vs: Vec<Var> = vars.iter().map(|&s| Var::scalar(s)).collect();
    let mut fs = vec![];
    for &c in comps {
        fs.push(mc_numfn::compile_expr(cx, c, &vs, &Options::default())?);
    }
    Ok(Source::Fun(fs))
}

struct FieldPlot {
    name: &'static str,
    kind: Kind,
    stream: bool,
}

impl Native for FieldPlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = self.name;
        let (a, o) = split(h, args);
        let base = Look { exact: true, size: Some((460.0, 440.0)), ..Look::default() };
        let extra_opts = ["plot_points", "color_function", "stream_points", "vector_points", "stream_scale", "vector_scale", "data_range", "vector_color_function", "stream_color_function", "stream_style", "vector_style", "vector_markers", "stream_markers", "region_function", "vector_range", "vector_size_function", "color_function_scaling", "region_bounding_style"];
        let look = match o {
            Some(o) => match plot_options(cx, o, base, &extra_opts, name) {
                Some(l) => l,
                None => return Ok(bad(h, name, a.len() as i64 + 1)),
            },
            None => base,
        };
        let (mut src, rect) = match (self.kind, a.as_slice()) {
            (Kind::Function, [f, sx, sy]) => {
                let Some((x, x0, x1)) = spec(cx, *sx)? else { return Ok(bad(h, name, 2)) };
                let Some((y, y0, y1)) = spec(cx, *sy)? else { return Ok(bad(h, name, 3)) };
                let Some(comps) = cx.as_list(*f).filter(|l| l.len() == 2).map(|l| l.to_vec()) else { return Ok(bad(h, name, 1)) };
                (compile_field(cx, &comps, &[x, y])?, [x0, x1, y0, y1])
            }
            (Kind::Complex, [f, s]) => {
                let Some(&[z, lo, hi]) = cx.as_list(*s) else { return Ok(bad(h, name, 2)) };
                let ends = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<(f64, f64)>> {
                    let re = crate::real(cx, h.call(h.sym("re"), &[t]))?;
                    let im = crate::real(cx, h.call(h.sym("im_part"), &[t]))?;
                    Ok(re.zip(im))
                };
                let (Some(p), Some(q)) = (ends(cx, lo)?, ends(cx, hi)?) else { return Ok(bad(h, name, 2)) };
                let (x, y) = (h.sym("plot$x"), h.sym("plot$y"));
                let i = h.call(h.sym(names::COMPLEX), &[h.int(0), h.int(1)]);
                let zz = h.call(h.sym(names::ADD), &[h.sym_term(x), h.call(h.sym(names::MUL), &[i, h.sym_term(y)])]);
                let g = h.call(h.sym("rewrite"), &[*f, h.call(h.sym(names::RULE), &[z, zz])]);
                let comps = [h.call(h.sym("re"), &[g]), h.call(h.sym("im_part"), &[g])];
                (compile_field(cx, &comps, &[x, y])?, [p.0.min(q.0), p.0.max(q.0), p.1.min(q.1), p.1.max(q.1)])
            }
            (Kind::List, [d]) => {
                let range = data_range(cx, o)?;
                let Some((src, b)) = list_source(cx, *d, 2, range)? else { return Ok(bad(h, name, 1)) };
                (src, [b[0].0, b[0].1, b[1].0, b[1].1])
            }
            _ => return Ok(Reply::Decline),
        };
        if !(rect[1] > rect[0] && rect[3] > rect[2]) {
            return Ok(bad(h, name, 2));
        }
        let g = default_gradient(&look);
        let under = if self.stream { streamlines(cx, &mut src, rect, &g, look.plot_points)? } else { arrows(cx, &mut src, rect, &g, look.plot_points)? };
        if under.is_empty() {
            return Ok(bad(h, name, 1));
        }
        let extra = Extra { under, bounds: Some([(rect[0], rect[1]), (rect[2], rect[3])]), entries: vec![] };
        finish(cx, &[], &look, extra)
    }
}

/// Arrows on an n x n grid of cell centres, centred on their points, the longest a cell's size.
fn arrows<'h>(cx: &mut NativeCx<'_, '_, 'h>, src: &mut Source<'h>, [x0, x1, y0, y1]: [f64; 4], g: &Gradient, points: Option<usize>) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let n = points.unwrap_or(15).clamp(3, 60);
    let (dx, dy) = ((x1 - x0) / n as f64, (y1 - y0) / n as f64);
    let mut items = vec![];
    let (mut dx, mut dy) = (dx, dy);
    if let Source::Scattered(pts) = src {
        // data at points: the arrows where the data are, sized by the points' spacing
        items = pts.iter().filter(|(p, _)| p.len() == 2).map(|(p, v)| ([p[0], p[1]], v.clone())).collect();
        let spacing = ((x1 - x0) * (y1 - y0) / items.len().max(1) as f64).sqrt();
        (dx, dy) = (spacing, spacing);
    } else {
        for j in 0..n {
            cx.tick(1)?;
            for i in 0..n {
                let p = [x0 + dx * (i as f64 + 0.5), y0 + dy * (j as f64 + 0.5)];
                if let Some(v) = src.at(cx, &p)? {
                    items.push((p, v));
                }
            }
        }
    }
    let mags: Vec<f64> = items.iter().map(|(_, v)| norm(v)).collect();
    let hi = mags.iter().copied().fold(0.0f64, f64::max);
    let lo = mags.iter().copied().fold(f64::INFINITY, f64::min);
    if hi <= 0.0 || !hi.is_finite() {
        return Ok(vec![]);
    }
    let cell = dx.min(dy) * 0.9;
    let mut out = vec![h.call(h.sym("arrowheads"), &[h.real(0.018)])];
    for ((p, v), m) in items.iter().zip(&mags) {
        if *m <= 0.0 {
            continue;
        }
        // lengths between 30 % and 100 % of a cell, by magnitude
        let len = cell * (0.3 + 0.7 * (m / hi));
        let (ux, uy) = (v[0] / m * len / 2.0, v[1] / m * len / 2.0);
        let col = colour_of(g, (lo, hi), *m);
        out.push(list(h, &[mc_graphics::rgb_term(h, col), h.call(h.sym("arrow"), &[h.real_array(&[2, 2], &[p[0] - ux, p[1] - uy, p[0] + ux, p[1] + uy])])]));
    }
    Ok(out)
}

/// One streamline from `p` both ways: its points and its mean magnitude.
#[allow(clippy::too_many_arguments)]
fn trace<'h>(cx: &mut NativeCx<'_, '_, 'h>, src: &mut Source<'h>, p: &[f64], lo: &[f64], hi: &[f64], ds: f64, occupied: &dyn Fn(&[f64]) -> bool, max: usize) -> Res<(Vec<Vec<f64>>, f64)> {
    let d = p.len();
    let mut halves = vec![];
    let mut msum = 0.0;
    let mut mcount = 0usize;
    for dir in [1.0, -1.0] {
        let mut pts = vec![p.to_vec()];
        let mut cur = p.to_vec();
        for step in 0..max {
            if step % 64 == 0 {
                cx.tick(1)?;
            }
            // RK4 on the unit direction
            let mut unit = |cx: &mut NativeCx<'_, '_, 'h>, q: &[f64]| -> Res<Option<(Vec<f64>, f64)>> {
                let Some(v) = src.at(cx, q)? else { return Ok(None) };
                let m = norm(&v);
                Ok((m > 1e-300).then(|| (v.iter().map(|c| dir * c / m).collect(), m)))
            };
            let Some((k1, m)) = unit(cx, &cur)? else { break };
            msum += m;
            mcount += 1;
            let add = |a: &[f64], b: &[f64], s: f64| a.iter().zip(b).map(|(x, y)| x + s * y).collect::<Vec<f64>>();
            let Some((k2, _)) = unit(cx, &add(&cur, &k1, ds / 2.0))? else { break };
            let Some((k3, _)) = unit(cx, &add(&cur, &k2, ds / 2.0))? else { break };
            let Some((k4, _)) = unit(cx, &add(&cur, &k3, ds))? else { break };
            let next: Vec<f64> = (0..d).map(|k| cur[k] + ds / 6.0 * (k1[k] + 2.0 * k2[k] + 2.0 * k3[k] + k4[k])).collect();
            if (0..d).any(|k| next[k] < lo[k] || next[k] > hi[k]) || occupied(&next) {
                break;
            }
            // closed on itself
            if step > 20 && norm(&add(&next, p, -1.0)) < ds * 0.75 {
                pts.push(next);
                break;
            }
            pts.push(next.clone());
            cur = next;
        }
        halves.push(pts);
    }
    let mut back = halves.pop().unwrap_or_default();
    back.reverse();
    let fwd = halves.pop().unwrap_or_default();
    back.pop();
    back.extend(fwd);
    Ok((back, if mcount > 0 { msum / mcount as f64 } else { 0.0 }))
}

/// Streamlines over the rectangle, from seeds on an n x n grid, kept apart by an occupancy grid.
fn streamlines<'h>(cx: &mut NativeCx<'_, '_, 'h>, src: &mut Source<'h>, [x0, x1, y0, y1]: [f64; 4], g: &Gradient, points: Option<usize>) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let n = points.unwrap_or(12).clamp(3, 40);
    let occ_n = 3 * n;
    let mut occ = vec![false; occ_n * occ_n];
    let cell_of = |p: &[f64]| -> usize {
        let i = (((p[0] - x0) / (x1 - x0)) * occ_n as f64).floor().clamp(0.0, (occ_n - 1) as f64) as usize;
        let j = (((p[1] - y0) / (y1 - y0)) * occ_n as f64).floor().clamp(0.0, (occ_n - 1) as f64) as usize;
        j * occ_n + i
    };
    let diag = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
    let ds = diag / 250.0;
    let mut lines: Vec<(Vec<Vec<f64>>, f64)> = vec![];
    // seeds from the middle outwards, so the central lines are drawn first
    let mut seeds = vec![];
    for j in 0..n {
        for i in 0..n {
            seeds.push([x0 + (x1 - x0) * (i as f64 + 0.5) / n as f64, y0 + (y1 - y0) * (j as f64 + 0.5) / n as f64]);
        }
    }
    for s in seeds {
        if occ[cell_of(&s)] {
            continue;
        }
        let snapshot = occ.clone();
        let own = cell_of(&s);
        let occupied = |p: &[f64]| {
            let c = cell_of(p);
            c != own && snapshot[c]
        };
        let (pts, m) = trace(cx, src, &s, &[x0, y0], &[x1, y1], ds, &occupied, 1500)?;
        if pts.len() < 6 {
            continue;
        }
        for p in &pts {
            occ[cell_of(p)] = true;
        }
        lines.push((pts, m));
    }
    let mags: Vec<f64> = lines.iter().map(|l| l.1).collect();
    let lo = mags.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = mags.iter().copied().fold(0.0f64, f64::max);
    let mut out = vec![h.call(h.sym("arrowheads"), &[h.real(0.022)]), h.call(h.sym("absolute_thickness"), &[h.real(1.1)])];
    for (pts, m) in &lines {
        let col = mc_graphics::rgb_term(h, colour_of(g, (lo, hi), *m));
        let flat: Vec<f64> = pts.iter().flat_map(|p| [p[0], p[1]]).collect();
        let mid = pts.len() / 2;
        let arrow = h.call(h.sym("arrow"), &[h.real_array(&[2, 2], &[pts[mid - 1][0], pts[mid - 1][1], pts[mid][0], pts[mid][1]])]);
        out.push(list(h, &[col, h.call(h.sym("line"), &[h.real_array(&[pts.len(), 2], &flat)]), arrow]));
    }
    Ok(out)
}

/// `vector_plot3d([vx, vy, vz], [x, ...], [y, ...], [z, ...])`, `stream_plot3d`, and their list forms.
struct Field3 {
    name: &'static str,
    stream: bool,
    list: bool,
}

impl Native for Field3 {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = self.name;
        let (a, o) = split(h, args);
        let (mut src, b) = match (self.list, a.as_slice()) {
            (false, [f, sx, sy, sz]) => {
                let mut vars = vec![];
                let mut b = vec![];
                for (k, s) in [sx, sy, sz].into_iter().enumerate() {
                    let Some((v, lo, hi)) = spec(cx, *s)? else { return Ok(bad(h, name, k as i64 + 2)) };
                    vars.push(v);
                    b.push((lo, hi));
                }
                let Some(comps) = cx.as_list(*f).filter(|l| l.len() == 3).map(|l| l.to_vec()) else { return Ok(bad(h, name, 1)) };
                (compile_field(cx, &comps, &vars)?, b)
            }
            (true, [d]) => {
                let range = data_range(cx, o)?;
                let Some(r) = list_source(cx, *d, 3, range)? else { return Ok(bad(h, name, 1)) };
                r
            }
            _ => return Ok(Reply::Decline),
        };
        let g = Gradient::default_gradient();
        let lo: Vec<f64> = b.iter().map(|x| x.0).collect();
        let hi: Vec<f64> = b.iter().map(|x| x.1).collect();
        let mut prims = vec![];
        if self.stream {
            let n: usize = 5;
            let diag = norm(&hi.iter().zip(&lo).map(|(a, b)| a - b).collect::<Vec<_>>());
            let mut lines = vec![];
            for k in 0..n * n * n {
                let s: Vec<f64> = (0..3).map(|d| lo[d] + (hi[d] - lo[d]) * (((k / n.pow(d as u32)) % n) as f64 + 0.5) / n as f64).collect();
                let (pts, m) = trace(cx, &mut src, &s, &lo, &hi, diag / 200.0, &|_| false, 400)?;
                if pts.len() >= 4 {
                    lines.push((pts, m));
                }
            }
            let mlo = lines.iter().map(|l| l.1).fold(f64::INFINITY, f64::min);
            let mhi = lines.iter().map(|l| l.1).fold(0.0f64, f64::max);
            for (pts, m) in lines {
                let flat: Vec<f64> = pts.iter().flatten().copied().collect();
                prims.push(list(h, &[mc_graphics::rgb_term(h, colour_of(&g, (mlo, mhi), m)), h.call(h.sym("line"), &[h.real_array(&[pts.len(), 3], &flat)])]));
            }
        } else {
            let n: usize = 7;
            let mut items = vec![];
            for k in 0..n * n * n {
                if k % 64 == 0 {
                    cx.tick(1)?;
                }
                let p: Vec<f64> = (0..3).map(|d| lo[d] + (hi[d] - lo[d]) * (((k / n.pow(d as u32)) % n) as f64 + 0.5) / n as f64).collect();
                if let Some(v) = src.at(cx, &p)? {
                    items.push((p, v));
                }
            }
            let mags: Vec<f64> = items.iter().map(|(_, v)| norm(v)).collect();
            let mhi = mags.iter().copied().fold(0.0f64, f64::max);
            let mlo = mags.iter().copied().fold(f64::INFINITY, f64::min);
            let cell = (0..3).map(|d| (hi[d] - lo[d]) / n as f64).fold(f64::INFINITY, f64::min) * 0.9;
            for ((p, v), m) in items.iter().zip(&mags) {
                if *m <= 0.0 || mhi <= 0.0 {
                    continue;
                }
                let len = cell * (0.3 + 0.7 * m / mhi) / 2.0;
                let u: Vec<f64> = v.iter().map(|c| c / m * len).collect();
                let ends = [p[0] - u[0], p[1] - u[1], p[2] - u[2], p[0] + u[0], p[1] + u[1], p[2] + u[2]];
                prims.push(list(h, &[mc_graphics::rgb_term(h, colour_of(&g, (mlo, mhi), *m)), h.call(h.sym("arrow"), &[h.real_array(&[2, 3], &ends)])]));
            }
        }
        if prims.is_empty() {
            return Ok(bad(h, name, 1));
        }
        let rule = |k: &str, v: T<'h>| h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v]);
        let opts = vec![rule("axes", h.sym_term(h.sym("true"))), rule("plot_range", list(h, &b.iter().map(|&(a, c)| h.reals(&[a, c])).collect::<Vec<_>>()))];
        let (passed, ignored) = pass_options(cx, o);
        let mut all_opts = opts;
        all_opts.extend(passed);
        let g3 = h.call(h.sym("graphics3d"), &[list(h, &prims), h.call(h.sym("options"), &all_opts)]);
        finish_graphic(cx, g3, name, &ignored)
    }
}

/// The options of a 3D picture that the renderer knows, passed on; the names of the others.
pub(crate) fn pass_options<'h>(cx: &NativeCx<'_, '_, 'h>, o: Option<T<'h>>) -> (Vec<T<'h>>, Vec<String>) {
    let h = cx.heap();
    let mut keep = vec![];
    let mut ignored = vec![];
    if let Some(o) = o {
        if let View::App { args: rs, .. } = h.view(o) {
            for &r in rs {
                if let View::App { args: &[k, _], .. } = h.view(r) {
                    if let View::Sym(s) = h.view(k) {
                        let key = h.sym_name(s);
                        if mc_graphics::three::OPTIONS.contains(&key.as_str()) || mc_graphics::model::known_option(&key) {
                            keep.push(r);
                        } else {
                            ignored.push(key);
                        }
                    }
                }
            }
        }
    }
    (keep, ignored)
}
