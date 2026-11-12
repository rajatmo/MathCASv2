//! Distances, nearest points and membership for tori, for the unbounded flats and cones (`Region::Affine`) and for
//! unions of pieces (mesh regions).
//!
//! The nearest point of a polyhedral cone p + span(F) + cone(G) is found by enumerating the active sets of G
//! (the subsets whose coefficients are positive at the optimum, as in active-set methods for non-negative least
//! squares: Lawson and Hanson, *Solving Least Squares Problems*, 1974, ch. 23): each candidate is the projection
//! onto p + span(F, S), kept when its coefficients on S are non-negative. The best candidate is chosen with machine
//! numbers and then written exactly from the region's own coordinates (orthogonal projection through the Gram
//! matrix, Cramer's rule for up to two directions).
//!
//! The torus distance is that of the tube around its centre circle: d = |(p - q)| - a with q the nearest point of
//! the circle of radius R (the distance to a circle is sqrt((rho - R)^2 + z^2)).
use crate::ex::{num, nums, points, E};
use crate::metric::{dot, solve, sub, Query};
use crate::props::torus_radii;
use crate::region::{Pt, Radius, Region};
use mc_core::{Store, T};
use mc_eval::{NativeCx, Res};

/// The numeric nearest point of p + span(free) + cone(pos) to x, with the directions used (indices into
/// free ++ pos) and the distance.
fn nearest_numeric(p: &[f64], free: &[Vec<f64>], pos: &[Vec<f64>], x: &[f64]) -> Option<(Vec<f64>, Vec<usize>, f64)> {
    let k = pos.len();
    if k > 12 {
        return None;
    }
    let mut best: Option<(Vec<f64>, Vec<usize>, f64)> = None;
    for mask in 0u32..(1 << k) {
        let mut idx: Vec<usize> = (0..free.len()).collect();
        idx.extend((0..k).filter(|i| mask & (1 << i) != 0).map(|i| free.len() + i));
        let dirs: Vec<&Vec<f64>> = idx.iter().map(|&i| if i < free.len() { &free[i] } else { &pos[i - free.len()] }).collect();
        let lam = if dirs.is_empty() {
            vec![]
        } else {
            let g = dirs.iter().map(|a| dirs.iter().map(|b| dot(a, b)).collect()).collect();
            let r = dirs.iter().map(|a| dot(a, &sub(x, p))).collect();
            match solve(g, r) {
                Some(l) => l,
                None => continue,
            }
        };
        if lam.iter().zip(&idx).any(|(&l, &i)| i >= free.len() && l < -1e-12) {
            continue;
        }
        let mut q = p.to_vec();
        for (l, d) in lam.iter().zip(&dirs) {
            for c in 0..q.len() {
                q[c] += l * d[c];
            }
        }
        let dist = dot(&sub(x, &q), &sub(x, &q)).sqrt();
        if best.as_ref().is_none_or(|b| dist < b.2 - 1e-14 * (1.0 + b.2)) {
            best = Some((q, idx, dist));
        }
    }
    best
}

