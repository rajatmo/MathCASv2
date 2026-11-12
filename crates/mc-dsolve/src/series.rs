//! `asymptotic_d_solve_value(eqs, y(x), [x, x0, n])`: series solutions.
//!
//! - At an ordinary point x0 (the equation solved for the highest derivative is analytic there): the Taylor
//!   polynomial of degree n. The derivatives at x0 come from the equation and its total derivatives; values not fixed
//!   by conditions are the constants (`c(k + 1)` for `y^(k)(x0)`), and the result is grouped by constant.
//! - At a regular singular point of a linear equation of order 1 or 2 (`x0 = 0` after the shift): Frobenius series
//!   `x^r sum a_k x^k` (k <= n) for each root r of the indicial polynomial `r (r - 1) + p0 r + q0`, the larger root
//!   first, when the roots do not differ by an integer.
//! - `asymptotic_d_solve_value(eqs, y(x), x, [e, 0, n])`: regular perturbation in the parameter e,
//!   `y = sum e^k y_k`, each `y_k` from a linear equation solved by `d_solve` (conditions expanded in e).
//!
//! Literature: E. L. Ince, *Ordinary Differential Equations*, chapters XV-XVI; C. M. Bender and S. A. Orszag,
//! *Advanced Mathematical Methods for Scientists and Engineers* (1978), chapters 3 and 7.
use super::{flatten, occurrences, ode_of, problem, Cx};
use mc_calculus::common::is_free;
use mc_api::{Store, View, T};
use mc_eval::Res;

/// The highest series order accepted.
const MAX_TERMS: usize = 40;

/// `asymptotic_d_solve_value`.
pub(crate) fn asymptotic<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, plain: &[T<'h>]) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    let (eqs, spec, xs) = match plain {
        [e, s, x] => (*e, *s, *x),
        [e, s, x, p] => return perturbation(c, *e, *s, *x, *p),
        _ => return Ok(None),
    };
    // x -> x0: the leading term of the series
    if c.head_is(xs, mc_api::names::RULE) {
        let View::App { args: &[x, x0], .. } = h.view(xs) else { return Ok(None) };
        for n in 0..=8 {
            let spec2 = c.list(&[x, x0, b.int(n)]);
            let Some(v) = asymptotic(c, &[eqs, spec, spec2])? else { return Ok(None) };
            if v != b.zero() {
                return Ok(Some(v));
            }
        }
        return Ok(None);
    }
    let Some(xi) = c.items(xs) else { return Ok(None) };
    let [x, x0, nn] = xi.as_slice() else { return Ok(None) };
    let View::Int(n) = h.view(*nn) else { return Ok(None) };
    if !(0..=MAX_TERMS as i64).contains(&n) || !is_free(b, *x0, *x) || c.bad(*x0) {
        return Ok(None);
    }
    let n = n as usize;
    let mut all = vec![];
    flatten(b, eqs, &mut all);
    let y = match h.view(spec) {
        View::App { head, args: &[a] } if a == *x => head,
        View::Sym(_) => spec,
        _ => {
            // a system: one series per unknown
            let Some(items) = c.items(spec) else { return Ok(None) };
            return system(c, &all, &items, *x, *x0, n);
        }
    };
    let Some(p) = problem(c, &all, y, *x) else { return Ok(None) };
    let Some(ode) = ode_of(c, p.eq, y, *x)? else { return Ok(None) };
    if let Some(v) = taylor(c, &ode, &p.conds, *x0, n)? {
        return Ok(Some(v));
    }
    if p.conds.is_empty() {
        return frobenius(c, &ode, *x0, n);
    }
    Ok(None)
}

