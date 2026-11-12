//! The table: one function of a linear argument, powers of a linear or quadratic base, and `exp(u) sin(v)`.
use super::{deriv, factors, sign, slope, subst, unary};
use crate::{args_of, is_free};
use mc_calculus::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// An antiderivative of `f` from the table, else `None`.
pub fn single<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    if let Some([base, ex]) = args_of(b, f, b.hd.pow) {
        let (base, ex) = (*base, *ex);
        if is_free(b, base, x) {
            // k^u = exp(u log k)
            let Some(a) = slope(cx, b, ex, x)? else { return Ok(None) };
            return Ok(Some(b.div(f, b.mul(&[a, b.call("log", &[base])]))));
        }
        if !is_free(b, ex, x) {
            return Ok(None);
        }
        if let Some(a) = slope(cx, b, base, x)? {
            return Ok(Some(power(b, base, ex, a)));
        }
        return quadratic(cx, b, base, ex, x);
    }
    if let Some((name, u)) = unary(b, f) {
        let Some(a) = slope(cx, b, u, x)? else { return Ok(None) };
        return Ok(of_linear(b, &name, u).map(|r| b.div(r, a)));
    }
    if let Some(r) = exp_trig(cx, b, f, x)? {
        return Ok(Some(r));
    }
    Ok(None)
}

/// `∫ u^n dx` for `u` of slope `a`.
pub fn power<'h>(b: &Build<'h>, u: T<'h>, n: T<'h>, a: T<'h>) -> T<'h> {
    if n == b.int(-1) {
        return b.div(b.call("log", &[u]), a);
    }
    let n1 = b.add(&[n, b.one()]);
    b.div(b.pow(u, n1), b.mul(&[n1, a]))
}

/// `∫ name(u) du`, from the table of functions of one argument.
pub fn of_linear<'h>(b: &Build<'h>, name: &str, u: T<'h>) -> Option<T<'h>> {
    let c = |n: &str, a: T<'h>| b.call(n, &[a]);
    let sq = |s: i64, k: i64| {
        // (s + k u^2)^(1/2)
        b.pow(b.add(&[b.int(s), b.mul(&[b.int(k), b.powi(u, 2)])]), b.rat(1, 2))
    };
    // u f(u) + rest
    let by_parts = |f: &str, rest: T<'h>| b.add(&[b.mul(&[u, c(f, u)]), rest]);
    Some(match name {
        "exp" => c("exp", u),
        "sin" => b.neg(c("cos", u)),
        "cos" => c("sin", u),
        "tan" => b.neg(c("log", c("cos", u))),
        "cot" => c("log", c("sin", u)),
        // log(sec u + tan u), log(tan(u/2)): forms with derivative rules
        "sec" => c("log", b.div(b.add(&[b.one(), c("sin", u)]), c("cos", u))),
        "csc" => c("log", c("tan", b.mul(&[b.rat(1, 2), u]))),
        "sinh" => c("cosh", u),
        "cosh" => c("sinh", u),
        "tanh" => c("log", c("cosh", u)),
        "log" => b.sub(b.mul(&[u, c("log", u)]), u),
        "atan" => by_parts("atan", b.mul(&[b.rat(-1, 2), c("log", b.add(&[b.one(), b.powi(u, 2)]))])),
        "asin" => by_parts("asin", sq(1, -1)),
        "acos" => by_parts("acos", b.neg(sq(1, -1))),
        "asinh" => by_parts("asinh", b.neg(sq(1, 1))),
        "acosh" => by_parts("acosh", b.neg(sq(-1, 1))),
        "atanh" => by_parts("atanh", b.mul(&[b.rat(1, 2), c("log", b.add(&[b.one(), b.neg(b.powi(u, 2))]))])),
        _ => return None,
    })
}

