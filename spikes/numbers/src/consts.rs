//! Constants by binary splitting (benchmarks N1, N2): pi by the Chudnovsky series, e by the factorial series, and
//! independent checks (Machin's formula for pi, the continued fraction of e). The series code is generic over an
//! integer backend so the same algorithm runs on this crate's integers and on num-bigint.
use crate::int::Int;
use crate::nat::Nat;
use num_bigint::{BigInt as NbInt, Sign};

/// The integer operations the constants need. Divisions and roots take non-negative operands.
pub trait Backend: Clone {
    const NAME: &'static str;
    fn from_u64(v: u64) -> Self;
    fn add(&self, o: &Self) -> Self;
    fn mul(&self, o: &Self) -> Self;
    fn neg(&self) -> Self;
    fn shl(&self, n: u64) -> Self;
    fn div_floor(&self, o: &Self) -> Self;
    fn rem(&self, o: &Self) -> Self;
    fn isqrt(&self) -> Self;
    fn pow10(n: u64) -> Self;
    fn to_decimal(&self) -> String;
    fn mul_u64(&self, v: u64) -> Self {
        self.mul(&Self::from_u64(v))
    }
}

impl Backend for Int {
    const NAME: &'static str = "own";
    fn from_u64(v: u64) -> Self {
        Int::from_u64(v)
    }
    fn add(&self, o: &Self) -> Self {
        Int::add(self, o)
    }
    fn mul(&self, o: &Self) -> Self {
        Int::mul(self, o)
    }
    fn neg(&self) -> Self {
        Int::neg(self)
    }
    fn shl(&self, n: u64) -> Self {
        Int::shl(self, n)
    }
    fn div_floor(&self, o: &Self) -> Self {
        assert!(!self.neg && !o.neg);
        Int::from_nat(self.mag.div(&o.mag))
    }
    fn rem(&self, o: &Self) -> Self {
        assert!(!self.neg && !o.neg);
        Int::from_nat(self.mag.divrem(&o.mag).1)
    }
    fn isqrt(&self) -> Self {
        Int::from_nat(self.mag.isqrt())
    }
    fn pow10(n: u64) -> Self {
        Int::from_nat(Nat::pow(10, n))
    }
    fn to_decimal(&self) -> String {
        assert!(!self.neg);
        self.mag.to_decimal()
    }
    fn mul_u64(&self, v: u64) -> Self {
        Int::new(self.neg, self.mag.mul_u64(v))
    }
}

impl Backend for NbInt {
    const NAME: &'static str = "num-bigint";
    fn from_u64(v: u64) -> Self {
        NbInt::from(v)
    }
    fn add(&self, o: &Self) -> Self {
        self + o
    }
    fn mul(&self, o: &Self) -> Self {
        self * o
    }
    fn neg(&self) -> Self {
        -self
    }
    fn shl(&self, n: u64) -> Self {
        self << n as usize
    }
    fn div_floor(&self, o: &Self) -> Self {
        self / o
    }
    fn rem(&self, o: &Self) -> Self {
        self % o
    }
    fn isqrt(&self) -> Self {
        self.sqrt()
    }
    fn pow10(n: u64) -> Self {
        NbInt::from(10u32).pow(n as u32)
    }
    fn to_decimal(&self) -> String {
        self.to_string()
    }
    fn mul_u64(&self, v: u64) -> Self {
        self * v
    }
}

/// Little-endian 64-bit limbs of a non-negative num-bigint value, for fingerprints.
pub fn nb_limbs(x: &NbInt) -> Vec<u64> {
    let (s, v) = x.to_u64_digits();
    assert!(s != Sign::Minus);
    v
}

pub fn nb_from_limbs(l: &[u64]) -> NbInt {
    let mut v = Vec::with_capacity(2 * l.len());
    for &x in l {
        v.push(x as u32);
        v.push((x >> 32) as u32);
    }
    NbInt::from_biguint(Sign::Plus, num_bigint::BigUint::new(v))
}

pub fn bits_for_digits(d: u64) -> u64 {
    (d as f64 * std::f64::consts::LOG2_10).ceil() as u64
}

// ---------------------------------------------------------------- pi

