//! Coefficient rings for the generic polynomial algorithms of [`crate::unipoly`] (roadmap phase F, item F2).
//!
//! One trait, [`CoeffRing`], describes a commutative ring with exact division: the integers ([`Integers`]), the
//! rationals ([`Rationals`]), the integers modulo a word-sized prime ([`Zp`]), polynomials over the integers in
//! further variables ([`PolyRing`], an integral domain), and rational functions in further variables
//! ([`RatFunField`], a field). `mc-algebra` implements the trait for its number fields, and the rings of functions
//! of the integration engine can implement it the same way, so subresultant chains, gcds and decompositions are
//! written once.
//!
//! A ring value carries the context of the ring (the prime, the number of variables, the modulus of a number
//! field); elements are plain values. Multiplication and division take the work meter, since they may be large
//! (polynomials, big integers); addition is cheap and does not. The existing integer and rational kernels
//! ([`crate::Poly`], [`crate::resultant`], [`crate::gcd`]) are untouched and stay the fast paths (decision F2.1).
use crate::gcd::gcd as poly_gcd;
use crate::modp::Fp;
use crate::poly::Poly;
use crate::ratfun::RatFun;
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use mc_arith::num::Rat;

/// A commutative ring with one, with exact division (decision F2.1).
pub trait CoeffRing {
    /// The elements.
    type E: Clone + PartialEq + std::fmt::Debug;
    /// Zero.
    fn zero(&self) -> Self::E;
    /// One.
    fn one(&self) -> Self::E;
    /// Is `a` zero?
    fn is_zero(&self, a: &Self::E) -> bool;
    /// Is `a` one?
    fn is_one(&self, a: &Self::E) -> bool {
        *a == self.one()
    }
    /// The image of an integer.
    fn of_i64(&self, n: i64) -> Self::E;
    /// `a + b`.
    fn add(&self, a: &Self::E, b: &Self::E) -> Self::E;
    /// `a - b`.
    fn sub(&self, a: &Self::E, b: &Self::E) -> Self::E;
    /// `-a`.
    fn neg(&self, a: &Self::E) -> Self::E;
    /// `a b`.
    fn mul(&self, a: &Self::E, b: &Self::E, m: &mut Meter) -> WorkResult<Self::E>;
    /// `a / b` when `b` divides `a` in the ring; `None` when it does not (or `b` is zero).
    fn div_exact(&self, a: &Self::E, b: &Self::E, m: &mut Meter) -> WorkResult<Option<Self::E>>;
    /// Is every nonzero element a unit?
    fn is_field(&self) -> bool;
    /// The characteristic (0 or a prime).
    fn characteristic(&self) -> u64 {
        0
    }
    /// A greatest common divisor of `a` and `b` (for contents and primitive parts); a field answers 1 unless both
    /// are zero.
    fn gcd(&self, a: &Self::E, b: &Self::E, _m: &mut Meter) -> WorkResult<Self::E> {
        Ok(if self.is_zero(a) && self.is_zero(b) { self.zero() } else { self.one() })
    }
    /// A size estimate of `a` for the work meter (1 for word-sized values).
    fn size(&self, _a: &Self::E) -> u64 {
        1
    }
    /// The inverse of a unit.
    fn inv(&self, a: &Self::E, m: &mut Meter) -> WorkResult<Option<Self::E>> {
        self.div_exact(&self.one(), a, m)
    }
    /// `a^e`.
    fn pow(&self, a: &Self::E, e: u64, m: &mut Meter) -> WorkResult<Self::E> {
        let mut r = self.one();
        let mut b = a.clone();
        let mut e = e;
        while e > 0 {
            if e & 1 == 1 {
                r = self.mul(&r, &b, m)?;
            }
            e >>= 1;
            if e > 0 {
                b = self.mul(&b, &b, m)?;
            }
        }
        Ok(r)
    }
}

/// The integers, elements [`Z`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Integers;

