//! Complex-ball enclosures for log gamma, gamma, the error functions and the
//! Riemann zeta function, so that `n(gamma(2 + im), 20)` and `n(zeta(1/2 + 14 im), 30)` reach their precision.
//!
//! - log gamma: Stirling's series at w = z + N with Re w >= p/2 + 10, truncated after M terms with the remainder
//!   bound |B_2M| / (2M (2M - 1) |w|^(2M-1)) sec(arg(w)/2)^(2M) <= that times 2^M (DLMF 5.11.ii, Re w > 0), minus
//!   the principal logarithms of z, z + 1, ..., z + N - 1 (the analytic log gamma); gamma as its exponential.
//! - erf: the Maclaurin series 2/sqrt(pi) sum (-1)^k z^(2k+1)/(k! (2k+1)) at a working precision raised by the
//!   cancellation 2 |z|^2 log2(e), with the tail after K terms bounded by twice the first omitted term once
//!   |z|^2/(K + 1) <= 1/2; erfc = 1 - erf, erfi(z) = -i erf(i z).
//! - zeta: Euler-Maclaurin with N terms and M Bernoulli corrections, the remainder bounded by Johansson's
//!   4 |(s)_2M| / (2 pi)^2M N^(1 - sigma - 2M) / (sigma + 2M - 1); the functional equation for Re s < 1/2.
//!
//! The error bounds are computed in machine arithmetic from logarithms with a safety margin of 8 bits.
use crate::bernoulli::b2n;
use mc_arith::num::{Ball, Float, Mag, Rat};
use mc_arith::{Kind, Real};

use mc_numeric::cball::{self, CBall};
use mc_numeric::consts::Const;
use mc_numeric::elementary as el;

fn rat_ball(q: &Rat, p: u64) -> Ball {
    match Real::Exact(q.clone()).to_kind(Kind::Ball, p) {
        Real::Ball(b, _) => b,
        _ => Ball::from_i64(0),
    }
}

/// The ball 0 +- e (e >= 0, a machine upper bound): the radius is the power of two above e.
fn err(e: f64) -> Ball {
    let k = if e > 0.0 && e.is_finite() { e.log2().ceil() as i64 + 1 } else if e == 0.0 { -100_000 } else { 100_000 };
    Ball { mid: Float::zero(), rad: Mag::pow2(k) }
}

fn widen(z: CBall, e: f64, p: u64) -> CBall {
    let b = err(e);
    CBall::new(z.re.add(&b, p), z.im.add(&b, p))
}

fn f64_of(b: &Ball) -> f64 {
    Real::Ball(b.clone(), 64).to_f64()
}

/// A lower bound of |z|.
fn abs_lower(z: &CBall, p: u64) -> f64 {
    match cball::abs(z, p) {
        Some(a) => (f64_of(&a) - (a.rad.m as f64) * 2f64.powi(a.rad.e as i32)).max(0.0),
        None => 0.0,
    }
}

/// log2 |B_2m| (B_2m ~ 2 (2m)! / (2 pi)^2m).
fn log2_b2(m: u64) -> f64 {
    let mut lf = 0.0;
    for k in 1..=2 * m {
        lf += (k as f64).log2();
    }
    1.0 + lf - (2 * m) as f64 * (2.0 * std::f64::consts::PI).log2()
}

/// The sum of the principal logarithms of z, z + 1, ..., z + n - 1. For an argument off the real axis it is the
/// logarithm of the product (one logarithm instead of n, which at thousands of bits is the whole cost) plus the
/// multiple of 2 pi i that the principal arguments of the factors add up to, found in machine arithmetic (the
/// arguments are far from the cut, so their sum is exact to 1e-9). A real argument, or one so close to the real
/// axis that a factor may sit on the cut, takes the sum of the logarithms. The stop flag is polled.
fn log_shift(z: &CBall, n: u64, w: u64) -> Option<CBall> {
    let (re, im) = (f64_of(&z.re), f64_of(&z.im));
    let off_axis = n > 8 && im.is_finite() && im.abs() > 1e-6 && (z.im.rad.m as f64) * 2f64.powi(z.im.rad.e as i32) < 1e-9;
    if off_axis {
        let w2 = w + 24 + 64 - (n.leading_zeros() as u64);
        let mut prod = CBall::from_i64(1);
        let mut args = 0.0f64;
        for j in 0..n {
            if j % 64 == 0 && mc_arith::num::stopped() {
                return None;
            }
            let t = z.add(&CBall::from_i64(j as i64), w2);
            args += im.atan2(re + j as f64);
            prod = prod.mul(&t, w2);
        }
        let l = cball::log(&prod, w2)?;
        let li = f64_of(&l.im);
        let k = ((args - li) / (2.0 * std::f64::consts::PI)).round();
        if ((args - li) / (2.0 * std::f64::consts::PI) - k).abs() > 1e-3 {
            return None;
        }
        let two_pi_k = Const::Pi.ball(w2)?.mul(&Ball::from_i64(2 * k as i64), w2);
        return Some(CBall::new(l.re, l.im.add(&two_pi_k, w2)).round(w));
    }
    let mut shift = CBall::from_i64(0);
    for j in 0..n {
        if j % 64 == 0 && mc_arith::num::stopped() {
            return None;
        }
        let t = z.add(&CBall::from_i64(j as i64), w);
        if t.contains_zero() {
            return None;
        }
        shift = shift.add(&cball::log(&t, w)?, w);
    }
    Some(shift)
}

