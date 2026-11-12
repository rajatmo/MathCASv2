//! Definite integrals of functions defined by cases (decision IOSYM.2): `abs`, `sign`,
//! `unit_step`, `max`, `min`, `if` and `piecewise` of the variable, and the integer-valued step functions `floor`,
//! `ceiling`, `round`, `integer_part` and `fractional_part` of a linear argument over a bounded interval (round 19,
//! Q19-34: cut where the argument crosses an integer, or a half-integer for `round`; at most 64 pieces).
//!
//! The interval is cut at the real zeros of the switching expressions (the arguments of `abs`, `sign` and
//! `unit_step`, the differences of the arguments of `max` and `min`, the two sides of every comparison in a
//! condition), found by `solve(g == 0, x, reals)`. On each piece every switch is replaced by its branch at one inner
//! point, and the plain integrand is integrated by [`super::integrate`]; the result is the sum. Anything not decided
//! (a zero set that is not a finite list of real constants, a condition or sign not decided at the inner point, a
//! piece without an integral) leaves the call unevaluated.
//!
//! The switching functions, their cut points and branches are `mc-limit`'s ([`mc_limit::switches`], shared with
//! one-sided limits); this module integrates piece by piece.
use super::{cmp, is_failure, point, sign, solve, subst, Job, Pt};
use crate::{apply, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use mc_limit::switches::{cut_points, resolve, step_cuts, switch, switches_of, term, zero_exprs, Switch};
use std::cmp::Ordering;

/// The integral of `f` in `x` over `[lo, hi]` by cases, or `None` (module comment).
pub(crate) fn by_cases<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
) -> Res<Option<T<'h>>> {
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
    let Some(mut cuts) = cut_points(cx, b, &gs, x, p, q)? else {
        // cut points with parameters (`unit_step(x) unit_step(y - x)`): the order of the bounds decides
        return param_steps(job, cx, b, f, x, p, q, flip);
    };
    if !step_cuts(cx, b, &switches, x, p, q, &mut cuts)? {
        return Ok(None);
    }
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
        let Some(piece) = resolve(cx, b, f, x, m, true)? else { return Ok(None) };
        let piece = cx.eval(piece)?;
        let Some(v) = solve(job, cx, b, piece, x, term(b, w[0]), term(b, w[1]))? else { return Ok(None) };
        if is_failure(b, v) {
            return Ok(Some(v));
        }
        parts.push(v);
    }
    let total = b.add(&parts);
    Ok(Some(if flip { b.neg(total) } else { total }))
}


/// The extreme (`max` when `up`, else `min`) of the points `ts`, as the list of cases `(conditions, point)` where
/// each case is the extreme when its conditions hold; the order of two points that is not decided splits the case
/// (at most 8 cases, else `None`).
#[allow(clippy::type_complexity)]
fn extremes<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, ts: &[T<'h>], up: bool) -> Res<Option<Vec<(Vec<T<'h>>, T<'h>)>>> {
    let mut cases: Vec<(Vec<T<'h>>, T<'h>)> = vec![(vec![], ts[0])];
    for &t in &ts[1..] {
        let mut next = vec![];
        for (conds, c) in cases {
            let d = cx.eval(b.sub(t, c))?;
            let bigger = |cx: &mut NativeCx<'_, '_, 'h>| -> Res<T<'h>> { apply(cx, b, "simplify", &[b.call(if up { "greater" } else { "less" }, &[d, b.zero()])]) };
            let rest = |cx: &mut NativeCx<'_, '_, 'h>| -> Res<T<'h>> { apply(cx, b, "simplify", &[b.call(if up { "less_equal" } else { "greater_equal" }, &[d, b.zero()])]) };
            match sign(cx, b, d)? {
                Some(k) if (k > 0) == up && k != 0 => next.push((conds, t)),
                Some(_) => next.push((conds, c)),
                None => {
                    let (g, r) = (bigger(cx)?, rest(cx)?);
                    let mut a = conds.clone();
                    a.push(g);
                    next.push((a, t));
                    let mut e = conds;
                    e.push(r);
                    next.push((e, c));
                }
            }
        }
        if next.len() > 8 {
            return Ok(None);
        }
        cases = next;
    }
    Ok(Some(cases))
}

