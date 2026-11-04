//! The critical points of an integrand and its antiderivative in a real interval.
//!
//! [`collect`] walks a term and gathers the expressions whose real zeros may be points where the term is not
//! continuous or not differentiable: the bases of negative and fractional powers, the arguments of `log`, `u ∓ 1`
//! for `asin(u)` and `acos(u)`. Poles of arguments (the jump of `atan(1/x)` at 0) come from the walk into the
//! argument. [`critical`] turns each expression into a rational function, factors its numerator and denominator
//! and locates the real roots of every factor in the interval: exactly for degree 1, 2 and binomials `c x^n + d`;
//! for other factors a Sturm sequence decides whether there is a root inside, and when there is one the search
//! gives up (decision 2C4.6).
use super::{call_of, cmp, real_constant, sign, subst, Pt};
use crate::integrate::{apply, args_of, is_free};
use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashSet;
use std::cmp::Ordering;

/// Push onto `out` the expressions whose real zeros are the candidate critical points of `t`; `false` when `t`
/// contains a function of `x` this search does not know.
pub(super) fn collect<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>, out: &mut Vec<T<'h>>) -> bool {
    let h = b.heap();
    let mut seen: FxHashSet<T<'h>> = FxHashSet::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x || is_free(b, u, x) || !seen.insert(u) {
            continue;
        }
        if let Some(args) = args_of(b, u, b.hd.add).or_else(|| args_of(b, u, b.hd.mul)) {
            stack.extend_from_slice(args);
            continue;
        }
        let Some((name, args)) = call_of(b, u) else { return false };
        match (name.as_str(), args) {
            ("pow", [base, e]) if b.sym_of(*base) == Some(b.hd.e) => stack.push(*e),
            ("pow", [base, e]) => {
                stack.push(*base);
                if !is_free(b, *e, x) {
                    stack.push(*e);
                    out.push(*base);
                } else if !matches!(h.view(*e), View::Int(1..)) {
                    out.push(*base);
                }
            }
            ("exp" | "sin" | "cos" | "sinh" | "cosh" | "tanh" | "atan", [a]) => stack.push(*a),
            ("log", [a]) => {
                stack.push(*a);
                out.push(*a);
            }
            ("asin" | "acos", [a]) => {
                stack.push(*a);
                out.push(b.sub(*a, b.one()));
                out.push(b.add(&[*a, b.one()]));
            }
            _ => return false,
        }
    }
    true
}

/// The calls `log(u)` in `t` (with `u` depending on `x`) that occur only in *additive* positions: as terms of
/// sums and constant multiples of such, never inside another function, a power or a product with another factor
/// depending on `x`. Replacing such a `log(u)` by `log(-u)` changes `t` by a constant. With their arguments.
pub(super) fn log_args<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> Vec<(T<'h>, T<'h>)> {
    let h = b.heap();
    let mut seen: FxHashSet<(T<'h>, bool)> = FxHashSet::default();
    let mut additive: Vec<(T<'h>, T<'h>)> = Vec::new();
    let mut other: FxHashSet<T<'h>> = FxHashSet::default();
    let mut stack = vec![(t, true)];
    while let Some((u, lin)) = stack.pop() {
        if is_free(b, u, x) || !seen.insert((u, lin)) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { continue };
        let hs = b.sym_of(head);
        if hs == Some(b.hd.add) {
            stack.extend(args.iter().map(|&a| (a, lin)));
        } else if hs == Some(b.hd.mul) {
            let dep = args.iter().filter(|&&a| !is_free(b, a, x)).count();
            stack.extend(args.iter().map(|&a| (a, lin && dep == 1)));
        } else {
            if let (Some("log"), [a]) = (hs.map(|s| h.sym_name(s)).as_deref(), args) {
                if lin {
                    additive.push((u, *a));
                } else {
                    other.insert(u);
                }
            }
            stack.extend(args.iter().map(|&a| (a, false)));
        }
    }
    let mut out: Vec<(T<'h>, T<'h>)> = Vec::new();
    for (u, a) in additive {
        if !other.contains(&u) && !out.iter().any(|&(v, _)| v == u) {
            out.push((u, a));
        }
    }
    out
}

/// The sorted distinct real zeros in `[lo, hi]` of the expressions `zs`, as points; `None` when they cannot be
/// located.
pub(super) fn critical<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    x: T<'h>,
    zs: &[T<'h>],
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<Vec<Pt<'h>>>> {
    let mut done: FxHashSet<T<'h>> = FxHashSet::default();
    let mut polys: FxHashSet<T<'h>> = FxHashSet::default();
    let mut roots: Vec<Pt<'h>> = Vec::new();
    for &z in zs {
        cx.tick(1)?;
        if !done.insert(z) {
            continue;
        }
        let Some((num, den)) = fraction(cx, b, z, x)? else { return Ok(None) };
        for p in [num, den] {
            if is_free(b, p, x) || !polys.insert(p) {
                continue;
            }
            let Some(fs) = factors(cx, b, p, x)? else { return Ok(None) };
            for g in fs {
                match roots_in(cx, b, g, x, lo, hi)? {
                    Some(Roots::Exact(rs)) => {
                        for r in rs {
                            if !insert_sorted(cx, b, &mut roots, Pt::Fin(r))? {
                                return Ok(None);
                            }
                        }
                    }
                    Some(Roots::Unknown) | None => return Ok(None),
                }
            }
        }
    }
    Ok(Some(roots))
}

/// Insert `p` into the sorted distinct `v`; `false` when an order is not decided.
fn insert_sorted<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: &mut Vec<Pt<'h>>, p: Pt<'h>) -> Res<bool> {
    for i in 0..v.len() {
        match cmp(cx, b, p, v[i])? {
            None => return Ok(false),
            Some(Ordering::Equal) => return Ok(true),
            Some(Ordering::Less) => {
                v.insert(i, p);
                return Ok(true);
            }
            Some(Ordering::Greater) => {}
        }
    }
    v.push(p);
    Ok(true)
}

