//! Regions built from measurements, and properties of regions and polygons: the triangles of given sides and
//! angles (`sss_triangle`, `sas_triangle`, `asa_triangle`, `aas_triangle`), `perimeter`, `circumsphere`,
//! `region_moment`, `is_constant_region`, and the polygon functions `canonicalize_polygon`, `polygon_coordinates`,
//! `outer_polygon`, `inner_polygon`, `is_simple_polygon`, `is_convex_polygon`.
//!
//! Triangles are placed with the first corner at the origin and the side c along the positive x axis, the third
//! corner above it (the law of cosines and of sines; any text on plane trigonometry). The circumsphere of k + 1
//! points has its centre c = p0 + sum l_i (p_i - p0) in their affine hull, with the Gram system
//! (p_i - p0).(c - p0) = |p_i - p0|^2 / 2.
use crate::ex::{num, points, E};
use crate::props;
use crate::region::{call_parts, read, Radius, Region, Ring};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

fn triangle<'h>(e: E<'h>, c: T<'h>, x: T<'h>, y: T<'h>) -> T<'h> {
    let o = e.list(&[e.int(0), e.int(0)]);
    e.f("triangle", &[e.list(&[o, e.list(&[c, e.int(0)]), e.list(&[x, y])])])
}

/// `sss_triangle(a, b, c)`, `sas_triangle(a, gamma, b)`, `asa_triangle(alpha, c, beta)`, `aas_triangle(alpha,
/// beta, a)`: corners (0, 0), (c, 0) and the third with |AC| = b, |BC| = a.
pub fn triangle_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], which: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let [p, q, r] = a else { return Ok(Reply::Decline) };
    let (p, q, r) = (*p, *q, *r);
    // numeric arguments must make a triangle
    let nums: Vec<Option<f64>> = vec![num(cx, p)?, num(cx, q)?, num(cx, r)?];
    let simp = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| cx.eval(e.f("simplify", &[t]));
    let t = match which {
        "sss" => {
            if let [Some(a), Some(b), Some(c)] = nums[..] {
                if !(a > 0.0 && b > 0.0 && c > 0.0 && a < b + c && b < a + c && c < a + b) {
                    return Ok(Reply::Decline);
                }
            }
            let (a, b, c) = (p, q, r);
            let x = e.div(e.sub(e.add(&[e.sq(b), e.sq(c)]), e.sq(a)), e.mul(&[e.int(2), c]));
            let x = simp(cx, x)?;
            let y = simp(cx, e.sqrt(e.sub(e.sq(b), e.sq(x))))?;
            triangle(e, c, x, y)
        }
        "sas" => {
            if let [Some(a), Some(g), Some(b)] = nums[..] {
                if !(a > 0.0 && b > 0.0 && g > 0.0 && g < std::f64::consts::PI) {
                    return Ok(Reply::Decline);
                }
            }
            let (a, g, b) = (p, q, r);
            let c = simp(cx, e.sqrt(e.sub(e.add(&[e.sq(a), e.sq(b)]), e.mul(&[e.int(2), a, b, e.f("cos", &[g])]))))?;
            let x = simp(cx, e.div(e.sub(e.add(&[e.sq(b), e.sq(c)]), e.sq(a)), e.mul(&[e.int(2), c])))?;
            let y = simp(cx, e.div(e.mul(&[a, b, e.f("sin", &[g])]), c))?;
            triangle(e, c, x, y)
        }
        "asa" => {
            if let [Some(al), Some(c), Some(be)] = nums[..] {
                if !(al > 0.0 && be > 0.0 && c > 0.0 && al + be < std::f64::consts::PI) {
                    return Ok(Reply::Decline);
                }
            }
            let (al, c, be) = (p, q, r);
            let b = e.div(e.mul(&[c, e.f("sin", &[be])]), e.f("sin", &[e.add(&[al, be])]));
            let x = simp(cx, e.mul(&[b, e.f("cos", &[al])]))?;
            let y = simp(cx, e.mul(&[b, e.f("sin", &[al])]))?;
            triangle(e, c, x, y)
        }
        _ => {
            if let [Some(al), Some(be), Some(a)] = nums[..] {
                if !(al > 0.0 && be > 0.0 && a > 0.0 && al + be < std::f64::consts::PI) {
                    return Ok(Reply::Decline);
                }
            }
            let (al, be, a) = (p, q, r);
            // sin(gamma) = sin(alpha + beta)
            let c = simp(cx, e.div(e.mul(&[a, e.f("sin", &[e.add(&[al, be])])]), e.f("sin", &[al])))?;
            let b = e.div(e.mul(&[a, e.f("sin", &[be])]), e.f("sin", &[al]));
            let x = simp(cx, e.mul(&[b, e.f("cos", &[al])]))?;
            let y = simp(cx, e.mul(&[b, e.f("sin", &[al])]))?;
            triangle(e, c, x, y)
        }
    };
    Ok(Reply::Done(cx.eval(t)?))
}

/// The closed polyline of a ring.
fn closed<'h>(ring: &[Vec<T<'h>>]) -> Vec<Vec<T<'h>>> {
    let mut v = ring.to_vec();
    v.push(ring[0].clone());
    v
}

