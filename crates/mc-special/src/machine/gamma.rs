//! gamma, log_gamma, digamma, polygamma and beta for machine reals.
//!
//! - **Gamma(x)**, |x| <= 20: x = n + z with |z| <= 1/2 and Gamma(x) = Gamma(1 + z) (z+1)...(z+n-1) (or divided
//!   by z (z-1)...(z+n) for n <= 0), with 1/Gamma(1 + z) from its Taylor series; every factor z + k is exact in
//!   double-double, so the relative accuracy near the poles is kept. x >= 20: exp of Stirling's series in
//!   double-double; x < -200: the reflection formula.
//! - **ln Gamma**: near its zeros 1 and 2 through ln Gamma(1 + z) = -log1p(z P(z)) with 1/Gamma(1 + z) =
//!   1 + z P(z) (no cancellation), Stirling's series for x >= 20, else the logarithm of the double-double gamma.
//! - **psi**: the upward shift to y >= 12 and the asymptotic series; the reflection for x < 0.
//! - **psi^(n)**: the shift to y >= n/2 + 20, the asymptotic series with the rising factorials (n)_2k, and the
//!   recurrence sum, all as scaled double-doubles (they overflow for large n).
//! - **B(a, b)**: exp(ln Gamma(a) + ln Gamma(b) - ln Gamma(a + b)) for a, b > 0 (a + b exact in double-double),
//!   else the quotient of gammas.
use super::rgamma_coefficients;
use crate::bernoulli::dd_table;
use crate::dd::{exp_scaled, log, sin_cos_pi_any, Scaled, DD, HALF_LN_2PI, LN_2PI, LN_PI, PI};
use mc_arith::num::{Int, Rat};
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::OnceLock;

/// 1/Gamma(1 + z) for |z| <= 1/2.
fn rgamma1(z: DD) -> DD {
    let g = rgamma_coefficients();
    let mut s = DD::ZERO;
    for c in g.iter().rev() {
        s = s.mul(z).add(*c);
    }
    s
}

/// P(z) = (1/Gamma(1 + z) - 1) / z.
fn rgamma1_p(z: DD) -> DD {
    let g = rgamma_coefficients();
    let mut s = DD::ZERO;
    for c in g[1..].iter().rev() {
        s = s.mul(z).add(*c);
    }
    s
}

/// B_2k / (2k (2k - 1)), the Stirling coefficients.
fn stirling_coefficients() -> &'static [DD] {
    static T: OnceLock<Vec<DD>> = OnceLock::new();
    dd_table(&T, 24, |k, b| b.mul(&Rat::new(Int::from_i64(1), Int::from_u64(2 * k * (2 * k - 1)))))
}

/// B_2k / (2k), the digamma coefficients.
fn digamma_coefficients() -> &'static [DD] {
    static T: OnceLock<Vec<DD>> = OnceLock::new();
    dd_table(&T, 24, |k, b| b.mul(&Rat::new(Int::from_i64(1), Int::from_u64(2 * k))))
}

/// B_2k / (2k)!, the Euler-Maclaurin and polygamma coefficients.
pub(crate) fn em_coefficients() -> &'static [DD] {
    static T: OnceLock<Vec<DD>> = OnceLock::new();
    dd_table(&T, 48, |k, b| {
        let mut f = mc_arith::num::Nat::from_u64(1);
        for i in 2..=2 * k {
            f = f.mul_u64(i);
        }
        b.mul(&Rat::new(Int::from_i64(1), Int::from_nat(f)))
    })
}

/// ln Gamma(x) by Stirling's series, x >= 10.
pub(crate) fn lgamma_stirling(x: DD) -> DD {
    let lead = x.add_f(-0.5).mul(log(x)).sub(x).add(HALF_LN_2PI);
    let r = x.recip();
    let r2 = r.sqr();
    let mut pw = r;
    let mut s = DD::ZERO;
    let tiny = lead.hi.abs() * 1e-34;
    for c in stirling_coefficients() {
        let t = c.mul(pw);
        s = s.add(t);
        if t.hi.abs() < tiny {
            break;
        }
        pw = pw.mul(r2);
    }
    lead.add(s)
}

