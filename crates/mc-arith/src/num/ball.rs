//! Ball arithmetic (semantics 2.3, 2.5): a midpoint that is a binary floating-point number of p bits and a radius
//! that is a small upper-bound float. Every operation returns a ball containing every exact result for inputs in
//! the argument balls (enclosure); comparisons of overlapping balls are undecided.
//!
//! The radius `Mag` has a 32-bit mantissa and a 64-bit exponent and is always rounded up (down where a lower bound
//! is needed), as in Arb's `mag_t`. A 32-bit mantissa keeps a product of two mantissas exact in a u64 before its one
//! rounding, and a full 64-bit exponent is required because radii of 10^6-digit balls are about 2^(-3.3 10^6),
//! far outside the range of an f64.
use super::nat::{isqrt_u128, Nat};
use std::cmp::Ordering;

// ---------------------------------------------------------------- radius

/// m 2^e with m = 0 or 2^31 <= m < 2^32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mag {
    /// The mantissa: zero, or in [2^31, 2^32).
    pub m: u64,
    /// The binary exponent.
    pub e: i64,
}

const MAG_BITS: u32 = 32;

impl Mag {
    /// The zero radius.
    pub const ZERO: Mag = Mag { m: 0, e: 0 };

    /// Is the radius zero?
    pub fn is_zero(&self) -> bool {
        self.m == 0
    }
    /// v 2^e rounded up to a 32-bit mantissa.
    fn up(v: u128, e: i64) -> Mag {
        Mag::norm(v, e, true)
    }
    fn down(v: u128, e: i64) -> Mag {
        Mag::norm(v, e, false)
    }
    fn norm(v: u128, e: i64, round_up: bool) -> Mag {
        if v == 0 {
            return Mag::ZERO;
        }
        let b = 128 - v.leading_zeros();
        if b <= MAG_BITS {
            let s = MAG_BITS - b;
            return Mag { m: (v << s) as u64, e: e - s as i64 };
        }
        let s = b - MAG_BITS;
        let mut m = (v >> s) as u64;
        let mut e = e + s as i64;
        if round_up && v & ((1u128 << s) - 1) != 0 {
            m += 1;
            if m == 1 << MAG_BITS {
                m = 1 << (MAG_BITS - 1);
                e += 1;
            }
        }
        Mag { m, e }
    }
    /// 2^e exactly.
    pub fn pow2(e: i64) -> Mag {
        Mag { m: 1 << (MAG_BITS - 1), e: e - (MAG_BITS as i64 - 1) }
    }
    /// An upper bound of self + o.
    pub fn add(&self, o: &Mag) -> Mag {
        if self.is_zero() {
            return *o;
        }
        if o.is_zero() {
            return *self;
        }
        let (a, b) = if self.e >= o.e { (self, o) } else { (o, self) };
        let d = a.e - b.e;
        if d >= 64 {
            // b < 2^(b.e + 32) <= 2^(a.e - 32): less than one unit of a's mantissa, so one unit bounds it
            return Mag::up(a.m as u128 + 1, a.e);
        }
        Mag::up(((a.m as u128) << d) + b.m as u128, b.e)
    }
    /// An upper bound of self o.
    pub fn mul(&self, o: &Mag) -> Mag {
        Mag::up(self.m as u128 * o.m as u128, self.e + o.e)
    }
    /// A lower bound of self o.
    pub fn mul_down(&self, o: &Mag) -> Mag {
        Mag::down(self.m as u128 * o.m as u128, self.e + o.e)
    }
    /// self 2^k exactly.
    pub fn mul_2exp(&self, k: i64) -> Mag {
        if self.is_zero() {
            return *self;
        }
        Mag { m: self.m, e: self.e + k }
    }
    /// Upper bound of self / o for o > 0 (o a lower bound of the true divisor).
    pub fn div_up(&self, o: &Mag) -> Mag {
        if o.is_zero() {
            super::after_stop("Mag::div_up by zero");
            return Mag::ZERO;
        }
        if self.is_zero() {
            return Mag::ZERO;
        }
        let num = (self.m as u128) << 64;
        let d = o.m as u128;
        let q = num / d + (!num.is_multiple_of(d)) as u128;
        Mag::up(q, self.e - 64 - o.e)
    }
    /// Lower bound of sqrt(self).
    pub fn sqrt_down(&self) -> Mag {
        if self.is_zero() {
            return Mag::ZERO;
        }
        let odd = self.e.rem_euclid(2) as u32;
        let big = (self.m as u128) << (64 + odd);
        let e = self.e - 64 - odd as i64;
        Mag::down(isqrt_u128(big), e / 2)
    }
    /// Upper bound of |x|.
    pub fn from_float_up(x: &Float) -> Mag {
        Mag::from_float(x, true)
    }
    /// Lower bound of |x|.
    pub fn from_float_down(x: &Float) -> Mag {
        Mag::from_float(x, false)
    }
    /// From the top 64 bits of the mantissa, read in place (no shifted copy) with a sticky bit for the rest.
    fn from_float(x: &Float, round_up: bool) -> Mag {
        let b = x.man.bits();
        if b == 0 {
            return Mag::ZERO;
        }
        let l = &x.man.limbs;
        if b <= 64 {
            return Mag::norm(l[0] as u128, x.exp, round_up);
        }
        let s = b - 64;
        let (w, bit) = ((s / 64) as usize, (s % 64) as u32);
        let top = if bit == 0 { l[w] } else { (l[w] >> bit) | (l[w + 1] << (64 - bit)) };
        let sticky = round_up && (l[..w].iter().any(|&v| v != 0) || (bit > 0 && l[w] & ((1u64 << bit) - 1) != 0));
        Mag::norm(top as u128 + sticky as u128, x.exp + s as i64, round_up)
    }
    /// Lower bound of self - o, or zero when that is not provably positive at this precision.
    pub fn sub_down(&self, o: &Mag) -> Mag {
        if o.is_zero() {
            return *self;
        }
        if self.is_zero() {
            return Mag::ZERO;
        }
        if self.e >= o.e {
            let d = self.e - o.e;
            if d >= 96 {
                // o < 2^(o.e + 32) <= 2^(self.e - 64): one unit of self's mantissa extended by 64 bits
                return Mag::down(((self.m as u128) << 64) - 1, self.e - 64);
            }
            let sm = (self.m as u128) << d;
            return if sm > o.m as u128 { Mag::down(sm - o.m as u128, o.e) } else { Mag::ZERO };
        }
        let d = self.e.abs_diff(o.e);
        if d >= 32 {
            return Mag::ZERO; // o >= 2^(o.e + 31) > 2^(self.e + 32) > self
        }
        let om = (o.m as u128) << d;
        if self.m as u128 > om {
            Mag::down(self.m as u128 - om, self.e)
        } else {
            Mag::ZERO
        }
    }
    /// The exact value as a midpoint-type number.
    pub fn to_float(&self) -> Float {
        Float { neg: false, man: Nat::from_u64(self.m), exp: if self.m == 0 { 0 } else { self.e } }
    }
    /// log2 of the radius, approximately (for reports).
    pub fn log2(&self) -> f64 {
        if self.is_zero() {
            f64::NEG_INFINITY
        } else {
            (self.m as f64).log2() + self.e as f64
        }
    }
}

