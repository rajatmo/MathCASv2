#![allow(clippy::neg_cmp_op_on_partial_ord)]
//! Klein's invariant J, the modular lambda function and the Dedekind eta function at any numeric point of the upper
//! half-plane, at any precision.
//!
//! Method: the point is reduced to the fundamental domain (|Re t| <= 1/2, |t| >= 1 up to a margin of 1e-12) by the
//! generators t -> t - n and t -> -1/t of the modular group (Apostol, "Modular Functions and Dirichlet Series in
//! Number Theory", 2nd ed., theorem 2.3), in the arithmetic of the argument: exact for a Gaussian rational, balls
//! otherwise, so the radius carries what the reduction loses near a cusp. In the fundamental domain Im t >= 0.866 and
//! the nome |q| = e^(-pi Im t) is at most 0.066: the theta constants (DLMF 20.2) and Euler's pentagonal series of eta
//! (DLMF 27.14.5) converge fast, and their tails are bounded by geometric series and added to the radius. The value is
//! carried back through the steps: J is invariant (DLMF 23.15.8), lambda(t + 1) = lambda/(lambda - 1) and
//! lambda(-1/t) = 1 - lambda (DLMF 23.15.6, both involutions), eta(t + 1) = e^(i pi/12) eta(t) and eta(-1/t) =
//! sqrt(-i t) eta(t) (DLMF 27.14.(iv)).
//!
//! Precision: an exact point (or the exact midpoint of a machine number) loses about log2(|tau|^2/Im tau) bits in the
//! reduction; the working precision adds twice that, and a machine result is recomputed with more bits until its
//! radius is below 2^-56 of its value. Beyond 10^4 bits of loss (Im tau below about 10^-3000 of |tau|^2) the call stays
//! unevaluated; a machine result outside the range of machine numbers (J and lambda near a cusp are exponentially
//! large or small) does too.
use crate::cover::{map_balls, parts, pi_at, work};
use mc_api::T;
use mc_arith::num::Mag;
use mc_arith::{Kind, Num, Real};
use mc_eval::Res;
use mc_numeric::formula::Fx;

type R<'h> = Res<Option<T<'h>>>;

/// Which modular function.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Modular {
    J,
    Lambda,
    Eta,
}

/// A step of the reduction: t -> t - n (n mod 24 kept), or t -> -1/t (with the t it was applied to).
enum Step<'h> {
    Shift(i64),
    Invert(T<'h>),
}

/// tau reduced to the fundamental domain, with the steps taken; `None` when a step has no number.
fn reduce<'h>(fx: &mut Fx<'_, '_, '_, 'h>, tau: T<'h>) -> Res<Option<(T<'h>, Vec<Step<'h>>)>> {
    let mut t = tau;
    let mut steps = vec![];
    for it in 0..200_000u32 {
        if it % 32 == 31 {
            fx.tick(1)?;
        }
        let Some((x, y)) = parts(fx, t) else { return Ok(None) };
        if !(y > 0.0) || !x.is_finite() {
            return Ok(None);
        }
        let n = x.round();
        let x = if n != 0.0 {
            let nt = if n.abs() < 1e15 {
                fx.int(n as i64)
            } else {
                // a real part beyond 2^50: its nearest integer exactly
                let re = fx.eval(fx.call("re", &[t]))?;
                let r = fx.eval(fx.call("round", &[re]))?;
                if !fx.num(r).is_some_and(|v| v.kind() == Kind::Exact) {
                    return Ok(None);
                }
                r
            };
            let m = fx.eval(fx.call("mod", &[nt, fx.int(24)]))?;
            let Some(m) = fx.f64_of(m) else { return Ok(None) };
            t = fx.eval(fx.sub(t, nt))?;
            steps.push(Step::Shift(m as i64));
            match parts(fx, t) {
                Some(p) => p.0,
                None => return Ok(None),
            }
        } else {
            x
        };
        if x * x + y * y < 1.0 - 1e-12 {
            steps.push(Step::Invert(t));
            t = fx.eval(fx.neg(fx.div(fx.int(1), t)))?;
        } else {
            return Ok(Some((t, steps)));
        }
    }
    Ok(None)
}

