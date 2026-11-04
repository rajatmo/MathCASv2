//! Integration of elementary functions by a table, substitution and parts (package 2C, item 3). OWNED BY THE
//! ELEMENTARY-INTEGRATION WORK PACKAGE.
//!
//! [`integrate`] runs after the dispatcher's linearity and base table and the rational method; it tries, in order:
//!
//! 1. [`table`]: one function of a linear argument `u = a x + c` (`exp`, the six trigonometric and three hyperbolic
//!    functions, `log`, the inverse functions, `k^u`, powers `u^n`), powers `-1`, `-1/2`, `1/2`, `-3/2` of a
//!    quadratic (atan, log and asin forms), and `exp(u) sin(v)`, `exp(u) cos(v)` (the cyclic case of parts, solved);
//! 2. [`trig`]: products of powers of `sin`, `cos`, `tan`, `cot`, `sec`, `csc` of one linear argument (odd powers by
//!    substitution, even ones by `trig_reduce` or the reduction formulas of negative powers), and products of
//!    sines and cosines of different linear arguments with polynomials (by `trig_reduce`);
//!    Products of `sinh` and `cosh` are rewritten in exponentials ([`trig::hyperbolic`]).
//! 3. [`parts::tabular`]: a polynomial times `exp`, `sin`, `cos`, `sinh`, `cosh`, `k^u` (u linear) or
//!    `exp(u) sin(v)`, by the tabular form of integration by parts (no recursion);
//! 4. [`subst`]: derivative-divides, `f = g(u) u'` for a subterm `u` of `f`;
//! 5. [`parts::by_parts`]: integration by parts with the LIATE choice of the factor to differentiate (a logarithm
//!    or an inverse function, else a polynomial);
//! 6. a product with a sum factor (or a power of a sum) is expanded and integrated term by term.
//!
//! Results avoid `sec`, `csc`, `cot`, `asinh`, `acosh`, `atanh` where a form with a derivative rule exists
//! (`log((1 + sin(u))/cos(u))` for `∫ sec`, `log(u + sqrt(u^2 + 1))` for `∫ (u^2 + 1)^(-1/2)`), and put no `abs`
//! in logarithms (like the base table's `log(x)`).
//!
//! Work is bounded: besides the dispatcher's depth, each top-level call has a budget of attempts (a thread-local
//! counter), so a search that fails (e.g. `exp(x^2)`) stops early.
pub mod parts;
pub mod subst;
pub mod table;
pub mod trig;

use super::{apply, args_of, is_free};
use crate::diff::{Diff, Halt};
use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;
use std::cell::Cell;

/// The attempts one top-level call of [`integrate`] may make (entries and substitution candidates).
const BUDGET: i64 = 4000;

thread_local! {
    static LEFT: Cell<i64> = const { Cell::new(BUDGET) };
}

/// Spend `n` attempts of the budget; false once it is exhausted.
pub(crate) fn spend(n: i64) -> bool {
    LEFT.with(|c| {
        let v = c.get() - n;
        c.set(v);
        v >= 0
    })
}

/// An antiderivative of `f` by the elementary methods, else `None`. May call
/// [`super::antiderivative`] with `depth + 1`.
pub fn integrate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    if depth == 0 {
        // a fresh budget for a top-level call; the outer one (of a nested evaluation) is restored after
        let saved = LEFT.with(|c| c.replace(BUDGET));
        let r = run(cx, b, f, x, depth);
        LEFT.with(|c| c.set(saved));
        return r;
    }
    run(cx, b, f, x, depth)
}

fn run<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    if !spend(1) {
        return Ok(None);
    }
    cx.tick(1)?;
    let f = normalize(b, f);
    if let Some(r) = table::single(cx, b, f, x)? {
        return Ok(Some(r));
    }
    if let Some(r) = trig::integrate(cx, b, f, x, depth)? {
        return Ok(Some(r));
    }
    if let Some(r) = trig::hyperbolic(cx, b, f, x, depth)? {
        return Ok(Some(r));
    }
    if let Some(r) = parts::tabular(cx, b, f, x)? {
        return Ok(Some(r));
    }
    if let Some(r) = subst::integrate(cx, b, f, x, depth, false)? {
        return Ok(Some(r));
    }
    if let Some(r) = parts::by_parts(cx, b, f, x, depth)? {
        return Ok(Some(r));
    }
    // u = log(x), u = exp(v) change the shape of the integrand: after parts (x^3 log(x) is better by parts)
    if let Some(r) = subst::integrate(cx, b, f, x, depth, true)? {
        return Ok(Some(r));
    }
    expanded(cx, b, f, x, depth)
}

