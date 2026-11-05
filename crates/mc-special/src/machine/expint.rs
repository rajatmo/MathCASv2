//! Exponential, logarithmic, sine and cosine integrals of machine reals: double-double inside, one rounding.
//!
//! - **E1(t), t > 0:** t <= 4 the series -gamma - ln t - sum (-t)^k/(k k!) (terms below e^4, so at most 12 of the
//!   106 bits cancel); t > 4 the continued fraction E1(t) = e^-t / (t + 1 - 1/(t + 3 - 4/(t + 5 - ...))) evaluated
//!   backwards from a depth n = 360/t + 40/sqrt(t) + 14 (measured: the truncation error is below 2^-110 there),
//!   with e^-t as a scaled double-double so that E1 underflows only at the final rounding.
//! - **Ei(t), t > 0:** t < 50 the positive series gamma + ln t + sum t^k/(k k!); t >= 50 the asymptotic series
//!   (e^t/t) sum k!/t^k up to its smallest term (below 2^-68 relative from t = 50). Ei(x) = -E1(-x) for x < 0.
//! - **Zeros:** near the zero 0.3725... of Ei, the zero 1.4513... of li, the zeros 0.6165... and 3.3842... of Ci
//!   and 0.5238... of Chi (|x - z| <= 2^-7) the functions are their Taylor series at the zero, sum_k c_k (x - z)^k,
//!   with z as a triple-double (so x - z is exact to about 2^-160) and 24 coefficients; the zeros (Newton steps on
//!   the ball functions at 320 bits) and the coefficients are computed once, at first use (decision SF.5). So the
//!   relative accuracy holds up to the zeros.
//! - **li(x) = Ei(ln x)** with ln x = log1p(x - 1) near 1 (x - 1 is exact there).
//! - **E_nu(x), x > 0:** integers -1000 <= nu = -j <= 0 the finite sum (e^-x/x) sum_l j!/((j-l)! x^l) (also real
//!   for x < 0, there up to j = 10^7); nu = 1: E1; the continued fraction E_nu(x) = e^-x/(x + nu - 1 nu/(x + nu + 2 - 2(nu+1)/(x + nu + 4 -
//!   ...))) for x > 4 (x >= 2 - nu when nu < 0), or for nu >= 40 at any x (depth 3600/nu + 16 there); otherwise
//!   for nu < 1/2 E_nu(x) = x^(nu-1) Gamma(1-nu) - e^-x sum_k x^k/(1-nu)_(k+1) (positive terms), and for
//!   1/2 <= nu < 40 the series of DLMF 8.19.10 with its two terms singular at the integer m + 1 = round(nu) joined:
//!   with eps = nu - m - 1, (-1)^(m+1) (x^m/m!) S - sum_{k != m} (-x)^k/(k! (k+1-nu)), where
//!   S = expm1(D)/eps, D = eps ln x + ln Gamma(1 - eps) - sum_{j<=m} log1p(eps/j) (each term computed relative to
//!   eps, so nu near and at integers has full accuracy; S = ln x - psi(m+1) at eps = 0).
//! - **Si, Ci, x <= 6:** their power series (terms below e^6); **x > 6:** e^(ix) E1(ix) = 1/T = g(x) - i f(x) by
//!   the complex continued fraction (depth 800/x + 12, measured), with amplitude A = 1/|T| and phase
//!   theta = atan(g/f); then Ci(x) = A sin(x - theta) and Si(x) = pi/2 - A cos(x - theta), the phase x - theta reduced
//!   by k pi with a triple-double pi (x < 2^45; beyond, by ball arithmetic). Near a zero of Ci the phase is small
//!   and accurate to about 2^-104/x absolute, so the relative error stays about 2^-53/(x^2 u) for a double at u
//!   ulps from the zero (rather than 2^-52/(x u) for f sin x - g cos x).
//! - **Shi, Chi:** |x| <= 8 their positive series, beyond (Ei(x) +- E1(x))/2.
//!
//! Domains: Ei(0) = -inf; li(x < 0), Ci(x < 0), Chi(x < 0) and E_nu(x < 0) (except integers nu <= 0) are complex:
//! NaN. li(0) = 0, li(1) = -inf, Ci(0) = Chi(0) = -inf, E_nu(0) = 1/(nu - 1) for nu > 1 and +inf otherwise.
use super::float_dd;
use super::gamma::{digamma_dd, gamma_scaled, lgamma_re};
use crate::ball::expint as bx;
use crate::ball::{f64_of, float_of};
use crate::dd::{exp_scaled, log, sin_cos_pi_any, Scaled, DD, LN2, PI};
use mc_arith::num::{Ball, Float, Nat};
use mc_numeric::elementary as el;
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::OnceLock;

