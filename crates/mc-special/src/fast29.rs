//! Machine fast paths for the functions of round 28: when every argument is a machine number
//! or an exact number and one is a machine number (no balls), the value is computed natively in complex
//! double-double arithmetic ([`crate::cdd`], about 32 digits) and rounded once, instead of through the ball loop of
//! the evaluator (0.05-0.5 s per value, so plots timed out). Each kernel estimates its own rounding error from the
//! largest term it summed (or the step sizes of a continuation) and declines (`None`) when the estimate exceeds
//! 2^-56 of the value, or outside the ranges it covers: the call then takes the slow path, which keeps every
//! value. So a fast value agrees with the 40-digit value of the exact argument as the slow one did.
//!
//! Kernels (literature in each section): Scorer functions (DLMF 9.12), ...
use crate::cdd::CD;
use crate::dd::DD;
use mc_api::T;
use mc_arith::{Num, Real};
use mc_numeric::formula::Fx;
use mc_numeric::machine::C64;
use std::sync::OnceLock;

/// The largest estimated relative error of a fast value: 2^-56 (a value within a quarter ulp before its rounding).
pub(crate) const TOL: f64 = 1.387_778_780_781_445_7e-17;

/// About the relative rounding of one double-double operation (2^-104), times a margin for the operations of a term.
const U: f64 = 4.0 * 4.930_380_657_631_324e-32;

/// The arguments as complex double-double numbers when every one is a machine or exact number and one is a machine
/// number (no balls), and whether all are real. Exact rationals keep about 32 digits (a parameter 1/3 rounded to a
/// double moved F2 by an ulp).
pub(crate) fn machine_args<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> Option<(Vec<CD>, bool)> {
    let mut out = Vec::with_capacity(a.len());
    let mut machine = false;
    let mut real = true;
    let part = |r: &Real, machine: &mut bool| -> Option<DD> {
        match r {
            Real::Ball(..) => None,
            Real::Machine(v) => {
                *machine = true;
                v.is_finite().then_some(DD::new(*v))
            }
            Real::Exact(q) => {
                let d = crate::bernoulli::rat_to_dd(q);
                d.hi.is_finite().then_some(d)
            }
        }
    };
    for &t in a {
        match fx.num(t)? {
            Num::Real(r) => out.push(CD::real(part(&r, &mut machine)?)),
            Num::Complex(x, y) => {
                real = false;
                out.push(CD::new(part(&x, &mut machine)?, part(&y, &mut machine)?));
            }
        }
    }
    machine.then_some((out, real))
}

/// The machine number of a value: a real when `real` (the imaginary part dropped), otherwise a complex number;
/// `None` when not finite.
pub(crate) fn machine_out<'h>(fx: &Fx<'_, '_, '_, 'h>, v: C64, real: bool) -> Option<T<'h>> {
    if !v.re.is_finite() || !v.im.is_finite() {
        return None;
    }
    Some(if real || (v.re != 0.0 && v.im != 0.0 && v.im.abs() <= 1e-40 * v.re.abs()) {
        fx.ar.number(&Num::Real(Real::Machine(v.re)))
    } else {
        fx.ar.number(&Num::Complex(Real::Machine(v.re), Real::Machine(v.im)))
    })
}

/// A double-double value of a ball computed once at 160 bits.
pub(crate) fn dd_of_ball(b: &mc_arith::num::Ball) -> DD {
    let hi = crate::ball::f64_of(&b.mid);
    let rest = mc_arith::num::Ball::exact(b.mid.clone()).sub(&mc_arith::num::Ball::exact(crate::ball::float_of(hi)), 160);
    DD { hi, lo: crate::ball::f64_of(&rest.mid) }
}

/// Constants as double-doubles: Bi(0), Bi'(0), 1/pi.
struct Consts {
    bi0: DD,
    bip0: DD,
    inv_pi: DD,
}

fn consts() -> &'static Consts {
    static C: OnceLock<Consts> = OnceLock::new();
    C.get_or_init(|| {
        use mc_arith::num::Ball;
        let w = 160;
        let g = |n: i64, d: i64| crate::ball::gamma::gamma_ball(&Ball::from_ratio(n, d, w), w).map(|b| dd_of_ball(&b)).unwrap_or(DD::new(f64::NAN));
        // 3^(1/6) by one Newton step on the double root: y - (y^6 - 3)/(6 y^5)
        let y = DD::new(3f64.powf(1.0 / 6.0));
        let y5 = y.powi(5);
        let r6 = y - (y5 * y - DD::new(3.0)) / y5.mul_f(6.0);
        Consts { bi0: DD::ONE / (r6 * g(2, 3)), bip0: r6 / g(1, 3), inv_pi: DD::ONE / crate::dd::PI }
    })
}

// ---------------------------------------------------------------- Scorer functions

/// Which Scorer function: Gi, Hi, Gi', Hi'.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Scorer {
    /// Gi
    Gi,
    /// Hi
    Hi,
    /// Gi'
    GiPrime,
    /// Hi'
    HiPrime,
}

/// Gi or Hi (and the derivative) by the Maclaurin series of w'' = z w -+ 1/pi (DLMF 9.12.1-2, 9.12.4-5): c0 = w(0),
/// c1 = w'(0), c2 = -+1/(2 pi), c_(n+3) = c_n/((n + 2)(n + 3)). The terms of Gi grow like exp(2/3 |z|^(3/2)) while
/// the value can be small, so the error estimate is the largest term times the rounding.
fn scorer_series(z: CD, hi: bool, deriv: bool) -> Option<C64> {
    let k = consts();
    let (c0, c1, c2) = if hi {
        (k.bi0.mul_f(2.0) / DD::new(3.0), k.bip0.mul_f(2.0) / DD::new(3.0), k.inv_pi.mul_f(0.5))
    } else {
        (k.bi0 / DD::new(3.0), k.bip0 / DD::new(3.0), -k.inv_pi.mul_f(0.5))
    };
    let mut c = [CD::real(c0), CD::real(c1), CD::real(c2)];
    let z3 = z * z * z;
    // the three interleaved series: sum_j c_(3j + r) z^(3j + r) and their derivatives
    let mut zr = [CD::ONE, z, z * z];
    let mut val = CD::ZERO;
    let mut big = 0.0f64;
    let zm = z.abs();
    let mut small = 0;
    for j in 0..2000usize {
        let mut tmax = 0.0f64;
        for r in 0..3 {
            let n = 3 * j + r;
            let t = if deriv {
                if n == 0 {
                    CD::ZERO
                } else {
                    // n c_n z^(n-1) = n c_n z^n / z, z^(n-1) kept separately
                    c[r] * zr[r].scale(n as f64)
                }
            } else {
                c[r] * zr[r]
            };
            val = val + t;
            let m = t.abs();
            big = big.max(m);
            tmax = tmax.max(m);
            c[r] = c[r] / CD::f(((n + 2) * (n + 3)) as f64);
            zr[r] = zr[r] * z3;
        }
        if j > 2 && tmax <= 1e-34 * val.abs() {
            small += 1;
            if small >= 2 {
                break;
            }
        } else {
            small = 0;
        }
        if j == 1999 || !val.finite() {
            return None;
        }
    }
    let v = if deriv {
        // the derivative series was formed with z^n: divide by z (z = 0 is handled by the caller)
        if zm == 0.0 {
            return None;
        }
        val / z
    } else {
        val
    };
    let err = big * U * (1.0 + zm.powf(1.5));
    (err <= TOL * v.abs() && v.finite()).then(|| v.to_c64())
}

