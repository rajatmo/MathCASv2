//! Number theory on machine words: Montgomery arithmetic modulo an odd 64-bit number, deterministic primality below
//! 2^64, integer roots, the Jacobi symbol and modular inverses.

/// Montgomery arithmetic modulo an odd `n` (R = 2^64). Residues are kept in Montgomery form `x R mod n`, in `0..n`.
#[derive(Clone, Copy, Debug)]
pub struct Mont64 {
    /// the modulus (odd)
    pub n: u64,
    /// n^(-1) mod 2^64
    inv: u64,
    /// R^2 mod n
    r2: u64,
    /// R mod n, the Montgomery form of one
    one: u64,
}

impl Mont64 {
    /// The arithmetic modulo an odd `n > 1`.
    pub fn new(n: u64) -> Mont64 {
        debug_assert!(n & 1 == 1 && n > 1);
        // Newton's iteration doubles the correct low bits of the inverse: 5 steps from 3 bits (n is its own
        // inverse modulo 8) reach 96 bits
        let mut inv = n;
        for _ in 0..5 {
            inv = inv.wrapping_mul(2u64.wrapping_sub(n.wrapping_mul(inv)));
        }
        let one = ((1u128 << 64) % n as u128) as u64;
        let r2 = ((one as u128 * one as u128) % n as u128) as u64;
        Mont64 { n, inv, r2, one }
    }
    /// t R^(-1) mod n for t < n R.
    #[inline]
    fn redc(&self, t: u128) -> u64 {
        // the low words of t and m n agree, so t - m n is exactly (t_hi - (m n)_hi) R
        let m = (t as u64).wrapping_mul(self.inv);
        let mn_hi = ((m as u128 * self.n as u128) >> 64) as u64;
        let (r, borrow) = ((t >> 64) as u64).overflowing_sub(mn_hi);
        if borrow {
            r.wrapping_add(self.n)
        } else {
            r
        }
    }
    /// The Montgomery form of `a` (any word).
    #[inline]
    pub fn to_mont(&self, a: u64) -> u64 {
        self.redc((a % self.n) as u128 * self.r2 as u128)
    }
    /// The ordinary value of a Montgomery form.
    #[inline]
    pub fn from_mont(&self, a: u64) -> u64 {
        self.redc(a as u128)
    }
    /// One, in Montgomery form.
    #[inline]
    pub fn one(&self) -> u64 {
        self.one
    }
    /// a b (Montgomery forms).
    #[inline]
    pub fn mul(&self, a: u64, b: u64) -> u64 {
        self.redc(a as u128 * b as u128)
    }
    /// a + b mod n.
    #[inline]
    pub fn add(&self, a: u64, b: u64) -> u64 {
        let (s, c) = a.overflowing_add(b);
        if c || s >= self.n {
            s.wrapping_sub(self.n)
        } else {
            s
        }
    }
    /// a - b mod n.
    #[inline]
    pub fn sub(&self, a: u64, b: u64) -> u64 {
        let (s, borrow) = a.overflowing_sub(b);
        if borrow {
            s.wrapping_add(self.n)
        } else {
            s
        }
    }
    /// a^e (a in Montgomery form, the result too).
    pub fn pow(&self, a: u64, mut e: u64) -> u64 {
        let mut r = self.one;
        let mut b = a;
        // bounded by the 64 bits of e
        while e > 0 {
            if e & 1 == 1 {
                r = self.mul(r, b);
            }
            b = self.mul(b, b);
            e >>= 1;
        }
        r
    }
}

pub use mc_arith::num::gcd::gcd_u64;

/// a b mod m.
#[inline]
pub fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}

/// a^e mod m (m > 0; 0^0 = 1, reduced mod m).
pub fn pow_mod(a: u64, e: u64, m: u64) -> u64 {
    if m == 1 {
        return 0;
    }
    if m & 1 == 1 {
        let mt = Mont64::new(m);
        return mt.from_mont(mt.pow(mt.to_mont(a), e));
    }
    let (mut r, mut b, mut e) = (1u64, a % m, e);
    while e > 0 {
        if e & 1 == 1 {
            r = mul_mod(r, b, m);
        }
        b = mul_mod(b, b, m);
        e >>= 1;
    }
    r
}

