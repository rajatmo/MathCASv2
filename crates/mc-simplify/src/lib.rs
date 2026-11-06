//! MathCAS v2 simplification (roadmap package 2B) and assumptions (package A).
//!
//! - [`simplify`](mod@simplify): `simplify(e)`, the cheapest equivalent form found by a bounded search over
//!   rational, trigonometric and exponential rewritings (cost: the leaf count);
//! - [`trig`]: `trig_to_exp`, `exp_to_trig`, `trig_expand`, `trig_reduce`;
//! - [`power`]: `power_expand`, the power and logarithm laws without their side conditions;
//! - [`facts`]: the facts of an assumption context and the decisions they support (sign, bounds, realness,
//!   integrality);
//! - [`refine`](mod@refine): `refine(e)`, the identities that hold under the facts;
//! - [`term`]: the term builder and the bottom-up rewriting walk they share.
//!
//! The rational moves are package K's (`together`, `cancel`, `factor`, `expand`), called through the evaluator.
//!
//! **Assumptions** (semantics 13): the held form `assuming(facts, body)` is the evaluator's (`mc_eval::assume`),
//! installed here. `refine`, `simplify`, `power_expand` and the decision functions `is_positive`, `is_negative`,
//! `is_nonnegative`, `is_real` and `is_integer` declare `uses_assumptions` and read the context (decision A.4:
//! the decision functions of `mc-numeric` and `mc-lists` are replaced by assumption-aware ones that give the same
//! answers outside every context).
//!
//! ```
//! use mc_bench::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("simplify(sin(x)^2 + cos(x)^2)", &stop).unwrap().printed, "1");
//! assert_eq!(e.eval("simplify(d(x/(1 + x), x))", &stop).unwrap().printed, "(1 + x)^-2");
//! assert_eq!(e.eval("assuming(x > 0, simplify(sqrt(x^2)))", &stop).unwrap().printed, "x");
//! assert_eq!(e.eval("refine(abs(x), x < 0)", &stop).unwrap().printed, "-x");
//! ```
//!
//! Decisions of this package are numbered 2B.1, 2B.2, ... and A.1, A.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod complex;
pub mod element;
pub mod facts;
pub mod power;
pub mod refine;
pub mod simplify;
pub mod term;
pub mod trig;

use facts::{Facts, Kind, Syms};
use mc_api::{Store, View, T};
use mc_arith::num::StopScope;
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_numeric::numeval;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use term::B;

/// Every function of this package with a one-line description (`is_positive`, `is_negative`, `is_real` and
/// `is_integer` are listed by the packages that first defined them).
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "simplify",
        "simplify(e): the simplest equivalent form found (fewest leaves) by rational, trigonometric and exponential rewritings; inside assuming, also by refine",
    ),
    ("trig_expand", "trig_expand(e): sines and cosines of sums and integer multiples by the addition theorems, expanded"),
    ("trig_reduce", "trig_reduce(e): products and powers of sines and cosines as sums of multiple angles"),
    ("trig_to_exp", "trig_to_exp(e): trigonometric and hyperbolic functions as exponentials"),
    ("exp_to_trig", "exp_to_trig(e): exponentials of imaginary arguments by Euler's formula, exp(i x) = cos(x) + i sin(x)"),
    (
        "power_expand",
        "power_expand(e): (a b)^c = a^c b^c, (a^b)^c = a^(b c), log(a b) = log(a) + log(b), log(a^b) = b log(a), without side conditions (inside assuming, not for bases proved negative)",
    ),
    (
        "assuming",
        "assuming(facts, body): body evaluated with the assumption context extended by facts (x > 0, a <= b, element(n, integers), and, lists)",
    ),
    (
        "complex_expand",
        "complex_expand(e), complex_expand(e, [z1, ...]): e as a + b*im with a, b real, every symbol real except z1, ...",
    ),
    ("refine", "refine(e), refine(e, facts): e rewritten by the identities that hold under the facts of the assumption context (and facts)"),
    ("is_nonnegative", "is_nonnegative(x): true/false for a number, a closed numeric expression or an expression decided by the assumptions"),
    ("element", "element(x, domain), element([x, y], domain): the fact that x (and y) are in domain, for assuming and refine"),
    ("integers", "the domain of the integers (element(n, integers))"),
    ("rationals", "the domain of the rational numbers"),
    ("reals", "the domain of the real numbers"),
    ("complexes", "the domain of the complex numbers"),
    ("positive_integers", "the domain of the integers > 0"),
    ("negative_integers", "the domain of the integers < 0"),
    ("nonnegative_integers", "the domain of the integers >= 0"),
    ("nonpositive_integers", "the domain of the integers <= 0"),
    ("positive_rationals", "the domain of the rational numbers > 0"),
    ("negative_rationals", "the domain of the rational numbers < 0"),
    ("positive_reals", "the domain of the real numbers > 0"),
    ("negative_reals", "the domain of the real numbers < 0"),
    ("nonnegative_reals", "the domain of the real numbers >= 0"),
    ("nonpositive_reals", "the domain of the real numbers <= 0"),
];

