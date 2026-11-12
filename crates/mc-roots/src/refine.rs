//! Refinement of a machine root with balls (`digits: d`), and the verification of a simple real root in one
//! variable by an interval Newton step.
//!
//! The refinement is Newton's method on numbers: the iterate is a vector of exact balls (zero radius, the midpoints
//! of the previous step), the residual and Jacobian are evaluated with balls at the working precision (the
//! function's constants were evaluated to that precision before, decision R.9), and the linear system is solved in
//! ball arithmetic; only the midpoint of each correction is kept. Without a symbolic Jacobian the Jacobian is a
//! forward difference with steps of half the working precision, which still gains half the digits per step.
//!
//! **Verification** (decision R.10): for a real root x̃ of one equation with a symbolic derivative, take the ball
//! X = [x̃ - r, x̃ + r] and compute N = x̃ - f(x̃) / f'(X) in ball arithmetic, where f'(X) encloses the derivative on
//! all of X. If N lies in the interior of X, X contains exactly one root, and it lies in N (the interval Newton
//! theorem: f' has no zero in X, and every root in X is in N). r starts a few hundred ulps wide and grows by 2^16
//! up to three times.
use crate::func::TermFn;
use crate::linalg::solve_num;
use mc_arith::num::{stopped, Ball, Float, Mag, Nat};
use mc_arith::{Arith, Num, Real};
use mc_core::{Heap, T};
use mc_eval::{NativeCx, Res};
use std::cmp::Ordering;

/// The result of a refinement.
pub(crate) enum Refine {
    /// the refined point; `x` balls at the working precision
    Done(Refined),
    /// a value at this point is not a number
    NotNumeric(Vec<Num>),
    /// the arithmetic was stopped
    Interrupted,
}

/// A refined point.
pub(crate) struct Refined {
    /// the root, as balls whose radius is the error estimate (or, verified, the enclosure)
    pub x: Vec<Num>,
    /// the error estimate met the goals
    pub converged: bool,
    /// log2 of the error estimate
    pub err_log2: f64,
    /// the root was verified (decision R.10)
    pub verified: bool,
}

/// The exact value of a machine real as a float (`None` for NaN and infinities).
pub(crate) fn float_of(v: f64) -> Option<Float> {
    if !v.is_finite() {
        return None;
    }
    if v == 0.0 {
        return Some(Float::zero());
    }
    let bits = v.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (man, exp) = if e == 0 { (frac, -1074) } else { (frac | (1u64 << 52), e - 1075) };
    Some(Float::new(v < 0.0, Nat::from_u64(man), exp))
}

/// A real as an exact ball at precision `p` (its midpoint; exact reals rounded to `p`).
fn exact_real(r: &Real, p: u64) -> Option<Real> {
    Some(match r {
        Real::Machine(v) => Real::Ball(Ball::exact(float_of(*v)?), p),
        Real::Ball(b, _) => Real::Ball(Ball::exact(b.mid.clone()), p),
        Real::Exact(_) => match r.to_kind(mc_arith::Kind::Ball, p) {
            Real::Ball(b, _) => Real::Ball(Ball::exact(b.mid), p),
            other => other,
        },
    })
}

/// A number as an exact ball (or a complex number of exact balls) at precision `p`: its midpoint.
pub(crate) fn exact_num(n: &Num, p: u64) -> Option<Num> {
    Some(match n {
        Num::Real(r) => Num::Real(exact_real(r, p)?),
        Num::Complex(a, b) => Num::Complex(exact_real(a, p)?, exact_real(b, p)?),
    })
}

/// log2 of the magnitude of a real (-inf for zero), to within one.
fn log2_real(r: &Real) -> f64 {
    match r {
        Real::Ball(b, _) if b.mid.is_zero() => f64::NEG_INFINITY,
        Real::Ball(b, _) => b.mid.top() as f64,
        _ => r.to_f64().abs().log2(),
    }
}

