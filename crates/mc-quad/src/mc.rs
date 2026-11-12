//! Monte Carlo (plan 10, section 2.3): plain sampling, and VEGAS (Lepage 1978) with the adaptive stratification
//! of VEGAS+ (Lepage 2021).
//!
//! **Random numbers** (decision quad-1.8): counter-based Philox streams (Salmon et al. 2011, `mc-random`). The
//! points of a run are cut into blocks of [`BLOCK`] by position, and block b of pass (or iteration) i draws from
//! `Stream::from_key(seed).split(i).split(b)`. Blocks are generated in parallel, values are evaluated through the
//! sampler, and every sum is added in point order on the calling thread, so results depend on `seed` only, never
//! on the number of threads.
//!
//! **VEGAS+**: the y-cube is cut into ns^d hypercubes (ns = floor((n/4)^(1/d)), at least 1), and each hypercube h
//! gets n_h = max(2, n σ_h^β / Σ σ^β) samples (β = 3/4; σ_h the standard deviation of its contribution in the
//! previous iteration). Within a hypercube y is uniform and is mapped to the unit cube by the VEGAS map: per axis
//! [`ninc`] increments, x = x_i + Δ_i (n_inc y - i), Jacobian n_inc Δ_i. After each iteration the increments are
//! refined so that each holds an equal share of the smoothed, compressed (α = 1/2) sums of (J f)^2 that fell in
//! it. The first [`WARMUP`] iterations only train; the estimate is the weighted average of the later ones, with
//! χ² per degree of freedom to show inconsistent iterations.
use crate::domain::{Sampler, CHUNK};
use crate::multi::Estimate;
use crate::options::Goals;
use mc_eval::{NativeCx, Res};
use mc_random::Stream;
use rayon::prelude::*;

/// Points per random-stream block.
pub(crate) const BLOCK: usize = 2048;

/// Training iterations of VEGAS before the averaged ones.
pub(crate) const WARMUP: usize = 5;

/// Averaged VEGAS iterations needed before the goal is checked (so that χ² has two degrees of freedom).
const MIN_ITERATIONS: usize = 3;

/// The default evaluations per VEGAS iteration.
pub(crate) const VEGAS_POINTS: u64 = 10_000;

/// The first pass of plain Monte Carlo.
const FIRST_PASS: u64 = 1 << 14;

/// Damping of the map refinement and of the hypercube allocation.
const ALPHA: f64 = 0.5;
const BETA: f64 = 0.75;

/// The most variables these methods take.
pub(crate) const MAX_DIMS: usize = 64;

/// A uniform number in (0, 1) from 64 random bits.
#[inline]
fn unit(w: u64) -> f64 {
    ((w >> 11) as f64 + 0.5) * (1.0 / 9_007_199_254_740_992.0)
}

/// Fill `u` (points `p0 ..` of a pass, each `d` coordinates) from the pass's blocks; `p0` is a multiple of
/// [`BLOCK`]. `fill(stream, first point, coordinates)` writes one block.
fn generate(base: &Stream, p0: u64, d: usize, u: &mut [f64], fill: &(dyn Fn(&mut Stream, u64, &mut [f64]) + Sync)) {
    let one = |(b, c): (usize, &mut [f64])| {
        let first = p0 + (b * BLOCK) as u64;
        let mut st = base.split(first / BLOCK as u64);
        fill(&mut st, first, c);
    };
    if u.len() >= 2 * BLOCK * d && rayon::current_num_threads() > 1 {
        u.par_chunks_mut(BLOCK * d).enumerate().for_each(one);
    } else {
        u.chunks_mut(BLOCK * d).enumerate().for_each(one);
    }
}

/// Mean and sum of squared deviations of a sample, combined pass by pass (Chan, Golub and LeVeque).
#[derive(Clone, Copy, Default)]
struct Moments {
    n: f64,
    mean: f64,
    m2: f64,
}

impl Moments {
    fn of(xs: &[f64]) -> Moments {
        let n = xs.len() as f64;
        if n == 0.0 {
            return Moments::default();
        }
        let mean = xs.iter().sum::<f64>() / n;
        let m2 = xs.iter().map(|&x| (x - mean) * (x - mean)).sum();
        Moments { n, mean, m2 }
    }

