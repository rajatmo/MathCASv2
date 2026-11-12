//! Multiplication by number-theoretic transform over three primes, with CRT reconstruction.
//!
//! The operands' 64-bit limbs are the convolution coefficients directly. A coefficient of the product is at most
//! `n (2^64 - 1)^2 < n 2^128`, and the three primes lie in (2^61, 2^62), so their product exceeds 2^183 and the
//! CRT recovers every coefficient exactly for n < 2^55 limbs. Three transforms of length N cost less than the two
//! transforms of length 2N that 32-bit coefficients with two primes would need.
//!
//! Butterflies follow Harvey ("Faster arithmetic for number-theoretic transforms", 2014): residues are kept lazily in
//! [0, 2p) or [0, 4p), which p < 2^62 keeps below 2^64, and twiddle factors w carry a precomputed
//! w' = floor(w 2^64 / p) so that x w mod p costs one high and two low products (Shoup's trick). Conditional
//! subtractions are branch-free selects: their outcome is data-dependent and unpredictable. The pointwise products
//! of two varying residues use Montgomery multiplication (R = 2^64).
//!
//! The forward transform is a recursive decimation in frequency (natural order in, bit-reversed out); the inverse is
//! a recursive decimation in time (bit-reversed in, natural out), so no bit-reversal permutation is needed. The
//! recursion splits the array in halves, so once a half fits in cache all its levels run in cache.
use std::cell::RefCell;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub struct Prime {
    pub p: u64,
    /// p^-1 mod 2^64
    pinv: u64,
    /// R^2 mod p, to enter Montgomery form
    r2: u64,
    /// a generator of the multiplicative group
    g: u64,
    /// 2^k divides p - 1
    k: u32,
}

impl Prime {
    fn new(p: u64, g: u64, k: u32) -> Prime {
        // Newton iteration for the inverse modulo 2^64: each step doubles the correct low bits (p odd: 1 bit at start)
        let mut inv: u64 = 1;
        for _ in 0..6 {
            inv = inv.wrapping_mul(2u64.wrapping_sub(p.wrapping_mul(inv)));
        }
        let r = ((1u128 << 64) % p as u128) as u64;
        let r2 = ((r as u128 * r as u128) % p as u128) as u64;
        Prime { p, pinv: inv, r2, g, k }
    }
    /// Montgomery reduction of t < p 2^64: returns t / 2^64 mod p in [0, p). With m = lo(t) p^-1 the low words of t
    /// and m p agree, so (t - m p) / 2^64 is the difference of the high words, in (-p, p).
    #[inline(always)]
    fn redc(&self, t: u128) -> u64 {
        let m = (t as u64).wrapping_mul(self.pinv);
        let mp_hi = ((m as u128 * self.p as u128) >> 64) as u64;
        let t_hi = (t >> 64) as u64;
        let (d, borrow) = t_hi.overflowing_sub(mp_hi);
        std::hint::select_unpredictable(borrow, d.wrapping_add(self.p), d)
    }
    #[inline(always)]
    fn mul(&self, a: u64, b: u64) -> u64 {
        self.redc(a as u128 * b as u128)
    }
    fn into_mont(self, a: u64) -> u64 {
        self.mul(a, self.r2)
    }
}

fn mulmod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}

fn powmod(mut a: u64, mut e: u64, m: u64) -> u64 {
    let mut r = 1u64 % m;
    a %= m;
    while e > 0 {
        if e & 1 == 1 {
            r = mulmod(r, a, m);
        }
        a = mulmod(a, a, m);
        e >>= 1;
    }
    r
}

/// x - m if x >= m, else x, without a branch.
#[inline(always)]
fn csub(x: u64, m: u64) -> u64 {
    std::hint::select_unpredictable(x >= m, x.wrapping_sub(m), x)
}

/// x w mod p in [0, 2p) for any x < 2^64, with w' = floor(w 2^64 / p) (Shoup).
#[inline(always)]
fn mul_shoup(x: u64, w: u64, wp: u64, p: u64) -> u64 {
    let q = ((x as u128 * wp as u128) >> 64) as u64;
    x.wrapping_mul(w).wrapping_sub(q.wrapping_mul(p))
}

fn shoup(w: u64, p: u64) -> u64 {
    (((w as u128) << 64) / p as u128) as u64
}

