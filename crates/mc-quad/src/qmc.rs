//! Randomised quasi-Monte Carlo (plan 10, section 2.3): a rank-1 lattice sequence with random shifts and the
//! tent (baker's) transformation, and Sobol points with a nested uniform (Owen) scrambling.
//!
//! - **Lattice** (Dick, Kuo and Sloan 2013): point k of the sequence is frac(φ(k) z), φ the base-2 radical inverse,
//!   so the first 2^m points form the lattice with generating vector z and 2^m points, and doubling keeps the
//!   points already used. Each replicate adds its own uniform shift Δ, and the tent map 1 - |2x - 1| makes the
//!   error of smooth, non-periodic integrands fall almost like 1/N^2 (Hickernell 2002).
//! - **Sobol** (Joe and Kuo 2008 construction, this crate's own parameters): the digital sequence in Gray-code
//!   order; each replicate scrambles every coordinate by a hash-based nested uniform scrambling (Owen 1995, in the
//!   form of Burley 2020: in bit-reversed order, operations in which a bit depends only on lower bits).
//!
//! The estimate is the mean of [`REPLICATES`] independent randomisations, its error the standard error of that
//! mean (decision quad-1.7). The number of points per replicate doubles from 2^10 until the error meets the goal
//! or the next doubling would exceed `max_points`. Randomisations are drawn from `seed` only, points are generated
//! in blocks by position, and sums are added in point order, so results do not depend on the number of threads.
use crate::domain::{Sampler, CHUNK};
use crate::multi::Estimate;
use crate::options::Goals;
use crate::qmc_tables::{DIMS, LATTICE, SOBOL};
use mc_eval::{NativeCx, Res};
use mc_random::philox::mix64;
use mc_random::Stream;
use rayon::prelude::*;

/// Independent randomisations.
pub(crate) const REPLICATES: usize = 10;

/// The first number of points per replicate: 2^FIRST.
const FIRST: u32 = 10;

/// Points generated per parallel task.
const GEN_BLOCK: usize = 1024;

/// The most variables these methods take.
pub(crate) const MAX_DIMS: usize = DIMS;

/// Which point set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// the rank-1 lattice sequence
    Lattice,
    /// scrambled Sobol points
    Sobol,
}

/// The Sobol direction numbers of every dimension (bit 31 the first digit).
fn sobol_directions(d: usize) -> Vec<[u32; 32]> {
    let mut out = vec![std::array::from_fn(|b| 1u32 << (31 - b))];
    for &(s, a, m) in SOBOL.iter().take(d.saturating_sub(1)) {
        let mut mm: Vec<u64> = m.iter().map(|&v| v as u64).collect();
        for k in s as usize..32 {
            let mut v = mm[k - s as usize] ^ (mm[k - s as usize] << s);
            for i in 1..s {
                if a >> (s - 1 - i) & 1 == 1 {
                    v ^= mm[k - i as usize] << i;
                }
            }
            mm.push(v);
        }
        out.push(std::array::from_fn(|b| (mm[b] << (31 - b)) as u32));
    }
    out
}

/// Nested uniform scrambling of the 32 digits of `x` keyed by `seed` (Burley 2020): in bit-reversed order every
/// step changes a bit only as a function of the lower bits, that is, of the earlier digits.
#[inline]
fn scramble(x: u32, seed: u64) -> u32 {
    let mut v = x.reverse_bits();
    v = v.wrapping_add(seed as u32);
    v ^= v.wrapping_mul(0x6c50_b47c);
    v ^= v.wrapping_mul(0xb82f_1e52);
    v = v.wrapping_add((seed >> 32) as u32 & !1);
    v ^= v.wrapping_mul(0xc7af_e638);
    v ^= v.wrapping_mul(0x8d22_f6e6);
    v.reverse_bits()
}

/// A coordinate in (0, 1) from 32 digits.
#[inline]
fn unit(x: u32) -> f64 {
    (x as f64 + 0.5) * (1.0 / 4_294_967_296.0)
}

