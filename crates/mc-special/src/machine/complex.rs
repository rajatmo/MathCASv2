//! Complex machine values of the special functions (decision P6N.9): plain double precision
//! (relative accuracy about 1e-13 in the tested ranges, not the 0.5 ulp of the real paths).
//!
//! - log gamma: Stirling's series (Bernoulli terms to B_30) after shifting to Re z >= 16, with the shift as a sum of
//!   principal logarithms, which is the analytic log gamma (log Gamma(z + 1) = log Gamma(z) + log z); gamma as its
//!   exponential, digamma by the shifted asymptotic series;
//! - Hurwitz zeta (and zeta, polygamma of integer order) by Euler-Maclaurin summation with N = 15 + |s| terms and
//!   twelve Bernoulli corrections; zeta at Re s < 1/2 by the functional equation;
//! - power series (with Kummer's transformation for Re z < 0 in 1F1) for 0F1, 1F1, |z| < 0.9 for 2F1, the
//!   exponential, sine and cosine integrals for |z| <= 40, Bessel J and I for |z| <= 25, Airy functions for
//!   |z| <= 8; Y and K for non-integer orders from J and I;
//! - Carlson's symmetric integrals R_F, R_D by duplication for the complete and incomplete elliptic integrals.
//!
//! Every function returns `None` outside the ranges where these methods are accurate (the call stays unevaluated).
use mc_numeric::machine::C64;

fn c(re: f64, im: f64) -> C64 {
    C64::new(re, im)
}
fn r(x: f64) -> C64 {
    C64::real(x)
}
fn abs(z: C64) -> f64 {
    z.re.hypot(z.im)
}
fn scale(z: C64, s: f64) -> C64 {
    c(z.re * s, z.im * s)
}
fn neg(z: C64) -> C64 {
    c(-z.re, -z.im)
}
/// z^w = exp(w log z)
fn powc(z: C64, w: C64) -> C64 {
    if z.re == 0.0 && z.im == 0.0 {
        return r(0.0);
    }
    (w * z.log()).exp()
}
fn finite(z: C64) -> Option<C64> {
    (z.re.is_finite() && z.im.is_finite()).then_some(z)
}

/// B_2k/(2k (2k - 1)) for k = 1..15 (Stirling's series).
const STIRLING: [f64; 15] = [
    1.0 / 12.0,
    -1.0 / 360.0,
    1.0 / 1260.0,
    -1.0 / 1680.0,
    1.0 / 1188.0,
    -691.0 / 360360.0,
    1.0 / 156.0,
    -3617.0 / 122400.0,
    43867.0 / 244188.0,
    -174611.0 / 125400.0,
    77683.0 / 5796.0,
    -236364091.0 / 1506960.0,
    657931.0 / 300.0,
    -3392780147.0 / 93960.0,
    1723168255201.0 / 2492028.0,
];

/// B_2k for k = 1..12.
const BERNOULLI: [f64; 12] = [
    1.0 / 6.0,
    -1.0 / 30.0,
    1.0 / 42.0,
    -1.0 / 30.0,
    5.0 / 66.0,
    -691.0 / 2730.0,
    7.0 / 6.0,
    -3617.0 / 510.0,
    43867.0 / 798.0,
    -174611.0 / 330.0,
    854513.0 / 138.0,
    -236364091.0 / 2730.0,
];

fn is_pole(z: C64) -> bool {
    z.im == 0.0 && z.re <= 0.0 && z.re == z.re.floor()
}

/// The analytic log gamma (principal at real z > 0).
pub fn log_gamma(z: C64) -> Option<C64> {
    if is_pole(z) || abs(z) > 1e15 {
        return None;
    }
    let mut shift = r(0.0);
    let mut w = z;
    let mut n = 0;
    while w.re < 16.0 {
        shift = shift + w.log();
        w = w + r(1.0);
        n += 1;
        if n > 200_000 {
            return None;
        }
    }
    // (w - 1/2) log w - w + log(2 pi)/2 + sum B_2k/(2k(2k-1) w^(2k-1))
    let lw = w.log();
    let mut s = (w - r(0.5)) * lw - w + r(0.918_938_533_204_672_8);
    let w2 = r(1.0) / (w * w);
    let mut t = r(1.0) / w;
    for b in STIRLING.iter() {
        s = s + scale(t, *b);
        t = t * w2;
    }
    finite(s - shift)
}

/// `gamma` at complex machine arguments (`None` outside the method's range).
pub fn gamma(z: C64) -> Option<C64> {
    finite(log_gamma(z)?.exp())
}

/// `digamma` at complex machine arguments (`None` outside the method's range).
pub fn digamma(z: C64) -> Option<C64> {
    if is_pole(z) {
        return None;
    }
    let mut acc = r(0.0);
    let mut w = z;
    let mut n = 0;
    while w.re < 16.0 {
        acc = acc - r(1.0) / w;
        w = w + r(1.0);
        n += 1;
        if n > 200_000 {
            return None;
        }
    }
    // log w - 1/(2w) - sum B_2k/(2k w^2k)
    let mut s = w.log() - r(0.5) / w;
    let w2 = r(1.0) / (w * w);
    let mut t = w2;
    for (k, b) in BERNOULLI.iter().enumerate() {
        s = s - scale(t, b / (2.0 * (k + 1) as f64));
        t = t * w2;
    }
    finite(s + acc)
}

