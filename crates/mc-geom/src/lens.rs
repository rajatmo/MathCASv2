//! Intersections of round regions with exact data: the arc of a circle inside a disk (the region itself) and the
//! volume of the intersection of two balls of space.
//!
//! The arc: a circle of centre c2 and radius r2 crosses the circle of the disk (c1, r1) at the two points seen from
//! c2 at the angles phi -+ alpha, where phi is the direction from c2 to c1 and, by the law of cosines in the
//! triangle of the two centres and a crossing point, cos alpha = (d^2 + r2^2 - r1^2) / (2 d r2) with d = |c1 - c2|.
//! The arc between them faces c1, so it is the part inside the disk.
//!
//! The volume: two spherical caps, whose sum is pi (r1 + r2 - d)^2 (d^2 + 2 d (r1 + r2) - 3 (r1 - r2)^2) / (12 d)
//! (E. W. Weisstein, "Sphere-Sphere Intersection", MathWorld; it follows from the cap volume
//! pi h^2 (3 r - h) / 3 with the cap heights h1 = (r2 - r1 + d)(r2 + r1 - d) / (2 d) and its mirror).
//!
//! Moments of a disk or ball under an affine map: by the change of variables x = L u + b, the moment integral over
//! the image is |det L| times the integral of the mapped monomial over the disk or ball.
use crate::ex::{inexact, num, E};
use crate::region::{read, Radius, Region};
use mc_core::{Store, T};
use mc_eval::{NativeCx, Res};

/// A round region with exact numeric data: centre, radius, whether it is solid.
struct Round<'h> {
    c: Vec<T<'h>>,
    r: T<'h>,
    solid: bool,
}

fn round<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, dim: usize) -> Res<Option<Round<'h>>> {
    let h = cx.heap();
    if inexact(h, t) {
        return Ok(None);
    }
    let Some(Region::Ball { c, r: Radius::One(r), solid }) = read(cx, t) else { return Ok(None) };
    if c.len() != dim || !num(cx, r)?.is_some_and(|v| v > 0.0) {
        return Ok(None);
    }
    for &x in &c {
        if num(cx, x)?.is_none() {
            return Ok(None);
        }
    }
    Ok(Some(Round { c, r, solid }))
}

/// Whether `a` and `b` are apart by more than rounding (so that comparing their machine values decides the exact
/// order).
fn apart(a: f64, b: f64) -> bool {
    (a - b).abs() > 1e-9 * (1.0 + a.abs().max(b.abs()))
}

/// `region_intersection(disk, circle)` (either order) of the plane with exact data: the arc of the circle inside the
/// disk as `circle(c, r, [t0, t1])` with 0 <= t0 < 2 pi, the whole circle when it lies in the disk, `empty_region(2)`
/// when they do not meet. `None` for anything else, and for touching circles (decided only when exact).
pub fn disk_circle<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [p, q] = a else { return Ok(None) };
    let (Some(p), Some(q)) = (round(cx, *p, 2)?, round(cx, *q, 2)?) else { return Ok(None) };
    let (disk, circle) = match (p.solid, q.solid) {
        (true, false) => (p, q),
        (false, true) => (q, p),
        _ => return Ok(None),
    };
    let h = cx.heap();
    let e = E { h };
    let dd = cx.eval(e.f("simplify", &[e.dist(&disk.c, &circle.c)]))?;
    let (Some(d), Some(r1), Some(r2)) = (num(cx, dd)?, num(cx, disk.r)?, num(cx, circle.r)?) else { return Ok(None) };
    let whole = || e.f("circle", &[e.list(&circle.c), circle.r]);
    let empty = || e.f("empty_region", &[e.int(2)]);
    if d == 0.0 {
        return Ok(apart(r1, r2).then(|| if r2 < r1 { whole() } else { empty() }));
    }
    if !apart(d + r2, r1) || !apart(d, r1 + r2) || !apart(d + r1, r2) {
        return Ok(None);
    }
    if d + r2 < r1 {
        return Ok(Some(whole()));
    }
    if d > r1 + r2 || d + r1 < r2 {
        return Ok(Some(empty()));
    }
    let to = e.vsub(&disk.c, &circle.c);
    let i = e.f(mc_api::names::COMPLEX, &[e.int(0), e.int(1)]);
    let phi = e.f("arg", &[e.add(&[to[0], e.mul(&[i, to[1]])])]);
    let alpha = e.f("acos", &[e.div(e.sub(e.add(&[e.sq(dd), e.sq(circle.r)]), e.sq(disk.r)), e.mul(&[e.int(2), dd, circle.r]))]);
    let mut t0 = cx.eval(e.f("simplify", &[e.sub(phi, alpha)]))?;
    let mut t1 = cx.eval(e.f("simplify", &[e.add(&[phi, alpha])]))?;
    let Some(v0) = num(cx, t0)? else { return Ok(None) };
    if !apart(v0, 0.0) {
        return Ok(None);
    }
    if v0 < 0.0 {
        let tau = e.mul(&[e.int(2), e.pi()]);
        t0 = cx.eval(e.f("simplify", &[e.add(&[t0, tau])]))?;
        t1 = cx.eval(e.f("simplify", &[e.add(&[t1, tau])]))?;
    }
    Ok(Some(e.f("circle", &[e.list(&circle.c), circle.r, e.list(&[t0, t1])])))
}

