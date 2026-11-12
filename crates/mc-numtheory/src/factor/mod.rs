//! Integer factorisation: `factor(n)` gives the prime factorisation of n >= 1 as sorted (prime, exponent) pairs.
//!
//! The pipeline (decision J.10): trial division by the primes below 2^16 (by residues modulo products of primes,
//! one pass over n per product); perfect powers by [`crate::power::perfect_power`]; then each composite cofactor is
//! split, recursively, by
//!
//! 1. Pollard's rho with Brent's cycle detection on machine words (below 2^64),
//! 2. a short rho run on big numbers (factors up to about 10 digits),
//! 3. ECM with a modest effort (factors up to about a third of the number's digits),
//! 4. SIQS for numbers up to [`siqs::MAX_DIGITS`] digits,
//! 5. ECM with growing bounds beyond, until a factor appears (interruptible through the budget).
//!
//! Every factor found is tested by [`crate::prime::is_prime`] (Baillie-PSW above 2^64), and the multiplicity of each
//! prime in n is found by [`crate::power::valuation`].
pub mod ecm;
pub mod rho;
pub mod siqs;

use crate::budget::Budget;
use crate::power::{perfect_power, valuation};
use crate::prime::{is_prime, mod_u64, to_u64};
use crate::ring::with_ring;
use crate::sieve::small_primes;
use crate::word::is_prime_u64;
use ecm::Ecm;
use mc_arith::num::Nat;
use mc_eval::Res;
use rho::{rho_u64, RhoBig};

/// The prime factorisation of n >= 1: (prime, exponent) pairs by increasing prime.
pub fn factor(n: &Nat, b: &mut dyn Budget) -> Res<Vec<(Nat, u64)>> {
    assert!(!n.is_zero());
    let mut out: Vec<(Nat, u64)> = vec![];
    let mut rest = n.clone();
    // the power of two
    let tz = rest.trailing_zeros();
    if tz > 0 {
        out.push((Nat::from_u64(2), tz));
        rest = rest.shr(tz);
    }
    rest = trial_divide(&rest, &mut out, b)?;
    if rest != Nat::from_u64(1) {
        // the cofactor's primes, then their multiplicities
        let (root, e) = perfect_power(&rest, b)?;
        let mut primes: Vec<Nat> = vec![];
        split_all(&root, &mut primes, b)?;
        primes.sort();
        primes.dedup();
        let mut c = root;
        for p in primes {
            let (v, cof) = valuation(&c, &p, b)?;
            c = cof;
            out.push((p, v * e));
        }
        debug_assert_eq!(c, Nat::from_u64(1));
    }
    out.sort_by(|x, y| x.0.cmp(&y.0));
    Ok(out)
}

/// Divide out the odd primes below 2^16; their exponents go to `out`. Products of primes fitting a word are
/// reduced in one pass over n, and a prime is divided out only when its residue is zero.
fn trial_divide(n: &Nat, out: &mut Vec<(Nat, u64)>, b: &mut dyn Budget) -> Res<Nat> {
    let ps = small_primes();
    let mut rest = n.clone();
    let mut i = 1;
    while i < ps.len() {
        if let Some(v) = to_u64(&rest) {
            // the rest fits a word: finish below sqrt(v) directly
            if v == 1 {
                return Ok(rest);
            }
            let mut v = v;
            while i < ps.len() && (ps[i] as u64) * (ps[i] as u64) <= v {
                let p = ps[i] as u64;
                if v % p == 0 {
                    let mut e = 0;
                    while v % p == 0 {
                        v /= p;
                        e += 1;
                    }
                    out.push((Nat::from_u64(p), e));
                }
                i += 1;
            }
            if v > 1 && (i < ps.len() || v < 1 << 32) {
                // v has no prime factor up to sqrt(v) (or none below 2^16 and v < 2^32): v is prime
                out.push((Nat::from_u64(v), 1));
                return Ok(Nat::from_u64(1));
            }
            return Ok(Nat::from_u64(v));
        }
        let start = i;
        let mut q = 1u64;
        while i < ps.len() && q.checked_mul(ps[i] as u64).is_some() {
            q *= ps[i] as u64;
            i += 1;
        }
        let r = mod_u64(&rest, q);
        for &p in &ps[start..i] {
            let p = p as u64;
            if r.is_multiple_of(p) {
                let (v, c) = valuation(&rest, &Nat::from_u64(p), b)?;
                rest = c;
                out.push((Nat::from_u64(p), v));
            }
        }
        b.charge(rest.len() as u64 / 512 + 1)?;
    }
    Ok(rest)
}

