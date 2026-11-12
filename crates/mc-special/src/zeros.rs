//! Zeros of special functions: `bessel_j_zero`, `bessel_y_zero`, `airy_ai_zero`,
//! `zeta_zero`.
//!
//! - `bessel_j_zero(nu, k)`: the k-th positive zero of J_nu; for a non-integer k = m - t (m = ceil k, 0 < t < 1) the
//!   m-th positive zero of the cylinder function J_nu cos(pi t) - Y_nu sin(pi t), so that the zero is continuous in k
//!   (the phase form of the zeros, DLMF 10.21(iii)); `bessel_j_zero(nu, k, x0)`: the k-th zero above x0.
//!   `bessel_y_zero` likewise with Y_nu cos(pi t) + J_nu sin(pi t), whose first zero enters the origin at t = 1/2 (so for
//!   t >= 1/2 the m-th zero of the family is its (m - 1)-th positive zero).
//! - `airy_ai_zero(k)`: the k-th (negative) zero of Ai, `airy_ai_zero(k, x0)` the k-th zero below x0.
//! - `zeta_zero(k)`: the k-th nontrivial zero 1/2 + i t_k of zeta in the upper half plane (k > 0; k < 0 its
//!   conjugate), found as the k-th sign change of the Riemann-Siegel Z function (DLMF 25.10.3), which is real on the
//!   critical line; `zeta_zero(k, t0)` the k-th zero above height t0 (k < 0: the |k|-th below it), the zeros ordered by imaginary part. (Zeros off the line or Z sign changes missed
//!   between sampling points, a Lehmer pair closer than the step, would be miscounted; the step is a sixth of the mean
//!   spacing 2 pi/log(t/(2 pi)).)
//!
//! Method: the sign changes are counted in machine arithmetic along the line, the bracket bisected to machine
//! precision, and the zero refined by Newton's method at the working precision on exact midpoints, the last step on
//! the balls so that the radii of the arguments reach the zero (the derivatives: J_nu' = J_(nu-1) - (nu/x) J_nu,
//! DLMF 10.6.2; Z' by a centred difference at the working precision). Exact arguments stay unevaluated; `n` gives the
//! values.
use crate::cover::{ev, mag, mid_term, numeric, parts, pi_at, work};
use mc_api::T;
use mc_arith::{Num, Real};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

/// The k-th sign change of f from `x0` in steps of `step(x)` (signed), bisected to machine precision; `None` beyond
/// `limit` steps.
fn kth_root(f: &mut dyn FnMut(f64) -> Option<f64>, x0: f64, k: usize, step: &dyn Fn(f64) -> f64, limit: usize) -> Option<f64> {
    let mut x = x0;
    let mut fx = f(x)?;
    let mut count = 0;
    for _ in 0..limit {
        let xn = x + step(x);
        let fn_ = f(xn)?;
        if fx == 0.0 || (fx < 0.0) != (fn_ < 0.0) && fn_ != 0.0 || fn_ == 0.0 {
            count += 1;
            if count == k {
                // bisect [x, xn]
                let (mut a, mut b, mut fa) = (x, xn, fx);
                if fn_ == 0.0 {
                    return Some(xn);
                }
                if fa == 0.0 {
                    return Some(a);
                }
                for _ in 0..200 {
                    let m = 0.5 * (a + b);
                    if m == a || m == b {
                        break;
                    }
                    let fm = f(m)?;
                    if fm == 0.0 {
                        return Some(m);
                    }
                    if (fm < 0.0) == (fa < 0.0) {
                        a = m;
                        fa = fm;
                    } else {
                        b = m;
                    }
                }
                return Some(0.5 * (a + b));
            }
            if fn_ == 0.0 {
                // step past an exact zero
                x = xn + step(xn) * 1e-6;
                fx = f(x)?;
                continue;
            }
        }
        x = xn;
        fx = fn_;
    }
    None
}

/// A machine number as a ball at w bits.
fn ball_at<'h>(fx: &Fx<'_, '_, '_, 'h>, v: f64, w: u64) -> T<'h> {
    fx.ar.number(&Num::Real(Real::Ball(mc_arith::num::Ball::exact(crate::ball::float_of(v)), w)))
}

