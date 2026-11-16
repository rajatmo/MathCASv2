//! Alternating series `sum_(k >= lo) s (-1)^k g(k)` with `g` a proper rational function of `k`.
//!
//! The terms tend to 0, so the series converges and equals the series of its consecutive pairs
//! `s (-1)^lo sum_(j >= 0) (g(lo + 2j) - g(lo + 2j + 1))`, whose summand is rational in `j` and `O(j^-2)`: the
//! rational methods of `sumfn` (Hurwitz `zeta` and `digamma` of the partial fractions) give its value. The sign
//! factor may be written `(-1)^(a k + b)` with an odd integer `a`, or `cos(theta(k))`, `sin(theta(k))` with
//! `theta(k + 1) - theta(k)` an odd multiple of `pi` (then `cos(theta(k)) = (-1)^k cos(theta(0))`).
//!
//! The values come out as Hurwitz zeta functions and digamma functions at quarter, third and sixth points; two
//! classical evaluations make them closed forms:
//! - Gauss's digamma theorem, `psi(p/q) = -gamma - log(2q) - (pi/2) cot(pi p/q) + 2 sum_(n = 1)^(ceil(q/2) - 1)
//!   cos(2 pi n p/q) log sin(pi n/q)` for `0 < p < q` (Abramowitz and Stegun 6.3.8; Gauss 1813), with
//!   `psi(x + 1) = psi(x) + 1/x` for the other rational points; used for `q` in 2, 3, 4, 6, where every sine is a
//!   square root;
//! - Dirichlet's beta function at odd integers: `zeta(s, 1/4) - zeta(s, 3/4) = 4^s beta(s)` and `beta(2m + 1) =
//!   (-1)^m E_(2m) pi^(2m + 1)/(4^(m + 1) (2m)!)` (Abramowitz and Stegun 23.2.22 and 23.2.23), so that
//!   `sum (-1)^k/(2k + 1)^3 = pi^3/32`.
use crate::cx::Sx;
use mc_api::{names, Store, View, T};
use mc_eval::Res;

/// The factors of a product (the term itself when it is no product).
fn factors<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>) -> Vec<T<'h>> {
    match sx.args_of(t, names::MUL) {
        Some(a) => a.to_vec(),
        None => vec![t],
    }
}

/// `t(k + 1) - t(k)` as an exact integer, when it is one.
fn step<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, k: T<'h>) -> Res<Option<i64>> {
    let k1 = sx.plus(k, 1)?;
    let t1 = sx.subst(t, k, k1)?;
    let d = sx.sub(t1, t)?;
    let d = sx.ev("expand", &[d])?;
    Ok(sx.as_int(d))
}

/// The value at `k = 0` of the sign factor `u` when `u = c (-1)^k` with `c` free of `k`.
fn sign_factor<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>, k: T<'h>) -> Res<Option<T<'h>>> {
    if sx.free(u, k) {
        return Ok(None);
    }
    if let Some(&[b, e]) = sx.args_of(u, names::POW) {
        if b == sx.int(-1) && sx.free(b, k) {
            if let Some(a) = step(sx, e, k)? {
                if a % 2 != 0 {
                    let e0 = sx.subst(e, k, sx.int(0))?;
                    return Ok(Some(sx.pow(sx.int(-1), e0)?));
                }
            }
        }
        return Ok(None);
    }
    for name in ["cos", "sin"] {
        if let Some(&[th]) = sx.args_of(u, name) {
            let pi = sx.sym("pi");
            let k1 = sx.plus(k, 1)?;
            let th1 = sx.subst(th, k, k1)?;
            let d = sx.sub(th1, th)?;
            let d = sx.div(d, pi)?;
            let d = sx.ev("expand", &[d])?;
            if let Some(a) = sx.as_int(d) {
                if a % 2 != 0 {
                    let th0 = sx.subst(th, k, sx.int(0))?;
                    return Ok(Some(sx.ev(name, &[th0])?));
                }
            }
            return Ok(None);
        }
    }
    Ok(None)
}

