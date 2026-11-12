//! Number fields `K = Q[t]/(f)`, `f` monic and irreducible over Q, and their elements in the power basis
//! `1, t, ..., t^(d-1)` (Cohen, *A Course in Computational Algebraic Number Theory*, section 4.2).
//!
//! An element is a [`QPoly`] of degree below `d` (trimmed; zero is empty), so the representation is canonical and
//! equality of elements is equality of vectors. A field may carry an [`Embedding`]: which complex root of `f` the
//! generator `t` stands for; then its elements are complex numbers with certified enclosures and the real ones have
//! exact signs ([`crate::algnum`]).
//!
//! Norms and characteristic polynomials are resultants `Res_t(f(t), x - a(t))`, computed by evaluation at
//! `x = 0, 1, ..., d`, each value a modular integer resultant (`mc-poly`), and interpolation: no symbolic
//! determinant and no coefficient swell inside the computation.
use crate::embed::Embedding;
use crate::qpoly::{self, q, QPoly};
use mc_arith::num::{Int, Rat};
use mc_poly::work::{Meter, Unlimited, WorkResult};
use mc_poly::{Poly, Z};
use std::sync::Arc;

/// An element of a number field: the coefficients of `1, t, ..., t^(d-1)`, trimmed.
pub type Elem = QPoly;

/// The data of a number field.
#[derive(Debug)]
pub struct FieldData {
    /// the monic irreducible modulus `f`: coefficients of `1, t, ..., t^d`
    pub modulus: QPoly,
    /// the primitive integer multiple of the modulus
    pub int_modulus: Vec<Int>,
    /// `Tr(t^k)` for `k = 0 .. 2d - 2` (Newton's identities)
    traces: Vec<Rat>,
    /// which root of the modulus the generator is, when known
    pub embedding: Option<Embedding>,
}

/// A number field `Q[t]/(f)`; cheap to clone (shared).
#[derive(Clone, Debug)]
pub struct NumberField(pub Arc<FieldData>);

impl PartialEq for NumberField {
    fn eq(&self, o: &NumberField) -> bool {
        Arc::ptr_eq(&self.0, &o.0) || (self.0.modulus == o.0.modulus && self.0.embedding == o.0.embedding)
    }
}

/// An unlimited meter for the element operations, which are bounded by the field's degree.
fn with_meter<X>(f: impl FnOnce(&mut Meter) -> WorkResult<X>) -> X {
    let mut w = Unlimited;
    let mut m = Meter::new(&mut w);
    // invariant: an unlimited budget never stops a computation
    f(&mut m).expect("an unlimited budget")
}

impl NumberField {
    /// The field of the monic irreducible `modulus` (irreducibility is the caller's promise: see
    /// [`NumberField::checked`]).
    pub fn new(modulus: QPoly, embedding: Option<Embedding>) -> NumberField {
        let modulus = qpoly::monic(&qpoly::trim(modulus));
        let int_modulus = qpoly::primitive(&modulus);
        let traces = power_traces(&modulus);
        NumberField(Arc::new(FieldData { modulus, int_modulus, traces, embedding }))
    }

    /// The field of `modulus` when it is irreducible of positive degree, else `None`.
    pub fn checked(modulus: &[Rat], embedding: Option<Embedding>, m: &mut Meter) -> WorkResult<Option<NumberField>> {
        let fs = qpoly::factor(modulus, m)?;
        Ok(match fs.as_slice() {
            [(_, 1)] => Some(NumberField::new(modulus.to_vec(), embedding)),
            _ => None,
        })
    }

    /// The degree `d = [K : Q]`.
    pub fn degree(&self) -> usize {
        self.0.modulus.len() - 1
    }

    /// The monic modulus.
    pub fn modulus(&self) -> &[Rat] {
        &self.0.modulus
    }

    /// The embedding, if the field has one.
    pub fn embedding(&self) -> Option<&Embedding> {
        self.0.embedding.as_ref()
    }

    /// Is the generator a known real number?
    pub fn is_real(&self) -> bool {
        self.0.embedding.as_ref().is_some_and(|e| e.real)
    }

    /// The same field with another embedding.
    pub fn with_embedding(&self, e: Option<Embedding>) -> NumberField {
        NumberField(Arc::new(FieldData {
            modulus: self.0.modulus.clone(),
            int_modulus: self.0.int_modulus.clone(),
            traces: self.0.traces.clone(),
            embedding: e,
        }))
    }

    /// `0`.
    pub fn zero(&self) -> Elem {
        vec![]
    }

    /// `1`.
    pub fn one(&self) -> Elem {
        vec![q(1)]
    }

    /// The rational `c`.
    pub fn of(&self, c: &Rat) -> Elem {
        qpoly::trim(vec![c.clone()])
    }

