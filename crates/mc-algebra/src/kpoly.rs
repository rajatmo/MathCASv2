//! Polynomials over a number field `K = Q(θ)`: a vector of field elements, index = degree, without trailing zeros.
//!
//! - **gcd** ([`gcd`]): a modular algorithm in the manner of Encarnación ("Computing GCDs of polynomials over
//!   algebraic number fields", J. Symbolic Computation 20, 1995) and Langemyr–McCallum (1989): the monic gcd is
//!   computed by Euclid's algorithm in `(F_p[t]/(f_p))[x]` for word-sized primes `p` (a prime at which a leading
//!   coefficient is not a unit is skipped; one giving a higher degree than another is unlucky and dropped), the
//!   images are combined by Chinese remaindering, the coefficients recovered by rational reconstruction (Wang), and
//!   the candidate is accepted only when it divides both inputs exactly over K. Euclid's algorithm over K is the
//!   fallback when the primes run out; the tests also check the modular result against it.
//! - **norm** ([`norm`]): `N(P)(x) = prod_i P_i(x)` over the conjugates, `Res_t(f(t), P(x, t))`, by evaluation at
//!   integer points and interpolation (each value a modular integer resultant).
//! - **factorisation** ([`factor`]): Trager's algorithm ("Algebraic factoring and rational function integration",
//!   SYMSAC 1976; Cohen, Algorithm 3.6.4): square-free decomposition over K, a shift `x -> x - s θ` that makes the
//!   norm square-free, the norm factored over Q by `mc-poly`, each factor's gcd with the shifted polynomial over K,
//!   shifted back. The product of the factors is expanded and compared with the input before it is returned.
use crate::field::{interpolate_0n, Elem, NumberField};
use crate::qpoly::{self, q, QPoly};
use mc_arith::num::{Int, Nat, Rat};
use mc_poly::modp::{self, Fp, Primes, UPoly};
use mc_poly::work::{Meter, WorkResult};

/// A polynomial over a number field.
pub type KPoly = Vec<Elem>;

/// Remove trailing zeros.
pub fn trim(mut p: KPoly) -> KPoly {
    while p.last().is_some_and(|c| c.is_empty()) {
        p.pop();
    }
    p
}

/// The degree (`None` for zero).
pub fn deg(p: &[Elem]) -> Option<usize> {
    p.len().checked_sub(1)
}

/// A rational polynomial over K.
pub fn lift(p: &[Rat]) -> KPoly {
    trim(p.iter().map(|c| qpoly::trim(vec![c.clone()])).collect())
}

/// The coefficients as rationals, when they all are.
pub fn as_rational(k: &NumberField, p: &[Elem]) -> Option<QPoly> {
    p.iter().map(|c| k.as_rat(c)).collect()
}

/// `a + b`.
pub fn add(k: &NumberField, a: &[Elem], b: &[Elem]) -> KPoly {
    let n = a.len().max(b.len());
    let z = vec![];
    trim((0..n).map(|i| k.add(a.get(i).unwrap_or(&z), b.get(i).unwrap_or(&z))).collect())
}

/// `a - b`.
pub fn sub(k: &NumberField, a: &[Elem], b: &[Elem]) -> KPoly {
    let n = a.len().max(b.len());
    let z = vec![];
    trim((0..n).map(|i| k.sub(a.get(i).unwrap_or(&z), b.get(i).unwrap_or(&z))).collect())
}

/// `c a`.
pub fn scale(k: &NumberField, a: &[Elem], c: &Elem) -> KPoly {
    trim(a.iter().map(|x| k.mul(x, c)).collect())
}

/// `a b`.
pub fn mul(k: &NumberField, a: &[Elem], b: &[Elem], m: &mut Meter) -> WorkResult<KPoly> {
    if a.is_empty() || b.is_empty() {
        return Ok(vec![]);
    }
    let d = k.degree() as u64;
    let mut out = vec![vec![]; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if x.is_empty() {
            continue;
        }
        m.step(b.len() as u64 * d * d)?;
        for (j, y) in b.iter().enumerate() {
            if !y.is_empty() {
                out[i + j] = k.add(&out[i + j], &k.mul(x, y));
            }
        }
    }
    Ok(trim(out))
}

