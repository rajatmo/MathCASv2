//! `function_surjective`, `function_bijective` and the maps of several variables or components of
//! `function_injective` (decision R25P.7).
//!
//! The map goes from the points of the domain where the condition holds to the points of the codomain where the
//! codomain condition holds (`[f, cond, ycond]` with codomain variables `y`); for injectivity the domain is the
//! set where both hold (`ycond` at `f(x)`).
//!
//! - **Linear maps** (every component linear in the variables, no parameters, no conditions): surjective when
//!   the rank of the coefficient matrix is the number of components, injective when it is the number of variables.
//! - **Polynomial maps over the reals**: the sentences `for_all(y, ycond implies exists(x, cond and f(x) == y))`
//!   and `for_all([x1, x2], cond(x1) and cond(x2) and f(x1) == f(x2) implies x1 == x2)` decided by the cylindrical
//!   decomposition (`resolve`); with parameters their condition. A scalar polynomial of one variable on the line or
//!   on one interval with an interior is injective exactly when its derivative keeps one weak sign and is not 0
//!   everywhere (`for_all(x, f' >= 0) and exists(x, f' != 0)`, or `<= 0`; zeros of f' are isolated), the condition
//!   the parameters are given; the condition is then exact on its boundary (`a^2 - 3 b <= 0` for a cubic).
//! - **Polynomial maps over the complexes** without conditions: the same sentences decided by complex quantifier
//!   elimination (`resolve(..., complexes)`); with conditions over the real and imaginary parts; one variable: a
//!   nonconstant polynomial is surjective, and injective only when of degree 1.
//! - **One real variable, other functions**: on each interval between consecutive singular points (and the ends
//!   of the domain) where `f` is continuous, the one-sided limits at the ends; limits `-inf` and `inf` prove
//!   surjectivity (intermediate values). A value `v` (0, the finite one-sided limits) with no real solution of
//!   `f == v` (`reduce` or `solve`, the solutions checked by substitution) at which `f - v` keeps one sign on
//!   every interval proves that `f` is not surjective.
//! - **One complex variable, other functions**: a nonzero complex period (`function_period`) proves that `f` is
//!   not injective; a value among 0, 1, -1, `im`, `-im` with no solution of `f == v` proves that it is not
//!   surjective; a solution `x = g(y)` of `f == y` defined wherever `ycond` holds proves that it is.
//! - **Integers**: `a x + b` (rational `a`, `b`) maps the integers onto the integers when `1/a` and `b/a` are
//!   integers, and is injective when `a != 0`.
use super::sets::{self, is_head};
use super::spec::{self, Dom, Gc, Spec};
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// Which property.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Want {
    /// `function_surjective`
    Surjective,
    /// `function_injective`
    Injective,
    /// `function_bijective`
    Bijective,
}

/// An answer: decided, true under a condition on the parameters, or not decided.
#[derive(Clone, Copy, Debug)]
enum Ans<'h> {
    Bool(bool),
    When(T<'h>),
    Unknown,
}

/// The property `w` for the call `s`.
pub fn decide<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, w: Want) -> Res<Option<T<'h>>> {
    let mut all = s.comps.clone();
    all.extend(s.cond);
    all.extend(s.ycond);
    let mut ps = spec::params(b, &all, &s.vars);
    ps.retain(|p| !s.yvars.contains(p));
    if s.speed && !ps.is_empty() {
        return Ok(None);
    }
    let a = match w {
        Want::Surjective => surjective(cx, b, s, &ps)?,
        Want::Injective => injective(cx, b, s, &ps)?,
        Want::Bijective => {
            let i = injective(cx, b, s, &ps)?;
            if matches!(i, Ans::Bool(false)) {
                Ans::Bool(false)
            } else {
                let su = surjective(cx, b, s, &ps)?;
                match (i, su) {
                    (_, Ans::Bool(false)) => Ans::Bool(false),
                    (Ans::Bool(true), x) | (x, Ans::Bool(true)) => x,
                    (Ans::When(c1), Ans::When(c2)) => Ans::When(cx.eval(b.call("and", &[c1, c2]))?),
                    _ => Ans::Unknown,
                }
            }
        }
    };
    Ok(match a {
        Ans::Bool(v) => Some(cx.bool(v)),
        Ans::When(c) => {
            let c = cx.eval(c)?;
            // true only on a set of parameters of measure zero (`a == 0 and b != 0`): false where the
            // equations fail
            let parts: Vec<T<'h>> = match b.heap().view(c) {
                View::App { head, args } if b.sym_of(head) == Some(b.heap().sym("and")) => args.to_vec(),
                _ => vec![c],
            };
            let eqs: Vec<T<'h>> = parts.iter().copied().filter(|&p| is_head(b, p, "equal")).collect();
            if !eqs.is_empty() && !ps.is_empty() {
                let mut ne = vec![];
                for e in eqs {
                    let View::App { args: &[l, r], .. } = b.heap().view(e) else { continue };
                    ne.push(b.call("unequal", &[l, r]));
                }
                let nc = match ne.as_slice() {
                    [n] => cx.eval(*n)?,
                    _ => cx.eval(b.call("or", &ne))?,
                };
                let f = cx.bool(false);
                return spec::conditional(cx, b, s, f, nc, &ps);
            }
            if cx.is_false(c) {
                if s.gc == Gc::No {
                    None
                } else {
                    Some(cx.bool(false))
                }
            } else {
                let t = cx.bool(true);
                spec::conditional(cx, b, s, t, c, &ps)?
            }
        }
        Ans::Unknown => None,
    })
}

/// The codomain variables: the given ones, or fresh symbols.
fn yvars<'h>(b: &Build<'h>, s: &Spec<'h>) -> Vec<T<'h>> {
    let h = b.heap();
    if s.yvars.len() == s.comps.len() {
        return s.yvars.clone();
    }
    (1..=s.comps.len()).map(|i| h.sym_term(h.sym(&format!("y${i}")))).collect()
}

/// `element(x, integers)` for every variable (and `y`) in the conditions: the integer case.
fn integer_conds<'h>(cx: &NativeCx<'_, '_, 'h>, b: &Build<'h>, c: Option<T<'h>>, vs: &[T<'h>]) -> bool {
    let h = b.heap();
    let Some(c) = c else { return false };
    let ints = h.sym_term(h.sym("integers"));
    let parts: Vec<T<'h>> = match h.view(c) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
        _ => vec![c],
    };
    let _ = cx;
    vs.iter().all(|&v| {
        parts.iter().any(|&p| matches!(h.view(p), View::App { head, args: &[e, d] } if b.sym_of(head) == Some(h.sym("element")) && e == v && d == ints))
    })
}

