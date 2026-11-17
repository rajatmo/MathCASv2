//! Membership of a symbolic point as a condition on its coordinates (a semialgebraic description of the region).
//!
//! - **Flat convex pieces** (simplices of any dimension, convex polygons in the plane or in space, convex
//!   polyhedra, flats with half-line or half-plane directions): the half-space (H-) representation. The affine
//!   hull gives equations, one per vector of the null space of the edge directions; inside the hull the facets give
//!   one inequality each. For a simplex with edge matrix `D` (rows `v_j - v_0`) the barycentric coordinates are
//!   `lambda = (D D^T)^-1 D (x - v_0)` and the conditions are `lambda_j >= 0` and `sum lambda_j <= 1`; a convex
//!   polygon or polyhedron has one inequality per edge or face, its normal turned towards the centroid (the
//!   region is checked to be convex: every corner on the inner side of every facet, otherwise the call declines).
//!   See Ziegler, Lectures on Polytopes (1995), lecture 1, for the two descriptions of a polytope.
//! - **Polylines**: the union of their segments.
//! - **Cylinders and cones** about the axis `a -> b` (`d = b - a`, `s = d.(x - a)`): `0 <= s <= |d|^2` and
//!   `|d|^2 |x - a|^2 - s^2 <= r^2 |d|^2` (cylinder) or `|d|^2 (|d|^2 |x - a|^2 - s^2) <= r^2 (|d|^2 - s)^2` (cone,
//!   apex at `b`).
//! - **Tori** with inner radius `r0` and outer radius `r1` about the vertical axis through `c`:
//!   `r0 r1 + |x - c|^2 <= (r0 + r1) sqrt((x - c_1)^2 + (y - c_2)^2)` (`==` on the surface), the square of the
//!   distance to the central circle of radius `(r0 + r1)/2` against the tube radius `(r1 - r0)/2` multiplied out;
//!   with symbolic radii it holds under `0 < r0 < r1`.
//!
//! Linear conditions are written as `crate::flat::linear_condition` writes them; polynomial ones with integer
//! coefficients of content 1, the positive terms on the left and the others on the right.
use crate::ex::{lit, num, E};
use crate::region::{call_parts, Region};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Res};

type Pt<'h> = Vec<T<'h>>;

