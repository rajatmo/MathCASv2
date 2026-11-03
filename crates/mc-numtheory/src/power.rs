//! Perfect powers: exact k-th roots, valuations, and the decomposition n = m^e with e maximal, fast also for numbers
//! with millions of digits (`primitive_root(prime(10^6)^(10^6))`, benchmark N8).
//!
//! **Exact roots** (decision J.6) are computed 2-adically: for odd n and odd k, the inverse k-th root y of n modulo
//! 2^w is lifted by Newton's iteration y <- y + y (1 - n y^k) / k, doubling the correct bits each step, and
//! m = n y^(k-1) mod 2^w is the only candidate below 2^w; squares use the inverse square root
//! y <- y (3 - n y^2) / 2. Everything works modulo 2^w with w about bits(n)/k, so a root costs a few products of
//! that size, and one full-size power confirms it.
//!
//! **Detection** (decision J.7): n is a k-th power for a prime k only if, for primes q = 1 (mod k), the residue
//! n mod q is a k-th power residue (n^((q-1)/k) = 1 or n = 0 mod q); and for k = 2, (n/q) != -1. Each test passes a
//! non-power with probability about 1/k, so each prime k up to bits(n)/40 gets enough moduli q that a false pass is
//! rare (20 for k = 2 down to 2 for large k). All residues n mod q come from one remainder tree. For larger k the
//! root has fewer than 40 bits: the candidate round(2^(log2(n)/k)) is computed in floating point and checked modulo
//! two 61-bit primes. Every surviving k is confirmed by an exact root, and the decomposition repeats on the root
//! with only the surviving k (the prime factors of the root's exponent divide the original exponent).
use crate::budget::Budget;
use crate::prime::{mod_u64, to_u64};
use crate::sieve::small_primes;
use crate::word::{iroot as iroot_u64, is_prime_u64, jacobi, pow_mod, Mont64};
use mc_arith::num::Nat;
use mc_eval::Res;

/// x mod 2^w.
fn low(x: &Nat, w: u64) -> Nat {
    x.low_bits(w)
}

/// x^e mod 2^w.
fn pow_low(x: &Nat, e: u64, w: u64) -> Nat {
    let mut r = Nat::from_u64(1);
    let mut i = 64 - e.leading_zeros();
    while i > 0 {
        i -= 1;
        r = low(&r.square(), w);
        if (e >> i) & 1 == 1 {
            r = low(&r.mul(x), w);
        }
    }
    r
}

/// k^(-1) mod 2^w for odd k.
fn inv_word_low(k: u64, w: u64) -> Nat {
    // Newton: z <- z (2 - k z) doubles the correct bits; z = k is correct to 3 bits
    let mut z = Nat::from_u64(k);
    let mut bits = 3;
    let kn = Nat::from_u64(k);
    let two = Nat::from_u64(2);
    while bits < w {
        bits = (2 * bits).min(w);
        let m = Nat::pow2(bits);
        let kz = low(&kn.mul(&z), bits);
        // 2 - k z mod 2^bits
        let t = if kz <= two { two.sub(&kz) } else { m.sub(&kz).add(&two).low_bits(bits) };
        z = low(&z.mul(&t), bits);
    }
    low(&z, w)
}

/// (2^w - x) mod 2^w.
fn neg_low(x: &Nat, w: u64) -> Nat {
    if x.is_zero() {
        Nat::zero()
    } else {
        Nat::pow2(w).sub(&low(x, w))
    }
}

/// The k-th root of an odd n modulo 2^w (k odd), the only one below 2^w.
fn odd_root_low(n: &Nat, k: u64, w: u64) -> Nat {
    // y = n^(-1/k): y <- y + y (1 - n y^k) / k, from y = 1 (correct modulo 2)
    let kinv = inv_word_low(k, w + 1);
    let mut y = Nat::from_u64(1);
    let mut bits = 1;
    while bits < w {
        bits = (2 * bits).min(w);
        let t = low(&low(n, bits).mul(&pow_low(&y, k, bits)), bits);
        // 1 - n y^k = -(t - 1)
        let e = neg_low(&t.sub_u64(1), bits);
        let corr = low(&low(&y.mul(&e), bits).mul(&kinv), bits);
        y = low(&y.add(&corr), bits);
    }
    low(&low(n, w).mul(&pow_low(&y, k - 1, w)), w)
}

