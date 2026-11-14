//! An inverse function of its direct function at a numeric argument: `asin(sin(c))`, `acoth(coth(c))`, and the
//! other ten circular and hyperbolic pairs, for an exact numeric c (a number or a closed form such as `pi/3` or
//! `sqrt(2)`, real or complex).
//!
//! Each direct function is periodic and, for the even and the sine-like ones, symmetric, so f(c) = f(w) for one
//! representative w in the strip where the principal branch of the inverse inverts f (DLMF 4.23.i, 4.37.i):
//!
//! | pair | strip of w | w |
//! |---|---|---|
//! | atan(tan), acot(cot) | abs(re w) < pi/2 | c - k pi |
//! | asin(sin), acsc(csc) | abs(re w) < pi/2 | (-1)^k (c - k pi) |
//! | acos(cos), asec(sec) | 0 < re w < pi | +-(c - 2 k pi) |
//! | atanh(tanh), acoth(coth) | abs(im w) < pi/2 | c - k pi i |
//! | asinh(sinh), acsch(csch) | abs(im w) < pi/2 | (-1)^k (c - k pi i) |
//! | acosh(cosh), asech(sech) | re w > 0, abs(im w) < pi | +-(c - 2 k pi i) |
//!
//! with k the nearest integer to re c/pi (re c/(2 pi), im c/pi, im c/(2 pi)). On these open strips the direct
//! function is one to one onto the plane cut along the inverse's branch cuts, so the inverse gives w back; the
//! reciprocal pairs reduce to the others (acot(cot c) = atan(tan c) where cot c != 0, and so on). The position of w
//! is decided from 40-digit values; a w on (or within 10^-30 of) the edge of its strip, where the cut decides the
//! side, and arguments too large to place at 40 digits are left alone: the call stays.
use crate::formula::Fx;
use mc_api::T;
use mc_arith::Num;
use mc_eval::Res;

/// The shape of a pair.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// period pi, odd: w = c - k pi (tan, cot)
    Tan,
    /// period 2 pi, sin(pi - x) = sin(x): w = (-1)^k (c - k pi)
    Sin,
    /// period 2 pi, even: w = +-(c - 2 k pi)
    Cos,
}

/// The direct function of the inverse `outer`, its kind, and whether the period runs along the imaginary axis (the
/// hyperbolic functions).
fn pair(outer: &str) -> Option<(&'static str, Kind, bool)> {
    Some(match outer {
        "atan" => ("tan", Kind::Tan, false),
        "acot" => ("cot", Kind::Tan, false),
        "asin" => ("sin", Kind::Sin, false),
        "acsc" => ("csc", Kind::Sin, false),
        "acos" => ("cos", Kind::Cos, false),
        "asec" => ("sec", Kind::Cos, false),
        "atanh" => ("tanh", Kind::Tan, true),
        "acoth" => ("coth", Kind::Tan, true),
        "asinh" => ("sinh", Kind::Sin, true),
        "acsch" => ("csch", Kind::Sin, true),
        "acosh" => ("cosh", Kind::Cos, true),
        "asech" => ("sech", Kind::Cos, true),
        _ => return None,
    })
}

/// The 40-digit value of the real term `t` as an `f64`, if it is a finite real number.
fn value<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let v = fx.eval(fx.call("n", &[t, fx.int(40)]))?;
    Ok(match fx.num(v) {
        Some(Num::Real(r)) => Some(r.to_f64()).filter(|x| x.is_finite()),
        Some(Num::Complex(x, y)) if y.is_zero() => Some(x.to_f64()).filter(|x| x.is_finite()),
        _ => None,
    })
}

/// The edge margin: a reduced coordinate closer than this to an edge of its strip is not decided.
const EDGE: f64 = 1e-30;

/// `outer(x)` for x = f(c), f the direct function of `outer` and c an exact numeric term: the representative of c
/// in the strip of `outer` (module comment), or `None` (not such a call, c not numeric, or w on an edge).
pub fn reduce<'h>(fx: &mut Fx<'_, '_, '_, 'h>, outer: &str, x: T<'h>) -> Res<Option<T<'h>>> {
    let Some((inner, kind, hyperbolic)) = pair(outer) else { return Ok(None) };
    let Some(&[c]) = fx.call_of(x, inner) else { return Ok(None) };
    // an exact numeric argument (an inexact one has made f(c) a number already)
    if fx.num(c).is_some_and(|n| !n.is_exact()) {
        return Ok(None);
    }
    let probe = fx.eval(fx.call("n", &[c, fx.int(40)]))?;
    if fx.num(probe).is_none() {
        return Ok(None);
    }
    let pi = fx.sym("pi");
    // the coordinate along the period: re c, or im c for the hyperbolic functions, and the period's unit
    let along = fx.call(if hyperbolic { "im_part" } else { "re" }, &[c]);
    let across = fx.call(if hyperbolic { "re" } else { "im_part" }, &[c]);
    let unit = if hyperbolic { fx.mul(&[fx.i(), pi]) } else { pi };
    let period = if kind == Kind::Cos { 2 } else { 1 };
    // s = along/(period pi), k its nearest integer, f = s - k accurately
    let s_term = fx.div(along, fx.mul(&[fx.int(period), pi]));
    let Some(s) = value(fx, s_term)? else { return Ok(None) };
    if s.abs() > 1e15 {
        return Ok(None);
    }
    let k = s.round() as i64;
    let Some(f) = value(fx, fx.sub(s_term, fx.int(k)))? else { return Ok(None) };
    if (f.abs() - 0.5).abs() < EDGE || f.abs() > 0.5 {
        return Ok(None);
    }
    let shifted = fx.sub(c, fx.mul(&[fx.int(period * k), unit]));
    let w = match kind {
        // |f| < 1/2: the strip of tan and of sin directly
        Kind::Tan => shifted,
        Kind::Sin => {
            if k % 2 == 0 {
                shifted
            } else {
                fx.neg(shifted)
            }
        }
        Kind::Cos => {
            // the coordinate across the period decides the sign: re w > 0 (cosh), re w in (0, pi) (cos: the
            // reduced coordinate f in (0, 1/2) for w = d, (-1/2, 0) for w = -d)
            if hyperbolic {
                let Some(r) = value(fx, across)? else { return Ok(None) };
                if r.abs() < EDGE {
                    return Ok(None);
                }
                if r > 0.0 {
                    shifted
                } else {
                    fx.neg(shifted)
                }
            } else {
                if f.abs() < EDGE {
                    return Ok(None);
                }
                if f > 0.0 {
                    shifted
                } else {
                    fx.neg(shifted)
                }
            }
        }
    };
    // a zero of the direct function's reciprocal or the pole of the direct function (c = k pi for cot): the
    // value is not a number there, and the call is left as it is
    if matches!(outer, "acot" | "acsc" | "acoth" | "acsch") && f.abs() < EDGE {
        let Some(r) = value(fx, across)? else { return Ok(None) };
        if r.abs() < EDGE {
            return Ok(None);
        }
    }
    Ok(Some(fx.eval(w)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_inverse_has_its_pair() {
        for o in ["atan", "acot", "asin", "acsc", "acos", "asec", "atanh", "acoth", "asinh", "acsch", "acosh", "asech"] {
            assert!(pair(o).is_some(), "{o}");
        }
        assert!(pair("log").is_none());
    }
}
