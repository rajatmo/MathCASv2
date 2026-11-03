//! Sieves of Eratosthenes on odd numbers: the primes below a bound, segmented windows anywhere below 2^64, and a
//! table of pi(z) for every z up to a limit (the prime counting of `crate::count`).
//!
//! A window holds one bit per odd number: bit i stands for `lo + 2 i`, where `lo` is congruent to 1 modulo 128 so
//! that windows are whole words of the global bit pattern. The primes 3 to 13 are removed by copying a precomputed
//! pattern of period 3 5 7 11 13 = 15015 words, and the larger primes by crossing out their odd multiples.
use crate::budget::Budget;
use mc_eval::Res;
use std::sync::OnceLock;

/// The primes 3 to 13, removed by the pattern.
const PATTERN_PRIMES: [u64; 5] = [3, 5, 7, 11, 13];
/// The pattern's period in words (each word is 64 odd numbers; the period in odd numbers is 15015).
const PATTERN_WORDS: usize = 15015;
/// Bits of one sieving segment (32 KB, so that the segment stays in the first-level cache).
const SEGMENT_BITS: usize = 1 << 18;

/// The words of the odd numbers 1, 3, 5, ... with the multiples of 3 to 13 cleared, one period.
fn pattern() -> &'static [u64] {
    static P: OnceLock<Vec<u64>> = OnceLock::new();
    P.get_or_init(|| {
        let mut w = vec![u64::MAX; PATTERN_WORDS];
        for p in PATTERN_PRIMES {
            // odd multiples of p: the number 1 + 2j is a multiple when j = (p - 1)/2 (mod p)
            let mut j = ((p - 1) / 2) as usize;
            while j < 64 * PATTERN_WORDS {
                w[j / 64] &= !(1u64 << (j % 64));
                j += p as usize;
            }
        }
        w
    })
}

/// The primes below `limit` (at most 2^32), in increasing order.
pub fn primes_below(limit: u64) -> Vec<u32> {
    assert!(limit <= 1 << 32);
    if limit <= 2 {
        return vec![];
    }
    let n = limit as usize;
    // odd-only byte sieve: index i stands for 2i + 1
    let mut comp = vec![false; n.div_ceil(2)];
    let mut i = 1;
    while (2 * i + 1) * (2 * i + 1) < n {
        if !comp[i] {
            let p = 2 * i + 1;
            let mut j = p * p / 2;
            while j < comp.len() {
                comp[j] = true;
                j += p;
            }
        }
        i += 1;
    }
    let mut out = vec![2u32];
    out.extend((1..comp.len()).filter(|&i| !comp[i] && 2 * i + 1 < n).map(|i| (2 * i + 1) as u32));
    out
}

/// The primes below 2^16, shared (the base primes of every window below 2^32 and trial division).
pub fn small_primes() -> &'static [u32] {
    static P: OnceLock<Vec<u32>> = OnceLock::new();
    P.get_or_init(|| primes_below(1 << 16))
}

/// Base primes for sieving up to `hi`: the primes up to sqrt(hi).
pub fn base_primes(hi: u64) -> Vec<u32> {
    let r = crate::word::isqrt(hi) + 1;
    if r < 1 << 16 {
        small_primes().iter().copied().take_while(|&p| (p as u64) <= r).collect()
    } else {
        primes_below(r + 1)
    }
}

/// Sieve the odd numbers `lo + 2i` (lo = 1 mod 128) of `words` with the base primes (which must include every prime
/// up to the square root of the window's end, in increasing order, starting at 2 or 3). Bits of primes stay set;
/// the number 1 is cleared.
pub fn sieve_window(lo: u64, words: &mut [u64], base: &[u32]) {
    debug_assert!(lo % 128 == 1);
    let pat = pattern();
    let w0 = ((lo - 1) / 128) as usize % PATTERN_WORDS;
    for (i, w) in words.iter_mut().enumerate() {
        *w = pat[(w0 + i) % PATTERN_WORDS];
    }
    let nbits = words.len() as u64 * 64;
    let hi = lo.saturating_add(2 * nbits);
    // the pattern also cleared 3 to 13 themselves, and 1 is not prime
    for p in PATTERN_PRIMES {
        if p >= lo && p < hi {
            let i = (p - lo) / 2;
            words[(i / 64) as usize] |= 1 << (i % 64);
        }
    }
    if lo == 1 {
        words[0] &= !1;
    }
    for &p in base {
        let p = p as u64;
        if p <= 13 {
            continue;
        }
        if p * p >= hi {
            break;
        }
        // the first odd multiple of p at or above max(lo, p^2), computed wide near 2^64
        let mut m = if p * p >= lo { p as u128 * p as u128 } else { (lo as u128).div_ceil(p as u128) * p as u128 };
        if m % 2 == 0 {
            m += p as u128;
        }
        let Ok(off) = u64::try_from((m - lo as u128) / 2) else { continue };
        let mut i = off as usize;
        let step = p as usize;
        let nb = nbits as usize;
        while i < nb {
            words[i >> 6] &= !(1u64 << (i & 63));
            i += step;
        }
    }
}

/// The odd numbers of `[lo, hi)` (any bounds below 2^64 - 2^20) as an aligned window: the aligned start and the
/// word count.
fn aligned(lo: u64, hi: u64) -> (u64, usize) {
    let start = lo.saturating_sub(1) / 128 * 128 + 1;
    let words = (hi.saturating_sub(start)).div_ceil(128) as usize;
    (start, words.max(1))
}

/// Sieving primes are taken up to this bound; above 2^40 the survivors of a window are confirmed by
/// [`crate::word::is_prime_u64`].
const BASE_LIMIT: u64 = 1 << 20;

