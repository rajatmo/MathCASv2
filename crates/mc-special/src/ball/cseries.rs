//! Complex-ball enclosures by power series (round 20, the precision contract): the hypergeometric series pFq with
//! complex parameters and argument, and through it the Bessel functions J, I (and Y, K of non-integer order) and the
//! Airy functions at complex arguments; digamma and polygamma at complex arguments by the asymptotic series.
//!
//! - **The series** ([`pfq_sum`]): t_(k+1) / t_k = z prod (k + a_i) / (prod (k + b_j) (k + 1)). With A_i >= |a_i|,
//!   B_j >= |b_j| and k > max B_j, |k + a_i| <= k + A_i and |k + b_j| >= k - B_j, so the ratio bound of the real
//!   series (see [`super::hyper`]) holds with the magnitudes: from the first k where it is rho < 1 the tail is at
//!   most |t_k| / (1 - rho), added to both parts' radii. The working precision is raised by the bits of the largest
//!   term (estimated in machine arithmetic) and [`super::refine`] doubles it while the result is too wide.
//! - **Bessel** (DLMF 10.2.2, 10.25.2): J_nu(z) = (z/2)^nu / Gamma(nu + 1) 0F1(; nu + 1; -z^2/4),
//!   I_nu(z) = (z/2)^nu / Gamma(nu + 1) 0F1(; nu + 1; z^2/4), principal powers (so J_nu(-x) = e^(i pi nu) J_nu(x)
//!   for x > 0, the value above the cut); J_-n = (-1)^n J_n, I_-n = I_n. Y_nu = (J_nu cos nu pi - J_-nu) / sin nu pi
//!   and K_nu = pi/2 (I_-nu - I_nu) / sin nu pi for orders whose ball excludes the integers (DLMF 10.2.3, 10.27.4).
//! - **Airy** (DLMF 9.4.1-4 written with 0F1): with w = z^3/9, c1 = Ai(0), c2 = -Ai'(0):
//!   f = 0F1(; 2/3; w), g = z 0F1(; 4/3; w), f' = z^2/2 0F1(; 5/3; w), g' = 0F1(; 1/3; w);
//!   Ai = c1 f - c2 g, Ai' = c1 f' - c2 g', Bi = sqrt 3 (c1 f + c2 g), Bi' = sqrt 3 (c1 f' + c2 g').
//!   The terms reach about e^(|zeta|), zeta = 2/3 z^(3/2), while Ai can be as small as e^-Re zeta: 2.9 |zeta| extra
//!   bits. Used for |z| <= 200.
//! - **digamma** (DLMF 5.11.2, 5.5.2): psi(z) = psi(w) - sum_{j<N} 1/(z + j), w = z + N with Re w >= p/2 + 10, and
//!   psi(w) = log w - 1/(2w) - sum_{k<M} B_2k / (2k w^2k) with remainder at most |B_2M| / (2M |w|^2M) sec(arg w /
//!   2)^(2M+1), and sec(arg w / 2)^2 <= 2 for Re w > 0 (the bound of the same type as Stirling's series for log
//!   gamma, Olver 1974 ch. 8 section 4). polygamma(n, z) (n >= 1): psi^(n)(z) = psi^(n)(w) - (-1)^n n!
//!   sum_{j<N} (z + j)^-(n+1), the asymptotic series of DLMF 5.15.8 with the analogous bound (an extra factor
//!   2^(n+1)).
use super::refine;
use crate::bernoulli::b2n;
use mc_arith::num::{stopped, Ball, Float, Int, Mag, Rat};
use mc_numeric::cball::{self, CBall};
use mc_numeric::consts::Const;
use mc_numeric::elementary as el;

/// The largest number of series terms.
const MAX_TERMS: u64 = 1 << 20;

/// The largest |z| of the Airy series.
const MAX_AIRY: f64 = 200.0;

fn f64_of(b: &Ball) -> f64 {
    super::f64_of(&b.mid)
}

/// An upper bound of |b| as a double.
fn up_real(b: &Ball) -> f64 {
    let v = super::f64_of(&Mag::from_float_up(&b.mid).add(&b.rad).to_float());
    v * (1.0 + 1e-12) + f64::MIN_POSITIVE
}