/// The membership condition of the symbolic point `x` (as many coordinates as the region's embedding), or `None`.
pub fn member<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, x: &[T<'h>]) -> Res<Option<T<'h>>> {
    let e = E { h: cx.heap() };
    Ok(match r {
        Region::Simplex(v) => simplex(cx, v, x)?,
        Region::Lines(ls) => {
            let mut parts = vec![];
            for l in ls {
                for w in l.windows(2) {
                    match simplex(cx, w, x)? {
                        Some(c) => parts.push(c),
                        None => return Ok(None),
                    }
                }
            }
            match parts.len() {
                0 => None,
                1 => Some(parts[0]),
                _ => Some(e.f("or", &parts)),
            }
        }
        Region::Polygon(rings) if rings.len() == 1 && rings[0].holes.is_empty() => {
            let v = &rings[0].outer;
            if v.len() == 3 { simplex(cx, v, x)? } else { convex_polygon(cx, v, x)? }
        }
        Region::Polyhedron { verts, faces } => polyhedron(cx, verts, faces, x)?,
        Region::Affine { p, free, pos } => affine(cx, p, free, pos, x)?,
        Region::Revolved { a, b, r, cone } => revolved(cx, a, b, *r, *cone, x)?,
        Region::Torus { c, r0, r1, solid } => torus(cx, c, *r0, *r1, *solid, x)?,
        _ => None,
    })
}

/// A matrix term from rows.
fn matrix<'h>(e: E<'h>, rows: &[Pt<'h>]) -> T<'h> {
    e.list(&rows.iter().map(|r| e.list(r)).collect::<Vec<_>>())
}

/// The rows of an evaluated matrix term.
fn rows_of<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<Pt<'h>>> {
    cx.as_list(t)?.iter().map(|&r| cx.as_list(r).map(|l| l.to_vec())).collect()
}

/// The rank of the vectors `rows` (exact), or `None` when it cannot be decided.
fn rank<'h>(cx: &mut NativeCx<'_, '_, 'h>, rows: &[Pt<'h>]) -> Res<Option<usize>> {
    let e = E { h: cx.heap() };
    let r = cx.eval(e.f("rank", &[matrix(e, rows)]))?;
    Ok(match cx.heap().view(r) {
        View::Int(k) if k >= 0 => Some(k as usize),
        _ => None,
    })
}

/// Vectors spanning the orthogonal complement of `rows` in `n` dimensions (the normals of their span).
fn normals<'h>(cx: &mut NativeCx<'_, '_, 'h>, rows: &[Pt<'h>], n: usize) -> Res<Option<Vec<Pt<'h>>>> {
    let e = E { h: cx.heap() };
    if rows.is_empty() {
        return Ok(Some((0..n).map(|i| (0..n).map(|j| e.int(i64::from(i == j))).collect()).collect()));
    }
    let ns = cx.eval(e.f("null_space", &[matrix(e, rows)]))?;
    let Some(ns) = rows_of(cx, ns) else { return Ok(None) };
    Ok(ns.iter().all(|v| v.len() == n).then_some(ns))
}

/// The coordinate functionals of the basis `rows` on its span: the rows of `(D D^T)^-1 D`.
fn duals<'h>(cx: &mut NativeCx<'_, '_, 'h>, rows: &[Pt<'h>]) -> Res<Option<Vec<Pt<'h>>>> {
    // on the span the coordinates are fixed by k of the n coordinates: the first set of k coordinates (in
    // lexicographic order) on which the vectors stay independent; then lambda = (D_S^T)^-1 (x - p)_S
    let e = E { h: cx.heap() };
    let (k, n) = (rows.len(), rows.first().map_or(0, |r| r.len()));
    let Some(s) = coordinate_set(cx, rows, k, n)? else { return Ok(None) };
    let ds: Vec<Pt<'h>> = rows.iter().map(|r| s.iter().map(|&j| r[j]).collect()).collect();
    let m = cx.eval(e.f("simplify", &[e.f("inverse", &[e.f("transpose", &[matrix(e, &ds)])])]))?;
    let Some(m) = rows_of(cx, m) else { return Ok(None) };
    if m.len() != k || m.iter().any(|r| r.len() != k) {
        return Ok(None);
    }
    Ok(Some(
        m.iter()
            .map(|r| {
                let mut full = vec![e.int(0); n];
                for (i, &j) in s.iter().enumerate() {
                    full[j] = r[i];
                }
                full
            })
            .collect(),
    ))
}

/// The first `k` of `n` coordinates (lexicographic order) on which the `k` vectors `rows` are independent.
fn coordinate_set<'h>(cx: &mut NativeCx<'_, '_, 'h>, rows: &[Pt<'h>], k: usize, n: usize) -> Res<Option<Vec<usize>>> {
    let e = E { h: cx.heap() };
    let mut s: Vec<usize> = (0..k).collect();
    loop {
        let sub: Vec<Pt<'h>> = rows.iter().map(|r| s.iter().map(|&j| r[j]).collect()).collect();
        let det = cx.eval(e.f("simplify", &[e.f("det", &[matrix(e, &sub)])]))?;
        if lit(cx.heap(), det) != Some(0.0) {
            return Ok(Some(s));
        }
        // the next combination
        let mut i = k;
        loop {
            if i == 0 {
                return Ok(None);
            }
            i -= 1;
            if s[i] < n - k + i {
                break;
            }
        }
        s[i] += 1;
        for j in i + 1..k {
            s[j] = s[j - 1] + 1;
        }
    }
}


/// `nv.(x - q) rel 0` (`rel` `equal` or `greater_equal`) as a linear relation: the form of
/// `crate::flat::linear_condition` for numeric coefficients, a polynomial relation otherwise.
fn linear<'h>(cx: &mut NativeCx<'_, '_, 'h>, nv: &[T<'h>], q: &[T<'h>], x: &[T<'h>], rel: &str) -> Res<Option<T<'h>>> {
    if let Some(c) = crate::flat::linear_condition(cx, nv, q, x, rel)? {
        return Ok(Some(c));
    }
    let e = E { h: cx.heap() };
    let lhs = e.dot(nv, &e.vsub(x, q));
    match rel {
        "equal" => polynomial(cx, lhs, "equal"),
        // nv.(x - q) >= 0 is -nv.(x - q) <= 0
        _ => polynomial(cx, e.neg(lhs), "less_equal"),
    }
}

/// The numeric coefficient of one term of an expanded sum (1 when it has none).
fn coefficient<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> (T<'h>, f64) {
    let h = cx.heap();
    if let Some(v) = lit(h, t) {
        return (t, v);
    }
    if let Some((name, args)) = call_parts(h, t) {
        if name == mc_api::names::MUL {
            if let Some(v) = args.first().and_then(|&c| lit(h, c)) {
                return (args[0], v);
            }
        }
    }
    (h.int(1), 1.0)
}

/// `p rel 0` (`rel` `less_equal` or `equal`) for a polynomial `p` in the coordinates: expanded, divided by the
/// content of its rational coefficients, the terms with positive coefficients on the left and the others, negated,
/// on the right. `None` when `p` has a coefficient that is not a rational number.
fn polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: T<'h>, rel: &str) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    // over one denominator: a numeric denominator only scales (its sign turns an inequality); a symbolic one is
    // taken as nonzero, which is enough for an equation only
    let t = cx.eval(e.f("together", &[p]))?;
    let den = cx.eval(e.f("denominator", &[t]))?;
    let p = match num(cx, den)? {
        Some(v) if v > 0.0 => cx.eval(e.f("numerator", &[t]))?,
        Some(v) if v < 0.0 => cx.eval(e.neg(e.f("numerator", &[t])))?,
        Some(_) => return Ok(None),
        None if rel == "equal" => cx.eval(e.f("numerator", &[t]))?,
        None => return Ok(None),
    };
    let d = cx.eval(e.f("expand", &[p]))?;
    let terms: Vec<T<'h>> = match call_parts(h, d) {
        Some((n, a)) if n == mc_api::names::ADD => a.to_vec(),
        _ => vec![d],
    };
    if terms.len() == 1 && lit(h, terms[0]).is_some() {
        // a constant: the relation decides itself
        return Ok(Some(cx.eval(e.f(rel, &[d, e.int(0)]))?));
    }
    let coefs: Vec<T<'h>> = terms.iter().map(|&t| coefficient(cx, t).0).collect();
    let content = cx.eval(e.f("gcd", &coefs))?;
    let Some(cv) = lit(h, content).filter(|v| *v > 0.0) else { return Ok(None) };
    let _ = cv;
    let (mut left, mut right) = (vec![], vec![]);
    for &t in &terms {
        let (_, v) = coefficient(cx, t);
        let s = cx.eval(e.div(t, content))?;
        if v > 0.0 {
            left.push(s);
        } else {
            right.push(cx.eval(e.neg(s))?);
        }
    }
    Ok(Some(e.f(rel, &[e.add(&left), e.add(&right)])))
}

/// The conjunction of conditions (one condition as itself).
fn all<'h>(e: E<'h>, parts: Vec<T<'h>>) -> T<'h> {
    if parts.len() == 1 { parts[0] } else { e.f("and", &parts) }
}

/// The closed simplex spanned by the corners `v` (any number up to the dimension plus one).
fn simplex<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: &[Pt<'h>], x: &[T<'h>]) -> Res<Option<T<'h>>> {
    let e = E { h: cx.heap() };
    let n = x.len();
    if v.is_empty() || v.iter().any(|p| p.len() != n) {
        return Ok(None);
    }
    if v.len() == 1 {
        return Ok(Some(all(e, (0..n).map(|k| e.f("equal", &[x[k], v[0][k]])).collect())));
    }
    let d: Vec<Pt<'h>> = v[1..].iter().map(|p| e.vsub(p, &v[0])).collect();
    if rank(cx, &d)? != Some(d.len()) {
        return Ok(None);
    }
    let mut parts = vec![];
    if d.len() < n {
        let Some(ns) = normals(cx, &d, n)? else { return Ok(None) };
        for nv in &ns {
            match linear(cx, nv, &v[0], x, "equal")? {
                Some(c) => parts.push(c),
                None => return Ok(None),
            }
        }
    }
    let Some(p) = duals(cx, &d)? else { return Ok(None) };
    for row in &p {
        match linear(cx, row, &v[0], x, "greater_equal")? {
            Some(c) => parts.push(c),
            None => return Ok(None),
        }
    }
    // sum lambda_j <= 1: -(sum of the rows).(x - v_1) >= 0
    let sum: Vec<T<'h>> = (0..n).map(|k| e.neg(e.add(&p.iter().map(|r| r[k]).collect::<Vec<_>>()))).collect();
    match linear(cx, &sum, &v[1], x, "greater_equal")? {
        Some(c) => parts.push(c),
        None => return Ok(None),
    }
    Ok(Some(all(e, parts)))
}