/// Does the rational function `f` (coefficients free of `x`) have a pole in `[lo, hi]`? `None` when `f` is not a
/// rational function with a denominator of real constant coefficients, or when it cannot be decided.
pub(super) fn rational_pole<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<bool>> {
    let Some((_, den)) = fraction(cx, b, f, x)? else { return Ok(None) };
    if is_free(b, den, x) {
        return Ok(Some(false));
    }
    let Some(fs) = factors(cx, b, den, x)? else { return Ok(None) };
    let mut undecided = false;
    for g in fs {
        match roots_in(cx, b, g, x, lo, hi)? {
            Some(Roots::Exact(rs)) if rs.is_empty() => {}
            Some(_) => return Ok(Some(true)),
            None => undecided = true,
        }
    }
    Ok(if undecided { None } else { Some(false) })
}

/// The numerator and denominator of `cancel(together(t))` when both are polynomials in `x`.
fn fraction<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let r = apply(cx, b, "together", &[t])?;
    let r = apply(cx, b, "cancel", &[r])?;
    let num = apply(cx, b, "numerator", &[r])?;
    let den = apply(cx, b, "denominator", &[r])?;
    if coefficients(cx, b, num, x)?.is_none() || coefficients(cx, b, den, x)?.is_none() {
        return Ok(None);
    }
    Ok(Some((num, den)))
}

/// The coefficients of the polynomial `p` in `x`, constant term first, when they are all free of `x`.
fn coefficients<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let cl = apply(cx, b, "coefficient_list", &[p, x])?;
    let Some(cs) = args_of(b, cl, b.hd.list) else { return Ok(None) };
    if cs.iter().any(|&c| !is_free(b, c, x)) {
        return Ok(None);
    }
    Ok(Some(cs.to_vec()))
}

/// The non-constant irreducible factors of the polynomial `p` (by `factor`), with coefficients that are real
/// constants; `None` otherwise.
fn factors<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let fp = apply(cx, b, "factor", &[p])?;
    let items: Vec<T<'h>> = match args_of(b, fp, b.hd.mul) {
        Some(fs) => fs.to_vec(),
        None => vec![fp],
    };
    let mut out = Vec::new();
    for g in items {
        if is_free(b, g, x) {
            continue;
        }
        let g = match args_of(b, g, b.hd.pow) {
            Some([u, k]) if matches!(b.heap().view(*k), View::Int(1..)) => *u,
            _ => g,
        };
        let Some(cs) = coefficients(cx, b, g, x)? else { return Ok(None) };
        for c in cs {
            if !real_constant(cx, b, c)? {
                return Ok(None);
            }
        }
        out.push(g);
    }
    Ok(Some(out))
}

/// The real roots of a polynomial factor in a closed interval.
enum Roots<'h> {
    /// all of them, exactly
    Exact(Vec<T<'h>>),
    /// at least one, not located
    Unknown,
}

