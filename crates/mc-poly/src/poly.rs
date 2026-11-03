//! The packed sparse multivariate polynomial over the integers (spike decision 1.5, semantics 11).
//!
//! A [`Poly`] in `nv` variables is a list of terms in strictly decreasing lexicographic order of their exponent
//! vectors (variable 0 most significant), with no zero coefficients. Exponents are stored flat (`nv` per term) and
//! coefficients as [`Z`]. The representation is unique, so equality of polynomials is equality of the structs.
//!
//! Rational coefficients are handled one level up ([`crate::qpoly::QPoly`]: an integer polynomial over a common
//! denominator); multiplication lives in [`crate::mul`]. Operations whose cost depends on the input take a
//! [`Meter`] and can be stopped.
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use std::cmp::Ordering;

/// A term as (exponents, coefficient).
pub type Term = (Vec<u32>, Z);

/// A sparse polynomial with integer coefficients in `nv` variables.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Poly {
    pub(crate) nv: usize,
    /// `nv` exponents per term, terms in decreasing lexicographic order
    pub(crate) exps: Vec<u32>,
    pub(crate) coefs: Vec<Z>,
}

impl Poly {
    /// The zero polynomial in `nv` variables.
    pub fn zero(nv: usize) -> Poly {
        Poly { nv, exps: vec![], coefs: vec![] }
    }
    /// The constant `c`.
    pub fn constant(nv: usize, c: Z) -> Poly {
        let mut p = Poly::zero(nv);
        if !c.is_zero() {
            p.exps.resize(nv, 0);
            p.coefs.push(c);
        }
        p
    }
    /// The monomial `c x^e` (`e` has `nv` entries).
    pub fn monomial(e: &[u32], c: Z) -> Poly {
        let mut p = Poly::zero(e.len());
        if !c.is_zero() {
            p.exps.extend_from_slice(e);
            p.coefs.push(c);
        }
        p
    }
    /// The variable `x_i`.
    pub fn var(nv: usize, i: usize) -> Poly {
        let mut e = vec![0; nv];
        e[i] = 1;
        Poly::monomial(&e, Z::ONE)
    }
    /// A polynomial from terms in any order (like terms are combined, zeros dropped).
    pub fn from_terms(nv: usize, mut terms: Vec<Term>) -> Poly {
        terms.sort_by(|a, b| b.0.cmp(&a.0));
        let mut p = Poly::zero(nv);
        let mut it = terms.into_iter().peekable();
        while let Some((e, mut c)) = it.next() {
            while it.peek().is_some_and(|(f, _)| *f == e) {
                c = c.add(&it.next().expect("peeked").1);
            }
            if !c.is_zero() {
                p.exps.extend_from_slice(&e);
                p.coefs.push(c);
            }
        }
        p
    }
    /// From terms already in decreasing order with distinct exponents and nonzero coefficients.
    pub(crate) fn from_sorted(nv: usize, exps: Vec<u32>, coefs: Vec<Z>) -> Poly {
        debug_assert_eq!(exps.len(), nv * coefs.len());
        let p = Poly { nv, exps, coefs };
        debug_assert!(p.is_canonical(), "terms out of order or zero");
        p
    }
    /// Are the terms strictly decreasing with nonzero coefficients?
    pub fn is_canonical(&self) -> bool {
        self.coefs.iter().all(|c| !c.is_zero()) && (1..self.len()).all(|i| self.exps(i - 1) > self.exps(i))
    }