/// The shift N of Stirling's series at precision p (Re (z + N) >= p/2 + 10), or `None` past 100,000.
fn stirling_shift(z: &CBall, p: u64) -> Option<u64> {
    let target = 10.0 + 0.5 * p as f64;
    let re = f64_of(&z.re);
    if !re.is_finite() {
        return None;
    }
    let n = (target - re).ceil().max(0.0) as u64;
    (n <= 100_000).then_some(n)
}

/// Stirling's series for log gamma at x (Re x large, |x| > 1) with its remainder, at working precision w for a
/// result to p bits.
fn stirling(x: &CBall, p: u64, w: u64) -> Option<CBall> {
    let ax = abs_lower(x, w);
    if ax <= 1.0 {
        return None;
    }
    // the number of Stirling terms: the first M with bound <= 2^-(p + 8)
    let mut m = 1u64;
    loop {
        let lb = log2_b2(m) - ((2 * m) as f64 * (2 * m - 1) as f64).log2() - (2 * m - 1) as f64 * ax.log2() + m as f64;
        if lb <= -((p + 8) as f64) {
            break;
        }
        m += 1;
        if m > 3000 {
            return None;
        }
    }
    let log2pi_half = {
        let two_pi = Const::Pi.ball(w)?.mul(&Ball::from_i64(2), w);
        let l = el::log(&two_pi, w)?;
        Ball { mid: Float::new(l.mid.neg, l.mid.man.clone(), l.mid.exp - 1), rad: l.rad.mul_2exp(-1) }
    };
    let lx = cball::log(x, w)?;
    let half = rat_ball(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64(2)), w);
    let mut s = x.sub(&CBall::real(half), w).mul(&lx, w).sub(x, w).add(&CBall::real(log2pi_half), w);
    let inv = CBall::from_i64(1).div(x, w)?;
    let inv2 = inv.square(w);
    let mut pw = inv.clone();
    for k in 1..m {
        let b = b2n(k)?;
        let c = b.mul(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64((2 * k * (2 * k - 1)) as i64)));
        s = s.add(&pw.mul_real(&rat_ball(&c, w), w), w);
        pw = pw.mul(&inv2, w);
    }
    let lb = log2_b2(m) - ((2 * m) as f64 * (2 * m - 1) as f64).log2() - (2 * m - 1) as f64 * ax.log2() + m as f64;
    Some(widen(s, 2f64.powf(lb + 1.0), w))
}

/// The analytic log gamma on a complex ball.
pub fn log_gamma(z: &CBall, p: u64) -> Option<CBall> {
    let w = p + 20;
    let n = stirling_shift(z, p)?;
    let shift = log_shift(z, n, w)?;
    let x = z.add(&CBall::from_i64(n as i64), w);
    let s = stirling(&x, p, w)?;
    Some(s.sub(&shift, w).round(p))
}

/// Gamma on a complex ball. At a point (both radii zero): exp of Stirling's series at z + N divided by z, z + 1,
/// ..., z + N - 1 in turn, which needs no logarithms (gamma has no branch) and is several times faster (the exact
/// points of the boundary walks of `count_roots`). A ball of any radius: exp of the analytic log gamma, whose sum of
/// logarithms keeps the radius additive; N successive complex divisions compound a radius (8.9 instead of 0.0036
/// for 2 + 3i +- (1 + i)/100, twice the radius already at 2^-50), so they are used at points only. Where a division
/// does not exclude 0, the logarithm form too.
pub fn gamma(z: &CBall, p: u64) -> Option<CBall> {
    if !(z.re.rad.is_zero() && z.im.rad.is_zero()) {
        return gamma_box(z, p);
    }
    let q = p + 32;
    let n = stirling_shift(z, q)?;
    // the divisions lose about log2(N) bits to rounding
    let w = q + 20 + (64 - n.leading_zeros() as u64);
    let x = z.add(&CBall::from_i64(n as i64), w);
    let mut v = cball::exp(&stirling(&x, q, q + 20)?, w)?;
    for j in 0..n {
        if j % 64 == 0 && mc_arith::num::stopped() {
            return None;
        }
        let t = z.add(&CBall::from_i64(j as i64), w);
        if t.contains_zero() {
            return None;
        }
        match v.div(&t, w) {
            Some(u) => v = u,
            None => return gamma_by_log(z, p),
        }
    }
    Some(v.round(p))
}

