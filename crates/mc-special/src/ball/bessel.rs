//! Modified Bessel functions I_nu(x) and K_nu(x) on balls, real nu and x.
//!
//! - **I_nu:** the power series sum_k (x/2)^(2k+nu) / (k! Gamma(k + nu + 1)). Once the term ratio
//!   r_k = (x^2/4) / ((k + 1)(k + 1 + nu)) is at most 1/2 (it decreases from there on), the tail after a term t is at
//!   most 2|t|. Exact integer orders allow any real x (I_n(-x) = (-1)^n I_n(x)); other orders need x > 0.
//! - **K_n, integer n:** the series with digamma coefficients (Abramowitz and Stegun 9.6.11)
//!   K_n(x) = (1/2)(x/2)^-n sum_{k<n} (n-k-1)!/k! (-x^2/4)^k + (-1)^(n+1) ln(x/2) I_n(x) +
//!   (-1)^n (1/2)(x/2)^n sum_k (psi(k+1) + psi(n+k+1)) (x^2/4)^k / (k! (n+k)!), psi(k+1) = H_k - gamma; the
//!   coefficient ratio is at most 1 + 2.4/(k+1) from k = 1 on, which bounds the tail as for I.
//! - **K_nu, other nu:** pi/2 (I_-nu(x) - I_nu(x)) / sin(nu pi).
//! - **Large x:** the asymptotic expansion K_nu(x) = sqrt(pi/(2x)) e^-x (sum_{k<K} a_k(nu) / x^k + R_K),
//!   a_k = prod_{j=1}^{k} (4 nu^2 - (2j-1)^2) / (k! 8^k); for real nu, x > 0 and K >= |nu| - 1/2 the remainder is
//!   bounded by the first omitted term (Watson 7.3, DLMF 10.40.10). It is used when its smallest term reaches
//!   2^-p; otherwise the series run with about 2.9 x extra bits for the cancellation (I and K differ by e^2x).
//! - **Ball arguments** (a nonzero radius): K_nu(x) decreases in x > 0 and increases in |nu|, so its range over the
//!   box is [K(min |nu|, x_hi), K(max |nu|, x_lo)], evaluated at those exact corners (decision SF.8).
use super::gamma::gamma_ball;
use super::{euler_gamma, hull, int, is_pos, log2_up, lower, mag_up, pi, real, refine, upper, widen};
use mc_arith::num::{stopped, Ball, Float, Int, Nat};
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

/// sum_k t_k with t_{k+1} = t_k q / ((k+1)(k+1+nu)), from t_0; `nu_lo` a lower bound of nu.
fn i_sum(t0: Ball, q: &Ball, nu: &Ball, nu_lo: f64, wp: u64) -> Option<Ball> {
    let q_hi = super::f64_of(&upper(q));
    let mut t = t0;
    let mut s = t.clone();
    let mut big = log2_up(&t);
    for k in 0..MAX_TERMS {
        if k % 32 == 0 && stopped() {
            return None;
        }
        let d = nu.add(&int(k as i64 + 1), wp).mul(&int(k as i64 + 1), wp);
        t = t.mul(q, wp).div(&d, wp)?;
        big = big.max(log2_up(&s));
        let next = (k as f64 + 2.0) * (k as f64 + 2.0 + nu_lo);
        if next > 0.0 && q_hi / next <= 0.5 && log2_up(&t) < big - wp as f64 - 4.0 {
            return Some(widen(s, &mag_up(&t).mul_2exp(1)));
        }
        s = s.add(&t, wp);
    }
    None
}

/// I_nu(x): `n` the exact integer value of nu if it has one.
pub fn i_ball(nu: &Ball, n: Option<i64>, x: &Ball, wp: u64) -> Option<Ball> {
    // the series needs about |x| terms at about |x| extra bits: beyond MAX_SERIES_X it would run for hours or
    // exhaust memory, so the value is not given
    let xa = super::f64_of(&upper(x)).abs().max(super::f64_of(&lower(x)).abs());
    if xa.is_nan() || xa > MAX_SERIES_X {
        return None;
    }
    let q = x.mul(x, wp).mul(&Ball::from_ratio(1, 4, wp), wp);
    let half = x.mul(&Ball::from_ratio(1, 2, wp), wp);
    if let Some(n) = n {
        let m = n.unsigned_abs();
        let t0 = el::pow_int(&half, &Int::from_u64(m), wp)?.div(&Ball::from_nat(nat_fact(m)), wp)?;
        return i_sum(t0, &q, &int(m as i64), m as f64, wp);
    }
    if !is_pos(x) {
        return None;
    }
    let pw = el::exp(&nu.mul(&el::log(&half, wp)?, wp), wp)?;
    let t0 = pw.div(&gamma_ball(&nu.add(&int(1), wp), wp)?, wp)?;
    i_sum(t0, &q, nu, super::f64_of(&lower(nu)), wp)
}

