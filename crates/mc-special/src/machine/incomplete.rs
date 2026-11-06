//! The incomplete gamma and beta functions and their inverses for machine reals (decision SF.3).
//!
//! Values are formed in double-double arithmetic and rounded once; the prefactors x^a e^-x / Gamma(a) and
//! x^a (1-x)^b / B(a, b) are exponentials of double-double logarithms (a ln x - x - ln Gamma(a), the logarithm of
//! 1 - x of the exact double-double 1 - x), carried with their own binary exponent, so nothing over- or underflows
//! before the final rounding.
//!
//! - **P(a, x), Q(a, x)**, a > 0: for a < 1 and x <= 3/2 Temme's form
//!   Gamma(a, x) = (Gamma(1+a) - 1)/a - (x^a - 1)/a - x^a sum_{k>=1} (-x)^k/(k! (a+k)), with
//!   (Gamma(1+a) - 1)/a = -P(a)/R(a) from the Taylor series R(a) = 1 + a P(a) of 1/Gamma(1+a) and
//!   (x^a - 1)/a = ln x expm1(a ln x)/(a ln x), so that Q = a R(a) Gamma(a, x) keeps its relative accuracy
//!   as a -> 0 (P by the series when Q > 0.9); otherwise for x < a + 1 the series
//!   P = x^a e^-x / Gamma(a+1) sum_k x^k / ((a+1)...(a+k)) (positive terms) and Q = 1 - P (then at least about 0.13,
//!   no cancellation to speak of), and for x >= a + 1 Legendre's continued fraction
//!   Gamma(a, x) = x^a e^-x / (x+1-a - 1(1-a)/(x+3-a - 2(2-a)/(x+5-a - ...))) by the modified Lentz method and
//!   P = 1 - Q. Both P and Q therefore have full relative accuracy. The series and the fraction take about
//!   sqrt(a) ln(1/eps) terms near x ~ a (some 1000 at a = 10^4, about 20 us; 9 ms at a = 10^8); they stop at
//!   2^-70 relative and give up (NaN) after 10^6 terms, which happens near x ~ a for a beyond about 10^10.
//! - **Gamma(a, x)** for any real a: a > 0 as Gamma(a) Q (Temme's form directly for a < 1, x <= 3/2); a <= 0 by the
//!   continued fraction for x >= 3/2, else Temme's form at a + m in [0, 1) (the limit a -> 0 is E1(x)) and the
//!   recurrence Gamma(b, x) = (Gamma(b+1, x) - x^b e^-x)/b down to a (m steps, each losing at most a few bits for
//!   x <= 3/2; an overflow shown by the bound Gamma(a, x) >= x^a e^(-2-2x)/|a| returns infinity at once).
//!   Gamma(a, 0) = Gamma(a) for a > 0, infinity for a <= 0. **gamma(a, x)** = Gamma(a) P, a > 0.
//! - **I_x(a, b)**: the continued fraction of the incomplete beta function (modified Lentz) for
//!   x <= (a+1)/(a+b+2), with prefactor x^a (1-x)^b / (a B(a, b)), else 1 - I_(1-x)(b, a); the complement is formed
//!   alongside, so I and 1 - I both have full relative accuracy. B_x(a, b) = B(a, b) I_x(a, b) for b > 0 and
//!   x^a sum_k (1-b)_k/k! x^k/(a+k) (positive terms) for b <= 0, x < 1.
//! - **Inverses**: a start (the Wilson-Hilferty cube for gamma with a >= 1/2 and the normal quantile from
//!   [`inverse_erfc`]; (p Gamma(a+1))^(1/a) in the lower tail; x = -ln(q Gamma(a)) + (a-1) ln x iterated in the
//!   upper tail; for beta the forms of Abramowitz-Stegun 26.5.22 and the two power tails), then Halley's iteration
//!   on the residual of whichever of P - p, q - Q (I - p, q - (1 - I)) is smaller, with a bracket that is narrowed
//!   at every evaluation and bisection whenever a step leaves it, so every iteration converges; it stops when the
//!   step is below 2^-30 relative (the cubic convergence then leaves far less than an ulp) or the bracket is a few
//!   ulps wide.
use super::erf::{exp_dd, inverse_erfc, log_dd};
use super::gamma::{gamma, lgamma_re};
use super::rgamma_coefficients;
use crate::dd::{Scaled, DD};
use std::ops::{Add, Div, Mul, Neg, Sub};