/// An upper bound of |z| as a double.
pub(crate) fn up(z: &CBall) -> f64 {
    let (a, b) = (up_real(&z.re), up_real(&z.im));
    a.hypot(b) * (1.0 + 1e-12) + f64::MIN_POSITIVE
}

/// The approximate value as a pair of doubles.
pub(crate) fn approx(z: &CBall) -> (f64, f64) {
    (f64_of(&z.re), f64_of(&z.im))
}

/// log2 of an upper bound of |z| (-inf for an exact zero), without the range of a double.
pub(crate) fn log2_up(z: &CBall) -> f64 {
    let (a, b) = (super::log2_up(&z.re), super::log2_up(&z.im));
    let m = a.max(b);
    if m == f64::NEG_INFINITY {
        m
    } else {
        m + 1.0
    }
}

/// The midpoint of a complex ball, exactly.
fn mid_c(z: &CBall) -> CBall {
    CBall::new(Ball::exact(z.re.mid.clone()), Ball::exact(z.im.mid.clone()))
}

/// log2 of 2^a + 2^b (an upper bound with a margin).
fn log2_add(a: f64, b: f64) -> f64 {
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    if hi == f64::NEG_INFINITY {
        return hi;
    }
    hi + (1.0 + (lo - hi).exp2()).log2() + 1e-9
}

/// log2 of a lower bound of |z| for an exact complex ball (the larger part, less a margin).
fn log2_low_exact(z: &CBall) -> f64 {
    super::log2_up(&z.re).max(super::log2_up(&z.im)) - 1e-6
}

/// One step of a product of many complex factors, t_(k+1) = t_k q: the term is kept as an exact midpoint `tm`
/// with a bound 2^`lrel` of its relative error, which grows by the relative radius of q and the rounding of each
/// step (|t q - tm qm| <= |tm qm| (rel_t + rel_q + rel_t rel_q), a rotation-invariant bound). Rectangular complex
/// balls multiplied directly grow their radii by (|Re q| + |Im q|)/|q| per factor (the wrapping effect): for
/// 2F1(1/2, 1/2; 1; w) at w = 0.33 + 0.74 i (|w| = 0.81) the terms' radii grew like 1.06^k, the tail test never
/// passed and the series ran to its term limit (elliptic_k of a complex ball hung, round 28 gap). Returns the term
/// as a ball, the new midpoint and the new bound.
fn polar_step(tm: &CBall, lrel: f64, q: &CBall, wp: u64) -> (CBall, CBall, f64) {
    let qm = mid_c(q);
    let lq = if q.re.rad.is_zero() && q.im.rad.is_zero() {
        f64::NEG_INFINITY
    } else {
        (q.re.rad.add(&q.im.rad)).log2() - log2_low_exact(&qm)
    };
    let p = tm.mul(&qm, wp);
    let pm = mid_c(&p);
    if pm.is_exact_zero() {
        return (p, pm, lrel);
    }
    let lr = if p.re.rad.is_zero() && p.im.rad.is_zero() { f64::NEG_INFINITY } else { (p.re.rad.add(&p.im.rad)).log2() - log2_low_exact(&pm) };
    // (1 + e1)(1 + e2)(1 + e3) - 1 <= (e1 + e2 + e3)(1 + 2^-8) while the sum is below 2^-10
    let mut l = log2_add(log2_add(lrel, lq), lr);
    if l == f64::NEG_INFINITY {
        // every factor so far exact
        return (pm.clone(), pm, l);
    }
    l = if l < -10.0 { l + 0.006 } else { l + 1.0 };
    let r = mag_up_c(&pm).mul(&Mag::pow2(l.ceil() as i64));
    let t = CBall::new(super::widen(pm.re.clone(), &r), super::widen(pm.im.clone(), &r));
    (t, pm, l)
}

/// An upper bound of |z| as a magnitude (no double range limit).
fn mag_up_c(z: &CBall) -> Mag {
    super::mag_up(&z.re).add(&super::mag_up(&z.im))
}

