//! The natives of the package and the comparison rules, as the evaluator sees them.
//!
//! | head | result |
//! |---|---|
//! | `exp`, `log`, `sin`, ... | an exact identity, a ball or machine value for a number, else unevaluated; lists map |
//! | `sqrt(x)` | the canonical power `x^(1/2)` |
//! | `n(e)`, `n(e, d)`, `n(e, digits: d)` | numerical evaluation (semantics 2.6); `failure(precision_not_reached, best)` |
//! | `is_positive`, `is_negative`, `is_real` | `true`/`false` for numbers and closed numeric terms when decided |
//! | rules on `less`, ..., `equal`, `unequal` | decided by refinement for closed numeric terms |
//!
//! The comparison natives of the engine decline for anything that is not a number, so the rules here see exactly
//! the comparisons involving constants and functions (decision L.8).
use crate::cball::CBall;
use crate::exact::Exact;
use crate::fun::{Fun, Mach};
use crate::numeval::{self, cball_term, first_precision, mach_term, Names, Target};
use mc_api::{names, Store, Sym, View, T};
use mc_arith::num::{stopped, StopScope};
use mc_arith::{Arith, Cmp3, Num, Real, DEFAULT_MAX_BITS};
use mc_core::Heap;
use mc_eval::pat::{call, cond, var};
use mc_eval::{Attributes, Binds, Kernel, Native, NativeCx, Reply, Res, Rhs};
use std::cell::Cell;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The flag `mc-arith`'s and this package's long loops poll.
type Stop = Arc<AtomicBool>;

/// Failure tags of this package.
pub mod failure_tags {
    /// `failure(precision_not_reached, best)`: `n(e, d)` did not reach d digits by the precision cap.
    pub const PRECISION_NOT_REACHED: &str = "precision_not_reached";
    /// `failure(invalid_argument, call)`: an argument of the wrong kind (`n(e, -1)`).
    pub const INVALID_ARGUMENT: &str = "invalid_argument";
}

fn failure<'h>(h: &'h Heap, tag: &str, detail: T<'h>) -> T<'h> {
    let tag = h.sym_term(h.sym(tag));
    h.call(h.sym(names::FAILURE), &[tag, detail])
}

/// `failure(interrupted, t)` once the stop flag is up (the evaluator's own interrupt is polled first).
fn interrupted<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Reply<'h>> {
    cx.tick(0)?;
    let h = cx.heap();
    let tag = h.sym_term(h.sym(names::failure::INTERRUPTED));
    Ok(Reply::Done(h.call(h.sym(names::FAILURE), &[tag, t])))
}

/// An elementary function.
struct Elementary {
    f: Fun,
    sym: Sym,
    nm: Arc<Names>,
    stop: Stop,
}