    /// The generator `t`.
    pub fn gen(&self) -> Elem {
        self.reduce(&[q(0), q(1)])
    }

    /// Is `a` rational? Its value then.
    pub fn as_rat(&self, a: &Elem) -> Option<Rat> {
        match a.len() {
            0 => Some(q(0)),
            1 => Some(a[0].clone()),
            _ => None,
        }
    }

    /// The reduction of a polynomial in `t` modulo `f`.
    pub fn reduce(&self, p: &[Rat]) -> Elem {
        let f = &self.0.modulus;
        let d = f.len() - 1;
        let mut r = qpoly::trim(p.to_vec());
        while r.len() > d {
            // invariant: the loop runs while r is longer than the degree, so it is nonempty
            let top = r.pop().expect("a coefficient");
            if top.is_zero() {
                continue;
            }
            let k = r.len() - d;
            for (i, fi) in f[..d].iter().enumerate() {
                if !fi.is_zero() {
                    r[k + i] = r[k + i].sub(&top.mul(fi));
                }
            }
            r = qpoly::trim(r);
        }
        r
    }

    /// `a + b`.
    pub fn add(&self, a: &Elem, b: &Elem) -> Elem {
        qpoly::add(a, b)
    }

    /// `a - b`.
    pub fn sub(&self, a: &Elem, b: &Elem) -> Elem {
        qpoly::sub(a, b)
    }

    /// `-a`.
    pub fn neg(&self, a: &Elem) -> Elem {
        a.iter().map(|c| c.neg()).collect()
    }

    /// `a b`.
    pub fn mul(&self, a: &Elem, b: &Elem) -> Elem {
        if a.is_empty() || b.is_empty() {
            return vec![];
        }
        if a.len() == 1 {
            return qpoly::scale(b, &a[0]);
        }
        if b.len() == 1 {
            return qpoly::scale(a, &b[0]);
        }
        self.reduce(&with_meter(|m| qpoly::mul(a, b, m)))
    }

    /// `c a` for a rational `c`.
    pub fn scale(&self, a: &Elem, c: &Rat) -> Elem {
        qpoly::scale(a, c)
    }

    /// `1 / a`, `None` for zero.
    pub fn inv(&self, a: &Elem) -> Option<Elem> {
        if a.is_empty() {
            return None;
        }
        if a.len() == 1 {
            return Some(vec![a[0].recip()?]);
        }
        let (g, s, _) = with_meter(|m| qpoly::xgcd(a, &self.0.modulus, m));
        // f irreducible: the gcd is 1
        (g.len() == 1).then(|| self.reduce(&s))
    }

    /// `a / b`, `None` when `b` is zero.
    pub fn div(&self, a: &Elem, b: &Elem) -> Option<Elem> {
        Some(self.mul(a, &self.inv(b)?))
    }

    /// `a^n` for an integer `n` (negative powers of zero give `None`).
    pub fn pow(&self, a: &Elem, n: i64) -> Option<Elem> {
        let base = if n < 0 { self.inv(a)? } else { a.clone() };
        let mut e = n.unsigned_abs();
        let mut acc = self.one();
        let mut b = base;
        while e > 0 {
            if e & 1 == 1 {
                acc = self.mul(&acc, &b);
            }
            e >>= 1;
            if e > 0 {
                b = self.mul(&b, &b);
            }
        }
        Some(acc)
    }

    /// The trace `Tr_{K/Q}(a)`.
    pub fn trace(&self, a: &Elem) -> Rat {
        let mut acc = q(0);
        for (c, t) in a.iter().zip(&self.0.traces) {
            acc = acc.add(&c.mul(t));
        }
        acc
    }

    /// The characteristic polynomial of `a` (monic, degree `d`): `prod (x - a(t_i))` over the roots `t_i` of `f`.
    pub fn charpoly(&self, a: &Elem, m: &mut Meter) -> WorkResult<QPoly> {
        let d = self.degree();
        if a.len() <= 1 {
            // (x - c)^d
            let c = a.first().cloned().unwrap_or_else(|| q(0));
            let mut p = vec![q(1)];
            for _ in 0..d {
                p = qpoly::mul(&p, &[c.neg(), q(1)], m)?;
            }
            return Ok(p);
        }
        // the values prod (x0 - a(t_i)) at x0 = 0 .. d, each a resultant
        let mut ys = Vec::with_capacity(d + 1);
        for x0 in 0..=d {
            m.step(d as u64 * d as u64)?;
            let g = qpoly::sub(&[q(x0 as i64)], a);
            ys.push(self.norm_poly_value(&g, m)?);
        }
        Ok(interpolate_0n(&ys))
    }