/// log2 of the magnitude of a number (the larger part for complex numbers).
pub(crate) fn log2_num(n: &Num) -> f64 {
    match n {
        Num::Real(r) => log2_real(r),
        Num::Complex(a, b) => log2_real(a).max(log2_real(b)),
    }
}

/// The largest log2 magnitude of the entries.
fn log2_max(v: &[Num]) -> f64 {
    v.iter().map(log2_num).fold(f64::NEG_INFINITY, f64::max)
}

/// An exact ball given the radius 2^e, at precision `p`.
fn with_radius(r: &Real, e: i64, p: u64) -> Real {
    match r {
        Real::Ball(b, _) => Real::Ball(Ball { mid: b.mid.clone(), rad: Mag::pow2(e) }, p),
        other => other.clone(),
    }
}

/// A number with radius 2^e on each part.
fn num_with_radius(n: &Num, e: i64, p: u64) -> Num {
    match n {
        Num::Real(r) => Num::Real(with_radius(r, e, p)),
        Num::Complex(a, b) => Num::Complex(with_radius(a, e, p), with_radius(b, e, p)),
    }
}

/// A ball rounded to `p` bits: the rounding error joins the radius.
fn round_real(r: &Real, p: u64) -> Real {
    match r {
        Real::Ball(b, q) if *q > p => {
            let (mid, e) = Float::round(b.mid.neg, b.mid.man.clone(), b.mid.exp, p);
            Real::Ball(Ball { mid, rad: b.rad.add(&e) }, p)
        }
        other => other.clone(),
    }
}

/// An upper bound of half a unit in the (d + 1)-th significant digit of a number whose first digit has the
/// decimal exponent `exp10`: 0.5 10^(exp10 - d).
fn half_unit_after(exp10: i64, d: u64) -> Mag {
    let k = exp10 - d as i64;
    let ten = |e: u64| Float::new(false, Nat::pow(10, e), 0);
    let half = Mag::pow2(-1);
    if k >= 0 {
        half.mul(&Mag::from_float_up(&ten(k as u64)))
    } else {
        half.div_up(&Mag::from_float_down(&ten(k.unsigned_abs())))
    }
}

/// A real result under `digits: d`: rounded to the result precision `p` (which keeps guard bits beyond d
/// digits), and showing at most d digits: a ball that would print more gets the radius of half a unit in the
/// (d + 1)-th digit, the least radius that keeps the printer from showing d + 1 digits (decision N1.7).
fn result_real(r: &Real, p: u64, d: u64) -> Real {
    let r = round_real(r, p);
    let Real::Ball(b, q) = &r else { return r };
    match mc_arith::text::ball_digits(b, *q) {
        Some(dg) if dg.digits.len() as u64 > d && !b.mid.is_zero() => {
            let rad = half_unit_after(dg.exp10, d);
            let smaller = !b.rad.is_zero() && (rad.e, rad.m) < (b.rad.e, b.rad.m);
            let rad = if smaller { b.rad } else { rad };
            Real::Ball(Ball { mid: b.mid.clone(), rad }, *q)
        }
        _ => r,
    }
}

/// A result under `digits: d` ([`result_real`] on each part).
pub(crate) fn result_num(n: &Num, p: u64, d: u64) -> Num {
    match n {
        Num::Real(r) => Num::Real(result_real(r, p, d)),
        Num::Complex(a, b) => Num::Complex(result_real(a, p, d), result_real(b, p, d)),
    }
}

/// The terms of numbers.
fn terms<'h>(ar: &Arith<'h, Heap>, x: &[Num]) -> Vec<T<'h>> {
    x.iter().map(|v| ar.number(v)).collect()
}

/// A real or complex ball that contains 0 (and is not an exact zero).
fn noise(v: &Num) -> bool {
    let z = |r: &Real| match r {
        Real::Ball(b, _) => b.contains_zero(),
        r => r.is_zero(),
    };
    match v {
        Num::Real(r) => z(r),
        Num::Complex(a, b) => z(a) && z(b),
    }
}

