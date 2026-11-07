//! Membership, distance, nearest points and signed distance. The nearest feature (vertex, edge, face or inside)
//! is found with machine numbers; the result is then built exactly from the region's own coordinates.
use crate::ex::{num, nums, points, E};
use crate::region::{Radius, Region};
use mc_core::{Heap, Store, T};
use mc_eval::{NativeCx, Res};

const EPS: f64 = 1e-12;

fn sub(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b).map(|(x, y)| x - y).collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Solve the small symmetric system g x = r (Gaussian elimination with pivoting); `None` when singular.
fn solve(mut g: Vec<Vec<f64>>, mut r: Vec<f64>) -> Option<Vec<f64>> {
    let n = r.len();
    let scale = g.iter().flatten().fold(0.0f64, |a, &b| a.max(b.abs())).max(1e-300);
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| g[i][c].abs().total_cmp(&g[j][c].abs()))?;
        if g[p][c].abs() <= 1e-13 * scale {
            return None;
        }
        g.swap(c, p);
        r.swap(c, p);
        let (top, rest) = g.split_at_mut(c + 1);
        let pivot = &top[c];
        for (i, row) in rest.iter_mut().enumerate() {
            let f = row[c] / pivot[c];
            for (x, &y) in row[c..].iter_mut().zip(&pivot[c..]) {
                *x -= f * y;
            }
            r[c + 1 + i] -= f * r[c];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|k| g[i][k] * x[k]).sum();
        x[i] = (r[i] - s) / g[i][i];
    }
    Some(x)
}

/// The nearest point of the convex hull of `v` to `p`, with the vertices (indices) of the face whose relative
/// interior holds it.
pub fn nearest_simplex(p: &[f64], v: &[Vec<f64>]) -> (Vec<f64>, Vec<usize>) {
    let m = v.len();
    let mut best: Option<(f64, Vec<f64>, Vec<usize>)> = None;
    for mask in 1u32..(1 << m) {
        let idx: Vec<usize> = (0..m).filter(|i| mask & (1 << i) != 0).collect();
        let a = &v[idx[0]];
        let edges: Vec<Vec<f64>> = idx[1..].iter().map(|&i| sub(&v[i], a)).collect();
        let lam = if edges.is_empty() {
            Some(vec![])
        } else {
            let g = edges.iter().map(|x| edges.iter().map(|y| dot(x, y)).collect()).collect();
            let r = edges.iter().map(|x| dot(&sub(p, a), x)).collect();
            solve(g, r)
        };
        let Some(lam) = lam else { continue };
        let sum: f64 = lam.iter().sum();
        if lam.iter().any(|&l| l < -1e-12) || sum > 1.0 + 1e-12 {
            continue;
        }
        let mut q = a.clone();
        for (l, e) in lam.iter().zip(&edges) {
            for k in 0..q.len() {
                q[k] += l * e[k];
            }
        }
        let d = dot(&sub(p, &q), &sub(p, &q));
        // ties: the face of most vertices that still holds it exactly (inside before an edge)
        if best.as_ref().is_none_or(|b| d < b.0 - 1e-15 * (1.0 + b.0)) {
            best = Some((d, q, idx));
        }
    }
    let (_, q, idx) = best.unwrap_or_else(|| (0.0, v[0].clone(), vec![0]));
    (q, idx)
}

/// The exact nearest point to `p` in the relative interior of the face `idx` of the points `v` (`None` for faces
/// this cannot write down in closed form).
fn face_nearest<'h>(e: E<'h>, p: &[T<'h>], v: &[Vec<T<'h>>], idx: &[usize]) -> Option<Vec<T<'h>>> {
    let n = p.len();
    Some(match idx.len() {
        1 => v[idx[0]].clone(),
        k if k == n + 1 => p.to_vec(),
        2 => {
            let (a, b) = (&v[idx[0]], &v[idx[1]]);
            let ab = e.vsub(b, a);
            let t = e.div(e.dot(&e.vsub(p, a), &ab), e.norm2(&ab));
            e.vadd(a, &e.scale(t, &ab))
        }
        3 if n == 3 => {
            let (a, b, c) = (&v[idx[0]], &v[idx[1]], &v[idx[2]]);
            let nn = e.cross(&e.vsub(b, a), &e.vsub(c, a));
            let t = e.div(e.dot(&e.vsub(p, a), &nn), e.norm2(&nn));
            e.vsub(p, &e.scale(t, &nn))
        }
        _ => return None,
    })
}