/// The value m >= 0 of an exact ball equal to the integer -m <= 0 (zero imaginary part).
pub(crate) fn nonpos_int(z: &CBall) -> Option<u64> {
    if !z.is_real() || !z.re.rad.is_zero() {
        return None;
    }
    let n = super::float_int(&z.re.mid)?.to_i64()?;
    (n <= 0).then_some(n.unsigned_abs())
}

/// The ball 0 +- m on both parts.
fn widen(z: CBall, m: &Mag) -> CBall {
    CBall::new(super::widen(z.re, m), super::widen(z.im, m))
}

/// A magnitude bound of a double (rounded up).
fn mag_f(v: f64) -> Mag {
    Mag::from_float_up(&super::float_of(v * (1.0 + 1e-12)))
}

fn int(v: i64) -> CBall {
    CBall::from_i64(v)
}

/// A lower bound of Re b as a double when Re b >= 0 (then |k + b| >= k + Re b for every k >= 0), else `None`.
fn nonneg_low(b: &CBall) -> Option<f64> {
    let lo = super::lower(&b.re);
    if lo.is_zero() {
        return Some(0.0);
    }
    if lo.neg {
        return None;
    }
    Some((super::f64_of(&lo) * (1.0 - 1e-12)).min(f64::MAX))
}

/// The bound rho of the term ratios from k on (`None` if p > q + 1, or k <= max B_j for a b_j whose real part is not
/// known to be non-negative). A b_j with a non-negative real part (lower bound `bl[j]`) gives |k + b_j| >= k + bl_j,
/// valid from any k: the factor (k + A)/(k + bl) is at most max(1, (K + A)/(K + bl)) for every k >= K.
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
        for &ai in &aa[q.min(p)..] {
            r *= k + ai;
        }
        r /= k + 1.0;
    }
    Some(r * (1.0 + 1e-12) * (1.0 + 1e-15 * (p + q + 2) as f64))
}

/// Does a non-terminating series with p <= q parameters and |z| = `za` need more than `MAX_TERMS` terms? Its terms
/// grow until k is about |z|^(1/(q + 1 - p)), so a larger argument cannot settle within the limit (this answers
/// at once for |z| = 10^400, where the loop would run to the limit and the refinement repeat it).
fn too_many_terms(p: usize, q: usize, za: f64) -> bool {
    q + 1 > p && (!za.is_finite() || za >= f64::MAX || za.powf(1.0 / (q + 1 - p) as f64) > MAX_TERMS as f64)
}

/// The series sum_k prod (a_i)_k / prod (b_j)_k z^k / k! on complex balls with a rigorous bound of its tail
/// (`None` when it diverges for these arguments, does not settle within the term limit, reaches a pole of the b's,
/// or the stop flag is raised).
pub fn pfq_sum(a: &[CBall], b: &[CBall], z: &CBall, wp: u64) -> Option<CBall> {
    let stop = a.iter().filter_map(nonpos_int).min();
    if z.is_exact_zero() {
        return Some(int(1));
    }
    if a.len() > b.len() + 1 && stop.is_none() {
        return None;
    }
    let aa: Vec<f64> = a.iter().map(up).collect();
    let bb: Vec<f64> = b.iter().map(up).collect();
    let za = up(z);
    let bl: Vec<Option<f64>> = b.iter().map(nonneg_low).collect();
    let bmax = if bl.iter().all(Option::is_some) { -1.0 } else { bb.iter().copied().fold(0.0, f64::max) };
    if stop.is_none() && (aa.iter().any(|v| !v.is_finite() || *v >= f64::MAX) || too_many_terms(a.len(), b.len(), za)) {
        return None;
    }
    if stop.is_none() && a.len() == b.len() + 1 && (za >= 1.0 || (wp as f64 + 64.0) / -za.log2() > MAX_TERMS as f64) {
        return None;
    }
    let mut t = int(1);
    // the term as an exact midpoint and a bound of its relative error (see `polar_step`)
    let mut tm = int(1);
    let mut lrel = f64::NEG_INFINITY;
    let mut s = int(1);
    for k in 0..MAX_TERMS {
        if k % 32 == 0 && stopped() {
            return None;
        }
        if Some(k) == stop {
            return Some(s);
        }
        let kf = k as f64;
        if stop.is_none() && kf > bmax + 1.0 {
            if let Some(rho) = ratio_bound(&aa, &bb, &bl, za, kf) {
                let target = log2_up(&s) - wp as f64 - 4.0;
                if rho < 1.0 && log2_up(&t) - (1.0 - rho).log2() < target {
                    let m = mag_up_c(&t).mul(&mag_f(1.0 / (1.0 - rho)));
                    return Some(widen(s.sub(&t, wp), &m));
                }
            }
        }
        let kb = int(k as i64);
        let mut num = z.clone();
        for ai in a {
            num = num.mul(&ai.add(&kb, wp), wp);
        }
        let mut den = int(k as i64 + 1);
        for bj in b {
            let d = bj.add(&kb, wp);
            if d.contains_zero() {
                return None;
            }
            den = den.mul(&d, wp);
        }
        let q = num.div(&den, wp)?;
        (t, tm, lrel) = polar_step(&tm, lrel, &q, wp);
        s = s.add(&t, wp);
    }
    None
}

