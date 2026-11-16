//! `function_sign`, `function_monotonicity` and `function_convexity` with parameters, in several variables, or
//! with `strict_inequalities: true` (decision R25P.8), by the cylindrical decomposition.
//!
//! The quantity whose sign is wanted is `f` (sign), `f'` (monotonicity, one variable) or the principal minors of
//! the Hessian (convexity: all of them `>= 0` for convex, the odd ones `<= 0` and the even ones `>= 0` for
//! concave, all zero for linear). Factors that are positive everywhere (`exp(u)`) are dropped; a rational
//! quantity `N/D` has the sign of `N D` where `D != 0`. With the region `R` (the condition, `true` by default):
//!
//! - `S(rel) = for_all(x, R and D != 0 implies N D rel 0)`, `Z = for_all(x, R and D != 0 implies N == 0)`, and the
//!   definedness `P = exists(x, R and D != 0)` (`a != 0` for `x^2/a^2`).
//! - `1` under `P and S(>=) and not Z` (`>` with strict inequalities), `-1` under `P and S(<=) and not Z`, `0`
//!   under `P and Z`; each decided by `resolve`. Without parameters these are `true` or `false`; with parameters
//!   the first that is not `false` is the condition of a `conditional_expression`; when all are `false` the
//!   answer is `indeterminate`.
//!
//! Only polynomial quantities (after dropping positive factors) are decided here; other calls go to the proofs and
//! refutations of [`super`].
use super::sets::{self, is_head};
use super::spec::{self, Spec};
use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The property: sign (0), monotonicity (1), convexity (2).
pub fn shape<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, order: usize) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if s.vector || s.dom != spec::Dom::Reals || order == 1 && s.vars.len() != 1 {
        return Ok(None);
    }
    // the parameters of the function and of the condition (`x > b`: a condition in `a` alone took b for a constant)
    let mut with_cond = s.comps.clone();
    with_cond.extend(s.cond);
    let ps = spec::params(b, &with_cond, &s.vars);
    if s.speed && !ps.is_empty() {
        return Ok(None);
    }
    let f = s.f;
    // functions with jumps or kinks: their derivatives miss them (refuted by differences in the caller)
    if super::nonsmooth_any(b, f, &s.vars) {
        return Ok(None);
    }
    // monotonicity and convexity from the sign of a derivative hold on one interval (a convex region) only: over
    // several pieces the values across the gaps decide (`x^3 - 3 x + a` on `x < -3/2 or x > 11/10` is not monotonic)
    if order >= 1 {
        if let Some(c) = s.cond {
            let convex = match s.vars.as_slice() {
                [x] => super::onto::fixed_interval(cx, b, c, *x)?,
                _ => is_box(b, c, &s.vars),
            };
            if !convex {
                return Ok(None);
            }
        }
    }
    // strictly monotonic (strictly convex in one variable) exactly when the derivative keeps one weak sign and is not
    // 0 on a piece of the interval: the zeros of a rational N/D with N != 0 are isolated (x^3 strictly increasing,
    // x^4 strictly convex); in several variables a definite Hessian is only sufficient (x^4 + y^4), see the end
    let weak_strict = s.strict && (order == 1 || order == 2 && s.vars.len() == 1);
    if s.strict && order == 2 && s.vars.len() > 1 {
        // strict minors: a definite Hessian proves strict convexity; anything else only when f is not convex at all
        let r = decide(cx, b, s, order, false, &ps)?;
        if r.is_some_and(|r| r == b.one() || r == b.int(-1)) {
            return Ok(r);
        }
        let weak = Spec { strict: false, ..s.clone() };
        let w = decide(cx, b, &weak, order, false, &ps)?;
        let ind = h.sym_term(h.sym("indeterminate"));
        if w.is_some_and(|w| w == ind) {
            return Ok(Some(ind));
        }
        // convex (concave) with a Hessian singular everywhere: the graph is developable, ruled by segments along which
        // f is affine (Hartman and Nirenberg, Amer. J. Math. 81 (1959)), so f is not strictly convex (x^2 in [x, y])
        if w.is_some_and(|w| w == b.one() || w == b.int(-1)) && ps.is_empty() {
            let mut rows = vec![];
            for &xi in &s.vars {
                let di = cx.eval(b.call("d", &[s.f, xi]))?;
                let mut row = vec![];
                for &xj in &s.vars {
                    row.push(cx.eval(b.call("d", &[di, xj]))?);
                }
                rows.push(cx.list(&row));
            }
            let m = cx.list(&rows);
            let det = cx.eval(b.call("expand", &[b.call("det", &[m])]))?;
            if b.is_zero(det) {
                return Ok(Some(ind));
            }
        }
        return Ok(None);
    }
    decide(cx, b, s, order, weak_strict, &ps)
}

