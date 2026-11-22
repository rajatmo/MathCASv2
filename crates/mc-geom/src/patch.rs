//! `b_spline_surface(net)` with the default degree and at most four control points each way: one clamped span,
//! which is the Bezier patch of the same control net (the clamped knot vector of n = d + 1 points has no interior
//! knots, so the B-spline basis is the Bernstein basis; de Boor, "A Practical Guide to Splines", ch. IX).
//!
//! Its area is the integral of |S_u x S_v| over the unit square, by composite Gauss-Legendre quadrature at two
//! resolutions that must agree; its bounds are the least and greatest value of each coordinate, found by
//! subdividing the patch (de Casteljau) with the Bernstein coefficients of a piece as bounds of the coordinate on it
//! (the convex hull property), until the bound and the best value found meet (Farin, "Curves and Surfaces for CAGD",
//! ch. 3 and 16).
use crate::ex::nums;
use crate::region::call_parts;
use mc_core::T;
use mc_eval::{NativeCx, Res};

type Net = Vec<Vec<[f64; 3]>>;

/// The control net of a `b_spline_surface` that is one Bezier patch (2 to 4 points each way, the default degree,
/// no options, numeric points in space).
pub(crate) fn bezier_net<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Net>> {
    let h = cx.heap();
    let Some((name, args)) = call_parts(h, t) else { return Ok(None) };
    let [net] = args else { return Ok(None) };
    if name != "b_spline_surface" {
        return Ok(None);
    }
    let Some(rows) = cx.as_list(*net).map(|l| l.to_vec()) else { return Ok(None) };
    if !(2..=4).contains(&rows.len()) {
        return Ok(None);
    }
    let mut out: Net = vec![];
    for r in rows {
        let Some(pts) = cx.as_list(r).map(|l| l.to_vec()) else { return Ok(None) };
        if !(2..=4).contains(&pts.len()) || out.first().is_some_and(|f: &Vec<[f64; 3]>| f.len() != pts.len()) {
            return Ok(None);
        }
        let mut row = vec![];
        for p in pts {
            let Some(c) = cx.as_list(p).map(|l| l.to_vec()) else { return Ok(None) };
            let Some(v) = nums(cx, &c)? else { return Ok(None) };
            let [x, y, z] = v.as_slice() else { return Ok(None) };
            if ![x, y, z].iter().all(|c| c.is_finite()) {
                return Ok(None);
            }
            row.push([*x, *y, *z]);
        }
        out.push(row);
    }
    Ok(Some(out))
}

/// The Bernstein polynomials of degree n at t.
fn bernstein(n: usize, t: f64) -> Vec<f64> {
    let mut b = vec![1.0];
    for _ in 0..n {
        let mut nb = vec![0.0; b.len() + 1];
        for (i, &x) in b.iter().enumerate() {
            nb[i] += (1.0 - t) * x;
            nb[i + 1] += t * x;
        }
        b = nb;
    }
    b
}

/// The derivatives of the Bernstein polynomials of degree n at t: n (B_{i-1,n-1} - B_{i,n-1}).
fn bernstein_d(n: usize, t: f64) -> Vec<f64> {
    let lower = bernstein(n - 1, t);
    (0..=n)
        .map(|i| {
            let a = if i >= 1 { lower[i - 1] } else { 0.0 };
            let b = if i < n { lower[i] } else { 0.0 };
            n as f64 * (a - b)
        })
        .collect()
}

/// |S_u x S_v| at (u, v).
fn area_element(net: &Net, u: f64, v: f64) -> f64 {
    let (m, n) = (net.len() - 1, net[0].len() - 1);
    let (bu, du, bv, dv) = (bernstein(m, u), bernstein_d(m, u), bernstein(n, v), bernstein_d(n, v));
    let mut su = [0.0; 3];
    let mut sv = [0.0; 3];
    for (i, row) in net.iter().enumerate() {
        for (j, p) in row.iter().enumerate() {
            for k in 0..3 {
                su[k] += du[i] * bv[j] * p[k];
                sv[k] += bu[i] * dv[j] * p[k];
            }
        }
    }
    let c = [su[1] * sv[2] - su[2] * sv[1], su[2] * sv[0] - su[0] * sv[2], su[0] * sv[1] - su[1] * sv[0]];
    (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
}

/// Nodes and weights of 10-point Gauss-Legendre quadrature on [-1, 1].
const GL10: [(f64, f64); 5] = [
    (0.148_874_338_981_631_2, 0.295_524_224_714_752_9),
    (0.433_395_394_129_247_2, 0.269_266_719_309_996_4),
    (0.679_409_568_299_024_4, 0.219_086_362_515_982_04),
    (0.865_063_366_688_984_5, 0.149_451_349_150_580_6),
    (0.973_906_528_517_171_7, 0.066_671_344_308_688_14),
];

fn area_with(net: &Net, pieces: usize) -> f64 {
    let w = 1.0 / pieces as f64;
    let nodes: Vec<(f64, f64)> = (0..pieces)
        .flat_map(|k| {
            let mid = (k as f64 + 0.5) * w;
            GL10.iter().flat_map(move |&(x, wt)| [(mid - x * w / 2.0, wt * w / 2.0), (mid + x * w / 2.0, wt * w / 2.0)])
        })
        .collect();
    let mut s = 0.0;
    for &(u, wu) in &nodes {
        for &(v, wv) in &nodes {
            s += wu * wv * area_element(net, u, v);
        }
    }
    s
}

/// The area of the patch, when two quadrature resolutions agree to 1e-12 relative (otherwise `None`).
pub(crate) fn area(net: &Net) -> Option<f64> {
    let mut prev = area_with(net, 4);
    for pieces in [8, 16, 32] {
        let next = area_with(net, pieces);
        if (next - prev).abs() <= 1e-12 * next.abs().max(1e-300) {
            return Some(next);
        }
        prev = next;
    }
    None
}

/// One coordinate of the patch as a grid of Bernstein coefficients, split at u = 1/2 (rows) or v = 1/2 (columns).
fn split(c: &[Vec<f64>], rows: bool) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let half = |p: &[f64]| -> (Vec<f64>, Vec<f64>) {
        let mut cur = p.to_vec();
        let mut left = vec![cur[0]];
        let mut right = vec![*cur.last().unwrap_or(&0.0)];
        while cur.len() > 1 {
            cur = cur.windows(2).map(|w| (w[0] + w[1]) / 2.0).collect();
            left.push(cur[0]);
            right.push(*cur.last().unwrap_or(&0.0));
        }
        right.reverse();
        (left, right)
    };
    if rows {
        let (m, n) = (c.len(), c[0].len());
        let mut l = vec![vec![0.0; n]; m];
        let mut r = vec![vec![0.0; n]; m];
        for j in 0..n {
            let col: Vec<f64> = c.iter().map(|row| row[j]).collect();
            let (a, b) = half(&col);
            for i in 0..m {
                l[i][j] = a[i];
                r[i][j] = b[i];
            }
        }
        (l, r)
    } else {
        let mut l = vec![];
        let mut r = vec![];
        for row in c {
            let (a, b) = half(row);
            l.push(a);
            r.push(b);
        }
        (l, r)
    }
}

