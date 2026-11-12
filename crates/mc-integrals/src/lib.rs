//! Integration (roadmap item 2C): the dispatcher of `integrate` and the base rules every method relies on.
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
//! Definite integrals are [`definite`]'s; partial fractions (`apart`) are `mc-calculus`'s.
//!
//! Results are built with [`Build`] (canonical sums, products and powers; plain calls) and handed back to the
//! evaluator, which evaluates the calls in them. No constant of integration is added.
//!
//! A crate of its own since round 34 (split out of `mc-calculus`, with the native of `integrate`); limits come
//! from `mc-limit`, derivatives and the shared helpers from `mc-calculus`.
#![warn(missing_docs)]
pub mod definite;
pub mod elementary;
pub mod portfolio;
mod natives;
pub mod rational;

use mc_calculus::common::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The deepest recursion of [`antiderivative`] (substitutions and parts call it again).
pub const MAX_DEPTH: u32 = 12;

/// An antiderivative of `f` with respect to the symbol term `x`, or `None` when no method finds one.
///
/// At depth 0 (a call from outside: the native, definite integration) the portfolio of [`portfolio`] runs and
/// checks its answer by differentiation; inside a method (depth 1 and more) the classic dispatcher runs.
pub fn antiderivative<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    if depth == 0 {
        return portfolio::integrate(cx, b, f, x);
    }
    classic(cx, b, f, x, depth)
}

/// The classic dispatcher: linearity, the base table, the rational integrator, the elementary rules.
pub(crate) fn classic<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
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
            match classic(cx, b, t, x, depth)? {
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
            return Ok(classic(cx, b, rest, x, depth)?.map(|r| b.mul(&[b.mul(&free), r])));
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
    // the derivative of an unknown function: f^(k)(x) integrates to f^(k-1)(x)
    if let ([a], Some((orders, g))) = (args, b.as_derivative(head)) {
        if *a == x {
            if let [k] = orders.as_slice() {
                if *k >= 1 {
                    return Some(if *k == 1 { b.app(g, &[x]) } else { b.app(b.derivative(&[*k - 1], g), &[x]) });
                }
            }
        }
    }
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

/// Does `t` hold a power with an exponent whose numerator is far beyond any polynomial the methods could
/// expand (`x^(2^31)`)? Such integrands only reach the table: the other methods would build a dense
/// polynomial of that degree (a 240 GB allocation, X23-9) or loop over it (X23-10).
pub fn has_huge_power<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    const LIMIT: i64 = 100_000;
    let h = b.heap();
    let mut stack = vec![t];
    let mut seen = 0usize;
    while let Some(u) = stack.pop() {
        seen += 1;
        if seen > 50_000 {
            return false;
        }
        if let View::App { head, args } = h.view(u) {
            if args.len() == 2 && b.sym_of(head) == Some(b.hd.pow) {
                let big = |k: T<'h>| match h.view(k) {
                    View::Int(k) => k.unsigned_abs() > LIMIT as u64,
                    View::BigInt { .. } => true,
                    _ => false,
                };
                let huge = match h.view(args[1]) {
                    View::Rational { num, .. } => big(num),
                    _ => big(args[1]),
                };
                if huge {
                    return true;
                }
            }
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut mc_eval::Kernel, h: &mc_core::Heap, stop: &std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use mc_eval::Attributes;
    // integrate reads the assumption context: symbolic bounds and parameters decided by its facts (decision 2C4.5)
    let reads = Attributes { uses_assumptions: true, ..Attributes::default() };
    k.register(h.sym("integrate"), reads, mc_calculus::common::wrap("integrate", natives::Integrate { stop: stop.clone() }));
    k.register(h.sym("risch_integrate"), Attributes::default(), Box::new(portfolio::RischIntegrate { stop: stop.clone() }));
}
