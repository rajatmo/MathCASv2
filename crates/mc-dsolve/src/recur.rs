//! `r_solve(eqs, a, n)` (or `a(n)`) for linear recurrences with constant coefficients `sum c_k a(n + k) == 0`
//! (shifts may be negative): the roots of the characteristic polynomial, `r^n` (`n^j r^n` for a multiple root), the
//! constants fixed by initial values `a(k) == v`. (`mc-sums` wraps this with its solver for polynomial coefficients.)
use super::{combination, flatten, occurrences, replace_terms, Cx, MAX_ORDER};
use mc_calculus::common::is_free;
use mc_api::{Store, View, T};
use mc_eval::Res;

/// The solutions, `None` when the recurrence is not of this kind.
pub(super) fn r_solve<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, eqs: T<'h>, a: T<'h>, n: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let h = b.heap();
    let mut all = vec![];
    flatten(b, eqs, &mut all);
    let mut rec = None;
    let mut inits = vec![];
    for e in all {
        let View::App { head, args: &[l, r] } = h.view(e) else { return Ok(None) };
        if b.sym_of(head) != Some(h.sym("equal")) {
            return Ok(None);
        }
        let mut occ = vec![];
        occurrences(b, e, a, &mut occ);
        if occ.iter().any(|o| o.0 != 0) || occ.is_empty() {
            return Ok(None);
        }
        if occ.iter().any(|o| !is_free(b, o.1, n)) {
            if rec.is_some() {
                return Ok(None);
            }
            rec = Some((e, occ));
        } else {
            let (k, v) = match (h.view(l), h.view(r)) {
                (View::App { head: hl, args: &[k] }, _) if hl == a => (k, r),
                (_, View::App { head: hr, args: &[k] }) if hr == a => (k, l),
                _ => return Ok(None),
            };
            inits.push((k, v));
        }
    }
    let Some((e, occ)) = rec else { return Ok(None) };
    // the shifts: a(n + s)
    let mut shifts = vec![];
    for &(_, arg, term) in &occ {
        let s = c.simp(b.sub(arg, n))?;
        let View::Int(s) = h.view(s) else { return Ok(None) };
        shifts.push((s, term));
    }
    let lo = shifts.iter().map(|s| s.0).min().unwrap_or(0);
    let hi = shifts.iter().map(|s| s.0).max().unwrap_or(0);
    let order = (hi - lo) as usize;
    if order == 0 || order > MAX_ORDER {
        return Ok(None);
    }
    let vs: Vec<T<'h>> = (0..=order).map(|k| c.sym(&format!("r_solve·{k}"))).collect();
    let pairs: Vec<(T<'h>, T<'h>)> = shifts.iter().map(|&(s, term)| (term, vs[(s - lo) as usize])).collect();
    let View::App { args: &[l, r], .. } = h.view(e) else { return Ok(None) };
    let diff = c.eval(replace_terms(b, b.sub(l, r), &pairs))?;
    let mut coefs = vec![];
    for &v in &vs {
        let ck = c.d(diff, v)?;
        if vs.iter().any(|&w| !is_free(b, ck, w)) || !is_free(b, ck, n) {
            return Ok(None);
        }
        coefs.push(ck);
    }
    let zeros: Vec<T<'h>> = vs.iter().map(|&v| c.rule(v, b.zero())).collect();
    let rest = c.subst(diff, &zeros)?;
    if c.simp(rest)? != b.zero() {
        return Ok(None);
    }
    let Some(rts) = super::linear::roots(c, &coefs)? else { return Ok(None) };
    let mut basis = vec![];
    for &(r, mult) in &rts {
        if r == b.zero() {
            return Ok(None);
        }
        for j in 0..mult {
            basis.push(c.eval(b.mul(&[b.powi(n, j as i64), b.pow(r, n)]))?);
        }
    }
    if basis.len() != order {
        return Ok(None);
    }
    let g = combination(c, &basis, 1)?;
    if inits.is_empty() {
        return Ok(Some(vec![g]));
    }
    let mut eqs = vec![];
    for &(k, v) in &inits {
        let at = c.subst(g, &[c.rule(n, k)])?;
        eqs.push(b.call("equal", &[at, v]));
    }
    let consts: Vec<T<'h>> = (1..=order).map(|i| c.constant(i)).collect();
    let s = c.ev("solve", &[c.list(&eqs), c.list(&consts)])?;
    let Some(items) = c.items(s) else { return Ok(None) };
    let mut out = vec![];
    for rs in items {
        let Some(rules) = c.items(rs) else { return Ok(None) };
        let v = c.subst(g, &rules)?;
        out.push(c.simp(v)?);
    }
    Ok(Some(out))
}
