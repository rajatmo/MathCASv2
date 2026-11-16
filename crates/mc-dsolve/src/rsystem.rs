//! `r_solve(eqs, [a, b, ...], n)`: systems of linear recurrences for several sequences (Elaydi, *An Introduction to
//! Difference Equations*, ch. 3).
//!
//! The equations are linear in the shifted unknowns `a(n + s)`; they are solved for the highest shift of every unknown
//! (an implicit system such as `a(n) - 3 b(n + 1) == 5` is allowed), which gives a first-order system
//! `X(n + 1) = A X(n) + G(n)` for the state `X(n) = (a(n + lo), ..., a(n + hi - 1), b(n + lo'), ...)`.
//!
//! - **constant `A`**: with the characteristic polynomial `p(t) = sum p_s t^s` of `A`, Cayley-Hamilton gives for every
//!   component the scalar recurrence `sum p_s x(n + s) = g(n)`, `g = sum_s p_s sum_{t<s} A^(s-1-t) G(n + t)`, which the
//!   scalar solver (`r_solve_value`) solves from the initial values `X(0), A X(0) + G(0), ...` of the system;
//! - **coefficients of period 2** (through `(-1)^n`): the two-step system `X(n + 2) = A(n + 1) A(n) X(n) + ...` has
//!   constant coefficients on each parity class `n = 2m + r`; the parts are joined with `(1 +- (-1)^n)/2`.
//!
//! The state `X(0)` is symbolic: initial values `a(k) == v` fix it (the system run forward exactly), what is left
//! becomes the constants `c(1), c(2), ...`. Every solution is substituted back into the recurrences (`simplify` to 0)
//! and checked against the initial values before it is returned.
use super::{flatten, occurrences, replace_terms, Cx, MAX_ORDER};
use mc_api::{Store, View, T};
use mc_calculus::common::is_free;
use mc_eval::Res;

/// The furthest initial value (steps of the system run forward).
const MAX_STEPS: i64 = 200;

/// `(i, k)` when `t` is `funs[i](k)` with an integer `k`.
fn init_of<'h>(c: &Cx<'_, '_, '_, '_, 'h>, funs: &[T<'h>], t: T<'h>) -> Option<(usize, i64)> {
    let h = c.b.heap();
    let View::App { head, args: &[k] } = h.view(t) else { return None };
    let i = funs.iter().position(|&f| f == head)?;
    match h.view(k) {
        View::Int(k) => Some((i, k)),
        _ => None,
    }
}

/// `t` with every `(-1)^(n + k)` (k an integer) replaced by `(-1)^k pm`.
fn signs<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>, n: T<'h>, pm: T<'h>) -> Res<T<'h>> {
    let b = c.b;
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if is_free(b, t, n) {
        return Ok(t);
    }
    if let &[base, e] = args {
        if b.sym_of(head) == Some(b.hd.pow) && matches!(h.view(base), View::Int(-1)) {
            let k = c.simp(b.sub(e, n))?;
            if let View::Int(k) = h.view(k) {
                return Ok(if k % 2 == 0 { pm } else { b.neg(pm) });
            }
        }
    }
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(signs(c, a, n, pm)?);
    }
    let head = signs(c, head, n, pm)?;
    Ok(b.app(head, &new))
}

/// With `MC_DSOLVE_TRACE` set: `what` and `t` on stderr (development aid).
fn show<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, what: &str, t: T<'h>) {
    if std::env::var_os("MC_DSOLVE_TRACE").is_none() {
        return;
    }
    let s = c.ev("to_string", &[t]).ok();
    let txt = match s.map(|s| c.b.heap().view(s)) {
        Some(View::Str(x)) => x.to_string(),
        _ => "?".into(),
    };
    eprintln!("r_solve system: {what}: {txt}");
}

