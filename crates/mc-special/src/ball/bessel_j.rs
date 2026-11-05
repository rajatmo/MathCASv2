//! Bessel functions of the first and second kind J_nu(x), Y_nu(x) and the Airy functions Ai, Bi, Ai', Bi' on
//! balls, real nu and x.
//!
//! - **J_nu, power series:** J_nu(x) = (x/2)^nu / Gamma(nu + 1) sum_k (-x^2/4)^k / (k! (nu + 1)_k). Once the
//!   term ratio r_k = (x^2/4) / ((k + 1)(k + 1 + nu)) is at most 1/2 with k + 1 + nu > 0 (it decreases from there
//!   on), the tail after a term t is at most 2|t|. The terms grow to about I_nu(x) <= e^x before they cancel, so the
//!   series runs with 1.45 x + 16 extra bits. Integer orders allow any real x (J_n(-x) = (-1)^n J_n(x),
//!   J_-n = (-1)^n J_n); other orders need x > 0.
//! - **Y_n, integer n:** the series with digamma coefficients (Abramowitz and Stegun 9.1.11)
//!   Y_n(x) = -(1/pi)(x/2)^-n sum_{k<n} (n-k-1)!/k! (x^2/4)^k + (2/pi) ln(x/2) J_n(x) -
//!   (1/pi)(x/2)^n sum_k (psi(k+1) + psi(n+k+1)) (-x^2/4)^k / (k! (n+k)!), psi(k+1) = H_k - gamma; from k = 1 on
//!   the coefficient H_k + H_{n+k} - 2 gamma is at least 0.84 and grows by a factor at most 1 + 2.4/(k+1), which
//!   bounds the tail as for J. Y_-n = (-1)^n Y_n.
//! - **Y_nu, other nu:** (J_nu cos(nu pi) - J_-nu) / sin(nu pi), with -log2 |nu - n| extra bits near an integer n.
//! - **Large x:** Hankel's expansion J_nu(x) = sqrt(2/(pi x)) (P cos w - Q sin w),
//!   Y_nu(x) = sqrt(2/(pi x)) (P sin w + Q cos w), w = x - (nu/2 + 1/4) pi, P = sum_k (-1)^k a_2k(nu)/x^2k,
//!   Q = sum_k (-1)^k a_2k+1(nu)/x^(2k+1), a_k(nu) = prod_{j=1}^{k} (4 nu^2 - (2j-1)^2) / (k! 8^k). For real nu and
//!   x > 0 the remainder of P after l terms is bounded by the first neglected term when l >= max(|nu|/2 - 1/4, 1),
//!   that of Q when l >= max(|nu|/2 - 3/4, 1) (Watson 7.32, DLMF 10.17(iii)); we sum at least ceil(|nu|/2) + 1
//!   terms of each, which satisfies both. Used when both first neglected terms are below 2^-wp and no term
//!   exceeds 2^8 (for orders beyond about sqrt(x) the terms grow huge first, and the sums would cancel).
//! - **Airy, Maclaurin series** (Abramowitz and Stegun 10.4.2-3): Ai = c1 f - c2 g, Bi = sqrt(3) (c1 f + c2 g),
//!   c1 = Ai(0) = 3^(-2/3)/Gamma(2/3), c2 = -Ai'(0) = 3^(-1/3)/Gamma(1/3), f = sum_k 3^k (1/3)_k x^3k/(3k)!,
//!   g = sum_k 3^k (2/3)_k x^(3k+1)/(3k+1)!, and the derivatives term by term. Each series has terms
//!   t_k = t_{k-1} x^3 / d(k) with d increasing, so once |x|^3/d(k+1) <= 1/2 the tail is at most 2|t_k|. Extra bits
//!   for the cancellation: 2.9 zeta (Ai, Ai' at x > 0), 1.45 zeta (x < 0), zeta = (2/3)|x|^(3/2).
//! - **Ai, Ai' for large x > 0:** Ai(x) = e^-zeta/(2 sqrt(pi) x^(1/4)) S_1/3, Ai'(x) = -x^(1/4) e^-zeta/(2 sqrt(pi))
//!   S_2/3 with S_nu = sum_k a_k(nu)/zeta^k, the expansion of K_nu(zeta): for K >= |nu| - 1/2 terms the remainder is
//!   bounded by the first omitted term (DLMF 10.40.10), as in [`super::bessel`].
//! - **Large x < 0:** with z = -x, phi = zeta - pi/4 and (P_nu, Q_nu) of Hankel's expansion at zeta (bounds as
//!   above): Ai(-z) = (P cos phi - Q sin phi)_1/3 / (sqrt(pi) z^(1/4)), Bi(-z) = -(P sin phi + Q cos phi)_1/3 /
//!   (sqrt(pi) z^(1/4)), Ai'(-z) = z^(1/4) (P sin phi + Q cos phi)_2/3 / sqrt(pi), Bi'(-z) = z^(1/4) (P cos phi -
//!   Q sin phi)_2/3 / sqrt(pi) (from DLMF 9.6.6-9 and the J, Y expansions).
//!
//! The asymptotic forms are tried first where they can reach 2^-wp; the series take over otherwise. Ball arguments
//! with radii are evaluated by the same ball arithmetic (every bound above holds pointwise, and the switching tests
//! use the ends of the balls), so the result contains the function's values over the whole argument box. A ball
//! order containing an integer gives no Y value (the quotient's divisor contains zero).
use super::gamma::gamma_ball;
use super::{euler_gamma, f64_of, int, is_pos, log2_up, lower, mag_up, pi, real, real_ball, refine, upper, widen};
use mc_arith::num::{stopped, Ball, Int, Nat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;

/// The largest term count of a series before giving up (the loops also poll the stop flag).
const MAX_TERMS: u64 = 1 << 22;

fn nat_fact(n: u64) -> Nat {
    let mut f = Nat::from_u64(1);
    for k in 2..=n {
        f = f.mul_u64(k);
    }
    f
}

/// An upper bound of |b| as a double.
fn hi_abs(b: &Ball) -> f64 {
    f64_of(&mag_up(b).to_float()) * (1.0 + 1e-12)
}

/// The lower end of a ball as a double (rounded to nearest; used only to choose methods).
fn lo_f(b: &Ball) -> f64 {
    f64_of(&lower(b))
}

/// The upper end of a ball as a double (rounded to nearest; used only to choose methods).
fn hi_f(b: &Ball) -> f64 {
    f64_of(&upper(b))
}

fn is_exact_zero(b: &Ball) -> bool {
    b.mid.is_zero() && b.rad.is_zero()
}

/// sum_k t_k with t_{k+1} = -t_k q / ((k+1)(k+1+nu)) from t_0, q = x^2/4; `nu_lo` a lower bound of nu.
fn j_sum(t0: Ball, q: &Ball, nu: &Ball, nu_lo: f64, wp: u64) -> Option<Ball> {
    let q_hi = hi_abs(q);
    let mq = q.neg();
    let mut t = t0;
    let mut s = t.clone();
    let mut big = log2_up(&s);
    for k in 0..MAX_TERMS {
        if k % 32 == 0 && stopped() {
            return None;
        }
        if is_exact_zero(&t) {
            return Some(s);
        }
        let d = nu.add(&int(k as i64 + 1), wp).mul(&int(k as i64 + 1), wp);
        t = t.mul(&mq, wp).div(&d, wp)?;
        big = big.max(log2_up(&s));
        // the ratio t_{k+2}/t_{k+1} and all later ones are at most 1/2
        let m = k as f64 + 2.0 + nu_lo;
        if m > 0.0 && q_hi / ((k as f64 + 2.0) * m) <= 0.5 && log2_up(&t) < big - wp as f64 - 4.0 {
            return Some(widen(s, &mag_up(&t).mul_2exp(1)));
        }
        s = s.add(&t, wp);
    }
    None
}

/// J_nu(x) by its power series: `n` the exact integer value of nu if it has one.
fn j_series(nu: &Ball, n: Option<i64>, x: &Ball, wp: u64) -> Option<Ball> {
    let half = x.mul(&Ball::from_ratio(1, 2, wp), wp);
    let q = half.mul(&half, wp);
    if let Some(n) = n {
        let m = n.unsigned_abs();
        let t0 = el::pow_int(&half, &Int::from_u64(m), wp)?.div(&Ball::from_nat(nat_fact(m)), wp)?;
        let s = j_sum(t0, &q, &int(m as i64), m as f64, wp)?;
        return Some(if n < 0 && m % 2 == 1 { s.neg() } else { s });
    }
    if !is_pos(x) {
        return None;
    }
    let pw = el::exp(&nu.mul(&el::log(&half, wp)?, wp), wp)?;
    let t0 = pw.div(&gamma_ball(&nu.add(&int(1), wp), wp)?, wp)?;
    j_sum(t0, &q, nu, lo_f(nu), wp)
}

/// Hankel's P(nu, x) and Q(nu, x) with their remainders added to the radii, if the first neglected terms reach
/// 2^-wp (x > 0, nu real; only nu^2 enters).
fn hankel_pq(nu: &Ball, x: &Ball, wp: u64) -> Option<(Ball, Ball)> {
    let nu_hi = hi_abs(nu);
    // terms of each of P and Q: at least max(|nu|/2 - 1/4, 1) (DLMF 10.17(iii)); ceil(|nu|/2) + 1 is more
    let lmin = (nu_hi / 2.0).ceil() as u64 + 1;
    let mu4 = nu.mul(nu, wp).mul(&int(4), wp);
    let r = int(1).div(&x.mul(&int(8), wp), wp)?;
    let cap = (4.0 * lo_f(x) + 2.0 * nu_hi + 32.0).min(1e7) as u64;
    let (mut p, mut q) = (int(0), int(0));
    let mut t = int(1);
    let mut prev = f64::INFINITY;
    let lim = -(wp as f64) - 4.0;
    for m in 0..cap {
        if m % 32 == 0 && stopped() {
            return None;
        }
        let j = 2 * m as i64 + 1;
        let next = t.mul(&mu4.sub(&int(j * j), wp), wp).mul(&r, wp).div(&int(m as i64 + 1), wp)?;
        let lt = log2_up(&t);
        if m % 2 == 0 && m / 2 >= lmin && lt < lim && log2_up(&next) < lim {
            // P has m/2 terms (its first neglected one is t_m), Q has m/2 terms (first neglected t_m+1)
            return Some((widen(p, &mag_up(&t)), widen(q, &mag_up(&next))));
        }
        // diverging, or terms so large (orders beyond about sqrt(x)) that the sums would cancel
        if m > 2 * lmin + 2 && lt > prev || lt > 8.0 {
            return None;
        }
        prev = lt;
        match m % 4 {
            0 => p = p.add(&t, wp),
            1 => q = q.add(&t, wp),
            2 => p = p.sub(&t, wp),
            _ => q = q.sub(&t, wp),
        }
        t = next;
    }
    None
}

/// (J_nu(x), Y_nu(x)) by Hankel's expansion (x > 0), if it reaches 2^-wp.
fn hankel_jy(nu: &Ball, x: &Ball, wp: u64) -> Option<(Ball, Ball)> {
    let (p, q) = hankel_pq(nu, x, wp)?;
    // the phase loses the integer bits of x
    let w = wp + x.mid.top().max(0) as u64 + 8;
    let pi = pi(w)?;
    let shift = nu.mul(&Ball::from_ratio(1, 2, w), w).add(&Ball::from_ratio(1, 4, w), w);
    let om = x.sub(&shift.mul(&pi, w), w);
    let (s, c) = el::sin_cos(&om, w)?;
    let a = el::sqrt(&int(2).div(&pi.mul(x, wp), wp)?, wp)?;
    let j = a.mul(&p.mul(&c, wp).sub(&q.mul(&s, wp), wp), wp);
    let y = a.mul(&p.mul(&s, wp).add(&q.mul(&c, wp), wp), wp);
    Some((j, y))
}

/// Extra bits for the cancellation of the power series at |x| <= `xh`.
fn series_extra(xh: f64) -> u64 {
    (1.45 * xh).ceil().min(1e7) as u64 + 16
}

/// J_nu(x) at working precision wp: `n` the exact integer value of nu if it has one.
fn j_point(nu: &Ball, n: Option<i64>, x: &Ball, wp: u64) -> Option<Ball> {
    if is_exact_zero(x) {
        // J_0(0) = 1, J_nu(0) = 0 for nu > 0 and for integers; a pole for other negative nu
        if n == Some(0) {
            return Some(int(1));
        }
        return (n.is_some() || is_pos(nu)).then(|| int(0));
    }
    if let Some(n) = n {
        if hi_f(x) < 0.0 {
            let v = j_point(nu, Some(n), &x.neg(), wp)?;
            return Some(if n % 2 != 0 { v.neg() } else { v });
        }
    }
    if lo_f(x) > 2.0 {
        if let Some((j, _)) = hankel_jy(nu, x, wp + 8) {
            return Some(j);
        }
    }
    j_series(nu, n, x, wp + series_extra(hi_abs(x)))
}

/// Y_n(x) for an integer n >= 0 and x > 0 by A&S 9.1.11.
fn y_int_series(n: u64, x: &Ball, wp: u64) -> Option<Ball> {
    let half = x.mul(&Ball::from_ratio(1, 2, wp), wp);
    let q = half.mul(&half, wp);
    let hn = el::pow_int(&half, &Int::from_u64(n), wp)?;
    let pi = pi(wp)?;
    // the finite part
    let mut a = int(0);
    if n > 0 {
        let mut t = Ball::from_nat(nat_fact(n - 1)); // (n-1)!/0!
        for k in 0..n {
            a = a.add(&t, wp);
            if k + 1 < n {
                // (n-k-2)!/(k+1)! = (n-k-1)!/k! / ((n-k-1)(k+1))
                t = t.mul(&q, wp).div(&int(((n - k - 1) * (k + 1)) as i64), wp)?;
            }
        }
        a = a.div(&hn, wp)?.div(&pi, wp)?.neg();
    }
    // the logarithmic part
    let j = j_series(&int(n as i64), Some(n as i64), x, wp)?;
    let b = el::log(&half, wp)?.mul(&j, wp).mul(&int(2), wp).div(&pi, wp)?;
    // the digamma series: u_k = (-q)^k / (k! (n+k)!), c_k = H_k + H_{n+k} - 2 gamma
    let mq = q.neg();
    let g2 = euler_gamma(wp)?.mul(&int(2), wp);
    let mut hk = int(0);
    let mut hnk = int(0);
    for j in 1..=n {
        hnk = hnk.add(&int(1).div(&int(j as i64), wp)?, wp);
    }
    let mut u = int(1).div(&Ball::from_nat(nat_fact(n)), wp)?;
    let mut s = hk.add(&hnk, wp).sub(&g2, wp).mul(&u, wp);
    let q_hi = hi_abs(&q);
    let mut big = log2_up(&s);
    let mut done = None;
    for k in 0..MAX_TERMS {
        if k % 32 == 0 && stopped() {
            return None;
        }
        let k1 = k + 1;
        u = u.mul(&mq, wp).div(&int((k1 * (n + k1)) as i64), wp)?;
        hk = hk.add(&int(1).div(&int(k1 as i64), wp)?, wp);
        hnk = hnk.add(&int(1).div(&int((n + k1) as i64), wp)?, wp);
        let t = hk.add(&hnk, wp).sub(&g2, wp).mul(&u, wp);
        big = big.max(log2_up(&s));
        let next = (k1 as f64 + 1.0) * ((n + k1) as f64 + 1.0);
        let rho = (1.0 + 2.4 / (k1 as f64 + 1.0)) * q_hi / next;
        if rho <= 0.5 && log2_up(&t) < big - wp as f64 - 4.0 {
            done = Some(widen(s, &mag_up(&t).mul_2exp(1)));
            break;
        }
        s = s.add(&t, wp);
    }
    let c = done?.mul(&hn, wp).div(&pi, wp)?.neg();
    Some(a.add(&b, wp).add(&c, wp))
}

/// Y_nu(x) at working precision wp (x > 0).
fn y_point(nu: &Ball, n: Option<i64>, x: &Ball, wp: u64) -> Option<Ball> {
    if !is_pos(x) {
        return None;
    }
    if lo_f(x) > 2.0 {
        if let Some((_, y)) = hankel_jy(nu, x, wp + 8) {
            return Some(y);
        }
    }
    let w = wp + series_extra(hi_abs(x));
    match n {
        Some(n) => {
            let m = n.unsigned_abs();
            let y = y_int_series(m, x, w)?;
            Some(if n < 0 && m % 2 == 1 { y.neg() } else { y })
        }
        None => {
            let (_, f) = super::split_int(nu, w)?;
            let near = (-log2_up(&f)).clamp(0.0, 1e6) as u64;
            let w = w + near;
            let jp = j_series(nu, None, x, w)?;
            let jm = j_series(&nu.neg(), None, x, w)?;
            let s = super::sin_pi(nu, w)?;
            let c = super::cos_pi(nu, w)?;
            jp.mul(&c, w).sub(&jm, w).div(&s, w)
        }
    }
}

/// The integer value of an exact real, or of an exact ball.
fn int_of(r: &Real) -> Option<i64> {
    super::exact_int(r)?.to_i64()
}

/// The argument x at working precision wp (with headroom for the phase of the large-x expansions).
fn arg_ball(x: &Real, wp: u64) -> Option<Ball> {
    real_ball(x, wp + 64)
}

/// J_nu(x) to p bits (any real x for integer nu, x >= 0 otherwise).
pub fn bessel_j(nu: &Real, x: &Real, p: u64) -> Option<CBall> {
    let n = int_of(nu);
    refine(p, 24, |wp| {
        let v = real_ball(nu, wp)?;
        Some(real(j_point(&v, n, &arg_ball(x, wp)?, wp)?))
    })
}

/// Y_nu(x) to p bits (x > 0).
pub fn bessel_y(nu: &Real, x: &Real, p: u64) -> Option<CBall> {
    let n = int_of(nu);
    refine(p, 24, |wp| {
        let v = real_ball(nu, wp)?;
        Some(real(y_point(&v, n, &arg_ball(x, wp)?, wp)?))
    })
}

/// Ai(0) = 3^(-2/3)/Gamma(2/3) and -Ai'(0) = 3^(-1/3)/Gamma(1/3) at precision wp (the constants c1, c2 of the
/// Maclaurin series).
pub fn airy_constants(wp: u64) -> Option<(Ball, Ball)> {
    let l3 = el::log(&int(3), wp)?;
    let c1 = el::exp(&l3.mul(&Ball::from_ratio(-2, 3, wp), wp), wp)?;
    let c1 = c1.div(&gamma_ball(&Ball::from_ratio(2, 3, wp), wp)?, wp)?;
    let c2 = el::exp(&l3.mul(&Ball::from_ratio(-1, 3, wp), wp), wp)?;
    let c2 = c2.div(&gamma_ball(&Ball::from_ratio(1, 3, wp), wp)?, wp)?;
    Some((c1, c2))
}

/// Which Airy function.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Airy {
    Ai,
    Bi,
    AiP,
    BiP,
}