/// Exponentials in one form: `e^u` and `exp(u)^k` are `exp(u)` and `exp(k u)`, and the exponentials among the
/// factors of a product are merged (`exp(u) exp(v)` is `exp(u + v)`).
pub(crate) fn normalize<'h>(b: &Build<'h>, f: T<'h>) -> T<'h> {
    let e = b.sym(b.hd.e);
    let f = rewrite(b, f, &mut |t| match args_of(b, t, b.hd.pow) {
        Some([base, u]) if *base == e => Some(b.call("exp", &[*u])),
        Some([base, k]) => match unary(b, *base) {
            Some((n, u)) if n == "exp" => Some(b.call("exp", &[b.mul(&[*k, u])])),
            _ => None,
        },
        _ => None,
    });
    let merged = |fs: &[T<'h>]| -> Option<T<'h>> {
        let (exps, rest): (Vec<T<'h>>, Vec<T<'h>>) =
            fs.iter().partition(|&&g| unary(b, g).is_some_and(|(n, _)| n == "exp"));
        if exps.len() < 2 {
            return None;
        }
        let sum: Vec<T<'h>> = exps.iter().filter_map(|&g| unary(b, g).map(|(_, u)| u)).collect();
        Some(b.mul(&[b.mul(&rest), b.call("exp", &[b.add(&sum)])]))
    };
    rewrite(b, f, &mut |t| args_of(b, t, b.hd.mul).and_then(merged))
}

/// A product with a sum factor, or a power of a sum, expanded and integrated term by term.
fn expanded<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let is_sum = |t: T<'h>| args_of(b, t, b.hd.add).is_some();
    let worth = factors(b, f).iter().any(|&g| {
        let (base, k) = pow_parts(b, g);
        is_sum(base) && !is_free(b, base, x) && int_of(b, k).is_some_and(|k| k >= 1)
    }) && (args_of(b, f, b.hd.mul).is_some() || args_of(b, f, b.hd.pow).is_some());
    if !worth {
        return Ok(None);
    }
    let g = apply(cx, b, "expand", &[f])?;
    if g == f || !is_sum(g) {
        return Ok(None);
    }
    super::antiderivative(cx, b, g, x, depth + 1)
}

// ---------------------------------------------------------------------------------------------------------------
// helpers shared by the methods

/// The name and arguments of a call with a symbol head.
pub(crate) fn fun<'h>(b: &Build<'h>, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    let View::App { head, args } = b.heap().view(t) else { return None };
    let s = b.sym_of(head)?;
    Some((b.heap().sym_name(s), args))
}

/// The name and argument of a call of one argument.
pub(crate) fn unary<'h>(b: &Build<'h>, t: T<'h>) -> Option<(String, T<'h>)> {
    match fun(b, t)? {
        (n, [u]) => Some((n, *u)),
        _ => None,
    }
}

/// `(base, exponent)` of a power; `(t, 1)` for any other term.
pub(crate) fn pow_parts<'h>(b: &Build<'h>, t: T<'h>) -> (T<'h>, T<'h>) {
    match args_of(b, t, b.hd.pow) {
        Some([base, e]) => (*base, *e),
        _ => (t, b.one()),
    }
}

/// The factors of a product; `[t]` for any other term.
pub(crate) fn factors<'h>(b: &Build<'h>, t: T<'h>) -> Vec<T<'h>> {
    match args_of(b, t, b.hd.mul) {
        Some(fs) => fs.to_vec(),
        None => vec![t],
    }
}

/// A machine integer's value.
pub(crate) fn int_of(b: &Build<'_>, t: T<'_>) -> Option<i64> {
    match b.heap().view(t) {
        View::Int(v) => Some(v),
        _ => None,
    }
}

/// The sign of an exact or approximate number.
fn num_sign(b: &Build<'_>, t: T<'_>) -> Option<i8> {
    let h = b.heap();
    match h.view(t) {
        View::Int(v) => Some(v.signum() as i8),
        View::BigInt { neg, .. } => Some(if neg { -1 } else { 1 }),
        View::Rational { num, .. } => num_sign(b, num),
        View::Real(v) if v.is_finite() => Some(if v > 0.0 {
            1
        } else if v < 0.0 {
            -1
        } else {
            0
        }),
        View::Ball { neg, .. } => Some(if neg { -1 } else { 1 }),
        _ => None,
    }
}

/// The sign of a constant (free of the variable): a number's, numerically for a constant expression, else the
/// sign that holds for all real nonzero values of its parameters (`4 a^2` is positive, `-a^2 b^4` negative), or
/// `None`.
pub(crate) fn sign<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<i8>> {
    if let Some(s) = num_sign(b, t) {
        return Ok(Some(s));
    }
    let v = apply(cx, b, "n", &[t])?;
    if let Some(s) = num_sign(b, v) {
        return Ok(Some(s));
    }
    Ok(struct_sign(b, t))
}