    /// `prod g(t_i)` over the roots of `f`, for a polynomial `g` over Q (the norm of `g(t)`).
    pub fn norm_poly_value(&self, g: &[Rat], m: &mut Meter) -> WorkResult<Rat> {
        let g = qpoly::trim(g.to_vec());
        if g.is_empty() {
            return Ok(q(0));
        }
        let d = self.degree();
        let (den, gi) = qpoly::clear(&g);
        if gi.len() == 1 {
            // a constant: c^d
            return Ok(g[0].pow_u64(d as u64));
        }
        let fz: Vec<Z> = self.0.int_modulus.iter().cloned().map(Z::from_int).collect();
        let gz: Vec<Z> = gi.iter().cloned().map(Z::from_int).collect();
        let r = mc_poly::resultant::resultant(&Poly::from_dense(1, 0, &fz), &Poly::from_dense(1, 0, &gz), 0, m)?;
        let r = Rat::from_int(r.constant_term().to_int());
        // Res(F, G) = lc(F)^deg G prod G(t_i), and G = den g
        let lc = Rat::from_int(self.0.int_modulus[d].clone());
        let scale = lc.pow_u64((gi.len() - 1) as u64).mul(&Rat::from_int(Int::from_nat(den)).pow_u64(d as u64));
        Ok(qpoly::rdiv(&r, &scale))
    }

    /// The norm `N_{K/Q}(a)`.
    pub fn norm(&self, a: &Elem, m: &mut Meter) -> WorkResult<Rat> {
        self.norm_poly_value(a, m)
    }

    /// The minimal polynomial of `a` over Q (monic): the square-free part of its characteristic polynomial, which
    /// is a power of it.
    pub fn minpoly(&self, a: &Elem, m: &mut Meter) -> WorkResult<QPoly> {
        if a.len() <= 1 {
            return Ok(vec![a.first().cloned().unwrap_or_else(|| q(0)).neg(), q(1)]);
        }
        let c = self.charpoly(a, m)?;
        qpoly::square_free(&c, m)
    }

    /// `a` as a polynomial in `t` evaluated at the element `x`: `a(x)`.
    pub fn compose(&self, a: &[Rat], x: &Elem) -> Elem {
        let mut acc = self.zero();
        for c in a.iter().rev() {
            acc = self.add(&self.mul(&acc, x), &self.of(c));
        }
        acc
    }
}

/// The traces `Tr(t^k)` of the powers of a root of the monic `f`, `k = 0 .. 2d - 2`, by Newton's identities.
fn power_traces(f: &[Rat]) -> Vec<Rat> {
    let d = f.len() - 1;
    // e_k-style coefficients: f = t^d + c_{d-1} t^{d-1} + ... + c_0; p_k + c_{d-1} p_{k-1} + ... + k c_{d-k} = 0
    let c = |i: usize| f[i].clone();
    let n = (2 * d).max(1);
    let mut p: Vec<Rat> = Vec::with_capacity(n);
    p.push(q(d as i64));
    for k in 1..n {
        let mut s = q(0);
        for i in 1..=k.min(d) {
            let ci = c(d - i);
            if ci.is_zero() {
                continue;
            }
            if i < k {
                s = s.add(&ci.mul(&p[k - i]));
            } else {
                s = s.add(&ci.mul(&q(k as i64)));
            }
        }
        p.push(s.neg());
    }
    p
}

/// The polynomial of degree at most `n` through `(k, ys[k])`, `k = 0 .. n` (Newton's divided differences).
pub fn interpolate_0n(ys: &[Rat]) -> QPoly {
    let n = ys.len();
    let mut dd = ys.to_vec();
    for j in 1..n {
        for i in (j..n).rev() {
            dd[i] = qpoly::rdiv(&dd[i].sub(&dd[i - 1]), &q(j as i64));
        }
    }
    // p = dd0 + dd1 x + dd2 x (x - 1) + ...
    let mut p: QPoly = vec![];
    for i in (0..n).rev() {
        // p = p (x - i) + dd[i]
        let mut np = vec![q(0); p.len() + 1];
        for (k, c) in p.iter().enumerate() {
            np[k + 1] = np[k + 1].add(c);
            np[k] = np[k].sub(&c.mul(&q(i as i64)));
        }
        np[0] = np[0].add(&dd[i]);
        p = qpoly::trim(np);
    }
    p
}

/// An element of a number field with its field: arithmetic without passing the field around (the type
/// `mc-linalg` computes with).
#[derive(Clone, Debug, PartialEq)]
pub struct Alg {
    /// the element
    pub c: Elem,
    /// the field
    pub k: NumberField,
}

