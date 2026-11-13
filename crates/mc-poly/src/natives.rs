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
            // raised with the evaluator's interrupt, which the fuel poll has not seen yet: the stop is the abort (a
            // caller that sees no abort tries its next method with the number kernels returning arbitrary values)
            if self.cx.stopped() {
                self.abort = Some(mc_eval::Abort::Limit);
            }
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
    /// The arguments of a call `name(...)`.
    pub fn args_of(&self, t: T<'h>, name: &str) -> Option<&'h [T<'h>]> {
        let h = self.h();
        match h.view(t) {
            View::App { head, args } if head == h.sym_term(h.sym(name)) => Some(args),
            _ => None,
        }
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
    Subresultants,
    SubresultantPolynomials,
    SubresultantPolynomialRemainders,
    Decompose,
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
        // the options of polynomial_gcd and polynomial_lcm (`modulus:`, `extension:`, `trig:`)
        let mut modulus = None;
        if matches!(self.f, PolyFn::PolynomialGcd | PolyFn::PolynomialLcm) {
            match gcd_options(cx, self.f == PolyFn::PolynomialLcm, self.name, args)? {
                GcdOpts::None => {}
                GcdOpts::Reply(r) => return Ok(r),
                GcdOpts::Modulus(rest, p) => modulus = Some((rest, p)),
            }
        }
        let args = modulus.as_ref().map_or(args, |(rest, _)| rest.as_slice());
        // `time_budget: s` (internal, gap G73): past the budget the first argument comes back unchanged
        let (args, budget) = time_budget_option(h, args);
        let env = Env { ar: Arith::new(h).with_stop(self.stop.clone()), hd: Heads::new(h), name: self.name };
        let _scope = StopScope::enter(Some(&self.stop));
        // series: the Taylor coefficients of the functions its kernels do not know, through the evaluator
        // (`derivatives: false`, a trailing option, turns that off; the limit walk uses it)
        let (args, derivatives) = if self.f == PolyFn::Series { crate::series::derivatives_option(h, args) } else { (args, false) };
        let taylor = if derivatives {
            crate::series::taylor_table(cx, args)?
        } else if self.f == PolyFn::Series {
            crate::series::non_analytic_table(cx, args)?
        } else {
            Default::default()
        };
        let mut tick = Tick { cx, stop: &self.stop, abort: None };
        let deadline = budget.map(|s| std::time::Instant::now() + std::time::Duration::from_secs_f64(s.min(1e6)));
        let mut budgeted = Budgeted { tick: &mut tick, deadline, expired: false };
        let r = {
            let mut m = Meter::new(&mut budgeted);
            match modulus {
                Some((_, p)) => crate::functions::polynomial_gcd_mod(&env, args, &mut m, self.f == PolyFn::PolynomialLcm, p),
                None => dispatch(self.f, &env, args, &taylor, &mut m),
            }
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

/// What the options of `polynomial_gcd`/`polynomial_lcm` ask for.
enum GcdOpts<'h> {
    /// no options: the plain gcd
    None,
    /// the reply (computed here, or a failure)
    Reply(Reply<'h>),
    /// the polynomials, to be taken modulo the prime
    Modulus(Vec<T<'h>>, u64),
}

/// The trailing `options(...)` of `polynomial_gcd(p, q, ...)` and `polynomial_lcm(p, q, ...)`:
///
/// - `modulus: p` (a prime; 0 is none): over `F_p` ([`crate::functions::polynomial_gcd_mod`]);
/// - `extension: a` (or `automatic`): the gcd is mc-algebra's (it wraps `polynomial_gcd`); the lcm is
///   `p (q / gcd(p, q))` with the quotient by the monic gcd taken in `Q[a][x]`, which is exact modulo the minimal
///   polynomial of `a` because the divisor is monic;
/// - `trig: true`: the arguments trig-expanded, with even powers of `cos(u)` written through `sin(u)` (a normal form
///   modulo `sin^2 + cos^2 = 1`), then the plain gcd or lcm.
///
/// Options set to their defaults (`extension: none`, `trig: false`, `modulus: 0`) are dropped; another option is
/// an argument failure (it used to be read as a factor of the polynomials).
fn gcd_options<'h>(cx: &mut NativeCx<'_, '_, 'h>, lcm: bool, name: &'static str, args: &[T<'h>]) -> Res<GcdOpts<'h>> {
    use mc_api::names;
    let h = cx.heap();
    let Some((&last, polys)) = args.split_last() else { return Ok(GcdOpts::None) };
    let View::App { head, args: rules } = h.view(last) else { return Ok(GcdOpts::None) };
    if head != h.sym_term(h.sym("options")) || polys.is_empty() {
        return Ok(GcdOpts::None);
    }
    let fail = |msg: &str| {
        let f = h.call(h.sym(names::FAILURE), &[h.sym_term(h.sym("argument")), h.sym_term(h.sym(name)), h.string(msg)]);
        Ok(GcdOpts::Reply(Reply::Done(f)))
    };
    let sym = |t: T<'h>| match h.view(t) {
        View::Sym(s) => Some(h.sym_name(s).to_string()),
        _ => None,
    };
    let (mut modulus, mut extension, mut trig) = (None, None, false);
    for &r in rules {
        let View::App { args: &[k, v], .. } = h.view(r) else { return fail("options are rules") };
        match sym(k).as_deref() {
            Some("time_budget") => return Ok(GcdOpts::None),
            Some("modulus") => match h.view(v) {
                View::Int(0) => {}
                View::Int(p) if p > 1 && crate::modp::is_prime(p as u64) => modulus = Some(p as u64),
                _ => return fail("the modulus must be a prime (or 0)"),
            },
            Some("extension") => match sym(v).as_deref() {
                Some("none" | "false") => {}
                _ => extension = Some(v),
            },
            Some("trig") => match sym(v).as_deref() {
                Some("true") => trig = true,
                Some("false") => {}
                _ => return fail("trig is true or false"),
            },
            _ => return fail("the options are modulus, extension and trig"),
        }
    }
    let call = |f: &str, xs: &[T<'h>]| h.call(h.sym(f), xs);
    let mut polys = polys.to_vec();
    if trig {
        for p in &mut polys {
            let e = cx.eval(call("expand", &[call("trig_expand", &[*p])]))?;
            *p = cx.eval(call("expand", &[cos_squares(h, e)]))?;
        }
    }
    if let Some(p) = modulus {
        return Ok(GcdOpts::Modulus(polys, p));
    }
    let Some(ext) = extension else {
        // only trig: the plain function of the rewritten polynomials
        return Ok(GcdOpts::Reply(Reply::Again(call(name, &polys))));
    };
    let opts = call("options", &[call(names::RULE, &[h.sym_term(h.sym("extension")), ext])]);
    if !lcm {
        // mc-algebra's wrapper of polynomial_gcd takes these calls before this native; here it declined
        return Ok(GcdOpts::Reply(Reply::Decline));
    }
    // the variable: the one symbol of the polynomials
    let vars = cx.eval(call("variables", &[call(names::ADD, &polys)]))?;
    let syms: Vec<T<'h>> = cx.as_list(vars).unwrap_or(&[]).iter().copied().filter(|&v| matches!(h.view(v), View::Sym(_))).collect();
    let [x] = syms.as_slice() else { return Ok(GcdOpts::Reply(Reply::Decline)) };
    let x = *x;
    let is_head = |t: T<'h>, n: &str| matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(n)));
    let mut acc = polys[0];
    for &p in &polys[1..] {
        let g = cx.eval(call("polynomial_gcd", &[acc, p, opts]))?;
        if is_head(g, "polynomial_gcd") || is_head(g, names::FAILURE) {
            return Ok(GcdOpts::Reply(Reply::Decline));
        }
        if g == h.int(0) || acc == h.int(0) || p == h.int(0) {
            acc = h.int(0);
            continue;
        }
        // p divided exactly by the gcd made monic
        let d = cx.eval(call("degree", &[g, x]))?;
        let lc = cx.eval(call("coefficient", &[g, x, d]))?;
        let gm = cx.eval(call("expand", &[call(names::MUL, &[g, call(names::POW, &[lc, h.int(-1)])])]))?;
        let q = cx.eval(call("polynomial_quotient", &[p, gm, x]))?;
        if is_head(q, "polynomial_quotient") || is_head(q, names::FAILURE) {
            return Ok(GcdOpts::Reply(Reply::Decline));
        }
        acc = cx.eval(call("expand", &[call(names::MUL, &[acc, q])]))?;
    }
    // a positive leading coefficient, as for the plain lcm
    let d = cx.eval(call("degree", &[acc, x]))?;
    let lc = cx.eval(call("coefficient", &[acc, x, d]))?;
    let neg = cx.eval(call("less", &[lc, h.int(0)]))?;
    if neg == h.sym_term(h.sym(names::TRUE)) {
        acc = cx.eval(call("expand", &[call(names::MUL, &[h.int(-1), acc])]))?;
    }
    Ok(GcdOpts::Reply(Reply::Done(acc)))
}

/// `t` with each `cos(u)^k` (k >= 2) written as `(1 - sin(u)^2)^(k div 2) cos(u)^(k mod 2)`.
fn cos_squares<'h>(h: &'h Heap, t: T<'h>) -> T<'h> {
    use mc_api::names;
    let (cos, pow) = (h.sym_term(h.sym("cos")), h.sym_term(h.sym(names::POW)));
    match h.view(t) {
        View::App { head, args: &[base, ex] } if head == pow => {
            if let (View::App { head: bh, args: &[u] }, View::Int(k)) = (h.view(base), h.view(ex)) {
                if bh == cos && k >= 2 {
                    let s2 = h.call(h.sym(names::POW), &[h.call(h.sym("sin"), &[u]), h.int(2)]);
                    let one_minus = h.call(h.sym(names::ADD), &[h.int(1), h.call(h.sym(names::MUL), &[h.int(-1), s2])]);
                    let p = h.call(h.sym(names::POW), &[one_minus, h.int(k / 2)]);
                    return if k % 2 == 1 { h.call(h.sym(names::MUL), &[p, base]) } else { p };
                }
            }
            h.app(head, &[cos_squares(h, base), cos_squares(h, ex)])
        }
        View::App { head, args } => {
            let xs: Vec<T<'h>> = args.iter().map(|&a| cos_squares(h, a)).collect();
            h.app(head, &xs)
        }
        _ => t,
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
        PolyFn::Subresultants => crate::ringfns::chain_fn(env, args, crate::ringfns::ChainKind::Coefficients, m),
        PolyFn::SubresultantPolynomials => crate::ringfns::chain_fn(env, args, crate::ringfns::ChainKind::Polynomials, m),
        PolyFn::SubresultantPolynomialRemainders => {
            crate::ringfns::chain_fn(env, args, crate::ringfns::ChainKind::Remainders, m)
        }
        PolyFn::Decompose => crate::ringfns::decompose_fn(env, args, m),
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