/// sin(pi x) for a double-double x.
fn sin_pi(x: DD) -> DD {
    sin_cos_pi_any(x).0
}

/// Gamma(x) as a scaled double-double, for x not a pole.
pub fn gamma_scaled(x: DD) -> Scaled {
    if x.hi >= 20.0 {
        return exp_scaled(lgamma_stirling(x));
    }
    if x.hi < -200.0 {
        // reflection: pi / (sin(pi x) Gamma(1 - x))
        let one_minus = DD::new(1.0).sub(x);
        let d = Scaled::new(sin_pi(x)).mul(exp_scaled(lgamma_stirling(one_minus)));
        return Scaled::new(PI).div(d);
    }
    let n = x.hi.round();
    let z = DD::sum(x.hi - n, x.lo);
    let r = rgamma1(z);
    if n >= 1.0 {
        let mut p = Scaled::new(DD::ONE);
        let mut k = 1.0;
        while k < n {
            p = p.mul_dd(z.add_f(k));
            k += 1.0;
        }
        return p.div(Scaled::new(r));
    }
    // n <= 0: Gamma(x) = Gamma(1 + z) / (z (z - 1) ... (z + n))
    let mut p = Scaled::new(r);
    let mut k = 0.0;
    while k >= n {
        p = p.mul_dd(z.add_f(k));
        k -= 1.0;
    }
    Scaled::new(DD::ONE).div(p)
}

/// Gamma(x) for a machine real.
pub fn gamma(x: f64) -> f64 {
    if x.is_nan() || x == f64::NEG_INFINITY {
        return f64::NAN;
    }
    if x == 0.0 {
        return 1.0 / x;
    }
    if x < 0.0 && x == x.floor() {
        return f64::NAN;
    }
    if x > 171.7 {
        return f64::INFINITY;
    }
    gamma_scaled(DD::new(x)).to_f64()
}

/// log1p(t) for a double-double t > -1, accurate relative to t.
fn log1p(t: DD) -> DD {
    if t.hi.abs() < 1e-3 {
        // t - t^2/2 + t^3/3 - ...: |t|^13 < 1e-39
        let mut s = DD::ZERO;
        let mut pw = t;
        for k in 1..=13 {
            let term = pw.div_f(k as f64);
            s = if k % 2 == 1 { s.add(term) } else { s.sub(term) };
            pw = pw.mul(t);
        }
        return s;
    }
    log(t.add_f(1.0))
}

/// ln |Gamma(x)| as a double-double, for x not a pole.
pub fn lgamma_re(x: DD) -> DD {
    if x.hi >= 20.0 {
        return lgamma_stirling(x);
    }
    if x.hi < -200.0 {
        let one_minus = DD::new(1.0).sub(x);
        return LN_PI.sub(log(sin_pi(x).abs())).sub(lgamma_stirling(one_minus));
    }
    if (0.5..1.5).contains(&x.hi) {
        // ln Gamma(1 + z) = -log1p(z P(z))
        let z = x.add_f(-1.0);
        return log1p(z.mul(rgamma1_p(z))).neg();
    }
    if (1.5..2.5).contains(&x.hi) {
        // ln Gamma(2 + z) = log((1 + z) / R(z)) = log1p(z (1 - P(z)) / R(z))
        let z = x.add_f(-2.0);
        let u = z.mul(DD::ONE.sub(rgamma1_p(z))).div(rgamma1(z));
        return log1p(u);
    }
    gamma_scaled(x).ln_abs()
}

