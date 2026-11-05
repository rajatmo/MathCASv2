//! Elliptic integrals (Carlson's symmetric forms, Legendre's complete and incomplete integrals) and the Jacobi
//! elliptic functions sn, cn, dn for machine reals, in double-double arithmetic (decision SF.3), rounded once.
//!
//! - **R_F, R_D, R_J** by Carlson's duplication (Carlson 1995): each step replaces the arguments t by (t + lambda)/4,
//!   lambda = sqrt(x y) + sqrt(x z) + sqrt(y z), which divides the relative deviations from their mean A by about 4.
//!   The steps stop once the deviations are below 2^-8 (R_F) or 2^-11.7 (R_D, R_J) and the symmetric Taylor
//!   polynomial finishes: degree 7 for R_F (remainder at most eps^8/(1 - eps) relative), degree 5 for R_D and R_J
//!   (remainder at most 6 eps^6/(1 - eps)^2 of the last term); both bounds are below 2^-63. R_J accumulates
//!   R_C(alpha^2, beta^2) = R_C(1, 1 + t)/alpha per step, by its series for |t| < 1/64 and by R_F otherwise; the Cauchy
//!   principal value for p < 0 comes from the transformation to a positive p (Carlson 1995, eq. 2.22; Numerical
//!   Recipes rj). R_C(x, y) = R_F(x, y, y), with the principal value sqrt(x/(x - y)) R_C(x - y, -y) for y < 0.
//!   Arguments far from 1 are scaled by powers of 4 (the functions are homogeneous).
//! - **K(m), E(m)** by the arithmetic-geometric mean of 1 and sqrt(1 - m): K = pi/(2 M), E = K (1 - sum 2^(n-1) c_n^2)
//!   with c_0^2 = m and c_(n+1) = c_n^2/(4 a_(n+1)) (no cancellation), stopped when c_n <= 2^-54 a_n (then
//!   a_n - M <= 2^-108 a_n). The parameter convention is m = k^2; 1 - m is formed exactly.
//! - **F, E, Pi (incomplete)**: phi = j pi + psi with |psi| <= pi/2, reduced in double-double (phi/pi to about
//!   2^-105 relative), sin psi and cos psi by their Taylor series; F(psi) = s R_F(c^2, Delta^2, 1),
//!   E(psi) = s R_F - (m/3) s^3 R_D(c^2, Delta^2, 1), Pi(psi) = s R_F + (n/3) s^3 R_J(c^2, Delta^2, 1, 1 - n s^2), with
//!   Delta^2 = c^2 + (1 - m) s^2 (no cancellation for m <= 1), and the quasi-periodicity F(j pi + psi) = 2 j K + F(psi)
//!   (likewise E, Pi). For m > 1 only |phi| <= pi/2 with m sin^2 phi <= 1 is real. For n sin^2 phi > 1 the incomplete
//!   Pi is the Cauchy principal value (R_J with a negative fourth argument); the complete Pi(n | m) for n > 1 is the
//!   principal value K(m) - Pi(m/n | m) = -(m/(3 n)) R_J(0, 1 - m, 1, 1 - m/n).
//! - **sn, cn, dn** for 0 <= m < 1 by the descending Landen (Gauss) transformation (Abramowitz and Stegun 16.4,
//!   Bulirsch's form): the AGM levels a_k, b_k, then sin and cos of u M (reduced modulo 2 pi in double-double, so
//!   large u keep an absolute phase error of about |u| 2^-105), then the backward recurrence of ratios; m = 1 by
//!   tanh and sech; tiny u by Taylor series; m < 0 and m > 1 by the reciprocal and imaginary-modulus
//!   transformations (A&S 16.10, 16.11). inverse_jacobi_sn(v, m) = v R_F(1 - v^2, 1 - m v^2, 1).
//!
//! Domains: complex values give NaN (K, E for m > 1; F, E, Pi for m sin^2 phi > 1 or |phi| > pi/2 with m > 1;
//! |v| > 1 for the inverse); the logarithmic poles K(1), Pi(n | 1), Pi(1 | m) and R_F, R_D, R_J with too many zero
//! arguments give +inf; F(phi | 1) beyond |phi| > pi/2 gives NaN (the integral passes through the pole).
use crate::dd::{exp, frexp, pow2, sin_cos_pi, sin_cos_pi_any, DD, PI};
use std::ops::{Div, Mul, Neg};

/// 1/pi as a double-double.
const INV_PI: DD = DD { hi: std::f64::consts::FRAC_1_PI, lo: -1.967_867_667_518_248_6e-17 };
/// R_F stops duplicating below this relative deviation (eps^8/(1 - eps) < 2^-63).
const RF_TOL: f64 = 1.0 / 256.0;
/// R_D and R_J stop below this one (6 eps^6/(1 - eps)^2 < 2^-64).
const RD_TOL: f64 = 3.0e-4;
/// A cap on the duplication steps (about 20 suffice for arguments 2^-1000 apart).
const MAX_DUP: usize = 400;

