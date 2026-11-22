//! Implicit regions that are disks, circles, balls, spheres or axis-aligned ellipses and ellipsoids: a condition
//! `p <= 0`, `p < 0` or `p == 0` (either side, or `>=` and `>` turned round) where p is a polynomial of degree two in
//! the region's two or three coordinates without cross terms, with exact numeric coefficients whose squares all
//! have one sign. Completing the squares, p = sum a_i (x_i - c_i)^2 + k with every a_i > 0; when -k > 0 the region
//! is the solid (or the surface) with centre c and semi-axes sqrt(-k / a_i). The identity is checked by expanding
//! the difference of the two sides, so a polynomial of another form is never taken for one.
use crate::ex::{inexact, num, E};
use crate::region::{call_parts, read, Region};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The disk, circle, ball, sphere (axes as a list when they differ) an implicit region term is, or `None`.
pub fn quadric<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    if !call_parts(h, t).is_some_and(|(n, _)| n == "implicit_region") || inexact(h, t) {
        return Ok(None);
    }
    let Some(Region::Implicit { cond, vars }) = read(cx, t) else { return Ok(None) };
    let n = vars.len();
    if !(2..=3).contains(&n) {
        return Ok(None);
    }
    let Some((rel, args)) = call_parts(h, cond) else { return Ok(None) };
    let [l, r] = args else { return Ok(None) };
    // the relation as p (<=, <, ==) 0
    let (p, surface) = match rel.as_str() {
        "less_equal" | "less" => (e.sub(*l, *r), false),
        "greater_equal" | "greater" => (e.sub(*r, *l), false),
        "equal" => (e.sub(*l, *r), true),
        _ => return Ok(None),
    };
    let p = cx.eval(e.f("expand", &[p]))?;
    let coef = |cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, x2: T<'h>, k: i64| -> Res<T<'h>> { cx.eval(e.f("coefficient", &[x, x2, e.int(k)])) };
    let mut a = vec![];
    let mut b = vec![];
    for &x in &vars {
        a.push(coef(cx, p, x, 2)?);
        b.push(coef(cx, p, x, 1)?);
    }
    let zero: Vec<T<'h>> = vars.iter().map(|&x| e.f("rule", &[x, e.int(0)])).collect();
    let c = cx.eval(e.f("rewrite", &[p, e.list(&zero)]))?;
    // exact numbers only (no coordinate left in a coefficient)
    let mut av = vec![];
    for &x in a.iter().chain(&b).chain(std::iter::once(&c)) {
        let Some(v) = num(cx, x)? else { return Ok(None) };
        av.push(v);
    }
    // the polynomial is exactly sum a_i x_i^2 + b_i x_i + c
    let rebuilt = e.add(&(0..n).flat_map(|i| [e.mul(&[a[i], e.sq(vars[i])]), e.mul(&[b[i], vars[i]])]).chain(std::iter::once(c)).collect::<Vec<_>>());
    let diff = cx.eval(e.f("expand", &[e.sub(p, rebuilt)]))?;
    if !matches!(h.view(diff), View::Int(0)) {
        return Ok(None);
    }
    let signs: Vec<f64> = av[..n].to_vec();
    let (p_neg, a, b, c) = if signs.iter().all(|&s| s > 0.0) {
        (false, a, b, c)
    } else if signs.iter().all(|&s| s < 0.0) {
        (true, a.iter().map(|&x| e.neg(x)).collect(), b.iter().map(|&x| e.neg(x)).collect(), e.neg(c))
    } else {
        return Ok(None);
    };
    // p <= 0 with p negated is the outside of the quadric: unbounded
    if p_neg && !surface {
        return Ok(None);
    }
    let centre: Vec<T<'h>> = (0..n).map(|i| e.div(e.neg(b[i]), e.mul(&[e.int(2), a[i]]))).collect();
    let k = e.sub(c, e.add(&(0..n).map(|i| e.div(e.sq(b[i]), e.mul(&[e.int(4), a[i]]))).collect::<Vec<_>>()));
    let rhs = cx.eval(e.neg(k))?;
    if !num(cx, rhs)?.is_some_and(|v| v > 0.0) {
        return Ok(None);
    }
    let mut radii = vec![];
    for &ai in &a {
        radii.push(cx.eval(e.f("simplify", &[e.sqrt(e.div(rhs, ai))]))?);
    }
    let centre: Vec<T<'h>> = centre.into_iter().map(|x| cx.eval(x)).collect::<Res<_>>()?;
    let radius = if radii.iter().all(|&r| r == radii[0]) { radii[0] } else { e.list(&radii) };
    let name = match (n, surface) {
        (2, false) => "disk",
        (2, true) => "circle",
        (3, false) => "ball",
        _ => "sphere",
    };
    Ok(Some(e.f(name, &[e.list(&centre), radius])))
}

