//! The logarithmic part `∫ a/d` (d square-free, deg a < deg d): Rothstein–Trager with Trager's gcds over the
//! residue fields, and real output (Bronstein, *Symbolic Integration I*, sections 2.4 and 2.8).
//!
//! The residues are the roots of `R(t) = res_x(d, a - t d')`, computed by evaluation at t = 0..deg d and
//! interpolation, and factored over the coefficient field. For an irreducible factor `r` of R and a root c of r,
//! `v_c = gcd(d, a - c d')` and the integral is `Σ_c c log(v_c)`:
//!
//! - deg r = 1: c lies in the field, one term `c log(v_c)`;
//! - deg r = 2 with real roots `c = (-r1 ± √δ)/2`: two terms with `√δ` in them;
//! - deg r = 2 with complex roots `c = α ± iβ`: with `v_c = P + iQ` (P, Q real), the pair is
//!   `α log(P² + Q²) + β · LogToAtan(P, Q)`, where Rioboo's `LogToAtan` writes `i log((P + iQ)/(P - iQ))` as a sum
//!   of `2 atan(polynomial)` terms (no spurious discontinuities from `atan` of a quotient). When the sign of δ is
//!   not known (parameters), δ is taken negative when it looks negative, and the formula is used formally: its
//!   derivative is the integrand either way;
//! - deg r = 4 even (`t^4 + p t^2 + q`, as for `1/(x^4 + 1)`): r splits into two quadratics over a real quadratic
//!   field `K(√w)`, and each is done as above over that field;
//! - any other degree: a `root_sum` over the residues (module `rootsum`) when the coefficients are rational, else
//!   given up (the integral stays unevaluated).
#![allow(clippy::type_complexity)]
use super::upoly::{
    add, deg, derivative, divrem, exact, ext_gcd, gcd, interpolate, lift, mul, neg, q, qq, resultant_monic, scale,
    split, sub, Fld, Quad, Stop, Ticker, P, R,
};
use super::{log_arg, scaled_term};
use mc_calculus::term::Build;
use mc_api::T;

/// A coefficient field whose polynomials can be factored.
pub trait Base: Fld {
    /// The distinct monic irreducible factors of positive degree of a nonzero polynomial.
    fn factor(&self, p: &[Self::E], tk: &mut dyn Ticker) -> R<Vec<P<Self::E>>>;
    /// Does `a` (of unknown sign) look negative? Chooses between the real and complex forms of a pair of residues.
    fn looks_negative(&self, a: &Self::E) -> bool {
        self.sign(a) == Some(-1)
    }
}

/// The terms of elements of a field.
type Et<'a, 'h, E> = &'a dyn Fn(&E) -> T<'h>;

/// The resultant `∏_{d(α)=0} (a(α) - t d'(α))` as a polynomial in t (d monic).
fn rothstein_trager<K: Fld>(k: &K, a: &[K::E], d: &[K::E], dp: &[K::E], tk: &mut dyn Ticker) -> R<P<K::E>> {
    let n = deg(d).ok_or(Stop::GiveUp)?;
    let mut ys = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let g = sub(k, a, &scale(k, &k.of(&q(i as i64)), dp));
        ys.push(resultant_monic(k, d, &g, tk)?);
    }
    interpolate(k, &ys, tk)
}

/// Rioboo's `LogToAtan(A, B)` (B nonzero): polynomials `u_k` with `Σ 2 atan(u_k)` having the derivative of
/// `i log((A + iB)/(A - iB))`.
fn log_to_atan<K: Fld>(k: &K, a: &[K::E], b: &[K::E], tk: &mut dyn Ticker) -> R<Vec<P<K::E>>> {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    let mut out = vec![];
    let bound = 2 * (a.len() + b.len()) + 4;
    for _ in 0..bound {
        if b.is_empty() {
            return Err(Stop::GiveUp);
        }
        let (qq, r) = divrem(k, &a, &b, tk)?;
        if r.is_empty() {
            out.push(qq);
            return Ok(out);
        }
        if a.len() < b.len() {
            let na = neg(k, &b);
            b = a;
            a = na;
            continue;
        }
        // B D - A C = G
        let (dd, cc, g) = ext_gcd(k, &b, &neg(k, &a), tk)?;
        let s = add(k, &mul(k, &a, &dd, tk)?, &mul(k, &b, &cc, tk)?);
        out.push(exact(k, &s, &g, tk)?);
        a = dd;
        b = cc;
    }
    Err(Stop::GiveUp)
}