/// The coefficient matrix of a linear map without constant terms' influence, or `None`.
fn linear_matrix<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>) -> Res<Option<T<'h>>> {
    let mut rows = vec![];
    for &f in &s.comps {
        if sets::poly_degree(b, f, &s.vars).is_none_or(|d| d > 1) {
            return Ok(None);
        }
        let mut row = vec![];
        for &x in &s.vars {
            let c = cx.eval(b.call("d", &[f, x]))?;
            if s.vars.iter().any(|&v| !is_free(b, c, v)) || b.ar.num(c).is_none() {
                return Ok(None);
            }
            row.push(c);
        }
        rows.push(cx.list(&row));
    }
    Ok(Some(cx.list(&rows)))
}

/// `resolve(sentence, dom)`: `Bool`, `When(condition)` with parameters, or `Unknown`.
fn resolve<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, sentence: T<'h>, complex: bool) -> Res<Ans<'h>> {
    let h = b.heap();
    let dom = h.sym_term(h.sym(if complex { "complexes" } else { "reals" }));
    let r = cx.eval(b.call("resolve", &[sentence, dom]))?;
    if cx.is_true(r) {
        return Ok(Ans::Bool(true));
    }
    if cx.is_false(r) {
        return Ok(Ans::Bool(false));
    }
    if is_head(b, r, "resolve") || is_head(b, r, names::FAILURE) || is_head(b, r, "for_all") || is_head(b, r, "exists") {
        return Ok(Ans::Unknown);
    }
    Ok(Ans::When(r))
}

/// The conjunction of the given terms (`true` for none).
fn conj<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, ts: &[T<'h>]) -> T<'h> {
    match ts {
        [] => cx.bool(true),
        [t] => *t,
        _ => b.call("and", ts),
    }
}

/// `t` with the rules applied.
fn sub<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, rules: &[T<'h>]) -> Res<T<'h>> {
    let h = b.heap();
    let l = cx.list(rules);
    cx.eval(h.call(h.sym("rewrite"), &[t, l]))
}

/// Under the facts about the parameters: a condition that they imply is `true`.
fn under_facts<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, a: Ans<'h>) -> Res<Ans<'h>> {
    if let Ans::When(c) = a {
        if !s.facts.is_empty() {
            let r = cx.eval_assuming(&s.facts, b.call("refine", &[c]))?;
            if cx.is_true(r) {
                return Ok(Ans::Bool(true));
            }
            if cx.is_false(r) {
                return Ok(Ans::Bool(false));
            }
        }
    }
    Ok(a)
}

fn surjective<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, ps: &[T<'h>]) -> Res<Ans<'h>> {
    let h = b.heap();
    let ys = yvars(b, s);
    let complex = s.dom == Dom::Complexes;
    // the integers: a linear function of one variable
    if s.comps.len() == 1 && s.vars.len() == 1 && integer_conds(cx, b, s.cond, &s.vars) && integer_conds(cx, b, s.ycond, &ys) {
        let (f, x) = (s.comps[0], s.vars[0]);
        if sets::poly_degree(b, f, &[x]) == Some(1) {
            let a = cx.eval(b.call("d", &[f, x]))?;
            let c = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, b.zero()])]))?;
            let (ia, ib) = (cx.eval(b.div(b.one(), a))?, cx.eval(b.div(c, a))?);
            if sets::real_rational(b, ia) && sets::real_rational(b, ib) {
                return Ok(Ans::Bool(matches!(h.view(ia), View::Int(_)) && matches!(h.view(ib), View::Int(_))));
            }
        }
        return Ok(Ans::Unknown);
    }
    if s.dom == Dom::Integers {
        return Ok(Ans::Unknown);
    }
    // linear maps
    if ps.is_empty() && s.cond.is_none() && s.ycond.is_none() {
        if let Some(m) = linear_matrix(cx, b, s)? {
            let r = cx.eval(b.call("rank", &[m]))?;
            return Ok(Ans::Bool(r == b.int(s.comps.len() as i64)));
        }
    }
    let poly = s.comps.iter().all(|&f| sets::poly_degree(b, f, &s.vars).is_some())
        && s.cond.is_none_or(|c| sets::polynomial_formula(b, c, &s.vars))
        && s.ycond.is_none_or(|c| sets::polynomial_formula(b, c, &ys));
    if poly {
        // a real polynomial of odd degree with a numeric leading coefficient: onto by intermediate values
        if !complex && s.vars.len() == 1 && s.comps.len() == 1 && s.cond.is_none() && s.ycond.is_none() {
            let cl = cx.eval(b.call("coefficient_list", &[s.comps[0], s.vars[0]]))?;
            if let Some(&lead) = cx.as_list(cl).and_then(|cs| cs.last().filter(|_| cs.len() % 2 == 0)) {
                if sets::real_rational(b, lead) && lead != b.zero() {
                    return Ok(Ans::Bool(true));
                }
            }
        }
        if complex && s.vars.len() == 1 && s.comps.len() == 1 && s.cond.is_none() && s.ycond.is_none() {
            // a polynomial of one complex variable is onto when some coefficient of a positive power is nonzero
            let cl = cx.eval(b.call("coefficient_list", &[s.comps[0], s.vars[0]]))?;
            if let Some(cs) = cx.as_list(cl) {
                let nz: Vec<T<'h>> = cs.iter().skip(1).map(|&c| b.call("unequal", &[c, b.zero()])).collect();
                let c = match nz.as_slice() {
                    [] => cx.bool(false),
                    [c] => cx.eval(*c)?,
                    _ => cx.eval(b.call("or", &nz))?,
                };
                return Ok(if cx.is_true(c) { Ans::Bool(true) } else if cx.is_false(c) { Ans::Bool(false) } else { Ans::When(c) });
            }
        }
        let eqs: Vec<T<'h>> = s.comps.iter().zip(&ys).map(|(&f, &y)| b.call("equal", &[f, y])).collect();
        let mut inner = eqs;
        if let Some(c) = s.cond {
            inner.insert(0, c);
        }
        let inner = conj(cx, b, &inner);
        let ex = b.call("exists", &[cx.list(&s.vars), inner]);
        let body = match s.ycond {
            Some(c) => b.call("implies", &[c, ex]),
            None => ex,
        };
        let sentence = b.call("for_all", &[cx.list(&ys), body]);
        if !complex {
            let r = resolve(cx, b, sentence, false)?;
            return under_facts(cx, b, s, r);
        }
        if s.cond.is_none() && s.ycond.is_none() {
            let r = resolve(cx, b, sentence, true)?;
            if !matches!(r, Ans::Unknown) {
                return under_facts(cx, b, s, r);
            }
        }
        return Ok(Ans::Unknown);
    }
    // a special function whose ontoness the table records for the parameters' facts
    if s.comps.len() == 1 && s.vars.len() == 1 && !complex && s.cond.is_none() && s.ycond.is_none() {
        if let Some(hit) = super::range::table_call(cx, b, s.comps[0], s.vars[0], &s.facts)? {
            if let Some(v) = hit.onto(cx, b)? {
                return Ok(Ans::Bool(v));
            }
            // periodic and continuous on the line: bounded, not onto
            if hit.periodic(cx, b)? && hit.atoms(cx, b, false, sets::Mode::Disc)?.is_some_and(|(a, partial)| a.is_empty() && !partial) {
                return Ok(Ans::Bool(false));
            }
        }
    }
    if !ps.is_empty() || s.comps.len() != 1 || s.vars.len() != 1 {
        return Ok(Ans::Unknown);
    }
    if complex {
        return surjective_complex(cx, b, s, ys[0]);
    }
    // a special function bounded above or below on the line (the table): not onto
    if s.cond.is_none() && s.ycond.is_none() && super::range::table_bounded(cx, b, s.comps[0], s.vars[0])? {
        return Ok(Ans::Bool(false));
    }
    let r = surjective_line(cx, b, s, ys[0])?;
    if matches!(r, Ans::Unknown) {
        if let Some(a) = surjective_by_range(cx, b, s, ys[0])? {
            return Ok(a);
        }
        // a sign proved on the domain (and the condition): the values of the other sign are not taken
        if s.ycond.is_none() {
            let (f, x) = (s.comps[0], s.vars[0]);
            let dom = cx.eval(b.call("function_domain", &[f, x]))?;
            if !is_head(b, dom, "function_domain") && !is_head(b, dom, names::FAILURE) {
                let region = match s.cond {
                    Some(k) => cx.eval(b.call("and", &[dom, k]))?,
                    None => dom,
                };
                let sg = cx.eval(b.call("function_sign", &[cx.list(&[f, region]), x]))?;
                if matches!(h.view(sg), View::Int(1 | -1)) {
                    return Ok(Ans::Bool(false));
                }
            }
        }
    }
    Ok(r)
}