/// The Hurwitz zeta function zeta(s, a) for Re a > 0 (a > 0 real) by Euler-Maclaurin.
pub fn hurwitz_zeta(s: C64, a: C64) -> Option<C64> {
    if s.re == 1.0 && s.im == 0.0 {
        return None;
    }
    if a.re <= 0.0 && a.im == 0.0 && a.re == a.re.floor() {
        return None;
    }
    let n = 15 + abs(s).ceil() as usize + (if a.re < 0.0 { (-a.re).ceil() as usize } else { 0 });
    if n > 100_000 {
        return None;
    }
    let mut sum = r(0.0);
    for k in 0..n {
        sum = sum + powc(a + r(k as f64), neg(s));
    }
    let an = a + r(n as f64);
    // tail: an^(1-s)/(s-1) + an^-s/2 + sum_j B_2j/(2j)! (s)_(2j-1) an^(-s-2j+1)
    let mut tail = powc(an, r(1.0) - s) / (s - r(1.0)) + scale(powc(an, neg(s)), 0.5);
    let mut poch = s; // (s)_(2j-1)
    let mut fact = 2.0; // (2j)!
    let mut p = powc(an, neg(s) - r(1.0)); // an^(-s-2j+1)
    let inv2 = r(1.0) / (an * an);
    for (j, b) in BERNOULLI.iter().enumerate() {
        let term = scale(poch * p, b / fact);
        tail = tail + term;
        let j2 = 2.0 * (j + 1) as f64;
        poch = poch * (s + r(j2 - 1.0)) * (s + r(j2));
        fact *= (j2 + 1.0) * (j2 + 2.0);
        p = p * inv2;
        if abs(term) < 1e-17 * abs(tail) {
            break;
        }
    }
    finite(sum + tail)
}

/// k^-s for an integer k >= 1 with the phase t log k (t = Im s) formed in double-double and reduced modulo 2 pi there:
/// in plain doubles the phase of 1e4 i log k (1e5) is wrong in its eleventh digit.
fn int_pow_neg(k: f64, s: C64) -> C64 {
    use crate::dd::{log, sin_cos_pi_any, DD, PI};
    use std::ops::Mul;
    let lk = log(DD::new(k));
    let mag = (-s.re * lk.hi).exp();
    let half_turns = lk.mul_f(-s.im).mul(PI.recip());
    let (sn, cs) = sin_cos_pi_any(half_turns);
    c(mag * cs.to_f64(), mag * sn.to_f64())
}

/// zeta(s) for Re s >= -1/2 and a large |s| by Euler-Maclaurin summation (DLMF 25.2.9 with N = |s| + 15 terms and 12
/// Bernoulli corrections, DLMF 25.2.12) with the phases of k^-s in double-double (round 27, B26-21: zeta(8735 i) took
/// the 192-bit enclosure, 40 s). The cost is |s| terms of about a microsecond.
fn zeta_em_large(s: C64) -> Option<C64> {
    let n = (15.0 + abs(s)).ceil();
    if n > 600_000.0 {
        return None;
    }
    let mut sum = r(0.0);
    let mut k = 1.0;
    while k < n {
        sum = sum + int_pow_neg(k, s);
        k += 1.0;
    }
    let n_s = int_pow_neg(n, s);
    let mut tail = scale(n_s, n) / (s - r(1.0)) + scale(n_s, 0.5);
    let mut poch = s;
    let mut fact = 2.0;
    let inv2 = 1.0 / (n * n);
    let mut p = scale(n_s, 1.0 / n);
    for (j, b) in BERNOULLI.iter().enumerate() {
        tail = tail + scale(poch * p, b / fact);
        let j2 = 2.0 * (j + 1) as f64;
        poch = poch * (s + r(j2 - 1.0)) * (s + r(j2));
        fact *= (j2 + 1.0) * (j2 + 2.0);
        p = scale(p, inv2);
    }
    finite(sum + tail)
}

/// The Riemann zeta function, with the functional equation for Re s < 1/2.
pub fn zeta(s: C64) -> Option<C64> {
    if s.re >= -0.5 && abs(s) >= 30.0 {
        return zeta_em_large(s);
    }
    if s.re < 0.5 {
        // zeta(s) = 2^s pi^(s-1) sin(pi s/2) gamma(1-s) zeta(1-s)
        let one_s = r(1.0) - s;
        let f = powc(r(2.0), s) * powc(r(std::f64::consts::PI), s - r(1.0)) * scale(s, std::f64::consts::FRAC_PI_2).sin();
        return finite(f * gamma(one_s)? * hurwitz_zeta(one_s, r(1.0))?);
    }
    hurwitz_zeta(s, r(1.0))
}

/// polygamma(m, z) = (-1)^(m+1) m! zeta(m + 1, z) for integer m >= 1.
pub fn polygamma(m: f64, z: C64) -> Option<C64> {
    if m == 0.0 {
        return digamma(z);
    }
    if m < 0.0 || m != m.floor() || m > 100.0 {
        return None;
    }
    let fact: f64 = (1..=m as u64).map(|k| k as f64).product();
    let sign = if (m as u64).is_multiple_of(2) { -1.0 } else { 1.0 };
    // zeta(m + 1, z) needs Re z > 0: the recurrence psi^(m)(z) = psi^(m)(z + 1) - (-1)^m m!/z^(m+1)
    let mut w = z;
    let mut acc = r(0.0);
    let mut big: f64 = 0.0;
    while w.re < 1.0 {
        let t = scale(r(1.0) / powc(w, r(m + 1.0)), sign * fact);
        big = big.max(abs(t));
        acc = acc + t;
        w = w + r(1.0);
    }
    let v = scale(hurwitz_zeta(r(m + 1.0), w)?, sign * fact) + acc;
    // the upward recurrence adds terms as large as m!/|z|^(m+1) near the imaginary axis: give the value up (the
    // caller falls back to balls) when more than about two digits cancel (B23-22)
    if big > 1e2 * abs(v) && big > 1e-300 {
        return None;
    }
    finite(v)
}

