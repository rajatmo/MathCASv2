//! The natives: argument checking, conversion through [`crate::convert`], and the bridge from the evaluator's fuel
//! and interrupt to the kernels' [`Work`] budget.
//!
//! Every function is one [`PolyFn`] variant behind a single [`Native`] implementation. A native reads its
//! arguments, runs the kernel inside `mc-arith`'s stop scope (so that big-integer loops poll the session's stop
//! flag), and maps the outcome: a value is the normal form; an interrupt or fuel exhaustion returns the
//! evaluator's abort; a result beyond the memory limit is `failure(limit, memory)`; a malformed call is
//! `failure(argument, name, message)` (decision K.6).
use crate::convert::{Conv, Heads, Ring};
use crate::work::{Halt, Meter, Work, WorkResult};
use mc_arith::num::StopScope;
use mc_arith::Arith;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Abort, Native, NativeCx, Reply, Res};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The evaluator's fuel and interrupt as a [`Work`] budget, also polling `mc-arith`'s stop flag.
pub(crate) struct Tick<'c, 'a, 'k, 'h> {
    pub cx: &'c mut NativeCx<'a, 'k, 'h>,
    pub stop: &'c AtomicBool,
    pub abort: Option<Abort>,
}

/// A [`Tick`] with a wall-clock deadline (the internal `time_budget:` option, gap G73): past it the kernel stops
/// and `expired` is set, so the native can give its fallback instead of an interrupt.
pub(crate) struct Budgeted<'t, 'c, 'a, 'k, 'h> {
    pub tick: &'t mut Tick<'c, 'a, 'k, 'h>,
    pub deadline: Option<std::time::Instant>,
    pub expired: bool,
}

impl Work for Budgeted<'_, '_, '_, '_, '_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if self.deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            self.expired = true;
            return Err(Halt::Stopped);
        }
        self.tick.charge(units)
    }
}

/// The trailing `options(time_budget: s)` of a polynomial function (seconds, a positive number): the arguments
/// without it and the budget. Internal: `simplify` bounds its moves with it (gap G73).
fn time_budget_option<'a, 'h>(h: &'h Heap, args: &'a [T<'h>]) -> (&'a [T<'h>], Option<f64>) {
    let Some((&last, rest)) = args.split_last() else { return (args, None) };
    let View::App { head, args: &[r] } = h.view(last) else { return (args, None) };
    if head != h.sym_term(h.sym("options")) {
        return (args, None);
    }
    let View::App { args: &[k, v], .. } = h.view(r) else { return (args, None) };
    if k != h.sym_term(h.sym("time_budget")) {
        return (args, None);
    }
    let secs = match h.view(v) {
        View::Int(n) if n > 0 => n as f64,
        View::Real(x) if x > 0.0 && x.is_finite() => x,
        _ => return (args, None),
    };
    (rest, Some(secs))
}

impl Work for Tick<'_, '_, '_, '_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(Halt::Stopped);
        }
        match self.cx.tick(units) {
            Ok(()) => Ok(()),
            Err(a) => {
                self.abort = Some(a);
                Err(Halt::Stopped)
            }
        }
    }
}

/// What a kernel gives back to the native wrapper.
pub(crate) enum Out<'h> {
    /// a normal form
    Done(T<'h>),
    /// a term to evaluate (it contains calls of other functions, for example `exp(1)` in a series coefficient)
    Again(T<'h>),
    /// not this function's business: the call stays unevaluated (or rules apply)
    Decline,
}

/// Shared state of one native call.
pub(crate) struct Env<'h> {
    pub ar: Arith<'h, Heap>,
    pub hd: Heads<'h>,
    pub name: &'static str,
}

