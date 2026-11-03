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
struct Tick<'c, 'a, 'k, 'h> {
    cx: &'c mut NativeCx<'a, 'k, 'h>,
    stop: &'c AtomicBool,
    abort: Option<Abort>,
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
        }
        let h = cx.heap();
        let env = Env { ar: Arith::new(h).with_stop(self.stop.clone()), hd: Heads::new(h), name: self.name };
        let _scope = StopScope::enter(Some(&self.stop));
        let mut tick = Tick { cx, stop: &self.stop, abort: None };
        let r = {
            let mut m = Meter::new(&mut tick);
            dispatch(self.f, &env, args, &mut m)
        };
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

fn dispatch<'h>(f: PolyFn, env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
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
        PolyFn::Series => crate::series::series_native(env, args, m),
        PolyFn::Normal => crate::series::normal_native(env, args, m),
    }
}
