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
//!   decomposition (`resolve`); with parameters their condition. A scalar polynomial of one variable is injective
//!   when its derivative is of one strict sign (`for_all(x, f' > 0)` or `< 0`), the condition the parameters are
//!   given.
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
    }
    Ok(r)
}

/// One real variable, when the limits do not decide (F27.9): the range of `f` ([`super::range`]) against the
/// codomain condition (`for_all(y, ycond implies range)`), or, without conditions, a continuous function whose
/// samples are unbounded above and below with growing magnitude (`x sin(x)`: the intermediate value theorem).
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
    if s.ycond.is_some() || s.cond.is_some() {
        return Ok(None);
    }
    // continuous on the whole line
    let Some(atoms) = sets::atoms(cx, b, &s.comps, &s.vars, false, sets::Mode::Disc, &s.facts)? else { return Ok(None) };
    if !atoms.is_empty() {
        return Ok(None);
    }
    // the extreme samples on wider and wider windows grow in both directions
    let mut ext: Vec<(f64, f64)> = vec![];
    for window in [1.0e3, 1.0e6, 1.0e9] {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for k in -2000..=2000 {
            cx.tick(1)?;
            if super::out_of_time() {
                return Ok(None);
            }
            let v = window * f64::from(k) / 2000.0 + 0.0123;
            if let Some(yv) = value(cx, b, f, x, v)? {
                lo = lo.min(yv);
                hi = hi.max(yv);
            }
        }
        ext.push((lo, hi));
    }
    let grows = ext.windows(2).all(|w| w[1].1 > 10.0 * w[0].1.max(1.0) && w[1].0 < 10.0 * w[0].0.min(-1.0)) && ext[2].1 > 1e5 && ext[2].0 < -1e5;
    Ok(if grows { Some(Ans::Bool(true)) } else { None })
}

/// Real singular points of the discontinuity atoms of `f` (families enumerated for `c(1)` in -3..3), sorted.
fn real_breaks<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, x: T<'h>) -> Res<Option<Vec<(f64, T<'h>)>>> {
    let h = b.heap();
    let Some(atoms) = sets::atoms(cx, b, &s.comps, &s.vars, false, sets::Mode::Disc, &s.facts)? else { return Ok(None) };
    let c1 = b.call("c", &[b.one()]);
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
    pts.dedup_by(|a, c| (a.0 - c.0).abs() < 1e-12);
    Ok(Some(pts))
}