/// The mean of the points.
fn centroid<'h>(e: E<'h>, v: &[Pt<'h>]) -> Pt<'h> {
    let n = v[0].len();
    (0..n).map(|k| e.mul(&[e.rat(1, v.len() as i64), e.add(&v.iter().map(|p| p[k]).collect::<Vec<_>>())])).collect()
}

/// The sign of `nv.(q - a)` for each point `q`, as machine numbers; `None` when one is not a number.
fn sides<'h>(cx: &mut NativeCx<'_, '_, 'h>, nv: &[T<'h>], a: &[T<'h>], qs: &[Pt<'h>]) -> Res<Option<Vec<f64>>> {
    let e = E { h: cx.heap() };
    let mut out = Vec::with_capacity(qs.len());
    for q in qs {
        match num(cx, e.dot(nv, &e.vsub(q, a)))? {
            Some(s) => out.push(s),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// The inner normal `nv` of a facet through `a`: turned towards the centroid `c`; `None` when the corners are not
/// all on that side (the region is not convex) or the normal is degenerate.
fn inner<'h>(cx: &mut NativeCx<'_, '_, 'h>, nv: Pt<'h>, a: &[T<'h>], c: &[T<'h>], corners: &[Pt<'h>]) -> Res<Option<Pt<'h>>> {
    let e = E { h: cx.heap() };
    let Some(sc) = sides(cx, &nv, a, &[c.to_vec()])? else { return Ok(None) };
    if sc[0].abs() < 1e-12 {
        return Ok(None);
    }
    let nv: Pt<'h> = if sc[0] < 0.0 { nv.iter().map(|&t| e.neg(t)).collect() } else { nv };
    let Some(s) = sides(cx, &nv, a, corners)? else { return Ok(None) };
    let scale = s.iter().fold(1.0f64, |m, v| m.max(v.abs()));
    Ok(s.iter().all(|&v| v >= -1e-9 * scale).then_some(nv))
}

/// A convex polygon of four or more corners, in the plane or in space.
fn convex_polygon<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: &[Pt<'h>], x: &[T<'h>]) -> Res<Option<T<'h>>> {
    let e = E { h: cx.heap() };
    let n = x.len();
    if !(n == 2 || n == 3) || v.len() < 3 || v.iter().any(|p| p.len() != n) {
        return Ok(None);
    }
    let d: Vec<Pt<'h>> = v[1..].iter().map(|p| e.vsub(p, &v[0])).collect();
    if rank(cx, &d)? != Some(2) {
        return Ok(None);
    }
    let mut parts = vec![];
    // a polygon in space: its plane, and the edges' half-planes in the first two coordinates onto which the plane
    // projects one to one (those whose complement has a nonzero normal component)
    let (mut i0, mut i1) = (0, 1);
    if n == 3 {
        let Some(ns) = normals(cx, &d, 3)? else { return Ok(None) };
        let [nv] = ns.as_slice() else { return Ok(None) };
        match linear(cx, nv, &v[0], x, "equal")? {
            Some(c) => parts.push(c),
            None => return Ok(None),
        }
        let nz = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<bool> { Ok(num(cx, t)?.is_none_or(|v| v != 0.0)) };
        (i0, i1) = if nz(cx, nv[2])? {
            (0, 1)
        } else if nz(cx, nv[1])? {
            (0, 2)
        } else {
            (1, 2)
        };
    }
    let c = centroid(e, v);
    let m = v.len();
    for i in 0..m {
        let (a, b) = (&v[i], &v[(i + 1) % m]);
        let dir = e.vsub(b, a);
        // an edge of symbolic length is not known to be an edge: no condition is better than a wrong one
        match num(cx, e.norm2(&dir))? {
            None => return Ok(None),
            Some(0.0) => continue,
            Some(_) => {}
        }
        let mut nv = vec![e.int(0); n];
        nv[i0] = e.neg(dir[i1]);
        nv[i1] = dir[i0];
        let Some(nv) = inner(cx, nv, a, &c, v)? else { return Ok(None) };
        match linear(cx, &nv, a, x, "greater_equal")? {
            Some(cond) => parts.push(cond),
            None => return Ok(None),
        }
    }
    Ok(Some(all(e, parts)))
}