/// The real roots of the irreducible polynomial `g` in `[lo, hi]`; `None` when it is not decided.
fn roots_in<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<Roots<'h>>> {
    let Some(cs) = coefficients(cx, b, g, x)? else { return Ok(None) };
    let n = cs.len().saturating_sub(1);
    let all = match n {
        0 => Some(vec![]),
        1 => Some(vec![b.neg(b.div(cs[0], cs[1]))]),
        2 => {
            let (c0, c1, c2) = (cs[0], cs[1], cs[2]);
            let d = b.sub(b.powi(c1, 2), b.mul(&[b.int(4), c0, c2]));
            let two_a = b.mul(&[b.int(2), c2]);
            match sign(cx, b, d)? {
                None => return Ok(None),
                Some(-1) => Some(vec![]),
                Some(0) => Some(vec![b.neg(b.div(c1, two_a))]),
                Some(_) => {
                    let s = b.pow(d, b.rat(1, 2));
                    Some(vec![b.div(b.sub(b.neg(c1), s), two_a), b.div(b.add(&[b.neg(c1), s]), two_a)])
                }
            }
        }
        _ if cs[1..n].iter().all(|&c| c == b.zero()) => {
            // a binomial c_n x^n + c_0: the real n-th roots of -c_0/c_n
            let w = b.neg(b.div(cs[0], cs[n]));
            let nn = n as i64;
            let root = |w: T<'h>| b.pow(w, b.rat(1, nn));
            match sign(cx, b, w)? {
                None => return Ok(None),
                Some(0) => Some(vec![b.zero()]),
                Some(1) if n % 2 == 0 => Some(vec![b.neg(root(w)), root(w)]),
                Some(1) => Some(vec![root(w)]),
                Some(_) if n % 2 == 0 => Some(vec![]),
                Some(_) => Some(vec![b.neg(root(b.neg(w)))]),
            }
        }
        _ => None,
    };
    if let Some(all) = all {
        let mut inside = Vec::new();
        for r in all {
            let r = cx.eval(r)?;
            let (Some(a), Some(c)) = (cmp(cx, b, lo, Pt::Fin(r))?, cmp(cx, b, Pt::Fin(r), hi)?) else {
                return Ok(None);
            };
            if a != Ordering::Greater && c != Ordering::Greater {
                inside.push(r);
            }
        }
        return Ok(Some(Roots::Exact(inside)));
    }
    // degree 3 or more: a Sturm sequence counts the roots in the interval
    Ok(match sturm_count(cx, b, g, x, n, lo, hi)? {
        None => None,
        Some(0) => Some(Roots::Exact(vec![])),
        Some(_) => Some(Roots::Unknown),
    })
}

/// The number of distinct real roots of the square-free polynomial `p` of degree `n` in `[lo, hi]`, by a Sturm
/// sequence; `None` when a sign is not decided.
fn sturm_count<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    p: T<'h>,
    x: T<'h>,
    n: usize,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<usize>> {
    let p = apply(cx, b, "expand", &[p])?;
    let dp = apply(cx, b, "d", &[p, x])?;
    let mut seq = vec![p, apply(cx, b, "expand", &[dp])?];
    while seq.len() <= n + 1 {
        cx.tick(1)?;
        let k = seq.len();
        let r = apply(cx, b, "polynomial_remainder", &[seq[k - 2], seq[k - 1], x])?;
        if r == b.zero() {
            break;
        }
        let r = apply(cx, b, "expand", &[b.neg(r)])?;
        if coefficients(cx, b, r, x)?.is_none() {
            return Ok(None);
        }
        seq.push(r);
        if is_free(b, r, x) {
            break;
        }
    }
    let mut changes = [0usize; 2];
    let mut at_lo_zero = false;
    for (i, pt) in [lo, hi].into_iter().enumerate() {
        let mut last = 0i8;
        for (j, &s) in seq.iter().enumerate() {
            let Some(v) = sign_at(cx, b, s, x, pt)? else { return Ok(None) };
            if j == 0 && i == 0 && v == 0 {
                at_lo_zero = true;
            }
            if v != 0 {
                if last != 0 && v != last {
                    changes[i] += 1;
                }
                last = v;
            }
        }
    }
    Ok(Some(changes[0].saturating_sub(changes[1]) + usize::from(at_lo_zero)))
}

/// The sign of the polynomial `s` at `pt` (at an infinity, the sign of its leading term).
fn sign_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: T<'h>, x: T<'h>, pt: Pt<'h>) -> Res<Option<i8>> {
    match pt {
        Pt::Fin(c) => {
            let v = subst(cx, b, s, x, c)?;
            sign(cx, b, v)
        }
        Pt::PosInf | Pt::NegInf => {
            let Some(cs) = coefficients(cx, b, s, x)? else { return Ok(None) };
            let Some(&lead) = cs.last() else { return Ok(Some(0)) };
            let Some(sg) = sign(cx, b, lead)? else { return Ok(None) };
            let odd = cs.len() % 2 == 0;
            Ok(Some(if pt == Pt::NegInf && odd { -sg } else { sg }))
        }
    }
}