/// One real variable, when the limits do not decide: the range of `f` ([`super::range`]) against the
/// codomain condition (`for_all(y, ycond implies range)`); nothing else (values growing along samples, as for
/// `x sin(x)`, prove nothing).
fn surjective_by_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, y: T<'h>) -> Res<Option<Ans<'h>>> {
    let h = b.heap();
    let (f, x) = (s.comps[0], s.vars[0]);
    let y = if s.yvars.len() == 1 { s.yvars[0] } else { y };
    let fc = match s.cond {
        Some(c) => cx.list(&[f, c]),
        None => f,
    };
    if let Some(r) = super::range::range(cx, b, &[fc, x, y])? {
        let sentence = match s.ycond {
            Some(yc) => b.call("for_all", &[cx.list(&[y]), b.call("implies", &[yc, r])]),
            None => b.call("for_all", &[cx.list(&[y]), r]),
        };
        let v = cx.eval(b.call("resolve", &[sentence, h.sym_term(h.sym("reals"))]))?;
        if cx.is_true(v) {
            return Ok(Some(Ans::Bool(true)));
        }
        if cx.is_false(v) {
            return Ok(Some(Ans::Bool(false)));
        }
        return Ok(None);
    }
    // (values unbounded above and below along samples prove nothing: the call stays)
    Ok(None)
}

/// Real singular points of the discontinuity atoms of `f` (families enumerated for `c(1)` in -3..3), sorted, and
/// whether a family was enumerated (then the points are not all of them).
#[allow(clippy::type_complexity)]
fn real_breaks<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, x: T<'h>) -> Res<Option<(Vec<(f64, T<'h>)>, bool)>> {
    let h = b.heap();
    let Some(atoms) = sets::atoms(cx, b, &s.comps, &s.vars, false, sets::Mode::Disc, &s.facts)? else { return Ok(None) };
    let c1 = b.call("c", &[b.one()]);
    let mut families = false;
    let mut pts: Vec<(f64, T<'h>)> = vec![];
    for a in atoms {
        // the boundary of each relation of the atom
        let parts: Vec<T<'h>> = match h.view(a.t) {
            View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
            _ => vec![a.t],
        };
        for p in parts {
            let View::App { args: &[l, r], .. } = h.view(p) else { return Ok(None) };
            let e = b.call("equal", &[l, r]);
            let sol = cx.eval(b.call("solve", &[e, x, h.sym_term(h.sym("reals"))]))?;
            let Some(rows) = cx.as_list(sol) else { return Ok(None) };
            for &row in rows {
                let Some(&[rule]) = cx.as_list(row) else { return Ok(None) };
                let View::App { args: &[_, v], .. } = h.view(rule) else { return Ok(None) };
                let vals: Vec<T<'h>> = if is_head(b, v, mc_decide::CONDITIONAL) {
                    let View::App { args, .. } = h.view(v) else { continue };
                    families = true;
                    let mut out = vec![];
                    for k in -3..=3 {
                        out.push(cx.eval(h.call(h.sym("rewrite"), &[args[0], h.call(h.sym(names::RULE), &[c1, b.int(k)])]))?);
                    }
                    out
                } else {
                    vec![v]
                };
                for v in vals {
                    let n = cx.eval(b.call("n", &[v]))?;
                    let View::Real(y) = h.view(n) else {
                        if let View::Int(k) = h.view(n) {
                            pts.push((k as f64, v));
                        }
                        continue;
                    };
                    pts.push((y, v));
                }
            }
        }
    }
    pts.sort_by(|a, c| a.0.total_cmp(&c.0));
    // neighbours closer than the machine can tell: one point when exactly equal, else their order is not known
    let mut out: Vec<(f64, T<'h>)> = vec![];
    for p in pts {
        if let Some(last) = out.last() {
            if (last.0 - p.0).abs() < 1e-12 * (1.0 + p.0.abs()) {
                if super::proof::closed_sign(cx, b, b.sub(p.1, last.1))? == Some(0) {
                    continue;
                }
                return Ok(None);
            }
        }
        out.push(p);
    }
    Ok(Some((out, families)))
}