/// The half-width of the Taylor expansions at the zeros.
const ZERO_RADIUS: f64 = 1.0 / 128.0;
/// The number of Taylor coefficients at a zero ((2^-7 / 0.37)^24 < 2^-130).
const ZERO_TERMS: usize = 24;
/// The working precision of the one-time constants.
const W: u64 = 320;

/// f(z + d) = sum_{k>=1} c_k d^k at a zero z.
struct ZeroExpansion {
    z: [f64; 3],
    c: Vec<DD>,
}

impl ZeroExpansion {
    fn near(&self, x: f64) -> bool {
        (x - self.z[0]).abs() <= ZERO_RADIUS
    }
    fn eval(&self, x: f64) -> DD {
        // x - z[0] is exact (Sterbenz), the rest of z is subtracted in double-double
        let d = DD::new(x - self.z[0]).add_f(-self.z[1]).add_f(-self.z[2]);
        let mut s = DD::ZERO;
        for c in self.c.iter().rev() {
            s = s.mul(d).add(*c);
        }
        s.mul(d)
    }
}

struct Consts {
    euler: DD,
    pi3: [f64; 3],
    ei_zero: ZeroExpansion,
    li_zero: ZeroExpansion,
    ci_zeros: [ZeroExpansion; 2],
    chi_zero: ZeroExpansion,
}

/// A float as three doubles (hi + mid + lo, about 160 bits).
fn triple(f: &Float) -> [f64; 3] {
    let a = f64_of(f);
    let r = f.sub_exact(&float_of(a));
    let b = f64_of(&r);
    let r = r.sub_exact(&float_of(b));
    [a, b, f64_of(&r)]
}

/// Newton's iteration z <- z - f(z)/f'(z) from a double guess, at W bits.
fn newton(f: impl Fn(&Ball) -> Option<Ball>, df: impl Fn(&Ball) -> Option<Ball>, guess: f64) -> Ball {
    let mut z = Ball::exact(float_of(guess));
    for _ in 0..7 {
        let step = f(&z).and_then(|v| v.div(&df(&z)?, W)).expect("a Newton step at a simple zero");
        z = Ball::exact(z.sub(&step, W).mid);
    }
    z
}

/// The expansion of f with f' = g/x at the zero z, from the Taylor coefficients g_j = g^(j)(z)/j!.
fn expansion_g_over_x(z: &Ball, g: &[Ball]) -> ZeroExpansion {
    let n = ZERO_TERMS;
    // 1/(z + d) = sum (-1)^i d^i / z^(i+1)
    let rz = Ball::from_i64(1).div(z, W).expect("z != 0");
    let mut inv = vec![rz.clone()];
    for i in 1..n {
        inv.push(inv[i - 1].mul(&rz, W).neg());
    }
    let mut c = Vec::with_capacity(n);
    for k in 1..=n {
        // h_(k-1) = sum_{j<k} g_j inv_(k-1-j); c_k = h_(k-1)/k
        let mut h = Ball::from_i64(0);
        for j in 0..k {
            h = h.add(&g[j].mul(&inv[k - 1 - j], W), W);
        }
        c.push(float_dd(&h.div(&Ball::from_i64(k as i64), W).expect("k > 0").mid));
    }
    ZeroExpansion { z: triple(&z.mid), c }
}

