//! Complex-ball enclosures (package p6-numeric, decision P6N.13) for log gamma, gamma, the error functions and the
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

/// The analytic log gamma on a complex ball.
pub fn log_gamma(z: &CBall, p: u64) -> Option<CBall> {
    let w = p + 20;
    let target = 10.0 + 0.5 * p as f64;
    let re = f64_of(&z.re);
    if !re.is_finite() {
        return None;
    }
    let n = (target - re).ceil().max(0.0) as u64;
    if n > 100_000 {
        return None;
    }
    let mut shift = CBall::from_i64(0);
    for j in 0..n {
        let t = z.add(&CBall::from_i64(j as i64), w);
        if t.contains_zero() {
            return None;
        }
        shift = shift.add(&cball::log(&t, w)?, w);
    }
    let x = z.add(&CBall::from_i64(n as i64), w);
    let ax = abs_lower(&x, w);
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
    let lx = cball::log(&x, w)?;
    let half = rat_ball(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64(2)), w);
    let mut s = x.sub(&CBall::real(half), w).mul(&lx, w).sub(&x, w).add(&CBall::real(log2pi_half), w);
    let inv = CBall::from_i64(1).div(&x, w)?;
    let inv2 = inv.square(w);
    let mut pw = inv.clone();
    for k in 1..m {
        let b = b2n(k)?;
        let c = b.mul(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64((2 * k * (2 * k - 1)) as i64)));
        s = s.add(&pw.mul_real(&rat_ball(&c, w), w), w);
        pw = pw.mul(&inv2, w);
    }
    let lb = log2_b2(m) - ((2 * m) as f64 * (2 * m - 1) as f64).log2() - (2 * m - 1) as f64 * ax.log2() + m as f64;
    let s = widen(s, 2f64.powf(lb + 1.0), w);
    Some(s.sub(&shift, w).round(p))
}

/// Gamma on a complex ball.
pub fn gamma(z: &CBall, p: u64) -> Option<CBall> {
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
    let w = p + 32 + (tau * 1.5) as u64;
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
    let mut sum = CBall::from_i64(0);
    for k in 1..n {
        sum = sum.add(&int_pow_neg(k, s, w)?, w);
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
