//! Prime counting by the combinatorial method: Lagarias, Miller and Odlyzko's formula with the division of the
//! special leaves of Deléglise and Rivat.
//!
//! Literature: J. C. Lagarias, V. S. Miller, A. M. Odlyzko, "Computing pi(x): the Meissel-Lehmer method", Math.
//! Comp. 44 (1985), 537-560; M. Deléglise, J. Rivat, "Computing pi(x): the Meissel, Lehmer, Lagarias, Miller,
//! Odlyzko method", Math. Comp. 65 (1996), 235-245; T. Oliveira e Silva, "Computing pi(x): the combinatorial
//! method", Revista do DETUA 4 (2006), 759-768.
//!
//! With y >= x^(1/3), a = pi(y) and phi(z, b) the count of the numbers up to z free of the first b primes,
//!
//! pi(x) = phi(x, a) + a - 1 - P2,  P2 = sum over y < p <= sqrt(x) of (pi(x / p) - pi(p) + 1),
//!
//! and phi(x, a) = S1 + S2 by expanding phi(z, b) = phi(z, b - 1) - phi(z / p_b, b - 1) from (x, a) down to the
//! first c = 6 primes (the wheel 2 3 5 7 11 13 = 30030):
//!
//! - S1 (ordinary leaves): the sum of mu(n) phi(x / n, c) over the squarefree n <= y whose least prime factor
//!   exceeds 13;
//! - S2 (special leaves): the sum of -mu(m) phi(x / (p m), b) over the primes p = p_(b+1) with c <= b < a and the
//!   squarefree m <= y < p m whose least prime factor exceeds p.
//!
//! A special leaf with v = x / (p m) is *trivial* when v < p (phi = 1), *easy* when v < p^2 (phi = 1 + pi(v) - b,
//! with v < sqrt(x), read from a table of pi up to sqrt(x)), and *hard* otherwise. Easy leaves of one p whose v
//! fall between the same two primes are counted together (the clustered leaves of Deléglise and Rivat). Hard leaves
//! need phi(v, b) for v up to x / y: the numbers below x / y are sieved in segments, prime by prime, with a count
//! of the survivors per block of the segment, and each hard leaf reads its count when the sieve has removed exactly
//! the first b primes. The same segments, sieved on with the primes up to the square root of their end, give the
//! values pi(x / p) of P2. Segments are worked in parallel in chunks; the counts at the start of a chunk are added
//! afterwards (each chunk keeps, per b, its count of survivors and the sum of the coefficients of its leaves).
//!
//! y = alpha x^(1/3) balances the sieve (about x / y numbers) against the leaves (about sqrt(x y) easy leaves and
//! y pi(sqrt(y)) candidates for hard ones).
use crate::budget::Budget;
use crate::sieve::{pattern, primes_below, PiTable, PATTERN_WORDS};
use crate::word::{iroot, isqrt};
use mc_eval::Res;
use rayon::prelude::*;

/// Primes removed by the wheel and the sieve's pattern: 2, 3, 5, 7, 11, 13.
const C: usize = 6;
/// The product of the first C primes and its count of coprime residues.
const WHEEL: u64 = 30030;
const WHEEL_PHI: u64 = 5760;
/// Words of one sieving segment (2^18 bits of odd numbers, 2^19 numbers, 32 KB).
const SEG_WORDS: usize = 4096;
/// Numbers per segment.
const SEG_NUMS: u64 = 128 * SEG_WORDS as u64;
/// log2 of the bits per survivor counter.
const CB: usize = 9;
/// Segments per parallel chunk.
const SEGS_PER_CHUNK: u64 = 4;
/// Chunks per parallel round (between two charges of the budget).
const ROUND: usize = 32;