/// An upper bound of |x| for a machine number as a radius.
fn mag_up(x: f64) -> Option<Mag> {
    let (neg, man, exp) = mc_arith::num::float::f64_to_dyadic(x.abs())?;
    Some(Mag::from_float_up(&Float::new(neg, man, exp)))
}

/// Gamma on a ball z = c + d (|d| <= r) of positive radius. A narrow ball (r sum 1/|c + j| <= 1/64, first order):
/// the logarithm form, as before. A wide ball (the covers of `count_roots`): also exp(S(z + N) - log P(z)) with the
/// product P(z) = z (z + 1) ... (z + N - 1) taken at the centre c (a point, one logarithm) and P(z) = P(c) (1 + u),
/// |u| <= prod (1 + r/|c + j|) - 1 <= exp(r sum 1/|c + j|) - 1 = g (each factor moves by at most r), so that
/// log P(z) is within -log(1 - g) of log P(c); the narrower of the two forms is the answer (each is an enclosure,
/// neither is always the narrower). The machine sums carry a relative margin of 1e-12 against their rounding. A box
/// that may reach a pole (g >= 1/2) takes the logarithm form.
fn gamma_box(z: &CBall, p: u64) -> Option<CBall> {
    let q = p + 32;
    let n = stirling_shift(z, q)?;
    let w = q + 20 + (64 - n.leading_zeros() as u64);
    let c = CBall::new(Ball::exact(z.re.mid.clone()), Ball::exact(z.im.mid.clone()));
    let radius = |m: &Mag| (m.m as f64) * 2f64.powi(m.e as i32);
    let r = radius(&z.re.rad).hypot(radius(&z.im.rad)) * (1.0 + 1e-12);
    let (cr, ci) = (f64_of(&c.re), f64_of(&c.im));
    if !(r.is_finite() && cr.is_finite() && ci.is_finite()) {
        return gamma_by_log(z, p);
    }
    let mut sum = 0.0f64;
    for j in 0..n {
        let d = (cr + j as f64).hypot(ci) * (1.0 - 1e-12);
        if d.is_nan() || d <= r {
            return gamma_by_log(z, p);
        }
        sum += 1.0 / d;
    }
    // a narrow ball (first order, r sum 1/|c + j| <= 1/64): the logarithm form, which is the tighter there
    if r * sum <= 1.0 / 64.0 {
        return gamma_by_log(z, p);
    }
    let mut prod = CBall::from_i64(1);
    for j in 0..n {
        if j % 64 == 0 && mc_arith::num::stopped() {
            return None;
        }
        prod = prod.mul(&c.add(&CBall::from_i64(j as i64), w), w);
    }
    // P(z) = P(c) (1 + u) with |u| <= g < 1, and |log(1 + u)| <= -log(1 - g): in the exponent, Gamma(z) =
    // exp(S(z + N) - log P(c) - log(1 + u)) (any branch of the logarithm of the point P(c): exp removes it), the
    // radius added there to both parts
    let g = (r * sum * (1.0 + 1e-12)).exp_m1() * (1.0 + 1e-12);
    if g.is_nan() || g >= 0.5 {
        return gamma_by_log(z, p);
    }
    let Some(lp) = cball::log(&prod, w) else { return gamma_by_log(z, p) };
    let x = z.add(&CBall::from_i64(n as i64), w);
    let s = stirling(&x, q, q + 20)?.sub(&lp, w);
    let spread = Ball { mid: Float::zero(), rad: mag_up(-(1.0 - g).ln() * (1.0 + 1e-12))? };
    let s = CBall::new(s.re.add(&spread, w), s.im.add(&spread, w));
    let v = cball::exp(&s, p + 16)?.round(p);
    // neither form is always the narrower on a wide ball: the narrower of the two
    match gamma_by_log(z, p) {
        Some(l) if radius(&l.re.rad).max(radius(&l.im.rad)) < radius(&v.re.rad).max(radius(&v.im.rad)) => Some(l),
        _ => Some(v),
    }
}

