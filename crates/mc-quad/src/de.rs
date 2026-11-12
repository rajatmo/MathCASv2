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
    let mut trunc = 0.0f64;
    let mut tail_piece: Option<usize> = None;
    for (pi, p) in pieces.iter().enumerate() {
        let mut terms: Vec<(i64, f64)> = Vec::new();
        let mut range = [0i64; 2];
        let mut side_terms: Vec<f64> = Vec::new();
        for (k, side) in [1i64, -1].into_iter().enumerate() {
            side_terms.clear();
            for j in 0.. {
                cx.tick(1)?;
                if j == 0 && side < 0 {
                    continue;
                }
                let t = (side * j) as f64;
                // a node at the end: the finer levels still have nodes before it
                let Some((x, w)) = node(p, t) else {
                    range[k] = j;
                    // the range of x ran out (1e308) with the terms still not negligible: the integral beyond is not in
                    // the sum. x^-1.01 on [16, inf) loses 0.09 of 97.27 that way. The last node that exists (found by
                    // bisection between the last valid j and this one) gives the term there, and the decay of the terms
                    // over the last quarter step the rate: the loss is that term over that rate, counted into the
                    // error estimate (a failure with the estimate rather than a value that is 0.1% off)
                    if !matches!(p.map, Map::Id) && side_terms.len() >= 2 {
                        let largest = terms.iter().fold(0.0f64, |m, v| m.max(v.1));
                        if side_terms[side_terms.len() - 1] > TINY * largest {
                            if matches!(p.map, Map::Upper(_) | Map::Lower(_)) {
                                tail_piece.get_or_insert(pi);
                            }
                            let (mut a, mut b) = (t - side as f64, t);
                            for _ in 0..50 {
                                let m = 0.5 * (a + b);
                                if node(p, m).is_some() {
                                    a = m;
                                } else {
                                    b = m;
                                }
                            }
                            let mut mags = [0.0f64; 2];
                            for (slot, tt) in mags.iter_mut().zip([a, a - 0.25 * side as f64]) {
                                if let Some((xx, ww)) = node(p, tt) {
                                    let (mut yy, mut ee) = ([S::real(0.0)], [0.0]);
                                    if f.eval(cx, &[xx], &mut yy, &mut ee)?.is_ok() {
                                        *slot = (yy[0] * S::real(ww)).abs();
                                    }
                                }
                            }
                            // a term that underflowed to zero (x^-1.1 at 1e308) leaves nothing beyond it
                            let kappa = if mags[0] > 0.0 && mags[1] > mags[0] { (mags[1] / mags[0]).ln() / 0.25 } else { 0.0 };
                            if mags[0] > 0.0 {
                                trunc += if kappa > 1e-3 { mags[0] / kappa } else { f64::INFINITY };
                            }
                        }
                    }
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
                side_terms.push(term.abs());
                range[k] = j;
                let largest = terms.iter().fold(0.0f64, |m, v| m.max(v.1));
                if j >= 3 && term.abs() <= TINY * largest {
                    break;
                }
            }
        }
        ranges.push(range);
    }
    // an algebraic tail beyond the machine range: that piece is integrated in ln|x - a| with the tail beyond e^230
    // added from its power law (R24-02)
    if let Some(pi) = tail_piece {
        if let Some(out) = algebraic_tail(cx, f, pieces, pi, goals, max_level)? {
            return Ok(out);
        }
    }
    let mut history = vec![sum];
    let mut h = 1.0;
    let mut level = 0;
    loop {
        let est = estimate(&history, h, abs, inner).max(trunc);
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

/// The integrand in `y = ln(u)` where `x = a + sgn u`: the integral of f over `u >= 1` becomes an integral over `y >= 0`
/// of `f e^y`, in which an algebraic tail `x^-p` is the smooth `e^(-(p-1) y)`.
struct LogMap<'a, 'h, S> {
    f: &'a mut dyn Integrand<'h, S>,
    a: f64,
    sgn: f64,
}

impl<'h, S: Scalar> Integrand<'h, S> for LogMap<'_, 'h, S> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, ys: &[f64], out: &mut [S], err: &mut [f64]) -> Res<Result<(), Vec<f64>>> {
        let xs: Vec<f64> = ys.iter().map(|y| self.a + self.sgn * y.exp()).collect();
        let r = self.f.eval(cx, &xs, out, err)?;
        if r.is_ok() {
            for ((o, e), y) in out.iter_mut().zip(err.iter_mut()).zip(ys) {
                let w = y.exp();
                *o = *o * S::real(w);
                *e *= w;
            }
        }
        Ok(r)
    }
}

