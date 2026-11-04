//! The double-exponential rules at machine precision (Takahasi and Mori): the trapezoidal rule with step h after
//! a substitution that makes the integrand decay double exponentially at both ends, so endpoint singularities
//! (`1/sqrt(x)`, `log(x)` on [0, 1]) cost nothing extra. Halving h roughly doubles the correct digits; the nodes
//! of each level are those of the previous one plus the midpoints.
//!
//! | piece | substitution |
//! |---|---|
//! | [a, b] (tanh-sinh) | x = (a + b)/2 + (b - a)/2 tanh(pi/2 sinh t), the distances to the ends computed directly |
//! | [a, inf) (exp-sinh) | x = a + exp(pi/2 sinh t) |
//! | (-inf, inf) (sinh-sinh) | x = sinh(pi/2 sinh t) |
//!
//! The range of t is found on the first level (h = 1): each side ends where a node coincides with the end in
//! machine arithmetic, where the term falls below 10^-20 of the largest one, or where the integrand stops being
//! numeric in the tail (`x^3 exp(-x)` overflows to a NaN far out). The error estimate is Bailey's: with d1, d2
//! the digits of agreement of the last level with the two previous ones, the error is about 10^(d1^2 / d2).
use crate::machine::{Integrand, Map, Outcome, Piece};
use crate::options::Goals;
use mc_eval::{NativeCx, Res};
use mc_numfn::Scalar;
use std::f64::consts::FRAC_PI_2;

/// The finest level (h = 2^-MAX_LEVEL).
pub(crate) const MAX_LEVEL: u32 = 10;

/// The first level whose estimate may be accepted (Bailey's estimate needs three levels in its regime).
const MIN_LEVEL: u32 = 3;

/// Terms below this fraction of the largest one end the range of t.
const TINY: f64 = 1e-20;

/// The node at t of a piece and its weight dx/dt (signed by the piece's direction); `None` where the node
/// coincides with an end or leaves the machine range.
pub(crate) fn node(p: &Piece, t: f64) -> Option<(f64, f64)> {
    let dir = if p.t1 >= p.t0 { 1.0 } else { -1.0 };
    let ch = t.cosh();
    let v = FRAC_PI_2 * t.sinh();
    match p.map {
        Map::Id => {
            let (a, b) = (p.t0, p.t1);
            let r = 0.5 * (b - a);
            if t == 0.0 {
                return Some((a + r, r * FRAC_PI_2));
            }
            let q = (-2.0 * v.abs()).exp();
            let d = 2.0 * q / (1.0 + q);
            let w = r * FRAC_PI_2 * ch * 4.0 * q / ((1.0 + q) * (1.0 + q));
            let x = if t < 0.0 { a + r * d } else { b - r * d };
            let end = if t < 0.0 { a } else { b };
            (q > 0.0 && x != end && w != 0.0).then_some((x, w))
        }
        Map::Upper(a) | Map::Lower(a) => {
            let e = v.exp();
            let x = if matches!(p.map, Map::Upper(_)) { a + e } else { a - e };
            let w = FRAC_PI_2 * ch * e * if matches!(p.map, Map::Upper(_)) { dir } else { -dir };
            (e > 0.0 && x.is_finite() && x != a && w.is_finite() && w != 0.0).then_some((x, w))
        }
        Map::Whole => {
            let x = v.sinh();
            let w = dir * FRAC_PI_2 * ch * v.cosh();
            (x.is_finite() && w.is_finite()).then_some((x, w))
        }
    }
}

