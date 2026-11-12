//! Hypergeometric functions on balls, real parameters and argument (decision SF.4).
//!
//! - **The series** ([`pfq_sum`]): t_(k+1) / t_k = z prod (k + a_i) / (prod (k + b_j) (k + 1)). With A_i >= |a_i| and
//!   B_j >= |b_j| upper bounds over the balls and K > max B_j, pair each a_i with a b_j: (k + A)/(k - B) decreases
//!   for k >= K; a leftover a (p = q + 1) goes with (k + 1): (k + A)/(k + 1) <= max(1, (K + A)/(K + 1)); leftover
//!   b's give 1/(k - B) and the 1/(k + 1) decreasing factors. Their product at K times |z| bounds every ratio from
//!   K on, rho; when rho < 1 the tail from t_K is at most |t_K| / (1 - rho), added to the radius. An exact
//!   non-positive integer a_i makes a finite sum (no tail); a b_j ball reaching a non-positive integer within the
//!   sum has no value. The loop polls the stop flag.
//! - **Cancellation**: the working precision gets the bits of the largest term over 1 (estimated in machine
//!   arithmetic before the first round), and [`refine`] doubles it while the result is too wide.
//! - **2F1**: |z| <= 1/2 the series; -1 <= z < -1/2 Pfaff's (1-z)^-a 2F1(a, c-b; c; z/(z-1)); 1/2 < z < 1 the
//!   connection formula in 1 - z (DLMF 15.8.4) when c - a - b is a ball without integers, otherwise (an integer
//!   c-a-b, the degenerate case) the series itself, convergent for |z| < 1 (ratio about z: wp / log2(1/z) terms;
//!   no limit formula); z < -1 Pfaff's transformation, then the same in w = z/(z-1) with 1 - w = 1/(1-z) (b - a
//!   decides the degenerate case); z = 1 Gauss's Gamma(c)Gamma(c-a-b)/(Gamma(c-a)Gamma(c-b)) for c-a-b > 0.
//! - **1F1**: z >= 0 the series (positive terms for a, b > 0; about z terms: arguments above 2^20 have no value);
//!   z < 0 Kummer's e^z M(b - a, b, -z) unless the series is finite.
//! - **0F1**: the series, with about 2.9 sqrt|z| extra bits for z < 0 (the terms reach e^(2 sqrt|z|)).
//! - **pFq** in general: p <= q any z, p = q + 1 for |z| < 1, finite sums for any p, q.
use super::gamma::gamma_ball;
use super::{f64_of, float_of, int, is_pos, log2_up, lower, mag_up, real, real_ball, refine, upper, widen};
use mc_arith::num::{stopped, Ball, Mag};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;

/// The largest number of series terms.
const MAX_TERMS: u64 = 1 << 22;
/// The largest 1F1 argument (the series takes about z terms).
const MAX_1F1_ARG: f64 = 1048576.0;

/// An upper bound of a ball's magnitude as a double.
fn up_f(b: &Ball) -> f64 {
    let v = f64_of(&mag_up(b).to_float());
    (v * (1.0 + 1e-12) + f64::MIN_POSITIVE).min(f64::MAX)
}

/// A lower bound of a ball as a double, when it is at least 0 (a positive real parameter), else `None`.
fn nonneg_low(b: &Ball) -> Option<f64> {
    let lo = lower(b);
    if lo.is_zero() {
        return Some(0.0);
    }
    if lo.neg {
        return None;
    }
    Some((f64_of(&lo) * (1.0 - 1e-12)).min(f64::MAX))
}

/// A double rounded up as a magnitude.
fn mag_f(v: f64) -> Mag {
    Mag::from_float_up(&float_of(v * (1.0 + 1e-12)))
}

/// The value m >= 0 of an exact ball equal to the integer -m <= 0.
fn nonpos_int(b: &Ball) -> Option<u64> {
    if !b.rad.is_zero() {
        return None;
    }
    let n = super::float_int(&b.mid)?.to_i64()?;
    (n <= 0).then_some(n.unsigned_abs())
}

