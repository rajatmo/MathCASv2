//! Regions made from other regions: `winding_polygon` and `crossing_polygon` (the region a closed curve
//! encloses by the non-zero winding rule or the even-odd rule), `polygon_decomposition` (a convex polygon whole, otherwise triangles by ear clipping),
//! `boolean_region` (any Boolean function of plane regions), `transformed_region` (a region under an affine
//! `transformation_function`), `discretize_region` (a triangle mesh), `highlight_mesh` (a mesh with the cells of
//! some dimensions drawn in a style) and `capsule_shape`.
//!
//! Methods: the arrangement of the curve's edges (see [`crate::clip`]) with the fill rule chosen by the caller;
//! ear clipping (Meisters, "Polygons have ears", Amer. Math. Monthly 1975); the Boolean function of regions by its
//! truth table: the union, over the truth assignments for which the function is true, of "inside exactly these
//! regions" (an intersection minus a union). Plane regions only; others stay unevaluated.
use crate::clip::{self, Fill, Op, P};
use crate::ex::{inexact, nums, points, E};
use crate::natives::{coordinate, plane_rings, shapes_term, triangulate};
use crate::region::{call_parts, read, vectors, Region};
use mc_core::{Heap, Store, T};
use mc_eval::{NativeCx, Reply, Res};

/// A closed plane curve as numeric points (the last point dropped when it repeats the first), with whether it is
/// exact: a list of points, a line, a polygon's outer boundary or a triangle.
fn curve<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<(Vec<P>, bool)>> {
    let pts = match vectors(cx, t) {
        Some(v) => v,
        None => match read(cx, t) {
            Some(Region::Lines(ls)) if ls.len() == 1 => ls[0].clone(),
            Some(Region::Polygon(rs)) if rs.len() == 1 && rs[0].holes.is_empty() => rs[0].outer.clone(),
            Some(Region::Simplex(v)) if v.len() == 3 => v,
            _ => return Ok(None),
        },
    };
    let Some(p) = points(cx, &pts)? else { return Ok(None) };
    if p.len() < 3 || p[0].len() != 2 {
        return Ok(None);
    }
    let mut ring: Vec<P> = p.iter().map(|q| [q[0], q[1]]).collect();
    if ring.first() == ring.last() {
        ring.pop();
    }
    Ok(Some((ring, !inexact(cx.heap(), t))))
}

