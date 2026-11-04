//! Products of powers of trigonometric functions.
//!
//! `sin(u)^m cos(u)^n` of one linear argument `u` (tan, cot, sec, csc rewritten in sines and cosines; m, n
//! integers):
//!
//! - one exponent odd (the other one non-negative, or both odd): the substitution `t = cos(u)` (m odd) or
//!   `t = sin(u)` (n odd) with `sin^2 = 1 - t^2`, `cos^2 = 1 - t^2`, which leaves a polynomial (or a rational
//!   function) of `t`;
//! - both non-negative and even: `trig_reduce` into multiple angles;
//! - a pure negative power: the reduction formulas
//!   `∫ cos^-k = sin cos^(1-k)/(k-1) + (k-2)/(k-1) ∫ cos^(2-k)`, `∫ sin^-k = -cos sin^(1-k)/(k-1) + ...`;
//! - both even, one negative: the positive one rewritten by `cos^2 = 1 - sin^2` (or the converse) and expanded;
//! - both negative and even: `sin^m cos^n = sin^(m+2) cos^n + sin^m cos^(n+2)`.
//!
//! Products of sines and cosines (positive powers) of different linear arguments, possibly times a polynomial,
//! go through `trig_reduce`.
use super::{factors, fresh, int_of, is_poly, normalize, pow_parts, slope, subst, unary};
use crate::integrate::{antiderivative, apply, is_free};
use crate::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// The largest |exponent| handled.
const MAX_POWER: i64 = 200;

/// An antiderivative of a product of trigonometric powers, else `None`.
pub fn integrate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let fs = factors(b, f);
    let mut arg: Option<T<'h>> = None;
    let mut same = true;
    let (mut m, mut n) = (0i64, 0i64);
    let mut trig_degree = 0i64;
    let mut sin_cos_only = true;
    let mut has_poly = false;
    for &g in &fs {
        let (base, k) = pow_parts(b, g);
        let Some(k) = int_of(b, k) else { return Ok(None) };
        if k.abs() > MAX_POWER {
            return Ok(None);
        }
        let Some((name, u)) = unary(b, base) else {
            if is_poly(b, g, x) {
                has_poly = true;
                continue;
            }
            return Ok(None);
        };
        let (dm, dn) = match name.as_str() {
            "sin" => (1, 0),
            "cos" => (0, 1),
            "tan" => (1, -1),
            "cot" => (-1, 1),
            "sec" => (0, -1),
            "csc" => (-1, 0),
            _ => return Ok(None),
        };
        if is_free(b, u, x) {
            return Ok(None);
        }
        sin_cos_only &= dm + dn == 1 && k > 0;
        trig_degree += k.abs();
        m += dm * k;
        n += dn * k;
        match arg {
            None => arg = Some(u),
            Some(a) if a == u => {}
            Some(_) => same = false,
        }
    }
    let Some(u) = arg else { return Ok(None) };
    if !same || has_poly {
        // sines and cosines of different arguments, or times a polynomial: multiple angles
        if !sin_cos_only || trig_degree < 2 {
            return Ok(None);
        }
        let g = apply(cx, b, "trig_reduce", &[f])?;
        if g == f {
            return Ok(None);
        }
        return antiderivative(cx, b, g, x, depth + 1);
    }
    let Some(a) = slope(cx, b, u, x)? else { return Ok(None) };
    if m == 0 && n == 0 {
        return Ok(None);
    }
    let w = fresh(b, "w", depth);
    let Some(r) = monomial(cx, b, m, n, w, depth)? else { return Ok(None) };
    Ok(Some(b.div(subst(b, r, w, u), a)))
}

