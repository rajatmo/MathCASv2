//! The double-exponential rules on balls (decision N2.9): the method under `digits: d` (numerics roadmap
//! NUM.4). The same substitutions as [`crate::de`] (tanh-sinh on [a, b] with the distances to the ends computed
//! directly, exp-sinh on half-lines, sinh-sinh on the real line), the nodes and weights computed with ball
//! arithmetic at the working precision, the integrand evaluated by the interpreted backend on balls (each level as
//! one batch, a speculative group). Halving the step doubles the correct digits, so d digits take about log2(d)
//! levels. The error estimate is Bailey's (see `de`): a heuristic, so the result's radius is an estimate, not a
//! bound (`rigorous: true` gives a bound).
use mc_arith::num::{Ball, Float, Mag};
use mc_arith::{Kind, Num, Real};
use mc_eval::{NativeCx, Res};
use mc_numeric::consts::Const;
use mc_numeric::elementary as el;
use mc_numfn::NumFn;

/// An end of the range.
#[derive(Clone, Debug)]
pub(crate) enum BEnd {
    /// a real number (a ball of the working precision)
    At(Ball),
    /// +inf (`true`) or -inf
    Inf(bool),
}

/// The result of the ball method.
pub(crate) enum BallOutcome {
    /// The last level's sum, the log2 of its error estimate, and whether it meets the goals.
    Done {
        /// the integral (a ball, or a complex number of balls)
        value: Num,
        /// log2 of the absolute error estimate
        err_log2: f64,
        /// the goals were met
        converged: bool,
    },
    /// The value at this point is not numeric.
    NotNumeric(Num),
}

/// The finest level for `d` digits: digits double per level, so log2(d) plus a margin.
pub(crate) fn max_level(d: u64) -> u32 {
    (64 - d.leading_zeros()) + 5
}

/// A piece of the range, its substitution decided by its ends.
#[derive(Clone, Debug)]
enum Kind2 {
    /// [a, b] (either direction)
    Finite(Ball, Ball),
    /// from a to +inf (`true`) or -inf
    Half(Ball, bool),
    /// the real line
    Whole,
}

/// A piece with its direction (+1 or -1).
#[derive(Clone, Debug)]
struct BPiece {
    kind: Kind2,
    dir: i64,
}

fn pieces(ends: &[BEnd]) -> Vec<BPiece> {
    let mut out = Vec::new();
    for w in ends.windows(2) {
        let pc = |kind, dir| BPiece { kind, dir };
        out.push(match (&w[0], &w[1]) {
            (BEnd::At(a), BEnd::At(b)) => pc(Kind2::Finite(a.clone(), b.clone()), 1),
            (BEnd::At(a), BEnd::Inf(s)) => pc(Kind2::Half(a.clone(), *s), 1),
            (BEnd::Inf(s), BEnd::At(b)) => pc(Kind2::Half(b.clone(), *s), -1),
            (BEnd::Inf(false), BEnd::Inf(true)) => pc(Kind2::Whole, 1),
            (BEnd::Inf(true), BEnd::Inf(false)) => pc(Kind2::Whole, -1),
            (BEnd::Inf(_), BEnd::Inf(_)) => continue,
        });
    }
    out
}

/// log2 |x| of a real (-inf for zero).
fn log2_real(r: &Real) -> f64 {
    match r {
        Real::Ball(b, _) => {
            if b.mid.is_zero() {
                if b.rad.is_zero() {
                    f64::NEG_INFINITY
                } else {
                    b.rad.log2()
                }
            } else {
                b.mid.top() as f64 - 0.5
            }
        }
        other => other.to_f64().abs().log2(),
    }
}

/// log2 |v| of a number (the larger part of a complex one), about to a bit.
pub(crate) fn log2_abs(v: &Num) -> f64 {
    match v {
        Num::Real(r) => log2_real(r),
        Num::Complex(a, b) => log2_real(a).max(log2_real(b)),
    }
}

/// A real as a ball at precision `p` with its radius increased by 2^e.
fn widen_real(r: &Real, e: f64, p: u64) -> Real {
    let b = match r.to_kind(Kind::Ball, p) {
        Real::Ball(b, _) => b,
        _ => Ball::from_i64(0),
    };
    let add = if e.is_finite() { Mag::pow2(e.ceil() as i64) } else { Mag::ZERO };
    Real::Ball(Ball { mid: b.mid, rad: b.rad.add(&add) }, p)
}