/// `winding_polygon(curve)` (non-zero rule) and `crossing_polygon(curve)` (even-odd rule).
///
/// A second argument names the rule: `nonzero_rule`, `even_odd_rule` (or `odd_rule`), `positive_rule` (the curve winds
/// about the point anticlockwise), `negative_rule` (clockwise). Options of the polygon (`vertex_colors: ...`) are kept
/// when the curve is one simple closed polygon (the region is the polygon itself, its corners those of the curve);
/// otherwise the call stays as it is.
pub fn enclosed<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], fill: Fill) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (a, opts) = match a.split_last() {
        Some((&o, rest)) if call_parts(h, o).is_some_and(|(n, _)| n == "options") => (rest, Some(o)),
        _ => (a, None),
    };
    let (c, fill) = match a {
        [c] => (c, fill),
        [c, r] => match h.view(*r) {
            mc_core::View::Sym(s) => match h.sym_name(s).as_str() {
                "nonzero_rule" => (c, Fill::NonZero),
                "even_odd_rule" | "odd_rule" => (c, Fill::EvenOdd),
                "positive_rule" => (c, Fill::Positive),
                "negative_rule" => (c, Fill::Negative),
                _ => return Ok(Reply::Decline),
            },
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    if let Some(o) = opts {
        let Some((ring, _)) = curve(cx, *c)? else { return Ok(Reply::Decline) };
        let Some(pts) = cx.as_list(*c).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
        let shapes = clip::boolean_fill(std::slice::from_ref(&ring), &[], Op::Union, fill);
        // the region is the polygon of the curve itself: one ring without holes, through the curve's corners only
        let same = match shapes.as_slice() {
            [(outer, holes)] => {
                holes.is_empty()
                    && outer.len() == ring.len()
                    && outer.iter().all(|p| ring.iter().any(|q| (p[0] - q[0]).abs() <= 1e-12 * (1.0 + q[0].abs()) && (p[1] - q[1]).abs() <= 1e-12 * (1.0 + q[1].abs())))
            }
            _ => false,
        };
        if !same || pts.len() != ring.len() {
            return Ok(Reply::Decline);
        }
        let e = E { h };
        return Ok(Reply::Done(e.f("polygon", &[cx.list(&pts), o])));
    }
    if let Some(t) = space_polygon(cx, *c, fill)? {
        return Ok(Reply::Done(t));
    }
    // one closed curve, or a list of them (the rule is applied to all their edges together)
    let (rings, exact) = match curve(cx, *c)? {
        Some((ring, exact)) => (vec![ring], exact),
        None => {
            let Some(items) = cx.as_list(*c).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
            if items.len() < 2 {
                return Ok(Reply::Decline);
            }
            let mut rings = vec![];
            let mut exact = true;
            for it in items {
                let Some((ring, ex)) = curve(cx, it)? else { return Ok(Reply::Decline) };
                rings.push(ring);
                exact &= ex;
            }
            (rings, exact)
        }
    };
    let shapes = clip::boolean_fill(&rings, &[], Op::Union, fill);
    Ok(Reply::Done(shapes_term(cx, &shapes, exact)))
}

/// A closed curve of points in a plane of space (three or more coordinates): the points are put in coordinates of
/// their plane (an orthonormal basis of the differences from the first point, by Gram-Schmidt); when the rule fills
/// exactly the polygon of the curve (one ring without holes through its own corners) the result is
/// `polygon(points, [1, ..., n])`, and when it fills nothing, `empty_region(d)`. Curves off a plane, and curves
/// whose filled region is another polygon, give `None`.
fn space_polygon<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>, fill: Fill) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    // the sense of turning in a plane of space depends on a chosen normal
    if !matches!(fill, Fill::NonZero | Fill::EvenOdd) {
        return Ok(None);
    }
    let Some(mut pts) = vectors(cx, c) else { return Ok(None) };
    let d = pts[0].len();
    if d < 3 || pts.len() < 3 {
        return Ok(None);
    }
    if pts.len() > 3 && pts.first() == pts.last() {
        pts.pop();
    }
    let Some(p) = points(cx, &pts)? else { return Ok(None) };
    let scale = p.iter().flatten().fold(1.0f64, |m, x| m.max(x.abs()));
    let diff: Vec<Vec<f64>> = p.iter().map(|q| q.iter().zip(&p[0]).map(|(a, b)| a - b).collect()).collect();
    let mut basis: Vec<Vec<f64>> = vec![];
    for v in &diff {
        let mut w = v.clone();
        for b in &basis {
            let dot: f64 = w.iter().zip(b).map(|(x, y)| x * y).sum();
            w.iter_mut().zip(b).for_each(|(x, y)| *x -= dot * y);
        }
        let len = w.iter().map(|x| x * x).sum::<f64>().sqrt();
        if len > 1e-9 * scale {
            if basis.len() == 2 {
                return Ok(None);
            }
            basis.push(w.iter().map(|x| x / len).collect());
        }
    }
    if basis.len() != 2 {
        return Ok(None);
    }
    let ring: Vec<P> = diff.iter().map(|v| [0, 1].map(|k| v.iter().zip(&basis[k]).map(|(x, y)| x * y).sum::<f64>())).collect();
    let shapes = clip::boolean_fill(std::slice::from_ref(&ring), &[], Op::Union, fill);
    let close = |a: &P, b: &P| (a[0] - b[0]).abs() <= 1e-9 * scale && (a[1] - b[1]).abs() <= 1e-9 * scale;
    match shapes.as_slice() {
        [] => Ok(Some(e.f("empty_region", &[e.int(d as i64)]))),
        [(outer, holes)] if holes.is_empty() && outer.len() == ring.len() && outer.iter().all(|q| ring.iter().any(|r| close(q, r))) => {
            let idx: Vec<T<'h>> = (1..=pts.len() as i64).map(|i| e.int(i)).collect();
            Ok(Some(e.f("polygon", &[e.list(&pts.iter().map(|q| e.list(q)).collect::<Vec<_>>()), e.list(&idx)])))
        }
        _ => Ok(None),
    }
}

/// `polygon_decomposition(region)`, `polygon_decomposition(region, convex)` or `polygon_decomposition(region,
/// triangle)`: the region (a polygon without holes, a rectangle, a triangle, or an implicit region given by bounds on
/// each coordinate) as a list of anticlockwise pieces `polygon(corners, [1, ..., k])`. A convex polygon is its own
/// convex piece; otherwise, and always with `triangle`, the pieces are triangles.
pub fn decomposition<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    match a {
        [_] => {}
        [_, form] if matches!(h.view(*form), mc_core::View::Sym(s) if matches!(h.sym_name(s).as_str(), "triangle" | "convex")) => {}
        _ => return Ok(Reply::Decline),
    }
    let triangles = a.len() == 2 && matches!(h.view(a[1]), mc_core::View::Sym(s) if h.sym_name(s) == "triangle");
    let rs = match crate::construct::rings(cx, a[0])? {
        Some(rs) => rs,
        None => match implicit_box(cx, a[0])? {
            Some((lo, hi)) => {
                let boxed = e.f("rectangle", &[e.list(&lo), e.list(&hi)]);
                match crate::construct::rings(cx, boxed)? {
                    Some(rs) => rs,
                    None => return Ok(Reply::Decline),
                }
            }
            None => return Ok(Reply::Decline),
        },
    };
    let mut out = vec![];
    for (ring, num, holes) in &rs {
        if !holes.is_empty() {
            return Ok(Reply::Decline);
        }
        let pts: Vec<P> = num.iter().map(|p| [p[0], p[1]]).collect();
        let corners_of =|cx: &mut NativeCx<'_, '_, 'h>, ix: &[usize]| -> Res<T<'h>> {
            let mut cs = Vec::with_capacity(ix.len());
            for &i in ix {
                cs.push(cx.eval(e.list(&ring.outer[i]))?);
            }
            let idx: Vec<T<'h>> = (1..=ix.len() as i64).map(|k| e.int(k)).collect();
            Ok(e.f("polygon", &[e.list(&cs), e.list(&idx)]))
        };
        // a convex polygon is its own convex piece (the default and `convex`)
        if !triangles && convex(&pts) {
            let ix: Vec<usize> = if signed_area2(&pts) >= 0.0 { (0..pts.len()).collect() } else { (0..pts.len()).rev().collect() };
            out.push(corners_of(cx, &ix)?);
            continue;
        }
        for t in triangulate(&pts) {
            out.push(corners_of(cx, &t)?);
        }
    }
    Ok(Reply::Done(e.list(&out)))
}