/// `beta` at complex machine arguments (`None` outside the method's range).
pub fn beta(a: C64, b: C64) -> Option<C64> {
    finite((log_gamma(a)? + log_gamma(b)? - log_gamma(a + b)?).exp())
}

/// sum_k term_k with term_(k+1) = term_k * ratio(k), until the terms are negligible.
fn series(first: C64, ratio: impl Fn(usize) -> C64, max: usize) -> Option<C64> {
    let mut t = first;
    let mut s = first;
    let mut big = abs(first);
    for k in 0..max {
        t = t * ratio(k);
        s = s + t;
        big = big.max(abs(t));
        if abs(t) <= 1e-17 * abs(s) && k > 2 {
            // cancellation: at most 4 digits lost
            return (big <= 1e4 * abs(s).max(1e-300)).then_some(s);
        }
    }
    None
}

/// `hyp0f1` at complex machine arguments (`None` outside the method's range).
pub fn hyp0f1(b: C64, z: C64) -> Option<C64> {
    if abs(z) > 900.0 || is_pole(b) {
        return None;
    }
    series(r(1.0), |k| z / ((b + r(k as f64)) * r((k + 1) as f64)), 2000)
}

/// `hyp1f1` at complex machine arguments (`None` outside the method's range).
pub fn hyp1f1(a: C64, b: C64, z: C64) -> Option<C64> {
    if abs(z) > 600.0 || is_pole(b) {
        return None;
    }
    if z.re < 0.0 {
        // Kummer: e^z 1F1(b - a; b; -z)
        let v = series(r(1.0), |k| (b - a + r(k as f64)) * neg(z) / ((b + r(k as f64)) * r((k + 1) as f64)), 5000)?;
        return finite(z.exp() * v);
    }
    series(r(1.0), |k| (a + r(k as f64)) * z / ((b + r(k as f64)) * r((k + 1) as f64)), 5000)
}

/// `hyp2f1` at complex machine arguments (`None` outside the method's range).
pub fn hyp2f1(a: C64, b: C64, cc: C64, z: C64) -> Option<C64> {
    let term = |a: C64, b: C64, z: C64| move |k: usize| (a + r(k as f64)) * (b + r(k as f64)) * z / ((cc + r(k as f64)) * r((k + 1) as f64));
    // a terminating series (a or b a non-positive integer before c reaches a pole): the polynomial, any z
    let ends = |p: C64| (is_pole(p) && p.re >= -10_000.0).then(|| -p.re as usize);
    if let Some(n) = [ends(a), ends(b)].into_iter().flatten().min() {
        if !(is_pole(cc) && -cc.re < n as f64) {
            let f = term(a, b, z);
            let (mut t, mut s) = (r(1.0), r(1.0));
            for k in 0..n {
                t = t * f(k);
                s = s + t;
            }
            return finite(s);
        }
    }
    if is_pole(cc) {
        return None;
    }
    // DLMF 15.4.6: F(a, b; a; z) = (1 - z)^-b, any z off [1, inf) and on it from below; an integer power by products, so
    // that a real value has no imaginary part (2F1(5, 6; 5; 4.5) had 4e-70 i)
    for (p, q) in [(a, b), (b, a)] {
        if p.re == cc.re && p.im == cc.im {
            let base = r(1.0) - z;
            if q.im == 0.0 && q.re.fract() == 0.0 && q.re.abs() <= 64.0 {
                let mut v = r(1.0);
                for _ in 0..q.re.abs() as usize {
                    v = v * base;
                }
                return finite(if q.re > 0.0 { r(1.0) / v } else { v });
            }
            return finite(powc(base, neg(q)));
        }
    }
    if abs(z) < 0.9 {
        return series(r(1.0), term(a, b, z), 2000);
    }
    // Pfaff: (1 - z)^-a 2F1(a, c - b; c; z/(z - 1))
    let one = r(1.0);
    let w = z / (z - one);
    if abs(w) < 0.9 {
        let f = series(one, term(a, cc - b, w), 2000)?;
        return finite(powc(one - z, neg(a)) * f);
    }
    hyp2f1_connection(a, b, cc, z)
}

/// pFq(a; b; z) at complex machine arguments by its power series (round 37): any z when p <= q, |z| < 0.9 when
/// p = q + 1, and the polynomial when a numerator parameter is a non-positive integer. `None` when a denominator
/// parameter is a pole the series reaches, when it needs more than 5000 terms, or when the largest term exceeds the
/// sum a thousandfold (the cancellation would leave fewer than 13 digits).
pub fn hyp_pfq(a: &[C64], b: &[C64], z: C64) -> Option<C64> {
    let ends = a.iter().filter(|&&p| is_pole(p) && p.re >= -5000.0).map(|p| -p.re as usize).min();
    if b.iter().any(|&q| is_pole(q) && ends.is_none_or(|n| (-q.re) < n as f64)) {
        return None;
    }
    if ends.is_none() && (a.len() > b.len() + 1 || (a.len() == b.len() + 1 && abs(z) >= 0.9)) {
        return None;
    }
    let (mut t, mut s) = (r(1.0), r(1.0));
    let mut big = 1.0f64;
    let mut small = 0;
    for k in 0..5000usize {
        if ends == Some(k) {
            return finite(s);
        }
        let kk = r(k as f64);
        let mut f = scale(z, 1.0 / (k + 1) as f64);
        for &p in a {
            f = f * (p + kk);
        }
        for &q in b {
            f = f / (q + kk);
        }
        t = t * f;
        s = s + t;
        let (at, as_) = (abs(t), abs(s));
        big = big.max(at);
        if !(at.is_finite() && as_.is_finite()) {
            return None;
        }
        // stop after three consecutive negligible terms past the largest one
        if at <= 1e-17 * as_ && abs(f) < 1.0 {
            small += 1;
            if small == 3 {
                return (big <= 1e3 * as_).then_some(s);
            }
        } else {
            small = 0;
        }
    }
    None
}

