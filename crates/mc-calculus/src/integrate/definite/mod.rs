//! Definite integrals (package 2C, item 4). OWNED BY THE DEFINITE-INTEGRATION WORK PACKAGE.
//!
//! `integrate(f, [x, a, b])` by the fundamental theorem, `F(b) - F(a)` with `F` an antiderivative of `f` from
//! [`super::antiderivative`], made safe by a discontinuity check:
//!
//! - **real bounds** (exact or numeric real constants, `inf`, `-inf`): the *critical points* of `f` and of `F` in
//!   `[a, b]` are found ([`points`]: zeros of the bases of negative and fractional powers, of the arguments of `log`,
//!   of the denominators of every argument, ...). Between consecutive critical points `F` is a continuous
//!   antiderivative, so the integral is the sum over the pieces of `F(right⁻) - F(left⁺)`; the one-sided values at
//!   critical points and at `±inf` come from a small limit helper ([`limit`]), the others by substitution. An
//!   infinite one-sided value means the integral diverges (decision 2C4.2). A jump of `F` (`atan(1/x)` at 0) is
//!   handled by the splitting, an integrable endpoint singularity (`x^(-1/2)` at 0) by the limit.
//! - **other bounds** (symbols, complex numbers): `F(b) - F(a)` only when `f` and `F` are continuous for every real
//!   `x` (polynomials, `exp`, `sin`, `cos`, `atan` of such, ...), else the call stays (decision 2C4.3).
//!
//! When something cannot be decided (a comparison of constants, the roots of a polynomial with symbolic
//! coefficients, a limit the helper does not know, a function the critical-point search does not know) the result
//! is `None` and the call stays unevaluated: no answer is better than a wrong one.
//!
//! Decisions are numbered 2C4.1, 2C4.2, ... and recorded in `notes/2c-definite.md`.
mod limit;
mod points;

use self::limit::Lim;
use super::{antiderivative, apply, args_of, is_free};
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;

/// A real bound or critical point: a finite real constant or an infinity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Pt<'h> {
    /// `-inf`
    NegInf,
    /// a finite real constant (exact or numeric)
    Fin(T<'h>),
    /// `inf`
    PosInf,
}

/// The integral of `f` in `x` from `lo` to `hi` (either may be `inf` or `-inf`), else `None`. Uses
/// [`super::antiderivative`]. A divergent integral gives `failure(divergent, integrate, f)`.
pub fn integrate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, lo: T<'h>, hi: T<'h>) -> Res<Option<T<'h>>> {
    if !is_free(b, lo, x) || !is_free(b, hi, x) {
        return Ok(None);
    }
    match (point(cx, b, lo)?, point(cx, b, hi)?) {
        (Some(p), Some(q)) => real_bounds(cx, b, f, x, p, q),
        (p, q) => {
            let Some(big) = antiderivative(cx, b, f, x, 0)? else { return Ok(None) };
            let big = cx.eval(big)?;
            if is_failure(b, big) {
                return Ok(None);
            }
            // decision 2C4.3: symbolic (or complex) bounds only for antiderivatives continuous on the whole line
            let infinite = |p: Option<Pt<'h>>| matches!(p, Some(Pt::NegInf | Pt::PosInf));
            if infinite(p) || infinite(q) {
                return Ok(None);
            }
            if !continuous(b, f, x) || !continuous(b, big, x) {
                // no critical point on the whole real line will do as well (`1/(1 + x^2)`)
                let mut zs = Vec::new();
                if !points::collect(b, f, x, &mut zs) || !points::collect(b, big, x, &mut zs) {
                    return Ok(None);
                }
                if !points::critical(cx, b, x, &zs, Pt::NegInf, Pt::PosInf)?.is_some_and(|c| c.is_empty()) {
                    return Ok(None);
                }
            }
            let a = subst(cx, b, big, x, lo)?;
            let z = subst(cx, b, big, x, hi)?;
            if is_failure(b, a) || is_failure(b, z) {
                return Ok(None);
            }
            Ok(Some(b.sub(z, a)))
        }
    }
}

