//! Systems of ordinary differential equations for several unknown functions of one variable.
//!
//! Algebraic equations (no derivatives) are solved for one unknown first and substituted into the others (with its
//! derivatives from the total derivative). One equation left: the scalar solver. Otherwise the system must be
//! linear with constant coefficients: it is written as `X' = A X + f(x)` (higher derivatives as extra components)
//! and solved by the matrix exponential, `X = exp(A x) c + exp(A x) integrate(exp(-A x) f)`. `exp(A x)` is built
//! without eigenvectors or a Jordan form: by Cayley-Hamilton `exp(A x) = sum_k phi_k(x) A^k`, where the `phi_k`
//! solve the scalar equation of the characteristic polynomial with `phi_k^(j)(0) = delta_jk`, taken from the real
//! basis of [`super::linear`] (so complex eigenvalues give cosines and sines). The constants are the components of
//! `X(0)`. Conditions fix the constants; every solution is substituted into every equation before it is returned.
use super::{general, occurrences, verify, Cx, Ode, Out, Sol, MAX_ORDER};
use mc_calculus::common::is_free;
use mc_api::{Store, View, T};
use mc_eval::Res;

/// A condition `f_i^(k)(x0) == v`.
#[derive(Clone, Copy)]
struct Cond<'h> {
    i: usize,
    k: usize,
    x0: T<'h>,
    v: T<'h>,
}