impl Airy {
    fn deriv(self) -> bool {
        matches!(self, Airy::AiP | Airy::BiP)
    }
}

/// sum_{k>=0} t_k with t_k = t_{k-1} x^3 / d(k) (d positive and increasing): the tail after t_k is at most 2|t_k|
/// once |x|^3 / d(k+1) <= 1/2.
fn airy_sum(t0: Ball, x3: &Ball, d: impl Fn(u64) -> u64, wp: u64) -> Option<Ball> {
    let x3_hi = hi_abs(x3);
    let mut t = t0;
    let mut s = t.clone();
    let mut big = log2_up(&s);
    for k in 1..MAX_TERMS {
        if k % 32 == 0 && stopped() {
            return None;
        }
        if is_exact_zero(&t) {
            return Some(s);
        }
        t = t.mul(x3, wp).div(&int(d(k) as i64), wp)?;
        big = big.max(log2_up(&s));
        if x3_hi / d(k + 1) as f64 <= 0.5 && log2_up(&t) < big - wp as f64 - 4.0 {
            return Some(widen(s, &mag_up(&t).mul_2exp(1)));
        }
        s = s.add(&t, wp);
    }
    None
}

/// (f, g) of the Maclaurin series, or (f', g').
fn airy_fg(x: &Ball, deriv: bool, wp: u64) -> Option<(Ball, Ball)> {
    let x2 = x.mul(x, wp);
    let x3 = x2.mul(x, wp);
    if !deriv {
        let f = airy_sum(int(1), &x3, |k| (3 * k - 1) * (3 * k), wp)?;
        let g = airy_sum(x.clone(), &x3, |k| (3 * k) * (3 * k + 1), wp)?;
        Some((f, g))
    } else {
        // f' = sum_{k>=1} 3k f_k x^(3k-1): t_0 = x^2/2, ratio x^3/((3j+2) 3j); g' = sum (3k+1) g_k x^3k
        let f = airy_sum(x2.mul(&Ball::from_ratio(1, 2, wp), wp), &x3, |j| (3 * j + 2) * (3 * j), wp)?;
        let g = airy_sum(int(1), &x3, |k| (3 * k - 2) * (3 * k), wp)?;
        Some((f, g))
    }
}