/// The terms of `∫ a/d` (d monic square-free, deg a < deg d) appended to `out`.
#[allow(clippy::too_many_arguments)]
pub fn integrate<'h, K: Base>(
    b: &Build<'h>,
    x: T<'h>,
    k: &K,
    et: Et<'_, 'h, K::E>,
    a: &[K::E],
    d: &[K::E],
    tk: &mut dyn Ticker,
    out: &mut Vec<T<'h>>,
) -> R<()> {
    if a.is_empty() {
        return Ok(());
    }
    let dp = derivative(k, d);
    let res = rothstein_trager(k, a, d, &dp, tk)?;
    if res.is_empty() {
        return Err(Stop::GiveUp);
    }
    let factors = k.factor(&res, tk)?;
    // residues of degree 3 or more: a root_sum over Q's number fields (all coefficients rational)
    let rational = |p: &[K::E]| -> Option<Vec<mc_arith::num::Rat>> { p.iter().map(|c| k.as_rat(c)).collect() };
    let over_q = match (rational(a), rational(d), rational(&dp)) {
        (Some(aq), Some(dq), Some(dpq)) => Some((aq, dq, dpq)),
        _ => None,
    };
    // decline before any term is built when a residue field is out of reach
    let reachable = |r: &[K::E]| matches!(deg(r), Some(1 | 2)) || even_quartic(k, r) || (over_q.is_some() && rational(r).is_some());
    if factors.iter().any(|r| !reachable(r)) {
        return Err(Stop::GiveUp);
    }
    let mut covered = 0;
    for r in &factors {
        covered += match deg(r) {
            Some(1) => root(b, x, k, et, a, d, &dp, &k.neg(&r[0]), tk, out)?,
            Some(2) => quadratic(b, x, k, et, a, d, &dp, r, tk, out)?,
            _ => {
                // an even quartic splits over a real quadratic field when it can; otherwise a root_sum
                let mut part = vec![];
                let split = if even_quartic(k, r) { Some(quartic(b, x, k, et, a, d, &dp, r, tk, &mut part)) } else { None };
                match (split, &over_q, rational(r)) {
                    (Some(Ok(n)), _, _) => {
                        out.extend(part);
                        n
                    }
                    (Some(Err(Stop::Abort(e))), _, _) => return Err(Stop::Abort(e)),
                    (_, Some((aq, dq, dpq)), Some(rq)) => super::rootsum::residues(b, x, &rq, aq, dq, dpq, tk, out)?,
                    (Some(Err(e)), _, _) => return Err(e),
                    _ => return Err(Stop::GiveUp),
                }
            }
        };
    }
    // every root of d belongs to exactly one residue
    if Some(covered) != deg(d) {
        return Err(Stop::GiveUp);
    }
    Ok(())
}

/// `c log(gcd(d, a - c d'))` for a residue c in the field; the degree of the gcd.
#[allow(clippy::too_many_arguments)]
fn root<'h, K: Fld>(
    b: &Build<'h>,
    x: T<'h>,
    k: &K,
    et: Et<'_, 'h, K::E>,
    a: &[K::E],
    d: &[K::E],
    dp: &[K::E],
    c: &K::E,
    tk: &mut dyn Ticker,
    out: &mut Vec<T<'h>>,
) -> R<usize> {
    let v = gcd(k, d, &sub(k, a, &scale(k, c, dp)), tk)?;
    let n = deg(&v).ok_or(Stop::GiveUp)?;
    if n == 0 {
        return Err(Stop::GiveUp);
    }
    out.push(b.mul(&[et(c), b.call("log", &[log_arg(b, k, &v, x, et)])]));
    Ok(n)
}

/// `r` (monic, degree 2) splits over the field: its two roots.
fn roots2<K: Fld>(k: &K, r: &[K::E]) -> Option<[K::E; 2]> {
    let delta = k.sub(&k.mul(&r[1], &r[1]), &k.mul(&k.of(&q(4)), &r[0]));
    let s = k.sqrt(&delta)?;
    let half = k.of(&qq(1, 2));
    Some([k.mul(&half, &k.sub(&s, &r[1])), k.mul(&half, &k.sub(&k.neg(&s), &r[1]))])
}

/// The field `K(√w)` (w not a square in K) with the simpler generator `√m`, `w = s^2 m`: the field and
/// `√w = s √m` in it. It is real (√m the positive root) when w is known to be positive.
fn root_field<'a, K: Fld>(k: &'a K, w: &K::E) -> (Quad<'a, K>, [K::E; 2]) {
    let (s, m) = k.square_split(w);
    let real = k.sign(w) == Some(1);
    (Quad { k, s: k.zero(), n: m, real }, [k.zero(), s])
}

