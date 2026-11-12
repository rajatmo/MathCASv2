//! Integrals with a `dirac_delta` factor (decision IOSYM.10).
//!
//! `integrate(g dirac_delta(u), [x, a, b])` is the sum of `g(r)/|u'(r)|` over the simple real zeros `r` of `u`
//! strictly inside `(a, b)` (found by `solve`, on a bounded interval when `u` is not rational); a zero at a bound
//! leaves the call. `integrate(g derivative(n)(dirac_delta)(x - c), [x, a, b])` is `(-1)^n g^(n)(c)` for `c` inside.
//! The antiderivative of `g dirac_delta(p x + q)` is `g(r)/|p| heaviside_theta(x - r)` with `r = -q/p`.
use super::{cmp, point, subst, Pt};
use crate::{apply, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use std::cmp::Ordering;

/// The factor `dirac_delta(u)` (or its n-th derivative, `n > 0`) of `f` and the other factors: `(u, n, g)`.
fn split_delta<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> Option<(T<'h>, i64, T<'h>)> {
    let h = b.heap();
    let factors: Vec<T<'h>> = match h.view(f) {
        View::App { head, args } if b.sym_of(head) == Some(b.hd.mul) => args.to_vec(),
        _ => vec![f],
    };
    let delta = h.sym("dirac_delta");
    let mut found = None;
    let mut rest = vec![];
    for t in factors {
        let View::App { head, args } = h.view(t) else {
            rest.push(t);
            continue;
        };
        let order = if b.sym_of(head) == Some(delta) {
            Some(0)
        } else {
            b.as_derivative(head).and_then(|(o, g)| (b.sym_of(g) == Some(delta) && o.len() == 1).then_some(o[0]))
        };
        match (order, args) {
            (Some(n), &[u]) if !is_free(b, u, x) && found.is_none() => found = Some((u, n)),
            (Some(_), _) if !is_free(b, t, x) => return None,
            _ => rest.push(t),
        }
    }
    let (u, n) = found?;
    if rest.iter().any(|&r| has_delta(b, r)) {
        return None;
    }
    Some((u, n, b.mul(&rest)))
}

fn has_delta<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    !is_free(b, t, b.heap().sym_term(b.heap().sym("dirac_delta")))
}

/// `u = p x + q` with p, q free of x: `(p, q)`.
fn linear<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let p = apply(cx, b, "d", &[u, x])?;
    if !is_free(b, p, x) || p == b.zero() {
        return Ok(None);
    }
    let q = subst(cx, b, u, x, b.zero())?;
    Ok(Some((p, q)))
}

/// The definite integral (module comment), or `None`.
pub(crate) fn definite<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, lo: T<'h>, hi: T<'h>) -> Res<Option<T<'h>>> {
    if !has_delta(b, f) {
        return Ok(None);
    }
    let Some((u, n, g)) = split_delta(b, f, x) else { return Ok(None) };
    let (Some(p), Some(q)) = (point(cx, b, lo)?, point(cx, b, hi)?) else { return Ok(None) };
    let (p, q, flip) = match cmp(cx, b, p, q)? {
        Some(Ordering::Less) => (p, q, false),
        Some(Ordering::Greater) => (q, p, true),
        Some(Ordering::Equal) => return Ok(Some(b.zero())),
        None => return Ok(None),
    };
    let h = b.heap();
    let inside = |cx: &mut NativeCx<'_, '_, 'h>, r: T<'h>| -> Res<Option<bool>> {
        let Some(c) = point(cx, b, r)? else { return Ok(None) };
        Ok(match (cmp(cx, b, c, p)?, cmp(cx, b, c, q)?) {
            (Some(Ordering::Greater), Some(Ordering::Less)) => Some(true),
            (Some(Ordering::Less), _) | (_, Some(Ordering::Greater)) => Some(false),
            _ => None,
        })
    };
    let total = if n > 0 {
        // derivatives of the delta: u = x - c only
        let Some((s, c0)) = linear(cx, b, u, x)? else { return Ok(None) };
        if s != b.one() {
            return Ok(None);
        }
        let c = b.neg(c0);
        match inside(cx, c)? {
            Some(true) => {
                let mut dg = g;
                for _ in 0..n {
                    dg = apply(cx, b, "d", &[dg, x])?;
                }
                let v = subst(cx, b, dg, x, c)?;
                b.mul(&[b.int(if n % 2 == 0 { 1 } else { -1 }), v])
            }
            Some(false) => b.zero(),
            None => return Ok(None),
        }
    } else {
        let mut eq = h.call(h.sym("equal"), &[u, b.zero()]);
        if let (Pt::Fin(l), Pt::Fin(r)) = (p, q) {
            eq = h.call(h.sym("and"), &[eq, h.call(h.sym("less_equal"), &[l, x]), h.call(h.sym("less_equal"), &[x, r])]);
        } else if linear(cx, b, u, x)?.is_none() && !polynomial(b, u, x) {
            return Ok(None);
        }
        let sols = apply(cx, b, "solve", &[eq, x, h.sym_term(h.sym("reals"))])?;
        let View::App { head, args: sols } = h.view(sols) else { return Ok(None) };
        if b.sym_of(head) != Some(b.hd.list) {
            return Ok(None);
        }
        let du = apply(cx, b, "d", &[u, x])?;
        let mut parts = vec![];
        for &sol in sols {
            let r = match h.view(sol) {
                View::App { head, args: &[rule] } if b.sym_of(head) == Some(b.hd.list) => match h.view(rule) {
                    View::App { args: &[k, v], .. } if k == x => v,
                    _ => return Ok(None),
                },
                _ => return Ok(None),
            };
            match inside(cx, r)? {
                Some(true) => {}
                Some(false) => continue,
                None => return Ok(None),
            }
            let slope = subst(cx, b, du, x, r)?;
            if slope == b.zero() {
                return Ok(None);
            }
            let gv = subst(cx, b, g, x, r)?;
            parts.push(b.mul(&[gv, b.powi(b.call("abs", &[slope]), -1)]));
        }
        b.add(&parts)
    };
    let total = cx.eval(total)?;
    Ok(Some(if flip { b.neg(total) } else { total }))
}

/// Is `t` a polynomial in `x`?
fn polynomial<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x || is_free(b, u, x) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { return false };
        match b.sym_of(head) {
            Some(s) if s == b.hd.add || s == b.hd.mul => stack.extend(args.iter().copied()),
            Some(s) if s == b.hd.pow && matches!(args, [_, e] if matches!(h.view(*e), View::Int(0..))) => stack.push(args[0]),
            _ => return false,
        }
    }
    true
}

/// The antiderivative of `g dirac_delta(p x + q)` (module comment), or `None`.
pub(crate) fn antiderivative<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    if !has_delta(b, f) {
        return Ok(None);
    }
    let Some((u, 0, g)) = split_delta(b, f, x) else { return Ok(None) };
    let Some((p, q)) = linear(cx, b, u, x)? else { return Ok(None) };
    let r = cx.eval(b.mul(&[b.neg(q), b.powi(p, -1)]))?;
    let gv = subst(cx, b, g, x, r)?;
    let h = b.heap();
    let step = h.call(h.sym("heaviside_theta"), &[b.sub(x, r)]);
    Ok(Some(cx.eval(b.mul(&[gv, b.powi(b.call("abs", &[p]), -1), step]))?))
}