/// Gi or Hi (or the derivative) at a large real argument where the function decays algebraically, by the
/// asymptotic expansions DLMF 9.12.27, 9.12.28 (Gi, Gi' for x -> +inf) and 9.12.29, 9.12.30 (Hi, Hi' for
/// x -> -inf): 1/(pi x) sum (3k)!/(k! (3x^3)^k) and -1/(pi x^2) sum (3k+1)!/(k! (3x^3)^k) at x > 0, the same with
/// (-3 x^3)^k... at negative x for Hi. Summed while the terms decrease; accepted when the smallest is below the
/// tolerance (the remainder is bounded by the first omitted term on the real axis).
fn scorer_asymptotic(x: f64, deriv: bool) -> Option<f64> {
    let k = consts();
    let ax = DD::new(x.abs());
    let x3 = ax.powi(3).mul_f(3.0);
    let mut t = DD::ONE;
    let mut s = DD::ONE;
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let mut last = f64::INFINITY;
    let mut ok = false;
    for kk in 0..200u64 {
        // (3k)!/k! -> (3k+3)!/(k+1)!: (3k+1)(3k+2)(3k+3)/(k+1); (3k+1)!/k! -> (3k+4)!/(k+1)!: (3k+2)(3k+3)(3k+4)/(k+1)
        let kf = kk as f64;
        let f = if deriv { (3.0 * kf + 2.0) * (3.0 * kf + 3.0) * (3.0 * kf + 4.0) / (kf + 1.0) } else { (3.0 * kf + 1.0) * (3.0 * kf + 2.0) * 3.0 };
        let next = t.mul_f(f * sign) / x3;
        let m = next.hi.abs();
        if m >= last {
            break;
        }
        last = m;
        if m <= TOL * 0.25 * s.hi.abs() {
            s = s + next;
            ok = true;
            break;
        }
        t = next;
        s = s + t;
    }
    if !ok {
        return None;
    }
    let v = if deriv {
        // Gi'(x) ~ -1/(pi x^2) sum (3k+1)!/(k!(3x^3)^k); Hi'(-x) ~ 1/(pi x^2) sum (-1)^k (3k+1)!/...
        let pre = k.inv_pi / ax.sqr();
        let v = pre * s;
        if x > 0.0 {
            -v
        } else {
            v
        }
    } else {
        k.inv_pi / ax * s
    };
    Some(v.to_f64())
}

/// A Scorer function at a machine argument.
pub(crate) fn scorer(zc: CD, real: bool, which: Scorer) -> Option<C64> {
    let z = zc.to_c64();
    let deriv = matches!(which, Scorer::GiPrime | Scorer::HiPrime);
    let hi = matches!(which, Scorer::Hi | Scorer::HiPrime);
    if z.re == 0.0 && z.im == 0.0 {
        return None;
    }
    let m = z.re.hypot(z.im);
    if m <= 9.5 || (hi && real && z.re > 0.0 && m <= 60.0) {
        if let Some(v) = scorer_series(zc, hi, deriv) {
            return Some(v);
        }
    }
    if !real || m <= 8.0 {
        return None;
    }
    let x = z.re;
    match (hi, x > 0.0) {
        // Gi, Gi' at +x and Hi, Hi' at -x: the asymptotic series
        (false, true) | (true, false) => scorer_asymptotic(x, deriv).map(C64::real),
        // Gi(-x) = Bi(-x) - Hi(-x)
        (false, false) => {
            let bi = if deriv { crate::machine::bessel_j::airy_bi_prime(x) } else { crate::machine::bessel_j::airy_bi(x) };
            let h = scorer_asymptotic(x, deriv)?;
            let v = bi - h;
            // Bi is rounded once: the difference keeps its digits unless it cancels
            (bi.is_finite() && (v.abs() >= 0.25 * bi.abs())).then_some(C64::real(v))
        }
        // Hi(x) = Bi(x) - Gi(x) (DLMF 9.12.12), Gi by its asymptotic series: Gi(x) ~ 1/(pi x) is far below Bi(x), so
        // the difference keeps the digits of Bi; an overflow of Bi is the overflow of Hi (round 37: scorer_hi(1e6) went
        // to the ball series of 2 10^9 terms and never returned)
        (true, true) => {
            let bi = if deriv { crate::machine::bessel_j::airy_bi_prime(x) } else { crate::machine::bessel_j::airy_bi(x) };
            // Bi(x) = e^zeta/(sqrt(pi) x^(1/4)) (1 + O(1/zeta)), zeta = 2 x^(3/2)/3 (DLMF 9.7.5): beyond the largest
            // double when its logarithm is (a kernel that fails there returns inf or NaN)
            let zeta = 2.0 / 3.0 * x.powf(1.5);
            let log_bi = zeta - 0.25 * x.ln() - 0.5 * std::f64::consts::PI.ln() + if deriv { 0.5 * x.ln() } else { 0.0 };
            if bi == f64::INFINITY || (!bi.is_finite() && log_bi > f64::MAX.ln() + 1.0) {
                return Some(C64::real(f64::INFINITY));
            }
            let g = scorer_asymptotic(x, deriv)?;
            let v = bi - g;
            (bi.is_finite() && v.abs() >= 0.25 * bi.abs()).then_some(C64::real(v))
        }
    }
}

// ---------------------------------------------------------------- Weierstrass functions

/// The lattice of invariants g2, g3 in complex double-double numbers (the method of [`crate::weierstrass`]: the
/// roots of 4 t^3 - g2 t - g3, the labelling with the largest Im tau, omega1 = K(m)/s, omega3 = i K(1 - m)/s, the
/// nome and eta1 = -pi^2 theta1'''(0)/(12 omega1 theta1'(0)), eta3 by Legendre's relation).
pub(crate) struct Lattice {
    /// the half-period omega1
    pub(crate) w1: CD,
    /// the half-period omega3
    pub(crate) w3: CD,
    tau: C64,
    q: CD,
    q4: CD,
    /// the quasi-period eta1 = zeta(omega1)
    pub(crate) eta1: CD,
    /// the quasi-period eta3 = zeta(omega3)
    pub(crate) eta3: CD,
}

fn pi_cd() -> CD {
    CD::real(crate::dd::PI)
}

/// K(m) = pi/(2 agm(1, sqrt(1 - m))), each geometric mean the root nearer to the arithmetic mean.
pub(crate) fn k_cd(m: CD) -> Option<CD> {
    let (mut a, mut b) = (CD::ONE, (CD::ONE - m).sqrt());
    // quadratic convergence: one step after |a - b| < 1e-17 |a| reaches the double-double rounding (a test at that
    // rounding itself was never met for some m, and the lattice was lost: inverse_weierstrass_p(5.0, [12, 7]))
    let mut last = false;
    for _ in 0..80 {
        let an = (a + b).scale(0.5);
        let mut bn = (a * b).sqrt();
        if (an - bn).abs() > (an + bn).abs() {
            bn = -bn;
        }
        let small = (an - bn).abs() <= 1e-17 * an.abs();
        a = an;
        b = bn;
        if last {
            return a.finite().then(|| pi_cd() / a.scale(2.0));
        }
        last = small;
    }
    None
}

