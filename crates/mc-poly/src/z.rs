//! Integer coefficients with a machine-word fast path.
//!
//! Most polynomial coefficients in practice fit in 64 bits; [`Z`] keeps those as an `i64` and switches to
//! `mc-arith`'s [`Int`] only when a value does not fit, so that the common case allocates nothing. The invariant
//! (`Z::B` only for values outside `i64`) makes the representation unique, so derived equality is value equality.
use mc_arith::num::{gcd, Int, Nat};
use std::cmp::Ordering;

/// An integer: `S` when it fits in `i64`, otherwise `B` (never holding an `i64`-sized value).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Z {
    /// a machine-word value
    S(i64),
    /// a value outside the `i64` range
    B(Int),
}

impl Default for Z {
    fn default() -> Z {
        Z::S(0)
    }
}

/// `v` as an [`Int`].
pub fn int_of_i128(v: i128) -> Int {
    Int::new(v < 0, Nat::from_u128(v.unsigned_abs()))
}

impl Z {
    /// Zero.
    pub const ZERO: Z = Z::S(0);
    /// One.
    pub const ONE: Z = Z::S(1);

    /// From an [`Int`], normalised.
    pub fn from_int(i: Int) -> Z {
        match i.to_i64() {
            Some(v) => Z::S(v),
            None => Z::B(i),
        }
    }
    /// From a 128-bit value.
    pub fn from_i128(v: i128) -> Z {
        match i64::try_from(v) {
            Ok(s) => Z::S(s),
            Err(_) => Z::B(int_of_i128(v)),
        }
    }
    /// From a natural number.
    pub fn from_nat(n: Nat) -> Z {
        Z::from_int(Int::from_nat(n))
    }
    /// The value as an [`Int`].
    pub fn to_int(&self) -> Int {
        match self {
            Z::S(v) => Int::from_i64(*v),
            Z::B(i) => i.clone(),
        }
    }
    /// The value as an `i64`, if it fits.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Z::S(v) => Some(*v),
            Z::B(_) => None,
        }
    }
    /// Is the value zero?
    pub fn is_zero(&self) -> bool {
        matches!(self, Z::S(0))
    }
    /// Is the value one?
    pub fn is_one(&self) -> bool {
        matches!(self, Z::S(1))
    }
    /// Is the value negative?
    pub fn is_neg(&self) -> bool {
        match self {
            Z::S(v) => *v < 0,
            Z::B(i) => i.neg,
        }
    }
    /// The sign: -1, 0 or 1.
    pub fn signum(&self) -> i32 {
        match self {
            Z::S(v) => v.signum() as i32,
            Z::B(i) => {
                if i.neg {
                    -1
                } else {
                    1
                }
            }
        }
    }
    /// The magnitude.
    pub fn mag(&self) -> Nat {
        match self {
            Z::S(v) => Nat::from_u64(v.unsigned_abs()),
            Z::B(i) => i.mag.clone(),
        }
    }
    /// The number of bits of the magnitude.
    pub fn bits(&self) -> u64 {
        match self {
            Z::S(v) => 64 - v.unsigned_abs().leading_zeros() as u64,
            Z::B(i) => i.mag.bits(),
        }
    }
    /// -self.
    pub fn neg(&self) -> Z {
        match self {
            Z::S(v) => match v.checked_neg() {
                Some(n) => Z::S(n),
                None => Z::from_i128(-(*v as i128)),
            },
            Z::B(i) => Z::from_int(i.neg()),
        }
    }
    /// |self|.
    pub fn abs(&self) -> Z {
        if self.is_neg() {
            self.neg()
        } else {
            self.clone()
        }
    }
    /// self + o.
    pub fn add(&self, o: &Z) -> Z {
        match (self, o) {
            (Z::S(a), Z::S(b)) => match a.checked_add(*b) {
                Some(c) => Z::S(c),
                None => Z::from_i128(*a as i128 + *b as i128),
            },
            _ => Z::from_int(self.to_int().add(&o.to_int())),
        }
    }
    /// self - o.
    pub fn sub(&self, o: &Z) -> Z {
        match (self, o) {
            (Z::S(a), Z::S(b)) => match a.checked_sub(*b) {
                Some(c) => Z::S(c),
                None => Z::from_i128(*a as i128 - *b as i128),
            },
            _ => Z::from_int(self.to_int().sub(&o.to_int())),
        }
    }
    /// self * o.
    pub fn mul(&self, o: &Z) -> Z {
        match (self, o) {
            (Z::S(a), Z::S(b)) => Z::from_i128(*a as i128 * *b as i128),
            (Z::S(0), _) | (_, Z::S(0)) => Z::ZERO,
            _ => Z::from_int(self.to_int().mul(&o.to_int())),
        }
    }
    /// self * v.
    pub fn mul_i64(&self, v: i64) -> Z {
        self.mul(&Z::S(v))
    }
    /// The truncated quotient and remainder (remainder with the sign of self); `d` nonzero.
    pub fn divrem(&self, d: &Z) -> (Z, Z) {
        match (self, d) {
            (Z::S(a), Z::S(b)) if !(*a == i64::MIN && *b == -1) => (Z::S(a / b), Z::S(a % b)),
            _ => {
                let (a, b) = (self.to_int(), d.to_int());
                let (q, r) = a.mag.divrem(&b.mag);
                (Z::from_int(Int::new(a.neg != b.neg, q)), Z::from_int(Int::new(a.neg, r)))
            }
        }
    }
    /// self / d for a divisor known to divide self exactly.
    pub fn div_exact(&self, d: &Z) -> Z {
        match (self, d) {
            (Z::S(a), Z::S(b)) if !(*a == i64::MIN && *b == -1) => Z::S(a / b),
            _ => {
                let (a, b) = (self.to_int(), d.to_int());
                Z::from_int(Int::new(a.neg != b.neg, gcd::div_exact(&a.mag, &b.mag)))
            }
        }
    }
    /// Does d divide self (d nonzero)?
    pub fn divisible(&self, d: &Z) -> bool {
        self.divrem(d).1.is_zero()
    }
    /// The nonnegative gcd.
    pub fn gcd(&self, o: &Z) -> Z {
        match (self, o) {
            (Z::S(a), Z::S(b)) => Z::from_i128(gcd::gcd_u64(a.unsigned_abs(), b.unsigned_abs()) as i128),
            _ => Z::from_nat(gcd::gcd(&self.mag(), &o.mag())),
        }
    }
    /// The residue in [0, p).
    pub fn rem_u64(&self, p: u64) -> u64 {
        match self {
            Z::S(v) => (*v as i128).rem_euclid(p as i128) as u64,
            Z::B(i) => {
                let r = i.mag.divrem_u64(p).1;
                if i.neg && r != 0 {
                    p - r
                } else {
                    r
                }
            }
        }
    }
    /// The value of the residue `r` mod `p` in the symmetric range (-p/2, p/2].
    pub fn from_residue_sym(r: u64, p: u64) -> Z {
        if r > p / 2 {
            Z::S(-((p - r) as i64))
        } else {
            Z::S(r as i64)
        }
    }
    /// Comparison by value.
    pub fn cmp_z(&self, o: &Z) -> Ordering {
        match (self, o) {
            (Z::S(a), Z::S(b)) => a.cmp(b),
            _ => self.to_int().cmp_int(&o.to_int()),
        }
    }
    /// Comparison of magnitudes.
    pub fn cmp_abs(&self, o: &Z) -> Ordering {
        match (self, o) {
            (Z::S(a), Z::S(b)) => a.unsigned_abs().cmp(&b.unsigned_abs()),
            _ => self.mag().cmp(&o.mag()),
        }
    }
    /// self^e.
    pub fn pow(&self, e: u64) -> Z {
        let mut r = Z::ONE;
        let mut b = self.clone();
        let mut e = e;
        while e > 0 {
            if e & 1 == 1 {
                r = r.mul(&b);
            }
            e >>= 1;
            if e > 0 {
                b = b.mul(&b);
            }
        }
        r
    }
}

impl std::fmt::Display for Z {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Z::S(v) => write!(f, "{v}"),
            Z::B(i) => write!(f, "{}{}", if i.neg { "-" } else { "" }, i.mag.to_decimal()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_boundaries() {
        let m = Z::S(i64::MAX);
        let s = m.add(&Z::ONE);
        assert!(matches!(s, Z::B(_)));
        assert_eq!(s.sub(&Z::ONE), m);
        let n = Z::S(i64::MIN);
        assert_eq!(n.neg().neg(), n);
        assert_eq!(n.divrem(&Z::S(-1)).0, n.neg());
        let p = m.mul(&m);
        assert_eq!(p.div_exact(&m), m);
        assert_eq!(Z::S(-7).rem_u64(5), 3);
        assert_eq!(p.neg().rem_u64(1_000_003), (1_000_003 - p.rem_u64(1_000_003)) % 1_000_003);
        assert_eq!(Z::S(12).gcd(&Z::S(-18)), Z::S(6));
        assert_eq!(Z::S(3).pow(5), Z::S(243));
        assert_eq!(Z::S(-7).divrem(&Z::S(2)), (Z::S(-3), Z::S(-1)));
    }
}