/// 1/Gamma(z), 0 at the poles.
fn rgamma(z: C64) -> Option<C64> {
    if is_pole(z) {
        return Some(r(0.0));
    }
    Some(r(1.0) / gamma(z)?)
}

/// 2F1 beyond the disks of the series and of Pfaff's transformation (round 37): the connection formulas to 1/z
/// (DLMF 15.8.2), to 1/(1 - z) (15.8.3) and to 1 - z (15.8.4), whichever has its new argument inside |w| < 0.9.
/// Each is a sum of two series; `None` (the ball path then) when the parameter difference that the formula divides
/// by is an integer (the formulas are singular there), when the two terms cancel to fewer than 9 of the 16 digits,
/// or when no transformation applies (near z = exp(+-i pi/3)). A real z > 1 is the limit from below (Im z = -0),
/// as in the ball path: the powers of -z and 1 - z take arguments +pi there.
fn hyp2f1_connection(a: C64, b: C64, cc: C64, z: C64) -> Option<C64> {
    let one = r(1.0);
    let term = |a: C64, b: C64, c: C64, z: C64| move |k: usize| (a + r(k as f64)) * (b + r(k as f64)) * z / ((c + r(k as f64)) * r((k + 1) as f64));
    // the limit from below on the cut: -z and 1 - z with Im +0
    let up = |w: C64| if w.im == 0.0 { c(w.re, 0.0) } else { w };
    let integer = |d: C64| d.im == 0.0 && d.re.fract() == 0.0;
    let gc = gamma(cc)?;
    let pair = |t1: C64, t2: C64| -> Option<C64> {
        let s = t1 + t2;
        (abs(s) >= 1e-7 * (abs(t1) + abs(t2))).then_some(s).and_then(finite)
    };
    if abs(z) > 1.0 / 0.9 && !integer(a - b) {
        // (-z)^-a and (-z)^-b with the series in 1/z
        let mz = up(neg(z));
        let w = one / z;
        let f1 = series(one, term(a, a - cc + one, a - b + one, w), 2000)?;
        let f2 = series(one, term(b, b - cc + one, b - a + one, w), 2000)?;
        let t1 = gc * gamma(b - a)? * rgamma(b)? * rgamma(cc - a)? * powc(mz, neg(a)) * f1;
        let t2 = gc * gamma(a - b)? * rgamma(a)? * rgamma(cc - b)? * powc(mz, neg(b)) * f2;
        return pair(t1, t2);
    }
    let omz = up(one - z);
    if abs(omz) > 1.0 / 0.9 && !integer(a - b) {
        let w = one / omz;
        let f1 = series(one, term(a, cc - b, a - b + one, w), 2000)?;
        let f2 = series(one, term(b, cc - a, b - a + one, w), 2000)?;
        let t1 = gc * gamma(b - a)? * rgamma(b)? * rgamma(cc - a)? * powc(omz, neg(a)) * f1;
        let t2 = gc * gamma(a - b)? * rgamma(a)? * rgamma(cc - b)? * powc(omz, neg(b)) * f2;
        return pair(t1, t2);
    }
    let s = cc - a - b;
    if abs(omz) < 0.9 && !integer(s) {
        let f1 = series(one, term(a, b, one - s, omz), 2000)?;
        let f2 = series(one, term(cc - a, cc - b, one + s, omz), 2000)?;
        let t1 = gc * gamma(s)? * rgamma(cc - a)? * rgamma(cc - b)? * f1;
        let t2 = gc * gamma(neg(s))? * rgamma(a)? * rgamma(b)? * powc(omz, s) * f2;
        return pair(t1, t2);
    }
    None
}

/// `exp_integral_ei` at complex machine arguments (`None` outside the method's range).
pub fn exp_integral_ei(z: C64) -> Option<C64> {
    if abs(z) > 40.0 || abs(z) == 0.0 {
        return None;
    }
    // gamma + log z + sum z^k/(k k!)
    let s = series(z, |k| z * r((k + 1) as f64) / r(((k + 2) * (k + 2)) as f64), 500)?;
    // the principal branch of Ei for non-real z: (log z - log(1/z))/2 - log(-z) + log z ... in the form of the
    // reference convention Ei(z) = gamma + log z + sum
    finite(r(std::f64::consts::EULER_GAMMA) + z.log() + s)
}

/// `log_integral` at complex machine arguments (`None` outside the method's range).
pub fn log_integral(z: C64) -> Option<C64> {
    exp_integral_ei(z.log())
}