/// t widened by 2^e in each part.
fn widen<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, e: f64) -> T<'h> {
    let m = Mag::pow2(e.ceil().max(-1e9) as i64);
    map_balls(fx, t, |b| crate::ball::widen(b.clone(), &m))
}

/// The value of `which` at a point t of the fundamental domain, at w bits.
fn reduced_value<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>, which: Modular, w: u64) -> R<'h> {
    let Some((_, y)) = parts(fx, t) else { return Ok(None) };
    let t = match fx.num(t) {
        Some(n) if n.kind() == Kind::Exact => fx.ar.number(&n.to_kind(Kind::Ball, w)),
        Some(_) => t,
        None => return Ok(None),
    };
    let pi = pi_at(fx, w)?;
    let i = fx.i();
    let ln2 = std::f64::consts::LN_2;
    if which == Modular::Eta {
        // Q = e^(2 i pi t), |Q| = e^(-2 pi y); terms Q^(n (3n -+ 1)/2)
        let qq = fx.eval(fx.call("exp", &[fx.mul(&[fx.int(2), i, pi, t])]))?;
        let mut n_max = 1i64;
        while (2.0 * std::f64::consts::PI * y * (n_max * (3 * n_max - 1) / 2) as f64) < (w as f64 + 16.0) * ln2 {
            n_max += 1;
        }
        let mut terms = vec![fx.int(1)];
        for n in 1..=n_max {
            let e1 = fx.pow(qq, fx.int(n * (3 * n - 1) / 2));
            let e2 = fx.pow(qq, fx.int(n * (3 * n + 1) / 2));
            let s = fx.add(&[e1, e2]);
            terms.push(if n % 2 == 1 { fx.neg(s) } else { s });
        }
        let s = fx.eval(fx.add(&terms))?;
        let n1 = n_max + 1;
        let tail = -(2.0 * std::f64::consts::PI * y * (n1 * (3 * n1 - 1) / 2) as f64) / ln2 + 2.0;
        let s = widen(fx, s, tail);
        let pre = fx.call("exp", &[fx.mul(&[fx.rat(1, 12), i, pi, t])]);
        return Ok(Some(fx.eval(fx.mul(&[pre, s]))?));
    }
    // theta constants: q = e^(i pi t), q^(1/4) = e^(i pi t/4)
    let q = fx.eval(fx.call("exp", &[fx.mul(&[i, pi, t])]))?;
    let q4 = fx.eval(fx.call("exp", &[fx.mul(&[fx.rat(1, 4), i, pi, t])]))?;
    let mut n_max = 1i64;
    while (std::f64::consts::PI * y * (n_max * n_max) as f64) < (w as f64 + 16.0) * ln2 {
        n_max += 1;
    }
    let (mut s3, mut s4, mut s2) = (vec![fx.int(1)], vec![fx.int(1)], vec![fx.int(1)]);
    for n in 1..=n_max {
        let p = fx.eval(fx.pow(q, fx.int(n * n)))?;
        s3.push(fx.mul(&[fx.int(2), p]));
        s4.push(fx.mul(&[fx.int(if n % 2 == 1 { -2 } else { 2 }), p]));
        s2.push(fx.pow(q, fx.int(n * (n + 1))));
    }
    let n1 = (n_max + 1) as f64;
    let tail = -(std::f64::consts::PI * y * n1 * n1) / ln2 + 2.0;
    let (e3, e4, e2) = (fx.add(&s3), fx.add(&s4), fx.add(&s2));
    let (v3, v4, v2) = (fx.eval(e3)?, fx.eval(e4)?, fx.eval(e2)?);
    let th3 = widen(fx, v3, tail);
    let th4 = widen(fx, v4, tail);
    let s2 = widen(fx, v2, tail);
    let th2 = fx.eval(fx.mul(&[fx.int(2), q4, s2]))?;
    let v = match which {
        Modular::Lambda => fx.pow(fx.div(th2, th3), fx.int(4)),
        _ => {
            let p8 = |x: T<'h>| fx.pow(x, fx.int(8));
            let num = fx.pow(fx.add(&[p8(th2), p8(th3), p8(th4)]), fx.int(3));
            fx.div(num, fx.mul(&[fx.int(54), p8(fx.mul(&[th2, th3, th4]))]))
        }
    };
    Ok(Some(fx.eval(v)?))
}