/// t[r] = the number of k in [1, r] coprime to 30030.
fn wheel_counts() -> &'static [u16] {
    use std::sync::OnceLock;
    static T: OnceLock<Vec<u16>> = OnceLock::new();
    T.get_or_init(|| {
        let mut c = 0u16;
        (0..WHEEL)
            .map(|r| {
                if r > 0 && [2u64, 3, 5, 7, 11, 13].iter().all(|&q| r % q != 0) {
                    c += 1;
                }
                c
            })
            .collect()
    })
}

/// phi(z, 6).
#[inline]
fn phi6(z: u64, t: &[u16]) -> u64 {
    (z / WHEEL) * WHEEL_PHI + t[(z % WHEEL) as usize] as u64
}

/// The parameter alpha of y = alpha x^(1/3).
fn alpha_for(x: u64) -> f64 {
    let l = (x as f64).ln();
    // (log x)^2 / 150: about 8 at 10^15, 10 at 10^17 (timed on 10^15 .. 10^17 with alpha from 1 to 32)
    (l * l / 150.0).max(1.0)
}

/// For the odd m <= y (index m / 2): mu(m) times the least prime factor of m, 0 when m is not squarefree, and
/// i32::MAX for m = 1.
fn factor_table(y: u64, primes: &[u32]) -> Vec<i32> {
    // the odd numbers 1, 3, ..., up to y
    let n = (y as usize).div_ceil(2);
    let mut lpf = vec![0i32; n];
    let mut mu = vec![1i8; n];
    for &p in primes.iter().skip(1) {
        let p = p as usize;
        let mut j = p >> 1;
        while j < n {
            if lpf[j] == 0 {
                lpf[j] = p as i32;
            }
            mu[j] = -mu[j];
            j += p;
        }
        let q = p as u64 * p as u64;
        if q <= y {
            let q = q as usize;
            let mut j = q >> 1;
            while j < n {
                mu[j] = 0;
                j += q;
            }
        }
    }
    for (j, (l, &m)) in lpf.iter_mut().zip(&mu).enumerate() {
        *l = if m == 0 {
            0
        } else if j == 0 {
            i32::MAX
        } else {
            m as i32 * *l
        };
    }
    lpf
}

/// The shared data of one count.
struct Ctx<'a> {
    x: u64,
    y: u64,
    /// numbers below z_end are sieved: every hard leaf and every x / p of P2 lies below it
    z_end: u64,
    sqrtx: u64,
    x14: u64,
    primes: &'a [u32],
    fac: &'a [i32],
    pit: &'a PiTable,
    /// the number of primes <= sqrt(y): primes[b] for b below it may have composite m
    pi_sqrty: usize,
    a: usize,
}

/// What one chunk of segments contributes.
struct ChunkOut {
    /// the sum of -mu(m) phi_local(v, b) over the hard leaves of the chunk (counts from the chunk's start)
    s: i128,
    /// per b: the sum of -mu(m) over the chunk's hard leaves of b (it multiplies the count before the chunk)
    musum: Vec<i64>,
    /// per b: the survivors of the chunk after removing the first b primes
    counts: Vec<u64>,
    /// P2: the queries answered, the sum of their local prime counts, the primes of the chunk at or above sqrt(x)
    nq: u64,
    p2_local: u128,
    primes_here: u64,
}

/// A segment of odd numbers low, low + 2, ... with survivor counts per block of 2^CB bits.
struct Seg {
    words: Vec<u64>,
    counters: Vec<u32>,
    /// valid bits (numbers below the segment's end)
    nb: usize,
    total: u64,
}

impl Seg {
    fn new() -> Seg {
        Seg { words: vec![0; SEG_WORDS], counters: vec![0; (SEG_WORDS * 64) >> CB], nb: 0, total: 0 }
    }