/// The least value of a coordinate given by its Bernstein coefficients over the unit square: a value the patch
/// takes (a corner coefficient of a piece) within 1e-13 of the proved lower bound, or `None`.
fn least(c: Vec<Vec<f64>>) -> Option<f64> {
    let scale = c.iter().flatten().fold(1.0f64, |m, x| m.max(x.abs()));
    let corners = |c: &[Vec<f64>]| {
        let (m, n) = (c.len() - 1, c[0].len() - 1);
        [c[0][0], c[0][n], c[m][0], c[m][n]].into_iter().fold(f64::INFINITY, f64::min)
    };
    let lower = |c: &[Vec<f64>]| c.iter().flatten().copied().fold(f64::INFINITY, f64::min);
    let mut best = corners(&c);
    let mut work = vec![(c, 0usize)];
    let mut steps = 0;
    while let Some((piece, depth)) = work.pop() {
        steps += 1;
        if steps > 200_000 {
            return None;
        }
        best = best.min(corners(&piece));
        if lower(&piece) >= best - 1e-13 * scale {
            continue;
        }
        if depth > 80 {
            return None;
        }
        let (a, b) = split(&piece, depth % 2 == 0);
        work.push((a, depth + 1));
        work.push((b, depth + 1));
    }
    Some(best)
}

/// The bounds of each coordinate of the patch: `[[xmin, xmax], [ymin, ymax], [zmin, zmax]]`.
pub(crate) fn bounds(net: &Net) -> Option<Vec<(f64, f64)>> {
    (0..3)
        .map(|k| {
            let c: Vec<Vec<f64>> = net.iter().map(|row| row.iter().map(|p| p[k]).collect()).collect();
            let neg: Vec<Vec<f64>> = c.iter().map(|row| row.iter().map(|x| -x).collect()).collect();
            Some((least(c)?, -least(neg)?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_bilinear_patch_has_the_area_of_its_parallelogram() {
        let net = vec![vec![[0.0, 0.0, 0.0], [0.0, 2.0, 0.0]], vec![[3.0, 0.0, 0.0], [3.0, 2.0, 0.0]]];
        assert!((area(&net).unwrap() - 6.0).abs() < 1e-13);
        let b = bounds(&net).unwrap();
        assert_eq!(b, vec![(0.0, 3.0), (0.0, 2.0), (0.0, 0.0)]);
    }

    #[test]
    fn bounds_of_a_bump_are_the_value_at_its_top() {
        // z = 4 u (1 - u) on a quadratic net: the middle control height 2 gives the top value 1 at u = 1/2
        let row = |x: f64, z: f64| vec![[x, 0.0, z], [x, 1.0, z]];
        let net = vec![row(0.0, 0.0), row(1.0, 2.0), row(2.0, 0.0)];
        let b = bounds(&net).unwrap();
        assert!((b[2].1 - 1.0).abs() < 1e-12 && b[2].0 == 0.0);
        // the area of the parabolic sheet: width 1 times the arc length of z = 4u(1-u) for x = 2u, u in [0, 1]
        // = int_0^1 sqrt(4 + (4 - 8u)^2) du = [s sqrt(4 + s^2)/2 + 2 asinh(s/2)]/8 from s = -4 to 4
        let exact = (4.0 * 20f64.sqrt() + 4.0 * 2f64.asinh()) / 8.0;
        assert!((area(&net).unwrap() - exact).abs() < 1e-12 * exact);
    }
}