/// `perimeter(reg)`: the length of the boundary of a plane region.
pub fn perimeter<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let [r] = a else { return Ok(Reply::Decline) };
    let Some(reg) = read(cx, *r) else { return Ok(Reply::Decline) };
    if reg.embedding() != 2 {
        return Ok(Reply::Decline);
    }
    let boundary = match &reg {
        Region::Ball { c, r: Radius::Axes(rs), solid: true } => {
            let _ = c;
            // 4 b E(1 - a^2/b^2) with b the second semi-axis
            let (ra, rb) = (rs[0], rs[1]);
            return Ok(Reply::Done(cx.eval(e.mul(&[e.int(4), rb, e.f("elliptic_e", &[e.sub(e.int(1), e.div(e.sq(ra), e.sq(rb)))])]))?));
        }
        Region::Ball { c, r: rad, solid: true } => Region::Ball { c: c.clone(), r: rad.clone(), solid: false },
        Region::Polygon(rings) => {
            let mut ls = vec![];
            for ring in rings {
                ls.push(closed(&ring.outer));
                ls.extend(ring.holes.iter().map(|x| closed(x)));
            }
            Region::Lines(ls)
        }
        Region::Simplex(v) if v.len() == 3 => Region::Lines(vec![closed(v)]),
        Region::Boxed { lo, hi } => {
            let t = e.mul(&[e.int(2), e.add(&[e.f("abs", &[e.sub(hi[0], lo[0])]), e.f("abs", &[e.sub(hi[1], lo[1])])])]);
            return Ok(Reply::Done(cx.eval(t)?));
        }
        Region::Annulus { r0, r1, .. } => return Ok(Reply::Done(cx.eval(e.mul(&[e.int(2), e.pi(), e.add(&[*r0, *r1])]))?)),
        _ => return Ok(Reply::Decline),
    };
    match props::measure(cx, &boundary)? {
        Some(t) => Ok(Reply::Done(cx.eval(e.f("simplify", &[t]))?)),
        None => Ok(Reply::Decline),
    }
}

/// The corners of a simplex-like argument: a list of points, or a triangle, tetrahedron or simplex.
fn corners<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<Vec<T<'h>>>> {
    if let Some(v) = crate::region::vectors(cx, t) {
        return Some(v);
    }
    match read(cx, t)? {
        Region::Simplex(v) => Some(v),
        Region::Polygon(rs) if rs.len() == 1 && rs[0].holes.is_empty() => Some(rs[0].outer.clone()),
        _ => None,
    }
}

/// `circumsphere(points)`: the sphere (a circle in the plane) through affinely independent points, in their hull.
pub fn circumsphere<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let [t] = a else { return Ok(Reply::Decline) };
    let Some(v) = corners(cx, *t) else { return Ok(Reply::Decline) };
    if v.len() < 2 || v.len() > v[0].len() + 1 {
        return Ok(Reply::Decline);
    }
    let Some(vn) = points(cx, &v)? else { return Ok(Reply::Decline) };
    let edges: Vec<Vec<f64>> = vn[1..].iter().map(|q| q.iter().zip(&vn[0]).map(|(a, b)| a - b).collect()).collect();
    if props::rank(&edges) != edges.len() {
        return Ok(Reply::Decline);
    }
    let ed: Vec<Vec<T<'h>>> = v[1..].iter().map(|q| e.vsub(q, &v[0])).collect();
    let g = e.list(&ed.iter().map(|x| e.list(&ed.iter().map(|y| e.dot(x, y)).collect::<Vec<_>>())).collect::<Vec<_>>());
    let b = e.list(&ed.iter().map(|x| e.mul(&[e.rat(1, 2), e.norm2(x)])).collect::<Vec<_>>());
    let lam = cx.eval(e.f("linear_solve", &[g, b]))?;
    let Some(lam) = cx.as_list(lam).map(|l| l.to_vec()).filter(|l| l.len() == ed.len()) else { return Ok(Reply::Decline) };
    let mut c = v[0].clone();
    for (l, x) in lam.iter().zip(&ed) {
        c = e.vadd(&c, &e.scale(*l, x));
    }
    let c: Vec<T<'h>> = c.into_iter().map(|x| cx.eval(e.f("simplify", &[x]))).collect::<Res<_>>()?;
    let r = cx.eval(e.f("simplify", &[e.dist(&c, &v[0])]))?;
    Ok(Reply::Done(e.f("sphere", &[e.list(&c), r])))
}

/// `region_moment(reg, [i, j, ...])`: the integral of x^i y^j ... over the region.
pub fn region_moment<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let [r, ks] = a else { return Ok(Reply::Decline) };
    let Some(reg) = read(cx, *r) else { return Ok(Reply::Decline) };
    let Some(ks) = cx.as_list(*ks).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
    if ks.len() != reg.embedding() || !ks.iter().all(|&k| matches!(h.view(k), View::Int(i) if i >= 0)) {
        return Ok(Reply::Decline);
    }
    let vars: Vec<T<'h>> = (0..ks.len()).map(|i| h.sym_term(h.sym(&format!("mc_geom_moment_x{i}")))).collect();
    let f = e.mul(&vars.iter().zip(&ks).map(|(&x, &k)| e.pow(x, k)).collect::<Vec<_>>());
    let v = cx.eval(e.f("region_integrate", &[f, e.list(&vars), *r]))?;
    // unevaluated: decline
    if call_parts(h, v).is_some_and(|(n, _)| n == "region_integrate" || n == "integrate") || mc_api_contains(h, v, &vars) {
        return Ok(Reply::Decline);
    }
    Ok(Reply::Done(v))
}

