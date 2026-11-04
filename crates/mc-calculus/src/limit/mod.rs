//! Limits (roadmap package 2C, item 5): the native `limit(f, x -> a)` and its Rust entry point [`limit`].
//!
//! `limit(f, x -> a)` is the two-sided limit along the real line, `direction: from_above` (`x ↓ a`) and
//! `direction: from_below` (`x ↑ a`) the one-sided ones; `a` may be `inf` or `-inf` (approached from the finite
//! side; a contrary direction is an argument failure). One-sided limits come from the [`walk`] at `t → 0⁺`; a
//! two-sided limit is the common value of both sides (decision C.1):
//!
//! | sides | result |
//! |---|---|
//! | equal values (or both `inf`, both `-inf`) | that value |
//! | provably different | `failure(no_limit, limit, f)` |
//! | either side unknown, or equality not decided | the call stays unevaluated |
//!
//! An unknown limit leaves the call as it is (decision C.2), like `integrate` without an antiderivative. A list
//! `f` maps elementwise.
pub(crate) mod walk;

use self::walk::Lim;
use crate::integrate::definite::{sign, Pt};
use crate::integrate::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The side a limit is taken from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// both sides: the limit exists when they agree
    TwoSided,
    /// `x ↓ a`
    FromAbove,
    /// `x ↑ a`
    FromBelow,
}

/// The outcome of a limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome<'h> {
    /// the limit: a value, `inf` or `-inf`
    Value(T<'h>),
    /// the one-sided limits differ: there is no two-sided limit
    NoLimit,
    /// not decided by this package
    Unknown,
}

/// The limit of `f` as `x` tends to `a` (a term free of `x`, or `inf`, `-inf`) from `direction`. At an infinity
/// the direction must be the finite side or [`Direction::TwoSided`], else the outcome is [`Outcome::Unknown`].
pub fn limit<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    a: T<'h>,
    direction: Direction,
) -> Res<Outcome<'h>> {
    if is_free(b, f, x) {
        return Ok(Outcome::Value(f));
    }
    let inf = b.heap().sym_term(b.heap().sym("inf"));
    let one = |cx: &mut NativeCx<'_, '_, 'h>, pt, side| walk::limit(cx, b, f, x, pt, side);
    let l = match (a, direction) {
        (a, Direction::TwoSided | Direction::FromBelow) if a == inf => one(cx, Pt::PosInf, 1)?,
        (a, Direction::TwoSided | Direction::FromAbove) if a == b.neg(inf) => one(cx, Pt::NegInf, 1)?,
        (a, _) if !is_free(b, a, inf) => None,
        (a, Direction::FromAbove) => one(cx, Pt::Fin(a), 1)?,
        (a, Direction::FromBelow) => one(cx, Pt::Fin(a), -1)?,
        (a, Direction::TwoSided) => {
            let (Some(above), Some(below)) = (one(cx, Pt::Fin(a), 1)?, one(cx, Pt::Fin(a), -1)?) else {
                return Ok(Outcome::Unknown);
            };
            return both_sides(cx, b, above, below);
        }
    };
    Ok(l.map_or(Outcome::Unknown, |l| Outcome::Value(value(b, l))))
}

/// The two-sided limit from the one-sided ones (decision C.1).
fn both_sides<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, above: Lim<'h>, below: Lim<'h>) -> Res<Outcome<'h>> {
    Ok(match (above, below) {
        (Lim::Inf(p), Lim::Inf(q)) if p == q => Outcome::Value(value(b, above)),
        (Lim::Inf(_), _) | (_, Lim::Inf(_)) => Outcome::NoLimit,
        (Lim::Fin(p, _), Lim::Fin(q, _)) if p == q => Outcome::Value(p),
        (Lim::Fin(p, _), Lim::Fin(q, _)) => {
            let diff = cx.eval(b.sub(p, q))?;
            match sign(cx, b, diff)? {
                Some(0) => Outcome::Value(p),
                Some(_) => Outcome::NoLimit,
                None => Outcome::Unknown,
            }
        }
    })
}

