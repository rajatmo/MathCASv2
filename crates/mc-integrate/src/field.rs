//! The field of the tower: rational functions over the rationals in all the variables of one integration problem
//! (the integration variable, the tower's monomials, the constants), with an optional imaginary unit.
//!
//! An element is a [`RatFun`] in lowest terms. When the field has an imaginary unit (variable `imag`), every result
//! is reduced modulo `i^2 + 1` and its denominator is made free of `i` by multiplying with the conjugate, so the
//! representation stays unique and zero is recognised: the field is `Q(i)(v_1, ..., v_n)`, not `Q(i, v_1, ...)` with
//! `i` transcendental. Every other constant (a symbol `a`, `pi`, `2^(1/2)`, `log(3)`) is an independent variable;
//! an identity found with them as indeterminates holds for every value of them (it is a polynomial identity), which
//! is all the ansatz and the Risch algorithm use (decision 14A.2 in the NOTES).
use mc_poly::ratfun::RatFun;
use mc_poly::ring::CoeffRing;
use mc_poly::work::{Halt, Meter, Work, WorkResult};
use mc_poly::{Poly, Z};

/// The field of one problem: `nv` variables, `imag` the index of the imaginary unit when there is one.
#[derive(Clone, Copy, Debug)]
pub struct Fld {
    /// the number of variables
    pub nv: usize,
    /// the variable that is the imaginary unit
    pub imag: Option<usize>,
}

/// A budget that stops only with `mc-arith`'s stop flag (for the infallible ring operations).
struct ArithStop;

impl Work for ArithStop {
    fn charge(&mut self, _units: u64) -> WorkResult<()> {
        if mc_arith::num::stopped() {
            Err(Halt::Stopped)
        } else {
            Ok(())
        }
    }
}