/// The point generator of one replicate.
struct Points {
    kind: Kind,
    d: usize,
    /// lattice: the shifts; Sobol: unused
    shift: Vec<f64>,
    /// Sobol: the direction numbers and the scrambling keys
    dirs: Vec<[u32; 32]>,
    keys: Vec<u64>,
}

impl Points {
    fn new(kind: Kind, d: usize, seed: u64, replicate: usize) -> Points {
        let mut st = Stream::from_key(seed).split(replicate as u64);
        let unit53 = |w: u64| ((w >> 11) as f64 + 0.5) * (1.0 / 9_007_199_254_740_992.0);
        match kind {
            Kind::Lattice => {
                Points { kind, d, shift: (0..d).map(|_| unit53(st.next_u64())).collect(), dirs: vec![], keys: vec![] }
            }
            Kind::Sobol => Points {
                kind,
                d,
                shift: vec![],
                dirs: sobol_directions(d),
                keys: (0..d).map(|j| mix64(st.next_u64() ^ j as u64)).collect(),
            },
        }
    }

    /// Points `k0 .. k0 + n` into `u` (each `d` coordinates); `k0` is a multiple of `n` when n is a power of two.
    fn fill(&self, k0: u64, u: &mut [f64]) {
        let d = self.d;
        match self.kind {
            Kind::Lattice => {
                for (i, p) in u.chunks_mut(d).enumerate() {
                    let r = ((k0 + i as u64) as u32).reverse_bits() as u64;
                    for j in 0..d {
                        let x = ((r * LATTICE[j] as u64) as u32) as f64 * (1.0 / 4_294_967_296.0) + self.shift[j];
                        let x = x - x.floor();
                        // the tent map, kept inside (0, 1)
                        let t = 1.0 - (2.0 * x - 1.0).abs();
                        p[j] = t.clamp(f64::EPSILON * 0.5, 1.0 - f64::EPSILON * 0.5);
                    }
                }
            }
            Kind::Sobol => {
                // Gray-code order: point k is the Sobol point of index k ^ (k >> 1)
                let gray = |k: u64| k ^ (k >> 1);
                let mut x: Vec<u32> = (0..d)
                    .map(|j| {
                        let g = gray(k0);
                        (0..32).filter(|&b| g >> b & 1 == 1).fold(0u32, |acc, b| acc ^ self.dirs[j][b])
                    })
                    .collect();
                for (i, p) in u.chunks_mut(d).enumerate() {
                    if i > 0 {
                        let c = (k0 + i as u64).trailing_zeros() as usize;
                        for (xj, dj) in x.iter_mut().zip(&self.dirs) {
                            *xj ^= dj[c.min(31)];
                        }
                    }
                    for ((pj, &xj), &key) in p.iter_mut().zip(&x).zip(&self.keys) {
                        *pj = unit(scramble(xj, key));
                    }
                }
            }
        }
    }
}

/// The sum of the sampler's values at points `k0 .. k1` of a replicate, added in point order.
fn sum<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    s: &mut Sampler<'h>,
    pts: &Points,
    k0: u64,
    k1: u64,
) -> Res<Result<f64, Vec<f64>>> {
    let d = s.dim();
    let mut total = 0.0;
    let mut u = vec![];
    let mut f = vec![];
    let mut k = k0;
    while k < k1 {
        cx.tick(1)?;
        let n = ((k1 - k) as usize).min(CHUNK);
        u.resize(n * d, 0.0);
        f.resize(n, 0.0);
        if n >= 2 * GEN_BLOCK && rayon::current_num_threads() > 1 {
            u.par_chunks_mut(GEN_BLOCK * d).enumerate().for_each(|(b, c)| pts.fill(k + (b * GEN_BLOCK) as u64, c));
        } else {
            for (b, c) in u.chunks_mut(GEN_BLOCK * d).enumerate() {
                pts.fill(k + (b * GEN_BLOCK) as u64, c);
            }
        }
        if let Err(p) = s.eval(cx, &u, &mut f)? {
            return Ok(Err(p));
        }
        total += f.iter().sum::<f64>();
        k += n as u64;
    }
    Ok(Ok(total))
}