/// max |A - t|/A over the arguments, in double precision (only to decide when to stop).
#[inline]
fn dev(a: DD, t: DD) -> f64 {
    ((a.hi - t.hi) / a.hi).abs()
}

/// (A - t)/A to double precision (the deviation, from a double-double difference).
#[inline]
fn rel(a: DD, t: DD) -> f64 {
    (a - t).hi / a.hi
}

/// R_F(x, y, z) for x, y, z >= 0 with at most one zero.
fn rf(mut x: DD, mut y: DD, mut z: DD) -> DD {
    let mut a = (x + y + z).div_f(3.0);
    for _ in 0..MAX_DUP {
        if dev(a, x).max(dev(a, y)).max(dev(a, z)) < RF_TOL {
            break;
        }
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let lam = sx * sy + sx * sz + sy * sz;
        x = (x + lam).ldexp(-2);
        y = (y + lam).ldexp(-2);
        z = (z + lam).ldexp(-2);
        a = (a + lam).ldexp(-2);
    }
    let (xx, yy) = (rel(a, x), rel(a, y));
    let zz = -(xx + yy);
    let e2 = xx * yy - zz * zz;
    let e3 = xx * yy * zz;
    let t = -e2 / 10.0 + e3 / 14.0 + e2 * e2 / 24.0 - 3.0 * e2 * e3 / 44.0 - 5.0 * e2 * e2 * e2 / 208.0
        + 3.0 * e3 * e3 / 104.0
        + e2 * e2 * e3 / 16.0;
    DD::ONE.add_f(t).div(a.sqrt())
}

/// The degree-5 polynomial of R_D and R_J in the elementary symmetric functions of the deviations.
#[inline]
fn poly5(e2: f64, e3: f64, e4: f64, e5: f64) -> f64 {
    -3.0 * e2 / 14.0 + e3 / 6.0 + 9.0 * e2 * e2 / 88.0 - 3.0 * e4 / 22.0 - 9.0 * e2 * e3 / 52.0 + 3.0 * e5 / 26.0
}

/// R_D(x, y, z) for x, y >= 0 (at most one zero) and z > 0.
fn rd(mut x: DD, mut y: DD, mut z: DD) -> DD {
    let mut sum = DD::ZERO;
    let mut fac = 1.0;
    let mut a = (x + y + z.mul_f(3.0)).div_f(5.0);
    for _ in 0..MAX_DUP {
        if dev(a, x).max(dev(a, y)).max(dev(a, z)) < RD_TOL {
            break;
        }
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let lam = sx * sy + sx * sz + sy * sz;
        sum = sum + DD::new(fac).div(sz * (z + lam));
        fac *= 0.25;
        x = (x + lam).ldexp(-2);
        y = (y + lam).ldexp(-2);
        z = (z + lam).ldexp(-2);
        a = (a + lam).ldexp(-2);
    }
    let (xx, yy) = (rel(a, x), rel(a, y));
    let zz = -(xx + yy) / 3.0;
    let xy = xx * yy;
    let z2 = zz * zz;
    let e2 = xy - 6.0 * z2;
    let e3 = (3.0 * xy - 8.0 * z2) * zz;
    let e4 = 3.0 * (xy - z2) * z2;
    let e5 = xy * z2 * zz;
    let tail = DD::ONE.add_f(poly5(e2, e3, e4, e5)).mul_f(fac).div(a * a.sqrt());
    sum.mul_f(3.0) + tail
}

/// R_C(1, 1 + t) for t > -1.
fn rc1(t: DD) -> DD {
    let th = t.hi;
    if th.abs() < 1.0 / 64.0 {
        // sum_k (-t)^k/(2k+1): the first two terms in double-double, the rest (below 2^-12) in double
        let mut s = 0.0;
        let mut pw = th * th;
        for k in 2..=11 {
            s += pw / (2 * k + 1) as f64;
            pw *= -th;
        }
        DD::ONE - t.div_f(3.0) + DD::new(s)
    } else {
        let y = t.add_f(1.0);
        rf(DD::ONE, y, y)
    }
}

