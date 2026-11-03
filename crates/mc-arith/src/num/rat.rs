//! Canonical rationals outside the store (semantics 1 and 2.4): reduced, positive denominator; a denominator of one
//! means the value is an integer. Sum and product follow Knuth 4.5.1 (Henrici): the gcds are taken of the
//! denominators and of the cofactors, which are much smaller than a gcd of the unreduced result, and a sum with an
//! integer or a small denominator needs only gcds with small numbers.
use super::gcd::{div_exact, gcd, is_one};
use super::int::Int;
use super::nat::Nat;

#[derive(Clone, PartialEq, Eq, Debug)]
/// A canonical rational: reduced, positive denominator (one for integers).
pub struct Rat {
    /// The numerator, carrying the sign.
    pub num: Int,
    /// The denominator, positive.
    pub den: Nat,
}

impl Rat {
    /// An integer.
    pub fn from_int(num: Int) -> Rat {
        Rat { num, den: Nat::from_u64(1) }
    }
    /// From a machine integer.
    pub fn from_i64(v: i64) -> Rat {
        Rat::from_int(Int::from_i64(v))
    }
    /// num / den reduced (den != 0; zero for den = 0 after a stop).
    pub fn new(num: Int, den: Int) -> Rat {
        if den.is_zero() {
            super::after_stop("division by zero");
            return Rat::from_i64(0);
        }
        let g = gcd(&num.mag, &den.mag);
        let neg = num.neg != den.neg;
        Rat { num: Int::new(neg, div_exact(&num.mag, &g)), den: div_exact(&den.mag, &g) }
    }
    /// Is the value an integer (denominator one)?
    pub fn is_int(&self) -> bool {
        is_one(&self.den)
    }
    /// -self.
    pub fn neg(&self) -> Rat {
        Rat { num: self.num.neg(), den: self.den.clone() }
    }
    /// self + o, by Knuth/Henrici (gcds of denominators and cofactors only).
    pub fn add(&self, o: &Rat) -> Rat {
        if self.is_int() && o.is_int() {
            return Rat::from_int(self.num.add(&o.num));
        }
        let (a, b, c, d) = (&self.num, &self.den, &o.num, &o.den);
        let g1 = gcd(b, d);
        if is_one(&g1) {
            let num = a.mul(&Int::from_nat(d.clone())).add(&c.mul(&Int::from_nat(b.clone())));
            return Rat { num, den: b.mul(d) };
        }
        let (b1, d1) = (div_exact(b, &g1), div_exact(d, &g1));
        let t = a.mul(&Int::from_nat(d1)).add(&c.mul(&Int::from_nat(b1.clone())));
        if t.is_zero() {
            return Rat::from_i64(0);
        }
        let g2 = gcd(&t.mag, &g1);
        let num = Int::new(t.neg, div_exact(&t.mag, &g2));
        Rat { num, den: b1.mul(&div_exact(d, &g2)) }
    }
    /// self - o.
    pub fn sub(&self, o: &Rat) -> Rat {
        self.add(&o.neg())
    }
    /// self o, by Knuth/Henrici (cross gcds only).
    pub fn mul(&self, o: &Rat) -> Rat {
        if self.is_int() && o.is_int() {
            return Rat::from_int(self.num.mul(&o.num));
        }
        if self.num.is_zero() || o.num.is_zero() {
            return Rat::from_i64(0);
        }
        let g1 = gcd(&self.num.mag, &o.den);
        let g2 = gcd(&o.num.mag, &self.den);
        let num = div_exact(&self.num.mag, &g1).mul(&div_exact(&o.num.mag, &g2));
        let den = div_exact(&self.den, &g2).mul(&div_exact(&o.den, &g1));
        Rat { num: Int::new(self.num.neg != o.num.neg, num), den }
    }
    /// Is the value zero?
    pub fn is_zero(&self) -> bool {
        self.num.is_zero()
    }
    /// 1 / self, or `None` for zero.
    pub fn recip(&self) -> Option<Rat> {
        if self.is_zero() {
            return None;
        }
        Some(Rat { num: Int::new(self.num.neg, self.den.clone()), den: self.num.mag.clone() })
    }
    /// self^e for e >= 0 (powers of a reduced fraction stay reduced, so no gcd is needed).
    pub fn pow_u64(&self, e: u64) -> Rat {
        let neg = self.num.neg && e % 2 == 1;
        Rat { num: Int::new(neg, self.num.mag.pow_u64(e)), den: self.den.pow_u64(e) }
    }
    /// floor(self) and the fractional part self - floor(self), in [0, 1).
    pub fn floor_fract(&self) -> (Int, Rat) {
        let (q, r) = self.num.divrem_floor(&self.den);
        (q, Rat { num: Int::from_nat(r), den: self.den.clone() }.reduced())
    }
    /// The same value with numerator and denominator reduced (for fractions built from parts).
    fn reduced(self) -> Rat {
        let neg = self.num.neg;
        Rat::new(Int::new(neg, self.num.mag), Int::from_nat(self.den))
    }
    /// Value comparison.
    pub fn cmp_value(&self, o: &Rat) -> std::cmp::Ordering {
        let a = self.num.mul(&Int::from_nat(o.den.clone()));
        let b = o.num.mul(&Int::from_nat(self.den.clone()));
        a.cmp_int(&b)
    }
    /// Is the representation canonical (reduced, positive denominator)?
    pub fn is_canonical(&self) -> bool {
        !self.den.is_zero() && (if self.num.is_zero() { self.is_int() } else { is_one(&gcd(&self.num.mag, &self.den)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(n: i64, d: i64) -> Rat {
        Rat::new(Int::from_i64(n), Int::from_i64(d))
    }

    #[test]
    fn canonical_arithmetic() {
        assert_eq!(r(2, 4), r(1, 2));
        assert_eq!(r(3, -6), r(-1, 2));
        assert!(r(4, 2).is_int());
        assert_eq!(r(1, 6).add(&r(1, 3)), r(1, 2));
        assert_eq!(r(1, 2).add(&r(-1, 2)), Rat::from_i64(0));
        assert_eq!(r(2, 3).mul(&r(9, 4)), r(3, 2));
        assert_eq!(r(5, 7).sub(&r(5, 7)), Rat::from_i64(0));
        // H(10) = 7381/2520
        let mut h = Rat::from_i64(0);
        for k in 1..=10 {
            h = h.add(&r(1, k));
            assert!(h.is_canonical());
        }
        assert_eq!(h, r(7381, 2520));
    }
}

#[cfg(test)]
mod harmonic_tests {
    use super::*;

    fn euclid(a: &Nat, b: &Nat) -> Nat {
        let (mut a, mut b) = (a.clone(), b.clone());
        while !b.is_zero() {
            let r = a.divrem(&b).1;
            a = b;
            b = r;
        }
        a
    }

    /// Binary splitting of H(n): every gcd taken by the sums is checked against Euclid, every node is canonical.
    /// (This structured input found a Lehmer cofactor-sign bug that random operands missed.)
    #[test]
    fn harmonic_split_is_canonical() {
        let mut level: Vec<Rat> = (1..=4000i64).map(|k| Rat::new(Int::from_i64(1), Int::from_i64(k))).collect();
        while level.len() > 1 {
            level = level
                .chunks(2)
                .map(|c| {
                    if c.len() == 1 {
                        return c[0].clone();
                    }
                    assert_eq!(gcd(&c[0].den, &c[1].den), euclid(&c[0].den, &c[1].den));
                    let s = c[0].add(&c[1]);
                    assert!(s.is_canonical());
                    s
                })
                .collect();
        }
    }
}