/// Deterministic Miller-Rabin for 64-bit n (the first twelve primes are a proven base set below 3.3 * 10^24).
pub fn is_prime_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    const BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    for &b in &BASES {
        if n.is_multiple_of(b) {
            return n == b;
        }
    }
    let s = (n - 1).trailing_zeros();
    let d = (n - 1) >> s;
    'outer: for &a in &BASES {
        let mut x = powmod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mulmod(x, x, n);
            if x == n - 1 {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

/// The distinct prime factors of n by trial division (n is at most about 2^31 here).
fn prime_factors(mut n: u64) -> Vec<u64> {
    let mut f = vec![];
    let mut d = 2;
    while d * d <= n {
        if n.is_multiple_of(d) {
            f.push(d);
            while n.is_multiple_of(d) {
                n /= d;
            }
        }
        d += 1;
    }
    if n > 1 {
        f.push(n);
    }
    f
}

/// The three largest primes c 2^32 + 1 below 2^62 (so transforms up to length 2^32), with generators. Found by search
/// at first use rather than hard-coded, so the constants are checked by construction.
pub fn primes() -> &'static [Prime; 3] {
    static P: OnceLock<[Prime; 3]> = OnceLock::new();
    P.get_or_init(|| {
        let mut found = vec![];
        let mut c: u64 = (1u64 << 30) - 1;
        while found.len() < 3 {
            let p = (c << 32) + 1;
            if is_prime_u64(p) {
                let mut qs = prime_factors(c);
                if !qs.contains(&2) {
                    qs.push(2);
                }
                let g = (2u64..)
                    .find(|&g| qs.iter().all(|&q| powmod(g, (p - 1) / q, p) != 1))
                    .unwrap();
                found.push(Prime::new(p, g, (p - 1).trailing_zeros()));
            }
            c -= 1;
        }
        [found[0], found[1], found[2]]
    })
}

/// Twiddle tables of one prime, laid out by level: entries [m/2, m) hold (w_m^j, shoup(w_m^j)) for j < m/2 (plain
/// residues), where w_m is a primitive m-th root of unity. The layout does not depend on the largest length, so a
/// table built for a long transform serves every shorter one.
struct Tables {
    fwd: Vec<[u64; 2]>,
    inv: Vec<[u64; 2]>,
}

impl Tables {
    fn grow(&mut self, pr: &Prime, n: usize) {
        if self.fwd.len() >= n {
            return;
        }
        let mut fwd = vec![[0u64; 2]; n];
        let mut inv = vec![[0u64; 2]; n];
        let p = pr.p;
        let mut m = 2;
        while m <= n {
            let w = powmod(pr.g, (p - 1) / m as u64, p);
            let wi = powmod(w, m as u64 - 1, p); // w^-1 = w^(m-1)
            let (mut a, mut b) = (1u64, 1u64);
            for j in 0..m / 2 {
                fwd[m / 2 + j] = [a, shoup(a, p)];
                inv[m / 2 + j] = [b, shoup(b, p)];
                a = mulmod(a, w, p);
                b = mulmod(b, wi, p);
            }
            m *= 2;
        }
        self.fwd = fwd;
        self.inv = inv;
    }
}

thread_local! {
    static TABLES: RefCell<Vec<Tables>> = RefCell::new((0..3).map(|_| Tables { fwd: vec![], inv: vec![] }).collect());
}

/// Below this length the recursion runs the levels iteratively (fewer calls, same memory pattern at that size).
const LEAF: usize = 64;

/// Harvey's DIF butterfly on one level: inputs and outputs in [0, 2p).
#[inline(always)]
fn dif_level(lo: &mut [u64], hi: &mut [u64], w: &[[u64; 2]], p: u64) {
    let p2 = 2 * p;
    for ((x, y), w) in lo.iter_mut().zip(hi.iter_mut()).zip(w) {
        let (a, b) = (*x, *y);
        *x = csub(a + b, p2);
        *y = mul_shoup(a + p2 - b, w[0], w[1], p);
    }
}

