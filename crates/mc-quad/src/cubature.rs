//! Adaptive cubature on hyper-rectangles (plan 10, section 2.2): the degree-7 rule of Genz and Malik (1980) with
//! its embedded degree-5 rule, a heap of regions ordered by error, and bisection along the coordinate with the
//! largest fourth difference (Genz and Malik 1980; Berntsen, Espelid and Genz 1991).
//!
//! The rule on the region c + h·[-1, 1]^d (volume V = 2^d prod h) uses 2^d + 2d^2 + 2d + 1 points: the centre,
//! ±λ2 and ±λ3 on each axis, (±λ4, ±λ4) in each pair of axes, and the 2^d points (±λ5, ..., ±λ5), with
//!
//! ```text
//! λ2 = sqrt(9/70), λ3 = λ4 = sqrt(9/10), λ5 = sqrt(9/19)
//! I7 = V (w1 f0 + w2 Σ2 + w3 Σ3 + w4 Σ4 + w5 Σ5)       I5 = V (v1 f0 + v2 Σ2 + v3 Σ3 + v4 Σ4)
//! w1 = (12824 - 9120 d + 400 d^2)/19683, w2 = 980/6561, w3 = (1820 - 400 d)/19683, w4 = 200/19683,
//! w5 = 6859/(19683 2^d); v1 = (729 - 950 d + 50 d^2)/729, v2 = 245/486, v3 = (265 - 100 d)/1458, v4 = 25/729
//! ```
//!
//! The error estimate of a region is |I7 - I5| (at least 50 eps times the rule applied to |f|); it is split in
//! two along the axis i with the largest |f(+λ2) + f(-λ2) - 2 f0 - (λ2/λ3)^2 (f(+λ3) + f(-λ3) - 2 f0)| (ties:
//! the widest axis). Every point lies inside its region, so singularities on faces are never sampled.
//!
//! **Blocks** (decision quad-1.3): each round takes the regions of largest error from the heap until their errors
//! add up to what must be removed to meet the goal (at least one, at most [`ROUND_POINTS`] points of children),
//! and evaluates all their children with one call of the integrand, which then runs in parallel. The rounds depend
//! on the values only, so results do not depend on the number of threads.
use crate::domain::Sampler;
use crate::machine::ROUNDOFF;
use crate::multi::Estimate;
use crate::options::Goals;
use mc_eval::{NativeCx, Res};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// The most points of children evaluated in one round.
const ROUND_POINTS: usize = 1 << 14;

/// A region is not split along an axis narrower than this (in the unit cube).
const MIN_WIDTH: f64 = 1e-13;

/// The degree-7/5 rule in d dimensions: offsets in units of the half-widths and the weights of each point.
pub(crate) struct Rule {
    d: usize,
    /// offsets, `len() * d`
    offsets: Vec<f64>,
    /// degree-7 weights (for a region of volume 1)
    w7: Vec<f64>,
    /// degree-5 weights
    w5: Vec<f64>,
}

impl Rule {
    /// The rule for `d` dimensions (d >= 1).
    pub fn new(d: usize) -> Rule {
        let l2 = (9.0f64 / 70.0).sqrt();
        let l3 = (9.0f64 / 10.0).sqrt();
        let l4 = l3;
        let l5 = (9.0f64 / 19.0).sqrt();
        let df = d as f64;
        let w = [
            (12824.0 - 9120.0 * df + 400.0 * df * df) / 19683.0,
            980.0 / 6561.0,
            (1820.0 - 400.0 * df) / 19683.0,
            200.0 / 19683.0,
            6859.0 / 19683.0 / 2f64.powi(d as i32),
        ];
        let v = [
            (729.0 - 950.0 * df + 50.0 * df * df) / 729.0,
            245.0 / 486.0,
            (265.0 - 100.0 * df) / 1458.0,
            25.0 / 729.0,
            0.0,
        ];
        let mut r = Rule { d, offsets: vec![], w7: vec![], w5: vec![] };
        let push = |off: Vec<f64>, k: usize, r: &mut Rule| {
            r.offsets.extend(off);
            r.w7.push(w[k]);
            r.w5.push(v[k]);
        };
        push(vec![0.0; d], 0, &mut r);
        for i in 0..d {
            for (l, k) in [(l2, 1), (-l2, 1), (l3, 2), (-l3, 2)] {
                let mut o = vec![0.0; d];
                o[i] = l;
                push(o, k, &mut r);
            }
        }
        for i in 0..d {
            for j in i + 1..d {
                for (a, b) in [(l4, l4), (l4, -l4), (-l4, l4), (-l4, -l4)] {
                    let mut o = vec![0.0; d];
                    o[i] = a;
                    o[j] = b;
                    push(o, 3, &mut r);
                }
            }
        }
        for m in 0..1usize << d {
            let o: Vec<f64> = (0..d).map(|i| if m >> i & 1 == 1 { -l5 } else { l5 }).collect();
            push(o, 4, &mut r);
        }
        r
    }