/// The roots of 4 t^3 - g2 t - g3: machine Durand-Kerner, then Newton steps in double-double (`real`: real
/// invariants keep the real roots exactly real).
fn w_roots(g2: CD, g3: CD, real: bool) -> Option<[CD; 3]> {
    let (g2f, g3f) = (g2.to_c64(), g3.to_c64());
    let z = crate::weierstrass::roots_f64((g2f.re, g2f.im), (g3f.re, g3f.im))?;
    let scale = 1.0 + g2f.re.hypot(g2f.im).sqrt() + g3f.re.hypot(g3f.im).cbrt();
    let mut out = [CD::ZERO; 3];
    for (k, &(re, im)) in z.iter().enumerate() {
        let im = if real && im.abs() < 1e-9 * scale { 0.0 } else { im };
        let mut x = CD::c(C64::new(re, im));
        for _ in 0..4 {
            let x2 = x * x;
            let p = x2 * x.scale(4.0) - g2 * x - g3;
            let dp = x2.scale(12.0) - g2;
            x = x - p / dp;
        }
        if !x.finite() {
            return None;
        }
        out[k] = x;
    }
    for i in 0..3 {
        for j in 0..i {
            if (out[i] - out[j]).abs() < 1e-7 * scale {
                return None;
            }
        }
    }
    Some(out)
}

/// theta_1 and its first three derivatives at v for the nome q (q4 = q^(1/4)): 2 sum (-1)^n q^((n+1/2)^2) times
/// (sin, (2n+1) cos, -(2n+1)^2 sin, -(2n+1)^3 cos)((2n+1) v), with e^(i (2n+1) v) by repeated products.
fn theta1_cd(lat: &Lattice, v: CD) -> Option<[CD; 4]> {
    let e1 = v.mul_i().exp();
    let em1 = CD::ONE / e1;
    let e2 = e1 * e1;
    let em2 = em1 * em1;
    let (mut ep, mut em) = (e1, em1);
    let mut qk = lat.q4;
    let q2 = lat.q * lat.q;
    let mut ratio = q2;
    let mut s = [CD::ZERO; 4];
    let mut small = 0;
    for n in 0..400i64 {
        let k = (2 * n + 1) as f64;
        let d = ep - em;
        let sn = CD { re: d.im.mul_f(0.5), im: -d.re.mul_f(0.5) };
        let cs = (ep + em).scale(0.5);
        let c = if n % 2 == 0 { qk } else { -qk };
        let t = [c * sn, (c * cs).scale(k), (c * sn).scale(-k * k), (c * cs).scale(-k * k * k)];
        let mut big = 0.0f64;
        for j in 0..4 {
            s[j] = s[j] + t[j];
            big = big.max(t[j].abs() / s[j].abs().max(1e-300));
        }
        if big < 1e-34 {
            small += 1;
            if small >= 2 {
                return s.iter().all(|x| x.finite()).then(|| [s[0].scale(2.0), s[1].scale(2.0), s[2].scale(2.0), s[3].scale(2.0)]);
            }
        } else {
            small = 0;
        }
        qk = qk * ratio;
        ratio = ratio * q2;
        ep = ep * e2;
        em = em * em2;
    }
    None
}

/// The lattice of g2, g3.
pub(crate) fn w_lattice(g2: CD, g3: CD, real: bool) -> Option<Lattice> {
    let e = w_roots(g2, g3, real)?;
    const PERMS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    let ef: Vec<C64> = e.iter().map(|x| x.to_c64()).collect();
    let mut best: Option<([usize; 3], f64)> = None;
    for p in PERMS {
        let (e1, e2, e3) = (ef[p[0]], ef[p[1]], ef[p[2]]);
        let m = (e2 - e3) / (e1 - e3);
        let on_cut = m.im.abs() <= 1e-12 * m.re.hypot(m.im).max(1.0) && (m.re <= 0.0 || m.re >= 1.0);
        if on_cut {
            continue;
        }
        let k1 = crate::weierstrass::k_f64((1.0 - m.re, -m.im));
        let k0 = crate::weierstrass::k_f64((m.re, m.im));
        let tau = C64::new(-k1.1, k1.0) / C64::new(k0.0, k0.1);
        let im = tau.im.abs();
        if im.is_finite() && best.is_none_or(|(_, b)| im > b) {
            best = Some((p, im));
        }
    }
    let (p, im_tau) = best?;
    if im_tau < 0.05 {
        return None;
    }
    let (e1, e2, e3) = (e[p[0]], e[p[1]], e[p[2]]);
    let m = (e2 - e3) / (e1 - e3);
    let s = (e1 - e3).sqrt();
    let k = k_cd(m)?;
    let kp = k_cd(CD::ONE - m)?;
    let w1 = k / s;
    let mut w3 = kp.mul_i() / s;
    let mut tau_c = w3 / w1;
    if tau_c.im.hi < 0.0 {
        w3 = -w3;
        tau_c = -tau_c;
    }
    let ipt = (pi_cd() * tau_c).mul_i();
    let q = ipt.exp();
    let q4 = ipt.scale(0.25).exp();
    let mut lat = Lattice { w1, w3, tau: tau_c.to_c64(), q, q4, eta1: CD::ZERO, eta3: CD::ZERO };
    let th = theta1_cd(&lat, CD::ZERO)?;
    let pi = pi_cd();
    lat.eta1 = -(pi * pi * th[3]) / (w1 * th[1]).scale(12.0);
    lat.eta3 = (lat.eta1 * w3 - pi.scale(0.5).mul_i()) / w1;
    (lat.eta1.finite() && lat.eta3.finite()).then_some(lat)
}