/// Si, Ci, Shi, Chi.
pub fn trig_integral(z: C64, which: u8) -> Option<C64> {
    if abs(z) > 40.0 || abs(z) == 0.0 {
        return None;
    }
    let z2 = z * z;
    let eg = r(std::f64::consts::EULER_GAMMA);
    match which {
        // Si: sum (-1)^k z^(2k+1)/((2k+1)(2k+1)!)
        0 => series(z, |k| {
            let k = k as f64;
            neg(z2) * r((2.0 * k + 1.0) / ((2.0 * k + 3.0) * (2.0 * k + 2.0) * (2.0 * k + 3.0)))
        }, 500),
        // Ci: gamma + log z + sum_(k>=1) (-1)^k z^2k/(2k (2k)!)
        1 => {
            let first = scale(neg(z2), 0.25);
            let s = series(first, |k| {
                let k = (k + 1) as f64;
                neg(z2) * r((2.0 * k) / ((2.0 * k + 2.0) * (2.0 * k + 1.0) * (2.0 * k + 2.0)))
            }, 500)?;
            finite(eg + z.log() + s)
        }
        // Shi
        2 => series(z, |k| {
            let k = k as f64;
            z2 * r((2.0 * k + 1.0) / ((2.0 * k + 3.0) * (2.0 * k + 2.0) * (2.0 * k + 3.0)))
        }, 500),
        // Chi
        _ => {
            let first = scale(z2, 0.25);
            let s = series(first, |k| {
                let k = (k + 1) as f64;
                z2 * r((2.0 * k) / ((2.0 * k + 2.0) * (2.0 * k + 1.0) * (2.0 * k + 2.0)))
            }, 500)?;
            finite(eg + z.log() + s)
        }
    }
}

/// J_nu(z) (sign -1) or I_nu(z) (sign +1) by the power series (z/2)^nu sum (s z^2/4)^k/(k! gamma(nu + k + 1)).
fn bessel_series(nu: C64, z: C64, sign: f64) -> Option<C64> {
    if abs(z) > 25.0 {
        return None;
    }
    let q = scale(z * z, sign * 0.25);
    let first = if is_pole(nu + r(1.0)) {
        // 1/gamma at a pole is 0: the series starts at the first nonzero term (integer nu < 0)
        return None;
    } else {
        powc(scale(z, 0.5), nu) / gamma(nu + r(1.0))?
    };
    series(first, |k| q / (r((k + 1) as f64) * (nu + r((k + 1) as f64))), 2000)
}

/// `bessel_j` at complex machine arguments (`None` outside the method's range).
pub fn bessel_j(nu: C64, z: C64) -> Option<C64> {
    if nu.im == 0.0 && nu.re < 0.0 && nu.re == nu.re.floor() {
        // J_-n = (-1)^n J_n
        let v = bessel_series(neg(nu), z, -1.0)?;
        return Some(if (-nu.re) as i64 % 2 == 0 { v } else { neg(v) });
    }
    bessel_series(nu, z, -1.0)
}

/// `bessel_i` at complex machine arguments (`None` outside the method's range).
pub fn bessel_i(nu: C64, z: C64) -> Option<C64> {
    if nu.im == 0.0 && nu.re < 0.0 && nu.re == nu.re.floor() {
        return bessel_series(neg(nu), z, 1.0);
    }
    bessel_series(nu, z, 1.0)
}

fn integer(nu: C64) -> bool {
    nu.im == 0.0 && nu.re == nu.re.floor()
}

/// Y_n(z) (`modified` false) or K_n(z) (`modified` true) for an integer order n >= 0 at complex z, by the series
/// with the logarithm (DLMF 10.8.1, 10.31.1): a finite sum in (z/2)^-n, the log term with J_n or I_n, and the sum
/// with psi(k + 1) + psi(n + k + 1). `None` when |z| is large or the sums cancel more than six digits.
fn integer_order_second_kind(n: u32, z: C64, modified: bool) -> Option<C64> {
    if abs(z) > 20.0 || abs(z) == 0.0 {
        return None;
    }
    let h = scale(z, 0.5);
    let q = h * h; // z^2/4
    let nf = n as f64;
    let pi = std::f64::consts::PI;
    // finite part: sum_(k<n) (n-k-1)!/k! (s q)^k, s = 1 (Y) or -1 (K)
    let sq = if modified { neg(q) } else { q };
    let mut fin = r(0.0);
    let mut pk = r(1.0);
    for k in 0..n {
        let mut fact = 1.0; // (n-k-1)!/k!
        for j in 1..=(n - k - 1) {
            fact *= j as f64;
        }
        for j in 1..=k {
            fact /= j as f64;
        }
        fin = fin + scale(pk, fact);
        pk = pk * sq;
    }
    let hn = powc(h, r(nf));
    let fin = if n == 0 { r(0.0) } else { fin / hn };
    // the psi sum: sum_k (psi(k+1) + psi(n+k+1)) (t q)^k/(k! (n+k)!), t = -1 (Y) or +1 (K)
    let tq = if modified { q } else { neg(q) };
    let eg = std::f64::consts::EULER_GAMMA;
    let mut hk = 0.0; // H_k
    let mut hnk: f64 = (1..=n).map(|j| 1.0 / j as f64).sum(); // H_(n+k)
    let mut nfact = 1.0;
    for j in 1..=n {
        nfact *= j as f64;
    }
    let mut term = r(1.0 / nfact); // (t q)^k/(k! (n+k)!)
    let mut sum = r(0.0);
    let mut big: f64 = 0.0;
    let mut done = false;
    for k in 0..3000u32 {
        let t = scale(term, -2.0 * eg + hk + hnk);
        sum = sum + t;
        big = big.max(abs(t));
        if k > 2 && abs(t) <= 1e-17 * abs(sum) {
            done = true;
            break;
        }
        let k1 = (k + 1) as f64;
        term = term * tq / r(k1 * (nf + k1));
        hk += 1.0 / k1;
        hnk += 1.0 / (nf + k1);
    }
    if !done || big > 1e6 * abs(sum).max(1e-300) {
        return None;
    }
    let lg = h.log();
    let nu = r(nf);
    let parts = if modified {
        // K_n = 1/2 (z/2)^-n fin + (-1)^(n+1) log(z/2) I_n + (-1)^n 1/2 (z/2)^n sum
        let sgn = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
        [scale(fin, 0.5), scale(lg * bessel_i(nu, z)?, -sgn), scale(hn * sum, 0.5 * sgn)]
    } else {
        // Y_n = -(z/2)^-n fin/pi + 2/pi log(z/2) J_n - (z/2)^n sum/pi
        [scale(fin, -1.0 / pi), scale(lg * bessel_j(nu, z)?, 2.0 / pi), scale(hn * sum, -1.0 / pi)]
    };
    let v = parts[0] + parts[1] + parts[2];
    // the three parts are as large as e^|z| (K at Re z > 0 is as small as e^-Re z): give the value up (the caller
    // falls back to balls) when more than about five digits cancel
    let biggest = parts.iter().map(|p| abs(*p)).fold(0.0, f64::max);
    if biggest > 1e5 * abs(v) && biggest > 1e-300 {
        return None;
    }
    finite(v)
}