/// The exact projection of x onto p + span(dirs) (up to two directions, or as many as the dimension).
fn project<'h>(e: E<'h>, p: &[T<'h>], dirs: &[Pt<'h>], x: &[T<'h>]) -> Option<Vec<T<'h>>> {
    let d = e.vsub(x, p);
    Some(match dirs.len() {
        0 => p.to_vec(),
        n if n == p.len() => x.to_vec(),
        1 => {
            let a = &dirs[0];
            e.vadd(p, &e.scale(e.div(e.dot(a, &d), e.norm2(a)), a))
        }
        2 => {
            let (a, b) = (&dirs[0], &dirs[1]);
            let (g11, g12, g22) = (e.norm2(a), e.dot(a, b), e.norm2(b));
            let (b1, b2) = (e.dot(a, &d), e.dot(b, &d));
            let det = e.sub(e.mul(&[g11, g22]), e.sq(g12));
            let l1 = e.div(e.sub(e.mul(&[b1, g22]), e.mul(&[b2, g12])), det);
            let l2 = e.div(e.sub(e.mul(&[b2, g11]), e.mul(&[b1, g12])), det);
            e.vadd(&e.vadd(p, &e.scale(l1, a)), &e.scale(l2, b))
        }
        _ => return None,
    })
}

/// Nearest point (exact when it can be written) and numeric distance of an affine region.
#[allow(clippy::type_complexity)]
fn affine_nearest<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &[T<'h>], free: &[Pt<'h>], pos: &[Pt<'h>], x: &[T<'h>], xn: &[f64]) -> Res<Option<(Option<Vec<T<'h>>>, Vec<f64>, f64)>> {
    let e = E { h: cx.heap() };
    let (Some(pn), Some(fnum), Some(gnum)) = (nums(cx, p)?, points(cx, free)?, points(cx, pos)?) else { return Ok(None) };
    let Some((qn, idx, d)) = nearest_numeric(&pn, &fnum, &gnum, xn) else { return Ok(None) };
    let dirs: Vec<Pt<'h>> = idx.iter().map(|&i| if i < free.len() { free[i].clone() } else { pos[i - free.len()].clone() }).collect();
    Ok(Some((project(e, p, &dirs, x), qn, d)))
}

/// Whether the affine region is full-dimensional with simplicial faces: its boundary faces drop one direction of
/// `pos` each.
fn faces<'h>(p: &[T<'h>], free: &[Pt<'h>], pos: &[Pt<'h>]) -> Option<Vec<(Vec<Pt<'h>>, Vec<Pt<'h>>)>> {
    (free.len() + pos.len() == p.len() && !pos.is_empty()).then(|| {
        (0..pos.len()).map(|j| (free.to_vec(), pos.iter().enumerate().filter(|(i, _)| *i != j).map(|(_, v)| v.clone()).collect())).collect()
    })
}