/// Newton's method from the machine root x0 for f with derivative df at the working precision: midpoints until the
/// last step.
#[allow(clippy::type_complexity)]
fn newton<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    x0: f64,
    w: u64,
    f: &mut dyn FnMut(&mut Fx<'_, '_, '_, 'h>, T<'h>) -> Res<Option<(T<'h>, T<'h>)>>,
) -> R<'h> {
    let steps = ((w as f64 / 40.0).log2().ceil().max(0.0) as usize) + 3;
    let mut x = ball_at(fx, x0, w);
    for s in 0..=steps {
        let Some((v, d)) = f(fx, x)? else { return Ok(None) };
        if fx.num(d).is_none_or(|n| n.is_zero()) {
            return Ok(None);
        }
        let nx = fx.eval(fx.sub(x, fx.div(v, d)))?;
        if fx.num(nx).is_none() {
            return Ok(None);
        }
        x = if s < steps { mid_term(fx, nx) } else { nx };
    }
    Ok(Some(x))
}

/// An order that is an integer carried as a ball of radius 0 (from `n`): the exact integer, so that Y takes its
/// integer branch.
fn exact_order<'h>(fx: &Fx<'_, '_, '_, 'h>, nu: T<'h>) -> T<'h> {
    match fx.num(nu) {
        Some(Num::Real(Real::Ball(b, _))) if b.rad.is_zero() => {
            let v = crate::ball::f64_of(&b.mid);
            if v.fract() == 0.0 && v.abs() < 1e9 { fx.int(v as i64) } else { nu }
        }
        Some(Num::Real(Real::Machine(v))) if v.fract() == 0.0 && v.abs() < 1e9 => fx.int(v as i64),
        _ => nu,
    }
}

/// bessel_j_zero (`y` false) or bessel_y_zero.
fn bessel_zero<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], y: bool) -> R<'h> {
    if !numeric(fx, a) {
        return Ok(None);
    }
    let (Some(nu), Some(k)) = (fx.f64_of(a[0]), fx.f64_of(a[1])) else { return Ok(None) };
    let x0 = if a.len() == 3 { fx.f64_of(a[2]).unwrap_or(f64::NAN) } else { 0.0 };
    if k.is_nan() || k <= 0.0 || !x0.is_finite() || x0 < 0.0 || nu.is_nan() || nu <= -1.0 || k > 1e5 {
        return Ok(None);
    }
    let m = k.ceil();
    let t = m - k;
    let (ct, st) = ((std::f64::consts::PI * t).cos(), (std::f64::consts::PI * t).sin());
    let mut f = |x: f64| -> Option<f64> {
        let j = crate::machine::bessel_j::bessel_j(nu, x);
        let yy = if t == 0.0 && !y { 0.0 } else { crate::machine::bessel_j::bessel_y(nu, x) };
        let v = if y { yy * ct + j * st } else { j * ct - yy * st };
        v.is_finite().then_some(v)
    };
    let start = x0.max(1e-3);
    let step = |_x: f64| 0.1;
    // Y cos(pi t) + J sin(pi t) tends to -infinity cos(pi t) at 0: for t >= 1/2 its first zero has gone into the
    // origin, and the m-th zero of the family is its (m - 1)-th positive zero (k = 1/3 has none)
    let count = if y && t >= 0.5 && a.len() == 2 { m as usize - 1 } else { m as usize };
    if count == 0 {
        return Ok(None);
    }
    let Some(r) = kth_root(&mut f, start, count, &step, 2_000_000) else { return Ok(None) };
    let Some((wk, xs)) = work(fx, a, 16) else { return Ok(None) };
    let nu_t = exact_order(fx, xs[0]);
    let nu_t = if fx.num(a[0]).is_some_and(|n| n.is_exact()) { a[0] } else { nu_t };
    // cos(pi t), sin(pi t) at the working precision, t = ceil(k) - k
    let (c, s) = if t == 0.0 {
        (fx.int(1), fx.int(0))
    } else {
        let pi = pi_at(fx, wk.w)?;
        let tt = fx.eval(fx.sub(fx.int(m as i64), xs[1]))?;
        (ev(fx, "cos", &[fx.mul(&[pi, tt])])?, ev(fx, "sin", &[fx.mul(&[pi, tt])])?)
    };
    let mut fun = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>| -> Res<Option<(T<'h>, T<'h>)>> {
        let num_m1 = fx.sub(nu_t, fx.int(1));
        let j = ev(fx, "bessel_j", &[nu_t, x])?;
        let j1 = ev(fx, "bessel_j", &[num_m1, x])?;
        let dj = fx.sub(j1, fx.mul(&[fx.div(nu_t, x), j]));
        let (v, d) = if t == 0.0 && !y {
            (j, dj)
        } else {
            let yy = ev(fx, "bessel_y", &[nu_t, x])?;
            let y1 = ev(fx, "bessel_y", &[num_m1, x])?;
            let dy = fx.sub(y1, fx.mul(&[fx.div(nu_t, x), yy]));
            if y {
                (fx.add(&[fx.mul(&[yy, c]), fx.mul(&[j, s])]), fx.add(&[fx.mul(&[dy, c]), fx.mul(&[dj, s])]))
            } else {
                (fx.sub(fx.mul(&[j, c]), fx.mul(&[yy, s])), fx.sub(fx.mul(&[dj, c]), fx.mul(&[dy, s])))
            }
        };
        let (v, d) = (fx.eval(v)?, fx.eval(d)?);
        Ok((fx.num(v).is_some() && fx.num(d).is_some()).then_some((v, d)))
    };
    let Some(z) = newton(fx, r, wk.w, &mut fun)? else { return Ok(None) };
    Ok(wk.out(fx, z, true))
}