/// `a` made monic (zero stays).
pub fn monic(k: &NumberField, a: &[Elem]) -> KPoly {
    match a.last() {
        // invariant: a trimmed polynomial has a nonzero leading coefficient, a unit in the field
        Some(l) => scale(k, a, &k.inv(l).expect("a nonzero leading coefficient")),
        None => vec![],
    }
}

/// Quotient and remainder of `a` by a nonzero `b`.
pub fn divrem(k: &NumberField, a: &[Elem], b: &[Elem], m: &mut Meter) -> WorkResult<(KPoly, KPoly)> {
    let mut r = trim(a.to_vec());
    // invariant: callers divide by nonzero polynomials
    let lead = k.inv(b.last().expect("a nonzero divisor")).expect("a nonzero leading coefficient");
    if r.len() < b.len() {
        return Ok((vec![], r));
    }
    let d = k.degree() as u64;
    let mut quo = vec![vec![]; r.len() + 1 - b.len()];
    while r.len() >= b.len() {
        m.step(b.len() as u64 * d * d)?;
        let s = r.len() - b.len();
        // invariant: the loop runs while the remainder is at least as long as the divisor
        let c = k.mul(r.last().expect("nonempty"), &lead);
        for (i, bi) in b.iter().enumerate() {
            if !bi.is_empty() {
                r[s + i] = k.sub(&r[s + i], &k.mul(&c, bi));
            }
        }
        quo[s] = c;
        r.pop();
        r = trim(r);
    }
    Ok((trim(quo), r))
}

/// The derivative.
pub fn derivative(k: &NumberField, a: &[Elem]) -> KPoly {
    trim(a.iter().enumerate().skip(1).map(|(i, c)| k.scale(c, &q(i as i64))).collect())
}

/// `a(x + c)`.
pub fn shift(k: &NumberField, a: &[Elem], c: &Elem, m: &mut Meter) -> WorkResult<KPoly> {
    let mut acc: KPoly = vec![];
    let lin = trim(vec![c.clone(), k.one()]);
    for coef in a.iter().rev() {
        acc = add(k, &mul(k, &acc, &lin, m)?, std::slice::from_ref(coef));
    }
    Ok(acc)
}

/// `a(x)` at a field element.
pub fn eval(k: &NumberField, a: &[Elem], x: &Elem) -> Elem {
    let mut acc = k.zero();
    for c in a.iter().rev() {
        acc = k.add(&k.mul(&acc, x), c);
    }
    acc
}

/// The monic gcd by Euclid's algorithm over K.
pub fn gcd_euclid(k: &NumberField, a: &[Elem], b: &[Elem], m: &mut Meter) -> WorkResult<KPoly> {
    let (mut a, mut b) = (trim(a.to_vec()), trim(b.to_vec()));
    while !b.is_empty() {
        let r = divrem(k, &a, &monic(k, &b), m)?.1;
        a = b;
        b = r;
    }
    Ok(monic(k, &a))
}

/// The monic gcd: modular, with Euclid's algorithm as the fallback.
pub fn gcd(k: &NumberField, a: &[Elem], b: &[Elem], m: &mut Meter) -> WorkResult<KPoly> {
    let (a, b) = (trim(a.to_vec()), trim(b.to_vec()));
    if a.is_empty() {
        return Ok(monic(k, &b));
    }
    if b.is_empty() {
        return Ok(monic(k, &a));
    }
    if deg(&a) == Some(0) || deg(&b) == Some(0) {
        return Ok(vec![k.one()]);
    }
    // rational inputs: the integer gcd
    if let (Some(aq), Some(bq)) = (as_rational(k, &a), as_rational(k, &b)) {
        return Ok(lift(&qpoly::gcd(&aq, &bq, m)?));
    }
    if k.degree() == 1 {
        return gcd_euclid(k, &a, &b, m);
    }
    match gcd_modular(k, &a, &b, m)? {
        Some(g) => Ok(g),
        None => gcd_euclid(k, &a, &b, m),
    }
}

// ------------------------------------------------------------------------------------------- modular gcd