fn mc_api_contains<'h>(h: &'h Heap, t: T<'h>, vars: &[T<'h>]) -> bool {
    if vars.contains(&t) {
        return true;
    }
    match h.view(t) {
        View::App { head, args } => mc_api_contains(h, head, vars) || args.iter().any(|&x| mc_api_contains(h, x, vars)),
        _ => false,
    }
}

/// Whether a term is free of symbols other than numeric constants (and `bound`).
fn constant_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, bound: &[T<'h>]) -> Res<bool> {
    let h = cx.heap();
    if bound.contains(&t) {
        return Ok(true);
    }
    match h.view(t) {
        View::Sym(s) if matches!(h.sym_name(s).as_str(), "inf" | "complex_infinity") => Ok(true),
        View::Sym(_) => Ok(num(cx, t)?.is_some()),
        View::App { args, .. } => {
            for &x in args {
                if !constant_term(cx, x, bound)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(true),
    }
}

/// `is_constant_region(reg)`: whether the region is given by numbers only (no free parameters).
pub fn is_constant_region<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [r] = a else { return Ok(Reply::Decline) };
    let Some(reg) = read(cx, *r) else { return Ok(Reply::Done(crate::metric::truth(h, false))) };
    let mut bound: Vec<T<'h>> = vec![];
    let mut body = *r;
    match &reg {
        Region::Implicit { cond, vars } => {
            bound.extend(vars.iter().copied());
            body = *cond;
        }
        Region::Parametric { fs, specs } => {
            bound.extend(specs.iter().map(|s| s.0));
            for s in specs {
                for x in [s.1, s.2] {
                    if !constant_term(cx, x, &[])? {
                        return Ok(Reply::Done(crate::metric::truth(h, false)));
                    }
                }
            }
            let mut ok = true;
            for &f in fs {
                ok &= constant_term(cx, f, &bound)?;
            }
            return Ok(Reply::Done(crate::metric::truth(h, ok)));
        }
        _ => {}
    }
    // the region term's arguments (its head and option names are not parameters)
    let args: Vec<T<'h>> = match h.view(body) {
        View::App { args, .. } if bound.is_empty() => crate::region::without_options(h, args).to_vec(),
        _ => vec![body],
    };
    let mut ok = true;
    for x in args {
        ok &= constant_term(cx, x, &bound)?;
    }
    // implicit regions: the variables of nested specs ([z, 0, 2]) are bound too
    Ok(Reply::Done(crate::metric::truth(h, ok)))
}

// ---- polygons

/// The rings of a plane polygon-like region, exactly, with their numeric coordinates.
#[allow(clippy::type_complexity)]
pub(crate) fn rings<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<(Ring<'h>, Vec<Vec<f64>>, Vec<Vec<Vec<f64>>>)>>> {
    let Some(reg) = read(cx, t) else { return Ok(None) };
    let e = E { h: cx.heap() };
    let rs: Vec<Ring<'h>> = match reg {
        Region::Polygon(rs) => rs,
        Region::Simplex(v) if v.len() == 3 => vec![Ring { outer: v, holes: vec![] }],
        Region::Boxed { lo, hi } if lo.len() == 2 => vec![Ring { outer: vec![lo.clone(), vec![hi[0], lo[1]], hi.clone(), vec![lo[0], hi[1]]], holes: vec![] }],
        _ => return Ok(None),
    };
    let _ = e;
    let mut out = vec![];
    for r in rs {
        if r.outer[0].len() != 2 {
            return Ok(None);
        }
        let Some(o) = points(cx, &r.outer)? else { return Ok(None) };
        let mut hs = vec![];
        for hole in &r.holes {
            match points(cx, hole)? {
                Some(x) => hs.push(x),
                None => return Ok(None),
            }
        }
        out.push((r, o, hs));
    }
    Ok(Some(out))
}

fn signed_area(r: &[Vec<f64>]) -> f64 {
    let n = r.len();
    (0..n).map(|i| r[i][0] * r[(i + 1) % n][1] - r[(i + 1) % n][0] * r[i][1]).sum::<f64>() / 2.0
}

/// Whether two segments ab and cd cross or touch (other than at a shared end of neighbouring edges).
fn segments_meet(a: &[f64], b: &[f64], c: &[f64], d: &[f64]) -> bool {
    let o = |p: &[f64], q: &[f64], r: &[f64]| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let on = |p: &[f64], q: &[f64], r: &[f64]| r[0] >= p[0].min(q[0]) && r[0] <= p[0].max(q[0]) && r[1] >= p[1].min(q[1]) && r[1] <= p[1].max(q[1]);
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    (d1 == 0.0 && on(c, d, a)) || (d2 == 0.0 && on(c, d, b)) || (d3 == 0.0 && on(a, b, c)) || (d4 == 0.0 && on(a, b, d))
}

/// Whether a closed ring does not cross itself.
pub fn ring_simple(r: &[Vec<f64>]) -> bool {
    let n = r.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        for j in i + 1..n {
            // neighbouring edges share an end
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if segments_meet(&r[i], &r[(i + 1) % n], &r[j], &r[(j + 1) % n]) {
                return false;
            }
        }
    }
    true
}