/// ln Gamma(x) for a machine real: (real part, imaginary part); the imaginary part is -pi ceil(-x) for negative
/// non-integers (the principal value, decision SF.6) and 0 otherwise. Poles give +inf.
pub fn log_gamma(x: f64) -> (f64, f64) {
    if x.is_nan() || x == f64::NEG_INFINITY {
        return (f64::NAN, 0.0);
    }
    if x == f64::INFINITY {
        return (x, 0.0);
    }
    if x <= 0.0 && x == x.floor() {
        return (f64::INFINITY, 0.0);
    }
    let re = lgamma_re(DD::new(x)).to_f64();
    if x > 0.0 {
        return (re, 0.0);
    }
    let k = (-x).ceil();
    (re, -PI.mul_f(k).to_f64())
}

/// Entry j: B_(2j + 2) / (4 j (j + 1)), the coefficients of the asymptotic series of ln G (DLMF 5.17.5); entry 0
/// is unused.
fn barnes_coefficients() -> &'static [DD] {
    static T: OnceLock<Vec<DD>> = OnceLock::new();
    dd_table(&T, 26, |k, b| if k < 2 { Rat::new(Int::from_i64(0), Int::from_i64(1)) } else { b.mul(&Rat::new(Int::from_i64(1), Int::from_u64(4 * (k - 1) * k))) })
}

/// ln G(v + 1) - zeta'(-1) for v >= 20: v^2/2 ln v - 3 v^2/4 + v/2 ln(2 pi) - ln(v)/12 + sum_k B_(2k+2)/(4 k (k + 1)
/// v^(2k)) (DLMF 5.17.5); at v = 20 the terms fall below 10^-34 of the sum before the series turns.
fn ln_barnes_asymptotic(v: DD) -> DD {
    let lv = log(v);
    let v2 = v.sqr();
    let lead = v2.mul(lv).mul_f(0.5).sub(v2.mul_f(0.75)).add(v.mul(LN_2PI).mul_f(0.5)).sub(lv.div_f(12.0));
    let r2 = v2.recip();
    let mut pw = r2;
    let mut s = DD::ZERO;
    let tiny = lead.hi.abs() * 1e-34;
    for c in &barnes_coefficients()[1..] {
        let t = c.mul(pw);
        s = s.add(t);
        if t.hi.abs() < tiny {
            break;
        }
        pw = pw.mul(r2);
    }
    lead.add(s)
}

/// zeta'(-1) from G(21) = 1! 2! ... 19!, that is ln G(21) = sum_(j = 2..19) (20 - j) ln j, and the series at v = 20.
fn zeta_prime_m1_dd() -> DD {
    static C: OnceLock<DD> = OnceLock::new();
    *C.get_or_init(|| {
        let mut l = DD::ZERO;
        for j in 2..=19u32 {
            l = l.add(log(DD::new(j as f64)).mul_f((20 - j) as f64));
        }
        l.sub(ln_barnes_asymptotic(DD::new(20.0)))
    })
}

/// The Barnes G-function of a machine real x (not an integer <= 0): G(x) = G(x + m) / prod_(j < m) Gamma(x + j)
/// (from G(z + 1) = Gamma(z) G(z)) with x + m >= 21, G(x + m) the exponential of the asymptotic series, the gammas
/// built up by Gamma(x + j + 1) = (x + j) Gamma(x + j) from one double-double gamma (each x + j exact). `None` when
/// the value is not a finite nonzero double (an overflow for x beyond about 30) or the shift would be long.
pub fn barnes_g(x: f64) -> Option<f64> {
    if !x.is_finite() || (x <= 0.0 && x == x.floor()) {
        return None;
    }
    let m = (21.0 - x).max(0.0).ceil();
    if m > 400.0 {
        return None;
    }
    let xd = DD::new(x);
    let lg = ln_barnes_asymptotic(xd.add_f(m - 1.0)).add(zeta_prime_m1_dd());
    let mut v = exp_scaled(lg);
    if m > 0.0 {
        let mut g = gamma_scaled(xd);
        let mut p = g;
        for j in 1..(m as i64) {
            g = g.mul_dd(xd.add_f((j - 1) as f64));
            p = p.mul(g);
        }
        v = v.div(p);
    }
    let r = v.to_f64();
    (r.is_finite() && r != 0.0).then_some(r)
}