/// The Taylor polynomial at an ordinary point.
fn taylor<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &super::Ode<'h>, conds: &[(usize, T<'h>, T<'h>)], x0: T<'h>, n: usize) -> Res<Option<T<'h>>> {
    let b = c.b;
    let k = ode.n;
    let x = ode.x;
    // y^(k) = g(x, y, ..., y^(k-1))
    let eq = c.equal(ode.diff, b.zero());
    let Some(v) = c.solve_for(eq, ode.ys[k])? else { return Ok(None) };
    let [g] = v.as_slice() else { return Ok(None) };
    if !is_free(b, *g, ode.ys[k]) {
        return Ok(None);
    }
    // the values at x0: from the conditions, the rest constants
    let mut vals = vec![];
    let mut next = 1;
    for j in 0..k {
        match conds.iter().find(|cd| cd.0 == j) {
            Some(cd) => {
                if c.simp(b.sub(cd.1, x0))? != b.zero() {
                    return Ok(None);
                }
                vals.push(cd.2);
            }
            None => {
                vals.push(c.constant(next));
                next += 1;
            }
        }
    }
    if conds.iter().any(|cd| cd.0 >= k) {
        return Ok(None);
    }
    let at = |c: &mut Cx<'_, '_, '_, '_, 'h>, e: T<'h>, vals: &[T<'h>]| -> Res<T<'h>> {
        let mut rules = vec![c.rule(x, x0)];
        for (j, &v) in vals.iter().enumerate() {
            rules.push(c.rule(ode.ys[j], v));
        }
        let r = c.subst(e, &rules)?;
        c.simp(r)
    };
    // an ordinary point: g finite at x0
    let g0 = at(c, *g, &vals)?;
    if c.bad(g0) {
        return Ok(None);
    }
    let mut coefs = vals.clone();
    let mut cur = *g;
    for m in k..=n {
        if c.late() {
            return Ok(None);
        }
        if m > k {
            // the total derivative, y^(k) replaced by g
            let mut terms = vec![c.d(cur, x)?];
            for j in 0..k {
                if !is_free(b, cur, ode.ys[j]) {
                    let dj = c.d(cur, ode.ys[j])?;
                    let nxt = if j + 1 < k { ode.ys[j + 1] } else { *g };
                    terms.push(b.mul(&[dj, nxt]));
                }
            }
            let t = c.eval(b.add(&terms))?;
            cur = c.together(t)?;
        }
        let v = at(c, cur, &coefs[..k])?;
        if c.bad(v) {
            return Ok(None);
        }
        coefs.push(v);
    }
    coefs.truncate(n + 1);
    // the polynomial, grouped by constant
    let dx = c.eval(b.sub(x, x0))?;
    let mut terms = vec![];
    for (m, &v) in coefs.iter().enumerate() {
        let f = c.ev("factorial", &[b.int(m as i64)])?;
        let cm = c.simp(b.div(v, f))?;
        terms.push((m, cm));
    }
    let consts = c.constants_in(c.list(&coefs), k);
    let poly = |c: &mut Cx<'_, '_, '_, '_, 'h>, part: &[(usize, T<'h>)]| -> Res<T<'h>> {
        let ts: Vec<T<'h>> = part.iter().map(|&(m, v)| b.mul(&[v, b.powi(dx, m as i64)])).collect();
        c.eval(b.add(&ts))
    };
    if consts.is_empty() {
        return Ok(Some(poly(c, &terms)?));
    }
    let mut out = vec![];
    let zero_rules: Vec<T<'h>> = consts.iter().map(|&k| c.rule(k, b.zero())).collect();
    for &k0 in &consts {
        let mut part = vec![];
        for &(m, v) in &terms {
            let e = c.expand(v)?;
            let d = c.d(e, k0)?;
            let d = c.simp(d)?;
            if !c.free_all(d, &consts) {
                return Ok(None);
            }
            part.push((m, d));
        }
        let p = poly(c, &part)?;
        out.push(b.mul(&[p, k0]));
    }
    let mut rest = vec![];
    for &(m, v) in &terms {
        let r = c.subst(v, &zero_rules)?;
        rest.push((m, c.simp(r)?));
    }
    out.push(poly(c, &rest)?);
    Ok(Some(c.eval(b.add(&out))?))
}