/// The matrix `a` (rows of terms) as a list.
fn matrix<'h>(c: &Cx<'_, '_, '_, '_, 'h>, a: &[Vec<T<'h>>]) -> T<'h> {
    let rows: Vec<T<'h>> = a.iter().map(|r| c.list(r)).collect();
    c.list(&rows)
}

/// `a x + g` expanded, as a vector.
fn affine<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, a: T<'h>, x: &[T<'h>], g: &[T<'h>]) -> Res<Option<Vec<T<'h>>>> {
    let ax = c.ev("dot", &[a, c.list(x)])?;
    let Some(ax) = c.items(ax) else { return Ok(None) };
    if ax.len() != g.len() {
        return Ok(None);
    }
    let mut out = vec![];
    for (&u, &v) in ax.iter().zip(g) {
        out.push(c.expand(c.b.add(&[u, v]))?);
    }
    Ok(Some(out))
}

/// The terms of `v` with `rules` applied.
fn subst_all<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, v: &[T<'h>], rules: &[T<'h>]) -> Res<Vec<T<'h>>> {
    let mut out = vec![];
    for &t in v {
        out.push(c.subst(t, rules)?);
    }
    Ok(out)
}

/// The components `want` of the solution of `Y(m + 1) = a Y(m) + g(m)`, `Y(0) = y0` (`a` constant, `None` when the
/// scalar solver gives no closed form or `a` is singular).
fn constant_system<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, a: T<'h>, g: &[T<'h>], y0: &[T<'h>], m: T<'h>, want: &[usize]) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let d = y0.len();
    let lam = c.sym("r_solve·t");
    let p = c.ev("characteristic_polynomial", &[a, lam])?;
    let cl = c.ev("coefficient_list", &[p, lam])?;
    let Some(ps) = c.items(cl) else { return Ok(None) };
    if ps.len() != d + 1 || ps[0] == b.zero() || !ps.iter().all(|&q| c.free(q, m) && c.free(q, lam)) {
        return Ok(None);
    }
    // the powers of a
    let mut pows = vec![c.ev("identity_matrix", &[b.int(d as i64)])?];
    for s in 1..d {
        let next = c.ev("dot", &[a, pows[s - 1]])?;
        pows.push(next);
    }
    // g(m + t)
    let mut gs = vec![];
    for t in 0..d {
        let r = c.rule(m, b.add(&[m, b.int(t as i64)]));
        let gt = subst_all(c, g, &[r])?;
        gs.push(c.list(&gt));
    }
    // per component (a sum of lists can cancel to the scalar 0)
    let mut parts = vec![vec![]; d];
    for (s, &ps_s) in ps.iter().enumerate().skip(1) {
        for (t, &gt) in gs.iter().enumerate().take(s) {
            let v = c.ev("dot", &[pows[s - 1 - t], gt])?;
            let Some(v) = c.items(v) else { return Ok(None) };
            if v.len() != d {
                return Ok(None);
            }
            for (part, &vi) in parts.iter_mut().zip(&v) {
                part.push(b.mul(&[ps_s, vi]));
            }
        }
    }
    let mut rhs = vec![];
    for part in &parts {
        rhs.push(c.eval(b.add(part))?);
    }
    // the initial values Y(0), ..., Y(d - 1)
    let mut ys = vec![y0.to_vec()];
    for t in 1..d {
        let r = c.rule(m, b.int(t as i64 - 1));
        let gt = subst_all(c, g, &[r])?;
        let Some(next) = affine(c, a, &ys[t - 1], &gt)? else { return Ok(None) };
        ys.push(next);
    }
    let x = c.sym("r_solve·x");
    let mut out = vec![];
    for &i in want {
        let lhs: Vec<T<'h>> = ps.iter().enumerate().map(|(s, &q)| b.mul(&[q, b.app(x, &[b.add(&[m, b.int(s as i64)])])])).collect();
        let rhs_i = c.simp(rhs[i])?;
        let mut eqs = vec![c.equal(b.add(&lhs), rhs_i)];
        for (t, y) in ys.iter().enumerate() {
            eqs.push(c.equal(b.app(x, &[b.int(t as i64)]), y[i]));
        }
        let xm = b.app(x, &[m]);
        let v = c.ev("r_solve_value", &[c.list(&eqs), xm, m])?;
        if std::env::var_os("MC_DSOLVE_TRACE").is_some() {
            let probe = c.list(&[c.list(&eqs), v]);
            show(c, "scalar", probe);
        }
        if c.stuck(v, "r_solve_value") || c.bad(v) || !c.free(v, x) || c.items(v).is_some() {
            return Ok(None);
        }
        out.push(v);
    }
    Ok(Some(out))
}