/// The functions of this package.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fun {
    Simplify,
    TrigExpand,
    TrigReduce,
    TrigToExp,
    ExpToTrig,
    PowerExpand,
    Refine,
}

const TABLE: &[(&str, Fun)] = &[
    ("simplify", Fun::Simplify),
    ("trig_expand", Fun::TrigExpand),
    ("trig_reduce", Fun::TrigReduce),
    ("trig_to_exp", Fun::TrigToExp),
    ("exp_to_trig", Fun::ExpToTrig),
    ("power_expand", Fun::PowerExpand),
    ("refine", Fun::Refine),
];

/// The arguments before a trailing `options(...)`, and the value of its `assumptions` option (if any).
fn split_options<'a, 'h>(h: &'h Heap, args: &'a [T<'h>]) -> (&'a [T<'h>], Option<T<'h>>) {
    let Some((&last, rest)) = args.split_last() else { return (args, None) };
    let View::App { head, args: rules } = h.view(last) else { return (args, None) };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options") {
        return (args, None);
    }
    let mut assumed = None;
    for &r in rules {
        if let View::App { head, args: &[k, v] } = h.view(r) {
            let is = |t: T<'h>, n: &str| matches!(h.view(t), View::Sym(s) if h.sym_name(s) == n);
            if is(head, mc_api::names::RULE) && is(k, "assumptions") && !is(v, "true") {
                assumed = Some(v);
            }
        }
    }
    (rest, assumed)
}

impl Fun {
    /// The name of the function.
    fn name(self) -> &'static str {
        TABLE.iter().find(|e| e.1 == self).map_or("simplify", |e| e.0)
    }

    /// Does the function read the assumption context?
    fn uses_assumptions(self) -> bool {
        matches!(self, Fun::Simplify | Fun::PowerExpand | Fun::Refine)
    }
}

/// The native of one function: one argument, mapped over nothing else (a list argument is walked like any term).
struct SimplifyNative {
    f: Fun,
    syms: Arc<Syms>,
    stop: Arc<AtomicBool>,
}

impl Native for SimplifyNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h: &'h Heap = cx.heap();
        // options: `assumptions: a` adds the facts a; the others (`complexity_function`, `trig`, ...) are accepted
        // and ignored (decision A.8)
        let (args, assumed) = split_options(h, args);
        // refine(e, facts), simplify(e, facts): the function with the context extended by the facts (decision A.5)
        let extra: Vec<T<'h>> = match (self.f, args) {
            (Fun::Refine | Fun::Simplify, [_, facts]) => assumed.into_iter().chain([*facts]).collect(),
            (_, [_]) => assumed.into_iter().collect(),
            _ => return Ok(Reply::Decline),
        };
        if !extra.is_empty() && self.f.uses_assumptions() {
            let inner = h.call(h.sym(self.f.name()), &[args[0]]);
            return Ok(Reply::Done(cx.eval_assuming(&extra, inner)?));
        }
        let [e] = args else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let b = B::new(h, &self.stop);
        let facts = if self.f.uses_assumptions() { cx.assumptions() } else { &[] };
        // refine runs without facts too: some of its rules need none (`log(x^(1/2))`)
        let mut f = if facts.is_empty() && !matches!(self.f, Fun::Refine) { None } else { Some(Facts::new(cx, &b, &self.syms, facts)?) };
        let r = match self.f {
            Fun::Simplify => simplify::simplify(cx, &b, f.as_mut(), *e)?,
            Fun::TrigExpand => trig::trig_expand(cx, &b, *e)?,
            Fun::TrigReduce => trig::trig_reduce(cx, &b, *e)?,
            Fun::TrigToExp => trig::trig_to_exp(cx, &b, *e)?,
            Fun::ExpToTrig => trig::exp_to_trig(cx, &b, *e)?,
            Fun::PowerExpand => {
                let r = power::power_expand(cx, &b, f.as_mut(), *e)?;
                cx.eval(r)?
            }
            Fun::Refine => match f.as_mut() {
                Some(f) => refine::refine(cx, &b, f, *e)?,
                None => *e,
            },
        };
        Ok(Reply::Done(r))
    }

    fn reads_term(&self) -> bool {
        false
    }
}

/// `complex_expand(e)`, `complex_expand(e, z)`, `complex_expand(e, [z1, ...])` (decision A.9).
struct ComplexExpandNative {
    stop: Arc<AtomicBool>,
}