/// A square root of an odd n = 1 (mod 8) modulo 2^w, w >= 3.
fn odd_sqrt_low(n: &Nat, w: u64) -> Nat {
    // y = n^(-1/2): y <- y (3 - n y^2) / 2 takes the precision from j to 2j - 2 bits (j >= 3)
    let mut y = Nat::from_u64(1);
    let mut bits = 3;
    while bits < w {
        bits = (2 * bits - 2).min(w);
        let wb = bits + 1;
        let t = low(&low(n, wb).mul(&y.square()), wb);
        // 3 - t mod 2^wb, then times y, halved (exactly: it is even)
        let three = Nat::from_u64(3);
        let d = if t <= three { three.sub(&t) } else { Nat::pow2(wb).sub(&t).add(&three).low_bits(wb) };
        y = low(&y.mul(&d), wb).shr(1);
        y = low(&y, bits);
    }
    low(&low(n, w).mul(&y), w)
}

/// m with m^k = n, if n is a perfect k-th power (k >= 2).
pub fn exact_root(n: &Nat, k: u64) -> Option<Nat> {
    if n.is_zero() || *n == Nat::from_u64(1) {
        return Some(n.clone());
    }
    if let Some(v) = to_u64(n) {
        let r = iroot_u64(v, k.min(64) as u32);
        return (k < 64 && r.checked_pow(k as u32) == Some(v)).then(|| Nat::from_u64(r));
    }
    if k >= n.bits() {
        return None;
    }
    let t = n.trailing_zeros();
    if !t.is_multiple_of(k) {
        return None;
    }
    let odd = n.shr(t);
    // the root of the odd part has at most ceil(bits/k) bits
    let rb = odd.bits().div_ceil(k);
    let m = if odd == Nat::from_u64(1) {
        Nat::from_u64(1)
    } else if k == 2 {
        if odd.limbs[0] % 8 != 1 {
            return None;
        }
        let w = rb + 2;
        let x = low(&odd_sqrt_low(&odd, w), w - 1);
        let y = Nat::pow2(w - 1).sub(&x);
        if x < y {
            x
        } else {
            y
        }
    } else if k % 2 == 1 {
        odd_root_low(&odd, k, rb + 1)
    } else {
        // even composite k: by its prime factors
        let r = exact_root(&odd, 2)?;
        exact_root(&r, k / 2)?
    };
    if m.bits() > rb {
        return None;
    }
    // a cheap check before the full power
    let q = (1u64 << 61) - 1;
    if pow_mod(mod_u64(&m, q), k, q) != mod_u64(&odd, q) {
        return None;
    }
    (m.pow_u64(k) == odd).then(|| m.shl(t / k))
}

/// The exponent of the prime (or any) p > 1 in n > 0 and the cofactor: n = p^v c with p not dividing c. Divides by
/// p, p^2, p^4, ... while they divide, then by the same powers downwards, so the number of big divisions is
/// logarithmic in v.
pub fn valuation(n: &Nat, p: &Nat, b: &mut dyn Budget) -> Res<(u64, Nat)> {
    let mut n = n.clone();
    let mut v = 0u64;
    let mut powers: Vec<Nat> = vec![];
    let mut cur = p.clone();
    loop {
        b.charge(crate::budget::limb_work(n.len().min(64), 1) + n.len() as u64 / 256)?;
        let (q, r) = n.divrem(&cur);
        if !r.is_zero() {
            break;
        }
        n = q;
        v += 1u64 << powers.len();
        let next = cur.square();
        powers.push(std::mem::replace(&mut cur, next));
        if cur > n {
            break;
        }
    }
    while let Some(pw) = powers.pop() {
        b.charge(n.len() as u64 / 256 + 1)?;
        let (q, r) = n.divrem(&pw);
        if r.is_zero() {
            n = q;
            v += 1u64 << powers.len();
        }
    }
    Ok((v, n))
}

