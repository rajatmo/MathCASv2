//! Primality of big numbers and the next prime.
//!
//! Below 2^64 the test is deterministic ([`crate::word::is_prime_u64`]). Above, it is the Baillie-PSW test: trial
//! division by the primes below 1000, a strong probable-prime test to base 2 and a strong Lucas probable-prime test
//! with Selfridge's parameters (D the first of 5, -7, 9, -11, ... with Jacobi symbol (D/n) = -1, P = 1,
//! Q = (1 - D)/4). No composite passing both is known, and none exists below 2^64 (decision J.4: `is_prime` is
//! this test, documented as a probable-prime test above 2^64).
use crate::budget::Budget;
use crate::ring::{with_ring, Ring, RingTask};
use crate::sieve::small_primes;
use crate::word::{is_prime_u64, jacobi};
use mc_arith::num::Nat;
use mc_eval::Res;

/// Trial division bound of the big test.
const TRIAL: u32 = 1000;

/// n mod d for a word d, by one pass over the limbs.
pub fn mod_u64(n: &Nat, d: u64) -> u64 {
    let mut r: u128 = 0;
    for &l in n.limbs.iter().rev() {
        r = ((r << 64) | l as u128) % d as u128;
    }
    r as u64
}

/// The value of n if it fits a word.
pub fn to_u64(n: &Nat) -> Option<u64> {
    match n.limbs.as_slice() {
        [] => Some(0),
        [x] => Some(*x),
        _ => None,
    }
}

/// Is n prime? Deterministic below 2^64, Baillie-PSW above.
pub fn is_prime(n: &Nat, b: &mut dyn Budget) -> Res<bool> {
    if let Some(v) = to_u64(n) {
        return Ok(is_prime_u64(v));
    }
    if !n.bit(0) {
        return Ok(false);
    }
    // trial division: residues modulo products of a few primes, one pass over n each
    let ps: Vec<u64> = small_primes().iter().map(|&p| p as u64).take_while(|&p| p < TRIAL as u64).collect();
    let mut i = 1;
    while i < ps.len() {
        let mut q = 1u64;
        let start = i;
        while i < ps.len() && q.checked_mul(ps[i]).is_some() {
            q *= ps[i];
            i += 1;
        }
        let r = mod_u64(n, q);
        if ps[start..i].iter().any(|&p| r.is_multiple_of(p)) {
            return Ok(false);
        }
    }
    with_ring(n, Bpsw { b })
}

/// The base-2 strong probable-prime test and the strong Lucas test on a ring.
struct Bpsw<'b> {
    b: &'b mut dyn Budget,
}

impl RingTask for Bpsw<'_> {
    type Out = Res<bool>;
    fn run<R: Ring>(self, r: &R) -> Res<bool> {
        let b = self.b;
        if !strong_base2(r, b)? {
            return Ok(false);
        }
        strong_lucas(r, b)
    }
}

/// The fuel of one modular multiplication on `l` limbs (in units of a thousand operations).
fn mul_units(l: usize) -> u64 {
    (l as u64 * l as u64 / 256).max(1)
}

/// Is n a strong probable prime to base 2?
fn strong_base2<R: Ring>(r: &R, b: &mut dyn Budget) -> Res<bool> {
    let n = r.modulus();
    let nm1 = n.sub_u64(1);
    let s = nm1.trailing_zeros();
    let d = nm1.shr(s);
    let one = r.one();
    let minus_one = r.sub(&r.zero(), &one);
    // the power by chunks of 64 bits so that the budget is charged as it goes
    let mut x = one.clone();
    let mut i = d.bits();
    let units = mul_units(r.limbs()) * 64;
    while i > 0 {
        let lo = i.saturating_sub(64);
        while i > lo {
            i -= 1;
            x = r.sqr(&x);
            if d.bit(i) {
                x = r.add(&x, &x);
            }
        }
        b.charge(units)?;
    }
    if x == one || x == minus_one {
        return Ok(true);
    }
    for _ in 1..s {
        x = r.sqr(&x);
        if x == minus_one {
            return Ok(true);
        }
        if x == one {
            return Ok(false);
        }
    }
    Ok(false)
}