    /// The number of points.
    pub fn len(&self) -> usize {
        self.w7.len()
    }
}

/// A region with its estimates.
#[derive(Clone, Debug)]
struct Region {
    c: Vec<f64>,
    h: Vec<f64>,
    value: f64,
    err: f64,
    /// the rule applied to |f|, with |weights|: the scale of roundoff
    abs: f64,
    /// the axis to split
    split: usize,
}

/// A heap entry: the largest error first; ties by creation order (deterministic).
struct Entry(Region, u64);

impl PartialEq for Entry {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Entry {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.err.total_cmp(&o.0.err).then(o.1.cmp(&self.1))
    }
}

/// Apply the rule to the regions `(c, h)` with one call of the integrand.
fn evaluate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    s: &mut Sampler<'h>,
    rule: &Rule,
    regions: Vec<(Vec<f64>, Vec<f64>)>,
) -> Res<Result<Vec<Region>, Vec<f64>>> {
    let (d, np) = (rule.d, rule.len());
    let mut u = Vec::with_capacity(regions.len() * np * d);
    for (c, h) in &regions {
        for p in 0..np {
            for i in 0..d {
                u.push(c[i] + h[i] * rule.offsets[p * d + i]);
            }
        }
    }
    let mut f = vec![0.0; regions.len() * np];
    if let Err(p) = s.eval(cx, &u, &mut f)? {
        return Ok(Err(p));
    }
    let ratio = (9.0 / 70.0) / (9.0 / 10.0);
    let mut out = Vec::with_capacity(regions.len());
    for (k, (c, h)) in regions.into_iter().enumerate() {
        let f = &f[k * np..(k + 1) * np];
        let vol: f64 = h.iter().map(|&x| 2.0 * x).product();
        let (mut i7, mut i5, mut abs) = (0.0, 0.0, 0.0);
        for ((&v, &w7), &w5) in f.iter().zip(&rule.w7).zip(&rule.w5) {
            i7 += w7 * v;
            i5 += w5 * v;
            abs += w7.abs() * v.abs();
        }
        // the axis with the largest fourth difference; near-ties go to the widest axis
        let f0 = f[0];
        let diffs: Vec<f64> = (0..d)
            .map(|i| {
                let b = 1 + 4 * i;
                (f[b] + f[b + 1] - 2.0 * f0 - ratio * (f[b + 2] + f[b + 3] - 2.0 * f0)).abs()
            })
            .collect();
        let top = diffs.iter().fold(0.0f64, |m, &v| m.max(v));
        let mut split = 0;
        let mut best: Option<(f64, f64)> = None;
        for i in 0..d {
            let key = (if diffs[i] >= top * (1.0 - 1e-10) { 1.0 } else { 0.0 }, h[i]);
            if h[i] * 2.0 < MIN_WIDTH {
                continue;
            }
            if best.is_none_or(|b| key.0 > b.0 || (key.0 == b.0 && key.1 > b.1)) {
                best = Some(key);
                split = i;
            }
        }
        // |I7 - I5| unscaled (decision quad-1.5): scaling it down where the degree-3, -5 and -7 values converge
        // (as null-rule estimates do) halved the work on smooth integrands but let kinks and jumps converge
        // falsely (continuous and discontinuous Genz families, achieved errors 10^3 above the estimates)
        let err = (vol * (i7 - i5).abs()).max(50.0 * f64::EPSILON * vol * abs);
        out.push(Region { value: vol * i7, err, abs: vol * abs, split, c, h });
    }
    Ok(Ok(out))
}

/// The initial regions: the grid of the ranges' pieces in the unit cube.
fn grid(s: &Sampler<'_>) -> Vec<(Vec<f64>, Vec<f64>)> {
    let d = s.dim();
    let counts: Vec<usize> = (0..d).map(|k| s.dom.pieces(k)).collect();
    let total: usize = counts.iter().product();
    (0..total)
        .map(|mut m| {
            let mut c = vec![0.0; d];
            let mut h = vec![0.0; d];
            for k in 0..d {
                let j = m % counts[k];
                m /= counts[k];
                let w = 1.0 / counts[k] as f64;
                c[k] = (j as f64 + 0.5) * w;
                h[k] = 0.5 * w;
            }
            (c, h)
        })
        .collect()
}

/// Sums of the regions: value, error, |f| scale.
fn totals<'a>(rs: impl Iterator<Item = &'a Region>) -> (f64, f64, f64) {
    rs.fold((0.0, 0.0, 0.0), |(v, e, a), r| (v + r.value, e + r.err, a + r.abs))
}

