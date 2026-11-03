//! Digits, lengths and exponents of integers in a base.
//!
//! Conversion to a base splits by the powers base^(2^j) (divide and conquer), so that the digits of a number with
//! millions of digits cost a few multiplications' worth of Newton divisions; base 10 uses `mc-arith`'s decimal
//! conversion and powers of two read the bits.
use crate::budget::Budget;
use crate::factor::factor;
use crate::power::valuation;
use crate::prime::to_u64;
use mc_arith::num::Nat;
use mc_eval::Res;

/// The digits of n in base b >= 2, most significant first (`[0]` for zero).
pub fn digits(n: &Nat, base: u64, b: &mut dyn Budget) -> Res<Vec<u64>> {
    if n.is_zero() {
        return Ok(vec![0]);
    }
    if base == 10 {
        b.charge(n.len() as u64 / 8 + 1)?;
        return Ok(n.to_decimal().bytes().map(|c| (c - b'0') as u64).collect());
    }
    if base.is_power_of_two() {
        let w = base.trailing_zeros() as u64;
        let nd = n.bits().div_ceil(w);
        let mut out = Vec::with_capacity(nd as usize);
        for i in (0..nd).rev() {
            let mut d = 0u64;
            for j in (0..w).rev() {
                d = d << 1 | n.bit(i * w + j) as u64;
            }
            out.push(d);
        }
        b.charge(nd / 1024 + 1)?;
        return Ok(out);
    }
    // powers[j] = base^(2^j) while it is at most n
    let mut powers = vec![Nat::from_u64(base)];
    while powers.last().unwrap().bits() * 2 <= n.bits() + 1 {
        let next = powers.last().unwrap().square();
        powers.push(next);
    }
    let mut out = vec![];
    split(n, base, &powers, powers.len(), false, &mut out, b)?;
    Ok(out)
}

/// Append the digits of n < base^(2^level) to `out`, padded to 2^level digits when `pad`.
fn split(
    n: &Nat,
    base: u64,
    powers: &[Nat],
    level: usize,
    pad: bool,
    out: &mut Vec<u64>,
    b: &mut dyn Budget,
) -> Res<()> {
    if level <= 4 || n.len() <= 2 {
        // schoolbook: repeated division by the base
        let mut ds = vec![];
        let mut x = n.clone();
        while !x.is_zero() {
            let (q, r) = x.divrem_u64(base);
            ds.push(r);
            x = q;
        }
        if pad {
            ds.resize(1usize << level, 0);
        }
        b.charge(ds.len() as u64 * n.len() as u64 / 256 + 1)?;
        out.extend(ds.iter().rev());
        return Ok(());
    }
    let p = &powers[level - 1];
    let (hi, lo) = n.divrem(p);
    b.charge(n.len() as u64 / 16 + 1)?;
    if hi.is_zero() && !pad {
        return split(&lo, base, powers, level - 1, false, out, b);
    }
    split(&hi, base, powers, level - 1, pad, out, b)?;
    split(&lo, base, powers, level - 1, true, out, b)
}

/// The number of digits of n in base b >= 2 (zero for zero).
pub fn length(n: &Nat, base: u64) -> u64 {
    if n.is_zero() {
        return 0;
    }
    if base.is_power_of_two() {
        return n.bits().div_ceil(base.trailing_zeros() as u64);
    }
    // a lower bound from the bit length, then exact comparisons with powers of the base
    let est = ((n.bits() - 1) as f64 / (base as f64).log2()).floor() as u64;
    let mut len = est.saturating_sub(1).max(1);
    let mut p = Nat::from_u64(base).pow_u64(len);
    // bounded: the estimate is within two of the length
    while p <= *n {
        p = p.mul_u64(base);
        len += 1;
    }
    len
}

/// The largest k with base^k dividing n > 0 (base >= 2).
pub fn exponent(n: &Nat, base: &Nat, b: &mut dyn Budget) -> Res<u64> {
    if *base == Nat::from_u64(2) {
        return Ok(n.trailing_zeros());
    }
    let fs = factor(base, b)?;
    let mut best = u64::MAX;
    for (p, e) in fs {
        let v = if p == Nat::from_u64(2) { n.trailing_zeros() } else { valuation(n, &p, b)?.0 };
        best = best.min(v / e);
        if best == 0 {
            break;
        }
    }
    Ok(best)
}

/// The number whose digits in base b are `ds`, most significant first (digits may exceed the base).
pub fn from_digits(ds: &[Nat], base: &Nat, b: &mut dyn Budget) -> Res<Nat> {
    if ds.is_empty() {
        return Ok(Nat::zero());
    }
    if ds.len() == 1 {
        return Ok(ds[0].clone());
    }
    // divide and conquer: value = high base^len(low) + low
    let mid = ds.len() / 2;
    let hi = from_digits(&ds[..mid], base, b)?;
    let lo = from_digits(&ds[mid..], base, b)?;
    let scale = match to_u64(base) {
        Some(v) => Nat::pow(v, (ds.len() - mid) as u64),
        None => base.pow_u64((ds.len() - mid) as u64),
    };
    b.charge(scale.len() as u64 / 16 + 1)?;
    Ok(hi.mul(&scale).add(&lo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;

    fn naive(n: &Nat, base: u64) -> Vec<u64> {
        let mut ds = vec![];
        let mut x = n.clone();
        while !x.is_zero() {
            let (q, r) = x.divrem_u64(base);
            ds.push(r);
            x = q;
        }
        ds.reverse();
        if ds.is_empty() {
            ds.push(0);
        }
        ds
    }

    #[test]
    fn digits_in_bases() {
        let u = &mut Unlimited;
        let xs =
            [Nat::zero(), Nat::from_u64(7), Nat::from_u64(1000), Nat::from_u64(3).pow_u64(2000), Nat::pow(10, 300)];
        for n in &xs {
            for base in [2u64, 3, 5, 7, 8, 10, 16, 36, 1000, 1 << 40, u64::MAX] {
                assert_eq!(digits(n, base, u).unwrap(), naive(n, base), "base {base}");
                assert_eq!(length(n, base), if n.is_zero() { 0 } else { naive(n, base).len() as u64 });
                let ds: Vec<Nat> = naive(n, base).into_iter().map(Nat::from_u64).collect();
                assert_eq!(from_digits(&ds, &Nat::from_u64(base), u).unwrap(), *n);
            }
        }
        assert_eq!(length(&Nat::pow(10, 5000), 10), 5001);
        assert_eq!(length(&Nat::pow(10, 5000).sub_u64(1), 10), 5000);
    }

    #[test]
    fn exponents() {
        let u = &mut Unlimited;
        let n = Nat::from_u64(2).pow_u64(10).mul(&Nat::from_u64(5).pow_u64(7));
        assert_eq!(exponent(&n, &Nat::from_u64(10), u).unwrap(), 7);
        assert_eq!(exponent(&n, &Nat::from_u64(2), u).unwrap(), 10);
        assert_eq!(exponent(&n, &Nat::from_u64(4), u).unwrap(), 5);
        assert_eq!(exponent(&n, &Nat::from_u64(3), u).unwrap(), 0);
        assert_eq!(exponent(&n, &Nat::from_u64(20), u).unwrap(), 5);
    }
}