/// The closed forms of the unknowns `funs` (in `n`) of a system of linear recurrences, `None` when the system is not
/// of a kind handled.
pub(super) fn r_solve_system<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, eqs: T<'h>, funs: &[T<'h>], n: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let h = b.heap();
    let k = funs.len();
    let mut all = vec![];
    flatten(b, eqs, &mut all);
    // the recurrences, as (lhs - rhs, [(unknown, shift, term)]), and the initial values
    let mut recs = vec![];
    let mut inits = vec![];
    for e in all {
        let View::App { head, args: &[l, r] } = h.view(e) else { return Ok(None) };
        if b.sym_of(head) != Some(h.sym("equal")) {
            return Ok(None);
        }
        let mut occ = vec![];
        for (i, &f) in funs.iter().enumerate() {
            let mut o = vec![];
            occurrences(b, e, f, &mut o);
            for (ord, arg, term) in o {
                if ord != 0 {
                    return Ok(None);
                }
                occ.push((i, arg, term));
            }
        }
        if occ.is_empty() {
            return Ok(None);
        }
        if occ.iter().any(|o| !is_free(b, o.1, n)) {
            let mut sh = vec![];
            for &(i, arg, term) in &occ {
                let s = c.simp(b.sub(arg, n))?;
                let View::Int(s) = h.view(s) else { return Ok(None) };
                sh.push((i, s, term));
            }
            recs.push((b.sub(l, r), sh));
        } else {
            let (lhs, v) = match (init_of(c, funs, l), init_of(c, funs, r)) {
                (Some(p), _) => (p, r),
                (_, Some(p)) => (p, l),
                _ => return Ok(None),
            };
            if !c.free_all(v, funs) {
                return Ok(None);
            }
            inits.push((lhs.0, lhs.1, v));
        }
    }
    if recs.len() != k || k < 2 {
        return Ok(None);
    }
    // the lowest and highest shift of every unknown
    let mut lo = vec![i64::MAX; k];
    let mut hi = vec![i64::MIN; k];
    for (_, sh) in &recs {
        for &(i, s, _) in sh {
            lo[i] = lo[i].min(s);
            hi[i] = hi[i].max(s);
        }
    }
    if (0..k).any(|i| lo[i] >= hi[i] || hi[i] - lo[i] > MAX_ORDER as i64) {
        return Ok(None);
    }
    let d: i64 = (0..k).map(|i| hi[i] - lo[i]).sum();
    if d > MAX_ORDER as i64 {
        return Ok(None);
    }
    let var = |i: usize, s: i64| h.sym_term(h.sym(&format!("r_solve·{i}·{s}")));
    let pm = c.sym("r_solve·pm");
    let mut diffs = vec![];
    for (diff, sh) in &recs {
        let pairs: Vec<(T<'h>, T<'h>)> = sh.iter().map(|&(i, s, term)| (term, var(i, s))).collect();
        let e = c.eval(replace_terms(b, *diff, &pairs))?;
        diffs.push(signs(c, e, n, pm)?);
    }
    let every: Vec<T<'h>> = (0..k).flat_map(|i| (lo[i]..=hi[i]).map(move |s| (i, s))).map(|(i, s)| var(i, s)).collect();
    // linear in the shifted unknowns
    for &e in &diffs {
        for &v in &every {
            let co = c.d(e, v)?;
            if !c.free_all(co, &every) {
                return Ok(None);
            }
        }
    }
    // solved for the highest shifts
    let tops: Vec<T<'h>> = (0..k).map(|i| var(i, hi[i])).collect();
    let zeqs: Vec<T<'h>> = diffs.iter().map(|&e| c.equal(e, b.zero())).collect();
    let s = c.ev("solve", &[c.list(&zeqs), c.list(&tops)])?;
    let Some(sols) = c.items(s) else { return Ok(None) };
    let [sol] = sols.as_slice() else { return Ok(None) };
    let Some(rules) = c.items(*sol) else { return Ok(None) };
    let mut top_val = vec![None; k];
    for r in rules {
        let View::App { args: &[l, v], .. } = h.view(r) else { return Ok(None) };
        let Some(i) = tops.iter().position(|&t| t == l) else { return Ok(None) };
        if !c.free_all(v, &tops) {
            return Ok(None);
        }
        top_val[i] = Some(v);
    }
    let Some(top_val) = top_val.into_iter().collect::<Option<Vec<_>>>() else { return Ok(None) };
    // the state and its successor
    let state: Vec<(usize, i64)> = (0..k).flat_map(|i| (lo[i]..hi[i]).map(move |s| (i, s))).collect();
    let xs: Vec<T<'h>> = state.iter().map(|&(i, s)| var(i, s)).collect();
    let next: Vec<T<'h>> = state.iter().map(|&(i, s)| if s + 1 < hi[i] { var(i, s + 1) } else { top_val[i] }).collect();
    let zero_x: Vec<T<'h>> = xs.iter().map(|&x| c.rule(x, b.zero())).collect();
    let mut a = vec![];
    let mut g = vec![];
    for &e in &next {
        let mut row = vec![];
        for &x in &xs {
            let co = c.d(e, x)?;
            let co = c.together(co)?;
            if !c.free(co, n) || !c.free_all(co, &xs) {
                return Ok(None);
            }
            row.push(co);
        }
        a.push(row);
        let ge = c.subst(e, &zero_x)?;
        g.push(c.together(ge)?);
    }
    let periodic = a.iter().flatten().chain(g.iter()).any(|&t| !c.free(t, pm));
    // the symbolic state X(0)
    let s0: Vec<T<'h>> = (0..xs.len()).map(|j| c.sym(&format!("r_solve·s{j}"))).collect();
    let want: Vec<usize> = (0..k).map(|i| state.iter().position(|&(ii, s)| ii == i && s == lo[i]).unwrap_or(0)).collect();
    let m = c.sym("r_solve·m");
    let closed = if periodic {
        let (one, mone) = (b.one(), b.int(-1));
        let a_at = |c: &mut Cx<'_, '_, '_, '_, 'h>, sg: T<'h>| -> Res<T<'h>> {
            let r = c.rule(pm, sg);
            let t = matrix(c, &a);
            c.subst(t, &[r])
        };
        let a0 = a_at(c, one)?;
        let a1 = a_at(c, mone)?;
        // g at n = 2m + r + j
        let g_at = |c: &mut Cx<'_, '_, '_, '_, 'h>, r: i64| -> Res<Vec<T<'h>>> {
            let sg = if r % 2 == 0 { b.one() } else { b.int(-1) };
            let rules = [c.rule(n, b.add(&[b.mul(&[b.int(2), m]), b.int(r)])), c.rule(pm, sg)];
            subst_all(c, &g, &rules)
        };
        let mut parts = vec![];
        for r in 0..2i64 {
            let (ar, ar1) = if r == 0 { (a0, a1) } else { (a1, a0) };
            let br = c.ev("dot", &[ar1, ar])?;
            let gr = g_at(c, r)?;
            let gr1 = g_at(c, r + 1)?;
            let Some(hr) = affine(c, ar1, &gr, &gr1)? else { return Ok(None) };
            let y0 = if r == 0 {
                s0.clone()
            } else {
                let g00 = g_at(c, 0)?;
                let g00 = subst_all(c, &g00, &[c.rule(m, b.zero())])?;
                let Some(y) = affine(c, a0, &s0, &g00)? else { return Ok(None) };
                y
            };
            let Some(y) = constant_system(c, br, &hr, &y0, m, &want)? else { return Ok(None) };
            parts.push(y);
        }
        let sgn = b.pow(b.int(-1), n);
        let even = b.mul(&[b.rat(1, 2), b.add(&[b.one(), sgn])]);
        let odd = b.mul(&[b.rat(1, 2), b.sub(b.one(), sgn)]);
        let mut out = vec![];
        for (&y0, &y1) in parts[0].iter().zip(&parts[1]) {
            let e0 = c.subst(y0, &[c.rule(m, b.mul(&[b.rat(1, 2), n]))])?;
            let e1 = c.subst(y1, &[c.rule(m, b.mul(&[b.rat(1, 2), b.sub(n, b.one())]))])?;
            out.push(c.eval(b.add(&[b.mul(&[even, e0]), b.mul(&[odd, e1])]))?);
        }
        out
    } else {
        let at = matrix(c, &a);
        let gm = subst_all(c, &g, &[c.rule(n, m)])?;
        let Some(y) = constant_system(c, at, &gm, &s0, m, &want)? else { return Ok(None) };
        let mut out = vec![];
        for (i, &yi) in y.iter().enumerate() {
            out.push(c.subst(yi, &[c.rule(m, b.sub(n, b.int(lo[i])))])?);
        }
        out
    };
    // the initial values fix the state: the system run forward exactly
    let mut fixed = vec![];
    if !inits.is_empty() {
        let far = inits.iter().map(|&(i, kk, _)| kk - lo[i]).max().unwrap_or(0);
        if inits.iter().any(|&(i, kk, _)| kk < lo[i]) || far > MAX_STEPS {
            return Ok(None);
        }
        let at_sign = |t: i64| if t % 2 == 0 { b.one() } else { b.int(-1) };
        let mut x = s0.clone();
        let mut eqs = vec![];
        for t in 0..=far {
            for &(i, kk, v) in &inits {
                if kk - lo[i] == t {
                    eqs.push(c.equal(x[want[i]], v));
                }
            }
            if t == far {
                break;
            }
            let rules = [c.rule(n, b.int(t)), c.rule(pm, at_sign(t))];
            let mt = c.subst(matrix(c, &a), &rules)?;
            let gt = subst_all(c, &g, &rules)?;
            let Some(nx) = affine(c, mt, &x, &gt)? else { return Ok(None) };
            x = nx;
        }
        let s = c.ev("solve", &[c.list(&eqs), c.list(&s0)])?;
        let Some(sols) = c.items(s) else { return Ok(None) };
        let [sol] = sols.as_slice() else { return Ok(None) };
        let Some(rules) = c.items(*sol) else { return Ok(None) };
        fixed = rules;
    }
    let mut out = subst_all(c, &closed, &fixed)?;
    // the state left free: the constants
    let mut renames = vec![];
    for &sj in &s0 {
        if out.iter().any(|&t| !c.free(t, sj)) {
            renames.push(c.rule(sj, c.constant(renames.len() + 1)));
        }
    }
    out = subst_all(c, &out, &renames)?;
    for t in out.iter_mut() {
        let u = tidy_powers(c, *t, n)?;
        *t = c.simp(u)?;
        if c.bad(*t) || !c.free(*t, pm) || !c.free(*t, m) {
            return Ok(None);
        }
    }
    // the check: the recurrences and the initial values
    for (diff, sh) in &recs {
        let mut pairs = vec![];
        for &(i, s, term) in sh {
            let v = c.subst(out[i], &[c.rule(n, b.add(&[n, b.int(s)]))])?;
            pairs.push((term, v));
        }
        let res = c.eval(replace_terms(b, *diff, &pairs))?;
        if std::env::var_os("MC_DSOLVE_TRACE").is_some() {
            let p = c.list(&out);
            let p = c.list(&[p, res]);
            show(c, "residual", p);
        }
        if !residual_zero(c, res, n)? {
            return Ok(None);
        }
    }
    for &(i, kk, v) in &inits {
        let at = c.subst(out[i], &[c.rule(n, b.int(kk))])?;
        if !c.is_zero(b.sub(at, v))? {
            return Ok(None);
        }
    }
    Ok(Some(out))
}