/// The relative size at which a series or fraction stops (about 2^-70).
const EPS: f64 = 1e-21;
/// The largest number of series terms or fraction levels.
const MAX_ITER: usize = 1_000_000;

/// a / b with two quotient digits (about 2^-104 relative; cheaper than the three-digit division).
#[inline]
fn ddiv(a: DD, b: DD) -> DD {
    let q1 = a.hi / b.hi;
    let r = a.sub(b.mul_f(q1));
    let q2 = r.hi / b.hi;
    DD::sum(q1, q2)
}

/// The scaled value of a double-double (zero stays zero; subnormals are scaled up first, since normalising them
/// directly would need a factor beyond the double range).
#[inline]
fn sc(x: DD) -> Scaled {
    if x.hi != 0.0 && x.hi.abs() < 1e-290 {
        return Scaled { m: x.ldexp(600), e: -600 }.norm();
    }
    Scaled::new(x)
}

/// A scaled double-double as a double-double (zero below the range).
#[inline]
fn unscale(s: Scaled) -> DD {
    s.m.ldexp(s.e.clamp(-2000, 2000) as i32)
}

/// ln Gamma(a) for a > 0 (a as a double-double).
#[inline]
fn lgam(a: DD) -> DD {
    lgamma_re(a)
}

/// a ln x - x, the logarithm of x^a e^-x.
#[inline]
fn log_pow_exp(a: f64, x: f64) -> DD {
    DD::new(a).mul(log_dd(DD::new(x))).sub(DD::new(x))
}

/// S = sum_k x^k / ((a+1)...(a+k)), positive terms (`None` without convergence).
fn lower_sum(a: f64, x: f64) -> Option<DD> {
    let mut t = DD::ONE;
    let mut s = DD::ONE;
    let mut ak = DD::new(a);
    for _ in 0..MAX_ITER {
        ak = ak.add_f(1.0);
        t = ddiv(t.mul_f(x), ak);
        s = s.add(t);
        if t.hi < EPS * s.hi {
            return Some(s);
        }
    }
    None
}

/// The continued fraction K with Gamma(a, x) = x^a e^-x K (modified Lentz; any real a, x > 0).
fn upper_cf(a: f64, x: f64) -> Option<DD> {
    let tiny = 1e-300;
    let ad = DD::new(a);
    let mut b = DD::new(x).add_f(1.0).sub(ad);
    let mut c = DD::new(1.0 / tiny);
    let mut d = ddiv(DD::ONE, b);
    let mut h = d;
    for i in 1..MAX_ITER {
        let fi = i as f64;
        // a_i = -i (i - a)
        let an = DD::new(fi).sub(ad).mul_f(-fi);
        b = b.add_f(2.0);
        d = an.mul(d).add(b);
        if d.hi.abs() < tiny {
            d = DD::new(tiny);
        }
        c = b.add(ddiv(an, c));
        if c.hi.abs() < tiny {
            c = DD::new(tiny);
        }
        d = ddiv(DD::ONE, d);
        let del = d.mul(c);
        h = h.mul(del);
        if (del.hi - 1.0).abs() + del.lo.abs() < EPS {
            return Some(h);
        }
    }
    None
}

/// expm1(u)/u for a double-double u (1 at u = 0).
fn expm1_ratio(u: DD) -> DD {
    if u.hi.abs() < 0.5 {
        // sum u^k / (k+1)!
        let mut t = DD::ONE;
        let mut s = DD::ONE;
        let mut k = 2.0;
        while k < 40.0 {
            t = t.mul(u).div_f(k);
            s = s.add(t);
            if t.hi.abs() < 1e-33 {
                break;
            }
            k += 1.0;
        }
        return s;
    }
    let e = unscale(exp_dd(u));
    e.add_f(-1.0).div(u)
}

/// (P(a), R(a)) with 1/Gamma(1 + a) = R(a) = 1 + a P(a), |a| <= 1.
fn rgamma1_parts(a: DD) -> (DD, DD) {
    let g = rgamma_coefficients();
    let mut p = DD::ZERO;
    for c in g[1..].iter().rev() {
        p = p.mul(a).add(*c);
    }
    (p, p.mul(a).add_f(1.0))
}

