//! Greatest common divisors on 64-bit limbs: Lehmer's algorithm (Knuth 4.5.2, algorithm L) with 63-bit leading
//! parts, so that the cofactors fit an i64; one-limb operands finish with the binary gcd. Quadratic, which is what
//! canonical rationals at the benchmark sizes need (a subquadratic half-gcd is a follow-up).
use super::nat::{divrem_1, Nat};
use std::cmp::Ordering;

/// gcd of two machine words by the binary algorithm.
pub fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    let s = (a | b).trailing_zeros();
    a >>= a.trailing_zeros();
    // bounded: every round removes at least one bit from b
    while b != 0 {
        b >>= b.trailing_zeros();
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        b -= a;
    }
    a << s
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    // bounded: Euclid on 128-bit values takes at most about 185 steps
    while b != 0 {
        if a <= u64::MAX as u128 && b <= u64::MAX as u128 {
            return gcd_u64(a as u64, b as u64) as u128;
        }
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn bits(a: &[u64]) -> u64 {
    64 * a.len() as u64 - a.last().map_or(64, |t| t.leading_zeros() as u64)
}

/// floor(a / 2^s) mod 2^64, for the leading part.
fn bits_at(a: &[u64], s: u64) -> u64 {
    let w = (s / 64) as usize;
    let r = (s % 64) as u32;
    let lo = a.get(w).copied().unwrap_or(0) >> r;
    let hi = if r == 0 { 0 } else { a.get(w + 1).copied().unwrap_or(0) << (64 - r) };
    lo | hi
}

fn trim(v: &mut Vec<u64>) {
    while v.last() == Some(&0) {
        v.pop();
    }
}

fn cmp(a: &[u64], b: &[u64]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

/// out = x cx - y cy (assumed non-negative, which Lehmer's conditions guarantee); x.len() >= y.len().
fn lincomb(x: &[u64], cx: u64, y: &[u64], cy: u64, out: &mut Vec<u64>) {
    out.clear();
    out.resize(x.len() + 1, 0);
    let mut carry = 0u64;
    for (o, &v) in out.iter_mut().zip(x) {
        let t = v as u128 * cx as u128 + carry as u128;
        *o = t as u64;
        carry = (t >> 64) as u64;
    }
    out[x.len()] = carry;
    let mut borrow = 0u64; // the high limb of y cy plus the borrow of the subtraction
    for (i, o) in out.iter_mut().enumerate() {
        let yi = y.get(i).copied().unwrap_or(0);
        let t = yi as u128 * cy as u128 + borrow as u128;
        let (d, b) = o.overflowing_sub(t as u64);
        *o = d;
        borrow = (t >> 64) as u64 + b as u64;
    }
    debug_assert_eq!(borrow, 0, "Lehmer combination went negative");
    trim(out);
}

/// gcd(a, b) by Lehmer's algorithm.
pub fn gcd(a: &Nat, b: &Nat) -> Nat {
    let (mut a, mut b) = if cmp(&a.limbs, &b.limbs) != Ordering::Less {
        (a.limbs.clone(), b.limbs.clone())
    } else {
        (b.limbs.clone(), a.limbs.clone())
    };
    let (mut na, mut nb) = (Vec::new(), Vec::new());
    // bounded: every round either performs one exact Euclidean step or a multi-step Lehmer reduction, each of which
    // shrinks a by at least one bit
    loop {
        if super::stopped() {
            return Nat::from_limbs(a);
        }
        if b.is_empty() {
            return Nat::from_limbs(a);
        }
        if a.len() <= 2 {
            let x = a[0] as u128 | (a.get(1).copied().unwrap_or(0) as u128) << 64;
            let y = b[0] as u128 | (b.get(1).copied().unwrap_or(0) as u128) << 64;
            return Nat::from_u128(gcd_u128(x, y));
        }
        if b.len() == 1 {
            let (_, r) = divrem_1(&a, b[0]);
            return Nat::from_u64(gcd_u64(b[0], r));
        }
        let s = bits(&a) - 63;
        let mut u = bits_at(&a, s) as i128;
        let mut v = bits_at(&b, s) as i128;
        let (mut ca, mut cb, mut cc, mut cd) = (1i128, 0i128, 0i128, 1i128);
        // bounded: v strictly decreases
        loop {
            // Knuth's test with floor quotients; any non-positive operand ends the run (conservatively)
            let (n1, d1, n2, d2) = (u + ca, v + cc, u + cb, v + cd);
            if d1 <= 0 || d2 <= 0 || n1 < 0 || n2 < 0 {
                break;
            }
            let q = n1 / d1;
            if q == 0 || q != n2 / d2 {
                break;
            }
            let t = ca - q * cc;
            ca = cc;
            cc = t;
            let t = cb - q * cd;
            cb = cd;
            cd = t;
            let t = u - q * v;
            u = v;
            v = t;
        }
        if cb == 0 {
            let (_, r) = Nat::from_limbs(std::mem::take(&mut a)).divrem(&Nat::from_limbs(b.clone()));
            a = std::mem::take(&mut b);
            b = r.limbs;
            continue;
        }
        // new a = ca a + cb b, new b = cc a + cd b; the coefficients of each pair have opposite signs
        let comb = |x: &[u64], y: &[u64], c1: i128, c2: i128, out: &mut Vec<u64>| {
            // one coefficient may be zero, so the sign test is on the second
            if c2 <= 0 {
                debug_assert!(c1 >= 0);
                lincomb(x, c1 as u64, y, (-c2) as u64, out);
            } else {
                debug_assert!(c1 <= 0);
                // y (-c2) - x (-c1) with -c2 >= 0; y may be shorter than x, so pad through lincomb's zero extension
                let mut ext = y.to_vec();
                ext.resize(x.len(), 0);
                lincomb(&ext, c2 as u64, x, (-c1) as u64, out);
            }
        };
        comb(&a, &b, ca, cb, &mut na);
        comb(&a, &b, cc, cd, &mut nb);
        std::mem::swap(&mut a, &mut na);
        std::mem::swap(&mut b, &mut nb);
        if cmp(&a, &b) == Ordering::Less {
            std::mem::swap(&mut a, &mut b);
        }
    }
}

/// Is the natural number one?
pub fn is_one(x: &Nat) -> bool {
    x.limbs.len() == 1 && x.limbs[0] == 1
}

/// x / d for a d known to divide x.
pub fn div_exact(x: &Nat, d: &Nat) -> Nat {
    if is_one(d) {
        return x.clone();
    }
    if d.limbs.len() == 1 {
        let (q, r) = x.divrem_u64(d.limbs[0]);
        debug_assert!(r == 0 || super::stopped(), "inexact division");
        return q;
    }
    let (q, r) = x.divrem(d);
    debug_assert!(r.is_zero() || super::stopped(), "inexact division");
    q
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::Rng;

    fn euclid(a: &Nat, b: &Nat) -> Nat {
        let (mut a, mut b) = (a.clone(), b.clone());
        while !b.is_zero() {
            let r = a.divrem(&b).1;
            a = b;
            b = r;
        }
        a
    }

    fn rand_nat(rng: &mut Rng, limbs: usize) -> Nat {
        Nat::from_limbs((0..limbs).map(|_| rng.next_u64()).collect())
    }

    #[test]
    fn lehmer_case() {
        let a = Nat::from_limbs(vec![5330967020343627744, 3203708557852282895, 11185367546950094332, 5439371820728081192, 2214584242224559977, 2512165]);
        let b = Nat::from_limbs(vec![359694291947937728, 12893085181525801053, 15197621682162859624, 8878044204072853807, 316049373543449753]);
        assert_eq!(gcd(&a, &b), euclid(&a, &b));
    }

    #[test]
    fn lehmer_agrees_with_euclid() {
        let mut rng = Rng(11);
        for &(la, lb, lg) in &[(1, 1, 0), (3, 2, 0), (5, 5, 1), (8, 3, 2), (20, 20, 4), (60, 40, 7), (130, 129, 30), (300, 300, 1), (400, 37, 2)] {
            for _ in 0..4 {
                let g = if lg == 0 { Nat::from_u64(1) } else { rand_nat(&mut rng, lg) };
                let a = rand_nat(&mut rng, la).mul(&g);
                let b = rand_nat(&mut rng, lb).mul(&g);
                let want = euclid(&a, &b);
                assert_eq!(gcd(&a, &b), want, "gcd {la} {lb} {lg}");
                assert_eq!(gcd(&b, &a), want);
                assert_eq!(gcd(&a, &Nat::zero()), a);
            }
        }
        // consecutive Fibonacci numbers: the worst case of the quotient sequence
        let (mut f0, mut f1) = (Nat::from_u64(1), Nat::from_u64(1));
        for _ in 0..3000 {
            let f2 = f0.add(&f1);
            f0 = f1;
            f1 = f2;
        }
        assert!(is_one(&gcd(&f0, &f1)));
        assert_eq!(gcd_u64(48, 18), 6);
        assert_eq!(gcd_u64(0, 5), 5);
    }
}