/// g_j = g^(j)(z)/j! for g with derivatives cycling through `cycle` (values at z).
fn taylor_cycle(cycle: &[Ball]) -> Vec<Ball> {
    let mut out = Vec::with_capacity(ZERO_TERMS);
    let mut fact = Ball::from_i64(1);
    for j in 0..ZERO_TERMS {
        if j > 0 {
            fact = fact.mul(&Ball::from_i64(j as i64), W);
        }
        out.push(cycle[j % cycle.len()].div(&fact, W).expect("nonzero factorial"));
    }
    out
}

fn build_consts() -> Consts {
    let _scope = mc_arith::num::StopScope::enter(None);
    let one = Ball::from_i64(1);
    let euler = float_dd(&crate::ball::euler_gamma(W).expect("Euler's constant").mid);
    let pi3 = triple(&crate::ball::pi(W).expect("pi").mid);
    let ez = |x: &Ball| el::exp(x, W);
    // Ei: f' = e^x/x
    let x0 = newton(|x| bx::ei_point(x, W), |x| ez(x)?.div(x, W), 0.372_507_410_781_366_6);
    let e0 = ez(&x0).expect("exp");
    let ei_zero = expansion_g_over_x(&x0, &taylor_cycle(std::slice::from_ref(&e0)));
    // li: zero mu = e^x0, f' = 1/ln x; ln(mu + d) = x0 + sum_{i>=1} (-1)^(i+1) (d/mu)^i / i, then 1/ln by division
    let mu = e0.clone();
    let rmu = one.div(&mu, W).expect("mu > 0");
    let mut l = vec![x0.clone()];
    let mut pw = one.clone();
    for i in 1..ZERO_TERMS {
        pw = pw.mul(&rmu, W);
        let t = pw.div(&Ball::from_i64(i as i64), W).expect("i > 0");
        l.push(if i % 2 == 1 { t } else { t.neg() });
    }
    let mut r: Vec<Ball> = vec![one.div(&l[0], W).expect("x0 != 0")];
    for j in 1..ZERO_TERMS {
        let mut s = Ball::from_i64(0);
        for i in 1..=j {
            s = s.add(&l[i].mul(&r[j - i], W), W);
        }
        r.push(s.mul(&r[0], W).neg());
    }
    let li_c =
        (1..=ZERO_TERMS).map(|k| float_dd(&r[k - 1].div(&Ball::from_i64(k as i64), W).expect("k > 0").mid)).collect();
    let li_zero = ZeroExpansion { z: triple(&mu.mid), c: li_c };
    // Ci: f' = cos x / x; derivatives of cos: cos, -sin, -cos, sin
    let ci_at = |guess: f64| {
        let z = newton(|x| bx::ci_point(x, W), |x| el::cos(x, W)?.div(x, W), guess);
        let (s, c) = el::sin_cos(&z, W).expect("sin, cos");
        expansion_g_over_x(&z, &taylor_cycle(&[c.clone(), s.neg(), c.neg(), s]))
    };
    let ci_zeros = [ci_at(0.616_505_485_620_716_2), ci_at(3.384_180_422_551_186)];
    // Chi: f' = cosh x / x
    let z = newton(|x| bx::chi_point(x, W), |x| el::cosh(x, W)?.div(x, W), 0.523_822_571_389_864_4);
    let (sh, ch) = (el::sinh(&z, W).expect("sinh"), el::cosh(&z, W).expect("cosh"));
    let chi_zero = expansion_g_over_x(&z, &taylor_cycle(&[ch, sh]));
    Consts { euler, pi3, ei_zero, li_zero, ci_zeros, chi_zero }
}

fn consts() -> &'static Consts {
    static C: OnceLock<Consts> = OnceLock::new();
    C.get_or_init(build_consts)
}

// ---------------------------------------------------------------- double-double helpers

/// ln x for x > 0, also for subnormal x (scaled first).
fn ln_dd(x: DD) -> DD {
    if x.hi < 1e-290 {
        return log(x.ldexp(600)).sub(LN2.mul_f(600.0));
    }
    log(x)
}