/// Gamma(a, x) by Temme's form for 0 <= a < 1 (a = 0 gives E1(x)) and 0 < x <= 3/2, with R(a) = 1/Gamma(1 + a).
fn upper_temme(a: DD, x: f64) -> (DD, DD) {
    let (p, r) = rgamma1_parts(a);
    let lx = log_dd(DD::new(x));
    let u = lx.mul(a);
    let t1 = ddiv(p, r).neg();
    let t2 = lx.mul(expm1_ratio(u));
    let xa = unscale(exp_dd(u));
    // S = sum_{k>=1} (-x)^k / (k! (a+k))
    let mut term = DD::ONE;
    let mut s = DD::ZERO;
    for k in 1..60 {
        let kf = k as f64;
        term = term.mul_f(-x).div_f(kf);
        let v = ddiv(term, a.add_f(kf));
        s = s.add(v);
        if v.hi.abs() < 1e-34 {
            break;
        }
    }
    (t1.sub(t2).sub(xa.mul(s)), r)
}

/// (P(a, x), Q(a, x)) for a > 0, x > 0, each with full relative accuracy (`None` without convergence).
fn pq(a: f64, x: f64) -> Option<(Scaled, Scaled)> {
    if !(x > 0.0 && x < f64::INFINITY) {
        return None;
    }
    if a < 1.0 && x <= 1.5 {
        let (g, r) = upper_temme(DD::new(a), x);
        let q = g.mul(r).mul_f(a);
        if q.hi <= 0.9 {
            return Some((sc(DD::ONE.sub(q)), sc(q)));
        }
        let p = series_p(a, x)?;
        return Some((p, sc(q)));
    }
    if x < a + 1.0 {
        let p = series_p(a, x)?;
        return Some((p, sc(DD::ONE.sub(unscale(p)))));
    }
    let k = upper_cf(a, x)?;
    let q = exp_dd(log_pow_exp(a, x).sub(lgam(DD::new(a)))).mul_dd(k);
    Some((sc(DD::ONE.sub(unscale(q))), q))
}

/// P(a, x) by the series.
fn series_p(a: f64, x: f64) -> Option<Scaled> {
    let s = lower_sum(a, x)?;
    let l = log_pow_exp(a, x).sub(lgam(DD::new(a).add_f(1.0)));
    Some(exp_dd(l).mul_dd(s))
}

/// The cases of P and Q at the ends of the domain: `Some((P, Q))` for x = 0, x = inf, NaN or a domain error.
fn pq_special(a: f64, x: f64) -> Option<(f64, f64)> {
    if a.is_nan() || x.is_nan() || a <= 0.0 || x < 0.0 || a == f64::INFINITY {
        return Some((f64::NAN, f64::NAN));
    }
    if x == 0.0 {
        return Some((0.0, 1.0));
    }
    if x == f64::INFINITY {
        return Some((1.0, 0.0));
    }
    None
}

/// The regularized lower incomplete gamma function P(a, x) = gamma(a, x) / Gamma(a), a > 0, x >= 0.
pub fn gamma_p(a: f64, x: f64) -> f64 {
    if let Some((p, _)) = pq_special(a, x) {
        return p;
    }
    pq(a, x).map_or(f64::NAN, |v| v.0.to_f64())
}

/// The regularized upper incomplete gamma function Q(a, x) = Gamma(a, x) / Gamma(a) = 1 - P(a, x), a > 0, x >= 0.
pub fn gamma_q(a: f64, x: f64) -> f64 {
    if let Some((_, q)) = pq_special(a, x) {
        return q;
    }
    pq(a, x).map_or(f64::NAN, |v| v.1.to_f64())
}

/// Gamma(a) as a scaled double-double, a > 0.
fn gamma_sc(a: f64) -> Scaled {
    exp_dd(lgam(DD::new(a)))
}

/// Gamma(a, x) as a scaled double-double for a <= 0, 0 < x <= 3/2: Temme's form at a + m and the recurrence down.
fn upper_neg_small_x(a: f64, x: f64) -> Option<Scaled> {
    let m = (-a).ceil();
    if a <= -1.0 {
        // Gamma(a, x) >= x^a e^(-2-2x) / |a| (the integral over [x, x(1 + 1/|a|)]): overflow without the recurrence
        if a * x.ln() - 2.0 - 2.0 * x - (-a).ln() > 710.0 {
            return Some(Scaled { m: DD::new(0.5), e: 2000 });
        }
    }
    if m > 1e7 {
        return None;
    }
    // b0 = a + m in [0, 1), exact in double-double
    let (g, _) = upper_temme(DD::sum(a, m), x);
    let mut g = sc(g);
    let lx = log_dd(DD::new(x));
    let mut j = m;
    while j > 0.0 {
        j -= 1.0;
        // b = a + j
        let b = DD::sum(a, j);
        let xb = exp_dd(lx.mul(b).sub(DD::new(x)));
        g = (g.add(xb.neg())).div(sc(b));
    }
    Some(g)
}

