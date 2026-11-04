//! Integration by parts: the tabular form for a polynomial times a function with known iterated antiderivatives,
//! and the general rule with the LIATE choice.
use super::{deriv, factors, int_of, is_poly, pow_parts, table, unary};
use crate::integrate::{antiderivative, apply, is_free};
use crate::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// The highest degree of a polynomial integrated by the tabular method.
const MAX_DEGREE: usize = 200;

/// `∫ P G` for a polynomial `P` and `G` one of `exp(u)`, `k^u`, `sin(u)`, `cos(u)`, `sinh(u)`, `cosh(u)` or
/// `exp(u) sin(v)`, `exp(u) cos(v)` (u, v linear): `Σ_k (-1)^k P^(k) G_(k+1)` with `G_j` the j-th iterated
/// antiderivative of `G`.
pub fn tabular<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let (poly, rest): (Vec<T<'h>>, Vec<T<'h>>) = factors(b, f).into_iter().partition(|&g| is_poly(b, g, x));
    if poly.is_empty() || rest.is_empty() || rest.len() > 2 {
        return Ok(None);
    }
    // the class of G
    let mut exps = 0;
    let mut trigs = 0;
    let mut hyps = 0;
    for &g in &rest {
        if table::exp_like(cx, b, g, x)?.is_some() {
            exps += 1;
            continue;
        }
        match unary(b, g) {
            Some((n, _)) if n == "sin" || n == "cos" => trigs += 1,
            Some((n, _)) if n == "sinh" || n == "cosh" => hyps += 1,
            _ => return Ok(None),
        }
    }
    let ok = match rest.len() {
        1 => true,
        _ => exps == 1 && trigs == 1 && hyps == 0,
    };
    if !ok {
        return Ok(None);
    }
    let mut p = b.mul(&poly);
    let mut g = b.mul(&rest);
    let mut terms = Vec::new();
    let mut sign = b.one();
    for _ in 0..=MAX_DEGREE {
        cx.tick(1)?;
        if b.is_zero(p) {
            return Ok(Some(b.add(&terms)));
        }
        let Some(mut gi) = simple(cx, b, g, x)? else { return Ok(None) };
        if rest.len() == 2 {
            // exp(u) (α sin v + β cos v): collected, or the iterated antiderivatives double in size at each step
            gi = apply(cx, b, "expand", &[gi])?;
        }
        terms.push(b.mul(&[sign, p, gi]));
        let Some(dp) = deriv(cx, b, p, x)? else { return Ok(None) };
        p = dp;
        g = gi;
        sign = b.neg(sign);
    }
    Ok(None)
}

/// An antiderivative by linearity and the table only (the iterated antiderivatives of [`tabular`]).
fn simple<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    if is_free(b, g, x) {
        return Ok(Some(b.mul(&[g, x])));
    }
    if let Some(ts) = crate::integrate::args_of(b, g, b.hd.add) {
        let mut out = Vec::with_capacity(ts.len());
        for &t in ts {
            cx.tick(1)?;
            let Some(r) = simple(cx, b, t, x)? else { return Ok(None) };
            out.push(r);
        }
        return Ok(Some(b.add(&out)));
    }
    let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = factors(b, g).into_iter().partition(|&t| is_free(b, t, x));
    // a sum factor (the iterated antiderivatives of exp(u) sin(v)) is distributed
    if let Some(i) = dep.iter().position(|&t| crate::integrate::args_of(b, t, b.hd.add).is_some()) {
        let others: Vec<T<'h>> = dep.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &t)| t).collect();
        let Some(ts) = crate::integrate::args_of(b, dep[i], b.hd.add) else { return Ok(None) };
        let mut out = Vec::with_capacity(ts.len());
        for &t in ts {
            let mut fs = others.clone();
            fs.push(t);
            let term = b.mul(&fs);
            if crate::integrate::args_of(b, term, b.hd.add).is_some() {
                return Ok(None);
            }
            let Some(r) = simple(cx, b, term, x)? else { return Ok(None) };
            out.push(r);
        }
        return Ok(Some(b.mul(&[b.mul(&free), b.add(&out)])));
    }
    let r = table::single(cx, b, b.mul(&dep), x)?;
    Ok(r.map(|r| b.mul(&[b.mul(&free), r])))
}

/// Integration by parts `∫ u dv = u v - ∫ v du`: `u` a logarithm or inverse function factor (a positive integer
/// power of it), else the polynomial factors; `dv` the rest.
pub fn by_parts<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let fs = factors(b, f);
    let rank = |g: T<'h>| -> Option<u8> {
        let (base, k) = pow_parts(b, g);
        if !int_of(b, k).is_some_and(|k| k >= 1) || is_free(b, base, x) {
            return None;
        }
        match unary(b, base)?.0.as_str() {
            "log" => Some(0),
            "atan" | "asin" | "acos" | "asinh" | "acosh" | "atanh" => Some(1),
            _ => None,
        }
    };
    let chosen = fs.iter().enumerate().filter_map(|(i, &g)| rank(g).map(|r| (r, i))).min();
    let (u, dv) = match chosen {
        Some((_, i)) => {
            let rest: Vec<T<'h>> = fs.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &g)| g).collect();
            (fs[i], b.mul(&rest))
        }
        None => {
            let (poly, rest): (Vec<T<'h>>, Vec<T<'h>>) = fs.iter().partition(|&&g| is_poly(b, g, x));
            if poly.is_empty() || rest.is_empty() {
                return Ok(None);
            }
            (b.mul(&poly), b.mul(&rest))
        }
    };
    let v = if dv == b.one() {
        x
    } else {
        match antiderivative(cx, b, dv, x, depth + 1)? {
            Some(v) => v,
            None => return Ok(None),
        }
    };
    let Some(du) = deriv(cx, b, u, x)? else { return Ok(None) };
    let Some(rest) = antiderivative(cx, b, b.mul(&[v, du]), x, depth + 1)? else { return Ok(None) };
    Ok(Some(b.sub(b.mul(&[u, v]), rest)))
}