/// The order of the Bessel functions behind an Airy function: 1/3 for Ai, Bi, 2/3 for the derivatives.
fn airy_order(kind: Airy, wp: u64) -> Ball {
    if kind.deriv() {
        Ball::from_ratio(2, 3, wp)
    } else {
        Ball::from_ratio(1, 3, wp)
    }
}

/// Ai or Ai' for x > 0 by the expansion of K_nu(zeta), if its smallest term reaches 2^-wp.
fn airy_k_asymptotic(kind: Airy, x: &Ball, wp: u64) -> Option<Ball> {
    // e^-zeta loses the integer bits of zeta
    let w = wp + (1.5 * x.mid.top().max(0) as f64) as u64 + 8;
    let sx = el::sqrt(x, w)?;
    let zeta = x.mul(&sx, w).mul(&Ball::from_ratio(2, 3, w), w);
    let r = int(1).div(&zeta, wp)?;
    let nu = airy_order(kind, wp);
    let mu4 = nu.mul(&nu, wp).mul(&int(4), wp);
    let cap = (4.0 * lo_f(&zeta) + 32.0).min(1e7) as u64;
    let mut t = int(1);
    let mut s = int(0);
    let mut prev = f64::INFINITY;
    let mut sum = None;
    for k in 0..cap {
        if k % 32 == 0 && stopped() {
            return None;
        }
        let lt = log2_up(&t);
        // K >= |nu| - 1/2 holds for every K >= 1 (nu <= 2/3)
        if k >= 1 && lt < -(wp as f64) - 4.0 {
            sum = Some(widen(s, &mag_up(&t)));
            break;
        }
        if k > 1 && lt > prev {
            return None;
        }
        prev = lt;
        s = s.add(&t, wp);
        let j = 2 * k as i64 + 1;
        t = t.mul(&mu4.sub(&int(j * j), wp), wp).mul(&r, wp).div(&int(8 * (k as i64 + 1)), wp)?;
    }
    let s = sum?;
    let x4 = el::sqrt(&sx, wp)?;
    let e = el::exp(&zeta.neg(), w)?;
    let den = el::sqrt(&pi(wp)?, wp)?.mul(&int(2), wp);
    let v = e.mul(&s, wp).div(&den, wp)?;
    match kind {
        Airy::Ai => v.div(&x4, wp),
        _ => Some(v.mul(&x4, wp).neg()),
    }
}

