//! Precision and approximation: `precision`, `accuracy`, `set_precision`, `rationalize`, `chop`.
//!
//! - **Digits (decision 1C.4).** `precision(x)` counts correct significant decimal digits, `accuracy(x)` correct
//!   digits after the decimal point, both as machine reals. Exact numbers have `inf` of both. A machine real has
//!   precision 53 log10(2) = 15.95... and accuracy 15.95... - log10|x| (a machine zero: 1075 log10(2), down to
//!   half the smallest subnormal). A ball at working precision p with midpoint m and radius r has accuracy
//!   min(p log10(2) - log10|m|, -log10 r) and precision accuracy + log10|m| (at least 0; 0 for a zero midpoint):
//!   its working precision when the radius is below it, its error otherwise. Complex numbers use the smaller
//!   accuracy of their parts and their larger part's magnitude. Over a term (a list, a sum) both are the least over
//!   its numbers, `inf` when it has none.
//! - **set_precision (decision 1C.5)** is the deliberate exit from enclosure: every number becomes a ball of d
//!   digits (ceil(d log2 10) bits, as a literal `x~d`) whose midpoint is the number's exact value rounded and whose
//!   radius is only that rounding: exact numbers are enclosed, a machine real is taken at its binary value
//!   (`set_precision(0.1, 20)` = 0.10000000000000000555), a ball keeps its midpoint and drops its radius. Closed
//!   numeric subterms are then evaluated to d digits as by `n`. `set_precision(x, inf)` makes every inexact number
//!   the exact rational of its binary value (a ball: its midpoint).
//! - **rationalize (decision 1C.6).** `rationalize(x)`: for an inexact real, the simplest rational (smallest
//!   denominator q) among the reals it stands for (the interval of reals rounding to a machine real, a ball's
//!   interval) if it lies within 10^-4 / q^2 of the number, else the number unchanged; exact numbers and closed
//!   terms stay. `rationalize(x, dx)`: the simplest rational within dx of x's value, for every kind of real,
//!   exact ones included (`rationalize(355/113, 1/100) = 22/7`), and for closed numeric terms by refinement
//!   (`rationalize(pi, 1/100) = 22/7`); an inexact x's own interval widens the tolerance. Complex numbers part by
//!   part (both parts or neither). Numbers inside any term are replaced (`rationalize(0.5*x)` = x/2).
//! - **chop (decision 1C.3)**: inexact reals with |x| < delta (balls: |m| + r < delta) become exact 0, parts of
//!   complex numbers separately (a chopped imaginary part leaves the real part, a chopped real part the exact 0
//!   with the imaginary part, which contagion makes inexact); exact numbers stay. delta defaults to 10^-10 and
//!   may be any positive real number.
use crate::numbers::{for_numbers, map_leaves, Env, Leaf};
use crate::numeval::{self, Graph, Target};
use crate::value::{
    ball_accuracy, ball_precision, float_rat, interval, log10_float, machine_rat, point, simplest, MACHINE_DIGITS,
    MACHINE_ZERO_ACCURACY,
};
use mc_api::{Store, View, T};
use mc_arith::num::{Ball, Float, Int, Nat, Rat};
use mc_arith::{Arith, Kind, Num, Real};
use mc_core::Heap;
use mc_eval::{NativeCx, Reply, Res};
use std::cmp::Ordering;

/// Accuracy and precision of one number, in decimal digits (`inf` for exact).
fn digits(x: &Num) -> (f64, f64) {
    let real = |r: &Real| -> (f64, f64, f64) {
        // (accuracy, precision, log10 |x|)
        match r {
            Real::Exact(_) => (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY),
            Real::Machine(v) if *v == 0.0 => (MACHINE_ZERO_ACCURACY, MACHINE_DIGITS, f64::NEG_INFINITY),
            Real::Machine(v) => (MACHINE_DIGITS - v.abs().log10(), MACHINE_DIGITS, v.abs().log10()),
            Real::Ball(b, p) => (ball_accuracy(b, *p), ball_precision(b, *p), log10_float(&b.mid)),
        }
    };
    match x {
        Num::Real(r) => {
            let (a, p, _) = real(r);
            (a, p)
        }
        Num::Complex(re, im) => {
            let ((a1, p1, l1), (a2, p2, l2)) = (real(re), real(im));
            if matches!(re, Real::Machine(_)) {
                return (a1.min(a2), MACHINE_DIGITS);
            }
            let acc = a1.min(a2);
            let prec = if acc.is_infinite() { p1.min(p2) } else { (acc + l1.max(l2)).max(0.0) };
            (acc, prec)
        }
    }
}