/// Twice the signed area of a ring (positive when anticlockwise).
fn signed_area2(p: &[P]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum()
}

/// Whether a ring is a convex polygon: every turn has the sense of the whole ring (straight corners allowed), and
/// it goes round once (the turning angles add up to one full turn).
fn convex(p: &[P]) -> bool {
    let n = p.len();
    if n < 3 {
        return false;
    }
    let s = signed_area2(p);
    if s == 0.0 {
        return false;
    }
    let scale = p.iter().fold(1.0f64, |m, q| m.max(q[0].abs()).max(q[1].abs()));
    let mut total = 0.0;
    for i in 0..n {
        let (a, b, c) = (p[i], p[(i + 1) % n], p[(i + 2) % n]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        let cross = u[0] * v[1] - u[1] * v[0];
        if cross * s.signum() < -1e-12 * scale * scale {
            return false;
        }
        total += cross.atan2(u[0] * v[0] + u[1] * v[1]);
    }
    (total.abs() - std::f64::consts::TAU).abs() < 1e-6
}

/// The box of an implicit region of the plane given as bounds on each variable separately, `a <= x <= b` (strict
/// or not: the decomposition is of the closure), with numeric bounds: `(lo, hi)` as terms.
#[allow(clippy::type_complexity)]
fn implicit_box<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<(Vec<T<'h>>, Vec<T<'h>>)>> {
    let h = cx.heap();
    let Some(Region::Implicit { cond, vars }) = read(cx, t) else { return Ok(None) };
    if vars.len() != 2 {
        return Ok(None);
    }
    let mut parts = vec![];
    let mut stack = vec![cond];
    while let Some(c) = stack.pop() {
        match call_parts(h, c) {
            Some((n, args)) if n == "and" => stack.extend(args.iter().copied()),
            Some((n, args)) if matches!(n.as_str(), "less" | "less_equal" | "greater" | "greater_equal") && args.len() >= 2 => {
                for w in args.windows(2) {
                    let (small, big) = if n.starts_with("less") { (w[0], w[1]) } else { (w[1], w[0]) };
                    parts.push((small, big));
                }
            }
            _ => return Ok(None),
        }
    }
    let mut lo: Vec<Option<(f64, T<'h>)>> = vec![None; 2];
    let mut hi: Vec<Option<(f64, T<'h>)>> = vec![None; 2];
    for (small, big) in parts {
        let (k, bound, upper) = match (vars.iter().position(|&v| v == small), vars.iter().position(|&v| v == big)) {
            (Some(k), None) => (k, big, true),
            (None, Some(k)) => (k, small, false),
            _ => return Ok(None),
        };
        let Some(x) = nums(cx, &[bound])?.and_then(|v| v.first().copied()).filter(|x| x.is_finite()) else { return Ok(None) };
        let slot = if upper { &mut hi[k] } else { &mut lo[k] };
        let tighter = match slot {
            None => true,
            Some((y, _)) => (upper && x < *y) || (!upper && x > *y),
        };
        if tighter {
            *slot = Some((x, bound));
        }
    }
    let mut l = vec![];
    let mut u = vec![];
    for k in 0..2 {
        let (Some((a, at)), Some((b, bt))) = (lo[k], hi[k]) else { return Ok(None) };
        if a >= b {
            return Ok(None);
        }
        l.push(at);
        u.push(bt);
    }
    Ok(Some((l, u)))
}

/// Whether the call `f(args)` for a Boolean function `f` is true.
fn truth<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, bits: &[bool]) -> Res<Option<bool>> {
    let h = cx.heap();
    let args: Vec<T<'h>> = bits.iter().map(|&b| h.sym_term(h.sym(if b { "true" } else { "false" }))).collect();
    let v = cx.eval(h.app(f, &args))?;
    Ok(match h.view(v) {
        mc_core::View::Sym(s) => match h.sym_name(s).as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

fn flat(shapes: &clip::Shapes) -> Vec<Vec<P>> {
    shapes.iter().flat_map(|(o, hs)| std::iter::once(o.clone()).chain(hs.iter().cloned())).collect()
}

/// `boolean_region(f, [r1, r2, ...])`: the points whose membership in the regions satisfies the Boolean function
/// `f` (`and`, `or`, `xor`, `not`, or a function of the memberships), for up to five plane regions.
pub fn boolean_region<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let [f, rs] = a else { return Ok(Reply::Decline) };
    let Some(items) = cx.as_list(*rs).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
    let k = items.len();
    if k == 0 || k > 5 {
        return Ok(Reply::Decline);
    }
    // mesh regions: a mesh of the kind of the first
    if let Some(t) = crate::csg::combine(cx, &items, |cx, bits| truth(cx, *f, bits))? {
        return Ok(Reply::Done(t));
    }
    let mut regions = vec![];
    let mut exact = true;
    for &t in &items {
        let Some((rings, ex)) = plane_rings(cx, t)? else { return Ok(Reply::Decline) };
        exact &= ex;
        regions.push(rings);
    }
    let mut result: Vec<Vec<P>> = vec![];
    for mask in 1u32..(1 << k) {
        // every assignment but the all-false one: its region is the complement, which is unbounded
        let bits: Vec<bool> = (0..k).map(|i| mask >> i & 1 == 1).collect();
        match truth(cx, *f, &bits)? {
            Some(true) => {}
            Some(false) => continue,
            None => return Ok(Reply::Decline),
        }
        let inside: Vec<usize> = (0..k).filter(|&i| bits[i]).collect();
        let mut acc = regions[inside[0]].clone();
        for &i in &inside[1..] {
            acc = flat(&clip::boolean(&acc, &regions[i], Op::Intersection));
        }
        for i in (0..k).filter(|&i| !bits[i]) {
            acc = flat(&clip::boolean(&acc, &regions[i], Op::Difference));
        }
        if !acc.is_empty() {
            result = flat(&clip::boolean(&result, &acc, Op::Union));
        }
    }
    let shapes = clip::boolean(&result, &[], Op::Union);
    Ok(Reply::Done(shapes_term(cx, &shapes, exact)))
}

/// The matrix of `transformation_function(m)` as numbers, with its terms.
#[allow(clippy::type_complexity)]
fn affine<'h>(cx: &mut NativeCx<'_, '_, 'h>, tf: T<'h>) -> Res<Option<Vec<Vec<f64>>>> {
    let h = cx.heap();
    let Some((name, args)) = call_parts(h, tf) else { return Ok(None) };
    if name != "transformation_function" || args.len() != 1 {
        return Ok(None);
    }
    let Some(rows) = cx.as_list(args[0]).map(|l| l.to_vec()) else { return Ok(None) };
    let mut m = vec![];
    for r in rows {
        let Some(cells) = cx.as_list(r).map(|l| l.to_vec()) else { return Ok(None) };
        let Some(v) = nums(cx, &cells)? else { return Ok(None) };
        m.push(v);
    }
    Ok((m.len() >= 2 && m.iter().all(|r| r.len() == m.len())).then_some(m))
}

fn apply<'h>(cx: &mut NativeCx<'_, '_, 'h>, tf: T<'h>, p: &[T<'h>]) -> Res<Option<Vec<T<'h>>>> {
    let h = cx.heap();
    let v = cx.eval(h.app(tf, &[cx.list(p)]))?;
    Ok(cx.as_list(v).map(|l| l.to_vec()).filter(|l| l.len() == p.len()))
}

