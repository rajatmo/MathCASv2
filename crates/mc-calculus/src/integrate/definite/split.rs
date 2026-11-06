//! Definite integrals of functions defined by cases (package io-sym, decision IOSYM.2): `abs`, `sign`,
//! `unit_step`, `max`, `min`, `if` and `piecewise` of the variable.
//!
//! The interval is cut at the real zeros of the switching expressions (the arguments of `abs`, `sign` and
//! `unit_step`, the differences of the arguments of `max` and `min`, the two sides of every comparison in a
//! condition), found by `solve(g == 0, x, reals)`. On each piece every switch is replaced by its branch at one inner
//! point, and the plain integrand is integrated by [`super::integrate`]; the result is the sum. Anything not decided
//! (a zero set that is not a finite list of real constants, a condition or sign not decided at the inner point, a
//! piece without an integral) leaves the call unevaluated.
use super::{cmp, integrate, is_failure, point, sign, subst, Pt};
use crate::diff::piecewise_parts;
use crate::integrate::{apply, is_free};
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;

/// The most cut points tried (beyond that the call stays).
const MAX_CUTS: usize = 64;

/// The kinds of switching calls.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Switch {
    Abs,
    Sign,
    UnitStep,
    Max,
    Min,
    If,
    Piecewise,
}

fn switch_of(name: &str, arity: usize) -> Option<Switch> {
    Some(match (name, arity) {
        ("abs", 1) => Switch::Abs,
        ("sign", 1) => Switch::Sign,
        ("unit_step", 1) => Switch::UnitStep,
        ("max", 2..) => Switch::Max,
        ("min", 2..) => Switch::Min,
        ("if", 3) => Switch::If,
        ("piecewise", 1 | 2) => Switch::Piecewise,
        _ => return None,
    })
}

/// The switch of a call with a symbol head.
fn switch<'h>(b: &Build<'h>, t: T<'h>) -> Option<(Switch, &'h [T<'h>])> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return None };
    let s = b.sym_of(head)?;
    switch_of(&h.sym_name(s), args.len()).map(|k| (k, args))
}

/// The integral of `f` in `x` over `[lo, hi]` by cases, or `None` (module comment).
pub(crate) fn by_cases<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, lo: T<'h>, hi: T<'h>) -> Res<Option<T<'h>>> {
    let switches = switches_of(b, f, x);
    if switches.is_empty() {
        return Ok(None);
    }
    let (Some(p), Some(q)) = (point(cx, b, lo)?, point(cx, b, hi)?) else { return Ok(None) };
    let (p, q, flip) = match cmp(cx, b, p, q)? {
        None => return Ok(None),
        Some(Ordering::Equal) => return Ok(Some(b.zero())),
        Some(Ordering::Less) => (p, q, false),
        Some(Ordering::Greater) => (q, p, true),
    };
    let Some(gs) = zero_exprs(b, &switches) else { return Ok(None) };
    let Some(cuts) = cut_points(cx, b, &gs, x, p, q)? else { return Ok(None) };
    let mut ends = vec![p];
    ends.extend(cuts);
    ends.push(q);
    let mut parts = Vec::with_capacity(ends.len());
    for w in ends.windows(2) {
        cx.tick(1)?;
        let m = match (w[0], w[1]) {
            (Pt::Fin(a), Pt::Fin(c)) => b.mul(&[b.rat(1, 2), b.add(&[a, c])]),
            (Pt::Fin(a), _) => b.add(&[a, b.one()]),
            (_, Pt::Fin(c)) => b.sub(c, b.one()),
            _ => b.zero(),
        };
        let m = cx.eval(m)?;
        let Some(piece) = resolve(cx, b, f, x, m)? else { return Ok(None) };
        let piece = cx.eval(piece)?;
        let Some(v) = integrate(cx, b, piece, x, term(b, w[0]), term(b, w[1]))? else { return Ok(None) };
        if is_failure(b, v) {
            return Ok(Some(v));
        }
        parts.push(v);
    }
    let total = b.add(&parts);
    Ok(Some(if flip { b.neg(total) } else { total }))
}