/// R_J(x, y, z, p) for x, y, z >= 0 (at most one zero) and p > 0.
fn rj(mut x: DD, mut y: DD, mut z: DD, mut p: DD) -> DD {
    let mut sum = DD::ZERO;
    let mut fac = 1.0;
    let mut a = (x + y + z + p.mul_f(2.0)).div_f(5.0);
    for _ in 0..MAX_DUP {
        if dev(a, x).max(dev(a, y)).max(dev(a, z)).max(dev(a, p)) < RD_TOL {
            break;
        }
        let (sx, sy, sz, sp) = (x.sqrt(), y.sqrt(), z.sqrt(), p.sqrt());
        let lam = sx * sy + sx * sz + sy * sz;
        let alpha = p * (sx + sy + sz) + sx * sy * sz;
        let beta = sp * (p + lam);
        // R_C(alpha^2, beta^2) = R_C(1, 1 + t)/alpha, t = (beta - alpha)(beta + alpha)/alpha^2
        let t = (beta - alpha) * (beta + alpha) / alpha.sqr();
        sum = sum + rc1(t).div(alpha).mul_f(fac);
        fac *= 0.25;
        x = (x + lam).ldexp(-2);
        y = (y + lam).ldexp(-2);
        z = (z + lam).ldexp(-2);
        p = (p + lam).ldexp(-2);
        a = (a + lam).ldexp(-2);
    }
    let (xx, yy, zz) = (rel(a, x), rel(a, y), rel(a, z));
    let pp = -(xx + yy + zz) / 2.0;
    let xyz = xx * yy * zz;
    let e2 = xx * yy + xx * zz + yy * zz - 3.0 * pp * pp;
    let e3 = xyz + 2.0 * e2 * pp + 4.0 * pp * pp * pp;
    let e4 = (2.0 * xyz + e2 * pp + 3.0 * pp * pp * pp) * pp;
    let e5 = xyz * pp * pp;
    let tail = DD::ONE.add_f(poly5(e2, e3, e4, e5)).mul_f(fac).div(a * a.sqrt());
    sum.mul_f(3.0) + tail
}

/// R_C(x, y) for x >= 0, y != 0 (the principal value for y < 0).
fn rc(x: DD, y: DD) -> DD {
    if y.hi > 0.0 {
        return rf(x, y, y);
    }
    if x.hi == 0.0 {
        return DD::ZERO;
    }
    let d = x - y;
    x.div(d).sqrt() * rf(d, y.neg(), y.neg())
}

/// The principal value of R_J(x, y, z, p) for p < 0, x, y, z >= 0 with at most one zero.
fn rj_pv(x: DD, y: DD, z: DD, p: DD) -> DD {
    let mut v = [x, y, z];
    v.sort_by(|a, b| a.hi.total_cmp(&b.hi));
    let [xt, yt, zt] = v;
    let a = DD::ONE.div(yt - p);
    let b = a * (zt - yt) * (yt - xt);
    let pt = yt + b;
    let rho = xt * zt / yt;
    let tau = p * pt / yt;
    let rcx = rc(rho, tau);
    a * (b * rj(xt, yt, zt, pt) + (rcx - rf(xt, yt, zt)).mul_f(3.0))
}

/// The exponent k for which 4^k max scales the arguments near 1 (0 inside [2^-500, 2^500]).
fn scale_of(mx: f64) -> i32 {
    if mx > pow2(500) || mx < pow2(-500) {
        -(frexp(mx).1 / 2)
    } else {
        0
    }
}

/// Do the arguments allow an evaluation: all finite and >= 0, with at most `zeros` zeros?
fn admissible(args: &[f64], zeros: usize) -> bool {
    args.iter().all(|&t| t >= 0.0 && t.is_finite()) && args.iter().filter(|&&t| t == 0.0).count() <= zeros
}

/// Carlson's R_F(x, y, z) = (1/2) int_0^inf dt / sqrt((t + x)(t + y)(t + z)) for x, y, z >= 0 with at most one
/// zero (two or three zeros: +inf; a negative or NaN argument: NaN; an infinite argument: 0).
pub fn carlson_rf(x: f64, y: f64, z: f64) -> f64 {
    let args = [x, y, z];
    if args.iter().any(|t| t.is_nan() || *t < 0.0) {
        return f64::NAN;
    }
    if args.iter().any(|t| t.is_infinite()) {
        return 0.0;
    }
    if !admissible(&args, 1) {
        return f64::INFINITY;
    }
    let k = scale_of(x.max(y).max(z));
    let s = pow2(2 * k);
    rf(DD::new(x * s), DD::new(y * s), DD::new(z * s)).mul_f(pow2(k)).to_f64()
}

/// Carlson's R_D(x, y, z) = (3/2) int_0^inf dt / ((t + z) sqrt((t + x)(t + y)(t + z))) for x, y >= 0 (at most one
/// zero) and z > 0 (z = 0 or x = y = 0: +inf; negative or NaN arguments: NaN).
pub fn carlson_rd(x: f64, y: f64, z: f64) -> f64 {
    let args = [x, y, z];
    if args.iter().any(|t| t.is_nan() || *t < 0.0) {
        return f64::NAN;
    }
    if args.iter().any(|t| t.is_infinite()) {
        return 0.0;
    }
    if z == 0.0 || !admissible(&args, 1) {
        return f64::INFINITY;
    }
    let k = scale_of(x.max(y).max(z));
    let s = pow2(2 * k);
    rd(DD::new(x * s), DD::new(y * s), DD::new(z * s)).mul_f(pow2(3 * k)).to_f64()
}