/// A convex polyhedron given by its corners and faces.
fn polyhedron<'h>(cx: &mut NativeCx<'_, '_, 'h>, verts: &[Pt<'h>], faces: &[Vec<usize>], x: &[T<'h>]) -> Res<Option<T<'h>>> {
    let e = E { h: cx.heap() };
    if x.len() != 3 || verts.iter().any(|p| p.len() != 3) {
        return Ok(None);
    }
    let c = centroid(e, verts);
    let mut parts: Vec<T<'h>> = vec![];
    for f in faces {
        if f.len() < 3 {
            continue;
        }
        let a = &verts[f[0]];
        // the first two edges from the face's first corner that span its plane
        let mut normal = None;
        'pick: for i in 1..f.len() {
            for j in i + 1..f.len() {
                let nv = e.cross(&e.vsub(&verts[f[i]], a), &e.vsub(&verts[f[j]], a));
                if num(cx, e.norm2(&nv))?.is_some_and(|l| l > 1e-20) {
                    normal = Some(nv);
                    break 'pick;
                }
            }
        }
        // a face collapsed to a segment or a point bounds nothing
        let Some(nv) = normal else { continue };
        let Some(nv) = inner(cx, nv, a, &c, verts)? else { return Ok(None) };
        match linear(cx, &nv, a, x, "greater_equal")? {
            Some(cond) => {
                if !parts.contains(&cond) {
                    parts.push(cond);
                }
            }
            None => return Ok(None),
        }
    }
    Ok((!parts.is_empty()).then(|| all(e, parts)))
}