/// The machine value of `t` at `x = v`, if real and finite.
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>, v: f64) -> Res<Option<f64>> {
    let h = b.heap();
    let r = cx.eval(h.call(h.sym("rewrite"), &[t, h.call(h.sym(names::RULE), &[x, h.real(v)])]))?;
    let r = cx.eval(b.call("n", &[r]))?;
    Ok(match h.view(r) {
        View::Real(y) if y.is_finite() => Some(y),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// A one-sided limit as `Some(+-inf)` or a finite value, `None` when not decided.
fn side_limit<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, at: T<'h>, from_above: bool) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let inf = h.sym_term(h.sym("inf"));
    let rule = h.call(h.sym(names::RULE), &[x, at]);
    let l = if at == inf || at == cx.eval(b.neg(inf))? {
        cx.eval(b.call("limit", &[f, rule]))?
    } else {
        let dir = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("direction")), h.sym_term(h.sym(if from_above { "from_above" } else { "from_below" }))]);
        let opts = h.call(h.sym("options"), &[dir]);
        cx.eval(b.call("limit", &[f, rule, opts]))?
    };
    if is_head(b, l, "limit") || is_head(b, l, names::FAILURE) {
        return Ok(None);
    }
    Ok(Some(l))
}

/// Is `f` (one real variable `x`, no parameters, no condition) proved to take every real value, by the limits on
/// the pieces between its real breaks ([`surjective_line`])? (`function_range` asks this last.)
pub fn onto_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<bool> {
    let s = Spec {
        f,
        comps: vec![f],
        vector: false,
        cond: None,
        ycond: None,
        vars: vec![x],
        yvars: vec![],
        dom: Dom::Reals,
        dom_given: false,
        gc: Gc::Yes,
        facts: vec![],
        strict: false,
        speed: false,
    };
    let y = b.heap().sym_term(b.heap().sym("y"));
    Ok(matches!(surjective_line(cx, b, &s, y)?, Ans::Bool(true)))
}