/// Carlson's R_J(x, y, z, p) = (3/2) int_0^inf dt / ((t + p) sqrt((t + x)(t + y)(t + z))) for x, y, z >= 0 with at
/// most one zero and p != 0; for p < 0 the Cauchy principal value. p = 0 or two zeros among x, y, z: +inf.
pub fn carlson_rj(x: f64, y: f64, z: f64, p: f64) -> f64 {
    let args = [x, y, z];
    if args.iter().chain([p].iter()).any(|t| t.is_nan()) || args.iter().any(|t| *t < 0.0) {
        return f64::NAN;
    }
    if args.iter().chain([p].iter()).any(|t| t.is_infinite()) {
        return 0.0;
    }
    if p == 0.0 || !admissible(&args, 1) {
        return f64::INFINITY;
    }
    let k = scale_of(x.max(y).max(z).max(p.abs()));
    let s = pow2(2 * k);
    let (xd, yd, zd, pd) = (DD::new(x * s), DD::new(y * s), DD::new(z * s), DD::new(p * s));
    let r = if p > 0.0 { rj(xd, yd, zd, pd) } else { rj_pv(xd, yd, zd, pd) };
    r.mul_f(pow2(3 * k)).to_f64()
}

/// Carlson's R_C(x, y) = R_F(x, y, y) for x >= 0, y != 0; for y < 0 the Cauchy principal value
/// sqrt(x/(x - y)) R_C(x - y, -y). y = 0: +inf.
pub fn carlson_rc(x: f64, y: f64) -> f64 {
    if x.is_nan() || y.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x.is_infinite() || y.is_infinite() {
        return 0.0;
    }
    if y == 0.0 {
        return f64::INFINITY;
    }
    let k = scale_of(x.max(y.abs()));
    let s = pow2(2 * k);
    rc(DD::new(x * s), DD::new(y * s)).mul_f(pow2(k)).to_f64()
}

/// (K(m), E(m)) for m < 1 given exactly as m and mc = 1 - m > 0, by the AGM.
fn agm_ke(m: DD, mc: DD) -> (DD, DD) {
    let b0 = mc.sqrt();
    let mut a = b0.add_f(1.0).mul_f(0.5);
    let mut b = b0.sqrt();
    let mut c = m.div(b0.add_f(1.0).mul_f(2.0));
    let mut s = m.mul_f(0.5) + c.sqr();
    let mut pw = 1.0;
    for _ in 0..64 {
        if c.hi.abs() <= pow2(-54) * a.hi {
            break;
        }
        let an = (a + b).mul_f(0.5);
        b = (a * b).sqrt();
        a = an;
        c = c.sqr().div(a.mul_f(4.0));
        pw *= 2.0;
        s = s + c.sqr().mul_f(pw);
    }
    let k = PI.ldexp(-1).div(a);
    (k, k * (DD::ONE - s))
}

/// The complete integral of the first kind K(m) = int_0^(pi/2) dt / sqrt(1 - m sin^2 t) (parameter m = k^2):
/// +inf at m = 1, NaN for m > 1.
pub fn elliptic_k(m: f64) -> f64 {
    if m.is_nan() || m > 1.0 {
        return f64::NAN;
    }
    if m == 1.0 {
        return f64::INFINITY;
    }
    if m == f64::NEG_INFINITY {
        return 0.0;
    }
    agm_ke(DD::new(m), DD::sum(1.0, -m)).0.to_f64()
}

/// The complete integral of the second kind E(m) = int_0^(pi/2) sqrt(1 - m sin^2 t) dt: E(1) = 1, NaN for m > 1.
pub fn elliptic_e(m: f64) -> f64 {
    if m.is_nan() || m > 1.0 {
        return f64::NAN;
    }
    if m == 1.0 {
        return 1.0;
    }
    if m == f64::NEG_INFINITY {
        return f64::INFINITY;
    }
    agm_ke(DD::new(m), DD::sum(1.0, -m)).1.to_f64()
}

/// phi = j pi + psi with |psi| <= pi/2 (up to rounding): j, sin psi, cos psi.
struct Phase {
    j: f64,
    s: DD,
    c: DD,
}

fn reduce(phi: f64) -> Phase {
    let t = INV_PI.mul_f(phi);
    let j = t.hi.round();
    let (s, c) = sin_cos_pi(DD::sum(t.hi - j, t.lo));
    Phase { j, s, c }
}

/// Delta^2 = 1 - m sin^2 psi (as c^2 + (1 - m) s^2 for m <= 1); `None` where the integrals are not real.
fn delta2(ph: &Phase, m: f64, mc: DD) -> Option<DD> {
    if m <= 1.0 {
        return Some(ph.c.sqr() + mc * ph.s.sqr());
    }
    if ph.j != 0.0 {
        return None;
    }
    let d = DD::ONE - ph.s.sqr().mul_f(m);
    (d.hi >= 0.0).then_some(d)
}

