//! Recurrences beyond hypergeometric solutions: three exact constructions for `r_solve`.
//!
//! - **k-step products** `y(n + k) = r(n) y(n)` (k >= 2, every middle coefficient 0) with
//!   `r(n) = C q^(alpha n) prod_j (n + a_j)^(e_j)` (rational `a_j`): `P(n) = C^(n/k) q^(alpha n (n - k)/(2k))
//!   prod_j (k^(n/k) Gamma((n + a_j)/k))^(e_j)`, divided by its value at 0, satisfies `P(n + k) = r(n) P(n)`
//!   (from `Gamma(z + 1) = z Gamma(z)`), and `omega^(j n) P(n)`, `omega = exp(2 pi i/k)`, `j = 0, ..., k - 1`, is
//!   a basis (their Casoratian is a Vandermonde determinant times `P` values). Hyper does not see these solutions:
//!   their ratio `y(n + 1)/y(n)` is not rational.
//! - **Classical three-term recurrences**: `p2(n) y(n + 2) + p1(n) y(n + 1) + p0(n) y(n) = 0` and a recurrence
//!   `q2 z(n + 2) + q1 z(n + 1) + q0 z(n) = 0` are related by `y = g z`, `g(n + 1) = rho(n) g(n)`, and a factor
//!   `lambda(n)` on the equation exactly when the invariant `J = p0(n + 1) p2(n)/(p1(n) p1(n + 1))` agrees (it is
//!   unchanged by both operations); then `lambda = q0/p0`, `rho = q1 p0/(q0 p1)`. The families compared are those
//!   whose two classical solutions are known: Chebyshev `T`, `U` (DLMF 18.9.1), Legendre `P`, `Q` (DLMF 14.10.3),
//!   Bessel `J`, `Y` (DLMF 10.6.1) and the modified `I`, `(-1)^n K` and `K`, `(-1)^n I` (DLMF 10.29.1). The
//!   argument `w` comes from `J` (it holds `w^2` free of `n`; a real `w` is preferred); the identity
//!   `lambda p2 rho(n) rho(n + 1) = q2` is checked exactly.
//! - **Quadratic maps** `y(n + 1) = alpha y(n)^2 + beta y(n) + gamma`: `u = alpha y + beta/2` gives
//!   `u(n + 1) = u(n)^2 + c`, `c = alpha gamma + beta/2 - beta^2/4`. For `c = 0`, `u = exp(C 2^n)`; for `c = -2`,
//!   `u = 2 cos(C 2^n)` (the doubling formula); no other `c` has an orbit in closed form of this kind (the
//!   Julia sets of `z^2 + c` are smooth curves only for `c = 0` and segments only for `c = -2`; A. F. Beardon,
//!   Iteration of Rational Functions, ch. 1). An initial value gives `u = u0^(2^(n - k))` or
//!   `2 cos(2^(n - k) acos(u0/2))`.
use crate::cx::Sx;
use mc_api::{names, Store, View, T};
use mc_eval::Res;

/// The factors of a product term (the term itself when it is not a product).
fn factors<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>) -> Vec<T<'h>> {
    match sx.args_of(t, names::MUL) {
        Some(xs) => xs.to_vec(),
        None => vec![t],
    }
}

/// `(base, exponent)` of a power (exponent 1 otherwise).
fn base_exp<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>) -> (T<'h>, T<'h>) {
    match sx.args_of(t, names::POW) {
        Some(&[b, e]) => (b, e),
        _ => (t, sx.int(1)),
    }
}

/// `t` as `c1 n + c0` with `c1`, `c0` free of `n` (`c1` nonzero).
fn linear<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, n: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let c1 = sx.ev("d", &[t, n])?;
    if !sx.free(c1, n) || c1 == sx.int(0) {
        return Ok(None);
    }
    let c1n = sx.mul(&[c1, n])?;
    let c0 = sx.sub(t, c1n)?;
    let c0 = sx.ev("expand", &[c0])?;
    if !sx.free(c0, n) {
        return Ok(None);
    }
    Ok(Some((c1, c0)))
}

/// `(numerator, denominator)` of a machine-size rational number.
fn rational_parts<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(i64, i64)> {
    match sx.h().view(t) {
        View::Int(v) => Some((v, 1)),
        View::Rational { num, den } => Some((sx.as_int(num)?, sx.as_int(den)?)),
        _ => None,
    }
}

