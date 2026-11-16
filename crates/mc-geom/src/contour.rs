//! Meshes of implicit regions (`discretize_region` of `implicit_region`): a region of the plane given by
//! inequalities becomes a triangle mesh, a curve of the plane (one equation) a line mesh, a surface of space (one
//! equation) a triangle mesh and a curve of space (two equations) a line mesh.
//!
//! Method: the box of the region (the given ranges, else its numeric bounds) is cut into a grid of squares, each
//! split into two triangles, or of cubes, each split into six tetrahedra around the main diagonal (Freudenthal's
//! subdivision, so neighbouring cells share faces). The condition's value function is sampled at the corners. In a
//! triangle the part where the value is at least 0 (or the segment where it changes sign) is cut out at the edge
//! crossings; in a tetrahedron the zero set is one triangle or a quadrilateral through the crossings (marching
//! tetrahedra; Doi and Koide, IEICE Trans. 1991). Each crossing on a grid edge is found by bisection on the function
//! itself and shared by the cells around that edge, so the mesh is conforming. For a curve of space the second
//! function is taken along the surface of the first: its sign changes on the surface's edges give the segments.
//!
//! The mesh approximates the region to the grid's resolution (100 squares or 36 cubes across); it is a picture of
//! the region, not a measurement of it.
use crate::ex::E;
use crate::region::{call_parts, Region};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Res};
use mc_numfn::NumFn;
use std::collections::HashMap;

/// What the condition describes.
enum Form<'h> {
    /// the points where the function is at least 0
    Solid(T<'h>),
    /// the zero set of each function
    Zeros(Vec<T<'h>>),
}

fn has_equation<'h>(h: &'h Heap, t: T<'h>) -> bool {
    let mut stack = mc_api::walk::Walk::new(t);
    let mut budget = 100_000usize;
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "equal") {
                return true;
            }
            if budget < args.len() {
                return true;
            }
            budget -= args.len() + 1;
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

fn form<'h>(h: &'h Heap, cond: T<'h>) -> Option<Form<'h>> {
    let e = E { h };
    let eq = |t: T<'h>| match call_parts(h, t) {
        Some((n, &[a, b])) if n == "equal" => Some(e.sub(a, b)),
        _ => None,
    };
    if let Some(f) = eq(cond) {
        return Some(Form::Zeros(vec![f]));
    }
    if let Some((n, parts)) = call_parts(h, cond) {
        if n == "and" && parts.len() == 2 {
            if let (Some(f), Some(g)) = (eq(parts[0]), eq(parts[1])) {
                return Some(Form::Zeros(vec![f, g]));
            }
        }
    }
    if has_equation(h, cond) {
        return None;
    }
    crate::margin(h, cond).map(Form::Solid)
}

/// The mesh of an implicit region in the box `bounds`, or `None` for conditions of other forms.
pub fn mesh<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, ranges: Option<Vec<(f64, f64)>>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Region::Implicit { cond, vars } = r else { return Ok(None) };
    let n = vars.len();
    if !(2..=3).contains(&n) {
        return Ok(None);
    }
    let Some(vs) = vars.iter().map(|&v| match h.view(v) {
        View::Sym(s) => Some(s),
        _ => None,
    }).collect::<Option<Vec<Sym>>>() else {
        return Ok(None);
    };
    let Some(form) = form(h, *cond) else { return Ok(None) };
    let bounds = match ranges {
        Some(b) => b,
        None => match crate::natives::implicit_bounds(cx, r)? {
            Some(b) => {
                // a margin so the boundary lies inside the grid
                b.iter().map(|&(lo, hi)| {
                    let pad = 0.02 * (hi - lo) + 1e-9 * (1.0 + lo.abs().max(hi.abs()));
                    (lo - pad, hi + pad)
                }).collect()
            }
            None => return Ok(None),
        },
    };
    if bounds.len() != n || bounds.iter().any(|b| !b.0.is_finite() || !b.1.is_finite() || b.1 <= b.0) {
        return Ok(None);
    }
    match (n, form) {
        (2, Form::Solid(f)) => plane(cx, f, &vs, &bounds, false),
        (2, Form::Zeros(fs)) if fs.len() == 1 => plane(cx, fs[0], &vs, &bounds, true),
        (3, Form::Zeros(fs)) => space(cx, &fs, &vs, &bounds),
        _ => Ok(None),
    }
}

