//! Adaptive Gauss–Kronrod integration with a global error heap (QUADPACK's `qag` strategy, decision N2.3):
//! every interval carries the Kronrod estimate and an error estimate; the interval with the largest error is
//! split in two (both halves evaluated in one batch call) until the sum of the errors meets the goals, the
//! subdivision limit is reached, or no interval can be split further.
//!
//! Error estimate of an interval (QUADPACK): with K the Kronrod and G the Gauss result, `resabs` the integral of
//! |f| and `resasc` the integral of |f - mean| (both by the Kronrod rule),
//!
//! ```text
//! err = resasc * min(1, (200 |K - G| / resasc)^1.5),  at least 50 eps resabs
//! ```
//!
//! An interval whose 15-point estimate is far from resolved (|K - G| > 10^-3 resasc: an oscillation the 7-point
//! rule cannot follow) is evaluated again with the 61-point rule instead of being split, and its halves keep that
//! rule (decision N2.4): oscillatory integrands then need about four times fewer evaluations than with
//! bisection of 15-point intervals alone.
use crate::machine::{Integrand, Map, Outcome, Piece, ROUNDOFF};
use crate::options::Goals;
use crate::rules::{self, Rule};
use mc_eval::{NativeCx, Res};
use mc_numfn::Scalar;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// An interval whose 15-point rule has |K - G| above this fraction of resasc gets the 61-point rule.
const UNRESOLVED: f64 = 1e-3;

/// Limits of one adaptive integration.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// the number of subdivisions (splits and rule upgrades)
    pub max_subdivisions: usize,
    /// the bisection depth
    pub max_recursion: u32,
}

/// Which rule an interval uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    /// 7-15 points
    Low,
    /// 30-61 points
    High,
}

impl Order {
    fn rule(self) -> &'static Rule {
        match self {
            Order::Low => rules::gk15(),
            Order::High => rules::gk61(),
        }
    }
}

/// An interval of a piece with its estimates.
#[derive(Clone, Debug)]
struct Interval<S> {
    piece: usize,
    a: f64,
    b: f64,
    depth: u32,
    order: Order,
    value: S,
    err: f64,
    resabs: f64,
    /// the 15-point rule is far from resolving it
    unresolved: bool,
}

/// A heap entry: the largest error first; ties by creation order, so the order of work is deterministic.
struct Entry<S>(Interval<S>, u64);

impl<S> PartialEq for Entry<S> {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl<S> Eq for Entry<S> {}
impl<S> PartialOrd for Entry<S> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl<S> Ord for Entry<S> {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.err.total_cmp(&o.0.err).then(o.1.cmp(&self.1))
    }
}

/// The intervals `spec` (piece, a, b, depth, order) evaluated with one batch call of the integrand.
fn evaluate<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn Integrand<'h, S>,
    pieces: &[Piece],
    spec: &[(usize, f64, f64, u32, Order)],
) -> Res<Result<Vec<Interval<S>>, Vec<f64>>> {
    // the points of every interval, mapped to x; a node at an infinite end contributes nothing
    let mut xs = Vec::new();
    let mut jac = Vec::new();
    let mut at = Vec::new();
    for &(p, a, b, _, order) in spec {
        let (c, r) = (0.5 * (a + b), 0.5 * (b - a));
        for &u in &order.rule().nodes {
            match pieces[p].map.apply(c + r * u) {
                Some((x, d)) => {
                    at.push(Some(xs.len()));
                    xs.push(x);
                    jac.push(d);
                }
                None => at.push(None),
            }
        }
    }
    let mut ys = vec![S::real(0.0); xs.len()];
    let mut es = vec![0.0; xs.len()];
    if let Err(p) = f.eval(cx, &xs, &mut ys, &mut es)? {
        return Ok(Err(p));
    }
    let mut out = Vec::with_capacity(spec.len());
    let mut k = 0;
    for &(piece, a, b, depth, order) in spec {
        let rule = order.rule();
        let r = 0.5 * (b - a);
        let n = rule.len();
        let g: Vec<(S, f64)> = (0..n)
            .map(|i| match at[k + i] {
                Some(j) => (ys[j] * S::real(jac[j]), es[j] * jac[j].abs()),
                None => (S::real(0.0), 0.0),
            })
            .collect();
        k += n;
        let (mut kr, mut ga, mut abs, mut inner) = (S::real(0.0), S::real(0.0), 0.0, 0.0);
        for (i, &(v, e)) in g.iter().enumerate() {
            kr = kr + S::real(rule.kronrod[i]) * v;
            ga = ga + S::real(rule.gauss[i]) * v;
            abs += rule.kronrod[i] * v.abs();
            inner += rule.kronrod[i] * e;
        }
        let mean = kr * S::real(0.5);
        let asc: f64 = g.iter().zip(&rule.kronrod).map(|(&(v, _), &w)| w * (v - mean).abs()).sum();
        let ra = r.abs();
        let (value, resabs, resasc) = (kr * S::real(r), abs * ra, asc * ra);
        let raw = (kr - ga).abs() * ra;
        let mut err = raw;
        if resasc != 0.0 && err != 0.0 {
            err = resasc * (200.0 * err / resasc).powf(1.5).min(1.0);
        }
        if resabs > f64::MIN_POSITIVE / (50.0 * f64::EPSILON) {
            err = err.max(50.0 * f64::EPSILON * resabs);
        }
        err += inner * ra;
        let unresolved = order == Order::Low && raw > UNRESOLVED * resasc;
        out.push(Interval { piece, a, b, depth, order, value, err, resabs, unresolved });
    }
    Ok(Ok(out))
}