/// The distance from `p` to the face `idx` (as [`face_nearest`]), in the simplest closed form.
fn face_distance<'h>(e: E<'h>, p: &[T<'h>], v: &[Vec<T<'h>>], idx: &[usize]) -> Option<T<'h>> {
    let n = p.len();
    Some(match idx.len() {
        1 => e.dist(p, &v[idx[0]]),
        k if k == n + 1 => e.int(0),
        2 => {
            let (a, b) = (&v[idx[0]], &v[idx[1]]);
            let (ap, ab) = (e.vsub(p, a), e.vsub(b, a));
            if n == 2 {
                e.div(e.f("abs", &[e.cross2(&ab, &ap)]), e.sqrt(e.norm2(&ab)))
            } else {
                e.sqrt(e.sub(e.norm2(&ap), e.div(e.sq(e.dot(&ap, &ab)), e.norm2(&ab))))
            }
        }
        3 if n == 3 => {
            let (a, b, c) = (&v[idx[0]], &v[idx[1]], &v[idx[2]]);
            let nn = e.cross(&e.vsub(b, a), &e.vsub(c, a));
            e.div(e.f("abs", &[e.dot(&e.vsub(p, a), &nn)]), e.sqrt(e.norm2(&nn)))
        }
        _ => return None,
    })
}

/// The nearest point and distance among several simplices (point sets), exact.
struct Best<'h> {
    d: f64,
    near: Option<Vec<T<'h>>>,
    dist: Option<T<'h>>,
}

fn consider<'h>(e: E<'h>, best: &mut Option<Best<'h>>, p: &[T<'h>], pn: &[f64], v: &[Vec<T<'h>>], vn: &[Vec<f64>]) {
    let (q, idx) = nearest_simplex(pn, vn);
    let d = dot(&sub(pn, &q), &sub(pn, &q)).sqrt();
    if best.as_ref().is_none_or(|b| d < b.d) {
        let near = face_nearest(e, p, v, &idx);
        let dist = face_distance(e, p, v, &idx);
        *best = Some(Best { d, near, dist });
    }
}

/// Whether a plane point is inside the polygon's rings (even-odd), on the boundary counting as inside.
pub fn in_polygon(p: &[f64], rings: &[Vec<Vec<f64>>]) -> bool {
    let mut inside = false;
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (&ring[i], &ring[(i + 1) % n]);
            // on the edge
            let (q, _) = nearest_simplex(p, &[a.clone(), b.clone()]);
            let scale = 1.0 + a[0].abs().max(a[1].abs()).max(b[0].abs()).max(b[1].abs());
            if dot(&sub(p, &q), &sub(p, &q)).sqrt() <= 1e-12 * scale {
                return true;
            }
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if p[0] < x {
                    inside = !inside;
                }
            }
        }
    }
    inside
}