/// `transformed_region(region, tf)` for a plane or space polygon-like region (points, lines, polygons, triangles,
/// rectangles, cuboids, tetrahedra) and a `transformation_function`; a disk or ball under a similarity or an axis
/// scaling stays a disk, ball or ellipse. Other regions and maps stay unevaluated.
pub fn transformed<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let [r, tf] = a else { return Ok(Reply::Decline) };
    let m = match affine(cx, *tf)? {
        Some(m) => m,
        None => {
            // a function that is affine: as a transformation function when it keeps the dimension, else the
            // convex hull of the images of a convex polytope's corners
            let Some(reg) = read(cx, *r) else { return Ok(Reply::Decline) };
            let n = reg.embedding();
            let Some((lin, off)) = affine_function(cx, *tf, n)? else { return Ok(Reply::Decline) };
            if lin.len() == n {
                let mut rows: Vec<T<'h>> = (0..n).map(|i| {
                    let mut row = lin[i].clone();
                    row.push(off[i]);
                    e.list(&row)
                }).collect();
                let mut last = vec![e.int(0); n];
                last.push(e.int(1));
                rows.push(e.list(&last));
                let tf2 = e.f("transformation_function", &[e.list(&rows)]);
                return transformed(cx, &[*r, tf2]);
            }
            return projected(cx, &reg, &lin, &off);
        }
    };
    let Some(reg) = read(cx, *r) else { return Ok(Reply::Decline) };
    let n = m.len() - 1;
    if reg.embedding() != n {
        return Ok(Reply::Decline);
    }
    let map = |cx: &mut NativeCx<'_, '_, 'h>, ps: &[Vec<T<'h>>]| -> Res<Option<Vec<T<'h>>>> {
        let mut out = vec![];
        for p in ps {
            match apply(cx, *tf, p)? {
                Some(q) => out.push(e.list(&q)),
                None => return Ok(None),
            }
        }
        Ok(Some(out))
    };
    let t = match &reg {
        Region::Points(ps) => map(cx, ps)?.map(|v| e.f("point", &[e.list(&v)])),
        Region::Lines(ls) => {
            let mut parts = vec![];
            for l in ls {
                match map(cx, l)? {
                    Some(v) => parts.push(e.list(&v)),
                    None => return Ok(Reply::Decline),
                }
            }
            Some(e.f("line", &[if parts.len() == 1 { parts[0] } else { e.list(&parts) }]))
        }
        Region::Polygon(rs) if rs.iter().all(|x| x.holes.is_empty()) => {
            let mut parts = vec![];
            for x in rs {
                match map(cx, &x.outer)? {
                    Some(v) => parts.push(e.list(&v)),
                    None => return Ok(Reply::Decline),
                }
            }
            Some(e.f("polygon", &[if parts.len() == 1 { parts[0] } else { e.list(&parts) }]))
        }
        // an affine image of a triangle is a triangle (of a tetrahedron a tetrahedron)
        Region::Simplex(v) if v.len() == 3 && n == 2 => map(cx, v)?.map(|v| e.f("triangle", &[e.list(&v)])),
        Region::Simplex(v) if v.len() == 4 && n == 3 => map(cx, v)?.map(|v| e.f("tetrahedron", &[e.list(&v)])),
        Region::Boxed { lo, hi } if n == 2 => {
            let corners = vec![lo.clone(), vec![hi[0], lo[1]], hi.clone(), vec![lo[0], hi[1]]];
            map(cx, &corners)?.map(|v| e.f("polygon", &[e.list(&v)]))
        }
        Region::Boxed { lo, hi } if n == 3 => {
            let (x, y, z) = ((lo[0], hi[0]), (lo[1], hi[1]), (lo[2], hi[2]));
            let c = |a: T<'h>, b: T<'h>, d: T<'h>| vec![a, b, d];
            let corners = vec![
                c(x.0, y.0, z.0),
                c(x.1, y.0, z.0),
                c(x.1, y.1, z.0),
                c(x.0, y.1, z.0),
                c(x.0, y.0, z.1),
                c(x.1, y.0, z.1),
                c(x.1, y.1, z.1),
                c(x.0, y.1, z.1),
            ];
            map(cx, &corners)?.map(|v| e.f("hexahedron", &[e.list(&v)]))
        }
        Region::Ball { c, r: crate::region::Radius::One(rad), solid } => {
            // a similarity keeps a disk a disk: the linear part L has L^T L = s^2 I
            let l: Vec<Vec<f64>> = (0..n).map(|i| m[i][..n].to_vec()).collect();
            let gram = |i: usize, j: usize| (0..n).map(|k| l[k][i] * l[k][j]).sum::<f64>();
            let s2 = gram(0, 0);
            let similar = s2 > 0.0 && (0..n).all(|i| (0..n).all(|j| (gram(i, j) - if i == j { s2 } else { 0.0 }).abs() <= 1e-12 * s2));
            if !similar {
                // a solid disk or ball under any other nonsingular affine map x -> L x + b is the ellipsoid
                // (x - c')^T (r^2 L L^T)^-1 (x - c') <= 1 with c' = L c + b
                return match (solid, ellipsoid_image(cx, *tf, c, *rad, n)?) {
                    (true, Some(t)) => Ok(Reply::Done(t)),
                    _ => Ok(Reply::Decline),
                };
            }
            let Some(c2) = apply(cx, *tf, c)? else { return Ok(Reply::Decline) };
            let s = s2.sqrt();
            let rad2 = if (s - 1.0).abs() <= 1e-12 { *rad } else { e.mul(&[h.real(s), *rad]) };
            let name = match (n, *solid) {
                (2, true) => "disk",
                (2, false) => "circle",
                (3, true) => "ball",
                (3, false) => "sphere",
                _ => return Ok(Reply::Decline),
            };
            Some(e.f(name, &[e.list(&c2), rad2]))
        }
        _ => None,
    };
    match t {
        Some(t) => Ok(Reply::Done(cx.eval(t)?)),
        None => Ok(Reply::Decline),
    }
}