/// The flat `p + sum s_i f_i + sum t_j g_j` (`s_i` real, `t_j >= 0`).
fn affine<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &[T<'h>], free: &[Pt<'h>], pos: &[Pt<'h>], x: &[T<'h>]) -> Res<Option<T<'h>>> {
    let e = E { h: cx.heap() };
    let n = x.len();
    let dirs: Vec<Pt<'h>> = free.iter().chain(pos).cloned().collect();
    if dirs.iter().any(|d| d.len() != n) || rank(cx, &dirs)? != Some(dirs.len()) {
        return Ok(None);
    }
    let mut parts = vec![];
    if dirs.len() < n {
        // a line of the plane: its normal (d2, -d1); otherwise the null space of the directions
        let ns = if n == 2 { vec![vec![dirs[0][1], e.neg(dirs[0][0])]] } else {
            let Some(ns) = normals(cx, &dirs, n)? else { return Ok(None) };
            ns
        };
        for nv in &ns {
            match linear(cx, nv, p, x, "equal")? {
                Some(c) => parts.push(c),
                None => return Ok(None),
            }
        }
    }
    if !pos.is_empty() {
        let Some(du) = duals(cx, &dirs)? else { return Ok(None) };
        for row in &du[free.len()..] {
            match linear(cx, row, p, x, "greater_equal")? {
                Some(c) => parts.push(c),
                None => return Ok(None),
            }
        }
    }
    Ok((!parts.is_empty()).then(|| all(e, parts)))
}