impl Fld {
    /// The integer `n`.
    pub fn int(&self, n: i64) -> RatFun {
        RatFun::constant(self.nv, Z::S(n), Z::ONE)
    }
    /// The rational `n/d` (d positive).
    pub fn rat(&self, n: i64, d: i64) -> RatFun {
        let g = gcd_i64(n, d);
        RatFun::constant(self.nv, Z::S(n / g), Z::S(d / g))
    }
    /// The variable `v`.
    pub fn var(&self, v: usize) -> RatFun {
        self.poly(Poly::var(self.nv, v))
    }
    /// A polynomial as an element (already reduced modulo `i^2 + 1` by the caller when needed).
    pub fn poly(&self, p: Poly) -> RatFun {
        RatFun { num: self.reduce_i(&p), den: Poly::constant(self.nv, Z::ONE) }
    }
    /// Is `a` free of variable `v` (numerator and denominator)?
    pub fn free_of(&self, a: &RatFun, v: usize) -> bool {
        a.num.degree(v).unwrap_or(0) == 0 && a.den.degree(v).unwrap_or(0) == 0
    }
    /// Is `a` free of every variable in `vs`?
    pub fn free_of_all(&self, a: &RatFun, vs: &[usize]) -> bool {
        vs.iter().all(|&v| self.free_of(a, v))
    }
    /// Is `a` a polynomial (denominator a nonzero rational)?
    pub fn is_poly(&self, a: &RatFun) -> bool {
        a.den.is_constant()
    }
    /// `p` with `i^2` replaced by `-1`.
    pub fn reduce_i(&self, p: &Poly) -> Poly {
        let Some(i) = self.imag else { return p.clone() };
        if p.degree(i).unwrap_or(0) < 2 {
            return p.clone();
        }
        let terms = p
            .terms()
            .map(|(e, c)| {
                let mut f = e.to_vec();
                let k = f[i] / 2;
                f[i] %= 2;
                (f, if k % 2 == 1 { c.neg() } else { c.clone() })
            })
            .collect();
        Poly::from_terms(self.nv, terms)
    }
    /// `num/den` in normal form (den nonzero).
    pub fn quo(&self, num: Poly, den: Poly, m: &mut Meter) -> WorkResult<RatFun> {
        let (mut num, mut den) = (self.reduce_i(&num), self.reduce_i(&den));
        if let Some(i) = self.imag {
            if den.degree(i).unwrap_or(0) > 0 {
                // den = d0 + d1 i: multiply by the conjugate d0 - d1 i
                let mut conj_terms = vec![];
                for (e, c) in den.terms() {
                    conj_terms.push((e.to_vec(), if e[i] == 1 { c.neg() } else { c.clone() }));
                }
                let conj = Poly::from_terms(self.nv, conj_terms);
                num = self.reduce_i(&num.mul(&conj, m)?);
                den = self.reduce_i(&den.mul(&conj, m)?);
            }
        }
        RatFun::reduced(num, den, m)
    }
    /// `a + b` (metered).
    pub fn addm(&self, a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<RatFun> {
        if a.num.is_zero() {
            return Ok(b.clone());
        }
        if b.num.is_zero() {
            return Ok(a.clone());
        }
        if a.den == b.den {
            return self.quo(a.num.add(&b.num), a.den.clone(), m);
        }
        let s = a.add(b, m)?;
        Ok(RatFun { num: self.reduce_i(&s.num), den: s.den })
    }
    /// `a - b` (metered).
    pub fn subm(&self, a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<RatFun> {
        self.addm(a, &self.neg(b), m)
    }
    /// `a b`.
    pub fn mulm(&self, a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<RatFun> {
        if a.num.is_zero() || b.num.is_zero() {
            return Ok(self.zero());
        }
        if self.imag.is_none() {
            return a.mul(b, m);
        }
        self.quo(a.num.mul(&b.num, m)?, a.den.mul(&b.den, m)?, m)
    }
    /// `a / b`, `None` when `b` is zero.
    pub fn divm(&self, a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<Option<RatFun>> {
        if b.num.is_zero() {
            return Ok(None);
        }
        if a.num.is_zero() {
            return Ok(Some(self.zero()));
        }
        Ok(Some(self.quo(a.num.mul(&b.den, m)?, a.den.mul(&b.num, m)?, m)?))
    }
    /// `a^k` for an integer `k` (`None` for a negative power of zero).
    pub fn powm(&self, a: &RatFun, k: i64, m: &mut Meter) -> WorkResult<Option<RatFun>> {
        let mut r = self.one();
        let mut b = a.clone();
        let mut e = k.unsigned_abs();
        while e > 0 {
            if e & 1 == 1 {
                r = self.mulm(&r, &b, m)?;
            }
            e >>= 1;
            if e > 0 {
                b = self.mulm(&b, &b, m)?;
            }
        }
        if k < 0 {
            return self.divm(&self.one(), &r, m);
        }
        Ok(Some(r))
    }
    /// The rational number of a constant element (no variables), else `None`.
    pub fn as_rational(&self, a: &RatFun) -> Option<(Z, Z)> {
        Some((a.num.as_constant().or_else(|| a.num.is_zero().then_some(Z::ZERO))?, a.den.as_constant()?))
    }
    /// The partial derivative with respect to variable `v`.
    pub fn partial(&self, a: &RatFun, v: usize, m: &mut Meter) -> WorkResult<RatFun> {
        let dn = a.num.derivative(v);
        let dd = a.den.derivative(v);
        if dd.is_zero() {
            return self.quo(dn, a.den.clone(), m);
        }
        let num = dn.mul(&a.den, m)?.sub(&a.num.mul(&dd, m)?);
        let den = a.den.mul(&a.den, m)?;
        self.quo(num, den, m)
    }
}

fn gcd_i64(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    (a.max(1)) as i64
}

impl CoeffRing for Fld {
    type E = RatFun;
    fn zero(&self) -> RatFun {
        RatFun::constant(self.nv, Z::ZERO, Z::ONE)
    }
    fn one(&self) -> RatFun {
        self.int(1)
    }
    fn is_zero(&self, a: &RatFun) -> bool {
        a.num.is_zero()
    }
    fn of_i64(&self, n: i64) -> RatFun {
        self.int(n)
    }
    fn add(&self, a: &RatFun, b: &RatFun) -> RatFun {
        let mut w = ArithStop;
        let mut m = Meter::new(&mut w);
        self.addm(a, b, &mut m).unwrap_or_else(|_| self.zero())
    }
    fn sub(&self, a: &RatFun, b: &RatFun) -> RatFun {
        self.add(a, &self.neg(b))
    }
    fn neg(&self, a: &RatFun) -> RatFun {
        RatFun { num: a.num.neg(), den: a.den.clone() }
    }
    fn mul(&self, a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<RatFun> {
        self.mulm(a, b, m)
    }
    fn div_exact(&self, a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<Option<RatFun>> {
        self.divm(a, b, m)
    }
    fn is_field(&self) -> bool {
        true
    }
    fn size(&self, a: &RatFun) -> u64 {
        (a.num.len() + a.den.len()) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_poly::work::Unlimited;

    #[test]
    fn imaginary_unit_is_reduced() {
        // variables: 0 = x, 1 = i
        let f = Fld { nv: 2, imag: Some(1) };
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let i = f.var(1);
        let one = f.int(1);
        // i^2 = -1
        assert_eq!(f.mulm(&i, &i, &mut m).unwrap(), f.int(-1));
        // 1/(1 + i) = (1 - i)/2
        let q = f.divm(&one, &f.addm(&one, &i, &mut m).unwrap(), &mut m).unwrap().unwrap();
        let want = f.quo(Poly::constant(2, Z::ONE).sub(&Poly::var(2, 1)), Poly::constant(2, Z::S(2)), &mut m).unwrap();
        assert_eq!(q, want);
        // (x + i)(x - i) = x^2 + 1
        let x = f.var(0);
        let p = f.mulm(&f.addm(&x, &i, &mut m).unwrap(), &f.subm(&x, &i, &mut m).unwrap(), &mut m).unwrap();
        let x2 = f.mulm(&x, &x, &mut m).unwrap();
        assert_eq!(p, f.addm(&x2, &one, &mut m).unwrap());
    }
}
