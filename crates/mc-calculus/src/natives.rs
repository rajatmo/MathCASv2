//! The native of `d`: the variable specifications, gradients over lists, and the hand-back to the evaluator.
//!
//! `d(f, s1, s2, ...)` applies the specifications from left to right: a variable `x` differentiates once, `[x, n]`
//! (n a non-negative integer) n times, and a list of variables `[x, y, ...]` makes a gradient: every scalar part of
//! `f` (a list maps elementwise) becomes the list of its derivatives, so the gradient of a list of functions is its
//! Jacobian (decision 2A.2). The result is handed back with [`Reply::Again`], so the calls it contains (`cos(0)`,
//! `log(e)`, ...) are evaluated (decision 2A.1).
use crate::diff::{nth, Halt};
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_core::Heap;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// One specification of `d`.
enum Spec<'h> {
    /// differentiate `n` times in the variable
    Var(T<'h>, i64),
    /// the gradient in these variables
    Grad(Vec<T<'h>>),
}

/// `d(f, specs...)`.
pub struct D {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// `failure(argument, d, message)` (the style of the other library packages).
fn arg_failure<'h>(h: &'h Heap, message: &str) -> T<'h> {
    let tags = [h.sym_term(h.sym("argument")), h.sym_term(h.sym("d")), h.string(message)];
    h.call(h.sym(names::FAILURE), &tags)
}

impl Native for D {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // fewer than two arguments is no derivative: the call stays as it is, so `d` remains usable as a plain
        // symbol in programs that do not differentiate (decision 2A.6)
        let [f, specs @ ..] = args else { return Ok(Reply::Decline) };
        if specs.is_empty() {
            return Ok(Reply::Decline);
        }
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let mut parsed = Vec::with_capacity(specs.len());
        for &s in specs {
            match spec(&b, s) {
                Ok(p) => parsed.push(p),
                Err(message) => return Ok(Reply::Done(arg_failure(h, message))),
            }
        }
        let mut tick = |n: u64| cx.tick(n);
        let mut t = *f;
        for s in &parsed {
            let r = match s {
                Spec::Var(x, n) => nth(&b, t, *x, *n, &mut tick),
                Spec::Grad(xs) => gradient(&b, t, xs, &mut tick),
            };
            t = match r {
                Ok(t) => t,
                Err(Halt::Abort(a)) => return Err(a),
                Err(Halt::Unsupported) => return Ok(Reply::Decline),
            };
        }
        Ok(Reply::Again(t))
    }
}

/// Is `t` something `d` can differentiate in: not a number, string, list, sum, product or power?
pub(crate) fn variable<'h>(b: &Build<'h>, t: T<'h>) -> Result<T<'h>, &'static str> {
    let h = b.heap();
    if b.ar.num(t).is_some() {
        return Err("a number is not a variable");
    }
    match h.view(t) {
        View::Str(_) => Err("a string is not a variable"),
        View::App { head, .. } => match b.sym_of(head) {
            Some(s) if s == b.hd.list => Err("a gradient takes a list of variables"),
            Some(s) if s == b.hd.add || s == b.hd.mul || s == b.hd.pow => {
                Err("a sum, product or power is not a variable")
            }
            _ => Ok(t),
        },
        _ => Ok(t),
    }
}

/// Read one specification: `x`, `[x, n]` or `[x, y, ...]`.
fn spec<'h>(b: &Build<'h>, s: T<'h>) -> Result<Spec<'h>, &'static str> {
    let h = b.heap();
    let items = match h.view(s) {
        View::App { head, args } if b.sym_of(head) == Some(b.hd.list) => args,
        _ => return Ok(Spec::Var(variable(b, s)?, 1)),
    };
    if let [x, n] = items {
        if b.ar.num(*n).is_some() {
            return match h.view(*n) {
                View::Int(n @ 0..) => Ok(Spec::Var(variable(b, *x)?, n)),
                _ => Err("the order of a derivative must be a non-negative machine integer"),
            };
        }
    }
    let xs = items.iter().map(|&x| variable(b, x)).collect::<Result<Vec<_>, _>>()?;
    Ok(Spec::Grad(xs))
}

/// The gradient of every scalar part of `t` in `xs`: lists map, every other term becomes `[d(t, x1), ...]`.
fn gradient<'h>(b: &Build<'h>, t: T<'h>, xs: &[T<'h>], tick: &mut crate::diff::Tick<'_>) -> Result<T<'h>, Halt> {
    let h = b.heap();
    // post-order over the list structure (an explicit stack: lists may nest deeply)
    let mut out: Vec<T<'h>> = Vec::new();
    let mut stack = vec![(t, false)];
    while let Some((u, ready)) = stack.pop() {
        let items = match h.view(u) {
            View::App { head, args } if b.sym_of(head) == Some(b.hd.list) => args,
            _ => {
                let mut g = Vec::with_capacity(xs.len());
                for &x in xs {
                    g.push(nth(b, u, x, 1, tick)?);
                }
                out.push(h.call(b.hd.list, &g));
                continue;
            }
        };
        if ready {
            let at = out.len() - items.len();
            let l = h.call(b.hd.list, &out[at..]);
            out.truncate(at);
            out.push(l);
        } else {
            stack.push((u, true));
            stack.extend(items.iter().rev().map(|&i| (i, false)));
        }
    }
    Ok(out.pop().expect("one result"))
}