/// Whether a ring is convex (every turn the same way; collinear corners allowed) and simple.
fn ring_convex(r: &[Vec<f64>]) -> bool {
    let n = r.len();
    let mut sign = 0.0f64;
    for i in 0..n {
        let (a, b, c) = (&r[i], &r[(i + 1) % n], &r[(i + 2) % n]);
        let z = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if z != 0.0 {
            if sign != 0.0 && z.signum() != sign {
                return false;
            }
            sign = z.signum();
        }
    }
    ring_simple(r)
}

/// A ring of points in space: planar (Newell's normal, every point within rounding of the plane), then projected along
/// the normal's largest component and tested as a plane ring.
fn space_ring_test(p: &[Vec<f64>], convex: bool) -> bool {
    let n = p.len();
    if n < 3 {
        return false;
    }
    let mut nrm = [0.0f64; 3];
    for i in 0..n {
        let (a, b) = (&p[i], &p[(i + 1) % n]);
        nrm[0] += (a[1] - b[1]) * (a[2] + b[2]);
        nrm[1] += (a[2] - b[2]) * (a[0] + b[0]);
        nrm[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let len = nrm.iter().map(|x| x * x).sum::<f64>().sqrt();
    if len == 0.0 {
        return false;
    }
    let scale = p.iter().flatten().fold(1.0f64, |m, x| m.max(x.abs()));
    if p.iter().any(|q| ((q[0] - p[0][0]) * nrm[0] + (q[1] - p[0][1]) * nrm[1] + (q[2] - p[0][2]) * nrm[2]).abs() / len > 1e-9 * scale) {
        return false;
    }
    let drop = (0..3).max_by(|&i, &j| nrm[i].abs().total_cmp(&nrm[j].abs())).unwrap_or(2);
    let keep: Vec<usize> = (0..3).filter(|&k| k != drop).collect();
    let flat: Vec<Vec<f64>> = p.iter().map(|q| vec![q[keep[0]], q[keep[1]]]).collect();
    if convex {
        ring_convex(&flat)
    } else {
        ring_simple(&flat)
    }
}

/// `is_simple_polygon(p)` / `is_convex_polygon(p)`.
pub fn polygon_test<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], convex: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [t] = a else { return Ok(Reply::Decline) };
    let Some(rs) = rings(cx, *t)? else {
        // a polygon in space: planar, then simple (convex) in the plane it lies in
        let ok = match read(cx, *t) {
            Some(Region::Polygon(rs)) if rs.len() == 1 && rs[0].holes.is_empty() && rs[0].outer[0].len() == 3 => match points(cx, &rs[0].outer)? {
                Some(p) => space_ring_test(&p, convex),
                None => false,
            },
            _ => false,
        };
        return Ok(Reply::Done(crate::metric::truth(h, ok)));
    };
    let ok = rs.len() == 1 && rs[0].2.is_empty() && if convex { ring_convex(&rs[0].1) } else { ring_simple(&rs[0].1) };
    Ok(Reply::Done(crate::metric::truth(h, ok)))
}

/// An indexed polygon: the distinct corners sorted, and each ring (anticlockwise outer, clockwise holes) as indices.
#[allow(clippy::type_complexity)]
fn indexed<'h>(cx: &mut NativeCx<'_, '_, 'h>, rs: &[(Vec<Vec<T<'h>>>, Vec<Vec<f64>>, bool)]) -> Res<(Vec<Vec<T<'h>>>, Vec<Vec<i64>>)> {
    let mut pts: Vec<(Vec<f64>, Vec<T<'h>>)> = vec![];
    for (ex, nu, _) in rs {
        for (p, q) in ex.iter().zip(nu) {
            if !pts.iter().any(|x| x.0 == *q) {
                pts.push((q.clone(), p.clone()));
            }
        }
    }
    pts.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])));
    let _ = cx;
    let mut idx = vec![];
    for (_, nu, hole) in rs {
        let mut ring: Vec<i64> = nu.iter().map(|q| pts.iter().position(|x| x.0 == *q).unwrap_or(0) as i64 + 1).collect();
        let a = signed_area(nu);
        if (a < 0.0) != *hole {
            ring.reverse();
            ring.rotate_right(1);
        }
        // the cycle starts at its lowest index (the canonical form: [1, 3, 2], not [3, 2, 1])
        if let Some(lo) = ring.iter().enumerate().min_by_key(|&(_, v)| *v).map(|(i, _)| i) {
            ring.rotate_left(lo);
        }
        idx.push(ring);
    }
    Ok((pts.into_iter().map(|x| x.1).collect(), idx))
}