    /// Fill for [low, high) (low = 1 mod 128) with the pattern of the wheel.
    fn fill(&mut self, low: u64, high: u64) {
        let pat = pattern();
        let w0 = ((low - 1) / 128) as usize % PATTERN_WORDS;
        for (i, w) in self.words.iter_mut().enumerate() {
            *w = pat[(w0 + i) % PATTERN_WORDS];
        }
        self.nb = (high - low).div_ceil(2) as usize;
        let full = self.nb / 64;
        if full < SEG_WORDS {
            self.words[full] &= (1u64 << (self.nb % 64)) - 1;
            for w in &mut self.words[full + 1..] {
                *w = 0;
            }
        }
        let per = 1usize << (CB - 6);
        self.total = 0;
        for (k, c) in self.counters.iter_mut().enumerate() {
            *c = self.words[k * per..(k + 1) * per].iter().map(|w| w.count_ones()).sum();
            self.total += *c as u64;
        }
    }

    /// Remove the odd multiples of p in [low, low + 2 nb), keeping the counters.
    fn cross_counted(&mut self, low: u64, p: u64) {
        let mut m = low.div_ceil(p) * p;
        if m.is_multiple_of(2) {
            m += p;
        }
        let mut i = ((m - low) / 2) as usize;
        let step = p as usize;
        let mut removed = 0u64;
        while i < self.nb {
            let bit = 1u64 << (i & 63);
            let w = &mut self.words[i >> 6];
            let was = (*w & bit) >> (i & 63);
            *w &= !bit;
            self.counters[i >> CB] -= was as u32;
            removed += was;
            i += step;
        }
        self.total -= removed;
    }

    /// Remove the odd multiples of p without the counters.
    fn cross_plain(&mut self, low: u64, p: u64) {
        let mut m = low.div_ceil(p) * p;
        if m.is_multiple_of(2) {
            m += p;
        }
        let mut i = ((m - low) / 2) as usize;
        let step = p as usize;
        while i < self.nb {
            self.words[i >> 6] &= !(1u64 << (i & 63));
            i += step;
        }
    }
}

/// Counting the survivors with index <= i for increasing i.
struct Cursor {
    block: usize,
    acc: u64,
}

impl Cursor {
    #[inline]
    fn upto(&mut self, seg: &Seg, i: usize) -> u64 {
        let blk = i >> CB;
        while self.block < blk {
            self.acc += seg.counters[self.block] as u64;
            self.block += 1;
        }
        let wi = i >> 6;
        let mut c = self.acc;
        for w in &seg.words[blk << (CB - 6)..wi] {
            c += w.count_ones() as u64;
        }
        c + (seg.words[wi] & (u64::MAX >> (63 - (i & 63)))).count_ones() as u64
    }
}