/// log2 of the largest term of the series over 1, in machine arithmetic.
pub(crate) fn max_term_log2(a: &[(f64, f64)], b: &[(f64, f64)], za: f64) -> f64 {
    if za == 0.0 {
        return 0.0;
    }
    let m = |(x, y): (f64, f64), k: f64| ((x + k) * (x + k) + y * y).sqrt();
    let pmax = a.iter().chain(b).map(|&(x, y)| (x * x + y * y).sqrt()).fold(0.0, f64::max);
    let lz = za.log2();
    let (mut l, mut big) = (0.0f64, 0.0f64);
    for k in 0..4_000_000u64 {
        let kf = k as f64;
        let mut d = lz - (kf + 1.0).log2();
        for &ai in a {
            let v = m(ai, kf);
            if v == 0.0 {
                return big;
            }
            d += v.log2();
        }
        for &bj in b {
            d -= m(bj, kf).max(1e-300).log2();
        }
        l += d;
        big = big.max(l);
        if kf > 2.0 * pmax + 2.0 && d < 0.0 && l < big - 64.0 {
            break;
        }
    }
    big
}

/// The working-precision guard of a series with these parameters.
pub(crate) fn guard_for(a: &[CBall], b: &[CBall], z: &CBall) -> u64 {
    let aa: Vec<(f64, f64)> = a.iter().map(approx).collect();
    let bb: Vec<(f64, f64)> = b.iter().map(approx).collect();
    let g = max_term_log2(&aa, &bb, up(z));
    24 + if g.is_finite() { g.clamp(0.0, 20000.0) as u64 } else { 0 }
}

/// pFq(a; b; z) to p bits by the series: p <= q any z, p = q + 1 for |z| < 1, terminating sums for any p, q.
pub fn hyp_pfq(a: &[CBall], b: &[CBall], z: &CBall, p: u64) -> Option<CBall> {
    let g = guard_for(a, b, z);
    refine(p, g, |wp| pfq_sum(a, b, z, wp))
}

// ---------------------------------------------------------------- Bessel

/// The integer value of an exact real ball (zero imaginary part).
pub(crate) fn int_of(z: &CBall) -> Option<i64> {
    if !z.is_real() || !z.re.rad.is_zero() {
        return None;
    }
    super::float_int(&z.re.mid)?.to_i64()
}

/// 1/Gamma(x) on a complex ball (an exact pole gives 0).
pub(crate) fn rgamma(x: &CBall, wp: u64) -> Option<CBall> {
    if nonpos_int(x).is_some() {
        return Some(int(0));
    }
    let g = if x.is_real() { CBall::real(super::gamma::gamma_ball(&x.re, wp)?) } else { super::complex::gamma(x, wp)? };
    int(1).div(&g, wp)
}

/// (z/2)^nu / Gamma(nu + 1) 0F1(; nu + 1; s z^2/4) at working precision wp (s = -1 for J, +1 for I), nu not a
/// negative integer.
fn bessel_series(nu: &CBall, z: &CBall, s: i64, wp: u64) -> Option<CBall> {
    let half_z = z.mul_real(&Ball::from_ratio(1, 2, wp), wp);
    let pw = match int_of(nu) {
        Some(n) => cball::pow_int(&half_z, &Int::from_i64(n), wp)?,
        None => cball::pow(&half_z, nu, wp)?,
    };
    let nu1 = nu.add(&int(1), wp);
    let w = half_z.square(wp);
    let w = if s < 0 { w.neg() } else { w };
    let f = pfq_sum(&[], std::slice::from_ref(&nu1), &w, wp)?;
    Some(pw.mul(&rgamma(&nu1, wp)?, wp).mul(&f, wp))
}