/// The sign of `t` for all real nonzero parameters, from its structure: numbers, even powers, products, and sums
/// of terms of one sign.
fn struct_sign(b: &Build<'_>, t: T<'_>) -> Option<i8> {
    if let Some(s) = num_sign(b, t) {
        return Some(s);
    }
    if let Some([_, k]) = args_of(b, t, b.hd.pow) {
        return match b.heap().view(*k) {
            View::Int(k) if k % 2 == 0 => Some(1),
            View::Rational { num, .. } if int_of(b, num).is_some_and(|n| n % 2 == 0) => Some(1),
            _ => None,
        };
    }
    if let Some(fs) = args_of(b, t, b.hd.mul) {
        let mut s = 1;
        for &f in fs {
            s *= struct_sign(b, f)?;
        }
        return Some(s);
    }
    if let Some(ts) = args_of(b, t, b.hd.add) {
        let first = struct_sign(b, *ts.first()?)?;
        return ts[1..].iter().all(|&u| struct_sign(b, u) == Some(first)).then_some(first);
    }
    None
}

/// `d(t, x)`, `None` when some part of `t` cannot be differentiated.
pub(crate) fn deriv<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let mut tick = |n: u64| cx.tick(n);
    match Diff::new(b, x).of(t, &mut tick) {
        Ok(d) => Ok(Some(d)),
        Err(Halt::Abort(a)) => Err(a),
        Err(Halt::Unsupported) => Ok(None),
    }
}

/// The slope `a` of a linear argument `u = a x + c` (a free of x, not 0), else `None`.
pub(crate) fn slope<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    if u == x {
        return Ok(Some(b.one()));
    }
    if is_free(b, u, x) {
        return Ok(None);
    }
    let Some(a) = deriv(cx, b, u, x)? else { return Ok(None) };
    if b.is_zero(a) || !is_free(b, a, x) || num_sign(b, a) == Some(0) {
        return Ok(None);
    }
    Ok(Some(a))
}

/// `t` with each subterm `s` for which `rule(s)` is `Some(r)` replaced by `r` (outermost first), rebuilt
/// canonically. An explicit stack over the DAG, each distinct subterm visited once.
pub(crate) fn rewrite<'h>(b: &Build<'h>, t: T<'h>, rule: &mut dyn FnMut(T<'h>) -> Option<T<'h>>) -> T<'h> {
    let h = b.heap();
    let mut memo: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    while let Some((u, ready)) = stack.pop() {
        if memo.contains_key(&u) {
            continue;
        }
        if !ready {
            if let Some(r) = rule(u) {
                memo.insert(u, r);
                continue;
            }
        }
        let View::App { head, args } = h.view(u) else {
            memo.insert(u, u);
            continue;
        };
        if !ready {
            stack.push((u, true));
            for &a in args.iter().rev().chain(std::iter::once(&head)) {
                if !memo.contains_key(&a) {
                    stack.push((a, false));
                }
            }
            continue;
        }
        let nh = memo[&head];
        let na: Vec<T<'h>> = args.iter().map(|a| memo[a]).collect();
        let same = nh == head && na.iter().zip(args.iter()).all(|(p, q)| p == q);
        memo.insert(u, if same { u } else { b.app(nh, &na) });
    }
    memo[&t]
}

/// `t` with every occurrence of `from` replaced by `to`.
pub(crate) fn subst<'h>(b: &Build<'h>, t: T<'h>, from: T<'h>, to: T<'h>) -> T<'h> {
    rewrite(b, t, &mut |s| (s == from).then_some(to))
}

/// The number of distinct subterms of `t`; counting stops at `cap`.
pub(crate) fn size<'h>(b: &Build<'h>, t: T<'h>, cap: usize) -> usize {
    let h = b.heap();
    let mut seen: FxHashMap<T<'h>, ()> = FxHashMap::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if seen.len() >= cap {
            break;
        }
        if seen.insert(u, ()).is_some() {
            continue;
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend_from_slice(args);
        }
    }
    seen.len()
}

/// Is `t` a polynomial in `x` (sums, products and non-negative integer powers of `x` and of terms free of it)?
pub(crate) fn is_poly<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x || is_free(b, u, x) {
            continue;
        }
        if let Some(ts) = args_of(b, u, b.hd.add).or_else(|| args_of(b, u, b.hd.mul)) {
            stack.extend_from_slice(ts);
            continue;
        }
        match args_of(b, u, b.hd.pow) {
            Some([base, k]) if int_of(b, *k).is_some_and(|k| k >= 0) => stack.push(*base),
            _ => return false,
        }
    }
    true
}

/// A symbol for the integration variable of a sub-problem, distinct per tag and depth (never in a result).
pub(crate) fn fresh<'h>(b: &Build<'h>, tag: &str, depth: u32) -> T<'h> {
    b.sym(b.heap().sym(&format!("integrate`{tag}{depth}")))
}