/// Chudnovsky: 1/pi = 12 sum_k (-1)^k (6k)! (13591409 + 545140134 k) / ((3k)! (k!)^3 640320^(3k + 3/2)), so
/// pi = 426880 sqrt(10005) Q / T with P, Q, T over the terms by binary splitting:
/// P(a,b) = prod p(k), Q(a,b) = prod q(k), T(a,b) = T(a,m) Q(m,b) + P(a,m) T(m,b).
fn chud_split<B: Backend>(a: u64, b: u64, need_p: bool) -> (B, B, B) {
    if b == a + 1 {
        if a == 0 {
            return (B::from_u64(1), B::from_u64(1), B::from_u64(13_591_409));
        }
        // p(k) = -(6k-5)(2k-1)(6k-1) fits a u64 for k < 10^5 and the products below stay in range for k < 10^6
        let p = B::from_u64((6 * a - 5) * (2 * a - 1)).mul_u64(6 * a - 1).neg();
        let q = B::from_u64(a * a).mul_u64(a).mul_u64(10_939_058_860_032_000); // 640320^3 / 24
        let t = p.mul_u64(13_591_409 + 545_140_134 * a);
        return (p, q, t);
    }
    let m = (a + b) / 2;
    let (p1, q1, t1) = chud_split::<B>(a, m, true);
    if crate::stopped() {
        return (p1, q1, t1);
    }
    let (p2, q2, t2) = chud_split::<B>(m, b, need_p);
    let t = t1.mul(&q2).add(&p1.mul(&t2));
    // the right spine never needs P: skipping it saves one large product per level
    let p = if need_p { p1.mul(&p2) } else { B::from_u64(0) };
    (p, q1.mul(&q2), t)
}

/// The series part of pi_fixed alone (Q and T), for the cost breakdown.
pub fn chud_qt<B: Backend>(bits: u64) -> (B, B) {
    let (_, q, t) = chud_split::<B>(0, (bits + 8) / 47 + 2, false);
    (q, t)
}

/// floor(pi 2^bits) up to an error below 2 units: the series is cut where its tail is below 2^-(bits+8) relative
/// (each term contributes log2(151931373056000) = 47.11 bits), sqrt(10005) 2^bits is floored (error < 1 unit, scaled
/// by 426880 Q / T ~ 0.03), and the final division floors (< 1 unit).
pub fn pi_fixed<B: Backend>(bits: u64) -> B {
    let terms = (bits + 8) / 47 + 2;
    let (_, q, t) = chud_split::<B>(0, terms, false);
    let s = B::from_u64(10005).shl(2 * bits).isqrt();
    s.mul(&q).mul_u64(426_880).div_floor(&t)
}

/// The first d decimal digits of pi ("31415..."), computed with 12 guard digits. A wrong last digit would need 12
/// consecutive nines or zeros at the cut; the self-consistency check at a second precision covers that case.
pub fn pi_digits<B: Backend>(d: u64) -> String {
    let g = d + 12;
    let bits = bits_for_digits(g) + 16;
    let x = pi_fixed::<B>(bits);
    let s = x.mul(&B::pow10(g - 1)).div_floor(&B::from_u64(1).shl(bits)).to_decimal();
    s[..d as usize].to_string()
}

/// atan(1/n) 2^bits by the Taylor series in fixed point (the classic quadratic method, for checking only).
fn atan_inv(n: u64, bits: u64) -> Int {
    let mut term = Nat::pow2(bits).divrem_u64(n).0;
    let n2 = n * n;
    let mut sum = Int::default();
    let mut k = 1u64;
    while !term.is_zero() {
        if crate::stopped() {
            break;
        }
        let t = Int::from_nat(term.divrem_u64(k).0);
        sum = if (k / 2).is_multiple_of(2) { sum.add(&t) } else { sum.sub(&t) };
        term = term.divrem_u64(n2).0;
        k += 2;
    }
    sum
}

/// pi by Machin's formula 16 atan(1/5) - 4 atan(1/239): an algorithm independent of Chudnovsky, quadratic, used
/// to check the first d digits at 10^4 and 10^5.
pub fn pi_digits_machin(d: u64) -> String {
    let g = d + 12;
    let bits = bits_for_digits(g) + 32;
    let x = atan_inv(5, bits).mul(&Int::from_u64(16)).sub(&atan_inv(239, bits).mul(&Int::from_u64(4)));
    let s = x.mag.mul(&Nat::pow(10, g - 1)).shr(bits).to_decimal();
    s[..d as usize].to_string()
}

// ---------------------------------------------------------------- e

/// sum_{k=a+1}^{b} a!/k! = T / Q with Q = (a+1)...b: Q = Q1 Q2, T = T1 Q2 + T2.
fn e_split<B: Backend>(a: u64, b: u64) -> (B, B) {
    if b == a + 1 {
        return (B::from_u64(b), B::from_u64(1));
    }
    let m = (a + b) / 2;
    let (q1, t1) = e_split::<B>(a, m);
    if crate::stopped() {
        return (q1, t1);
    }
    let (q2, t2) = e_split::<B>(m, b);
    (q1.mul(&q2), t1.mul(&q2).add(&t2))
}