/// The bound rho of the term ratios from k = K on (`None` if p > q + 1, or K <= max B_j for a parameter b_j that is
/// not known to be non-negative). A non-negative b_j (lower bound `bl[j]`) gives |k + b_j| >= k + bl_j, so the
/// bound holds from any K: the factor (k + A)/(k + bl) is at most max(1, (K + A)/(K + bl)) for every k >= K (it
/// decreases when A >= bl and stays below 1 otherwise). This is what lets a lower parameter near 1e308 settle after
/// a few terms.
fn ratio_bound(aa: &[f64], bb: &[f64], bl: &[Option<f64>], za: f64, k: f64) -> Option<f64> {
    let (p, q) = (aa.len(), bb.len());
    if p > q + 1 {
        return None;
    }
    let mut r = za;
    for (i, &bj) in bb.iter().enumerate() {
        match bl[i] {
            Some(l) => {
                if i < p {
                    r *= ((k + aa[i]) / (k + l)).max(1.0);
                } else {
                    r /= k + l;
                }
            }
            None => {
                if k <= bj {
                    return None;
                }
                r /= k - bj;
                if i < p {
                    r *= k + aa[i];
                }
            }
        }
    }
    if p == q + 1 {
        r *= ((k + aa[p - 1]) / (k + 1.0)).max(1.0);
    } else {
        r /= k + 1.0;
    }
    Some(r * (1.0 + 1e-12) * (1.0 + 1e-15 * (p + q + 2) as f64))
}

/// Does a non-terminating series with p <= q parameters and |z| = `za` need more than `MAX_TERMS` terms? Its terms
/// grow until k is about |z|^(1/(q + 1 - p)), so a larger argument cannot settle within the limit.
fn too_many_terms(p: usize, q: usize, za: f64) -> bool {
    q + 1 > p && (!za.is_finite() || za >= f64::MAX || za.powf(1.0 / (q + 1 - p) as f64) > MAX_TERMS as f64)
}

/// The series sum_k prod (a_i)_k / prod (b_j)_k z^k / k! with a rigorous bound of its tail (`None` when it
/// diverges for these arguments, does not settle within the term limit, reaches a pole of the b's, or the stop
/// flag is raised).
pub fn pfq_sum(a: &[Ball], b: &[Ball], z: &Ball, wp: u64) -> Option<Ball> {
    let stop = a.iter().filter_map(nonpos_int).min();
    if z.mid.is_zero() && z.rad.is_zero() {
        return Some(int(1));
    }
    let aa: Vec<f64> = a.iter().map(up_f).collect();
    let bb: Vec<f64> = b.iter().map(up_f).collect();
    let za = up_f(z);
    if stop.is_none() && (aa.iter().any(|v| !v.is_finite() || *v >= f64::MAX) || too_many_terms(a.len(), b.len(), za)) {
        return None;
    }
    let bl: Vec<Option<f64>> = b.iter().map(nonneg_low).collect();
    let all_nonneg = bl.iter().all(Option::is_some);
    let bmax = if all_nonneg { -1.0 } else { bb.iter().copied().fold(0.0, f64::max) };
    if stop.is_none() && a.len() == b.len() + 1 {
        // the terms decay like |z|^k: decline at once when that needs more than the term limit
        if za >= 1.0 || (wp as f64 + 64.0) / -za.log2() > MAX_TERMS as f64 {
            return None;
        }
    }
    let mut t = int(1);
    let mut s = int(1);
    for k in 0..MAX_TERMS {
        if k % 32 == 0 && stopped() {
            return None;
        }
        if Some(k) == stop {
            return Some(s);
        }
        let kf = k as f64;
        // t is t_k; the tail from here on
        if stop.is_none() && kf > bmax + 1.0 {
            if let Some(rho) = ratio_bound(&aa, &bb, &bl, za, kf) {
                let target = log2_up(&s) - wp as f64 - 4.0;
                if rho < 1.0 && log2_up(&t) - (1.0 - rho).log2() < target {
                    return Some(widen(s.sub(&t, wp), &mag_up(&t).mul(&mag_f(1.0 / (1.0 - rho)))));
                }
            }
        }
        let mut num = z.clone();
        for ai in a {
            num = num.mul(&ai.add(&int(k as i64), wp), wp);
        }
        let mut den = int(k as i64 + 1);
        for bj in b {
            den = den.mul(&bj.add(&int(k as i64), wp), wp);
        }
        t = t.mul(&num, wp).div(&den, wp)?;
        s = s.add(&t, wp);
    }
    None
}