/// The piece `pi` (a half line whose exp-sinh range ran out at 1e308 with the terms still large) by another route,
/// writing u = |x - a|: the integral over u in [0, 1] by tanh-sinh, over [1, e^230] in y = ln u by tanh-sinh (there
/// an integrand `u^-p` is the smooth `e^(-(p-1) y)`), and the tail beyond u = e^230 from the power law `c u^-p`
/// (relative error of the law about 1e-100 for an algebraic integrand). The exponent is fitted from the integrand
/// at e^230, e^230/16 and e^230/256; the error counts the disagreement of the two fits. `None` when the integrand
/// is not an algebraic tail (an exponent not above 1.00001, the two fits more than 1e-3 apart, or the phases
/// disagree), the caller then keeps the truncated sum with its tail estimate. The other pieces are integrated
/// as before and added.
fn algebraic_tail<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn Integrand<'h, S>,
    pieces: &[Piece],
    pi: usize,
    goals: Goals,
    max_level: u32,
) -> Res<Option<Outcome<S>>> {
    const Y: f64 = 230.0;
    let p = pieces[pi];
    let (a, sgn) = match p.map {
        Map::Upper(a) => (a, 1.0),
        Map::Lower(a) => (a, -1.0),
        _ => return Ok(None),
    };
    let dir = if p.t1 >= p.t0 { 1.0 } else { -1.0 };
    let u0 = Y.exp();
    let mut vals = [S::real(0.0); 3];
    for (k, slot) in vals.iter_mut().enumerate() {
        let u = u0 / 16f64.powi(k as i32);
        let (mut v, mut e) = ([S::real(0.0)], [0.0]);
        if f.eval(cx, &[a + sgn * u], &mut v, &mut e)?.is_err() || !v[0].is_finite() || v[0].abs() == 0.0 {
            return Ok(None);
        }
        *slot = v[0];
    }
    let ln16 = 16f64.ln();
    let pa = (vals[1].abs() / vals[0].abs()).ln() / ln16;
    let pb = (vals[2].abs() / vals[1].abs()).ln() / ln16;
    if !(pa > 1.00001 && pb > 1.00001) || (pa - pb).abs() > 1e-3 || pa > 60.0 {
        return Ok(None);
    }
    // the phases: f(u0/16) = f(u0) 16^p
    let predicted = vals[0] * S::real(16f64.powf(pa));
    if (vals[1] - predicted).abs() > 1e-8 * vals[1].abs() {
        return Ok(None);
    }
    let tail = vals[0] * S::real(u0 / (pa - 1.0));
    let tail_err = tail.abs() * ((pa - pb).abs() * (Y + 1.0 / (pa - 1.0)) + 1e-13);
    // the rest of the pieces
    let mut value = S::real(0.0);
    let mut err = tail_err;
    let mut converged = true;
    let rest: Vec<Piece> = pieces.iter().enumerate().filter(|(i, _)| *i != pi).map(|(_, q)| *q).collect();
    let mut parts: Vec<Outcome<S>> = Vec::new();
    if !rest.is_empty() {
        parts.push(integrate(cx, f, &rest, goals, max_level)?);
    }
    // u in [0, 1]: x from a to a + sgn, the integral is sgn times the magnitude
    let near = [Piece { map: Map::Id, t0: a, t1: a + sgn }];
    let o = integrate(cx, f, &near, goals, max_level)?;
    parts.push(match o {
        Outcome::Done { value, err, converged } => Outcome::Done { value: value * S::real(sgn), err, converged },
        other => other,
    });
    let far = [Piece { map: Map::Id, t0: 0.0, t1: Y }];
    parts.push(integrate(cx, &mut LogMap { f, a, sgn }, &far, goals, max_level)?);
    // the half line's three parts are magnitudes of u: the oriented value is dir * sgn times their sum
    let mut half = S::real(0.0);
    for (i, o) in parts.into_iter().enumerate() {
        match o {
            Outcome::Done { value: v, err: e, converged: c } => {
                if i == 0 && !rest.is_empty() {
                    value = value + v;
                } else {
                    half = half + v;
                }
                err += e;
                converged &= c;
            }
            Outcome::NotNumeric(pt) => return Ok(Some(Outcome::NotNumeric(pt))),
        }
    }
    half = half + tail;
    value = value + half * S::real(dir * sgn);
    let conv = converged && err <= goals.tol(value.abs());
    Ok(Some(Outcome::Done { value, err, converged: conv }))
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
