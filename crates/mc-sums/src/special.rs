//! Infinite sums of special-function terms and double series with closed forms:
//!
//! - **Tails of the Hurwitz zeta function**: `sum_(k >= lo) zeta(s, k + b) = zeta(s - 1, lo + b) - (lo + b - 1)
//!   zeta(s, lo + b)` for a number `s > 2`. Writing zeta(s, k + b) = sum_(j >= 0) (j + k + b)^-s and collecting the
//!   terms with j + k - lo = N, each N occurs N + 1 times: sum_N (N + 1)(N + lo + b)^-s, and N + 1 = (N + lo + b) -
//!   (lo + b - 1).
//! - **Linear Euler sums** `sum_(k >= lo) H_(k + d)^(p)/(k + e)^q` with integer shifts, `q >= 2`: the shift moved into
//!   the harmonic number (H_(m - c) = H_m - sum_(j < c) (m - j)^-p), which leaves S_(p,q) = sum_(m >= 1) H_m^(p)/m^q
//!   less finitely many terms and rational sums. S_(p,p) = (zeta(p)^2 + zeta(2 p))/2 (from the symmetry S_(p,q) +
//!   S_(q,p) = zeta(p) zeta(q) + zeta(p + q)) and Euler's S_(1,q) = (1 + q/2) zeta(q + 1) - (1/2) sum_(k=1)^(q-2)
//!   zeta(k + 1) zeta(q - k) (Flajolet and Salvy, *Euler sums and contour integral representations*, Experimental
//!   Mathematics 7 (1998), Theorem 2.2 and (5)); other orders are not done.
//! - **Appell's functions** as sums over m of Pochhammer symbols in m times a Gauss function: the inner sums of their
//!   double series (DLMF 16.13.1-16.13.4), so the double series summed over n first are recognised too.
use crate::cx::Sx;
use mc_api::{names, T};
use mc_eval::Res;

/// The factors of a product (the term itself when it is no product).
fn factors<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>) -> Vec<T<'h>> {
    match sx.args_of(t, names::MUL) {
        Some(a) => a.to_vec(),
        None => vec![t],
    }
}

/// `u - k` when it is free of k (u = k + b), else `None`.
fn shift<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>, k: T<'h>) -> Res<Option<T<'h>>> {
    let b = sx.sub(u, k)?;
    Ok(sx.free(b, k).then_some(b))
}

/// `sum(f, [k, lo, inf])` by the methods of this module.
pub fn infinite<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>) -> Res<Option<T<'h>>> {
    if !sx.free(lo, k) {
        return Ok(None);
    }
    let fs = factors(sx, f);
    let (dep, free): (Vec<T<'h>>, Vec<T<'h>>) = fs.iter().partition(|&&t| !sx.free(t, k));
    let c = sx.mul(&free)?;
    if let [z] = dep[..] {
        if let Some(v) = hurwitz_tail(sx, z, k, lo)? {
            return Ok(Some(sx.mul(&[c, v])?));
        }
    }
    if let [a, b] = dep[..] {
        for (hn, pw) in [(a, b), (b, a)] {
            if let Some(v) = euler_sum(sx, hn, pw, k, lo)? {
                return Ok(Some(sx.mul(&[c, v])?));
            }
        }
    }
    if lo == sx.int(0) {
        return appell(sx, f, k);
    }
    Ok(None)
}

/// `sum_(k >= lo) zeta(s, k + b)` (module comment).
fn hurwitz_tail<'h>(sx: &mut Sx<'_, '_, '_, 'h>, z: T<'h>, k: T<'h>, lo: T<'h>) -> Res<Option<T<'h>>> {
    let Some(&[s, u]) = sx.args_of(z, "zeta") else { return Ok(None) };
    if !sx.free(s, k) {
        return Ok(None);
    }
    let gt = sx.ev("greater", &[s, sx.int(2)])?;
    if !sx.is_true(gt) {
        return Ok(None);
    }
    let Some(b) = shift(sx, u, k)? else { return Ok(None) };
    let a = sx.add(&[lo, b])?;
    let s1 = sx.plus(s, -1)?;
    let z1 = sx.ev("zeta", &[s1, a])?;
    let z2 = sx.ev("zeta", &[s, a])?;
    let a1 = sx.plus(a, -1)?;
    let t = sx.mul(&[a1, z2])?;
    Ok(Some(sx.sub(z1, t)?))
}