/// log1p(u) for |u| <= 1/4, relative to u: 2 atanh(u/(2 + u)).
fn log1p_dd(u: DD) -> DD {
    let s = u.div(u.add_f(2.0));
    let s2 = s.sqr();
    let (mut t, mut sum, mut k) = (s, s, 1.0);
    loop {
        t = t.mul(s2);
        k += 2.0;
        let term = t.div_f(k);
        sum = sum.add(term);
        if term.hi.abs() <= sum.hi.abs() * 1e-35 || !term.hi.is_finite() {
            break;
        }
    }
    sum.mul_f(2.0)
}

/// e^d - 1 relative to d (Taylor series for |d| < 1/2).
fn expm1_dd(d: DD) -> DD {
    if d.hi.abs() >= 0.5 {
        return crate::dd::exp(d).add_f(-1.0);
    }
    let (mut term, mut sum) = (d, d);
    let mut n = 2.0;
    loop {
        term = term.mul(d).div_f(n);
        sum = sum.add(term);
        if term.hi.abs() <= sum.hi.abs() * 1e-35 || !term.hi.is_finite() {
            break;
        }
        n += 1.0;
    }
    sum
}

/// atan(r) for |r| <= 1: halved to |r| <= 1/10, then its Taylor series.
fn atan_dd(r: DD) -> DD {
    let mut r = r;
    let mut m = 1.0;
    while r.hi.abs() > 0.1 {
        r = r.div(r.sqr().add_f(1.0).sqrt().add_f(1.0));
        m *= 2.0;
    }
    let r2 = r.sqr();
    let (mut t, mut sum, mut k) = (r, r, 1.0);
    loop {
        t = t.mul(r2).neg();
        k += 2.0;
        let term = t.div_f(k);
        sum = sum.add(term);
        if term.hi.abs() <= sum.hi.abs() * 1e-35 || term.hi.is_nan() {
            break;
        }
    }
    sum.mul_f(m)
}

/// The depth of the continued fractions at x (and order nu for E_nu).
fn cf_depth(x: f64, nu: f64) -> usize {
    let a = 360.0 / x + 40.0 / x.sqrt() + 14.0;
    let b = if nu >= 40.0 { 3600.0 / nu + 16.0 } else { f64::INFINITY };
    a.min(b).min(1e6).ceil() as usize
}

/// E_nu(x) = e^-x / T by the continued fraction, T evaluated backwards.
fn en_cf(nu: DD, x: DD) -> Scaled {
    let n = cf_depth(x.hi, nu.hi);
    let xn = x.add(nu);
    let mut t = xn.add_f(2.0 * n as f64);
    for k in (1..=n).rev() {
        let kf = k as f64;
        let a = nu.add_f(kf - 1.0).mul_f(kf);
        t = xn.add_f(2.0 * (kf - 1.0)).sub(a.div(t));
    }
    exp_scaled(x.neg()).div(Scaled::new(t))
}

// ---------------------------------------------------------------- E1 and Ei

/// E1(t) for t > 0.
fn e1_pos(t: DD) -> Scaled {
    if t.hi > 4.0 {
        return en_cf(DD::ONE, t);
    }
    let mt = t.neg();
    let (mut u, mut s) = (DD::ONE, DD::ZERO);
    let mut k = 1.0;
    loop {
        u = u.mul(mt).div_f(k);
        let term = u.div_f(k);
        s = s.add(term);
        if (k > t.hi && term.hi.abs() < 1e-36) || !term.hi.is_finite() {
            break;
        }
        k += 1.0;
    }
    Scaled::new(consts().euler.neg().sub(ln_dd(t)).sub(s))
}

