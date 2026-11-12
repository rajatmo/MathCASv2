//! Hypergeometric terms: `t(k)` whose ratio `t(k + 1) / t(k)` is a rational function of `k`.
//!
//! The ratio is found factor by factor (Petkovšek, Wilf and Zeilberger, *A = B*, chapter 5): rational functions of
//! `k`, powers `c^(a k + b)` and `exp(a k + b)`, `factorial`, `gamma`, `pochhammer` and `binomial` of arguments
//! linear in `k` with integer slopes, products and integer powers of these. Anything else is not recognised.
use crate::cx::Sx;
use mc_api::{names, Store, View, T};
use mc_eval::Res;

/// The integer slope `u(k + 1) - u(k)` of an argument linear in `k`.
pub fn slope<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>, k: T<'h>) -> Res<Option<i64>> {
    if sx.free(u, k) {
        return Ok(Some(0));
    }
    let k1 = sx.plus(k, 1)?;
    let u1 = sx.subst(u, k, k1)?;
    let d = sx.sub(u1, u)?;
    let d = sx.ev("expand", &[d])?;
    Ok(sx.as_int(d))
}

/// `prod_{i=0}^{s-1} (u + i)` (s > 0), or its inverse over `prod_{i=1}^{-s} (u - i)` (s < 0): the ratio
/// `gamma(u + s) / gamma(u)`.
fn gamma_ratio<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>, s: i64) -> Res<Option<T<'h>>> {
    if s.abs() > 64 {
        return Ok(None);
    }
    let mut fs = vec![];
    if s > 0 {
        for i in 0..s {
            fs.push(sx.plus(u, i)?);
        }
        return Ok(Some(sx.mul(&fs)?));
    }
    for i in 1..=-s {
        fs.push(sx.plus(u, -i)?);
    }
    let p = sx.mul(&fs)?;
    Ok(Some(sx.pow(p, sx.int(-1))?))
}

/// The ratio `t(k + 1) / t(k)` as a term (not yet a normal form), or `None` when `t` is not recognised as
/// hypergeometric in `k`.
pub fn ratio<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, k: T<'h>) -> Res<Option<T<'h>>> {
    ratio_depth(sx, t, k, 0)
}