/// A basis of `p0(n) y(n) + pk(n) y(n + k) = 0` (`ps = [p0, 0, ..., 0, pk]`, k >= 2) by the k-step product.
pub fn kstep_basis<'h>(sx: &mut Sx<'_, '_, '_, 'h>, ps: &[T<'h>], n: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let k = ps.len() as i64 - 1;
    if !(2..=6).contains(&k) || ps[1..k as usize].iter().any(|&c| c != sx.int(0)) {
        return Ok(None);
    }
    let r = sx.div(ps[0], ps[k as usize])?;
    let r = sx.mul(&[sx.int(-1), r])?;
    let r = sx.ev("together", &[r])?;
    let r = sx.ev("factor", &[r])?;
    if r == sx.int(0) || sx.bad(r) {
        return Ok(None);
    }
    let kk = sx.int(k);
    let inv_k = sx.div(sx.int(1), kk)?;
    // (base, exponent) pairs of P, and the gamma factors with their values at 0
    let mut powers: Vec<(T<'h>, T<'h>)> = vec![];
    let mut gammas: Vec<T<'h>> = vec![];
    let mut norms: Vec<T<'h>> = vec![];
    let mut constant: Vec<T<'h>> = vec![];
    for f in factors(sx, r) {
        sx.cx.tick(1)?;
        if sx.free(f, n) {
            constant.push(f);
            continue;
        }
        let (b, e) = base_exp(sx, f);
        if sx.free(b, n) {
            // b^(alpha n + beta)
            let Some((alpha, beta)) = linear(sx, e, n)? else { return Ok(None) };
            constant.push(sx.pow(b, beta)?);
            // q^(alpha n (n - k)/(2k))
            let nk = sx.plus(n, -k)?;
            let inv2k = sx.div(sx.int(1), sx.int(2 * k))?;
            let ex = sx.mul(&[alpha, n, nk, inv2k])?;
            powers.push((b, ex));
            continue;
        }
        let Some(e) = sx.as_int(e) else { return Ok(None) };
        let Some((c1, c0)) = linear(sx, b, n)? else { return Ok(None) };
        // (c1 n + c0)^e = c1^e (n + a)^e
        constant.push(sx.pow(c1, sx.int(e))?);
        let a = sx.div(c0, c1)?;
        if !matches!(sx.h().view(a), View::Int(_) | View::Rational { .. }) {
            return Ok(None);
        }
        // a/k a pole of gamma: no normalisation at 0
        let a_k = sx.div(a, kk)?;
        if let Some(v) = sx.as_int(a_k) {
            if v <= 0 {
                return Ok(None);
            }
        }
        let ne = sx.mul(&[sx.int(e), n, inv_k])?;
        powers.push((kk, ne));
        let na = sx.add(&[n, a])?;
        let arg = sx.mul(&[na, inv_k])?;
        let g = sx.pow(sx.call("gamma", &[arg]), sx.int(e))?;
        let g0 = sx.pow(sx.call("gamma", &[a_k]), sx.int(-e))?;
        gammas.push(g);
        norms.push(g0);
    }
    let c = sx.mul(&constant)?;
    if c == sx.int(0) || sx.bad(c) {
        return Ok(None);
    }
    if c != sx.int(1) {
        let ex = sx.mul(&[n, inv_k])?;
        powers.push((c, ex));
    }
    // equal bases combined, their exponents expanded
    let mut merged: Vec<(T<'h>, T<'h>)> = vec![];
    for (b, e) in powers {
        match merged.iter_mut().find(|m| m.0 == b) {
            Some(m) => m.1 = sx.add(&[m.1, e])?,
            None => merged.push((b, e)),
        }
    }
    // the normalisation 1/P(0): its rational factor's powers of an integer base of P go into that exponent
    // (4 2^(n/2) is written 2^(2 + n/2))
    let norm = sx.mul(&norms)?;
    let (coef, rest) = match sx.args_of(norm, names::MUL) {
        Some(xs) if !xs.is_empty() && rational_parts(sx, xs[0]).is_some() => {
            let rest = sx.mul(&xs[1..])?;
            (xs[0], rest)
        }
        _ if rational_parts(sx, norm).is_some() => (norm, sx.int(1)),
        _ => (sx.int(1), norm),
    };
    let (mut num, mut den) = rational_parts(sx, coef).unwrap_or((1, 1));
    let mut parts = gammas;
    parts.push(rest);
    for (b, e) in merged {
        let mut shift = 0i64;
        if let Some(bv) = sx.as_int(b).filter(|&v| v >= 2) {
            while num % bv == 0 && num != 0 {
                num /= bv;
                shift += 1;
            }
            while den % bv == 0 {
                den /= bv;
                shift -= 1;
            }
        }
        let e = sx.plus(e, shift)?;
        let e = sx.ev("expand", &[e])?;
        parts.push(sx.pow(b, e)?);
    }
    let c0 = sx.div(sx.int(num), sx.int(den))?;
    parts.push(c0);
    let p = sx.mul(&parts)?;
    if sx.bad(p) {
        return Ok(None);
    }
    let mut basis = vec![p];
    for j in 1..k {
        // omega^(j n) = (-1)^(2 j n/k)
        let ex = sx.mul(&[sx.int(2 * j), n, inv_k])?;
        let w = sx.pow(sx.int(-1), ex)?;
        basis.push(sx.mul(&[w, p])?);
    }
    Ok(Some(basis))
}