/// The totals of the intervals in the heap and the finished ones: value, error, integral of |f|.
fn totals<S: Scalar>(heap: &BinaryHeap<Entry<S>>, done: &[Interval<S>]) -> (S, f64, f64) {
    let mut v = S::real(0.0);
    let (mut e, mut a) = (0.0, 0.0);
    for i in heap.iter().map(|e| &e.0).chain(done) {
        v = v + i.value;
        e += i.err;
        a += i.resabs;
    }
    (v, e, a)
}

/// Adaptive Gauss–Kronrod integration of `f` over the pieces.
pub(crate) fn integrate<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn Integrand<'h, S>,
    pieces: &[Piece],
    goals: Goals,
    lim: Limits,
) -> Res<Outcome<S>> {
    let spec: Vec<_> = pieces.iter().enumerate().map(|(i, p)| (i, p.t0, p.t1, 0, Order::Low)).collect();
    let first = match evaluate(cx, f, pieces, &spec)? {
        Ok(v) => v,
        Err(p) => return Ok(Outcome::NotNumeric(p)),
    };
    let mut serial = 0u64;
    let mut heap = BinaryHeap::new();
    for i in first {
        heap.push(Entry(i, serial));
        serial += 1;
    }
    let mut done: Vec<Interval<S>> = Vec::new();
    let (mut value, mut err, mut resabs) = totals(&heap, &done);
    let mut steps = 0usize;
    loop {
        cx.tick(1)?;
        let tol = goals.tol(value.abs()).max(ROUNDOFF * resabs);
        if err <= tol {
            return Ok(Outcome::Done { value, err, converged: true });
        }
        if steps >= lim.max_subdivisions {
            break;
        }
        let Some(Entry(worst, _)) = heap.pop() else { break };
        let mid = 0.5 * (worst.a + worst.b);
        let spec: Vec<_> = if worst.unresolved {
            // the same interval with the 61-point rule
            vec![(worst.piece, worst.a, worst.b, worst.depth, Order::High)]
        } else if worst.depth >= lim.max_recursion || mid == worst.a || mid == worst.b || !splittable(pieces[worst.piece].map, mid) {
            // cannot be split further: it keeps its estimate (the totals do not change)
            done.push(worst);
            continue;
        } else {
            let d = worst.depth + 1;
            vec![(worst.piece, worst.a, mid, d, worst.order), (worst.piece, mid, worst.b, d, worst.order)]
        };
        steps += 1;
        let new = match evaluate(cx, f, pieces, &spec)? {
            Ok(v) => v,
            Err(p) => return Ok(Outcome::NotNumeric(p)),
        };
        value = value - worst.value;
        err -= worst.err;
        resabs -= worst.resabs;
        for i in new {
            value = value + i.value;
            err += i.err;
            resabs += i.resabs;
            heap.push(Entry(i, serial));
            serial += 1;
        }
        // the running sums drift by rounding: recompute them now and then
        if steps.is_multiple_of(256) {
            (value, err, resabs) = totals(&heap, &done);
        }
    }
    let (value, err, resabs) = totals(&heap, &done);
    let converged = err <= goals.tol(value.abs()).max(ROUNDOFF * resabs);
    Ok(Outcome::Done { value, err, converged })
}

/// Can an interval of this map be split at `mid` (a point in the interior of the t-domain)?
fn splittable(map: Map, mid: f64) -> bool {
    match map {
        Map::Id => mid.is_finite(),
        Map::Upper(_) | Map::Lower(_) => mid < 1.0,
        Map::Whole => mid.abs() < 1.0,
    }
}