/// One real variable (module comment).
fn surjective_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, y: T<'h>) -> Res<Ans<'h>> {
    let h = b.heap();
    let (f, x) = (s.comps[0], s.vars[0]);
    if s.ycond.is_some() {
        return Ok(Ans::Unknown);
    }
    let Some((pts, families)) = real_breaks(cx, b, s, x)? else { return Ok(Ans::Unknown) };
    let inf = h.sym_term(h.sym("inf"));
    let ninf = cx.eval(b.neg(inf))?;
    if pts.is_empty() && !families && s.cond.is_none() && oscillates_unboundedly(cx, b, f, x)? {
        return Ok(Ans::Bool(true));
    }
    // a continuous function with a positive period is bounded (it is continuous on a closed period interval), so it is
    // not onto the line
    if pts.is_empty() && !families && s.cond.is_none() && periodic_everywhere(cx, b, f, x)? {
        return Ok(Ans::Bool(false));
    }
    // values taken at the break points themselves (sqrt(x) at 0)
    let mut attained = vec![];
    for &(_, p) in &pts {
        let at = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, p])]))?;
        let v = cx.eval(b.call("n", &[at]))?;
        match h.view(v) {
            View::Real(y) if y.is_finite() => attained.push(at),
            View::Int(_) => attained.push(at),
            _ => {}
        }
    }
    let mut ends: Vec<(f64, T<'h>)> = vec![(f64::NEG_INFINITY, ninf)];
    ends.extend(pts);
    ends.push((f64::INFINITY, inf));
    // per interval: the limits at its ends and whether f is strictly monotonic on it (a proved sign of f')
    let df = cx.eval(b.call("d", &[f, x]))?;
    let mut images: Vec<Option<(T<'h>, T<'h>)>> = vec![];
    // rays: a piece on which f tends to -inf at an end contains every value below f(p) for each exact point p inside
    // it (intermediate values on the stretch from p to that end), and one where f tends to +inf every value above
    // f(p); a ray below some v and a ray above some w <= v cover the line (-cos(x)/x: -inf at 0+, +inf at 0-)
    let (mut below, mut above): (Vec<T<'h>>, Vec<T<'h>>) = (vec![], vec![]);
    // finite limits at both ends of every piece: f is bounded on each piece (continuous there), and there are
    // finitely many pieces, so it is bounded and not onto
    let mut bounded = !families && s.cond.is_none();
    for w in ends.windows(2) {
        cx.tick(1)?;
        let (a, c) = (w[0], w[1]);
        // with a family of breaks only the stretches between two listed breaks are known to be pieces
        if families && (a.0.is_infinite() || c.0.is_infinite()) {
            images.push(None);
            continue;
        }
        let mid = if a.0.is_infinite() && c.0.is_infinite() {
            0.5
        } else if a.0.is_infinite() {
            c.0 - 1.0
        } else if c.0.is_infinite() {
            a.0 + 1.0
        } else {
            0.5 * (a.0 + c.0)
        };
        // inside the domain and the condition
        if value(cx, b, f, x, mid)?.is_none() {
            continue;
        }
        if let Some(k) = s.cond {
            let r = cx.eval(h.call(h.sym("rewrite"), &[k, h.call(h.sym(names::RULE), &[x, h.real(mid)])]))?;
            if !cx.is_true(r) {
                return Ok(Ans::Unknown);
            }
        }
        let la = side_limit(cx, b, f, x, a.1, true)?;
        let lc = side_limit(cx, b, f, x, c.1, false)?;
        for l in [la, lc] {
            bounded &= match l {
                Some(l) => {
                    let v = cx.eval(b.call("n", &[l]))?;
                    matches!(h.view(v), View::Real(y) if y.is_finite()) || matches!(h.view(v), View::Int(_))
                }
                None => false,
            };
        }
        if let (Some(la), Some(lc)) = (la, lc) {
            if [la, lc].contains(&inf) && [la, lc].contains(&ninf) {
                return Ok(Ans::Bool(true));
            }
        }
        // (not with a family of breaks: members beyond the listed ones may fall between two listed breaks)
        if s.cond.is_none() && !families && [la, lc].iter().any(|l| l.is_some_and(|l| l == inf || l == ninf)) {
            // exact points inside the piece: rationals near the machine midpoint and nearer each end
            let near: Vec<f64> = if a.0.is_finite() && c.0.is_finite() {
                vec![mid, a.0 + 0.25 * (c.0 - a.0), a.0 + 0.75 * (c.0 - a.0)]
            } else {
                vec![mid, mid + (mid - if a.0.is_finite() { a.0 } else { c.0 })]
            };
            for p in near {
                let q = cx.eval(b.call("rationalize", &[h.real(p), h.real(1e-3)]))?;
                if !sets::real_rational(b, q) {
                    continue;
                }
                // strictly inside, decided exactly
                let inside = (a.0.is_infinite() || super::proof::closed_sign(cx, b, b.sub(q, a.1))? == Some(1))
                    && (c.0.is_infinite() || super::proof::closed_sign(cx, b, b.sub(c.1, q))? == Some(1));
                if !inside {
                    continue;
                }
                let v = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, q])]))?;
                if value(cx, b, v, x, 0.0)?.is_none() {
                    continue;
                }
                if la == Some(ninf) || lc == Some(ninf) {
                    below.push(v);
                }
                if la == Some(inf) || lc == Some(inf) {
                    above.push(v);
                }
            }
        }
        // strictly monotonic on the open piece: f' of one strict sign, proved by refine there (or by enclosures over
        // a bounded piece whose ends are not poles of f')
        let mut facts = vec![b.call("element", &[x, h.sym_term(h.sym("reals"))])];
        if a.1 != ninf {
            facts.push(b.call("less", &[a.1, x]));
        }
        if c.1 != inf {
            facts.push(b.call("less", &[x, c.1]));
        }
        let mut mono = false;
        for rel in ["greater", "less"] {
            let r = cx.eval_assuming(&facts, b.call("refine", &[b.call(rel, &[df, b.zero()])]))?;
            mono |= cx.is_true(r);
        }
        if !mono && a.0.is_finite() && c.0.is_finite() {
            mono = super::proof::sign_on(cx, b, df, x, (a.0, c.0), Some((a.1, c.1)), true)?.is_some_and(|s| s != 0);
        }
        // the ends of the image: the two limits, ordered (the infinities by their sign)
        let order = |cx: &mut NativeCx<'_, '_, 'h>, p: T<'h>, q: T<'h>| -> Res<Option<(T<'h>, T<'h>)>> {
            if p == ninf || q == inf {
                return Ok(Some((p, q)));
            }
            if p == inf || q == ninf {
                return Ok(Some((q, p)));
            }
            Ok(match super::proof::closed_sign(cx, b, b.sub(q, p))? {
                Some(s) if s >= 0 => Some((p, q)),
                Some(_) => Some((q, p)),
                None => None,
            })
        };
        images.push(match (la, lc, mono) {
            (Some(p), Some(q), true) => order(cx, p, q)?,
            _ => None,
        });
    }
    for &v in &below {
        for &w in &above {
            if super::proof::closed_sign(cx, b, b.sub(v, w))?.is_some_and(|s| s >= 0) {
                return Ok(Ans::Bool(true));
            }
        }
    }
    if bounded && !images.is_empty() {
        return Ok(Ans::Bool(false));
    }
    // every image known (and every break point listed, not a family): a value outside all of them (0, or an end
    // of an image), decided exactly
    if images.is_empty() || images.iter().any(|i| i.is_none()) || families {
        return Ok(Ans::Unknown);
    }
    let images: Vec<(T<'h>, T<'h>)> = images.into_iter().flatten().collect();
    let mut tries = vec![b.zero()];
    for &(p, q) in &images {
        for e in [p, q] {
            if e != inf && e != ninf {
                tries.push(e);
                tries.push(cx.eval(b.sub(e, b.one()))?);
                tries.push(cx.eval(b.add(&[e, b.one()]))?);
            }
        }
    }
    // Some(true) when v lies strictly between p and q, Some(false) when not, None when undecided
    let between = |cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>, p: T<'h>, q: T<'h>| -> Res<Option<bool>> {
        let above = if p == ninf { Some(1) } else if p == inf { Some(-1) } else { super::proof::closed_sign(cx, b, b.sub(v, p))? };
        let below = if q == inf { Some(1) } else if q == ninf { Some(-1) } else { super::proof::closed_sign(cx, b, b.sub(q, v))? };
        Ok(match (above, below) {
            (Some(s), Some(t)) => Some(s > 0 && t > 0),
            (Some(s), _) if s <= 0 => Some(false),
            (_, Some(t)) if t <= 0 => Some(false),
            _ => None,
        })
    };
    'tries: for v in tries {
        for &(p, q) in &images {
            if between(cx, v, p, q)? != Some(false) {
                continue 'tries;
            }
        }
        for &a in &attained {
            if super::proof::closed_sign(cx, b, b.sub(v, a))?.is_none_or(|s| s == 0) {
                continue 'tries;
            }
        }
        let _ = y;
        return Ok(Ans::Bool(false));
    }
    Ok(Ans::Unknown)
}

/// Is `f` real-valued with a positive real period (the caller has shown it continuous on the line)?
fn periodic_everywhere<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<bool> {
    let h = b.heap();
    // (no break points were found, so the function is defined and continuous wherever the singular sets let it be:
    // a real value at a sample point is the sanity check)
    if value(cx, b, f, x, 0.4321)?.is_none() {
        return Ok(false);
    }
    let p = cx.eval(b.call("function_period", &[f, x]))?;
    let n = cx.eval(b.call("n", &[p]))?;
    Ok(match h.view(n) {
        View::Real(v) => v.is_finite() && v > 0.0,
        View::Int(k) => k > 0,
        _ => false,
    })
}