/// The integral over real bounds (section comment of the module).
fn real_bounds<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    p: Pt<'h>,
    q: Pt<'h>,
) -> Res<Option<T<'h>>> {
    let (lo, hi, flip) = match cmp(cx, b, p, q)? {
        None => return Ok(None),
        Some(Ordering::Equal) => return Ok(Some(b.zero())),
        Some(Ordering::Less) => (p, q, false),
        Some(Ordering::Greater) => (q, p, true),
    };
    // a rational f with a real pole in [lo, hi] diverges, whatever the degree of the pole's factor and whether an
    // antiderivative is known (decision 2C4.4)
    if points::rational_pole(cx, b, f, x, lo, hi)? == Some(true) {
        return Ok(Some(divergent(b, f)));
    }
    let Some(big) = antiderivative(cx, b, f, x, 0)? else { return Ok(None) };
    let big = cx.eval(big)?;
    if is_failure(b, big) {
        return Ok(None);
    }
    // the critical points of f and F
    let mut zs = Vec::new();
    if !points::collect(b, f, x, &mut zs) || !points::collect(b, big, x, &mut zs) {
        return Ok(None);
    }
    let Some(crit) = points::critical(cx, b, x, &zs, lo, hi)? else { return Ok(None) };
    // the cut points with whether each is critical; a critical point equal in value to a bound marks the bound
    let mut cuts = vec![(lo, false)];
    let mut hi_crit = false;
    for &c in &crit {
        match (cmp(cx, b, c, lo)?, cmp(cx, b, c, hi)?) {
            (Some(Ordering::Equal), _) => cuts[0].1 = true,
            (_, Some(Ordering::Equal)) => hi_crit = true,
            (Some(Ordering::Greater), Some(Ordering::Less)) => cuts.push((c, true)),
            _ => return Ok(None),
        }
    }
    cuts.push((hi, hi_crit));
    let logs = points::log_args(b, big, x);
    let mut parts = Vec::with_capacity(2 * cuts.len());
    let mut unknown = false;
    for w in cuts.windows(2) {
        cx.tick(1)?;
        let ((l, l_crit), (r, r_crit)) = (w[0], w[1]);
        // on a piece, an additive log(u) with u < 0 becomes log(-u): F changes by a constant, and the value is real
        // (decision 2C4.5)
        let piece = realify(cx, b, big, x, &logs, l, r)?;
        let right = value(cx, b, piece, x, r, -1, r_crit)?;
        let left = value(cx, b, piece, x, l, 1, l_crit)?;
        for (v, sign) in [(right, 1), (left, -1)] {
            match v {
                Some(Lim::Inf(_)) => return Ok(Some(divergent(b, f))),
                Some(Lim::Fin(v, _)) => parts.push(if sign == 1 { v } else { b.neg(v) }),
                None => unknown = true,
            }
        }
    }
    if unknown {
        return Ok(None);
    }
    let total = b.add(&parts);
    Ok(Some(if flip { b.neg(total) } else { total }))
}

/// The value of `g` at `pt` approached from the side `side` (1: from above, -1: from below): substitution at a
/// finite point that is not critical, else the limit helper.
fn value<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    pt: Pt<'h>,
    side: i8,
    critical: bool,
) -> Res<Option<Lim<'h>>> {
    if let (Pt::Fin(c), false) = (pt, critical) {
        let v = subst(cx, b, g, x, c)?;
        if !is_failure(b, v) && is_free(b, v, inf(b)) {
            return Ok(Some(Lim::Fin(v, 0)));
        }
    }
    limit::limit(cx, b, g, x, pt, side)
}

/// `g` with `log(u)` replaced by `log(-u)` for the additive logarithms ([`points::log_args`]) whose arguments `u`
/// are negative on the piece `(l, r)`, which holds no zero of `u` (they are critical points). The sign is taken at
/// one inner point.
fn realify<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    logs: &[(T<'h>, T<'h>)],
    l: Pt<'h>,
    r: Pt<'h>,
) -> Res<T<'h>> {
    if logs.is_empty() {
        return Ok(g);
    }
    let m = match (l, r) {
        (Pt::Fin(a), Pt::Fin(c)) => b.mul(&[b.rat(1, 2), b.add(&[a, c])]),
        (Pt::Fin(a), _) => b.add(&[a, b.one()]),
        (_, Pt::Fin(c)) => b.sub(c, b.one()),
        _ => b.zero(),
    };
    let mut map = FxHashMap::default();
    for &(call, u) in logs {
        let v = subst(cx, b, u, x, m)?;
        if sign(cx, b, v)? == Some(-1) {
            map.insert(call, b.call("log", &[b.neg(u)]));
        }
    }
    if map.is_empty() {
        return Ok(g);
    }
    let mut memo = FxHashMap::default();
    match replace(b, g, &map, &mut memo, 0) {
        Some(t) => cx.eval(t),
        None => Ok(g),
    }
}

/// `t` with the subterms that are keys of `map` replaced (sums, products and powers rebuilt canonically); `None`
/// beyond a nesting depth.
fn replace<'h>(
    b: &Build<'h>,
    t: T<'h>,
    map: &FxHashMap<T<'h>, T<'h>>,
    memo: &mut FxHashMap<T<'h>, T<'h>>,
    depth: u32,
) -> Option<T<'h>> {
    if let Some(&r) = map.get(&t).or_else(|| memo.get(&t)) {
        return Some(r);
    }
    if depth > 200 {
        return None;
    }
    let r = match b.heap().view(t) {
        View::App { head, args } => {
            let mut out = Vec::with_capacity(args.len());
            for &a in args {
                out.push(replace(b, a, map, memo, depth + 1)?);
            }
            if out.as_slice() == args {
                t
            } else {
                b.app(head, &out)
            }
        }
        _ => t,
    };
    memo.insert(t, r);
    Some(r)
}