/// The switching calls of `f` that depend on `x`.
fn switches_of<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> Vec<T<'h>> {
    let mut switches = vec![];
    let mut stack = vec![f];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) || is_free(b, u, x) {
            continue;
        }
        if let View::App { args, .. } = b.heap().view(u) {
            if switch(b, u).is_some() {
                switches.push(u);
            }
            stack.extend(args.iter().copied());
        }
    }
    switches
}

/// The expressions whose zeros are the cut points of the switches; `None` for a condition of another shape.
fn zero_exprs<'h>(b: &Build<'h>, switches: &[T<'h>]) -> Option<Vec<T<'h>>> {
    let mut gs = vec![];
    for &s in switches {
        let Some((kind, args)) = switch(b, s) else { continue };
        match kind {
            Switch::Abs | Switch::Sign | Switch::UnitStep => gs.push(args[0]),
            Switch::Max | Switch::Min => {
                for i in 0..args.len() {
                    for j in i + 1..args.len() {
                        gs.push(b.sub(args[i], args[j]));
                    }
                }
            }
            Switch::If => {
                if !comparisons(b, args[0], &mut gs) {
                    return None;
                }
            }
            Switch::Piecewise => {
                let (pieces, _) = piecewise_parts(b, args)?;
                for (_, c) in pieces {
                    if !comparisons(b, c, &mut gs) {
                        return None;
                    }
                }
            }
        }
    }
    Some(gs)
}

/// The zeros of `gs` strictly inside `(p, q)` (an ordered pair of points), in order; `None` when not all are found.
fn cut_points<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, gs: &[T<'h>], x: T<'h>, p: Pt<'h>, q: Pt<'h>) -> Res<Option<Vec<Pt<'h>>>> {
    let mut cuts: Vec<Pt<'h>> = vec![];
    let h = b.heap();
    for &g in gs {
        if is_free(b, g, x) {
            continue;
        }
        cx.tick(1)?;
        let mut eq = h.call(h.sym("equal"), &[g, b.zero()]);
        if !rational_in(b, g, x) {
            // not a rational function: solve finds every zero only on a bounded interval (its numeric search)
            let (Pt::Fin(l), Pt::Fin(r)) = (p, q) else { return Ok(None) };
            let inside = [h.call(h.sym("less"), &[l, x]), h.call(h.sym("less"), &[x, r])];
            eq = h.call(h.sym("and"), &[eq, inside[0], inside[1]]);
        }
        let sols = apply(cx, b, "solve", &[eq, x, h.sym_term(h.sym("reals"))])?;
        let View::App { head, args: sols } = h.view(sols) else { return Ok(None) };
        if b.sym_of(head) != Some(b.hd.list) {
            return Ok(None);
        }
        for &sol in sols {
            let v = match h.view(sol) {
                View::App { head, args: &[r] } if b.sym_of(head) == Some(b.hd.list) => match h.view(r) {
                    View::App { args: &[k, v], .. } if k == x => v,
                    _ => return Ok(None),
                },
                _ => return Ok(None),
            };
            let Some(c @ Pt::Fin(_)) = point(cx, b, v)? else { return Ok(None) };
            if cmp(cx, b, c, p)? != Some(Ordering::Greater) || cmp(cx, b, c, q)? != Some(Ordering::Less) {
                continue;
            }
            // insert in order, once
            let mut at = cuts.len();
            for (i, &d) in cuts.iter().enumerate() {
                match cmp(cx, b, c, d)? {
                    Some(Ordering::Equal) => {
                        at = usize::MAX;
                        break;
                    }
                    Some(Ordering::Less) => {
                        at = i;
                        break;
                    }
                    Some(Ordering::Greater) => {}
                    None => return Ok(None),
                }
            }
            if at != usize::MAX {
                cuts.insert(at, c);
            }
            if cuts.len() > MAX_CUTS {
                return Ok(None);
            }
        }
    }
    Ok(Some(cuts))
}