/// The term of a limit: the value, `inf` or `-inf`.
fn value<'h>(b: &Build<'h>, l: Lim<'h>) -> T<'h> {
    let inf = b.heap().sym_term(b.heap().sym("inf"));
    match l {
        Lim::Fin(v, _) => v,
        Lim::Inf(1) => inf,
        Lim::Inf(_) => b.neg(inf),
    }
}

/// `limit(f, x -> a)`, `limit(f, x -> a, direction: from_above | from_below)`.
pub struct LimitFn {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// `failure(tag, details...)`.
fn failure<'h>(b: &Build<'h>, tag: &str, details: &[T<'h>]) -> T<'h> {
    let h = b.heap();
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

/// `failure(argument, limit, message)`.
fn bad<'h>(b: &Build<'h>, message: &str) -> T<'h> {
    let h = b.heap();
    failure(b, "argument", &[h.sym_term(h.sym("limit")), h.string(message)])
}

/// The direction of a trailing `options(direction -> d)`, and the arguments before it; `Err` with the failure.
fn options<'a, 'h>(b: &Build<'h>, args: &'a [T<'h>]) -> Result<(&'a [T<'h>], Direction), T<'h>> {
    let h = b.heap();
    let Some((&last, rest)) = args.split_last() else { return Ok((args, Direction::TwoSided)) };
    let View::App { head, args: rules } = h.view(last) else { return Ok((args, Direction::TwoSided)) };
    if b.sym_of(head) != Some(h.sym("options")) {
        return Ok((args, Direction::TwoSided));
    }
    let mut direction = Direction::TwoSided;
    for &r in rules {
        let View::App { head, args: &[k, v] } = h.view(r) else { return Err(bad(b, "options are name: value")) };
        let (Some(rule), Some(name)) = (b.sym_of(head), b.sym_of(k)) else {
            return Err(bad(b, "options are name: value"));
        };
        if rule != h.sym(names::RULE) {
            return Err(bad(b, "options are name: value"));
        }
        if h.sym_name(name) != "direction" {
            return Err(failure(b, "unknown_option", &[k]));
        }
        direction = match b.sym_of(v).map(|s| h.sym_name(s)).as_deref() {
            Some("from_above") => Direction::FromAbove,
            Some("from_below") => Direction::FromBelow,
            Some("two_sided") => Direction::TwoSided,
            _ => return Err(bad(b, "direction is from_above, from_below or two_sided")),
        };
    }
    Ok((rest, direction))
}

impl Native for LimitFn {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let (args, direction) = match options(&b, args) {
            Ok(v) => v,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let [f, spec] = args else { return Ok(Reply::Decline) };
        let (x, a) = match h.view(*spec) {
            View::App { head, args: &[x, a] } if b.sym_of(head) == Some(h.sym(names::RULE)) => (x, a),
            _ => return Ok(Reply::Done(bad(&b, "the point is x -> a"))),
        };
        let x = match crate::natives::variable(&b, x) {
            Ok(x) => x,
            Err(message) => return Ok(Reply::Done(bad(&b, message))),
        };
        if !is_free(&b, a, x) {
            return Ok(Reply::Done(bad(&b, "the point depends on the variable")));
        }
        let inf = h.sym_term(h.sym("inf"));
        let contrary = (a == inf && direction == Direction::FromAbove)
            || (a == b.neg(inf) && direction == Direction::FromBelow);
        if contrary {
            return Ok(Reply::Done(bad(&b, "an infinity is approached from the finite side")));
        }
        let one = |cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>| -> Res<Option<T<'h>>> {
            Ok(match limit(cx, &b, g, x, a, direction)? {
                Outcome::Value(v) => Some(v),
                Outcome::NoLimit => Some(failure(&b, "no_limit", &[h.sym_term(h.sym("limit")), g])),
                Outcome::Unknown => None,
            })
        };
        let r = match h.view(*f) {
            View::App { head, args: items } if b.sym_of(head) == Some(b.hd.list) => {
                let mut out = Vec::with_capacity(items.len());
                for &g in items {
                    match one(cx, g)? {
                        Some(v) => out.push(v),
                        None => return Ok(Reply::Decline),
                    }
                }
                Some(h.call(b.hd.list, &out))
            }
            _ => one(cx, *f)?,
        };
        Ok(r.map_or(Reply::Decline, Reply::Again))
    }
}
