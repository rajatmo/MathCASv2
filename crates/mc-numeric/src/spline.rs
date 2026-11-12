//! Cubic splines through tabulated data: the one evaluation `interpolation` (mc-io) and compiled code (mc-jit) share,
//! so a compiled call gives the interpreter's value bit for bit.
//!
//! A spline is the nodes `xs` (increasing), the values `ys` and the second derivatives `ms` at the nodes (all zero
//! for straight segments). Compiled code refers to a spline by its number in a process-wide registry: splines are
//! registered once per distinct content (the registry deduplicates by content, so repeated compilations of one
//! spline do not grow it) and never removed, so a number stays valid for the life of the process.
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// A cubic spline through (xs, ys) with second derivatives ms at the nodes.
#[derive(Debug, PartialEq)]
pub struct Spline {
    /// the nodes, increasing
    pub xs: Vec<f64>,
    /// the values at the nodes
    pub ys: Vec<f64>,
    /// the second derivatives at the nodes
    pub ms: Vec<f64>,
}

/// The spline's value at `x`: `None` when `x` is not finite or lies outside [xs[0], xs[n-1]], or the data is not a
/// spline (fewer than two nodes, lengths that differ).
pub fn value(xs: &[f64], ys: &[f64], ms: &[f64], x: f64) -> Option<f64> {
    let n = xs.len();
    if n < 2 || ys.len() != n || ms.len() != n || !x.is_finite() || !(xs[0]..=xs[n - 1]).contains(&x) {
        return None;
    }
    // the interval [x_i, x_{i+1}] holding x
    let i = xs.partition_point(|&a| a <= x).clamp(1, n - 1) - 1;
    let h = xs[i + 1] - xs[i];
    let (a, b) = ((xs[i + 1] - x) / h, (x - xs[i]) / h);
    Some(a * ys[i] + b * ys[i + 1] + ((a * a * a - a) * ms[i] + (b * b * b - b) * ms[i + 1]) * h * h / 6.0)
}

/// The `d`-th derivative of the spline at `x` (`d = 0`: [`value`]); `None` as for [`value`].
pub fn derivative(xs: &[f64], ys: &[f64], ms: &[f64], x: f64, d: u32) -> Option<f64> {
    if d == 0 {
        return value(xs, ys, ms, x);
    }
    let n = xs.len();
    if n < 2 || ys.len() != n || ms.len() != n || !x.is_finite() || !(xs[0]..=xs[n - 1]).contains(&x) {
        return None;
    }
    let i = xs.partition_point(|&a| a <= x).clamp(1, n - 1) - 1;
    let h = xs[i + 1] - xs[i];
    let (a, b) = ((xs[i + 1] - x) / h, (x - xs[i]) / h);
    Some(match d {
        1 => (ys[i + 1] - ys[i]) / h + ((1.0 - 3.0 * a * a) * ms[i] + (3.0 * b * b - 1.0) * ms[i + 1]) * h / 6.0,
        2 => a * ms[i] + b * ms[i + 1],
        3 => (ms[i + 1] - ms[i]) / h,
        _ => 0.0,
    })
}

/// The `d`-th derivative at `x` of the local interpolating polynomial of degree `k`: the polynomial through the
/// k + 1 nodes nearest the interval holding x (x_i <= x < x_{i+1}: the nodes from i - (k - 1)/2, moved inside the
/// table at its ends), so that for k = 3 it is the cubic through the two nodes on each side. Tables of at most k
/// points use all of them. `None` when `x` is not finite or lies outside the table.
pub fn local(xs: &[f64], ys: &[f64], k: usize, x: f64, d: u32) -> Option<f64> {
    let n = xs.len();
    if n < 2 || ys.len() != n || k == 0 || !x.is_finite() || !(xs[0]..=xs[n - 1]).contains(&x) {
        return None;
    }
    let m = k.min(n - 1) + 1;
    let i = xs.partition_point(|&a| a <= x).clamp(1, n - 1) - 1;
    let start = i.saturating_sub((k.min(n - 1) - 1) / 2).min(n - m);
    let (px, py) = (&xs[start..start + m], &ys[start..start + m]);
    // Newton divided differences
    let mut c: Vec<f64> = py.to_vec();
    for j in 1..m {
        for r in (j..m).rev() {
            c[r] = (c[r] - c[r - 1]) / (px[r] - px[r - j]);
        }
    }
    // nested multiplication carrying the derivatives: dv[q] = p^(q)(x)/q!
    let d = d as usize;
    let mut dv = vec![0.0; d + 1];
    for j in (0..m).rev() {
        for q in (1..=d).rev() {
            dv[q] = dv[q] * (x - px[j]) + dv[q - 1];
        }
        dv[0] = dv[0] * (x - px[j]) + c[j];
    }
    let fact: f64 = (1..=d).map(|v| v as f64).product();
    Some(dv[d] * fact)
}