/// `ellipsoid(L c + b, r^2 L L^T)` for `transformation_function(m)` with exact entries whose linear part L is
/// nonsingular, or `None`.
fn ellipsoid_image<'h>(cx: &mut NativeCx<'_, '_, 'h>, tf: T<'h>, c: &[T<'h>], rad: T<'h>, n: usize) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    if !(2..=3).contains(&n) || inexact(h, tf) || inexact(h, rad) || c.iter().any(|&x| inexact(h, x)) {
        return Ok(None);
    }
    let Some(("transformation_function", &[m])) = call_parts(h, tf).as_ref().map(|(k, a)| (k.as_str(), *a)) else { return Ok(None) };
    let Some(rows) = cx.as_list(m).map(|l| l.to_vec()) else { return Ok(None) };
    let mut l = vec![];
    for r in rows.iter().take(n) {
        let Some(x) = cx.as_list(*r).filter(|x| x.len() == n + 1) else { return Ok(None) };
        l.push(x[..n].to_vec());
    }
    if l.len() != n {
        return Ok(None);
    }
    let det = cx.eval(e.f("det", &[e.list(&l.iter().map(|r| e.list(r)).collect::<Vec<_>>())]))?;
    if !crate::ex::num(cx, det)?.is_some_and(|v| v != 0.0) {
        return Ok(None);
    }
    let Some(c2) = apply(cx, tf, c)? else { return Ok(None) };
    let r2 = e.sq(rad);
    let mut mat = vec![];
    for i in 0..n {
        let mut row = vec![];
        for j in 0..n {
            let s = e.add(&(0..n).map(|k| e.mul(&[l[i][k], l[j][k]])).collect::<Vec<_>>());
            row.push(cx.eval(e.f("simplify", &[e.mul(&[r2, s])]))?);
        }
        mat.push(e.list(&row));
    }
    Ok(Some(e.f("ellipsoid", &[e.list(&c2), e.list(&mat)])))
}