/// The number with the error 2^err_log2 joined to the radius of each part.
pub(crate) fn widen(v: &Num, err_log2: f64, p: u64) -> Num {
    match v {
        Num::Real(r) => Num::Real(widen_real(r, err_log2, p)),
        Num::Complex(a, b) => Num::Complex(widen_real(a, err_log2, p), widen_real(b, err_log2, p)),
    }
}

/// A real rounded to `p` bits, the rounding error joining the radius.
fn round_real(r: &Real, p: u64) -> Real {
    match r {
        Real::Ball(b, q) if *q > p => Real::Ball(el::round(b.clone(), p), p),
        other => other.clone(),
    }
}

/// A number rounded to the result precision `p`.
pub(crate) fn round(v: &Num, p: u64) -> Num {
    match v {
        Num::Real(r) => Num::Real(round_real(r, p)),
        Num::Complex(a, b) => Num::Complex(round_real(a, p), round_real(b, p)),
    }
}

/// log2 |b| of a ball's midpoint (-inf for zero).
fn top(b: &Ball) -> f64 {
    if b.mid.is_zero() {
        f64::NEG_INFINITY
    } else {
        b.mid.top() as f64
    }
}

/// The node and weight of a piece at t = j / 2^level; `None` where the node coincides with an end at precision
/// `p` (or a ball function cannot be evaluated, for example after a stop).
fn node(pc: &BPiece, j: i64, level: u32, half_pi: &Ball, p: u64) -> Option<(Ball, Ball)> {
    let t = Ball::from_ratio(j, 1i64 << level, p);
    let sh = el::sinh(&t, p)?;
    let ch = el::cosh(&t, p)?;
    let v = half_pi.mul(&sh, p);
    let dir = Ball::from_i64(pc.dir);
    match &pc.kind {
        Kind2::Finite(a, b) => {
            let r = b.sub(a, p).mul(&Ball::from_ratio(1, 2, p), p);
            if j == 0 {
                return Some((a.add(&r, p), r.mul(half_pi, p)));
            }
            // q = exp(-2|v|), distance to the end r 2q/(1+q), weight r pi/2 cosh t 4q/(1+q)^2
            let av = if j < 0 { v.neg() } else { v };
            let q = el::exp(&av.mul(&Ball::from_i64(-2), p), p)?;
            let one_q = Ball::from_i64(1).add(&q, p);
            let d = Ball::from_i64(2).mul(&q, p).div(&one_q, p)?;
            let rd = r.mul(&d, p);
            let w = r.mul(half_pi, p).mul(&ch, p).mul(&Ball::from_i64(4).mul(&q, p), p).div(&one_q.mul(&one_q, p), p)?;
            let end = if j < 0 { a } else { b };
            // the node equals the end at this precision
            if top(&rd) < top(end) - p as f64 || rd.mid.is_zero() {
                return None;
            }
            let x = if j < 0 { a.add(&rd, p) } else { b.sub(&rd, p) };
            Some((x, w))
        }
        Kind2::Half(a, up) => {
            let e = el::exp(&v, p)?;
            if top(&e) < top(a) - p as f64 || e.mid.is_zero() {
                return None;
            }
            let x = if *up { a.add(&e, p) } else { a.sub(&e, p) };
            let s = Ball::from_i64(if *up { 1 } else { -1 });
            Some((x, half_pi.mul(&ch, p).mul(&e, p).mul(&s, p).mul(&dir, p)))
        }
        Kind2::Whole => {
            let x = el::sinh(&v, p)?;
            let w = half_pi.mul(&ch, p).mul(&el::cosh(&v, p)?, p).mul(&dir, p);
            Some((x, w))
        }
    }
}

fn num(b: Ball, p: u64) -> Num {
    Num::Real(Real::Ball(b, p))
}

