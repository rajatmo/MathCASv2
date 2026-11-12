//! Piecewise functions of reals: `clip`, `rescale`, `unit_step`.
//!
//! Comparisons are three-valued (semantics 2.5): numbers compare directly, closed numeric terms by refinement
//! (decision L.8); a comparison that is not decided (overlapping balls, a symbol, NaN) leaves the call unevaluated.
//!
//! - `clip(x, [lo, hi], [vlo, vhi])` is vlo for x < lo, vhi for x > hi, and x otherwise (x itself, so its kind is
//!   kept; at the bounds x is returned); vlo and vhi default to lo and hi, the bounds to [-1, 1]. A list x maps.
//!   lo > hi gives `failure(invalid_argument, call)`.
//! - `rescale(x, [a, b], [c, d]) = c + (d - c)(x - a)/(b - a)` for any x (symbolic too), [c, d] defaulting to
//!   [0, 1]; a list x maps; `rescale(xs)` without bounds uses the least and greatest element of the list xs (all
//!   compared; a list of equal values gives zeros). b = a gives `failure(division_by_zero)` from the arithmetic.
//! - `unit_step(x1, x2, ...)` is 1 when every xi >= 0 and 0 when some xi < 0, as an exact integer for every kind of
//!   real (decision 1C.8); decided factors equal to 1 are dropped from a call that stays. Lists thread.
//!
//! Derivatives (for item 2A): `clip` has derivative 1 inside (lo, hi) and 0 outside (with the default values);
//! `rescale` is linear in x with slope (d - c)/(b - a); `unit_step` has derivative 0 away from 0.
use crate::numbers::Env;
use mc_api::{Store, View, T};
use mc_arith::{Cmp3, Num, Real};
use mc_eval::{NativeCx, Reply, Res};

/// `clip(x)`, `clip(x, [lo, hi])`, `clip(x, [lo, hi], [vlo, vhi])`.
pub(crate) fn clip<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let ar = env.arith(h);
    let Some((&x, rest)) = args.split_first() else { return Ok(Reply::Decline) };
    if rest.len() > 2 {
        return Ok(Reply::Decline);
    }
    // the pairs of bounds and values: two-element lists
    let pair = |i: usize| -> Result<Option<[T<'h>; 2]>, ()> {
        match rest.get(i) {
            None => Ok(None),
            Some(&b) => match env.as_list(h, b) {
                Some(&[a, b]) => Ok(Some([a, b])),
                _ => Err(()),
            },
        }
    };
    let (Ok(bounds), Ok(values)) = (pair(0), pair(1)) else {
        return Ok(if rest.iter().any(|&b| matches!(h.view(b), View::Sym(_))) {
            Reply::Decline
        } else {
            env.invalid(h, t)
        });
    };
    let [lo, hi] = bounds.unwrap_or([h.int(-1), h.int(1)]);
    let [vlo, vhi] = values.unwrap_or([lo, hi]);
    if let Some(xs) = env.as_list(h, x) {
        return env.map_first(cx, env.s.clip, xs, rest);
    }
    cx.tick(1)?;
    if env.compare(h, &ar, lo, hi) == Some(Cmp3::Greater) {
        return Ok(env.invalid(h, t));
    }
    Ok(match env.compare(h, &ar, x, lo) {
        Some(Cmp3::Less) => Reply::Done(vlo),
        Some(Cmp3::Equal | Cmp3::Greater) => match env.compare(h, &ar, x, hi) {
            Some(Cmp3::Greater) => Reply::Done(vhi),
            Some(Cmp3::Equal | Cmp3::Less) => Reply::Done(x),
            _ => Reply::Decline,
        },
        _ => Reply::Decline,
    })
}