/// `sum(f, [k, lo, inf])` for an alternating `f` (module comment); `None` when `f` is not of that form.
pub fn infinite<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>) -> Res<Option<T<'h>>> {
    let Some(lo_i) = sx.as_int(lo) else { return Ok(None) };
    let fs = factors(sx, f);
    let mut sign = None;
    let mut rest = vec![];
    for &u in &fs {
        if sign.is_none() {
            if let Some(s0) = sign_factor(sx, u, k)? {
                sign = Some(s0);
                continue;
            }
        }
        rest.push(u);
    }
    let Some(s0) = sign else { return Ok(None) };
    let g = sx.mul(&rest)?;
    if sx.free(g, k) || !crate::hyper::rational_in(sx, g, k)? {
        return Ok(None);
    }
    // a proper rational function: its terms tend to 0 (the pairs then are O(j^-2) and the pairing is exact)
    let to = sx.rule(k, sx.sym("inf"));
    let lim = sx.ev("limit", &[g, to])?;
    if lim != sx.int(0) {
        return Ok(None);
    }
    // no pole at an integer of the range
    let gt = sx.ev("together", &[g])?;
    let den = sx.ev("denominator", &[gt])?;
    let roots = sx.ev("solve", &[sx.call("equal", &[den, sx.int(0)]), k])?;
    if let Some(rs) = sx.as_list(roots) {
        for &r in rs {
            if let Some(&[rule]) = sx.as_list(r) {
                if let Some(&[_, v]) = sx.args_of(rule, names::RULE) {
                    if let Some(vi) = sx.as_int(v) {
                        if vi >= lo_i {
                            return Ok(None);
                        }
                    }
                }
            }
        }
    }
    if let Some(v) = centred_even(sx, g, gt, den, k, lo_i)? {
        let v = sx.mul(&[s0, v])?;
        return Ok(Some(sx.ev("simplify", &[v])?));
    }
    // with parameters only the residue form: the pairs' digamma values of algebraic points say little
    let vars = sx.ev("variables", &[g])?;
    if sx.as_list(vars).is_none_or(|v| v.len() != 1) {
        return Ok(None);
    }
    let j = sx.sym("sum$j");
    let two_j = sx.mul(&[sx.int(2), j])?;
    let a0 = sx.add(&[lo, two_j])?;
    let a1 = sx.plus(a0, 1)?;
    let g0 = sx.subst(g, k, a0)?;
    let g1 = sx.subst(g, k, a1)?;
    let pair = sx.sub(g0, g1)?;
    let pair = sx.ev("together", &[pair])?;
    let spec = sx.list(&[j, sx.int(0), sx.sym("inf")]);
    let s = sx.ev("sum", &[pair, spec])?;
    if sx.bad(s) || sx.contains_any(s, &["sum", "conditional_expression"]) || !sx.free(s, j) {
        return Ok(None);
    }
    let s = closed_values(sx, s)?;
    let sgn = sx.pow(sx.int(-1), lo)?;
    let v = sx.mul(&[s0, sgn, s])?;
    let v = sx.ev("simplify", &[v])?;
    Ok(Some(v))
}

/// `sum_(k >= lo) (-1)^k g(k)` when `h(m) = g(m + c)` is even for the integer `c` at the centre of the poles:
/// `(-1)^c sum_(m >= lo - c) (-1)^m h(m)`, the sum from 0 by [`even_csc`] and the terms between 0 and `lo - c` added
/// or taken away one by one.
fn centred_even<'h>(sx: &mut Sx<'_, '_, '_, 'h>, g: T<'h>, gt: T<'h>, den: T<'h>, k: T<'h>, lo: i64) -> Res<Option<T<'h>>> {
    let deg = sx.ev("exponent", &[den, k])?;
    let Some(d) = sx.as_int(deg) else { return Ok(None) };
    if d < 2 {
        return Ok(None);
    }
    let lead = sx.ev("coefficient", &[den, k, sx.int(d)])?;
    let next = sx.ev("coefficient", &[den, k, sx.int(d - 1)])?;
    let dl = sx.mul(&[sx.int(-d), lead])?;
    let c = sx.div(next, dl)?;
    let Some(c) = sx.as_int(c) else { return Ok(None) };
    if c == 0 {
        let roots = sx.ev("solve", &[sx.call("equal", &[den, sx.int(0)]), k])?;
        return even_csc(sx, g, gt, den, roots, k, lo);
    }
    if c.abs() > 200 {
        return Ok(None);
    }
    let kc = sx.plus(k, c)?;
    let h = sx.subst(gt, k, kc)?;
    let h = sx.ev("together", &[h])?;
    let hden = sx.ev("denominator", &[h])?;
    let roots = sx.ev("solve", &[sx.call("equal", &[hden, sx.int(0)]), k])?;
    let Some(s0) = even_csc(sx, h, h, hden, roots, k, 0)? else { return Ok(None) };
    let start = lo - c;
    let mut parts = vec![s0];
    for m in start.min(0)..start.max(0) {
        let hm = sx.subst(h, k, sx.int(m))?;
        let sgn = if m.rem_euclid(2) == 0 { 1 } else { -1 };
        let t = sx.mul(&[sx.int(if start < 0 { sgn } else { -sgn }), hm])?;
        parts.push(t);
    }
    let v = sx.add(&parts)?;
    let sc = sx.pow(sx.int(-1), sx.int(c))?;
    Ok(Some(sx.mul(&[sc, v])?))
}