/// Is `n` a strong probable prime to base `a` (in Montgomery form), for odd n = d 2^s + 1?
fn strong_probable_prime(m: &Mont64, a: u64, d: u64, s: u32) -> bool {
    let one = m.one();
    let minus_one = m.n - one;
    let mut x = m.pow(a, d);
    if x == one || x == minus_one {
        return true;
    }
    for _ in 1..s {
        x = m.mul(x, x);
        if x == minus_one {
            return true;
        }
        if x == one {
            return false;
        }
    }
    false
}

/// Small primes for trial division before the strong probable-prime tests.
const SMALL: [u64; 15] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47];

/// Is `n` prime? Deterministic for every 64-bit word: bases 2, 7, 61 below 2^32 (Jaeschke) and Sinclair's seven
/// bases above, which have no strong pseudoprime below 2^64.
pub fn is_prime_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    for p in SMALL {
        if n.is_multiple_of(p) {
            return n == p;
        }
    }
    if n < 47 * 47 {
        return true;
    }
    let s = (n - 1).trailing_zeros();
    let d = (n - 1) >> s;
    let m = Mont64::new(n);
    let bases: &[u64] = if n < 1 << 32 { &[2, 7, 61] } else { &[2, 325, 9375, 28178, 450775, 9780504, 1795265022] };
    bases.iter().all(|&a| {
        let a = a % n;
        a == 0 || strong_probable_prime(&m, m.to_mont(a), d, s)
    })
}

/// floor(sqrt(n)).
pub fn isqrt(n: u64) -> u64 {
    let mut r = (n as f64).sqrt() as u64;
    // the float estimate is within one of the root; correct both ways
    while r.checked_mul(r).is_none_or(|sq| sq > n) {
        r -= 1;
    }
    while (r + 1).checked_mul(r + 1).is_some_and(|sq| sq <= n) {
        r += 1;
    }
    r
}

/// base^e if it fits a word.
pub fn checked_pow(base: u64, e: u32) -> Option<u64> {
    base.checked_pow(e)
}

/// floor(n^(1/k)) for k >= 1.
pub fn iroot(n: u64, k: u32) -> u64 {
    if k == 1 || n < 2 {
        return n;
    }
    if k >= 64 {
        return 1;
    }
    let mut r = (n as f64).powf(1.0 / k as f64) as u64;
    // the float estimate is within a few units; correct both ways
    while r > 0 && checked_pow(r, k).is_none_or(|p| p > n) {
        r -= 1;
    }
    while checked_pow(r + 1, k).is_some_and(|p| p <= n) {
        r += 1;
    }
    r
}

/// The Jacobi symbol (a / n) for odd n > 0.
pub fn jacobi(mut a: u64, mut n: u64) -> i32 {
    debug_assert!(n & 1 == 1);
    a %= n;
    let mut t = 1;
    // Euclid-like: bounded by the bits of n
    while a != 0 {
        let z = a.trailing_zeros();
        a >>= z;
        if z & 1 == 1 && (n % 8 == 3 || n % 8 == 5) {
            t = -t;
        }
        if a % 4 == 3 && n % 4 == 3 {
            t = -t;
        }
        std::mem::swap(&mut a, &mut n);
        a %= n;
    }
    if n == 1 {
        t
    } else {
        0
    }
}

/// a^(-1) mod m, if gcd(a, m) = 1 (m >= 1).
pub fn inverse_mod(a: u64, m: u64) -> Option<u64> {
    if m == 1 {
        return Some(0);
    }
    let (mut r0, mut r1) = (m as i128, (a % m) as i128);
    let (mut t0, mut t1) = (0i128, 1i128);
    // Euclid: bounded by the bits of m
    while r1 != 0 {
        let q = r0 / r1;
        (r0, r1) = (r1, r0 - q * r1);
        (t0, t1) = (t1, t0 - q * t1);
    }
    (r0 == 1).then(|| t0.rem_euclid(m as i128) as u64)
}