/// The common checks of the incomplete integrals: `Some(early value)` for NaN, infinite and tiny phi.
fn incomplete_special(phi: f64, m: f64, inf: f64) -> Option<f64> {
    if phi.is_nan() || m.is_nan() || m.is_infinite() {
        return Some(f64::NAN);
    }
    if phi.is_infinite() {
        return Some(if m < 1.0 { inf } else { f64::NAN });
    }
    if phi.abs() < 1e-150 {
        // the next terms are of relative size phi^2 max(|m|, |n|) (checked by the callers' domains)
        return Some(if m * phi * phi <= 1.0 { phi } else { f64::NAN });
    }
    None
}

/// The incomplete integral of the first kind F(phi | m) = int_0^phi dt / sqrt(1 - m sin^2 t), for any real phi when
/// m < 1 (F(phi + j pi) = F(phi) + 2 j K(m)) and for |phi| <= pi/2 with m sin^2 phi <= 1 when m >= 1; NaN otherwise.
pub fn elliptic_f(phi: f64, m: f64) -> f64 {
    if let Some(v) = incomplete_special(phi, m, phi) {
        return v;
    }
    let ph = reduce(phi);
    let mc = DD::sum(1.0, -m);
    let Some(d2) = delta2(&ph, m, mc) else { return f64::NAN };
    let mut v = ph.s * rf(ph.c.sqr(), d2, DD::ONE);
    if ph.j != 0.0 {
        if m >= 1.0 {
            return f64::NAN;
        }
        v = v + agm_ke(DD::new(m), mc).0.mul_f(2.0 * ph.j);
    }
    v.to_f64()
}

/// The incomplete integral of the second kind E(phi | m) = int_0^phi sqrt(1 - m sin^2 t) dt, with the conventions of
/// [`elliptic_f`] (E(phi + j pi) = E(phi) + 2 j E(m), also for m = 1).
pub fn elliptic_e_inc(phi: f64, m: f64) -> f64 {
    if let Some(v) = incomplete_special(phi, m, phi * f64::INFINITY) {
        return v;
    }
    let ph = reduce(phi);
    let mc = DD::sum(1.0, -m);
    let Some(d2) = delta2(&ph, m, mc) else { return f64::NAN };
    let c2 = ph.c.sqr();
    let s3 = ph.s.sqr() * ph.s;
    let mut v = ph.s * rf(c2, d2, DD::ONE) - s3 * rd(c2, d2, DD::ONE).mul_f(m).div_f(3.0);
    if ph.j != 0.0 {
        let e = if m == 1.0 {
            DD::ONE
        } else if m < 1.0 {
            agm_ke(DD::new(m), mc).1
        } else {
            return f64::NAN;
        };
        v = v + e.mul_f(2.0 * ph.j);
    }
    v.to_f64()
}

/// Pi(n | m) for m < 1, n != 1 (the principal value for n > 1), given K(m).
fn pi_complete(n: f64, m: f64, mc: DD, k: DD) -> DD {
    if n < 1.0 {
        let r = rj(DD::ZERO, mc, DD::ONE, DD::sum(1.0, -n));
        k + r.mul_f(n).div_f(3.0)
    } else {
        // Pi(n | m) = K(m) - Pi(m/n | m) for n > 1
        let q = DD::new(m).div_f(n);
        let r = rj(DD::ZERO, mc, DD::ONE, DD::ONE - q);
        (q * r).div_f(-3.0)
    }
}

/// The complete integral of the third kind Pi(n | m) = int_0^(pi/2) dt / ((1 - n sin^2 t) sqrt(1 - m sin^2 t)) for
/// m < 1; for n > 1 the Cauchy principal value; +inf at n = 1 and at m = 1 (n < 1); NaN for m > 1.
pub fn elliptic_pi(n: f64, m: f64) -> f64 {
    if n.is_nan() || m.is_nan() || m > 1.0 {
        return f64::NAN;
    }
    if m == 1.0 {
        return if n < 1.0 { f64::INFINITY } else { f64::NAN };
    }
    if n == 1.0 {
        return f64::INFINITY;
    }
    if n.is_infinite() {
        return 0.0;
    }
    if m == f64::NEG_INFINITY {
        return 0.0;
    }
    let mc = DD::sum(1.0, -m);
    let k = agm_ke(DD::new(m), mc).0;
    pi_complete(n, m, mc, k).to_f64()
}