/// `∫ sin(w)^m cos(w)^n dw`.
fn monomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, m: i64, n: i64, w: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    cx.tick(1)?;
    let (sin, cos) = (b.call("sin", &[w]), b.call("cos", &[w]));
    let odd = |k: i64| k.rem_euclid(2) == 1;
    if n == 0 && m < 0 {
        return reduction(cx, b, -m, w, true).map(Some);
    }
    if m == 0 && n < 0 {
        return reduction(cx, b, -n, w, false).map(Some);
    }
    // a negative power times a positive even one: cos^2 = 1 - sin^2 (or the converse), expanded
    if (m < 0 && n > 0 && !odd(n)) || (n < 0 && m > 0 && !odd(m)) {
        let g = if m < 0 {
            let c2 = b.sub(b.one(), b.powi(sin, 2));
            b.mul(&[b.powi(sin, m), b.powi(c2, n / 2)])
        } else {
            let s2 = b.sub(b.one(), b.powi(cos, 2));
            b.mul(&[b.powi(s2, m / 2), b.powi(cos, n)])
        };
        let g = apply(cx, b, "expand", &[g])?;
        return antiderivative(cx, b, g, w, depth + 1);
    }
    // odd powers: a substitution; prefer the one that leaves a polynomial in t (and powers of t)
    if odd(m) || odd(n) {
        // t = cos(u) takes one sin(u) as dt and leaves a polynomial when m ≥ 1; the smaller odd power goes
        let pick_cos = match (odd(m), odd(n)) {
            (true, true) => match (m >= 1, n >= 1) {
                (true, true) => m <= n,
                (c, s) => c || !s,
            },
            (c, _) => c,
        };
        let t = fresh(b, "t", depth);
        let one_minus = b.sub(b.one(), b.powi(t, 2));
        // sin^m cos^n dw = -(1 - t^2)^((m-1)/2) t^n dt (t = cos w), or t^m (1 - t^2)^((n-1)/2) dt (t = sin w)
        let (g, back) = if pick_cos {
            (b.neg(b.mul(&[b.powi(one_minus, (m - 1) / 2), b.powi(t, n)])), cos)
        } else {
            (b.mul(&[b.powi(t, m), b.powi(one_minus, (n - 1) / 2)]), sin)
        };
        let g = apply(cx, b, "expand", &[g])?;
        return Ok(antiderivative(cx, b, g, t, depth + 1)?.map(|r| subst(b, r, t, back)));
    }
    if m >= 0 && n >= 0 {
        let g = apply(cx, b, "trig_reduce", &[b.mul(&[b.powi(sin, m), b.powi(cos, n)])])?;
        return antiderivative(cx, b, g, w, depth + 1);
    }
    // both negative and even: 1 = sin^2 + cos^2
    let g = b.add(&[b.mul(&[b.powi(sin, m + 2), b.powi(cos, n)]), b.mul(&[b.powi(sin, m), b.powi(cos, n + 2)])]);
    antiderivative(cx, b, g, w, depth + 1)
}

/// `∫ sin(w)^-k dw` (`of_sin`) or `∫ cos(w)^-k dw`, k ≥ 1, by the reduction formula.
fn reduction<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, k: i64, w: T<'h>, of_sin: bool) -> Res<T<'h>> {
    let (sin, cos) = (b.call("sin", &[w]), b.call("cos", &[w]));
    let mut terms = Vec::new();
    let mut coef = b.one();
    let mut k = k;
    while k >= 2 {
        cx.tick(1)?;
        let c = b.rat(1, k - 1);
        let t = if of_sin {
            b.neg(b.mul(&[cos, b.powi(sin, 1 - k)]))
        } else {
            b.mul(&[sin, b.powi(cos, 1 - k)])
        };
        terms.push(b.mul(&[coef, c, t]));
        coef = b.mul(&[coef, b.rat(k - 2, k - 1)]);
        k -= 2;
    }
    if k == 1 {
        let last = if of_sin {
            b.call("log", &[b.call("tan", &[b.mul(&[b.rat(1, 2), w])])])
        } else {
            b.call("log", &[b.div(b.add(&[b.one(), sin]), cos)])
        };
        terms.push(b.mul(&[coef, last]));
    }
    Ok(b.add(&terms))
}

/// Products with positive powers of `sinh` and `cosh` of total degree at least 2 (with any other factors):
/// `sinh(u) = (exp(u) - exp(-u))/2`, `cosh(u) = (exp(u) + exp(-u))/2`, expanded.
pub fn hyperbolic<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let mut degree = 0;
    for g in factors(b, f) {
        let (base, k) = pow_parts(b, g);
        match unary(b, base) {
            Some((n, _)) if n == "sinh" || n == "cosh" => match int_of(b, k) {
                Some(k @ 1..=MAX_POWER) => degree += k,
                _ => return Ok(None),
            },
            _ => {}
        }
    }
    if degree < 2 {
        return Ok(None);
    }
    let half = b.rat(1, 2);
    let g = super::rewrite(b, f, &mut |s| {
        let (n, u) = unary(b, s)?;
        let sg = match n.as_str() {
            "sinh" => -1,
            "cosh" => 1,
            _ => return None,
        };
        // exp(-u) as exp(u)^-1, so that expanding cancels exp(u) exp(u)^-1
        let p = b.call("exp", &[u]);
        Some(b.mul(&[half, b.add(&[p, b.mul(&[b.int(sg), b.powi(p, -1)])])]))
    });
    let g = normalize(b, apply(cx, b, "expand", &[g])?);
    if g == f {
        return Ok(None);
    }
    antiderivative(cx, b, g, x, depth + 1)
}