/// Solve the system `all` for `funs` in `x`.
pub(crate) fn solve<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, all: &[T<'h>], funs: &[T<'h>], x: T<'h>) -> Res<Option<Out<'h>>> {
    let b = c.b;
    let h = b.heap();
    let m = funs.len();
    // conditions and equations
    let mut eqs = vec![];
    let mut conds = vec![];
    let mut orders = vec![0usize; m];
    for &e in all {
        let View::App { args: &[l, r], .. } = h.view(e) else { return Ok(None) };
        let mut occ = vec![];
        for (i, &f) in funs.iter().enumerate() {
            let mut o = vec![];
            occurrences(b, e, f, &mut o);
            occ.extend(o.into_iter().map(|(k, a, t)| (i, k, a, t)));
        }
        if occ.is_empty() {
            return Ok(None);
        }
        if occ.iter().all(|o| o.2 == x) {
            for o in &occ {
                orders[o.0] = orders[o.0].max(o.1);
            }
            eqs.push((e, occ));
            continue;
        }
        // a condition: one side an occurrence at a point
        let side = |t: T<'h>| funs.iter().enumerate().find_map(|(i, &f)| super::occurrence(b, t, f).map(|(k, a)| (i, k, a)));
        let (o, v) = match (side(l), side(r)) {
            (Some(o), _) if funs.iter().all(|&f| is_free(b, r, f)) => (o, r),
            (_, Some(o)) if funs.iter().all(|&f| is_free(b, l, f)) => (o, l),
            _ => return Ok(None),
        };
        if !is_free(b, o.2, x) {
            return Ok(None);
        }
        conds.push(Cond { i: o.0, k: o.1, x0: o.2, v });
    }
    if eqs.is_empty() || orders.iter().any(|&n| n > MAX_ORDER) {
        return Ok(None);
    }
    // the symbols Y[i][k]
    let nmax = orders.iter().copied().max().unwrap_or(0) + 1;
    let ysym: Vec<Vec<T<'h>>> = (0..m).map(|i| (0..=nmax).map(|k| c.sym(&format!("d_solve·y{i}·{k}"))).collect()).collect();
    let mut exprs = vec![];
    for (e, occ) in &eqs {
        let pairs: Vec<(T<'h>, T<'h>)> = occ.iter().map(|o| (o.3, ysym[o.0][o.1])).collect();
        let View::App { args: &[l, r], .. } = h.view(*e) else { return Ok(None) };
        let d = c.eval(super::replace_terms(b, b.sub(l, r), &pairs))?;
        exprs.push(c.expand(d)?);
    }
    // eliminate algebraic equations: an unknown in terms of the others
    let mut elim: Vec<(usize, T<'h>)> = vec![];
    loop {
        let alg = exprs.iter().position(|&e| (0..m).all(|i| (1..=nmax).all(|k| is_free(b, e, ysym[i][k]))));
        let Some(pos) = alg else { break };
        let e = exprs.remove(pos);
        let Some(i) = (0..m).rev().find(|&i| !is_free(b, e, ysym[i][0]) && !elim.iter().any(|el| el.0 == i)) else { return Ok(None) };
        let eq = c.equal(e, b.zero());
        let Some(vals) = c.solve_for(eq, ysym[i][0])? else { return Ok(None) };
        let [val] = vals.as_slice() else { return Ok(None) };
        // its derivatives by the total derivative
        let mut ders = vec![*val];
        for k in 1..=nmax {
            let prev = ders[k - 1];
            ders.push(total(c, prev, x, &ysym, nmax)?);
        }
        let rules: Vec<T<'h>> = (0..=nmax).map(|k| c.rule(ysym[i][k], ders[k])).collect();
        for e in exprs.iter_mut() {
            let s = c.subst(*e, &rules)?;
            *e = c.expand(s)?;
        }
        for el in elim.iter_mut() {
            el.1 = c.subst(el.1, &rules)?;
        }
        elim.push((i, *val));
        orders[i] = 0;
        if exprs.is_empty() {
            return Ok(None);
        }
    }
    let rest: Vec<usize> = (0..m).filter(|i| !elim.iter().any(|el| el.0 == *i)).collect();
    // orders after the elimination
    for &i in &rest {
        orders[i] = (1..=nmax).rev().find(|&k| exprs.iter().any(|&e| !is_free(b, e, ysym[i][k]))).unwrap_or(0);
    }
    let sols: Vec<Vec<T<'h>>> = if rest.len() == 1 && exprs.len() == 1 {
        let i = rest[0];
        let n = orders[i];
        if n == 0 {
            return Ok(None);
        }
        let ode = Ode { diff: exprs[0], ys: ysym[i][..=n].to_vec(), x, n };
        let Some(gs) = general(c, &ode)? else { return Ok(None) };
        let mut out = vec![];
        for g in gs {
            let Sol::E(g) = g else { return Ok(None) };
            let mut v = vec![b.zero(); m];
            v[i] = g;
            out.push(v);
        }
        out
    } else {
        let Some(v) = linear_system(c, &exprs, &rest, &orders, &ysym, x)? else { return Ok(None) };
        vec![v]
    };
    // the eliminated unknowns, from the derivatives of the others
    let mut full = vec![];
    for mut v in sols {
        let mut rules = vec![];
        for &i in &rest {
            let mut dk = v[i];
            for (k, &yk) in ysym[i].iter().enumerate().take(nmax + 1) {
                if k > 0 {
                    dk = c.d(dk, x)?;
                }
                rules.push(c.rule(yk, dk));
            }
        }
        for &(i, e) in elim.iter().rev() {
            let s = c.subst(e, &rules)?;
            v[i] = c.simp(s)?;
        }
        full.push(v);
    }
    // conditions
    let mut out = vec![];
    for v in full {
        if conds.is_empty() {
            out.push(v);
            continue;
        }
        let mut ceqs = vec![];
        for cd in &conds {
            let dk = c.dn(v[cd.i], x, cd.k)?;
            let at = c.subst(dk, &[c.rule(x, cd.x0)])?;
            ceqs.push(c.equal(at, cd.v));
        }
        let all_v = c.list(&v);
        let used = c.constants_in(all_v, 2 * MAX_ORDER);
        if used.is_empty() {
            out.push(v);
            continue;
        }
        let s = c.ev("solve", &[c.list(&ceqs), c.list(&used)])?;
        let Some(items) = c.items(s) else { return Ok(None) };
        for rs in items {
            let Some(rules) = c.items(rs) else { return Ok(None) };
            let mut w = vec![];
            for &vi in &v {
                let s = c.subst(vi, &rules)?;
                w.push(super::tidy(c, s, x)?);
            }
            out.push(w);
        }
    }
    // verification: every solution in every equation
    let mut kept = vec![];
    for v in out {
        let mut ok = true;
        for (e, occ) in &eqs {
            let mut pairs = vec![];
            for o in occ {
                let dk = c.dn(v[o.0], x, o.1)?;
                pairs.push((o.3, dk));
            }
            let View::App { args: &[l, r], .. } = h.view(*e) else { return Ok(None) };
            let res = c.eval(super::replace_terms(b, b.sub(l, r), &pairs))?;
            if verify::zero(c, res)? == verify::Verdict::Fail {
                ok = false;
                break;
            }
        }
        if ok {
            kept.push(v);
        }
    }
    if kept.is_empty() {
        return Ok(None);
    }
    Ok(Some(Out::Sols(kept)))
}