/// `c mod p` for a rational whose denominator is a unit mod p.
fn rat_mod(c: &Rat, f: Fp) -> Option<u64> {
    let d = nat_mod(&c.den, f.p);
    if d == 0 {
        return None;
    }
    let n = nat_mod(&c.num.mag, f.p);
    let n = if c.num.neg { f.neg(n) } else { n };
    Some(f.mul(n, f.inv(d)))
}

fn nat_mod(n: &Nat, p: u64) -> u64 {
    n.divrem_u64(p).1
}

/// An element's image in `F_p[t]/(f_p)`.
fn elem_mod(c: &[Rat], f: Fp) -> Option<UPoly> {
    let mut v: UPoly = c.iter().map(|x| rat_mod(x, f)).collect::<Option<_>>()?;
    modp::trim(&mut v);
    Some(v)
}

/// The inverse of `a` in `F_p[t]/(fp)`, `None` for a zero divisor.
fn inv_mod(f: Fp, a: &[u64], fp: &[u64]) -> Option<UPoly> {
    let (g, s, _) = modp::xgcd(f, a, fp);
    (g.len() == 1).then(|| modp::rem(f, &s, fp))
}

/// The monic gcd in `(F_p[t]/(fp))[x]`; `None` when a leading coefficient is a zero divisor.
fn gcd_mod(f: Fp, fp: &[u64], a: Vec<UPoly>, b: Vec<UPoly>, m: &mut Meter) -> WorkResult<Option<Vec<UPoly>>> {
    let trimk = |mut p: Vec<UPoly>| {
        while p.last().is_some_and(|c| c.is_empty()) {
            p.pop();
        }
        p
    };
    let mulk = |x: &[u64], y: &[u64]| modp::rem(f, &modp::mul(f, x, y), fp);
    let (mut a, mut b) = (trimk(a), trimk(b));
    let d = fp.len() as u64;
    while !b.is_empty() {
        // a mod b, with b made monic
        let Some(li) = inv_mod(f, b.last().expect("nonempty"), fp) else { return Ok(None) };
        let bm: Vec<UPoly> = b.iter().map(|c| mulk(c, &li)).collect();
        let mut r = a;
        while r.len() >= bm.len() {
            m.step(bm.len() as u64 * d * d / 4 + 1)?;
            let s = r.len() - bm.len();
            let c = r.pop().expect("nonempty");
            if !c.is_empty() {
                for (i, bi) in bm[..bm.len() - 1].iter().enumerate() {
                    if !bi.is_empty() {
                        r[s + i] = modp::sub(f, &r[s + i], &mulk(&c, bi));
                    }
                }
            }
            r = trimk(r);
        }
        a = bm;
        b = r;
    }
    let Some(l) = a.last() else { return Ok(Some(vec![])) };
    let Some(li) = inv_mod(f, l, fp) else { return Ok(None) };
    Ok(Some(a.iter().map(|c| mulk(c, &li)).collect()))
}

/// `x` with `x = a mod mo` and `x = r mod p`, in `[0, mo p)`.
fn crt(a: &Nat, mo: &Nat, r: u64, p: u64) -> Nat {
    let f = Fp::new(p);
    let am = nat_mod(a, p);
    let mm = nat_mod(mo, p);
    let t = f.mul(f.sub(r, am), f.inv(mm));
    a.add(&mo.mul(&Nat::from_u64(t)))
}

/// Wang's rational reconstruction of `u mod mo`: `n/d` with `|n|, d <= sqrt(mo / 2)`.
pub fn rational_reconstruct(u: &Nat, mo: &Nat) -> Option<Rat> {
    let bound = mo.shr(1).isqrt();
    let (mut r0, mut r1) = (mo.clone(), u.clone());
    let (mut s0, mut s1) = (Int::from_i64(0), Int::from_i64(1));
    while r1.cmp_nat(&bound) == std::cmp::Ordering::Greater {
        let (qu, r) = r0.divrem(&r1);
        let s = s0.sub(&s1.mul(&Int::from_nat(qu)));
        (r0, r1) = (r1, r);
        (s0, s1) = (s1, s);
    }
    if s1.is_zero() || s1.mag.cmp_nat(&bound) == std::cmp::Ordering::Greater {
        return None;
    }
    // u = r1 / s1 (mod mo)
    let num = Int::new(s1.neg, r1);
    Some(Rat::new(num, Int::from_nat(s1.mag)))
}