    /// The number of variables.
    pub fn nvars(&self) -> usize {
        self.nv
    }
    /// The number of terms.
    pub fn len(&self) -> usize {
        self.coefs.len()
    }
    /// Is this the zero polynomial?
    pub fn is_zero(&self) -> bool {
        self.coefs.is_empty()
    }
    /// Is this the zero polynomial (no terms)?
    pub fn is_empty(&self) -> bool {
        self.is_zero()
    }
    /// The exponents of term `i`.
    #[inline]
    pub fn exps(&self, i: usize) -> &[u32] {
        &self.exps[i * self.nv..(i + 1) * self.nv]
    }
    /// The coefficient of term `i`.
    #[inline]
    pub fn coef(&self, i: usize) -> &Z {
        &self.coefs[i]
    }
    /// The coefficients, in term order.
    pub fn coefs(&self) -> &[Z] {
        &self.coefs
    }
    /// The terms as (exponents, coefficient).
    pub fn terms(&self) -> impl Iterator<Item = (&[u32], &Z)> + '_ {
        (0..self.len()).map(move |i| (self.exps(i), &self.coefs[i]))
    }
    /// Is this a constant (possibly zero)?
    pub fn is_constant(&self) -> bool {
        self.is_zero() || (self.len() == 1 && self.exps(0).iter().all(|&e| e == 0))
    }
    /// The constant value, if this is a constant.
    pub fn as_constant(&self) -> Option<Z> {
        if self.is_zero() {
            Some(Z::ZERO)
        } else if self.is_constant() {
            Some(self.coefs[0].clone())
        } else {
            None
        }
    }
    /// The coefficient of the constant term.
    pub fn constant_term(&self) -> Z {
        match self.len() {
            0 => Z::ZERO,
            n if self.exps(n - 1).iter().all(|&e| e == 0) => self.coefs[n - 1].clone(),
            _ => Z::ZERO,
        }
    }
    /// The leading coefficient (of the lexicographically largest term); zero for the zero polynomial.
    pub fn lc(&self) -> Z {
        self.coefs.first().cloned().unwrap_or(Z::ZERO)
    }

    /// The degree in variable `v`; `None` for the zero polynomial.
    pub fn degree(&self, v: usize) -> Option<u32> {
        (0..self.len()).map(|i| self.exps(i)[v]).max()
    }
    /// The lowest exponent of variable `v`; `None` for the zero polynomial.
    pub fn low_degree(&self, v: usize) -> Option<u32> {
        (0..self.len()).map(|i| self.exps(i)[v]).min()
    }
    /// The degree in every variable (zeros for the zero polynomial).
    pub fn degrees(&self) -> Vec<u32> {
        let mut d = vec![0; self.nv];
        for i in 0..self.len() {
            for (dv, &e) in d.iter_mut().zip(self.exps(i)) {
                *dv = (*dv).max(e);
            }
        }
        d
    }
    /// The total degree; `None` for the zero polynomial.
    pub fn total_degree(&self) -> Option<u64> {
        (0..self.len()).map(|i| self.exps(i).iter().map(|&e| e as u64).sum()).max()
    }
    /// The largest number of bits of a coefficient.
    pub fn max_bits(&self) -> u64 {
        self.coefs.iter().map(Z::bits).max().unwrap_or(0)
    }

    /// The variables that occur.
    pub fn used_vars(&self) -> Vec<usize> {
        let d = self.degrees();
        (0..self.nv).filter(|&v| d[v] > 0).collect()
    }

    // ------------------------------------------------------------------ ring operations

    /// -self.
    pub fn neg(&self) -> Poly {
        Poly { nv: self.nv, exps: self.exps.clone(), coefs: self.coefs.iter().map(Z::neg).collect() }
    }
    /// self + o (merge of the sorted term lists).
    pub fn add(&self, o: &Poly) -> Poly {
        self.combine(o, false)
    }
    /// self - o.
    pub fn sub(&self, o: &Poly) -> Poly {
        self.combine(o, true)
    }
    fn combine(&self, o: &Poly, negate: bool) -> Poly {
        debug_assert_eq!(self.nv, o.nv);
        let nv = self.nv;
        let mut exps = Vec::with_capacity(self.exps.len() + o.exps.len());
        let mut coefs = Vec::with_capacity(self.len() + o.len());
        let (mut i, mut j) = (0, 0);
        let other = |j: usize| {
            if negate {
                o.coefs[j].neg()
            } else {
                o.coefs[j].clone()
            }
        };
        while i < self.len() || j < o.len() {
            let ord = if i == self.len() {
                Ordering::Less
            } else if j == o.len() {
                Ordering::Greater
            } else {
                self.exps(i).cmp(o.exps(j))
            };
            match ord {
                Ordering::Greater => {
                    exps.extend_from_slice(self.exps(i));
                    coefs.push(self.coefs[i].clone());
                    i += 1;
                }
                Ordering::Less => {
                    exps.extend_from_slice(o.exps(j));
                    coefs.push(other(j));
                    j += 1;
                }
                Ordering::Equal => {
                    let c = if negate { self.coefs[i].sub(&o.coefs[j]) } else { self.coefs[i].add(&o.coefs[j]) };
                    if !c.is_zero() {
                        exps.extend_from_slice(self.exps(i));
                        coefs.push(c);
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        Poly { nv, exps, coefs }
    }
    /// c * self.
    pub fn scale(&self, c: &Z) -> Poly {
        if c.is_zero() {
            return Poly::zero(self.nv);
        }
        Poly { nv: self.nv, exps: self.exps.clone(), coefs: self.coefs.iter().map(|x| x.mul(c)).collect() }
    }
    /// self / c for a constant dividing every coefficient.
    pub fn div_exact_scalar(&self, c: &Z) -> Poly {
        Poly { nv: self.nv, exps: self.exps.clone(), coefs: self.coefs.iter().map(|x| x.div_exact(c)).collect() }
    }
    /// c x^e * self.
    pub fn mul_term(&self, e: &[u32], c: &Z) -> Poly {
        if c.is_zero() {
            return Poly::zero(self.nv);
        }
        let mut exps = self.exps.clone();
        for t in exps.chunks_mut(self.nv.max(1)) {
            for (x, &y) in t.iter_mut().zip(e) {
                *x += y;
            }
        }
        Poly { nv: self.nv, exps, coefs: self.coefs.iter().map(|x| x.mul(c)).collect() }
    }
    /// self * o.
    pub fn mul(&self, o: &Poly, m: &mut Meter) -> WorkResult<Poly> {
        crate::mul::mul(self, o, m)
    }
    /// self^e (binary powering).
    pub fn pow(&self, e: u64, m: &mut Meter) -> WorkResult<Poly> {
        if e == 0 {
            return Ok(Poly::constant(self.nv, Z::ONE));
        }
        // the result's size, bounded by the dense span and by e times the bits of the 1-norm
        // the result's size: at most the dense span, and at most the number of products of e of the terms
        // (binomial(len + e - 1, e)); each coefficient at most e times the bits of the 1-norm
        let span = self.degrees().iter().fold(1f64, |acc, &d| acc * (d as f64 * e as f64 + 1.0));
        let t = self.len() as f64;
        let choose = (0..e.min(1 << 20)).fold(1f64, |acc, i| (acc * (t + i as f64) / (i as f64 + 1.0)).min(1e30));
        let norm_bits = self.coefs.iter().fold(Z::ZERO, |acc, c| acc.add(&c.abs())).bits();
        let bytes = span.min(choose) * ((e as f64) * norm_bits as f64 / 8.0 + 24.0 + 4.0 * self.nv as f64);
        if self.len() > 1 && bytes > crate::mul::MAX_BYTES as f64 {
            return Err(crate::work::Halt::Memory);
        }
        if self.len() == 1 {
            // a monomial: no multiplication needed
            let ex: Option<Vec<u32>> = self.exps(0).iter().map(|&x| u32::try_from(x as u64 * e).ok()).collect();
            let ex = ex.ok_or(crate::work::Halt::Memory)?;
            if self.coefs[0].bits().saturating_mul(e) > 8 * crate::mul::MAX_BYTES {
                return Err(crate::work::Halt::Memory);
            }
            return Ok(Poly::monomial(&ex, self.coefs[0].pow(e)));
        }
        let mut result: Option<Poly> = None;
        let mut base = self.clone();
        let mut e = e;
        loop {
            if e & 1 == 1 {
                result = Some(match result {
                    None => base.clone(),
                    Some(r) => r.mul(&base, m)?,
                });
            }
            e >>= 1;
            if e == 0 {
                break;
            }
            base = base.mul(&base, m)?;
        }
        Ok(result.expect("e >= 1"))
    }

    // ------------------------------------------------------------------ content, derivative, substitution

    /// The nonnegative gcd of the coefficients (zero for the zero polynomial).
    pub fn content(&self) -> Z {
        let mut g = Z::ZERO;
        for c in &self.coefs {
            g = g.gcd(c);
            if g.is_one() {
                break;
            }
        }
        g
    }
    /// The primitive part with a positive leading coefficient, and the signed content (self = content * pp).
    pub fn primitive(&self) -> (Z, Poly) {
        if self.is_zero() {
            return (Z::ZERO, self.clone());
        }
        let mut c = self.content();
        if self.lc().is_neg() {
            c = c.neg();
        }
        (c.clone(), if c.is_one() { self.clone() } else { self.div_exact_scalar(&c) })
    }
    /// The derivative with respect to variable `v`.
    pub fn derivative(&self, v: usize) -> Poly {
        let mut exps = vec![];
        let mut coefs = vec![];
        for (e, c) in self.terms() {
            if e[v] > 0 {
                let mut f = e.to_vec();
                f[v] -= 1;
                exps.extend_from_slice(&f);
                coefs.push(c.mul_i64(e[v] as i64));
            }
        }
        // decreasing the same exponent of every term keeps the order (terms with e[v] = 0 are gone)
        Poly::from_sorted(self.nv, exps, coefs)
    }
    /// The polynomial with variable `v` replaced by the integer `a`.
    pub fn eval_var(&self, v: usize, a: &Z) -> Poly {
        let mut terms = Vec::with_capacity(self.len());
        // powers of a by exponent, computed on demand
        let mut powers: Vec<Z> = vec![Z::ONE];
        for (e, c) in self.terms() {
            while powers.len() <= e[v] as usize {
                let next = powers.last().expect("nonempty").mul(a);
                powers.push(next);
            }
            let mut f = e.to_vec();
            f[v] = 0;
            terms.push((f, c.mul(&powers[e[v] as usize])));
        }
        Poly::from_terms(self.nv, terms)
    }
    /// The coefficients with respect to variable `v`: (exponent, coefficient with `v` removed), by decreasing
    /// exponent.
    pub fn coeffs_in(&self, v: usize) -> Vec<(u32, Poly)> {
        let mut groups: Vec<(u32, Vec<Term>)> = vec![];
        let mut by_exp: rustc_hash::FxHashMap<u32, usize> = Default::default();
        for (e, c) in self.terms() {
            let k = e[v];
            let mut f = e.to_vec();
            f[v] = 0;
            let i = *by_exp.entry(k).or_insert_with(|| {
                groups.push((k, vec![]));
                groups.len() - 1
            });
            groups[i].1.push((f, c.clone()));
        }
        let mut out: Vec<(u32, Poly)> = groups.into_iter().map(|(k, ts)| (k, Poly::from_terms(self.nv, ts))).collect();
        out.sort_by_key(|p| std::cmp::Reverse(p.0));
        out
    }
    /// The polynomial in the variables `vars` of a larger ring of `nv` variables (variable i of self becomes
    /// `vars[i]`).
    pub fn embed(&self, nv: usize, vars: &[usize]) -> Poly {
        let terms = self
            .terms()
            .map(|(e, c)| {
                let mut f = vec![0; nv];
                for (i, &x) in e.iter().enumerate() {
                    f[vars[i]] += x;
                }
                (f, c.clone())
            })
            .collect();
        Poly::from_terms(nv, terms)
    }
    /// The polynomial with its variables permuted: variable i of self becomes variable `perm[i]`.
    pub fn permute(&self, perm: &[usize]) -> Poly {
        self.embed(self.nv, perm)
    }

    // ------------------------------------------------------------------ division

    /// The exact quotient self / d, or `None` when d does not divide self over the integers. Sparse division in
    /// lexicographic order: the leading term of the remainder must be divisible by the leading term of d.
    pub fn div_exact(&self, d: &Poly, m: &mut Meter) -> WorkResult<Option<Poly>> {
        assert!(!d.is_zero(), "division by the zero polynomial");
        if self.is_zero() {
            return Ok(Some(Poly::zero(self.nv)));
        }
        if d.len() == 1 {
            return Ok(self.div_term(d.exps(0), d.coef(0)));
        }
        // the degree bounds of a quotient
        let (ds, dd) = (self.degrees(), d.degrees());
        if ds.iter().zip(&dd).any(|(a, b)| a < b) {
            return Ok(None);
        }
        let lt = d.exps(0).to_vec();
        let lc = d.lc();
        let mut r = self.clone();
        let mut q_terms: Vec<Term> = vec![];
        while !r.is_zero() {
            m.step((r.len() + d.len()) as u64)?;
            let e = r.exps(0);
            if e.iter().zip(&lt).any(|(a, b)| a < b) {
                return Ok(None);
            }
            let (qc, rem) = r.coefs[0].divrem(&lc);
            if !rem.is_zero() {
                return Ok(None);
            }
            let qe: Vec<u32> = e.iter().zip(&lt).map(|(a, b)| a - b).collect();
            r = r.sub(&d.mul_term(&qe, &qc));
            q_terms.push((qe, qc));
        }
        // the quotient terms come out in decreasing order
        let mut q = Poly::zero(self.nv);
        for (e, c) in q_terms {
            q.exps.extend_from_slice(&e);
            q.coefs.push(c);
        }
        Ok(Some(q))
    }
    /// self / (c x^e), if exact.
    fn div_term(&self, e: &[u32], c: &Z) -> Option<Poly> {
        let mut exps = Vec::with_capacity(self.exps.len());
        let mut coefs = Vec::with_capacity(self.len());
        for (f, x) in self.terms() {
            if f.iter().zip(e).any(|(a, b)| a < b) {
                return None;
            }
            let (q, r) = x.divrem(c);
            if !r.is_zero() {
                return None;
            }
            exps.extend(f.iter().zip(e).map(|(a, b)| a - b));
            coefs.push(q);
        }
        Some(Poly::from_sorted(self.nv, exps, coefs))
    }
}

/// Univariate helpers: a polynomial in one variable of the ring, as a dense coefficient vector (index = degree).
impl Poly {
    /// The dense coefficients in variable `v` (the polynomial must not involve other variables).
    pub fn to_dense(&self, v: usize) -> Vec<Z> {
        let n = self.degree(v).map_or(0, |d| d as usize + 1);
        let mut out = vec![Z::ZERO; n];
        for (e, c) in self.terms() {
            out[e[v] as usize] = c.clone();
        }
        out
    }
    /// The polynomial `sum c[k] x_v^k` in `nv` variables.
    pub fn from_dense(nv: usize, v: usize, c: &[Z]) -> Poly {
        let mut p = Poly::zero(nv);
        let mut e = vec![0; nv];
        for k in (0..c.len()).rev() {
            if !c[k].is_zero() {
                e[v] = k as u32;
                p.exps.extend_from_slice(&e);
                p.coefs.push(c[k].clone());
            }
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Unlimited;

    fn p(nv: usize, t: &[(&[u32], i64)]) -> Poly {
        Poly::from_terms(nv, t.iter().map(|(e, c)| (e.to_vec(), Z::S(*c))).collect())
    }

    #[test]
    fn ring_operations() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let x = Poly::var(2, 0);
        let y = Poly::var(2, 1);
        let s = x.add(&y);
        let sq = s.mul(&s, &mut m).unwrap();
        assert_eq!(sq, p(2, &[(&[2, 0], 1), (&[1, 1], 2), (&[0, 2], 1)]));
        assert_eq!(sq.sub(&sq), Poly::zero(2));
        assert_eq!(s.pow(3, &mut m).unwrap().len(), 4);
        let q = sq.div_exact(&s, &mut m).unwrap().unwrap();
        assert_eq!(q, s);
        assert!(sq.add(&Poly::constant(2, Z::ONE)).div_exact(&s, &mut m).unwrap().is_none());
        assert_eq!(sq.derivative(0), p(2, &[(&[1, 0], 2), (&[0, 1], 2)]));
        assert_eq!(sq.eval_var(1, &Z::S(2)), p(2, &[(&[2, 0], 1), (&[1, 0], 4), (&[0, 0], 4)]));
        let (c, pp) = sq.scale(&Z::S(-6)).primitive();
        assert_eq!((c, pp), (Z::S(-6), sq.clone()));
        assert_eq!(sq.coeffs_in(0).len(), 3);
    }
}
