//! MathCAS v2 simplification (roadmap item 2B) and assumptions (item A).
//!
//! - [`simplify`](mod@simplify): `simplify(e)`, the cheapest equivalent form found by a bounded search over
//!   rational, trigonometric and exponential rewritings (cost: [`cost`], a digit-weighted leaf count, or the
//!   `complexity_function:` option);
//! - [`cost`]: the complexity measure, cached over the hash-consed subterms of one call;
//! - [`trig`]: `trig_to_exp`, `exp_to_trig`, `trig_expand`, `trig_reduce`;
//! - [`power`]: `power_expand`, the power and logarithm laws without their side conditions;
//! - [`facts`]: the facts of an assumption context and the decisions they support (sign, bounds, realness,
//!   integrality);
//! - [`refine`](mod@refine): `refine(e)`, the identities that hold under the facts;
//! - [`term`]: the term builder and the bottom-up rewriting walk they share.
//!
//! The rational moves are item K's (`together`, `cancel`, `factor`, `expand`), called through the evaluator.
//!
//! **Assumptions** (semantics 13): the held form `assuming(facts, body)` is the evaluator's (`mc_eval::assume`),
//! installed here. `refine`, `simplify`, `power_expand` and the decision functions `is_positive`, `is_negative`,
//! `is_nonnegative`, `is_real` and `is_integer` declare `uses_assumptions` and read the context (decision A.4:
//! the decision functions of `mc-numeric` and `mc-lists` are replaced by assumption-aware ones that give the same
//! answers outside every context).
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("simplify(sin(x)^2 + cos(x)^2)", &stop).unwrap().printed, "1");
//! assert_eq!(e.eval("simplify(d(x/(1 + x), x))", &stop).unwrap().printed, "(1 + x)^-2");
//! assert_eq!(e.eval("assuming(x > 0, simplify(sqrt(x^2)))", &stop).unwrap().printed, "x");
//! assert_eq!(e.eval("refine(abs(x), x < 0)", &stop).unwrap().printed, "-x");
//! ```
//!
//! Decisions of this crate are numbered 2B.1, 2B.2, ... and A.1, A.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod boolean;
pub mod complex;
pub mod cost;
pub mod element;
pub mod facts;
pub mod fexpand;
pub mod identities;
pub mod logic;
pub mod moves;
pub mod power;
pub mod power_exact;
pub mod powers;
pub mod refine;
pub mod simplify;
pub mod term;
pub mod trig;
pub mod trigconst;
pub mod verify;

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

/// Every function of this crate with a one-line description (`is_positive`, `is_negative`, `is_real` and
/// `is_integer` are listed by the crates that first defined them).
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "simplify",
        "simplify(e): the simplest equivalent form found (fewest leaves, integers weighed by their digits; or by complexity_function: f) by rational, trigonometric and exponential rewritings; inside assuming, also by refine",
    ),
    (
        "full_simplify",
        "full_simplify(e): simplify with function_expand and the special-function identities as moves, every subexpression searched bottom up, a deeper search; options assumptions:, complexity_function:, transformation_functions:, excluded_forms:, time_constraint:, trig:",
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
    (
        "function_expand",
        "function_expand(e): special functions written with more elementary ones (factorial, binomial, beta, pochhammer as gamma functions; Bessel functions of order 1/2; erfc; reciprocal trigonometric functions)",
    ),
    ("refine", "refine(e), refine(e, facts): e rewritten by the identities that hold under the facts of the assumption context (and facts)"),
    ("is_nonnegative", "is_nonnegative(x): true/false for a number, a closed numeric expression or an expression decided by the assumptions"),
    ("element", "element(x, domain), element([x, y], domain): the fact that x (and y) are in domain, for assuming and refine"),
    ("not_element", "not_element(x, domain): true when x is not in domain, false when it is, else unevaluated"),
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

/// The functions of this crate.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fun {
    Simplify,
    FullSimplify,
    TrigExpand,
    TrigReduce,
    TrigToExp,
    ExpToTrig,
    PowerExpand,
    Refine,
    FunctionExpand,
}