/// S_(p,q) = sum_(m >= 1) H_m^(p)/m^q where a closed form is known (module comment).
fn euler_s<'h>(sx: &mut Sx<'_, '_, '_, 'h>, p: i64, q: i64) -> Res<Option<T<'h>>> {
    fn zeta<'h>(sx: &mut Sx<'_, '_, '_, 'h>, s: i64) -> Res<T<'h>> {
        let a = sx.int(s);
        sx.ev("zeta", &[a])
    }
    if q < 2 || p < 1 {
        return Ok(None);
    }
    if p == q {
        let zp = zeta(sx, p)?;
        let z2p = zeta(sx, 2 * p)?;
        let sq = sx.mul(&[zp, zp])?;
        let s = sx.add(&[sq, z2p])?;
        return Ok(Some(sx.div(s, sx.int(2))?));
    }
    if p == 1 {
        let zq1 = zeta(sx, q + 1)?;
        let coef = sx.div(sx.int(q + 2), sx.int(2))?;
        let mut parts = vec![sx.mul(&[coef, zq1])?];
        for j in 1..=q - 2 {
            let a = zeta(sx, j + 1)?;
            let b = zeta(sx, q - j)?;
            let half = sx.div(sx.int(-1), sx.int(2))?;
            parts.push(sx.mul(&[half, a, b])?);
        }
        return Ok(Some(sx.add(&parts)?));
    }
    Ok(None)
}