/// p (0), p' (1), zeta (2) or sigma (3) at u on the lattice (the formulas of [`crate::weierstrass`]); the error
/// estimate is the rounding of the largest part of the final combination.
pub(crate) fn w_eval(lat: &Lattice, u: CD, which: u8) -> Option<C64> {
    let pi = pi_cd();
    let c = pi / lat.w1.scale(2.0);
    let v = c * u;
    let vf = v.to_c64();
    let pf = std::f64::consts::PI;
    let n = (vf.im / (pf * lat.tau.im)).round();
    let vr = (vf.re - n * pf * lat.tau.re, vf.im - n * pf * lat.tau.im);
    let m = (vr.0 / pf).round();
    if !n.is_finite() || !m.is_finite() || n.abs() > 1e9 || m.abs() > 1e9 {
        return None;
    }
    let u1 = u - lat.w1.scale(2.0 * m) - lat.w3.scale(2.0 * n);
    let v1 = c * u1;
    let [t0, t1, t2, t3] = theta1_cd(lat, v1)?;
    if t0.abs() == 0.0 {
        return None;
    }
    let ratio = lat.eta1 / lat.w1;
    let (val, parts) = match which {
        0 => {
            let a = -ratio;
            let b = c * c * ((t2 * t0 - t1 * t1) / (t0 * t0));
            (a - b, a.abs() + b.abs() + (c * c * t1 * t1 / (t0 * t0)).abs())
        }
        1 => {
            let num = t3 * t0 * t0 - (t0 * t1 * t2).scale(3.0) + (t1 * t1 * t1).scale(2.0);
            let v = -(c * c * c * (num / (t0 * t0 * t0)));
            let mag = (c * c * c).abs() * ((t3 * t0 * t0).abs() + 3.0 * (t0 * t1 * t2).abs() + 2.0 * (t1 * t1 * t1).abs()) / (t0 * t0 * t0).abs();
            (v, mag)
        }
        2 => {
            let a = ratio * u1;
            let b = c * (t1 / t0);
            let d = lat.eta1.scale(2.0 * m) + lat.eta3.scale(2.0 * n);
            (a + b + d, a.abs() + b.abs() + d.abs())
        }
        _ => {
            let th0 = theta1_cd(lat, CD::ZERO)?;
            let g = (lat.eta1 * u1 * u1 / lat.w1.scale(2.0)).exp();
            let s1 = g * t0 / (c * th0[1]);
            let (mi, ni) = (m as i64, n as i64);
            let sign = if (mi + ni + mi * ni).rem_euclid(2) == 0 { 1.0 } else { -1.0 };
            let h = lat.eta1.scale(2.0 * m) + lat.eta3.scale(2.0 * n);
            let at = u1 + lat.w1.scale(m) + lat.w3.scale(n);
            let ex = h * at;
            let v = (ex.exp() * s1).scale(sign);
            // the exponents' own roundings are absolute: |exponent| U relative
            (v, v.abs() * (1.0 + ex.abs() + (lat.eta1 * u1 * u1 / lat.w1).abs()))
        }
    };
    let err = parts * U * 64.0;
    (val.finite() && err <= TOL * val.abs()).then(|| val.to_c64())
}

/// A Weierstrass function at machine arguments: which = 0 p, 1 p', 2 zeta, 3 sigma.
pub(crate) fn weierstrass(u: CD, g2: CD, g3: CD, real: bool, which: u8) -> Option<C64> {
    let lat = w_lattice(g2, g3, real)?;
    w_eval(&lat, u, which)
}

/// inverse_weierstrass_p at machine arguments (the method of [`crate::weierstrass`]): u = R_F(p - e1, p - e2,
/// p - e3), the sign with p'(u) = q (or the principal sqrt(4 p^3 - g2 p - g3)), real arguments a negative real u
/// moved by the real period.
pub(crate) fn inverse_p(p: CD, g2: CD, g3: CD, q: Option<CD>, real: bool) -> Option<C64> {
    let e = w_roots(g2, g3, real)?;
    let mut u = carlson_rf_cd(p - e[0], p - e[1], p - e[2])?;
    let chop = |u: CD| if real && u.im.hi.abs() <= 1e-26 * u.re.hi.abs() { CD::real(u.re) } else { u };
    u = chop(u);
    let target = match q {
        Some(q) => q,
        None => (p * p * p.scale(4.0) - g2 * p - g3).sqrt(),
    };
    // the sign from p'(u); without a lattice the ball path decides
    let lat = w_lattice(g2, g3, real)?;
    let dp = CD::c(w_eval(&lat, u, 1)?);
    if (dp - target).abs() > (dp + target).abs() {
        u = -u;
    }
    if q.is_none() && real && u.im.hi == 0.0 && u.re.hi < 0.0 {
        let mut reals: Vec<(f64, usize)> = e.iter().enumerate().filter(|(_, r)| r.im.hi == 0.0).map(|(k, r)| (r.re.hi, k)).collect();
        reals.sort_by(|a, b| b.0.total_cmp(&a.0));
        if let Some(&(_, k)) = reals.first() {
            let o: Vec<CD> = (0..3).filter(|&j| j != k).map(|j| e[k] - e[j]).collect();
            u = u + carlson_rf_cd(CD::ZERO, o[0], o[1])?.scale(2.0);
        }
    }
    let u = chop(u);
    u.finite().then(|| u.to_c64())
}

/// R_F(x, y, z) by Carlson's duplication (DLMF 19.36.1) in double-double numbers, the principal roots: the
/// arguments are moved until their spread about the mean is below 1e-5, then the series of order 7 (omitted terms
/// O(1e-40)).
pub(crate) fn carlson_rf_cd(x: CD, y: CD, z: CD) -> Option<CD> {
    let (mut x, mut y, mut z) = (x, y, z);
    for _ in 0..200 {
        let a = (x + y + z) / CD::f(3.0);
        let am = a.abs();
        if am == 0.0 || !a.finite() {
            return None;
        }
        let d = (x - a).abs().max((y - a).abs()).max((z - a).abs());
        if d <= 1e-5 * am {
            let dx = (a - x) / a;
            let dy = (a - y) / a;
            let dz = -(dx + dy);
            let e2 = dx * dy - dz * dz;
            let e3 = dx * dy * dz;
            let s = CD::ONE - e2 / CD::f(10.0) + e3 / CD::f(14.0) + e2 * e2 / CD::f(24.0) - (e2 * e3).scale(3.0) / CD::f(44.0) - (e2 * e2 * e2).scale(5.0) / CD::f(208.0)
                + (e3 * e3).scale(3.0) / CD::f(104.0)
                + e2 * e2 * e3 / CD::f(16.0);
            return Some(s / a.sqrt());
        }
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let l = sx * sy + sy * sz + sz * sx;
        x = (x + l).scale(0.25);
        y = (y + l).scale(0.25);
        z = (z + l).scale(0.25);
    }
    None
}

// ---------------------------------------------------------------- Mathieu functions

/// (y, y') at z of y'' + (a - 2 q cos 2z) y = 0 with y(0) = y0, y'(0) = d0 (the method of [`crate::mathieu`]:
/// Taylor steps with cos(2 z0 + 2t) = sum (2t)^i/i! cos(2 z0 + i pi/2) expanded at each point), and the largest
/// |y| + |y'| met (for the error estimate).
fn mathieu_solve(a: CD, q: CD, z: CD, y0: f64, d0: f64) -> Option<(CD, CD, f64)> {
    let size = a.abs() + 2.0 * q.abs();
    let hmax = 0.5f64.min(1.0 / (1.0 + size).sqrt());
    let n = ((z.abs() / hmax).ceil() as usize).max(1);
    if n > 20_000 {
        return None;
    }
    let h = z / CD::f(n as f64);
    let mh = h.abs();
    let q2 = q.scale(2.0);
    let (mut y, mut dy) = (CD::f(y0), CD::f(d0));
    let mut big = y.abs() + dy.abs();
    let mut cs: Vec<CD> = Vec::with_capacity(64);
    let mut ys: Vec<CD> = Vec::with_capacity(64);
    for s in 0..n {
        let z2 = (h * CD::f(s as f64)).scale(2.0);
        let (c, sn) = (z2.cos(), z2.sin());
        cs.clear();
        ys.clear();
        ys.push(y);
        ys.push(dy);
        let (mut val, mut der) = (CD::ZERO, CD::ZERO);
        let mut hk = CD::ONE;
        let mut hk1 = CD::ZERO; // h^(k-1)
        let mut pk = DD::ONE;
        let mut small = 0;
        let mut ok = false;
        for k in 0..400usize {
            if k > 0 {
                pk = pk.mul_f(2.0) / DD::new(k as f64);
            }
            let trig = match k % 4 {
                0 => c,
                1 => -sn,
                2 => -c,
                _ => sn,
            };
            cs.push(trig.mul_dd(pk));
            if k >= 2 {
                let m = k - 2;
                let mut conv = CD::ZERO;
                for i in 0..=m {
                    conv = conv + cs[i] * ys[m - i];
                }
                ys.push((q2 * conv - a * ys[m]) / CD::f((k * (k - 1)) as f64));
            }
            let t = ys[k] * hk;
            val = val + t;
            if k > 0 {
                der = der + (ys[k] * hk1).scale(k as f64);
            }
            hk1 = hk;
            hk = hk * h;
            let mt = t.abs() * (1.0 + k as f64 / mh.max(1e-300));
            if k >= 4 && mt <= 1e-34 * (val.abs() + der.abs() * mh).max(1e-300) {
                small += 1;
                if small >= 3 {
                    ok = true;
                    break;
                }
            } else {
                small = 0;
            }
        }
        if !ok || !val.finite() || !der.finite() {
            return None;
        }
        y = val;
        dy = der;
        big = big.max(y.abs() + dy.abs());
    }
    Some((y, dy, big * n as f64))
}