/// Ei(t) for t > 0.
fn ei_pos(t: DD) -> Scaled {
    if t.hi < 50.0 {
        let (mut u, mut s) = (DD::ONE, DD::ZERO);
        let mut k = 1.0;
        loop {
            u = u.mul(t).div_f(k);
            let term = u.div_f(k);
            s = s.add(term);
            if (k > t.hi && term.hi < 1e-34 * s.hi.max(1.0)) || !term.hi.is_finite() {
                break;
            }
            k += 1.0;
        }
        return Scaled::new(consts().euler.add(ln_dd(t)).add(s));
    }
    let r = t.recip();
    let (mut u, mut s) = (DD::ONE, DD::ONE);
    let mut k = 1.0;
    while k < t.hi {
        let next = u.mul(r).mul_f(k);
        if next.hi > u.hi || next.hi < 1e-34 {
            break;
        }
        u = next;
        s = s.add(u);
        k += 1.0;
    }
    exp_scaled(t).mul(Scaled::new(s.mul(r)))
}

/// Ei(x) for a double-double x != 0.
fn ei_dd(x: DD) -> Scaled {
    if x.hi > 0.0 {
        ei_pos(x)
    } else {
        e1_pos(x.neg()).neg()
    }
}

/// The exponential integral Ei(x) = -PV int_-x^inf e^-t/t dt for a machine real (Ei(0) = -inf; x < 0: -E1(-x)).
pub fn exp_integral_ei(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x == f64::INFINITY {
        return x;
    }
    if x == f64::NEG_INFINITY {
        return -0.0;
    }
    let c = consts();
    if c.ei_zero.near(x) {
        return c.ei_zero.eval(x).to_f64();
    }
    ei_dd(DD::new(x)).to_f64()
}

// ---------------------------------------------------------------- E_nu

/// E_-j(x) = (e^-x/x) sum_{l<=j} j!/((j-l)! x^l), x != 0.
fn en_negint(j: f64, x: DD) -> Scaled {
    let r = Scaled::new(x.recip());
    let (mut t, mut s) = (Scaled::new(DD::ONE), Scaled::new(DD::ONE));
    let mut l = 1.0;
    while l <= j {
        t = t.mul(r).mul_dd(DD::new(j - l + 1.0));
        s = s.add(t);
        l += 1.0;
    }
    exp_scaled(x.neg()).mul(s).mul(r)
}

/// E_nu(x) = x^(nu-1) Gamma(1-nu) - e^-x sum_k x^k/(1-nu)_(k+1) for nu < 1/2, x > 0.
fn en_pos_series(nu: f64, x: DD) -> Scaled {
    let a = DD::sum(1.0, -nu);
    let lead = exp_scaled(ln_dd(x).mul(DD::sum(nu, -1.0))).mul(gamma_scaled(a));
    let mut term = a.recip();
    let mut s = term;
    let mut k = 1.0;
    loop {
        term = term.mul(x).div(a.add_f(k));
        s = s.add(term);
        if (k > x.hi && term.hi.abs() < 1e-35 * s.hi.abs()) || !term.hi.is_finite() {
            break;
        }
        k += 1.0;
    }
    lead.add(exp_scaled(x.neg()).mul_dd(s).neg())
}

/// E_nu(x) for 1/2 <= nu < 40 and 0 < x <= 4 by the joined series of the module docs.
fn en_joined_series(nu: f64, x: DD) -> Scaled {
    let m = nu.round() - 1.0;
    let eps = nu - (m + 1.0); // exact (Sterbenz)
    let lx = ln_dd(x);
    let s = if eps == 0.0 {
        lx.sub(digamma_dd(DD::new(m + 1.0)))
    } else {
        let e = DD::new(eps);
        // L = ln x + ln Gamma(1 - eps)/eps - sum_j log1p(eps/j)/eps; D = eps L
        let mut l = lx.add(lgamma_re(DD::sum(1.0, -eps)).div(e));
        let mut j = 1.0;
        while j <= m {
            l = l.sub(log1p_dd(e.div_f(j)).div(e));
            j += 1.0;
        }
        let d = l.mul(e);
        if d.hi == 0.0 {
            l
        } else {
            l.mul(expm1_dd(d).div(d))
        }
    };
    // (-1)^(m+1) x^m/m! S
    let mut p = Scaled::new(s);
    let mut k = 1.0;
    while k <= m {
        p = p.mul_dd(x.div_f(k));
        k += 1.0;
    }
    let sing = if (m as i64) % 2 == 0 { p.neg() } else { p };
    // sum_{k != m} (-x)^k/(k! (k - m - eps))
    let mx = x.neg();
    let mut u = DD::ONE;
    let mut sum = DD::ZERO;
    let mut k = 0.0;
    loop {
        if k != m {
            let term = u.div(DD::new(k - m).add_f(-eps));
            sum = sum.add(term);
            if (k > m && k > x.hi && term.hi.abs() < 1e-36) || !term.hi.is_finite() {
                break;
            }
        }
        k += 1.0;
        u = u.mul(mx).div_f(k);
    }
    sing.add(Scaled::new(sum).neg())
}