    fn add(self, o: Moments) -> Moments {
        if self.n == 0.0 {
            return o;
        }
        let n = self.n + o.n;
        let delta = o.mean - self.mean;
        Moments { n, mean: self.mean + delta * o.n / n, m2: self.m2 + o.m2 + delta * delta * self.n * o.n / n }
    }
}

/// Plain Monte Carlo: uniform points in passes that double until the error meets the goal or `max_points` would
/// be exceeded; `points`: exactly that many points instead.
pub(crate) fn plain<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    s: &mut Sampler<'h>,
    goals: Goals,
    max_points: u64,
    points: Option<u64>,
    seed: u64,
) -> Res<Result<Estimate, Vec<f64>>> {
    let d = s.dim();
    let base = Stream::from_key(seed);
    let mut acc = Moments::default();
    let mut done = 0u64;
    let mut target = points.unwrap_or(FIRST_PASS).min(max_points.max(1)).max(2);
    let mut u = vec![];
    let mut f = vec![];
    let fill = |st: &mut Stream, _first: u64, c: &mut [f64]| c.iter_mut().for_each(|v| *v = unit(st.next_u64()));
    loop {
        while done < target {
            cx.tick(1)?;
            let n = ((target - done) as usize).min(CHUNK);
            u.resize(n * d, 0.0);
            f.resize(n, 0.0);
            generate(&base, done, d, &mut u, &fill);
            if let Err(p) = s.eval(cx, &u, &mut f)? {
                return Ok(Err(p));
            }
            acc = acc.add(Moments::of(&f));
            done += n as u64;
        }
        let err = (acc.m2 / (acc.n - 1.0).max(1.0) / acc.n).sqrt();
        let converged = err <= goals.tol(acc.mean.abs());
        if points.is_some() || converged || 2 * target > max_points {
            return Ok(Ok(Estimate { value: acc.mean, err, converged, calls: s.calls, worst: None, chi2: None }));
        }
        target *= 2;
    }
}

/// The VEGAS map of one axis: increment edges from 0 to 1.
struct Axis {
    edges: Vec<f64>,
}

impl Axis {
    fn new(n: usize) -> Axis {
        Axis { edges: (0..=n).map(|i| i as f64 / n as f64).collect() }
    }

    fn ninc(&self) -> usize {
        self.edges.len() - 1
    }

    /// x(y), its derivative and the increment of y.
    #[inline]
    fn map(&self, y: f64) -> (f64, f64, usize) {
        let n = self.ninc();
        let t = y * n as f64;
        let i = (t as usize).min(n - 1);
        let (a, b) = (self.edges[i], self.edges[i + 1]);
        (a + (b - a) * (t - i as f64), n as f64 * (b - a), i)
    }

    /// Refine the increments from the sums `d` of (J f)^2 per increment.
    fn refine(&mut self, d: &[f64]) {
        let n = self.ninc();
        if n < 2 || !d.iter().any(|&v| v > 0.0) || !d.iter().all(|v| v.is_finite()) {
            return;
        }
        let mut s: Vec<f64> = (0..n)
            .map(|i| match i {
                0 => (7.0 * d[0] + d[1]) / 8.0,
                _ if i == n - 1 => (d[n - 2] + 7.0 * d[n - 1]) / 8.0,
                _ => (d[i - 1] + 6.0 * d[i] + d[i + 1]) / 8.0,
            })
            .collect();
        let total: f64 = s.iter().sum();
        for v in s.iter_mut() {
            let r = *v / total;
            *v = if r > 0.0 && r < 1.0 {
                ((1.0 - r) / -r.ln()).powf(ALPHA)
            } else if r >= 1.0 {
                1.0
            } else {
                0.0
            };
        }
        let total: f64 = s.iter().sum();
        if total <= 0.0 || !total.is_finite() {
            return;
        }
        let share = total / n as f64;
        let mut edges = vec![0.0; n + 1];
        let (mut acc, mut i) = (0.0, 0usize);
        for (k, e) in edges.iter_mut().enumerate().take(n).skip(1) {
            let need = k as f64 * share;
            while i < n - 1 && acc + s[i] < need {
                acc += s[i];
                i += 1;
            }
            let frac = if s[i] > 0.0 { ((need - acc) / s[i]).clamp(0.0, 1.0) } else { 0.0 };
            *e = self.edges[i] + frac * (self.edges[i + 1] - self.edges[i]);
        }
        edges[n] = 1.0;
        // keep the edges increasing (rounding)
        for k in 1..=n {
            if edges[k] < edges[k - 1] {
                edges[k] = edges[k - 1];
            }
        }
        self.edges = edges;
    }
}