/// acos w = -i log(w + i sqrt(1 - w^2)).
fn acos_cd(w: CD) -> CD {
    let r = (CD::ONE - w * w).sqrt();
    let l = (w + r.mul_i()).log();
    CD { re: l.im, im: -l.re }
}

/// The Floquet normalization of [`crate::mathieu`] (ce(0) for the even function, se'(0) for the odd one), `None`
/// near a characteristic value (cos(pi nu) near +-1, where nu is ill-conditioned).
fn mathieu_norm(a: CD, q: CD, even: bool) -> Option<CD> {
    let pi = pi_cd();
    let (y_pi, _, _) = mathieu_solve(a, q, pi, 1.0, 0.0)?;
    if (CD::ONE - y_pi * y_pi).abs() < 1e-10 {
        return None;
    }
    let nu1 = acos_cd(y_pi) / pi;
    let sa = a.sqrt().to_c64();
    let n1 = nu1.to_c64();
    let mut best = (f64::INFINITY, 1.0f64, 0.0f64);
    for sg in [1.0f64, -1.0] {
        let m = ((sa.re - sg * n1.re) / 2.0).round();
        let dist = (sg * n1.re + 2.0 * m - sa.re).abs();
        if dist < best.0 {
            best = (dist, sg, m);
        }
    }
    let nu = nu1.scale(best.1) + CD::f(2.0 * best.2);
    let kmax = 34 + (q.abs().sqrt() * 3.0) as i64;
    let d = |k: i64| {
        let m = nu + CD::f(2.0 * k as f64);
        a - m * m
    };
    let mut r = vec![CD::ZERO; (kmax + 2) as usize];
    let mut sr = vec![CD::ZERO; (kmax + 2) as usize];
    for (sign, ratios) in [(1i64, &mut r), (-1i64, &mut sr)] {
        for k in (1..=kmax).rev() {
            let den = d(sign * k) - q * ratios[(k + 1) as usize];
            if den.abs() == 0.0 {
                return None;
            }
            ratios[k as usize] = q / den;
        }
    }
    let (mut s2, mut s1, mut sd) = (CD::ONE, CD::ONE, nu);
    for (sign, ratios) in [(1i64, &r), (-1i64, &sr)] {
        let mut c = CD::ONE;
        for k in 1..=kmax {
            c = c * ratios[k as usize];
            s2 = s2 + c * c;
            s1 = s1 + c;
            sd = sd + (nu + CD::f(2.0 * (sign * k) as f64)) * c;
        }
    }
    let v = (if even { s1 } else { sd }) / s2.sqrt();
    v.finite().then_some(v)
}

/// A Mathieu function at machine arguments: which = 0 C, 1 S, 2 S'. The normalization depends on a and q only and
/// is kept for the last pair (a plot varies z).
pub(crate) fn mathieu(a: CD, q: CD, z: CD, which: u8) -> Option<C64> {
    thread_local! {
        static LAST: std::cell::Cell<Option<(CD, CD, bool, CD)>> = const { std::cell::Cell::new(None) };
    }
    // 0 = ce, 1 = se, 2 = se', 3 = ce'
    let even = which == 0 || which == 3;
    let norm = match LAST.with(|c| c.get()) {
        Some((la, lq, le, v)) if la == a && lq == q && le == even => v,
        _ => {
            let v = mathieu_norm(a, q, even)?;
            LAST.with(|c| c.set(Some((a, q, even, v))));
            v
        }
    };
    let (y0, d0) = if even { (1.0, 0.0) } else { (0.0, 1.0) };
    let (y, d, big) = mathieu_solve(a, q, z, y0, d0)?;
    let v = if which >= 2 { d } else { y } * norm;
    let err = big * norm.abs() * U * 1e3;
    (v.finite() && err <= TOL * v.abs()).then(|| v.to_c64())
}

/// mathieu_characteristic_a(r, q) at a real machine q (the method of [`crate::mathieu`]: the eigenvalue of the
/// truncated tridiagonal matrix at integer r, then the secant method on y'(pi/2) (even r) or y(pi/2) (odd r) of the
/// even solution, or on y(pi) - cos(pi r) at other r), in double-double numbers. With `odd`, the characteristic
/// value b_r(q) at an even integer r >= 2: the secant method on y(pi/2) of the odd solution (y(0) = 0, y'(0) = 1).
pub(crate) fn mathieu_char_a(r: f64, q: DD, start: f64, odd: bool) -> Option<f64> {
    let rf = r.abs();
    let int_r = rf.fract() == 0.0 && rf < 1e4;
    let qd = CD::real(q);
    let half_pi = pi_cd().scale(0.5);
    let cpr = if int_r { CD::ZERO } else { CD::real(CD::sin_cos_real(DD::new(rf) * crate::dd::PI).1) };
    let cond = |av: DD| -> Option<DD> {
        let a = CD::real(av);
        if odd {
            let (y, _, _) = mathieu_solve(a, qd, half_pi, 0.0, 1.0)?;
            Some(y.re)
        } else if int_r {
            let (y, d, _) = mathieu_solve(a, qd, half_pi, 1.0, 0.0)?;
            Some(if (rf as u64).is_multiple_of(2) { d.re } else { y.re })
        } else {
            let (y, _, _) = mathieu_solve(a, qd, pi_cd(), 1.0, 0.0)?;
            Some((y - cpr).re)
        }
    };
    let mut x1 = DD::new(start);
    let mut x0 = x1 + DD::new(1e-7 * (1.0 + start.abs()));
    let mut f0 = cond(x0)?;
    for _ in 0..60 {
        let f1 = cond(x1)?;
        let df = f1 - f0;
        if df.hi == 0.0 {
            return (f1.hi == 0.0).then(|| x1.to_f64());
        }
        let x2 = x1 - f1 * (x1 - x0) / df;
        let step = (x2 - x1).hi.abs();
        x0 = x1;
        f0 = f1;
        x1 = x2;
        if !x1.hi.is_finite() {
            return None;
        }
        if step <= 1e-28 * x1.hi.abs().max(1.0) {
            return Some(x1.to_f64());
        }
    }
    None
}