/// The residues n mod q for word moduli q, by a remainder tree (one division of n by the product of all moduli,
/// then down a product tree).
pub fn residues(n: &Nat, qs: &[u64], b: &mut dyn Budget) -> Res<Vec<u64>> {
    if qs.len() <= 4 || n.len() <= 8 {
        return Ok(qs.iter().map(|&q| mod_u64(n, q)).collect());
    }
    // levels[0] are the moduli, levels[i + 1][j] = levels[i][2j] levels[i][2j + 1]
    let mut levels: Vec<Vec<Nat>> = vec![qs.iter().map(|&q| Nat::from_u64(q)).collect()];
    while levels.last().unwrap().len() > 1 {
        let prev = levels.last().unwrap();
        let next: Vec<Nat> =
            prev.chunks(2).map(|c| if c.len() == 2 { c[0].mul(&c[1]) } else { c[0].clone() }).collect();
        b.charge(next.iter().map(|x| x.len() as u64).sum::<u64>() / 64 + 1)?;
        levels.push(next);
    }
    let top = levels.last().unwrap()[0].clone();
    let mut rem = vec![if *n >= top { n.divrem(&top).1 } else { n.clone() }];
    b.charge(n.len() as u64 / 16 + 1)?;
    for level in levels.iter().rev().skip(1) {
        let next: Vec<Nat> = level
            .iter()
            .enumerate()
            .map(|(j, m)| {
                let r = &rem[j / 2];
                if r < m {
                    r.clone()
                } else {
                    r.divrem(m).1
                }
            })
            .collect();
        b.charge(next.iter().map(|x| x.len() as u64).sum::<u64>() / 32 + 1)?;
        rem = next;
    }
    Ok(rem.iter().map(|r| to_u64(r).expect("a residue below a word modulus")).collect())
}

/// The primes up to `limit`, as words.
fn primes_upto(limit: u64) -> Vec<u64> {
    if limit < 1 << 16 {
        small_primes().iter().map(|&p| p as u64).take_while(|&p| p <= limit).collect()
    } else {
        crate::sieve::primes_below(limit + 1).into_iter().map(|p| p as u64).collect()
    }
}

/// How many moduli test k-th powerness for the prime k.
fn tests_for(k: u64) -> usize {
    match k {
        2 => 20,
        3 => 12,
        5 => 8,
        7 => 7,
        11..=29 => 5,
        31..=997 => 3,
        _ => 2,
    }
}

/// Two 61-bit primes for checking small candidate roots.
const CHECK: [u64; 2] = [(1 << 61) - 1, 2_305_843_009_213_693_921];