fn ratio_depth<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, k: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    sx.cx.tick(1)?;
    if depth > 40 {
        return Ok(None);
    }
    if sx.free(t, k) {
        return Ok(Some(sx.int(1)));
    }
    let h = sx.h();
    if t == k {
        let k1 = sx.plus(k, 1)?;
        return Ok(Some(sx.div(k1, k)?));
    }
    let View::App { head, args } = h.view(t) else { return Ok(None) };
    let name = match h.view(head) {
        View::Sym(s) => h.sym_name(s),
        _ => return Ok(None),
    };
    match (name.as_str(), args) {
        (names::MUL, _) => {
            let mut out = vec![];
            for &a in args {
                out.push(crate::some!(ratio_depth(sx, a, k, depth + 1)?));
            }
            Ok(Some(sx.mul(&out)?))
        }
        (names::ADD, _) => {
            // a polynomial (or rational function) in k: its own shift over itself
            if !rational_in(sx, t, k)? {
                return Ok(None);
            }
            let k1 = sx.plus(k, 1)?;
            let t1 = sx.subst(t, k, k1)?;
            Ok(Some(sx.div(t1, t)?))
        }
        (names::POW, &[b, e]) => {
            let bf = sx.free(b, k);
            let ef = sx.free(e, k);
            if !bf && ef {
                if sx.as_int(e).is_none() {
                    return Ok(None);
                }
                let r = crate::some!(ratio_depth(sx, b, k, depth + 1)?);
                return Ok(Some(sx.pow(r, e)?));
            }
            if bf && !ef {
                // c^(e(k+1) - e(k)) with the difference free of k
                let k1 = sx.plus(k, 1)?;
                let e1 = sx.subst(e, k, k1)?;
                let d = sx.sub(e1, e)?;
                let d = sx.ev("expand", &[d])?;
                if !sx.free(d, k) {
                    return Ok(None);
                }
                return Ok(Some(sx.pow(b, d)?));
            }
            Ok(None)
        }
        ("exp", &[u]) => {
            let k1 = sx.plus(k, 1)?;
            let u1 = sx.subst(u, k, k1)?;
            let d = sx.sub(u1, u)?;
            let d = sx.ev("expand", &[d])?;
            if !sx.free(d, k) {
                return Ok(None);
            }
            Ok(Some(sx.ev("exp", &[d])?))
        }
        ("factorial", &[u]) => {
            let s = crate::some!(slope(sx, u, k)?);
            let u1 = sx.plus(u, 1)?;
            gamma_ratio(sx, u1, s)
        }
        ("gamma", &[u]) => {
            let s = crate::some!(slope(sx, u, k)?);
            gamma_ratio(sx, u, s)
        }
        ("pochhammer", &[x, y]) => {
            // gamma(x + y) / gamma(x)
            let xy = sx.add(&[x, y])?;
            let s1 = crate::some!(slope(sx, xy, k)?);
            let s2 = crate::some!(slope(sx, x, k)?);
            let a = crate::some!(gamma_ratio(sx, xy, s1)?);
            let b = crate::some!(gamma_ratio(sx, x, s2)?);
            Ok(Some(sx.div(a, b)?))
        }
        ("binomial", &[u, v]) => {
            // gamma(u + 1) / (gamma(v + 1) gamma(u - v + 1))
            let u1 = sx.plus(u, 1)?;
            let v1 = sx.plus(v, 1)?;
            let w = sx.sub(u1, v)?;
            let (su, sv, sw) = (slope(sx, u1, k)?, slope(sx, v1, k)?, slope(sx, w, k)?);
            let (su, sv, sw) = (crate::some!(su), crate::some!(sv), crate::some!(sw));
            let a = crate::some!(gamma_ratio(sx, u1, su)?);
            let b = crate::some!(gamma_ratio(sx, v1, sv)?);
            let c = crate::some!(gamma_ratio(sx, w, sw)?);
            let bc = sx.mul(&[b, c])?;
            Ok(Some(sx.div(a, bc)?))
        }
        ("factorial_power", &[x, m]) => {
            // gamma(x + 1) / gamma(x - m + 1)
            let x1 = sx.plus(x, 1)?;
            let w = sx.sub(x1, m)?;
            let (s1, s2) = (slope(sx, x1, k)?, slope(sx, w, k)?);
            let (s1, s2) = (crate::some!(s1), crate::some!(s2));
            let a = crate::some!(gamma_ratio(sx, x1, s1)?);
            let b = crate::some!(gamma_ratio(sx, w, s2)?);
            Ok(Some(sx.div(a, b)?))
        }
        ("catalan_number", &[u]) => {
            // gamma(2 u + 1) / (gamma(u + 1) gamma(u + 2))
            let u2 = sx.mul(&[sx.int(2), u])?;
            let a1 = sx.plus(u2, 1)?;
            let b1 = sx.plus(u, 1)?;
            let c1 = sx.plus(u, 2)?;
            let (sa, sb) = (slope(sx, a1, k)?, slope(sx, b1, k)?);
            let (sa, sb) = (crate::some!(sa), crate::some!(sb));
            let a = crate::some!(gamma_ratio(sx, a1, sa)?);
            let b = crate::some!(gamma_ratio(sx, b1, sb)?);
            let c = crate::some!(gamma_ratio(sx, c1, sb)?);
            let bc = sx.mul(&[b, c])?;
            Ok(Some(sx.div(a, bc)?))
        }
        // an indefinite product prod_{j<k} g(j): the ratio g(k)
        ("product", &[g, spec]) if spec == k => Ok(Some(g)),
        _ => Ok(None),
    }
}

/// Is `t` a rational function of `k` (every kernel other than `k` free of `k`)?
pub fn rational_in<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, k: T<'h>) -> Res<bool> {
    let stop = sx.stop.clone();
    let mut b = crate::cx::Budget::new(&stop, 5);
    let mut m = mc_poly::work::Meter::new(&mut b);
    let Ok(ring) = sx.ring(&[t], &[k], &mut m) else { return Ok(false) };
    Ok(sx.clean(&ring, &[k]))
}
