//! Integration (roadmap package 2C): the dispatcher of `integrate` and the base rules every method relies on.
//!
//! [`antiderivative`] tries, in order:
//!
//! 1. **linearity**: a sum term by term (all terms must succeed), constant factors (free of `x`) pulled out;
//! 2. **base table**: constants, `x^n` (n free of x; `x^-1` gives `log(x)`), `exp(x)`, `sin(x)`, `cos(x)`;
//! 3. **rational functions** of `x` ([`rational`]);
//! 4. **elementary functions** ([`elementary`]): a table, substitution, integration by parts.
//!
//! Every method returns `Ok(None)` when it does not apply or fails; the call `integrate(f, x)` then stays as it is.
//! Methods may call [`antiderivative`] recursively with `depth + 1`; it gives up beyond [`MAX_DEPTH`].
//! Definite integrals are [`definite`]'s; partial fractions (`apart`) are [`crate::apart`]'s.
//!
//! Results are built with [`Build`] (canonical sums, products and powers; plain calls) and handed back to the
//! evaluator, which evaluates the calls in them. No constant of integration is added.
pub mod definite;
pub mod elementary;
pub mod rational;

use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;

/// The deepest recursion of [`antiderivative`] (substitutions and parts call it again).
pub const MAX_DEPTH: u32 = 12;

/// An antiderivative of `f` with respect to the symbol term `x`, or `None` when no method finds one.
pub fn antiderivative<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    if is_free(b, f, x) {
        return Ok(Some(b.mul(&[f, x])));
    }
    // linearity: sums term by term
    if let Some(terms) = args_of(b, f, b.hd.add) {
        let mut out = Vec::with_capacity(terms.len());
        for &t in terms {
            match antiderivative(cx, b, t, x, depth)? {
                Some(r) => out.push(r),
                None => return Ok(None),
            }
        }
        return Ok(Some(b.add(&out)));
    }
    // linearity: constant factors
    if let Some(fs) = args_of(b, f, b.hd.mul) {
        let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = fs.iter().partition(|&&g| is_free(b, g, x));
        if !free.is_empty() {
            let rest = b.mul(&dep);
            return Ok(antiderivative(cx, b, rest, x, depth)?.map(|r| b.mul(&[b.mul(&free), r])));
        }
    }
    if let Some(r) = base(b, f, x) {
        return Ok(Some(r));
    }
    if let Some(r) = rational::integrate(cx, b, f, x, depth)? {
        return Ok(Some(r));
    }
    elementary::integrate(cx, b, f, x, depth)
}

/// The base table: `x^n` (n free of x), `x^-1`, `exp(x)`, `sin(x)`, `cos(x)`, with the argument exactly `x`.
fn base<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> Option<T<'h>> {
    if f == x {
        return Some(b.mul(&[b.rat(1, 2), b.powi(x, 2)]));
    }
    let h = b.heap();
    let View::App { head, args } = h.view(f) else { return None };
    let name = b.sym_of(head).map(|s| h.sym_name(s))?;
    match (name.as_str(), args) {
        ("pow", [base, n]) if *base == x && is_free(b, *n, x) => {
            if *n == b.int(-1) {
                Some(b.call("log", &[x]))
            } else {
                let n1 = b.add(&[*n, b.one()]);
                Some(b.div(b.pow(x, n1), n1))
            }
        }
        ("exp", [u]) if *u == x => Some(f),
        ("sin", [u]) if *u == x => Some(b.neg(b.call("cos", &[x]))),
        ("cos", [u]) if *u == x => Some(b.call("sin", &[x])),
        _ => None,
    }
}

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

/// `name(args...)` evaluated through the evaluator (a function of another package: `together`, `factor`,
/// `expand`, `simplify`, `series`, ...).
pub fn apply<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
    cx.eval(b.call(name, args))
}