impl Ctx<'_> {
    /// The primes whose hard leaves reach the segments at or above `low`: every p <= sqrt(y), and the p with
    /// x / p^2 >= low and p^4 <= x. Never increases with low.
    fn b_hi(&self, low: u64) -> usize {
        let t = self.x14.min(isqrt(self.x / low.max(1)));
        let k = if t >= self.y { self.a } else { self.pit.pi(t) as usize };
        k.max(self.pi_sqrty).min(self.a).max(C)
    }

    /// The hard leaves of primes[b] with v in [low, high), added to `s` and `musum` with phi counted from the
    /// chunk's start (`before` survivors in earlier segments of the chunk).
    #[allow(clippy::too_many_arguments)]
    fn leaves(&self, b: usize, low: u64, high: u64, seg: &Seg, before: u64, s: &mut i128, musum: &mut i64) {
        let p = self.primes[b] as u64;
        let xp = self.x / p;
        let hard_max = xp / p / p;
        let mut cur = Cursor { block: 0, acc: 0 };
        if b < self.pi_sqrty {
            let max_m = (xp / low).min(self.y).min(hard_max);
            let min_m = (xp / high).max(self.y / p);
            if max_m <= min_m {
                return;
            }
            let mut m = if max_m.is_multiple_of(2) { max_m - 1 } else { max_m };
            let (mut acc, mut ms) = (0i128, 0i64);
            while m > min_m {
                let f = self.fac[(m >> 1) as usize];
                if f.unsigned_abs() as u64 > p {
                    let v = xp / m;
                    let c = (before + cur.upto(seg, ((v - low) / 2) as usize)) as i128;
                    if f > 0 {
                        acc -= c;
                        ms -= 1;
                    } else {
                        acc += c;
                        ms += 1;
                    }
                }
                if m < 3 {
                    break;
                }
                m -= 2;
            }
            *s += acc;
            *musum += ms;
        } else {
            let q_hi = self.y.min(hard_max).min(xp / low);
            let q_lo = p.max(xp / high);
            if q_hi <= q_lo {
                return;
            }
            let k0 = self.pit.pi(q_lo) as usize;
            let k1 = self.pit.pi(q_hi) as usize;
            let mut acc = 0i128;
            for k in (k0..k1).rev() {
                let v = xp / self.primes[k] as u64;
                acc += (before + cur.upto(seg, ((v - low) / 2) as usize)) as i128;
            }
            *s += acc;
            *musum += (k1 - k0) as i64;
        }
    }

    /// One chunk of segments, the first starting at 1 + first * SEG_NUMS.
    fn chunk(&self, first: u64) -> ChunkOut {
        let low0 = 1 + first * SEG_NUMS;
        let bh0 = self.b_hi(low0);
        let mut out = ChunkOut {
            s: 0,
            musum: vec![0; bh0],
            counts: vec![0; bh0],
            nq: 0,
            p2_local: 0,
            primes_here: 0,
        };
        let mut seg = Seg::new();
        let mut last_bh = bh0;
        let mut ps = Vec::new();
        for k in 0..SEGS_PER_CHUNK {
            let low = low0 + k * SEG_NUMS;
            if low >= self.z_end {
                break;
            }
            let high = (low + SEG_NUMS).min(self.z_end);
            seg.fill(low, high);
            let bh = self.b_hi(low);
            last_bh = bh;
            for b in C..bh {
                let p = self.primes[b] as u64;
                self.leaves(b, low, high, &seg, out.counts[b], &mut out.s, &mut out.musum[b]);
                out.counts[b] += seg.total;
                seg.cross_counted(low, p);
            }
            if high > self.sqrtx {
                // the survivors at or above sqrt(x) are primes once every prime up to sqrt(high) is removed
                for &p in &self.primes[bh..] {
                    let p = p as u64;
                    if p * p >= high {
                        break;
                    }
                    seg.cross_plain(low, p);
                }
                let lo_v = low.max(self.sqrtx);
                let i0 = (lo_v - low).div_ceil(2) as usize;
                let p_hi = self.sqrtx.min(self.x / lo_v);
                let p_lo = self.y.max(self.x / high);
                ps.clear();
                if p_hi > p_lo {
                    self.pit.primes_between(p_lo + 1, p_hi, &mut ps);
                }
                // survivors with index in [i0, i] for increasing i (acc: the words before wi)
                let from = |w: usize| if w == i0 >> 6 { u64::MAX << (i0 & 63) } else { u64::MAX };
                let mut wi = i0 >> 6;
                let mut acc = 0u64;
                for &p in ps.iter().rev() {
                    let i = ((self.x / p - low) / 2) as usize;
                    let c = if i < i0 {
                        0
                    } else {
                        while wi < i >> 6 {
                            acc += (seg.words[wi] & from(wi)).count_ones() as u64;
                            wi += 1;
                        }
                        acc + (seg.words[wi] & from(wi) & (u64::MAX >> (63 - (i & 63)))).count_ones() as u64
                    };
                    out.p2_local += (out.primes_here + c) as u128;
                    out.nq += 1;
                }
                let mut tot = 0u64;
                let mut i = i0;
                while i < seg.nb {
                    let w = i >> 6;
                    tot += (seg.words[w] & (u64::MAX << (i & 63))).count_ones() as u64;
                    i = (w + 1) * 64;
                }
                out.primes_here += tot;
            }
        }
        out.counts.truncate(last_bh);
        out
    }
}