/// `t` with the powers `z^e` of a positive rational `z = w^k` (`w` rational) written `w^(k e)` (`(1/4)^(n/2)` is
/// `2^-n`), which holds for every `e` as `z > 0`.
fn tidy_powers<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>, v: T<'h>) -> Res<T<'h>> {
    let b = c.b;
    let h = b.heap();
    if c.free(t, v) {
        return Ok(t);
    }
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let positive_rational = |z: T<'h>| match h.view(z) {
        View::Int(i) => i > 1,
        View::Rational { num, .. } => matches!(h.view(num), View::Int(1..)),
        _ => false,
    };
    if let (Some(s), &[z, e]) = (b.sym_of(head), args) {
        if s == b.hd.pow && positive_rational(z) {
            for k in (2..=6).rev() {
                let w = c.eval(b.pow(z, b.rat(1, k)))?;
                if positive_rational(w) {
                    let ke = c.expand(b.mul(&[b.int(k), e]))?;
                    return c.eval(b.pow(w, ke));
                }
            }
            return Ok(t);
        }
    }
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(tidy_powers(c, a, v)?);
    }
    c.eval(b.app(head, &new))
}

/// `t` with the powers `z^(u + w)` (`z` and `u` free of `v`) split into `z^u K`, `K` a new symbol that stands for
/// `z^w` (the same for equal `z^w`; `kernels` holds them). `z^(u + w) = z^u z^w` holds for every `z != 0` (principal
/// powers), so `t` is zero when the result is zero as a function of independent `K`s. With `integer` (`v` takes
/// integer values only) also `z^(k v)` = `(z^k)^v` for an integer `k`.
fn split_powers<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>, v: T<'h>, integer: bool, kernels: &mut Vec<(T<'h>, T<'h>)>) -> Res<T<'h>> {
    let b = c.b;
    let h = b.heap();
    if c.free(t, v) {
        return Ok(t);
    }
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if let (Some(s), &[z, e]) = (b.sym_of(head), args) {
        if s == b.hd.pow && c.free(z, v) && z != b.zero() {
            let e = c.expand(e)?;
            let (fixed, moving): (Vec<T<'h>>, Vec<T<'h>>) = c.terms(e).into_iter().partition(|&u| c.free(u, v));
            let w = c.eval(b.add(&moving))?;
            let k = c.eval(b.div(w, v))?;
            let kernel = match h.view(k) {
                View::Int(_) if integer => {
                    let zk = c.eval(b.pow(z, k))?;
                    b.pow(zk, v)
                }
                _ => b.pow(z, w),
            };
            let sym = match kernels.iter().find(|p| p.0 == kernel) {
                Some(p) => p.1,
                None => {
                    let s = c.sym(&format!("r_solve·k{}", kernels.len()));
                    kernels.push((kernel, s));
                    s
                }
            };
            let lead = c.eval(b.pow(z, b.add(&fixed)))?;
            return c.eval(b.mul(&[lead, sym]));
        }
    }
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(split_powers(c, a, v, integer, kernels)?);
    }
    let head = split_powers(c, head, v, integer, kernels)?;
    c.eval(b.app(head, &new))
}

