//! Signed integers as sign and magnitude over `Nat` (zero is never negative).
use super::nat::Nat;
use std::cmp::Ordering;

#[derive(Clone, PartialEq, Eq, Debug, Default)]
/// A signed integer: sign and magnitude, zero never negative.
pub struct Int {
    /// The sign (false for zero).
    pub neg: bool,
    /// The magnitude.
    pub mag: Nat,
}

impl Int {
    /// (-1)^neg mag, with zero normalised to positive.
    pub fn new(neg: bool, mag: Nat) -> Int {
        let neg = neg && !mag.is_zero();
        Int { neg, mag }
    }
    /// A natural number.
    pub fn from_nat(mag: Nat) -> Int {
        Int { neg: false, mag }
    }
    /// From a machine integer.
    pub fn from_i64(v: i64) -> Int {
        Int::new(v < 0, Nat::from_u64(v.unsigned_abs()))
    }
    /// From an unsigned machine integer.
    pub fn from_u64(v: u64) -> Int {
        Int::from_nat(Nat::from_u64(v))
    }
    /// Is the value zero?
    pub fn is_zero(&self) -> bool {
        self.mag.is_zero()
    }
    /// -self.
    pub fn neg(&self) -> Int {
        Int::new(!self.neg, self.mag.clone())
    }
    /// self + o.
    pub fn add(&self, o: &Int) -> Int {
        if self.neg == o.neg {
            return Int::new(self.neg, self.mag.add(&o.mag));
        }
        match self.mag.cmp(&o.mag) {
            Ordering::Less => Int::new(o.neg, o.mag.sub(&self.mag)),
            _ => Int::new(self.neg, self.mag.sub(&o.mag)),
        }
    }
    /// self - o.
    pub fn sub(&self, o: &Int) -> Int {
        self.add(&o.neg())
    }
    /// self o.
    pub fn mul(&self, o: &Int) -> Int {
        Int::new(self.neg != o.neg, self.mag.mul(&o.mag))
    }
    /// self 2^n.
    pub fn shl(&self, n: u64) -> Int {
        Int::new(self.neg, self.mag.shl(n))
    }
    /// Value comparison.
    pub fn cmp_int(&self, o: &Int) -> Ordering {
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.mag.cmp(&o.mag),
            (true, true) => o.mag.cmp(&self.mag),
        }
    }
}

impl Int {
    /// The value as an `i64`, if it fits.
    pub fn to_i64(&self) -> Option<i64> {
        let m = match self.mag.limbs.as_slice() {
            [] => 0,
            [m] => *m,
            _ => return None,
        };
        if self.neg {
            (m <= 1u64 << 63).then(|| (m as i64).wrapping_neg())
        } else {
            i64::try_from(m).ok()
        }
    }
    /// floor(self / d) and the remainder self - d floor(self / d) (which has the sign of d), for d > 0.
    pub fn divrem_floor(&self, d: &Nat) -> (Int, Nat) {
        let (q, r) = self.mag.divrem(d);
        if !self.neg || r.is_zero() {
            return (Int::new(self.neg, q), r);
        }
        // -m = -(q d + r) = -(q + 1) d + (d - r)
        (Int::new(true, q.add_u64(1)), d.sub(&r))
    }
}