/// The options of a VEGAS run.
pub(crate) struct VegasPlan {
    /// evaluations per iteration (the first; later ones may grow, see [`vegas`])
    pub points: u64,
    /// a fixed number of averaged iterations instead of the goal
    pub iterations: Option<usize>,
    /// the evaluation budget, training included
    pub max_points: u64,
    /// the random seed
    pub seed: u64,
}

/// VEGAS+ (see the module notes).
pub(crate) fn vegas<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    s: &mut Sampler<'h>,
    goals: Goals,
    plan: &VegasPlan,
) -> Res<Result<Estimate, Vec<f64>>> {
    let d = s.dim();
    let mut neval = plan.points.max(64);
    let ninc = ((neval / 10) as usize).clamp(50, 1000);
    let mut axes: Vec<Axis> = (0..d).map(|_| Axis::new(ninc)).collect();
    // hypercubes: ns per axis, at most n/4 of them
    let mut ns = ((neval as f64 / 4.0).powf(1.0 / d as f64).floor() as usize).max(1);
    while ns > 1 && (ns as f64).powi(d as i32) > neval as f64 / 4.0 {
        ns -= 1;
    }
    let cubes = ns.pow(d as u32);
    let vol = 1.0 / cubes as f64;
    let mut sigma: Vec<f64> = vec![1.0; cubes];
    let base = Stream::from_key(plan.seed);
    // per iteration: (estimate, variance)
    let mut results: Vec<(f64, f64)> = vec![];
    let mut it = 0usize;
    let mut u = vec![];
    let mut f = vec![];
    let mut y = vec![];
    loop {
        // the allocation of samples to hypercubes
        let total: f64 = sigma.iter().map(|&v| v.powf(BETA)).sum();
        let counts: Vec<u64> = sigma
            .iter()
            .map(|&v| {
                let share = if total > 0.0 && total.is_finite() { v.powf(BETA) / total } else { 1.0 / cubes as f64 };
                ((neval as f64 * share) as u64).max(2)
            })
            .collect();
        let mut starts = Vec::with_capacity(cubes + 1);
        starts.push(0u64);
        for &c in &counts {
            starts.push(starts.last().unwrap() + c);
        }
        let n = *starts.last().unwrap();
        if s.calls + n > plan.max_points && !results.is_empty() {
            break;
        }
        let stream = base.split(it as u64);
        let (mut sum, mut sum2) = (vec![0.0f64; cubes], vec![0.0f64; cubes]);
        let mut train = vec![vec![0.0f64; ninc]; d];
        let mut done = 0u64;
        while done < n {
            cx.tick(1)?;
            let m = ((n - done) as usize).min(CHUNK);
            y.resize(m * d, 0.0);
            u.resize(m * d, 0.0);
            f.resize(m, 0.0);
            // y uniform in the sample's hypercube
            let fill = |st: &mut Stream, first: u64, c: &mut [f64]| {
                let mut h = starts.partition_point(|&v| v <= first) - 1;
                for (i, p) in c.chunks_mut(d).enumerate() {
                    let k = first + i as u64;
                    while starts[h + 1] <= k {
                        h += 1;
                    }
                    let mut idx = h;
                    for v in p.iter_mut() {
                        let j = idx % ns;
                        idx /= ns;
                        *v = (j as f64 + unit(st.next_u64())) / ns as f64;
                    }
                }
            };
            generate(&stream, done, d, &mut y, &fill);
            // through the map: u and the map's Jacobian
            let mut jac = vec![1.0f64; m];
            for i in 0..m {
                for k in 0..d {
                    let (x, dx, _) = axes[k].map(y[i * d + k]);
                    u[i * d + k] = x.clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON * 0.5);
                    jac[i] *= dx;
                }
            }
            if let Err(p) = s.eval(cx, &u, &mut f)? {
                return Ok(Err(p));
            }
            let mut h = starts.partition_point(|&v| v <= done) - 1;
            for i in 0..m {
                let k = done + i as u64;
                while starts[h + 1] <= k {
                    h += 1;
                }
                let v = f[i] * jac[i];
                sum[h] += v;
                sum2[h] += v * v;
                let v2 = v * v;
                for (a, ax) in axes.iter().enumerate() {
                    let (_, _, inc) = ax.map(y[i * d + a]);
                    train[a][inc] += v2;
                }
            }
            done += m as u64;
        }
        // the iteration's estimate and variance; the hypercubes' spreads for the next allocation
        let (mut est, mut var) = (0.0, 0.0);
        for h in 0..cubes {
            let c = counts[h] as f64;
            let mean = sum[h] / c;
            let s2 = ((sum2[h] - sum[h] * mean) / (c - 1.0)).max(0.0);
            est += vol * mean;
            var += vol * vol * s2 / c;
            sigma[h] = vol * s2.sqrt();
        }
        for (a, ax) in axes.iter_mut().enumerate() {
            ax.refine(&train[a]);
        }
        if it >= WARMUP {
            results.push((est, var));
        }
        it += 1;
        if it > WARMUP {
            let (value, err, _) = combine(&results);
            let k = results.len();
            let enough = match plan.iterations {
                Some(t) => k >= t,
                None => k >= MIN_ITERATIONS && err <= goals.tol(value.abs()),
            };
            if enough {
                break;
            }
            // many more iterations needed at this size: double the points per iteration (deterministic)
            if plan.iterations.is_none() && k >= MIN_ITERATIONS {
                let tol = goals.tol(value.abs());
                if tol > 0.0 && (err / tol).powi(2) * k as f64 > (k + 10) as f64 {
                    neval = (neval * 2).min(1 << 22);
                }
            }
        }
    }
    let (value, err, chi2) = combine(&results);
    let converged = err <= goals.tol(value.abs());
    let dof = results.len().saturating_sub(1);
    Ok(Ok(Estimate { value, err, converged, calls: s.calls, worst: None, chi2: (dof > 0).then_some((chi2, dof)) }))
}

