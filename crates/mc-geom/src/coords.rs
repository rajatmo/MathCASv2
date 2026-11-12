//! Measures of a region written as a map and its ranges (round 33): `arc_length(map, [t, a, b])`,
//! `area(map, [u, a, b], [v, c, d])`, `volume(map, [u, ..], [v, ..], [w, ..])`, each with an optional coordinate
//! system `"Polar"`, `"Cylindrical"` or `"Spherical"` naming the coordinates of the map, and `perimeter(map, [u, a,
//! b], [v, c, d])` (also of a `parametric_region`).
//!
//! A map with its ranges is the `parametric_region(map, ranges)` of the same ranges; the system turns the map into
//! cartesian coordinates first (polar `[r, t]` is `[r cos t, r sin t]`, cylindrical `[r, t, z]` is `[r cos t,
//! r sin t, z]`, spherical `[r, t, p]` with `t` the polar angle is `[r sin t cos p, r sin t sin p, r cos t]`).
//! The perimeter of the image of a rectangle of parameters is the sum of the lengths of its four edges, an edge
//! that coincides with the opposite one (the seam of a full turn) being inside the region and not counted.
use crate::ex::E;
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The ranges `[v, lo, hi]` among the arguments and the coordinate system named by a trailing string.
fn ranges_of<'h>(cx: &NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Option<(Vec<[T<'h>; 3]>, Option<String>)> {
    let h = cx.heap();
    let mut rest = &a[1..];
    let mut system = None;
    if let Some((&last, init)) = rest.split_last() {
        if let View::Str(s) = h.view(last) {
            system = Some(s.to_string());
            rest = init;
        }
    }
    if rest.is_empty() {
        return None;
    }
    let mut out = vec![];
    for &r in rest {
        let &[v, lo, hi] = cx.as_list(r)? else { return None };
        if !matches!(h.view(v), View::Sym(_)) {
            return None;
        }
        out.push([v, lo, hi]);
    }
    Some((out, system))
}

/// The map in cartesian coordinates, evaluated; `None` for an unknown system or a map of the wrong length.
fn cartesian<'h>(cx: &mut NativeCx<'_, '_, 'h>, map: &[T<'h>], system: Option<&str>) -> Res<Option<Vec<T<'h>>>> {
    let e = E { h: cx.heap() };
    let out: Vec<T<'h>> = match (system, map) {
        (None | Some("Cartesian"), m) => m.to_vec(),
        (Some("Polar"), &[r, t]) => vec![e.mul(&[r, e.f("cos", &[t])]), e.mul(&[r, e.f("sin", &[t])])],
        (Some("Cylindrical"), &[r, t, z]) => vec![e.mul(&[r, e.f("cos", &[t])]), e.mul(&[r, e.f("sin", &[t])]), z],
        (Some("Spherical"), &[r, t, p]) => vec![
            e.mul(&[r, e.f("sin", &[t]), e.f("cos", &[p])]),
            e.mul(&[r, e.f("sin", &[t]), e.f("sin", &[p])]),
            e.mul(&[r, e.f("cos", &[t])]),
        ],
        _ => return Ok(None),
    };
    let mut ev = Vec::with_capacity(out.len());
    for t in out {
        ev.push(cx.eval(t)?);
    }
    Ok(Some(ev))
}

/// The call `name(map, ranges...)` as the call of `name` on the parametric region of the map, when its arguments
/// have that shape; `None` otherwise (the arguments are a region, or something else).
pub fn parametric<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, a: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    if a.len() < 2 {
        return Ok(None);
    }
    let Some(map) = cx.as_list(a[0]).map(|m| m.to_vec()) else { return Ok(None) };
    // a list of points is a region, not a map
    if map.iter().any(|&c| cx.as_list(c).is_some()) {
        return Ok(None);
    }
    let Some((ranges, system)) = ranges_of(cx, a) else { return Ok(None) };
    let Some(map) = cartesian(cx, &map, system.as_deref())? else { return Ok(None) };
    let specs: Vec<T<'h>> = ranges.iter().map(|r| cx.list(r)).collect();
    let region = if specs.len() == 1 { e.f("parametric_region", &[cx.list(&map), specs[0]]) } else { e.f("parametric_region", &[cx.list(&map), cx.list(&specs)]) };
    Ok(Some(e.f(name, &[region])))
}