/// K_n(x) for an integer n >= 0 and x > 0 by the digamma series.
fn k_int_series(n: u64, x: &Ball, wp: u64) -> Option<Ball> {
    let half = x.mul(&Ball::from_ratio(1, 2, wp), wp);
    let q = half.mul(&half, wp);
    let hn = el::pow_int(&half, &Int::from_u64(n), wp)?;
    // the finite part
    let mut a = int(0);
    if n > 0 {
        let mut t = Ball::from_nat(nat_fact(n - 1)); // (n-1)!/0!
        let mq = q.neg();
        for k in 0..n {
            a = a.add(&t, wp);
            if k + 1 < n {
                // (n-k-2)!/(k+1)! = (n-k-1)!/k! / ((n-k-1)(k+1))
                t = t.mul(&mq, wp).div(&int(((n - k - 1) * (k + 1)) as i64), wp)?;
            }
        }
        a = a.div(&hn, wp)?.mul(&Ball::from_ratio(1, 2, wp), wp);
    }
    // the logarithmic part
    let i = i_ball(&int(n as i64), Some(n as i64), x, wp)?;
    let b = el::log(&half, wp)?.mul(&i, wp);
    let b = if n.is_multiple_of(2) { b.neg() } else { b };
    // the digamma series: u_k = q^k / (k! (n+k)!), c_k = H_k + H_{n+k} - 2 gamma
    let g2 = euler_gamma(wp)?.mul(&int(2), wp);
    let mut hk = int(0);
    let mut hnk = int(0);
    for j in 1..=n {
        hnk = hnk.add(&int(1).div(&int(j as i64), wp)?, wp);
    }
    let mut u = int(1).div(&Ball::from_nat(nat_fact(n)), wp)?;
    let mut s = hk.add(&hnk, wp).sub(&g2, wp).mul(&u, wp);
    let q_hi = super::f64_of(&upper(&q));
    let mut big = log2_up(&s);
    let mut done = None;
    for k in 0..MAX_TERMS {
        if k % 32 == 0 && stopped() {
            return None;
        }
        let k1 = k + 1;
        u = u.mul(&q, wp).div(&int((k1 * (n + k1)) as i64), wp)?;
        hk = hk.add(&int(1).div(&int(k1 as i64), wp)?, wp);
        hnk = hnk.add(&int(1).div(&int((n + k1) as i64), wp)?, wp);
        let t = hk.add(&hnk, wp).sub(&g2, wp).mul(&u, wp);
        big = big.max(log2_up(&s));
        let next = (k1 as f64 + 1.0) * ((n + k1) as f64 + 1.0);
        let rho = (1.0 + 2.4 / (k1 as f64 + 1.0)) * q_hi / next;
        if k1 >= 1 && rho <= 0.5 && log2_up(&t) < big - wp as f64 - 4.0 {
            done = Some(widen(s, &mag_up(&t).mul_2exp(1)));
            break;
        }
        s = s.add(&t, wp);
    }
    let c = done?.mul(&hn, wp).mul(&Ball::from_ratio(1, 2, wp), wp);
    let c = if n % 2 == 1 { c.neg() } else { c };
    Some(a.add(&b, wp).add(&c, wp))
}

/// K_nu(x) for a non-integer nu by the reflection of I.
fn k_frac_series(nu: &Ball, x: &Ball, wp: u64) -> Option<Ball> {
    let ip = i_ball(nu, None, x, wp)?;
    let im = i_ball(&nu.neg(), None, x, wp)?;
    let s = super::sin_pi(nu, wp)?;
    let h = pi(wp)?.mul(&Ball::from_ratio(1, 2, wp), wp);
    h.mul(&im.sub(&ip, wp), wp).div(&s, wp)
}

/// The asymptotic expansion, if its terms reach 2^-wp relative (nu >= 0, x > 0).
fn k_asymptotic(nu: &Ball, x: &Ball, wp: u64) -> Option<Ball> {
    let nu_hi = super::f64_of(&upper(nu));
    let kmin = (nu_hi - 0.5).ceil().max(0.0) as u64;
    let mu = nu.mul(nu, wp).mul(&int(4), wp);
    let r = int(1).div(x, wp)?;
    let mut t = int(1);
    let mut s = int(0);
    let mut prev = f64::INFINITY;
    let x_lo = super::f64_of(&lower(x));
    for k in 0..(4.0 * x_lo + 2.0 * nu_hi + 16.0) as u64 {
        if k % 32 == 0 && stopped() {
            return None;
        }
        let lt = log2_up(&t);
        if k >= kmin && lt < -(wp as f64) - 4.0 {
            let s = widen(s, &mag_up(&t));
            let f = pi(wp)?.div(&x.mul(&int(2), wp), wp)?;
            let pre = el::sqrt(&f, wp)?.mul(&el::exp(&x.neg(), wp)?, wp);
            return Some(pre.mul(&s, wp));
        }
        if k > kmin && lt > prev {
            return None;
        }
        prev = lt;
        s = s.add(&t, wp);
        let j = 2 * k as i64 + 1;
        t = t.mul(&mu.sub(&int(j * j), wp), wp).mul(&r, wp).div(&int(8 * (k as i64 + 1)), wp)?;
    }
    None
}