/// Newton's method on balls from the machine root `start`, at `p` bits. `jac` is the symbolic Jacobian (n·n
/// outputs), `None` for forward differences. `goals` are (absolute, relative) error goals as log2.
#[allow(clippy::too_many_arguments)]
pub(crate) fn refine<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    f: &mut TermFn<'h>,
    mut jac: Option<&mut TermFn<'h>>,
    start: &[Num],
    p: u64,
    goals: (f64, f64),
    max_iterations: usize,
    verify: bool,
) -> Res<Refine> {
    let n = start.len();
    let Some(mut x) = start.iter().map(|v| exact_num(v, p)).collect::<Option<Vec<Num>>>() else {
        return Ok(Refine::NotNumeric(start.to_vec()));
    };
    let floor = -2.0 * p as f64;
    let mut prev = f64::INFINITY;
    let mut step = f64::INFINITY;
    let mut full = false;
    let mut stalled = 0;
    let mut linear = false;
    for it in 0..max_iterations {
        cx.tick(1 + p / 64)?;
        let Some(fx) = f.eval_terms(cx, ar, &terms(ar, &x))? else { return Ok(Refine::NotNumeric(x)) };
        // a residual that is exactly zero (a ball with midpoint 0): the point is a root, there is no step to take
        if fx.iter().all(Num::is_zero) {
            step = f64::NEG_INFINITY;
            full = true;
            break;
        }
        // a residual ball around 0 is the noise level of f (at a multiple root it is reached long before the full
        // precision): the last step estimates the error, no Newton step from noise is taken
        if it > 1 && linear && fx.iter().all(noise) {
            break;
        }
        let j = match jac.as_deref_mut() {
            Some(jf) => jf.eval_terms(cx, ar, &terms(ar, &x))?,
            None => None,
        };
        let j = match j {
            Some(j) => j,
            None => match differences(cx, ar, f, &x, &fx, p)? {
                Some(j) => j,
                None => return Ok(Refine::NotNumeric(x)),
            },
        };
        let neg: Vec<Num> = fx.iter().map(Num::neg).collect();
        let d = solve_num(n, &j, &neg);
        if stopped() {
            cx.tick(0)?;
            return Ok(Refine::Interrupted);
        }
        let Some(d) = d else { break };
        let Some(next) = x.iter().zip(&d).map(|(a, b)| exact_num(&a.add(b), p)).collect::<Option<Vec<Num>>>() else {
            return Ok(Refine::NotNumeric(x));
        };
        x = next;
        let before = step;
        step = log2_max(&d);
        // linear convergence: the step shrinks by less than 2^4 (a quadratic one doubles the exponent)
        linear = before.is_finite() && before - step < 4.0;
        let xn = log2_max(&x).max(floor);
        if step == f64::NEG_INFINITY || step <= xn - (p as f64 - 4.0) {
            full = true;
            break;
        }
        // linear convergence (a multiple root) still shrinks the step; no shrinking at all is stagnation. The step is
        // a power of two (an exponent), so a triple root (ratio 2/3) keeps it equal on one step in two: stagnation is
        // four steps without a new least step (one step without shrinking stopped (x - 2)^3 at 2 - 1.5e-8)
        if step < prev - 0.1 {
            prev = step;
            stalled = 0;
        } else if it >= 2 {
            stalled += 1;
            if stalled >= 4 {
                break;
            }
        }
    }
    let xn = log2_max(&x).max(floor);
    // a full-precision step leaves an error far below the last bits; otherwise the last step estimates it
    // (the error of linear convergence at a root of multiplicity k is (k - 1) times the step: up to k = 5 by step + 2)
    let err_log2 = if full { xn - (p as f64 - 2.0) } else if linear { step + 2.0 } else { step + 1.0 };
    // no step was ever computed (a singular Jacobian at the start): no digits are claimed
    let err_log2 = if err_log2.is_finite() { err_log2 } else { xn.max(0.0) };
    let converged = err_log2 <= goals.0.max(goals.1 + xn);
    let e = err_log2.ceil().clamp(-(i64::MAX / 4) as f64, (i64::MAX / 4) as f64) as i64;
    let mut out: Vec<Num> = x.iter().map(|v| num_with_radius(v, e, p)).collect();
    let mut verified = false;
    if verify && converged && n == 1 {
        if let (Some(jf), Num::Real(Real::Ball(m, _))) = (jac, &x[0]) {
            if let Some(nb) = interval_newton(cx, ar, f, jf, m, xn, p)? {
                out[0] = Num::Real(Real::Ball(nb, p));
                verified = true;
            }
        }
    }
    if stopped() {
        cx.tick(0)?;
        return Ok(Refine::Interrupted);
    }
    Ok(Refine::Done(Refined { x: out, converged, err_log2, verified }))
}

