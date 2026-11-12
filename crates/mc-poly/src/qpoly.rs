//! Polynomials with rational coefficients: an integer polynomial over one positive common denominator, kept
//! reduced (the denominator is coprime to the numerator's content), so the representation is unique.
use crate::poly::Poly;
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use mc_arith::num::{Int, Rat};

/// `num / den` with `den > 0` and `gcd(content(num), den) = 1` (`den = 1` for the zero polynomial).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct QPoly {
    /// the numerator
    pub num: Poly,
    /// the positive denominator
    pub den: Z,
}

/// A rational number as (numerator, positive denominator).
pub fn rat_parts(q: &Rat) -> (Z, Z) {
    (Z::from_int(q.num.clone()), Z::from_nat(q.den.clone()))
}

/// The rational `n / d` (d > 0, any common factor allowed).
pub fn rat_of(n: &Z, d: &Z) -> Rat {
    Rat::new(n.to_int(), d.to_int())
}

impl QPoly {
    /// An integer polynomial.
    pub fn from_poly(num: Poly) -> QPoly {
        QPoly { num, den: Z::ONE }
    }
    /// The zero polynomial.
    pub fn zero(nv: usize) -> QPoly {
        QPoly::from_poly(Poly::zero(nv))
    }
    /// The constant `q`.
    pub fn constant(nv: usize, q: &Rat) -> QPoly {
        let (n, d) = rat_parts(q);
        QPoly { num: Poly::constant(nv, n), den: d }
    }
    /// `num / den` reduced (den nonzero, any sign).
    pub fn new(num: Poly, den: Z) -> QPoly {
        let (num, den) = if den.is_neg() { (num.neg(), den.neg()) } else { (num, den) };
        if num.is_zero() {
            return QPoly::zero(num.nv);
        }
        if den.is_one() {
            return QPoly { num, den };
        }
        let g = num.content().gcd(&den);
        if g.is_one() {
            QPoly { num, den }
        } else {
            QPoly { num: num.div_exact_scalar(&g), den: den.div_exact(&g) }
        }
    }
    /// The number of variables.
    pub fn nvars(&self) -> usize {
        self.num.nv
    }
    /// Is this zero?
    pub fn is_zero(&self) -> bool {
        self.num.is_zero()
    }
    /// The coefficient of term `i` of the numerator, as a rational.
    pub fn coef(&self, i: usize) -> Rat {
        rat_of(self.num.coef(i), &self.den)
    }
    /// The constant value, if this is a constant.
    pub fn as_constant(&self) -> Option<Rat> {
        self.num.as_constant().map(|c| rat_of(&c, &self.den))
    }
    /// -self.
    pub fn neg(&self) -> QPoly {
        QPoly { num: self.num.neg(), den: self.den.clone() }
    }
    /// self + o.
    pub fn add(&self, o: &QPoly) -> QPoly {
        if self.den == o.den {
            return QPoly::new(self.num.add(&o.num), self.den.clone());
        }
        let g = self.den.gcd(&o.den);
        let (sa, sb) = (o.den.div_exact(&g), self.den.div_exact(&g));
        QPoly::new(self.num.scale(&sa).add(&o.num.scale(&sb)), self.den.mul(&sa))
    }
    /// self - o.
    pub fn sub(&self, o: &QPoly) -> QPoly {
        self.add(&o.neg())
    }
    /// self * o.
    pub fn mul(&self, o: &QPoly, m: &mut Meter) -> WorkResult<QPoly> {
        Ok(QPoly::new(self.num.mul(&o.num, m)?, self.den.mul(&o.den)))
    }
    /// self^e.
    pub fn pow(&self, e: u64, m: &mut Meter) -> WorkResult<QPoly> {
        // the denominator's power is already coprime to the numerator's content
        Ok(QPoly { num: self.num.pow(e, m)?, den: self.den.pow(e) })
    }
    /// q * self.
    pub fn scale(&self, q: &Rat) -> QPoly {
        let (n, d) = rat_parts(q);
        QPoly::new(self.num.scale(&n), self.den.mul(&d))
    }
    /// The primitive integer polynomial with the same roots (positive leading coefficient) and the rational
    /// factor: self = factor * primitive.
    pub fn primitive(&self) -> (Rat, Poly) {
        let (c, pp) = self.num.primitive();
        (rat_of(&c, &self.den), pp)
    }
}

/// The rational `1`.
pub fn rat_one() -> Rat {
    Rat::from_int(Int::from_i64(1))
}
