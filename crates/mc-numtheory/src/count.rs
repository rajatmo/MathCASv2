//! Prime counting and the k-th prime.
//!
//! `prime_pi(x)` is Meissel's formula with a = pi(x^(1/3)):
//!
//! pi(x) = phi(x, a) + a - 1 - P2(x, a),  P2 = sum over a < i <= pi(sqrt x) of (pi(x / p_i) - i + 1),
//!
//! where phi(x, a) counts the numbers up to x with no prime factor among the first a primes. phi is expanded by
//! phi(z, b) = phi(z, 6) - sum_{6 < i <= b} phi(z / p_i, i - 1), with phi(z, b) for b <= 6 from the wheel of
//! 2 3 5 7 11 13 = 30030, and every phi(z, b) with z < p_(b+1)^2 truncated to 1 + max(0, pi(z) - b) (the only
//! survivors are 1 and the primes above p_b). The values pi(z) needed, both there and in P2, are below about x^(2/3)
//! and come from one sieved table (`crate::sieve::PiTable`). The recursion divides by at least 17 per level, so its
//! depth is below 16.
//!
//! `nth_prime(k)` inverts Riemann's R(x) for an estimate x0, counts pi(x0) exactly, and walks the remaining few
//! primes in sieved windows.
use crate::budget::Budget;
use crate::sieve::{primes_below, primes_in, PiTable};
use crate::word::{iroot, isqrt};
use mc_eval::Res;

/// Below this bound prime_pi sieves directly.
const DIRECT: u64 = 1 << 23;
/// Largest argument of [`prime_pi`]: the table of pi up to x^(2/3) then needs about 200 MB (decision J.9).
pub const PI_MAX: u64 = 1 << 47;
/// The primes of the wheel.
const WHEEL: usize = 6;
/// Rows of the cache of phi(z, b) for small z: b from 6 to 6 + CACHE_ROWS - 1.
const CACHE_ROWS: usize = 94;
/// Top-level subtrees evaluated in parallel between two charges of the budget.
const ROUND: usize = 32;

/// The phi(z, b) machinery for one x.
struct Phi<'a> {
    table: &'a PiTable,
    primes: &'a [u32],
    /// squares of the primes: sq[i] = primes[i]^2
    sq: Vec<u64>,
    /// for b <= 6: the product P_b of the first b primes, phi(P_b, b), and t[r] = phi(r, b) for r < P_b
    wheel: Vec<(u64, u64, Vec<u16>)>,
    /// cache[b - 6][z] = phi(z, b) for z < cache_z (phi(z, b) <= 0.2 z + 5760 for b >= 6, so it fits 16 bits
    /// for z < 2^18)
    cache: Vec<Vec<u16>>,
    cache_z: u64,
}