/// Harvey's DIT butterfly on one level: inputs in [0, 4p), outputs in [0, 4p).
#[inline(always)]
fn dit_level(lo: &mut [u64], hi: &mut [u64], w: &[[u64; 2]], p: u64) {
    let p2 = 2 * p;
    for ((x, y), w) in lo.iter_mut().zip(hi.iter_mut()).zip(w) {
        let a = csub(*x, p2);
        let t = mul_shoup(*y, w[0], w[1], p);
        *x = a + t;
        *y = a + p2 - t;
    }
}

fn dif(p: u64, a: &mut [u64], tw: &[[u64; 2]]) {
    let n = a.len();
    if n <= LEAF {
        let mut m = n;
        while m >= 2 {
            let h = m / 2;
            for block in a.chunks_exact_mut(m) {
                let (lo, hi) = block.split_at_mut(h);
                dif_level(lo, hi, &tw[h..m], p);
            }
            m = h;
        }
        return;
    }
    let h = n / 2;
    let (lo, hi) = a.split_at_mut(h);
    dif_level(lo, hi, &tw[h..n], p);
    dif(p, lo, tw);
    dif(p, hi, tw);
}

fn dit(p: u64, a: &mut [u64], tw: &[[u64; 2]]) {
    let n = a.len();
    if n <= LEAF {
        let mut m = 2;
        while m <= n {
            let h = m / 2;
            for block in a.chunks_exact_mut(m) {
                let (lo, hi) = block.split_at_mut(h);
                dit_level(lo, hi, &tw[h..m], p);
            }
            m *= 2;
        }
        return;
    }
    let h = n / 2;
    let (lo, hi) = a.split_at_mut(h);
    dit(p, lo, tw);
    dit(p, hi, tw);
    dit_level(lo, hi, &tw[h..n], p);
}

/// Cyclic convolution of a and b modulo one prime, length n (a power of two, at least a.len() + b.len() - 1);
/// returns the residues of the product's coefficients in [0, p). `square` reuses the transform of a.
fn conv_mod(i: usize, a: &[u64], b: &[u64], n: usize, square: bool) -> Vec<u64> {
    let pr = &primes()[i];
    let p = pr.p;
    assert!(n.trailing_zeros() <= pr.k, "transform length beyond the primes' 2-adicity");
    // a limb is below 2^64 < 8p: two conditional subtractions bring it into [0, 2p)
    let load = |x: &[u64]| -> Vec<u64> {
        let mut f = vec![0u64; n];
        for (d, &v) in f.iter_mut().zip(x) {
            *d = csub(csub(v, 4 * p), 2 * p);
        }
        f
    };
    TABLES.with(|t| {
        let mut t = t.borrow_mut();
        let tab = &mut t[i];
        tab.grow(pr, n);
        let mut fa = load(a);
        dif(p, &mut fa, &tab.fwd);
        // Montgomery products of residues below 2p (4p^2 < p 2^64): results in [0, p), with a factor R^-1
        if square {
            for x in fa.iter_mut() {
                *x = pr.mul(*x, *x);
            }
        } else {
            let mut fb = load(b);
            dif(p, &mut fb, &tab.fwd);
            for (x, y) in fa.iter_mut().zip(&fb) {
                *x = pr.mul(*x, *y);
            }
        }
        dit(p, &mut fa, &tab.inv);
        // the inverse transform added a factor n: multiplying by k = R / n mod p leaves the plain coefficient
        let ninv = powmod(n as u64 % p, p - 2, p);
        let k = mulmod(((1u128 << 64) % p as u128) as u64, ninv, p);
        let kp = shoup(k, p);
        for x in fa.iter_mut() {
            *x = csub(mul_shoup(*x, k, kp, p), p);
        }
        fa
    })
}

/// a - b mod p for a, b in [0, p).
#[inline(always)]
fn subm(a: u64, b: u64, p: u64) -> u64 {
    csub(a + p - b, p)
}

/// CRT constants for Garner's reconstruction, in Montgomery form where they multiply residues.
struct Crt {
    /// p1^-1 mod p2, times R
    c12: u64,
    /// p1 mod p3, times R
    p1_mod3: u64,
    /// (p1 p2)^-1 mod p3, times R
    c123: u64,
    /// p1 p2 as two words
    p12: u128,
}