/// The upper incomplete gamma function Gamma(a, x) = int_x^inf t^(a-1) e^-t dt for real a and x >= 0
/// (Gamma(a, 0) = Gamma(a) for a > 0 and infinity for a <= 0; Gamma(0, x) = E1(x)).
pub fn gamma_upper(a: f64, x: f64) -> f64 {
    if a.is_nan() || x.is_nan() || x < 0.0 || a.is_infinite() {
        return f64::NAN;
    }
    if x == f64::INFINITY {
        return 0.0;
    }
    if x == 0.0 {
        return if a > 0.0 { gamma(a) } else { f64::INFINITY };
    }
    let v = if a > 0.0 {
        if a < 1.0 && x <= 1.5 {
            Some(sc(upper_temme(DD::new(a), x).0))
        } else if x >= a + 1.0 {
            upper_cf(a, x).map(|k| exp_dd(log_pow_exp(a, x)).mul_dd(k))
        } else {
            pq(a, x).map(|(_, q)| gamma_sc(a).mul(q))
        }
    } else if x >= 1.5 {
        upper_cf(a, x).map(|k| exp_dd(log_pow_exp(a, x)).mul_dd(k))
    } else {
        upper_neg_small_x(a, x)
    };
    v.map_or(f64::NAN, |s| s.to_f64())
}

/// The lower incomplete gamma function gamma(a, x) = int_0^x t^(a-1) e^-t dt, a > 0, x >= 0.
pub fn gamma_lower(a: f64, x: f64) -> f64 {
    if let Some((p, _)) = pq_special(a, x) {
        return if p.is_nan() || p == 0.0 { p } else { gamma(a) };
    }
    if x < a + 1.0 && !(a < 1.0 && x <= 1.5) {
        // x^a e^-x / a S directly
        return lower_sum(a, x).map_or(f64::NAN, |s| exp_dd(log_pow_exp(a, x)).mul_dd(s.div_f(a)).to_f64());
    }
    pq(a, x).map_or(f64::NAN, |(p, _)| gamma_sc(a).mul(p).to_f64())
}

// ---------------------------------------------------------------- beta

/// ln B(a, b) for a, b > 0.
fn lbeta(a: f64, b: f64) -> DD {
    lgam(DD::new(a)).add(lgam(DD::new(b))).sub(lgam(DD::sum(a, b)))
}

/// The continued fraction of I_x(a, b) (without the prefactor), x and y = 1 - x as double-doubles.
fn beta_cf(x: DD, a: f64, b: f64) -> Option<DD> {
    let tiny = 1e-300;
    let qab = DD::sum(a, b);
    let ad = DD::new(a);
    let mut c = DD::ONE;
    let mut d = DD::ONE.sub(ddiv(qab.mul(x), ad.add_f(1.0)));
    if d.hi.abs() < tiny {
        d = DD::new(tiny);
    }
    d = ddiv(DD::ONE, d);
    let mut h = d;
    for m in 1..MAX_ITER {
        let mf = m as f64;
        let m2 = 2.0 * mf;
        // d_2m = m (b - m) x / ((a + 2m - 1)(a + 2m))
        let num = DD::sum(b, -mf).mul_f(mf).mul(x);
        let aa = ddiv(num, ad.add_f(m2 - 1.0).mul(ad.add_f(m2)));
        d = DD::ONE.add(aa.mul(d));
        if d.hi.abs() < tiny {
            d = DD::new(tiny);
        }
        c = DD::ONE.add(ddiv(aa, c));
        if c.hi.abs() < tiny {
            c = DD::new(tiny);
        }
        d = ddiv(DD::ONE, d);
        h = h.mul(d).mul(c);
        // d_2m+1 = -(a + m)(a + b + m) x / ((a + 2m)(a + 2m + 1))
        let num = ad.add_f(mf).mul(qab.add_f(mf)).mul(x);
        let aa = ddiv(num, ad.add_f(m2).mul(ad.add_f(m2 + 1.0))).neg();
        d = DD::ONE.add(aa.mul(d));
        if d.hi.abs() < tiny {
            d = DD::new(tiny);
        }
        c = DD::ONE.add(ddiv(aa, c));
        if c.hi.abs() < tiny {
            c = DD::new(tiny);
        }
        d = ddiv(DD::ONE, d);
        let del = d.mul(c);
        h = h.mul(del);
        if (del.hi - 1.0).abs() + del.lo.abs() < EPS {
            return Some(h);
        }
    }
    None
}

