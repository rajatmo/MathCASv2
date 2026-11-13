//! `generating_function(f, n, z) = sum_{n >= 0} f(n) z^n`.
//!
//! Linear in `f` (constant factors and sums); an unknown sequence `g(n + s)` gives the shift rules
//! `(G - g(0) - ... - g(s - 1) z^(s-1)) / z^s` and `z^(-s) G + ...` with `G = generating_function(g(n), n, z)`
//! left unevaluated; `b^n g(n)` is `G` at `b z`, `n g(n)` is `z G'`. Known terms are summed by `sum` (geometric,
//! hypergeometric and rational series), the result in factored form.
use crate::cx::Sx;
use crate::{hyper, sumfn};
use mc_api::{names, Store, View, T};
use mc_eval::Res;

/// Is `t` an unknown sequence `g(n + s)` (a call of a symbol without rules, argument `n + s`)? Its `(g, s)`.
fn unknown<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, n: T<'h>) -> Res<Option<(T<'h>, i64)>> {
    let h = sx.h();
    let View::App { head, args: &[a] } = h.view(t) else { return Ok(None) };
    if !matches!(h.view(head), View::Sym(_)) || sx.free(a, n) {
        return Ok(None);
    }
    // a known term has a ratio (the shift rules hold for any sequence, but known ones are summed)
    if hyper::ratio(sx, t, n)?.is_some() {
        return Ok(None);
    }
    let d = sx.sub(a, n)?;
    let d = sx.ev("expand", &[d])?;
    Ok(sx.as_int(d).map(|s| (head, s)))
}