const TABLE: &[(&str, Fun)] = &[
    ("simplify", Fun::Simplify),
    ("full_simplify", Fun::FullSimplify),
    ("trig_expand", Fun::TrigExpand),
    ("trig_reduce", Fun::TrigReduce),
    ("trig_to_exp", Fun::TrigToExp),
    ("exp_to_trig", Fun::ExpToTrig),
    ("power_expand", Fun::PowerExpand),
    ("refine", Fun::Refine),
    ("function_expand", Fun::FunctionExpand),
];

/// The options of a call this crate reads.
#[derive(Default)]
struct Opts<'h> {
    /// `assumptions: a` (`true` included: the empty context)
    assumed: Option<T<'h>>,
    /// `complexity_function: f` (not `automatic`), the rule itself kept to pass it on
    complexity: Option<(T<'h>, T<'h>)>,
    /// the options passed on when the call re-enters itself under extra facts (all but `assumptions:`)
    passed: Vec<T<'h>>,
    /// `transformation_functions: f` or `[automatic, f, ...]`
    transformations: Option<T<'h>>,
    /// `excluded_forms: p` or `[p, ...]`
    excluded: Option<T<'h>>,
    /// `time_constraint: s`
    time: Option<T<'h>>,
    /// `trig: false` was given
    no_trig: bool,
    /// `modulus: p` (`trig_reduce`; `modulus: 0` is no modulus)
    modulus: Option<T<'h>>,
    /// the internal flag of an explicit `assumptions:` on `power_expand`: the branch corrections are kept
    exact: bool,
}