/// J_nu (s = -1) or I_nu (s = 1) at wp, any order.
fn ji_point(nu: &CBall, z: &CBall, s: i64, wp: u64) -> Option<CBall> {
    if let Some(n) = int_of(nu) {
        if n < 0 {
            let v = bessel_series(&int(-n), z, s, wp)?;
            // J_-n = (-1)^n J_n, I_-n = I_n
            return Some(if s < 0 && n % 2 != 0 { v.neg() } else { v });
        }
    }
    bessel_series(nu, z, s, wp)
}

fn ji_guard(nu: &CBall, z: &CBall) -> u64 {
    let w = up(z) * up(z) / 4.0;
    // a negative integer order is evaluated as its positive twin
    let nu = match int_of(nu) {
        Some(n) if n < 0 => int(-n),
        _ => nu.clone(),
    };
    let nu1 = nu.add(&int(1), 64);
    guard_for(&[], std::slice::from_ref(&nu1), &CBall::real(Ball::exact(super::float_of(w))))
}

/// An order of more than 2^40 in size: the power series of the Bessel functions would need that many terms to
/// leave the underflow (J_nu(z) is about (z/2)^nu/nu!, below 10^(-10^12)), so no enclosure is attempted (the ball
/// search used to spend nine seconds on `bessel_j(2^53 + 1, 0.5)` and then give up).
fn order_too_large(nu: &CBall) -> bool {
    log2_up(nu) > 40.0
}

/// J_nu(z) to p bits for complex (or negative real) z.
pub fn bessel_j(nu: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if up(z) > 1e5 || order_too_large(nu) {
        return None;
    }
    let g = ji_guard(nu, z);
    refine(p, g, |wp| ji_point(nu, z, -1, wp))
}

/// I_nu(z) to p bits for complex (or negative real) z.
pub fn bessel_i(nu: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if up(z) > 1e5 || order_too_large(nu) {
        return None;
    }
    let g = ji_guard(nu, z);
    refine(p, g, |wp| ji_point(nu, z, 1, wp))
}

/// sin(nu pi) and cos(nu pi) for an order whose ball excludes the integers (`None` otherwise), with the bits lost
/// near an integer.
fn sin_cos_pi_order(nu: &CBall, wp: u64) -> Option<(CBall, CBall, u64)> {
    if int_of(nu).is_some() {
        return None;
    }
    let pi = Const::Pi.ball(wp)?;
    let (s, c) = cball::sin_cos(&nu.mul_real(&pi, wp), wp)?;
    if s.contains_zero() {
        return None;
    }
    let near = (-log2_up(&s)).clamp(0.0, 1e6) as u64;
    Some((s, c, near))
}

/// Y_nu(z) = (J_nu cos nu pi - J_-nu) / sin nu pi to p bits, nu not an integer.
pub fn bessel_y(nu: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if order_too_large(nu) {
        return None;
    }
    let (_, _, near) = sin_cos_pi_order(nu, 64)?;
    let g = ji_guard(nu, z).max(ji_guard(&nu.neg(), z)) + near;
    refine(p, g, |wp| {
        let (s, c, _) = sin_cos_pi_order(nu, wp)?;
        let jp = ji_point(nu, z, -1, wp)?;
        let jm = ji_point(&nu.neg(), z, -1, wp)?;
        jp.mul(&c, wp).sub(&jm, wp).div(&s, wp)
    })
}

/// K_nu(z) = pi/2 (I_-nu - I_nu) / sin nu pi to p bits, nu not an integer.
pub fn bessel_k(nu: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if order_too_large(nu) {
        return None;
    }
    let (_, _, near) = sin_cos_pi_order(nu, 64)?;
    let g = ji_guard(nu, z).max(ji_guard(&nu.neg(), z)) + near + 2 * up(z) as u64;
    refine(p, g, |wp| {
        let (s, _, _) = sin_cos_pi_order(nu, wp)?;
        let ip = ji_point(nu, z, 1, wp)?;
        let im = ji_point(&nu.neg(), z, 1, wp)?;
        let hp = Const::Pi.ball(wp)?.mul(&Ball::from_ratio(1, 2, wp), wp);
        im.sub(&ip, wp).div(&s, wp).map(|v| v.mul_real(&hp, wp))
    })
}