fn bessel_j_zero<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    bessel_zero(fx, a, false)
}
fn bessel_y_zero<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    bessel_zero(fx, a, true)
}

fn airy_ai_zero<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    airy_zero(fx, a, false)
}

/// airy_bi_zero(k) (round 29): the k-th (negative) zero of Bi, as for Ai.
fn airy_bi_zero<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    airy_zero(fx, a, true)
}

/// The k-th zero of Ai (`bi` false) or Bi below x0 (default 0): sign changes counted in machine numbers, then
/// Newton's method at the working precision.
fn airy_zero<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], bi: bool) -> R<'h> {
    if !numeric(fx, a) {
        return Ok(None);
    }
    let Some(k) = fx.f64_of(a[0]) else { return Ok(None) };
    let x0 = if a.len() == 2 { fx.f64_of(a[1]).unwrap_or(f64::NAN) } else { 0.0 };
    if k.is_nan() || k < 1.0 || k.fract() != 0.0 || !x0.is_finite() || k > 1e5 {
        return Ok(None);
    }
    let mut f = |x: f64| {
        let v = if bi { crate::machine::bessel_j::airy_bi(x) } else { crate::machine::bessel_j::airy_ai(x) };
        v.is_finite().then_some(v)
    };
    // the zeros are below -2.33 (Ai) and -1.17 (Bi): start at min(x0, 0); the spacing near -x is about pi/sqrt(-x)
    let start = x0.min(0.0) - 1e-9;
    let step = |x: f64| -0.1f64.min(0.5 / (1.0 + x.abs()).sqrt());
    let Some(r) = kth_root(&mut f, start, k as usize, &step, 10_000_000) else { return Ok(None) };
    let Some((wk, _)) = work(fx, a, 16) else { return Ok(None) };
    let mut fun = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>| -> Res<Option<(T<'h>, T<'h>)>> {
        let v = ev(fx, if bi { "airy_bi" } else { "airy_ai" }, &[x])?;
        let d = ev(fx, if bi { "airy_bi_prime" } else { "airy_ai_prime" }, &[x])?;
        Ok((fx.num(v).is_some() && fx.num(d).is_some()).then_some((v, d)))
    };
    let Some(z) = newton(fx, r, wk.w, &mut fun)? else { return Ok(None) };
    Ok(wk.out(fx, z, true))
}

