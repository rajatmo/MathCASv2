//! Arithmetic modulo a word-sized prime and dense univariate polynomials over it: the images that the modular
//! algorithms (gcd, resultant, factorisation) compute with.
//!
//! A [`UPoly`] is a coefficient vector, index = degree, without trailing zeros (the zero polynomial is empty).
use crate::work::{Meter, WorkResult};

/// A prime modulus below 2^63 with its arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fp {
    /// the prime
    pub p: u64,
}

impl Fp {
    /// The field of a prime p < 2^63.
    pub fn new(p: u64) -> Fp {
        debug_assert!(p > 1 && p < 1 << 63);
        Fp { p }
    }
    /// a + b
    #[inline]
    pub fn add(self, a: u64, b: u64) -> u64 {
        let s = a + b;
        if s >= self.p {
            s - self.p
        } else {
            s
        }
    }
    /// a - b
    #[inline]
    pub fn sub(self, a: u64, b: u64) -> u64 {
        if a >= b {
            a - b
        } else {
            a + self.p - b
        }
    }
    /// -a
    #[inline]
    pub fn neg(self, a: u64) -> u64 {
        if a == 0 {
            0
        } else {
            self.p - a
        }
    }
    /// a b
    #[inline]
    pub fn mul(self, a: u64, b: u64) -> u64 {
        if self.p < 1 << 32 {
            // the product fits in a word: a cheaper division
            (a * b) % self.p
        } else {
            ((a as u128 * b as u128) % self.p as u128) as u64
        }
    }
    /// a^e
    pub fn pow(self, mut a: u64, mut e: u64) -> u64 {
        let mut r = 1 % self.p;
        while e > 0 {
            if e & 1 == 1 {
                r = self.mul(r, a);
            }
            a = self.mul(a, a);
            e >>= 1;
        }
        r
    }
    /// a^-1 (a nonzero)
    pub fn inv(self, a: u64) -> u64 {
        // extended Euclid on (a, p)
        let (mut r0, mut r1) = (self.p as i128, a as i128);
        let (mut s0, mut s1) = (0i128, 1i128);
        while r1 != 0 {
            let q = r0 / r1;
            (r0, r1) = (r1, r0 - q * r1);
            (s0, s1) = (s1, s0 - q * s1);
        }
        debug_assert_eq!(r0, 1, "not invertible");
        s0.rem_euclid(self.p as i128) as u64
    }
    /// The residue of a signed value.
    pub fn of_i64(self, v: i64) -> u64 {
        (v as i128).rem_euclid(self.p as i128) as u64
    }
}

/// Deterministic Miller–Rabin for 64-bit n (the first twelve primes are a proven base set below 3.3 * 10^24).
pub fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    const BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    for &b in &BASES {
        if n.is_multiple_of(b) {
            return n == b;
        }
    }
    let f = Fp::new(n);
    let s = (n - 1).trailing_zeros();
    let d = (n - 1) >> s;
    'base: for &b in &BASES {
        let mut x = f.pow(b, d);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = f.mul(x, x);
            if x == n - 1 {
                continue 'base;
            }
        }
        return false;
    }
    true
}

/// The primes below `start`, descending.
pub struct Primes(u64);

impl Primes {
    /// Primes just below 2^62 (large: few are needed, and unlucky ones are rare).
    pub fn large() -> Primes {
        Primes(1 << 62)
    }
    /// Primes below `start`.
    pub fn below(start: u64) -> Primes {
        Primes(start)
    }
}

impl Iterator for Primes {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        while self.0 > 2 {
            self.0 -= 1;
            if is_prime(self.0) {
                return Some(self.0);
            }
        }
        None
    }
}

/// A dense polynomial mod p (index = degree, no trailing zeros).
pub type UPoly = Vec<u64>;

/// Remove trailing zeros.
pub fn trim(a: &mut UPoly) {
    while a.last() == Some(&0) {
        a.pop();
    }
}

/// The degree (-1 for zero).
pub fn deg(a: &[u64]) -> isize {
    a.len() as isize - 1
}

/// a + b
pub fn add(f: Fp, a: &[u64], b: &[u64]) -> UPoly {
    let mut r: UPoly = (0..a.len().max(b.len()))
        .map(|i| f.add(a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0)))
        .collect();
    trim(&mut r);
    r
}