/// psi(x) as a double-double, x not a pole.
pub fn digamma_dd(x: DD) -> DD {
    if x.hi < 0.0 {
        // psi(x) = psi(1 - x) - pi cot(pi x)
        let (s, c) = sin_cos_pi_any(x);
        return digamma_dd(DD::new(1.0).sub(x)).sub(PI.mul(c).div(s));
    }
    let mut x = x;
    let mut acc = DD::ZERO;
    while x.hi < 12.0 {
        acc = acc.sub(x.recip());
        x = x.add_f(1.0);
    }
    let r = x.recip();
    let r2 = r.sqr();
    let mut s = log(x).sub(r.mul_f(0.5));
    let mut pw = r2;
    for c in digamma_coefficients() {
        let t = c.mul(pw);
        s = s.sub(t);
        if t.hi.abs() < 1e-34 {
            break;
        }
        pw = pw.mul(r2);
    }
    s.add(acc)
}

/// psi(x) for a machine real.
pub fn digamma(x: f64) -> f64 {
    if x.is_nan() || x == f64::NEG_INFINITY {
        return f64::NAN;
    }
    if x == f64::INFINITY {
        return x;
    }
    if x == 0.0 {
        return -1.0 / x;
    }
    if x < 0.0 && x == x.floor() {
        return f64::NAN;
    }
    digamma_dd(DD::new(x)).to_f64()
}

/// |t|^-(n+1) with the sign of t^-(n+1), scaled.
fn inv_pow(t: DD, n: u64) -> Scaled {
    let m = exp_scaled(log(t.abs()).mul_f(-((n + 1) as f64)));
    if t.hi < 0.0 && n.is_multiple_of(2) {
        m.neg()
    } else {
        m
    }
}

/// The largest recurrence length of polygamma for negative arguments.
const MAX_SHIFT: f64 = 2e6;

/// psi^(n)(x) for a machine real and an integer order n >= 0 (NaN for other orders).
pub fn polygamma(n: f64, x: f64) -> f64 {
    if !(n >= 0.0 && n == n.floor() && n <= 1e5) || x.is_nan() || x == f64::NEG_INFINITY {
        return f64::NAN;
    }
    if n == 0.0 {
        return digamma(x);
    }
    let odd = n as u64 % 2 == 1;
    let sign = if odd { 1.0 } else { -1.0 };
    if x == f64::INFINITY {
        return 0.0 * sign;
    }
    if x <= 0.0 && x == x.floor() {
        // the pole term n! / (x - m)^(n+1): one-sided for odd n
        return if odd {
            f64::INFINITY
        } else if x == 0.0 {
            -sign * f64::INFINITY * if x.is_sign_negative() { -1.0 } else { 1.0 }
        } else {
            f64::NAN
        };
    }
    let n = n as u64;
    let y0 = 0.5 * n as f64 + 20.0;
    let steps = (y0 - x).ceil().max(0.0);
    if steps > MAX_SHIFT {
        return f64::NAN;
    }
    // n! sum_j (x + j)^-(n+1)
    let mut sum = Scaled::new(DD::ZERO);
    let mut j = 0.0;
    while j < steps {
        sum = sum.add(inv_pow(DD::sum(x, j), n));
        j += 1.0;
    }
    let y = DD::sum(x, steps);
    let mut fact = Scaled::new(DD::ONE); // (n-1)!
    for k in 2..n {
        fact = fact.mul_dd(DD::new(k as f64));
    }
    let nfact = fact.mul_dd(DD::new(n as f64));
    // the asymptotic bracket 1 + n/(2y) + sum_k B_2k/(2k)! (n)_2k / y^2k
    let r = y.recip();
    let r2 = r.sqr();
    let mut br = DD::ONE.add(r.mul_f(0.5 * n as f64));
    let mut f = DD::ONE;
    for (k, c) in em_coefficients().iter().enumerate() {
        let k = k as f64 + 1.0;
        f = f.mul_f(n as f64 + 2.0 * k - 2.0).mul_f(n as f64 + 2.0 * k - 1.0).mul(r2);
        let t = c.mul(f);
        br = br.add(t);
        if t.hi.abs() < 1e-34 {
            break;
        }
    }
    let lead = fact.mul(exp_scaled(log(y).mul_f(-(n as f64)))).mul_dd(br);
    let total = lead.add(nfact.mul(sum));
    let v = total.to_f64();
    v * sign
}

