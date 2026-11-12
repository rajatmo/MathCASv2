//! The gamma function family on balls: log gamma, gamma, digamma, polygamma and beta.
//!
//! - **log gamma, x > 0:** the argument is shifted to y = x + N >= Y(p) = p/2 + 10 and Stirling's series
//!   ln Gamma(y) = (y - 1/2) ln y - y + ln(2 pi)/2 + sum_{k=1}^{K} B_2k / (2k (2k - 1) y^(2k-1)) + R_K is summed
//!   until a term falls below 2^-p of the leading part. For real y > 0 the series is enveloping: |R_K| is at most
//!   the first omitted term, which is evaluated on the ball (so it bounds every point) and added to the radius.
//!   Then ln Gamma(x) = ln Gamma(y) - ln(x (x + 1) ... (x + N - 1)).
//! - **gamma:** Gamma(x) = exp(ln Gamma(y)) / (x ... (x + N - 1)) for x > 0; for other x (no pole in the ball) the
//!   reflection Gamma(x) = pi / (sin(pi x) Gamma(1 - x)), with sin(pi x) reduced exactly ([`super::sin_pi`]).
//! - **log gamma, x < 0:** the principal value ln|Gamma(x)| - i pi ceil(-x) (the limit from the upper half plane,
//!   decision SF.6), with ln|Gamma(x)| = ln pi - ln|sin pi x| - ln Gamma(1 - x).
//! - **digamma:** psi(y) = ln y - 1/(2y) - sum B_2k / (2k y^2k) + R (enveloping for y > 0), psi(x) = psi(x + N) -
//!   sum 1/(x + j); x <= 0 by the reflection psi(x) = psi(1 - x) - pi cot(pi x).
//! - **polygamma(n, x), n >= 1:** psi^(n)(y) = (-1)^(n+1) [(n-1)!/y^n + n!/(2 y^(n+1)) + sum B_2k (2k+n-1)! /
//!   ((2k)! y^(2k+n))] + R (enveloping for y > 0), shifted by psi^(n)(x) = psi^(n)(x + N) + (-1)^(n+1) n!
//!   sum (x + j)^-(n+1); for x <= 0 the same upward recurrence (N up to 10^6).
//! - **beta:** exp(ln Gamma(a) + ln Gamma(b) - ln Gamma(a + b)) for a, b > 0, else Gamma(a) Gamma(b) / Gamma(a + b).
use super::{contains_int_at_most, int, is_pos, log2_up, lower, mag_up, pi, rat_ball, real, refine, sin_pi, widen};
use crate::bernoulli::b2n;
use mc_arith::num::{stopped, Ball, Int, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;

/// The shift target: arguments are moved to at least this before an asymptotic series.
fn y_min(wp: u64, n: u64) -> f64 {
    wp as f64 / 2.0 + n as f64 + 10.0
}

/// N with x_lo + N >= the shift target.
fn shift(x: &Ball, wp: u64, n: u64) -> Option<u64> {
    let lo = super::f64_of(&lower(x));
    let need = (y_min(wp, n) - lo).ceil();
    if !need.is_finite() || need > 2e6 {
        return None;
    }
    Some(need.max(0.0) as u64)
}

/// x (x + 1) ... (x + N - 1).
fn rising(x: &Ball, n: u64, wp: u64) -> Option<Ball> {
    let mut p = int(1);
    for j in 0..n {
        if j % 64 == 0 && stopped() {
            return None;
        }
        p = p.mul(&x.add(&int(j as i64), wp), wp);
    }
    Some(p)
}

/// ln Gamma(y) by Stirling's series for a ball y >= the shift target.
fn stirling(y: &Ball, wp: u64) -> Option<Ball> {
    let ly = el::log(y, wp)?;
    let half = Ball::from_ratio(1, 2, wp);
    let lead = y.sub(&half, wp).mul(&ly, wp).sub(y, wp);
    let ln2pi = el::log(&pi(wp)?.mul(&int(2), wp), wp)?;
    let mut s = lead.add(&ln2pi.mul(&half, wp), wp);
    let target = log2_up(&lead).max(0.0) - wp as f64 - 4.0;
    let r = int(1).div(y, wp)?;
    let r2 = r.mul(&r, wp);
    let mut pw = r.clone();
    for k in 1u64.. {
        if stopped() {
            return None;
        }
        let b = b2n(k)?;
        let c = b.mul(&Rat::new(Int::from_i64(1), Int::from_u64(2 * k * (2 * k - 1))));
        let term = rat_ball(&c, wp).mul(&pw, wp);
        if log2_up(&term) < target {
            // the first omitted term bounds the remainder (enveloping series)
            return Some(widen(s, &mag_up(&term)));
        }
        s = s.add(&term, wp);
        pw = pw.mul(&r2, wp);
    }
    None
}

/// ln Gamma(x) for a provably positive ball.
pub fn lgamma_pos(x: &Ball, wp: u64) -> Option<Ball> {
    let n = shift(x, wp, 0)?;
    let y = x.add(&int(n as i64), wp);
    let l = stirling(&y, wp)?;
    if n == 0 {
        return Some(l);
    }
    Some(l.sub(&el::log(&rising(x, n, wp)?, wp)?, wp))
}

/// Gamma(x) for a provably positive ball.
pub fn gamma_pos(x: &Ball, wp: u64) -> Option<Ball> {
    let n = shift(x, wp, 0)?;
    let y = x.add(&int(n as i64), wp);
    let g = el::exp(&stirling(&y, wp)?, wp)?;
    if n == 0 {
        return Some(g);
    }
    g.div(&rising(x, n, wp)?, wp)
}

/// Gamma(x) on a ball without a pole.
pub fn gamma_ball(x: &Ball, wp: u64) -> Option<Ball> {
    if is_pos(x) {
        return gamma_pos(x, wp);
    }
    if contains_int_at_most(x, 0) {
        return None;
    }
    let one_minus = int(1).sub(x, wp);
    let d = sin_pi(x, wp)?.mul(&gamma_pos(&one_minus, wp)?, wp);
    pi(wp)?.div(&d, wp)
}

/// ln Gamma(x): real for x > 0, the principal complex value for negative non-integers.
pub fn lgamma_ball(x: &Ball, wp: u64) -> Option<CBall> {
    if is_pos(x) {
        return Some(real(lgamma_pos(x, wp)?));
    }
    if contains_int_at_most(x, 0) {
        return None;
    }
    let one_minus = int(1).sub(x, wp);
    let s = sin_pi(x, wp)?;
    let abs_s = if s.mid.neg { s.neg() } else { s };
    let re = el::log(&pi(wp)?, wp)?.sub(&el::log(&abs_s, wp)?, wp).sub(&lgamma_pos(&one_minus, wp)?, wp);
    // ceil(-x) is constant on a ball without integers
    let k = super::floor_float(&x.mid.neg())? + 1;
    let im = pi(wp)?.mul(&int(-k), wp);
    Some(CBall::new(re, im))
}

/// psi(y) by its asymptotic series, y >= the shift target.
fn digamma_asym(y: &Ball, wp: u64) -> Option<Ball> {
    let ly = el::log(y, wp)?;
    let r = int(1).div(y, wp)?;
    let mut s = ly.sub(&r.mul(&Ball::from_ratio(1, 2, wp), wp), wp);
    let target = log2_up(&s).max(0.0) - wp as f64 - 4.0;
    let r2 = r.mul(&r, wp);
    let mut pw = r2.clone();
    for k in 1u64.. {
        if stopped() {
            return None;
        }
        let c = b2n(k)?.mul(&Rat::new(Int::from_i64(1), Int::from_u64(2 * k)));
        let term = rat_ball(&c, wp).mul(&pw, wp);
        if log2_up(&term) < target {
            return Some(widen(s, &mag_up(&term)));
        }
        s = s.sub(&term, wp);
        pw = pw.mul(&r2, wp);
    }
    None
}

/// psi(x) on a ball without a pole.
pub fn digamma_ball(x: &Ball, wp: u64) -> Option<Ball> {
    if is_pos(x) {
        let n = shift(x, wp, 0)?;
        let mut s = digamma_asym(&x.add(&int(n as i64), wp), wp)?;
        for j in 0..n {
            if j % 64 == 0 && stopped() {
                return None;
            }
            s = s.sub(&int(1).div(&x.add(&int(j as i64), wp), wp)?, wp);
        }
        return Some(s);
    }
    if contains_int_at_most(x, 0) {
        return None;
    }
    let (s, c) = (sin_pi(x, wp)?, super::cos_pi(x, wp)?);
    let cot = c.div(&s, wp)?;
    Some(digamma_ball(&int(1).sub(x, wp), wp)?.sub(&pi(wp)?.mul(&cot, wp), wp))
}

/// n! as an exact ball.
fn factorial(n: u64) -> Ball {
    let mut f = mc_arith::num::Nat::from_u64(1);
    for k in 2..=n {
        f = f.mul_u64(k);
    }
    Ball::from_nat(f)
}

/// psi^(n)(y), n >= 1, by its asymptotic series, y >= the shift target.
fn polygamma_asym(n: u64, y: &Ball, wp: u64) -> Option<Ball> {
    let r = int(1).div(y, wp)?;
    let rn = el::pow_int(&r, &Int::from_u64(n), wp)?;
    // (n-1)!/y^n + n!/(2 y^(n+1))
    let fn1 = factorial(n - 1);
    let lead = fn1.mul(&rn, wp);
    let mut s = lead.add(&factorial(n).mul(&rn, wp).mul(&r, wp).mul(&Ball::from_ratio(1, 2, wp), wp), wp);
    let target = log2_up(&lead) - wp as f64 - 4.0;
    let r2 = r.mul(&r, wp);
    let mut pw = rn.mul(&r2, wp);
    // f_k = (2k + n - 1)! / (2k)!, from f_1 = (n + 1)! / 2
    let mut f = Rat::from_int(Int::from_nat(fact_nat(n + 1))).mul(&Rat::new(Int::from_i64(1), Int::from_i64(2)));
    for k in 1u64.. {
        if stopped() {
            return None;
        }
        let term = rat_ball(&b2n(k)?.mul(&f), wp).mul(&pw, wp);
        if log2_up(&term) < target {
            let s = widen(s, &mag_up(&term));
            return Some(if n % 2 == 1 { s } else { s.neg() });
        }
        s = s.add(&term, wp);
        pw = pw.mul(&r2, wp);
        let num = Int::from_u64((2 * k + n) * (2 * k + n + 1));
        let den = Int::from_u64((2 * k + 1) * (2 * k + 2));
        f = f.mul(&Rat::new(num, den));
    }
    None
}

pub(crate) fn fact_nat(n: u64) -> mc_arith::num::Nat {
    let mut f = mc_arith::num::Nat::from_u64(1);
    for k in 2..=n {
        f = f.mul_u64(k);
    }
    f
}

/// psi^(n)(x), n >= 1, on a ball without a pole.
pub fn polygamma_ball(n: u64, x: &Ball, wp: u64) -> Option<Ball> {
    if n == 0 {
        return digamma_ball(x, wp);
    }
    if contains_int_at_most(x, 0) {
        return None;
    }
    let wp = wp + 2 * (64 - n.leading_zeros() as u64);
    let m = shift(x, wp, n)?;
    let mut s = polygamma_asym(n, &x.add(&int(m as i64), wp), wp)?;
    let mut sum = int(0);
    let e = Int::from_u64(n + 1);
    for j in 0..m {
        if j % 64 == 0 && stopped() {
            return None;
        }
        let t = int(1).div(&x.add(&int(j as i64), wp), wp)?;
        sum = sum.add(&el::pow_int(&t, &e, wp)?, wp);
    }
    let sum = factorial(n).mul(&sum, wp);
    s = if n % 2 == 1 { s.add(&sum, wp) } else { s.sub(&sum, wp) };
    Some(s)
}

/// B(a, b) on balls.
pub fn beta_ball(a: &Ball, b: &Ball, wp: u64) -> Option<Ball> {
    let ab = a.add(b, wp);
    if is_pos(a) && is_pos(b) {
        let l = lgamma_pos(a, wp)?.add(&lgamma_pos(b, wp)?, wp).sub(&lgamma_pos(&ab, wp)?, wp);
        return el::exp(&l, wp);
    }
    gamma_ball(a, wp)?.mul(&gamma_ball(b, wp)?, wp).div(&gamma_ball(&ab, wp)?, wp)
}

/// The guard bits for an argument: its magnitude (shifts and reflections lose about log2|x| bits).
fn guard(x: &Ball) -> u64 {
    (log2_up(x).max(0.0) as u64) + 24
}

/// Gamma(x) to p bits.
pub fn gamma(x: &Real, p: u64) -> Option<CBall> {
    let g = super::real_ball(x, p)?;
    refine(p, guard(&g), |wp| Some(real(gamma_ball(&super::real_ball(x, wp)?, wp)?)))
}

/// ln Gamma(x) to p bits (complex for negative non-integers).
pub fn log_gamma(x: &Real, p: u64) -> Option<CBall> {
    let g = super::real_ball(x, p)?;
    refine(p, guard(&g), |wp| lgamma_ball(&super::real_ball(x, wp)?, wp))
}

/// psi(x) to p bits.
pub fn digamma(x: &Real, p: u64) -> Option<CBall> {
    let g = super::real_ball(x, p)?;
    refine(p, guard(&g), |wp| Some(real(digamma_ball(&super::real_ball(x, wp)?, wp)?)))
}

/// psi^(n)(x) to p bits, n a non-negative integer.
pub fn polygamma(n: &Real, x: &Real, p: u64) -> Option<CBall> {
    let n = super::exact_int(n)?;
    if n.neg {
        return None;
    }
    let n = n.to_i64()? as u64;
    if n > 100_000 {
        return None;
    }
    let g = super::real_ball(x, p)?;
    refine(p, guard(&g), |wp| Some(real(polygamma_ball(n, &super::real_ball(x, wp)?, wp)?)))
}

/// B(a, b) to p bits.
pub fn beta(a: &Real, b: &Real, p: u64) -> Option<CBall> {
    let g = guard(&super::real_ball(a, p)?).max(guard(&super::real_ball(b, p)?));
    refine(p, g, |wp| Some(real(beta_ball(&super::real_ball(a, wp)?, &super::real_ball(b, wp)?, wp)?)))
}

/// The Taylor coefficients g_0 .. g_{count-1} of 1/Gamma(1 + z) at precision p (for the machine tables):
/// ln(1/Gamma(1 + z)) = gamma z + sum_{k>=2} (-1)^(k+1) zeta(k) z^k / k, exponentiated as a power series.
pub fn rgamma_taylor(count: usize, p: u64) -> Option<Vec<Ball>> {
    let mut l = vec![int(0), super::euler_gamma(p)?];
    for k in 2..count {
        let z = super::zeta::zeta_ball(&int(k as i64), p)?;
        let t = z.div(&int(k as i64), p)?;
        l.push(if k % 2 == 1 { t } else { t.neg() });
    }
    // e = exp(l): n e_n = sum_{k=1}^{n} k l_k e_{n-k}
    let mut e = vec![int(1)];
    for n in 1..count {
        let mut s = int(0);
        for k in 1..=n {
            s = s.add(&l[k].mul(&int(k as i64), p).mul(&e[n - k], p), p);
        }
        e.push(s.div(&int(n as i64), p)?);
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digits(b: &Ball, d: usize) -> String {
        let (s, e) = b.mid.to_sci(d);
        format!("{}{s}e{e}", if b.mid.neg { "-" } else { "" })
    }

    fn q(n: i64, d: i64) -> Real {
        Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)))
    }

    #[test]
    fn published_values() {
        // Gamma(1/3) = 2.678938534707747633655692940974677644128689377957301100950428...
        let g = gamma(&q(1, 3), 200).unwrap().re;
        assert_eq!(digits(&g, 50), "26789385347077476336556929409746776441286893779573e0");
        // Gamma(1/2) = sqrt(pi)
        let g = gamma(&q(1, 2), 200).unwrap().re;
        assert_eq!(digits(&g, 40), "1772453850905516027298167483341145182798e0");
        // psi(1) = -euler_gamma
        let d = digamma(&q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&d, 40), "-5772156649015328606065120900824024310422e-1");
        // Gamma(-3/2) = 4 sqrt(pi) / 3
        let g = gamma(&q(-3, 2), 200).unwrap().re;
        assert_eq!(digits(&g, 30), "236327180120735470306422331112e0");
        // psi'(1) = pi^2/6
        let t = polygamma(&q(1, 1), &q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&t, 30), "164493406684822643647241516665e0");
        assert!(gamma(&q(-2, 1), 100).is_none());
        let lg = log_gamma(&q(-1, 2), 100).unwrap();
        // ln(2 sqrt(pi)) = 1.2655121234846453964889...
        assert_eq!(digits(&lg.re, 20), "12655121234846453965e0");
        assert!(lg.im.mid.neg);
    }
}