/// a - b
pub fn sub(f: Fp, a: &[u64], b: &[u64]) -> UPoly {
    let mut r: UPoly = (0..a.len().max(b.len()))
        .map(|i| f.sub(a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0)))
        .collect();
    trim(&mut r);
    r
}

/// c a
pub fn scale(f: Fp, a: &[u64], c: u64) -> UPoly {
    if c == 0 {
        return vec![];
    }
    a.iter().map(|&x| f.mul(x, c)).collect()
}

/// a b (schoolbook with delayed reduction for primes below 2^32).
pub fn mul(f: Fp, a: &[u64], b: &[u64]) -> UPoly {
    if a.is_empty() || b.is_empty() {
        return vec![];
    }
    let n = a.len() + b.len() - 1;
    let mut r = vec![0u64; n];
    if f.p < 1 << 31 {
        // products below 2^62: accumulate up to 4 before reducing
        let mut acc = vec![0u64; n];
        for (i, &x) in a.iter().enumerate() {
            if x == 0 {
                continue;
            }
            for (j, &y) in b.iter().enumerate() {
                let s = acc[i + j] + x * y;
                acc[i + j] = if s >= 1 << 63 { s % f.p } else { s };
            }
        }
        for (o, s) in r.iter_mut().zip(acc) {
            *o = s % f.p;
        }
    } else {
        for (i, &x) in a.iter().enumerate() {
            if x == 0 {
                continue;
            }
            for (j, &y) in b.iter().enumerate() {
                r[i + j] = f.add(r[i + j], f.mul(x, y));
            }
        }
    }
    trim(&mut r);
    r
}

/// (quotient, remainder) of a by a nonzero b.
pub fn divrem(f: Fp, a: &[u64], b: &[u64]) -> (UPoly, UPoly) {
    // invariant: callers divide by nonzero polynomials (a gcd step or a factor)
    assert!(!b.is_empty(), "division by zero polynomial");
    if a.len() < b.len() {
        return (vec![], a.to_vec());
    }
    let mut r = a.to_vec();
    let db = b.len() - 1;
    let inv = f.inv(b[db]);
    let mut q = vec![0u64; a.len() - db];
    for k in (0..q.len()).rev() {
        let c = f.mul(r[k + db], inv);
        q[k] = c;
        if c != 0 {
            for (j, &y) in b.iter().enumerate() {
                r[k + j] = f.sub(r[k + j], f.mul(c, y));
            }
        }
    }
    r.truncate(db);
    trim(&mut r);
    trim(&mut q);
    (q, r)
}

/// a mod b
pub fn rem(f: Fp, a: &[u64], b: &[u64]) -> UPoly {
    divrem(f, a, b).1
}

/// a / lc(a) (a nonzero)
pub fn monic(f: Fp, a: &[u64]) -> UPoly {
    match a.last() {
        None => vec![],
        Some(&1) => a.to_vec(),
        Some(&l) => scale(f, a, f.inv(l)),
    }
}

/// The monic gcd (zero only for two zeros).
pub fn gcd(f: Fp, a: &[u64], b: &[u64], m: &mut Meter) -> WorkResult<UPoly> {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    while !b.is_empty() {
        m.step((a.len() * b.len()) as u64 / 4 + 1)?;
        let r = rem(f, &a, &b);
        a = b;
        b = r;
    }
    Ok(monic(f, &a))
}

/// (g, s, t) with g = gcd(a, b) monic and s a + t b = g.
pub fn xgcd(f: Fp, a: &[u64], b: &[u64]) -> (UPoly, UPoly, UPoly) {
    let (mut r0, mut r1) = (a.to_vec(), b.to_vec());
    let (mut s0, mut s1): (UPoly, UPoly) = (vec![1], vec![]);
    let (mut t0, mut t1): (UPoly, UPoly) = (vec![], vec![1]);
    while !r1.is_empty() {
        let (q, r) = divrem(f, &r0, &r1);
        let s = sub(f, &s0, &mul(f, &q, &s1));
        let t = sub(f, &t0, &mul(f, &q, &t1));
        (r0, r1) = (r1, r);
        (s0, s1) = (s1, s);
        (t0, t1) = (t1, t);
    }
    let Some(&l) = r0.last() else {
        return (vec![], vec![], vec![]);
    };
    let inv = f.inv(l);
    (scale(f, &r0, inv), scale(f, &s0, inv), scale(f, &t0, inv))
}