/// Airy functions for x < 0 through Hankel's expansion at zeta, if it reaches 2^-wp.
fn airy_hankel(kind: Airy, x: &Ball, wp: u64) -> Option<Ball> {
    let z = x.neg();
    let w = wp + (1.5 * z.mid.top().max(0) as f64) as u64 + 8;
    let sz = el::sqrt(&z, w)?;
    let zeta = z.mul(&sz, w).mul(&Ball::from_ratio(2, 3, w), w);
    let (p, q) = hankel_pq(&airy_order(kind, wp), &zeta, wp)?;
    let pi = pi(w)?;
    let phi = zeta.sub(&pi.mul(&Ball::from_ratio(1, 4, w), w), w);
    let (s, c) = el::sin_cos(&phi, w)?;
    let z4 = el::sqrt(&sz, wp)?;
    let isp = int(1).div(&el::sqrt(&pi, wp)?, wp)?;
    let pcqs = p.mul(&c, wp).sub(&q.mul(&s, wp), wp);
    let psqc = p.mul(&s, wp).add(&q.mul(&c, wp), wp);
    match kind {
        Airy::Ai => pcqs.mul(&isp, wp).div(&z4, wp),
        Airy::Bi => psqc.mul(&isp, wp).div(&z4, wp).map(|v| v.neg()),
        Airy::AiP => Some(psqc.mul(&isp, wp).mul(&z4, wp)),
        Airy::BiP => Some(pcqs.mul(&isp, wp).mul(&z4, wp)),
    }
}