/// Frobenius series at a regular singular point (linear, order 1 or 2, homogeneous).
fn frobenius<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &super::Ode<'h>, x0: T<'h>, n: usize) -> Res<Option<T<'h>>> {
    let b = c.b;
    let x = ode.x;
    let Some((coefs, f)) = super::linear::coefficients(c, ode)? else { return Ok(None) };
    if f != b.zero() || ode.n > 2 {
        return Ok(None);
    }
    // in t = x - x0: x^k p_k(x) form; p = t c1/c2 and q = t^2 c0/c2 (order 2), or p = t c0/c1 (order 1)
    let t = c.sym("d_solve·t·frob");
    let shift = |c: &mut Cx<'_, '_, '_, '_, 'h>, e: T<'h>| -> Res<T<'h>> {
        let r = c.subst(e, &[c.rule(x, b.add(&[t, x0]))])?;
        c.simp(r)
    };
    let (p, q) = if ode.n == 2 {
        let p = c.eval(b.div(b.mul(&[b.sub(x, x0), coefs[1]]), coefs[2]))?;
        let q = c.eval(b.div(b.mul(&[b.powi(b.sub(x, x0), 2), coefs[0]]), coefs[2]))?;
        (shift(c, p)?, shift(c, q)?)
    } else {
        let q = c.eval(b.div(b.mul(&[b.sub(x, x0), coefs[0]]), coefs[1]))?;
        (b.zero(), shift(c, q)?)
    };
    // their Taylor coefficients at 0
    let series_coefs = |c: &mut Cx<'_, '_, '_, '_, 'h>, e: T<'h>| -> Res<Option<Vec<T<'h>>>> {
        let s = c.ev("series", &[e, t, b.zero(), b.int(n as i64)])?;
        if c.stuck(s, "series") || c.bad(s) {
            return Ok(None);
        }
        let nrm = c.ev("normal", &[s])?;
        let mut out = vec![];
        for k in 0..=n {
            let ck = c.ev("coefficient", &[nrm, t, b.int(k as i64)])?;
            let ck = c.simp(ck)?;
            if !is_free(b, ck, t) {
                return Ok(None);
            }
            out.push(ck);
        }
        Ok(Some(out))
    };
    // a regular singular point: p and q analytic at 0
    for e in [p, q] {
        let at = c.subst(e, &[c.rule(t, b.zero())])?;
        if c.bad(at) {
            let l = c.ev("limit", &[e, c.rule(t, b.zero())])?;
            if c.bad(l) || c.stuck(l, "limit") {
                return Ok(None);
            }
        }
    }
    let Some(pk) = series_coefs(c, p)? else { return Ok(None) };
    let Some(qk) = series_coefs(c, q)? else { return Ok(None) };
    // the indicial polynomial F(r) = r (r - 1) + p0 r + q0 (order 2), r + q0 (order 1)
    let r = c.sym("d_solve·r");
    let fr = |c: &mut Cx<'_, '_, '_, '_, 'h>, rr: T<'h>| -> Res<T<'h>> {
        let e = if ode.n == 2 { b.add(&[b.mul(&[rr, b.sub(rr, b.one())]), b.mul(&[pk[0], rr]), qk[0]]) } else { b.add(&[rr, qk[0]]) };
        let e = c.eval(e)?;
        c.expand(e)
    };
    let ind = fr(c, r)?;
    let Some(rts) = c.solve_for(c.equal(ind, b.zero()), r)? else { return Ok(None) };
    // a double root (order 2): the second solution has a logarithm
    let double = ode.n == 2 && rts.len() == 1;
    if rts.len() != ode.n && !double {
        return Ok(None);
    }
    // the larger root first; roots differing by an integer are not handled
    let mut rv = vec![];
    for &rt in &rts {
        let Some(v) = super::verify::magnitude(c, rt)? else { return Ok(None) };
        let nv = c.ev("n", &[rt])?;
        let re = match b.ar.num(nv) {
            Some(mc_arith::Num::Real(x)) => x.to_f64(),
            _ => return Ok(None),
        };
        let _ = v;
        rv.push((re, rt));
    }
    rv.sort_by(|a, bb| bb.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    // roots that differ by an integer N (or a double root, N = 0): the second solution is
    // `C y1 log(t) + t^r2 sum b_k t^k` (the method of Frobenius, Ince chapter XVI; Bender and Orszag 3.3)
    let mut resonance: Option<usize> = double.then_some(0);
    if rv.len() == 2 {
        let d = rv[0].0 - rv[1].0;
        if (d - d.round()).abs() < 1e-12 {
            resonance = Some(d.round() as usize);
        }
    }
    if double {
        rv.push(rv[0]);
    }
    let mut parts = vec![];
    let mut first: Vec<T<'h>> = vec![];
    for (i, &(_, rt)) in rv.iter().enumerate() {
        if i == 1 {
            if let Some(nn) = resonance {
                let dx = c.eval(b.sub(x, x0))?;
                let r1 = rv[0].1;
                let Some(second) = log_solution(c, ode.n, &pk, &qk, &fr, r1, rt, nn, &first, n, dx)? else { return Ok(None) };
                parts.push(b.mul(&[second, c.constant(2)]));
                continue;
            }
        }
        let mut a = vec![b.one()];
        for k in 1..=n {
            if c.late() {
                return Ok(None);
            }
            let rk = c.eval(b.add(&[rt, b.int(k as i64)]))?;
            let fk = fr(c, rk)?;
            let fk = c.simp(fk)?;
            if fk == b.zero() {
                // a resonance below the first root cannot happen for the larger root; give up
                return Ok(None);
            }
            let mut s = vec![];
            for (j, &aj) in a.iter().enumerate() {
                let rj = b.add(&[rt, b.int(j as i64)]);
                let term = if ode.n == 2 { b.add(&[b.mul(&[rj, pk[k - j]]), qk[k - j]]) } else { qk[k - j] };
                s.push(b.mul(&[aj, term]));
            }
            let ak = c.simp(b.neg(b.div(b.add(&s), fk)))?;
            a.push(ak);
        }
        if i == 0 {
            first = a.clone();
        }
        let dx = c.eval(b.sub(x, x0))?;
        let ts: Vec<T<'h>> = a.iter().enumerate().map(|(k, &ak)| b.mul(&[ak, b.powi(dx, k as i64)])).collect();
        let ser = c.eval(b.add(&ts))?;
        let pre = c.eval(b.pow(dx, rt))?;
        parts.push(b.mul(&[pre, ser, c.constant(i + 1)]));
    }
    Ok(Some(c.eval(b.add(&parts))?))
}