/// log2 of the largest term of the series over 1, in machine arithmetic (for the working precision).
fn max_term_log2(a: &[f64], b: &[f64], z: f64) -> f64 {
    if z == 0.0 {
        return 0.0;
    }
    let pmax = a.iter().chain(b).map(|v| v.abs()).fold(0.0, f64::max);
    let lz = z.abs().log2();
    let (mut l, mut big) = (0.0f64, 0.0f64);
    for k in 0..1_000_000u64 {
        let kf = k as f64;
        let mut d = lz - (kf + 1.0).log2();
        for &ai in a {
            if ai + kf == 0.0 {
                return big;
            }
            d += (ai + kf).abs().log2();
        }
        for &bj in b {
            d -= (bj + kf).abs().max(1e-300).log2();
        }
        l += d;
        big = big.max(l);
        if kf > 2.0 * pmax + 2.0 && d < 0.0 && l < big - 64.0 {
            break;
        }
    }
    big
}

fn approx(r: &Real) -> f64 {
    match r {
        Real::Exact(q) => crate::bernoulli::rat_to_f64(q),
        Real::Ball(b, _) => f64_of(&b.mid),
        Real::Machine(v) => *v,
    }
}

/// The working-precision guard for a series with these parameters.
fn guard_for(a: &[f64], b: &[f64], z: f64) -> u64 {
    let g = max_term_log2(a, b, z);
    24 + if g.is_finite() { g.clamp(0.0, 1e6) as u64 } else { 0 }
}

/// 1/Gamma(x): zero at an exact pole, `None` for a ball that touches one.
fn rgamma(x: &Ball, wp: u64) -> Option<Ball> {
    if nonpos_int(x).is_some() {
        return Some(int(0));
    }
    int(1).div(&gamma_ball(x, wp)?, wp)
}

/// Does the ball contain an integer?
fn has_int(x: &Ball) -> bool {
    let lo = super::floor_float(&lower(x));
    let hi = super::floor_float(&upper(x));
    match (lo, hi) {
        (Some(l), Some(h)) => l != h || super::float_int(&lower(x)).is_some(),
        _ => true,
    }
}

// ---------------------------------------------------------------- 2F1

/// The connection formula in 1 - z (`omz` = 1 - z in (0, 1/2)), c - a - b without integers.
fn f21_connection(a: &Ball, b: &Ball, c: &Ball, omz: &Ball, wp: u64) -> Option<Ball> {
    let one = int(1);
    let s = c.sub(a, wp).sub(b, wp);
    let gc = gamma_ball(c, wp)?;
    let (ca, cb) = (c.sub(a, wp), c.sub(b, wp));
    let (rca, rcb, ra, rb) = (rgamma(&ca, wp)?, rgamma(&cb, wp)?, rgamma(a, wp)?, rgamma(b, wp)?);
    let mut out = int(0);
    if !(rca.mid.is_zero() && rca.rad.is_zero()) && !(rcb.mid.is_zero() && rcb.rad.is_zero()) {
        let f1 = pfq_sum(&[a.clone(), b.clone()], &[one.sub(&s, wp)], omz, wp)?;
        out = out.add(&gc.mul(&gamma_ball(&s, wp)?, wp).mul(&rca, wp).mul(&rcb, wp).mul(&f1, wp), wp);
    }
    if !(ra.mid.is_zero() && ra.rad.is_zero()) && !(rb.mid.is_zero() && rb.rad.is_zero()) {
        let f2 = pfq_sum(&[ca, cb], &[s.add(&one, wp)], omz, wp)?;
        let pw = el::exp(&el::log(omz, wp)?.mul(&s, wp), wp)?;
        let g = gc.mul(&gamma_ball(&s.neg(), wp)?, wp).mul(&ra, wp).mul(&rb, wp);
        out = out.add(&g.mul(&pw, wp).mul(&f2, wp), wp);
    }
    Some(out)
}