/// The incomplete integral of the third kind Pi(n; phi | m) = int_0^phi dt / ((1 - n sin^2 t) sqrt(1 - m sin^2 t)),
/// with the conventions of [`elliptic_f`]; where n sin^2 t passes 1 the Cauchy principal value (NaN where it is
/// exactly 1 at phi).
pub fn elliptic_pi_inc(n: f64, phi: f64, m: f64) -> f64 {
    if n.is_nan() || n.is_infinite() {
        return f64::NAN;
    }
    if let Some(v) = incomplete_special(phi, m, phi * f64::INFINITY) {
        if v.is_finite() && v != 0.0 && n * phi * phi > 1.0 {
            return f64::NAN;
        }
        return if phi.is_infinite() { f64::NAN } else { v };
    }
    let ph = reduce(phi);
    let mc = DD::sum(1.0, -m);
    let Some(d2) = delta2(&ph, m, mc) else { return f64::NAN };
    let c2 = ph.c.sqr();
    let s2 = ph.s.sqr();
    let pn = DD::ONE - s2.mul_f(n);
    if pn.hi == 0.0 {
        return f64::NAN;
    }
    let r = if pn.hi > 0.0 { rj(c2, d2, DD::ONE, pn) } else { rj_pv(c2, d2, DD::ONE, pn) };
    let mut v = ph.s * rf(c2, d2, DD::ONE) + (s2 * ph.s) * r.mul_f(n).div_f(3.0);
    if ph.j != 0.0 {
        if m >= 1.0 || n == 1.0 {
            return f64::NAN;
        }
        let k = agm_ke(DD::new(m), mc).0;
        v = v + pi_complete(n, m, mc, k).mul_f(2.0 * ph.j);
    }
    v.to_f64()
}

/// (tanh u, sech u).
fn tanh_sech(u: DD) -> (DD, DD) {
    let e = exp(u.abs().neg());
    let e2 = e.sqr();
    let den = DD::ONE + e2;
    let t = (DD::ONE - e2).div(den);
    (if u.hi < 0.0 { t.neg() } else { t }, e.mul_f(2.0).div(den))
}

/// (sn, cn, dn)(u | m) for 0 <= m <= 1, mc = 1 - m, by the descending Landen transformation.
fn sncndn(u: DD, m: DD, mc: DD) -> (DD, DD, DD) {
    if u.hi == 0.0 {
        return (u, DD::ONE, DD::ONE);
    }
    if u.hi.abs() < pow2(-26) {
        // the next terms are of relative size u^4 < 2^-104
        let u2 = u.hi * u.hi;
        let sn = u.mul(DD::ONE.add_f(-(1.0 + m.hi) * u2 / 6.0));
        return (sn, DD::ONE.add_f(-u2 / 2.0), DD::ONE.add_f(-m.hi * u2 / 2.0));
    }
    if mc.hi == 0.0 {
        let (t, s) = tanh_sech(u);
        return (t, s, s);
    }
    let mut em = [DD::ZERO; 64];
    let mut en = [DD::ZERO; 64];
    let mut a = DD::ONE;
    let mut emc = mc;
    let mut c = DD::ONE;
    let mut l = 0;
    for i in 0..64 {
        em[i] = a;
        emc = emc.sqrt();
        en[i] = emc;
        c = (a + emc).mul_f(0.5);
        l = i;
        if (a.hi - emc.hi).abs() <= pow2(-52) * a.hi {
            break;
        }
        emc = emc * a;
        a = c;
    }
    let (s0, c0) = sin_cos_pi_any((u * c) * INV_PI);
    if s0.hi == 0.0 {
        return (s0, c0, DD::ONE);
    }
    let mut a = c0.div(s0);
    let mut c = c * a;
    let mut dn = DD::ONE;
    for ii in (0..=l).rev() {
        let b = em[ii];
        a = a * c;
        c = c * dn;
        dn = (en[ii] + a).div(b + a);
        a = c.div(b);
    }
    let r = (c.sqr().add_f(1.0)).sqrt().recip();
    let sn = if s0.hi >= 0.0 { r } else { r.neg() };
    (sn, c * sn, dn)
}

/// (sn, cn, dn)(u | m) for any real m (NaN for NaN or infinite arguments).
fn jacobi(u: f64, m: f64) -> (f64, f64, f64) {
    if u.is_nan() || m.is_nan() || u.is_infinite() || m.is_infinite() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    let ud = DD::new(u);
    let (s, c, d) = if (0.0..=1.0).contains(&m) {
        sncndn(ud, DD::new(m), DD::sum(1.0, -m))
    } else if m > 1.0 {
        // A&S 16.11: mu = 1/m, v = u sqrt(m): sn = sn(v|mu)/sqrt(m), cn = dn(v|mu), dn = cn(v|mu)
        let md = DD::new(m);
        let rt = md.sqrt();
        let mu = DD::ONE.div_f(m);
        let muc = DD::sum(m, -1.0).div_f(m);
        let (s, c, d) = sncndn(rt.mul_f(u), mu, muc);
        (s.div(rt), d, c)
    } else {
        // A&S 16.10: mu = -m/(1-m), v = u sqrt(1-m): sn = sd(v|mu)/sqrt(1-m), cn = cd(v|mu), dn = nd(v|mu)
        let mc = DD::sum(1.0, -m);
        let rt = mc.sqrt();
        let mu = DD::new(-m).div(mc);
        let muc = DD::ONE.div(mc);
        let (s, c, d) = sncndn(rt.mul_f(u), mu, muc);
        (s.div(d * rt), c.div(d), d.recip())
    };
    (s.to_f64(), c.to_f64(), d.to_f64())
}