impl Native for Elementary {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        // atan(x, y): the argument of x + i y, for real numbers not both zero
        if let (Fun::Atan, &[x, y]) = (self.f, args) {
            let h = cx.heap();
            let ar = Arith::new(h);
            let real = |v: T<'h>| matches!(ar.num(v), Some(Num::Real(_)));
            let zero = |v: T<'h>| ar.num(v).is_some_and(|n| n.is_zero());
            if real(x) && real(y) && !(zero(x) && zero(y)) {
                let i = ar.complex(h.int(0), h.int(1));
                let z = ar.add(&[x, ar.mul(&[i, y])]);
                return Ok(Reply::Again(h.call(h.sym("arg"), &[z])));
            }
            return Ok(Reply::Decline);
        }
        let [x] = args else { return Ok(Reply::Decline) };
        // a truncated power series: the series of the composition (gap G59)
        if let Some(v) = crate::series_hook::apply(cx, &self.stop, self.f.name(), args)? {
            return Ok(Reply::Done(v));
        }
        let h = cx.heap();
        if crate::uncertain::any(h, args) {
            if let Some(v) = crate::uncertain::apply(cx, t, args)? {
                return Ok(Reply::Again(v));
            }
        }
        // a list of machine numbers: elementwise without the evaluator (gap queue G48)
        if self.f != Fun::Sqrt {
            if let Some(zs) = crate::machine::machine_list(h, *x) {
                cx.tick(zs.len() as u64 / 16)?;
                // each element by its kind: a complex leaf stays complex (as one number alone and packed arrays)
                let out: Vec<Mach> = zs.iter().map(|&z| self.f.machine(z)).collect();
                return Ok(Reply::Done(crate::machine::machine_list_term(h, &out)));
            }
        }
        // a list argument maps (decision L.10)
        if let Some(items) = cx.as_list(*x) {
            cx.tick(items.len() as u64)?;
            let mapped: Vec<T<'h>> = items.iter().map(|&i| h.call(self.sym, &[i])).collect();
            return Ok(Reply::Again(cx.list(&mapped)));
        }
        let _scope = StopScope::enter(Some(&self.stop));
        let ar = Arith::new(h).with_stop(self.stop.clone());
        // complex balls (N0C.1); sqrt of one is computed here, not left as a power
        if let Some(Num::Complex(Real::Ball(re, pa), Real::Ball(im, pb))) = ar.num(*x) {
            let p = pa.max(pb);
            cx.tick(1 + p / 1024)?;
            let r = self.f.cball(&CBall::new(re, im), p);
            if stopped() {
                return interrupted(cx, t);
            }
            match r {
                Some(r) => return Ok(Reply::Done(cball_term(&ar, r, p))),
                None if self.f != Fun::Sqrt => return Ok(Reply::Decline),
                None => {}
            }
        }
        if self.f == Fun::Sqrt {
            let half = ar.rational(h.int(1), h.int(2));
            return Ok(Reply::Done(ar.pow(*x, half)));
        }
        if let Some(r) = Exact::new(h, &ar, &self.nm).simplify(self.f, *x) {
            return Ok(Reply::Again(r));
        }
        let v = match ar.num(*x) {
            Some(Num::Real(Real::Ball(b, p))) => {
                cx.tick(1 + p / 1024)?;
                // outside the real domain: the principal complex value (N0C.4, as L.9 for machine reals)
                let r = match self.f.ball(&b, p) {
                    Some(r) => Some(CBall::real(r)),
                    None if stopped() => None,
                    None => self.f.cball(&CBall::real(b), p),
                };
                if stopped() {
                    return interrupted(cx, t);
                }
                match r {
                    Some(r) => cball_term(&ar, r, p),
                    None => return Ok(Reply::Decline),
                }
            }
            Some(Num::Real(Real::Machine(v))) => mach_term(&ar, self.f.machine(Mach::Real(v))),
            Some(Num::Complex(Real::Machine(re), Real::Machine(im))) => {
                mach_term(&ar, self.f.machine(Mach::Complex(crate::machine::C64::new(re, im))))
            }
            // exact numbers stay (L.2)
            _ => return Ok(Reply::Decline),
        };
        Ok(Reply::Done(v))
    }
}

/// `n(e)` and `n(e, d)`.
struct NumericN {
    nm: Arc<Names>,
    stop: Stop,
}

impl Native for NumericN {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // the option form n(x, digits: d) is n(x, d) (decision 1C.9)
        let (args, digits) = match split_options(h, args) {
            Ok(split) => split,
            Err(name) => return Ok(Reply::Done(failure(h, "unknown_option", h.sym_term(name)))),
        };
        let d = match (args, digits) {
            ([_], None) => None,
            (&[_, d], None) | (&[_], Some(d)) => Some(d),
            (&[_, d], Some(e)) if d == e => Some(d),
            ([_, _], Some(_)) => return Ok(Reply::Done(failure(h, failure_tags::INVALID_ARGUMENT, t))),
            _ => return Ok(Reply::Decline),
        };
        let x = args[0];
        let target = match d.map(|d| h.view(d)) {
            None => Target::Machine,
            Some(View::Int(d @ 1..)) => Target::Digits(d as u64),
            Some(_) => return Ok(Reply::Done(failure(h, failure_tags::INVALID_ARGUMENT, t))),
        };
        if let Target::Digits(d) = target {
            if first_precision(d) > DEFAULT_MAX_BITS {
                let limit = h.sym_term(h.sym(names::failure::LIMIT));
                let memory = h.sym_term(h.sym(names::failure::MEMORY));
                return Ok(Reply::Done(h.call(h.sym(names::FAILURE), &[limit, memory, t])));
            }
        }
        let _scope = StopScope::enter(Some(&self.stop));
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let r = numeval::numericize(h, &ar, &self.nm, x, target, &mut |n| cx.tick(n))?;
        if stopped() {
            return interrupted(cx, t);
        }
        Ok(if !r.reached {
            Reply::Done(failure(h, failure_tags::PRECISION_NOT_REACHED, r.term))
        } else if r.is_number {
            Reply::Done(r.term)
        } else {
            Reply::Again(r.term)
        })
    }
}