/// The tanh-sinh family on balls at precision `p` over the range `ends`, refined until the error estimate meets
/// the goals `(abs, rel)` (log2) or `max_level` is reached.
pub(crate) fn integrate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn NumFn<'h, Num>,
    ends: &[BEnd],
    p: u64,
    goals: (f64, f64),
    max_level: u32,
) -> Res<BallOutcome> {
    let pcs = pieces(ends);
    let Some(pi) = Const::Pi.ball(p) else {
        cx.tick(1)?;
        return Ok(BallOutcome::Done { value: Num::int(0), err_log2: f64::INFINITY, converged: false });
    };
    let half_pi = pi.mul(&Ball::from_ratio(1, 2, p), p);
    let zero = Num::int(0);
    // level 0 point by point: the range of j per piece and side
    let mut sum = zero.clone();
    let mut range = Vec::with_capacity(pcs.len());
    for pc in &pcs {
        let mut r = [0i64; 2];
        let mut largest = f64::NEG_INFINITY;
        for (k, side) in [1i64, -1].into_iter().enumerate() {
            for j in 0.. {
                cx.tick(1)?;
                if j == 0 && side < 0 {
                    continue;
                }
                // a node at the end: the finer levels still have nodes before it
                let Some((x, w)) = node(pc, side * j, 0, &half_pi, p) else {
                    r[k] = j;
                    break;
                };
                let mut y = [zero.clone()];
                if !f.eval(cx, &[num(x.clone(), p)], &mut y)? {
                    if j < 2 {
                        return Ok(BallOutcome::NotNumeric(num(x, p)));
                    }
                    // a tail that stops being numeric ends the range (decision N2.7)
                    break;
                }
                let term = y[0].mul(&num(w, p));
                let l = log2_abs(&term);
                largest = largest.max(l);
                sum = sum.add(&term);
                r[k] = j;
                if j >= 3 && l < largest - p as f64 - 10.0 {
                    break;
                }
            }
        }
        range.push(r);
    }
    let mut history = vec![sum.clone()];
    let mut level = 0u32;
    loop {
        let value = history[history.len() - 1].clone();
        let err = estimate(&history, p);
        let mag = log2_abs(&value);
        let converged = level >= 3 && err <= goals.0.max(goals.1 + mag);
        if converged || level >= max_level {
            return Ok(BallOutcome::Done { value, err_log2: err, converged });
        }
        level += 1;
        // the odd multiples of 2^-level inside each piece's range
        let mut xs = Vec::new();
        let mut ws = Vec::new();
        for (pc, r) in pcs.iter().zip(&range) {
            let (hi, lo) = (r[0] << level, -(r[1] << level));
            let mut j = lo + 1;
            while j < hi {
                cx.tick(1)?;
                if let Some((x, w)) = node(pc, j, level, &half_pi, p) {
                    xs.push(num(x, p));
                    ws.push(num(w, p));
                }
                j += 2;
            }
        }
        let mut ys = vec![zero.clone(); xs.len()];
        if !f.eval_flat(cx, &xs, &mut ys)? {
            // the first point that is not numeric
            for x in &xs {
                let mut y = [zero.clone()];
                if !f.eval(cx, std::slice::from_ref(x), &mut y)? {
                    return Ok(BallOutcome::NotNumeric(x.clone()));
                }
            }
            cx.tick(1)?;
            return Ok(BallOutcome::NotNumeric(xs[0].clone()));
        }
        for (y, w) in ys.iter().zip(&ws) {
            sum = sum.add(&y.mul(w));
        }
        let h = num(Ball::from_ratio(1, 1i64 << level, p), p);
        history.push(sum.mul(&h));
    }
}

/// Bailey's estimate (log2 of the absolute error) of the last level sum: with d1, d2 the log2 relative
/// differences to the two previous levels, 2^(max(d1^2/d2, 2 d1)) relative; never below the working precision.
fn estimate(history: &[Num], p: u64) -> f64 {
    let n = history.len();
    let s = &history[n - 1];
    let mag = log2_abs(s);
    let floor = mag - p as f64 + 4.0;
    if n < 3 {
        return f64::INFINITY;
    }
    let e1 = log2_abs(&s.add(&history[n - 2].neg()));
    let e2 = log2_abs(&s.add(&history[n - 3].neg()));
    if e1 == f64::NEG_INFINITY {
        return floor;
    }
    let (d1, d2) = (e1 - mag, e2 - mag);
    let rel = if d1 < 0.0 && d2 < 0.0 { (d1 * d1 / d2).max(2.0 * d1) } else { d1 };
    (rel + mag).max(floor)
}

/// An exact ball of a machine real (for the rigorous bounds' box sizes).
pub(crate) fn ball_f64(x: f64) -> Ball {
    match mc_arith::num::float::f64_to_dyadic(x) {
        Some((neg, man, exp)) => Ball::exact(Float::new(neg, man, exp)),
        None => Ball::from_i64(0),
    }
}
