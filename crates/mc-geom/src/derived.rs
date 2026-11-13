//! Regions made from other regions (round 29): `winding_polygon` and `crossing_polygon` (the region a closed curve
//! encloses by the non-zero winding rule or the even-odd rule), `polygon_decomposition` (triangles by ear clipping),
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
/// A second argument names the rule: "NonzeroRule", "EvenOddRule" (or "OddRule"), "PositiveRule" (the curve winds
/// about the point anticlockwise), "NegativeRule" (clockwise). Options of the polygon (`vertex_colors: ...`) are kept
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
            mc_core::View::Str(s) => match s {
                "NonzeroRule" => (c, Fill::NonZero),
                "EvenOddRule" | "OddRule" => (c, Fill::EvenOdd),
                "PositiveRule" => (c, Fill::Positive),
                "NegativeRule" => (c, Fill::Negative),
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

/// `polygon_decomposition(region)` or `polygon_decomposition(region, "Triangle")`: the region (a polygon without
/// holes, a rectangle, a triangle) as a list of triangles `polygon([p1, p2, p3], [1, 2, 3])`, anticlockwise.
pub fn decomposition<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    match a {
        [_] => {}
        [_, form] if matches!(h.view(*form), mc_core::View::Str(s) if s == "Triangle" || s == "Convex") => {}
        _ => return Ok(Reply::Decline),
    }
    let Some(rs) = crate::construct::rings(cx, a[0])? else { return Ok(Reply::Decline) };
    let mut out = vec![];
    for (ring, num, holes) in &rs {
        if !holes.is_empty() {
            return Ok(Reply::Decline);
        }
        let pts: Vec<P> = num.iter().map(|p| [p[0], p[1]]).collect();
        for t in triangulate(&pts) {
            let corners: Vec<T<'h>> = t.iter().map(|&i| e.list(&ring.outer[i])).collect();
            out.push(e.f("polygon", &[e.list(&corners), e.list(&[e.int(1), e.int(2), e.int(3)])]));
        }
    }
    Ok(Reply::Done(e.list(&out)))
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
    let Some(m) = affine(cx, *tf)? else { return Ok(Reply::Decline) };
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
        Region::Simplex(v) if v.len() == 3 && n == 2 => map(cx, v)?.map(|v| e.f("polygon", &[e.list(&v)])),
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
                return Ok(Reply::Decline);
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

/// Does `t` contain the symbol `name` (a small explicit walk)?
fn mentions(h: &mc_core::Heap, t: T<'_>, name: &str) -> bool {
    let s = h.sym_term(h.sym(name));
    let mut stack = vec![t];
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
    // so, not a mesh that ignores it (round 36)
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
        g.push(prims);
        groups.push(e.list(&g));
    }
    let g = e.f(if dim == 2 { "graphics" } else { "graphics3d" }, &[e.list(&groups)]);
    Ok(Reply::Done(cx.eval(e.f("show", &[m, g]))?))
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