/// The term `√w` written as `s √m` (`w = s^2 m`).
fn sqrt_term<'h, K: Fld>(b: &Build<'h>, k: &K, et: Et<'_, 'h, K::E>, w: &K::E) -> T<'h> {
    let (s, m) = k.square_split(w);
    b.mul(&[et(&s), b.pow(et(&m), et(&k.of(&qq(1, 2))))])
}

/// The terms of the two roots of an irreducible monic quadratic `r`; twice the degree of the gcd.
#[allow(clippy::too_many_arguments)]
fn quadratic<'h, K: Base>(
    b: &Build<'h>,
    x: T<'h>,
    k: &K,
    et: Et<'_, 'h, K::E>,
    a: &[K::E],
    d: &[K::E],
    dp: &[K::E],
    r: &[K::E],
    tk: &mut dyn Ticker,
    out: &mut Vec<T<'h>>,
) -> R<usize> {
    if let Some(cs) = roots2(k, r) {
        let mut n = 0;
        for c in &cs {
            n += root(b, x, k, et, a, d, dp, c, tk, out)?;
        }
        return Ok(n);
    }
    // r = θ^2 + r1 θ + r0, so θ^2 = -r1 θ - r0
    let (r0, r1) = (&r[0], &r[1]);
    let k1 = Quad { k, s: k.neg(r1), n: k.neg(r0), real: false };
    let theta = [k.zero(), k.one()];
    let g = gcd(&k1, &lift(&k1, d), &sub(&k1, &lift(&k1, a), &scale(&k1, &theta, &lift(&k1, dp))), tk)?;
    let n = deg(&g).ok_or(Stop::GiveUp)?;
    if n == 0 {
        return Err(Stop::GiveUp);
    }
    // g = V0 + θ V1
    let (v0, v1) = split(&k1, &g);
    let delta = k.sub(&k.mul(r1, r1), &k.mul(&k.of(&q(4)), r0));
    let half = k.of(&qq(1, 2));
    let half_t = et(&half);
    let complex = match k.sign(&delta) {
        Some(s) => s < 0,
        None => k.looks_negative(&delta),
    };
    if !complex {
        // real roots c = (-r1 ± √δ)/2: c log(V0 + c V1) for each
        let sq = sqrt_term(b, k, et, &delta);
        for sign in [1, -1] {
            let c = b.mul(&[half_t, b.add(&[et(&k.neg(r1)), b.mul(&[b.int(sign), sq])])]);
            let terms: Vec<T<'h>> = (0..g.len())
                .map(|i| {
                    let z = k.zero();
                    let (c0, c1) = (v0.get(i).unwrap_or(&z), v1.get(i).unwrap_or(&z));
                    b.mul(&[b.add(&[et(c0), b.mul(&[c, et(c1)])]), b.powi(x, i as i64)])
                })
                .collect();
            out.push(b.mul(&[c, b.call("log", &[b.add(&terms)])]));
        }
        return Ok(2 * n);
    }
    // complex roots c = α ± iβ, α = -r1/2, β^2 = m = -δ/4; v_c = P + iQ with P = V0 + α V1, Q = β V1
    let alpha = k.mul(&half, &k.neg(r1));
    let m = k.mul(&k.of(&qq(-1, 4)), &delta);
    let p = add(k, &v0, &scale(k, &alpha, &v1));
    if !k.is_zero(&alpha) {
        // P^2 + Q^2 = P^2 + m V1^2
        let s = add(k, &mul(k, &p, &p, tk)?, &scale(k, &m, &mul(k, &v1, &v1, tk)?));
        out.push(b.mul(&[et(&alpha), b.call("log", &[log_arg(b, k, &s, x, et)])]));
    }
    let two = b.int(2);
    match k.sqrt(&m) {
        Some(beta) => {
            let us = log_to_atan(k, &p, &scale(k, &beta, &v1), tk)?;
            let coef = b.mul(&[two, et(&beta)]);
            for u in us {
                out.push(b.mul(&[coef, b.call("atan", &[scaled_term(b, k, &u, x, et)])]));
            }
        }
        None => {
            // over K(β), β^2 = m
            let (k2, beta) = root_field(k, &m);
            let gen = b.pow(et(&k2.n), half_t);
            let et2 = |e: &[K::E; 2]| b.add(&[et(&e[0]), b.mul(&[et(&e[1]), gen])]);
            let qb = scale(&k2, &beta, &lift(&k2, &v1));
            let us = log_to_atan(&k2, &lift(&k2, &p), &qb, tk)?;
            let coef = b.mul(&[two, et2(&beta)]);
            for u in us {
                out.push(b.mul(&[coef, b.call("atan", &[scaled_term(b, &k2, &u, x, &et2)])]));
            }
        }
    }
    Ok(2 * n)
}