// ---------------------------------------------------------------- midpoint

/// (-1)^neg man 2^exp; zero has man = 0, neg = false.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Float {
    /// The sign (false for zero).
    pub neg: bool,
    /// The mantissa.
    pub man: Nat,
    /// The binary exponent.
    pub exp: i64,
}

impl Float {
    /// Zero.
    pub fn zero() -> Float {
        Float { neg: false, man: Nat::zero(), exp: 0 }
    }
    /// (-1)^neg man 2^exp, with zero normalised to positive.
    pub fn new(neg: bool, man: Nat, exp: i64) -> Float {
        if man.is_zero() {
            return Float::zero();
        }
        Float { neg, man, exp }
    }
    /// An integer, exactly.
    pub fn from_i64(v: i64) -> Float {
        Float::new(v < 0, Nat::from_u64(v.unsigned_abs()), 0)
    }
    /// A natural number, exactly.
    pub fn from_nat(n: Nat) -> Float {
        Float::new(false, n, 0)
    }
    /// Is the value zero?
    pub fn is_zero(&self) -> bool {
        self.man.is_zero()
    }
    /// |x| < 2^top (and >= 2^(top - 1)) for x != 0.
    pub fn top(&self) -> i64 {
        self.exp + self.man.bits() as i64
    }
    /// -self, exactly.
    pub fn neg(&self) -> Float {
        Float::new(!self.neg, self.man.clone(), self.exp)
    }
    /// |self|, exactly.
    pub fn abs(&self) -> Float {
        Float::new(false, self.man.clone(), self.exp)
    }
    /// Exact sum (no rounding): the operands are aligned to the smaller exponent.
    pub fn add_exact(&self, o: &Float) -> Float {
        if self.is_zero() {
            return o.clone();
        }
        if o.is_zero() {
            return self.clone();
        }
        let e = self.exp.min(o.exp);
        let a = self.man.shl((self.exp - e) as u64);
        let b = o.man.shl((o.exp - e) as u64);
        if self.neg == o.neg {
            return Float::new(self.neg, a.add(&b), e);
        }
        match a.cmp(&b) {
            Ordering::Less => Float::new(o.neg, b.sub(&a), e),
            _ => Float::new(self.neg, a.sub(&b), e),
        }
    }
    /// Exact difference.
    pub fn sub_exact(&self, o: &Float) -> Float {
        self.add_exact(&o.neg())
    }
    /// Exact product.
    pub fn mul_exact(&self, o: &Float) -> Float {
        Float::new(self.neg != o.neg, self.man.mul(&o.man), self.exp + o.exp)
    }
    /// Exact comparison.
    pub fn cmp_exact(&self, o: &Float) -> Ordering {
        // different signs, or magnitudes in different binades (|x| in [2^(top - 1), 2^top)): decided without
        // aligning the mantissas, which for exponents far apart (1 against exp(-10^12)) took unbounded memory
        if !self.is_zero() && !o.is_zero() {
            if self.neg != o.neg {
                return if self.neg { Ordering::Less } else { Ordering::Greater };
            }
            let (ts, to) = (self.top(), o.top());
            if ts != to {
                let larger = if ts > to { Ordering::Greater } else { Ordering::Less };
                return if self.neg { larger.reverse() } else { larger };
            }
        }
        let d = self.sub_exact(o);
        if d.is_zero() {
            Ordering::Equal
        } else if d.neg {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }

    /// Rounds ±man 2^exp to p bits (to nearest, ties away from zero); returns the result and an upper bound of the
    /// rounding error (zero when exact).
    pub fn round(neg: bool, man: Nat, exp: i64, p: u64) -> (Float, Mag) {
        let b = man.bits();
        if b <= p {
            return (Float::new(neg, man, exp), Mag::ZERO);
        }
        let s = b - p;
        let mut keep = man.shr(s);
        let half = man.bit(s - 1);
        let inexact = half || man.low_bits_nonzero(s - 1);
        let mut e = exp + s as i64;
        let err = if inexact { Mag::pow2(e - 1) } else { Mag::ZERO };
        if half {
            keep = keep.add_u64(1);
            if keep.bits() > p {
                keep = keep.shr(1); // keep was 2^p: exact
                e += 1;
            }
        }
        (Float::new(neg, keep, e), err)
    }

    /// Sum rounded to p bits, with an error bound. An operand below a quarter of the other's last place is not
    /// aligned (that could take unbounded memory): the result is the larger operand and the smaller one's magnitude
    /// joins the error bound.
    pub fn add(&self, o: &Float, p: u64) -> (Float, Mag) {
        if self.is_zero() {
            return Float::round(o.neg, o.man.clone(), o.exp, p);
        }
        if o.is_zero() {
            return Float::round(self.neg, self.man.clone(), self.exp, p);
        }
        let (big, small) = if self.top() >= o.top() { (self, o) } else { (o, self) };
        if small.top() + 2 < big.top() - p as i64 {
            let (r, e) = Float::round(big.neg, big.man.clone(), big.exp, p);
            return (r, e.add(&Mag::from_float_up(small)));
        }
        let s = self.add_exact(o);
        Float::round(s.neg, s.man, s.exp, p)
    }
    /// Difference rounded to p bits, with an error bound.
    pub fn sub(&self, o: &Float, p: u64) -> (Float, Mag) {
        self.add(&o.neg(), p)
    }
    /// Product rounded to p bits, with an error bound.
    pub fn mul(&self, o: &Float, p: u64) -> (Float, Mag) {
        Float::round(self.neg != o.neg, self.man.mul(&o.man), self.exp + o.exp, p)
    }
    /// Quotient rounded to p bits: a truncated quotient with p + 2 bits (error below its last unit), then rounding.
    pub fn div(&self, o: &Float, p: u64) -> (Float, Mag) {
        if o.is_zero() {
            super::after_stop("Float::div by zero");
            return (Float::zero(), Mag::ZERO);
        }
        if self.is_zero() {
            return (Float::zero(), Mag::ZERO);
        }
        let s = (p as i64 + 2 + o.man.bits() as i64 - self.man.bits() as i64).max(0) as u64;
        let (q, r) = self.man.shl(s).divrem(&o.man);
        let eq = self.exp - s as i64 - o.exp;
        let trunc = if r.is_zero() { Mag::ZERO } else { Mag::pow2(eq) };
        let (m, e) = Float::round(self.neg != o.neg, q, eq, p);
        (m, e.add(&trunc))
    }
    /// Square root rounded to p bits (self >= 0): a floor root with p + 2 bits, then rounding. The floor is always
    /// counted as inexact (a squaring to prove exactness would cost more than the bound loses).
    pub fn sqrt(&self, p: u64) -> (Float, Mag) {
        if self.neg {
            super::after_stop("Float::sqrt of a negative number");
            return (Float::zero(), Mag::ZERO);
        }
        if self.is_zero() {
            return (Float::zero(), Mag::ZERO);
        }
        let mut s = (2 * (p as i64 + 2) - self.man.bits() as i64).max(0);
        if (self.exp - s).rem_euclid(2) == 1 {
            s += 1;
        }
        let r = self.man.shl(s as u64).isqrt();
        let er = (self.exp - s) / 2;
        let (m, e) = Float::round(false, r, er, p);
        (m, e.add(&Mag::pow2(er)))
    }

    /// d significant decimal digits of |self| (rounded) and the decimal exponent of the first: |self| ~ 0.d1d2.. 10^(e+1)
    /// with digit string d1d2... Used for reports and digit checks.
    pub fn to_sci(&self, d: usize) -> (String, i64) {
        if self.is_zero() {
            return ("0".repeat(d), 0);
        }
        if self.exp.unsigned_abs() > EXACT_SCI_EXP {
            return self.to_sci_scaled(d);
        }
        let mut e10 = ((self.top() - 1) as f64 * std::f64::consts::LOG10_2).floor() as i64;
        for _ in 0..4 {
            let k = d as i64 - 1 - e10;
            let mut num = self.man.clone();
            let mut den = Nat::from_u64(1);
            if k >= 0 {
                num = num.mul(&Nat::pow(10, k as u64));
            } else {
                den = den.mul(&Nat::pow(10, (-k) as u64));
            }
            if self.exp >= 0 {
                num = num.shl(self.exp as u64);
            } else {
                den = den.shl((-self.exp) as u64);
            }
            let n = num.add(&den.shr(1)).div(&den);
            let s = n.to_decimal();
            match s.len().cmp(&d) {
                Ordering::Greater => e10 += 1,
                Ordering::Less => e10 -= 1,
                Ordering::Equal => return (s, e10),
            }
        }
        super::after_stop("to_sci: exponent estimate did not settle");
        ("0".repeat(d), 0)
    }

    /// [`Float::to_sci`] for a huge binary exponent: the exact conversion builds integers of about |exp| bits (a
    /// value near e^(-2^31) needs 2^31 bits and ran the process out of memory), so the scaling by 10^k is done in
    /// floating point with 128 guard bits. The digits are the correctly rounded ones unless the value lies within
    /// 2^-128 (relative) of a rounding boundary.
    fn to_sci_scaled(&self, d: usize) -> (String, i64) {
        let w = d as u64 * 4 + 128;
        let x = self.abs();
        let mut e10 = ((self.top() - 1) as f64 * std::f64::consts::LOG10_2).floor() as i64;
        for _ in 0..4 {
            let k = d as i64 - 1 - e10;
            // 10^|k| by binary powering at w bits
            let (mut p, mut b) = (Float::from_nat(Nat::from_u64(1)), Float::from_nat(Nat::from_u64(10)));
            let mut n = k.unsigned_abs();
            while n > 0 {
                if n & 1 == 1 {
                    p = p.mul(&b, w).0;
                }
                n >>= 1;
                if n > 0 {
                    b = b.mul(&b, w).0;
                }
            }
            let y = if k >= 0 { x.mul(&p, w).0 } else { x.div(&p, w).0 };
            // y rounded to an integer
            let n = if y.exp >= 0 {
                y.man.shl(y.exp as u64)
            } else {
                let s = y.exp.unsigned_abs();
                y.man.add(&Nat::pow2(s - 1)).shr(s)
            };
            let s = n.to_decimal();
            match s.len().cmp(&d) {
                Ordering::Greater => e10 += 1,
                Ordering::Less => e10 -= 1,
                Ordering::Equal => return (s, e10),
            }
        }
        super::after_stop("to_sci: exponent estimate did not settle");
        ("0".repeat(d), 0)
    }
}

/// The largest |binary exponent| [`Float::to_sci`] converts exactly (integers of up to about 2^16 bits).
const EXACT_SCI_EXP: u64 = 1 << 16;

// ---------------------------------------------------------------- balls

#[derive(Clone, Debug, PartialEq, Eq)]
/// A ball: the reals within `rad` of `mid` (semantics 2.3).
pub struct Ball {
    /// The midpoint.
    pub mid: Float,
    /// The radius, an upper bound of the accumulated error.
    pub rad: Mag,
}

impl Ball {
    /// An exact ball (zero radius).
    pub fn exact(mid: Float) -> Ball {
        Ball { mid, rad: Mag::ZERO }
    }
    /// An integer as an exact ball.
    pub fn from_i64(v: i64) -> Ball {
        Ball::exact(Float::from_i64(v))
    }
    /// A natural number as an exact ball.
    pub fn from_nat(n: Nat) -> Ball {
        Ball::exact(Float::from_nat(n))
    }
    /// A ball around num / den at precision p.
    pub fn from_ratio(num: i64, den: i64, p: u64) -> Ball {
        Ball::from_i64(num).div(&Ball::from_i64(den), p).expect("nonzero denominator")
    }
    /// -self, exactly.
    pub fn neg(&self) -> Ball {
        Ball { mid: self.mid.neg(), rad: self.rad }
    }
    /// An enclosure of the sum, with the midpoint at p bits.
    pub fn add(&self, o: &Ball, p: u64) -> Ball {
        let (m, e) = self.mid.add(&o.mid, p);
        Ball { mid: m, rad: self.rad.add(&o.rad).add(&e) }
    }
    /// An enclosure of the difference, with the midpoint at p bits.
    pub fn sub(&self, o: &Ball, p: u64) -> Ball {
        self.add(&o.neg(), p)
    }
    /// |xy - ma mb| <= |ma| rb + |mb| ra + ra rb for x, y in the balls.
    pub fn mul(&self, o: &Ball, p: u64) -> Ball {
        let (m, e) = self.mid.mul(&o.mid, p);
        let mut r = e;
        if !o.rad.is_zero() {
            r = r.add(&Mag::from_float_up(&self.mid).mul(&o.rad));
        }
        if !self.rad.is_zero() {
            r = r.add(&Mag::from_float_up(&o.mid).mul(&self.rad));
            r = r.add(&self.rad.mul(&o.rad));
        }
        Ball { mid: m, rad: r }
    }
    /// None when the divisor ball may contain zero (decided on a 32-bit lower bound of |mb| - rb, so a ball that
    /// excludes zero by less than 2^-31 relative also counts: its quotient would be uselessly wide anyway).
    /// For x, y in the balls, |x/y - ma/mb| <= (|ma| rb + |mb| ra) / (|mb| (|mb| - rb)).
    pub fn div(&self, o: &Ball, p: u64) -> Option<Ball> {
        let mb_lo = Mag::from_float_down(&o.mid);
        let gap = mb_lo.sub_down(&o.rad);
        if gap.is_zero() {
            return None;
        }
        let (m, e) = self.mid.div(&o.mid, p);
        let mut r = e;
        if !(o.rad.is_zero() && self.rad.is_zero()) {
            let num = Mag::from_float_up(&self.mid).mul(&o.rad).add(&Mag::from_float_up(&o.mid).mul(&self.rad));
            r = r.add(&num.div_up(&mb_lo.mul_down(&gap)));
        }
        Some(Ball { mid: m, rad: r })
    }
    /// None unless the ball is provably positive (to 32 bits, as for div).
    /// |sqrt(x) - sqrt(m)| = |x - m| / (sqrt x + sqrt m) <= r / (2 sqrt(m - r)).
    pub fn sqrt(&self, p: u64) -> Option<Ball> {
        if self.mid.is_zero() && self.rad.is_zero() {
            return Some(self.clone());
        }
        if self.mid.neg {
            return None;
        }
        let lo = Mag::from_float_down(&self.mid).sub_down(&self.rad);
        if lo.is_zero() {
            return None;
        }
        let (m, e) = self.mid.sqrt(p);
        let mut r = e;
        if !self.rad.is_zero() {
            r = r.add(&self.rad.div_up(&lo.sqrt_down().mul_2exp(1)));
        }
        Some(Ball { mid: m, rad: r })
    }
    /// Does the ball contain zero?
    pub fn contains_zero(&self) -> bool {
        self.mid.abs().cmp_exact(&self.rad.to_float()) != Ordering::Greater
    }
    /// Three-valued comparison (semantics 2.5): decided only when the balls are disjoint, or both exact.
    pub fn cmp3(&self, o: &Ball) -> Option<Ordering> {
        let s = self.rad.add(&o.rad);
        // midpoints of magnitudes more than 2 binades apart: the difference has the sign of the larger term (negated
        // for o), and no exact difference is formed (its alignment could take unbounded memory). Its modulus exceeds
        // |big| - 2^top(small) >= |big| - 2^e with e = max(top(small), exp(big) - 64): a bound within 2^-64 of |big|
        // at worst (so a radius sum just below |big| still decides) whose exact form needs an alignment of at most
        // 64 bits beyond the mantissa of big.
        let (a, bm) = (&self.mid, &o.mid);
        if !a.is_zero() && !bm.is_zero() && (a.top() - bm.top()).abs() > 2 {
            let (big, small, neg) = if a.top() > bm.top() { (a, bm, a.neg) } else { (bm, a, !bm.neg) };
            let e = small.top().max(big.exp - 64);
            let lower = big.abs().sub_exact(&Float::new(false, Nat::from_u64(1), e));
            if s.is_zero() || lower.cmp_exact(&s.to_float()) == Ordering::Greater {
                return Some(if neg { Ordering::Less } else { Ordering::Greater });
            }
            return None;
        }
        let d = self.mid.sub_exact(&o.mid);
        if s.is_zero() {
            return Some(if d.is_zero() { Ordering::Equal } else if d.neg { Ordering::Less } else { Ordering::Greater });
        }
        if d.abs().cmp_exact(&s.to_float()) == Ordering::Greater {
            Some(if d.neg { Ordering::Less } else { Ordering::Greater })
        } else {
            None
        }
    }
    /// Three-valued self < o: `None` when undecided.
    pub fn lt(&self, o: &Ball) -> Option<bool> {
        self.cmp3(o).map(|c| c == Ordering::Less)
    }
    /// At least d correct significant decimal digits: r 10^d <= |m| - r, so every point of the ball agrees with m to
    /// a relative error of 10^-d. An exact ball qualifies; a ball containing zero never does.
    pub fn has_digits(&self, d: u32) -> bool {
        if self.rad.is_zero() {
            return true;
        }
        // r 10^d <= |m| - r, as r (10^d + 1) <= |m|: no exact |m| - r, whose alignment for a tiny r (the sum 1 +
        // exp(-10^12)) took unbounded memory; the comparison decides far-apart magnitudes by their exponents
        let rf = self.rad.to_float();
        if self.mid.is_zero() {
            return false;
        }
        let lhs = rf.mul_exact(&Float::from_nat(Nat::pow(10, d as u64).add_u64(1)));
        lhs.cmp_exact(&self.mid.abs()) != Ordering::Greater
    }
    /// Approximate number of correct bits relative to the midpoint (for reports).
    pub fn rel_bits(&self) -> f64 {
        if self.rad.is_zero() {
            return f64::INFINITY;
        }
        self.mid.top() as f64 - 1.0 - self.rad.log2()
    }
}

// ---------------------------------------------------------------- enclosure trials

#[cfg(test)]
/// Counts of the randomised enclosure test.
#[derive(Clone, Copy, Debug, Default)]
pub struct Trials {
    pub checks: u64,
    pub failures: u64,
    /// operations that returned None (divisor containing zero, sqrt of a non-positive ball)
    pub undefined: u64,
}

#[cfg(test)]
fn rand_float(rng: &mut crate::num::Rng, bits: u64) -> Float {
    let limbs = bits.div_ceil(64) as usize;
    let man = Nat::from_limbs((0..limbs).map(|_| rng.next_u64()).collect()).shr(limbs as u64 * 64 - bits);
    let exp = rng.below(80) as i64 - 40 - bits as i64;
    Float::new(rng.below(2) == 1, man, exp)
}

#[cfg(test)]
fn rand_ball(rng: &mut crate::num::Rng, bits: u64) -> Ball {
    let mbits = 1 + rng.below(bits);
    let mid = rand_float(rng, mbits);
    let top = match rng.below(4) {
        0 => return Ball::exact(mid),
        // small: near the midpoint's last place
        1 | 2 => mid.top() - bits as i64 - rng.below(20) as i64,
        // large: comparable to the midpoint (exercises the zero-containing and near-zero cases)
        _ => mid.top() + 1 - rng.below(6) as i64,
    };
    // a radius of about 2^top
    let rad = Mag::up((rng.next_u64() | 1 << 63) as u128, top - 64);
    Ball { mid, rad }
}

#[cfg(test)]
/// Points of a ball: both ends, the midpoint and a random interior dyadic point, all exact.
fn points(rng: &mut crate::num::Rng, b: &Ball) -> Vec<Float> {
    let r = b.rad.to_float();
    let j = rng.below(513) as i64 - 256;
    let t = Float::new(j < 0, Nat::from_u64(j.unsigned_abs()), -8);
    vec![b.mid.sub_exact(&r), b.mid.add_exact(&r), b.mid.clone(), b.mid.add_exact(&r.mul_exact(&t))]
}

#[cfg(test)]
/// |v - m| <= r, exactly.
fn inside(v: &Float, b: &Ball) -> bool {
    v.sub_exact(&b.mid).abs().cmp_exact(&b.rad.to_float()) != Ordering::Greater
}

#[cfg(test)]
/// Random balls at random precisions; for every operation the exact result at each test point of the inputs must
/// lie in the output ball. Exact checks: add, sub and mul compare dyadic values; div checks |x - m y| <= r |y|;
/// sqrt checks lo^2 <= x <= hi^2 with lo = max(m - r, 0), hi = m + r.
pub fn enclosure_trials(seed: u64, n: u64) -> Trials {
    let mut rng = crate::num::Rng(seed);
    let mut t = Trials::default();
    for _ in 0..n {
        let p = [24u64, 53, 64, 100, 200, 333, 700][rng.below(7) as usize];
        let bits = 1 + rng.below(2 * p);
        let a = rand_ball(&mut rng, bits);
        let b = rand_ball(&mut rng, bits);
        let pa = points(&mut rng, &a);
        let pb = points(&mut rng, &b);
        let sum = a.add(&b, p);
        let dif = a.sub(&b, p);
        let prd = a.mul(&b, p);
        for x in &pa {
            for y in &pb {
                t.checks += 3;
                t.failures += !inside(&x.add_exact(y), &sum) as u64;
                t.failures += !inside(&x.sub_exact(y), &dif) as u64;
                t.failures += !inside(&x.mul_exact(y), &prd) as u64;
            }
        }
        match a.div(&b, p) {
            None => t.undefined += 1,
            Some(q) => {
                let r = q.rad.to_float();
                for x in &pa {
                    for y in &pb {
                        t.checks += 1;
                        let lhs = x.sub_exact(&q.mid.mul_exact(y)).abs();
                        let rhs = r.mul_exact(&y.abs());
                        t.failures += (lhs.cmp_exact(&rhs) == Ordering::Greater) as u64;
                    }
                }
            }
        }
        let a_abs = Ball { mid: a.mid.abs(), rad: a.rad };
        match a_abs.sqrt(p) {
            None => t.undefined += 1,
            Some(s) => {
                let r = s.rad.to_float();
                let lo = s.mid.sub_exact(&r);
                let hi = s.mid.add_exact(&r);
                for x in points(&mut rng, &a_abs) {
                    t.checks += 1;
                    let lo_ok = lo.neg || lo.is_zero() || lo.mul_exact(&lo).cmp_exact(&x) != Ordering::Greater;
                    let hi_ok = !hi.neg && hi.mul_exact(&hi).cmp_exact(&x) != Ordering::Less;
                    t.failures += !(lo_ok && hi_ok) as u64;
                }
            }
        }
        // comparisons: a decided answer must hold at every pair of points
        if let Some(c) = a.cmp3(&b) {
            for x in &pa {
                for y in &pb {
                    t.checks += 1;
                    t.failures += (x.cmp_exact(y) != c) as u64;
                }
            }
        }
    }
    // chains: 30 operations on exact 64-bit inputs at 100 bits, against the exact dyadic value
    for _ in 0..n / 10 {
        let mut exact = rand_float(&mut rng, 64);
        let mut ball = Ball::exact(exact.clone());
        for _ in 0..30 {
            let y = rand_float(&mut rng, 64);
            let yb = Ball::exact(y.clone());
            match rng.below(3) {
                0 => {
                    exact = exact.add_exact(&y);
                    ball = ball.add(&yb, 100);
                }
                1 => {
                    exact = exact.sub_exact(&y);
                    ball = ball.sub(&yb, 100);
                }
                _ => {
                    exact = exact.mul_exact(&y);
                    ball = ball.mul(&yb, 100);
                }
            }
        }
        t.checks += 1;
        t.failures += !inside(&exact, &ball) as u64;
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_decimal_conversion_agrees_with_the_exact_one() {
        // just inside the exact range, both conversions run; beyond it only the scaled one (no 2^31-bit integers)
        for &e in &[-(1i64 << 16), (1 << 16) - 70, -(1 << 16) + 3] {
            for m in [1u64, 3, 0xDEAD_BEEF, u64::MAX] {
                let f = Float::new(false, Nat::from_u64(m), e);
                for d in [1, 5, 17, 30] {
                    assert_eq!(f.to_sci(d), f.to_sci_scaled(d), "{m} 2^{e} to {d} digits");
                }
            }
        }
        let (s, e) = Float::new(false, Nat::from_u64(1), -(1 << 31)).to_sci(10);
        assert_eq!((s.as_str(), e), ("5676615526", -646456994));
    }

    #[test]
    fn enclosure_holds() {
        let t = enclosure_trials(11, 3000);
        assert_eq!(t.failures, 0, "{t:?}");
        assert!(t.checks > 100_000 && t.undefined > 0, "{t:?}");
    }

    #[test]
    fn mag_bounds_hold() {
        let mut rng = crate::num::Rng(5);
        for _ in 0..20000 {
            let a = Mag::up((rng.next_u64() | 1) as u128, rng.below(200) as i64 - 100);
            let b = Mag::up((rng.next_u64() | 1) as u128, rng.below(200) as i64 - 100);
            let (fa, fb) = (a.to_float(), b.to_float());
            let ge = |m: &Mag, x: &Float| m.to_float().cmp_exact(x) != Ordering::Less;
            let le = |m: &Mag, x: &Float| m.to_float().cmp_exact(x) != Ordering::Greater;
            assert!(ge(&a.add(&b), &fa.add_exact(&fb)));
            assert!(ge(&a.mul(&b), &fa.mul_exact(&fb)));
            assert!(le(&a.mul_down(&b), &fa.mul_exact(&fb)));
            let d = fa.sub_exact(&fb);
            let sd = a.sub_down(&b);
            assert!(le(&sd, &d) || (sd.is_zero() && d.neg), "{a:?} - {b:?}");
            // q >= a / b  <=>  q b >= a
            assert!(a.div_up(&b).to_float().mul_exact(&fb).cmp_exact(&fa) != Ordering::Less);
            let s = a.sqrt_down().to_float();
            assert!(s.mul_exact(&s).cmp_exact(&fa) != Ordering::Greater);
            let x = Float::new(false, Nat::from_limbs(vec![rng.next_u64(), rng.next_u64(), rng.next_u64()]), -70);
            assert!(ge(&Mag::from_float_up(&x), &x) && le(&Mag::from_float_down(&x), &x));
        }
    }

    #[test]
    fn overlap_is_undecided() {
        let p = 100;
        let third = Ball::from_ratio(1, 3, p);
        let x = third.mul(&Ball::from_i64(3), p);
        // 3 (1/3) overlaps 1 but is not exactly 1: undecided both ways
        assert_eq!(x.cmp3(&Ball::from_i64(1)), None);
        assert_eq!(x.lt(&Ball::from_i64(1)), None);
        assert_eq!(x.lt(&Ball::from_i64(2)), Some(true));
        assert_eq!(Ball::from_i64(2).cmp3(&Ball::from_i64(2)), Some(Ordering::Equal));
        assert!(x.has_digits(25) && !x.has_digits(40));
    }

    #[test]
    fn sqrt_two_digits() {
        let s = Ball::from_i64(2).sqrt(200).unwrap();
        let (d, e) = s.mid.to_sci(50);
        assert_eq!((d.as_str(), e), ("14142135623730950488016887242096980785696718753769", 0));
        assert!(s.has_digits(55));
        assert!(Ball::from_i64(-1).sqrt(64).is_none());
        let z = Ball { mid: Float::from_i64(1), rad: Mag::pow2(1) };
        assert!(Ball::from_i64(1).div(&z, 64).is_none());
    }
}