type Fun<'h> = Box<dyn NumFn<'h, f64> + 'h>;

/// The value of a compiled function at a point (NaN where it has none).
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut Fun<'h>, p: &[f64]) -> Res<f64> {
    let mut out = [0.0];
    Ok(if f.eval(cx, p, &mut out)? && out[0].is_finite() { out[0] } else { f64::NAN })
}

/// Whether a sampled value counts as inside (at least 0; a value of exactly 0 counts as inside, so a zero set is
/// crossed between an inside and an outside corner).
fn inside(v: f64) -> bool {
    v >= 0.0
}

/// The crossing between p (inside) and q (outside), by bisection on f.
fn crossing<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut Fun<'h>, p: &[f64], q: &[f64]) -> Res<Vec<f64>> {
    let (mut a, mut b) = (0.0f64, 1.0f64);
    let at = |t: f64| -> Vec<f64> { p.iter().zip(q).map(|(x, y)| x + (y - x) * t).collect() };
    for _ in 0..40 {
        let m = 0.5 * (a + b);
        let v = value(cx, f, &at(m))?;
        if v.is_nan() {
            break;
        }
        if inside(v) {
            a = m;
        } else {
            b = m;
        }
    }
    Ok(at(0.5 * (a + b)))
}

/// Points of the mesh, each made once for its key.
struct Points {
    pts: Vec<Vec<f64>>,
    keys: HashMap<(usize, usize), usize>,
}

impl Points {
    fn new() -> Points {
        Points { pts: vec![], keys: HashMap::new() }
    }
    /// The grid node k (key (k, k)) or the crossing on the grid edge between nodes a and b.
    fn get(&mut self, key: (usize, usize), make: impl FnOnce() -> Res<Vec<f64>>) -> Res<usize> {
        let key = (key.0.min(key.1), key.0.max(key.1));
        if let Some(&i) = self.keys.get(&key) {
            return Ok(i);
        }
        let p = make()?;
        self.pts.push(p);
        self.keys.insert(key, self.pts.len() - 1);
        Ok(self.pts.len() - 1)
    }
}

fn mesh_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, pts: &[Vec<f64>], head: &str, cells: &[Vec<usize>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if cells.is_empty() {
        return Ok(None);
    }
    let p: Vec<T<'h>> = pts.iter().map(|q| h.reals(q)).collect();
    let c: Vec<T<'h>> = cells.iter().map(|c| cx.list(&c.iter().map(|&i| h.int(i as i64 + 1)).collect::<Vec<_>>())).collect();
    let cell = h.call(h.sym(head), &[cx.list(&c)]);
    let t = h.call(h.sym("mesh_region"), &[cx.list(&p), cx.list(&[cell])]);
    Ok(Some(cx.eval(t)?))
}