/// Adaptive cubature of the sampler's integrand over the unit cube, until the goals are met or the next round
/// would exceed `max_points` evaluations.
pub(crate) fn integrate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    s: &mut Sampler<'h>,
    goals: Goals,
    max_points: u64,
) -> Res<Result<Estimate, Vec<f64>>> {
    let rule = Rule::new(s.dim());
    let np = rule.len();
    let mut heap = BinaryHeap::new();
    let mut serial = 0u64;
    let mut done: Vec<Region> = vec![];
    let start = grid(s);
    for block in start.chunks((ROUND_POINTS / np).max(1)) {
        match evaluate(cx, s, &rule, block.to_vec())? {
            Ok(rs) => {
                for r in rs {
                    heap.push(Entry(r, serial));
                    serial += 1;
                }
            }
            Err(p) => return Ok(Err(p)),
        }
    }
    let (mut value, mut err, mut abs) = totals(heap.iter().map(|e| &e.0));
    let mut rounds = 0u64;
    let converged = loop {
        cx.tick(1)?;
        let tol = goals.tol(value.abs()).max(ROUNDOFF * abs);
        if err <= tol {
            break true;
        }
        // the regions to split this round
        let mut take = vec![];
        let mut taken = 0.0;
        while heap.peek().is_some() {
            let next = (take.len() + 1) * 2 * np;
            if !take.is_empty() && (taken >= err - tol || next > ROUND_POINTS) {
                break;
            }
            if s.calls + next as u64 > max_points {
                break;
            }
            let Entry(r, _) = heap.pop().expect("peeked");
            if r.h[r.split] * 2.0 < MIN_WIDTH {
                done.push(r);
                continue;
            }
            taken += r.err;
            take.push(r);
        }
        if take.is_empty() {
            break false;
        }
        let mut children = Vec::with_capacity(2 * take.len());
        for r in &take {
            let i = r.split;
            let mut h = r.h.clone();
            h[i] *= 0.5;
            for sign in [-1.0, 1.0] {
                let mut c = r.c.clone();
                c[i] += sign * h[i];
                children.push((c, h.clone()));
            }
        }
        let new = match evaluate(cx, s, &rule, children)? {
            Ok(v) => v,
            Err(p) => return Ok(Err(p)),
        };
        for r in &take {
            value -= r.value;
            err -= r.err;
            abs -= r.abs;
        }
        for r in new {
            value += r.value;
            err += r.err;
            abs += r.abs;
            heap.push(Entry(r, serial));
            serial += 1;
        }
        rounds += 1;
        if rounds.is_multiple_of(64) {
            (value, err, abs) = totals(heap.iter().map(|e| &e.0).chain(&done));
        }
    };
    let (value, err, abs) = totals(heap.iter().map(|e| &e.0).chain(&done));
    let converged = converged || err <= goals.tol(value.abs()).max(ROUNDOFF * abs);
    // where the largest error is left, for the message
    let worst =
        heap.peek().map(|e| e.0.clone()).or_else(|| done.iter().max_by(|a, b| a.err.total_cmp(&b.err)).cloned());
    let mut note = None;
    if !converged {
        if let Some(r) = worst {
            if let Some(x) = s.point(cx, &r.c)? {
                note = Some((x, r.err));
            }
        }
    }
    Ok(Ok(Estimate { value, err, converged, calls: s.calls, worst: note, chi2: None }))
}

#[cfg(test)]
mod tests {
    use super::Rule;

    /// The rule integrates monomials of degree <= 7 (the embedded one those of degree <= 5) over [-1, 1]^d exactly.
    #[test]
    fn exactness() {
        for d in 1..=7 {
            let r = Rule::new(d);
            // monomials x1^a x2^b x3^c with a + b + c <= 7 (all variables beyond the third have exponent 0)
            for a in 0..=7u32 {
                for b in 0..=7 - a {
                    for c in 0..=7 - a - b {
                        let e = [a, b, c];
                        if e.iter().enumerate().any(|(i, &k)| i >= d && k > 0) {
                            continue;
                        }
                        let exact: f64 =
                            e.iter().map(|&k| if k % 2 == 1 { 0.0 } else { 1.0 / (k as f64 + 1.0) }).product();
                        let (mut s7, mut s5) = (0.0, 0.0);
                        for p in 0..r.len() {
                            let v: f64 = (0..d.min(3)).map(|i| r.offsets[p * d + i].powi(e[i] as i32)).product();
                            s7 += r.w7[p] * v;
                            s5 += r.w5[p] * v;
                        }
                        assert!((s7 - exact).abs() < 1e-13, "d {d} degree-7 {e:?}: {s7} vs {exact}");
                        if a + b + c <= 5 {
                            assert!((s5 - exact).abs() < 1e-13, "d {d} degree-5 {e:?}: {s5} vs {exact}");
                        }
                    }
                }
            }
        }
    }
}