/// The least accuracy or precision over the numbers of a term.
fn least<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], accuracy: bool) -> Res<Reply<'h>> {
    let &[x] = args else { return Ok(Reply::Decline) };
    let h = cx.heap();
    let ar = env.arith(h);
    let mut m = f64::INFINITY;
    for_numbers(cx, &ar, x, &mut |n| {
        let (a, p) = digits(n);
        m = m.min(if accuracy { a } else { p });
    })?;
    Ok(Reply::Done(if m == f64::INFINITY { h.sym_term(env.s.inf) } else { h.real(m) }))
}

/// `precision(x)`.
pub(crate) fn precision<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    least(env, cx, args, false)
}

/// `accuracy(x)`.
pub(crate) fn accuracy<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    least(env, cx, args, true)
}

fn exact_term<'h>(ar: &Arith<'h, Heap>, q: Rat) -> T<'h> {
    ar.number(&Num::Real(Real::Exact(q)))
}

/// One real at precision p bits (`None`: infinite precision, the exact value), per decision 1C.5; `None` for a
/// non-finite machine real.
fn reset(r: &Real, p: Option<u64>) -> Option<Real> {
    let q = match r {
        Real::Exact(q) => return Some(p.map_or_else(|| r.clone(), |p| Real::Exact(q.clone()).to_kind(Kind::Ball, p))),
        Real::Machine(v) => machine_rat(*v)?,
        Real::Ball(b, _) => {
            if let Some(p) = p {
                let (mid, rad) = Float::round(b.mid.neg, b.mid.man.clone(), b.mid.exp, p);
                return Some(Real::Ball(Ball { mid, rad }, p));
            }
            float_rat(&b.mid)
        }
    };
    Some(match p {
        Some(p) => Real::Exact(q).to_kind(Kind::Ball, p),
        None => Real::Exact(q),
    })
}

/// `set_precision(x, d)`.
pub(crate) fn set_precision<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let &[x, d] = args else { return Ok(Reply::Decline) };
    let h = cx.heap();
    let ar = env.arith(h);
    let digits = match h.view(d) {
        View::Int(d @ 1..) => Some(d as u64),
        View::Sym(s) if s == env.s.inf => None,
        View::Sym(_) | View::App { .. } => return Ok(Reply::Decline),
        _ => return Ok(env.invalid(h, t)),
    };
    let p = digits.map(|d| mc_arith::text::digits_to_bits(d).max(mc_arith::number::MIN_PREC));
    if p.is_some_and(|p| p > mc_arith::DEFAULT_MAX_BITS) {
        return Ok(Reply::Done(env.fail(
            h,
            mc_api::names::failure::LIMIT,
            &[h.sym_term(h.sym(mc_api::names::failure::MEMORY)), t],
        )));
    }
    let y = map_leaves(
        cx,
        &ar,
        x,
        &mut |cx, n, _| {
            cx.tick(p.unwrap_or(64) / 1024)?;
            let v = match n {
                Num::Real(r) => reset(r, p).map(Num::Real),
                Num::Complex(a, b) => reset(a, p).zip(reset(b, p)).map(|(a, b)| Num::complex(a, b)),
            };
            Ok(v.map_or(Leaf::Keep, |v| Leaf::With(ar.number(&v))))
        },
        &mut |_, _| Ok(None),
    )?;
    // closed numeric subterms that are not numbers (pi, sqrt(2), pi + 0.1~20) to d digits, the best value kept
    // where d is not reached; numbers keep the precision just set
    let (Some(d), None) = (digits, ar.num(y)) else { return Ok(Reply::Again(y)) };
    let g = Graph::build(h, &ar, &env.nm, y);
    let z = map_leaves(cx, &ar, y, &mut |_, _, _| Ok(Leaf::Keep), &mut |cx, u| {
        if g.node(u).is_none() {
            return Ok(None);
        }
        let r = numeval::numericize(h, &ar, &env.nm, u, Target::Digits(d), &mut |n| cx.tick(n))?;
        Ok(Some(r.term))
    })?;
    Ok(Reply::Again(z))
}

/// |x - s| q^2 < 10^-4 with q the denominator of s (the closeness test of `rationalize(x)`).
fn close_enough(x: &Rat, s: &Rat) -> bool {
    let d = x.sub(s);
    let err = Rat { num: Int::new(false, d.num.mag.clone()), den: d.den };
    let q2 = Rat::from_int(Int::from_nat(s.den.square()));
    err.mul(&q2).mul(&Rat::from_i64(10_000)).cmp_value(&Rat::from_i64(1)) == Ordering::Less
}