trait CmpNat {
    fn cmp_nat(&self, o: &Nat) -> std::cmp::Ordering;
}

impl CmpNat for Nat {
    fn cmp_nat(&self, o: &Nat) -> std::cmp::Ordering {
        Int::from_nat(self.clone()).cmp_int(&Int::from_nat(o.clone()))
    }
}

/// The most primes the modular gcd tries before falling back to Euclid's algorithm.
const MAX_PRIMES: usize = 400;

/// The modular gcd; `None` when it did not finish within [`MAX_PRIMES`] primes.
fn gcd_modular(k: &NumberField, a: &[Elem], b: &[Elem], m: &mut Meter) -> WorkResult<Option<KPoly>> {
    let d = k.degree();
    let mut best: Option<usize> = None;
    // the images combined so far: per coefficient of x, per coefficient of t
    let mut acc: Vec<Vec<Nat>> = vec![];
    let mut modulus = Nat::from_u64(1);
    let mut last: Option<KPoly> = None;
    let mut used = 0usize;
    let mut next_check = 1usize;
    for (tried, p) in Primes::large().enumerate() {
        // every prime is charged and the stop polled: after a stop the number kernels return arbitrary values, every
        // prime can be skipped below (an unlucky reduction), and the loop would never reach `used`
        if mc_arith::num::stopped() {
            return Err(mc_poly::work::Halt::Stopped);
        }
        m.step(64)?;
        if used >= MAX_PRIMES || tried >= 8 * MAX_PRIMES {
            return Ok(None);
        }
        let f = Fp::new(p);
        let Some(fp) = elem_mod(k.modulus(), f) else { continue };
        if fp.len() != d + 1 {
            continue;
        }
        let image = |x: &[Elem]| -> Option<Vec<UPoly>> { x.iter().map(|c| elem_mod(c, f)).collect() };
        let (Some(ap), Some(bp)) = (image(a), image(b)) else { continue };
        // leading coefficients must stay nonzero
        if ap.last().is_none_or(|c| c.is_empty()) || bp.last().is_none_or(|c| c.is_empty()) {
            continue;
        }
        let Some(g) = gcd_mod(f, &fp, ap, bp, m)? else { continue };
        used += 1;
        let gd = g.len() - 1;
        if gd == 0 {
            return Ok(Some(vec![k.one()]));
        }
        match best {
            Some(b) if gd > b => continue,
            Some(b) if gd < b => {
                acc.clear();
                modulus = Nat::from_u64(1);
                last = None;
                next_check = used + 1;
            }
            _ => {}
        }
        best = Some(gd);
        // combine
        if acc.is_empty() {
            acc = g.iter().map(|c| (0..d).map(|i| Nat::from_u64(c.get(i).copied().unwrap_or(0))).collect()).collect();
            modulus = Nat::from_u64(p);
        } else {
            for (row, c) in acc.iter_mut().zip(&g) {
                for (j, a) in row.iter_mut().enumerate() {
                    let r = c.get(j).copied().unwrap_or(0);
                    *a = crt(a, &modulus, r, p);
                }
            }
            modulus = modulus.mul(&Nat::from_u64(p));
        }
        // reconstruct at doubling intervals, then try the candidate
        if used < next_check {
            continue;
        }
        next_check = used + used.div_ceil(2).max(1);
        let Some(cand) = reconstruct(k, &acc, &modulus) else { continue };
        if last.as_ref() == Some(&cand) || last.is_none() {
            // stable or first: test by division
            let ok_a = divrem(k, a, &cand, m)?.1.is_empty();
            if ok_a && divrem(k, b, &cand, m)?.1.is_empty() {
                return Ok(Some(cand));
            }
        }
        last = Some(cand);
    }
    Ok(None)
}