/// A function of n coordinates that is affine: its linear part (m rows of n exact numbers) and its offset, read off
/// its value at a point of fresh symbols and checked by expanding the difference.
#[allow(clippy::type_complexity)]
fn affine_function<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, n: usize) -> Res<Option<(Vec<Vec<T<'h>>>, Vec<T<'h>>)>> {
    let h = cx.heap();
    let e = E { h };
    if n == 0 || call_parts(h, f).is_some_and(|(n, _)| n == "transformation_function") {
        return Ok(None);
    }
    let xs: Vec<T<'h>> = (0..n).map(|i| e.sym(&format!("region$x{i}"))).collect();
    let v = cx.eval(h.app(f, &[e.list(&xs)]))?;
    let Some(comps) = cx.as_list(v).map(|l| l.to_vec()) else { return Ok(None) };
    if comps.is_empty() || comps.len() > 3 {
        return Ok(None);
    }
    let zero: Vec<T<'h>> = xs.iter().map(|&x| e.f("rule", &[x, e.int(0)])).collect();
    let mut lin = vec![];
    let mut off = vec![];
    for c in comps {
        let c = cx.eval(e.f("expand", &[c]))?;
        let mut row = vec![];
        for &x in &xs {
            let k = cx.eval(e.f("coefficient", &[c, x, e.int(1)]))?;
            if crate::ex::num(cx, k)?.is_none() {
                return Ok(None);
            }
            row.push(k);
        }
        let k0 = cx.eval(e.f("rewrite", &[c, e.list(&zero)]))?;
        if crate::ex::num(cx, k0)?.is_none() {
            return Ok(None);
        }
        let rebuilt = e.add(&xs.iter().zip(&row).map(|(&x, &k)| e.mul(&[k, x])).chain(std::iter::once(k0)).collect::<Vec<_>>());
        let diff = cx.eval(e.f("expand", &[e.sub(c, rebuilt)]))?;
        if !matches!(h.view(diff), mc_core::View::Int(0)) {
            return Ok(None);
        }
        lin.push(row);
        off.push(k0);
    }
    Ok(Some((lin, off)))
}

/// The image of a box or simplex under an affine map into the plane: the convex hull of the images of its corners
/// (the image of a convex set under a linear map is the hull of the images of its extreme points), as a polygon,
/// segment or point.
fn projected<'h>(cx: &mut NativeCx<'_, '_, 'h>, reg: &Region<'h>, lin: &[Vec<T<'h>>], off: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    if lin.len() != 2 {
        return Ok(Reply::Decline);
    }
    let corners: Vec<Vec<T<'h>>> = match reg {
        Region::Boxed { lo, hi } if lo.len() <= 4 => (0..1usize << lo.len()).map(|mask| (0..lo.len()).map(|k| if mask >> k & 1 == 1 { hi[k] } else { lo[k] }).collect()).collect(),
        Region::Simplex(v) => v.clone(),
        _ => return Ok(Reply::Decline),
    };
    let mut imgs: Vec<(Vec<T<'h>>, P)> = vec![];
    for c in &corners {
        let mut q = vec![];
        for i in 0..2 {
            let t = e.add(&lin[i].iter().zip(c).map(|(&k, &x)| e.mul(&[k, x])).chain(std::iter::once(off[i])).collect::<Vec<_>>());
            q.push(cx.eval(t)?);
        }
        let Some(v) = nums(cx, &q)? else { return Ok(Reply::Decline) };
        imgs.push((q, [v[0], v[1]]));
    }
    // Andrew's monotone chain, anticlockwise, collinear points dropped
    imgs.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]).then(a.1[1].total_cmp(&b.1[1])));
    imgs.dedup_by(|a, b| a.1 == b.1);
    let cross = |o: P, a: P, b: P| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let scale = imgs.iter().fold(1.0f64, |m, p| m.max(p.1[0].abs()).max(p.1[1].abs()));
    let tol = 1e-12 * scale * scale;
    let mut hull: Vec<usize> = vec![];
    for pass in 0..2 {
        let start = hull.len();
        let order: Vec<usize> = if pass == 0 { (0..imgs.len()).collect() } else { (0..imgs.len()).rev().collect() };
        for i in order {
            while hull.len() >= start + 2 && cross(imgs[hull[hull.len() - 2]].1, imgs[hull[hull.len() - 1]].1, imgs[i].1) <= tol {
                hull.pop();
            }
            hull.push(i);
        }
        hull.pop();
    }
    let pts: Vec<T<'h>> = hull.iter().map(|&i| e.list(&imgs[i].0)).collect();
    let t = match pts.len() {
        0 => match imgs.first() {
            Some(p) => e.f("point", &[e.list(&p.0)]),
            None => return Ok(Reply::Decline),
        },
        1 | 2 => e.f("line", &[e.list(&[e.list(&imgs[0].0), e.list(&imgs[imgs.len() - 1].0)])]),
        _ => e.f("polygon", &[e.list(&pts)]),
    };
    Ok(Reply::Done(cx.eval(t)?))
}