impl Alg {
    /// The rational `c` in `k`.
    pub fn rational(c: Rat, k: &NumberField) -> Alg {
        Alg { c: k.of(&c), k: k.clone() }
    }
    /// The generator of `k`.
    pub fn generator(k: &NumberField) -> Alg {
        Alg { c: k.gen(), k: k.clone() }
    }
    /// Is it zero?
    pub fn is_zero(&self) -> bool {
        self.c.is_empty()
    }
    /// The coefficient of `t^i` (0 beyond the stored ones).
    pub fn coef(&self, i: usize) -> Rat {
        self.c.get(i).cloned().unwrap_or_else(|| q(0))
    }
    /// `self + o`.
    pub fn add(&self, o: &Alg) -> Alg {
        Alg { c: self.k.add(&self.c, &o.c), k: self.k.clone() }
    }
    /// `self - o`.
    pub fn sub(&self, o: &Alg) -> Alg {
        Alg { c: self.k.sub(&self.c, &o.c), k: self.k.clone() }
    }
    /// `self o`.
    pub fn mul(&self, o: &Alg) -> Alg {
        Alg { c: self.k.mul(&self.c, &o.c), k: self.k.clone() }
    }
    /// `self / o`, `None` for zero.
    pub fn div(&self, o: &Alg) -> Option<Alg> {
        Some(Alg { c: self.k.div(&self.c, &o.c)?, k: self.k.clone() })
    }
}

/// A number field as a coefficient ring of `mc-poly`'s generic algorithms (subresultant chains, gcds,
/// decompositions over K; item F2).
impl mc_poly::ring::CoeffRing for NumberField {
    type E = Elem;
    fn zero(&self) -> Elem {
        NumberField::zero(self)
    }
    fn one(&self) -> Elem {
        NumberField::one(self)
    }
    fn is_zero(&self, a: &Elem) -> bool {
        a.is_empty()
    }
    fn of_i64(&self, n: i64) -> Elem {
        self.of(&Rat::from_i64(n))
    }
    fn add(&self, a: &Elem, b: &Elem) -> Elem {
        NumberField::add(self, a, b)
    }
    fn sub(&self, a: &Elem, b: &Elem) -> Elem {
        NumberField::sub(self, a, b)
    }
    fn neg(&self, a: &Elem) -> Elem {
        NumberField::neg(self, a)
    }
    fn mul(&self, a: &Elem, b: &Elem, m: &mut Meter) -> WorkResult<Elem> {
        m.step((a.len() * b.len()) as u64 + 1)?;
        Ok(NumberField::mul(self, a, b))
    }
    fn div_exact(&self, a: &Elem, b: &Elem, m: &mut Meter) -> WorkResult<Option<Elem>> {
        m.step(self.degree() as u64 * 4)?;
        Ok(NumberField::div(self, a, b))
    }
    fn is_field(&self) -> bool {
        true
    }
    fn size(&self, a: &Elem) -> u64 {
        a.len() as u64 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qpoly::of_i64;

    fn meter_run<X>(f: impl FnOnce(&mut Meter) -> WorkResult<X>) -> X {
        with_meter(f)
    }

    #[test]
    fn sqrt2() {
        let k = NumberField::new(of_i64(&[-2, 0, 1]), None);
        let t = k.gen();
        assert_eq!(k.mul(&t, &t), k.of(&q(2)));
        let one_plus = k.add(&k.one(), &t);
        assert_eq!(k.inv(&one_plus).unwrap(), k.sub(&t, &k.one()));
        assert_eq!(k.trace(&t), q(0));
        assert_eq!(k.trace(&k.one()), q(2));
        assert_eq!(meter_run(|m| k.norm(&one_plus, m)), q(-1));
        assert_eq!(meter_run(|m| k.minpoly(&one_plus, m)), of_i64(&[-1, -2, 1]));
    }

    #[test]
    fn cubic() {
        // t^3 - t - 1: Tr(t^k) are 3, 0, 2, 3, 2
        let k = NumberField::new(of_i64(&[-1, -1, 0, 1]), None);
        assert_eq!(k.0.traces[..5].to_vec(), of_i64(&[3, 0, 2, 3, 2]));
        let t = k.gen();
        let t2 = k.mul(&t, &t);
        // t^2 has minimal polynomial x^3 - 2x^2 + x - 1
        assert_eq!(meter_run(|m| k.minpoly(&t2, m)), of_i64(&[-1, 1, -2, 1]));
        assert_eq!(meter_run(|m| k.charpoly(&k.of(&q(2)), m)), of_i64(&[-8, 12, -6, 1]));
        assert_eq!(interpolate_0n(&[q(1), q(2), q(5)]), of_i64(&[1, 0, 1]));
        let inv = k.inv(&t2).unwrap();
        assert_eq!(k.mul(&inv, &t2), k.one());
        assert_eq!(k.pow(&t, -2).unwrap(), inv);
    }
}