/// `∫ p^e dx` for a quadratic `p = A x^2 + B x + C` and `e` in -1, -1/2, 1/2, -3/2, when the signs the formula
/// needs are known.
fn quadratic<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>, e: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let which = [(-1, 1), (-1, 2), (1, 2), (-3, 2)].into_iter().find(|&(n, d)| e == b.rat(n, d));
    let Some(which) = which else { return Ok(None) };
    let Some(l) = deriv(cx, b, p, x)? else { return Ok(None) }; // L = 2 A x + B
    let Some(d2) = deriv(cx, b, l, x)? else { return Ok(None) };
    if b.is_zero(d2) || !is_free(b, d2, x) {
        return Ok(None);
    }
    let a = b.mul(&[b.rat(1, 2), d2]);
    let bb = subst(b, l, x, b.zero());
    let c = subst(b, p, x, b.zero());
    // D = 4 A C - B^2
    let disc = b.sub(b.mul(&[b.int(4), a, c]), b.powi(bb, 2));
    let half = b.rat(1, 2);
    let sqrt = |t: T<'h>| b.pow(t, half);
    let inv_sqrt = |cx: &mut NativeCx<'_, '_, 'h>| -> Res<Option<T<'h>>> {
        match sign(cx, b, a)? {
            Some(1) => {
                // log(L/(2 sqrt A) + sqrt p)/sqrt A
                let sa = sqrt(a);
                let arg = b.add(&[b.div(l, b.mul(&[b.int(2), sa])), sqrt(p)]);
                Ok(Some(b.div(b.call("log", &[arg]), sa)))
            }
            Some(-1) if sign(cx, b, disc)? == Some(-1) => {
                // asin(-L/sqrt(-D))/sqrt(-A)
                let arg = b.div(b.neg(l), sqrt(b.neg(disc)));
                Ok(Some(b.div(b.call("asin", &[arg]), sqrt(b.neg(a)))))
            }
            _ => Ok(None),
        }
    };
    Ok(match which {
        (-1, 1) => match sign(cx, b, disc)? {
            Some(1) => {
                let s = sqrt(disc);
                Some(b.mul(&[b.int(2), b.div(b.call("atan", &[b.div(l, s)]), s)]))
            }
            Some(-1) => {
                let s = sqrt(b.neg(disc));
                Some(b.div(b.call("log", &[b.div(b.sub(l, s), b.add(&[l, s]))]), s))
            }
            Some(_) => Some(b.div(b.int(-2), l)),
            None => None,
        },
        (-1, 2) => inv_sqrt(cx)?,
        (1, 2) => {
            // L sqrt(p)/(4 A) + D/(8 A) ∫ p^(-1/2)
            let Some(i) = inv_sqrt(cx)? else { return Ok(None) };
            let first = b.div(b.mul(&[l, sqrt(p)]), b.mul(&[b.int(4), a]));
            Some(b.add(&[first, b.mul(&[b.div(disc, b.mul(&[b.int(8), a])), i])]))
        }
        _ => match sign(cx, b, disc)? {
            // 2 L/(D sqrt p)
            Some(s) if s != 0 => Some(b.div(b.mul(&[b.int(2), l]), b.mul(&[disc, sqrt(p)]))),
            _ => None,
        },
    })
}

/// The exponential factor of `exp(u) sin(v)`: `(the factor, the slope of its exponent)` for `exp(u)` or `k^u`
/// with `u` linear.
pub fn exp_like<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    if let Some(("exp", u)) = unary(b, g).as_ref().map(|(n, u)| (n.as_str(), *u)) {
        return slope(cx, b, u, x);
    }
    if let Some([base, u]) = args_of(b, g, b.hd.pow) {
        if is_free(b, *base, x) {
            return Ok(slope(cx, b, *u, x)?.map(|a| b.mul(&[a, b.call("log", &[*base])])));
        }
    }
    Ok(None)
}

/// `∫ exp(u) sin(v)` and `∫ exp(u) cos(v)` (u, v linear of slopes p, q):
/// `exp(u) (p sin v - q cos v)/(p^2 + q^2)`, `exp(u) (p cos v + q sin v)/(p^2 + q^2)`.
fn exp_trig<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let fs = factors(b, f);
    let [g, h] = fs[..] else { return Ok(None) };
    for (e, t) in [(g, h), (h, g)] {
        let Some((name, v)) = unary(b, t) else { continue };
        if name != "sin" && name != "cos" {
            continue;
        }
        let Some(p) = exp_like(cx, b, e, x)? else { continue };
        let Some(q) = slope(cx, b, v, x)? else { continue };
        let (s, c) = (b.call("sin", &[v]), b.call("cos", &[v]));
        let r = b.add(&[b.powi(p, 2), b.powi(q, 2)]);
        let comb = if name == "sin" {
            b.sub(b.mul(&[p, s]), b.mul(&[q, c]))
        } else {
            b.add(&[b.mul(&[p, c]), b.mul(&[q, s])])
        };
        return Ok(Some(b.div(b.mul(&[e, comb]), r)));
    }
    Ok(None)
}