/// The monic polynomial over K whose coefficients reconstruct the images; `None` when one does not.
fn reconstruct(k: &NumberField, acc: &[Vec<Nat>], mo: &Nat) -> Option<KPoly> {
    let mut out = Vec::with_capacity(acc.len());
    for c in acc {
        let mut e = Vec::with_capacity(c.len());
        for u in c {
            e.push(rational_reconstruct(u, mo)?);
        }
        out.push(qpoly::trim(e));
    }
    let out = trim(out);
    (out.last() == Some(&k.one())).then_some(out)
}

// ------------------------------------------------------------------------------------------- norms

/// The norm `N_{K/Q}(P)`, a polynomial over Q of degree `d deg P`.
pub fn norm(k: &NumberField, p: &[Elem], m: &mut Meter) -> WorkResult<QPoly> {
    let p = trim(p.to_vec());
    if p.is_empty() {
        return Ok(vec![]);
    }
    if let Some(pq) = as_rational(k, &p) {
        // P^d
        let mut out = vec![q(1)];
        for _ in 0..k.degree() {
            out = qpoly::mul(&out, &pq, m)?;
        }
        return Ok(out);
    }
    let n = k.degree() * (p.len() - 1);
    let mut ys = Vec::with_capacity(n + 1);
    for x0 in 0..=n {
        let v = eval(k, &p, &k.of(&q(x0 as i64)));
        ys.push(k.norm(&v, m)?);
    }
    Ok(interpolate_0n(&ys))
}

// ------------------------------------------------------------------------------------------- factorisation

/// A factorisation over K: `unit * prod f_i^e_i`, factors monic and irreducible.
#[derive(Clone, Debug, PartialEq)]
pub struct Factored {
    /// the leading coefficient
    pub unit: Elem,
    /// (monic irreducible factor, multiplicity)
    pub factors: Vec<(KPoly, u32)>,
}

/// The square-free decomposition of a monic `a` (Yun): `(factor, multiplicity)` with square-free, pairwise coprime
/// factors of positive degree.
pub fn square_free(k: &NumberField, a: &[Elem], m: &mut Meter) -> WorkResult<Vec<(KPoly, u32)>> {
    let a = monic(k, a);
    let mut out = vec![];
    if deg(&a).unwrap_or(0) == 0 {
        return Ok(out);
    }
    let da = derivative(k, &a);
    let g = gcd(k, &a, &da, m)?;
    let mut b = divrem(k, &a, &g, m)?.0;
    let mut c = divrem(k, &da, &g, m)?.0;
    let mut dd = sub(k, &c, &derivative(k, &b));
    let mut i = 1;
    while deg(&b).unwrap_or(0) > 0 {
        let h = gcd(k, &b, &dd, m)?;
        if deg(&h).unwrap_or(0) > 0 {
            out.push((h.clone(), i));
        }
        b = divrem(k, &b, &h, m)?.0;
        c = divrem(k, &dd, &h, m)?.0;
        dd = sub(k, &c, &derivative(k, &b));
        i += 1;
    }
    Ok(out)
}

/// The shifts tried for a square-free norm: 0, 1, -1, 2, -2, ...
const MAX_SHIFT: i64 = 40;

/// The irreducible monic factors of a square-free monic `a` over K (Trager); `None` when no shift up to
/// [`MAX_SHIFT`] gave a square-free norm.
pub fn factor_square_free(k: &NumberField, a: &[Elem], m: &mut Meter) -> WorkResult<Option<Vec<KPoly>>> {
    let a = monic(k, a);
    if deg(&a).unwrap_or(0) <= 1 {
        return Ok(Some(if a.len() == 2 { vec![a] } else { vec![] }));
    }
    let t = k.gen();
    for s in (0..=2 * MAX_SHIFT).map(|i| if i % 2 == 0 { -(i / 2) } else { i / 2 + 1 }) {
        if s == 0 && as_rational(k, &a).is_some() && k.degree() > 1 {
            // a rational polynomial's norm is a power: skip the useless shift
            continue;
        }
        // b(x) = a(x - s t)
        let c = k.scale(&t, &q(-s));
        let b = shift(k, &a, &c, m)?;
        let n = norm(k, &b, m)?;
        if !qpoly::is_square_free(&n, m)? {
            continue;
        }
        let fs = qpoly::factor(&n, m)?;
        if fs.len() == 1 {
            return Ok(Some(vec![a]));
        }
        let mut out = vec![];
        let back = k.scale(&t, &q(s));
        for (h, _) in &fs {
            let g = gcd(k, &lift(h), &b, m)?;
            out.push(shift(k, &g, &back, m)?);
        }
        return Ok(Some(out));
    }
    Ok(None)
}