/// `f = P sin(w)` or `P cos(w)` (`w` affine in `x` with a nonzero slope, `P` free of that factor) with `P^2 -> inf`
/// at `inf` or `-inf`, for an `f` continuous on the whole line: at the points where the circular factor is 1 and -1
/// (a sequence tending to that end) the values are `P` and `-P`, and `P` keeps one sign there (continuous and
/// nonzero), so f has values beyond every bound of both signs and takes every real value (`exp(x^2/4) sin(x)`).
fn oscillates_unboundedly<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<bool> {
    let h = b.heap();
    let View::App { head, args } = h.view(f) else { return Ok(false) };
    if b.sym_of(head) != Some(b.hd.mul) {
        return Ok(false);
    }
    let mut circ = None;
    let mut rest = vec![];
    for &a in args {
        let is_circ = matches!(h.view(a), View::App { head: ah, args: &[w] } if b.sym_of(ah).is_some_and(|s| matches!(h.sym_name(s).as_str(), "sin" | "cos")) && !is_free(b, w, x));
        if is_circ && circ.is_none() {
            circ = Some(a);
        } else {
            rest.push(a);
        }
    }
    let Some(cf) = circ else { return Ok(false) };
    let View::App { args: &[w], .. } = h.view(cf) else { return Ok(false) };
    if !super::range::affine_in(cx, b, w, x)? || rest.is_empty() {
        return Ok(false);
    }
    let p = cx.eval(b.mul(&rest))?;
    // P free of the circular factor (its argument w)
    if !is_free(b, p, cf) {
        return Ok(false);
    }
    let inf = h.sym_term(h.sym("inf"));
    let p2 = cx.eval(b.powi(p, 2))?;
    for at in [inf, cx.eval(b.neg(inf))?] {
        let l = cx.eval(b.call("limit", &[p2, h.call(h.sym(names::RULE), &[x, at])]))?;
        if l == inf {
            return Ok(true);
        }
    }
    Ok(false)
}

/// One complex variable (module comment).
fn surjective_complex<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, y: T<'h>) -> Res<Ans<'h>> {
    let h = b.heap();
    let (f, x) = (s.comps[0], s.vars[0]);
    if s.cond.is_some() {
        return Ok(Ans::Unknown);
    }
    let imu = h.sym_term(h.sym("im"));
    if s.ycond.is_none() {
        let two_pi_im = b.mul(&[b.int(2), h.sym_term(h.sym("pi")), imu]);
        for v in [b.zero(), b.one(), b.int(-1), imu, b.neg(imu), two_pi_im] {
            cx.tick(1)?;
            if omitted(cx, b, f, x, v)? {
                return Ok(Ans::Bool(false));
            }
        }
    }
    // a solution x = g(y) defined wherever ycond holds
    let ys = if s.yvars.len() == 1 { s.yvars[0] } else { y };
    let sol = cx.eval(b.call("solve", &[b.call("equal", &[f, ys]), x]))?;
    let Some(rows) = cx.as_list(sol) else { return Ok(Ans::Unknown) };
    for &row in rows {
        let Some(&[rule]) = cx.as_list(row) else { continue };
        let View::App { args: &[_, g], .. } = h.view(rule) else { continue };
        let mut stated = false;
        let g = if is_head(b, g, mc_decide::CONDITIONAL) {
            let View::App { args, .. } = h.view(g) else { continue };
            if args.len() == 2 && is_head(b, args[1], "element") {
                cx.eval(h.call(h.sym("rewrite"), &[args[0], h.call(h.sym(names::RULE), &[b.call("c", &[b.one()]), b.zero()])]))?
            } else if args.len() == 2 && s.ycond.is_some_and(|yc| yc == args[1] || {
                let r = cx.eval_assuming(&[yc], b.call("refine", &[args[1]])).ok();
                r.is_some_and(|r| cx.is_true(r))
            }) {
                // a solution under a condition the codomain condition implies
                stated = true;
                args[0]
            } else {
                continue;
            }
        } else {
            g
        };
        // f(g(y)) == y
        let back = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, g])]))?;
        // f(g(y)) == y must be proved (values at points only refute); a solution `solve` states under a condition
        // that the codomain's condition implies is one by what `solve` returns (log(x) == y: x = exp(y) for -pi <
        // im(y) <= pi)
        let d = cx.eval(b.call("simplify", &[b.sub(back, ys)]))?;
        if d != b.zero() && !stated {
            let proved = match s.ycond {
                Some(yc) => {
                    let r = cx.eval_assuming(&[yc], b.call("simplify", &[d]))?;
                    r == b.zero()
                }
                None => false,
            };
            if !proved {
                continue;
            }
        }
        let dom = cx.eval(b.call("function_domain", &[g, ys, h.sym_term(h.sym("complexes"))]))?;
        if cx.is_true(dom) {
            return Ok(Ans::Bool(true));
        }
        if let Some(yc) = s.ycond {
            let r = cx.eval_assuming(&[yc], b.call("refine", &[dom]))?;
            if cx.is_true(r) {
                return Ok(Ans::Bool(true));
            }
        }
    }
    Ok(Ans::Unknown)
}

/// Is `v` not a value of `f`: no solution of `f == v` that checks by substitution? A solution is set aside only
/// when f at it is proved different from `v` (the 30-digit ball of |f - v|^2 excludes 0).
fn omitted<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, v: T<'h>) -> Res<bool> {
    let h = b.heap();
    let sol = cx.eval(b.call("solve", &[b.call("equal", &[f, v]), x]))?;
    let Some(rows) = cx.as_list(sol) else { return Ok(false) };
    for &row in rows {
        let Some(&[rule]) = cx.as_list(row) else { return Ok(false) };
        let View::App { args: &[_, g], .. } = h.view(rule) else { return Ok(false) };
        let g = if is_head(b, g, mc_decide::CONDITIONAL) {
            let View::App { args, .. } = h.view(g) else { return Ok(false) };
            cx.eval(h.call(h.sym("rewrite"), &[args[0], h.call(h.sym(names::RULE), &[b.call("c", &[b.one()]), b.zero()])]))?
        } else {
            g
        };
        // a finite solution that checks
        let gn = cx.eval(b.call("n", &[g, b.int(30)]))?;
        let finite = b.ar.num(gn).is_some() || is_head(b, gn, "complex");
        if !finite {
            continue;
        }
        let at = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, gn])]))?;
        let d = cx.eval(b.call("n", &[b.sub(at, v), b.int(30)]))?;
        let re = cx.eval(b.call("re", &[d]))?;
        let im = cx.eval(b.call("im_part", &[d]))?;
        let sq = b.add(&[b.powi(re, 2), b.powi(im, 2)]);
        if super::proof::closed_sign(cx, b, sq)? != Some(1) {
            return Ok(false);
        }
        // a solution that does not check (exp(2 pi im) for log(x) == 2 pi im) is no solution
    }
    // no solution, or only solutions that are not finite numbers (atan(im))
    Ok(true)
}