/// Does `t` contain the symbol `name` (a small explicit walk)?
fn mentions(h: &mc_core::Heap, t: T<'_>, name: &str) -> bool {
    let s = h.sym_term(h.sym(name));
    let mut stack = mc_api::walk::Walk::new(t);
    let mut budget = 10_000usize;
    while let Some(u) = stack.pop() {
        if u == s {
            return true;
        }
        if let mc_core::View::App { head, args } = h.view(u) {
            if budget < args.len() {
                return false;
            }
            budget -= args.len() + 1;
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// `discretize_region(region)` (and with plotting ranges, which are ignored for regions that are bounded): a
/// triangle `mesh_region` of a plane polygon-like region (disks as fine polygons), a line mesh of lines.
pub fn discretize<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let Some(&r) = a.first() else { return Ok(Reply::Decline) };
    if a.len() > 2 {
        return Ok(Reply::Decline);
    }
    // a bound on the cell size is not honoured here (the mesh is the polygon's triangulation): a failure that says
    // so, not a mesh that ignores it
    if a.len() == 2 && mentions(h, a[1], "max_cell_measure") {
        return Ok(Reply::Done(e.f("failure", &[h.sym_term(h.sym("unsupported_option")), h.sym_term(h.sym("max_cell_measure"))])));
    }
    if call_parts(h, r).is_some_and(|(n, _)| n == "mesh_region" || n == "boundary_mesh_region") {
        return Ok(Reply::Done(r));
    }
    // the intersection of two disks, kept whole: the mesh of its polygon
    if let Some(("region_intersection", args)) = call_parts(h, r).as_ref().map(|(n, x)| (n.as_str(), *x)) {
        if crate::natives::two_disks(cx, args)?.is_some() {
            if let Reply::Done(p) = crate::natives::boolean_polygons(cx, args, Op::Intersection)? {
                let mut b = vec![p];
                b.extend_from_slice(&a[1..]);
                return discretize(cx, &b);
            }
        }
    }
    // an implicit region: a mesh of the grid's cells (in the given box, else in its bounds)
    if let Some(reg @ Region::Implicit { .. }) = read(cx, r) {
        let ranges = match a.get(1) {
            Some(&b) => {
                let Some(rows) = cx.as_list(b).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
                let mut out = vec![];
                for row in rows {
                    let Some(&[lo, hi]) = cx.as_list(row) else { return Ok(Reply::Decline) };
                    match nums(cx, &[lo, hi])? {
                        Some(v) => out.push((v[0], v[1])),
                        None => return Ok(Reply::Decline),
                    }
                }
                Some(out)
            }
            None => None,
        };
        return Ok(match crate::contour::mesh(cx, &reg, ranges)? {
            Some(t) => Reply::Done(t),
            None => Reply::Decline,
        });
    }
    if let Some(Region::Lines(ls)) = read(cx, r) {
        let mut pts = vec![];
        let mut cells = vec![];
        for l in &ls {
            let first = pts.len() as i64 + 1;
            pts.extend(l.iter().map(|p| e.list(p)));
            let idx: Vec<T<'h>> = (0..l.len() as i64).map(|i| e.int(first + i)).collect();
            cells.push(e.f("line", &[e.list(&idx)]));
        }
        return Ok(Reply::Done(cx.eval(e.f("mesh_region", &[e.list(&pts), e.list(&cells)]))?));
    }
    let Some((rings, exact)) = plane_rings(cx, r)? else { return Ok(Reply::Decline) };
    // the pieces with their holes are not triangulated: only simple polygons here
    let shapes = clip::boolean(&rings, &[], Op::Union);
    if shapes.iter().any(|s| !s.1.is_empty()) {
        return Ok(Reply::Decline);
    }
    let mut pts = vec![];
    let mut cells = vec![];
    for (ring, _) in &shapes {
        let first = pts.len() as i64 + 1;
        pts.extend(ring.iter().map(|p| e.list(&[coordinate(h, p[0], exact), coordinate(h, p[1], exact)])));
        for t in triangulate(ring) {
            let idx: Vec<T<'h>> = t.iter().map(|&i| e.int(first + i as i64)).collect();
            cells.push(e.f("triangle", &[e.list(&idx)]));
        }
    }
    if cells.is_empty() {
        return Ok(Reply::Decline);
    }
    Ok(Reply::Done(cx.eval(e.f("mesh_region", &[e.list(&pts), e.list(&cells)]))?))
}

/// `highlight_mesh(mesh, style(d, directives...), ...)`: the mesh shown with its cells of dimension `d` drawn with
/// the directives (`d` alone: in red).
pub fn highlight<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let Some((&m, styles)) = a.split_first() else { return Ok(Reply::Decline) };
    if styles.is_empty() || !call_parts(h, m).is_some_and(|(n, _)| n == "mesh_region" || n == "boundary_mesh_region") {
        return Ok(Reply::Decline);
    }
    let dim = match cx.eval(e.f("region_embedding_dimension", &[m]))? {
        // a mesh of the line is drawn on the x axis of the plane
        v if matches!(h.view(v), mc_core::View::Int(1)) => 1,
        v if matches!(h.view(v), mc_core::View::Int(2)) => 2,
        v if matches!(h.view(v), mc_core::View::Int(3)) => 3,
        _ => return Ok(Reply::Decline),
    };
    let mut groups = vec![];
    for &s in styles {
        let (d, dirs): (T<'h>, Vec<T<'h>>) = match call_parts(h, s) {
            Some((n, args)) if n == "style" && !args.is_empty() => (args[0], args[1..].to_vec()),
            _ => (s, vec![h.sym_term(h.sym("red"))]),
        };
        let prims = cx.eval(e.f("mesh_primitives", &[m, d]))?;
        if call_parts(h, prims).is_some_and(|(n, _)| n == "mesh_primitives") {
            return Ok(Reply::Decline);
        }
        let mut g = dirs;
        g.push(if dim == 1 { on_axis(cx, prims) } else { prims });
        groups.push(e.list(&g));
    }
    let g = e.f(if dim <= 2 { "graphics" } else { "graphics3d" }, &[e.list(&groups)]);
    Ok(Reply::Done(cx.eval(e.f("show", &[m, g]))?))
}

/// Primitives of a mesh of the line with every point `[x]` written `[x, 0]` (on the x axis of the plane).
fn on_axis<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> T<'h> {
    let h = cx.heap();
    if let Some(items) = cx.as_list(t) {
        if let [x] = items {
            use mc_core::View;
            if matches!(h.view(*x), View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. }) {
                return cx.list(&[*x, h.int(0)]);
            }
        }
        let items = items.to_vec();
        return cx.list(&items.into_iter().map(|x| on_axis(cx, x)).collect::<Vec<_>>());
    }
    match h.view(t) {
        mc_core::View::App { head, args } => {
            let args = args.to_vec();
            h.app(head, &args.into_iter().map(|x| on_axis(cx, x)).collect::<Vec<_>>())
        }
        _ => t,
    }
}