/// The Jacobi symbol (d / n) for a small odd d (either sign) and an odd big n.
fn jacobi_small(d: i64, n: &Nat) -> i32 {
    let a = d.unsigned_abs();
    let n4 = n.limbs[0] % 4;
    // (-1 / n) = (-1)^((n-1)/2)
    let sign_neg = if d < 0 && n4 == 3 { -1 } else { 1 };
    // reciprocity for odd a: (a / n) = (n / a) (-1)^((a-1)/2 (n-1)/2)
    let flip = if a % 4 == 3 && n4 == 3 { -1 } else { 1 };
    sign_neg * flip * jacobi(mod_u64(n, a), a)
}

/// The element of a small signed integer.
fn signed<R: Ring>(r: &R, v: i64) -> R::E {
    let m = r.elem_u64(v.unsigned_abs());
    if v < 0 {
        r.sub(&r.zero(), &m)
    } else {
        m
    }
}

/// The strong Lucas probable-prime test with Selfridge's parameters.
fn strong_lucas<R: Ring>(r: &R, b: &mut dyn Budget) -> Res<bool> {
    let n = r.modulus();
    // D = 5, -7, 9, -11, ...; a square n never gives (D/n) = -1, so test squareness once the search runs long
    let mut d: i64 = 5;
    let mut tries = 0;
    loop {
        let j = jacobi_small(d, n);
        if j == -1 {
            break;
        }
        if j == 0 && mod_u64(n, d.unsigned_abs()) == 0 && Nat::from_u64(d.unsigned_abs()) != *n {
            return Ok(false);
        }
        tries += 1;
        if tries == 20 {
            let s = n.isqrt();
            if s.square() == *n {
                return Ok(false);
            }
        }
        d = if d > 0 { -(d + 2) } else { -d + 2 };
    }
    let q = (1 - d) / 4;
    let (de, qe) = (signed(r, d), signed(r, q));
    let np1 = n.add_u64(1);
    let s = np1.trailing_zeros();
    let k = np1.shr(s);
    // U_1 = 1, V_1 = P = 1, Q^1
    let (mut u, mut v, mut qk) = (r.one(), r.one(), qe.clone());
    let mut i = k.bits() - 1;
    let units = mul_units(r.limbs()) * 64 * 4;
    let mut steps = 0;
    while i > 0 {
        i -= 1;
        // doubling: U_2k = U_k V_k, V_2k = V_k^2 - 2 Q^k
        u = r.mul(&u, &v);
        v = r.sub(&r.sqr(&v), &r.add(&qk, &qk));
        qk = r.sqr(&qk);
        if k.bit(i) {
            // U_(k+1) = (P U + V)/2, V_(k+1) = (D U + P V)/2 with P = 1
            let nu = r.half(&r.add(&u, &v));
            let nv = r.half(&r.add(&r.mul(&de, &u), &v));
            u = nu;
            v = nv;
            qk = r.mul(&qk, &qe);
        }
        steps += 1;
        if steps % 64 == 0 {
            b.charge(units)?;
        }
    }
    if r.is_zero(&u) || r.is_zero(&v) {
        return Ok(true);
    }
    for _ in 1..s {
        v = r.sub(&r.sqr(&v), &r.add(&qk, &qk));
        qk = r.sqr(&qk);
        if r.is_zero(&v) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Window of candidates sieved at once by [`next_prime`] for big numbers.
const WINDOW: u64 = 1 << 14;

/// The smallest prime greater than n.
pub fn next_prime(n: &Nat, b: &mut dyn Budget) -> Res<Nat> {
    if let Some(v) = to_u64(n) {
        if v < 2 {
            return Ok(Nat::from_u64(2));
        }
        // candidates above v; the prime gaps below 2^64 are below 1600
        let mut c = v;
        while let Some(next) = c.checked_add(1) {
            c = next;
            if is_prime_u64(c) {
                return Ok(Nat::from_u64(c));
            }
            if c.is_multiple_of(1024) {
                b.charge(1)?;
            }
        }
        // past 2^64 - 59, the largest 64-bit prime: continue with big numbers
    }
    // windows [start, start + WINDOW) of odd candidates, sieved by the primes below 2^16
    let primes = small_primes();
    let mut start = n.add_u64(1);
    if !start.bit(0) {
        start = start.add_u64(1);
    }
    loop {
        let mut comp = vec![false; WINDOW as usize];
        for &p in &primes[1..] {
            let p = p as u64;
            let rem = mod_u64(&start, p);
            // first offset j with start + 2j = 0 mod p: 2j = -rem mod p
            let need = (p - rem) % p;
            let mut j = if need.is_multiple_of(2) { need / 2 } else { (need + p) / 2 };
            while j < WINDOW {
                comp[j as usize] = true;
                j += p;
            }
        }
        b.charge(primes.len() as u64 / 64 + 1)?;
        for (j, &c) in comp.iter().enumerate() {
            if c {
                continue;
            }
            let cand = start.add_u64(2 * j as u64);
            if is_prime(&cand, b)? {
                return Ok(cand);
            }
        }
        start = start.add_u64(2 * WINDOW);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;

    fn nat(s: &str) -> Nat {
        Nat::from_decimal(s).unwrap()
    }

    #[test]
    fn big_primes_and_composites() {
        assert!(is_prime(&nat("18446744073709551629"), &mut Unlimited).unwrap()); // the first prime above 2^64
                                                                                  // Mersenne primes, on Montgomery rings of 2, 9 and 10 limbs and on the Barrett ring (2203 bits)
        for e in [89u64, 107, 127, 521, 607, 2203] {
            let m = Nat::pow2(e).sub_u64(1);
            assert!(is_prime(&m, &mut Unlimited).unwrap(), "2^{e} - 1");
            assert!(!is_prime(&m.add_u64(2), &mut Unlimited).unwrap(), "2^{e} + 1");
        }
        for e in [67u64, 101, 257, 1277, 2207] {
            assert!(!is_prime(&Nat::pow2(e).sub_u64(1), &mut Unlimited).unwrap(), "2^{e} - 1");
        }
        let composites = [
            "18446744073709551617",                    // 2^64 + 1 = 274177 * 67280421310721
            "340282366920938463463374607431768211457", // 2^128 + 1
            // a strong pseudoprime to bases 2..31 (Arnault-style products are caught by the Lucas test)
            "3317044064679887385961981",
        ];
        for c in composites {
            assert!(!is_prime(&nat(c), &mut Unlimited).unwrap(), "{c}");
        }
        // 2^128 - 159 is prime; squares must stop the Lucas parameter search
        assert!(is_prime(&nat("340282366920938463463374607431768211297"), &mut Unlimited).unwrap());
        let p = nat("18446744073709551557");
        assert!(!is_prime(&p.mul(&p), &mut Unlimited).unwrap());
        // products of two primes near 2^64 (n - 1 smooth or not)
        let q = nat("18446744073709551533");
        assert!(!is_prime(&p.mul(&q), &mut Unlimited).unwrap());
    }

    #[test]
    fn next_primes() {
        assert_eq!(next_prime(&Nat::zero(), &mut Unlimited).unwrap(), Nat::from_u64(2));
        assert_eq!(next_prime(&Nat::from_u64(2), &mut Unlimited).unwrap(), Nat::from_u64(3));
        assert_eq!(next_prime(&Nat::from_u64(89), &mut Unlimited).unwrap(), Nat::from_u64(97));
        assert_eq!(next_prime(&nat("18446744073709551557"), &mut Unlimited).unwrap(), nat("18446744073709551629"));
        assert_eq!(next_prime(&nat("100000000000000000000"), &mut Unlimited).unwrap(), nat("100000000000000000039"));
        for s in [
            "200000000000000000000000000000",
            "5000000000000000000000000000000000000000000000",
            "123456789012345678901234567890123456789012345678901234567890",
        ] {
            let n = nat(s);
            let p = next_prime(&n, &mut Unlimited).unwrap();
            assert!(mr_prime(&p), "{s}");
            let mut c = n.add_u64(1);
            while c < p {
                assert!(!mr_prime(&c));
                c = c.add_u64(1);
            }
        }
    }

    /// Miller-Rabin by num-bigint with the prime bases below 100.
    fn mr_prime(n: &Nat) -> bool {
        use num_bigint::BigUint;
        let m = BigUint::parse_bytes(n.to_decimal().as_bytes(), 10).unwrap();
        let one = BigUint::from(1u32);
        let nm1 = &m - &one;
        let s = nm1.trailing_zeros().unwrap();
        let d = &nm1 >> s;
        crate::sieve::primes_below(100).iter().all(|&a| {
            let a = BigUint::from(a);
            if &a % &m == BigUint::from(0u32) {
                return true;
            }
            let mut x = a.modpow(&d, &m);
            if x == one || x == nm1 {
                return true;
            }
            for _ in 1..s {
                x = &x * &x % &m;
                if x == nm1 {
                    return true;
                }
            }
            false
        })
    }
}