/// A distance query on a torus, flat, cone or union; `None` when it is not known (a symbolic point).
pub fn query<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, x: &[T<'h>], xn: Option<&[f64]>, q: Query) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    let Some(xn) = xn else { return Ok(None) };
    match r {
        Region::Torus { c, r0, r1, solid } => {
            let (a, big) = torus_radii(e, *r0, *r1);
            let dxy = vec![e.sub(x[0], c[0]), e.sub(x[1], c[1])];
            let rho = e.sqrt(e.norm2(&dxy));
            let dz = e.sub(x[2], c[2]);
            // in the plane of the centre circle the tube distance is |rho - R|
            let tube = match (num(cx, dz)?, num(cx, e.sub(rho, big))?) {
                (Some(0.0), Some(v)) => if v >= 0.0 { e.sub(rho, big) } else { e.sub(big, rho) },
                _ => e.sqrt(e.add(&[e.sq(e.sub(rho, big)), e.sq(dz)])),
            };
            let d = e.sub(tube, a);
            let Some(dv) = num(cx, d)? else { return Ok(None) };
            Ok(Some(match q {
                Query::Nearest => {
                    if *solid && dv <= 0.0 {
                        e.list(x)
                    } else {
                        let Some(rv) = num(cx, rho)? else { return Ok(None) };
                        if rv == 0.0 {
                            return Ok(None);
                        }
                        // the nearest point of the centre circle, then a tube radius towards x
                        let k = e.div(big, rho);
                        let qc = vec![e.add(&[c[0], e.mul(&[k, dxy[0]])]), e.add(&[c[1], e.mul(&[k, dxy[1]])]), c[2]];
                        let dir = e.vsub(x, &qc);
                        e.list(&e.vadd(&qc, &e.scale(e.div(a, tube), &dir)))
                    }
                }
                Query::Signed if *solid => d,
                _ if *solid && dv <= 0.0 => e.int(0),
                _ if dv < 0.0 => e.neg(d),
                _ => d,
            }))
        }
        Region::Affine { p, free, pos } => {
            let Some((near, _, dist)) = affine_nearest(cx, p, free, pos, x, xn)? else { return Ok(None) };
            let inside = dist <= 1e-12 * (1.0 + xn.iter().fold(0.0f64, |a, b| a.max(b.abs())));
            let dterm = |near: &Option<Vec<T<'h>>>| match near {
                Some(v) if !inside => e.dist(x, v),
                _ if inside => e.int(0),
                _ => h.real(dist),
            };
            Ok(Some(match q {
                Query::Nearest => match (near, inside) {
                    (_, true) => e.list(x),
                    (Some(v), _) => e.list(&v),
                    (None, _) => return Ok(None),
                },
                Query::Distance => dterm(&near),
                Query::Signed => {
                    let Some(fs) = faces(p, free, pos).filter(|_| inside) else { return Ok(Some(dterm(&near))) };
                    // inside a full-dimensional cone: minus the distance to the nearest face
                    let mut best: Option<(f64, Option<Vec<T<'h>>>)> = None;
                    for (f, g) in fs {
                        let Some((nf, _, df)) = affine_nearest(cx, p, &f, &g, x, xn)? else { return Ok(None) };
                        if best.as_ref().is_none_or(|b| df < b.0) {
                            best = Some((df, nf));
                        }
                    }
                    match best {
                        Some((_, Some(v))) => e.neg(e.dist(x, &v)),
                        Some((d, None)) => h.real(-d),
                        None => return Ok(None),
                    }
                }
            }))
        }
        Region::Union(parts) => {
            // the part that answers best: least distance (the nearest point of that part)
            let mut best: Option<(f64, T<'h>)> = None;
            let sub_q = if q == Query::Signed && parts.len() > 1 { Query::Distance } else { q };
            for part in parts {
                let d = crate::metric::query(cx, part, x, Query::Distance)?;
                let Some(d) = d else { return Ok(None) };
                let Some(dv) = num(cx, d)? else { return Ok(None) };
                if best.as_ref().is_none_or(|b| dv < b.0) {
                    let Some(ans) = crate::metric::query(cx, part, x, sub_q)? else { return Ok(None) };
                    best = Some((dv, ans));
                }
            }
            Ok(best.map(|b| b.1))
        }
        Region::Revolved { a, b, r, cone } => revolved(cx, a, b, *r, *cone, x, q),
        Region::Ball { c, r: Radius::Axes(rs), solid } => ellipsoid(cx, c, rs, *solid, x, xn, q),
        _ => Ok(None),
    }
}

/// Whether the numeric point x lies in a torus, flat, cone or union.
pub fn member<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, x: &[T<'h>], xn: &[f64]) -> Res<Option<bool>> {
    let scale = 1.0 + xn.iter().fold(0.0f64, |a, b| a.max(b.abs()));
    match r {
        Region::Union(parts) => {
            for part in parts {
                match crate::metric::member(cx, part, x)? {
                    Some(t) => {
                        let v = cx.eval(t)?;
                        if crate::region::call_parts(cx.heap(), v).is_none() && matches!(cx.heap().view(v), mc_core::View::Sym(s) if cx.heap().sym_name(s) == "true") {
                            return Ok(Some(true));
                        }
                    }
                    None => return Ok(None),
                }
            }
            Ok(Some(false))
        }
        Region::Torus { solid, .. } => {
            let Some(d) = query(cx, r, x, Some(xn), Query::Signed)? else { return Ok(None) };
            let Some(dv) = num(cx, d)? else { return Ok(None) };
            Ok(Some(if *solid { dv <= 1e-10 * scale } else { dv.abs() <= 1e-10 * scale }))
        }
        Region::Affine { p, free, pos } => {
            let Some((_, _, d)) = affine_nearest(cx, p, free, pos, x, xn)? else { return Ok(None) };
            Ok(Some(d <= 1e-10 * scale))
        }
        Region::Revolved { .. } => {
            let Some(d) = query(cx, r, x, Some(xn), Query::Distance)? else { return Ok(None) };
            let Some(dv) = num(cx, d)? else { return Ok(None) };
            Ok(Some(dv <= 1e-10 * scale))
        }
        _ => Ok(None),
    }
}

fn is_rational<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
    matches!(h.view(t), mc_core::View::Int(_) | mc_core::View::Rational { .. } | mc_core::View::BigInt { .. })
}