// ---------------------------------------------------------------- Airy

/// Which Airy function: Ai, Bi, Ai', Bi'.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Airy {
    /// Ai
    Ai,
    /// Bi
    Bi,
    /// Ai'
    AiP,
    /// Bi'
    BiP,
}

fn airy_point(kind: Airy, z: &CBall, wp: u64) -> Option<CBall> {
    let w = z.square(wp).mul(z, wp).mul_real(&Ball::from_ratio(1, 9, wp), wp);
    let b = |n: i64| CBall::real(Ball::from_ratio(n, 3, wp));
    let (c1, c2) = super::bessel_j::airy_constants(wp)?;
    let (f, g) = match kind {
        Airy::Ai | Airy::Bi => {
            let f = pfq_sum(&[], &[b(2)], &w, wp)?;
            let g = z.mul(&pfq_sum(&[], &[b(4)], &w, wp)?, wp);
            (f, g)
        }
        Airy::AiP | Airy::BiP => {
            let f = z.square(wp).mul_real(&Ball::from_ratio(1, 2, wp), wp).mul(&pfq_sum(&[], &[b(5)], &w, wp)?, wp);
            let g = pfq_sum(&[], &[b(1)], &w, wp)?;
            (f, g)
        }
    };
    let (cf, cg) = (f.mul_real(&c1, wp), g.mul_real(&c2, wp));
    match kind {
        Airy::Ai | Airy::AiP => Some(cf.sub(&cg, wp)),
        Airy::Bi | Airy::BiP => Some(cf.add(&cg, wp).mul_real(&el::sqrt(&Ball::from_i64(3), wp)?, wp)),
    }
}

/// An Airy function at a complex ball to p bits (|z| <= 200).
pub fn airy(kind: Airy, z: &CBall, p: u64) -> Option<CBall> {
    let a = up(z);
    if a > MAX_AIRY {
        return None;
    }
    let zeta = 2.0 / 3.0 * a * a.sqrt();
    let g = 24 + (2.9 * zeta).ceil() as u64;
    refine(p, g, |wp| airy_point(kind, z, wp))
}

// ---------------------------------------------------------------- digamma and polygamma

/// log2 |B_2m| (B_2m ~ 2 (2m)! / (2 pi)^2m).
fn log2_b2(m: u64) -> f64 {
    let mut lf = 0.0;
    for k in 1..=2 * m {
        lf += (k as f64).log2();
    }
    1.0 + lf - (2 * m) as f64 * (2.0 * std::f64::consts::PI).log2()
}

/// log2 (a! ) for small a.
fn log2_fact(a: u64) -> f64 {
    (2..=a).map(|k| (k as f64).log2()).sum()
}

/// A lower bound of |z| as a double.
fn abs_lower(z: &CBall) -> f64 {
    let (x, y) = approx(z);
    let r = up_real(&Ball { mid: Float::zero(), rad: z.re.rad }) + up_real(&Ball { mid: Float::zero(), rad: z.im.rad });
    ((x * x + y * y).sqrt() * (1.0 - 1e-12) - r).max(0.0)
}

/// The ball 0 +- 2^e on both parts.
fn err_pow2(z: CBall, e: f64) -> CBall {
    let k = if e.is_finite() { e.ceil() as i64 + 1 } else { 100_000 };
    widen(z, &Mag::pow2(k))
}