/// The Jacobi elliptic function sn(u | m) (parameter m = k^2), any real u and m.
pub fn jacobi_sn(u: f64, m: f64) -> f64 {
    jacobi(u, m).0
}

/// The Jacobi elliptic function cn(u | m), any real u and m.
pub fn jacobi_cn(u: f64, m: f64) -> f64 {
    jacobi(u, m).1
}

/// The Jacobi elliptic function dn(u | m), any real u and m.
pub fn jacobi_dn(u: f64, m: f64) -> f64 {
    jacobi(u, m).2
}

/// The inverse of sn: F(asin v | m) = v R_F(1 - v^2, 1 - m v^2, 1) for -1 <= v <= 1 with m v^2 <= 1 (NaN otherwise;
/// ±inf at v = ±1, m = 1).
pub fn inverse_jacobi_sn(v: f64, m: f64) -> f64 {
    if v.is_nan() || m.is_nan() || v.abs() > 1.0 || m.is_infinite() {
        return f64::NAN;
    }
    if v.abs() < 1e-150 {
        return v;
    }
    let vd = DD::new(v);
    let x = DD::sum(1.0, -v) * DD::sum(1.0, v);
    let y = DD::ONE - vd.sqr().mul_f(m);
    if y.hi < 0.0 {
        return f64::NAN;
    }
    if x.hi == 0.0 && y.hi == 0.0 {
        return v * f64::INFINITY;
    }
    (vd * rf(x, y, DD::ONE)).to_f64()
}

