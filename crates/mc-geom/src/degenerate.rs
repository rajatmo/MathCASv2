//! Degenerate primitives read as the regions they are. A triangle with two equal corners is the segment between its
//! distinct corners, three collinear corners the segment between the outer two; a tetrahedron with coplanar corners
//! is the convex polygon they span in their plane; a polygon whose corners lie on one line is a segment; a line whose
//! points coincide, a disk, ball, circle or sphere of radius 0, an annulus of outer radius 0 are a point; an annulus
//! with equal radii is a circle; a cylinder or cone of radius 0 is its axis. Dimension, measure, centroid, bounds,
//! membership, distances and integrals are then those of the actual set: the measure is that of its own dimension
//! (the number of points for a finite set), and the centroid is the mean over the set with that measure.
//!
//! Coincidence and collinearity are decided on the machine values of the coordinates with the relative tolerance of
//! [`crate::props::rank`] (the same test `region_dimension` makes), and the exact coordinates of the chosen corners
//! are kept. Regions with symbolic coordinates are taken as they are (generic position).
use crate::ex::{num, points};
use crate::props::rank;
use crate::region::{Pt, Radius, Region, Ring};
use mc_core::T;
use mc_eval::{NativeCx, Res};

/// The relative tolerance for equal points (as for the rank).
const TOL: f64 = 1e-12;

fn diff(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b).map(|(x, y)| x - y).collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// The scale of a set of points: 1 plus the largest absolute coordinate.
fn scale(pn: &[Vec<f64>]) -> f64 {
    1.0 + pn.iter().flatten().fold(0.0f64, |a, &b| a.max(b.abs()))
}

/// The distinct points of `v` (first occurrences kept), with their machine values.
fn distinct<'h>(v: &[Pt<'h>], pn: &[Vec<f64>]) -> (Vec<Pt<'h>>, Vec<Vec<f64>>) {
    let s = scale(pn);
    let (mut out, mut outn): (Vec<Pt<'h>>, Vec<Vec<f64>>) = (vec![], vec![]);
    for (p, q) in v.iter().zip(pn) {
        if !outn.iter().any(|o| o.iter().zip(q).all(|(a, b)| (a - b).abs() <= TOL * s)) {
            out.push(p.clone());
            outn.push(q.clone());
        }
    }
    (out, outn)
}

/// The dimension of the smallest flat through the points.
fn affine_rank(pn: &[Vec<f64>]) -> usize {
    if pn.len() < 2 {
        return 0;
    }
    rank(&pn[1..].iter().map(|q| diff(q, &pn[0])).collect::<Vec<_>>())
}

/// The convex hull of distinct points spanning a flat of dimension `k` (0, 1 or 2): a point, the segment between
/// the two outermost points, or the convex polygon of the points in their plane (a triangle as a simplex); `None`
/// for a higher dimension.
fn hull<'h>(v: &[Pt<'h>], pn: &[Vec<f64>], k: usize) -> Option<Region<'h>> {
    match k {
        0 => Some(Region::Points(vec![v[0].clone()])),
        1 => {
            let far = (0..pn.len()).max_by(|&i, &j| dot(&diff(&pn[i], &pn[0]), &diff(&pn[i], &pn[0])).total_cmp(&dot(&diff(&pn[j], &pn[0]), &diff(&pn[j], &pn[0]))))?;
            let d = diff(&pn[far], &pn[0]);
            let t: Vec<f64> = pn.iter().map(|q| dot(&diff(q, &pn[0]), &d)).collect();
            let lo = (0..t.len()).min_by(|&i, &j| t[i].total_cmp(&t[j]))?;
            let hi = (0..t.len()).max_by(|&i, &j| t[i].total_cmp(&t[j]))?;
            Some(Region::Simplex(vec![v[lo].clone(), v[hi].clone()]))
        }
        2 => {
            // an orthonormal basis (u, w) of the plane, the points in its coordinates, Andrew's monotone chain
            let len2 = |a: &[f64]| dot(a, a);
            let far = (0..pn.len()).max_by(|&i, &j| len2(&diff(&pn[i], &pn[0])).total_cmp(&len2(&diff(&pn[j], &pn[0]))))?;
            let u0 = diff(&pn[far], &pn[0]);
            let u: Vec<f64> = u0.iter().map(|x| x / len2(&u0).sqrt()).collect();
            let perp = |q: &[f64]| {
                let d = diff(q, &pn[0]);
                let a = dot(&d, &u);
                d.iter().zip(&u).map(|(x, y)| x - a * y).collect::<Vec<f64>>()
            };
            let other = (0..pn.len()).max_by(|&i, &j| len2(&perp(&pn[i])).total_cmp(&len2(&perp(&pn[j]))))?;
            let w0 = perp(&pn[other]);
            let w: Vec<f64> = w0.iter().map(|x| x / len2(&w0).sqrt()).collect();
            let xy: Vec<(f64, f64)> = pn.iter().map(|q| (dot(&diff(q, &pn[0]), &u), dot(&diff(q, &pn[0]), &w))).collect();
            let mut idx: Vec<usize> = (0..xy.len()).collect();
            idx.sort_by(|&i, &j| xy[i].0.total_cmp(&xy[j].0).then(xy[i].1.total_cmp(&xy[j].1)));
            let cross = |o: usize, a: usize, b: usize| (xy[a].0 - xy[o].0) * (xy[b].1 - xy[o].1) - (xy[a].1 - xy[o].1) * (xy[b].0 - xy[o].0);
            let eps = TOL * scale(pn) * scale(pn);
            let mut chain: Vec<usize> = vec![];
            for pass in 0..2 {
                let start = chain.len();
                let order: Vec<usize> = if pass == 0 { idx.clone() } else { idx.iter().rev().copied().collect() };
                for &i in &order {
                    while chain.len() >= start + 2 && cross(chain[chain.len() - 2], chain[chain.len() - 1], i) <= eps {
                        chain.pop();
                    }
                    chain.push(i);
                }
                chain.pop();
            }
            let corners: Vec<Pt<'h>> = chain.iter().map(|&i| v[i].clone()).collect();
            Some(if corners.len() == 3 { Region::Simplex(corners) } else { Region::Polygon(vec![Ring { outer: corners, holes: vec![] }]) })
        }
        _ => None,
    }
}