fn indexed_term<'h>(e: E<'h>, pts: &[Vec<T<'h>>], idx: &[Vec<i64>], one: bool) -> T<'h> {
    let ints = |v: &[i64]| e.list(&v.iter().map(|&i| e.int(i)).collect::<Vec<_>>());
    let ix = if one && idx.len() == 1 { ints(&idx[0]) } else { e.list(&idx.iter().map(|v| ints(v)).collect::<Vec<_>>()) };
    e.f("polygon", &[e.list(&pts.iter().map(|p| e.list(p)).collect::<Vec<_>>()), ix])
}

/// `canonicalize_polygon(p)`, `outer_polygon(p)`, `inner_polygon(p)`, `polygon_coordinates(p)`.
pub fn polygon_form<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], which: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let [t] = a else { return Ok(Reply::Decline) };
    let Some(rs) = rings(cx, *t)? else { return Ok(Reply::Decline) };
    let mut parts = vec![];
    for (r, o, hs) in &rs {
        if which != "inner" {
            parts.push((r.outer.clone(), o.clone(), false));
        }
        if which != "outer" {
            for (ex, nu) in r.holes.iter().zip(hs) {
                parts.push((ex.clone(), nu.clone(), which != "inner"));
            }
        }
    }
    if parts.is_empty() {
        return Ok(Reply::Done(e.f("empty_region", &[e.int(2)])));
    }
    let (pts, idx) = indexed(cx, &parts)?;
    Ok(Reply::Done(match which {
        "coordinates" => e.list(&pts.iter().map(|p| e.list(p)).collect::<Vec<_>>()),
        "inner" => indexed_term(e, &pts, &idx, false),
        _ => indexed_term(e, &pts, &idx, true),
    }))
}

// ---- relations between regions

/// A region of the line as a union of closed intervals (numeric), or `None`.
fn intervals_1d<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<Vec<(f64, f64)>>> {
    if r.embedding() != 1 {
        return Ok(None);
    }
    let mut out = vec![];
    match r {
        Region::Points(ps) => {
            for p in ps {
                let Some(v) = num(cx, p[0])? else { return Ok(None) };
                out.push((v, v));
            }
        }
        Region::Lines(ls) => {
            for l in ls {
                for w in l.windows(2) {
                    let (Some(a), Some(b)) = (num(cx, w[0][0])?, num(cx, w[1][0])?) else { return Ok(None) };
                    out.push((a.min(b), a.max(b)));
                }
            }
        }
        Region::Intervals(iv) => {
            for &(a, b) in iv {
                let (Some(a), Some(b)) = (num(cx, a)?, num(cx, b)?) else { return Ok(None) };
                out.push((a.min(b), a.max(b)));
            }
        }
        Region::Ball { c, r: Radius::One(rad), solid } => {
            let (Some(c), Some(rad)) = (num(cx, c[0])?, num(cx, *rad)?) else { return Ok(None) };
            if *solid {
                out.push((c - rad, c + rad));
            } else {
                out.push((c - rad, c - rad));
                out.push((c + rad, c + rad));
            }
        }
        Region::Boxed { lo, hi } => {
            let (Some(a), Some(b)) = (num(cx, lo[0])?, num(cx, hi[0])?) else { return Ok(None) };
            out.push((a.min(b), a.max(b)));
        }
        Region::Simplex(v) => {
            let Some(vn) = points(cx, v)? else { return Ok(None) };
            let xs: Vec<f64> = vn.iter().map(|p| p[0]).collect();
            out.push((xs.iter().copied().fold(f64::INFINITY, f64::min), xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)));
        }
        Region::Affine { p, free, pos } => {
            let Some(x) = num(cx, p[0])? else { return Ok(None) };
            let mut lo = x;
            let mut hi = x;
            for v in free {
                if num(cx, v[0])? != Some(0.0) {
                    lo = f64::NEG_INFINITY;
                    hi = f64::INFINITY;
                }
            }
            for v in pos {
                match num(cx, v[0])? {
                    Some(d) if d > 0.0 => hi = f64::INFINITY,
                    Some(d) if d < 0.0 => lo = f64::NEG_INFINITY,
                    Some(_) => {}
                    None => return Ok(None),
                }
            }
            out.push((lo, hi));
        }
        Region::Union(parts) => {
            for p in parts {
                match intervals_1d(cx, p)? {
                    Some(v) => out.extend(v),
                    None => return Ok(None),
                }
            }
        }
        _ => return Ok(None),
    }
    // merged
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = vec![];
    for (a, b) in out {
        match merged.last_mut() {
            Some(l) if a <= l.1 => l.1 = l.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    Ok(Some(merged))
}

fn within_1d(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.iter().all(|&(x, y)| b.iter().any(|&(u, v)| u <= x && y <= v))
}

fn disjoint_1d(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.iter().all(|&(x, y)| b.iter().all(|&(u, v)| y < u || v < x))
}

/// The plane polygon rings of a region (numeric), for the Boolean tests.
fn plane_rings<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<Vec<crate::clip::P>>>> {
    let Some(rs) = rings(cx, t)? else { return Ok(None) };
    let mut out = vec![];
    for (_, o, hs) in rs {
        out.push(o.iter().map(|p| [p[0], p[1]]).collect());
        for hole in hs {
            out.push(hole.iter().map(|p| [p[0], p[1]]).collect());
        }
    }
    Ok(Some(out))
}

fn total_area(s: &crate::clip::Shapes) -> f64 {
    s.iter().map(|(o, hs)| crate::clip::area(o).abs() - hs.iter().map(|h| crate::clip::area(h).abs()).sum::<f64>()).sum()
}

fn as_vecs(r: &[Vec<crate::clip::P>]) -> Vec<Vec<Vec<f64>>> {
    r.iter().map(|ring| ring.iter().map(|p| p.to_vec()).collect()).collect()
}

/// Whether two sets of plane rings share a point (overlap, touch, or one inside the other).
fn plane_meet(x: &[Vec<crate::clip::P>], y: &[Vec<crate::clip::P>], scale: f64) -> bool {
    if total_area(&crate::clip::boolean(x, y, crate::clip::Op::Intersection)) > scale {
        return true;
    }
    for rx in x {
        for ry in y {
            for k in 0..rx.len() {
                for l in 0..ry.len() {
                    if segments_meet(&rx[k], &rx[(k + 1) % rx.len()], &ry[l], &ry[(l + 1) % ry.len()]) {
                        return true;
                    }
                }
            }
        }
    }
    crate::metric::in_polygon(&x[0][0], &as_vecs(y)) || crate::metric::in_polygon(&y[0][0], &as_vecs(x))
}

/// `region_equal(r1, r2, ...)`, `region_within(r1, r2)`, `region_disjoint(r1, r2, ...)` for regions of the line
/// (as unions of intervals) and plane polygons (through their Boolean combinations); other regions stay
/// unevaluated.
pub fn relation<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], which: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    if a.len() < 2 || (which == "within" && a.len() != 2) {
        return Ok(Reply::Decline);
    }
    let mut regs = vec![];
    for &t in a {
        match read(cx, t) {
            Some(r) => regs.push(r),
            None => return Ok(Reply::Decline),
        }
    }
    let pairs: Vec<(usize, usize)> = match which {
        "within" => vec![(0, 1)],
        "equal" => (1..a.len()).map(|i| (0, i)).collect(),
        _ => (0..a.len()).flat_map(|i| (i + 1..a.len()).map(move |j| (i, j))).collect(),
    };
    let mut verdict = true;
    for (i, j) in pairs {
        let ok = if let (Some(x), Some(y)) = (intervals_1d(cx, &regs[i])?, intervals_1d(cx, &regs[j])?) {
            match which {
                // the second inside the first
                "within" => within_1d(&y, &x),
                "equal" => within_1d(&x, &y) && within_1d(&y, &x),
                _ => disjoint_1d(&x, &y),
            }
        } else if let (Some(x), Some(y)) = (plane_rings(cx, a[i])?, plane_rings(cx, a[j])?) {
            let scale = 1e-9 * (1.0 + crate::clip::area(&x[0]).abs() + crate::clip::area(&y[0]).abs());
            let diff = |p: &[Vec<crate::clip::P>], q: &[Vec<crate::clip::P>]| total_area(&crate::clip::boolean(p, q, crate::clip::Op::Difference));
            match which {
                "within" => diff(&y, &x) <= scale,
                "equal" => diff(&x, &y) <= scale && diff(&y, &x) <= scale,
                _ => !plane_meet(&x, &y, scale),
            }
        } else {
            return Ok(Reply::Decline);
        };
        verdict &= ok;
    }
    Ok(Reply::Done(crate::metric::truth(h, verdict)))
}