impl<'a> Phi<'a> {
    fn new(table: &'a PiTable, primes: &'a [u32], a: usize, cache_z: usize) -> Phi<'a> {
        debug_assert!(cache_z <= 1 << 18);
        let sq = primes[..=a].iter().map(|&p| p as u64 * p as u64).collect();
        // phi(z, 0) = z: the product 1 with phi(1, 0) = 1 and t = [0]
        let mut wheel = vec![(1u64, 1u64, vec![0u16])];
        let mut prod = 1u64;
        for b in 1..=WHEEL {
            prod *= primes[b - 1] as u64;
            let mut c = 0u16;
            let t: Vec<u16> = (0..prod)
                .map(|r| {
                    if r > 0 && primes[..b].iter().all(|&q| r % q as u64 != 0) {
                        c += 1;
                    }
                    c
                })
                .collect();
            // P_b itself is not coprime to P_b, so phi(P_b, b) = t[P_b - 1]
            let f = t[prod as usize - 1] as u64;
            wheel.push((prod, f, t));
        }
        let mut phi = Phi { table, primes, sq, wheel, cache: vec![], cache_z: cache_z as u64 };
        // rows by phi(z, b) = phi(z, b - 1) - phi(z / p_b, b - 1)
        let mut row: Vec<u16> = (0..cache_z as u64).map(|z| phi.small(z, WHEEL) as u16).collect();
        for b in WHEEL + 1..(WHEEL + CACHE_ROWS).min(a + 1) {
            let p = primes[b - 1] as usize;
            let next: Vec<u16> = (0..cache_z).map(|z| row[z] - row[z / p]).collect();
            phi.cache.push(std::mem::replace(&mut row, next));
        }
        phi.cache.push(row);
        phi
    }

    /// phi(z, b) for b <= 6.
    #[inline]
    fn small(&self, z: u64, b: usize) -> i64 {
        let (p, f, t) = &self.wheel[b];
        ((z / p) * f + t[(z % p) as usize] as u64) as i64
    }

    /// phi(z, b) for z < p_(b+1)^2: 1 and the primes in (p_b, z].
    #[inline]
    fn truncated(&self, z: u64, b: usize) -> i64 {
        if z == 0 {
            return 0;
        }
        let pz = self.table.pi(z) as i64;
        1 + (pz - b as i64).max(0)
    }

    /// phi(z, b); `work` counts the loop iterations.
    fn phi(&self, z: u64, b: usize, work: &mut u64) -> i64 {
        if b <= WHEEL {
            return self.small(z, b);
        }
        if z < self.sq[b] {
            return self.truncated(z, b);
        }
        if z < self.cache_z && b - WHEEL < self.cache.len() {
            return self.cache[b - WHEEL][z as usize] as i64;
        }
        *work += (b - WHEEL) as u64;
        let mut s = self.small(z, WHEEL);
        for j in WHEEL..b {
            let w = z / self.primes[j] as u64;
            s -= if w < self.sq[j] { self.truncated(w, j) } else { self.phi(w, j, work) };
        }
        s
    }

    /// phi(x, a), its top-level subtrees phi(x / p_j, j) in parallel rounds on the current thread pool.
    fn top(&self, x: u64, a: usize, b: &mut dyn Budget) -> Res<i64> {
        use rayon::prelude::*;
        if x < self.sq[a] || a <= WHEEL {
            let mut work = 0;
            return Ok(self.phi(x, a, &mut work));
        }
        let mut s = self.small(x, WHEEL);
        let mut j = WHEEL;
        while j < a {
            let end = (j + ROUND).min(a);
            let (sum, work) = (j..end)
                .into_par_iter()
                .map(|j| {
                    let mut work = 0;
                    (self.phi(x / self.primes[j] as u64, j, &mut work), work)
                })
                .reduce(|| (0, 0), |p, q| (p.0 + q.0, p.1 + q.1));
            s -= sum;
            b.charge(work / 256 + 1)?;
            j = end;
        }
        Ok(s)
    }
}

/// pi(x), the number of primes up to x (x <= [`PI_MAX`]). The table and the top-level subtrees of phi are computed
/// in parallel on the current thread pool.
pub fn prime_pi(x: u64, b: &mut dyn Budget) -> Res<u64> {
    assert!(x <= PI_MAX);
    if x < DIRECT {
        return Ok(PiTable::new(x, b)?.pi(x));
    }
    let y = iroot(x, 3);
    let r = isqrt(x);
    let primes = primes_below(r + 1);
    let a = primes.partition_point(|&p| p as u64 <= y);
    let nb = primes.len();
    let next = primes[a] as u64;
    let limit = (x / y).max(next * next);
    let table = PiTable::new(limit, b)?;
    // the cache pays for itself only when the tree is large
    let cache_z = if x >= 1 << 36 { 1 << 18 } else { 1 << 14 };
    let phi = Phi::new(&table, &primes, a, cache_z);
    let phi_xa = phi.top(x, a, b)?;
    let mut p2 = 0i64;
    for (k, &p) in primes.iter().enumerate().take(nb).skip(a) {
        if k % 4096 == 0 {
            b.charge(4)?;
        }
        // 1-based index i = k + 1: pi(x / p_i) - i + 1
        p2 += table.pi(x / p as u64) as i64 - k as i64;
    }
    Ok((phi_xa + a as i64 - 1 - p2) as u64)
}

/// li(x) for x > 1, by the series gamma + ln ln x + sum (ln x)^n / (n n!).
fn li(x: f64) -> f64 {
    let l = x.ln();
    let (mut term, mut sum) = (1.0, 0.0);
    for n in 1..400 {
        term *= l / n as f64;
        let add = term / n as f64;
        sum += add;
        if add < 1e-17 * sum {
            break;
        }
    }
    std::f64::consts::EULER_GAMMA + l.ln() + sum
}

/// Riemann's R(x) = sum mu(n)/n li(x^(1/n)), truncated where x^(1/n) < 2.
fn riemann_r(x: f64) -> f64 {
    const MU: [i8; 13] = [0, 1, -1, -1, 0, -1, 1, -1, 0, 0, 1, -1, 0];
    let mut s = 0.0;
    for (n, &m) in MU.iter().enumerate().skip(1) {
        let r = x.powf(1.0 / n as f64);
        if r < 2.0 {
            break;
        }
        if m != 0 {
            s += m as f64 / n as f64 * li(r);
        }
    }
    s
}

