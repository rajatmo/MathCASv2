//! Field elements back to terms: each variable becomes the term it stands for; a power of an exponential monomial
//! `exp(w)^n` is written `exp(n w)`, and the exponential factors of a denominator move to the numerator as
//! negative multiples.
use crate::terms::fun;
use crate::tower::{Kind, Tower};
use mc_api::{Store, T};
use mc_arith::Arith;
use mc_core::Heap;
use mc_poly::ratfun::RatFun;
use mc_poly::{Poly, Z};

/// Writes the elements of one tower.
pub struct Writer<'w, 'h> {
    /// the tower
    pub tw: &'w Tower<'h>,
    /// the arithmetic
    pub ar: &'w Arith<'h, Heap>,
}

impl<'h> Writer<'_, 'h> {
    fn z(&self, c: &Z) -> T<'h> {
        match c.as_i64() {
            Some(v) => self.ar.int(v),
            None => self.ar.integer(&c.to_int()),
        }
    }

    /// The term of variable `v` to the power `e` (any sign).
    fn var_pow(&self, v: usize, e: i64) -> T<'h> {
        let ar = self.ar;
        let h = ar.store();
        if v >= self.tw.vars.len() {
            let t = self.tw.spare.get(v - self.tw.vars.len()).copied().unwrap_or_else(|| ar.int(0));
            return ar.pow(t, ar.int(e));
        }
        let var = &self.tw.vars[v];
        if let Kind::Exp { .. } = var.kind {
            if let Some((_, [w])) = fun(h, var.term) {
                return h.call(h.sym("exp"), &[ar.mul(&[ar.int(e), *w])]);
            }
        }
        ar.pow(var.term, ar.int(e))
    }

    /// The term of a polynomial, every monomial multiplied by `shift` (exponents, possibly negative).
    pub fn poly(&self, p: &Poly, shift: &[i64]) -> T<'h> {
        let ar = self.ar;
        let mut terms = Vec::with_capacity(p.len());
        for (e, c) in p.terms() {
            let mut fs = vec![self.z(c)];
            for (v, &ev) in e.iter().enumerate() {
                let k = ev as i64 + shift.get(v).copied().unwrap_or(0);
                if k != 0 {
                    fs.push(self.var_pow(v, k));
                }
            }
            terms.push(ar.mul(&fs));
        }
        ar.add(&terms)
    }

    /// The term of a field element.
    pub fn ratfun(&self, r: &RatFun) -> T<'h> {
        let ar = self.ar;
        let nv = self.tw.fld.nv;
        // exponential monomials dividing the denominator
        let mut shift = vec![0i64; nv];
        let mut den = r.den.clone();
        for v in 0..self.tw.vars.len() {
            if !matches!(self.tw.vars[v].kind, Kind::Exp { .. }) {
                continue;
            }
            let k = den.low_degree(v).unwrap_or(0);
            if k > 0 {
                shift[v] = -(k as i64);
                let terms = den
                    .terms()
                    .map(|(e, c)| {
                        let mut f = e.to_vec();
                        f[v] -= k;
                        (f, c.clone())
                    })
                    .collect();
                den = Poly::from_terms(nv, terms);
            }
        }
        let n = self.poly(&r.num, &shift);
        if den.as_constant() == Some(Z::ONE) {
            return n;
        }
        let d = self.poly(&den, &[]);
        ar.mul(&[n, ar.pow(d, ar.int(-1))])
    }
}