/// The primes in `[lo, hi)`, in increasing order (for windows of moderate size).
pub fn primes_in(lo: u64, hi: u64, b: &mut dyn Budget) -> Res<Vec<u64>> {
    let mut out = vec![];
    if lo <= 2 && hi > 2 {
        out.push(2);
    }
    let complete = crate::word::isqrt(hi) < BASE_LIMIT;
    let base = if complete { base_primes(hi) } else { primes_below(BASE_LIMIT) };
    let mut seg_lo = aligned(lo, hi).0;
    while seg_lo < hi {
        let (s, total) = aligned(seg_lo, hi);
        let n = total.min(SEGMENT_BITS / 64);
        let mut words = vec![0u64; n];
        sieve_window(s, &mut words, &base);
        b.charge(n as u64 / 4 + 1)?;
        for (wi, &w) in words.iter().enumerate() {
            let mut w = w;
            while w != 0 {
                let bit = w.trailing_zeros() as u64;
                w &= w - 1;
                let Some(v) = s.checked_add(2 * (64 * wi as u64 + bit)) else { break };
                if v >= lo && v < hi && (complete || crate::word::is_prime_u64(v)) {
                    out.push(v);
                }
            }
        }
        match s.checked_add(128 * n as u64) {
            Some(next) => seg_lo = next,
            None => break,
        }
    }
    Ok(out)
}

/// pi(z) for every z up to a limit: the odd-number bit table and the count of primes before each word.
///
/// Each entry covers 32 odd numbers: the low half holds their prime bits, the high half the number of odd primes
/// before them, so that a lookup reads one word (the table is far larger than the caches, and the prime counting
/// makes tens of millions of scattered lookups).
pub struct PiTable {
    entries: Vec<u64>,
    limit: u64,
}

/// Sieving segments processed in parallel between two charges of the budget.
const ROUND: usize = 16;

impl PiTable {
    /// The table for z <= limit (limit < 2^40, so that the counts fit 32 bits). The segments are sieved in
    /// parallel on the current thread pool.
    pub fn new(limit: u64, b: &mut dyn Budget) -> Res<PiTable> {
        use rayon::prelude::*;
        assert!(limit < 1 << 40);
        let nwords = (limit / 128 + 1) as usize;
        let mut entries = vec![0u64; 2 * nwords];
        let base = base_primes(128 * nwords as u64 + 1);
        let seg = SEGMENT_BITS / 64;
        for (r, round) in entries.chunks_mut(2 * seg * ROUND).enumerate() {
            round.par_chunks_mut(2 * seg).enumerate().for_each(|(k, chunk)| {
                let mut words = vec![0u64; chunk.len() / 2];
                let first = (r * ROUND + k) * seg;
                sieve_window(1 + 128 * first as u64, &mut words, &base);
                for (e, &w) in chunk.chunks_mut(2).zip(&words) {
                    e[0] = w & 0xFFFF_FFFF;
                    e[1] = w >> 32;
                }
            });
            b.charge(round.len() as u64 / 8 + 1)?;
        }
        let mut c = 0u64;
        for e in &mut entries {
            let bits = *e;
            *e = c << 32 | bits;
            c += bits.count_ones() as u64;
        }
        Ok(PiTable { entries, limit })
    }
    /// The largest z the table answers.
    pub fn limit(&self) -> u64 {
        self.limit
    }
    /// pi(z) for z <= limit.
    #[inline]
    pub fn pi(&self, z: u64) -> u64 {
        debug_assert!(z <= self.limit);
        if z < 2 {
            return 0;
        }
        // odd numbers 1 ..= z (or z - 1) are bits 0 ..= (z - 1)/2; the prime 2 is added
        let i = ((z - 1) / 2) as usize;
        let e = self.entries[i / 32];
        let mask = (2u64 << (i % 32)) - 1;
        1 + (e >> 32) + (e & mask).count_ones() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;
    use crate::word::is_prime_u64;

    #[test]
    fn primes_below_small_bounds() {
        assert_eq!(primes_below(0), Vec::<u32>::new());
        assert_eq!(primes_below(3), vec![2]);
        assert_eq!(primes_below(30), vec![2, 3, 5, 7, 11, 13, 17, 19, 23, 29]);
        assert_eq!(primes_below(1_000_000).len(), 78498);
        assert_eq!(small_primes().len(), 6542);
    }

    #[test]
    fn table_counts() {
        let t = crate::test_pool().install(|| PiTable::new(2_000_000, &mut Unlimited).unwrap());
        let mut c = 0;
        for z in 0..=200_000u64 {
            if is_prime_u64(z) {
                c += 1;
            }
            assert_eq!(t.pi(z), c, "{z}");
        }
        assert_eq!(t.pi(1_000_000), 78498);
        assert_eq!(t.pi(2_000_000), 148933);
    }

    #[test]
    fn windows_match_primality() {
        for &(lo, hi) in &[
            (0u64, 1000u64),
            (1, 2),
            (2, 3),
            (1_000_000_000_000, 1_000_000_100_000),
            (u64::MAX - 100_000, u64::MAX - 10),
        ] {
            if lo >= hi {
                continue;
            }
            let got = primes_in(lo, hi, &mut Unlimited).unwrap();
            let want: Vec<u64> = (lo..hi).filter(|&n| is_prime_u64(n)).collect();
            assert_eq!(got, want, "[{lo}, {hi})");
        }
        let lo = (1u64 << 63) + 12345;
        let got = primes_in(lo, lo + 50_000, &mut Unlimited).unwrap();
        let want: Vec<u64> = (lo..lo + 50_000).filter(|&n| is_prime_u64(n)).collect();
        assert_eq!(got, want);
    }
}