fn injective<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, ps: &[T<'h>]) -> Res<Ans<'h>> {
    let h = b.heap();
    let ys = yvars(b, s);
    let complex = s.dom == Dom::Complexes;
    // the integers: a linear function
    if s.comps.len() == 1 && s.vars.len() == 1 && integer_conds(cx, b, s.cond, &s.vars) {
        let (f, x) = (s.comps[0], s.vars[0]);
        if sets::poly_degree(b, f, &[x]) == Some(1) {
            return Ok(Ans::Bool(true));
        }
        return Ok(Ans::Unknown);
    }
    if s.dom == Dom::Integers {
        return Ok(Ans::Unknown);
    }
    // a special function of an affine argument whose injectivity the table records
    if s.comps.len() == 1 && s.vars.len() == 1 && s.cond.is_none() && s.ycond.is_none() {
        if let Some(hit) = super::range::table_call(cx, b, s.comps[0], s.vars[0], &s.facts)? {
            if super::range::affine_in(cx, b, hit.arg, s.vars[0])? {
                if let Some(v) = hit.injective(cx, b, complex)? {
                    return Ok(Ans::Bool(v));
                }
            }
        }
    }
    // a special function periodic in the variable for the parameters' facts (the table): not injective
    if s.comps.len() == 1 && s.vars.len() == 1 && !complex && s.cond.is_none() && s.ycond.is_none() {
        if let Some(hit) = super::range::table_call(cx, b, s.comps[0], s.vars[0], &s.facts)? {
            if hit.periodic(cx, b)? {
                return Ok(Ans::Bool(false));
            }
            // strictly monotonic on its domain
            if hit.arg == s.vars[0] && hit.shape(cx, b, 1)?.is_some_and(|k| k.abs() == 1) {
                return Ok(Ans::Bool(true));
            }
        }
    }
    // the condition of the domain: cond, and ycond at f(x)
    let mut dcond = s.cond;
    if let Some(yc) = s.ycond {
        let rules: Vec<T<'h>> = ys.iter().zip(&s.comps).map(|(&y, &f)| h.call(h.sym(names::RULE), &[y, f])).collect();
        let at = sub(cx, b, yc, &rules)?;
        dcond = Some(match dcond {
            Some(c) => cx.eval(b.call("and", &[c, at]))?,
            None => at,
        });
        if dcond.is_some_and(|c| cx.is_true(c)) {
            dcond = None;
        }
    }
    if ps.is_empty() && dcond.is_none() {
        if let Some(m) = linear_matrix(cx, b, s)? {
            let r = cx.eval(b.call("rank", &[m]))?;
            return Ok(Ans::Bool(r == b.int(s.vars.len() as i64)));
        }
    }
    let poly = s.comps.iter().all(|&f| sets::poly_degree(b, f, &s.vars).is_some()) && dcond.is_none_or(|c| sets::polynomial_formula(b, c, &s.vars));
    if complex {
        if matches!(h.view(s.comps[0]), View::App { head, args: &[a] } if b.sym_of(head) == Some(h.sym("log")) && a == s.vars[0]) && s.comps.len() == 1 {
            // the principal logarithm is injective on the whole punctured plane
            return Ok(Ans::Bool(true));
        }
        if poly && s.vars.len() == 1 && s.comps.len() == 1 && dcond.is_none() {
            // injective only when of degree 1: the coefficient of x nonzero and the higher ones zero
            let cl = cx.eval(b.call("coefficient_list", &[s.comps[0], s.vars[0]]))?;
            if let Some(cs) = cx.as_list(cl) {
                if cs.len() < 2 {
                    return Ok(Ans::Bool(false));
                }
                let mut parts = vec![b.call("unequal", &[cs[1], b.zero()])];
                parts.extend(cs.iter().skip(2).map(|&c| b.call("equal", &[c, b.zero()])));
                let cj = conj(cx, b, &parts);
                let c = cx.eval(cj)?;
                return Ok(if cx.is_true(c) { Ans::Bool(true) } else if cx.is_false(c) { Ans::Bool(false) } else { Ans::When(c) });
            }
        }
        let poly_comps = s.comps.iter().all(|&f| sets::poly_degree(b, f, &s.vars).is_some());
        // four real variables and a degree of 4 or more: the decomposition of the pairs does not finish within an evaluation's
        // time budget (it also ignored an interrupt that arrived inside its arithmetic), so the question stays open
        let heavy = s.vars.len() >= 2 || s.comps.iter().any(|&f| sets::poly_degree(b, f, &s.vars).is_some_and(|d| d >= 4));
        if let Some(c) = dcond.filter(|_| poly_comps && ps.is_empty() && !heavy) {
            // pairs over the real and imaginary parts
            let (x2, rn) = renamed(b, &s.vars);
            let mut hyp = vec![c, sub(cx, b, c, &rn)?];
            for &f in &s.comps {
                let f2 = sub(cx, b, f, &rn)?;
                hyp.push(b.call("equal", &[f, f2]));
            }
            let concl: Vec<T<'h>> = s.vars.iter().zip(&x2).map(|(&a, &c)| b.call("equal", &[a, c])).collect();
            let mut all = s.vars.clone();
            all.extend(x2);
            let hyp = conj(cx, b, &hyp);
            let concl = conj(cx, b, &concl);
            let body = b.call("implies", &[hyp, concl]);
            let (rv, rb, _) = sets::realify(cx, b, body, &all, &[])?;
            let rb = cx.eval(rb)?;
            if sets::polynomial_formula(b, rb, &rv) {
                let r = resolve(cx, b, b.call("for_all", &[cx.list(&rv), rb]), false)?;
                if !matches!(r, Ans::Unknown) {
                    return Ok(r);
                }
            }
        }
        if poly && dcond.is_none() && ps.is_empty() {
            let (x2, rn) = renamed(b, &s.vars);
            let mut eqs = vec![];
            for &f in &s.comps {
                let f2 = sub(cx, b, f, &rn)?;
                eqs.push(b.call("equal", &[f, f2]));
            }
            let diff: Vec<T<'h>> = s.vars.iter().zip(&x2).map(|(&a, &c)| b.call("unequal", &[a, c])).collect();
            let d = match diff.as_slice() {
                [d] => *d,
                _ => b.call("or", &diff),
            };
            let mut body = eqs;
            body.push(d);
            let mut all = s.vars.clone();
            all.extend(x2);
            let body = conj(cx, b, &body);
            let r = resolve(cx, b, b.call("exists", &[cx.list(&all), body]), true)?;
            return Ok(match r {
                Ans::Bool(v) => Ans::Bool(!v),
                _ => Ans::Unknown,
            });
        }
        // a nonzero complex period: not injective
        if s.comps.len() == 1 && s.vars.len() == 1 && dcond.is_none() && ps.is_empty() {
            let p = cx.eval(b.call("function_period", &[s.comps[0], s.vars[0], h.sym_term(h.sym("complexes"))]))?;
            if cx.as_list(p).is_some_and(|l| !l.is_empty() && l[0] != b.zero()) {
                return Ok(Ans::Bool(false));
            }

        }
        return Ok(Ans::Unknown);
    }
    if !poly {
        return Ok(Ans::Unknown);
    }
    // one variable, one component, on the line or on one interval with an interior: strictly monotonic exactly when
    // the derivative keeps one weak sign and is not 0 everywhere there (a polynomial's zeros are isolated, so f' >= 0
    // with f' not identically 0 gives a strictly increasing f: `x^3 + 3 x^2 + 3 x` with f' = 3 (x + 1)^2). Over a
    // region of several intervals one sign of f' proves nothing (the images of the pieces may overlap): the pairs.
    if s.vars.len() == 1 && s.comps.len() == 1 && dcond.map_or(Ok(true), |c| fixed_interval(cx, b, c, s.vars[0]))? {
        let x = s.vars[0];
        let d = cx.eval(b.call("d", &[s.comps[0], x]))?;
        let region = dcond.unwrap_or_else(|| cx.bool(true));
        let nonzero = b.call("exists", &[x, b.call("and", &[region, b.call("unequal", &[d, b.zero()])])]);
        let mk = |cx: &mut NativeCx<'_, '_, 'h>, rel: &str| {
            let p = b.call(rel, &[d, b.zero()]);
            let body = match dcond {
                Some(c) => b.call("implies", &[c, p]),
                None => p,
            };
            let _ = cx;
            b.call("and", &[b.call("for_all", &[x, body]), nonzero])
        };
        let up = mk(cx, "greater_equal");
        let down = mk(cx, "less_equal");
        let (u, w) = (resolve(cx, b, up, false)?, resolve(cx, b, down, false)?);
        let r = match (u, w) {
            (Ans::Bool(true), _) | (_, Ans::Bool(true)) => Ans::Bool(true),
            (Ans::When(c), Ans::Bool(false)) | (Ans::Bool(false), Ans::When(c)) => Ans::When(c),
            (Ans::When(c1), Ans::When(c2)) => Ans::When(cx.eval(b.call("or", &[c1, c2]))?),
            _ => Ans::Unknown,
        };
        if !matches!(r, Ans::Bool(true)) && ps.is_empty() {
            // not strictly monotonic: the pairs decide
        } else {
            return under_facts(cx, b, s, r);
        }
    }
    // pairs of points
    let (x2, rn) = renamed(b, &s.vars);
    let mut hyp = vec![];
    if let Some(c) = dcond {
        hyp.push(c);
        hyp.push(sub(cx, b, c, &rn)?);
    }
    for &f in &s.comps {
        let f2 = sub(cx, b, f, &rn)?;
        hyp.push(b.call("equal", &[f, f2]));
    }
    let concl: Vec<T<'h>> = s.vars.iter().zip(&x2).map(|(&a, &c)| b.call("equal", &[a, c])).collect();
    let mut all = s.vars.clone();
    all.extend(x2);
    let hyp = conj(cx, b, &hyp);
    let concl = conj(cx, b, &concl);
    let sentence = b.call("for_all", &[cx.list(&all), b.call("implies", &[hyp, concl])]);
    let r = resolve(cx, b, sentence, false)?;
    under_facts(cx, b, s, r)
}

