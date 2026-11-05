//! The Riemann zeta function on balls.
//!
//! - **Euler-Maclaurin** (for s > -1, s != 1): zeta(s) = sum_{k=1}^{N-1} k^-s + N^(1-s)/(s - 1) + N^-s / 2 +
//!   sum_{j=1}^{M} B_2j / (2j)! (s)_{2j-1} N^(-s-2j+1) - R, with (s)_m the rising factorial and the remainder bound
//!   |R| <= 4 |(s)_{2M}| / (2 pi)^(2M) N^(1 - sigma - 2M) / (sigma + 2M - 1) for sigma + 2M - 1 > 0 (from
//!   |B_2M(t - floor t)| <= 4 (2M)! / (2 pi)^(2M); Johansson 2015, theorem 1). The bound is evaluated with ball
//!   arithmetic at the ends of s that maximise it and added to the radius. N and M are chosen to minimise the work
//!   for a bound below 2^-p.
//! - **The functional equation** for s <= -1: zeta(s) = 2 (2 pi)^(s-1) sin(pi s / 2) Gamma(1 - s) zeta(1 - s), with
//!   sin(pi s / 2) reduced exactly so that the trivial zeros keep their relative accuracy.
use super::gamma::gamma_pos;
use super::{float_of, int, log2_up, lower, mag_up, pi, rat_ball, real, refine, sin_pi, upper, widen};
use crate::bernoulli::b2n;
use mc_arith::num::{stopped, Ball, Int, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;

/// (N, M) for a remainder below 2^-target: the cheapest pair whose bound reaches it (a power costs about four
/// terms of the correction sum). The bound 4 |(s)_2M| / (2 pi)^2M N^(1 - sigma - 2M) / (sigma + 2M - 1) is followed
/// in machine arithmetic (log2, incrementally in M), only to choose; the rigorous bound is computed with balls.
fn choose(sigma: f64, a: f64, target: f64) -> Option<(u64, u64)> {
    let l2pi = (2.0 * std::f64::consts::PI).log2();
    let mut best: Option<(u64, u64, f64)> = None;
    let mut n = 2u64;
    while n < 1 << 22 {
        if stopped() {
            return None;
        }
        let ln = (n as f64).log2();
        let mut poch = 0.0;
        let mut prev = f64::INFINITY;
        for m in 1..=(4 * target as u64 + 64) {
            let mf = m as f64;
            poch += (a + 2.0 * mf - 2.0).max(1e-300).log2() + (a + 2.0 * mf - 1.0).max(1e-300).log2();
            let den = sigma + 2.0 * mf - 1.0;
            if den <= 0.0 {
                continue;
            }
            let b = 2.0 + poch - 2.0 * mf * l2pi + (1.0 - sigma - 2.0 * mf) * ln - den.log2();
            if b < -target {
                let cost = 4.0 * n as f64 + mf;
                if best.is_none_or(|(_, _, c)| cost < c) {
                    best = Some((n, m, cost));
                }
                break;
            }
            if b > prev || best.is_some_and(|(_, _, c)| 4.0 * n as f64 + mf > c) {
                break;
            }
            prev = b;
        }
        if best.is_some_and(|(_, _, c)| 4.0 * n as f64 > c) {
            break;
        }
        n = (n * 5).div_ceil(4).max(n + 1);
    }
    best.map(|(n, m, _)| (n, m))
}

/// k^-s, by an integer power when s is an exact integer.
fn pow_neg(k: u64, s: &Ball, s_int: Option<&Int>, wp: u64) -> Option<Ball> {
    let kb = int(k as i64);
    match s_int {
        Some(n) => el::pow_int(&kb, &n.neg(), wp),
        None => el::exp(&el::log(&kb, wp)?.mul(&s.neg(), wp), wp),
    }
}

/// zeta(s) by Euler-Maclaurin for a ball s excluding 1 with sigma > -1.
fn euler_maclaurin(s: &Ball, wp: u64) -> Option<Ball> {
    let s_int = super::float_int(&s.mid).filter(|_| s.rad.is_zero());
    let sigma = super::f64_of(&lower(s));
    let a = super::f64_of(&s.mid).abs() + super::f64_of(&s.rad.to_float());
    let (n, m) = choose(sigma, a, wp as f64 + 8.0)?;
    let mut sum = int(0);
    for k in 1..n {
        if k % 32 == 0 && stopped() {
            return None;
        }
        sum = sum.add(&pow_neg(k, s, s_int.as_ref(), wp)?, wp);
    }
    let nb = int(n as i64);
    let n_s = pow_neg(n, s, s_int.as_ref(), wp)?;
    let s1 = s.sub(&int(1), wp);
    sum = sum.add(&nb.mul(&n_s, wp).div(&s1, wp)?, wp);
    sum = sum.add(&n_s.mul(&Ball::from_ratio(1, 2, wp), wp), wp);
    // the correction terms: poch = (s)_{2j-1}, pw = N^(-s-2j+1)
    let r = int(1).div(&nb, wp)?;
    let r2 = r.mul(&r, wp);
    let mut poch = s.clone();
    let mut pw = n_s.mul(&r, wp);
    let mut fact = Rat::from_i64(2); // (2j)!
    for j in 1..=m {
        if stopped() {
            return None;
        }
        let c = b2n(j)?.mul(&fact.recip()?);
        sum = sum.add(&rat_ball(&c, wp).mul(&poch, wp).mul(&pw, wp), wp);
        // (s)_{2j+1} = (s)_{2j-1} (s + 2j - 1) (s + 2j)
        poch = poch.mul(&s.add(&int(2 * j as i64 - 1), wp), wp).mul(&s.add(&int(2 * j as i64), wp), wp);
        pw = pw.mul(&r2, wp);
        fact = fact.mul(&Rat::from_i64(((2 * j + 1) * (2 * j + 2)) as i64));
    }
    // the remainder: (s)_{2M} = (s)_{2M-1} (s + 2M - 1); N^(1 - sigma - 2M) at the lower end of sigma
    let poch2m = poch_2m(s, m, wp);
    let sig = Ball::exact(lower(s));
    let e = int(1).sub(&sig, wp).sub(&int(2 * m as i64), wp);
    let npow = el::exp(&el::log(&nb, wp)?.mul(&e, wp), wp)?;
    let den = sig.add(&int(2 * m as i64 - 1), wp);
    if !super::is_pos(&den) {
        return None;
    }
    let twopi = pi(wp)?.mul(&int(2), wp);
    let tp = el::pow_int(&twopi, &Int::from_u64(2 * m), wp)?;
    let bound = int(4).mul(&poch2m, wp).mul(&npow, wp).div(&tp.mul(&den, wp), wp)?;
    Some(widen(sum, &mag_up(&bound)))
}

/// |(s)_{2M}| as a ball (only its upper end matters).
fn poch_2m(s: &Ball, m: u64, wp: u64) -> Ball {
    let mut p = int(1);
    for i in 0..2 * m {
        let f = s.add(&int(i as i64), wp);
        p = p.mul(&Ball::exact(mag_up(&f).to_float()), wp);
    }
    p
}

/// zeta(s) on a ball excluding 1.
pub fn zeta_ball(s: &Ball, wp: u64) -> Option<Ball> {
    let one = int(1);
    if s.cmp3(&one).is_none() {
        return None;
    }
    if super::f64_of(&lower(s)) > -1.0 {
        return euler_maclaurin(s, wp);
    }
    if super::f64_of(&upper(s)) > -0.5 {
        return None;
    }
    // the functional equation
    let half = s.mul(&Ball::from_ratio(1, 2, wp), wp);
    let sn = sin_pi(&half, wp)?;
    let one_minus = one.sub(s, wp);
    let g = gamma_pos(&one_minus, wp)?;
    let z = euler_maclaurin(&one_minus, wp)?;
    let twopi = pi(wp)?.mul(&int(2), wp);
    let tp = el::exp(&el::log(&twopi, wp)?.mul(&s.sub(&one, wp), wp), wp)?;
    Some(int(2).mul(&tp, wp).mul(&sn, wp).mul(&g, wp).mul(&z, wp))
}

/// zeta(s) to p bits.
pub fn zeta(s: &Real, p: u64) -> Option<CBall> {
    let b = super::real_ball(s, p)?;
    let guard = log2_up(&b).max(0.0) as u64 + 24;
    refine(p, guard, |wp| Some(real(zeta_ball(&super::real_ball(s, wp)?, wp)?)))
}

/// A ball at the double nearest x (for tests and the tables).
pub fn exact_f64(x: f64) -> Ball {
    Ball::exact(float_of(x))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digits(b: &Ball, d: usize) -> String {
        let (s, e) = b.mid.to_sci(d);
        format!("{}{s}e{e}", if b.mid.neg { "-" } else { "" })
    }

    #[test]
    fn published_values() {
        let z = |n: i64, d: i64| {
            zeta(&Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d))), 200).unwrap().re
        };
        // Apery's constant 1.2020569031595942853997381615114499907649862923404988817922715553...
        assert_eq!(digits(&z(3, 1), 50), "12020569031595942853997381615114499907649862923405e0");
        // zeta(1/2) = -1.4603545088095868128894991525152980125030...
        assert_eq!(digits(&z(1, 2), 40), "-1460354508809586812889499152515298012467e0");
        // zeta(-1) = -1/12, zeta(-3/2) = -0.025485201889833035949542...
        assert_eq!(digits(&z(-1, 1), 30), "-833333333333333333333333333333e-2");
        assert_eq!(digits(&z(-3, 2), 20), "-25485201889833035950e-2");
        // zeta(2) = pi^2/6, zeta(60) - 1 ~ 8.67e-19
        assert_eq!(digits(&z(2, 1), 30), "164493406684822643647241516665e0");
        assert!(zeta(&Real::Exact(Rat::from_i64(1)), 64).is_none());
        let _ = exact_f64(0.5);
    }
}