/// Gamma as the exponential of the analytic log gamma.
fn gamma_by_log(z: &CBall, p: u64) -> Option<CBall> {
    let lg = log_gamma(z, p + 32)?;
    Some(cball::exp(&lg, p + 16)?.round(p))
}

/// erf on a complex ball (|z| <= 30).
pub fn erf(z: &CBall, p: u64) -> Option<CBall> {
    let az = f64_of(&cball::abs(z, 64)?) + 1e-6;
    if az > 30.0 {
        return None;
    }
    let w = p + 20 + (2.0 * az * az * std::f64::consts::LOG2_E) as u64;
    let z2 = z.square(w).neg();
    let mut term = z.clone(); // z^(2k+1) (-1)^k / k!
    let mut sum = z.clone();
    let mut k = 0u64;
    loop {
        k += 1;
        term = term.mul(&z2, w).mul_real(&rat_ball(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_u64(k)), w), w);
        let t = term.mul_real(&rat_ball(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_u64(2 * k + 1)), w), w);
        sum = sum.add(&t, w);
        let at = f64_of(&cball::abs(&t, 64)?);
        if az * az / (k as f64 + 1.0) <= 0.5 && at < 2f64.powi(-((w + 8) as i32)) * f64_of(&cball::abs(&sum, 64)?).max(1e-300) {
            sum = widen(sum, 2.0 * at, w);
            break;
        }
        if k > 100_000 {
            return None;
        }
    }
    // 2/sqrt(pi)
    let sp = Const::Pi.ball(w)?.sqrt(w)?;
    let c = Ball::from_i64(2).div(&sp, w)?;
    Some(sum.mul_real(&c, w).round(p))
}

/// erfc = 1 - erf.
pub fn erfc(z: &CBall, p: u64) -> Option<CBall> {
    Some(CBall::from_i64(1).sub(&erf(z, p + 16)?, p + 16).round(p))
}

/// erfi(z) = -i erf(i z).
pub fn erfi(z: &CBall, p: u64) -> Option<CBall> {
    Some(erf(&z.mul_i(), p)?.div_i())
}

/// n^-s for a positive integer n.
fn int_pow_neg(n: u64, s: &CBall, w: u64) -> Option<CBall> {
    let l = el::log(&Ball::from_i64(n as i64), w)?;
    cball::exp(&s.mul_real(&l, w).neg(), w)
}