/// The total derivative of `e` (in x and the symbols `ys[i][k]`, `k < n`).
fn total<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, e: T<'h>, x: T<'h>, ys: &[Vec<T<'h>>], n: usize) -> Res<T<'h>> {
    let b = c.b;
    let mut terms = vec![c.d(e, x)?];
    for row in ys {
        for k in 0..n {
            if !is_free(b, e, row[k]) {
                let dk = c.d(e, row[k])?;
                terms.push(b.mul(&[dk, row[k + 1]]));
            }
        }
    }
    let t = c.eval(b.add(&terms))?;
    c.expand(t)
}

/// A linear system with constant coefficients: the values of every unknown in `rest` (others 0).
fn linear_system<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, exprs: &[T<'h>], rest: &[usize], orders: &[usize], ys: &[Vec<T<'h>>], x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    if rest.iter().any(|&i| orders[i] == 0) || exprs.len() != rest.len() {
        return Ok(None);
    }
    // the highest derivatives solved for
    let tops: Vec<T<'h>> = rest.iter().map(|&i| ys[i][orders[i]]).collect();
    let eqs: Vec<T<'h>> = exprs.iter().map(|&e| c.equal(e, b.zero())).collect();
    let s = c.ev("solve", &[c.list(&eqs), c.list(&tops)])?;
    let Some(first) = c.items(s).and_then(|v| v.first().copied()) else { return Ok(None) };
    let Some(rules) = c.items(first) else { return Ok(None) };
    let h = b.heap();
    let mut top_vals = vec![];
    for &t in &tops {
        let Some(v) = rules.iter().find_map(|&r| match h.view(r) {
            View::App { args: &[k, v], .. } if k == t => Some(v),
            _ => None,
        }) else {
            return Ok(None);
        };
        top_vals.push(c.generic(v));
    }
    // the state: (i, k) for k < orders[i]
    let state: Vec<(usize, usize)> = rest.iter().flat_map(|&i| (0..orders[i]).map(move |k| (i, k))).collect();
    let nn = state.len();
    if nn > 8 {
        return Ok(None);
    }
    let svars: Vec<T<'h>> = state.iter().map(|&(i, k)| ys[i][k]).collect();
    let mut a = vec![vec![b.zero(); nn]; nn];
    let mut f = vec![b.zero(); nn];
    for (row, &(i, k)) in state.iter().enumerate() {
        let rhs = if k + 1 < orders[i] {
            ys[i][k + 1]
        } else {
            let j = rest.iter().position(|&r| r == i).unwrap_or(0);
            top_vals[j]
        };
        let rhs = c.expand(rhs)?;
        for (col, &v) in svars.iter().enumerate() {
            let d = c.d(rhs, v)?;
            let d = c.simp(d)?;
            if !is_free(b, d, x) || !c.free_all(d, &svars) {
                return Ok(None);
            }
            a[row][col] = d;
        }
        let zeros: Vec<T<'h>> = svars.iter().map(|&v| c.rule(v, b.zero())).collect();
        let r0 = c.subst(rhs, &zeros)?;
        f[row] = c.simp(r0)?;
    }
    let Some(e) = exp_matrix(c, &a, x)? else { return Ok(None) };
    // X = E c + E integrate(E(-x) f)
    let mut xs = vec![];
    let forced = f.iter().any(|&t| t != b.zero());
    let mut inner = vec![b.zero(); nn];
    if forced {
        let mut em = vec![];
        for row in &e {
            let mut r = vec![];
            for &t in row {
                r.push(c.subst(t, &[c.rule(x, b.neg(x))])?);
            }
            em.push(r);
        }
        for (i, row) in em.iter().enumerate() {
            let s: Vec<T<'h>> = row.iter().zip(&f).map(|(&p, &q)| b.mul(&[p, q])).collect();
            let s = c.eval(b.add(&s))?;
            let s = c.simp(s)?;
            let Some(is) = c.integrate(s, x)? else { return Ok(None) };
            inner[i] = is;
        }
    }
    for row in &e {
        let terms: Vec<T<'h>> = row.iter().enumerate().map(|(j, &p)| b.mul(&[p, b.add(&[c.constant(j + 1), inner[j]])])).collect();
        let s = c.eval(b.add(&terms))?;
        xs.push(s);
    }
    let mut out = vec![b.zero(); ys.len()];
    for (row, &(i, k)) in state.iter().enumerate() {
        if k == 0 {
            out[i] = if forced { c.simp(xs[row])? } else { xs[row] };
        }
    }
    Ok(Some(out))
}