/// a(x) at x = v.
pub fn eval(f: Fp, a: &[u64], v: u64) -> u64 {
    a.iter().rev().fold(0, |acc, &c| f.add(f.mul(acc, v), c))
}

/// The derivative.
pub fn derivative(f: Fp, a: &[u64]) -> UPoly {
    let mut r: UPoly = a.iter().enumerate().skip(1).map(|(i, &c)| f.mul(c, i as u64 % f.p)).collect();
    trim(&mut r);
    r
}

/// a b mod m
pub fn mulmod(f: Fp, a: &[u64], b: &[u64], m: &[u64]) -> UPoly {
    rem(f, &mul(f, a, b), m)
}

/// a^e mod m, with e as little-endian 64-bit limbs.
pub fn powmod(f: Fp, a: &[u64], e: &[u64], m: &[u64], meter: &mut Meter) -> WorkResult<UPoly> {
    let mut r: UPoly = rem(f, &[1], m);
    let base = rem(f, a, m);
    let bits = e.len() * 64;
    for i in (0..bits).rev() {
        meter.step((m.len() * m.len()) as u64 / 2 + 1)?;
        r = mulmod(f, &r, &r, m);
        if (e[i / 64] >> (i % 64)) & 1 == 1 {
            r = mulmod(f, &r, &base, m);
        }
    }
    Ok(r)
}

/// The resultant of a and b (Euclid over the field).
pub fn resultant(f: Fp, a: &[u64], b: &[u64], m: &mut Meter) -> WorkResult<u64> {
    if a.is_empty() || b.is_empty() {
        return Ok(0);
    }
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    let mut res = 1u64;
    loop {
        let (n, k) = (a.len() - 1, b.len() - 1);
        if k == 0 {
            return Ok(f.mul(res, f.pow(b[0], n as u64)));
        }
        m.step((a.len() * b.len()) as u64 / 4 + 1)?;
        let r = rem(f, &a, &b);
        if r.is_empty() {
            return Ok(0);
        }
        let d = r.len() - 1;
        // res(a, b) = (-1)^(n k) lc(b)^(n - d) res(b, a mod b)
        if (n * k) % 2 == 1 {
            res = f.neg(res);
        }
        res = f.mul(res, f.pow(b[k], (n - d) as u64));
        a = b;
        b = r;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Unlimited;

    #[test]
    fn field_and_polynomials() {
        let f = Fp::new(1_000_000_007);
        assert_eq!(f.mul(f.inv(12345), 12345), 1);
        assert!(is_prime((1 << 61) - 1));
        assert!(!is_prime(1 << 61));
        let p = Primes::large().next().unwrap();
        assert!(is_prime(p) && p < 1 << 62);
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // (x + 1)(x + 2) and (x + 1)(x + 3)
        let a = mul(f, &[1, 1], &[2, 1]);
        let b = mul(f, &[1, 1], &[3, 1]);
        assert_eq!(gcd(f, &a, &b, &mut m).unwrap(), vec![1, 1]);
        let (g, s, t) = xgcd(f, &[2, 1], &[3, 1]);
        assert_eq!(g, vec![1]);
        assert_eq!(add(f, &mul(f, &s, &[2, 1]), &mul(f, &t, &[3, 1])), vec![1]);
        let (q, r) = divrem(f, &a, &[2, 1]);
        assert_eq!((q, r), (vec![1, 1], vec![]));
        // res(x^2 - 2, x - 1) = -1; res(x + 1, x + 3) = 2
        assert_eq!(resultant(f, &[f.of_i64(-2), 0, 1], &[f.of_i64(-1), 1], &mut m).unwrap(), f.of_i64(-1));
        assert_eq!(resultant(f, &[1, 1], &[3, 1], &mut m).unwrap(), 2);
        let small = Fp::new(13);
        assert_eq!(mul(small, &[12, 12], &[12, 12]), vec![1, 2, 1]);
        // x^13 = x mod 13 (Frobenius)
        assert_eq!(powmod(small, &[0, 1], &[13], &[1, 0, 0, 1], &mut m).unwrap(), vec![0, 1]);
    }
}