/// Is `t` a rational function of `x` (sums, products, integer powers; other subterms free of `x`)?
fn rational_in<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x || is_free(b, u, x) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { return false };
        match b.sym_of(head) {
            Some(s) if s == b.hd.add || s == b.hd.mul => stack.extend(args.iter().copied()),
            Some(s) if s == b.hd.pow && matches!(args, [_, e] if matches!(h.view(*e), View::Int(_))) => stack.push(args[0]),
            _ => return false,
        }
    }
    true
}

/// A point as a term.
fn term<'h>(b: &Build<'h>, p: Pt<'h>) -> T<'h> {
    let inf = b.heap().sym_term(b.heap().sym("inf"));
    match p {
        Pt::NegInf => b.neg(inf),
        Pt::Fin(t) => t,
        Pt::PosInf => inf,
    }
}

/// The differences `l - r` of the comparisons in a condition (`and`, `or`, `not` of comparisons); false for another
/// shape.
fn comparisons<'h>(b: &Build<'h>, c: T<'h>, out: &mut Vec<T<'h>>) -> bool {
    let h = b.heap();
    let mut stack = vec![c];
    while let Some(u) = stack.pop() {
        if u == h.sym_term(h.sym(names::TRUE)) || u == h.sym_term(h.sym(names::FALSE)) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { return false };
        let Some(s) = b.sym_of(head) else { return false };
        match (h.sym_name(s).as_str(), args) {
            ("and" | "or" | "not", _) => stack.extend(args.iter().copied()),
            ("less" | "greater" | "less_equal" | "greater_equal" | "equal" | "unequal", [l, r]) => out.push(b.sub(*l, *r)),
            _ => return false,
        }
    }
    true
}

/// Is `c` true at `x = m`? `None` when not decided.
fn truth<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, x: T<'h>, m: T<'h>) -> Res<Option<bool>> {
    let h = b.heap();
    let (t, f) = (h.sym_term(h.sym(names::TRUE)), h.sym_term(h.sym(names::FALSE)));
    let mut v = subst(cx, b, c, x, m)?;
    if v != t && v != f {
        v = apply(cx, b, "n", &[v])?;
    }
    Ok(if v == t {
        Some(true)
    } else if v == f {
        Some(false)
    } else {
        None
    })
}