/// The value of `which` at tau (a ball or exact number) at w bits: reduced, summed and carried back.
fn value_at<'h>(fx: &mut Fx<'_, '_, '_, 'h>, tau: T<'h>, which: Modular, w: u64) -> R<'h> {
    let Some((t, steps)) = reduce(fx, tau)? else { return Ok(None) };
    let Some(mut v) = reduced_value(fx, t, which, w)? else { return Ok(None) };
    if which == Modular::J {
        return Ok(Some(v));
    }
    let pi = pi_at(fx, w)?;
    for s in steps.iter().rev() {
        let e = match (which, s) {
            (Modular::Lambda, Step::Invert(_)) => fx.sub(fx.int(1), v),
            (Modular::Lambda, Step::Shift(n)) if n % 2 == 1 => fx.div(v, fx.sub(v, fx.int(1))),
            (Modular::Lambda, Step::Shift(_)) => v,
            (_, Step::Shift(n)) => fx.mul(&[v, fx.call("exp", &[fx.mul(&[fx.rat(*n, 12), fx.i(), pi])])]),
            (_, Step::Invert(tk)) => {
                let tk = match fx.num(*tk) {
                    Some(n) if n.kind() == Kind::Exact => fx.ar.number(&n.to_kind(Kind::Ball, w)),
                    _ => *tk,
                };
                fx.div(v, fx.call("sqrt", &[fx.mul(&[fx.int(-1), fx.i(), tk])]))
            }
        };
        v = fx.eval(e)?;
        if fx.num(v).is_none() {
            return Ok(None);
        }
    }
    Ok(Some(v))
}

/// log2 of the relative radius of a number (0 for an exact one, +inf when it has no midpoint away from 0).
pub(crate) fn rel_radius<'h>(fx: &Fx<'_, '_, '_, 'h>, v: T<'h>) -> f64 {
    let part = |r: &Real| match r {
        Real::Ball(b, _) => (b.rad.log2(), crate::ball::f64_of(&b.mid).abs()),
        other => (f64::NEG_INFINITY, other.to_f64().abs()),
    };
    let (r, m) = match fx.num(v) {
        Some(Num::Real(x)) => part(&x),
        Some(Num::Complex(x, y)) => {
            let (a, b) = (part(&x), part(&y));
            (a.0.max(b.0), a.1.hypot(b.1))
        }
        None => return f64::INFINITY,
    };
    if m == 0.0 || !m.is_finite() {
        return if r == f64::NEG_INFINITY { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    r - m.log2()
}

/// J, lambda or eta at a numeric tau (machine or ball) by the reduction, in the kind of the argument; `None` outside
/// the stated bounds.
pub(crate) fn modular<'h>(fx: &mut Fx<'_, '_, '_, 'h>, tau: T<'h>, which: Modular) -> R<'h> {
    let Some((x, y)) = parts(fx, tau) else { return Ok(None) };
    if !(y > 0.0) || !x.is_finite() || !y.is_finite() {
        return Ok(None);
    }
    let loss = ((x * x + y * y) / y).log2().max(0.0);
    let loss = if loss.is_finite() { loss } else { (x.abs().log2() * 2.0 - y.log2()).max(0.0) };
    if !(loss <= 1e4) {
        return Ok(None);
    }
    let mut extra = 32 + 2 * loss.ceil() as u64;
    for _ in 0..4 {
        let Some((wk, xs)) = work(fx, &[tau], extra) else { return Ok(None) };
        let Some(v) = value_at(fx, xs[0], which, wk.w)? else { return Ok(None) };
        if !wk.machine {
            return Ok(wk.out(fx, v, false));
        }
        if rel_radius(fx, v) <= -56.0 {
            let Some(o) = wk.out(fx, v, false) else { return Ok(None) };
            // a machine value outside the range of machine numbers stays unevaluated
            let m = parts(fx, o).map_or(f64::NAN, |p| p.0.hypot(p.1));
            return Ok((m.is_finite() && m > 1e-290).then_some(o));
        }
        extra *= 2;
        if extra > 40_000 {
            break;
        }
    }
    Ok(None)
}