/// The second Frobenius solution when the indicial roots `r1 >= r2` differ by the integer `nn >= 0`:
/// `C y1 log(dx) + dx^r2 sum_k b_k dx^k`, `y1 = dx^r1 sum a_k dx^k` (`a` given, `a_0 = 1`). Writing the equation as
/// `t^2 y'' + t p y' + q y = 0`, `L[y1 log t] = 2 t y1' + (p - 1) y1`, so the coefficient of `t^(r1 + k)` in it is
/// `(2 (r1 + k) - 1 + p_0) a_k + sum_(j<k) a_j p_(k-j)`; `b_0 = 1`, `b_nn = 0`, and for `k >= 1`
/// `F(r2 + k) b_k = -sum_(j<k) b_j ((r2 + j) p_(k-j) + q_(k-j)) - C [log term at order k - nn]`, where at `k = nn`
/// the left side vanishes and fixes `C = -S_nn/nn` (for `nn = 0`, `C = 1` normalises).
#[allow(clippy::too_many_arguments)]
fn log_solution<'h>(
    c: &mut Cx<'_, '_, '_, '_, 'h>,
    order: usize,
    pk: &[T<'h>],
    qk: &[T<'h>],
    fr: &dyn Fn(&mut Cx<'_, '_, '_, '_, 'h>, T<'h>) -> Res<T<'h>>,
    r1: T<'h>,
    r2: T<'h>,
    nn: usize,
    a: &[T<'h>],
    n: usize,
    dx: T<'h>,
) -> Res<Option<T<'h>>> {
    let b = c.b;
    if order != 2 || a.is_empty() {
        return Ok(None);
    }
    // L[y1 log t] at order k
    let lg = |c: &mut Cx<'_, '_, '_, '_, 'h>, k: usize| -> Res<T<'h>> {
        let Some(&ak) = a.get(k) else { return Ok(b.zero()) };
        let mut terms = vec![b.mul(&[b.sub(b.add(&[b.mul(&[b.int(2), r1]), b.int(2 * k as i64)]), b.one()), ak]), b.mul(&[pk[0], ak])];
        for j in 0..k {
            if let Some(&aj) = a.get(j) {
                terms.push(b.mul(&[aj, pk[k - j]]));
            }
        }
        c.simp(b.add(&terms))
    };
    let src = |c: &mut Cx<'_, '_, '_, '_, 'h>, bs: &[T<'h>], k: usize| -> Res<T<'h>> {
        let mut s = vec![];
        for (j, &bj) in bs.iter().enumerate().take(k) {
            let rj = b.add(&[r2, b.int(j as i64)]);
            s.push(b.mul(&[bj, b.add(&[b.mul(&[rj, pk[k - j]]), qk[k - j]])]));
        }
        c.simp(b.add(&s))
    };
    let mut bs = vec![b.one()];
    let mut cc = if nn == 0 { b.one() } else { b.zero() };
    for k in 1..=n {
        if c.late() {
            return Ok(None);
        }
        let s = src(c, &bs, k)?;
        if k == nn {
            // F(r2 + nn) = 0: the log coefficient is fixed, b_nn is free (taken 0)
            cc = c.simp(b.neg(b.div(s, b.int(nn as i64))))?;
            bs.push(b.zero());
            continue;
        }
        let logterm = if k >= nn && nn > 0 { b.mul(&[cc, lg(c, k - nn)?]) } else if nn == 0 { b.mul(&[cc, lg(c, k)?]) } else { b.zero() };
        let rk = c.eval(b.add(&[r2, b.int(k as i64)]))?;
        let fk = fr(c, rk)?;
        let fk = c.simp(fk)?;
        if fk == b.zero() {
            return Ok(None);
        }
        let bk = c.simp(b.neg(b.div(b.add(&[s, logterm]), fk)))?;
        bs.push(bk);
    }
    let pre1 = c.eval(b.pow(dx, r1))?;
    let ts1: Vec<T<'h>> = a.iter().enumerate().map(|(k, &ak)| b.mul(&[ak, b.powi(dx, k as i64)])).collect();
    let y1 = c.eval(b.mul(&[pre1, b.add(&ts1)]))?;
    let pre2 = c.eval(b.pow(dx, r2))?;
    let ts2: Vec<T<'h>> = bs.iter().enumerate().map(|(k, &bk)| b.mul(&[bk, b.powi(dx, k as i64)])).collect();
    let w = c.eval(b.mul(&[pre2, b.add(&ts2)]))?;
    let log = b.mul(&[cc, y1, b.call("log", &[dx])]);
    Ok(Some(c.eval(b.add(&[log, w]))?))
}