/// The number of terms N with N! > 10^(d + extra): then e - sum_{k<=N} 1/k! < 2/(N+1)! < 10^-(d + extra).
pub fn e_terms(d: u64, extra: u64) -> u64 {
    let target = (d + extra) as f64 * std::f64::consts::LN_10;
    let mut acc = 0.0f64;
    let mut n = 1u64;
    while acc <= target {
        n += 1;
        acc += (n as f64).ln();
    }
    n
}

/// e = (Q + T) / Q over N terms (an exact rational within 10^-(d + extra) of e).
pub fn e_ratio<B: Backend>(d: u64, extra: u64) -> (B, B) {
    let (q, t) = e_split::<B>(0, e_terms(d, extra));
    (q.add(&t), q)
}

/// The first d decimal digits of e ("27182..."), 12 guard digits.
pub fn e_digits<B: Backend>(d: u64) -> String {
    let (num, den) = e_ratio::<B>(d, 20);
    let s = num.mul(&B::pow10(d + 11)).div_floor(&den).to_decimal();
    s[..d as usize].to_string()
}

/// k decimal digits of e starting at position off + 1 after the point (N2), without converting the digits before:
/// with e ~ E = num/den, the digits are floor(((num 10^off) mod den) 10^k / den). E undershoots e by less than
/// 10^-(off + k + 20), so 20 extra digits are computed and must not all be nines (else the carry would be unknown).
pub fn e_digits_at<B: Backend>(off: u64, k: u64) -> String {
    let (num, den) = e_ratio::<B>(off + k, 20);
    let r = num.mul(&B::pow10(off)).rem(&den);
    let s = r.mul(&B::pow10(k + 20)).div_floor(&den).to_decimal();
    let s = format!("{}{}", "0".repeat((k + 20) as usize - s.len()), s);
    assert!(!s[k as usize..].bytes().all(|c| c == b'9'), "e digits at {off}: carry undetermined");
    s[..k as usize].to_string()
}

/// e's digits by an independent route: convergents of the continued fraction e = [2; 1, 2, 1, 1, 4, 1, 1, 6, ...],
/// as a product of 2x2 matrices by binary splitting. |e - p/q| < 1/q^2, so q > 10^((d + 12) / 2) suffices.
pub fn e_digits_cf(d: u64) -> String {
    fn coeff(i: u64) -> u64 {
        match i {
            0 => 2,
            _ if i % 3 == 2 => 2 * i.div_ceil(3),
            _ => 1,
        }
    }
    // product of [[a_i, 1], [1, 0]] for i in lo..hi, as (m00, m01, m10, m11)
    fn split(lo: u64, hi: u64) -> [Nat; 4] {
        if hi == lo + 1 {
            return [Nat::from_u64(coeff(lo)), Nat::from_u64(1), Nat::from_u64(1), Nat::zero()];
        }
        let m = (lo + hi) / 2;
        let a = split(lo, m);
        if crate::stopped() {
            return a;
        }
        let b = split(m, hi);
        [
            a[0].mul(&b[0]).add(&a[1].mul(&b[2])),
            a[0].mul(&b[1]).add(&a[1].mul(&b[3])),
            a[2].mul(&b[0]).add(&a[3].mul(&b[2])),
            a[2].mul(&b[1]).add(&a[3].mul(&b[3])),
        ]
    }
    let need = bits_for_digits(d + 12) / 2 + 8;
    let mut n = 64;
    loop {
        let m = split(0, n);
        // the convergent p/q = m00 / m10
        if m[2].bits() >= need || crate::stopped() {
            let s = m[0].mul(&Nat::pow(10, d + 11)).div(&m[2]).to_decimal();
            return s[..d as usize].to_string();
        }
        n *= 2; // bounded: q grows faster than 2^n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PI50: &str = "31415926535897932384626433832795028841971693993751";
    const E50: &str = "27182818284590452353602874713526624977572470936999";

    #[test]
    fn pi_by_both_backends_and_machin() {
        let own = pi_digits::<Int>(3000);
        assert_eq!(&own[..50], PI50);
        assert_eq!(own, pi_digits::<NbInt>(3000));
        assert_eq!(own, pi_digits_machin(3000));
        // six nines starting at digit 762 after the point
        assert_eq!(&own[762..768], "999999");
    }

    #[test]
    fn e_by_series_and_continued_fraction() {
        let s = e_digits::<Int>(3000);
        assert_eq!(&s[..50], E50);
        assert_eq!(s, e_digits_cf(3000));
        assert_eq!(s, e_digits::<NbInt>(3000));
        // digits after the point at an offset, against the full expansion
        assert_eq!(e_digits_at::<Int>(2000, 30), s[2001..2031]);
        assert_eq!(e_digits_at::<NbInt>(2000, 30), s[2001..2031]);
    }
}