/// `region_moment(transformed_region(reg, transformation_function(m)), [e1, ..., en])` for an affine m with a
/// nonsingular linear part L and translation b, and a ball or disk `reg` with exact data: by the change of
/// variables x = L u + b, the integral of prod (L u + b)_k^e_k over `reg` times |det L|. `None` when the map is not
/// such, or the integral is not found.
pub fn mapped_moment<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    let [t, ex] = a else { return Ok(None) };
    let Some(("transformed_region", &[r, tf])) = crate::region::call_parts(h, *t).as_ref().map(|(n, x)| (n.as_str(), *x)) else { return Ok(None) };
    let Some(("transformation_function", &[m])) = crate::region::call_parts(h, tf).as_ref().map(|(n, x)| (n.as_str(), *x)) else { return Ok(None) };
    let Some(exps) = cx.as_list(*ex).map(|l| l.to_vec()) else { return Ok(None) };
    let n = exps.len();
    if !(2..=3).contains(&n) || !exps.iter().all(|&k| matches!(h.view(k), mc_core::View::Int(v) if (0..=12).contains(&v))) {
        return Ok(None);
    }
    if round(cx, r, n)?.is_none_or(|b| !b.solid) {
        return Ok(None);
    }
    let Some(rows) = cx.as_list(m).map(|l| l.to_vec()) else { return Ok(None) };
    if rows.len() != n + 1 {
        return Ok(None);
    }
    let mut mat = vec![];
    for row in &rows {
        match cx.as_list(*row) {
            Some(x) if x.len() == n + 1 && !x.iter().any(|&y| inexact(h, y)) => mat.push(x.to_vec()),
            _ => return Ok(None),
        }
    }
    // an affine map: the last row is (0, ..., 0, 1)
    let last = mat[n].clone();
    for (k, &x) in last.iter().enumerate() {
        if num(cx, x)? != Some(if k == n { 1.0 } else { 0.0 }) {
            return Ok(None);
        }
    }
    let l: Vec<T<'h>> = mat[..n].iter().map(|row| e.list(&row[..n])).collect();
    let det = cx.eval(e.f("det", &[e.list(&l)]))?;
    if !num(cx, det)?.is_some_and(|v| v != 0.0) {
        return Ok(None);
    }
    let vars: Vec<T<'h>> = (0..n).map(|k| h.sym_term(h.sym(&format!("moment$u{k}")))).collect();
    let mut factors = vec![];
    for (k, &p) in exps.iter().enumerate() {
        let mut terms: Vec<T<'h>> = (0..n).map(|j| e.mul(&[mat[k][j], vars[j]])).collect();
        terms.push(mat[k][n]);
        factors.push(e.pow(e.add(&terms), p));
    }
    let integrand = e.f("expand", &[e.mul(&factors)]);
    let v = cx.eval(e.f("integrate", &[integrand, e.f("element", &[e.list(&vars), r])]))?;
    if mentions(h, v, "integrate") || mentions(h, v, "failure") || vars.iter().any(|&u| mentions_term(h, v, u)) {
        return Ok(None);
    }
    Ok(Some(cx.eval(e.f("simplify", &[e.mul(&[e.f("abs", &[det]), v])]))?))
}

fn mentions(h: &mc_core::Heap, t: T<'_>, name: &str) -> bool {
    match h.view(t) {
        mc_core::View::Sym(s) => h.sym_name(s) == name,
        mc_core::View::App { head, args } => mentions(h, head, name) || args.iter().any(|&a| mentions(h, a, name)),
        _ => false,
    }
}

fn mentions_term(h: &mc_core::Heap, t: T<'_>, u: T<'_>) -> bool {
    t == u
        || match h.view(t) {
            mc_core::View::App { head, args } => mentions_term(h, head, u) || args.iter().any(|&a| mentions_term(h, a, u)),
            _ => false,
        }
}

/// The volume of `region_intersection(ball(c1, r1), ball(c2, r2))` of space with exact data: 0 apart, the smaller
/// ball when one lies in the other, else the two caps. `None` for anything else.
pub fn ball_lens_volume<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [p, q] = a else { return Ok(None) };
    let (Some(p), Some(q)) = (round(cx, *p, 3)?, round(cx, *q, 3)?) else { return Ok(None) };
    if !p.solid || !q.solid {
        return Ok(None);
    }
    let h = cx.heap();
    let e = E { h };
    let dd = cx.eval(e.f("simplify", &[e.dist(&p.c, &q.c)]))?;
    let (Some(d), Some(a1), Some(a2)) = (num(cx, dd)?, num(cx, p.r)?, num(cx, q.r)?) else { return Ok(None) };
    let (r1, r2) = (p.r, q.r);
    let v = if d >= a1 + a2 {
        e.int(0)
    } else if d <= (a1 - a2).abs() {
        let r = if a1 <= a2 { r1 } else { r2 };
        e.mul(&[e.rat(4, 3), e.pi(), e.pow(r, e.int(3))])
    } else {
        let gap = e.sq(e.sub(e.add(&[r1, r2]), dd));
        let rest = e.sub(e.add(&[e.sq(dd), e.mul(&[e.int(2), dd, e.add(&[r1, r2])])]), e.mul(&[e.int(3), e.sq(e.sub(r1, r2))]));
        e.div(e.mul(&[e.pi(), gap, rest]), e.mul(&[e.int(12), dd]))
    };
    Ok(Some(cx.eval(e.f("simplify", &[v]))?))
}