impl CoeffRing for Integers {
    type E = Z;
    fn zero(&self) -> Z {
        Z::ZERO
    }
    fn one(&self) -> Z {
        Z::ONE
    }
    fn is_zero(&self, a: &Z) -> bool {
        a.is_zero()
    }
    fn of_i64(&self, n: i64) -> Z {
        Z::S(n)
    }
    fn add(&self, a: &Z, b: &Z) -> Z {
        a.add(b)
    }
    fn sub(&self, a: &Z, b: &Z) -> Z {
        a.sub(b)
    }
    fn neg(&self, a: &Z) -> Z {
        a.neg()
    }
    fn mul(&self, a: &Z, b: &Z, m: &mut Meter) -> WorkResult<Z> {
        m.step(1 + (a.bits() + b.bits()) / 4096)?;
        Ok(a.mul(b))
    }
    fn div_exact(&self, a: &Z, b: &Z, _m: &mut Meter) -> WorkResult<Option<Z>> {
        if b.is_zero() {
            return Ok(None);
        }
        let (q, r) = a.divrem(b);
        Ok(r.is_zero().then_some(q))
    }
    fn is_field(&self) -> bool {
        false
    }
    fn gcd(&self, a: &Z, b: &Z, _m: &mut Meter) -> WorkResult<Z> {
        Ok(a.gcd(b))
    }
    fn size(&self, a: &Z) -> u64 {
        1 + a.bits() / 64
    }
}

/// The rationals, elements `mc-arith`'s [`Rat`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Rationals;

impl CoeffRing for Rationals {
    type E = Rat;
    fn zero(&self) -> Rat {
        Rat::from_i64(0)
    }
    fn one(&self) -> Rat {
        Rat::from_i64(1)
    }
    fn is_zero(&self, a: &Rat) -> bool {
        a.is_zero()
    }
    fn of_i64(&self, n: i64) -> Rat {
        Rat::from_i64(n)
    }
    fn add(&self, a: &Rat, b: &Rat) -> Rat {
        a.add(b)
    }
    fn sub(&self, a: &Rat, b: &Rat) -> Rat {
        a.sub(b)
    }
    fn neg(&self, a: &Rat) -> Rat {
        a.neg()
    }
    fn mul(&self, a: &Rat, b: &Rat, m: &mut Meter) -> WorkResult<Rat> {
        m.step(1 + (a.num.mag.bits() + a.den.bits() + b.num.mag.bits() + b.den.bits()) / 2048)?;
        Ok(a.mul(b))
    }
    fn div_exact(&self, a: &Rat, b: &Rat, _m: &mut Meter) -> WorkResult<Option<Rat>> {
        Ok(b.recip().map(|r| a.mul(&r)))
    }
    fn is_field(&self) -> bool {
        true
    }
    fn size(&self, a: &Rat) -> u64 {
        1 + (a.num.mag.bits() + a.den.bits()) / 64
    }
}

/// The integers modulo a prime below 2^63, elements residues in `0..p`.
#[derive(Clone, Copy, Debug)]
pub struct Zp(pub Fp);

impl Zp {
    /// The field with `p` elements (`p` a prime below 2^63).
    pub fn new(p: u64) -> Zp {
        Zp(Fp::new(p))
    }
    /// The residue of an integer.
    pub fn of_z(&self, z: &Z) -> u64 {
        z.rem_u64(self.0.p)
    }
    /// The residue of a rational; `None` when p divides the denominator.
    pub fn of_rat(&self, q: &Rat) -> Option<u64> {
        let d = Z::from_nat(q.den.clone()).rem_u64(self.0.p);
        if d == 0 {
            return None;
        }
        Some(self.0.mul(Z::from_int(q.num.clone()).rem_u64(self.0.p), self.0.inv(d)))
    }
}

impl CoeffRing for Zp {
    type E = u64;
    fn zero(&self) -> u64 {
        0
    }
    fn one(&self) -> u64 {
        1 % self.0.p
    }
    fn is_zero(&self, a: &u64) -> bool {
        *a == 0
    }
    fn of_i64(&self, n: i64) -> u64 {
        self.0.of_i64(n)
    }
    fn add(&self, a: &u64, b: &u64) -> u64 {
        self.0.add(*a, *b)
    }
    fn sub(&self, a: &u64, b: &u64) -> u64 {
        self.0.sub(*a, *b)
    }
    fn neg(&self, a: &u64) -> u64 {
        self.0.neg(*a)
    }
    fn mul(&self, a: &u64, b: &u64, _m: &mut Meter) -> WorkResult<u64> {
        Ok(self.0.mul(*a, *b))
    }
    fn div_exact(&self, a: &u64, b: &u64, _m: &mut Meter) -> WorkResult<Option<u64>> {
        if *b == 0 {
            return Ok(None);
        }
        Ok(Some(self.0.mul(*a, self.0.inv(*b))))
    }
    fn is_field(&self) -> bool {
        true
    }
    fn characteristic(&self) -> u64 {
        self.0.p
    }
}

/// Polynomials over the integers in `nv` variables ([`Poly`]), an integral domain with exact division.
#[derive(Clone, Copy, Debug)]
pub struct PolyRing {
    /// the number of variables
    pub nv: usize,
}