/// The forward-difference Jacobian on balls, with steps 2^(e - p/2) where 2^e is about the variable's magnitude
/// (at least 1). Exact steps, so the only error is the truncation, about half the working precision.
fn differences<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    f: &mut TermFn<'h>,
    x: &[Num],
    fx: &[Num],
    p: u64,
) -> Res<Option<Vec<Num>>> {
    let n = x.len();
    let m = fx.len();
    let mut j = vec![Num::int(0); m * n];
    for c in 0..n {
        cx.tick(1)?;
        let e = log2_num(&x[c]).max(0.0) as i64 - (p / 2) as i64;
        let h = Num::Real(Real::Ball(Ball::exact(Float::new(false, Nat::from_u64(1), e)), p));
        let inv = Num::Real(Real::Ball(Ball::exact(Float::new(false, Nat::from_u64(1), -e)), p));
        let mut pt = x.to_vec();
        pt[c] = pt[c].add(&h);
        let Some(fc) = f.eval_terms(cx, ar, &terms(ar, &pt))? else { return Ok(None) };
        for r in 0..m {
            j[r * n + c] = fc[r].add(&fx[r].neg()).mul(&inv);
        }
    }
    Ok(Some(j))
}

/// The interval Newton step around the midpoint `m` (magnitude about 2^xn): the enclosure N of the unique root in
/// a ball X around `m`, if one of the radii tried proves it.
fn interval_newton<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    f: &mut TermFn<'h>,
    df: &mut TermFn<'h>,
    m: &Ball,
    xn: f64,
    p: u64,
) -> Res<Option<Ball>> {
    let mid = Real::Ball(Ball::exact(m.mid.clone()), p);
    let Some(fm) = f.eval_terms(cx, ar, &[ar.number(&Num::Real(mid.clone()))])? else { return Ok(None) };
    let Some(Num::Real(fm)) = fm.into_iter().next() else { return Ok(None) };
    let base = (xn - (p as f64 - 8.0)).ceil() as i64;
    for k in 0..4 {
        cx.tick(1)?;
        let r = Mag::pow2(base + 16 * k);
        let x = Num::Real(Real::Ball(Ball { mid: m.mid.clone(), rad: r }, p));
        let Some(dx) = df.eval_terms(cx, ar, &[ar.number(&x)])? else { return Ok(None) };
        let Some(Num::Real(dx)) = dx.into_iter().next() else { return Ok(None) };
        // a derivative enclosure containing zero only widens with X: give up
        let Ok(q) = fm.div(&dx) else { return Ok(None) };
        let Real::Ball(nb, _) = mid.sub(&q).to_kind(mc_arith::Kind::Ball, p) else { continue };
        // N inside the open ball X: |mid(N) - m| + rad(N) < r
        let reach = nb.mid.sub_exact(&m.mid).abs().add_exact(&nb.rad.to_float());
        if reach.cmp_exact(&r.to_float()) == Ordering::Less {
            return Ok(Some(nb));
        }
    }
    Ok(None)
}
