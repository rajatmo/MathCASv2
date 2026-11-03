//! Just enough natural-number arithmetic on 64-bit limbs (least significant first, no high zero limbs; empty is
//! zero) to build integer and rational literals exactly and to print big integers. Literals are short, so the
//! simple quadratic algorithms are adequate; `mc-arith` supplies the fast ones for evaluation.
use std::cmp::Ordering;

pub type Nat = Vec<u64>;

fn trim(mut a: Nat) -> Nat {
    while a.last() == Some(&0) {
        a.pop();
    }
    a
}

/// `a * m + c` in place.
fn mul_add_small(a: &mut Nat, m: u64, c: u64) {
    let mut carry = c as u128;
    for limb in a.iter_mut() {
        let x = *limb as u128 * m as u128 + carry;
        *limb = x as u64;
        carry = x >> 64;
    }
    if carry != 0 {
        a.push(carry as u64);
    }
}

/// Parses digits of `radix` (2 to 36, all valid).
pub fn from_radix(digits: &str, radix: u32) -> Nat {
    let mut a = Nat::new();
    for c in digits.chars() {
        let d = c.to_digit(radix).unwrap_or(0) as u64;
        mul_add_small(&mut a, radix as u64, d);
    }
    trim(a)
}

/// Divides in place by a nonzero `d`, returning the remainder.
fn divrem_small(a: &mut Nat, d: u64) -> u64 {
    let mut rem = 0u128;
    for limb in a.iter_mut().rev() {
        let x = (rem << 64) | *limb as u128;
        *limb = (x / d as u128) as u64;
        rem = x % d as u128;
    }
    while a.last() == Some(&0) {
        a.pop();
    }
    rem as u64
}

/// Decimal text of a natural number.
pub fn to_decimal(a: &[u64]) -> String {
    const CHUNK: u64 = 10_000_000_000_000_000_000; // 10^19
    let mut a = trim(a.to_vec());
    if a.is_empty() {
        return "0".to_string();
    }
    let mut chunks = Vec::new();
    while !a.is_empty() {
        chunks.push(divrem_small(&mut a, CHUNK));
    }
    let mut s = chunks.pop().map(|c| c.to_string()).unwrap_or_default();
    for c in chunks.iter().rev() {
        s.push_str(&format!("{c:019}"));
    }
    s
}

pub fn cmp(a: &[u64], b: &[u64]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

pub fn is_one(a: &[u64]) -> bool {
    a == [1]
}

/// `a - b` for `a >= b`.
fn sub(a: &[u64], b: &[u64]) -> Nat {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0u64;
    for (i, &x) in a.iter().enumerate() {
        let y = b.get(i).copied().unwrap_or(0);
        let (d1, o1) = x.overflowing_sub(y);
        let (d2, o2) = d1.overflowing_sub(borrow);
        out.push(d2);
        borrow = (o1 || o2) as u64;
    }
    trim(out)
}

fn trailing_zeros(a: &[u64]) -> u64 {
    let mut n = 0;
    for &l in a {
        if l == 0 {
            n += 64;
        } else {
            return n + l.trailing_zeros() as u64;
        }
    }
    n
}

fn shr(a: &[u64], k: u64) -> Nat {
    let limbs = (k / 64) as usize;
    let bits = (k % 64) as u32;
    if limbs >= a.len() {
        return Nat::new();
    }
    let src = &a[limbs..];
    let mut out = Vec::with_capacity(src.len());
    for i in 0..src.len() {
        let lo = src[i] >> bits;
        let hi = if bits == 0 { 0 } else { src.get(i + 1).map_or(0, |&h| h << (64 - bits)) };
        out.push(lo | hi);
    }
    trim(out)
}

fn shl(a: &[u64], k: u64) -> Nat {
    let limbs = (k / 64) as usize;
    let bits = (k % 64) as u32;
    let mut out = vec![0u64; limbs];
    let mut carry = 0u64;
    for &l in a {
        out.push((l << bits) | carry);
        carry = if bits == 0 { 0 } else { l >> (64 - bits) };
    }
    out.push(carry);
    trim(out)
}

/// Greatest common divisor (binary algorithm).
pub fn gcd(a: &[u64], b: &[u64]) -> Nat {
    let (mut a, mut b) = (trim(a.to_vec()), trim(b.to_vec()));
    if a.is_empty() {
        return b;
    }
    if b.is_empty() {
        return a;
    }
    let za = trailing_zeros(&a);
    let zb = trailing_zeros(&b);
    let shift = za.min(zb);
    a = shr(&a, za);
    // Each round removes at least one bit from the larger operand, so the loop runs at most bits(a) + bits(b) times.
    loop {
        b = shr(&b, trailing_zeros(&b));
        if cmp(&a, &b) == Ordering::Greater {
            std::mem::swap(&mut a, &mut b);
        }
        b = sub(&b, &a);
        if b.is_empty() {
            break;
        }
    }
    shl(&a, shift)
}

/// The quotient `a / b` for nonzero `b` (shift-and-subtract long division).
pub fn div(a: &[u64], b: &[u64]) -> Nat {
    let a = trim(a.to_vec());
    let bits = a.len() as u64 * 64;
    let mut q = vec![0u64; a.len()];
    let mut r = Nat::new();
    for i in (0..bits).rev() {
        r = shl(&r, 1);
        if (a[(i / 64) as usize] >> (i % 64)) & 1 == 1 {
            if r.is_empty() {
                r.push(1);
            } else {
                r[0] |= 1;
            }
        }
        if cmp(&r, b) != Ordering::Less {
            r = sub(&r, b);
            q[(i / 64) as usize] |= 1 << (i % 64);
        }
    }
    trim(q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigUint;

    fn big(a: &[u64]) -> BigUint {
        let mut words = Vec::new();
        for &l in a {
            words.push(l as u32);
            words.push((l >> 32) as u32);
        }
        BigUint::new(words)
    }

    #[test]
    fn against_oracle() {
        let samples = [
            "0",
            "1",
            "18446744073709551615",
            "18446744073709551616",
            "123456789012345678901234567890123456789012345678901234567890",
            "340282366920938463463374607431768211456",
        ];
        for s in samples {
            let a = from_radix(s, 10);
            assert_eq!(big(&a).to_string(), s);
            assert_eq!(to_decimal(&a), s);
        }
        assert_eq!(big(&from_radix("ffffffffffffffffffff", 16)).to_str_radix(16), "ffffffffffffffffffff");
        let x = from_radix("123456789012345678901234567890", 10);
        let y = from_radix("987654321098765432109876543210", 10);
        let g = gcd(&x, &y);
        assert_eq!(big(&g), num_integer_gcd(big(&x), big(&y)));
        assert_eq!(big(&div(&y, &g)), big(&y) / big(&g));
        assert_eq!(big(&div(&from_radix("1000", 10), &from_radix("7", 10))), BigUint::from(142u32));
    }

    fn num_integer_gcd(mut a: BigUint, mut b: BigUint) -> BigUint {
        while b != BigUint::from(0u32) {
            let r = &a % &b;
            a = b;
            b = r;
        }
        a
    }
}