/// Gamma(a, z) for a large |z| (at least 40, |a| <= |z|/4, not near the negative axis) by the asymptotic series
/// z^(a-1) e^-z sum_k (a - 1)(a - 2) ... (a - k)/z^k (DLMF 8.11.2; Temme, *Special Functions*, 1996, section 11.2),
/// summed until a term is below 1e-17 of the sum (round 27, B26-20: the formula through 1F1 loses everything at
/// z = 30000 i). The two exponentials are formed separately, so the phase of e^-z, a machine number times pi
/// reduced exactly, is not rounded together with the small phase of z^(a-1).
pub fn gamma_upper_large(a: C64, z: C64) -> Option<C64> {
    let az = abs(z);
    if az < 40.0 || z.re < -0.5 * az || abs(a) > 0.25 * az {
        return None;
    }
    let (mut term, mut sum) = (r(1.0), r(1.0));
    let mut converged = false;
    for k in 1..2000 {
        term = term * (a - r(k as f64)) / z;
        sum = sum + term;
        if abs(term) <= 1e-17 * abs(sum) {
            converged = true;
            break;
        }
    }
    if !converged {
        return None;
    }
    let p = (((a - r(1.0)) * z.log()).exp()) * neg(z).exp();
    finite(p * sum)
}

/// E_nu(z) at complex machine arguments: the continued fraction e^-z/(z + nu/(1 + 1/(z + (nu+1)/(1 + 2/(z + ...)))))
/// for |z| >= 2 off the negative axis (modified Lentz, DLMF 8.19.17 with 8.9.2), else the series
/// Gamma(1 - nu) z^(nu - 1) - sum_k (-z)^k/(k! (1 - nu + k)) (DLMF 8.19.10, nu not a positive integer; for a
/// positive integer nu = n the limit form with psi(n), DLMF 8.19.8).
pub fn exp_integral_e(nu: C64, z: C64) -> Option<C64> {
    let az = abs(z);
    if az == 0.0 || z.re < -700.0 || az > 1e15 {
        return None;
    }
    if az >= 2.0 && !(z.im == 0.0 && z.re < 0.0) {
        // b_k = z + nu + ... in the even form: E = e^-z/(z + nu - 1*nu/(z + nu + 2 - 2(nu+1)/(z + nu + 4 - ...)))
        let tiny = 1e-300;
        let mut b = z + nu;
        let mut f = b;
        let mut cc = f;
        let mut d = r(0.0);
        let mut ok = false;
        for k in 1..5000 {
            let kf = k as f64;
            let an = neg(scale(nu + r(kf - 1.0), kf));
            b = b + r(2.0);
            d = b + an * d;
            if abs(d) < tiny {
                d = r(tiny);
            }
            cc = b + an / cc;
            if abs(cc) < tiny {
                cc = r(tiny);
            }
            d = r(1.0) / d;
            let delta = cc * d;
            f = f * delta;
            if abs(delta - r(1.0)) < 1e-16 {
                ok = true;
                break;
            }
        }
        if !ok {
            return None;
        }
        return finite(neg(z).exp() / f);
    }
    let one = r(1.0);
    if integer(nu) && nu.re >= 1.0 {
        // E_n(z) = (-z)^(n-1)/(n-1)! (psi(n) - log z) - sum_(k != n-1) (-z)^k/(k! (1 - n + k))
        let n = nu.re as i64;
        let mut s = r(0.0);
        let mut t = one; // (-z)^k/k!
        let mut psi = -std::f64::consts::EULER_GAMMA;
        for j in 1..n {
            psi += 1.0 / j as f64;
        }
        let mut lead = r(0.0);
        for k in 0..400i64 {
            if k == n - 1 {
                lead = t * (r(psi) - z.log());
            } else {
                let add = t / r((1 - n + k) as f64);
                s = s + add;
                if k > n && abs(add) <= 1e-17 * abs(s) {
                    break;
                }
            }
            t = t * neg(z) / r((k + 1) as f64);
        }
        return finite(lead - s);
    }
    let g = gamma(one - nu)?;
    let mut s = r(0.0);
    let mut t = one;
    for k in 0..400 {
        let add = t / (one - nu + r(k as f64));
        s = s + add;
        if k > 2 && abs(add) <= 1e-17 * abs(s) {
            break;
        }
        t = t * neg(z) / r((k + 1) as f64);
    }
    finite(g * powc(z, nu - one) - s)
}