fn zeta_zero<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if !numeric(fx, a) {
        return Ok(None);
    }
    let Some(kf) = fx.f64_of(a[0]) else { return Ok(None) };
    if kf.fract() != 0.0 || kf == 0.0 || kf.abs() > 1e6 {
        return Ok(None);
    }
    let mut conj = kf < 0.0;
    let mut k = kf.abs() as usize;
    let t0 = if a.len() == 2 { fx.f64_of(a[1]).unwrap_or(f64::NAN) } else { 0.0 };
    if !t0.is_finite() {
        return Ok(None);
    }
    // Z(t) in machine numbers
    let mut zf = |t: f64| -> Option<f64> {
        let c = fx.call("riemann_siegel_z", &[fx.real(t)]);
        let v = fx.eval(c).ok()?;
        fx.f64_of(v).filter(|v| v.is_finite())
    };
    let step = |t: f64| {
        let spacing = 2.0 * std::f64::consts::PI / (t / (2.0 * std::f64::consts::PI)).ln().max(1.0);
        spacing / 6.0
    };
    let mut start = t0.abs().max(1.0);
    // zeta_zero(k, t0): the zeros are the points 1/2 +- i t_j (t_j > 0 ascending, Z even), ordered by imaginary part:
    // k > 0 the k-th above t0 counting upwards, k < 0 the |k|-th below t0 counting downwards
    if a.len() == 2 && (t0 < 0.0 && !conj || t0 > 0.0 && conj) {
        // the zeros between t0 and 0 (t0 < 0, going up) or between 0 and t0 (t0 > 0, going down): m = #{t_j < |t0|}
        let mut m = 0usize;
        let (mut x, mut fv) = (1.0f64, zf(1.0));
        while x < t0.abs() {
            let xn = x + step(x);
            let fnv = zf(xn);
            let (Some(p), Some(q)) = (fv, fnv) else { return Ok(None) };
            if (p < 0.0) != (q < 0.0) && xn <= t0.abs() {
                m += 1;
            }
            x = xn;
            fv = Some(q);
        }
        start = 1.0;
        if k <= m {
            k = m + 1 - k;
            // t0 < 0 going up: the negative zero -t_(m+1-k); t0 > 0 going down: the positive zero t_(m+1-k)
            conj = t0 < 0.0;
        } else {
            k -= m;
            conj = t0 > 0.0;
        }
    } else if a.len() == 2 && t0 <= 0.0 && conj {
        // below t0 <= 0: the negatives of the zeros above |t0|
        start = t0.abs().max(1.0);
    }
    let Some(r) = kth_root(&mut zf, start, k, &step, 50_000_000) else { return Ok(None) };
    let Some((wk, _)) = work(fx, a, 16) else { return Ok(None) };
    let h_bits = (wk.w / 2 + 8) as i32;
    let mut fun = |fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>| -> Res<Option<(T<'h>, T<'h>)>> {
        let v = ev(fx, "riemann_siegel_z", &[t])?;
        // Z' by a centred difference with h = 2^-(w/2 + 8): error h^2 Z'''/6, far below the Newton tolerance
        let h = fx.pow(fx.int(2), fx.int(-(h_bits as i64)));
        let hp = fx.eval(fx.add(&[t, h]))?;
        let hm = fx.eval(fx.sub(t, h))?;
        let vp = ev(fx, "riemann_siegel_z", &[hp])?;
        let vm = ev(fx, "riemann_siegel_z", &[hm])?;
        let d = fx.eval(fx.div(fx.sub(vp, vm), fx.mul(&[fx.int(2), h])))?;
        Ok((fx.num(v).is_some() && fx.num(d).is_some()).then_some((v, d)))
    };
    // the working precision raised by the bits the difference quotient cancels
    let w = wk.w + wk.w / 2 + 16;
    let Some(t) = newton(fx, r, w, &mut fun)? else { return Ok(None) };
    let _ = mag;
    let half = match fx.num(t) {
        Some(Num::Real(Real::Machine(_))) => fx.real(0.5),
        _ => fx.rat(1, 2),
    };
    let t = if conj { fx.neg(t) } else { t };
    let z = fx.eval(fx.add(&[half, fx.mul(&[fx.i(), t])]))?;
    let _ = parts;
    Ok(wk.out(fx, z, false))
}

macro_rules! def {
    ($name:literal, $ar:expr, $e:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: true, exact: Some($e), numeric: None, summary: $s }
    };
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("bessel_j_zero", &[2, 3], bessel_j_zero, "bessel_j_zero(nu, k): the k-th positive zero of the Bessel function J_nu; bessel_j_zero(nu, k, x0): the k-th zero above x0"),
    def!("bessel_y_zero", &[2, 3], bessel_y_zero, "bessel_y_zero(nu, k): the k-th positive zero of the Bessel function Y_nu; bessel_y_zero(nu, k, x0): the k-th zero above x0"),
    def!("airy_ai_zero", &[1, 2], airy_ai_zero, "airy_ai_zero(k): the k-th zero of the Airy function Ai (negative); airy_ai_zero(k, x0): the k-th zero below x0"),
    def!("airy_bi_zero", &[1, 2], airy_bi_zero, "airy_bi_zero(k): the k-th zero of the Airy function Bi (negative); airy_bi_zero(k, x0): the k-th zero below x0"),
    def!("zeta_zero", &[1, 2], zeta_zero, "zeta_zero(k): the k-th nontrivial zero 1/2 + i t of the zeta function above the real axis (k < 0: below); zeta_zero(k, t0): the k-th zero above height t0"),
];