/// The randomised quasi-Monte Carlo estimate. `points`: a fixed number of evaluations in all (rounded down to
/// [`REPLICATES`] times a power of two, at least 2^4 each), instead of the goal-driven doubling.
pub(crate) fn integrate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    s: &mut Sampler<'h>,
    kind: Kind,
    goals: Goals,
    max_points: u64,
    points: Option<u64>,
    seed: u64,
) -> Res<Result<Estimate, Vec<f64>>> {
    let d = s.dim();
    let reps: Vec<Points> = (0..REPLICATES).map(|r| Points::new(kind, d, seed, r)).collect();
    let mut sums = [0.0f64; REPLICATES];
    let r = REPLICATES as u64;
    let fixed = points.map(|p| (p / r).max(16).ilog2());
    let mut m = fixed.unwrap_or(FIRST).min(FIRST);
    let mut done = 0u64;
    loop {
        let n = 1u64 << m;
        for (k, pts) in reps.iter().enumerate() {
            match sum(cx, s, pts, done, n)? {
                Ok(v) => sums[k] += v,
                Err(p) => return Ok(Err(p)),
            }
        }
        done = n;
        let means: Vec<f64> = sums.iter().map(|&v| v / n as f64).collect();
        let mean = means.iter().sum::<f64>() / r as f64;
        let var = means.iter().map(|&v| (v - mean) * (v - mean)).sum::<f64>() / (r - 1) as f64;
        let err = (var / r as f64).sqrt();
        let converged = err <= goals.tol(mean.abs());
        let stop = match fixed {
            Some(f) => m >= f,
            None => converged || s.calls + r * n > max_points || m >= 32,
        };
        if stop {
            return Ok(Ok(Estimate { value: mean, err, converged, calls: s.calls, worst: None, chi2: None }));
        }
        m += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first 2^m points of every dimension are a permutation of the 2^m equal cells (before scrambling and
    /// shifts): stratification in one dimension.
    #[test]
    fn one_dimensional_stratification() {
        let dirs = sobol_directions(DIMS);
        for m in [1u32, 4, 10] {
            for (j, v) in dirs.iter().enumerate() {
                let mut seen = vec![false; 1 << m];
                for k in 0u64..1 << m {
                    let x = (0..32).filter(|&b| k >> b & 1 == 1).fold(0u32, |acc, b| acc ^ v[b]);
                    let cell = (x >> (32 - m)) as usize;
                    assert!(!seen[cell], "sobol dim {j} m {m}");
                    seen[cell] = true;
                }
                let z = LATTICE[j] as u64;
                let mut seen = vec![false; 1 << m];
                for k in 0u64..1 << m {
                    let r = (k as u32).reverse_bits() as u64;
                    let x = (r * z) as u32;
                    let cell = (x >> (32 - m)) as usize;
                    assert!(!seen[cell], "lattice dim {j} m {m}");
                    seen[cell] = true;
                }
            }
        }
    }

    /// Scrambling is a bijection that keeps the cells of every level: points in one cell of size 2^-k stay
    /// together.
    #[test]
    fn scrambling_is_nested() {
        for seed in [0u64, 1, 0xdead_beef_1234_5678] {
            for k in 1..8 {
                for x in (0u32..64).map(|i| i.wrapping_mul(0x9e37_79b9)) {
                    let y = x ^ (1u32 << (31 - k)).wrapping_sub(1) & (x.wrapping_mul(7));
                    // same first k digits before, same first k digits after
                    if x >> (32 - k) == y >> (32 - k) {
                        assert_eq!(scramble(x, seed) >> (32 - k), scramble(y, seed) >> (32 - k));
                    }
                }
            }
        }
    }
}