/// An Airy function at working precision wp.
fn airy_point(kind: Airy, x: &Ball, wp: u64) -> Option<Ball> {
    let (xl, xh) = (lo_f(x), hi_f(x));
    if xl > 1.0 && matches!(kind, Airy::Ai | Airy::AiP) {
        if let Some(v) = airy_k_asymptotic(kind, x, wp + 8) {
            return Some(v);
        }
    }
    if xh < -1.0 {
        if let Some(v) = airy_hankel(kind, x, wp + 8) {
            return Some(v);
        }
    }
    let a = hi_abs(x);
    let zeta = 2.0 / 3.0 * a * a.sqrt();
    let factor = match kind {
        Airy::Ai | Airy::AiP if xh > 0.0 => 2.9,
        Airy::Bi | Airy::BiP if xl >= 0.0 => 0.0,
        _ => 1.45,
    };
    let w = wp + (factor * zeta).ceil().min(1e7) as u64 + 16;
    let (c1, c2) = airy_constants(w)?;
    let (f, g) = airy_fg(x, kind.deriv(), w)?;
    let (cf, cg) = (c1.mul(&f, w), c2.mul(&g, w));
    match kind {
        Airy::Ai | Airy::AiP => Some(cf.sub(&cg, w)),
        Airy::Bi | Airy::BiP => Some(el::sqrt(&int(3), w)?.mul(&cf.add(&cg, w), w)),
    }
}