impl<'h> Env<'h> {
    pub fn h(&self) -> &'h Heap {
        self.ar.store()
    }
    pub fn conv(&self) -> Conv<'_, 'h, Heap> {
        Conv::new(&self.ar, &self.hd)
    }
    pub fn sym(&self, name: &str) -> T<'h> {
        let h = self.h();
        h.sym_term(h.sym(name))
    }
    /// `failure(argument, name, message)`.
    pub fn arg_failure(&self, message: &str) -> Out<'h> {
        let h = self.h();
        let f = h.call(h.sym(mc_api::names::FAILURE), &[self.sym("argument"), self.sym(self.name), h.string(message)]);
        Out::Done(f)
    }
    /// The elements of a list term.
    pub fn as_list(&self, t: T<'h>) -> Option<&'h [T<'h>]> {
        match self.h().view(t) {
            View::App { head, args } if head == self.hd.list => Some(args),
            _ => None,
        }
    }
    pub fn list(&self, items: &[T<'h>]) -> T<'h> {
        self.h().app(self.hd.list, items)
    }
    /// A machine integer argument.
    pub fn int(&self, t: T<'h>) -> Option<i64> {
        match self.h().view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }
    /// Can `t` be a polynomial variable (a kernel: not a number, sum or product)?
    pub fn is_variable(&self, t: T<'h>) -> bool {
        if self.ar.num(t).is_some() {
            return false;
        }
        !matches!(self.h().view(t), View::App { head, .. } if head == self.hd.add || head == self.hd.mul || head == self.hd.list)
    }
    /// The ring of `terms` with the variables `extra`, and `terms` expanded in it.
    pub fn read(
        &self,
        terms: &[T<'h>],
        extra: &[T<'h>],
        m: &mut Meter,
    ) -> WorkResult<(Ring<'h>, Vec<crate::qpoly::QPoly>)> {
        let c = self.conv();
        let (ring, shared) = c.ring(terms, extra, m)?;
        let mut out = Vec::with_capacity(terms.len());
        for &t in terms {
            out.push(c.to_poly(t, &ring, &shared, m)?);
        }
        Ok((ring, out))
    }
    /// The ring of `t` with the variables `extra`, and the terms of t's expansion (read directly when t is
    /// already expanded).
    pub fn read_terms(
        &self,
        t: T<'h>,
        extra: &[T<'h>],
        m: &mut Meter,
    ) -> WorkResult<(Ring<'h>, crate::convert::Terms)> {
        let c = self.conv();
        let (ring, shared) = c.ring(&[t], extra, m)?;
        if !ring.interacting() {
            if let Some(ts) = c.monomials(t, &ring, m)? {
                return Ok((ring, ts));
            }
        }
        let p = c.to_poly(t, &ring, &shared, m)?;
        Ok((ring, crate::convert::Terms::of_qpoly(&p)))
    }
}

/// The functions of this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PolyFn {
    Expand,
    Coefficient,
    CoefficientList,
    Degree,
    Variables,
    PolynomialGcd,
    PolynomialLcm,
    Gcd,
    Lcm,
    PolynomialQuotient,
    PolynomialRemainder,
    Factor,
    FactorSquareFree,
    Resultant,
    Discriminant,
    Together,
    Cancel,
    Series,
    Normal,
}

/// A function of this crate as a native.
pub(crate) struct PolyNative {
    pub f: PolyFn,
    pub name: &'static str,
    pub stop: Arc<AtomicBool>,
}

impl Native for PolyNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if let PolyFn::Gcd | PolyFn::Lcm = self.f {
            // this crate owns `gcd` and `lcm`; exact numbers go to the half-gcd of mc-numtheory (decision 9.1)
            let _scope = StopScope::enter(Some(&self.stop));
            match mc_numtheory::number_gcd_lcm(cx, args, self.f == PolyFn::Lcm)? {
                Reply::Decline => {}
                r => return Ok(r),
            }
            // a nonzero number with a symbolic argument: the symbol may stand for an integer (gcd(a, 12) in a
            // condition over the integers), so the call stays (decision IOSYM.14)
            let h = cx.heap();
            let ar = Arith::new(h);
            let exact = |t: &T<'_>| ar.num(*t).is_some_and(|n| n.is_exact());
            let number = |t: &T<'_>| ar.num(*t).is_some();
            if args.iter().any(|t| exact(t) && *t != h.int(0)) && args.iter().any(|t| !number(t)) {
                return Ok(Reply::Decline);
            }
        }
        let h = cx.heap();
        // an equation or inequality: both sides (decision IOSYM.12)
        if matches!(self.f, PolyFn::Factor | PolyFn::FactorSquareFree | PolyFn::Together | PolyFn::Cancel | PolyFn::Expand) {
            if let Some(r) = thread_relation(h, self.name, args) {
                return Ok(Reply::Again(r));
            }
        }
        // `time_budget: s` (internal, gap G73): past the budget the first argument comes back unchanged
        let (args, budget) = time_budget_option(h, args);
        let env = Env { ar: Arith::new(h).with_stop(self.stop.clone()), hd: Heads::new(h), name: self.name };
        let _scope = StopScope::enter(Some(&self.stop));
        // series: the Taylor coefficients of the functions its kernels do not know, through the evaluator
        // (`derivatives: false`, a trailing option, turns that off; the limit walk uses it)
        let (args, derivatives) = if self.f == PolyFn::Series { crate::series::derivatives_option(h, args) } else { (args, false) };
        let taylor = if derivatives { crate::series::taylor_table(cx, args)? } else { Default::default() };
        let mut tick = Tick { cx, stop: &self.stop, abort: None };
        let deadline = budget.map(|s| std::time::Instant::now() + std::time::Duration::from_secs_f64(s.min(1e6)));
        let mut budgeted = Budgeted { tick: &mut tick, deadline, expired: false };
        let r = {
            let mut m = Meter::new(&mut budgeted);
            dispatch(self.f, &env, args, &taylor, &mut m)
        };
        if budgeted.expired {
            if let (Err(Halt::Stopped), Some(&first)) = (&r, args.first()) {
                return Ok(Reply::Done(first));
            }
        }
        match r {
            Ok(Out::Done(t)) => Ok(Reply::Done(t)),
            Ok(Out::Again(t)) => Ok(Reply::Again(t)),
            Ok(Out::Decline) => Ok(Reply::Decline),
            Err(Halt::Stopped) => match tick.abort {
                Some(a) => Err(a),
                None => {
                    let f = h.sym(mc_api::names::FAILURE);
                    Ok(Reply::Done(h.call(f, &[env.sym(mc_api::names::failure::INTERRUPTED)])))
                }
            },
            Err(Halt::Memory) => {
                let f = h.sym(mc_api::names::FAILURE);
                let tags = [env.sym(mc_api::names::failure::LIMIT), env.sym(mc_api::names::failure::MEMORY)];
                Ok(Reply::Done(h.call(f, &tags)))
            }
        }
    }
}

fn dispatch<'h>(f: PolyFn, env: &Env<'h>, args: &[T<'h>], taylor: &crate::series::TaylorTable<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    use crate::functions as fns;
    match f {
        PolyFn::Expand => fns::expand(env, args, m),
        PolyFn::Coefficient => fns::coefficient(env, args, m),
        PolyFn::CoefficientList => fns::coefficient_list(env, args, m),
        PolyFn::Degree => fns::degree(env, args, m),
        PolyFn::Variables => fns::variables(env, args, m),
        PolyFn::PolynomialGcd => fns::polynomial_gcd(env, args, m, false),
        PolyFn::PolynomialLcm => fns::polynomial_gcd(env, args, m, true),
        PolyFn::Gcd => fns::gcd(env, args, m, false),
        PolyFn::Lcm => fns::gcd(env, args, m, true),
        PolyFn::PolynomialQuotient => fns::polynomial_division(env, args, m, true),
        PolyFn::PolynomialRemainder => fns::polynomial_division(env, args, m, false),
        PolyFn::Factor => fns::factor(env, args, m, false),
        PolyFn::FactorSquareFree => fns::factor(env, args, m, true),
        PolyFn::Resultant => fns::resultant(env, args, m),
        PolyFn::Discriminant => fns::discriminant(env, args, m),
        PolyFn::Together => fns::together(env, args, m, false),
        PolyFn::Cancel => fns::together(env, args, m, true),
        PolyFn::Series => crate::series::series_native(env, args, taylor, m),
        PolyFn::Normal => crate::series::normal_native(env, args, m),
    }
}

/// The relations a function of one expression maps over (both sides).
pub const RELATIONS: [&str; 6] = ["equal", "unequal", "less", "less_equal", "greater", "greater_equal"];

/// `f(l op r, rest...)` as `f(l, rest...) op f(r, rest...)` when the first argument is a relation (and `f` mapped
/// over the operands of `and`, `or`, `not`); `None` otherwise.
pub fn thread_relation<'h>(h: &'h Heap, name: &str, args: &[T<'h>]) -> Option<T<'h>> {
    let (&first, rest) = args.split_first()?;
    let View::App { head, args: parts } = h.view(first) else { return None };
    let View::Sym(s) = h.view(head) else { return None };
    let head_name = h.sym_name(s);
    // relations map over their two sides, the connectives over their operands
    let ok = match parts {
        [_, _] if RELATIONS.contains(&head_name.as_str()) => true,
        _ => matches!(head_name.as_str(), "and" | "or" | "not") && !parts.is_empty(),
    };
    if !ok {
        return None;
    }
    let side = |t: T<'h>| {
        let mut a = vec![t];
        a.extend_from_slice(rest);
        h.call(h.sym(name), &a)
    };
    let mapped: Vec<T<'h>> = parts.iter().map(|&p| side(p)).collect();
    Some(h.call(s, &mapped))
}