/// 2F1 for 1/2 < z < 1 (`omz` = 1 - z): the connection formula, or the series when c - a - b may be an integer.
fn f21_upper(a: &Ball, b: &Ball, c: &Ball, z: &Ball, omz: &Ball, wp: u64) -> Option<Ball> {
    let s = c.sub(a, wp).sub(b, wp);
    if has_int(&s) {
        return pfq_sum(&[a.clone(), b.clone()], std::slice::from_ref(c), z, wp);
    }
    f21_connection(a, b, c, omz, wp)
}

/// 2F1(a, b; c; z) on balls, z < 1 or z = 1 exactly.
fn f21(a: &Ball, b: &Ball, c: &Ball, z: &Ball, wp: u64) -> Option<Ball> {
    let one = int(1);
    let term = match (nonpos_int(a), nonpos_int(b)) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    };
    if let Some(m) = nonpos_int(c) {
        if term.is_none_or(|t| t > m) {
            return None;
        }
    }
    // 2F1(a, b; b; z) = (1 - z)^-a (DLMF 15.4.6), z < 1: the series would need terms of the order of the
    // convergence of (1 - z)^-a itself (and none at all near z = -inf)
    if nonpos_int(c).is_none() && f64_of(&upper(z)) < 1.0 {
        let same = |x: &Ball| x.rad.is_zero() && c.rad.is_zero() && x.mid == c.mid;
        let other = if same(b) { Some(a) } else if same(a) { Some(b) } else { None };
        if let Some(e) = other {
            let omz = one.sub(z, wp);
            return el::exp(&el::log(&omz, wp)?.mul(&e.neg(), wp), wp);
        }
    }
    let zlo = f64_of(&lower(z));
    let zhi = f64_of(&upper(z));
    let za = up_f(z);
    match z.cmp3(&one) {
        Some(std::cmp::Ordering::Less) => {}
        Some(std::cmp::Ordering::Equal) => {
            if term.is_some() {
                return pfq_sum(&[a.clone(), b.clone()], std::slice::from_ref(c), z, wp);
            }
            let s = c.sub(a, wp).sub(b, wp);
            if !is_pos(&s) {
                return None;
            }
            let g = gamma_ball(c, wp)?.mul(&gamma_ball(&s, wp)?, wp);
            return Some(g.mul(&rgamma(&c.sub(a, wp), wp)?, wp).mul(&rgamma(&c.sub(b, wp), wp)?, wp));
        }
        _ => return None,
    }
    if za <= 0.5 + 1e-9 {
        return pfq_sum(&[a.clone(), b.clone()], std::slice::from_ref(c), z, wp);
    }
    let omz = one.sub(z, wp);
    if term.is_some() {
        if zlo >= -0.5 {
            return pfq_sum(&[a.clone(), b.clone()], std::slice::from_ref(c), z, wp);
        }
        // Pfaff with the terminating parameter first
        let (p, q) = if nonpos_int(a).is_some() && (nonpos_int(b).is_none() || nonpos_int(a) <= nonpos_int(b)) {
            (a, b)
        } else {
            (b, a)
        };
        let w = z.div(&z.sub(&one, wp), wp)?;
        let pre = el::exp(&el::log(&omz, wp)?.mul(&p.neg(), wp), wp)?;
        return Some(pre.mul(&pfq_sum(&[p.clone(), c.sub(q, wp)], std::slice::from_ref(c), &w, wp)?, wp));
    }
    if zlo > 0.5 {
        return f21_upper(a, b, c, z, &omz, wp);
    }
    if zhi < 0.0 {
        // Pfaff: (1-z)^-a 2F1(a, c-b; c; w), w = z/(z-1), 1 - w = 1/(1-z)
        let pre = el::exp(&el::log(&omz, wp)?.mul(&a.neg(), wp), wp)?;
        let w_omz = one.div(&omz, wp)?;
        let w = one.sub(&w_omz, wp);
        let cb = c.sub(b, wp);
        let f = if up_f(&w) <= 0.5 + 1e-9 {
            pfq_sum(&[a.clone(), cb], std::slice::from_ref(c), &w, wp)?
        } else {
            f21_upper(a, &cb, c, &w, &w_omz, wp)?
        };
        return Some(pre.mul(&f, wp));
    }
    None
}

