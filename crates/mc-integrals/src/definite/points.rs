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
use crate::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashSet;
use std::cmp::Ordering;

/// Push onto `out` the expressions whose real zeros are the candidate critical points of `t`; `false` when `t`
/// contains a function of `x` this search does not know.
pub(super) fn collect<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>, out: &mut Vec<T<'h>>) -> bool {
    collect_with(b, t, x, out, false)
}

/// [`collect`]; with `trig`, the trigonometric functions with poles (`tan`, `sec`, `cot`, `csc`) and `sin`, `cos`
/// as bases of negative or fractional powers and as arguments of `log` are accepted without collecting their zeros:
/// the caller locates those with [`trig_poles`].
pub(super) fn collect_with<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>, out: &mut Vec<T<'h>>, trig: bool) -> bool {
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
                let zeros = !(trig && is_sin_cos(b, *base));
                if !is_free(b, *e, x) {
                    stack.push(*e);
                    if zeros {
                        out.push(*base);
                    }
                } else if !matches!(h.view(*e), View::Int(1..)) && zeros {
                    out.push(*base);
                }
            }
            ("exp" | "sin" | "cos" | "sinh" | "cosh" | "tanh" | "atan", [a]) => stack.push(*a),
            // special functions of the antiderivatives (sin(x)/x, exp(-x^2), cos(x)/x): entire ones, and those with a
            // logarithmic singularity where the argument is 0
            (
                "erf" | "erfc" | "erfi" | "sin_integral" | "sinh_integral" | "fresnel_s" | "fresnel_c" | "dawson_f" | "airy_ai"
                | "airy_bi",
                [a],
            ) => stack.push(*a),
            ("cos_integral" | "cosh_integral" | "exp_integral_ei", [a]) => {
                stack.push(*a);
                out.push(*a);
            }
            ("tan" | "sec" | "cot" | "csc", [a]) if trig => stack.push(*a),
            ("log", [a]) => {
                stack.push(*a);
                if !(trig && is_sin_cos(b, *a)) {
                    out.push(*a);
                }
            }
            // a root_sum of the logarithmic part of a rational integral: its logarithms log(x - t) are singular at
            // the real roots t of the polynomial, so those are the critical points
            ("root_sum", [p, body]) => {
                let (Some((v, poly)), Some((w, inner))) = (fn_parts(b, *p), fn_parts(b, *body)) else { return false };
                if !is_free(b, inner, x) && !only_logs_of_x_minus(b, inner, w, x) {
                    return false;
                }
                out.push(replace(b, poly, v, x));
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
        if !rational(b, z, x) {
            // a transcendental expression (`log(x)`, `1 + log(x)^2`): its real zeros by `solve` over the reals,
            // when they are a finite list of real constants (round 19, Q19-34); the result is checked numerically
            let Some(rs) = solved_zeros(cx, b, z, x)? else { return Ok(None) };
            for r in rs {
                let p = Pt::Fin(r);
                let (Some(a), Some(c)) = (cmp(cx, b, lo, p)?, cmp(cx, b, p, hi)?) else { return Ok(None) };
                if a != Ordering::Greater && c != Ordering::Greater && !insert_sorted(cx, b, &mut roots, p)? {
                    return Ok(None);
                }
            }
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

/// Is `t` a rational function of `x` (sums, products, integer powers; other subterms free of `x`)?
fn rational<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
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

/// The real zeros of `z` from `solve(z == 0, x, reals)`: `None` unless they are a finite list of real constants.
fn solved_zeros<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, z: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let h = b.heap();
    let eq = h.call(h.sym("equal"), &[z, b.zero()]);
    let sols = apply(cx, b, "solve", &[eq, x, h.sym_term(h.sym("reals"))])?;
    let View::App { head, args: sols } = h.view(sols) else { return Ok(None) };
    if b.sym_of(head) != Some(b.hd.list) {
        return Ok(None);
    }
    let mut out = vec![];
    for &sol in sols {
        let v = match h.view(sol) {
            View::App { head, args: &[r] } if b.sym_of(head) == Some(b.hd.list) => match h.view(r) {
                View::App { args: &[k, v], .. } if k == x => v,
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        if !is_free(b, v, x) || !real_constant(cx, b, v)? {
            return Ok(None);
        }
        out.push(v);
    }
    Ok(Some(out))
}

/// Insert `p` into the sorted distinct `v`; `false` when an order is not decided.
pub(super) fn insert_sorted<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: &mut Vec<Pt<'h>>, p: Pt<'h>) -> Res<bool> {
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
pub(super) fn fraction<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
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
pub(super) fn coefficients<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let cl = apply(cx, b, "coefficient_list", &[p, x])?;
    let Some(cs) = args_of(b, cl, b.hd.list) else { return Ok(None) };
    if cs.iter().any(|&c| !is_free(b, c, x)) {
        return Ok(None);
    }
    Ok(Some(cs.to_vec()))
}

/// The non-constant irreducible factors of the polynomial `p` (by `factor`), with coefficients that are real
/// constants; `None` otherwise.
pub(super) fn factors<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
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

/// Is `t` `sin(u)` or `cos(u)`?
fn is_sin_cos<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    matches!(call_of(b, t), Some((n, [_])) if n == "sin" || n == "cos")
}

/// Insert the point `p` into the sorted distinct `v`; `false` when an order is not decided.
pub(super) fn insert_point<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: &mut Vec<Pt<'h>>, p: Pt<'h>) -> Res<bool> {
    insert_sorted(cx, b, v, p)
}

/// The most trigonometric poles located in one interval.
const MAX_TRIG_POLES: i64 = 64;