// ---------------------------------------------------------------- Appell functions

/// Is z exactly a non-positive integer (as a machine number)?
fn nonpos_int(z: CD) -> Option<u64> {
    let v = z.to_c64();
    (v.im == 0.0 && v.re <= 0.0 && v.re == v.re.floor() && v.re > -1e6).then(|| (-v.re) as u64)
}

/// The Gauss series 2F1(a, b; c; z) in double-double numbers and the sum of the moduli of its terms: the power
/// series for |z| < 0.9, Pfaff's transformation (1 - z)^-a 2F1(a, c - b; c; z/(z - 1)) when that argument is below
/// 0.9 (DLMF 15.8.1), `None` otherwise or at a pole of c.
pub(crate) fn f21_cd(a: CD, b: CD, c: CD, z: CD) -> Option<(CD, f64)> {
    let ends = [nonpos_int(a), nonpos_int(b)].into_iter().flatten().min();
    if let Some(m) = nonpos_int(c) {
        if ends.is_none_or(|n| n > m) {
            return None;
        }
    }
    let series = |a: CD, b: CD, z: CD| -> Option<(CD, f64)> {
        let ends = [nonpos_int(a), nonpos_int(b)].into_iter().flatten().min();
        let (mut t, mut s, mut big) = (CD::ONE, CD::ONE, 1.0f64);
        let mut small = 0;
        for k in 0..5000u64 {
            if ends == Some(k) {
                return Some((s, big));
            }
            let kf = CD::f(k as f64);
            t = t * (a + kf) * (b + kf) * z / ((c + kf) * CD::f((k + 1) as f64));
            s = s + t;
            big = big.max(t.abs());
            if t.abs() <= 1e-34 * s.abs() {
                small += 1;
                if small >= 2 {
                    return s.finite().then_some((s, big));
                }
            } else {
                small = 0;
            }
        }
        None
    };
    let w = z / (z - CD::ONE);
    // the smaller of |z| and |z/(z - 1)| (at Re z < 0 the direct series alternates and cancels for large a)
    if (z.abs() < 0.9 && z.abs() <= w.abs()) || ends.is_some() && z.abs() < 1e3 {
        return series(a, b, z);
    }
    if w.abs() < 0.9 {
        let pre = (CD::ONE - z).pow(-a);
        let (s, big) = series(a, c - b, w)?;
        return Some((pre * s, big * pre.abs()));
    }
    None
}

/// An Appell function at machine arguments (which: 0 F1, 1 F2, 2 F3, 3 F4; `p` the parameters without x, y): the
/// rows of [`crate::appell`] (the variable of smaller modulus outside, below 1) times Gauss functions of the other.
pub(crate) fn appell(which: u8, p: &[CD], x: CD, y: CD) -> Option<C64> {
    let (mut x, mut y) = (x, y);
    let mut p = p.to_vec();
    // the outer variable: the one of smaller modulus (the rows of F1, F3 stay bounded in m); for F2 the rows grow
    // like (1 - y)^-m, so the order with the smaller |x|/|1 - y|
    let swap = if which == 1 { x.abs() / (CD::ONE - y).abs() > y.abs() / (CD::ONE - x).abs() } else { x.abs() > y.abs() };
    if swap {
        std::mem::swap(&mut x, &mut y);
        p = match which {
            0 => vec![p[0], p[2], p[1], p[3]],
            1 => vec![p[0], p[2], p[1], p[4], p[3]],
            2 => vec![p[1], p[0], p[3], p[2], p[4]],
            _ => vec![p[0], p[1], p[3], p[2]],
        };
    }
    if x.abs() >= 0.95 {
        return None;
    }
    let mut sum = CD::ZERO;
    let mut coef = CD::ONE;
    let mut big = 0.0f64;
    let mut small = 0;
    for m in 0..3000u64 {
        let mf = CD::f(m as f64);
        let m1 = CD::f((m + 1) as f64);
        let (ratio, g) = match which {
            0 => ((p[0] + mf) * (p[1] + mf) / ((p[3] + mf) * m1), [p[0] + mf, p[2], p[3] + mf]),
            1 => ((p[0] + mf) * (p[1] + mf) / ((p[3] + mf) * m1), [p[0] + mf, p[2], p[4]]),
            2 => ((p[0] + mf) * (p[2] + mf) / ((p[4] + mf) * m1), [p[1], p[3], p[4] + mf]),
            _ => ((p[0] + mf) * (p[1] + mf) / ((p[2] + mf) * m1), [p[0] + mf, p[1] + mf, p[3]]),
        };
        let (f, fb) = f21_cd(g[0], g[1], g[2], y)?;
        let t = coef * f;
        sum = sum + t;
        big = big.max(coef.abs() * fb).max(t.abs());
        if t.abs() <= 1e-33 * sum.abs() {
            small += 1;
            if small >= 3 {
                break;
            }
        } else {
            small = 0;
        }
        coef = coef * ratio * x;
        if coef.abs() == 0.0 {
            break;
        }
        if m == 2999 || !sum.finite() {
            return None;
        }
    }
    let err = big * U * 1e3;
    (sum.finite() && err <= TOL * sum.abs()).then(|| sum.to_c64())
}

// ---------------------------------------------------------------- Heun functions

/// p(z0 + t) in powers of t.
fn shift_poly(p: &[CD], z0: CD) -> Vec<CD> {
    let mut c = p.to_vec();
    let n = c.len();
    for i in 0..n {
        for j in (i..n.saturating_sub(1)).rev() {
            c[j] = c[j] + z0 * c[j + 1];
        }
    }
    c
}

/// One Taylor (or, at a regular singular start, Frobenius exponent-0) step of A y'' + B y' + C y = 0 from z0 by h
/// (the recurrence of [`crate::heun`]): (y(z0 + h), y'(z0 + h)) and the largest term.
#[allow(clippy::too_many_arguments)]
fn heun_step(a: &[CD], b: &[CD], c: &[CD], z0: CD, h: CD, y0: CD, d0: CD, regular: bool) -> Option<(CD, CD, f64)> {
    let a = shift_poly(a, z0);
    let b = shift_poly(b, z0);
    let c = shift_poly(c, z0);
    let mut y: Vec<CD> = if regular { vec![CD::ONE] } else { vec![y0, d0] };
    let mh = h.abs();
    let (mut val, mut der) = (CD::ZERO, CD::ZERO);
    let mut hk = CD::ONE;
    let mut hk1 = CD::ZERO; // k h^(k-1)
    let mut small = 0;
    let mut big = 0.0f64;
    let len = a.len().max(b.len()).max(c.len());
    for k in 0..6000usize {
        if k >= y.len() {
            let n = if regular { k - 1 } else { k - 2 };
            let mut rest = CD::ZERO;
            let mut lead = CD::ZERO;
            for i in 0..len {
                if let Some(&ai) = a.get(i) {
                    if n + 2 >= i {
                        let j = n + 2 - i;
                        let f = (j * j.saturating_sub(1)) as f64;
                        if j > k {
                            if ai.abs() != 0.0 {
                                return None;
                            }
                        } else if j == k {
                            lead = lead + ai.scale(f);
                        } else if f != 0.0 {
                            rest = rest + (ai * y[j]).scale(f);
                        }
                    }
                }
                if let Some(&bi) = b.get(i) {
                    if n + 1 >= i {
                        let j = n + 1 - i;
                        if j == k {
                            lead = lead + bi.scale(j as f64);
                        } else if j > 0 {
                            rest = rest + (bi * y[j]).scale(j as f64);
                        }
                    }
                }
                if let Some(&ci) = c.get(i) {
                    if n >= i {
                        let j = n - i;
                        if j == k {
                            lead = lead + ci;
                        } else {
                            rest = rest + ci * y[j];
                        }
                    }
                }
            }
            if lead.abs() == 0.0 {
                return None;
            }
            y.push(-(rest / lead));
        }
        let t = y[k] * hk;
        val = val + t;
        if k > 0 {
            der = der + y[k] * hk1;
        }
        hk1 = hk.scale((k + 1) as f64);
        hk = hk * h;
        let mt = t.abs().max((y[k] * hk1).abs() * mh);
        big = big.max(mt);
        if k >= 4 && mt <= 1e-34 * (val.abs() + der.abs() * mh).max(1e-300) {
            small += 1;
            if small >= 3 {
                return (val.finite() && der.finite()).then_some((val, der, big));
            }
        } else {
            small = 0;
        }
    }
    None
}