fn airy(kind: Airy, x: &Real, p: u64) -> Option<CBall> {
    refine(p, 24, |wp| Some(real(airy_point(kind, &arg_ball(x, wp)?, wp)?)))
}

/// Ai(x) to p bits.
pub fn airy_ai(x: &Real, p: u64) -> Option<CBall> {
    airy(Airy::Ai, x, p)
}

/// Bi(x) to p bits.
pub fn airy_bi(x: &Real, p: u64) -> Option<CBall> {
    airy(Airy::Bi, x, p)
}

/// Ai'(x) to p bits.
pub fn airy_ai_prime(x: &Real, p: u64) -> Option<CBall> {
    airy(Airy::AiP, x, p)
}

/// Bi'(x) to p bits.
pub fn airy_bi_prime(x: &Real, p: u64) -> Option<CBall> {
    airy(Airy::BiP, x, p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_arith::num::Rat;

    fn digits(b: &Ball, d: usize) -> String {
        let (s, e) = b.mid.to_sci(d);
        format!("{}{s}e{e}", if b.mid.neg { "-" } else { "" })
    }

    fn q(n: i64, d: i64) -> Real {
        Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)))
    }

    #[test]
    fn published_values() {
        // J_0(1) = 0.76519768655796655145...
        let v = bessel_j(&q(0, 1), &q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 20), "76519768655796655145e-1");
        // Y_0(1) = 0.08825696421567695798...
        let v = bessel_y(&q(0, 1), &q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 19), "8825696421567695798e-2");
        // J_1(10) = 0.04347274616886143667...
        let v = bessel_j(&q(1, 1), &q(10, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 19), "4347274616886143667e-2");
        // Ai(0), Bi(0), Ai(1)
        let v = airy_ai(&q(0, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 20), "35502805388781723926e-1");
        let v = airy_bi(&q(0, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 20), "61492662744600073515e-1");
        let v = airy_ai(&q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 20), "13529241631288141552e-1");
        // J_1/2(x) = sqrt(2/(pi x)) sin x, Y_1/2(x) = -sqrt(2/(pi x)) cos x at x = 3/2:
        // 0.6492904008... and -0.04621411...
        let j = bessel_j(&q(1, 2), &q(3, 2), 200).unwrap().re;
        let y = bessel_y(&q(1, 2), &q(3, 2), 200).unwrap().re;
        let w = (2.0 / (std::f64::consts::PI * 1.5)).sqrt();
        assert!((f64_of(&j.mid) - w * 1.5f64.sin()).abs() < 1e-15);
        assert!((f64_of(&y.mid) + w * 1.5f64.cos()).abs() < 1e-15);
    }

    #[test]
    fn zero_of_j0() {
        // j_0,1 = 2.404825557695772768621631879326454643124...: |J_0| there is below 2^-130
        let x = Real::Exact(Rat::new(
            Int::from_i64(2404825557695772768),
            Int::from_i64(1_000_000_000_000_000_000),
        ));
        let v = bessel_j(&q(0, 1), &x, 200).unwrap().re;
        // J_0(x) = 3.2271863438767675741787720592954603519748794751412...e-19 (about J_1(j) (j - x))
        assert_eq!(digits(&v, 25), "3227186343876767574178772e-19");
    }

    #[test]
    fn identities_at_50_digits() {
        let p = 180;
        let close = |a: &Ball, b: &Ball, scale: f64| {
            let d = f64_of(&a.sub(b, 400).mid).abs();
            assert!(d <= scale * 1e-50, "{} vs {}", digits(a, 30), digits(b, 30));
        };
        for (nu, x) in [((1, 3), (7, 1)), ((5, 2), (3, 10)), ((-1, 3), (25, 2)), ((2, 1), (40, 1)), ((7, 1), (5, 1))] {
            let nu_b = |k: i64| q(nu.0 + k * nu.1, nu.1);
            let xr = q(x.0, x.1);
            let xf = x.0 as f64 / x.1 as f64;
            let j = |k| bessel_j(&nu_b(k), &xr, p).unwrap().re;
            let y = |k| bessel_y(&nu_b(k), &xr, p).unwrap().re;
            // J_{nu-1} + J_{nu+1} = 2 nu/x J_nu
            let nuf = nu.0 as f64 / nu.1 as f64;
            let lhs = j(-1).add(&j(1), 400);
            let two_nu_x = Ball::from_ratio(2 * nu.0 * x.1, nu.1 * x.0, 400);
            close(&lhs, &two_nu_x.mul(&j(0), 400), 1.0 + (2.0 * nuf / xf).abs());
            let lhs = y(-1).add(&y(1), 400);
            let scale = 1.0 + f64_of(&lhs.mid).abs();
            close(&lhs, &two_nu_x.mul(&y(0), 400), scale);
            // J_{nu+1} Y_nu - J_nu Y_{nu+1} = 2/(pi x)
            let w = j(1).mul(&y(0), 400).sub(&j(0).mul(&y(1), 400), 400);
            let want = int(2).div(&pi(400).unwrap().mul(&rat(x), 400), 400).unwrap();
            close(&w, &want, 1.0 + f64_of(&j(0).mul(&y(1), 400).mid).abs());
        }
        // Ai Bi' - Ai' Bi = 1/pi
        for x in [(-31, 1), (-5, 2), (0, 1), (1, 3), (9, 1), (30, 1)] {
            let xr = q(x.0, x.1);
            let ai = airy_ai(&xr, p).unwrap().re;
            let bi = airy_bi(&xr, p).unwrap().re;
            let aip = airy_ai_prime(&xr, p).unwrap().re;
            let bip = airy_bi_prime(&xr, p).unwrap().re;
            let w = ai.mul(&bip, 400).sub(&aip.mul(&bi, 400), 400);
            let want = int(1).div(&pi(400).unwrap(), 400).unwrap();
            close(&w, &want, 1.0 + f64_of(&aip.mul(&bi, 400).mid).abs());
        }
    }

    fn rat((n, d): (i64, i64)) -> Ball {
        Ball::from_ratio(n, d, 400)
    }

    #[test]
    fn hankel_against_series() {
        // the two methods agree where both apply (J_1/3(60), Ai(-30) at 100 bits)
        let x = Ball::from_i64(60);
        let nu = Ball::from_ratio(1, 3, 200);
        let (j, y) = hankel_jy(&nu, &x, 100).unwrap();
        let js = j_series(&nu, None, &x, 300).unwrap();
        assert!(j.sub(&js, 200).contains_zero());
        let ys = y_point(&nu, None, &x, 100).unwrap();
        assert!(y.sub(&ys, 200).contains_zero());
        let xa = Ball::from_i64(-30);
        for kind in [Airy::Ai, Airy::Bi, Airy::AiP, Airy::BiP] {
            let h = airy_hankel(kind, &xa, 100).unwrap();
            let w = 200 + 140;
            let (c1, c2) = airy_constants(w).unwrap();
            let (f, g) = airy_fg(&xa, kind.deriv(), w).unwrap();
            let (cf, cg) = (c1.mul(&f, w), c2.mul(&g, w));
            let s = match kind {
                Airy::Ai | Airy::AiP => cf.sub(&cg, w),
                _ => el::sqrt(&int(3), w).unwrap().mul(&cf.add(&cg, w), w),
            };
            assert!(h.sub(&s, w).contains_zero(), "{kind:?}");
            assert!(h.rel_bits() > 90.0, "{kind:?}");
        }
        // the K expansion against the series: Ai(12), Ai'(12)
        let x = Ball::from_i64(12);
        for kind in [Airy::Ai, Airy::AiP] {
            let a = airy_k_asymptotic(kind, &x, 60).unwrap();
            let w = 200 + 120;
            let (c1, c2) = airy_constants(w).unwrap();
            let (f, g) = airy_fg(&x, kind.deriv(), w).unwrap();
            let s = c1.mul(&f, w).sub(&c2.mul(&g, w), w);
            assert!(a.sub(&s, w).contains_zero(), "{kind:?}");
            assert!(a.rel_bits() > 55.0, "{kind:?}");
        }
    }

    #[test]
    fn large_orders() {
        // Hankel's terms grow far beyond 1 for nu = 1000 at x = 500 and 1100: the series take over
        // J_1000(500) = 1.9704922060099743070...e-198, J_1000(1100) = -0.032631556608876544188...
        let v = bessel_j(&q(1000, 1), &q(500, 1), 64).unwrap().re;
        assert!(v.rel_bits() > 60.0 && (f64_of(&v.mid) / 1.970_492_206_009_974_3e-198 - 1.0).abs() < 1e-15);
        let v = bessel_j(&q(1000, 1), &q(1100, 1), 64).unwrap().re;
        assert!(v.rel_bits() > 60.0 && (f64_of(&v.mid) / -0.032_631_556_608_876_544 - 1.0).abs() < 1e-15);
    }

    #[test]
    fn domains() {
        assert!(bessel_y(&q(0, 1), &q(0, 1), 64).is_none());
        assert!(bessel_y(&q(1, 1), &q(-1, 1), 64).is_none());
        assert!(bessel_j(&q(1, 3), &q(-1, 1), 64).is_none());
        // J_n(-x) = (-1)^n J_n(x)
        let a = bessel_j(&q(3, 1), &q(-5, 2), 100).unwrap().re;
        let b = bessel_j(&q(3, 1), &q(5, 2), 100).unwrap().re;
        assert!(a.add(&b, 100).contains_zero() && !a.contains_zero());
        // J_-n = (-1)^n J_n, Y_-n = (-1)^n Y_n
        let a = bessel_y(&q(-3, 1), &q(5, 2), 100).unwrap().re;
        let b = bessel_y(&q(3, 1), &q(5, 2), 100).unwrap().re;
        assert!(a.add(&b, 100).contains_zero());
        assert_eq!(f64_of(&bessel_j(&q(0, 1), &q(0, 1), 64).unwrap().re.mid), 1.0);
        assert!(bessel_j(&q(1, 2), &q(0, 1), 64).unwrap().re.mid.is_zero());
        assert!(bessel_j(&Real::Machine(1.0), &q(1, 1), 64).is_none());
    }
}