/// psi^(n)(z) (n = 0: digamma) at working precision wp, for a ball avoiding the poles.
fn polygamma_point(n: u64, z: &CBall, wp: u64) -> Option<CBall> {
    let target = 10.0 + 0.5 * wp as f64 + n as f64;
    let re = f64_of(&z.re);
    if !re.is_finite() {
        return None;
    }
    let shift = (target - re).ceil().max(0.0) as u64;
    if shift > 200_000 {
        return None;
    }
    let w = wp + 16;
    // the shifted sum: sum_{j<N} (z + j)^-(n+1)
    let mut corr = int(0);
    for j in 0..shift {
        if j % 64 == 0 && stopped() {
            return None;
        }
        let t = z.add(&int(j as i64), w);
        if t.contains_zero() {
            return None;
        }
        let r = t.recip(w)?;
        let r = if n == 0 { r } else { cball::pow_int(&r, &Int::from_u64(n + 1), w)? };
        corr = corr.add(&r, w);
    }
    let x = z.add(&int(shift as i64), w);
    let ax = abs_lower(&x);
    if ax <= 1.0 + n as f64 {
        return None;
    }
    let lax = ax.log2();
    // the remainder after M - 1 Bernoulli terms: |B_2M| (2M + n - 1)! / ((2M)! |x|^(2M + n)) 2^(M + n + 1)
    let bound = |m: u64| {
        log2_b2(m) + log2_fact(2 * m + n - 1) - log2_fact(2 * m) - (2 * m + n) as f64 * lax + (m + n + 1) as f64
    };
    let mut m = 1u64;
    while bound(m) > -((wp + 8) as f64) {
        m += 1;
        if m > 4000 {
            return None;
        }
    }
    let inv = x.recip(w)?;
    let inv2 = inv.square(w);
    let rat = |q: &Rat| super::rat_ball(q, w);
    let mut s;
    let mut pw;
    if n == 0 {
        // log x - 1/(2x) - sum B_2k / (2k x^2k)
        s = cball::log(&x, w)?.sub(&inv.mul_real(&Ball::from_ratio(1, 2, w), w), w);
        pw = inv2.clone();
        for k in 1..m {
            let c = b2n(k)?.mul(&Rat::new(Int::from_i64(1), Int::from_i64(2 * k as i64)));
            s = s.sub(&pw.mul_real(&rat(&c), w), w);
            pw = pw.mul(&inv2, w);
        }
    } else {
        // (-1)^(n+1) [(n-1)!/x^n + n!/(2 x^(n+1)) + sum B_2k (2k+n-1)!/((2k)! x^(2k+n))]
        let fact = |a: u64| Ball::from_nat(super::gamma::fact_nat(a));
        let rn = cball::pow_int(&inv, &Int::from_u64(n), w)?;
        s = rn.mul_real(&fact(n - 1), w).add(&rn.mul(&inv, w).mul_real(&fact(n).mul(&Ball::from_ratio(1, 2, w), w), w), w);
        pw = rn.mul(&inv2, w);
        // f_k = (2k + n - 1)! / (2k)!
        let mut f = Rat::from_int(Int::from_nat(super::gamma::fact_nat(n + 1))).mul(&Rat::new(Int::from_i64(1), Int::from_i64(2)));
        for k in 1..m {
            s = s.add(&pw.mul_real(&rat(&b2n(k)?.mul(&f)), w), w);
            pw = pw.mul(&inv2, w);
            let num = Int::from_u64((2 * k + n) * (2 * k + n + 1));
            let den = Int::from_u64((2 * k + 1) * (2 * k + 2));
            f = f.mul(&Rat::new(num, den));
        }
        if n.is_multiple_of(2) {
            s = s.neg();
        }
    }
    let s = err_pow2(s, bound(m));
    // psi^(n)(z) = psi^(n)(x) - (-1)^n n! corr
    let corr = if n == 0 { corr } else { corr.mul_real(&Ball::from_nat(super::gamma::fact_nat(n)), w) };
    Some(if n.is_multiple_of(2) { s.sub(&corr, w) } else { s.add(&corr, w) })
}

/// digamma(z) on a complex ball to p bits (`None` at or touching a pole).
pub fn digamma(z: &CBall, p: u64) -> Option<CBall> {
    refine(p, 16, |wp| polygamma_point(0, z, wp))
}

/// polygamma(n, z) on a complex ball to p bits, n a non-negative integer.
pub fn polygamma(n: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    let n = int_of(n)?;
    if !(0..=10_000).contains(&n) {
        return None;
    }
    refine(p, 16, |wp| polygamma_point(n as u64, z, wp))
}