/// The arguments before a trailing `options(...)`, and the options this crate reads.
fn split_options<'a, 'h>(h: &'h Heap, args: &'a [T<'h>]) -> (&'a [T<'h>], Opts<'h>) {
    let mut o = Opts::default();
    let Some((&last, rest)) = args.split_last() else { return (args, o) };
    let View::App { head, args: rules } = h.view(last) else { return (args, o) };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options") {
        return (args, o);
    }
    for &r in rules {
        if let View::App { head, args: &[k, v] } = h.view(r) {
            let is = |t: T<'h>, n: &str| matches!(h.view(t), View::Sym(s) if h.sym_name(s) == n);
            if !is(head, mc_api::names::RULE) {
                continue;
            }
            if is(k, "assumptions") {
                o.assumed = Some(v);
            }
            if is(k, "complexity_function") && !is(v, "automatic") {
                o.complexity = Some((r, v));
            }
            if !is(k, "assumptions") {
                o.passed.push(r);
            }
            if is(k, "transformation_functions") {
                o.transformations = Some(v);
            }
            if is(k, "excluded_forms") {
                o.excluded = Some(v);
            }
            if is(k, "time_constraint") {
                o.time = Some(v);
            }
            if is(k, "modulus") && !matches!(h.view(v), View::Int(0)) {
                o.modulus = Some(v);
            }
            if is(k, "exact_branches") {
                o.exact = true;
            }
            if is(k, "trig") && is(v, "false") {
                o.no_trig = true;
            }
        }
    }
    (rest, o)
}

impl Fun {
    /// The name of the function.
    fn name(self) -> &'static str {
        TABLE.iter().find(|e| e.1 == self).map_or("simplify", |e| e.0)
    }

    /// Does the function read the assumption context?
    fn uses_assumptions(self) -> bool {
        matches!(self, Fun::Simplify | Fun::FullSimplify | Fun::PowerExpand | Fun::Refine | Fun::FunctionExpand)
    }

    /// Does the function take facts as a second argument?
    fn takes_facts(self) -> bool {
        matches!(self, Fun::Simplify | Fun::FullSimplify | Fun::Refine | Fun::FunctionExpand)
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
        // options: `assumptions: a` replaces the surrounding context by the facts a (decision r17fix.5);
        // `complexity_function: f` is simplify's measure (decision s-cost.3); the others (`trig`, ...) are accepted
        // and ignored (decision A.8)
        let (args, opts) = split_options(h, args);
        let assumed = opts.assumed;
        // refine(e, facts), simplify(e, facts): the function with the context extended by the facts (decision A.5)
        let extra: Vec<T<'h>> = match (self.f.takes_facts(), args) {
            (true, [_, facts]) => assumed.into_iter().chain([*facts]).collect(),
            (_, [_]) => assumed.into_iter().collect(),
            _ => return Ok(Reply::Decline),
        };
        if !extra.is_empty() && self.f.uses_assumptions() {
            // an explicit `assumptions:` on power_expand asks for the exact laws (branch corrections)
            let mut passed = opts.passed.clone();
            if self.f == Fun::PowerExpand && assumed.is_some() {
                passed.push(h.call(h.sym(mc_api::names::RULE), &[h.sym_term(h.sym("exact_branches")), h.sym_term(h.sym("true"))]));
            }
            let inner = if passed.is_empty() {
                h.call(h.sym(self.f.name()), &[args[0]])
            } else {
                h.call(h.sym(self.f.name()), &[args[0], h.call(h.sym("options"), &passed)])
            };
            let v = if assumed.is_some() { cx.eval_replacing(&extra, inner)? } else { cx.eval_assuming(&extra, inner)? };
            return Ok(Reply::Done(v));
        }
        let [e] = args else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let b = B::new(h, &self.stop);
        let facts = if self.f.uses_assumptions() { cx.assumptions() } else { &[] };
        // refine and function_expand run without facts too: some of their rules need none (`log(x^(1/2))`)
        let needs = matches!(self.f, Fun::Refine | Fun::FunctionExpand);
        let mut f = if facts.is_empty() && !needs { None } else { Some(Facts::new(cx, &b, &self.syms, facts)?) };
        let r = match self.f {
            Fun::Simplify | Fun::FullSimplify => {
                // equational axioms (`for_all(vars, s == t)`) prove an equation outright (round 19)
                if let Some(v) = logic::prove(cx, &b, facts, *e)? {
                    return Ok(Reply::Done(v));
                }
                let measure = match opts.complexity {
                    Some((_, f)) => cost::Measure::User(f),
                    None => cost::Measure::Default,
                };
                let mut cfg = simplify::Config::new(self.f == Fun::FullSimplify, self.syms.clone());
                cfg.user_measure = opts.complexity.is_some();
                cfg.trig = !opts.no_trig;
                cfg.facts = facts.to_vec();
                cfg.stop = self.stop.clone();
                if let Some(tf) = opts.transformations {
                    let items = match b.call_of(tf) {
                        Some((l, xs)) if l == b.hd.list => xs.to_vec(),
                        _ => vec![tf],
                    };
                    let automatic = h.sym_term(h.sym("automatic"));
                    cfg.automatic = items.contains(&automatic);
                    cfg.user = items.into_iter().filter(|&x| x != automatic).collect();
                }
                if let Some(secs) = opts.time.and_then(|t| seconds(&b, t)) {
                    cfg.deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(secs);
                }
                let (hidden, back) = match opts.excluded {
                    Some(p) => hide(cx, &b, *e, p)?,
                    None => (*e, vec![]),
                };
                let mut c = cost::Cost::with(h, measure);
                let r = simplify::simplify(cx, &b, f.as_mut(), &mut c, &cfg, hidden)?;
                if back.is_empty() {
                    r
                } else {
                    let rules: Vec<T<'h>> = back.iter().map(|&(s, o)| b.call(h.sym(mc_api::names::RULE), &[s, o])).collect();
                    cx.eval(b.call_named("rewrite", &[r, b.call(b.hd.list, &rules)]))?
                }
            }
            Fun::TrigExpand => trig::trig_expand(cx, &b, *e)?,
            Fun::TrigReduce => match opts.modulus {
                None => trig::trig_reduce(cx, &b, *e)?,
                // a modulus that is no integer >= 2 leaves the expression as it is
                Some(p) => trig::trig_reduce_mod(cx, &b, *e, p)?.unwrap_or(*e),
            },
            Fun::TrigToExp => trig::trig_to_exp(cx, &b, *e)?,
            Fun::ExpToTrig => trig::exp_to_trig(cx, &b, *e)?,
            Fun::PowerExpand => {
                if opts.exact {
                    power_exact::power_expand_exact(cx, &b, f.as_mut(), *e)?
                } else {
                    let r = power::power_expand(cx, &b, f.as_mut(), *e)?;
                    cx.eval(r)?
                }
            }
            Fun::FunctionExpand => match f.as_mut() {
                Some(f) => fexpand::function_expand(cx, &b, f, *e)?,
                None => *e,
            },
            Fun::Refine => match f.as_mut() {
                Some(f) => {
                    let r = refine::refine(cx, &b, f, *e)?;
                    match case_split(cx, &b, facts, *e)? {
                        Some(v) if v != r => v,
                        _ => r,
                    }
                }
                None => *e,
            },
        };
        Ok(Reply::Done(r))
    }

    fn reads_term(&self) -> bool {
        false
    }
}