/// A Heun function at machine arguments: kind 0 G, 1 C, 2 B, 3 D, 4 T (the equations and the path of
/// [`crate::heun`]: re-expansion at points at most half the distance to the nearest singularity and 1/2 apart, a
/// detour above every finite singularity within 0.3 of the segment), and whether the path made a detour.
pub(crate) fn heun(kind: u8, p: &[CD], z: CD) -> Option<(C64, bool)> {
    let one = CD::ONE;
    let (start, regular) = match kind {
        3 => (C64::real(1.0), false),
        4 => (C64::real(0.0), false),
        _ => (C64::real(0.0), true),
    };
    type Equation = (Vec<CD>, Vec<CD>, Vec<CD>, Vec<(f64, f64)>);
    let (ae, be, ce, sing): Equation = match kind {
        0 => {
            let [a, q, al, be, ga, de] = p[..] else { return None };
            let ep = al + be + one - ga - de;
            let af = a.to_c64();
            (vec![CD::ZERO, a, -(one + a), one], vec![ga * a, -(ga * (one + a) + de * a + ep), ga + de + ep], vec![-q, al * be], vec![(0.0, 0.0), (1.0, 0.0), (af.re, af.im)])
        }
        1 => {
            let [q, al, ga, de, ep] = p[..] else { return None };
            (vec![CD::ZERO, -one, one], vec![-ga, ga + de - ep, ep], vec![-q, al], vec![(0.0, 0.0), (1.0, 0.0)])
        }
        2 => {
            let [q, al, ga, de, ep] = p[..] else { return None };
            (vec![CD::ZERO, one], vec![ga, de, ep], vec![-q, al], vec![(0.0, 0.0)])
        }
        3 => {
            let [q, al, ga, de, ep] = p[..] else { return None };
            (vec![CD::ZERO, CD::ZERO, one], vec![ga, de, ep], vec![-q, al], vec![(0.0, 0.0)])
        }
        _ => {
            let [q, al, ga, de, ep] = p[..] else { return None };
            (vec![one], vec![ga, de, ep], vec![-q, al], vec![])
        }
    };
    let zf = z.to_c64();
    let pts = crate::heun::path((start.re, start.im), (zf.re, zf.im), &sing)?;
    let detour = pts.len() > 2;
    let d = |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).hypot(p.1 - q.1);
    let mut cur = (start.re, start.im);
    let mut cur_t = CD::c(start);
    let (mut y, mut dy) = (CD::ONE, CD::ZERO);
    let mut first = regular;
    let mut steps = 0;
    let mut big = 1.0f64;
    for (i, &target) in pts.iter().enumerate().skip(1) {
        let last = i == pts.len() - 1;
        loop {
            let r = sing.iter().map(|&s| d(s, cur)).filter(|&x| x > 1e-12).fold(f64::INFINITY, f64::min);
            let hmax = (0.5 * r).min(0.5);
            let dist = d(cur, target);
            let (next, next_t) = if dist <= hmax {
                (target, if last { z } else { CD::c(C64::new(target.0, target.1)) })
            } else {
                let f = hmax / dist;
                let p = (cur.0 + f * (target.0 - cur.0), cur.1 + f * (target.1 - cur.1));
                (p, CD::c(C64::new(p.0, p.1)))
            };
            let (v, dv, b) = heun_step(&ae, &be, &ce, cur_t, next_t - cur_t, y, dy, first)?;
            first = false;
            big = big.max(b);
            y = v;
            dy = dv;
            cur = next;
            cur_t = next_t;
            steps += 1;
            if steps > 4000 {
                return None;
            }
            if next == target {
                break;
            }
        }
    }
    let err = big * U * 1e3 * steps as f64;
    (y.finite() && err <= TOL * y.abs()).then(|| (y.to_c64(), detour))
}

// ---------------------------------------------------------------- modified Lommel function t

/// t_(mu,nu)(z) = sum_k z^(mu + 1 + 2k)/prod_(j<=k) ((mu + 2j + 1)^2 - nu^2) (the series of [`crate::cover28`]),
/// in double-double numbers; `None` at a zero denominator.
pub(crate) fn lommel_t1(mu: CD, nu: CD, z: CD) -> Option<C64> {
    lommel_t1_cd(mu, nu, z).map(CD::to_c64)
}

/// [`lommel_t1`] before rounding to machine numbers.
pub(crate) fn lommel_t1_cd(mu: CD, nu: CD, z: CD) -> Option<CD> {
    let nu2 = nu * nu;
    let den = |j: f64| {
        let b = mu + CD::f(2.0 * j + 1.0);
        b * b - nu2
    };
    let d0 = den(0.0);
    if d0.abs() == 0.0 {
        return None;
    }
    let z2 = z * z;
    let mut t = CD::ONE / d0;
    let (mut s, mut big) = (CD::ZERO, 0.0f64);
    let mut small = 0;
    for k in 0..5000 {
        s = s + t;
        big = big.max(t.abs());
        if t.abs() <= 1e-34 * s.abs() {
            small += 1;
            if small >= 2 {
                break;
            }
        } else {
            small = 0;
        }
        let d = den((k + 1) as f64);
        if d.abs() == 0.0 || k == 4999 {
            return None;
        }
        t = t * z2 / d;
    }
    let v = z.pow(mu + CD::ONE) * s;
    (v.finite() && big * U * 1e2 <= TOL * s.abs()).then_some(v)
}