/// Whether a term is the number 0.
fn is_zero<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<bool> {
    Ok(num(cx, t)? == Some(0.0))
}

/// A ring with consecutive repeated corners (and a repeated first corner at the end) removed.
fn dedupe_ring<'h>(v: &[Pt<'h>], pn: &[Vec<f64>]) -> (Vec<Pt<'h>>, Vec<Vec<f64>>) {
    let s = scale(pn);
    let same = |a: &[f64], b: &[f64]| a.iter().zip(b).all(|(x, y)| (x - y).abs() <= TOL * s);
    let (mut out, mut outn): (Vec<Pt<'h>>, Vec<Vec<f64>>) = (vec![], vec![]);
    for (p, q) in v.iter().zip(pn) {
        if outn.last().is_none_or(|l: &Vec<f64>| !same(l, q)) {
            out.push(p.clone());
            outn.push(q.clone());
        }
    }
    while outn.len() > 1 && same(&outn[0], &outn[outn.len() - 1]) {
        out.pop();
        outn.pop();
    }
    (out, outn)
}

/// The convex hull of a list of points as the region it is (a simplex when they are affinely independent).
fn simplex_region<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: Vec<Pt<'h>>) -> Res<Region<'h>> {
    let Some(pn) = points(cx, &v)? else { return Ok(Region::Simplex(v)) };
    let (d, dn) = distinct(&v, &pn);
    let k = affine_rank(&dn);
    Ok(if d.len() == 1 {
        Region::Points(d)
    } else if k + 1 == d.len() {
        if d.len() == v.len() {
            Region::Simplex(v)
        } else {
            Region::Simplex(d)
        }
    } else {
        hull(&d, &dn, k).unwrap_or(Region::Simplex(v))
    })
}

/// A union of pieces, or the one piece.
fn union_of(mut parts: Vec<Region<'_>>) -> Region<'_> {
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        Region::Union(parts)
    }
}

/// The region a primitive actually is (see the module documentation); other regions are given back as they are.
pub fn actual<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: Region<'h>) -> Res<Region<'h>> {
    Ok(match r {
        Region::Simplex(v) => simplex_region(cx, v)?,
        Region::Polyhedron { verts, faces } => {
            let Some(pn) = points(cx, &verts)? else { return Ok(Region::Polyhedron { verts, faces }) };
            let (d, dn) = distinct(&verts, &pn);
            let k = affine_rank(&dn);
            if k < 3 {
                hull(&d, &dn, k).unwrap_or(Region::Polyhedron { verts, faces })
            } else {
                Region::Polyhedron { verts, faces }
            }
        }
        Region::Polygon(rings) => {
            let mut good: Vec<Ring<'h>> = vec![];
            let mut pieces: Vec<Region<'h>> = vec![];
            for ring in rings {
                let Some(pn) = points(cx, &ring.outer)? else {
                    good.push(ring);
                    continue;
                };
                let (outer, on) = dedupe_ring(&ring.outer, &pn);
                let (d, dn) = distinct(&outer, &on);
                let k = affine_rank(&dn);
                if k < 2 {
                    // the corners on one line: the polygon is the segment between the outermost (or a point); its
                    // holes have no inside to take away
                    pieces.extend(hull(&d, &dn, k));
                    continue;
                }
                let mut holes = vec![];
                for hole in ring.holes {
                    match points(cx, &hole)? {
                        Some(hn) => {
                            let (hv, hvn) = dedupe_ring(&hole, &hn);
                            // a hole without inside takes nothing away
                            if affine_rank(&hvn) >= 2 {
                                holes.push(hv);
                            }
                        }
                        None => holes.push(hole),
                    }
                }
                good.push(Ring { outer, holes });
            }
            let mut parts = vec![];
            if !good.is_empty() {
                parts.push(Region::Polygon(good));
            }
            parts.extend(pieces);
            union_of(parts)
        }
        Region::Lines(ls) => {
            let mut kept: Vec<Vec<Pt<'h>>> = vec![];
            let mut single: Vec<Pt<'h>> = vec![];
            for l in ls {
                let Some(pn) = points(cx, &l)? else {
                    kept.push(l);
                    continue;
                };
                let s = scale(&pn);
                let mut out: Vec<Pt<'h>> = vec![];
                let mut last: Option<&Vec<f64>> = None;
                for (p, q) in l.iter().zip(&pn) {
                    if last.is_none_or(|a| a.iter().zip(q).any(|(x, y)| (x - y).abs() > TOL * s)) {
                        out.push(p.clone());
                        last = Some(q);
                    }
                }
                if out.len() == 1 {
                    single.extend(out);
                } else {
                    kept.push(out);
                }
            }
            if single.is_empty() {
                return Ok(Region::Lines(kept));
            }
            let single = match points(cx, &single)? {
                Some(pn) => distinct(&single, &pn).0,
                None => single,
            };
            let mut parts = vec![];
            if !kept.is_empty() {
                parts.push(Region::Lines(kept));
            }
            parts.push(Region::Points(single));
            union_of(parts)
        }
        Region::Ball { c, r: Radius::One(rad), solid } => {
            if is_zero(cx, rad)? {
                Region::Points(vec![c])
            } else {
                Region::Ball { c, r: Radius::One(rad), solid }
            }
        }
        Region::Ball { c, r: Radius::Axes(rs), solid } => {
            let mut nonzero = vec![];
            for (k, &x) in rs.iter().enumerate() {
                if !is_zero(cx, x)? {
                    nonzero.push(k);
                }
            }
            if nonzero.is_empty() {
                Region::Points(vec![c])
            } else if let [k] = nonzero[..] {
                // one semi-axis left: the ellipse or ellipsoid (solid or not) is the segment along it
                let e = crate::ex::E { h: cx.heap() };
                let end = |s: i64| -> Pt<'h> { c.iter().enumerate().map(|(j, &x)| if j == k { e.add(&[x, e.mul(&[e.int(s), rs[k]])]) } else { x }).collect() };
                let (a, b) = (end(-1), end(1));
                let mut ends = vec![];
                for p in [a, b] {
                    let mut q = Vec::with_capacity(p.len());
                    for x in p {
                        q.push(cx.eval(x)?);
                    }
                    ends.push(q);
                }
                Region::Simplex(ends)
            } else {
                Region::Ball { c, r: Radius::Axes(rs), solid }
            }
        }
        Region::Annulus { c, r0, r1 } => {
            if is_zero(cx, r1)? {
                Region::Points(vec![c])
            } else if num(cx, r0)?.is_some() && num(cx, r0)? == num(cx, r1)? {
                Region::Ball { c, r: Radius::One(r1), solid: false }
            } else {
                Region::Annulus { c, r0, r1 }
            }
        }
        Region::Revolved { a, b, r, cone } => {
            if is_zero(cx, r)? {
                simplex_region(cx, vec![a, b])?
            } else {
                Region::Revolved { a, b, r, cone }
            }
        }
        Region::Union(parts) => {
            let mut out = Vec::with_capacity(parts.len());
            for p in parts {
                out.push(actual(cx, p)?);
            }
            Region::Union(out)
        }
        other => other,
    })
}