/// The factorisation of a nonzero `a` over K, checked by expansion; `None` when Trager's method did not apply
/// within its bounds (or, which would be a bug, the check failed).
pub fn factor(k: &NumberField, a: &[Elem], m: &mut Meter) -> WorkResult<Option<Factored>> {
    let a = trim(a.to_vec());
    let Some(unit) = a.last().cloned() else { return Ok(None) };
    let mut factors = vec![];
    for (s, e) in square_free(k, &a, m)? {
        // a rational part is factored over Q first: Trager's norms then have degree d deg(f_i), not d deg(s)
        let pieces: Vec<KPoly> = match as_rational(k, &s) {
            Some(sq) if k.degree() > 1 => qpoly::factor(&sq, m)?.into_iter().map(|(f, _)| lift(&f)).collect(),
            _ => vec![s],
        };
        for piece in pieces {
            let Some(fs) = factor_square_free(k, &piece, m)? else { return Ok(None) };
            for f in fs {
                factors.push((f, e));
            }
        }
    }
    // the check: unit * prod f^e == a
    let mut prod = vec![unit.clone()];
    for (f, e) in &factors {
        for _ in 0..*e {
            prod = mul(k, &prod, f, m)?;
        }
    }
    if prod != a {
        return Ok(None);
    }
    Ok(Some(Factored { unit, factors }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qpoly::of_i64;
    use mc_poly::work::Unlimited;

    fn sqrt2() -> NumberField {
        NumberField::new(of_i64(&[-2, 0, 1]), None)
    }

    #[test]
    fn gcd_over_sqrt2() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let k = sqrt2();
        let t = k.gen();
        // (x - t)(x + 1) and (x - t)(x - 3)
        let xt = vec![k.neg(&t), k.one()];
        let a = mul(&k, &xt, &lift(&of_i64(&[1, 1])), &mut m).unwrap();
        let b = mul(&k, &xt, &lift(&of_i64(&[-3, 1])), &mut m).unwrap();
        let g = gcd(&k, &a, &b, &mut m).unwrap();
        assert_eq!(g, xt);
        assert_eq!(gcd_euclid(&k, &a, &b, &mut m).unwrap(), xt);
        // 1/2 and -2/3 modulo 1000003
        assert_eq!(rational_reconstruct(&Nat::from_u64(500002), &Nat::from_u64(1000003)), Some(qpoly::qq(1, 2)));
        assert_eq!(rational_reconstruct(&Nat::from_u64(666668), &Nat::from_u64(1000003)), Some(qpoly::qq(-2, 3)));
    }

    #[test]
    fn factor_x4_plus_1() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let k = sqrt2();
        let f = factor(&k, &lift(&of_i64(&[1, 0, 0, 0, 1])), &mut m).unwrap().unwrap();
        assert_eq!(f.factors.len(), 2);
        assert!(f.factors.iter().all(|(g, e)| g.len() == 3 && *e == 1));
        // x^2 - 2 over Q(sqrt 2): two linear factors
        let f = factor(&k, &lift(&of_i64(&[-2, 0, 1])), &mut m).unwrap().unwrap();
        assert_eq!(f.factors.len(), 2);
        // (x^2 - 2)^2 (x + 1): multiplicities
        let p = qpoly::mul(&qpoly::mul(&of_i64(&[-2, 0, 1]), &of_i64(&[-2, 0, 1]), &mut m).unwrap(), &of_i64(&[1, 1]), &mut m).unwrap();
        let f = factor(&k, &lift(&p), &mut m).unwrap().unwrap();
        let mut es: Vec<u32> = f.factors.iter().map(|(_, e)| *e).collect();
        es.sort();
        assert_eq!(es, vec![1, 2, 2]);
    }
}