/// E_nu(x) for x > 0, finite nu.
fn en_pos(nu: f64, x: DD) -> Scaled {
    let is_int = nu == nu.trunc();
    if is_int && (-1000.0..=0.0).contains(&nu) {
        return en_negint(-nu, x);
    }
    if nu == 1.0 {
        return e1_pos(x);
    }
    let cf = nu >= 40.0 || (x.hi > 4.0 && (nu >= 0.0 || x.hi >= 2.0 - nu));
    if cf {
        en_cf(DD::new(nu), x)
    } else if nu < 0.5 {
        en_pos_series(nu, x)
    } else {
        en_joined_series(nu, x)
    }
}

/// The generalized exponential integral E_nu(x) = int_1^inf e^(-x t) t^-nu dt for machine reals: x > 0 any real
/// nu; x = 0: 1/(nu - 1) for nu > 1, +inf otherwise; x < 0 only for integers nu <= 0 (real there), else NaN.
pub fn exp_integral_e(nu: f64, x: f64) -> f64 {
    if nu.is_nan() || x.is_nan() || nu.is_infinite() {
        return f64::NAN;
    }
    if x == 0.0 {
        return if nu > 1.0 { 1.0 / (nu - 1.0) } else { f64::INFINITY };
    }
    if x == f64::INFINITY {
        return 0.0;
    }
    let neg_int = nu == nu.trunc() && nu <= 0.0;
    if x < 0.0 {
        if !neg_int {
            return f64::NAN;
        }
        if x == f64::NEG_INFINITY {
            return f64::NEG_INFINITY;
        }
        if nu < -1e7 {
            return f64::NAN;
        }
        return en_negint(-nu, DD::new(x)).to_f64();
    }
    en_pos(nu, DD::new(x)).to_f64()
}

// ---------------------------------------------------------------- li

/// The logarithmic integral li(x) = PV int_0^x dt/ln t for x >= 0 (li(0) = 0, li(1) = -inf; x < 0 NaN).
pub fn log_integral(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x == 1.0 {
        return f64::NEG_INFINITY;
    }
    if x == f64::INFINITY {
        return x;
    }
    let c = consts();
    if c.li_zero.near(x) {
        return c.li_zero.eval(x).to_f64();
    }
    let t = if (0.75..1.25).contains(&x) { log1p_dd(DD::new(x - 1.0)) } else { ln_dd(DD::new(x)) };
    ei_dd(t).to_f64()
}

// ---------------------------------------------------------------- Si, Ci

/// The amplitude A = |e^(ix) E1(ix)| and phase theta = atan(g/f) for x > 6, by the complex continued fraction
/// (depth 800/x + 12: measured, the truncation error is below 2^-110 there).
fn amp_phase(x: f64) -> (DD, DD) {
    if x > 1e100 {
        // f = 1/x (1 + O(x^-2)), g = O(x^-2): A = 1/x and theta = 1/x to far below an ulp
        let r = DD::new(x).ldexp(-600).recip().ldexp(-600);
        return (r, r);
    }
    let n = (800.0 / x + 12.0).ceil() as usize;
    let xd = DD::new(x);
    let mut tr = DD::new(2.0 * n as f64 + 1.0);
    let mut ti = xd;
    for k in (1..=n).rev() {
        let kf = k as f64;
        let q = DD::new(kf * kf).div(tr.sqr().add(ti.sqr()));
        let (nr, ni) = (DD::new(2.0 * kf - 1.0).sub(q.mul(tr)), xd.add(q.mul(ti)));
        tr = nr;
        ti = ni;
    }
    // |T| = ti sqrt(1 + r^2), r = tr/ti (no overflow for huge x)
    let r = tr.div(ti);
    let amp = ti.mul(r.sqr().add_f(1.0).sqrt()).recip();
    (amp, atan_dd(r))
}