/// `f` with every switch of `x` replaced by its branch at `x = m` (bottom-up on an explicit stack); `None` when a
/// branch is not decided.
fn resolve<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, m: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut memo: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(f, false)];
    while let Some((u, ready)) = stack.pop() {
        if memo.contains_key(&u) {
            continue;
        }
        if is_free(b, u, x) {
            memo.insert(u, u);
            continue;
        }
        let View::App { head, args } = h.view(u) else {
            memo.insert(u, u);
            continue;
        };
        let kind = switch(b, u).map(|s| s.0);
        // the conditions of `if` and `piecewise` are decided on the original terms, not rebuilt
        let children: Vec<T<'h>> = match kind {
            Some(Switch::If) => args[1..].to_vec(),
            Some(Switch::Piecewise) => match piecewise_parts(b, args) {
                Some((pieces, d)) => pieces.iter().map(|p| p.0).chain(d).collect(),
                None => return Ok(None),
            },
            _ => args.to_vec(),
        };
        if !ready {
            stack.push((u, true));
            stack.extend(children.iter().filter(|c| !memo.contains_key(c)).map(|&c| (c, false)));
            continue;
        }
        let na: Vec<T<'h>> = args.iter().map(|a| memo.get(a).copied().unwrap_or(*a)).collect();
        let r = match kind {
            None => {
                if na.as_slice() == args {
                    u
                } else {
                    b.app(head, &na)
                }
            }
            Some(Switch::Abs | Switch::Sign | Switch::UnitStep) => {
                let v = subst(cx, b, na[0], x, m)?;
                let Some(s) = sign(cx, b, v)? else { return Ok(None) };
                match kind {
                    Some(Switch::Abs) => {
                        if s < 0 {
                            b.neg(na[0])
                        } else {
                            na[0]
                        }
                    }
                    Some(Switch::Sign) => b.int(s as i64),
                    _ => b.int(i64::from(s >= 0)),
                }
            }
            Some(k @ (Switch::Max | Switch::Min)) => {
                let mut best = na[0];
                for &a in &na[1..] {
                    let d = subst(cx, b, b.sub(a, best), x, m)?;
                    let Some(s) = sign(cx, b, d)? else { return Ok(None) };
                    if (k == Switch::Max && s > 0) || (k == Switch::Min && s < 0) {
                        best = a;
                    }
                }
                best
            }
            Some(Switch::If) => match truth(cx, b, args[0], x, m)? {
                Some(true) => memo[&args[1]],
                Some(false) => memo[&args[2]],
                None => return Ok(None),
            },
            Some(Switch::Piecewise) => {
                let Some((pieces, d)) = piecewise_parts(b, args) else { return Ok(None) };
                let mut chosen = None;
                for (v, c) in pieces {
                    match truth(cx, b, c, x, m)? {
                        Some(true) => {
                            chosen = Some(memo[&v]);
                            break;
                        }
                        Some(false) => {}
                        None => return Ok(None),
                    }
                }
                chosen.unwrap_or_else(|| d.map_or(b.zero(), |d| memo[&d]))
            }
        };
        memo.insert(u, r);
    }
    Ok(Some(memo[&f]))
}

/// `f` with its switches of `x` replaced by their branches just beside `pt` (from above for `side` 1, from below
/// for -1; an infinity from its finite side), for one-sided limits; `None` when `f` has no switches or the branches
/// there are not decided.
pub(crate) fn resolve_near<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, pt: Pt<'h>, side: i8) -> Res<Option<T<'h>>> {
    let switches = switches_of(b, f, x);
    if switches.is_empty() {
        return Ok(None);
    }
    let Some(gs) = zero_exprs(b, &switches) else { return Ok(None) };
    let m = match pt {
        Pt::Fin(c) => {
            let (p, q) = if side > 0 { (c, b.add(&[c, b.one()])) } else { (b.sub(c, b.one()), c) };
            let (p, q) = (cx.eval(p)?, cx.eval(q)?);
            let Some(cuts) = cut_points(cx, b, &gs, x, Pt::Fin(p), Pt::Fin(q))? else { return Ok(None) };
            let near = if side > 0 { cuts.first() } else { cuts.last() };
            let other = match near {
                Some(Pt::Fin(d)) => *d,
                Some(_) => return Ok(None),
                None if side > 0 => q,
                None => p,
            };
            b.mul(&[b.rat(1, 2), b.add(&[c, other])])
        }
        Pt::PosInf | Pt::NegInf => {
            // every zero of rational expressions; the sample beyond them all
            if gs.iter().any(|&g| !rational_in(b, g, x)) {
                return Ok(None);
            }
            let Some(cuts) = cut_points(cx, b, &gs, x, Pt::NegInf, Pt::PosInf)? else { return Ok(None) };
            match (pt, cuts.first(), cuts.last()) {
                (Pt::PosInf, _, Some(Pt::Fin(d))) => b.add(&[*d, b.one()]),
                (Pt::NegInf, Some(Pt::Fin(d)), _) => b.sub(*d, b.one()),
                (_, None, None) => b.zero(),
                _ => return Ok(None),
            }
        }
    };
    let m = cx.eval(m)?;
    match resolve(cx, b, f, x, m)? {
        Some(g) => Ok(Some(cx.eval(g)?)),
        None => Ok(None),
    }
}