/// A system at an ordinary point: the Taylor polynomials of every unknown (conditions at x0 for all of them).
fn system<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, all: &[T<'h>], funs: &[T<'h>], x: T<'h>, x0: T<'h>, n: usize) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    let mut ys = vec![];
    for &f in funs {
        match h.view(f) {
            View::App { head, args: &[a] } if a == x => ys.push(head),
            View::Sym(_) => ys.push(f),
            _ => return Ok(None),
        }
    }
    // first-order explicit systems y_i' = g_i with values at x0
    let m = ys.len();
    let syms: Vec<T<'h>> = (0..m).map(|i| c.sym(&format!("d_solve·z{i}"))).collect();
    let dsyms: Vec<T<'h>> = (0..m).map(|i| c.sym(&format!("d_solve·dz{i}"))).collect();
    let mut exprs = vec![];
    let mut vals = vec![None; m];
    for &e in all {
        let View::App { args: &[l, r], .. } = h.view(e) else { return Ok(None) };
        let mut pairs = vec![];
        let mut at_x = true;
        for (i, &y) in ys.iter().enumerate() {
            let mut occ = vec![];
            occurrences(b, e, y, &mut occ);
            for o in occ {
                if o.1 != x {
                    at_x = false;
                    if o.0 != 0 {
                        return Ok(None);
                    }
                    // y_i(x0) == v
                    let v = if super::occurrence(b, l, y).is_some() { r } else { l };
                    if c.simp(b.sub(o.1, x0))? != b.zero() {
                        return Ok(None);
                    }
                    vals[i] = Some(v);
                    continue;
                }
                if o.0 > 1 {
                    return Ok(None);
                }
                pairs.push((o.2, if o.0 == 0 { syms[i] } else { dsyms[i] }));
            }
        }
        if at_x {
            exprs.push(c.eval(super::replace_terms(b, b.sub(l, r), &pairs))?);
        }
    }
    let eqs: Vec<T<'h>> = exprs.iter().map(|&e| c.equal(e, b.zero())).collect();
    let s = c.ev("solve", &[c.list(&eqs), c.list(&dsyms)])?;
    let Some(first) = c.items(s).and_then(|v| v.first().copied()) else { return Ok(None) };
    let Some(rules) = c.items(first) else { return Ok(None) };
    let mut g = vec![];
    for &d in &dsyms {
        let Some(v) = rules.iter().find_map(|&r| match h.view(r) {
            View::App { args: &[k, v], .. } if k == d => Some(v),
            _ => None,
        }) else {
            return Ok(None);
        };
        g.push(v);
    }
    let mut next = 1;
    let v0: Vec<T<'h>> = vals
        .iter()
        .map(|v| {
            v.unwrap_or_else(|| {
                next += 1;
                c.constant(next - 1)
            })
        })
        .collect();
    let mut cur: Vec<T<'h>> = syms.clone();
    let mut coefs: Vec<Vec<T<'h>>> = vec![vec![]; m];
    let mut point = vec![c.rule(x, x0)];
    for (i, &s) in syms.iter().enumerate() {
        point.push(c.rule(s, v0[i]));
    }
    for k in 0..=n {
        if c.late() {
            return Ok(None);
        }
        if k > 0 {
            for e in cur.iter_mut() {
                let mut terms = vec![c.d(*e, x)?];
                for (i, &s) in syms.iter().enumerate() {
                    if !is_free(b, *e, s) {
                        let di = c.d(*e, s)?;
                        terms.push(b.mul(&[di, g[i]]));
                    }
                }
                let t = c.eval(b.add(&terms))?;
                *e = c.expand(t)?;
            }
        }
        for (i, &e) in cur.iter().enumerate() {
            let v = c.subst(e, &point)?;
            let f = c.ev("factorial", &[b.int(k as i64)])?;
            coefs[i].push(c.simp(b.div(v, f))?);
        }
    }
    let dx = c.eval(b.sub(x, x0))?;
    let mut out = vec![];
    for cs in coefs {
        let ts: Vec<T<'h>> = cs.iter().enumerate().map(|(k, &v)| b.mul(&[v, b.powi(dx, k as i64)])).collect();
        out.push(c.eval(b.add(&ts))?);
    }
    Ok(Some(c.list(&out)))
}