/// `sum_(k >= lo) (-1)^k g(k)` for an even `g` with simple poles off the integers, `lo` 0 or 1: half the bilateral
/// sum `-pi sum_z Res(g, z) csc(pi z)` (the residue theorem with the kernel `pi csc(pi z)`, whose residue at the
/// integer `k` is `(-1)^k`; Mitrinović and Kečkić, The Cauchy Method of Residues, 1984, section 5.2), plus or minus
/// half of `g(0)`. `None` when the closed form keeps a `csc` or `digamma` it cannot reduce.
fn even_csc<'h>(
    sx: &mut Sx<'_, '_, '_, 'h>,
    g: T<'h>,
    gt: T<'h>,
    den: T<'h>,
    roots: T<'h>,
    k: T<'h>,
    lo: i64,
) -> Res<Option<T<'h>>> {
    if lo != 0 && lo != 1 {
        return Ok(None);
    }
    let nk = sx.mul(&[sx.int(-1), k])?;
    let gm = sx.subst(g, k, nk)?;
    let d = sx.sub(g, gm)?;
    if sx.ev("together", &[d])? != sx.int(0) {
        return Ok(None);
    }
    let Some(rs) = sx.as_list(roots) else { return Ok(None) };
    let deg = sx.ev("exponent", &[den, k])?;
    if sx.as_int(deg) != Some(rs.len() as i64) || rs.is_empty() {
        return Ok(None);
    }
    let num = sx.ev("numerator", &[gt])?;
    let dden = sx.ev("d", &[den, k])?;
    let pi = sx.sym("pi");
    let mut terms = vec![];
    let mut seen = vec![];
    for &r in rs {
        let Some(&[rule]) = sx.as_list(r) else { return Ok(None) };
        let Some(&[_, z]) = sx.args_of(rule, names::RULE) else { return Ok(None) };
        if seen.contains(&z) {
            return Ok(None); // a multiple pole
        }
        seen.push(z);
        let dz = sx.subst(dden, k, z)?;
        let dz = sx.ev("simplify", &[dz])?;
        if dz == sx.int(0) {
            return Ok(None);
        }
        let nz = sx.subst(num, k, z)?;
        let pz = sx.mul(&[pi, z])?;
        let pz = sx.ev("expand", &[pz])?;
        let c = sx.ev("csc", &[pz])?;
        let res = sx.div(nz, dz)?;
        terms.push(sx.mul(&[res, c])?);
    }
    let b = sx.add(&terms)?;
    let b = sx.mul(&[sx.int(-1), pi, b])?;
    let g0 = sx.subst(g, k, sx.int(0))?;
    let g0 = if lo == 0 { g0 } else { sx.mul(&[sx.int(-1), g0])? };
    let v = sx.add(&[b, g0])?;
    let v = sx.div(v, sx.int(2))?;
    let v = sx.ev("simplify", &[v])?;
    if sx.bad(v) || sx.contains_any(v, &["csc", "digamma", "sin", "cos"]) {
        return Ok(None);
    }
    Ok(Some(v))
}

/// `t` with `digamma(r)` at rational `r` of denominator 2, 3, 4 or 6 and `zeta(s, 3/4)` at odd integers `s >= 3`
/// replaced by their closed forms (module comment).
fn closed_values<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    match h.view(t) {
        View::App { head, args } => {
            let mut out = Vec::with_capacity(args.len());
            for &a in args {
                out.push(closed_values(sx, a)?);
            }
            let u = h.app(head, &out);
            if let Some(&[r]) = sx.args_of(u, "digamma") {
                if let Some(v) = gauss_digamma(sx, r)? {
                    return Ok(v);
                }
            }
            if let Some(&[s, a]) = sx.args_of(u, "zeta") {
                if let Some(v) = zeta_three_quarters(sx, s, a)? {
                    return Ok(v);
                }
            }
            sx.eval(u)
        }
        _ => Ok(t),
    }
}