/// The arguments before a trailing `options(name -> v, ...)`, and the value of its `digits` option; `Err` with the
/// name of any other option. The last occurrence of a repeated option wins.
fn split_options<'a, 'h>(h: &'h Heap, args: &'a [T<'h>]) -> Result<(&'a [T<'h>], Option<T<'h>>), Sym> {
    let Some((&last, rest)) = args.split_last() else { return Ok((args, None)) };
    let View::App { head, args: rules } = h.view(last) else { return Ok((args, None)) };
    if head != h.sym_term(h.sym("options")) {
        return Ok((args, None));
    }
    let rule = h.sym_term(h.sym(names::RULE));
    let digits = h.sym("digits");
    let mut value = None;
    for &r in rules {
        match h.view(r) {
            View::App { head, args: &[k, v] } if head == rule => match h.view(k) {
                View::Sym(s) if s == digits => value = Some(v),
                View::Sym(s) => return Err(s),
                _ => return Ok((args, None)),
            },
            _ => return Ok((args, None)),
        }
    }
    Ok((rest, value))
}

/// `is_positive(x)`, `is_negative(x)` and `is_real(x)`.
struct Predicate {
    kind: Pred,
    nm: Arc<Names>,
    stop: Stop,
}

#[derive(Clone, Copy)]
enum Pred {
    Positive,
    Negative,
    Real,
}

impl Native for Predicate {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = args else { return Ok(Reply::Decline) };
        let h = cx.heap();
        cx.tick(1)?;
        let _scope = StopScope::enter(Some(&self.stop));
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let decided = match self.kind {
            Pred::Positive => numeval::sign(h, &ar, &self.nm, *x).map(|o| o == Ordering::Greater),
            Pred::Negative => numeval::sign(h, &ar, &self.nm, *x).map(|o| o == Ordering::Less),
            Pred::Real => numeval::is_real(h, &ar, &self.nm, *x),
        };
        Ok(match decided {
            Some(b) => Reply::Done(cx.bool(b)),
            None => Reply::Decline,
        })
    }
}

/// The comparison heads with rules here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rel {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    Unequal,
}

impl Rel {
    const ALL: [Rel; 6] = [Rel::Less, Rel::LessEqual, Rel::Greater, Rel::GreaterEqual, Rel::Equal, Rel::Unequal];

    fn name(self) -> &'static str {
        match self {
            Rel::Less => "less",
            Rel::LessEqual => "less_equal",
            Rel::Greater => "greater",
            Rel::GreaterEqual => "greater_equal",
            Rel::Equal => "equal",
            Rel::Unequal => "unequal",
        }
    }

    /// The truth value for a decided comparison. Equality is decided `false` only for values proven apart (and
    /// `true` for two exact points that coincide); NaN makes every order relation and `==` false (2.5).
    fn holds(self, c: Cmp3) -> Option<bool> {
        let o = match c {
            Cmp3::Less => Ordering::Less,
            Cmp3::Equal => Ordering::Equal,
            Cmp3::Greater => Ordering::Greater,
            Cmp3::Unordered => return Some(self == Rel::Unequal),
            Cmp3::Undecided => return None,
        };
        Some(match self {
            Rel::Less => o.is_lt(),
            Rel::LessEqual => o.is_le(),
            Rel::Greater => o.is_gt(),
            Rel::GreaterEqual => o.is_ge(),
            Rel::Equal => o.is_eq(),
            Rel::Unequal => o.is_ne(),
        })
    }
}

thread_local! {
    /// The decision the last comparison condition reached on this thread, keyed by the operand handles and the
    /// relation: the right side, instantiated right after the condition on the same task, reads it instead of
    /// refining again.
    static DECIDED: Cell<Option<(u32, u32, Rel, bool)>> = const { Cell::new(None) };
}

fn decide<'h>(h: &'h Heap, nm: &Names, stop: &Stop, rel: Rel, a: T<'h>, b: T<'h>) -> Option<bool> {
    let _scope = StopScope::enter(Some(stop));
    let ar = Arith::new(h).with_stop(stop.clone());
    // quantities: their magnitudes in a common unit
    let cm = crate::quantity::common_magnitudes(h, &ar, a, b);
    let (a, b) = cm.unwrap_or((a, b));
    if let Some((up, down)) = crate::uncertain::compare_intervals(h, &ar, nm, a, b) {
        // up: max(a) against min(b); down: min(a) against max(b)
        use Cmp3::{Equal as E, Greater as G, Less as L};
        let below = up == L;
        let at_most = matches!(up, L | E);
        let above = down == G;
        let at_least = matches!(down, G | E);
        return match rel {
            Rel::Less if below => Some(true),
            Rel::Less if at_least => Some(false),
            Rel::LessEqual if at_most => Some(true),
            Rel::LessEqual if above => Some(false),
            Rel::Greater if above => Some(true),
            Rel::Greater if at_most => Some(false),
            Rel::GreaterEqual if at_least => Some(true),
            Rel::GreaterEqual if below => Some(false),
            Rel::Equal if below || above => Some(false),
            Rel::Unequal if below || above => Some(true),
            _ => None,
        };
    }
    // two numbers (the magnitudes of quantities): the canonical comparison
    if let (Some(_), Some(_)) = (ar.num(a), ar.num(b)) {
        return rel.holds(ar.compare(a, b)?);
    }
    rel.holds(numeval::compare(h, &ar, nm, a, b)?)
}