/// The largest |x| the power series of I and K are summed at (about 3·10^5 extra bits and 10^5 terms).
const MAX_SERIES_X: f64 = 1e5;

/// K_nu(x) at a narrow point: nu >= 0 (with its integer value if exact), x > 0.
fn k_point(nu: &Ball, n: Option<i64>, x: &Ball, wp: u64) -> Option<Ball> {
    if !is_pos(x) {
        return None;
    }
    let xf = super::f64_of(&upper(x));
    if xf > 2.0 {
        if let Some(k) = k_asymptotic(nu, x, wp + 8) {
            return Some(k);
        }
    }
    // the series cancel by about e^2x
    if xf > MAX_SERIES_X {
        // the asymptotic expansion did not apply: the series would need about 3x extra bits (gigabytes for x = 2^31)
        return None;
    }
    let extra = (2.9 * xf).ceil() as u64 + 16;
    let w = wp + extra;
    match n {
        Some(n) => k_int_series(n.unsigned_abs(), x, w),
        None => {
            let (_, f) = super::split_int(nu, w)?;
            let near = -log2_up(&f).min(0.0) as u64;
            k_frac_series(nu, x, w + near)
        }
    }
}

/// The integer value of an exact real, or of an exact ball.
fn int_of(r: &Real) -> Option<i64> {
    super::exact_int(r)?.to_i64()
}

/// |nu| as a ball.
fn abs_ball(b: Ball) -> Ball {
    if b.mid.neg {
        b.neg()
    } else {
        b
    }
}

/// K_nu(x) at an exact point (dyadic or rational) to p bits.
fn k_exact(nu: &Real, x: &Real, p: u64) -> Option<Ball> {
    let n = int_of(nu);
    let r = refine(p, 24, |wp| {
        let v = abs_ball(super::real_ball(nu, wp)?);
        Some(real(k_point(&v, n, &super::real_ball(x, wp)?, wp)?))
    })?;
    Some(r.re)
}

fn is_wide(r: &Real) -> bool {
    matches!(r, Real::Ball(b, _) if !b.rad.is_zero())
}

/// K_nu(x) to p bits (x > 0).
pub fn bessel_k(nu: &Real, x: &Real, p: u64) -> Option<CBall> {
    if !is_wide(nu) && !is_wide(x) {
        return Some(real(k_exact(nu, x, p)?));
    }
    // the monotone corners (SF.8)
    let nb = super::real_ball(nu, p)?;
    let xb = super::real_ball(x, p)?;
    let (x_lo, x_hi) = (lower(&xb), upper(&xb));
    if x_lo.neg || x_lo.is_zero() {
        return None;
    }
    let (a, b) = (lower(&nb).abs(), upper(&nb).abs());
    let (nu_min, nu_max) = if nb.contains_zero() {
        (Float::zero(), if a.cmp_exact(&b).is_gt() { a } else { b })
    } else if a.cmp_exact(&b).is_lt() {
        (a, b)
    } else {
        (b, a)
    };
    let pt = |f: Float| Real::Ball(Ball::exact(f), p);
    let lo = k_exact(&pt(nu_min), &pt(x_hi), p)?;
    let hi = k_exact(&pt(nu_max), &pt(x_lo), p)?;
    Some(real(hull(&lower(&lo), &upper(&hi), p)))
}

/// I_nu(x) to p bits.
pub fn bessel_i(nu: &Real, x: &Real, p: u64) -> Option<CBall> {
    let n = int_of(nu);
    refine(p, 24, |wp| {
        let v = super::real_ball(nu, wp)?;
        let xw = super::real_ball(x, wp)?;
        // I_n(-x) = (-1)^n I_n(x): the series in x^2 handles negative x for integer orders directly
        Some(real(i_ball(&v, n, &xw, wp)?))
    })
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
        // K_0(1) = 0.42102443824070833333562737921260903613621974822666...
        let k = bessel_k(&q(0, 1), &q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&k, 40), "4210244382407083333356273792126090361362e-1");
        // K_1(2) = 0.13986588181652242728459880703541102...
        let k = bessel_k(&q(1, 1), &q(2, 1), 200).unwrap().re;
        assert_eq!(digits(&k, 30), "139865881816522427284598807035e-1");
        // I_0(1) = 1.2660658777520083355982446252147175376076703113550...
        let i = bessel_i(&q(0, 1), &q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&i, 40), "1266065877752008335598244625214717537608e0");
        // K_1/2(x) = sqrt(pi/(2x)) e^-x
        let k = bessel_k(&q(1, 2), &q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&k, 30), "461068504447894558439575873876e-1");
        // large x by the asymptotic series: K_0(100) = 4.6566282291759032e-45
        let k = bessel_k(&q(0, 1), &q(100, 1), 100).unwrap().re;
        assert_eq!(digits(&k, 15), "465662822917590e-45");
        assert!(bessel_k(&q(1, 1), &q(0, 1), 64).is_none());
    }
}
