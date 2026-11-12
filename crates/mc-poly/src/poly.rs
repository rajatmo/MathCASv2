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
                // invariant: `peek` returned a term
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
        if self.len() > 1 && bytes > crate::mul::max_bytes() as f64 {
            return Err(crate::work::Halt::Memory);
        }
        if self.len() == 1 {
            // a monomial: no multiplication needed
            let ex: Option<Vec<u32>> = self.exps(0).iter().map(|&x| u32::try_from(x as u64 * e).ok()).collect();
            let ex = ex.ok_or(crate::work::Halt::Memory)?;
            if self.coefs[0].bits().saturating_mul(e) > 8 * crate::mul::max_bytes() {
                return Err(crate::work::Halt::Memory);
            }
            return Ok(Poly::monomial(&ex, self.coefs[0].pow(e)));
        }
        // a power of a few-term multivariate polynomial with a large result: the coefficient recurrence builds the
        // result in (terms of the base) steps per result term, where repeated squaring multiplies two huge operands
        if self.nv >= 2 && self.len() <= 16 && span.min(choose) >= 2000.0 {
            if let Some(p) = self.pow_recurrence(e, m)? {
                return Ok(p);
            }
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
        // invariant: the exponent is at least 1 (smaller ones returned above), so the loop multiplied once
        Ok(result.expect("e >= 1"))
    }

    /// self^e by the coefficient recurrence of J. C. P. Miller (1962), in several variables: with a weight `w`
    /// injective on the monomials of `f` (a mixed-radix encoding) and `m_1` the term of least weight, `g = f^e`
    /// satisfies `f E(g) = e g E(f)` for the Euler operator `E` of `w`. Writing `k(N) = w(N) - e w(m_1)` for the
    /// excess of a monomial `N` of `g` and `u_i = w(m_i) - w(m_1) > 0`, the identity gives
    /// `f_1 k g_N = sum_{i>1} f_i (e u_i - k') g_{N'}` with `N' = N m_1 / m_i`, `k' = k - u_i`.
    /// Pushing contributions forward from `m_1^e` in order of excess builds every term once, each from at most
    /// `len - 1` earlier ones. `None` when the weights would overflow.
    fn pow_recurrence(&self, e: u64, m: &mut Meter) -> WorkResult<Option<Poly>> {
        use std::collections::BTreeMap;
        let nv = self.nv;
        let radix = self.degrees().into_iter().max().unwrap_or(0) as u128 + 1;
        let mut w = vec![1u128; nv];
        for v in 1..nv {
            let Some(x) = w[v - 1].checked_mul(radix) else { return Ok(None) };
            w[v] = x;
        }
        let weight = |ex: &[u32]| -> u128 { ex.iter().zip(&w).map(|(&x, &wv)| x as u128 * wv).sum() };
        let ws: Vec<u128> = (0..self.len()).map(|i| weight(self.exps(i))).collect();
        let top = ws.iter().copied().max().unwrap_or(0);
        if top.checked_mul(e as u128).and_then(|x| x.checked_mul(1 << 10)).is_none_or(|x| x >= 1 << 120) {
            return Ok(None);
        }
        let i1 = (0..self.len()).min_by_key(|&i| ws[i]).expect("len >= 2");
        let (m1, f1) = (self.exps(i1), self.coefs[i1].clone());
        let others: Vec<(usize, u128)> = (0..self.len()).filter(|&i| i != i1).map(|i| (i, ws[i] - ws[i1])).collect();
        let start: Option<Vec<u32>> = m1.iter().map(|&x| u32::try_from(x as u64 * e).ok()).collect();
        let Some(start) = start else { return Ok(None) };
        let term_bytes = nv as u64 * 4 + (f1.bits().saturating_mul(e) + e * 8).div_ceil(8) + 96;
        let max_terms = crate::mul::max_bytes() / term_bytes;
        let mut pending: BTreeMap<u128, rustc_hash::FxHashMap<Vec<u32>, Z>> = BTreeMap::new();
        let mut out: Vec<Term> = vec![];
        let mut npending = 0u64;
        let mut cur = Some((0u128, vec![(start, f1.pow(e))]));
        while let Some((k, level)) = cur.take() {
            for (n, g) in level {
                for &(i, u) in &others {
                    let fi = &self.coefs[i];
                    let factor = e as i128 * u as i128 - k as i128;
                    if factor == 0 {
                        continue;
                    }
                    let mi = self.exps(i);
                    let mut t = Vec::with_capacity(nv);
                    let mut ok = true;
                    for v in 0..nv {
                        let x = n[v] as u64 + mi[v] as u64;
                        if x < m1[v] as u64 || x - m1[v] as u64 > u32::MAX as u64 {
                            ok = false;
                            break;
                        }
                        t.push((x - m1[v] as u64) as u32);
                    }
                    if !ok {
                        continue;
                    }
                    let c = g.mul(fi).mul(&Z::from_i128(factor));
                    let slot = pending.entry(k + u).or_default();
                    let before = slot.len();
                    match slot.get_mut(&t) {
                        Some(a) => *a = a.add(&c),
                        None => {
                            slot.insert(t, c);
                        }
                    }
                    npending += (slot.len() - before) as u64;
                }
                m.step(others.len() as u64 * 8 + 16)?;
                out.push((n, g));
            }
            if out.len() as u64 + npending > max_terms {
                return Err(crate::work::Halt::Memory);
            }
            if let Some((k2, map)) = pending.pop_first() {
                npending -= map.len() as u64;
                let div = f1.mul(&Z::from_i128(k2 as i128));
                let level: Vec<(Vec<u32>, Z)> =
                    map.into_iter().filter(|(_, a)| !a.is_zero()).map(|(n, a)| (n, a.div_exact(&div))).collect();
                cur = Some((k2, level));
            }
        }
        Ok(Some(Poly::from_terms(nv, out)))
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
                // invariant: `powers` starts with one element and only grows
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
        if d.is_zero() {
            // a divisor computed after the stop flag was raised may be anything, zero included (`work::exact`)
            if mc_arith::num::stopped() {
                return Err(crate::work::Halt::Stopped);
            }
            // invariant: a zero divisor is a bug unless the stop flag made it (checked above); `work::exact` explains
            panic!("division by the zero polynomial");
        }
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

/// The highest degree in one variable the dense algorithms accept (univariate gcd and factoring, resultants,
/// coefficient lists, numeric roots): they hold one coefficient per degree, so `x^(2^31) - x + 1` would need tens of
/// gigabytes. Beyond it they give `Halt::Memory`, the `failure(limit, memory)` of the natives.
pub const MAX_DENSE_DEGREE: u32 = 1 << 22;

/// Univariate helpers: a polynomial in one variable of the ring, as a dense coefficient vector (index = degree).
impl Poly {
    /// `Err(Halt::Memory)` when the degree in some variable is beyond [`MAX_DENSE_DEGREE`] (checked before a dense
    /// algorithm allocates one coefficient per degree).
    pub fn check_dense(&self) -> WorkResult<()> {
        if self.degrees().iter().any(|&d| d > MAX_DENSE_DEGREE) {
            return Err(crate::work::Halt::Memory);
        }
        Ok(())
    }
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

    #[test]
    fn recurrence_power_agrees_with_repeated_multiplication() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // signed coefficients, a constant term, a missing monomial, three variables
        let f = p(3, &[(&[2, 0, 1], 3), (&[0, 1, 0], -2), (&[0, 0, 0], 5), (&[1, 1, 1], 7)]);
        for e in [2u64, 3, 5, 9] {
            let mut r = f.clone();
            for _ in 1..e {
                r = r.mul(&f, &mut m).unwrap();
            }
            assert_eq!(f.pow_recurrence(e, &mut m).unwrap().unwrap(), r, "e = {e}");
        }
        let t = p(3, &[(&[1, 0, 0], 1), (&[0, 1, 0], 1), (&[0, 0, 1], 1)]);
        let big = t.pow(60, &mut m).unwrap();
        assert_eq!(big.len(), 61 * 62 / 2);
        assert_eq!(big.pow(2, &mut m).unwrap(), t.pow(120, &mut m).unwrap());
    }
}