/// The Riemann zeta function on a complex ball.
pub fn zeta(s: &CBall, p: u64) -> Option<CBall> {
    let sigma = f64_of(&s.re);
    let tau = f64_of(&s.im).abs();
    if !sigma.is_finite() {
        return None;
    }
    let one = CBall::from_i64(1);
    if s.sub(&one, 64).contains_zero() {
        return None;
    }
    if sigma < 0.5 {
        // zeta(s) = 2^s pi^(s-1) sin(pi s/2) gamma(1 - s) zeta(1 - s)
        let w = p + 32;
        let one_s = one.sub(s, w);
        let pi = Const::Pi.ball(w)?;
        let two_s = cball::exp(&s.mul_real(&el::log(&Ball::from_i64(2), w)?, w), w)?;
        let pi_s1 = cball::exp(&s.sub(&one, w).mul_real(&el::log(&pi, w)?, w), w)?;
        let half = Ball { mid: Float::new(pi.mid.neg, pi.mid.man.clone(), pi.mid.exp - 1), rad: pi.rad.mul_2exp(-1) };
        let sn = cball::sin(&s.mul_real(&half, w), w)?;
        let v = two_s.mul(&pi_s1, w).mul(&sn, w).mul(&gamma(&one_s, w)?, w).mul(&zeta(&one_s, w)?, w);
        return Some(v.round(p));
    }
    // the phase tau log k of k^-s needs log2(tau log k) extra bits, not a multiple of tau (tau = 1e5 worked at
    // 150000 bits); the ball arithmetic carries whatever is lost
    let w = p + 40 + 2 * (tau + 2.0).log2().ceil() as u64;
    let n = (20.0 + p as f64 / 4.0 + tau).ceil() as u64;
    if n > 200_000 {
        return None;
    }
    // M: the first with 4 |(s)_2M| / (2 pi)^2M N^(1 - sigma - 2M) / (sigma + 2M - 1) <= 2^-(p + 8)
    let abs_s = sigma.hypot(tau);
    let mut m = 1u64;
    let bound = |m: u64| {
        let mut lp = 0.0;
        for j in 0..2 * m {
            lp += (abs_s + j as f64).log2();
        }
        2.0 + lp - (2 * m) as f64 * (2.0 * std::f64::consts::PI).log2()
            + (1.0 - sigma - (2 * m) as f64) * (n as f64).log2()
            - (sigma + (2 * m) as f64 - 1.0).log2()
    };
    while bound(m) > -((p + 8) as f64) {
        m += 1;
        if m > 2000 {
            return None;
        }
    }
    // k^-s is multiplicative: one exponential per prime, one product per composite (smallest prime factor sieve)
    let mut spf = vec![0u32; n as usize];
    for i in 2..n as usize {
        if spf[i] == 0 {
            let mut j = i;
            while j < n as usize {
                if spf[j] == 0 {
                    spf[j] = i as u32;
                }
                j += i;
            }
        }
    }
    let mut pows: Vec<CBall> = Vec::with_capacity(n as usize);
    pows.push(CBall::from_i64(0));
    pows.push(CBall::from_i64(1));
    let mut sum = CBall::from_i64(1);
    for k in 2..n as usize {
        if k % 64 == 0 && mc_arith::num::stopped() {
            return None;
        }
        let q = spf[k] as usize;
        let v = if q == k { int_pow_neg(k as u64, s, w)? } else { pows[q].mul(&pows[k / q], w) };
        sum = sum.add(&v, w);
        pows.push(v);
    }
    let nb = CBall::from_i64(n as i64);
    let n_s = int_pow_neg(n, s, w)?;
    // N^(1-s)/(s-1) + N^-s/2
    sum = sum.add(&n_s.mul(&nb, w).div(&s.sub(&one, w), w)?, w);
    sum = sum.add(&n_s.mul_real(&rat_ball(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64(2)), w), w), w);
    // sum_j B_2j/(2j)! (s)_(2j-1) N^(-s-2j+1)
    let mut poch = s.clone();
    let mut fact = Rat::from_i64(2);
    let inv_n = rat_ball(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_u64(n)), w);
    let inv_n2 = inv_n.mul(&inv_n, w);
    let mut pw = n_s.mul_real(&inv_n, w);
    for j in 1..=m {
        let c = b2n(j)?.mul(&fact.recip()?);
        sum = sum.add(&poch.mul(&pw, w).mul_real(&rat_ball(&c, w), w), w);
        let j2 = 2 * j as i64;
        poch = poch.mul(&s.add(&CBall::from_i64(j2 - 1), w), w).mul(&s.add(&CBall::from_i64(j2), w), w);
        fact = fact.mul(&Rat::from_i64((j2 + 1) * (j2 + 2)));
        pw = pw.mul_real(&inv_n2, w);
    }
    Some(widen(sum, 2f64.powf(bound(m) + 1.0), w).round(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mag(m: &Mag) -> f64 {
        (m.m as f64) * 2f64.powi(m.e as i32)
    }

    /// The radius of gamma on a grid of centres and radii is never wider than that of the logarithm form by more
    /// than a few percent (and a few units of the last place for points).
    #[test]
    fn gamma_is_as_tight_as_the_logarithm_form() {
        let p = 64;
        let mut worst = 0.0f64;
        for re in [-7, -5, -3, -1, 1, 2, 3, 5, 9] {
            for im in [-6, -3, -1, 1, 2, 3, 7] {
                for k in [0i64, -50, -40, -30, -20, -12, -7, -4] {
                    let rad = if k == 0 { Mag::ZERO } else { Mag::pow2(k) };
                    let part = |n: i64| Ball { mid: Ball::from_ratio(n, 2, p).mid, rad };
                    let z = CBall::new(part(re), part(im));
                    let (Some(a), Some(b)) = (gamma(&z, p), gamma_by_log(&z, p)) else { continue };
                    let (ra, rb) = (mag(&a.re.rad).max(mag(&a.im.rad)), mag(&b.re.rad).max(mag(&b.im.rad)));
                    let size = f64_of(&b.re).abs().max(f64_of(&b.im).abs());
                    let slack = size * 2f64.powi(-(p as i32) + 8);
                    assert!(ra <= 1.05 * rb + slack, "z = {re}/2 + {im}/2 i +- 2^{k}: radius {ra:e} against {rb:e}");
                    if rb > slack {
                        worst = worst.max(ra / rb);
                    }
                }
            }
        }
        assert!(worst <= 1.05, "{worst}");
    }
}