/// The plane: triangles of the part where f >= 0, or segments of f = 0 (`curve`).
fn plane<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, vs: &[Sym], b: &[(f64, f64)], curve: bool) -> Res<Option<T<'h>>> {
    const N: usize = 100;
    let opts = mc_numfn::Options::default();
    let mut fun: Fun<'h> = mc_numfn::compile_expr(cx, f, &vs.iter().map(|&s| mc_numfn::Var::scalar(s)).collect::<Vec<_>>(), &opts)?;
    let node = |i: usize, j: usize| -> Vec<f64> { vec![b[0].0 + (b[0].1 - b[0].0) * i as f64 / N as f64, b[1].0 + (b[1].1 - b[1].0) * j as f64 / N as f64] };
    let id = |i: usize, j: usize| i * (N + 1) + j;
    let mut vals = vec![f64::NAN; (N + 1) * (N + 1)];
    for i in 0..=N {
        cx.tick(N as u64 + 1)?;
        for j in 0..=N {
            vals[id(i, j)] = value(cx, &mut fun, &node(i, j))?;
        }
    }
    let mut pts = Points::new();
    let mut cells: Vec<Vec<usize>> = vec![];
    for i in 0..N {
        cx.tick(N as u64)?;
        for j in 0..N {
            for tri in [[(i, j), (i + 1, j), (i + 1, j + 1)], [(i, j), (i + 1, j + 1), (i, j + 1)]] {
                let k: Vec<usize> = tri.iter().map(|&(a, c)| id(a, c)).collect();
                let v: Vec<f64> = k.iter().map(|&x| vals[x]).collect();
                if v.iter().any(|x| x.is_nan()) {
                    continue;
                }
                let ins: Vec<bool> = v.iter().map(|&x| inside(x)).collect();
                if curve {
                    // the segment between the two crossings
                    let mut ends = vec![];
                    for e in 0..3 {
                        let (a, c) = (e, (e + 1) % 3);
                        if ins[a] != ins[c] {
                            let (p, q) = if ins[a] { (tri[a], tri[c]) } else { (tri[c], tri[a]) };
                            let idx = pts.get((k[a], k[c]), || crossing(cx, &mut fun, &node(p.0, p.1), &node(q.0, q.1)))?;
                            ends.push(idx);
                        }
                    }
                    if ends.len() == 2 && ends[0] != ends[1] {
                        cells.push(ends);
                    }
                    continue;
                }
                // the corners inside and the crossings, in order round the triangle
                let mut poly = vec![];
                for e in 0..3 {
                    let c = (e + 1) % 3;
                    if ins[e] {
                        let (x, y) = tri[e];
                        poly.push(pts.get((k[e], k[e]), || Ok(node(x, y)))?);
                    }
                    if ins[e] != ins[c] {
                        let (p, q) = if ins[e] { (tri[e], tri[c]) } else { (tri[c], tri[e]) };
                        poly.push(pts.get((k[e], k[c]), || crossing(cx, &mut fun, &node(p.0, p.1), &node(q.0, q.1)))?);
                    }
                }
                for t in 1..poly.len().saturating_sub(1) {
                    cells.push(vec![poly[0], poly[t], poly[t + 1]]);
                }
            }
        }
    }
    mesh_term(cx, &pts.pts, if curve { "line" } else { "polygon" }, &cells)
}