/// The rings of a plane polygon region, numerically, with the exact rings beside them.
#[allow(clippy::type_complexity)]
fn polygon_rings<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<(Vec<Vec<Vec<T<'h>>>>, Vec<Vec<Vec<f64>>>)>> {
    let Region::Polygon(rs) = r else { return Ok(None) };
    let mut exact = vec![];
    for ring in rs {
        exact.push(ring.outer.clone());
        exact.extend(ring.holes.iter().cloned());
    }
    let mut numeric = vec![];
    for ring in &exact {
        match points(cx, ring)? {
            Some(v) => numeric.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some((exact, numeric)))
}

/// The pieces of a region as simplices (exact and numeric), for distances: points, segments, triangles of a
/// polygon in space, and full-dimensional simplices. `None` for regions measured otherwise.
#[allow(clippy::type_complexity)]
fn pieces<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, boundary: bool) -> Res<Option<Vec<(Vec<Vec<T<'h>>>, Vec<Vec<f64>>)>>> {
    let mut out: Vec<Vec<Vec<T<'h>>>> = vec![];
    match r {
        Region::Points(ps) => out.extend(ps.iter().map(|p| vec![p.clone()])),
        Region::Lines(ls) => {
            for l in ls {
                out.extend(l.windows(2).map(|w| w.to_vec()));
            }
        }
        Region::Polygon(rs) => {
            for ring in rs {
                let mut all = vec![&ring.outer];
                all.extend(ring.holes.iter());
                for pts in all {
                    let n = pts.len();
                    if pts[0].len() == 2 || boundary {
                        out.extend((0..n).map(|i| vec![pts[i].clone(), pts[(i + 1) % n].clone()]));
                    } else {
                        out.extend((1..n - 1).map(|i| vec![pts[0].clone(), pts[i].clone(), pts[i + 1].clone()]));
                    }
                }
            }
        }
        Region::Simplex(v) => {
            if boundary && v.len() == v[0].len() + 1 {
                // the facets
                for skip in 0..v.len() {
                    out.push(v.iter().enumerate().filter(|(i, _)| *i != skip).map(|(_, p)| p.clone()).collect());
                }
            } else {
                out.push(v.clone());
            }
        }
        _ => return Ok(None),
    }
    let mut res = vec![];
    for s in out {
        match points(cx, &s)? {
            Some(n) => res.push((s, n)),
            None => return Ok(None),
        }
    }
    Ok(Some(res))
}

/// What a distance query asks for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Query {
    /// the distance (0 inside)
    Distance,
    /// the distance, negative inside
    Signed,
    /// the nearest point
    Nearest,
}

fn is_numeric(v: &Option<Vec<f64>>) -> bool {
    v.as_ref().is_some_and(|v| v.iter().all(|x| x.is_finite()))
}

/// Whether the region (of full dimension) holds the point, numerically.
fn inside_numeric<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, pn: &[f64]) -> Res<Option<bool>> {
    Ok(match r {
        Region::Polygon(_) => {
            let Some((_, rings)) = polygon_rings(cx, r)? else { return Ok(None) };
            Some(in_polygon(pn, &rings))
        }
        Region::Simplex(v) if v.len() == v[0].len() + 1 => {
            let Some(vn) = points(cx, v)? else { return Ok(None) };
            let (q, _) = nearest_simplex(pn, &vn);
            Some(dot(&sub(pn, &q), &sub(pn, &q)).sqrt() <= EPS * (1.0 + dot(pn, pn).sqrt()))
        }
        _ => None,
    })
}