/// Is `r` a monic even quartic `t^4 + p t^2 + q`?
fn even_quartic<K: Fld>(k: &K, r: &[K::E]) -> bool {
    r.len() == 5 && k.is_zero(&r[1]) && k.is_zero(&r[3])
}

/// An irreducible even quartic `t^4 + p t^2 + q` split into two quadratics over a real field `K(√w)`, each done by
/// [`quadratic`]; gives up when no such split is found.
#[allow(clippy::too_many_arguments)]
fn quartic<'h, K: Base>(
    b: &Build<'h>,
    x: T<'h>,
    k: &K,
    et: Et<'_, 'h, K::E>,
    a: &[K::E],
    d: &[K::E],
    dp: &[K::E],
    r: &[K::E],
    tk: &mut dyn Ticker,
    out: &mut Vec<T<'h>>,
) -> R<usize> {
    if !even_quartic(k, r) {
        return Err(Stop::GiveUp);
    }
    let (p, q0) = (&r[2], &r[0]);
    // w > 0 (or of unknown sign and not looking negative: the split is then formal) and not a square
    let ok = |w: &K::E| {
        let positive = match k.sign(w) {
            Some(s) => s > 0,
            None => !k.looks_negative(w),
        };
        positive && k.sqrt(w).is_none()
    };
    // (t^2 + θ t + β)(t^2 - θ t + β) with β^2 = q, θ^2 = 2β - p
    let mut split: Option<(K::E, Vec<[K::E; 2]>)> = None;
    if let Some(sq) = k.sqrt(q0) {
        for beta in [sq.clone(), k.neg(&sq)] {
            let w = k.sub(&k.mul(&k.of(&q(2)), &beta), p);
            if split.is_none() && ok(&w) {
                split = Some((w, vec![[beta, k.zero()]]));
            }
        }
    }
    // (t^2 - s1)(t^2 - s2) with s = (-p ± √Δ)/2, Δ = p^2 - 4q
    let disc = k.sub(&k.mul(p, p), &k.mul(&k.of(&q(4)), q0));
    if split.is_none() && ok(&disc) {
        split = Some((disc, vec![]));
    }
    let Some((w, data)) = split else { return Err(Stop::GiveUp) };
    let (l, th) = root_field(k, &w);
    let gen = b.pow(et(&l.n), et(&k.of(&qq(1, 2))));
    let etl = |e: &[K::E; 2]| b.add(&[et(&e[0]), b.mul(&[et(&e[1]), gen])]);
    let factors: Vec<P<[K::E; 2]>> = match data.first() {
        Some(beta) => {
            let one = l.one();
            vec![vec![beta.clone(), th.clone(), one.clone()], vec![beta.clone(), l.neg(&th), one]]
        }
        None => {
            let half = l.of(&qq(1, 2));
            let p_l = [p.clone(), k.zero()];
            let s1 = l.mul(&half, &l.sub(&th, &p_l));
            let s2 = l.mul(&half, &l.sub(&l.neg(&th), &p_l));
            vec![vec![l.neg(&s1), l.zero(), l.one()], vec![l.neg(&s2), l.zero(), l.one()]]
        }
    };
    let (al, dl, dpl) = (lift(&l, a), lift(&l, d), lift(&l, dp));
    let mut n = 0;
    for f in &factors {
        n += quadratic(b, x, &l, &etl, &al, &dl, &dpl, f, tk, out)?;
    }
    Ok(n)
}

/// A quadratic extension as a [`Base`]: only its quadratics are used, its polynomials are not factored here.
impl<K: Base> Base for Quad<'_, K> {
    fn factor(&self, _p: &[Self::E], _tk: &mut dyn Ticker) -> R<Vec<P<Self::E>>> {
        Err(Stop::GiveUp)
    }
    fn looks_negative(&self, a: &Self::E) -> bool {
        match self.sign(a) {
            Some(s) => s < 0,
            None => self.k.is_zero(&a[1]) && self.k.looks_negative(&a[0]),
        }
    }
}