/// pi(x) by the combinatorial method with y = alpha x^(1/3) (x >= 10^6). The sieve and the leaves are worked in
/// parallel rounds on the current thread pool.
pub fn prime_pi_leaves(x: u64, b: &mut dyn Budget) -> Res<u64> {
    prime_pi_alpha(x, alpha_for(x), b)
}

/// [`prime_pi_leaves`] with a given alpha.
pub fn prime_pi_alpha(x: u64, alpha: f64, b: &mut dyn Budget) -> Res<u64> {
    let [s1, s2, a, p2] = parts(x, alpha, b)?;
    Ok((s1 + s2 + a - 1 - p2) as u64)
}

/// S1, S2, a and P2.
fn parts(x: u64, alpha: f64, b: &mut dyn Budget) -> Res<[i128; 4]> {
    assert!(x >= 1_000_000);
    let x13 = iroot(x, 3);
    let sqrtx = isqrt(x);
    let y = ((alpha * x13 as f64) as u64).max(x13).min(sqrtx);
    let primes = primes_below(y + 1);
    let a = primes.len();
    let pit = PiTable::new(sqrtx, b)?;
    b.charge(y / 1000 + 1)?;
    let fac = factor_table(y, &primes);
    let pi_sqrty = pit.pi(isqrt(y)) as usize;
    let ctx = Ctx {
        x,
        y,
        z_end: x / (y + 1) + 1,
        sqrtx,
        x14: iroot(x, 4),
        primes: &primes,
        fac: &fac,
        pit: &pit,
        pi_sqrty,
        a,
    };
    let t = wheel_counts();

    // S1: the ordinary leaves
    b.charge(y / 1000 + 1)?;
    let s1: i128 = fac
        .par_chunks(1 << 16)
        .enumerate()
        .map(|(ci, ch)| {
            let mut s = 0i128;
            for (j, &f) in ch.iter().enumerate() {
                if f != 0 && f.unsigned_abs() > 13 {
                    let m = 2 * ((ci << 16) + j) as u64 + 1;
                    let v = phi6(x / m, t) as i128;
                    s += if f > 0 { v } else { -v };
                }
            }
            s
        })
        .sum();

    // S2, the trivial and easy leaves
    let mut s2 = 0i128;
    // the work per prime falls as p grows: rounds of growing length, one prime per task
    let mut bb = C;
    let mut len = 16;
    while bb < a {
        let end = (bb + len).min(a);
        let part: i128 = (bb..end).into_par_iter().with_max_len(1).map(|b| ctx.easy(b)).sum();
        s2 += part;
        b.charge(((end - bb) as u64) * (y / 4096 + 1))?;
        bb = end;
        len = (len * 2).min(4096);
    }
    s2 += ctx.sparse_all(b)?;

    // S2, the hard leaves, and P2 from one sieve
    let nsegs = (ctx.z_end - 1).div_ceil(SEG_NUMS);
    let nchunks = nsegs.div_ceil(SEGS_PER_CHUNK);
    let mut offsets: Vec<u64> = vec![0; ctx.b_hi(1)];
    let (mut p2_sum, mut primes_before, mut nq) = (0u128, 0u64, 0u64);
    let mut ci = 0u64;
    while ci < nchunks {
        let end = (ci + ROUND as u64).min(nchunks);
        let outs: Vec<ChunkOut> = (ci..end).into_par_iter().map(|c| ctx.chunk(c * SEGS_PER_CHUNK)).collect();
        for o in outs {
            s2 += o.s;
            for (bi, &ms) in o.musum.iter().enumerate() {
                if ms != 0 {
                    s2 += ms as i128 * offsets[bi] as i128;
                }
            }
            for (off, &c) in offsets.iter_mut().zip(&o.counts) {
                *off += c;
            }
            p2_sum += o.p2_local + o.nq as u128 * primes_before as u128;
            primes_before += o.primes_here;
            nq += o.nq;
        }
        b.charge((end - ci) * SEGS_PER_CHUNK * 600)?;
        ci = end;
    }
    let k = pit.pi(sqrtx) as u128;
    debug_assert_eq!(nq as u128, k - a as u128);
    let below = pit.pi(sqrtx - 1) as u128;
    let p2 = p2_sum as i128 + nq as i128 * below as i128 - ((k * (k - 1)) / 2) as i128
        + ((a as u128 * (a as u128).saturating_sub(1)) / 2) as i128;
    Ok([s1, s2, a as i128, p2])
}

