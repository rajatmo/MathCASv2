//! Properties of regions as exact terms where the region's coordinates allow it: dimension, measure, centroid
//! and bounds.
use crate::ex::{num, points, E};
use crate::region::{Radius, Region, Ring};
use mc_core::T;
use mc_eval::{NativeCx, Res};

/// The dimension of the region (0 points, 1 curves, 2 surfaces, ...); `None` when it cannot be told.
pub fn dimension<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<usize>> {
    let h = cx.heap();
    Ok(Some(match r {
        Region::Points(_) => 0,
        Region::Lines(_) => 1,
        Region::Polygon(_) | Region::Annulus { .. } => 2,
        Region::Ball { c, solid, .. } => c.len() - usize::from(!solid),
        Region::Boxed { lo, hi } => {
            let mut d = 0;
            for (&a, &b) in lo.iter().zip(hi) {
                let e = E { h };
                match num(cx, e.sub(b, a))? {
                    Some(0.0) => {}
                    _ => d += 1,
                }
            }
            d
        }
        Region::Simplex(v) => {
            // the rank of the edge vectors
            let Some(p) = points(cx, v)? else { return Ok(Some(v.len() - 1)) };
            rank(&p.iter().skip(1).map(|q| q.iter().zip(&p[0]).map(|(a, b)| a - b).collect()).collect::<Vec<Vec<f64>>>())
        }
        Region::Intervals(_) => 1,
        Region::Implicit { cond, vars } => {
            // equations each take one dimension away
            let eqs = count_equations(h, *cond);
            vars.len().saturating_sub(eqs)
        }
        Region::Parametric { specs, .. } => specs.len(),
    }))
}

fn count_equations<'h>(h: &'h mc_core::Heap, t: T<'h>) -> usize {
    match crate::region::call_parts(h, t) {
        Some((n, _)) if n == "equal" => 1,
        Some((n, a)) if n == "and" => a.iter().map(|&x| count_equations(h, x)).sum(),
        _ => 0,
    }
}

/// The rank of a list of vectors (Gaussian elimination with a relative tolerance).
pub fn rank(rows: &[Vec<f64>]) -> usize {
    let mut m: Vec<Vec<f64>> = rows.to_vec();
    let scale = m.iter().flatten().fold(0.0f64, |a, &b| a.max(b.abs())).max(1e-300);
    let mut r = 0;
    let cols = m.first().map_or(0, |x| x.len());
    for c in 0..cols {
        let Some(p) = (r..m.len()).max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs())) else { break };
        if m[p][c].abs() <= 1e-12 * scale {
            continue;
        }
        m.swap(r, p);
        let pivot = m[r].clone();
        for (i, row) in m.iter_mut().enumerate() {
            if i != r {
                let f = row[c] / pivot[c];
                for (x, &y) in row.iter_mut().zip(&pivot) {
                    *x -= f * y;
                }
            }
        }
        r += 1;
    }
    r
}

/// The volume of the unit ball in n dimensions: pi^(n/2) / gamma(n/2 + 1).
fn unit_ball<'h>(e: E<'h>, n: usize) -> T<'h> {
    let n = n as i64;
    e.div(e.pow(e.pi(), e.rat(n, 2)), e.f("gamma", &[e.add(&[e.rat(n, 2), e.int(1)])]))
}