// ---- bounding regions

/// The centre (exact terms) and squared radius (numeric) of the smallest sphere through the points of `idx`
/// centred in their affine hull.
#[allow(clippy::type_complexity)]
fn sphere_through<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: &[Vec<T<'h>>], vn: &[Vec<f64>], idx: &[usize]) -> Res<Option<(Vec<T<'h>>, Vec<f64>, f64)>> {
    let e = E { h: cx.heap() };
    let p0 = &vn[idx[0]];
    let edges: Vec<Vec<f64>> = idx[1..].iter().map(|&i| vn[i].iter().zip(p0).map(|(a, b)| a - b).collect()).collect();
    if props::rank(&edges) != edges.len() {
        return Ok(None);
    }
    // numeric centre first
    let g: Vec<Vec<f64>> = edges.iter().map(|a| edges.iter().map(|b| a.iter().zip(b).map(|(x, y)| x * y).sum()).collect()).collect();
    let r: Vec<f64> = edges.iter().map(|a| a.iter().map(|x| x * x).sum::<f64>() / 2.0).collect();
    let lam = if edges.is_empty() { vec![] } else {
        match crate::metric::solve(g, r) {
            Some(l) => l,
            None => return Ok(None),
        }
    };
    let mut cn = p0.clone();
    for (l, ed) in lam.iter().zip(&edges) {
        for k in 0..cn.len() {
            cn[k] += l * ed[k];
        }
    }
    let r2: f64 = cn.iter().zip(p0).map(|(a, b)| (a - b) * (a - b)).sum();
    // the exact centre
    let ex: Vec<Vec<T<'h>>> = idx[1..].iter().map(|&i| e.vsub(&v[i], &v[idx[0]])).collect();
    let c = if ex.is_empty() {
        v[idx[0]].clone()
    } else {
        let gm = e.list(&ex.iter().map(|x| e.list(&ex.iter().map(|y| e.dot(x, y)).collect::<Vec<_>>())).collect::<Vec<_>>());
        let b = e.list(&ex.iter().map(|x| e.mul(&[e.rat(1, 2), e.norm2(x)])).collect::<Vec<_>>());
        let lam = cx.eval(e.f("linear_solve", &[gm, b]))?;
        let Some(lam) = cx.as_list(lam).map(|l| l.to_vec()).filter(|l| l.len() == ex.len()) else { return Ok(None) };
        let mut c = v[idx[0]].clone();
        for (l, x) in lam.iter().zip(&ex) {
            c = e.vadd(&c, &e.scale(*l, x));
        }
        c
    };
    Ok(Some((c, cn, r2)))
}

