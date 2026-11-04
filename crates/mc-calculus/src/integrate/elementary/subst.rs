//! Derivative-divides substitution: `∫ g(u(x)) u'(x) dx = G(u(x))` with `G = ∫ g(t) dt`.
//!
//! For each candidate `u` (the subterms of `f` that depend on `x`, largest first), `q = f / u'` is built
//! canonically (and again with `u'` factored when it is a sum), the occurrences of `u` in `q` are replaced by a
//! fresh variable `t`, and the substitution applies when no `x` is left. Besides plain occurrences, two
//! identities are used: for `u = x^m`, `x^(j m)` is `t^j`; for `u = sin(v)` (`cos(v)`), `cos(v)^(2j)`
//! (`sin(v)^(2j)`) is `(1 - t^2)^j`; for `u = log(x)`, `x^k` is `exp(k t)`; for `u = exp(v)`, `exp(k v)` is
//! `t^k`. The sub-integral may exceed `f` by at most two subterms.
use super::{deriv, fresh, int_of, rewrite, size, spend, subst, unary};
use crate::integrate::{antiderivative, apply, args_of, is_free};
use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashSet;

/// The most candidates tried for one integrand.
const MAX_CANDIDATES: usize = 24;
/// Integrands larger than this (distinct subterms) are not searched.
const MAX_SIZE: usize = 400;

/// An antiderivative of `f` by a substitution, else `None`. With `exotic`, only the candidates `log(x)` and
/// `exp(v)` are tried, with their identities (`x^k -> exp(k t)`, `exp(k v) -> t^k`); without, those identities
/// are off.
pub fn integrate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    depth: u32,
    exotic: bool,
) -> Res<Option<T<'h>>> {
    let fsize = size(b, f, MAX_SIZE + 1);
    if fsize > MAX_SIZE {
        return Ok(None);
    }
    let t = fresh(b, "u", depth);
    for u in candidates(b, f, x) {
        if exotic && !unary(b, u).is_some_and(|(n, v)| (n == "log" && v == x) || n == "exp") {
            continue;
        }
        if !spend(1) {
            return Ok(None);
        }
        cx.tick(1)?;
        let Some(du) = deriv(cx, b, u, x)? else { continue };
        if b.is_zero(du) {
            continue;
        }
        let mut g = replace(b, b.div(f, du), u, t, x, exotic);
        if !is_free(b, g, x) && args_of(b, du, b.hd.add).is_some() {
            let fd = apply(cx, b, "factor", &[du])?;
            if fd != du {
                g = replace(b, b.div(f, fd), u, t, x, exotic);
            }
        }
        if !is_free(b, g, x) || size(b, g, fsize + 3) > fsize + 2 {
            continue;
        }
        if let Some(r) = antiderivative(cx, b, g, t, depth + 1)? {
            return Ok(Some(subst(b, r, t, u)));
        }
    }
    Ok(None)
}

/// The subterms of `f` that depend on `x`, other than `x`, `f` and products, largest first.
fn candidates<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> Vec<T<'h>> {
    let h = b.heap();
    let mut seen: FxHashSet<T<'h>> = FxHashSet::default();
    let mut out = Vec::new();
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if u == x || !seen.insert(u) || is_free(b, u, x) {
            continue;
        }
        let View::App { args, .. } = h.view(u) else { continue };
        stack.extend_from_slice(args);
        if u != f && args_of(b, u, b.hd.mul).is_none() {
            out.push(u);
        }
    }
    let mut sized: Vec<(usize, T<'h>)> = out.into_iter().map(|u| (size(b, u, 64), u)).collect();
    sized.sort_by_key(|p| std::cmp::Reverse(p.0));
    sized.into_iter().take(MAX_CANDIDATES).map(|(_, u)| u).collect()
}

/// `q` with `u` replaced by `t`, using the identities of the module documentation.
fn replace<'h>(b: &Build<'h>, q: T<'h>, u: T<'h>, t: T<'h>, x: T<'h>, exotic: bool) -> T<'h> {
    // u = x^m: x^(j m) -> t^j
    let xm = match args_of(b, u, b.hd.pow) {
        Some([base, m]) if *base == x => int_of(b, *m).filter(|&m| m >= 2 || m <= -1),
        _ => None,
    };
    // u = sin(v): cos(v)^(2j) -> (1 - t^2)^j, and the converse
    let pyth = unary(b, u).and_then(|(n, v)| match n.as_str() {
        "sin" => Some(("cos", v)),
        "cos" => Some(("sin", v)),
        _ => None,
    });
    // u = log(x): x^k -> exp(k t); u = exp(v): exp(k v) -> t^k (k an integer)
    let log_x = exotic && unary(b, u).is_some_and(|(n, v)| n == "log" && v == x);
    let exp_v = unary(b, u).and_then(|(n, v)| (exotic && n == "exp").then_some(v));
    let one_minus = b.sub(b.one(), b.powi(t, 2));
    rewrite(b, q, &mut |s| {
        if s == u {
            return Some(t);
        }
        if log_x && s == x {
            return Some(b.call("exp", &[t]));
        }
        if let Some(v) = exp_v {
            if let Some(("exp", w)) = unary(b, s).as_ref().map(|(n, w)| (n.as_str(), *w)) {
                if let Some(k) = int_of(b, b.div(w, v)) {
                    return Some(b.powi(t, k));
                }
            }
        }
        let (base, k) = match args_of(b, s, b.hd.pow) {
            Some([base, k]) => (*base, int_of(b, *k)?),
            _ => return None,
        };
        if let Some(m) = xm {
            if base == x && k % m == 0 {
                return Some(b.powi(t, k / m));
            }
        }
        if log_x && base == x {
            return Some(b.call("exp", &[b.mul(&[b.int(k), t])]));
        }
        if let Some((other, v)) = pyth {
            if k % 2 == 0 && unary(b, base).is_some_and(|(n, w)| n == other && w == v) {
                return Some(b.powi(one_minus, k / 2));
            }
        }
        None
    })
}