/// The distinct primes of n >= 1 (no factor below 2^16), appended to `out`.
fn split_all(n: &Nat, out: &mut Vec<Nat>, b: &mut dyn Budget) -> Res<()> {
    let one = Nat::from_u64(1);
    let mut work = vec![n.clone()];
    // bounded: every split replaces a number by two smaller ones
    while let Some(m) = work.pop() {
        if m == one {
            continue;
        }
        if is_prime(&m, b)? {
            out.push(m);
            continue;
        }
        let (root, _) = perfect_power(&m, b)?;
        if root != m {
            work.push(root);
            continue;
        }
        let f = split(&m, b)?;
        let g = m.divrem(&f).0;
        work.push(f);
        work.push(g);
    }
    Ok(())
}

/// A nontrivial factor of the composite n (odd, no factor below 2^16, not a perfect power).
fn split(n: &Nat, b: &mut dyn Budget) -> Res<Nat> {
    if let Some(v) = to_u64(n) {
        return Ok(Nat::from_u64(rho_u64(v, b)?));
    }
    let digits = (n.bits() as f64 * std::f64::consts::LOG10_2) as u32 + 1;
    // small factors first: a short rho run and a little ECM, sized well below the cost of the sieve
    if let Some(f) = with_ring(n, RhoBig { steps: 1 << 15, b: &mut *b })? {
        return Ok(f);
    }
    let pre: &[(u64, u64)] = match digits {
        0..=30 => &[],
        31..=45 => &[(2000, 8)],
        46..=60 => &[(2000, 16), (11000, 8)],
        61..=80 => &[(2000, 25), (11000, 40)],
        _ => &[(2000, 25), (11000, 90), (50000, 100)],
    };
    let mut sigma = 6;
    for &(b1, curves) in pre {
        if let Some(f) = with_ring(n, Ecm { b1, b2: 100 * b1, curves, sigma, b: &mut *b })? {
            return Ok(f);
        }
        sigma += curves;
    }
    if digits <= siqs::MAX_DIGITS {
        if let Some(f) = siqs::siqs(n, b)? {
            return Ok(f);
        }
    }
    // ECM with growing bounds (Zimmermann's table for factors of 30 to 70 digits)
    let mut b1 = 250_000u64;
    loop {
        if let Some(f) = with_ring(n, Ecm { b1, b2: 100 * b1, curves: 50, sigma, b: &mut *b })? {
            return Ok(f);
        }
        sigma += 50;
        b1 = b1 * 3 / 2;
    }
}

/// Is n a prime power (n >= 2)? The prime and the exponent.
pub fn prime_power(n: &Nat, b: &mut dyn Budget) -> Res<Option<(Nat, u64)>> {
    if let Some(v) = to_u64(n) {
        if is_prime_u64(v) {
            return Ok(Some((n.clone(), 1)));
        }
    }
    let (m, e) = perfect_power(n, b)?;
    Ok(if is_prime(&m, b)? { Some((m, e)) } else { None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;

    fn fac(n: &Nat) -> Vec<(String, u64)> {
        factor(n, &mut Unlimited).unwrap().into_iter().map(|(p, e)| (p.to_decimal(), e)).collect()
    }

    fn check(n: &Nat) {
        let f = factor(n, &mut Unlimited).unwrap();
        let mut prod = Nat::from_u64(1);
        for (p, e) in &f {
            assert!(is_prime(p, &mut Unlimited).unwrap(), "{} is not prime", p.to_decimal());
            prod = prod.mul(&p.pow_u64(*e));
        }
        assert_eq!(prod, *n);
        assert!(f.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn small_and_word_sized() {
        assert_eq!(fac(&Nat::from_u64(1)), vec![]);
        assert_eq!(fac(&Nat::from_u64(360)), vec![("2".into(), 3), ("3".into(), 2), ("5".into(), 1)]);
        for v in
            [2u64, 97, 65537, 4_294_967_297, 600_851_475_143, 18_446_744_073_709_551_615, 18_446_744_073_709_551_557]
        {
            check(&Nat::from_u64(v));
        }
        for v in (1u64 << 40)..(1u64 << 40) + 300 {
            check(&Nat::from_u64(v));
        }
    }

    #[test]
    fn big_numbers() {
        // 2^128 + 1 = 59649589127497217 * 5704689200685129054721
        assert_eq!(
            fac(&Nat::pow2(128).add_u64(1)),
            vec![("59649589127497217".into(), 1), ("5704689200685129054721".into(), 1)]
        );
        check(&Nat::pow2(256).sub_u64(1));
        check(&Nat::from_u64(3).pow_u64(100).add_u64(2));
        // a prime power with a big prime
        let p = crate::prime::next_prime(&Nat::pow(10, 30), &mut Unlimited).unwrap();
        assert_eq!(factor(&p.pow_u64(7), &mut Unlimited).unwrap(), vec![(p, 7)]);
    }
}