impl Native for ComplexExpandNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h: &'h Heap = cx.heap();
        let b = B::new(h, &self.stop);
        let (e, vars) = match args {
            [e] => (*e, vec![]),
            [e, v] => match b.call_of(*v) {
                Some((s, xs)) if s == b.hd.list => (*e, xs.to_vec()),
                _ => (*e, vec![*v]),
            },
            _ => return Ok(Reply::Decline),
        };
        let _scope = StopScope::enter(Some(&self.stop));
        let mut ex = complex::Expander::new(&b, vars);
        Ok(Reply::Done(ex.expand(cx, e)?))
    }

    fn reads_term(&self) -> bool {
        false
    }
}

/// A decision function (decision A.4).
#[derive(Clone, Copy)]
enum Decision {
    Positive,
    Negative,
    NonNegative,
    Real,
    Integer,
}

const DECISIONS: &[(&str, Decision)] = &[
    ("is_positive", Decision::Positive),
    ("is_negative", Decision::Negative),
    ("is_nonnegative", Decision::NonNegative),
    ("is_real", Decision::Real),
    ("is_integer", Decision::Integer),
];

/// An assumption-aware decision function: numbers and closed numeric expressions are decided as `mc-numeric`
/// decides them (and `is_integer` as `mc-lists`' kind predicate does), then the facts of the context are asked.
/// Undecided calls stay unevaluated, except `is_integer`, which answers `false` (decision 1A.1).
struct Decide {
    d: Decision,
    syms: Arc<Syms>,
    stop: Arc<AtomicBool>,
}

impl Decide {
    /// The answer without facts.
    fn plain<'h>(&self, h: &'h Heap, x: T<'h>) -> Option<bool> {
        if let Decision::Integer = self.d {
            return Some(matches!(h.view(x), View::Int(_) | View::BigInt { .. }));
        }
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let nm = &self.syms.num;
        match self.d {
            Decision::Positive => numeval::sign(h, &ar, nm, x).map(|o| o == Ordering::Greater),
            Decision::Negative => numeval::sign(h, &ar, nm, x).map(|o| o == Ordering::Less),
            Decision::NonNegative => numeval::sign(h, &ar, nm, x).map(|o| o != Ordering::Less),
            Decision::Real => numeval::is_real(h, &ar, nm, x),
            Decision::Integer => None,
        }
    }

    /// The answer from the facts.
    fn by_facts<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, facts: &[T<'h>], x: T<'h>) -> Res<Option<bool>> {
        let h = cx.heap();
        let b = B::new(h, &self.stop);
        let mut f = Facts::new(cx, &b, &self.syms, facts)?;
        let i = f.info(cx, x)?;
        Ok(match self.d {
            Decision::Positive => decided(f.positive(&i), f.nonpos(&i)),
            Decision::Negative => decided(f.negative(&i), f.nonneg(&i)),
            Decision::NonNegative => decided(f.nonneg(&i), f.negative(&i)),
            Decision::Real => i.is_real().then_some(true),
            Decision::Integer => (i.kind == Kind::Integer).then_some(true),
        })
    }
}

/// `Some(true)` when `yes` is proved, `Some(false)` when `no` is.
fn decided(yes: bool, no: bool) -> Option<bool> {
    if yes {
        Some(true)
    } else if no {
        Some(false)
    } else {
        None
    }
}

impl Native for Decide {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = args else { return Ok(Reply::Decline) };
        let h = cx.heap();
        cx.tick(1)?;
        let _scope = StopScope::enter(Some(&self.stop));
        let mut answer = self.plain(h, *x);
        // `is_integer` answers `false` for anything but an integer number; facts may still prove one
        if answer.is_none() || matches!((self.d, answer), (Decision::Integer, Some(false))) {
            let facts = cx.assumptions();
            if !facts.is_empty() {
                answer = self.by_facts(cx, facts, *x)?.or(answer);
            }
        }
        Ok(match answer {
            Some(v) => Reply::Done(cx.bool(v)),
            None => Reply::Decline,
        })
    }

    fn reads_term(&self) -> bool {
        false
    }
}

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag). Install after `mc-poly` (`simplify` calls its functions), `mc-numeric` and
/// `mc-lists` (the decision functions replace theirs, keeping their attributes).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    mc_eval::assume::install(k, h);
    let syms = Arc::new(Syms::new(h));
    for &(name, f) in TABLE {
        let a = Attributes { uses_assumptions: f.uses_assumptions(), ..Attributes::default() };
        k.register(h.sym(name), a, Box::new(SimplifyNative { f, syms: syms.clone(), stop: stop.clone() }));
    }
    k.register(h.sym("complex_expand"), Attributes::default(), Box::new(ComplexExpandNative { stop: stop.clone() }));
    k.register(h.sym("element"), Attributes::default(), Box::new(element::ElementNative { syms: syms.clone(), stop: stop.clone() }));
    for &(name, d) in DECISIONS {
        let s = h.sym(name);
        let a = Attributes { uses_assumptions: true, ..k.attributes(s) };
        k.register(s, a, Box::new(Decide { d, syms: syms.clone(), stop: stop.clone() }));
    }
}