/// Is the residual `t` zero for every integer `n`? `simplify` after splitting the powers, and else on the two parity
/// classes `n = 2m + r` separately (with `(-1)^n` replaced by its value there).
fn residual_zero<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>, n: T<'h>) -> Res<bool> {
    if c.is_zero(t)? {
        return Ok(true);
    }
    let e = split_powers(c, t, n, false, &mut vec![])?;
    let e = c.expand(e)?;
    if c.is_zero(e)? {
        return Ok(true);
    }
    let b = c.b;
    let pm = c.sym("r_solve·pm");
    let m = c.sym("r_solve·m");
    let e = signs(c, t, n, pm)?;
    for (r, sg) in [(0, b.one()), (1, b.int(-1))] {
        let at = c.subst(e, &[c.rule(pm, sg)])?;
        let at = c.subst(at, &[c.rule(n, b.add(&[b.mul(&[b.int(2), m]), b.int(r)]))])?;
        let at = split_powers(c, at, m, true, &mut vec![])?;
        let at = c.expand(at)?;
        if !c.is_zero(at)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Every `f(u, w)` in `t`.
fn applications<'h>(c: &Cx<'_, '_, '_, '_, 'h>, t: T<'h>, f: T<'h>, out: &mut Vec<T<'h>>) {
    let h = c.b.heap();
    if let View::App { head, args } = h.view(t) {
        if head == f && args.len() == 2 {
            if !out.contains(&t) {
                out.push(t);
            }
            return;
        }
        applications(c, head, f, out);
        for &a in args {
            applications(c, a, f, out);
        }
    }
}