/// `exp(A x)` by Cayley-Hamilton over the real basis of the characteristic polynomial.
pub(crate) fn exp_matrix<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, a: &[Vec<T<'h>>], x: T<'h>) -> Res<Option<Vec<Vec<T<'h>>>>> {
    let b = c.b;
    let n = a.len();
    let mvar = c.sym("d_solve·m");
    // det(m I - A)
    let rows: Vec<T<'h>> = (0..n)
        .map(|i| {
            let r: Vec<T<'h>> = (0..n).map(|j| if i == j { b.sub(mvar, a[i][j]) } else { b.neg(a[i][j]) }).collect();
            c.list(&r)
        })
        .collect();
    let m = c.list(&rows);
    let p = c.ev("det", &[m])?;
    let p = c.expand(p)?;
    let cl = c.ev("coefficient_list", &[p, mvar])?;
    let Some(coefs) = c.items(cl) else { return Ok(None) };
    if coefs.len() != n + 1 {
        return Ok(None);
    }
    let Some(rts) = super::linear::char_roots(c, &coefs)? else { return Ok(None) };
    let basis = super::linear::exp_basis(c, &rts, x, false)?;
    if basis.len() != n {
        return Ok(None);
    }
    // W0[j][i] = basis_i^(j)(0); M = W0^-1; phi_k = sum_i M[i][k] basis_i
    let mut w0 = vec![];
    let mut ders = basis.clone();
    for j in 0..n {
        if j > 0 {
            for d in ders.iter_mut() {
                *d = c.d(*d, x)?;
            }
        }
        let mut row = vec![];
        for &d in &ders {
            row.push(c.subst(d, &[c.rule(x, b.zero())])?);
        }
        w0.push(c.list(&row));
    }
    let inv = c.ev("inverse", &[c.list(&w0)])?;
    let Some(inv_rows) = c.items(inv) else { return Ok(None) };
    let mut minv = vec![];
    for r in inv_rows {
        let Some(r) = c.items(r) else { return Ok(None) };
        if r.len() != n {
            return Ok(None);
        }
        minv.push(r);
    }
    if minv.len() != n {
        return Ok(None);
    }
    let mut phi = vec![];
    #[allow(clippy::needless_range_loop)]
    for k in 0..n {
        let terms: Vec<T<'h>> = (0..n).map(|i| b.mul(&[minv[i][k], basis[i]])).collect();
        let s = c.eval(b.add(&terms))?;
        phi.push(s);
    }
    // sum_k phi_k A^k
    let am = c.list(&a.iter().map(|r| c.list(r)).collect::<Vec<_>>());
    let mut pw = c.ev("identity_matrix", &[b.int(n as i64)])?;
    let mut acc: Vec<Vec<T<'h>>> = vec![vec![b.zero(); n]; n];
    for (k, &ph) in phi.iter().enumerate() {
        if k > 0 {
            pw = c.ev("dot", &[pw, am])?;
        }
        let Some(prow) = c.items(pw) else { return Ok(None) };
        for (i, r) in prow.into_iter().enumerate() {
            let Some(r) = c.items(r) else { return Ok(None) };
            for (j, t) in r.into_iter().enumerate() {
                acc[i][j] = b.add(&[acc[i][j], b.mul(&[ph, t])]);
            }
        }
    }
    let mut out = vec![];
    for row in acc {
        let mut r = vec![];
        for t in row {
            let t = c.eval(t)?;
            let s = c.simp(t)?;
            r.push(c.together(s)?);
        }
        out.push(r);
    }
    Ok(Some(out))
}