/// `rel(a, b) /; decided := the decision`.
fn define_comparison(k: &mut Kernel, h: &Heap, rel: Rel, nm: &Arc<Names>, stop: &Stop) {
    let s = h.sym(rel.name());
    let (nm_c, stop_c) = (nm.clone(), stop.clone());
    let lhs = cond(call(h, s, vec![var(0), var(1)]), &[0, 1], move |cx| {
        let (a, b) = (cx.one(0), cx.one(1));
        let d = decide(cx.heap(), &nm_c, &stop_c, rel, a, b);
        if let Some(v) = d {
            DECIDED.with(|c| c.set(Some((a.raw(), b.raw(), rel, v))));
        }
        Ok(d.is_some())
    });
    let (nm_r, stop_r) = (nm.clone(), stop.clone());
    let rhs: Rhs = Arc::new(move |binds: &Binds<'_>, h: &Heap| {
        let (a, b) = (binds.one(0), binds.one(1));
        let v = match DECIDED.with(|c| c.take()) {
            Some((x, y, r, v)) if (x, y, r) == (a.raw(), b.raw(), rel) => Some(v),
            _ => decide(h, &nm_r, &stop_r, rel, a, b),
        };
        match v {
            Some(v) => h.sym_term(h.sym(if v { names::TRUE } else { names::FALSE })),
            // only if the condition's decision is lost and cannot be repeated (a raised stop flag)
            None => failure(h, failure_tags::PRECISION_NOT_REACHED, h.call(s, &[a, b])),
        }
    });
    k.define(h, lhs, rhs).expect("a two-variable comparison pattern is valid");
}

/// Register the functions of this package with `k` (heap `h`), with a stop flag of their own. See
/// [`install_with_stop`].
pub fn install(k: &mut Kernel, h: &Heap) {
    install_with_stop(k, h, &Arc::new(AtomicBool::new(false)));
}

/// Register the functions of this package with `k` (heap `h`, decision L.1): the natives of the elementary
/// functions, `n`, the predicates, and the comparison rules. `stop` is the flag the long numeric loops poll (the
/// engine raises `mc-arith`'s flag with the evaluator's interrupt; passing that flag makes a 10^6-digit
/// computation stop promptly).
pub fn install_with_stop(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let nm = Arc::new(Names::new(h));
    let strict = Attributes::default();
    for f in Fun::ALL {
        let sym = h.sym(f.name());
        k.register(sym, strict, Box::new(Elementary { f, sym, nm: nm.clone(), stop: stop.clone() }));
    }
    k.register(h.sym("n"), strict, Box::new(NumericN { nm: nm.clone(), stop: stop.clone() }));
    for (name, kind) in [("is_positive", Pred::Positive), ("is_negative", Pred::Negative), ("is_real", Pred::Real)] {
        k.register(h.sym(name), strict, Box::new(Predicate { kind, nm: nm.clone(), stop: stop.clone() }));
    }
    for rel in Rel::ALL {
        define_comparison(k, h, rel, &nm, stop);
    }
    crate::numbers::install(k, h, &nm, stop);
    crate::formula::install(k, h, crate::more::DEFS);
    crate::formula_lib::register();
    crate::formula::install(k, h, crate::uncertain::DEFS);
    crate::formula::install(k, h, crate::quantity::DEFS);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relations() {
        assert_eq!(Rel::Less.holds(Cmp3::Less), Some(true));
        assert_eq!(Rel::GreaterEqual.holds(Cmp3::Equal), Some(true));
        assert_eq!(Rel::Equal.holds(Cmp3::Greater), Some(false));
        assert_eq!(Rel::Unequal.holds(Cmp3::Unordered), Some(true));
        assert_eq!(Rel::Less.holds(Cmp3::Unordered), Some(false));
        assert_eq!(Rel::Less.holds(Cmp3::Undecided), None);
    }
}