/// The rationalized real, `None` to keep it.
fn rationalize_real(r: &Real, dx: Option<&Rat>, tick: &mut dyn FnMut() -> Res<()>) -> Res<Option<Rat>> {
    let Some((lo, hi)) = interval(r) else { return Ok(None) };
    match dx {
        Some(dx) => Ok(Some(simplest(&lo.sub(dx), &hi.add(dx), tick)?)),
        None => {
            if matches!(r, Real::Exact(_)) {
                return Ok(None);
            }
            let s = simplest(&lo, &hi, tick)?;
            let x = point(r).expect("a finite number");
            Ok(close_enough(&x, &s).then_some(s))
        }
    }
}

/// The largest precision tried when rationalizing a closed numeric term with a tolerance.
const RATIONALIZE_CAP: u64 = 1 << 14;

/// The simplest rational within dx of a closed real numeric term, by refinement: with the value in [m - r, m + r],
/// the simplest rational s of [m - r - dx, m + r + dx] is the answer once it lies in [m + r - dx, m - r + dx]
/// (both intervals bracket the true one). `None` if not decided by the cap.
fn rationalize_closed<'h>(
    g: &Graph<'h>,
    ar: &Arith<'h, Heap>,
    cx: &mut NativeCx<'_, '_, 'h>,
    u: T<'h>,
    dx: &Rat,
) -> Res<Option<T<'h>>> {
    let Some(i) = g.node(u) else { return Ok(None) };
    let is = g.island(i);
    if is.is_machine() {
        let v = match g.eval_machine(&is) {
            crate::fun::Mach::Real(v) => v,
            _ => return Ok(None),
        };
        let s = rationalize_real(&Real::Machine(v), Some(dx), &mut || cx.tick(1))?;
        return Ok(s.map(|s| exact_term(ar, s)));
    }
    let mut p = 64;
    while p <= RATIONALIZE_CAP {
        cx.tick(1 + p / 1024)?;
        if let Some(b) = g.eval_ball(&is, p) {
            let m = float_rat(&b.mid);
            let r = float_rat(&b.rad.to_float());
            let s = simplest(&m.sub(&r).sub(dx), &m.add(&r).add(dx), &mut || cx.tick(1))?;
            let (ilo, ihi) = (m.add(&r).sub(dx), m.sub(&r).add(dx));
            if ilo.cmp_value(&s) != Ordering::Greater && s.cmp_value(&ihi) != Ordering::Greater {
                return Ok(Some(exact_term(ar, s)));
            }
        }
        if mc_arith::num::stopped() {
            return Ok(None);
        }
        p *= 2;
    }
    Ok(None)
}

/// `rationalize(x)` and `rationalize(x, dx)`.
pub(crate) fn rationalize<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let ar = env.arith(h);
    let (x, dx) = match *args {
        [x] => (x, None),
        [x, d] => match ar.num(d) {
            Some(Num::Real(r)) => match point(&r) {
                Some(q) if !q.num.neg => (x, Some(q)),
                _ => return Ok(env.invalid(h, t)),
            },
            Some(Num::Complex(..)) => return Ok(env.invalid(h, t)),
            None => match h.view(d) {
                View::Sym(_) | View::App { .. } => return Ok(Reply::Decline),
                _ => return Ok(env.invalid(h, t)),
            },
        },
        _ => return Ok(Reply::Decline),
    };
    // the numeric subterms of x, read once (only a tolerance rationalizes closed terms)
    let graph = dx.as_ref().map(|_| Graph::build(h, &ar, &env.nm, x));
    let y = map_leaves(
        cx,
        &ar,
        x,
        &mut |cx, n, _| {
            let mut tick = || cx.tick(1);
            let v = match n {
                Num::Real(r) => rationalize_real(r, dx.as_ref(), &mut tick)?.map(|q| exact_term(&ar, q)),
                Num::Complex(a, b) => {
                    match (rationalize_real(a, dx.as_ref(), &mut tick)?, rationalize_real(b, dx.as_ref(), &mut tick)?) {
                        (Some(a), Some(b)) => Some(ar.number(&Num::complex(Real::Exact(a), Real::Exact(b)))),
                        _ => None,
                    }
                }
            };
            Ok(v.map_or(Leaf::Keep, Leaf::With))
        },
        &mut |cx, u| match (&graph, &dx) {
            (Some(g), Some(dx)) => rationalize_closed(g, &ar, cx, u, dx),
            _ => Ok(None),
        },
    )?;
    Ok(if y == x { Reply::Done(x) } else { Reply::Again(y) })
}