impl Ctx<'_> {
    /// The trivial and easy leaves of primes[b], and for p <= sqrt(y) every leaf that is not hard.
    fn easy(&self, b: usize) -> i128 {
        let p = self.primes[b] as u64;
        let xp = self.x / p;
        let xp2 = xp / p;
        let xp3 = xp2 / p;
        let bb = b as i128;
        let mut s = 0i128;
        if b < self.pi_sqrty {
            // m in (max(y / p, x / p^3), y]: v < p^2
            let min_m = (self.y / p).max(xp3);
            let mut m = if self.y.is_multiple_of(2) { self.y - 1 } else { self.y };
            while m > min_m {
                let f = self.fac[(m >> 1) as usize];
                if f.unsigned_abs() as u64 > p {
                    let v = xp / m;
                    let phi = if v < p { 1 } else { 1 + self.pit.pi(v) as i128 - bb };
                    s += if f > 0 { -phi } else { phi };
                }
                if m < 3 {
                    break;
                }
                m -= 2;
            }
            return s;
        }
        // q prime in (p, y]: trivial for q > x / p^2, easy for x / p^3 < q <= x / p^2
        let pi_y = self.a as u64;
        let triv_lo = p.max(xp2);
        if triv_lo < self.y {
            s += (pi_y - self.pit.pi(triv_lo)) as i128;
        }
        // the easy leaves with v <= y here (q > x / (p (y + 1))); those with v > y in `sparse`
        let e_lo = p.max(xp3).max(xp / (self.y + 1));
        let e_hi = self.y.min(xp2);
        if e_hi <= e_lo {
            return s;
        }
        let mut i = self.pit.pi(e_lo) as usize;
        let iend = self.pit.pi(e_hi) as usize; // exclusive
        while i < iend {
            let q = self.primes[i] as u64;
            let v = xp / q;
            let l = self.pit.pi(v);
            // every q' with x / (p q') >= p_l has the same pi
            let pl = self.primes[l as usize - 1] as u64;
            let t = xp / pl;
            let j = if t >= self.y { iend } else { (self.pit.pi(t) as usize).min(iend) };
            s += (j - i) as i128 * (1 + l as i128 - bb);
            i = j;
        }
        s
    }

    /// The easy leaves of primes[b] (p > sqrt(y)) with y < v and v in [lo, hi): one pi(v) each, worked in windows of
    /// v so that the lookups stay in a small part of the table.
    fn sparse(&self, b: usize, lo: u64, hi: u64) -> i128 {
        let p = self.primes[b] as u64;
        let xp = self.x / p;
        let xp2 = xp / p;
        let q_lo = p.max(xp2 / p).max(xp / hi);
        let q_hi = self.y.min(xp2).min(xp / (self.y + 1)).min(xp / lo);
        if q_hi <= q_lo {
            return 0;
        }
        let (k0, k1) = (self.pit.pi(q_lo) as usize, self.pit.pi(q_hi) as usize);
        let bb = b as i128 - 1;
        let mut s = 0i128;
        for &q in &self.primes[k0..k1] {
            s += self.pit.pi(xp / q as u64) as i128 - bb;
        }
        s
    }

    /// Every sparse easy leaf: windows of v over (y, sqrt(x)), primes p > sqrt(y) with p^2 <= x / (y + 1).
    fn sparse_all(&self, b: &mut dyn Budget) -> Res<i128> {
        const W: u64 = 1 << 21;
        let first = self.pi_sqrty.max(C);
        let last = (self.pit.pi(isqrt(self.x / (self.y + 1)).min(self.y)) as usize).min(self.a);
        if first >= last || self.y + 1 > self.sqrtx {
            return Ok(0);
        }
        let windows: Vec<(u64, u64)> = (0..)
            .map(|k| self.y + 1 + k * W)
            .take_while(|&lo| lo <= self.sqrtx)
            .map(|lo| (lo, (lo + W).min(self.sqrtx + 1)))
            .collect();
        let mut s = 0i128;
        for round in windows.chunks(16) {
            s += round
                .par_iter()
                .with_max_len(1)
                .map(|&(lo, hi)| (first..last).map(|bi| self.sparse(bi, lo, hi)).sum::<i128>())
                .sum::<i128>();
            b.charge(round.len() as u64 * (last - first) as u64 / 8 + 1)?;
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;

    fn pi_alpha(x: u64, alpha: f64) -> u64 {
        crate::test_pool().install(|| prime_pi_alpha(x, alpha, &mut Unlimited).unwrap())
    }

    /// The trivial and easy leaves of each prime against direct counting.
    #[test]
    fn easy_leaves_directly() {
        let x = 12_345_679u64;
        let y = 1848u64;
        let sqrtx = isqrt(x);
        let primes = primes_below(y + 1);
        let pit = crate::test_pool().install(|| PiTable::new(sqrtx, &mut Unlimited).unwrap());
        let fac = factor_table(y, &primes);
        let ctx = Ctx {
            x,
            y,
            z_end: x / (y + 1) + 1,
            sqrtx,
            x14: iroot(x, 4),
            primes: &primes,
            fac: &fac,
            pit: &pit,
            pi_sqrty: pit.pi(isqrt(y)) as usize,
            a: primes.len(),
        };
        let zmax = (x / (y + 1)) as usize + 1;
        // phi[v] for the current b
        let mut phi: Vec<i64> = (0..=zmax as i64).collect();
        let mut alive = vec![true; zmax + 1];
        alive[0] = false;
        let mut hard = 0i128;
        let mut easy_total = 0i128;
        let out = ctx.chunk(0);
        for (b, &p) in primes.iter().enumerate() {
            if b >= C {
                let p = p as u64;
                let mut want = 0i128;
                let mut hb = 0i64;
                for m in (1..=y).filter(|m| m % 2 == 1) {
                    let f = fac[(m >> 1) as usize];
                    if f == 0 || f.unsigned_abs() as u64 <= p || p * m <= y {
                        continue;
                    }
                    let v = x / (p * m);
                    if v >= p * p {
                        let ph = phi[v as usize] as i128;
                        hard += if f > 0 { -ph } else { ph };
                        hb += if f > 0 { -1 } else { 1 };
                    }
                }
                assert_eq!(out.musum.get(b).copied().unwrap_or(0), hb, "musum b = {b}");
                for m in (1..=y).filter(|m| m % 2 == 1) {
                    let f = fac[(m >> 1) as usize];
                    if f == 0 || f.unsigned_abs() as u64 <= p || p * m <= y {
                        continue;
                    }
                    let v = x / (p * m);
                    if v >= p * p {
                        continue;
                    }
                    let ph = phi[v as usize] as i128;
                    want += if f > 0 { -ph } else { ph };
                }
                let sp = if b >= ctx.pi_sqrty { ctx.sparse(b, y + 1, sqrtx + 1) } else { 0 };
                assert_eq!(ctx.easy(b) + sp, want, "b = {b}, p = {p}");
                easy_total += want;
            }
            let p = p as usize;
            let mut k = p;
            while k <= zmax {
                alive[k] = false;
                k += p;
            }
            let mut c = 0;
            for v in 0..=zmax {
                if alive[v] {
                    c += 1;
                }
                phi[v] = c;
            }
        }
        assert_eq!(out.s, hard);
        let [s1, s2, _, _] = crate::test_pool().install(|| parts(x, 8.0, &mut Unlimited).unwrap());
        assert_eq!(s2, hard + easy_total);
        let t = wheel_counts();
        let mut s1w = 0i128;
        for m in 1..=y {
            let ok = m == 1 || (m % 2 == 1 && fac[(m >> 1) as usize] != 0 && fac[(m >> 1) as usize].unsigned_abs() > 13);
            if ok {
                let f = if m == 1 { 1 } else { fac[(m >> 1) as usize].signum() as i128 };
                let z = x / m;
                let direct = (1..=z).filter(|n| [2, 3, 5, 7, 11, 13].iter().all(|q| n % q != 0)).count() as i128;
                assert_eq!(phi6(z, t) as i128, direct, "z = {z}");
                s1w += f * direct;
            }
        }
        assert_eq!(s1, s1w);
    }

    /// phi(x, a) and P2 against direct counting.
    #[test]
    fn parts_directly() {
        let x = 12_345_679u64;
        let t = crate::test_pool().install(|| PiTable::new(x, &mut Unlimited).unwrap());
        for alpha in [1.0, 3.5, 8.0] {
            let [s1, s2, a, p2] = crate::test_pool().install(|| parts(x, alpha, &mut Unlimited).unwrap());
            let ps = primes_below(isqrt(x) + 1);
            let y = ps[a as usize - 1] as u64;
            // phi(x, a): numbers up to x free of the primes up to y
            let small: Vec<u64> = ps.iter().map(|&p| p as u64).filter(|&p| p <= y).collect();
            let phi = (1..=x).filter(|&n| small.iter().all(|&p| n % p != 0)).count() as i128;
            let mut p2w = 0i128;
            for (k, &p) in ps.iter().enumerate().skip(a as usize) {
                p2w += t.pi(x / p as u64) as i128 - k as i128;
            }
            assert_eq!((s1 + s2, p2), (phi, p2w), "alpha = {alpha}");
        }
    }

    #[test]
    fn against_the_table() {
        let t = crate::test_pool().install(|| PiTable::new(40_000_000, &mut Unlimited).unwrap());
        for x in [1_000_000u64, 1_000_003, 2_345_678, 10_000_000, 12_345_679, 39_999_999] {
            for alpha in [1.0, 2.0, 3.5, 8.0] {
                assert_eq!(pi_alpha(x, alpha), t.pi(x), "x = {x}, alpha = {alpha}");
            }
        }
    }

    /// Timings over alpha (run with --ignored --nocapture; MC_PI_X sets x).
    #[test]
    #[ignore]
    fn timings() {
        let x: u64 = std::env::var("MC_PI_X").ok().and_then(|s| s.parse().ok()).unwrap_or(1_000_000_000_000_000);
        let alphas: Vec<f64> = std::env::var("MC_PI_ALPHAS")
            .ok()
            .map(|s| s.split(',').filter_map(|a| a.parse().ok()).collect())
            .unwrap_or_else(|| vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0]);
        for alpha in alphas {
            let t0 = std::time::Instant::now();
            let v = pi_alpha(x, alpha);
            println!("x = {x} alpha = {alpha}: {v} in {:?}", t0.elapsed());
        }
    }

    #[test]
    fn known_values() {
        // pi(10^k), k = 9 .. 13
        for (x, want) in [
            (1_000_000_000u64, 50_847_534u64),
            (10_000_000_000, 455_052_511),
            (100_000_000_000, 4_118_054_813),
            (1_000_000_000_000, 37_607_912_018),
            (10_000_000_000_000, 346_065_536_839),
        ] {
            for alpha in [1.0, 3.0, 10.0] {
                assert_eq!(pi_alpha(x, alpha), want, "x = {x}, alpha = {alpha}");
            }
        }
    }
}
