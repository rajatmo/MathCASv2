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
use super::{int, log2_up, lower, mag_up, pi, rat_ball, real, refine, sin_pi, upper, widen};
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
        None => el::exp(&log_int(k, wp)?.mul(&s.neg(), wp), wp),
    }
}

/// The ball log(k) at precision wp of a positive integer k: the logarithms of 1, ..., 4096 at the last 8 precisions
/// are kept (Euler-Maclaurin sums take the same logarithms at every call of zeta and Hurwitz zeta at a precision; a
/// kept ball is the one computed, so the sums are the same).
fn log_int(k: u64, wp: u64) -> Option<Ball> {
    const KEPT: u64 = 4096;
    type Table = Vec<(u64, Vec<Option<Ball>>)>;
    static CACHE: std::sync::Mutex<Table> = std::sync::Mutex::new(Vec::new());
    if k == 0 || k > KEPT || wp > 1 << 16 {
        return el::log(&int(k as i64), wp);
    }
    if let Some(v) = CACHE.lock().ok()?.iter().find(|e| e.0 == wp).and_then(|e| e.1[k as usize].clone()) {
        return Some(v);
    }
    let v = el::log(&int(k as i64), wp)?;
    if let Ok(mut c) = CACHE.lock() {
        let pos = match c.iter().position(|e| e.0 == wp) {
            Some(i) => i,
            None => {
                if c.len() >= 8 {
                    c.remove(0);
                }
                c.push((wp, vec![None; KEPT as usize + 1]));
                c.len() - 1
            }
        };
        c[pos].1[k as usize] = Some(v.clone());
    }
    Some(v)
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
    // a ball containing the pole has no value
    s.cmp3(&one)?;
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

/// The Hurwitz zeta function zeta(s, a) on balls, a > 0 and s > -1 excluding 1, by Euler-Maclaurin with the terms
/// shifted by a: sum_{k<N} (k + a)^-s + (N + a)^(1-s)/(s - 1) + (N + a)^-s/2 + sum_j B_2j/(2j)! (s)_{2j-1}
/// (N + a)^(-s-2j+1) - R, |R| <= 4 |(s)_2M|/(2 pi)^2M (N + a)^(1 - sigma - 2M)/(sigma + 2M - 1) (Johansson 2015,
/// theorem 1, which holds for Re(N + a) > 0). N and M are chosen as for a = 0 (the bound only falls as N + a grows).
pub fn hurwitz_ball(s: &Ball, a: &Ball, wp: u64) -> Option<Ball> {
    let one = int(1);
    s.cmp3(&one)?;
    if !super::is_pos(a) || super::f64_of(&lower(s)) <= -1.0 {
        return None;
    }
    let sigma = super::f64_of(&lower(s));
    let sa = super::f64_of(&s.mid).abs() + super::f64_of(&s.rad.to_float());
    let (n, m) = choose(sigma, sa, wp as f64 + 8.0)?;
    let pw = |x: &Ball, e: &Ball| -> Option<Ball> {
        // an exact positive integer (an integer a): its logarithm is kept
        let l = match super::float_int(&x.mid).filter(|_| x.rad.is_zero()).and_then(|v| v.to_i64()) {
            Some(k) if k >= 1 => log_int(k as u64, wp)?,
            _ => el::log(x, wp)?,
        };
        el::exp(&l.mul(e, wp), wp)
    };
    let ns = s.neg();
    let mut sum = int(0);
    for k in 0..n {
        if k % 32 == 0 && stopped() {
            return None;
        }
        sum = sum.add(&pw(&a.add(&int(k as i64), wp), &ns)?, wp);
    }
    let na = a.add(&int(n as i64), wp);
    let n_s = pw(&na, &ns)?;
    sum = sum.add(&na.mul(&n_s, wp).div(&s.sub(&one, wp), wp)?, wp);
    sum = sum.add(&n_s.mul(&Ball::from_ratio(1, 2, wp), wp), wp);
    let r = one.div(&na, wp)?;
    let r2 = r.mul(&r, wp);
    let mut poch = s.clone();
    let mut pwr = n_s.mul(&r, wp);
    let mut fact = Rat::from_i64(2);
    for j in 1..=m {
        if stopped() {
            return None;
        }
        let c = b2n(j)?.mul(&fact.recip()?);
        sum = sum.add(&rat_ball(&c, wp).mul(&poch, wp).mul(&pwr, wp), wp);
        poch = poch.mul(&s.add(&int(2 * j as i64 - 1), wp), wp).mul(&s.add(&int(2 * j as i64), wp), wp);
        pwr = pwr.mul(&r2, wp);
        fact = fact.mul(&Rat::from_i64(((2 * j + 1) * (2 * j + 2)) as i64));
    }
    let poch2m = poch_2m(s, m, wp);
    let sig = Ball::exact(lower(s));
    let e = one.sub(&sig, wp).sub(&int(2 * m as i64), wp);
    // (N + a)^e with e < 0 is largest at the lower end of N + a
    let npow = pw(&Ball::exact(lower(&na)), &e)?;
    let den = sig.add(&int(2 * m as i64 - 1), wp);
    if !super::is_pos(&den) {
        return None;
    }
    let twopi = pi(wp)?.mul(&int(2), wp);
    let tp = el::pow_int(&twopi, &Int::from_u64(2 * m), wp)?;
    let bound = int(4).mul(&poch2m, wp).mul(&npow, wp).div(&tp.mul(&den, wp), wp)?;
    Some(widen(sum, &mag_up(&bound)))
}

/// The Hurwitz zeta function zeta(s, a) on complex balls, s excluding 1 and Re a > 0, by Euler-Maclaurin as in
/// [`hurwitz_ball`] with the principal powers (a + k)^-s = exp(-s log(a + k)). The remainder is the integral of
/// B_2M(t - floor t)/(2M)! (s)_2M (t + a)^(-s-2M) over t >= N (DLMF 2.10.1); with |B_2M(t - floor t)| <= 4 (2M)! /
/// (2 pi)^2M, |(t + a)^(-s-2M)| = |t + a|^(-sigma-2M) exp(Im s arg(t + a)) <= (t + Re a)^(-sigma-2M) exp(|Im s|
/// |Im a|/(N + Re a)) (|arg(t + a)| <= |Im a|/(t + Re a), largest at t = N) and the integral of (t + Re a)^(-sigma-2M),
/// |R| <= 4 |(s)_2M|/(2 pi)^2M (N + Re a)^(1 - sigma - 2M)/(sigma + 2M - 1) exp(|Im s| |Im a|/(N + Re a)) for
/// sigma + 2M > 1 (the bound of Johansson 2015, theorem 1, with the phase factor of a complex a). At a large negative
/// real part the terms are far larger than the value; the working precision absorbs that ([`hurwitz_complex`]).
pub fn hurwitz_cball(s: &CBall, a: &CBall, wp: u64) -> Option<CBall> {
    use mc_numeric::cball;
    if !super::is_pos(&lower_ball(&a.re)) {
        return None;
    }
    let one = CBall::from_i64(1);
    let s1 = s.sub(&one, wp);
    if s1.contains_zero() {
        return None;
    }
    let sigma = super::f64_of(&lower(&s.re));
    let sabs = super::f64_of(&cball::abs(s, 64)?.mid) + super::f64_of(&s.re.rad.to_float()) + super::f64_of(&s.im.rad.to_float());
    let (n, m) = choose(sigma, sabs, wp as f64 + 8.0)?;
    let pw = |x: &CBall, e: &CBall| -> Option<CBall> { cball::exp(&cball::log(x, wp)?.mul(e, wp), wp) };
    let ns = s.neg();
    let mut sum = CBall::from_i64(0);
    for k in 0..n {
        if k % 32 == 0 && stopped() {
            return None;
        }
        sum = sum.add(&pw(&a.add(&CBall::from_i64(k as i64), wp), &ns)?, wp);
    }
    let na = a.add(&CBall::from_i64(n as i64), wp);
    let n_s = pw(&na, &ns)?;
    sum = sum.add(&na.mul(&n_s, wp).div(&s1, wp)?, wp);
    sum = sum.add(&n_s.mul_real(&Ball::from_ratio(1, 2, wp), wp), wp);
    let r = na.recip(wp)?;
    let r2 = r.mul(&r, wp);
    let mut poch = s.clone();
    let mut pwr = n_s.mul(&r, wp);
    let mut fact = Rat::from_i64(2);
    for j in 1..=m {
        if stopped() {
            return None;
        }
        let c = b2n(j)?.mul(&fact.recip()?);
        sum = sum.add(&poch.mul(&pwr, wp).mul_real(&rat_ball(&c, wp), wp), wp);
        poch = poch.mul(&s.add(&CBall::from_i64(2 * j as i64 - 1), wp), wp).mul(&s.add(&CBall::from_i64(2 * j as i64), wp), wp);
        pwr = pwr.mul(&r2, wp);
        fact = fact.mul(&Rat::from_i64(((2 * j + 1) * (2 * j + 2)) as i64));
    }
    // |(s)_2M| from above
    let mut poch2m = int(1);
    for i in 0..2 * m {
        let f = cball::abs(&s.add(&CBall::from_i64(i as i64), wp), wp)?;
        poch2m = poch2m.mul(&Ball::exact(mag_up(&f).to_float()), wp);
    }
    let sig = Ball::exact(lower(&s.re));
    let den = sig.add(&int(2 * m as i64 - 1), wp);
    if !super::is_pos(&den) {
        return None;
    }
    // N + Re a at its lower end, the exponent 1 - sigma - 2M < 0
    let base = Ball::exact(lower(&na.re));
    let e = int(1).sub(&sig, wp).sub(&int(2 * m as i64), wp);
    let npow = el::exp(&el::log(&base, wp)?.mul(&e, wp), wp)?;
    let im_s = Ball::exact(mag_up(&s.im).to_float());
    let im_a = Ball::exact(mag_up(&a.im).to_float());
    let phase = el::exp(&im_s.mul(&im_a, wp).div(&base, wp)?, wp)?;
    let twopi = pi(wp)?.mul(&int(2), wp);
    let tp = el::pow_int(&twopi, &Int::from_u64(2 * m), wp)?;
    let bound = int(4).mul(&poch2m, wp).mul(&npow, wp).mul(&phase, wp).div(&tp.mul(&den, wp), wp)?;
    let b = mag_up(&bound);
    Some(CBall::new(widen(sum.re, &b), widen(sum.im, &b)))
}

/// The lower end of a ball as an exact ball.
fn lower_ball(x: &Ball) -> Ball {
    Ball::exact(lower(x))
}

/// zeta(s, a) to p bits for complex s (excluding 1) and a with Re a > 0, the working precision raised until the
/// cancellation of the terms is absorbed. `args(wp)` gives s and a at the working precision (an exact number is
/// rounded there: its rounding error at p bits would be multiplied by the cancellation).
pub fn hurwitz_complex(args: &dyn Fn(u64) -> Option<(CBall, CBall)>, p: u64) -> Option<CBall> {
    let (s, a) = args(64)?;
    // the terms are as large as |N + a|^-sigma: that many bits more at the start
    let sigma = super::f64_of(&s.re.mid);
    let size = super::f64_of(&mc_numeric::cball::abs(&s, 64)?.mid) + super::f64_of(&mc_numeric::cball::abs(&a, 64)?.mid) + 16.0;
    let guard = if sigma < 0.0 { (-sigma * size.log2()).ceil() as u64 + 24 } else { 24 };
    refine(p, guard, |wp| {
        let (s, a) = args(wp)?;
        hurwitz_cball(&s, &a, wp)
    })
}

/// zeta(s, a) to p bits (real s > -1 excluding 1, real a > 0).
pub fn hurwitz(s: &Real, a: &Real, p: u64) -> Option<CBall> {
    let b = super::real_ball(s, p)?;
    let guard = log2_up(&b).max(0.0) as u64 + 24;
    refine(p, guard, |wp| Some(real(hurwitz_ball(&super::real_ball(s, wp)?, &super::real_ball(a, wp)?, wp)?)))
}

/// zeta(s) to p bits.
pub fn zeta(s: &Real, p: u64) -> Option<CBall> {
    let b = super::real_ball(s, p)?;
    let guard = log2_up(&b).max(0.0) as u64 + 24;
    refine(p, guard, |wp| Some(real(zeta_ball(&super::real_ball(s, wp)?, wp)?)))
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
        let z = |n: i64, d: i64| zeta(&Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d))), 200).unwrap().re;
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
    }
}