/// Is |x| < delta for an inexact real (a ball: |m| + r < delta)? Exact numbers are never chopped.
fn below(r: &Real, delta: &Rat) -> bool {
    let (mag, top) = match r {
        Real::Exact(_) => return false,
        Real::Machine(v) if v.is_nan() => return false,
        Real::Machine(v) => {
            if v.is_infinite() {
                return false;
            }
            let Some(q) = machine_rat(v.abs()) else { return false };
            return q.cmp_value(delta) == Ordering::Less;
        }
        Real::Ball(b, _) => {
            let upper = b.mid.abs().add_exact(&b.rad.to_float());
            let top = if upper.is_zero() { i64::MIN } else { upper.top() };
            (upper, top)
        }
    };
    // decide by magnitudes first, so a far-off exponent never becomes a huge rational
    let dtop = delta.num.mag.bits() as i64 - delta.den.bits() as i64;
    if top < dtop - 2 {
        return true;
    }
    if top > dtop + 2 {
        return false;
    }
    float_rat(&mag).cmp_value(delta) == Ordering::Less
}

/// `chop(x)` and `chop(x, delta)`.
pub(crate) fn chop<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let ar = env.arith(h);
    let (x, delta) = match *args {
        [x] => (x, Rat { num: Int::from_i64(1), den: Nat::pow(10, 10) }),
        [x, d] => match ar.num(d) {
            Some(Num::Real(r)) if r.is_positive() => match point(&r) {
                Some(q) => (x, q),
                None => return Ok(env.invalid(h, t)),
            },
            None if matches!(h.view(d), View::Sym(_) | View::App { .. }) => return Ok(Reply::Decline),
            _ => return Ok(env.invalid(h, t)),
        },
        _ => return Ok(Reply::Decline),
    };
    let zero = h.int(0);
    let y = map_leaves(
        cx,
        &ar,
        x,
        &mut |_, n, _| {
            Ok(match n {
                Num::Real(r) if below(r, &delta) => Leaf::With(zero),
                Num::Real(_) => Leaf::Keep,
                Num::Complex(a, b) => {
                    let part = |r: &Real, c: bool| if c { zero } else { ar.number(&Num::Real(r.clone())) };
                    match (below(a, &delta), below(b, &delta)) {
                        (false, false) => Leaf::Keep,
                        // a chopped imaginary part leaves a real number
                        (ca, true) => Leaf::With(part(a, ca)),
                        (true, false) => Leaf::With(ar.complex(zero, part(b, false))),
                    }
                }
            })
        },
        &mut |_, _| Ok(None),
    )?;
    Ok(if y == x { Reply::Done(x) } else { Reply::Again(y) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: i64, d: i64) -> Rat {
        Rat::new(Int::from_i64(n), Int::from_i64(d))
    }

    #[test]
    fn digits_of_numbers() {
        let (a, p) = digits(&Num::Real(Real::Machine(1.5)));
        assert!((p - MACHINE_DIGITS).abs() < 1e-12);
        assert!((a - 15.778498511135323).abs() < 1e-9);
        let (a, _) = digits(&Num::Real(Real::Machine(0.0)));
        assert!((a - 323.6072453387798).abs() < 1e-9);
        assert_eq!(digits(&Num::int(3)), (f64::INFINITY, f64::INFINITY));
        // 1/3 at 200 bits: about 60 digits of both (the magnitude is below 1)
        let Real::Ball(b, p) = Real::Exact(q(1, 3)).to_kind(Kind::Ball, 200) else { unreachable!() };
        let (a, pr) = digits(&Num::Real(Real::Ball(b, p)));
        assert!((pr - 200.0 * crate::value::LOG10_2).abs() < 0.5, "{pr}");
        assert!((a - pr - 3f64.log10()).abs() < 1e-9);
    }

    #[test]
    fn close_enough_rule() {
        assert!(close_enough(&q(314159, 100000), &q(314159, 100000)));
        assert!(close_enough(&q(1, 3).add(&q(1, 10_000_000)), &q(1, 3)));
        assert!(!close_enough(&q(1, 3).add(&q(1, 10_000)), &q(1, 3)));
    }

    #[test]
    fn chop_threshold() {
        let d = q(1, 10_000_000_000);
        assert!(below(&Real::Machine(1e-11), &d));
        assert!(!below(&Real::Machine(1e-9), &d));
        assert!(!below(&Real::Machine(f64::NAN), &d));
        assert!(!below(&Real::int(0), &d));
        let tiny = Ball { mid: Float::new(false, Nat::from_u64(1), -1_000_000), rad: mc_arith::num::Mag::ZERO };
        assert!(below(&Real::Ball(tiny, 64), &d));
    }
}