/// Gamma(a) Gamma(b) / Gamma(a + b) for machine reals.
pub fn beta(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let pole = |v: f64| v <= 0.0 && v == v.floor();
    // a pole of Gamma(a) against a positive integer b: (b - 1)!/(a)_b
    for (x, y) in [(a, b), (b, a)] {
        if pole(x) && y >= 1.0 && y == y.floor() && y <= 170.0 {
            let mut den = 1.0f64;
            for i in 0..y as i64 {
                den *= x + i as f64;
            }
            let mut f = 1.0f64;
            for i in 2..(y as i64) {
                f *= i as f64;
            }
            return if den == 0.0 { f64::NAN } else { f / den };
        }
    }
    if pole(a) || pole(b) {
        return f64::NAN;
    }
    let ab = DD::sum(a, b);
    if pole(ab.hi) && ab.lo == 0.0 {
        return 0.0;
    }
    if a > 0.0 && b > 0.0 {
        let l = lgamma_re(DD::new(a)).add(lgamma_re(DD::new(b))).sub(lgamma_re(ab));
        return exp_scaled(l).to_f64();
    }
    gamma_scaled(DD::new(a)).mul(gamma_scaled(DD::new(b))).div(gamma_scaled(ab)).to_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ulps(got: f64, want: f64) -> f64 {
        ((got - want) / want).abs() / f64::EPSILON
    }

    #[test]
    fn known_values() {
        assert_eq!(gamma(5.0), 24.0);
        assert_eq!(gamma(1.0), 1.0);
        assert!(ulps(gamma(0.5), 1.772_453_850_905_516) <= 1.0);
        assert!(ulps(gamma(1.0 / 3.0), 2.678_938_534_707_747_6) <= 1.0);
        assert!(ulps(gamma(-1.5), 2.363_271_801_207_355) <= 1.0);
        assert_eq!(gamma(172.0), f64::INFINITY);
        assert!(gamma(-3.0).is_nan());
        assert_eq!(gamma(-0.0), f64::NEG_INFINITY);
        assert!(gamma(-250.5).abs() == 0.0);
        assert_eq!(log_gamma(1.0).0, 0.0);
        assert_eq!(log_gamma(2.0).0, 0.0);
        // ln Gamma(1 + z) = -gamma z + zeta(2)/2 z^2 - ..., z = 2^-33
        let z = 2f64.powi(-33);
        assert!(
            ulps(log_gamma(1.0 + z).0, -std::f64::consts::EULER_GAMMA * z + 0.822_467_033_424_113_2 * z * z) <= 2.0
        );
        assert!(ulps(digamma(1.0), -std::f64::consts::EULER_GAMMA) <= 1.0);
        assert!(ulps(digamma(0.5), -1.963_510_026_021_423_5) <= 1.0);
        assert!(ulps(polygamma(1.0, 1.0), 1.644_934_066_848_226_4) <= 2.0);
        assert!(ulps(beta(0.5, 0.5), std::f64::consts::PI) <= 1.0);
        assert!(ulps(beta(2.0, 3.0), 1.0 / 12.0) <= 1.0);
    }
}
