//! Conversions between exact values and machine reals (IEEE binary64). Exact to machine is correctly rounded (to
//! nearest, ties to even, with subnormals), so contagion (semantics 2.2) gives the machine real nearest the exact
//! value rather than whatever a sequence of floating-point operations would produce.
use super::nat::Nat;

/// 2^e as a machine real, for -1074 <= e <= 1023.
fn pow2(e: i64) -> f64 {
    debug_assert!((-1074..=1023).contains(&e));
    if e >= -1022 {
        f64::from_bits(((e + 1023) as u64) << 52)
    } else {
        f64::from_bits(1u64 << (e + 1074))
    }
}

/// (-1)^neg man 2^exp rounded to the nearest machine real (ties to even). Overflow gives an infinity; zero is +0.
pub fn dyadic_to_f64(neg: bool, man: &Nat, exp: i64) -> f64 {
    let b = man.bits() as i64;
    if b == 0 {
        return 0.0;
    }
    let sign = if neg { -1.0 } else { 1.0 };
    // the value lies in [2^t, 2^(t+1))
    let t = exp.saturating_add(b - 1);
    if t > 1023 {
        return sign * f64::INFINITY;
    }
    if t < -1076 {
        // below half the smallest subnormal
        return sign * 0.0;
    }
    // bits kept: 53 for normal results, fewer in the subnormal range; e0 is the exponent of the last kept bit
    let keep = if t >= -1022 { 53 } else { t + 1075 };
    let e0 = t - keep + 1;
    let s = e0 - exp;
    let mut q = if s <= 0 { man.shl((-s) as u64) } else { man.shr(s as u64) };
    if s > 0 {
        let half = man.bit((s - 1) as u64);
        let sticky = s >= 2 && man.low_bits_nonzero((s - 1) as u64);
        if half && (sticky || q.bit(0)) {
            q = q.add_u64(1);
        }
    }
    let q = q.to_u128().expect("at most 2^53") as u64;
    if q == 0 {
        return sign * 0.0;
    }
    if t < -1022 {
        // subnormal range: e0 is -1074, so the bits of q are the encoding (q = 2^52 is the smallest normal)
        return sign * f64::from_bits(q);
    }
    // q <= 2^53 is exact as a machine real, and so is the product unless it overflows (q = 2^53 at t = 1023)
    sign * (q as f64 * pow2(e0))
}

/// num / den (den > 0) rounded to the nearest machine real (ties to even).
pub fn ratio_to_f64(neg: bool, num: &Nat, den: &Nat) -> f64 {
    if den.limbs == [1] {
        return dyadic_to_f64(neg, num, 0);
    }
    if num.is_zero() {
        return 0.0;
    }
    // a quotient of 55 or 56 bits, then the remainder as a sticky bit: rounding that to at most 53 bits is the
    // rounding of the exact quotient (two guard bits rule out double rounding)
    let s = 55 + den.bits() as i64 - num.bits() as i64;
    let (q, r) = if s >= 0 { num.shl(s as u64).divrem(den) } else { num.divrem(&den.shl((-s) as u64)) };
    let q2 = q.shl(1).add_u64(!r.is_zero() as u64);
    dyadic_to_f64(neg, &q2, -s - 1)
}

/// The exact value of a finite machine real as (-1)^neg man 2^exp, or `None` for infinities and NaN.
pub fn f64_to_dyadic(v: f64) -> Option<(bool, Nat, i64)> {
    if !v.is_finite() {
        return None;
    }
    let bits = v.to_bits();
    let neg = bits >> 63 == 1;
    let be = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (man, exp) = if be == 0 { (frac, -1074) } else { (frac | 1u64 << 52, be - 1075) };
    Some((neg, Nat::from_u64(man), exp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::Rng;

    #[test]
    fn machine_reals_round_trip() {
        let mut rng = Rng(21);
        for _ in 0..20000 {
            let v = f64::from_bits(rng.next_u64());
            if !v.is_finite() {
                continue;
            }
            let (neg, man, exp) = f64_to_dyadic(v).unwrap();
            let back = dyadic_to_f64(neg, &man, exp);
            assert!(back == v && (v != 0.0 || !neg || back == 0.0), "{v:e}");
        }
        assert_eq!(dyadic_to_f64(false, &Nat::from_u64(1), 1024), f64::INFINITY);
        assert_eq!(dyadic_to_f64(true, &Nat::from_u64(1), -1075), -0.0);
        assert_eq!(dyadic_to_f64(false, &Nat::from_u64(3), -1076), f64::from_bits(1));
    }

    #[test]
    fn ratios_round_to_nearest() {
        let third = ratio_to_f64(false, &Nat::from_u64(1), &Nat::from_u64(3));
        assert_eq!(third, 1.0 / 3.0);
        let mut rng = Rng(22);
        for _ in 0..20000 {
            let a = rng.next_u64() >> rng.below(60);
            let b = (rng.next_u64() >> rng.below(60)) | 1;
            // an IEEE division of two exactly representable values is correctly rounded
            if a < 1 << 53 && b < 1 << 53 {
                assert_eq!(ratio_to_f64(false, &Nat::from_u64(a), &Nat::from_u64(b)), a as f64 / b as f64);
            }
        }
        // halfway between two machine reals: ties to even
        let n = Nat::from_u64((1 << 53) + 1);
        assert_eq!(dyadic_to_f64(false, &n, 0), (1u64 << 53) as f64);
        let n = Nat::from_u64((1 << 53) + 3);
        assert_eq!(dyadic_to_f64(false, &n, 0), ((1u64 << 53) + 4) as f64);
        // subnormal quotient
        let tiny = ratio_to_f64(false, &Nat::from_u64(1), &Nat::pow2(1074));
        assert_eq!(tiny, f64::from_bits(1));
    }
}