/// The zeros in `[lo, hi]` of the `sin(u)` and `cos(u)` behind the poles and branch points that
/// [`collect_with`] accepted with `trig` (`tan`, `sec`, `cot`, `csc`, `sin` and `cos` as bases of negative or
/// fractional powers and as arguments of `log`), for arguments `u = a x + c` linear in `x` with real constant `a`
/// and `c`: `x = (k pi + phi - c)/a` with `phi = pi/2` for the cosine. `None` when such a function has another
/// argument, a bound is infinite and there is one, or there are too many.
pub(super) fn trig_poles<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    t: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<Vec<Pt<'h>>>> {
    let h = b.heap();
    // (u, cosine zeros?) for every pole kind found
    let mut kinds: Vec<(T<'h>, bool)> = vec![];
    let mut seen: FxHashSet<T<'h>> = FxHashSet::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if is_free(b, u, x) || !seen.insert(u) {
            continue;
        }
        let View::App { args, .. } = h.view(u) else { continue };
        stack.extend_from_slice(args);
        let Some((name, args)) = call_of(b, u) else { continue };
        let mut push = |w: T<'h>| match call_of(b, w) {
            Some((n, [a])) if n == "sin" => kinds.push((*a, false)),
            Some((n, [a])) if n == "cos" => kinds.push((*a, true)),
            _ => {}
        };
        match (name.as_str(), args) {
            ("tan" | "sec", [a]) => kinds.push((*a, true)),
            ("cot" | "csc", [a]) => kinds.push((*a, false)),
            ("pow", [base, e]) if !matches!(h.view(*e), View::Int(1..)) => push(*base),
            ("log", [a]) => push(*a),
            _ => {}
        }
    }
    let mut out: Vec<Pt<'h>> = vec![];
    if kinds.is_empty() {
        return Ok(Some(out));
    }
    let (Pt::Fin(l), Pt::Fin(r)) = (lo, hi) else { return Ok(None) };
    let pi = h.sym_term(h.sym("pi"));
    for (u, cosine) in kinds {
        cx.tick(1)?;
        let a = apply(cx, b, "d", &[u, x])?;
        if !is_free(b, a, x) || a == b.zero() || !real_constant(cx, b, a)? {
            return Ok(None);
        }
        let c = subst(cx, b, u, x, b.zero())?;
        if !real_constant(cx, b, c)? {
            return Ok(None);
        }
        let phi = if cosine { b.mul(&[b.rat(1, 2), pi]) } else { b.zero() };
        // the range of k from the numerical values of u at the bounds
        let ul = subst(cx, b, u, x, l)?;
        let ul = apply(cx, b, "n", &[ul])?;
        let ur = subst(cx, b, u, x, r)?;
        let ur = apply(cx, b, "n", &[ur])?;
        let (View::Real(ul), View::Real(ur)) = (h.view(ul), h.view(ur)) else { return Ok(None) };
        let (umin, umax) = if ul <= ur { (ul, ur) } else { (ur, ul) };
        let off = if cosine { 0.5 } else { 0.0 };
        let k0 = ((umin / std::f64::consts::PI) - off).floor() as i64 - 1;
        let k1 = ((umax / std::f64::consts::PI) - off).ceil() as i64 + 1;
        if k1 - k0 > MAX_TRIG_POLES {
            return Ok(None);
        }
        for k in k0..=k1 {
            let xk = cx.eval(b.div(b.sub(b.add(&[b.mul(&[b.int(k), pi]), phi]), c), a))?;
            let p = Pt::Fin(xk);
            match (cmp(cx, b, p, lo)?, cmp(cx, b, p, hi)?) {
                (Some(Ordering::Less), _) | (_, Some(Ordering::Greater)) => continue,
                (Some(_), Some(_)) => {
                    if !insert_sorted(cx, b, &mut out, p)? {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            }
        }
    }
    Ok(Some(out))
}

/// The variable and body of a pure function `fn([t], body)`.
fn fn_parts<'h>(b: &Build<'h>, f: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let h = b.heap();
    let (name, args) = call_of(b, f)?;
    if name != "fn" {
        return None;
    }
    let [vars, body] = args else { return None };
    match h.view(*vars) {
        View::App { args: &[v], .. } => Some((v, *body)),
        _ => None,
    }
}

/// `t` with every occurrence of `from` replaced by `to`, structurally.
fn replace<'h>(b: &Build<'h>, t: T<'h>, from: T<'h>, to: T<'h>) -> T<'h> {
    if t == from {
        return to;
    }
    match b.heap().view(t) {
        View::App { head, args } => {
            let new: Vec<T<'h>> = args.iter().map(|&a| replace(b, a, from, to)).collect();
            b.app(replace(b, head, from, to), &new)
        }
        _ => t,
    }
}

/// Does `t` depend on `x` only through logarithms `log(x - w)` (`w` the root_sum's bound variable)?
fn only_logs_of_x_minus<'h>(b: &Build<'h>, t: T<'h>, w: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return false;
        }
        if is_free(b, u, x) {
            continue;
        }
        let Some((name, args)) = call_of(b, u) else { return false };
        match (name.as_str(), args) {
            ("add" | "mul", rest) => stack.extend_from_slice(rest),
            ("log", [a]) => {
                // x - w
                if !is_free(b, *a, w) && is_free(b, replace(b, *a, w, b.zero()), w) {
                    let rest = replace(b, *a, x, b.zero());
                    if !is_free(b, rest, x) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            ("pow", [base, e]) if is_free(b, *base, x) || is_free(b, *e, x) => {
                if !is_free(b, *base, x) {
                    stack.push(*base);
                }
            }
            _ => return false,
        }
    }
    true
}
