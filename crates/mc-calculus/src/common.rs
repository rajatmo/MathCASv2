//! Helpers every calculus crate uses (round 34: moved here from the integration and differentiation modules when
//! `mc-calculus` was split into `mc-calculus`, `mc-limit`, `mc-integrals`, `mc-transforms` and `mc-dsolve`): free
//! occurrence, calls by name, evaluation through the kernel, failures, the variable of `d`, and the wrapper that
//! gives a native the `assumptions:` option.
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;

/// The arguments of `t` when it is a call of the symbol `s`.
pub fn args_of<'h>(b: &Build<'h>, t: T<'h>, s: mc_api::Sym) -> Option<&'h [T<'h>]> {
    match b.heap().view(t) {
        View::App { head, args } if b.sym_of(head) == Some(s) => Some(args),
        _ => None,
    }
}

/// Is `t` free of `x` (no subterm equal to `x`)? A walk over the DAG with a memo.
pub fn is_free<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut seen: FxHashMap<T<'h>, ()> = FxHashMap::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return false;
        }
        if seen.insert(u, ()).is_some() {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    true
}

/// `name(args...)` evaluated through the evaluator (a function of another crate: `together`, `factor`,
/// `expand`, `simplify`, `series`, ...).
pub fn apply<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
    cx.eval(b.call(name, args))
}

/// Is `t` a `failure(...)`, or does it contain one?
pub fn is_failure<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    !is_free(b, t, h.sym_term(h.sym(names::FAILURE)))
}

/// The name of the head of a call with symbol head, and its arguments.
pub fn call_of<'h>(b: &Build<'h>, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } => b.sym_of(head).map(|s| (h.sym_name(s), args)),
        _ => None,
    }
}


/// Is `t` something `d` can differentiate in: not a number, string, list, sum, product or power?
pub fn variable<'h>(b: &Build<'h>, t: T<'h>) -> Result<T<'h>, &'static str> {
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


/// `failure(argument, name, message)`.
pub fn named_failure<'h>(h: &'h Heap, name: &str, message: &str) -> T<'h> {
    let tags = [h.sym_term(h.sym("argument")), h.sym_term(h.sym(name)), h.string(message)];
    h.call(h.sym(names::FAILURE), &tags)
}


/// A native with the `assumptions:` option (decision IOSYM.4).
pub fn wrap<N: mc_eval::Native + 'static>(name: &'static str, inner: N) -> Box<dyn mc_eval::Native> {
    Box::new(mc_poly::opts::WithAssumptions { name, inner })
}