/// `perimeter(map, [u, a, b], [v, c, d], system?)` and `perimeter(parametric_region(map, [[u, a, b], [v, c, d]]))`.
pub fn perimeter<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    let (map, ranges): (Vec<T<'h>>, Vec<[T<'h>; 3]>) = if let [t] = a {
        let View::App { head, args: &[map, specs] } = h.view(*t) else { return Ok(None) };
        if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "parametric_region") {
            return Ok(None);
        }
        let Some(map) = cx.as_list(map).map(|m| m.to_vec()) else { return Ok(None) };
        let Some(items) = cx.as_list(specs).map(|l| l.to_vec()) else { return Ok(None) };
        let mut rs = vec![];
        for r in items {
            let Some(&[v, lo, hi]) = cx.as_list(r) else { return Ok(None) };
            rs.push([v, lo, hi]);
        }
        (map, rs)
    } else {
        if a.len() < 3 {
            return Ok(None);
        }
        let Some(map) = cx.as_list(a[0]).map(|m| m.to_vec()) else { return Ok(None) };
        if map.iter().any(|&c| cx.as_list(c).is_some()) {
            return Ok(None);
        }
        let Some((ranges, system)) = ranges_of(cx, a) else { return Ok(None) };
        let Some(map) = cartesian(cx, &map, system.as_deref())? else { return Ok(None) };
        (map, ranges)
    };
    // the image of a rectangle of two parameters in the plane
    if ranges.len() != 2 || map.len() != 2 {
        return Ok(None);
    }
    let sub = |cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>, x: T<'h>| -> Res<Vec<T<'h>>> {
        let rule = e.f(mc_api::names::RULE, &[v, x]);
        let mut out = vec![];
        for &c in &map {
            out.push(cx.eval(e.f("rewrite", &[c, rule]))?);
        }
        Ok(out)
    };
    // the edges: one parameter fixed at an end, the other running over its range
    let mut edges: Vec<(Vec<T<'h>>, T<'h>)> = vec![];
    for (fixed, free) in [(0usize, 1usize), (1, 0)] {
        for end in [ranges[fixed][1], ranges[fixed][2]] {
            let curve = sub(cx, ranges[fixed][0], end)?;
            edges.push((curve, cx.list(&ranges[free])));
        }
    }
    let same = |cx: &mut NativeCx<'_, '_, 'h>, p: &[T<'h>], q: &[T<'h>]| -> Res<bool> {
        for (&x, &y) in p.iter().zip(q) {
            let d = cx.eval(e.f("simplify", &[e.add(&[x, e.mul(&[e.int(-1), y])])]))?;
            if d != e.int(0) {
                return Ok(false);
            }
        }
        Ok(true)
    };
    // opposite edges (0, 1) and (2, 3) that coincide as curves are seams
    let mut counted = [true; 4];
    if edges[0].1 == edges[1].1 && same(cx, &edges[0].0, &edges[1].0)? {
        counted[0] = false;
        counted[1] = false;
    }
    if edges[2].1 == edges[3].1 && same(cx, &edges[2].0, &edges[3].0)? {
        counted[2] = false;
        counted[3] = false;
    }
    let mut total = vec![];
    for (k, (curve, range)) in edges.iter().enumerate() {
        if !counted[k] {
            continue;
        }
        // a closed curve run several times over (the astroid for theta in [0, 4 pi]): the boundary of the point set
        // is one period, the largest k <= 12 with curve(t + L/k) = curve(t) (round 36)
        let mut range = *range;
        if let Some(&[v, lo, hi]) = cx.as_list(range) {
            let span = cx.eval(e.add(&[hi, e.mul(&[e.int(-1), lo])]))?;
            for k in (2..=12i64).rev() {
                let step = cx.eval(e.mul(&[span, e.f(mc_api::names::POW, &[e.int(k), e.int(-1)])]))?;
                let shifted = sub_curve(cx, &e, curve, v, e.add(&[v, step]))?;
                // compared at three points (simplify does not reduce cos(t + 2 pi) to cos(t))
                let mut periodic = true;
                for t in [0.3711, 1.2345, 2.6417] {
                    let a = sub_curve(cx, &e, curve, v, h.real(t))?;
                    let b = sub_curve(cx, &e, &shifted, v, h.real(t))?;
                    for (&x, &y) in a.iter().zip(&b) {
                        let d = cx.eval(e.f("n", &[e.add(&[x, e.mul(&[e.int(-1), y])])]))?;
                        let s = cx.eval(e.f("n", &[x]))?;
                        match (h.view(d), h.view(s)) {
                            (View::Real(d), View::Real(s)) if d.abs() <= 1e-12 * (1.0 + s.abs()) => {}
                            (View::Int(0), _) => {}
                            _ => periodic = false,
                        }
                    }
                }
                if periodic {
                    let top = cx.eval(e.add(&[lo, step]))?;
                    range = cx.list(&[v, lo, top]);
                    break;
                }
            }
        }
        let region = e.f("parametric_region", &[cx.list(curve), range]);
        let len = cx.eval(e.f("arc_length", &[region]))?;
        if matches!(h.view(len), View::App { head, .. } if head == h.sym_term(h.sym("arc_length"))) {
            return Ok(None);
        }
        total.push(len);
    }
    Ok(Some(cx.eval(e.f("simplify", &[e.add(&total)]))?))
}

/// The curve with the variable `v` replaced by `x`.
fn sub_curve<'h>(cx: &mut NativeCx<'_, '_, 'h>, e: &E<'h>, curve: &[T<'h>], v: T<'h>, x: T<'h>) -> Res<Vec<T<'h>>> {
    let rule = e.f(mc_api::names::RULE, &[v, x]);
    let mut out = vec![];
    for &c in curve {
        out.push(cx.eval(e.f("rewrite", &[c, rule]))?);
    }
    Ok(out)
}

/// Reply for a measure call whose arguments are a map and ranges.
pub fn reply<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, a: &[T<'h>]) -> Res<Option<Reply<'h>>> {
    Ok(parametric(cx, name, a)?.map(Reply::Again))
}