/// The condition n.(x - q) rel 0 (rel `greater_equal` or `equal`) written as a linear relation in the usual
/// form: coefficients cleared of denominators (or made rational where one factor does it), the first coefficient
/// positive, the positive terms on the left and the rest on the right (`3*x >= 4*y`, `x + y <= 1`, `z == 0`).
pub fn linear_condition<'h>(cx: &mut NativeCx<'_, '_, 'h>, n: &[T<'h>], q: &[T<'h>], x: &[T<'h>], rel: &str) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    let mut coef: Vec<T<'h>> = vec![];
    for &c in n {
        coef.push(cx.eval(e.f("simplify", &[c]))?);
    }
    let mut k = cx.eval(e.f("simplify", &[e.neg(e.dot(n, q))]))?;
    let mut vals = vec![];
    for &c in &coef {
        match num(cx, c)? {
            Some(v) => vals.push(v),
            None => return Ok(None),
        }
    }
    let Some(lead) = vals.iter().position(|&v| v != 0.0) else { return Ok(None) };
    let scale_by = |cx: &mut NativeCx<'_, '_, 'h>, coef: &mut Vec<T<'h>>, k: &mut T<'h>, f: T<'h>| -> Res<()> {
        for c in coef.iter_mut() {
            *c = cx.eval(e.f("simplify", &[e.mul(&[*c, f])]))?;
        }
        *k = cx.eval(e.f("simplify", &[e.mul(&[*k, f])]))?;
        Ok(())
    };
    // the first coefficient made 1 (the relation turned when it was negative), then denominators cleared
    let mut rel = rel.to_string();
    if vals[lead] < 0.0 && rel == "greater_equal" {
        rel = "less_equal".into();
    }
    let inv = e.div(e.int(1), coef[lead]);
    scale_by(cx, &mut coef, &mut k, inv)?;
    let all: Vec<T<'h>> = coef.iter().copied().chain([k]).collect();
    let dens = cx.eval(e.f("lcm", &all.iter().map(|&c| e.f("denominator", &[c])).collect::<Vec<_>>()))?;
    if is_rational(h, dens) {
        scale_by(cx, &mut coef, &mut k, dens)?;
    }
    // rational coefficients and an irrational constant: the constant made 1, the terms kept together
    if num(cx, k)? != Some(0.0) && !is_rational(h, k) && coef.iter().all(|&c| is_rational(h, c)) {
        let Some(kv) = num(cx, k)? else { return Ok(None) };
        let terms: Vec<T<'h>> = coef.iter().zip(x).filter(|(c, _)| crate::ex::lit(h, **c) != Some(0.0)).map(|(&c, &xi)| e.mul(&[c, xi])).collect();
        if !terms.is_empty() {
            let f = cx.eval(e.f("simplify", &[e.div(e.int(1), e.f("abs", &[k]))]))?;
            let lhs = e.mul(&[f, e.add(&terms)]);
            // sum + k rel 0: sum rel -k, divided by |k|
            let rhs = e.int(if kv < 0.0 { 1 } else { -1 });
            return Ok(Some(e.f(&rel, &[lhs, rhs])));
        }
    }
    let (mut lhs, mut rhs) = (vec![], vec![]);
    for (&c, &xi) in coef.iter().zip(x) {
        match num(cx, c)? {
            Some(v) if v > 0.0 => lhs.push(e.mul(&[c, xi])),
            Some(v) if v < 0.0 => rhs.push(e.mul(&[e.neg(c), xi])),
            _ => {}
        }
    }
    if num(cx, k)? != Some(0.0) {
        rhs.push(e.neg(k));
    }
    let side = |v: Vec<T<'h>>| match v.len() {
        0 => e.int(0),
        1 => v[0],
        _ => e.add(&v),
    };
    Ok(Some(e.f(&rel, &[side(lhs), side(rhs)])))
}

// ---- cylinders and cones