fn convex_hull_order(p: &[Vec<f64>]) -> Vec<usize> {
    // Andrew's monotone chain, then turned to start at the first hull point in input order
    let mut idx: Vec<usize> = (0..p.len()).collect();
    idx.sort_by(|&a, &b| p[a][0].total_cmp(&p[b][0]).then(p[a][1].total_cmp(&p[b][1])));
    let cross = |o: usize, a: usize, b: usize| (p[a][0] - p[o][0]) * (p[b][1] - p[o][1]) - (p[a][1] - p[o][1]) * (p[b][0] - p[o][0]);
    let mut hull: Vec<usize> = vec![];
    for pass in 0..2 {
        let start = hull.len();
        let seq: Vec<usize> = if pass == 0 { idx.clone() } else { idx.iter().rev().copied().collect() };
        for i in seq {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], i) <= 0.0 {
                hull.pop();
            }
            hull.push(i);
        }
        hull.pop();
    }
    if let Some(k) = (0..hull.len()).min_by_key(|&k| hull[k]) {
        hull.rotate_left(k);
    }
    hull
}

/// `bounding_region(points, form)`: "MinRectangle" / "MinCuboid" / "MinInterval" (the box of the coordinates,
/// the default), "MinDisk" / "MinBall" (the smallest enclosing ball, by trying the balls through two to four of
/// the points: Welzl's characterisation that the smallest ball is fixed by at most d + 1 of them), and
/// "MinConvexPolygon" (the convex hull, anticlockwise from its first point in the input order).
#[allow(clippy::type_complexity)]
pub fn bounding_region<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let e = E { h };
    let (pt, form) = match a {
        [p] => (*p, "MinRectangle".to_string()),
        [p, f] => match h.view(*f) {
            View::Str(s) => (*p, s.to_string()),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let v = match crate::region::vectors(cx, pt) {
        Some(v) => v,
        None => match read(cx, pt) {
            Some(Region::Points(v)) => v,
            _ => return Ok(Reply::Decline),
        },
    };
    let Some(vn) = points(cx, &v)? else { return Ok(Reply::Decline) };
    let n = v[0].len();
    match form.as_str() {
        "MinRectangle" | "MinCuboid" | "MinInterval" => {
            let mut lo = vec![];
            let mut hi = vec![];
            for k in 0..n {
                let col: Vec<T<'h>> = v.iter().map(|p| p[k]).collect();
                let i0 = (0..v.len()).min_by(|&i, &j| vn[i][k].total_cmp(&vn[j][k])).unwrap_or(0);
                let i1 = (0..v.len()).max_by(|&i, &j| vn[i][k].total_cmp(&vn[j][k])).unwrap_or(0);
                lo.push(col[i0]);
                hi.push(col[i1]);
            }
            Ok(Reply::Done(match n {
                1 => e.f("interval", &[e.list(&[lo[0], hi[0]])]),
                2 => e.f("rectangle", &[e.list(&lo), e.list(&hi)]),
                _ => e.f("cuboid", &[e.list(&lo), e.list(&hi)]),
            }))
        }
        "MinDisk" | "MinBall" if v.len() <= 60 => {
            let m = v.len();
            let mut best: Option<(Vec<T<'h>>, Vec<f64>, f64, Vec<usize>)> = None;
            let mut sets: Vec<Vec<usize>> = (0..m).map(|i| vec![i]).collect();
            for k in 2..=(n + 1).min(m) {
                let mut next = vec![];
                for s in &sets {
                    for j in s[s.len() - 1] + 1..m {
                        let mut t = s.clone();
                        t.push(j);
                        next.push(t);
                    }
                }
                sets = next;
                let _ = k;
                for s in &sets {
                    let Some((c, cn, r2)) = sphere_through(cx, &v, &vn, s)? else { continue };
                    if best.as_ref().is_some_and(|b| r2 >= b.2) {
                        continue;
                    }
                    let tol = 1e-9 * (1.0 + r2);
                    if vn.iter().all(|q| q.iter().zip(&cn).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() <= r2 + tol) {
                        best = Some((c, cn, r2, s.clone()));
                    }
                }
            }
            let Some((c, cn, r2, s)) = best else { return Ok(Reply::Decline) };
            let head = if n == 2 { "disk" } else { "ball" };
            if n == 2 {
                let c: Vec<T<'h>> = c.into_iter().map(|x| cx.eval(e.f("simplify", &[x]))).collect::<Res<_>>()?;
                let r = cx.eval(e.f("simplify", &[e.dist(&c, &v[s[0]])]))?;
                Ok(Reply::Done(e.f(head, &[e.list(&c), r])))
            } else {
                Ok(Reply::Done(e.f(head, &[h.reals(&cn), h.real(r2.sqrt())])))
            }
        }
        "MinConvexPolygon" if n == 2 => {
            let hull = convex_hull_order(&vn);
            if hull.len() < 3 {
                return Ok(Reply::Decline);
            }
            let pts: Vec<T<'h>> = hull.iter().map(|&i| e.list(&v[i])).collect();
            let idx: Vec<T<'h>> = (1..=hull.len()).map(|i| e.int(i as i64)).collect();
            Ok(Reply::Done(e.f("polygon", &[e.list(&pts), e.list(&idx)])))
        }
        _ => Ok(Reply::Decline),
    }
}

// ---- winding and crossing counts

/// The closed curve of a list of points, a line or a polygon (numeric plane points).
fn closed_curve<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<Vec<f64>>>> {
    let pts = match crate::region::vectors(cx, t) {
        Some(v) => v,
        None => match read(cx, t) {
            Some(Region::Lines(ls)) if ls.len() == 1 => ls[0].clone(),
            Some(Region::Polygon(rs)) if rs.len() == 1 && rs[0].holes.is_empty() => rs[0].outer.clone(),
            Some(Region::Simplex(v)) if v.len() == 3 => v,
            _ => return Ok(None),
        },
    };
    let Some(mut p) = points(cx, &pts)? else { return Ok(None) };
    if p.is_empty() || p[0].len() != 2 {
        return Ok(None);
    }
    if p.len() > 1 && p[0] == p[p.len() - 1] {
        p.pop();
    }
    Ok(Some(p))
}

/// `winding_count(curve, p)`: how many times the closed curve turns about p (anticlockwise positive);
/// `crossing_count(curve, p)`: the least number of times a ray from p crosses it, over the ray's directions (720 of
/// them, off the vertices), plus 1 when p lies on the curve (so the arrowhead's notch is 0
/// below the notch, 1 on it; a winding of -2 needs 2). A polygon is a region: a
/// vertex cycle traced several times is taken once and its orientation made positive, so its winding count is 1
/// inside (lines and point lists keep their direction and multiplicity).
pub fn winding<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], crossing: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [c, p] = a else { return Ok(Reply::Decline) };
    let Some(mut curve) = closed_curve(cx, *c)? else { return Ok(Reply::Decline) };
    let is_polygon = matches!(h.view(*c), View::App { head, .. } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "polygon"));
    if is_polygon {
        // one traversal of a repeated cycle
        let n = curve.len();
        if let Some(k) = (1..n).find(|&k| n % k == 0 && (0..n).all(|i| curve[i] == curve[i % k])) {
            curve.truncate(k);
        }
        let area: f64 = (0..curve.len()).map(|i| {
            let (a, b) = (&curve[i], &curve[(i + 1) % curve.len()]);
            a[0] * b[1] - b[0] * a[1]
        }).sum();
        if area < 0.0 {
            curve.reverse();
        }
    }
    let one = |q: &[f64]| -> i64 {
        let n = curve.len();
        let mut w = 0i64;
        for i in 0..n {
            let (a, b) = (&curve[i], &curve[(i + 1) % n]);
            let side = (b[0] - a[0]) * (q[1] - a[1]) - (q[0] - a[0]) * (b[1] - a[1]);
            if a[1] <= q[1] && b[1] > q[1] && side > 0.0 {
                w += 1;
            } else if a[1] > q[1] && b[1] <= q[1] && side < 0.0 {
                w -= 1;
            }
        }
        if !crossing {
            return w;
        }
        // on the curve
        let on = (0..n).any(|i| {
            let (a, b) = (&curve[i], &curve[(i + 1) % n]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len2 = dx * dx + dy * dy;
            let t = if len2 == 0.0 { 0.0 } else { (((q[0] - a[0]) * dx + (q[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) };
            let (ex, ey) = (a[0] + t * dx - q[0], a[1] + t * dy - q[1]);
            (ex * ex + ey * ey).sqrt() <= 1e-12 * (1.0 + len2.sqrt())
        });
        let mut best = i64::MAX;
        for k in 0..720 {
            let th = std::f64::consts::PI * (2.0 * k as f64 + 0.618) / 720.0;
            let (ux, uy) = (th.cos(), th.sin());
            let mut cnt = 0i64;
            for i in 0..n {
                let (a, b) = (&curve[i], &curve[(i + 1) % n]);
                // q + s u = a + t (b - a), s > 0, 0 <= t < 1
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let den = ux * (-dy) - uy * (-dx);
                if den.abs() < 1e-300 {
                    continue;
                }
                let (rx, ry) = (a[0] - q[0], a[1] - q[1]);
                let sr = (rx * (-dy) - ry * (-dx)) / den;
                let t = (ux * ry - uy * rx) / den;
                if sr > 1e-12 && (0.0..1.0).contains(&t) {
                    cnt += 1;
                }
            }
            best = best.min(cnt);
        }
        best + i64::from(on)
    };
    let as_point = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<Vec<f64>>> {
        match cx.as_list(t).map(|l| l.to_vec()) {
            Some(v) if v.len() == 2 => crate::ex::nums(cx, &v),
            _ => Ok(None),
        }
    };
    if let Some(q) = as_point(cx, *p)? {
        return Ok(Reply::Done(h.int(one(&q))));
    }
    let Some(items) = cx.as_list(*p).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
    let mut out = vec![];
    for it in items {
        match as_point(cx, it)? {
            Some(q) => out.push(h.int(one(&q))),
            None => return Ok(Reply::Decline),
        }
    }
    Ok(Reply::Done(cx.list(&out)))
}