/// An estimate of the k-th prime (k >= 2): R^(-1)(k) by Newton's iteration.
fn estimate_nth(k: u64) -> f64 {
    let kf = k as f64;
    let mut x = kf * kf.ln().max(1.0);
    for _ in 0..40 {
        let step = (riemann_r(x) - kf) * x.ln();
        x = (x - step).max(2.0);
        if step.abs() < 0.5 {
            break;
        }
    }
    x
}

/// The k-th prime (k >= 1), for k-th primes up to about [`PI_MAX`] (`None` beyond).
pub fn nth_prime(k: u64, b: &mut dyn Budget) -> Res<Option<u64>> {
    assert!(k >= 1);
    if k < 6 {
        return Ok(Some([2, 3, 5, 7, 11][k as usize - 1]));
    }
    let kf = k as f64;
    // Rosser: p_k < k (ln k + ln ln k) for k >= 6
    let bound = kf * (kf.ln() + kf.ln().ln());
    if bound < DIRECT as f64 {
        let ps = primes_below(bound as u64 + 1);
        return Ok(Some(ps[k as usize - 1] as u64));
    }
    let x0 = estimate_nth(k) as u64;
    if x0 > PI_MAX - (1 << 30) {
        return Ok(None);
    }
    let c = prime_pi(x0, b)?;
    // the average gap near x0 sizes the windows
    let gap = (x0 as f64).ln();
    if c >= k {
        // p_c is the largest prime <= x0; step down c - k primes from it
        let mut need = (c - k + 1) as usize;
        let mut hi = x0 + 1;
        loop {
            let w = ((need as f64 * gap * 1.2) as u64).max(1 << 16).min(hi);
            let ps = primes_in(hi - w, hi, b)?;
            if ps.len() >= need {
                return Ok(Some(ps[ps.len() - need]));
            }
            need -= ps.len();
            hi -= w;
        }
    }
    let mut need = (k - c) as usize;
    let mut lo = x0 + 1;
    loop {
        let w = ((need as f64 * gap * 1.2) as u64).max(1 << 16);
        let ps = primes_in(lo, lo + w, b)?;
        if ps.len() >= need {
            return Ok(Some(ps[need - 1]));
        }
        need -= ps.len();
        lo += w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::{Limited, Unlimited};

    /// pi(x) on the 4-thread test pool.
    fn pi(x: u64) -> u64 {
        crate::test_pool().install(|| prime_pi(x, &mut Unlimited).unwrap())
    }

    fn nth(k: u64) -> Option<u64> {
        crate::test_pool().install(|| nth_prime(k, &mut Unlimited).unwrap())
    }

    #[test]
    fn small_counts() {
        for (x, want) in [(0u64, 0u64), (1, 0), (2, 1), (3, 2), (10, 4), (100, 25), (1000, 168), (1_000_000, 78498)] {
            assert_eq!(pi(x), want);
        }
    }

    #[test]
    fn meissel_against_the_table() {
        let t = crate::test_pool().install(|| PiTable::new(60_000_000, &mut Unlimited).unwrap());
        for x in [DIRECT, DIRECT + 1, 10_000_000, 12_345_678, 31_622_777, 59_999_999, 60_000_000] {
            assert_eq!(pi(x), t.pi(x), "{x}");
        }
    }

    #[test]
    fn known_values() {
        assert_eq!(pi(100_000_000), 5_761_455);
        assert_eq!(pi(1_000_000_000), 50_847_534);
        assert_eq!(pi(10_000_000_000), 455_052_511);
        // pi(2^32)
        assert_eq!(pi(1 << 32), 203_280_221);
    }

    #[test]
    fn nth_primes() {
        let ps = primes_below(2_000_000);
        for k in [1u64, 2, 3, 5, 6, 7, 100, 1000, 10_000, 100_000, 148_933] {
            assert_eq!(nth(k), Some(ps[k as usize - 1] as u64), "{k}");
        }
        assert_eq!(nth(1_000_000), Some(15_485_863));
        assert_eq!(nth(100_000_000), Some(2_038_074_743));
    }

    #[test]
    fn counting_stops_on_the_budget() {
        assert!(crate::test_pool().install(|| prime_pi(10_000_000_000, &mut Limited { left: 10 }).is_err()));
    }
}