/// A query on a solid of revolution through its meridian section: the axial coordinate s = (x - a).u and the
/// distance t from the axis place x in the half-plane of the section, which is the rectangle [0, h] x [-r, r]
/// (cylinder) or the triangle (0, -r), (0, r), (h, 0) (cone); the section's answer is the solid's (the section is
/// symmetric, so its nearest point to (s, t) with t >= 0 has t >= 0 too).
fn revolved<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], b: &[T<'h>], r: T<'h>, cone: bool, x: &[T<'h>], q: Query) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    if a.len() != x.len() {
        return Ok(None);
    }
    let axis = e.vsub(b, a);
    let len = cx.eval(e.sqrt(e.norm2(&axis)))?;
    let d = e.vsub(x, a);
    let s = cx.eval(e.f("simplify", &[e.div(e.dot(&d, &axis), len)]))?;
    let t = cx.eval(e.f("simplify", &[e.sqrt(e.sub(e.norm2(&d), e.sq(s)))]))?;
    let section = if cone {
        Region::Simplex(vec![vec![e.int(0), e.neg(r)], vec![e.int(0), r], vec![len, e.int(0)]])
    } else {
        Region::Boxed { lo: vec![e.int(0), e.neg(r)], hi: vec![len, r] }
    };
    let ans = crate::metric::query(cx, &section, &[s, t], q)?;
    let Some(ans) = ans else { return Ok(None) };
    if q != Query::Nearest {
        return Ok(Some(ans));
    }
    // back to space: a + s' u + t' w, w the unit vector from the axis towards x
    let ans = cx.eval(ans)?;
    let Some(&[s1, t1]) = cx.as_list(ans) else { return Ok(None) };
    let u: Vec<T<'h>> = axis.iter().map(|&c| e.div(c, len)).collect();
    let radial = e.vsub(&d, &e.scale(s, &u));
    let w: Vec<T<'h>> = match num(cx, t)? {
        Some(tv) if tv > 0.0 => radial.iter().map(|&c| e.div(c, t)).collect(),
        _ => {
            if num(cx, t1)? != Some(0.0) {
                return Ok(None);
            }
            vec![e.int(0); x.len()]
        }
    };
    let p = e.vadd(&e.vadd(a, &e.scale(s1, &u)), &e.scale(t1, &w));
    let mut out = vec![];
    for c in p {
        out.push(cx.eval(e.f("simplify", &[c]))?);
    }
    Ok(Some(e.list(&out)))
}

// ---- ellipses and ellipsoids

/// A rational number near v with a denominator at most `max` (continued fractions), when it is within 1e-12.
fn rational_near(v: f64, max: i64) -> Option<(i64, i64)> {
    if !v.is_finite() || v.abs() > 1e9 {
        return None;
    }
    let (mut h0, mut h1, mut k0, mut k1) = (0i64, 1i64, 1i64, 0i64);
    let mut x = v;
    for _ in 0..40 {
        let a = x.floor();
        let ai = a as i64;
        let (h2, k2) = (ai.checked_mul(h1)?.checked_add(h0)?, ai.checked_mul(k1)?.checked_add(k0)?);
        if k2 > max {
            break;
        }
        (h0, h1, k0, k1) = (h1, h2, k1, k2);
        if (v - h1 as f64 / k1 as f64).abs() <= 1e-12 * (1.0 + v.abs()) {
            return Some((h1, k1));
        }
        let f = x - a;
        if f.abs() < 1e-15 {
            break;
        }
        x = 1.0 / f;
    }
    None
}