/// A classical family (its coefficients in [`family_coeffs`]): the power `s` of the argument `w` in the
/// invariant, and the two solutions `f1(n, w)`, `f2(n, w)` (the second times `(-1)^n` when `alt`).
struct Family {
    s: i64,
    f1: &'static str,
    f2: &'static str,
    alt: bool,
}

/// The coefficients of a family as terms in `n` and `w`.
fn family_coeffs<'h>(sx: &mut Sx<'_, '_, '_, 'h>, which: usize, n: T<'h>, w: T<'h>) -> Res<[T<'h>; 3]> {
    let one = sx.int(1);
    let n1 = sx.plus(n, 1)?;
    Ok(match which {
        // Chebyshev: z(n + 2) - 2 w z(n + 1) + z(n) = 0
        0 => [one, sx.mul(&[sx.int(-2), w])?, one],
        // Legendre: (n + 2) z(n + 2) - (2n + 3) w z(n + 1) + (n + 1) z(n) = 0
        1 => {
            let n2 = sx.plus(n, 2)?;
            let t = sx.mul(&[sx.int(2), n])?;
            let t = sx.plus(t, 3)?;
            [n2, sx.mul(&[sx.int(-1), t, w])?, n1]
        }
        // Bessel: z(n + 2) - 2 (n + 1)/w z(n + 1) + z(n) = 0
        2 => {
            let t = sx.div(n1, w)?;
            [one, sx.mul(&[sx.int(-2), t])?, one]
        }
        // modified Bessel I: z(n + 2) + 2 (n + 1)/w z(n + 1) - z(n) = 0
        3 => {
            let t = sx.div(n1, w)?;
            [one, sx.mul(&[sx.int(2), t])?, sx.int(-1)]
        }
        // modified Bessel K: z(n + 2) - 2 (n + 1)/w z(n + 1) - z(n) = 0
        _ => {
            let t = sx.div(n1, w)?;
            [one, sx.mul(&[sx.int(-2), t])?, sx.int(-1)]
        }
    })
}

const FAMILIES: [Family; 5] = [
    Family { s: -2, f1: "chebyshev_t", f2: "chebyshev_u", alt: false },
    Family { s: -2, f1: "legendre_p", f2: "legendre_q", alt: false },
    Family { s: 2, f1: "bessel_j", f2: "bessel_y", alt: false },
    Family { s: 2, f1: "bessel_i", f2: "bessel_k", alt: true },
    Family { s: 2, f1: "bessel_k", f2: "bessel_i", alt: true },
];

/// The invariant `p0(n + 1) p2(n) / (p1(n) p1(n + 1))`, brought together.
fn invariant<'h>(sx: &mut Sx<'_, '_, '_, 'h>, p: &[T<'h>; 3], n: T<'h>) -> Res<Option<T<'h>>> {
    let n1 = sx.plus(n, 1)?;
    let p0s = sx.subst(p[0], n, n1)?;
    let p1s = sx.subst(p[1], n, n1)?;
    let num = sx.mul(&[p0s, p[2]])?;
    let den = sx.mul(&[p[1], p1s])?;
    if den == sx.int(0) {
        return Ok(None);
    }
    let j = sx.div(num, den)?;
    let j = sx.ev("together", &[j])?;
    Ok(if sx.bad(j) { None } else { Some(j) })
}