/// The signed area of a plane ring (shoelace), anticlockwise positive.
fn shoelace<'h>(e: E<'h>, ring: &[Vec<T<'h>>]) -> T<'h> {
    let n = ring.len();
    let terms: Vec<T<'h>> = (0..n).map(|i| e.cross2(&ring[i], &ring[(i + 1) % n])).collect();
    e.mul(&[e.rat(1, 2), e.add(&terms)])
}

/// The area of a ring in space, through the vector area of its fan of triangles.
fn area3<'h>(e: E<'h>, ring: &[Vec<T<'h>>]) -> T<'h> {
    let n = ring.len();
    let mut s: [Vec<T<'h>>; 3] = [vec![], vec![], vec![]];
    for i in 0..n {
        let c = e.cross(&ring[i], &ring[(i + 1) % n]);
        for k in 0..3 {
            s[k].push(c[k]);
        }
    }
    let v: Vec<T<'h>> = s.iter().map(|x| e.add(x)).collect();
    e.mul(&[e.rat(1, 2), e.sqrt(e.norm2(&v))])
}

/// The area of a ring: its absolute signed area in the plane, the norm of its vector area in space.
fn ring_area<'h>(e: E<'h>, ring: &[Vec<T<'h>>]) -> T<'h> {
    if ring[0].len() == 2 {
        e.f("abs", &[shoelace(e, ring)])
    } else {
        area3(e, ring)
    }
}

fn polygon_area<'h>(e: E<'h>, r: &Ring<'h>) -> T<'h> {
    let mut terms = vec![ring_area(e, &r.outer)];
    for hole in &r.holes {
        terms.push(e.neg(ring_area(e, hole)));
    }
    e.add(&terms)
}

/// The k-volume of the simplex on k + 1 points in n dimensions: sqrt(det(Gram)) / k!, or |det| / n! when k = n.
fn simplex_measure<'h>(e: E<'h>, v: &[Vec<T<'h>>]) -> T<'h> {
    let k = v.len() - 1;
    if k == 0 {
        return e.int(0);
    }
    let edges: Vec<Vec<T<'h>>> = v[1..].iter().map(|p| e.vsub(p, &v[0])).collect();
    let fact: i64 = (1..=k as i64).product();
    if k == v[0].len() {
        let m = e.list(&edges.iter().map(|r| e.list(r)).collect::<Vec<_>>());
        return e.div(e.f("abs", &[e.f("det", &[m])]), e.int(fact));
    }
    let gram: Vec<T<'h>> = edges.iter().map(|a| e.list(&edges.iter().map(|b| e.dot(a, b)).collect::<Vec<_>>())).collect();
    e.div(e.sqrt(e.f("det", &[e.list(&gram)])), e.int(fact))
}

fn ellipse_or_sphere_measure<'h>(e: E<'h>, n: usize, r: &Radius<'h>, solid: bool) -> Option<T<'h>> {
    Some(match (r, solid) {
        (Radius::One(r), true) => e.mul(&[unit_ball(e, n), e.pow(*r, e.int(n as i64))]),
        (Radius::Axes(rs), true) => {
            let mut f = vec![unit_ball(e, n)];
            f.extend(rs.iter().copied());
            e.mul(&f)
        }
        // the surface of the n-ball: n * V_n * r^(n - 1)
        (Radius::One(r), false) => e.mul(&[e.int(n as i64), unit_ball(e, n), e.pow(*r, e.int(n as i64 - 1))]),
        (Radius::Axes(rs), false) if n == 2 => {
            // an ellipse: 4 a E(1 - b^2/a^2)
            let (a, b) = (rs[0], rs[1]);
            e.mul(&[e.int(4), a, e.f("elliptic_e", &[e.sub(e.int(1), e.div(e.sq(b), e.sq(a)))])])
        }
        _ => return None,
    })
}

/// The measure (length, area, volume, ...) of the region in its own dimension, as a term to evaluate; `None`
/// when it is not known in closed form.
pub fn measure<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<T<'h>>> {
    let e = E { h: cx.heap() };
    Ok(Some(match r {
        Region::Points(_) => e.int(0),
        Region::Lines(ls) => {
            let mut terms = vec![];
            for l in ls {
                for w in l.windows(2) {
                    terms.push(e.dist(&w[1], &w[0]));
                }
            }
            e.add(&terms)
        }
        Region::Polygon(rings) => e.add(&rings.iter().map(|r| polygon_area(e, r)).collect::<Vec<_>>()),
        Region::Ball { c, r, solid } => match ellipse_or_sphere_measure(e, c.len(), r, *solid) {
            Some(t) => t,
            None => return Ok(None),
        },
        Region::Boxed { lo, hi } => {
            let mut f = vec![];
            for (&a, &b) in lo.iter().zip(hi) {
                let d = e.sub(b, a);
                match num(cx, d)? {
                    Some(0.0) => {}
                    Some(_) => f.push(e.f("abs", &[d])),
                    None => f.push(d),
                }
            }
            e.mul(&f)
        }
        Region::Simplex(v) => simplex_measure(e, v),
        Region::Intervals(iv) => e.add(&iv.iter().map(|&(a, b)| e.sub(b, a)).collect::<Vec<_>>()),
        Region::Annulus { r0, r1, .. } => e.mul(&[e.pi(), e.sub(e.sq(*r1), e.sq(*r0))]),
        Region::Parametric { fs, specs } => {
            // the integral of sqrt(det(J^T J)) over the parameters
            let us: Vec<T<'h>> = specs.iter().map(|s| s.0).collect();
            let jac: Vec<Vec<T<'h>>> = us.iter().map(|&u| fs.iter().map(|&f| e.f("d", &[f, u])).collect()).collect();
            let density = if us.len() == fs.len() {
                let m = e.list(&jac.iter().map(|r| e.list(r)).collect::<Vec<_>>());
                e.f("abs", &[e.f("det", &[m])])
            } else {
                let g: Vec<T<'h>> = jac.iter().map(|a| e.list(&jac.iter().map(|b| e.dot(a, b)).collect::<Vec<_>>())).collect();
                e.sqrt(e.f("simplify", &[e.f("det", &[e.list(&g)])]))
            };
            let mut args = vec![density];
            for &(u, a, b) in specs {
                args.push(e.list(&[u, a, b]));
            }
            e.f("integrate", &args)
        }
        Region::Implicit { .. } => return Ok(None),
    }))
}

/// Points weighted: sum(w_i p_i) / sum(w_i).
fn weighted<'h>(e: E<'h>, items: &[(T<'h>, Vec<T<'h>>)]) -> Vec<T<'h>> {
    let total = e.add(&items.iter().map(|x| x.0).collect::<Vec<_>>());
    let n = items[0].1.len();
    (0..n)
        .map(|k| {
            let s = e.add(&items.iter().map(|(w, p)| e.mul(&[*w, p[k]])).collect::<Vec<_>>());
            e.div(s, total)
        })
        .collect()
}

fn mean<'h>(e: E<'h>, ps: &[Vec<T<'h>>]) -> Vec<T<'h>> {
    let n = ps.len() as i64;
    (0..ps[0].len()).map(|k| e.mul(&[e.rat(1, n), e.add(&ps.iter().map(|p| p[k]).collect::<Vec<_>>())])).collect()
}

/// The centroid of a plane ring with its signed area (shoelace formulas).
fn ring_centroid<'h>(e: E<'h>, ring: &[Vec<T<'h>>]) -> (T<'h>, Vec<T<'h>>) {
    let n = ring.len();
    let a = shoelace(e, ring);
    let mut sx = vec![];
    let mut sy = vec![];
    for i in 0..n {
        let (p, q) = (&ring[i], &ring[(i + 1) % n]);
        let c = e.cross2(p, q);
        sx.push(e.mul(&[e.add(&[p[0], q[0]]), c]));
        sy.push(e.mul(&[e.add(&[p[1], q[1]]), c]));
    }
    let k = e.div(e.rat(1, 6), a);
    (a, vec![e.mul(&[k, e.add(&sx)]), e.mul(&[k, e.add(&sy)])])
}

/// The centroid of the region, as terms to evaluate.
pub fn centroid<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<Vec<T<'h>>>> {
    let e = E { h: cx.heap() };
    Ok(Some(match r {
        Region::Points(p) => mean(e, p),
        Region::Lines(ls) => {
            let mut items = vec![];
            for l in ls {
                for w in l.windows(2) {
                    items.push((e.dist(&w[1], &w[0]), mean(e, w)));
                }
            }
            weighted(e, &items)
        }
        Region::Polygon(rings) => {
            let mut items = vec![];
            for ring in rings {
                let mut parts = vec![(ring.outer.as_slice(), 1)];
                parts.extend(ring.holes.iter().map(|h| (h.as_slice(), -1)));
                for (pts, sign) in parts {
                    if pts[0].len() == 2 {
                        let (a, c) = ring_centroid(e, pts);
                        // the orientation of each ring decides the sign of its area: made positive (negative for holes)
                        let s = match num(cx, a)? {
                            Some(v) if v < 0.0 => -sign,
                            _ => sign,
                        };
                        items.push((e.mul(&[e.int(s), a]), c));
                    } else {
                        // in space: the fan of triangles from the first vertex
                        for i in 1..pts.len() - 1 {
                            let tri = [pts[0].clone(), pts[i].clone(), pts[i + 1].clone()];
                            let w = area3(e, &tri);
                            items.push((e.mul(&[e.int(sign), w]), mean(e, &tri)));
                        }
                    }
                }
            }
            weighted(e, &items)
        }
        Region::Ball { c, .. } | Region::Annulus { c, .. } => c.clone(),
        Region::Boxed { lo, hi } => mean(e, &[lo.clone(), hi.clone()]),
        Region::Simplex(v) => mean(e, v),
        Region::Intervals(iv) => {
            let items: Vec<_> = iv.iter().map(|&(a, b)| (e.sub(b, a), vec![e.mul(&[e.rat(1, 2), e.add(&[a, b])])])).collect();
            weighted(e, &items)
        }
        Region::Parametric { .. } | Region::Implicit { .. } => return Ok(None),
    }))
}

/// The least and greatest of terms, chosen numerically when they are numbers (the exact term is kept), else
/// `min(...)` and `max(...)`.
fn extremes<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: &[T<'h>]) -> Res<(T<'h>, T<'h>)> {
    let e = E { h: cx.heap() };
    let mut vals = Vec::with_capacity(v.len());
    for &x in v {
        match num(cx, x)? {
            Some(n) => vals.push(n),
            None => return Ok((e.f("min", v), e.f("max", v))),
        }
    }
    let lo = (0..v.len()).min_by(|&i, &j| vals[i].total_cmp(&vals[j])).unwrap_or(0);
    let hi = (0..v.len()).max_by(|&i, &j| vals[i].total_cmp(&vals[j])).unwrap_or(0);
    Ok((v[lo], v[hi]))
}

/// The bounds `[[lo_1, hi_1], ...]` of the region's coordinates, as terms to evaluate; `None` when they are not
/// known (implicit and parametric regions are bounded numerically by the caller).
pub fn bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<Vec<(T<'h>, T<'h>)>>> {
    let e = E { h: cx.heap() };
    let of_points = |cx: &mut NativeCx<'_, '_, 'h>, ps: Vec<&Vec<T<'h>>>| -> Res<Vec<(T<'h>, T<'h>)>> {
        let n = ps[0].len();
        let mut out = vec![];
        for k in 0..n {
            let col: Vec<T<'h>> = ps.iter().map(|p| p[k]).collect();
            out.push(extremes(cx, &col)?);
        }
        Ok(out)
    };
    Ok(Some(match r {
        Region::Points(p) => of_points(cx, p.iter().collect())?,
        Region::Lines(ls) => of_points(cx, ls.iter().flatten().collect())?,
        Region::Polygon(rings) => of_points(cx, rings.iter().flat_map(|r| r.outer.iter()).collect())?,
        Region::Simplex(v) => of_points(cx, v.iter().collect())?,
        Region::Ball { c, r, .. } => c
            .iter()
            .enumerate()
            .map(|(k, &x)| {
                let rk = match r {
                    Radius::One(r) => *r,
                    Radius::Axes(rs) => rs[k],
                };
                (e.sub(x, rk), e.add(&[x, rk]))
            })
            .collect(),
        Region::Boxed { lo, hi } => {
            let mut out = vec![];
            for (&a, &b) in lo.iter().zip(hi) {
                out.push(extremes(cx, &[a, b])?);
            }
            out
        }
        Region::Annulus { c, r1, .. } => c.iter().map(|&x| (e.sub(x, *r1), e.add(&[x, *r1]))).collect(),
        Region::Intervals(iv) => {
            let mut ends = vec![];
            for &(a, b) in iv {
                ends.push(a);
                ends.push(b);
            }
            vec![extremes(cx, &ends)?]
        }
        Region::Implicit { .. } | Region::Parametric { .. } => return Ok(None),
    }))
}

/// Whether the region is bounded: `None` when it cannot be told.
pub fn bounded<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<bool>> {
    Ok(match r {
        Region::Implicit { .. } => None,
        Region::Parametric { specs, .. } => {
            let mut all = true;
            for &(_, a, b) in specs {
                let (a, b) = (num(cx, a)?, num(cx, b)?);
                if !(a.is_some_and(f64::is_finite) && b.is_some_and(f64::is_finite)) {
                    all = false;
                }
            }
            all.then_some(true)
        }
        _ => {
            let Some(b) = bounds(cx, r)? else { return Ok(None) };
            let mut fin = true;
            for (lo, hi) in b {
                // symbolic coordinates are finite
                if let (Some(a), Some(b)) = (num(cx, lo)?, num(cx, hi)?) {
                    fin &= a.is_finite() && b.is_finite();
                }
            }
            Some(fin)
        }
    })
}