/// sin(x - theta) and cos(x - theta) for x > 6 with an exact reduction of x by multiples of pi.
fn sin_cos_shift(x: f64, theta: DD) -> (DD, DD) {
    let c = consts();
    let k = ((x - theta.hi) / PI.hi).round();
    let (r, odd) = if k < 3.5e13 {
        let r = DD::new(x).sub(DD::new(c.pi3[0]).mul_f(k)).sub(DD::new(c.pi3[1]).mul_f(k)).add_f(-c.pi3[2] * k);
        (r, k.rem_euclid(2.0) == 1.0)
    } else {
        reduce_big(x)
    };
    let (s, co) = sin_cos_pi_any(r.sub(theta).div(PI));
    if odd {
        (s.neg(), co.neg())
    } else {
        (s, co)
    }
}

/// x - k pi for large x (k = round(x/pi)) by ball arithmetic, and whether k is odd.
fn reduce_big(x: f64) -> (DD, bool) {
    let _scope = mc_arith::num::StopScope::enter(None);
    let w = 1200;
    let pi = crate::ball::pi(w).expect("pi");
    let xb = Ball::exact(float_of(x));
    let q = xb.div(&pi, w).expect("pi > 0").mid;
    // k = floor(q + 1/2): q > 2^40 here, its fraction bits are below the binary point
    let qh = q.add_exact(&Float::new(false, Nat::from_u64(1), -1));
    let k = if qh.exp >= 0 { qh } else { Float::new(false, qh.man.shr((-qh.exp) as u64), 0) };
    let odd = k.exp == 0 && k.man.low_bits_nonzero(1);
    let r = xb.sub(&Ball::exact(k).mul(&pi, w), w);
    (float_dd(&r.mid), odd)
}

/// Si(x) = int_0^x sin(t)/t dt for |x| <= 6 by its series.
fn si_series(x: DD) -> DD {
    let mx2 = x.sqr().neg();
    let (mut u, mut s) = (x, x);
    let mut k = 1.0;
    loop {
        u = u.mul(mx2).div_f((2.0 * k) * (2.0 * k + 1.0));
        let term = u.div_f(2.0 * k + 1.0);
        s = s.add(term);
        if term.hi.abs() <= s.hi.abs() * 1e-35 || !term.hi.is_finite() {
            break;
        }
        k += 1.0;
    }
    s
}

/// sum_{k>=1} (+-1)^k x^2k/(2k (2k)!) with the sign of `sign` (-1 for Ci, +1 for Chi).
fn cos_type_sum(x: DD, sign: f64) -> DD {
    let x2 = x.sqr().mul_f(sign);
    let (mut u, mut s) = (DD::ONE, DD::ZERO);
    let mut k = 1.0;
    loop {
        u = u.mul(x2).div_f((2.0 * k - 1.0) * (2.0 * k));
        let term = u.div_f(2.0 * k);
        s = s.add(term);
        if term.hi.abs() <= s.hi.abs().max(1e-300) * 1e-35 || !term.hi.is_finite() {
            break;
        }
        k += 1.0;
    }
    s
}