/// The prime exponents k for which n may be a perfect k-th power (every k for which it is, and rarely others).
fn candidates(n: &Nat, ks: Option<&[u64]>, b: &mut dyn Budget) -> Res<Vec<u64>> {
    let bits = n.bits();
    let all;
    let ks = match ks {
        Some(k) => k,
        None => {
            all = primes_upto(bits);
            &all
        }
    };
    let filter_max = bits / 40;
    // moduli for the filtered exponents: q = 1 (mod 2k), prime; the residues of the 61-bit check primes too
    let mut moduli: Vec<u64> = CHECK.to_vec();
    let mut tests: Vec<(u64, usize, usize)> = vec![];
    for &k in ks.iter().filter(|&&k| k <= filter_max) {
        let start = moduli.len();
        let want = tests_for(k);
        if k == 2 {
            moduli.extend(small_primes()[1..=want].iter().map(|&p| p as u64));
        } else {
            let mut j = 1u64;
            while moduli.len() - start < want {
                let q = 2 * j * k + 1;
                if is_prime_u64(q) {
                    moduli.push(q);
                }
                j += 1;
            }
        }
        tests.push((k, start, moduli.len()));
        if tests.len().is_multiple_of(1024) {
            b.charge(64)?;
        }
    }
    let res = residues(n, &moduli, b)?;
    let mut out = vec![];
    for &(k, s, e) in &tests {
        let pass = (s..e).all(|i| {
            let (q, r) = (moduli[i], res[i]);
            r == 0 || if k == 2 { jacobi(r, q) == 1 } else { pow_mod(r, (q - 1) / k, q) == 1 }
        });
        if pass {
            out.push(k);
        }
    }
    // large k: the root has fewer than 41 bits, compute it in floating point. log2(n) = (bits - 64) + log2(top),
    // and the exponent is split into an integer and a fractional part before dividing by k
    let top = n.shr(bits.saturating_sub(64)).limbs.first().copied().unwrap_or(0);
    let lt = (top as f64).log2();
    let base = bits.saturating_sub(64);
    let checks = CHECK.map(Mont64::new);
    for (i, &k) in ks.iter().filter(|&&k| k > filter_max).enumerate() {
        if i % 4096 == 4095 {
            b.charge(64)?;
        }
        let (qi, rem) = (base / k, base % k);
        let f = (rem as f64 + lt) / k as f64;
        let e = qi as f64 + f;
        if e > 60.0 {
            // cannot happen for k > bits / 40 except at the boundary; let the exact test decide
            out.push(k);
            continue;
        }
        // the float root is within 0.05 of the true one: a neighbour needs testing only near a half
        let x = e.exp2();
        let m0 = x.round() as u64;
        let other = if x > m0 as f64 { m0 + 1 } else { m0.saturating_sub(1) };
        let is_root =
            |m: u64| m >= 2 && checks.iter().zip(&res[..2]).all(|(c, &r)| c.from_mont(c.pow(c.to_mont(m), k)) == r);
        if is_root(m0) || ((x - m0 as f64).abs() > 0.4 && is_root(other)) {
            out.push(k);
        }
    }
    Ok(out)
}

/// n = m^e with e maximal (n >= 2), as (m, e).
pub fn perfect_power(n: &Nat, b: &mut dyn Budget) -> Res<(Nat, u64)> {
    let mut m = n.clone();
    let mut e = 1u64;
    if let Some(v) = to_u64(&m) {
        return Ok(perfect_power_u64(v));
    }
    // a power of two, or an even number whose exponent must divide v2(n)
    let t = m.trailing_zeros();
    if t > 0 && m.shr(t) == Nat::from_u64(1) {
        return Ok((Nat::from_u64(2), t));
    }
    let mut ks: Option<Vec<u64>> = if t > 0 { Some(prime_factors_u64(t)) } else { None };
    loop {
        if let Some(v) = to_u64(&m) {
            let (r, f) = perfect_power_u64(v);
            return Ok((r, e * f));
        }
        let cands = match &ks {
            Some(k) if k.is_empty() => return Ok((m, e)),
            Some(k) => candidates(&m, Some(k), b)?,
            None => candidates(&m, None, b)?,
        };
        let mut found = None;
        for &k in &cands {
            b.charge(m.len() as u64 / 64 + 1)?;
            if let Some(r) = exact_root(&m, k) {
                found = Some((k, r));
                break;
            }
        }
        match found {
            None => return Ok((m, e)),
            Some((k, r)) => {
                m = r;
                e *= k;
                // only exponents that may still divide the root's exponent
                ks = Some(cands.into_iter().filter(|&c| c >= k).collect());
            }
        }
    }
}

/// The distinct prime factors of a word, by trial division (small words only).
fn prime_factors_u64(mut v: u64) -> Vec<u64> {
    let mut out = vec![];
    let mut d = 2;
    while d * d <= v {
        if v.is_multiple_of(d) {
            out.push(d);
            while v.is_multiple_of(d) {
                v /= d;
            }
        }
        d += 1;
    }
    if v > 1 {
        out.push(v);
    }
    out
}

