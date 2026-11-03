//! Signed integers as sign and magnitude over `Nat` (zero is never negative). Only what the constants and the
//! tests need.
use crate::nat::Nat;
use std::cmp::Ordering;

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Int {
    pub neg: bool,
    pub mag: Nat,
}

impl Int {
    pub fn new(neg: bool, mag: Nat) -> Int {
        let neg = neg && !mag.is_zero();
        Int { neg, mag }
    }
    pub fn from_nat(mag: Nat) -> Int {
        Int { neg: false, mag }
    }
    pub fn from_i64(v: i64) -> Int {
        Int::new(v < 0, Nat::from_u64(v.unsigned_abs()))
    }
    pub fn from_u64(v: u64) -> Int {
        Int::from_nat(Nat::from_u64(v))
    }
    pub fn is_zero(&self) -> bool {
        self.mag.is_zero()
    }
    pub fn neg(&self) -> Int {
        Int::new(!self.neg, self.mag.clone())
    }
    pub fn add(&self, o: &Int) -> Int {
        if self.neg == o.neg {
            return Int::new(self.neg, self.mag.add(&o.mag));
        }
        match self.mag.cmp(&o.mag) {
            Ordering::Less => Int::new(o.neg, o.mag.sub(&self.mag)),
            _ => Int::new(self.neg, self.mag.sub(&o.mag)),
        }
    }
    pub fn sub(&self, o: &Int) -> Int {
        self.add(&o.neg())
    }
    pub fn mul(&self, o: &Int) -> Int {
        Int::new(self.neg != o.neg, self.mag.mul(&o.mag))
    }
    pub fn shl(&self, n: u64) -> Int {
        Int::new(self.neg, self.mag.shl(n))
    }
    pub fn cmp_int(&self, o: &Int) -> Ordering {
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.mag.cmp(&o.mag),
            (true, true) => o.mag.cmp(&self.mag),
        }
    }
}