/// The numerator and denominator of an exact rational number.
fn rational<'h>(sx: &mut Sx<'_, '_, '_, 'h>, r: T<'h>) -> Res<Option<(i64, i64)>> {
    if let Some(n) = sx.as_int(r) {
        return Ok(Some((n, 1)));
    }
    if !matches!(sx.h().view(r), View::Rational { .. }) {
        return Ok(None);
    }
    let n = sx.ev("numerator", &[r])?;
    let d = sx.ev("denominator", &[r])?;
    Ok(match (sx.as_int(n), sx.as_int(d)) {
        (Some(n), Some(d)) => Some((n, d)),
        _ => None,
    })
}

/// Gauss's digamma theorem at `p/q` (module comment), `q` in 2, 3, 4, 6, shifted into (0, 1) by
/// `psi(x + 1) = psi(x) + 1/x`.
fn gauss_digamma<'h>(sx: &mut Sx<'_, '_, '_, 'h>, r: T<'h>) -> Res<Option<T<'h>>> {
    let Some((n, q)) = rational(sx, r)? else { return Ok(None) };
    if ![2, 3, 4, 6].contains(&q) || n.abs() > 64 * q {
        return Ok(None);
    }
    let p = n.rem_euclid(q);
    let m = (n - p) / q; // r = m + p/q
    let base = sx.div(sx.int(p), sx.int(q))?;
    let pi = sx.sym("pi");
    let mut terms = vec![sx.mul(&[sx.int(-1), sx.sym("euler_gamma")])?];
    let l2q = sx.ev("log", &[sx.int(2 * q)])?;
    terms.push(sx.mul(&[sx.int(-1), l2q])?);
    let ang = sx.mul(&[pi, base])?;
    let cot = sx.ev("cot", &[ang])?;
    let mhalf = sx.div(sx.int(-1), sx.int(2))?;
    terms.push(sx.mul(&[mhalf, pi, cot])?);
    let inv_q = sx.div(sx.int(1), sx.int(q))?;
    for nn in 1..(q + 1) / 2 {
        let c_arg = sx.mul(&[sx.int(2 * nn * p), pi, inv_q])?;
        let c = sx.ev("cos", &[c_arg])?;
        let s_arg = sx.mul(&[sx.int(nn), pi, inv_q])?;
        let s = sx.ev("sin", &[s_arg])?;
        let l = sx.ev("log", &[s])?;
        terms.push(sx.mul(&[sx.int(2), c, l])?);
    }
    // psi(base + m) = psi(base) + sum_(i < m) 1/(base + i) for m > 0, minus sum_(m <= i < 0) 1/(base + i) for m < 0
    for i in m.min(0)..m.max(0) {
        let x = sx.add(&[base, sx.int(i)])?;
        let inv = sx.div(sx.int(1), x)?;
        terms.push(if m > 0 { inv } else { sx.mul(&[sx.int(-1), inv])? });
    }
    let v = sx.add(&terms)?;
    Ok(Some(sx.ev("expand", &[v])?))
}

/// `zeta(s, 3/4) = zeta(s, 1/4) - 4^s beta(s)` for odd integers `s >= 3` (module comment).
fn zeta_three_quarters<'h>(sx: &mut Sx<'_, '_, '_, 'h>, s: T<'h>, a: T<'h>) -> Res<Option<T<'h>>> {
    let Some(si) = sx.as_int(s) else { return Ok(None) };
    if si < 3 || si % 2 == 0 || si > 41 || rational(sx, a)? != Some((3, 4)) {
        return Ok(None);
    }
    let m = (si - 1) / 2;
    let e = sx.ev("euler_e", &[sx.int(2 * m)])?;
    let f = sx.ev("factorial", &[sx.int(2 * m)])?;
    let pw = sx.pow(sx.sym("pi"), s)?;
    let sg = sx.pow(sx.int(-1), sx.int(m))?;
    let num = sx.mul(&[sg, e, pw])?;
    let p4 = sx.pow(sx.int(4), sx.int(m + 1))?;
    let den = sx.mul(&[p4, f])?;
    let beta = sx.div(num, den)?;
    let quarter = sx.div(sx.int(1), sx.int(4))?;
    let z1 = sx.ev("zeta", &[s, quarter])?;
    let c = sx.pow(sx.int(4), s)?;
    let cb = sx.mul(&[c, beta])?;
    Ok(Some(sx.sub(z1, cb)?))
}