/// Space: the triangles of the surface f = 0, or the segments of f = g = 0.
fn space<'h>(cx: &mut NativeCx<'_, '_, 'h>, fs: &[T<'h>], vs: &[Sym], b: &[(f64, f64)]) -> Res<Option<T<'h>>> {
    const N: usize = 36;
    let opts = mc_numfn::Options::default();
    let vars: Vec<mc_numfn::Var> = vs.iter().map(|&s| mc_numfn::Var::scalar(s)).collect();
    let mut fun: Fun<'h> = mc_numfn::compile_expr(cx, fs[0], &vars, &opts)?;
    let mut second: Option<Fun<'h>> = match fs.get(1) {
        Some(&g) => Some(mc_numfn::compile_expr(cx, g, &vars, &opts)?),
        None => None,
    };
    let m = N + 1;
    let id = |i: usize, j: usize, k: usize| (i * m + j) * m + k;
    let node_of = |n: usize| -> Vec<f64> {
        let (i, j, k) = (n / (m * m), (n / m) % m, n % m);
        vec![
            b[0].0 + (b[0].1 - b[0].0) * i as f64 / N as f64,
            b[1].0 + (b[1].1 - b[1].0) * j as f64 / N as f64,
            b[2].0 + (b[2].1 - b[2].0) * k as f64 / N as f64,
        ]
    };
    let mut vals = vec![f64::NAN; m * m * m];
    for (n, v) in vals.iter_mut().enumerate() {
        if n % m == 0 {
            cx.tick(m as u64)?;
        }
        *v = value(cx, &mut fun, &node_of(n))?;
    }
    // the six tetrahedra of a cube around its diagonal from corner 0 to corner 7 (corner bits: x 4, y 2, z 1)
    const TETS: [[usize; 4]; 6] = [[0, 4, 6, 7], [0, 4, 5, 7], [0, 1, 5, 7], [0, 1, 3, 7], [0, 2, 3, 7], [0, 2, 6, 7]];
    let mut pts = Points::new();
    let mut tris: Vec<[usize; 3]> = vec![];
    for i in 0..N {
        cx.tick((N * N) as u64)?;
        for j in 0..N {
            for k in 0..N {
                let corner = |c: usize| id(i + (c >> 2 & 1), j + (c >> 1 & 1), k + (c & 1));
                for t in TETS {
                    let g: Vec<usize> = t.iter().map(|&c| corner(c)).collect();
                    let v: Vec<f64> = g.iter().map(|&x| vals[x]).collect();
                    if v.iter().any(|x| x.is_nan()) {
                        continue;
                    }
                    let ins: Vec<bool> = v.iter().map(|&x| inside(x)).collect();
                    let count = ins.iter().filter(|&&x| x).count();
                    if count == 0 || count == 4 {
                        continue;
                    }
                    let mut cut = |a: usize, c: usize, pts: &mut Points, cx: &mut NativeCx<'_, '_, 'h>| -> Res<usize> {
                        let (p, q) = if ins[a] { (g[a], g[c]) } else { (g[c], g[a]) };
                        pts.get((g[a], g[c]), || crossing(cx, &mut fun, &node_of(p), &node_of(q)))
                    };
                    if count == 1 || count == 3 {
                        let lone = (0..4).find(|&x| ins[x] == (count == 1)).expect("one corner");
                        let others: Vec<usize> = (0..4).filter(|&x| x != lone).collect();
                        let a = cut(lone, others[0], &mut pts, cx)?;
                        let c = cut(lone, others[1], &mut pts, cx)?;
                        let d = cut(lone, others[2], &mut pts, cx)?;
                        tris.push([a, c, d]);
                    } else {
                        let ins_c: Vec<usize> = (0..4).filter(|&x| ins[x]).collect();
                        let out_c: Vec<usize> = (0..4).filter(|&x| !ins[x]).collect();
                        // the quadrilateral through the four crossings, in order round it
                        let p0 = cut(ins_c[0], out_c[0], &mut pts, cx)?;
                        let p1 = cut(ins_c[0], out_c[1], &mut pts, cx)?;
                        let p2 = cut(ins_c[1], out_c[1], &mut pts, cx)?;
                        let p3 = cut(ins_c[1], out_c[0], &mut pts, cx)?;
                        tris.push([p0, p1, p2]);
                        tris.push([p0, p2, p3]);
                    }
                }
            }
        }
    }
    let Some(g) = second.as_mut() else {
        let cells: Vec<Vec<usize>> = tris.iter().filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).map(|t| t.to_vec()).collect();
        return mesh_term(cx, &pts.pts, "polygon", &cells);
    };
    // the curve: the sign changes of g along the surface's edges
    let mut gv = Vec::with_capacity(pts.pts.len());
    for p in &pts.pts {
        gv.push(value(cx, g, p)?);
    }
    let mut out = Points::new();
    let mut segs: Vec<Vec<usize>> = vec![];
    for (n, t) in tris.iter().enumerate() {
        if n % 1024 == 0 {
            cx.tick(1024)?;
        }
        if t.iter().any(|&x| gv[x].is_nan()) {
            continue;
        }
        let mut ends = vec![];
        for e in 0..3 {
            let (a, c) = (t[e], t[(e + 1) % 3]);
            if a != c && inside(gv[a]) != inside(gv[c]) {
                let (p, q) = if inside(gv[a]) { (a, c) } else { (c, a) };
                let (pp, qq) = (pts.pts[p].clone(), pts.pts[q].clone());
                let s = gv[p] / (gv[p] - gv[q]);
                ends.push(out.get((a, c), || Ok(pp.iter().zip(&qq).map(|(x, y)| x + (y - x) * s).collect()))?);
            }
        }
        if ends.len() == 2 && ends[0] != ends[1] {
            segs.push(ends);
        }
    }
    // each segment once (a surface edge is shared by two triangles)
    let mut seen = std::collections::HashSet::new();
    segs.retain(|s| seen.insert((s[0].min(s[1]), s[0].max(s[1]))));
    mesh_term(cx, &out.pts, "line", &segs)
}