/// A square root of a modulo an odd prime p, if a is a quadratic residue (Tonelli-Shanks).
pub fn sqrt_mod(a: u64, p: u64) -> Option<u64> {
    let a = a % p;
    if a == 0 || p == 2 {
        return Some(a);
    }
    if pow_mod(a, (p - 1) / 2, p) != 1 {
        return None;
    }
    if p % 4 == 3 {
        return Some(pow_mod(a, p.div_ceil(4), p));
    }
    let s = (p - 1).trailing_zeros();
    let q = (p - 1) >> s;
    // the least non-residue: found within a few hundred tries for any 64-bit prime
    let mut z = 2;
    while pow_mod(z, (p - 1) / 2, p) != p - 1 {
        z += 1;
    }
    let (mut m, mut c, mut t, mut r) = (s, pow_mod(z, q, p), pow_mod(a, q, p), pow_mod(a, q.div_ceil(2), p));
    while t != 1 {
        let mut i = 0;
        let mut t2 = t;
        while t2 != 1 {
            t2 = mul_mod(t2, t2, p);
            i += 1;
        }
        let b = pow_mod(c, 1 << (m - i - 1), p);
        m = i;
        c = mul_mod(b, b, p);
        t = mul_mod(t, c, p);
        r = mul_mod(r, b, p);
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive_prime(n: u64) -> bool {
        n >= 2 && (2..).take_while(|d| d * d <= n).all(|d| !n.is_multiple_of(d))
    }

    #[test]
    fn primality_agrees_with_trial_division() {
        for n in 0..20_000 {
            assert_eq!(is_prime_u64(n), naive_prime(n), "{n}");
        }
        for n in (1u64 << 40)..(1u64 << 40) + 2000 {
            assert_eq!(is_prime_u64(n), naive_prime(n), "{n}");
        }
    }

    #[test]
    fn known_primes_and_pseudoprimes() {
        assert!(is_prime_u64(18446744073709551557)); // the largest 64-bit prime
        assert!(!is_prime_u64(18446744073709551559));
        // strong pseudoprimes to several small bases
        for n in [3215031751u64, 2152302898747, 3474749660383, 341550071728321, 3825123056546413051] {
            assert!(!is_prime_u64(n), "{n}");
        }
        assert!(!is_prime_u64(4294967297)); // 641 * 6700417
    }

    #[test]
    fn montgomery_matches_u128() {
        for &n in &[3u64, 1_000_000_007, (1 << 63) + 29, u64::MAX] {
            let m = Mont64::new(n);
            for (a, b) in [(2u64, 3u64), (n - 1, n - 1), (123456789, 987654321)] {
                let r = m.from_mont(m.mul(m.to_mont(a), m.to_mont(b)));
                assert_eq!(r, mul_mod(a % n, b % n, n));
            }
            assert_eq!(pow_mod(3, n - 1, n), m.from_mont(m.pow(m.to_mont(3), n - 1)));
        }
    }

    #[test]
    fn roots_jacobi_inverse_sqrt() {
        for n in [0u64, 1, 2, 3, 4, 15, 16, 17, u64::MAX, 1 << 62] {
            let r = isqrt(n);
            assert!(r * r <= n && (r + 1).checked_mul(r + 1).is_none_or(|s| s > n));
            for k in 2..10 {
                let r = iroot(n, k);
                assert!(r.pow(k) <= n && (r + 1).checked_pow(k).is_none_or(|s| s > n));
            }
        }
        assert_eq!(jacobi(1001, 9907), -1);
        assert_eq!(jacobi(19, 45), 1);
        assert_eq!(jacobi(8, 21), -1);
        assert_eq!(jacobi(5, 21), 1);
        assert_eq!(inverse_mod(3, 11), Some(4));
        assert_eq!(inverse_mod(6, 9), None);
        for p in [3u64, 5, 13, 17, 97, 1_000_000_009, 998_244_353] {
            for a in 1..50 {
                if let Some(r) = sqrt_mod(a, p) {
                    assert_eq!(mul_mod(r, r, p), a % p);
                } else {
                    assert_eq!(pow_mod(a, (p - 1) / 2, p), p - 1);
                }
            }
        }
    }
}