/// A solid cylinder or cone about the axis from `a` to `b`.
fn revolved<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], b: &[T<'h>], r: T<'h>, cone: bool, x: &[T<'h>]) -> Res<Option<T<'h>>> {
    let e = E { h: cx.heap() };
    if a.len() != x.len() || b.len() != x.len() {
        return Ok(None);
    }
    let d = e.vsub(b, a);
    let dd = e.norm2(&d);
    if num(cx, dd)?.is_none_or(|v| v <= 0.0) || num(cx, r)?.is_none_or(|v| v <= 0.0) {
        return Ok(None);
    }
    let Some(lo) = linear(cx, &d, a, x, "greater_equal")? else { return Ok(None) };
    let nd: Vec<T<'h>> = d.iter().map(|&t| e.neg(t)).collect();
    let Some(hi) = linear(cx, &nd, b, x, "greater_equal")? else { return Ok(None) };
    let xa = e.vsub(x, a);
    let s = e.dot(&d, &xa);
    let perp = e.sub(e.mul(&[dd, e.norm2(&xa)]), e.sq(s));
    let q = if cone {
        e.sub(e.mul(&[dd, perp]), e.mul(&[e.sq(r), e.sq(e.sub(dd, s))]))
    } else {
        e.sub(perp, e.mul(&[e.sq(r), dd]))
    };
    let Some(q) = polynomial(cx, q, "less_equal")? else { return Ok(None) };
    Ok(Some(e.f("and", &[lo, hi, q])))
}

/// A torus or solid torus about the vertical axis through `c`.
fn torus<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &[T<'h>], r0: T<'h>, r1: T<'h>, solid: bool, x: &[T<'h>]) -> Res<Option<T<'h>>> {
    let e = E { h: cx.heap() };
    if c.len() != 3 || x.len() != 3 {
        return Ok(None);
    }
    let xc = e.vsub(x, c);
    let lhs = e.add(&[e.mul(&[r0, r1]), e.norm2(&xc)]);
    let rhs = e.mul(&[e.add(&[r0, r1]), e.sqrt(e.add(&[e.sq(xc[0]), e.sq(xc[1])]))]);
    let cond = e.f(if solid { "less_equal" } else { "equal" }, &[lhs, rhs]);
    match (num(cx, r0)?, num(cx, r1)?) {
        (Some(a), Some(b)) if 0.0 < a && a < b => Ok(Some(cond)),
        (Some(_), Some(_)) => Ok(None),
        // symbolic radii: the description holds for 0 < r0 < r1
        _ => {
            let zero = e.int(0);
            let k = e.f("and", &[e.f("greater", &[r0, zero]), e.f("greater", &[r1, zero]), e.f("less", &[r0, r1])]);
            Ok(Some(e.f("conditional_expression", &[cond, k])))
        }
    }
}