/// `bessel_y` at complex machine arguments (`None` outside the method's range).
pub fn bessel_y(nu: C64, z: C64) -> Option<C64> {
    if integer(nu) {
        // Y_-n = (-1)^n Y_n
        let n = nu.re.abs();
        if n > 200.0 {
            return None;
        }
        let v = integer_order_second_kind(n as u32, z, false)?;
        return Some(if nu.re < 0.0 && (n as u32) % 2 == 1 { neg(v) } else { v });
    }
    // a real negative argument -x at a real order: Y_nu(x e^(i pi)) = e^(-i nu pi) Y_nu(x) + 2 i cos(nu pi) J_nu(x)
    // (DLMF 10.11.2, the value above the cut), from the real functions of x > 0 (round 27, B26-22: the series through
    // J_nu and J_-nu took 5 s at x = 30510 and gave up)
    if nu.im == 0.0 && z.im == 0.0 && z.re < 0.0 {
        use crate::dd::{sin_cos_pi_any, DD};
        let x = -z.re;
        let (jv, yv) = (super::bessel_j::bessel_j(nu.re, x), super::bessel_j::bessel_y(nu.re, x));
        if jv.is_finite() && yv.is_finite() {
            let (sn, cs) = sin_cos_pi_any(DD::new(nu.re));
            let (sn, mut cs) = (sn.to_f64(), cs.to_f64());
            if (2.0 * nu.re.abs()) % 2.0 == 1.0 {
                cs = 0.0;
            }
            return finite(c(cs * yv, -sn * yv + 2.0 * cs * jv));
        }
    }
    // (J_nu cos(nu pi) - J_-nu)/sin(nu pi)
    let a = scale(nu, std::f64::consts::PI);
    let (t1, t2) = (bessel_j(nu, z)? * a.cos(), bessel_j(neg(nu), z)?);
    if abs(t1).max(abs(t2)) > 1e5 * abs(t1 - t2) {
        return None;
    }
    finite((t1 - t2) / a.sin())
}

/// K_nu(z) for a large |z| (at least 25 and 4 nu^2) by the Hankel expansion sqrt(pi/(2z)) e^-z sum_k a_k(nu)/z^k,
/// a_k = (4nu^2 - 1)(4nu^2 - 9) ... (4nu^2 - (2k - 1)^2)/(k! 8^k) (DLMF 10.40.2, valid for |arg z| < 3 pi/2); the
/// exponential is formed from e^-Re z and the exactly reduced phase of Im z (round 27, B26-22: K_1(9664 i) did not
/// return). `None` when the terms do not fall below 1e-17 of the sum.
fn bessel_k_large(nu: C64, z: C64) -> Option<C64> {
    let az = abs(z);
    if az < 25.0 || z.re < -0.5 * az || abs(nu) * abs(nu) * 4.0 > az {
        return None;
    }
    let mu = nu * nu * r(4.0);
    let (mut term, mut sum) = (r(1.0), r(1.0));
    let mut ok = false;
    for k in 1..400 {
        let kf = k as f64;
        let odd = 2.0 * kf - 1.0;
        term = term * (mu - r(odd * odd)) / scale(z, 8.0 * kf);
        sum = sum + term;
        if abs(term) <= 1e-17 * abs(sum) || abs(term) == 0.0 {
            ok = true;
            break;
        }
    }
    if !ok {
        return None;
    }
    let pre = (scale(r(std::f64::consts::FRAC_PI_2) / z, 1.0)).sqrt();
    finite(pre * neg(z).exp() * sum)
}

/// `bessel_k` at complex machine arguments (`None` outside the method's range).
pub fn bessel_k(nu: C64, z: C64) -> Option<C64> {
    if abs(z) >= 25.0 {
        if let Some(v) = bessel_k_large(nu, z) {
            return Some(v);
        }
    }
    if integer(nu) {
        // K_-n = K_n
        let n = nu.re.abs();
        if n > 200.0 {
            return None;
        }
        return integer_order_second_kind(n as u32, z, true);
    }
    if let Some(v) = bessel_k_large(nu, z) {
        return Some(v);
    }
    // pi/2 (I_-nu - I_nu)/sin(nu pi)
    let a = scale(nu, std::f64::consts::PI);
    let (i1, i2) = (bessel_i(neg(nu), z)?, bessel_i(nu, z)?);
    // I_-nu - I_nu is as small as e^-Re z against terms of size e^Re z: give up on more than five lost digits
    if abs(i1).max(abs(i2)) > 1e5 * abs(i1 - i2) {
        return None;
    }
    finite(scale(i1 - i2, std::f64::consts::FRAC_PI_2) / a.sin())
}