/// The generating function, or `None` (left unevaluated).
pub fn gf<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, n: T<'h>, z: T<'h>) -> Res<Option<T<'h>>> {
    let h = sx.h();
    let unevaluated = |sx: &Sx<'_, '_, '_, 'h>, g: T<'h>, z: T<'h>| sx.call("generating_function", &[g, n, z]);
    if sx.free(f, n) {
        let d = sx.sub(sx.int(1), z)?;
        return Ok(Some(sx.div(f, d)?));
    }
    // discrete_delta(n - k) is 1 at n = k (z^k, or 0 when k < 0); unit_step(n - k) is 1 for n >= k (z^k/(1 - z), with
    // k clamped to 0)
    if let View::App { head, args: &[a] } = h.view(f) {
        let name = match h.view(head) {
            View::Sym(s) => h.sym_name(s),
            _ => String::new(),
        };
        if matches!(name.as_str(), "discrete_delta" | "unit_step") && !sx.free(a, n) {
            let d = sx.sub(n, a)?;
            let d = sx.ev("expand", &[d])?;
            if let Some(k) = sx.as_int(d) {
                if name.as_str() == "discrete_delta" {
                    return Ok(Some(if k < 0 { sx.int(0) } else { sx.pow(z, sx.int(k))? }));
                }
                let zk = sx.pow(z, sx.int(k.max(0)))?;
                let den = sx.sub(sx.int(1), z)?;
                return Ok(Some(sx.div(zk, den)?));
            }
        }
    }
    if let Some((g, s)) = unknown(sx, f, n)? {
        let gn = h.app(g, &[n]);
        let big = unevaluated(sx, gn, z);
        if s == 0 {
            return Ok(Some(big));
        }
        let mut terms = vec![];
        if s > 0 {
            // (G - sum_{j<s} g(j) z^j) / z^s
            terms.push(big);
            for j in 0..s {
                let gj = h.app(g, &[sx.int(j)]);
                let zj = sx.pow(z, sx.int(j))?;
                terms.push(sx.mul(&[sx.int(-1), gj, zj])?);
            }
            let sum = sx.add(&terms)?;
            let zs = sx.pow(z, sx.int(-s))?;
            let prod = sx.mul(&[sum, zs])?;
            return Ok(Some(sx.ev("expand", &[prod])?));
        }
        // z^(-s) G + sum_{j=s}^{-1} g(j) z^(j - s)
        let zs = sx.pow(z, sx.int(-s))?;
        terms.push(sx.mul(&[zs, big])?);
        for j in s..0 {
            let gj = h.app(g, &[sx.int(j)]);
            let zj = sx.pow(z, sx.int(j - s))?;
            terms.push(sx.mul(&[gj, zj])?);
        }
        return Ok(Some(sx.add(&terms)?));
    }
    if let View::App { head, args } = h.view(f) {
        if head == sx.sym(names::ADD) {
            let mut out = vec![];
            for &a in args {
                out.push(match gf(sx, a, n, z)? {
                    Some(v) => v,
                    None => unevaluated(sx, a, z),
                });
            }
            return Ok(Some(sx.add(&out)?));
        }
        if head == sx.sym(names::MUL) {
            let consts: Vec<T<'h>> = args.iter().copied().filter(|&a| sx.free(a, n)).collect();
            if !consts.is_empty() {
                let rest: Vec<T<'h>> = args.iter().copied().filter(|&a| !sx.free(a, n)).collect();
                let r = sx.mul(&rest)?;
                let c = sx.mul(&consts)?;
                return Ok(match gf(sx, r, n, z)? {
                    Some(v) => Some(sx.mul(&[c, v])?),
                    None => Some(sx.mul(&[c, unevaluated(sx, r, z)])?),
                });
            }
            // b^n g(n): G at b z; n g(n): z G'
            let formal = args.iter().any(|&a| matches!(h.view(a), View::App { args: &[x], .. } if !sx.free(x, n)) && sx.args_of(a, names::POW).is_none());
            if formal {
                // a polynomial factor in n that is a sum, (n + 1) g(n): by linearity after expanding
                if args.iter().any(|&a| sx.args_of(a, names::ADD).is_some() && !sx.free(a, n)) {
                    let e = sx.ev("expand", &[f])?;
                    if e != f && sx.args_of(e, names::ADD).is_some() {
                        return gf(sx, e, n, z);
                    }
                }
                for (i, &a) in args.iter().enumerate() {
                    let rest: Vec<T<'h>> = args.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, &x)| x).collect();
                    let r = sx.mul(&rest)?;
                    // n^k g(n): (z d/dz)^k G (the derivative of a held G stays as d(G, z))
                    let k = if a == n {
                        1
                    } else {
                        match sx.args_of(a, names::POW) {
                            Some(&[b, e]) if b == n => sx.as_int(e).filter(|&k| (1..=10).contains(&k)).unwrap_or(0),
                            _ => 0,
                        }
                    };
                    if k > 0 {
                        let mut v = match gf(sx, r, n, z)? {
                            Some(v) => v,
                            None => return Ok(None),
                        };
                        for _ in 0..k {
                            let dv = sx.ev("d", &[v, z])?;
                            v = sx.mul(&[z, dv])?;
                        }
                        return Ok(Some(sx.ev("expand", &[v])?));
                    }
                    if let Some(&[b, e]) = sx.args_of(a, names::POW) {
                        // b^(c n): G at b^c z
                        let c = sx.div(e, n)?;
                        if sx.free(b, n) && sx.free(c, n) {
                            let bc = sx.pow(b, c)?;
                            let bz = sx.mul(&[bc, z])?;
                            return gf(sx, r, n, bz);
                        }
                    }
                }
                return Ok(None);
            }
        }
    }
    // a known term: the series
    if hyper::ratio(sx, f, n)?.is_none() && !hyper::rational_in(sx, f, n)? {
        return Ok(None);
    }
    let zn = sx.pow(z, n)?;
    let t = sx.mul(&[f, zn])?;
    let spec = sx.list(&[n, sx.int(0), sx.sym("inf")]);
    let s = sx.ev("sum", &[t, spec])?;
    if sx.contains_any(s, &["sum"]) || sx.bad(s) {
        return Ok(None);
    }
    let fs = sx.ev("factor", &[s])?;
    let best = if sx.bad(fs) { s } else { sumfn::sign_normal(sx, fs)? };
    Ok(Some(best))
}