/// The sine integral Si(x) = int_0^x sin(t)/t dt for a machine real (odd).
pub fn sin_integral(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let a = x.abs();
    let v = if a <= 6.0 {
        si_series(DD::new(a)).to_f64()
    } else if a == f64::INFINITY {
        PI.hi / 2.0
    } else {
        let (amp, th) = amp_phase(a);
        let (_, c) = sin_cos_shift(a, th);
        PI.mul_f(0.5).sub(amp.mul(c)).to_f64()
    };
    if x < 0.0 {
        -v
    } else {
        v
    }
}

/// The cosine integral Ci(x) = gamma + ln x + int_0^x (cos t - 1)/t dt for x > 0 (Ci(0) = -inf; x < 0 NaN).
pub fn cos_integral(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x == f64::INFINITY {
        return 0.0;
    }
    let c = consts();
    for z in &c.ci_zeros {
        if z.near(x) {
            return z.eval(x).to_f64();
        }
    }
    if x <= 6.0 {
        let xd = DD::new(x);
        return c.euler.add(ln_dd(xd)).add(cos_type_sum(xd, -1.0)).to_f64();
    }
    let (amp, th) = amp_phase(x);
    let (s, _) = sin_cos_shift(x, th);
    amp.mul(s).to_f64()
}

// ---------------------------------------------------------------- Shi, Chi

/// The hyperbolic sine integral Shi(x) = int_0^x sinh(t)/t dt for a machine real (odd).
pub fn sinh_integral(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let a = x.abs();
    let v = if a <= 8.0 {
        let x2 = DD::new(a).sqr();
        let (mut u, mut s) = (DD::new(a), DD::new(a));
        let mut k = 1.0;
        loop {
            u = u.mul(x2).div_f((2.0 * k) * (2.0 * k + 1.0));
            let term = u.div_f(2.0 * k + 1.0);
            s = s.add(term);
            if term.hi <= s.hi * 1e-35 || !term.hi.is_finite() {
                break;
            }
            k += 1.0;
        }
        s.to_f64()
    } else if a == f64::INFINITY {
        a
    } else {
        let d = DD::new(a);
        ei_pos(d).add(e1_pos(d)).mul_dd(DD::new(0.5)).to_f64()
    };
    if x < 0.0 {
        -v
    } else {
        v
    }
}

/// The hyperbolic cosine integral Chi(x) = gamma + ln x + int_0^x (cosh t - 1)/t dt for x > 0 (Chi(0) = -inf;
/// x < 0 NaN).
pub fn cosh_integral(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x == f64::INFINITY {
        return x;
    }
    let c = consts();
    if c.chi_zero.near(x) {
        return c.chi_zero.eval(x).to_f64();
    }
    let d = DD::new(x);
    if x <= 8.0 {
        return c.euler.add(ln_dd(d)).add(cos_type_sum(d, 1.0)).to_f64();
    }
    ei_pos(d).add(e1_pos(d).neg()).mul_dd(DD::new(0.5)).to_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, ulps: f64) -> bool {
        (a - b).abs() <= ulps * f64::EPSILON * b.abs()
    }

    #[test]
    fn published_values() {
        assert!(close(exp_integral_ei(1.0), 1.895_117_816_355_936_8, 1.0));
        assert!(close(exp_integral_e(1.0, 1.0), 0.219_383_934_395_520_27, 1.0));
        assert!(close(log_integral(2.0), 1.045_163_780_117_493, 1.0));
        assert!(close(sin_integral(1.0), 0.946_083_070_367_183, 1.0));
        assert!(close(cos_integral(1.0), 0.337_403_922_900_968_1, 1.0));
        assert_eq!(exp_integral_ei(0.0), f64::NEG_INFINITY);
        assert_eq!(log_integral(1.0), f64::NEG_INFINITY);
        assert_eq!(log_integral(0.0), 0.0);
        assert!(cos_integral(-1.0).is_nan() && log_integral(-1.0).is_nan());
        assert_eq!(exp_integral_e(2.0, 0.0), 1.0);
        assert!(exp_integral_e(0.5, -1.0).is_nan());
        // E_0(x) = e^-x/x is real for x < 0
        assert!(close(exp_integral_e(0.0, -1.0), -std::f64::consts::E, 2.0));
    }
}