/// The machine value of `t` at `x = v`, if real and finite.
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>, v: f64) -> Res<Option<f64>> {
    let h = b.heap();
    if super::out_of_time() {
        return Ok(None);
    }
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

/// One real variable (module comment).
fn surjective_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, y: T<'h>) -> Res<Ans<'h>> {
    let h = b.heap();
    let (f, x) = (s.comps[0], s.vars[0]);
    if s.ycond.is_some() {
        return Ok(Ans::Unknown);
    }
    let Some(pts) = real_breaks(cx, b, s, x)? else { return Ok(Ans::Unknown) };
    let inf = h.sym_term(h.sym("inf"));
    let ninf = cx.eval(b.neg(inf))?;
    // values taken at the break points themselves (sqrt(x) at 0)
    let mut attained = vec![];
    for &(_, p) in &pts {
        let at = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, p])]))?;
        let v = cx.eval(b.call("n", &[at]))?;
        match h.view(v) {
            View::Real(y) if y.is_finite() => attained.push(y),
            View::Int(k) => attained.push(k as f64),
            _ => {}
        }
    }
    let mut ends: Vec<(f64, T<'h>)> = vec![(f64::NEG_INFINITY, ninf)];
    ends.extend(pts);
    ends.push((f64::INFINITY, inf));
    // per interval: the limits at its ends and whether f is strictly monotonic on it (derivative samples)
    let df = cx.eval(b.call("d", &[f, x]))?;
    let mut images: Vec<Option<(f64, f64)>> = vec![];
    for w in ends.windows(2) {
        cx.tick(1)?;
        let (a, c) = (w[0], w[1]);
        let (lo, hi) = (if a.0.is_infinite() { c.0.min(0.0) - 30.0 } else { a.0 }, if c.0.is_infinite() { a.0.max(0.0) + 30.0 } else { c.0 });
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
        if let (Some(la), Some(lc)) = (la, lc) {
            if [la, lc].contains(&inf) && [la, lc].contains(&ninf) {
                return Ok(Ans::Bool(true));
            }
        }
        // strictly monotonic: the derivative of one strict sign at the samples
        let mut sign = 0.0f64;
        let mut mono = true;
        for k in 1..200 {
            if super::out_of_time() {
                return Ok(Ans::Unknown);
            }
            let t = lo + (hi - lo) * (k as f64 / 200.0);
            let Some(yv) = value(cx, b, df, x, t)? else { continue };
            if yv == 0.0 || sign != 0.0 && yv.signum() != sign {
                mono = false;
                break;
            }
            sign = yv.signum();
        }
        let num = |cx: &mut NativeCx<'_, '_, 'h>, l: Option<T<'h>>| -> Res<Option<f64>> {
            let Some(l) = l else { return Ok(None) };
            if l == inf {
                return Ok(Some(f64::INFINITY));
            }
            if l == ninf {
                return Ok(Some(f64::NEG_INFINITY));
            }
            let v = cx.eval(b.call("n", &[l]))?;
            Ok(match h.view(v) {
                View::Real(y) => Some(y),
                View::Int(k) => Some(k as f64),
                _ => None,
            })
        };
        let (na, nc) = (num(cx, la)?, num(cx, lc)?);
        images.push(match (na, nc, mono) {
            (Some(p), Some(q), true) => Some((p.min(q), p.max(q))),
            _ => None,
        });
    }
    // every image known: a value outside all of them (0, or an end of an image)
    if super::out_of_time() || images.is_empty() || images.iter().any(|i| i.is_none()) {
        return Ok(Ans::Unknown);
    }
    let images: Vec<(f64, f64)> = images.into_iter().flatten().collect();
    let mut tries = vec![0.0];
    for &(p, q) in &images {
        tries.extend([p, q, p - 1.0, q + 1.0].into_iter().filter(|v| v.is_finite()));
    }
    for v in tries {
        if !images.iter().any(|&(p, q)| p < v && v < q) && !attained.iter().any(|&a| (a - v).abs() <= 1e-12 * (1.0 + v.abs())) {
            let _ = y;
            return Ok(Ans::Bool(false));
        }
    }
    Ok(Ans::Unknown)
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
        let g = if is_head(b, g, mc_decide::CONDITIONAL) {
            let View::App { args, .. } = h.view(g) else { continue };
            if args.len() == 2 && is_head(b, args[1], "element") {
                cx.eval(h.call(h.sym("rewrite"), &[args[0], h.call(h.sym(names::RULE), &[b.call("c", &[b.one()]), b.zero()])]))?
            } else if args.len() == 2 && s.ycond.is_some_and(|yc| yc == args[1] || {
                let r = cx.eval_assuming(&[yc], b.call("refine", &[args[1]])).ok();
                r.is_some_and(|r| cx.is_true(r))
            }) {
                // a solution under a condition the codomain condition implies
                args[0]
            } else {
                continue;
            }
        } else {
            g
        };
        // f(g(y)) == y
        let back = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, g])]))?;
        let d = cx.eval(b.call("simplify", &[b.sub(back, ys)]))?;
        if d != b.zero() && !inverse_on(cx, b, d, ys, s.ycond)? {
            continue;
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

/// Does `d` (`f(g(y)) - y`) vanish where `ycond` holds: at no fewer than four of the points `a + b im` of a grid
/// (a, b from -2.3 to 2.3) inside `ycond`, to 30 digits?
fn inverse_on<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, d: T<'h>, y: T<'h>, ycond: Option<T<'h>>) -> Res<bool> {
    let h = b.heap();
    let Some(yc) = ycond else { return Ok(false) };
    let imu = h.sym_term(h.sym("im"));
    let mut good = 0;
    for i in -2..=2 {
        for j in -2..=2 {
            cx.tick(1)?;
            let y0 = b.add(&[b.rat(23 * i + 3, 20), b.mul(&[b.rat(23 * j + 1, 20), imu])]);
            let rule = h.call(h.sym(names::RULE), &[y, y0]);
            let inside = cx.eval(h.call(h.sym("rewrite"), &[yc, rule]))?;
            let inside = cx.eval(b.call("n", &[inside]))?;
            if !cx.is_true(inside) {
                continue;
            }
            let v = cx.eval(h.call(h.sym("rewrite"), &[d, rule]))?;
            if v == b.zero() {
                good += 1;
                continue;
            }
            let v = cx.eval(b.call("n", &[v, b.int(30)]))?;
            // zero to the working limit of the numeric evaluation (a ball around 0 below 10^-1000)
            if is_head(b, v, names::FAILURE)
                && matches!(h.view(v), View::App { args, .. } if args.first().is_some_and(|&a| a == h.sym_term(h.sym("precision_not_reached"))))
            {
                good += 1;
                continue;
            }
            let re = cx.eval(b.call("re", &[v]))?;
            let im = cx.eval(b.call("im_part", &[v]))?;
            let small = cx.eval(b.call("less", &[b.add(&[b.powi(re, 2), b.powi(im, 2)]), b.pow(b.int(10), b.int(-40))]))?;
            if !cx.is_true(small) {
                return Ok(false);
            }
            good += 1;
        }
    }
    Ok(good >= 4)
}

/// Is `v` not a value of `f`: no solution of `f == v` that checks by substitution?
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
        let small = cx.eval(b.call("less", &[b.add(&[b.powi(re, 2), b.powi(im, 2)]), b.pow(b.int(10), b.int(-30))]))?;
        if cx.is_true(small) {
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
    // a special function periodic in the variable for the parameters' facts (the table): not injective
    if s.comps.len() == 1 && s.vars.len() == 1 && !complex && s.cond.is_none() && s.ycond.is_none() {
        if let Some(hit) = super::range::table_call(cx, b, s.comps[0], s.vars[0], &s.facts)? {
            if hit.periodic(cx, b)? {
                return Ok(Ans::Bool(false));
            }
            // strictly monotonic on its domain
            if hit.arg == s.vars[0] && hit.shape(cx, b, 1)?.is_some() {
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
        // time budget (round 33: it also ignored an interrupt that arrived inside its arithmetic), so the question stays open
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
    // one variable, one component: a derivative of one strict sign
    if s.vars.len() == 1 && s.comps.len() == 1 {
        let x = s.vars[0];
        let d = cx.eval(b.call("d", &[s.comps[0], x]))?;
        let mk = |cx: &mut NativeCx<'_, '_, 'h>, rel: &str| {
            let p = b.call(rel, &[d, b.zero()]);
            let body = match dcond {
                Some(c) => b.call("implies", &[c, p]),
                None => p,
            };
            let _ = cx;
            b.call("for_all", &[x, body])
        };
        let up = mk(cx, "greater");
        let down = mk(cx, "less");
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