/// The weighted average of iteration results, its error and χ² per degree of freedom.
fn combine(r: &[(f64, f64)]) -> (f64, f64, f64) {
    if r.is_empty() {
        return (0.0, f64::INFINITY, 0.0);
    }
    let floor = |e: f64, v: f64| v.max((e * f64::EPSILON).powi(2)).max(f64::MIN_POSITIVE);
    let (mut sw, mut swx) = (0.0, 0.0);
    for &(e, v) in r {
        let w = 1.0 / floor(e, v);
        sw += w;
        swx += w * e;
    }
    let mean = swx / sw;
    let chi2: f64 = r.iter().map(|&(e, v)| (e - mean).powi(2) / floor(e, v)).sum();
    let dof = r.len().saturating_sub(1).max(1) as f64;
    (mean, (1.0 / sw).sqrt(), chi2 / dof)
}

#[cfg(test)]
mod tests {
    use super::Axis;

    /// Refining on data concentrated near 0 moves increments there, keeping the edges increasing from 0 to 1.
    #[test]
    fn refinement_follows_the_data() {
        let mut a = Axis::new(100);
        let d: Vec<f64> = (0..100).map(|i| (-(i as f64) / 5.0).exp()).collect();
        for _ in 0..5 {
            a.refine(&d);
        }
        assert_eq!(a.edges[0], 0.0);
        assert_eq!(a.edges[100], 1.0);
        assert!(a.edges.windows(2).all(|w| w[0] <= w[1]));
        assert!(a.edges[50] < 0.3, "{}", a.edges[50]);
    }
}