/// Is the condition a conjunction of relations between one variable and a term free of the variables (a box)?
fn is_box<'h>(b: &Build<'h>, c: T<'h>, vars: &[T<'h>]) -> bool {
    let h = b.heap();
    let atoms: Vec<T<'h>> = match h.view(c) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
        _ => vec![c],
    };
    let free = |t: T<'h>| vars.iter().all(|&x| crate::common::is_free(b, t, x));
    atoms.iter().all(|&a| {
        let View::App { head, args: &[l, r] } = h.view(a) else { return false };
        let Some(sy) = b.sym_of(head) else { return false };
        matches!(h.sym_name(sy).as_str(), "less" | "greater" | "less_equal" | "greater_equal")
            && (vars.contains(&l) && free(r) || vars.contains(&r) && free(l))
    })
}

/// The answer from the signs of the quantities (module comment); `weak_strict`: strictness from a weak sign that is
/// not 0 everywhere.
fn decide<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, order: usize, weak_strict: bool, ps: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let ps = ps.to_vec();
    let f = s.f;
    // the quantities: (expression, relation for "1", relation for "-1")
    let mut qs: Vec<(T<'h>, &str, &str)> = vec![];
    match order {
        0 => qs.push((f, "greater_equal", "less_equal")),
        1 => qs.push((cx.eval(b.call("d", &[f, s.vars[0]]))?, "greater_equal", "less_equal")),
        _ => {
            let n = s.vars.len();
            if n > 3 {
                return Ok(None);
            }
            let mut hm = vec![vec![b.zero(); n]; n];
            for (row, &xi) in hm.iter_mut().zip(&s.vars) {
                let di = cx.eval(b.call("d", &[f, xi]))?;
                for (cell, &xj) in row.iter_mut().zip(&s.vars) {
                    *cell = cx.eval(b.call("d", &[di, xj]))?;
                }
            }
            // principal minors by subsets of the indices
            for mask in 1u32..(1 << n) {
                let idx: Vec<usize> = (0..n).filter(|&i| mask & (1 << i) != 0).collect();
                let rows: Vec<T<'h>> = idx.iter().map(|&i| cx.list(&idx.iter().map(|&j| hm[i][j]).collect::<Vec<_>>())).collect();
                let m = cx.list(&rows);
                let det = cx.eval(b.call("simplify", &[b.call("det", &[m])]))?;
                let odd = idx.len() % 2 == 1;
                qs.push((det, "greater_equal", if odd { "less_equal" } else { "greater_equal" }));
            }
        }
    }
    let region = s.cond.unwrap_or_else(|| cx.bool(true));
    let xs = cx.list(&s.vars);
    // monotonicity and convexity: a real pole of `f` inside the region breaks it (the function jumps across the pole,
    // B26-5; and the domain is no longer one interval: `1/x^2 + a` is not convex on x != 0, as `1/x^2` is not)
    let mut no_pole = vec![];
    if order >= 1 {
        let tf = cx.eval(b.call("together", &[f]))?;
        let fden = cx.eval(b.call("denominator", &[tf]))?;
        let mut syms = s.vars.clone();
        syms.extend(ps.iter().copied());
        if sets::poly_degree(b, fden, &syms).is_none() {
            return Ok(None);
        }
        if fden != b.one() {
            let e = b.call("and", &[region, b.call("equal", &[fden, b.zero()])]);
            no_pole.push(b.call("not", &[b.call("exists", &[xs, e])]));
        }
    }
    let mut s_up = vec![];
    let mut s_down = vec![];
    let mut s_zero = vec![];
    let mut defined = vec![];
    for (q, up, down) in qs {
        let q = strip_positive(cx, b, q)?;
        let t = cx.eval(b.call("together", &[q]))?;
        let num = cx.eval(b.call("numerator", &[t]))?;
        let den = cx.eval(b.call("denominator", &[t]))?;
        let mut syms = s.vars.clone();
        syms.extend(ps.iter().copied());
        if sets::poly_degree(b, num, &syms).is_none() || sets::poly_degree(b, den, &syms).is_none() {
            return Ok(None);
        }
        let nd = cx.eval(b.mul(&[num, den]))?;
        let dz = b.call("unequal", &[den, b.zero()]);
        let hyp = b.call("and", &[region, dz]);
        let up = if s.strict && !weak_strict { strictly(up) } else { up };
        let down_rel = if s.strict && !weak_strict { strictly(down) } else { down };
        let free = |t: T<'h>| s.vars.iter().all(|&x| crate::common::is_free(b, t, x));
        // for_all(x, hyp implies body), without the quantifier when nothing depends on x
        let all = |cx: &mut NativeCx<'_, '_, 'h>, body: T<'h>| -> Res<T<'h>> {
            let imp = cx.eval(b.call("or", &[b.call("not", &[hyp]), body]))?;
            Ok(if free(imp) { imp } else { b.call("for_all", &[xs, imp]) })
        };
        s_up.push(all(cx, b.call(up, &[nd, b.zero()]))?);
        s_down.push(all(cx, b.call(down_rel, &[nd, b.zero()]))?);
        s_zero.push(all(cx, b.call("equal", &[num, b.zero()]))?);
        if den != b.one() {
            let e = cx.eval(hyp)?;
            defined.push(if free(e) { e } else { b.call("exists", &[xs, e]) });
        }
    }
    let and = |cx: &mut NativeCx<'_, '_, 'h>, v: &[T<'h>]| -> T<'h> {
        match v {
            [] => cx.bool(true),
            [a] => *a,
            _ => b.call("and", v),
        }
    };
    let zero_all = and(cx, &s_zero);
    let p = and(cx, &defined);
    // monotonicity: a constant function is not increasing (`a x + 1` increases for `a > 0`)
    let not_zero = if order == 1 || weak_strict { b.call("not", &[zero_all]) } else { cx.bool(true) };
    let all_up = and(cx, &s_up);
    let all_down = and(cx, &s_down);
    let np = and(cx, &no_pole);
    let c_up = and(cx, &[p, all_up, not_zero, np]);
    let c_down = and(cx, &[p, all_down, not_zero, np]);
    let c_zero = and(cx, &[p, zero_all]);
    let mut res = vec![];
    for c in [c_up, c_down, c_zero] {
        let r = cx.eval(b.call("resolve", &[c, h.sym_term(h.sym("reals"))]))?;
        if is_head(b, r, "resolve") || is_head(b, r, mc_api::names::FAILURE) || is_head(b, r, "for_all") || is_head(b, r, "exists") {
            return Ok(None);
        }
        // under the facts about the parameters
        let mut r = if s.facts.is_empty() || cx.is_true(r) || cx.is_false(r) {
            r
        } else {
            cx.eval_assuming(&s.facts, b.call("refine", &[r]))?
        };
        // the facts imply the condition (`a <= 0 or a >= 0`)
        if !s.facts.is_empty() && !cx.is_true(r) && !cx.is_false(r) {
            let pf = match s.facts.as_slice() {
                [f] => *f,
                _ => b.call("and", &s.facts),
            };
            let pl = cx.list(&ps);
            let imp = b.call("for_all", &[pl, b.call("implies", &[pf, r])]);
            let ok = cx.eval(b.call("resolve", &[imp, h.sym_term(h.sym("reals"))]))?;
            if cx.is_true(ok) {
                r = cx.bool(true);
            }
        }
        // one parameter: the condition in reduced form (`a > 0` for `a >= 0 and a != 0`)
        if let [p1] = ps.as_slice() {
            if !cx.is_true(r) && !cx.is_false(r) {
                let rr = cx.eval(b.call("reduce", &[r, *p1, h.sym_term(h.sym("reals"))]))?;
                if !is_head(b, rr, "reduce") && !is_head(b, rr, mc_api::names::FAILURE) {
                    r = sets::tidy_cond(cx, b, rr)?;
                }
            }
        }
        res.push(r);
    }
    let vals = [b.one(), b.int(-1), b.zero()];
    if cx.is_true(res[2]) {
        return Ok(Some(b.zero()));
    }
    for (i, &r) in res.iter().enumerate().take(2) {
        if cx.is_true(r) {
            return Ok(Some(vals[i]));
        }
    }
    for (i, &r) in res.iter().enumerate().take(2) {
        if !cx.is_false(r) {
            return spec::conditional(cx, b, s, vals[i], r, &ps);
        }
    }
    if !cx.is_false(res[2]) {
        return spec::conditional(cx, b, s, b.zero(), res[2], &ps);
    }
    Ok(Some(h.sym_term(h.sym("indeterminate"))))
}

fn strictly(rel: &str) -> &'static str {
    match rel {
        "greater_equal" => "greater",
        "less_equal" => "less",
        _ => "greater",
    }
}

/// `q` without the factors that are positive everywhere (`exp(u)`, positive numbers).
fn strip_positive<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, q: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let is_exp = |t: T<'h>| matches!(h.view(t), View::App { head, args: &[base, _] } if b.sym_of(head) == Some(b.hd.pow) && base == h.sym_term(b.hd.e));
    if is_exp(q) {
        return Ok(b.one());
    }
    if let View::App { head, args } = h.view(q) {
        if b.sym_of(head) == Some(b.hd.mul) {
            let kept: Vec<T<'h>> = args
                .iter()
                .copied()
                .filter(|&a| !is_exp(a) && !(b.ar.num(a).is_some() && b.ar.compare(a, b.zero()) == Some(mc_arith::Cmp3::Greater)))
                .collect();
            return cx.eval(b.mul(&kept));
        }
    }
    Ok(q)
}