/// The double-exponential integral of `f` over the pieces, refined level by level until Bailey's error estimate
/// meets the goals or `max_level` is reached.
pub(crate) fn integrate<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn Integrand<'h, S>,
    pieces: &[Piece],
    goals: Goals,
    max_level: u32,
) -> Res<Outcome<S>> {
    // level 0, point by point: the sums of w f, of |w f| and of the inner errors, and the range of t per side
    let mut sum = S::real(0.0);
    let (mut abs, mut inner) = (0.0, 0.0);
    let mut ranges = Vec::with_capacity(pieces.len());
    for p in pieces {
        let mut terms: Vec<(i64, f64)> = Vec::new();
        let mut range = [0i64; 2];
        for (k, side) in [1i64, -1].into_iter().enumerate() {
            for j in 0.. {
                cx.tick(1)?;
                if j == 0 && side < 0 {
                    continue;
                }
                let t = (side * j) as f64;
                // a node at the end: the finer levels still have nodes before it
                let Some((x, w)) = node(p, t) else {
                    range[k] = j;
                    break;
                };
                let (mut y, mut e) = ([S::real(0.0)], [0.0]);
                match f.eval(cx, &[x], &mut y, &mut e)? {
                    Ok(()) => {}
                    // a tail that stops being numeric ends the range (decision N2.7); a failure in the bulk is one
                    Err(pt) if j < 2 => return Ok(Outcome::NotNumeric(pt)),
                    Err(_) => break,
                }
                let term = y[0] * S::real(w);
                sum = sum + term;
                abs += term.abs();
                inner += e[0] * w.abs();
                terms.push((side * j, term.abs()));
                range[k] = j;
                let largest = terms.iter().fold(0.0f64, |m, v| m.max(v.1));
                if j >= 3 && term.abs() <= TINY * largest {
                    break;
                }
            }
        }
        ranges.push(range);
    }
    let mut history = vec![sum];
    let mut h = 1.0;
    let mut level = 0;
    loop {
        let est = estimate(&history, h, abs, inner);
        let value = history[history.len() - 1];
        let converged = level >= MIN_LEVEL && est <= goals.tol(value.abs());
        if converged || level >= max_level {
            return Ok(Outcome::Done { value, err: est, converged });
        }
        // the next level: the midpoints of the current one
        level += 1;
        h *= 0.5;
        let mut xs = Vec::new();
        let mut ws = Vec::new();
        for (p, range) in pieces.iter().zip(&ranges) {
            let (hi, lo) = (range[0] as f64, -(range[1] as f64));
            let mut t = lo + h;
            while t < hi {
                cx.tick(1)?;
                if let Some((x, w)) = node(p, t) {
                    xs.push(x);
                    ws.push(w);
                }
                t += 2.0 * h;
            }
        }
        let mut ys = vec![S::real(0.0); xs.len()];
        let mut es = vec![0.0; xs.len()];
        if let Err(pt) = f.eval(cx, &xs, &mut ys, &mut es)? {
            return Ok(Outcome::NotNumeric(pt));
        }
        for ((&y, &w), &e) in ys.iter().zip(&ws).zip(&es) {
            let term = y * S::real(w);
            sum = sum + term;
            abs += term.abs();
            inner += e * w.abs();
        }
        history.push(sum * S::real(h));
    }
}

/// Bailey's error estimate of the last of the level sums `history` (`h` the last step, `abs` the sum of |w f| and
/// `inner` the weighted inner errors, unscaled by h): about 10^(d1^2/d2) relative, with d1 and d2 the (negative)
/// decimal logarithms of the relative differences to the two previous levels; never below the roundoff of the
/// sum.
fn estimate<S: Scalar>(history: &[S], h: f64, abs: f64, inner: f64) -> f64 {
    let n = history.len();
    let s = history[n - 1];
    let floor = 10.0 * f64::EPSILON * abs * h + inner * h;
    if n < 3 {
        return f64::INFINITY;
    }
    let mag = s.abs().max(f64::MIN_POSITIVE);
    let e1 = (s - history[n - 2]).abs();
    let e2 = (s - history[n - 3]).abs();
    if e1 == 0.0 {
        return floor;
    }
    let d1 = (e1 / mag).log10();
    let d2 = (e2 / mag).log10();
    let rel = if d1 < 0.0 && d2 < 0.0 { (d1 * d1 / d2).max(2.0 * d1) } else { d1 };
    (10f64.powf(rel) * mag).max(floor)
}