/// `capsule_shape()`, `capsule_shape([p1, p2])`, `capsule_shape([p1, p2], r)`: the points within r of the segment
/// p1 p2 (a cylinder with a hemisphere on each end); the defaults are the segment from (-1, 0, 0) to (1, 0, 0) and
/// the radius 1. Shorter forms evaluate to the full one.
pub fn capsule<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let seg = match a.first() {
        None => e.list(&[e.list(&[e.int(-1), e.int(0), e.int(0)]), e.list(&[e.int(1), e.int(0), e.int(0)])]),
        Some(&s) if vectors(cx, s).is_some_and(|v| v.len() == 2) => s,
        _ => return Ok(Reply::Decline),
    };
    match a {
        [] | [_] => Ok(Reply::Done(e.f("capsule_shape", &[seg, e.int(1)]))),
        [_, _] => Ok(Reply::Decline),
        _ => Ok(Reply::Decline),
    }
}

/// The measure, as a term, of `capsule_shape([p1, p2], r)`: pi r^2 |p2 - p1| + 4/3 pi r^3.
pub fn capsule_volume<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<T<'h>> {
    let h: &'h Heap = cx.heap();
    let e = E { h };
    let (name, args) = call_parts(h, t)?;
    let [seg, r] = args else { return None };
    if name != "capsule_shape" {
        return None;
    }
    let v = vectors(cx, *seg).filter(|v| v.len() == 2)?;
    let len = e.dist(&v[0], &v[1]);
    let r2 = e.sq(*r);
    Some(e.mul(&[e.pi(), r2, e.add(&[len, e.mul(&[e.rat(4, 3), *r])])]))
}