/// The answer to a distance query, as a term to evaluate; `None` when it is not known.
pub fn query<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, p: &[T<'h>], q: Query) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    if p.len() != r.embedding() {
        return Ok(None);
    }
    let pn = nums(cx, p)?;
    let numeric = is_numeric(&pn);
    if let Region::Ball { c, r: Radius::One(rad), solid } = r {
            let d = e.dist(p, c);
            let edge = e.sub(d, *rad);
            if q == Query::Nearest {
                // c + r (p - c)/|p - c| outside (or on a sphere); p itself inside a ball
                let on = e.vadd(c, &e.scale(e.div(*rad, d), &e.vsub(p, c)));
                if !*solid {
                    return Ok(Some(e.list(&on)));
                }
                return Ok(match (numeric, num(cx, edge)?) {
                    (true, Some(v)) if v <= 0.0 => Some(e.list(p)),
                    (true, Some(_)) => Some(e.list(&on)),
                    _ => None,
                });
            }
            return Ok(Some(match (q, solid) {
                (Query::Signed, true) => edge,
                (_, false) => e.f("abs", &[edge]),
                _ => match num(cx, edge)? {
                    Some(v) if numeric && v <= 0.0 => e.int(0),
                    Some(_) if numeric => edge,
                    _ => e.f("max", &[e.int(0), edge]),
                },
            }));
    }
    if let Region::Boxed { lo, hi } = r {
        return boxed(cx, lo, hi, p, pn.as_deref().filter(|_| numeric), q);
    }
    if let Region::Intervals(iv) = r {
        // the nearest interval's box
        let Some(pn) = pn.filter(|_| numeric) else { return Ok(None) };
        let mut best: Option<(f64, usize)> = None;
        for (k, &(a, b)) in iv.iter().enumerate() {
            let (Some(a), Some(b)) = (num(cx, a)?, num(cx, b)?) else { return Ok(None) };
            let d = if pn[0] < a.min(b) { a.min(b) - pn[0] } else if pn[0] > a.max(b) { pn[0] - a.max(b) } else { 0.0 };
            if best.is_none_or(|x| d < x.0) {
                best = Some((d, k));
            }
        }
        let Some((_, k)) = best else { return Ok(None) };
        return boxed(cx, &[iv[k].0], &[iv[k].1], p, Some(&pn), q);
    }
    if let Region::Points(ps) = r {
        if !numeric {
            if q == Query::Nearest {
                return Ok(None);
            }
            let ds: Vec<T<'h>> = ps.iter().map(|x| e.dist(p, x)).collect();
            return Ok(Some(if ds.len() == 1 { ds[0] } else { e.f("min", &ds) }));
        }
    }
    let Some(pn) = pn.filter(|_| numeric) else { return Ok(None) };
    let full = matches!(r, Region::Polygon(rs) if rs[0].outer[0].len() == 2) || matches!(r, Region::Simplex(v) if v.len() == v[0].len() + 1);
    let inside = if full { inside_numeric(cx, r, &pn)? } else { Some(false) };
    let Some(inside) = inside else { return Ok(None) };
    if inside && q != Query::Signed {
        return Ok(Some(match q {
            Query::Nearest => e.list(p),
            _ => e.int(0),
        }));
    }
    // the boundary when the point is inside (signed distance), else the region's pieces
    let Some(ps) = pieces(cx, r, inside || (q == Query::Signed && full))? else { return Ok(None) };
    let mut best = None;
    for (s, sn) in &ps {
        consider(e, &mut best, p, &pn, s, sn);
    }
    let Some(b) = best else { return Ok(None) };
    Ok(match q {
        Query::Nearest => b.near.map(|v| e.list(&v)),
        Query::Distance => b.dist.or_else(|| Some(h.real(b.d))),
        Query::Signed => {
            let d = b.dist.unwrap_or_else(|| h.real(b.d));
            Some(if inside { e.neg(d) } else { d })
        }
    })
}

/// Distance queries on an axis-aligned box.
fn boxed<'h>(cx: &mut NativeCx<'_, '_, 'h>, lo: &[T<'h>], hi: &[T<'h>], p: &[T<'h>], pn: Option<&[f64]>, q: Query) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    let Some(pn) = pn else {
        // symbolic: sqrt(sum(max(0, lo - x, x - hi)^2))
        if q != Query::Distance {
            return Ok(None);
        }
        let terms: Vec<T<'h>> = (0..p.len()).map(|k| e.sq(e.f("max", &[e.int(0), e.sub(lo[k], p[k]), e.sub(p[k], hi[k])]))).collect();
        return Ok(Some(e.sqrt(e.add(&terms))));
    };
    let mut near = vec![];
    let mut inside = true;
    // the least distance to a face from inside, with the face's coordinate
    let mut wall: Option<(f64, T<'h>)> = None;
    for k in 0..p.len() {
        let (Some(a), Some(b)) = (num(cx, lo[k])?, num(cx, hi[k])?) else { return Ok(None) };
        let (l, u, lt, ut) = if a <= b { (a, b, lo[k], hi[k]) } else { (b, a, hi[k], lo[k]) };
        if pn[k] < l {
            inside = false;
            near.push(lt);
        } else if pn[k] > u {
            inside = false;
            near.push(ut);
        } else {
            near.push(p[k]);
        }
        for (d, t) in [(pn[k] - l, e.sub(p[k], lt)), (u - pn[k], e.sub(ut, p[k]))] {
            if wall.as_ref().is_none_or(|w| d < w.0) {
                wall = Some((d, t));
            }
        }
    }
    Ok(Some(match q {
        Query::Nearest => e.list(&near),
        Query::Signed if inside => e.neg(wall.map_or(e.int(0), |w| w.1)),
        _ if inside => e.int(0),
        _ => e.dist(p, &near),
    }))
}