/// The nearest point of the ellipsoid surface sum (y_i / r_i)^2 = 1 to d (coordinates from the centre), by the
/// root t of sum (r_i d_i / (r_i^2 + t))^2 = 1 on (-r_min^2, inf) with y_i = r_i^2 d_i / (r_i^2 + t), and the
/// degenerate case where no root exists there (t = -r_min^2: d lies in a symmetry plane, inside); D. Eberly,
/// "Distance from a point to an ellipse, an ellipsoid, or a hyperellipsoid" (Geometric Tools, 2013).
fn ellipsoid_surface_nearest(r: &[f64], d: &[f64]) -> Option<(f64, Vec<f64>)> {
    let n = r.len();
    let kmin = (0..n).min_by(|&i, &j| r[i].total_cmp(&r[j]))?;
    let rmin2 = r[kmin] * r[kmin];
    let f = |t: f64| -> f64 { (0..n).map(|i| (r[i] * d[i] / (r[i] * r[i] + t)).powi(2)).sum::<f64>() - 1.0 };
    let lo = -rmin2;
    // degenerate: the component along the shortest axis is zero and the root is not in range
    if d[kmin] == 0.0 {
        let lim: f64 = (0..n).filter(|&i| i != kmin).map(|i| (r[i] * d[i] / (r[i] * r[i] - rmin2)).powi(2)).sum();
        if lim.is_finite() && lim < 1.0 {
            let mut y: Vec<f64> = (0..n).map(|i| if i == kmin { 0.0 } else { r[i] * r[i] * d[i] / (r[i] * r[i] - rmin2) }).collect();
            y[kmin] = r[kmin] * (1.0 - lim).sqrt();
            return Some((lo, y));
        }
    }
    let (mut a, mut b) = (lo + 1e-15 * (1.0 + rmin2), 1.0f64);
    while f(b) > 0.0 {
        b *= 2.0;
        if b > 1e300 {
            return None;
        }
    }
    if f(a) < 0.0 {
        return None;
    }
    for _ in 0..200 {
        let m = 0.5 * (a + b);
        if f(m) > 0.0 {
            a = m;
        } else {
            b = m;
        }
    }
    let t = 0.5 * (a + b);
    Some((t, (0..n).map(|i| r[i] * r[i] * d[i] / (r[i] * r[i] + t)).collect()))
}

/// Distance queries on an ellipse or ellipsoid with radii per axis: exact when the multiplier t is rational (the
/// usual case for the documented examples), else machine numbers.
fn ellipsoid<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &[T<'h>], rs: &[T<'h>], solid: bool, x: &[T<'h>], xn: &[f64], q: Query) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    let (Some(cn), Some(rn)) = (nums(cx, c)?, nums(cx, rs)?) else { return Ok(None) };
    if rn.iter().any(|&r| r <= 0.0) {
        return Ok(None);
    }
    let d: Vec<f64> = xn.iter().zip(&cn).map(|(a, b)| a - b).collect();
    let level: f64 = d.iter().zip(&rn).map(|(a, r)| (a / r).powi(2)).sum();
    let inside = level <= 1.0 + 1e-14;
    if solid && inside && q != Query::Signed {
        return Ok(Some(if q == Query::Nearest { e.list(x) } else { e.int(0) }));
    }
    let Some((t, y)) = ellipsoid_surface_nearest(&rn, &d) else { return Ok(None) };
    let exact = rs.iter().chain(c).chain(x).all(|&v| crate::ex::lit(h, v).is_some() && !crate::ex::inexact(h, v));
    let near: Vec<T<'h>> = match rational_near(t, 10_000).filter(|_| exact) {
        Some((p, qd)) => {
            let tt = e.rat(p, qd);
            let kmin = (0..rn.len()).min_by(|&i, &j| rn[i].total_cmp(&rn[j])).unwrap_or(0);
            let degenerate = (t + rn[kmin] * rn[kmin]).abs() < 1e-12 * (1.0 + t.abs()) && d[kmin] == 0.0;
            let mut out: Vec<T<'h>> = (0..rn.len()).map(|i| {
                let di = e.sub(x[i], c[i]);
                if degenerate && i == kmin { e.int(0) } else { e.div(e.mul(&[e.sq(rs[i]), di]), e.add(&[e.sq(rs[i]), tt])) }
            }).collect();
            if degenerate {
                let rest: Vec<T<'h>> = (0..rn.len()).filter(|&i| i != kmin).map(|i| e.sq(e.div(out[i], rs[i]))).collect();
                out[kmin] = e.mul(&[rs[kmin], e.sqrt(e.sub(e.int(1), e.add(&rest)))]);
            }
            let mut res = vec![];
            for (i, v) in out.into_iter().enumerate() {
                res.push(cx.eval(e.add(&[c[i], v]))?);
            }
            res
        }
        None => y.iter().zip(&cn).map(|(a, b)| h.real(a + b)).collect(),
    };
    let dist = e.dist(x, &near);
    Ok(Some(match q {
        Query::Nearest => e.list(&near),
        Query::Distance => dist,
        Query::Signed if solid && inside => e.neg(dist),
        Query::Signed => dist,
    }))
}