/// The integral over `[p, q]` of `h(x) unit_step(g1) unit_step(g2) ...` with the `gi` linear in `x` with a constant
/// slope, whose zeros are not all comparable (they hold parameters): the support is `[max lower bound, min upper
/// bound]`, the value `integral of h over it` times `unit_step(upper - lower)`; when the order of two bounds depends
/// on the parameters, a `piecewise` over the cases of the order (the convolution of two boxes is a triangle).
#[allow(clippy::too_many_arguments)]
fn param_steps<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    p: Pt<'h>,
    q: Pt<'h>,
    flip: bool,
) -> Res<Option<T<'h>>> {
    let h = b.heap();
    // the factors: unit steps of x and the rest
    let factors: Vec<T<'h>> = match h.view(f) {
        View::App { head, args } if b.sym_of(head) == Some(b.hd.mul) => args.to_vec(),
        _ => vec![f],
    };
    let (mut lower, mut upper, mut rest) = (vec![], vec![], vec![]);
    for &fac in &factors {
        match switch(b, fac) {
            Some((Switch::UnitStep, [g])) if !is_free(b, *g, x) => {
                let slope = apply(cx, b, "d", &[*g, x])?;
                if !is_free(b, slope, x) {
                    return Ok(None);
                }
                let Some(sg) = sign(cx, b, slope)? else { return Ok(None) };
                if sg == 0 {
                    return Ok(None);
                }
                let g0 = subst(cx, b, *g, x, b.zero())?;
                let x0 = cx.eval(b.neg(b.div(g0, slope)))?;
                if sg > 0 {
                    lower.push(x0);
                } else {
                    upper.push(x0);
                }
            }
            _ => rest.push(fac),
        }
    }
    if lower.is_empty() && upper.is_empty() {
        return Ok(None);
    }
    let h_part = cx.eval(b.mul(&rest))?;
    if !switches_of(b, h_part, x).is_empty() {
        return Ok(None);
    }
    if let Pt::Fin(t) = p {
        lower.push(t);
    }
    if let Pt::Fin(t) = q {
        upper.push(t);
    }
    let ninf = term(b, Pt::NegInf);
    let pinf = term(b, Pt::PosInf);
    let lows = if lower.is_empty() { vec![(vec![], ninf)] } else { match extremes(cx, b, &lower, true)? { Some(v) => v, None => return Ok(None) } };
    let ups = if upper.is_empty() { vec![(vec![], pinf)] } else { match extremes(cx, b, &upper, false)? { Some(v) => v, None => return Ok(None) } };
    let mut cases: Vec<(Vec<T<'h>>, T<'h>)> = vec![];
    for (cl, a) in &lows {
        for (cu, e) in &ups {
            cx.tick(1)?;
            let mut conds: Vec<T<'h>> = cl.iter().chain(cu).copied().collect();
            let finite = *a != ninf && *e != pinf;
            let width = cx.eval(b.sub(*e, *a))?;
            if finite {
                match sign(cx, b, width)? {
                    Some(k) if k <= 0 => continue,
                    Some(_) => {}
                    None => conds.push(apply(cx, b, "simplify", &[b.call("greater", &[width, b.zero()])])?),
                }
            }
            // an impossible combination of orders (`y > 0 and y < 0`) is dropped
            if conds.len() > 1 {
                let all = b.call("and", &conds);
                let vars = apply(cx, b, "variables", &[all])?;
                let mut params = vec![];
                if let Some(vs) = cx.as_list(vars) {
                    for &v in vs {
                        let numeric = apply(cx, b, "is_numeric", &[v])?;
                        if !cx.is_true(numeric) {
                            params.push(v);
                        }
                    }
                }
                let ex = b.call("exists", &[cx.list(&params), all]);
                let feasible = apply(cx, b, "resolve", &[ex, h.sym_term(h.sym("reals"))])?;
                if cx.is_false(feasible) {
                    continue;
                }
            }
            let Some(v) = solve(job, cx, b, h_part, x, *a, *e)? else { return Ok(None) };
            if is_failure(b, v) {
                return Ok(Some(v));
            }
            cases.push((conds, v));
        }
    }
    let sgn = |v: T<'h>| if flip { b.neg(v) } else { v };
    let out = match cases.as_slice() {
        [] => b.zero(),
        [(conds, v)] if conds.is_empty() => sgn(*v),
        [(conds, v)] if conds.len() == 1 && lows.len() == 1 && ups.len() == 1 && lows[0].0.is_empty() && ups[0].0.is_empty() => {
            // one support, positive width a parameter condition: a unit step
            let width = match h.view(conds[0]) {
                View::App { args: &[w, _], .. } => w,
                _ => return Ok(None),
            };
            b.mul(&[sgn(*v), b.call("unit_step", &[width])])
        }
        _ => {
            let rows: Vec<T<'h>> = cases
                .iter()
                .map(|(c, v)| {
                    let cond = match c.as_slice() {
                        [] => h.sym_term(h.sym("true")),
                        [one] => *one,
                        many => b.call("and", many),
                    };
                    h.call(b.hd.list, &[sgn(*v), cond])
                })
                .collect();
            b.call("piecewise", &[h.call(b.hd.list, &rows), b.zero()])
        }
    };
    Ok(Some(cx.eval(out)?))
}