/// `sum_(k >= lo) H_(k + d)^(p) (k + e)^-q` (module comment).
fn euler_sum<'h>(sx: &mut Sx<'_, '_, '_, 'h>, hn: T<'h>, pw: T<'h>, k: T<'h>, lo: T<'h>) -> Res<Option<T<'h>>> {
    let (u, p) = match sx.args_of(hn, "harmonic_number") {
        Some(&[u]) => (u, 1),
        Some(&[u, p]) => match sx.as_int(p) {
            Some(p) if p >= 1 => (u, p),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    let Some(&[base, e]) = sx.args_of(pw, names::POW) else { return Ok(None) };
    let Some(q) = sx.as_int(e).map(|e| -e) else { return Ok(None) };
    let Some(lo) = sx.as_int(lo) else { return Ok(None) };
    let (Some(d), Some(e)) = (shift(sx, u, k)?, shift(sx, base, k)?) else { return Ok(None) };
    let (Some(d), Some(e)) = (sx.as_int(d), sx.as_int(e)) else { return Ok(None) };
    // n = k + d runs from n0; m = n + c = k + e from big_m
    let n0 = lo + d;
    let c = e - d;
    let big_m = lo + e;
    if n0 < 0 || !(1..=1000).contains(&big_m) || c.abs() > 1000 {
        return Ok(None);
    }
    let Some(s) = euler_s(sx, p, q)? else { return Ok(None) };
    let mut parts = vec![s];
    // the terms m < big_m of S_(p,q)
    for m in 1..big_m {
        let hm = sx.ev("harmonic_number", &[sx.int(m), sx.int(p)])?;
        let mq = sx.pow(sx.int(m), sx.int(-q))?;
        let t = sx.mul(&[sx.int(-1), hm, mq])?;
        parts.push(t);
    }
    // H_n = H_m - sum_(j < c) (m - j)^-p for c >= 0, H_m + sum_(j = 1..|c|) (m + j)^-p for c < 0: the rational sums
    // over m = k + e from k = lo
    let m_of_k = sx.plus(k, e)?;
    let inf = sx.sym("inf");
    let spec = sx.list(&[k, sx.int(lo), inf]);
    let mq = sx.pow(m_of_k, sx.int(-q))?;
    let js: Vec<(i64, i64)> = if c >= 0 { (0..c).map(|j| (-j, -1)).collect() } else { (1..=-c).map(|j| (j, 1)).collect() };
    for (off, sign) in js {
        let mj = sx.plus(m_of_k, off)?;
        let r = sx.pow(mj, sx.int(-p))?;
        let term = sx.mul(&[r, mq])?;
        let s = sx.ev("sum", &[term, spec])?;
        if sx.contains_any(s, &["sum"]) || sx.bad(s) {
            return Ok(None);
        }
        parts.push(sx.mul(&[sx.int(sign), s])?);
    }
    let s = sx.add(&parts)?;
    Ok(Some(sx.ev("expand", &[s])?))
}

/// `sum_(m >= 0) c x^m (a)_m (b)_m/(m! (c')_m) 2F1(u, v; w; y)` read as one of Appell's functions: the inner sums
/// of their double series (DLMF 16.13.1-4) over n are the Gauss functions F1: 2F1(b2, a + m; c + m; y), F2: 2F1(b2,
/// a + m; c2; y), F3: 2F1(a2, b2; c + m; y), F4: 2F1(a + m, b + m; c2; y), with the Pochhammer symbols in m outside;
/// `None` for other terms.
fn appell<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, m: T<'h>) -> Res<Option<T<'h>>> {
    let mut x = None;
    let mut fm = false;
    let mut num: Vec<T<'h>> = vec![];
    let mut den: Vec<T<'h>> = vec![];
    let mut gauss = None;
    let mut coef = vec![];
    for t in factors(sx, f) {
        if sx.free(t, m) {
            coef.push(t);
            continue;
        }
        if let Some(&[u, v, w, y]) = sx.args_of(t, "hypergeometric_2f1") {
            if gauss.is_some() || !sx.free(y, m) {
                return Ok(None);
            }
            gauss = Some((u, v, w, y));
            continue;
        }
        let (b, e) = match sx.args_of(t, names::POW) {
            Some(&[b, e]) => (b, e),
            _ => (t, sx.int(1)),
        };
        if e == m && sx.free(b, m) && x.is_none() {
            x = Some(b);
            continue;
        }
        let sign = match sx.as_int(e) {
            Some(1) => 1,
            Some(-1) => -1,
            _ => return Ok(None),
        };
        if let Some(&[a, l]) = sx.args_of(b, "pochhammer") {
            if l != m || !sx.free(a, m) {
                return Ok(None);
            }
            if sign == 1 {
                num.push(a)
            } else {
                den.push(a)
            }
            continue;
        }
        if sx.args_of(b, "factorial") == Some(&[m][..]) && sign == -1 && !fm {
            fm = true;
            continue;
        }
        return Ok(None);
    }
    let (Some(x), Some((u, v, w, y)), true) = (x, gauss, fm) else { return Ok(None) };
    // each parameter of the Gauss function as (base, shifted by m)
    let split = |sx: &mut Sx<'_, '_, '_, 'h>, p: T<'h>| -> Res<Option<(T<'h>, bool)>> {
        if sx.free(p, m) {
            return Ok(Some((p, false)));
        }
        let b = sx.sub(p, m)?;
        Ok(sx.free(b, m).then_some((b, true)))
    };
    let (Some(u), Some(v), Some(w)) = (split(sx, u)?, split(sx, v)?, split(sx, w)?) else { return Ok(None) };
    let [n1, n2] = num[..] else { return Ok(None) };
    let [d1] = den[..] else { return Ok(None) };
    // the upper parameter shifted by m is one of the Pochhammer symbols; `other` the remaining one
    let other = |a: T<'h>| if a == n1 { Some(n2) } else if a == n2 { Some(n1) } else { None };
    let call = match (u, v, w) {
        // F1 and F2: one upper parameter a + m with a among the symbols
        ((b2, false), (a, true), wc) | ((a, true), (b2, false), wc) => {
            let Some(b1) = other(a) else { return Ok(None) };
            match wc {
                (c, true) if c == d1 => sx.call("appell_f1", &[a, b1, b2, c, x, y]),
                (c2, false) => sx.call("appell_f2", &[a, b1, b2, d1, c2, x, y]),
                _ => return Ok(None),
            }
        }
        // F3: no upper parameter shifted, the lower one c + m
        ((a2, false), (b2, false), (c, true)) if c == d1 => {
            // F3 is symmetric in (a2, b2): the canonical order
            let l = sx.list(&[a2, b2]);
            let s = sx.ev("sort", &[l])?;
            let (a2, b2) = match sx.as_list(s) {
                Some(&[p, q]) => (p, q),
                _ => (a2, b2),
            };
            sx.call("appell_f3", &[n1, a2, n2, b2, c, x, y])
        }
        // F4: both upper parameters shifted, they are the two symbols
        ((a, true), (b, true), (c2, false)) if other(a) == Some(b) => {
            let (a, b) = if a == n1 { (a, b) } else { (b, a) };
            sx.call("appell_f4", &[a, b, d1, c2, x, y])
        }
        _ => return Ok(None),
    };
    coef.push(call);
    Ok(Some(sx.mul(&coef)?))
}