/// Ai, Bi, Ai', Bi' by the Maclaurin series f, g (DLMF 9.4.1-9.4.4).
pub fn airy(z: C64, which: u8) -> Option<C64> {
    if abs(z) > 8.0 {
        return None;
    }
    let z3 = z * z * z;
    // f = sum 3^k (1/3)_k z^3k/(3k)!, g = sum 3^k (2/3)_k z^(3k+1)/(3k+1)!
    let f = series(r(1.0), |k| {
        let k = k as f64;
        z3 / r((3.0 * k + 2.0) * (3.0 * k + 3.0))
    }, 500)?;
    let g = series(z, |k| {
        let k = k as f64;
        z3 / r((3.0 * k + 3.0) * (3.0 * k + 4.0))
    }, 500)?;
    // f' = sum_(k>=1) 3^k (1/3)_k z^(3k-1)/(3k-1)!, g' = sum 3^k (2/3)_k z^3k/(3k)!
    let fp = series(scale(z * z, 0.5), |k| {
        let k = k as f64;
        z3 / r((3.0 * k + 3.0) * (3.0 * k + 5.0))
    }, 500)?;
    let gp = series(r(1.0), |k| {
        let k = k as f64;
        z3 / r((3.0 * k + 1.0) * (3.0 * k + 3.0))
    }, 500)?;
    let c1 = 0.355_028_053_887_817_2;
    let c2 = 0.258_819_403_792_806_8;
    let s3 = 3f64.sqrt();
    Some(match which {
        0 => scale(f, c1) - scale(g, c2),
        1 => scale(scale(f, c1) + scale(g, c2), s3),
        2 => scale(fp, c1) - scale(gp, c2),
        _ => scale(scale(fp, c1) + scale(gp, c2), s3),
    })
}

/// Carlson's R_F by duplication (complex arguments off the negative real axis).
pub fn carlson_rf(x: C64, y: C64, z: C64) -> Option<C64> {
    let (mut x, mut y, mut z) = (x, y, z);
    for _ in 0..100 {
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let l = sx * sy + sx * sz + sy * sz;
        x = scale(x + l, 0.25);
        y = scale(y + l, 0.25);
        z = scale(z + l, 0.25);
        let a = scale(x + y + z, 1.0 / 3.0);
        let d = abs(x - a).max(abs(y - a)).max(abs(z - a));
        if d < 1e-4 * abs(a) {
            let (dx, dy, dz) = ((a - x) / a, (a - y) / a, (a - z) / a);
            let e2 = dx * dy - dz * dz;
            let e3 = dx * dy * dz;
            let s = r(1.0) - scale(e2, 0.1) + scale(e3, 1.0 / 14.0) + scale(e2 * e2, 1.0 / 24.0)
                - scale(e2 * e3, 3.0 / 44.0);
            return finite(s / a.sqrt());
        }
    }
    None
}

/// Carlson's R_D by duplication (the series of Carlson 1995 after convergence).
pub fn carlson_rd(x: C64, y: C64, z: C64) -> Option<C64> {
    let (mut x, mut y, mut z) = (x, y, z);
    let mut sum = r(0.0);
    let mut fac = 1.0;
    for _ in 0..100 {
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let l = sx * (sy + sz) + sy * sz;
        sum = sum + scale(r(1.0) / (sz * (z + l)), fac);
        fac *= 0.25;
        x = scale(x + l, 0.25);
        y = scale(y + l, 0.25);
        z = scale(z + l, 0.25);
        let a = scale(x + y + scale(z, 3.0), 0.2);
        let (dx, dy, dz) = ((a - x) / a, (a - y) / a, (a - z) / a);
        if abs(dx).max(abs(dy)).max(abs(dz)) < 1e-3 {
            let ea = dx * dy;
            let eb = dz * dz;
            let ec = ea - eb;
            let ed = ea - scale(eb, 6.0);
            let ee = ed + ec + ec;
            let (c1, c2, c3, c4, c5, c6) = (3.0 / 14.0, 1.0 / 6.0, 9.0 / 22.0, 3.0 / 26.0, 3.0 / 56.0, 9.0 / 52.0);
            let inner1 = ed * (r(-c1) + scale(ed, c5) - scale(dz * ee, c6));
            let inner2 = dz * (scale(ee, c2) + dz * (scale(ec, -c3) + scale(dz * ea, c4)));
            let s = r(1.0) + inner1 + inner2;
            return finite(scale(sum, 3.0) + scale(s, fac) / (a * a.sqrt()));
        }
    }
    None
}

/// `elliptic_k` at complex machine arguments (`None` outside the method's range).
pub fn elliptic_k(m: C64) -> Option<C64> {
    carlson_rf(r(0.0), r(1.0) - m, r(1.0))
}

/// `elliptic_e` at complex machine arguments (`None` outside the method's range).
pub fn elliptic_e(m: C64) -> Option<C64> {
    let k1 = r(1.0) - m;
    finite(carlson_rf(r(0.0), k1, r(1.0))? - scale(m * carlson_rd(r(0.0), k1, r(1.0))?, 1.0 / 3.0))
}

/// F(phi | m) for |Re phi| <= pi/2 (reduced by periodicity, F(phi + k pi) = F(phi) + 2 k K).
pub fn elliptic_f(phi: C64, m: C64) -> Option<C64> {
    let k = (phi.re / std::f64::consts::PI).round();
    let p = phi - r(k * std::f64::consts::PI);
    let (s, cphi) = (p.sin(), p.cos());
    let v = s * carlson_rf(cphi * cphi, r(1.0) - m * s * s, r(1.0))?;
    if k == 0.0 {
        return finite(v);
    }
    finite(v + scale(elliptic_k(m)?, 2.0 * k))
}

/// E(phi | m) likewise.
pub fn elliptic_e_inc(phi: C64, m: C64) -> Option<C64> {
    let k = (phi.re / std::f64::consts::PI).round();
    let p = phi - r(k * std::f64::consts::PI);
    let (s, cphi) = (p.sin(), p.cos());
    let (x, y) = (cphi * cphi, r(1.0) - m * s * s);
    let v = s * carlson_rf(x, y, r(1.0))? - scale(m * s * s * s * carlson_rd(x, y, r(1.0))?, 1.0 / 3.0);
    if k == 0.0 {
        return finite(v);
    }
    finite(v + scale(elliptic_e(m)?, 2.0 * k))
}