/// `ellipsoid(c, m)` for a symmetric positive definite matrix m of exact numbers (two or three coordinates), as the
/// image `transformed_region(disk or ball, transformation_function)` of the unit disk or ball under x -> L x + c with
/// L the Cholesky factor of m (L L^T = m, L lower triangular with a positive diagonal: the region
/// (x - c)^T m^-1 (x - c) <= 1 is {L u + c : |u| <= 1}). The term is not evaluated (that would give the ellipsoid
/// back): it is for the functions that measure mapped disks and balls. `None` for other terms.
pub fn ellipsoid_as_mapped<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    let Some(("ellipsoid", &[c, m])) = call_parts(h, t).as_ref().map(|(n, a)| (n.as_str(), *a)) else { return Ok(None) };
    if inexact(h, t) {
        return Ok(None);
    }
    let (Some(c), Some(rows)) = (cx.as_list(c).map(|l| l.to_vec()), cx.as_list(m).map(|l| l.to_vec())) else { return Ok(None) };
    let n = c.len();
    if !(2..=3).contains(&n) || rows.len() != n {
        return Ok(None);
    }
    let mut a = vec![];
    for &r in &rows {
        match cx.as_list(r) {
            Some(x) if x.len() == n => a.push(x.to_vec()),
            _ => return Ok(None),
        }
    }
    // exact numbers, symmetric
    let entries: Vec<(T<'h>, T<'h>)> = (0..n).flat_map(|i| (0..n).map(move |j| (i, j))).map(|(i, j)| (a[i][j], a[j][i])).collect();
    for (x, y) in entries {
        if num(cx, x)?.is_none() || num(cx, e.sub(x, y))? != Some(0.0) {
            return Ok(None);
        }
    }
    // Cholesky: L_jj = sqrt(m_jj - sum_k<j L_jk^2), L_ij = (m_ij - sum_k<j L_ik L_jk) / L_jj for i > j
    let mut l = vec![vec![e.int(0); n]; n];
    for j in 0..n {
        let s = e.sub(a[j][j], e.add(&(0..j).map(|k| e.sq(l[j][k])).collect::<Vec<_>>()));
        let s = cx.eval(e.f("simplify", &[s]))?;
        if !num(cx, s)?.is_some_and(|v| v > 0.0) {
            return Ok(None);
        }
        l[j][j] = cx.eval(e.f("simplify", &[e.sqrt(s)]))?;
        for i in j + 1..n {
            let s = e.sub(a[i][j], e.add(&(0..j).map(|k| e.mul(&[l[i][k], l[j][k]])).collect::<Vec<_>>()));
            l[i][j] = cx.eval(e.f("simplify", &[e.div(s, l[j][j])]))?;
        }
    }
    let mut rows = vec![];
    for i in 0..n {
        let mut row = l[i].clone();
        row.push(c[i]);
        rows.push(e.list(&row));
    }
    let mut last = vec![e.int(0); n];
    last.push(e.int(1));
    rows.push(e.list(&last));
    let unit = e.f(if n == 2 { "disk" } else { "ball" }, &[e.list(&vec![e.int(0); n]), e.int(1)]);
    Ok(Some(e.f("transformed_region", &[unit, e.f("transformation_function", &[e.list(&rows)])])))
}

/// `transformed_region(c, transformation_function(m))` for a circle or disk c of the plane and an affine m whose
/// linear part L is nonsingular: the ellipse congruent to the image, centred at the origin with its axes along the
/// coordinates. Its semi-axes are r s1 and r s2 for the singular values s1, s2 of L, whose squares are the
/// eigenvalues (t ± sqrt(t^2 - 4 D^2)) / 2 of L^T L (t the sum of the squares of L's entries, D = det L).
/// Lengths and areas of the image are those of this ellipse.
pub fn mapped_ellipse<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    let Some(("transformed_region", &[r, tf])) = call_parts(h, t).as_ref().map(|(n, a)| (n.as_str(), *a)) else { return Ok(None) };
    let Some(("transformation_function", &[m])) = call_parts(h, tf).as_ref().map(|(n, a)| (n.as_str(), *a)) else { return Ok(None) };
    let Some(rows) = cx.as_list(m).map(|l| l.to_vec()) else { return Ok(None) };
    if rows.len() != 3 {
        return Ok(None);
    }
    let mut l = vec![];
    for row in &rows {
        match cx.as_list(*row) {
            Some(x) if x.len() == 3 => l.push(x.to_vec()),
            _ => return Ok(None),
        }
    }
    // an affine map: the last row is (0, 0, 1)
    for (k, want) in [(0, 0.0), (1, 0.0), (2, 1.0)] {
        if num(cx, l[2][k])? != Some(want) {
            return Ok(None);
        }
    }
    let Some(Region::Ball { c, r: crate::region::Radius::One(rad), solid }) = read(cx, r) else { return Ok(None) };
    if c.len() != 2 {
        return Ok(None);
    }
    let (a, b, cc, d) = (l[0][0], l[0][1], l[1][0], l[1][1]);
    let det = e.sub(e.mul(&[a, d]), e.mul(&[b, cc]));
    if !num(cx, det)?.is_some_and(|v| v != 0.0) {
        return Ok(None);
    }
    // s1, s2 = (sqrt(t + 2 |D|) ± sqrt(t - 2 |D|)) / 2 (the square roots denested), so s1 s2 = |D| expands
    let tr = e.add(&[e.sq(a), e.sq(b), e.sq(cc), e.sq(d)]);
    let ad = e.f("abs", &[det]);
    let p = e.sqrt(e.add(&[tr, e.mul(&[e.int(2), ad])]));
    let q = e.sqrt(e.sub(tr, e.mul(&[e.int(2), ad])));
    let s1 = e.mul(&[e.rat(1, 2), e.add(&[p, q])]);
    let s2 = e.mul(&[e.rat(1, 2), e.sub(p, q)]);
    let mut axes = vec![];
    for s in [s1, s2] {
        axes.push(cx.eval(e.f("simplify", &[e.mul(&[rad, s])]))?);
    }
    let radius = if axes[0] == axes[1] { axes[0] } else { e.list(&axes) };
    Ok(Some(e.f(if solid { "disk" } else { "circle" }, &[e.list(&[e.int(0), e.int(0)]), radius])))
}
