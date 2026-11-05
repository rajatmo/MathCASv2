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