/// `derivative(n1, ..., nk)(f)(a1, ..., ak)` for a known function `f`: its derivative at the arguments, so `sin'(x)`
/// is `cos(x)` outside `d` too (decision 2A.7). A curried call of the native; an unknown `f`, a wrong number of
/// arguments or orders that are not non-negative integers leave the call as it is.
pub struct DerivativeAt {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

impl Native for DerivativeAt {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let Some((orders, f)) = b.as_derivative(head) else { return Ok(Reply::Decline) };
        if orders.len() != args.len() || orders.iter().any(|&n| n < 0) {
            return Ok(Reply::Decline);
        }
        if let Some(g) = interpolating_derivative(&b, f, &orders) {
            return Ok(Reply::Again(h.app(g, args)));
        }
        let known = b.sym_of(f).is_some_and(|s| crate::rules::resolvable(&h.sym_name(s), &orders));
        if !known {
            return Ok(Reply::Decline);
        }
        let mut tick = |n: u64| cx.tick(n);
        match crate::diff::resolve(&b, f, &orders, args, &mut tick) {
            Ok(r) => Ok(Reply::Again(r)),
            Err(Halt::Abort(a)) => Err(a),
            Err(Halt::Unsupported) => Ok(Reply::Decline),
        }
    }

    fn curried(&self) -> bool {
        true
    }
}

/// The highest derivative order written as primes on an interpolating function's name.
const MAX_PRIMES: i64 = 1000;

/// `derivative(n)(interpolating_function("y", rest...))` is the interpolating function of the n-th derivative,
/// `interpolating_function("y" + n primes, rest...)`: the solution store of `n_dsolve` reads the primes as the
/// derivative order (decision C.6). `None` for anything else.
fn interpolating_derivative<'h>(b: &Build<'h>, f: T<'h>, orders: &[i64]) -> Option<T<'h>> {
    let h = b.heap();
    let &[n @ 0..=MAX_PRIMES] = orders else { return None };
    let View::App { head, args: [name, rest @ ..] } = h.view(f) else { return None };
    if b.sym_of(head).map(|s| h.sym_name(s)).as_deref() != Some("interpolating_function") {
        return None;
    }
    let View::Str(name) = h.view(*name) else { return None };
    let mut primed = String::with_capacity(name.len() + n as usize);
    primed.push_str(name);
    primed.extend(std::iter::repeat_n('\'', n as usize));
    let mut items = vec![h.string(&primed)];
    items.extend_from_slice(rest);
    Some(h.app(head, &items))
}

/// `integrate(f, x)`: an antiderivative; `integrate(f, [x, a, b])`: the definite integral. A list `f` integrates
/// elementwise. When no method applies the call stays as it is.
pub struct Integrate {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// `failure(argument, name, message)`.
fn named_failure<'h>(h: &'h Heap, name: &str, message: &str) -> T<'h> {
    let tags = [h.sym_term(h.sym("argument")), h.sym_term(h.sym(name)), h.string(message)];
    h.call(h.sym(names::FAILURE), &tags)
}

impl Native for Integrate {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [f, spec] = args else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        // [x, a, b]: a definite integral
        let bounds = match h.view(*spec) {
            View::App { head, args: [x, lo, hi] } if b.sym_of(head) == Some(b.hd.list) => Some((*x, *lo, *hi)),
            View::App { head, .. } if b.sym_of(head) == Some(b.hd.list) => {
                return Ok(Reply::Done(named_failure(h, "integrate", "the range is [x, a, b]")))
            }
            _ => None,
        };
        let x = bounds.map_or(*spec, |(x, _, _)| x);
        let x = match variable(&b, x) {
            Ok(x) => x,
            Err(message) => return Ok(Reply::Done(named_failure(h, "integrate", message))),
        };
        let one = |cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>| match bounds {
            Some((_, lo, hi)) => crate::integrate::definite::integrate(cx, &b, f, x, lo, hi),
            None => crate::integrate::antiderivative(cx, &b, f, x, 0),
        };
        let r = match h.view(*f) {
            View::App { head, args: items } if b.sym_of(head) == Some(b.hd.list) => {
                let mut out = Vec::with_capacity(items.len());
                for &g in items {
                    match one(cx, g)? {
                        Some(r) => out.push(r),
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

/// `apart(e)`, `apart(e, x)`: partial fractions.
pub struct Apart {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

impl Native for Apart {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let (e, x) = match args {
            [e] => (*e, None),
            [e, x] => match variable(&b, *x) {
                Ok(x) => (*e, Some(x)),
                Err(message) => return Ok(Reply::Done(named_failure(h, "apart", message))),
            },
            _ => return Ok(Reply::Decline),
        };
        Ok(crate::apart::apart(cx, &b, e, x)?.map_or(Reply::Decline, Reply::Again))
    }
}