/// `failure(divergent, integrate, f)` (decision 2C4.2).
fn divergent<'h>(b: &Build<'h>, f: T<'h>) -> T<'h> {
    let h = b.heap();
    h.call(h.sym(names::FAILURE), &[h.sym_term(h.sym("divergent")), h.sym_term(h.sym("integrate")), f])
}

/// The symbol `inf`.
fn inf<'h>(b: &Build<'h>) -> T<'h> {
    b.heap().sym_term(b.heap().sym("inf"))
}

/// A real bound: `inf`, `-inf` or a term whose numerical value is a finite machine real.
fn point<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<Pt<'h>>> {
    let i = inf(b);
    if t == i {
        return Ok(Some(Pt::PosInf));
    }
    if t == b.neg(i) {
        return Ok(Some(Pt::NegInf));
    }
    if !is_free(b, t, i) {
        return Ok(None);
    }
    Ok(real_constant(cx, b, t)?.then_some(Pt::Fin(t)))
}

/// Is `t` a real constant: its numerical value a finite machine real?
pub(crate) fn real_constant<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<bool> {
    if let View::Int(_) | View::Rational { .. } = b.heap().view(t) {
        return Ok(true);
    }
    let v = apply(cx, b, "n", &[t])?;
    Ok(matches!(b.heap().view(v), View::Real(r) if r.is_finite()))
}

/// The sign of a constant as -1, 0 or 1, or `None` when `sign` does not decide it.
pub(crate) fn sign<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<Option<i8>> {
    if v == b.zero() {
        return Ok(Some(0));
    }
    let s = apply(cx, b, "sign", &[v])?;
    Ok(match b.heap().view(s) {
        View::Int(k @ -1..=1) => Some(k as i8),
        _ => None,
    })
}

/// The order of two points, `None` when it is not decided.
pub(crate) fn cmp<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: Pt<'h>, q: Pt<'h>) -> Res<Option<Ordering>> {
    Ok(match (p, q) {
        (Pt::Fin(a), Pt::Fin(c)) => sign(cx, b, b.sub(a, c))?.map(|s| s.cmp(&0)),
        (Pt::NegInf, Pt::NegInf) | (Pt::PosInf, Pt::PosInf) => Some(Ordering::Equal),
        (Pt::NegInf, _) | (_, Pt::PosInf) => Some(Ordering::Less),
        (Pt::PosInf, _) | (_, Pt::NegInf) => Some(Ordering::Greater),
    })
}

/// `rewrite(g, [x -> v])`, evaluated.
pub(crate) fn subst<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, v: T<'h>) -> Res<T<'h>> {
    if is_free(b, g, x) {
        return Ok(g);
    }
    let h = b.heap();
    let rule = h.call(h.sym(names::RULE), &[x, v]);
    apply(cx, b, "rewrite", &[g, h.call(b.hd.list, &[rule])])
}

/// Is `t` a `failure(...)`, or does it contain one?
pub(crate) fn is_failure<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    !is_free(b, t, h.sym_term(h.sym(names::FAILURE)))
}

/// The name of the head of a call with symbol head, and its arguments.
pub(crate) fn call_of<'h>(b: &Build<'h>, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } => b.sym_of(head).map(|s| (h.sym_name(s), args)),
        _ => None,
    }
}

/// Is `t` continuous for every real `x` (and every real value of the other symbols)? Polynomials, and `exp`,
/// `sin`, `cos`, `sinh`, `cosh`, `tanh`, `atan` of such, sums, products and non-negative integer powers.
fn continuous<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if is_free(b, u, x) || u == x {
            continue;
        }
        if let Some(args) = args_of(b, u, b.hd.add).or_else(|| args_of(b, u, b.hd.mul)) {
            stack.extend_from_slice(args);
            continue;
        }
        let Some((name, args)) = call_of(b, u) else { return false };
        match (name.as_str(), args) {
            ("pow", [base, e]) if b.sym_of(*base) == Some(b.hd.e) => stack.push(*e),
            ("pow", [base, n]) if matches!(b.heap().view(*n), View::Int(0..)) => stack.push(*base),
            ("exp" | "sin" | "cos" | "sinh" | "cosh" | "tanh" | "atan", [a]) => stack.push(*a),
            _ => return false,
        }
    }
    true
}