/// A basis of `p0 y(n) + p1 y(n + 1) + p2 y(n + 2) = 0` by a classical family (gauge factor times its solutions).
pub fn classical_basis<'h>(sx: &mut Sx<'_, '_, '_, 'h>, ps: &[T<'h>], n: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let &[p0, p1, p2] = ps else { return Ok(None) };
    if p1 == sx.int(0) || p0 == sx.int(0) || p2 == sx.int(0) {
        return Ok(None);
    }
    let p = [p0, p1, p2];
    let Some(jp) = invariant(sx, &p, n)? else { return Ok(None) };
    let mut best: Option<(u8, Vec<T<'h>>)> = None;
    for (which, fam) in FAMILIES.iter().enumerate() {
        sx.cx.tick(1)?;
        // the family's invariant at w = 1: J = Jq1 w^s
        let [q2, q1, q0] = family_coeffs(sx, which, n, sx.int(1))?;
        let Some(jq1) = invariant(sx, &[q0, q1, q2], n)? else { continue };
        let ratio = sx.div(jp, jq1)?;
        let ratio = sx.ev("together", &[ratio])?;
        let ratio = sx.ev("factor", &[ratio])?;
        if !sx.free(ratio, n) || ratio == sx.int(0) || sx.bad(ratio) {
            continue;
        }
        let w2 = if fam.s == 2 { ratio } else { sx.div(sx.int(1), ratio)? };
        let half = sx.div(sx.int(1), sx.int(2))?;
        let root = sx.pow(w2, half)?;
        let w = sx.ev("power_expand", &[root])?;
        let w = sx.ev("simplify", &[w])?;
        if sx.bad(w) || w == sx.int(0) {
            continue;
        }
        // the gauge: lambda = q0/p0, rho = q1 p0/(q0 p1), and lambda p2 rho(n) rho(n + 1) = q2 exactly
        let [q2, q1, q0] = family_coeffs(sx, which, n, w)?;
        let lambda = sx.div(q0, p0)?;
        let rho = sx.mul(&[q1, p0])?;
        let qp = sx.mul(&[q0, p1])?;
        let rho = sx.div(rho, qp)?;
        let rho = sx.ev("together", &[rho])?;
        let n1 = sx.plus(n, 1)?;
        let rho1 = sx.subst(rho, n, n1)?;
        let lhs = sx.mul(&[lambda, p2, rho, rho1])?;
        let diff = sx.sub(lhs, q2)?;
        let diff = sx.ev("together", &[diff])?;
        let diff = sx.ev("simplify", &[diff])?;
        if diff != sx.int(0) {
            continue;
        }
        // g(n) = prod_{k=1}^{n-1} rho(k)
        let g = if rho == sx.int(1) {
            sx.int(1)
        } else {
            let kk = sx.sym("rclassic$k");
            let rk = sx.subst(rho, n, kk)?;
            let nm1 = sx.plus(n, -1)?;
            let spec = sx.list(&[kk, sx.int(1), nm1]);
            let g = sx.ev("product", &[rk, spec])?;
            if sx.contains_any(g, &["product"]) || sx.bad(g) {
                continue;
            }
            g
        };
        let f1 = sx.ev(fam.f1, &[n, w])?;
        let f2 = sx.ev(fam.f2, &[n, w])?;
        let f2 = if fam.alt {
            let sgn = sx.pow(sx.int(-1), n)?;
            sx.mul(&[sgn, f2])?
        } else {
            f2
        };
        let b1 = sx.mul(&[g, f1])?;
        let b2 = sx.mul(&[g, f2])?;
        // preferred: no gauge factor and a real argument (J_n(i x) is i^n I_n(x)), then a real argument
        let real = !sx.contains_any(w, &[names::COMPLEX]) && !sx.contains_any(w, &["im"]);
        let score = match (g == sx.int(1), real) {
            (true, true) => 0,
            (false, true) => 1,
            _ => 2,
        };
        if score == 0 {
            return Ok(Some(vec![b1, b2]));
        }
        if best.as_ref().is_none_or(|b: &(u8, Vec<T<'h>>)| score < b.0) {
            best = Some((score, vec![b1, b2]));
        }
    }
    Ok(best.map(|b| b.1))
}

/// `u^p` for a symbolic `p`; a positive rational `u` as the product of its prime powers `q^(e p)` (so that the
/// powers of equal primes combine), otherwise `u^p`.
fn prime_power<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>, p: T<'h>) -> Res<T<'h>> {
    let Some((num, den)) = rational_parts(sx, u).filter(|&(a, b)| a > 0 && b > 0) else { return sx.pow(u, p) };
    let mut parts = vec![];
    for (v, sign) in [(num, 1i64), (den, -1)] {
        if v == 1 {
            continue;
        }
        let f = sx.ev("factor_integer", &[sx.int(v)])?;
        let Some(pairs) = sx.as_list(f) else { return sx.pow(u, p) };
        for &pe in pairs {
            let Some(&[q, e]) = sx.as_list(pe) else { return sx.pow(u, p) };
            // e 2^m with e a power 2^j is written 2^(m + j)
            let mut ex = None;
            if let (Some(&[b, m]), Some(ev)) = (sx.args_of(p, names::POW), sx.as_int(e)) {
                if let Some(bv) = sx.as_int(b).filter(|&v| v >= 2) {
                    let (mut v, mut j) = (ev, 0i64);
                    while v % bv == 0 {
                        v /= bv;
                        j += 1;
                    }
                    if v == 1 {
                        let mj = sx.plus(m, j)?;
                        let pw = sx.pow(b, mj)?;
                        ex = Some(sx.mul(&[sx.int(sign), pw])?);
                    }
                }
            }
            let ex = match ex {
                Some(x) => x,
                None => sx.mul(&[sx.int(sign), e, p])?,
            };
            parts.push(sx.pow(q, ex)?);
        }
    }
    sx.mul(&parts)
}

/// The orbit of `y(n + 1) = alpha y(n)^2 + beta y(n) + gamma` in closed form: with `init = (k, v)` the solution
/// through `y(k) = v`, else the general solution in the constant `cst`. `None` unless `c = alpha gamma + beta/2 -
/// beta^2/4` is 0 or -2.
pub fn quadratic_orbit<'h>(
    sx: &mut Sx<'_, '_, '_, 'h>,
    coef: [T<'h>; 3],
    n: T<'h>,
    init: Option<(T<'h>, T<'h>)>,
    cst: T<'h>,
) -> Res<Option<T<'h>>> {
    let [alpha, beta, gamma] = coef;
    if alpha == sx.int(0) {
        return Ok(None);
    }
    let half = sx.div(sx.int(1), sx.int(2))?;
    let quarter = sx.div(sx.int(1), sx.int(4))?;
    let ag = sx.mul(&[alpha, gamma])?;
    let b2 = sx.mul(&[beta, half])?;
    let bb = sx.pow(beta, sx.int(2))?;
    let bb = sx.mul(&[bb, quarter])?;
    let c = sx.add(&[ag, b2])?;
    let c = sx.sub(c, bb)?;
    let c = sx.ev("simplify", &[c])?;
    let doubling = if c == sx.int(0) {
        false
    } else if c == sx.int(-2) {
        true
    } else {
        return Ok(None);
    };
    let two = sx.int(2);
    let u = match init {
        None => {
            let p = sx.pow(two, n)?;
            let arg = sx.mul(&[cst, p])?;
            if doubling {
                sx.mul(&[two, sx.call("cos", &[arg])])?
            } else {
                sx.call("exp", &[arg])
            }
        }
        Some((k, v)) => {
            let u0 = sx.mul(&[alpha, v])?;
            let u0 = sx.add(&[u0, b2])?;
            let u0 = sx.ev("simplify", &[u0])?;
            let nk = sx.sub(n, k)?;
            let p = sx.pow(two, nk)?;
            if doubling {
                let hu = sx.mul(&[u0, half])?;
                let a = sx.ev("acos", &[hu])?;
                let arg = sx.mul(&[a, p])?;
                sx.mul(&[two, sx.call("cos", &[arg])])?
            } else {
                if u0 == sx.int(0) {
                    return Ok(None);
                }
                prime_power(sx, u0, p)?
            }
        }
    };
    let u = sx.eval(u)?;
    let y = sx.sub(u, b2)?;
    let y = sx.div(y, alpha)?;
    let y = sx.ev("expand", &[y])?;
    Ok(if sx.bad(y) { None } else { Some(y) })
}
