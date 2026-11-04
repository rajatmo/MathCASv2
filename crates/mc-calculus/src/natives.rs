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
fn variable<'h>(b: &Build<'h>, t: T<'h>) -> Result<T<'h>, &'static str> {
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