impl Spline {
    /// The value at `x` ([`value`]).
    pub fn at(&self, x: f64) -> Option<f64> {
        value(&self.xs, &self.ys, &self.ms, x)
    }
}

struct Registry {
    all: Vec<Arc<Spline>>,
    by_content: HashMap<u64, Vec<u32>>,
}

fn registry() -> &'static RwLock<Registry> {
    static R: OnceLock<RwLock<Registry>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(Registry { all: vec![], by_content: HashMap::new() }))
}

fn content_hash(xs: &[f64], ys: &[f64], ms: &[f64]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut s = std::collections::hash_map::DefaultHasher::new();
    for v in [xs, ys, ms] {
        v.len().hash(&mut s);
        for x in v {
            x.to_bits().hash(&mut s);
        }
    }
    s.finish()
}

/// The registry number of the spline with this data (registered on first use).
pub fn register(xs: &[f64], ys: &[f64], ms: &[f64]) -> u32 {
    let key = content_hash(xs, ys, ms);
    let same = |s: &Spline| s.xs == xs && s.ys == ys && s.ms == ms;
    {
        let r = registry().read().expect("the spline registry is not poisoned");
        if let Some(ids) = r.by_content.get(&key) {
            if let Some(&id) = ids.iter().find(|&&id| same(&r.all[id as usize])) {
                return id;
            }
        }
    }
    let mut r = registry().write().expect("the spline registry is not poisoned");
    if let Some(ids) = r.by_content.get(&key) {
        if let Some(&id) = ids.iter().find(|&&id| same(&r.all[id as usize])) {
            return id;
        }
    }
    let id = r.all.len() as u32;
    r.all.push(Arc::new(Spline { xs: xs.to_vec(), ys: ys.to_vec(), ms: ms.to_vec() }));
    r.by_content.entry(key).or_default().push(id);
    id
}

/// The spline registered as `id`.
pub fn get(id: u32) -> Option<Arc<Spline>> {
    registry().read().expect("the spline registry is not poisoned").all.get(id as usize).cloned()
}

#[cfg(test)]
mod derivative_tests {
    use super::*;

    /// The local polynomial of degree k reproduces polynomials of degree k, with their derivatives, also at the ends.
    #[test]
    fn local_polynomials_are_exact() {
        let xs: Vec<f64> = (0..12).map(|i| i as f64 * 0.7 + (i * i) as f64 * 0.01).collect();
        let p = |x: f64| 2.0 * x * x * x - x * x + 3.0;
        let ys: Vec<f64> = xs.iter().map(|&x| p(x)).collect();
        for x in [0.0, 0.1, 1.234, 4.0, xs[11]] {
            assert!((local(&xs, &ys, 3, x, 0).unwrap() - p(x)).abs() < 1e-9);
            assert!((local(&xs, &ys, 3, x, 1).unwrap() - (6.0 * x * x - 2.0 * x)).abs() < 1e-8);
            assert!((local(&xs, &ys, 3, x, 2).unwrap() - (12.0 * x - 2.0)).abs() < 1e-7);
            assert!((local(&xs, &ys, 3, x, 3).unwrap() - 12.0).abs() < 1e-6);
            assert_eq!(local(&xs, &ys, 3, x, 4), Some(0.0));
        }
        assert_eq!(local(&xs, &ys, 3, -1.0, 0), None);
        // fewer points than the degree: all of them
        assert!((local(&[0.0, 1.0, 2.0], &[0.0, 1.0, 4.0], 5, 1.5, 0).unwrap() - 2.25).abs() < 1e-12);
    }

    /// The spline's derivatives agree with differences of its values.
    #[test]
    fn spline_derivatives_match_differences() {
        let xs = [0.0, 0.5, 1.2, 2.0, 3.1];
        let ys = [1.0, 0.3, -0.4, 0.8, 2.0];
        let ms = [0.5, -1.0, 2.0, 0.3, -0.7];
        let e = 1e-6;
        for x in [0.2, 0.9, 1.7, 2.5] {
            let v = |x| value(&xs, &ys, &ms, x).unwrap();
            let d1 = derivative(&xs, &ys, &ms, x, 1).unwrap();
            let d2 = derivative(&xs, &ys, &ms, x, 2).unwrap();
            assert!((d1 - (v(x + e) - v(x - e)) / (2.0 * e)).abs() < 1e-7, "x = {x}");
            assert!((d2 - (v(x + e) - 2.0 * v(x) + v(x - e)) / (e * e)).abs() < 1e-3, "x = {x}");
        }
    }
}