/// `(u, v, g)` with `u a + v b = g = gcd(a, b)`.
fn bezout(a: i64, b: i64) -> (i64, i64, i64) {
    if b == 0 {
        return if a < 0 { (-1, 0, -a) } else { (1, 0, a) };
    }
    let (u, v, g) = bezout(b, a.rem_euclid(b));
    (v, u - a.div_euclid(b) * v, g)
}

/// `r_solve(eq, a(m, n), [m, n])` for a partial recurrence with two terms and constant coefficients,
/// `alpha a(m + p1, n + q1) + beta a(m + p2, n + q2) == f(m, n)` with `f` a polynomial (Jordan, *Calculus of Finite
/// Differences*, ch. 12): with `d = (p1 - p2, q1 - q2)` primitive, the homogeneous solutions are `r^t F(I)`, `r =
/// -beta/alpha`, `I = d2 m - d1 n` constant along `d` (one free value on every line `I = const`, so `F` is an
/// arbitrary function, the constant `c(1)`) and `t` linear with `t(d) = 1`; a particular solution is a polynomial (of
/// one degree more when `alpha + beta = 0`), its coefficients by comparison. The answer is substituted back.
pub(super) fn r_solve_partial<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, eqs: T<'h>, f: T<'h>, m: T<'h>, n: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    let mut all = vec![];
    flatten(b, eqs, &mut all);
    let [e] = all.as_slice() else { return Ok(None) };
    let View::App { head, args: &[l, r] } = h.view(*e) else { return Ok(None) };
    if b.sym_of(head) != Some(h.sym("equal")) {
        return Ok(None);
    }
    let mut apps = vec![];
    applications(c, *e, f, &mut apps);
    // the shifts, two distinct ones
    let mut terms: Vec<((i64, i64), T<'h>)> = vec![];
    for &t in &apps {
        let View::App { args: &[u, w], .. } = h.view(t) else { return Ok(None) };
        let p = c.simp(b.sub(u, m))?;
        let q = c.simp(b.sub(w, n))?;
        let (View::Int(p), View::Int(q)) = (h.view(p), h.view(q)) else { return Ok(None) };
        if terms.iter().any(|x| x.0 == (p, q)) {
            return Ok(None);
        }
        terms.push(((p, q), t));
    }
    let [((p1, q1), t1), ((p2, q2), t2)] = terms.as_slice() else { return Ok(None) };
    let (v1, v2) = (c.sym("r_solve·v1"), c.sym("r_solve·v2"));
    let diff = c.eval(replace_terms(b, b.sub(l, r), &[(*t1, v1), (*t2, v2)]))?;
    if !c.free(diff, f) {
        return Ok(None);
    }
    let alpha = c.d(diff, v1)?;
    let beta = c.d(diff, v2)?;
    for co in [alpha, beta] {
        if !c.free_all(co, &[m, n, v1, v2]) || co == b.zero() {
            return Ok(None);
        }
    }
    let zero_v = [c.rule(v1, b.zero()), c.rule(v2, b.zero())];
    let rest = c.subst(diff, &zero_v)?;
    let rhs = c.expand(b.neg(rest))?;
    let (d1, d2) = (p1 - p2, q1 - q2);
    let (u, v, g) = bezout(d1, d2);
    if g != 1 {
        return Ok(None);
    }
    // t with t(d) = 1, preferring a single index; I = d2 m - d1 n with a positive leading coefficient
    let t = if d2.abs() == 1 {
        b.mul(&[b.int(d2), n])
    } else if d1.abs() == 1 {
        b.mul(&[b.int(d1), m])
    } else {
        b.add(&[b.mul(&[b.int(u), m]), b.mul(&[b.int(v), n])])
    };
    let sign = if d2 > 0 || (d2 == 0 && d1 < 0) { 1 } else { -1 };
    let inv = b.add(&[b.mul(&[b.int(sign * d2), m]), b.mul(&[b.int(-sign * d1), n])]);
    let ratio = c.simp(b.neg(b.div(beta, alpha)))?;
    // the particular solution
    let mut part = b.zero();
    if rhs != b.zero() {
        let dm = c.ev("exponent", &[rhs, m])?;
        let dn = c.ev("exponent", &[rhs, n])?;
        let (View::Int(dm), View::Int(dn)) = (h.view(dm), h.view(dn)) else { return Ok(None) };
        let sum = c.simp(b.add(&[alpha, beta]))?;
        let deg = dm + dn + i64::from(sum == b.zero());
        if !(0..=8).contains(&deg) {
            return Ok(None);
        }
        let mut ks = vec![];
        let mut ansatz = vec![];
        for i in 0..=deg {
            for j in 0..=deg - i {
                let k = c.sym(&format!("r_solve·k{i}·{j}"));
                ks.push(k);
                ansatz.push(b.mul(&[k, b.powi(m, i), b.powi(n, j)]));
            }
        }
        let pa = b.add(&ansatz);
        let shifted = |c: &mut Cx<'_, '_, '_, '_, 'h>, p: i64, q: i64| -> Res<T<'h>> {
            let rules = [c.rule(m, b.add(&[m, b.int(p)])), c.rule(n, b.add(&[n, b.int(q)]))];
            c.subst(pa, &rules)
        };
        let s1 = shifted(c, *p1, *q1)?;
        let s2 = shifted(c, *p2, *q2)?;
        let res = c.expand(b.sub(b.add(&[b.mul(&[alpha, s1]), b.mul(&[beta, s2])]), rhs))?;
        let cl = c.ev("coefficient_list", &[res, c.list(&[m, n])])?;
        let flat = c.ev("flatten", &[cl])?;
        let Some(cos) = c.items(flat) else { return Ok(None) };
        if cos.iter().any(|&q| !c.free_all(q, &[m, n])) {
            return Ok(None);
        }
        let eqs: Vec<T<'h>> = cos.iter().map(|&q| c.equal(q, b.zero())).collect();
        let s = c.ev("solve", &[c.list(&eqs), c.list(&ks)])?;
        let Some(sols) = c.items(s) else { return Ok(None) };
        let Some(&sol) = sols.first() else { return Ok(None) };
        let Some(mut rules) = c.items(sol) else { return Ok(None) };
        let pa = c.subst(pa, &rules)?;
        rules = ks.iter().map(|&k| c.rule(k, b.zero())).collect();
        part = c.subst(pa, &rules)?;
    }
    // the solution at (m + p, n + q), the argument of the arbitrary function expanded
    let at = |c: &mut Cx<'_, '_, '_, '_, 'h>, p: i64, q: i64| -> Res<T<'h>> {
        let rules = [c.rule(m, b.add(&[m, b.int(p)])), c.rule(n, b.add(&[n, b.int(q)]))];
        let pp = c.subst(part, &rules)?;
        let tt = c.subst(t, &rules)?;
        let tt = c.expand(tt)?;
        let ii = c.subst(inv, &rules)?;
        let ii = c.expand(ii)?;
        c.eval(b.add(&[pp, b.mul(&[b.pow(ratio, tt), h.app(c.constant(1), &[ii])])]))
    };
    let sol = at(c, 0, 0)?;
    // the check
    let w1 = at(c, *p1, *q1)?;
    let w2 = at(c, *p2, *q2)?;
    let res = c.eval(replace_terms(b, b.sub(l, r), &[(*t1, w1), (*t2, w2)]))?;
    if !c.is_zero(res)? {
        let mut kernels = vec![];
        let e = split_powers(c, res, m, false, &mut kernels)?;
        let e = split_powers(c, e, n, false, &mut kernels)?;
        let e = c.expand(e)?;
        if !c.is_zero(e)? {
            return Ok(None);
        }
    }
    Ok(Some(sol))
}