impl CoeffRing for PolyRing {
    type E = Poly;
    fn zero(&self) -> Poly {
        Poly::zero(self.nv)
    }
    fn one(&self) -> Poly {
        Poly::constant(self.nv, Z::ONE)
    }
    fn is_zero(&self, a: &Poly) -> bool {
        a.is_zero()
    }
    fn is_one(&self, a: &Poly) -> bool {
        a.as_constant() == Some(Z::ONE)
    }
    fn of_i64(&self, n: i64) -> Poly {
        Poly::constant(self.nv, Z::S(n))
    }
    fn add(&self, a: &Poly, b: &Poly) -> Poly {
        a.add(b)
    }
    fn sub(&self, a: &Poly, b: &Poly) -> Poly {
        a.sub(b)
    }
    fn neg(&self, a: &Poly) -> Poly {
        a.neg()
    }
    fn mul(&self, a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
        a.mul(b, m)
    }
    fn div_exact(&self, a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Option<Poly>> {
        if b.is_zero() {
            return Ok(None);
        }
        a.div_exact(b, m)
    }
    fn is_field(&self) -> bool {
        false
    }
    fn gcd(&self, a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
        poly_gcd(a, b, m)
    }
    fn size(&self, a: &Poly) -> u64 {
        a.len() as u64 * (1 + a.max_bits() / 64)
    }
}

/// Rational functions over the rationals in `nv` variables ([`RatFun`], in lowest terms), a field.
#[derive(Clone, Copy, Debug)]
pub struct RatFunField {
    /// the number of variables
    pub nv: usize,
}

impl RatFunField {
    /// The element `num / den` (den nonzero) in lowest terms.
    pub fn quotient(&self, num: &Poly, den: &Poly, m: &mut Meter) -> WorkResult<RatFun> {
        RatFun::reduced(num.clone(), den.clone(), m)
    }
    /// The element of a polynomial.
    pub fn of_poly(&self, p: &Poly) -> RatFun {
        RatFun { num: p.clone(), den: Poly::constant(self.nv, Z::ONE) }
    }
    /// The element of a rational number.
    pub fn of_rat(&self, q: &Rat) -> RatFun {
        let (n, d) = crate::qpoly::rat_parts(q);
        RatFun { num: Poly::constant(self.nv, n), den: Poly::constant(self.nv, d) }
    }
}

impl CoeffRing for RatFunField {
    type E = RatFun;
    fn zero(&self) -> RatFun {
        RatFun { num: Poly::zero(self.nv), den: Poly::constant(self.nv, Z::ONE) }
    }
    fn one(&self) -> RatFun {
        self.of_i64(1)
    }
    fn is_zero(&self, a: &RatFun) -> bool {
        a.num.is_zero()
    }
    fn of_i64(&self, n: i64) -> RatFun {
        RatFun { num: Poly::constant(self.nv, Z::S(n)), den: Poly::constant(self.nv, Z::ONE) }
    }
    fn add(&self, a: &RatFun, b: &RatFun) -> RatFun {
        // the gcds of RatFun::add take a meter: one that stops with mc-arith's stop flag; a stopped sum is any
        // value (zero), and the caller's next metered step reports the stop
        let mut w = ArithStop;
        let mut m = Meter::new(&mut w);
        a.add(b, &mut m).unwrap_or_else(|_| self.zero())
    }
    fn sub(&self, a: &RatFun, b: &RatFun) -> RatFun {
        self.add(a, &self.neg(b))
    }
    fn neg(&self, a: &RatFun) -> RatFun {
        RatFun { num: a.num.neg(), den: a.den.clone() }
    }
    fn mul(&self, a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<RatFun> {
        a.mul(b, m)
    }
    fn div_exact(&self, a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<Option<RatFun>> {
        match b.inv(1, m)? {
            Ok(i) => Ok(Some(a.mul(&i, m)?)),
            Err(_) => Ok(None),
        }
    }
    fn is_field(&self) -> bool {
        true
    }
    fn size(&self, a: &RatFun) -> u64 {
        (a.num.len() + a.den.len()) as u64
    }
}

/// A budget that stops with `mc-arith`'s stop flag (for the infallible ring operations).
struct ArithStop;

impl crate::work::Work for ArithStop {
    fn charge(&mut self, _units: u64) -> WorkResult<()> {
        if mc_arith::num::stopped() {
            Err(crate::work::Halt::Stopped)
        } else {
            Ok(())
        }
    }
}