/// I_x(a, b) without symmetry: x^a y^b / (a B(a, b)) times the fraction.
fn beta_direct(x: DD, y: DD, a: f64, b: f64) -> Option<Scaled> {
    let h = beta_cf(x, a, b)?;
    let l = log_dd(x).mul_f(a).add(log_dd(y).mul_f(b)).sub(log_dd(DD::new(a))).sub(lbeta(a, b));
    Some(exp_dd(l).mul_dd(h))
}

/// (I_x(a, b), 1 - I_x(a, b)) for 0 < x < 1, a, b > 0, each with full relative accuracy.
fn beta_pq(x: f64, a: f64, b: f64) -> Option<(Scaled, Scaled)> {
    // (the logarithms of 0 would be infinities, which the double-double products cannot take)
    if !(x > 0.0 && x < 1.0) {
        return None;
    }
    let xd = DD::new(x);
    let yd = DD::sum(1.0, -x);
    if x <= (a + 1.0) / (a + b + 2.0) {
        let i = beta_direct(xd, yd, a, b)?;
        Some((i, sc(DD::ONE.sub(unscale(i)))))
    } else {
        let j = beta_direct(yd, xd, b, a)?;
        Some((sc(DD::ONE.sub(unscale(j))), j))
    }
}

/// The regularized incomplete beta function I_x(a, b), 0 <= x <= 1, a, b > 0.
pub fn beta_reg(x: f64, a: f64, b: f64) -> f64 {
    if let Some(v) = beta_inc_poly(x, a, b) {
        return v / super::gamma::beta(a, b);
    }
    if x.is_nan() || a.is_nan() || b.is_nan() || !(0.0..=1.0).contains(&x) || a <= 0.0 || b <= 0.0 {
        return f64::NAN;
    }
    if a.is_infinite() || b.is_infinite() {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x == 1.0 {
        return 1.0;
    }
    beta_pq(x, a, b).map_or(f64::NAN, |v| v.0.to_f64())
}

/// B_x(a, b) for x > 1, a > 0 and a positive integer b <= 64, where (1 - t)^(b - 1) is a polynomial: the sum
/// sum_{j < b} C(b - 1, j) (-1)^j x^(a + j)/(a + j) (the integral continued past 1; `None` otherwise).
fn beta_inc_poly(x: f64, a: f64, b: f64) -> Option<f64> {
    if !(x > 1.0 && x.is_finite() && a > 0.0 && a.is_finite() && (1.0..=64.0).contains(&b) && b.fract() == 0.0) {
        return None;
    }
    let n = b as i32 - 1;
    let mut c = 1.0f64;
    let mut s = 0.0f64;
    for j in 0..=n {
        let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
        s += sign * c * x.powf(a + j as f64) / (a + j as f64);
        c = c * (n - j) as f64 / (j + 1) as f64;
    }
    Some(s)
}

/// B_x(a, b) = x^a sum_k (1-b)_k / k! x^k / (a+k) for b < 1, 0 < x < 1 (positive terms, about ln(eps)/ln(x) of them).
fn beta_inc_series(x: f64, a: f64, b: f64) -> Option<Scaled> {
    let mut t = DD::ONE;
    let mut s = DD::ONE.div_f(a);
    for k in 1..MAX_ITER {
        let kf = k as f64;
        t = t.mul(DD::sum(kf, -b)).mul_f(x).div_f(kf);
        let v = ddiv(t, DD::sum(a, kf));
        s = s.add(v);
        if v.hi < EPS * s.hi {
            return Some(exp_dd(log_dd(DD::new(x)).mul_f(a)).mul_dd(s));
        }
    }
    None
}

/// The incomplete beta function B_x(a, b) = int_0^x t^(a-1) (1-t)^(b-1) dt, 0 <= x <= 1, a > 0 (b <= 0 for x < 1
/// by its positive series).
pub fn beta_inc(x: f64, a: f64, b: f64) -> f64 {
    if let Some(v) = beta_inc_poly(x, a, b) {
        return v;
    }
    if x.is_nan() || a.is_nan() || b.is_nan() || !(0.0..=1.0).contains(&x) || a <= 0.0 || a.is_infinite() {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    if b.is_infinite() {
        return f64::NAN;
    }
    if b <= 0.0 {
        if x == 1.0 {
            return f64::INFINITY;
        }
        return beta_inc_series(x, a, b).map_or(f64::NAN, |s| s.to_f64());
    }
    if x == 1.0 {
        return super::gamma::beta(a, b);
    }
    beta_pq(x, a, b).map_or(f64::NAN, |(i, _)| exp_dd(lbeta(a, b)).mul(i).to_f64())
}

// ---------------------------------------------------------------- inverses

/// The variable of the far-field Newton steps: ln x (power-law lower tails), x (exponential upper tails) or
/// ln(1 - x) (power-law upper tails of the beta function).
#[derive(Clone, Copy)]
enum Space {
    LogX,
    Lin,
    LogOneMinus,
}

/// One evaluation for the iteration: the residual f = F - F* (signed so that it increases with x), f', f''/f',
/// rel = ln(F/F*) with the same sign, and the derivative of rel with respect to the variable of the space.
struct Eval {
    f: Scaled,
    df: Scaled,
    ratio: f64,
    rel: f64,
    slope: f64,
}

/// ln of a positive scaled double-double, as a double.
fn ln_sc(s: Scaled) -> f64 {
    s.m.hi.ln() + s.e as f64 * std::f64::consts::LN_2
}

/// Solve f(x) = 0 for an increasing f on (lo, hi) from x0: Newton's method on rel in the given space while the
/// residual is large (|rel| > 0.05), Halley's iteration on f near the root, bisection (geometric for brackets wider
/// than a factor 4) whenever a step leaves the bracket. A root below the smallest subnormal gives 0.
fn solve(x0: f64, mut lo: f64, mut hi: f64, space: Space, eval: impl Fn(f64) -> Option<Eval>) -> f64 {
    let tiny = f64::from_bits(1);
    let mut x = x0;
    if !(x > lo && x < hi) {
        x = if lo == 0.0 && x0 <= 0.0 {
            tiny
        } else if hi.is_finite() {
            0.5 * (lo + hi)
        } else {
            lo.max(0.0) + 1.0
        };
    }
    // |rel| at the ends of the bracket (infinite while an end is a bound of the domain)
    let (mut rlo, mut rhi) = (f64::INFINITY, f64::INFINITY);
    for _ in 0..300 {
        let Some(e) = eval(x) else { return f64::NAN };
        let fs = e.f.m.hi;
        if fs == 0.0 {
            return x;
        }
        if fs < 0.0 {
            lo = x;
            rlo = e.rel.abs();
        } else {
            hi = x;
            rhi = e.rel.abs();
            if x == tiny {
                return 0.0;
            }
        }
        let far = e.rel.abs() > 0.05 && e.slope.is_finite() && e.slope != 0.0;
        let (mut xn, halley, dx) = if far {
            let t = -e.rel / e.slope;
            let xn = match space {
                Space::LogX => x * t.exp(),
                Space::Lin => x + t,
                Space::LogOneMinus => 1.0 - (1.0 - x) * t.exp(),
            };
            (xn, false, 0.0)
        } else {
            let u = e.f.div(e.df).to_f64();
            let den = 1.0 - 0.5 * u * e.ratio;
            let dx = if den > 0.25 && den.is_finite() { u / den } else { u };
            (x - dx, true, dx)
        };
        let scale = if hi <= 1.0 { x.min(1.0 - x) } else { x };
        if halley && dx.abs() <= 1e-9 * scale {
            // converged (the cubic step leaves far less than an ulp; on (0, 1) relative to the nearer end)
            return if xn > lo && xn < hi { xn } else { x };
        }
        if !(xn > lo && xn < hi && xn.is_finite()) {
            xn = if lo == 0.0 && xn <= 0.0 && x > tiny {
                // a step below zero: try the bottom of the range
                tiny
            } else if hi.is_finite() {
                if lo > 0.0 && hi > 4.0 * lo {
                    // the geometric mean (as a product of roots: lo hi may underflow)
                    lo.sqrt() * hi.sqrt()
                } else {
                    0.5 * (lo + hi)
                }
            } else {
                2.0 * x.max(1.0)
            };
        }
        let mid = 0.5 * (lo + hi);
        if hi.is_finite() && !(mid > lo && mid < hi) {
            // adjacent doubles: the end with the smaller residual
            return if rlo <= rhi { lo } else { hi };
        }
        if !(xn > lo && xn < hi) {
            xn = mid;
        }
        x = xn;
    }
    x
}

/// The normal quantile z with Phi(z) = p, from p and q = 1 - p (the smaller one exact).
fn normal_quantile(p: f64, q: f64) -> f64 {
    let s2 = std::f64::consts::SQRT_2;
    if p <= q {
        -s2 * inverse_erfc(2.0 * p)
    } else {
        s2 * inverse_erfc(2.0 * q)
    }
}

/// A start for the inverse of P(a, .) at p (q = 1 - p).
fn gamma_start(a: f64, p: f64, q: f64) -> f64 {
    let lg = lgam(DD::new(a)).to_f64();
    // the lower tail: P ~ x^a / Gamma(a+1)
    let lower = ((p.ln() + lg + a.ln()) / a).exp();
    if a >= 0.5 {
        let z = normal_quantile(p, q);
        let c = 1.0 - 1.0 / (9.0 * a) + z / (3.0 * a.sqrt());
        if c > 0.3 {
            return a * c * c * c;
        }
        return lower;
    }
    if lower < 1.0 {
        return lower;
    }
    // the upper tail: Q ~ x^(a-1) e^-x / Gamma(a)
    let t = -(q.ln() + lg);
    let mut x = t.max(1.0);
    for _ in 0..5 {
        x = (t + (a - 1.0) * x.ln()).max(0.5);
    }
    x
}

/// The inverse of P(a, .) given p and q = 1 - p, the smaller one exact.
fn inverse_gamma(a: f64, p: f64, q: f64) -> f64 {
    if a.is_nan() || p.is_nan() || q.is_nan() || a <= 0.0 || a.is_infinite() || p < 0.0 || q < 0.0 {
        return f64::NAN;
    }
    if p == 0.0 {
        return 0.0;
    }
    if q == 0.0 {
        return f64::INFINITY;
    }
    let lg = lgam(DD::new(a));
    let use_p = p <= q;
    let (sp, sq) = (sc(DD::new(p)), sc(DD::new(q)));
    let eval = |x: f64| {
        let (pv, qv) = pq(a, x)?;
        // the density x^(a-1) e^-x / Gamma(a)
        let lx = log_dd(DD::new(x));
        let df = exp_dd(DD::sum(a, -1.0).mul(lx).sub(DD::new(x)).sub(lg));
        let ratio = (a - 1.0) / x - 1.0;
        Some(if use_p {
            // rel = ln(P/p) against ln x: slope x P'/P
            let slope = df.mul(sc(DD::new(x))).div(pv).to_f64();
            Eval { f: pv.add(sp.neg()), df, ratio, rel: ln_sc(pv.div(sp)), slope }
        } else {
            // rel = ln(q/Q) against x: slope -Q'/Q
            let slope = df.div(qv).to_f64();
            Eval { f: sq.add(qv.neg()), df, ratio, rel: ln_sc(sq.div(qv)), slope }
        })
    };
    let space = if use_p { Space::LogX } else { Space::Lin };
    solve(gamma_start(a, p, q), 0.0, f64::INFINITY, space, eval)
}

/// The inverse of the regularized lower incomplete gamma function: x >= 0 with P(a, x) = p, 0 <= p <= 1.
pub fn inverse_gamma_p(a: f64, p: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    // the complement is exact for p >= 1/2 (and not needed below)
    inverse_gamma(a, p, if p >= 0.5 { 1.0 - p } else { 1.0 })
}

/// The inverse of the regularized upper incomplete gamma function: x >= 0 with Q(a, x) = q, 0 <= q <= 1.
pub fn inverse_gamma_q(a: f64, q: f64) -> f64 {
    if !(0.0..=1.0).contains(&q) {
        return f64::NAN;
    }
    inverse_gamma(a, if q >= 0.5 { 1.0 - q } else { 1.0 }, q)
}

/// A start for the inverse of I_.(a, b) at p (q = 1 - p).
fn beta_start(a: f64, b: f64, p: f64, q: f64) -> f64 {
    if a >= 1.0 && b >= 1.0 {
        // Abramowitz-Stegun 26.5.22 with the normal quantile
        let z = -normal_quantile(p, q);
        let al = (z * z - 3.0) / 6.0;
        let h = 2.0 / (1.0 / (2.0 * a - 1.0) + 1.0 / (2.0 * b - 1.0));
        let w = z * (al + h).sqrt() / h
            - (1.0 / (2.0 * b - 1.0) - 1.0 / (2.0 * a - 1.0)) * (al + 5.0 / 6.0 - 2.0 / (3.0 * h));
        let x = a / (a + b * (2.0 * w).exp());
        if x > 0.0 && x < 1.0 {
            return x;
        }
    }
    // the two power tails: I ~ x^a / (a B), 1 - I ~ (1-x)^b / (b B)
    let lb = lbeta(a, b).to_f64();
    let lo = ((p.ln() + a.ln() + lb) / a).exp();
    let hi = ((q.ln() + b.ln() + lb) / b).exp();
    let t = (a * (a / (a + b)).ln() - a.ln() - lb).exp();
    let u = (b * (b / (a + b)).ln() - b.ln() - lb).exp();
    if p < t / (t + u) {
        lo
    } else {
        1.0 - hi
    }
}

/// The inverse of I_.(a, b) given p and q = 1 - p, the smaller one exact.
fn inverse_beta(a: f64, b: f64, p: f64, q: f64) -> f64 {
    if [a, b, p, q].iter().any(|v| v.is_nan()) || a <= 0.0 || b <= 0.0 || a.is_infinite() || b.is_infinite() {
        return f64::NAN;
    }
    if p == 0.0 {
        return 0.0;
    }
    if q == 0.0 {
        return 1.0;
    }
    let lb = lbeta(a, b);
    let use_p = p <= q;
    let (sp, sq) = (sc(DD::new(p)), sc(DD::new(q)));
    let eval = |x: f64| {
        let (iv, jv) = beta_pq(x, a, b)?;
        let y = DD::sum(1.0, -x);
        // the density x^(a-1) (1-x)^(b-1) / B(a, b)
        let l = log_dd(DD::new(x)).mul_f(a - 1.0).add(log_dd(y).mul_f(b - 1.0)).sub(lb);
        let df = exp_dd(l);
        let ratio = (a - 1.0) / x - (b - 1.0) / y.hi;
        Some(if use_p {
            // rel = ln(I/p) against ln x: slope x I'/I
            let slope = df.mul(sc(DD::new(x))).div(iv).to_f64();
            Eval { f: iv.add(sp.neg()), df, ratio, rel: ln_sc(iv.div(sp)), slope }
        } else {
            // rel = ln(q/J), J = 1 - I, against ln(1 - x): slope -(1-x) (-J')/J
            let slope = -df.mul(sc(y)).div(jv).to_f64();
            Eval { f: sq.add(jv.neg()), df, ratio, rel: ln_sc(sq.div(jv)), slope }
        })
    };
    let space = if use_p { Space::LogX } else { Space::LogOneMinus };
    let x = solve(beta_start(a, b, p, q), 0.0, 1.0, space, eval);
    x.clamp(0.0, 1.0)
}

/// The inverse of the regularized incomplete beta function: 0 <= x <= 1 with I_x(a, b) = p, 0 <= p <= 1.
pub fn inverse_beta_reg(a: f64, b: f64, p: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    inverse_beta(a, b, p, if p >= 0.5 { 1.0 - p } else { 1.0 })
}

#[cfg(test)]
#[allow(clippy::excessive_precision)] // published values to more digits than a double holds
mod tests {
    use super::*;

    fn close(got: f64, want: f64, ulps: f64) -> bool {
        ((got - want) / want).abs() <= ulps * f64::EPSILON
    }

    #[test]
    fn known_values() {
        // Q(n, x) = e^-x sum_{k<n} x^k/k!
        let x: f64 = 2.5;
        let want = (-x).exp() * (1.0 + x + x * x / 2.0);
        assert!(close(gamma_q(3.0, x), want, 4.0), "{} {}", gamma_q(3.0, x), want);
        assert!(close(gamma_p(1.0, 0.5), -(-0.5f64).exp_m1(), 2.0));
        // Gamma(1/2, 1) = sqrt(pi) erfc(1)
        let g = gamma_upper(0.5, 1.0);
        assert!(close(g, std::f64::consts::PI.sqrt() * 0.157_299_207_050_285_130_658_779_364_917_390_740_703_933, 4.0));
        // E1(1) = 0.21938393439552027368
        assert!(close(gamma_upper(0.0, 1.0), 0.219_383_934_395_520_273_677_163_775_460_121_6, 4.0));
        assert!(close(beta_reg(0.3, 2.5, 1.0), 0.3f64.powf(2.5), 4.0));
        assert!(close(beta_reg(0.3, 1.0, 3.5), 1.0 - 0.7f64.powf(3.5), 4.0));
        assert!(close(inverse_gamma_p(3.0, gamma_p(3.0, 2.0)), 2.0, 8.0));
        assert!(close(inverse_beta_reg(2.0, 3.0, beta_reg(0.4, 2.0, 3.0)), 0.4, 8.0));
    }
}