/// Is the region of the condition `c` on `x` one interval with an interior for every value of the parameters (a
/// half-line, or `lo < x < hi` with `lo < hi` proved, each end open or closed)? Two ends with parameters may meet or
/// cross, where the region is a point or empty; such a region is not taken.
pub(super) fn fixed_interval<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, x: T<'h>) -> Res<bool> {
    let h = b.heap();
    let r = cx.eval(b.call("reduce", &[c, x, h.sym_term(h.sym("reals"))]))?;
    // (end, x above it)
    let end = |t: T<'h>| -> Option<(T<'h>, bool)> {
        let View::App { head, args: &[l, rt] } = h.view(t) else { return None };
        let name = h.sym_name(b.sym_of(head)?);
        let numeric = |e: T<'h>| is_free(b, e, x);
        let above = match name.as_str() {
            "greater" | "greater_equal" => true,
            "less" | "less_equal" => false,
            _ => return None,
        };
        if l == x && numeric(rt) {
            Some((rt, above))
        } else if rt == x && numeric(l) {
            Some((l, !above))
        } else {
            None
        }
    };
    if end(r).is_some() {
        return Ok(true);
    }
    let View::App { head, args: &[p, q] } = h.view(r) else { return Ok(false) };
    if b.sym_of(head) != Some(h.sym("and")) {
        return Ok(false);
    }
    let (Some((e1, a1)), Some((e2, a2))) = (end(p), end(q)) else { return Ok(false) };
    let (lo, hi) = match (a1, a2) {
        (true, false) => (e1, e2),
        (false, true) => (e2, e1),
        _ => return Ok(false),
    };
    let lt = cx.eval(b.call("less", &[lo, hi]))?;
    Ok(cx.is_true(lt))
}

/// Fresh copies of the variables and the rules renaming them.
fn renamed<'h>(b: &Build<'h>, vars: &[T<'h>]) -> (Vec<T<'h>>, Vec<T<'h>>) {
    let h = b.heap();
    let mut out = vec![];
    let mut rules = vec![];
    for &v in vars {
        let name = match h.view(v) {
            View::Sym(s) => h.sym_name(s).to_string(),
            _ => "v".to_string(),
        };
        let w = h.sym_term(h.sym(&format!("{name}$2")));
        out.push(w);
        rules.push(h.call(h.sym(names::RULE), &[v, w]));
    }
    (out, rules)
}