/// The Jacobi theta function theta_a(u, q) or its u-derivative in double-double numbers (round 37): the series of
/// DLMF 20.2.1-20.2.4 with the principal q^(1/4) (the ball path's terms), e^(+-i m u) by products. `None` for |q| >=
/// 0.99 or when the largest term leaves fewer than the machine digits of the value.
pub(crate) fn theta(a: u8, u: CD, q: CD, deriv: bool) -> Option<C64> {
    if !(1..=4).contains(&a) || q.abs() >= 0.99 || q.abs() == 0.0 {
        return None;
    }
    let odd = a <= 2;
    // e^(i u), and the step e^(2iu) between the frequencies 2k+1 (odd) or 2k (even)
    let e1 = u.mul_i().exp();
    let em1 = CD::ONE / e1;
    let (e2, em2) = (e1 * e1, em1 * em1);
    let (mut ep, mut em) = if odd { (e1, em1) } else { (e2, em2) };
    // q^(k(k+1)) or q^(k^2) with ratios q^(2k+2) or q^(2k+1)
    let q2 = q * q;
    let (mut qk, mut ratio) = if odd { (CD::ONE, q2) } else { (q, q2 * q) };
    let mut s = CD::ZERO;
    let mut big = 0.0f64;
    let mut small = 0;
    let lq = q.abs().ln();
    let start = if odd { 0 } else { 1 };
    for k in start..2000i64 {
        let m = if odd { 2 * k + 1 } else { 2 * k } as f64;
        let sine = matches!((a, deriv), (1, false) | (2, true) | (3, true) | (4, true));
        // sin = (e - 1/e)/(2i), cos = (e + 1/e)/2
        let trig = if sine { (ep - em).scale(0.5).mul_i().scale(-1.0) } else { (ep + em).scale(0.5) };
        let mut t = qk * trig;
        if deriv {
            t = t.scale(m);
        }
        if (a == 1 || a == 4) && k % 2 == 1 {
            t = -t;
        }
        s = s + t;
        let at = t.abs();
        if !at.is_finite() {
            return None;
        }
        big = big.max(at);
        let decreasing = (2.0 * k as f64 + 1.0) * lq + 2.0 * u.im.hi.abs() < 0.0;
        if decreasing && at <= 1e-34 * s.abs().max(1e-300) {
            small += 1;
            if small == 3 {
                break;
            }
        } else {
            small = 0;
        }
        if k == 1999 {
            return None;
        }
        qk = qk * ratio;
        ratio = ratio * q2;
        ep = ep * e2;
        em = em * em2;
    }
    let v = if odd {
        let q4 = q.log().scale(0.25).exp();
        (q4 * s).scale(if a == 2 && deriv { -2.0 } else { 2.0 })
    } else if deriv {
        s.scale(-2.0)
    } else {
        CD::ONE + s.scale(2.0)
    };
    let size = v.abs().max(if odd || deriv { 0.0 } else { 1.0 });
    (v.finite() && big * U * 1e2 <= TOL * size).then(|| v.to_c64())
}

/// The q-Pochhammer product (x; q)_inf = prod_(k >= 0) (1 - x q^k) in double-double numbers, |q| < 0.99 (round 37).
/// The rounding error of a factor 1 - x q^k is |x q^k| U, so sum |x q^k|/|1 - x q^k| bounds the relative error in
/// units of U; `None` when that leaves fewer than the machine digits, or after 100000 factors.
pub(crate) fn q_product(x: CD, q: CD) -> Option<C64> {
    if q.abs() >= 0.99 {
        return None;
    }
    let (mut p, mut t) = (CD::ONE, x);
    let mut cond = 1.0f64;
    for _ in 0..100_000 {
        let f = CD::ONE - t;
        let (at, af) = (t.abs(), f.abs());
        if af == 0.0 {
            return Some(C64::real(0.0));
        }
        cond += at / af + 1.0;
        p = p * f;
        if at <= 1e-34 {
            return (p.finite() && cond * U * 1e2 <= TOL).then(|| p.to_c64());
        }
        t = t * q;
    }
    None
}

/// The basic hypergeometric series r phi s (DLMF 17.4.1) in double-double numbers by the term ratio prod (1 - a_i
/// q^n)/prod (1 - b_j q^n)/(1 - q^(n+1)) ((-1) q^n)^(1 + s - r) z (round 37), |q| < 1. `None` after 20000 terms, at
/// a zero denominator, or when the largest term leaves fewer than the machine digits of the sum.
pub(crate) fn q_hyper(a: &[CD], b: &[CD], q: CD, z: CD) -> Option<C64> {
    if q.abs() >= 1.0 {
        return None;
    }
    let e = 1 + b.len() as i32 - a.len() as i32;
    let (mut s, mut t, mut qn) = (CD::ONE, CD::ONE, CD::ONE);
    let mut big = 1.0f64;
    let mut small = 0;
    for n in 0..20_000 {
        let qn1 = qn * q;
        let mut num = t * z;
        for &ai in a {
            num = num * (CD::ONE - ai * qn);
        }
        let mut den = CD::ONE - qn1;
        for &bj in b {
            den = den * (CD::ONE - bj * qn);
        }
        if e != 0 {
            let mut fe = CD::ONE;
            for _ in 0..e.unsigned_abs() {
                fe = fe * (-qn);
            }
            num = if e > 0 { num * fe } else { num / fe };
        }
        if den.abs() == 0.0 {
            return None;
        }
        t = num / den;
        s = s + t;
        qn = qn1;
        let at = t.abs();
        if !(at.is_finite() && s.finite()) {
            return None;
        }
        big = big.max(at);
        if at == 0.0 {
            break;
        }
        if at <= 1e-34 * s.abs() {
            small += 1;
            if small >= 3 {
                break;
            }
        } else {
            small = 0;
        }
        if n == 19_999 {
            return None;
        }
    }
    (big * U * 1e2 <= TOL * s.abs()).then(|| s.to_c64())
}

/// Lommel's s_(mu,nu)(z) at machine arguments (round 37): the series of [`crate::cover`] in double-double numbers,
/// z^(mu+1) sum_k (-z^2)^k / prod_(j<=k) ((mu + 2j + 1)^2 - nu^2), with the cancellation bound of `lommel_t1`.
pub(crate) fn lommel_s1(mu: CD, nu: CD, z: CD) -> Option<C64> {
    lommel_s1_cd(mu, nu, z).map(CD::to_c64)
}

/// [`lommel_s1`] before rounding to machine numbers.
pub(crate) fn lommel_s1_cd(mu: CD, nu: CD, z: CD) -> Option<CD> {
    let nu2 = nu * nu;
    let den = |j: f64| {
        let b = mu + CD::f(2.0 * j + 1.0);
        b * b - nu2
    };
    let d0 = den(0.0);
    if d0.abs() == 0.0 {
        return None;
    }
    let mz2 = -(z * z);
    let mut t = CD::ONE / d0;
    let (mut s, mut big) = (CD::ZERO, 0.0f64);
    let mut small = 0;
    for k in 0..5000 {
        s = s + t;
        big = big.max(t.abs());
        if t.abs() <= 1e-34 * s.abs() {
            small += 1;
            if small >= 2 {
                break;
            }
        } else {
            small = 0;
        }
        let d = den((k + 1) as f64);
        if d.abs() == 0.0 || k == 4999 {
            return None;
        }
        t = t * mz2 / d;
    }
    let v = z.pow(mu + CD::ONE) * s;
    (v.finite() && big * U * 1e2 <= TOL * s.abs()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scorer_values() {
        // Gi(0.5) (the ball value at 20 digits: 0.24472104327655820301); asymptotic Gi(12)
        let gi = scorer(CD::f(0.5), true, Scorer::Gi).unwrap().re;
        assert!((gi - 0.244_721_043_276_558_2).abs() < 1e-16, "{gi}");
        assert!(scorer(CD::f(16.0), true, Scorer::Gi).is_some());
    }
}