/// `rescale(x, [a, b])`, `rescale(x, [a, b], [c, d])`, `rescale(xs)`.
pub(crate) fn rescale<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let ar = env.arith(h);
    let s = &env.s;
    let pair = |b: T<'h>| match env.as_list(h, b) {
        Some(&[a, b]) => Some([a, b]),
        _ => None,
    };
    let (x, [a, b], [c, d]) = match *args {
        [xs] => {
            let Some(items) = env.as_list(h, xs) else {
                return Ok(if matches!(h.view(xs), View::Sym(_) | View::App { .. }) {
                    Reply::Decline
                } else {
                    env.invalid(h, t)
                });
            };
            if items.is_empty() {
                return Ok(Reply::Done(xs));
            }
            // the least and greatest elements, every comparison decided
            let (mut lo, mut hi) = (items[0], items[0]);
            for &v in &items[1..] {
                cx.tick(1)?;
                match (env.compare(h, &ar, v, lo), env.compare(h, &ar, v, hi)) {
                    (Some(l), Some(g)) if l != Cmp3::Undecided && g != Cmp3::Undecided && l != Cmp3::Unordered => {
                        if l == Cmp3::Less {
                            lo = v;
                        }
                        if g == Cmp3::Greater {
                            hi = v;
                        }
                    }
                    _ => return Ok(Reply::Decline),
                }
            }
            // a list of equal values has no range: every element goes to the lower end (decision 1C.10)
            if env.compare(h, &ar, lo, hi) == Some(Cmp3::Equal) {
                let zeros = vec![h.int(0); items.len()];
                return Ok(Reply::Done(h.call(s.list, &zeros)));
            }
            let bounds = h.call(s.list, &[lo, hi]);
            return env.map_first(cx, s.rescale, items, &[bounds]);
        }
        [x, ab] => match pair(ab) {
            Some(ab) => (x, ab, [h.int(0), h.int(1)]),
            None => return Ok(Reply::Decline),
        },
        [x, ab, cd] => match (pair(ab), pair(cd)) {
            (Some(ab), Some(cd)) => (x, ab, cd),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    if let Some(xs) = env.as_list(h, x) {
        return env.map_first(cx, s.rescale, xs, &args[1..]);
    }
    let neg = |u: T<'h>| h.call(s.mul, &[h.int(-1), u]);
    let diff = |u: T<'h>, v: T<'h>| h.call(s.add, &[u, neg(v)]);
    // c + (d - c) (x - a) / (b - a), canonicalised by the evaluator
    let ratio = h.call(s.mul, &[diff(d, c), diff(x, a), h.call(s.pow, &[diff(b, a), h.int(-1)])]);
    Ok(Reply::Again(h.call(s.add, &[c, ratio])))
}

/// The unit step of one argument: `Some(true)` for x >= 0, `Some(false)` for x < 0, `None` when not decided.
fn step<'h>(env: &Env, h: &'h mc_core::Heap, ar: &mc_arith::Arith<'h, mc_core::Heap>, x: T<'h>) -> Option<bool> {
    match ar.num(x) {
        Some(Num::Real(r)) => {
            if let Real::Machine(v) = r {
                return (!v.is_nan()).then_some(v >= 0.0);
            }
            if r.is_negative() {
                Some(false)
            } else if r.is_positive() || r.is_zero() {
                Some(true)
            } else {
                None
            }
        }
        Some(Num::Complex(..)) => None,
        None if ar.inf_sign(x).is_some() => ar.inf_sign(x).map(|s| s > 0),
        None => crate::numeval::sign(h, ar, &env.nm, x).map(|o| o.is_ge()),
    }
}

/// `unit_step(x1, ...)`.
pub(crate) fn unit_step<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    if args.is_empty() {
        return Ok(Reply::Decline);
    }
    let h = cx.heap();
    let ar = env.arith(h);
    let mut open = vec![];
    for &x in args {
        cx.tick(1)?;
        match step(env, h, &ar, x) {
            Some(false) => return Ok(Reply::Done(h.int(0))),
            Some(true) => {}
            None => open.push(x),
        }
    }
    Ok(if open.is_empty() {
        Reply::Done(h.int(1))
    } else if open.len() < args.len() {
        Reply::Again(h.call(h.sym("unit_step"), &open))
    } else {
        Reply::Decline
    })
}