/// `refine(e)` under a fact `p or q or ...` of the context: the refinement under each alternative, when all give the
/// same term (`refine(abs(x^2 - 1), x <= -1 or x >= 1)` is `x^2 - 1` in both cases). Each alternative replaces the
/// `or` fact, so the recursion ends after as many splits as the context has `or` facts.
fn case_split<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, facts: &[T<'h>], e: T<'h>) -> Res<Option<T<'h>>> {
    let or = b.heap().sym("or");
    let Some(pos) = facts.iter().position(|&f| matches!(b.call_of(f), Some((s, a)) if s == or && a.len() > 1)) else { return Ok(None) };
    let Some((_, alts)) = b.call_of(facts[pos]) else { return Ok(None) };
    let rest: Vec<T<'h>> = facts.iter().enumerate().filter(|&(i, _)| i != pos).map(|(_, &f)| f).collect();
    let mut common: Option<T<'h>> = None;
    for &alt in alts {
        let mut ctx = rest.clone();
        ctx.push(alt);
        let v = cx.eval_replacing(&ctx, b.call_named("refine", &[e]))?;
        match common {
            None => common = Some(v),
            Some(c) if c == v => {}
            Some(_) => return Ok(None),
        }
    }
    Ok(common)
}

/// Seconds of a `time_constraint:` value (a positive number; `inf` for no limit beyond a day).
fn seconds<'h>(b: &B<'h>, t: T<'h>) -> Option<f64> {
    let h = b.heap();
    if matches!(h.view(t), View::Sym(s) if h.sym_name(s) == "inf") {
        return Some(86_400.0);
    }
    match b.ar.num(t)? {
        mc_arith::number::Num::Real(r) => {
            let v = r.to_f64();
            (v.is_finite() && v > 0.0).then_some(v.min(86_400.0))
        }
        _ => None,
    }
}

/// `t` with every subterm that matches a pattern of `excluded_forms:` replaced by a fresh symbol, and the
/// replacements (symbol, subterm) to put them back after the search (the subterms are not looked into).
fn hide<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, pats: T<'h>) -> Res<(T<'h>, Vec<(T<'h>, T<'h>)>)> {
    let pats = match b.call_of(pats) {
        Some((l, xs)) if l == b.hd.list => xs.to_vec(),
        _ => vec![pats],
    };
    let mut back = vec![];
    let r = hide_walk(cx, b, t, &pats, &mut back)?;
    Ok((r, back))
}

fn hide_walk<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, pats: &[T<'h>], back: &mut Vec<(T<'h>, T<'h>)>) -> Res<T<'h>> {
    let h = b.heap();
    if b.call_of(t).is_some() && !b.is_number(t) {
        for &p in pats {
            let m = cx.eval(b.call_named("match_q", &[t, p]))?;
            if cx.is_true(m) {
                if let Some(&(s, _)) = back.iter().find(|x| x.1 == t) {
                    return Ok(s);
                }
                let s = h.sym_term(h.sym(&format!("excluded${}", back.len() + 1)));
                back.push((s, t));
                return Ok(s);
            }
        }
    }
    match h.view(t) {
        View::App { head, args } if b.complex_parts(t).is_none() => {
            let mut out = Vec::with_capacity(args.len());
            for &a in args {
                out.push(hide_walk(cx, b, a, pats, back)?);
            }
            if out.as_slice() == args {
                Ok(t)
            } else {
                Ok(b.app(head, &out))
            }
        }
        _ => Ok(t),
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
        // a number that is not real is neither positive, negative nor nonnegative
        if matches!(self.d, Decision::Positive | Decision::Negative | Decision::NonNegative) && numeval::is_real(h, &ar, nm, x) == Some(false) {
            return Some(false);
        }
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

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
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
    k.register(h.sym("not_element"), Attributes::default(), Box::new(element::NotElementNative));
    for &(name, d) in DECISIONS {
        let s = h.sym(name);
        let a = Attributes { uses_assumptions: true, ..k.attributes(s) };
        k.register(s, a, Box::new(Decide { d, syms: syms.clone(), stop: stop.clone() }));
    }
}