/// An approximation of the amplitude am(u | m) (sn = sin am, cn = cos am) for m < 1 given with mc = 1 - m > 0, to
/// about double precision: the start of the ball path's Newton iteration.
pub fn amplitude(u: f64, m: f64, mc: f64) -> f64 {
    let (md, mcd) = (DD::new(m), DD::new(mc));
    let ud = DD::new(u);
    let (s, c, d) = if m >= 0.0 {
        sncndn(ud, md, mcd)
    } else {
        let rt = mcd.sqrt();
        let (s, c, d) = sncndn(rt.mul_f(u), DD::new(-m).div(mcd), DD::ONE.div(mcd));
        (s.div(d * rt), c.div(d), d.recip())
    };
    let _ = d;
    let base = s.hi.atan2(c.hi);
    // am(u) - pi u/(2K) is periodic with period 2K and below pi/2 in size: choose the branch nearest pi u/(2K)
    let k = agm_ke(md, mcd).0.hi;
    let lin = std::f64::consts::PI * u / (2.0 * k);
    let turns = ((lin - base) / (2.0 * std::f64::consts::PI)).round();
    base + 2.0 * std::f64::consts::PI * turns
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(got: f64, want: f64, ulps: f64) {
        let d = (got - want).abs() / (want.abs() * f64::EPSILON);
        assert!(d <= ulps, "{got} vs {want}: {d} ulps");
    }

    #[test]
    fn constants() {
        let r = DD::ONE.div(PI);
        assert!((r - INV_PI).hi.abs() < 1e-32);
    }

    #[test]
    #[allow(clippy::excessive_precision)] // the references are quoted to more digits than a double holds
    fn published_values() {
        // K(1/2), E(1/2) (Gauss's lemniscate case)
        close(elliptic_k(0.5), 1.854_074_677_301_371_918_433_850_347_195_260_046_217_598_823_521_766_905_585_928, 1.0);
        close(elliptic_e(0.5), 1.350_643_881_047_675_502_520_174_735_338_725_841_349_522_366_92, 1.0);
        close(elliptic_k(0.0), std::f64::consts::FRAC_PI_2, 0.5);
        assert_eq!(elliptic_k(1.0), f64::INFINITY);
        assert!(elliptic_k(1.5).is_nan());
        assert_eq!(elliptic_e(1.0), 1.0);
        // R_F(1, 2, 0) = 1.3110287771460599052 (Carlson 1995 test values), R_D(0, 2, 1) = 1.7972103521033883112,
        // R_J(0, 1, 2, 3) = 0.77688623778582332014, R_C(0, 1/4) = pi
        close(carlson_rf(1.0, 2.0, 0.0), 1.311_028_777_146_059_905_2, 2.0);
        close(carlson_rd(0.0, 2.0, 1.0), 1.797_210_352_103_388_311_2, 2.0);
        close(carlson_rj(0.0, 1.0, 2.0, 3.0), 0.776_886_237_785_823_320_14, 2.0);
        close(carlson_rc(0.0, 0.25), std::f64::consts::PI, 1.0);
        // the principal values R_J(1, 2, 3, -1), R_C(1, -2), Pi(3 | 1/2)
        close(carlson_rj(1.0, 2.0, 3.0, -1.0), -0.093_240_452_438_676_412_014_809_038_115_33, 4.0);
        close(carlson_rc(1.0, -2.0), 0.380_172_998_150_473_173_765_547_127_440_2, 2.0);
        close(elliptic_pi(3.0, 0.5), -0.192_988_208_490_139_329_156_209_148_562_05, 4.0);
        close(elliptic_pi(-2.0, 1.0 / 3.0), 0.974_484_017_926_749_583_200_309_477_914_6, 2.0);
        close(elliptic_e(-0.5), 1.751_771_275_694_817_862_026_501_838_984_5, 2.0);
        // incomplete values and the Jacobi functions at m < 0, m > 1
        close(elliptic_f(3.5, 1.0 / 3.0), 3.828_781_481_273_049_787_675_730_551_811_2, 2.0);
        close(elliptic_pi_inc(3.0, 0.2, 0.5), 0.209_273_475_638_584_483_415_639_088_551, 2.0);
        close(elliptic_pi_inc(3.0, 2.0, 0.5), -0.518_728_493_930_321_847_913_738_561_181_2, 8.0);
        close(elliptic_e_inc(2.0, 1.0), 1.090_702_573_174_318_304_603_980_134_088_3, 2.0);
        close(elliptic_f(0.25, 2.0), 0.255_450_659_149_792_734_503_175_233_456_9, 2.0);
        assert!(elliptic_f(2.0, 2.0).is_nan() && elliptic_f(2.0, 1.0).is_nan());
        close(jacobi_sn(3.0, 0.7), 0.849_605_564_618_232_140_574_057_894_050_2, 4.0);
        close(jacobi_sn(1.0, -3.0), 0.987_850_573_944_705_624_111_480_280_228_4, 4.0);
        close(jacobi_cn(1.0, -3.0), 0.155_406_703_707_129_976_297_745_853_953_4, 8.0);
        close(jacobi_dn(1.0, -3.0), 1.981_803_791_834_260_504_386_759_956_428_7, 4.0);
        close(jacobi_sn(1.0, 3.0), 0.577_349_599_030_952_194_845_886_418_135_5, 4.0);
        close(jacobi_cn(1.0, 3.0), 0.816_497_054_801_056_048_753_647_057_108_2, 4.0);
        close(jacobi_dn(1.0, 3.0), 0.001_523_645_758_099_074_179_791_476_997_583_4, 64.0);
    }

    #[test]
    fn identities() {
        // Legendre's relation, the Pythagorean identities, F(am u) = u, periodicity
        for &m in &[0.1, 0.5, 0.9, 0.999, 1e-6, 0.3] {
            let (k, e) = (elliptic_k(m), elliptic_e(m));
            let (k1, e1) = (elliptic_k(1.0 - m), elliptic_e(1.0 - m));
            let l = e * k1 + e1 * k - k * k1;
            assert!((l - std::f64::consts::FRAC_PI_2).abs() < 4e-15, "{m}: {l}");
            for &u in &[0.3, 1.7, -2.5, 7.0, 15.5] {
                let (s, c, d) = (jacobi_sn(u, m), jacobi_cn(u, m), jacobi_dn(u, m));
                assert!((s * s + c * c - 1.0).abs() < 4e-16);
                assert!((d * d + m * s * s - 1.0).abs() < 4e-16);
                let s4 = jacobi_sn(u + 4.0 * k, m);
                assert!((s4 - s).abs() < 1e-14 * (1.0 + u.abs()), "{u} {m}");
                let am = amplitude(u, m, 1.0 - m);
                assert!((elliptic_f(am, m) - u).abs() < 1e-14 * (1.0 + u.abs()));
            }
            // F(pi/2) = K, E(pi/2) = E, and the inverse of sn
            assert!((elliptic_f(std::f64::consts::FRAC_PI_2, m) / k - 1.0).abs() < 1e-15);
            assert!((elliptic_e_inc(std::f64::consts::FRAC_PI_2, m) / e - 1.0).abs() < 1e-15);
            assert!((inverse_jacobi_sn(jacobi_sn(0.4, m), m) - 0.4).abs() < 1e-15);
        }
        // the addition theorem of sn
        for &(u, v, m) in &[(0.7, 1.9, 0.6), (-3.1, 0.4, 0.95), (5.0, 2.5, -2.0), (1.2, 0.3, 4.0)] {
            let (su, cu, du) = (jacobi_sn(u, m), jacobi_cn(u, m), jacobi_dn(u, m));
            let (sv, cv, dv) = (jacobi_sn(v, m), jacobi_cn(v, m), jacobi_dn(v, m));
            let s = (su * cv * dv + sv * cu * du) / (1.0 - m * su * su * sv * sv);
            assert!((jacobi_sn(u + v, m) - s).abs() < 1e-14, "{u} {v} {m}");
        }
        // R_J(x, y, z, z) = R_D(x, y, z) and the principal value is independent of the ordering
        close(carlson_rj(0.5, 2.0, 3.0, 3.0), carlson_rd(0.5, 2.0, 3.0), 4.0);
        let a = carlson_rj(1.0, 2.0, 3.0, -1.0);
        close(carlson_rj(3.0, 1.0, 2.0, -1.0), a, 2.0);
    }
}