/// The Gauss hypergeometric function 2F1(a; b; c; z) to p bits for z <= 1 (`None` for z > 1, a pole c = 0, -1, ...
/// reached by the series, a divergent z = 1, or z close to -infinity in the degenerate case b - a an integer).
pub fn hyp_2f1(a: &Real, b: &Real, c: &Real, z: &Real, p: u64) -> Option<CBall> {
    let (af, bf, cf, zf) = (approx(a), approx(b), approx(c), approx(z));
    let g =
        if zf.abs() <= 0.5 { guard_for(&[af, bf], &[cf], zf) } else { 24 + (af.abs() + bf.abs() + cf.abs()) as u64 };
    refine(p, g, |wp| {
        let (ab, bb, cb, zb) = (real_ball(a, wp)?, real_ball(b, wp)?, real_ball(c, wp)?, real_ball(z, wp)?);
        Some(real(f21(&ab, &bb, &cb, &zb, wp)?))
    })
}

// ---------------------------------------------------------------- 1F1, 0F1, pFq

/// M(a, b, z) on balls.
fn m11(a: &Ball, b: &Ball, z: &Ball, wp: u64) -> Option<Ball> {
    let term = nonpos_int(a);
    if let Some(m) = nonpos_int(b) {
        if term.is_none_or(|t| t > m) {
            return None;
        }
    }
    if up_f(z) > MAX_1F1_ARG && term.is_none() {
        return None;
    }
    if term.is_none() && f64_of(&upper(z)) < 0.0 {
        let e = el::exp(z, wp)?;
        return Some(e.mul(&pfq_sum(&[b.sub(a, wp)], std::slice::from_ref(b), &z.neg(), wp)?, wp));
    }
    pfq_sum(std::slice::from_ref(a), std::slice::from_ref(b), z, wp)
}

/// Kummer's M(a, b, z) = 1F1(a; b; z) to p bits (`None` at a pole b = 0, -1, ... reached by the series, or for
/// |z| > 2^20 when the series is infinite).
pub fn hyp_1f1(a: &Real, b: &Real, z: &Real, p: u64) -> Option<CBall> {
    let (af, bf, zf) = (approx(a), approx(b), approx(z));
    let g = if zf < 0.0 { guard_for(&[bf - af], &[bf], -zf) } else { guard_for(&[af], &[bf], zf) };
    // the result can be far below the largest term (zeros, Kummer's exponential): the guard is relative to 1
    refine(p, g, |wp| {
        let (ab, bb, zb) = (real_ball(a, wp)?, real_ball(b, wp)?, real_ball(z, wp)?);
        Some(real(m11(&ab, &bb, &zb, wp)?))
    })
}

/// 0F1(; b; z) to p bits (`None` at b = 0, -1, ...).
pub fn hyp_0f1(b: &Real, z: &Real, p: u64) -> Option<CBall> {
    let (bf, zf) = (approx(b), approx(z));
    let g = guard_for(&[], &[bf], zf);
    refine(p, g, |wp| {
        let (bb, zb) = (real_ball(b, wp)?, real_ball(z, wp)?);
        if nonpos_int(&bb).is_some() {
            return None;
        }
        Some(real(pfq_sum(&[], &[bb], &zb, wp)?))
    })
}