/// Regular perturbation `y = sum e^k y_k` up to order n.
fn perturbation<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, eqs: T<'h>, spec: T<'h>, x: T<'h>, p: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    let Some(pi) = c.items(p) else { return Ok(None) };
    let [e, e0, nn] = pi.as_slice() else { return Ok(None) };
    let View::Int(n) = h.view(*nn) else { return Ok(None) };
    let param = matches!(h.view(*e), View::Sym(s) if !c.cx.protected(s));
    if !param || *e0 != b.zero() || !(0..=8).contains(&n) || !matches!(h.view(x), View::Sym(_)) {
        return Ok(None);
    }
    let n = n as usize;
    let y = match h.view(spec) {
        View::App { head, args: &[a] } if a == x => head,
        _ => return Ok(None),
    };
    let mut all = vec![];
    flatten(b, eqs, &mut all);
    let Some(pb) = problem(c, &all, y, x) else { return Ok(None) };
    // the unknown pieces u_k(x)
    let us: Vec<T<'h>> = (0..=n).map(|k| c.sym(&format!("d_solve·u{k}"))).collect();
    let ux: Vec<T<'h>> = us.iter().map(|&u| h.app(u, &[x])).collect();
    let sum: Vec<T<'h>> = ux.iter().enumerate().map(|(k, &u)| b.mul(&[b.powi(*e, k as i64), u])).collect();
    let yfn = b.call("fn", &[c.list(&[x]), b.add(&sum)]);
    let View::App { args: &[l, r], .. } = h.view(pb.eq) else { return Ok(None) };
    let lhs = c.subst(b.sub(l, r), &[c.rule(y, yfn)])?;
    let lhs = c.expand(lhs)?;
    let mut sols: Vec<T<'h>> = vec![];
    for k in 0..=n {
        if c.late() {
            return Ok(None);
        }
        let ck = c.ev("coefficient", &[lhs, *e, b.int(k as i64)])?;
        // the known pieces
        let known: Vec<T<'h>> = (0..k).map(|j| c.rule(us[j], b.call("fn", &[c.list(&[x]), sols[j]]))).collect();
        let ck = if known.is_empty() { ck } else { c.subst(ck, &known)? };
        let ck = c.expand(ck)?;
        let mut eqk = vec![c.equal(ck, b.zero())];
        for &(o, x0, v) in &pb.conds {
            let vk = c.ev("series_coefficient", &[v, c.list(&[*e, b.zero(), b.int(k as i64)])])?;
            let vk = if c.stuck(vk, "series_coefficient") {
                if k == 0 {
                    c.subst(v, &[c.rule(*e, b.zero())])?
                } else {
                    b.zero()
                }
            } else {
                vk
            };
            let occ = if o == 0 { h.app(us[k], &[x0]) } else { h.app(b.derivative(&[o as i64], us[k]), &[x0]) };
            eqk.push(c.equal(occ, vk));
        }
        let Some(out) = super::single(c, &eqk, us[k], x)? else { return Ok(None) };
        let super::Out::Sols(s) = out else { return Ok(None) };
        let [s] = s.as_slice() else { return Ok(None) };
        sols.push(s[0]);
    }
    let terms: Vec<T<'h>> = sols.iter().enumerate().map(|(k, &s)| b.mul(&[b.powi(*e, k as i64), s])).collect();
    let r = c.eval(b.add(&terms))?;
    Ok(Some(r))
}