fn crt() -> &'static Crt {
    static C: OnceLock<Crt> = OnceLock::new();
    C.get_or_init(|| {
        let [a, b, c] = primes();
        let inv = |x: u64, m: u64| powmod(x % m, m - 2, m);
        Crt {
            c12: b.into_mont(inv(a.p, b.p)),
            p1_mod3: c.into_mont(a.p % c.p),
            c123: c.into_mont(inv(mulmod(a.p, b.p, c.p), c.p)),
            p12: a.p as u128 * b.p as u128,
        }
    })
}

/// Product of two limb slices (little-endian, any lengths >= 1); returns exactly a.len() + b.len() limbs.
pub fn mul(a: &[u64], b: &[u64]) -> Vec<u64> {
    let len = a.len() + b.len();
    // la + lb - 1 coefficients; the top limb of the result comes from the carries
    let n = (len - 1).next_power_of_two();
    let square = std::ptr::eq(a, b);
    let mut res: Vec<Vec<u64>> = Vec::with_capacity(3);
    for i in 0..3 {
        if crate::stopped() {
            return vec![0; len];
        }
        res.push(conv_mod(i, a, b, n, square));
    }
    let [p1, p2, p3] = primes();
    let k = crt();
    let mut out = vec![0u64; len];
    let mut carry: u128 = 0;
    for (i, o) in out.iter_mut().enumerate() {
        // the last limb (index la + lb - 1) has no coefficient of its own when n = la + lb - 1
        let (r1, r2, r3) = if i < n { (res[0][i], res[1][i], res[2][i]) } else { (0, 0, 0) };
        // x = r1 + p1 t2 + p1 p2 t3 with t2 = (r2 - r1) / p1 mod p2, t3 = (r3 - r1 - p1 t2) / (p1 p2) mod p3
        // the primes decrease, so r1 < p1 < 2 p2 < 2 p3 and t2 < p2 < 2 p3
        let r1_2 = csub(r1, p2.p);
        let t2 = p2.mul(subm(r2, r1_2, p2.p), k.c12);
        let r1_3 = csub(r1, p3.p);
        let t2_3 = csub(t2, p3.p);
        let s3 = csub(r1_3 + p3.mul(t2_3, k.p1_mod3), p3.p);
        let t3 = p3.mul(subm(r3, s3, p3.p), k.c123);
        let low = r1 as u128 + p1.p as u128 * t2 as u128; // < 2^127
        // p12 * t3 as three words
        let lo_part = (k.p12 as u64) as u128 * t3 as u128;
        let hi_part = (k.p12 >> 64) * t3 as u128;
        let w0 = lo_part as u64 as u128;
        let w1 = (lo_part >> 64) + (hi_part as u64 as u128);
        let w2 = hi_part >> 64;
        // coefficient c = (w2:w1:w0) + low, then add the running carry
        let s0 = w0 + (low as u64 as u128) + (carry as u64 as u128);
        *o = s0 as u64;
        let s1 = w1 + (low >> 64) + (carry >> 64) + (s0 >> 64);
        let s2 = w2 + (s1 >> 64);
        carry = (s1 as u64 as u128) | (s2 << 64);
    }
    debug_assert_eq!(carry, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primes_are_ntt_friendly() {
        for pr in primes() {
            assert!(pr.p > 1 << 61 && pr.p < 1 << 62);
            assert!(is_prime_u64(pr.p));
            assert!(pr.k >= 32);
            // g generates: g^((p-1)/2) = -1
            assert_eq!(powmod(pr.g, (pr.p - 1) / 2, pr.p), pr.p - 1);
            assert_eq!(pr.p.wrapping_mul(pr.pinv), 1);
        }
    }

    #[test]
    fn small_products_match_schoolbook() {
        let mut rng = crate::Rng(7);
        for la in [1usize, 2, 3, 7, 64, 65, 200] {
            for lb in [1usize, 5, 64, 129] {
                let a: Vec<u64> = (0..la).map(|_| rng.next_u64()).collect();
                let b: Vec<u64> = (0..lb).map(|_| rng.next_u64()).collect();
                let mut want = vec![0u64; la + lb];
                crate::nat::mul_school(&a, &b, &mut want);
                assert_eq!(mul(&a, &b), want, "{la}x{lb}");
            }
        }
        // all-ones limbs: the largest coefficients
        let a = vec![u64::MAX; 300];
        let mut want = vec![0u64; 600];
        crate::nat::mul_school(&a, &a, &mut want);
        assert_eq!(mul(&a, &a), want);
    }
}