/// n = m^e with e maximal, for a word n >= 2.
pub fn perfect_power_u64(n: u64) -> (Nat, u64) {
    let (mut m, mut e) = (n, 1u64);
    // bounded: every step takes a root of at least half the bits
    'outer: loop {
        for k in [2u32, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61] {
            if (1u64 << k.min(63)) > m && k > 1 {
                break;
            }
            let r = iroot_u64(m, k);
            if r >= 2 && r.checked_pow(k) == Some(m) {
                m = r;
                e *= k as u64;
                continue 'outer;
            }
        }
        return (Nat::from_u64(m), e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;

    #[test]
    fn exact_roots() {
        let p = Nat::from_u64(15_485_863);
        for k in [2u64, 3, 4, 5, 6, 7, 10, 64, 101, 1000] {
            let n = p.pow_u64(k);
            assert_eq!(exact_root(&n, k), Some(p.clone()), "k = {k}");
            assert_eq!(exact_root(&n.add_u64(2), k), None);
            if k > 2 {
                assert_eq!(exact_root(&n, k - 1), None);
            }
        }
        // even roots and powers of two inside
        let m = Nat::from_u64(12).pow_u64(37);
        assert_eq!(exact_root(&m, 37), Some(Nat::from_u64(12)));
        assert_eq!(exact_root(&m.pow_u64(2), 74), Some(Nat::from_u64(12)));
        assert_eq!(exact_root(&Nat::from_u64(1 << 40), 5), Some(Nat::from_u64(256)));
        assert_eq!(exact_root(&Nat::from_u64(17).pow_u64(8).shl(3), 2), None);
    }

    #[test]
    fn valuations() {
        let n = Nat::from_u64(3).pow_u64(1000).mul(&Nat::from_u64(10));
        let (v, c) = valuation(&n, &Nat::from_u64(3), &mut Unlimited).unwrap();
        assert_eq!((v, c), (1000, Nat::from_u64(10)));
        let (v, c) = valuation(&Nat::from_u64(10), &Nat::from_u64(3), &mut Unlimited).unwrap();
        assert_eq!((v, c), (0, Nat::from_u64(10)));
        let n = Nat::from_u64(7).pow_u64(12345);
        assert_eq!(valuation(&n, &Nat::from_u64(7), &mut Unlimited).unwrap(), (12345, Nat::from_u64(1)));
    }

    #[test]
    fn remainder_tree() {
        let n = Nat::from_u64(3).pow_u64(5000).add_u64(17);
        let qs: Vec<u64> = (0..300).map(|i| 1_000_003 + 2 * i).collect();
        let got = residues(&n, &qs, &mut Unlimited).unwrap();
        for (q, r) in qs.iter().zip(got) {
            assert_eq!(r, mod_u64(&n, *q));
        }
    }

    #[test]
    fn decompositions() {
        let cases: Vec<(Nat, Nat, u64)> = vec![
            (Nat::from_u64(15_485_863).pow_u64(1000), Nat::from_u64(15_485_863), 1000),
            (Nat::from_u64(6).pow_u64(77), Nat::from_u64(6), 77),
            (Nat::from_u64(2).pow_u64(300), Nat::from_u64(2), 300),
            (Nat::from_u64(10).pow_u64(10), Nat::from_u64(10), 10),
            (Nat::from_u64(1_000_003).pow_u64(3 * 7 * 11 * 13), Nat::from_u64(1_000_003), 3 * 7 * 11 * 13),
            (
                Nat::from_u64(999_983).pow_u64(2).mul(&Nat::from_u64(1_000_003).pow_u64(2)).pow_u64(50),
                Nat::from_u64(999_983 * 1_000_003),
                100,
            ),
        ];
        for (n, m, e) in cases {
            assert_eq!(perfect_power(&n, &mut Unlimited).unwrap(), (m, e));
        }
        let n = Nat::from_u64(15_485_863).pow_u64(1000).add_u64(1);
        assert_eq!(perfect_power(&n, &mut Unlimited).unwrap(), (n.clone(), 1));
        assert_eq!(perfect_power_u64(1 << 62), (Nat::from_u64(2), 62));
        assert_eq!(perfect_power_u64(3_486_784_401), (Nat::from_u64(3), 20));
        assert_eq!(perfect_power_u64(1_000_000_007), (Nat::from_u64(1_000_000_007), 1));
    }
}