/// The generalized hypergeometric function pFq(a; b; z) to p bits: 2F1, 1F1 and 0F1 as above; otherwise the series
/// for p <= q (any z), p = q + 1 (|z| < 1) and finite sums.
pub fn hyp_pfq(a: &[Real], b: &[Real], z: &Real, p: u64) -> Option<CBall> {
    match (a.len(), b.len()) {
        (2, 1) => return hyp_2f1(&a[0], &a[1], &b[0], z, p),
        (1, 1) => return hyp_1f1(&a[0], &b[0], z, p),
        (0, 1) => return hyp_0f1(&b[0], z, p),
        _ => {}
    }
    let af: Vec<f64> = a.iter().map(approx).collect();
    let bf: Vec<f64> = b.iter().map(approx).collect();
    let g = guard_for(&af, &bf, approx(z));
    refine(p, g, |wp| {
        let ab: Vec<Ball> = a.iter().map(|r| real_ball(r, wp)).collect::<Option<_>>()?;
        let bb: Vec<Ball> = b.iter().map(|r| real_ball(r, wp)).collect::<Option<_>>()?;
        let zb = real_ball(z, wp)?;
        let terminating = ab.iter().any(|x| nonpos_int(x).is_some());
        if !terminating && ab.len() == bb.len() + 1 && up_f(&zb) >= 1.0 {
            return None;
        }
        Some(real(pfq_sum(&ab, &bb, &zb, wp)?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_arith::num::{Int, Rat};

    fn q(n: i64, d: i64) -> Real {
        Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)))
    }

    fn close(a: &Ball, b: &Ball, p: u64, tol: f64) -> bool {
        let d = a.sub(b, p);
        d.contains_zero() || f64_of(&d.mid).abs() < tol
    }

    #[test]
    fn identities() {
        let p = 200;
        let tol = 1e-55;
        // 2F1(1, 1; 2; z) = -log(1 - z)/z at z = 1/3, 4/5, -1/2, -7
        for (n, d) in [(1, 3), (4, 5), (-1, 2), (-7, 1)] {
            let v = hyp_2f1(&q(1, 1), &q(1, 1), &q(2, 1), &q(n, d), p).unwrap().re;
            let z = Ball::from_ratio(n, d, p);
            let want = el::log(&int(1).sub(&z, p), p).unwrap().neg().div(&z, p).unwrap();
            assert!(close(&v, &want, p, tol), "2F1 at {n}/{d}");
        }
        // 1F1(1; 2; z) = (e^z - 1)/z at z = 3 and -11/2
        for (n, d) in [(3, 1), (-11, 2)] {
            let v = hyp_1f1(&q(1, 1), &q(2, 1), &q(n, d), p).unwrap().re;
            let z = Ball::from_ratio(n, d, p);
            let want = el::expm1(&z, p).unwrap().div(&z, p).unwrap();
            assert!(close(&v, &want, p, tol), "1F1 at {n}/{d}");
        }
        // erf(x) = 2x/sqrt(pi) 1F1(1/2; 3/2; -x^2) at x = 1: erf(1) = 0.84270079294971486934122063508260925929606699796630...
        let v = hyp_1f1(&q(1, 2), &q(3, 2), &q(-1, 1), p).unwrap().re;
        let two_rpi = int(2).div(&el::sqrt(&super::super::pi(p).unwrap(), p).unwrap(), p).unwrap();
        let erf = two_rpi.mul(&v, p);
        let (dg, e) = erf.mid.to_sci(45);
        assert_eq!((dg.as_str(), e), ("842700792949714869341220635082609259296066998", -1));
        // 0F1(; 1; -x^2/4) = J_0(x) at x = 3
        let v = hyp_0f1(&q(1, 1), &q(-9, 4), p).unwrap().re;
        let j0 = super::super::bessel_j::bessel_j(&q(0, 1), &q(3, 1), p).unwrap().re;
        assert!(close(&v, &j0, p, tol));
        // Gauss: 2F1(1/2, 1/3; 2; 1) = Gamma(2) Gamma(7/6) / (Gamma(3/2) Gamma(5/3)) = 1.1595952669639283657699920515...
        let v = hyp_2f1(&q(1, 2), &q(1, 3), &q(2, 1), &q(1, 1), p).unwrap().re;
        let (dg, e) = v.mid.to_sci(28);
        assert_eq!((dg.as_str(), e), ("1159595266963928365769992052", 0));
        assert!(hyp_2f1(&q(1, 1), &q(1, 1), &q(2, 1), &q(3, 2), p).is_none());
        assert!(hyp_1f1(&q(1, 1), &q(-2, 1), &q(1, 1), p).is_none());
    }
}