/// The condition that `p` is in the region, as a term to evaluate (`None` when it cannot be written; numeric
/// points are then decided directly).
pub fn member<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, p: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let e = E { h };
    if p.len() != r.embedding() {
        return Ok(Some(e.sym("false")));
    }
    let and = |v: Vec<T<'h>>| if v.len() == 1 { v[0] } else { e.f("and", &v) };
    let or = |v: Vec<T<'h>>| if v.len() == 1 { v[0] } else { e.f("or", &v) };
    Ok(Some(match r {
        Region::Ball { c, r: rad, solid } => {
            let rel = if *solid { "less_equal" } else { "equal" };
            match rad {
                // |p - c|^2 against r^2
                Radius::One(rr) => e.f(rel, &[e.norm2(&e.vsub(p, c)), e.sq(*rr)]),
                Radius::Axes(rs) => {
                    let lhs = e.add(&(0..p.len()).map(|k| e.div(e.sq(e.sub(p[k], c[k])), e.sq(rs[k]))).collect::<Vec<_>>());
                    e.f(rel, &[lhs, e.int(1)])
                }
            }
        }
        Region::Boxed { lo, hi } => {
            let mut parts = vec![];
            for k in 0..p.len() {
                let (l, u) = match (num(cx, lo[k])?, num(cx, hi[k])?) {
                    (Some(a), Some(b)) if a > b => (hi[k], lo[k]),
                    _ => (lo[k], hi[k]),
                };
                parts.push(e.f("less_equal", &[l, p[k]]));
                parts.push(e.f("less_equal", &[p[k], u]));
            }
            and(parts)
        }
        Region::Annulus { c, r0, r1 } => {
            let d2 = e.norm2(&e.vsub(p, c));
            and(vec![e.f("less_equal", &[e.sq(*r0), d2]), e.f("less_equal", &[d2, e.sq(*r1)])])
        }
        Region::Intervals(iv) => or(iv.iter().map(|&(a, b)| e.f("and", &[e.f("less_equal", &[a, p[0]]), e.f("less_equal", &[p[0], b])])).collect()),
        Region::Points(ps) => or(ps.iter().map(|q| and((0..p.len()).map(|k| e.f("equal", &[p[k], q[k]])).collect())).collect()),
        Region::Implicit { cond, vars } => {
            let rules: Vec<T<'h>> = vars.iter().zip(p).map(|(&v, &x)| e.f("rule", &[v, x])).collect();
            e.f("rewrite", &[*cond, e.list(&rules)])
        }
        _ => {
            let Some(pn) = nums(cx, p)? else { return Ok(None) };
            let d = match r {
                Region::Polygon(rs) if rs[0].outer[0].len() == 2 => {
                    let Some((_, rings)) = polygon_rings(cx, r)? else { return Ok(None) };
                    return Ok(Some(e.sym(if in_polygon(&pn, &rings) { "true" } else { "false" })));
                }
                _ => {
                    let Some(ps) = pieces(cx, r, false)? else { return Ok(None) };
                    ps.iter()
                        .map(|(_, sn)| {
                            let (q, _) = nearest_simplex(&pn, sn);
                            dot(&sub(&pn, &q), &sub(&pn, &q)).sqrt()
                        })
                        .fold(f64::INFINITY, f64::min)
                }
            };
            let scale = 1.0 + pn.iter().fold(0.0f64, |a, b| a.max(b.abs()));
            e.sym(if d <= 1e-10 * scale { "true" } else { "false" })
        }
    }))
}

/// A heap's `true`/`false`.
pub fn truth<'h>(h: &'h Heap, b: bool) -> T<'h> {
    h.sym_term(h.sym(if b { "true" } else { "false" }))
}
