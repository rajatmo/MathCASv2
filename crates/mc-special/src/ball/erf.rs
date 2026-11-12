//! The error functions and their inverses on balls (decision SF.4).
//!
//! - **erf** by Kummer's positive-term form erf(x) = 2x/sqrt(pi) e^(-x^2) sum_k (2x^2)^k / (1 3 ... (2k+1)) =
//!   2x/sqrt(pi) e^(-x^2) 1F1(1; 3/2; x^2) ([`pfq_sum`] bounds the tail), so there is no cancellation at any x;
//!   for 1.44 x^2 > wp + 20 as 1 - erfc(x) with the asymptotic erfc.
//! - **erfc** = 1 - erf for x <= 0 or moderate x, the erf series computed with about 1.45 x^2 + 10 extra bits (the
//!   size of 1/erfc(x)), estimated up front; for x > 0 with 1.44 x^2 > wp + 20 the asymptotic expansion
//!   erfc(x) = e^(-x^2)/(x sqrt(pi)) (sum_{k<n} (-1)^k (2k-1)!!/(2x^2)^k + R_n), |R_n| <= (2k-1)!!/(2x^2)^n at k = n
//!   (enveloping for real x > 0, DLMF 7.12.1): the first omitted term, evaluated on the ball, is added to the
//!   radius. When its smallest term does not reach 2^-wp the series path is taken.
//! - **erfi** by its positive series erfi(x) = 2x/sqrt(pi) 1F1(1/2; 3/2; x^2).
//! - **inverses**: Newton's method on the midpoints (not rigorous) at doubling precisions from the machine value,
//!   then a proof: for the monotone function f and the target ball [t-, t+], f(x - d) and f(x + d) are evaluated
//!   as balls at the two exact ends and must lie strictly on the two sides of the target ball, so every root lies in
//!   [x - d, x + d]; d is widened 16-fold and the check retried when it fails. inverse_erf(y) for |y| > 1/2 is
//!   sign(y) inverse_erfc(1 - |y|) and inverse_erfc(q) for 1/2 < q < 3/2 is inverse_erf(1 - q) (both differences
//!   exact), so the tails and the neighbourhood of 0 keep their relative accuracy.
use super::hyper::pfq_sum;
use super::{f64_of, float_of, hull, int, is_pos, log2_up, lower, mag_up, pi, real, refine, upper, widen};
use mc_arith::num::{stopped, Ball, Float, Mag, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;
use std::cmp::Ordering;

/// 2 / sqrt(pi).
fn two_over_sqrt_pi(wp: u64) -> Option<Ball> {
    int(2).div(&el::sqrt(&pi(wp + 4)?, wp + 4)?, wp)
}

/// The smallest |x|^2 over the ball, as a double (0 when the ball contains zero).
fn x2_low(x: &Ball) -> f64 {
    if x.contains_zero() {
        return 0.0;
    }
    let lo = f64_of(&lower(x)).abs().min(f64_of(&upper(x)).abs());
    lo * lo
}

/// The largest |x|^2 over the ball, as a double.
fn x2_high(x: &Ball) -> f64 {
    let m = f64_of(&mag_up(x).to_float());
    m * m
}

/// erf(x) by Kummer's form: positive terms, about 2^-wp relative error.
fn erf_series(x: &Ball, wp: u64) -> Option<Ball> {
    let x2 = x.mul(x, wp);
    let s = pfq_sum(&[int(1)], &[Ball::from_ratio(3, 2, wp)], &x2, wp)?;
    let e = el::exp(&x2.neg(), wp)?;
    Some(two_over_sqrt_pi(wp)?.mul(x, wp).mul(&e, wp).mul(&s, wp))
}

/// erfc(x) for a provably positive ball by the asymptotic expansion (`None` when its smallest term exceeds
/// 2^-(wp+4) of the leading term).
fn erfc_asym(x: &Ball, wp: u64) -> Option<Ball> {
    let x2 = x.mul(x, wp);
    let inv = int(1).div(&x2.mul(&int(2), wp), wp)?;
    let kmax = x2_low(x).floor();
    let mut t = int(1);
    let mut s = int(1);
    let mut k = 1u64;
    loop {
        if k.is_multiple_of(32) && stopped() {
            return None;
        }
        if k as f64 > kmax {
            return None;
        }
        t = t.mul(&inv, wp).mul(&int(1 - 2 * k as i64), wp);
        if log2_up(&t) < -(wp as f64) - 4.0 {
            // the first omitted term bounds the remainder (enveloping for real x > 0)
            s = widen(s, &mag_up(&t));
            break;
        }
        s = s.add(&t, wp);
        k += 1;
    }
    let e = el::exp(&x2.neg(), wp)?;
    let d = x.mul(&el::sqrt(&pi(wp)?, wp)?, wp);
    e.mul(&s, wp).div(&d, wp)
}

/// Does the asymptotic expansion apply (x provably positive and its smallest term, about e^-x^2, below 2^-wp)?
fn asym_ok(x: &Ball, wp: u64) -> bool {
    is_pos(x) && std::f64::consts::LOG2_E * x2_low(x) > wp as f64 + 20.0
}

/// erf(x) on a ball.
pub fn erf_ball(x: &Ball, wp: u64) -> Option<Ball> {
    if x.mid.is_zero() && x.rad.is_zero() {
        return Some(int(0));
    }
    if asym_ok(x, wp) {
        if let Some(c) = erfc_asym(x, wp) {
            return Some(int(1).sub(&c, wp));
        }
    }
    let nx = x.neg();
    if asym_ok(&nx, wp) {
        if let Some(c) = erfc_asym(&nx, wp) {
            return Some(c.sub(&int(1), wp));
        }
    }
    erf_series(x, wp)
}

/// erfc(x) on a ball, to about wp bits relative to the value.
pub fn erfc_ball(x: &Ball, wp: u64) -> Option<Ball> {
    if asym_ok(x, wp) {
        if let Some(c) = erfc_asym(x, wp) {
            return Some(c);
        }
    }
    if x.mid.neg && !x.contains_zero() {
        // erfc(x) = 1 + erf(-x) in [1, 2]: no cancellation
        return Some(int(1).add(&erf_ball(&x.neg(), wp)?, wp));
    }
    // 1 - erf(x) loses log2(1/erfc(x)) <= 1.443 x^2 + log2(x sqrt(pi)) + 1 bits
    let extra = (std::f64::consts::LOG2_E * x2_high(x) + 0.5 * x2_high(x).max(1.0).log2() + 12.0).ceil();
    // deep in the tail only the asymptotic expansion is feasible: x = 10^8 would need about 1.4 * 10^16 bits here
    if extra.is_nan() || extra >= (1u64 << 26) as f64 {
        return None;
    }
    let w = wp + extra as u64;
    let e = erf_series(x, w)?;
    Some(el::round(int(1).sub(&e, w), wp))
}

/// erfi(x) on a ball: 2x/sqrt(pi) 1F1(1/2; 3/2; x^2), positive terms.
pub fn erfi_ball(x: &Ball, wp: u64) -> Option<Ball> {
    if x.mid.is_zero() && x.rad.is_zero() {
        return Some(int(0));
    }
    let x2 = x.mul(x, wp);
    let s = pfq_sum(&[Ball::from_ratio(1, 2, wp)], &[Ball::from_ratio(3, 2, wp)], &x2, wp)?;
    Some(two_over_sqrt_pi(wp)?.mul(x, wp).mul(&s, wp))
}

/// The extra working bits for an argument of the error functions (the shift of erfc's exponent is handled inside).
fn guard(x: &Real) -> u64 {
    let b = match x {
        Real::Exact(q) => super::rat_ball(q, 64),
        Real::Ball(b, _) => b.clone(),
        Real::Machine(_) => return 16,
    };
    16 + log2_up(&b).clamp(0.0, 64.0) as u64
}

/// erf(x) to p bits.
pub fn erf(x: &Real, p: u64) -> Option<CBall> {
    refine(p, guard(x), |wp| Some(real(erf_ball(&super::real_ball(x, wp)?, wp)?)))
}

/// erfc(x) to p bits (relative to the value, also deep in the tail).
pub fn erfc(x: &Real, p: u64) -> Option<CBall> {
    refine(p, guard(x), |wp| Some(real(erfc_ball(&super::real_ball(x, wp)?, wp)?)))
}

/// erfi(x) to p bits.
pub fn erfi(x: &Real, p: u64) -> Option<CBall> {
    refine(p, guard(x), |wp| Some(real(erfi_ball(&super::real_ball(x, wp)?, wp)?)))
}

// ---------------------------------------------------------------- inverses

/// An argument of the inverses: the ball and, for exact arguments, the rational (so 1 - y is exact).
enum Arg {
    Exact(Rat),
    Ball(Ball),
}

impl Arg {
    fn of(r: &Real) -> Option<Arg> {
        match r {
            Real::Exact(q) => Some(Arg::Exact(q.clone())),
            Real::Ball(b, _) => Some(Arg::Ball(b.clone())),
            Real::Machine(_) => None,
        }
    }
    fn ball(&self, wp: u64) -> Ball {
        match self {
            Arg::Exact(q) => super::rat_ball(q, wp),
            Arg::Ball(b) => b.clone(),
        }
    }
    /// c - self, exactly (a ball keeps its radius).
    fn rsub(&self, c: i64) -> Arg {
        match self {
            Arg::Exact(q) => Arg::Exact(Rat::from_i64(c).sub(q)),
            Arg::Ball(b) => Arg::Ball(Ball { mid: Float::from_i64(c).sub_exact(&b.mid), rad: b.rad }),
        }
    }
    fn neg(&self) -> Arg {
        match self {
            Arg::Exact(q) => Arg::Exact(q.neg()),
            Arg::Ball(b) => Arg::Ball(b.neg()),
        }
    }
    /// The midpoint as a double.
    fn approx(&self) -> f64 {
        f64_of(&self.ball(64).mid)
    }
    fn is_exact_zero(&self) -> bool {
        match self {
            Arg::Exact(q) => q.is_zero(),
            Arg::Ball(b) => b.mid.is_zero() && b.rad.is_zero(),
        }
    }
}

/// x - corr at precision q (midpoints only).
fn step(x: &Float, corr: &Ball, q: u64) -> Float {
    el::round(Ball::exact(x.sub_exact(&corr.mid)), q).mid
}

/// The root of the monotone f with f(x) in the ball t, from the approximation x: [x - d, x + d] with f strictly
/// on the two sides of t at the ends; `slope` is an upper bound of |dx/dt| near x, for the target's radius.
fn prove(
    x: &Float,
    t: &Ball,
    slope: f64,
    increasing: bool,
    f: impl Fn(&Ball, u64) -> Option<Ball>,
    wp: u64,
) -> Option<Ball> {
    let mut d = Mag::pow2(x.top() - wp as i64 + 6);
    if !t.rad.is_zero() {
        d = d.add(&t.rad.mul(&Mag::from_float_up(&float_of(slope * 4.0))));
    }
    let (t_lo, t_hi) = (lower(t), upper(t));
    for _ in 0..12 {
        if stopped() {
            return None;
        }
        let df = d.to_float();
        let (lo, hi) = (x.sub_exact(&df), x.add_exact(&df));
        let fl = f(&Ball::exact(lo.clone()), wp);
        let fh = f(&Ball::exact(hi.clone()), wp);
        let ok = match (fl, fh) {
            (Some(fl), Some(fh)) if increasing => {
                upper(&fl).cmp_exact(&t_lo) == Ordering::Less && lower(&fh).cmp_exact(&t_hi) == Ordering::Greater
            }
            (Some(fl), Some(fh)) => {
                lower(&fl).cmp_exact(&t_hi) == Ordering::Greater && upper(&fh).cmp_exact(&t_lo) == Ordering::Less
            }
            _ => false,
        };
        if ok {
            return Some(hull(&lo, &hi, wp));
        }
        d = d.mul_2exp(4);
    }
    None
}

/// Newton's iteration x <- x - (f(x) - t) / f'(x) on midpoints, at doubling precisions up to wp + 16.
fn newton(
    start: f64,
    t: &Ball,
    f: impl Fn(&Ball, u64) -> Option<Ball>,
    df: impl Fn(&Ball, u64) -> Option<Ball>,
    wp: u64,
) -> Option<Float> {
    let mut x = float_of(start);
    let top = wp + 16;
    // a few low-precision steps first (the start may be rough when it did not come from the machine inverse)
    for _ in 0..60 {
        if stopped() {
            return None;
        }
        let xb = Ball::exact(x.clone());
        let corr = f(&xb, 64)?.sub(t, 64).div(&df(&xb, 64)?, 64)?;
        x = step(&x, &corr, 64);
        if corr.mid.is_zero() || corr.mid.top() < x.top() - 40 {
            break;
        }
    }
    let mut q = 128u64;
    let mut finals = 0;
    while finals < 2 {
        if stopped() {
            return None;
        }
        let qq = q.min(top);
        let xb = Ball::exact(x.clone());
        let corr = f(&xb, qq)?.sub(t, qq).div(&df(&xb, qq)?, qq)?;
        x = step(&x, &corr, qq);
        if qq == top {
            finals += 1;
        }
        q *= 2;
    }
    Some(x)
}

/// d/dx erf(x) = 2/sqrt(pi) e^(-x^2).
fn erf_deriv(x: &Ball, wp: u64) -> Option<Ball> {
    Some(two_over_sqrt_pi(wp)?.mul(&el::exp(&x.mul(x, wp).neg(), wp)?, wp))
}

/// inverse_erf(y) for 0 < y <= 1/2 (about; any 0 < y < 1 works).
fn inv_erf_core(y: &Ball, wp: u64) -> Option<Ball> {
    if !is_pos(y) {
        return None;
    }
    let start = crate::machine::erf::inverse_erf(f64_of(&y.mid));
    if !start.is_finite() {
        return None;
    }
    let x = newton(start, y, erf_ball, erf_deriv, wp)?;
    // |dx/dy| = sqrt(pi)/2 e^(x^2) <= e^(x^2)
    let slope = (start * start).exp() * 1.01 + 1.0;
    prove(&x, y, slope, true, erf_ball, wp)
}

/// A rough start for inverse_erfc(q) for q > 0 below the double range: x^2 + ln(x sqrt(pi)) = -ln q.
fn erfc_start(q: &Ball) -> Option<f64> {
    let l = -f64_of(&el::log(q, 64)?.mid);
    let mut x = l.max(1.0).sqrt();
    for _ in 0..50 {
        let g = x * x + (x * std::f64::consts::PI.sqrt()).ln() - l;
        let nx = x - g / (2.0 * x + 1.0 / x);
        if (nx - x).abs() < 1e-14 * x {
            return Some(nx);
        }
        x = nx;
    }
    Some(x)
}

/// inverse_erfc(q) for 0 < q <= 1/2 (about; any 0 < q < 1 works).
fn inv_erfc_core(q: &Ball, wp: u64) -> Option<Ball> {
    if !is_pos(q) {
        return None;
    }
    let qf = f64_of(&q.mid);
    let start = if qf >= f64::MIN_POSITIVE { crate::machine::erf::inverse_erfc(qf) } else { erfc_start(q)? };
    if !start.is_finite() {
        return None;
    }
    let neg_deriv = |x: &Ball, w: u64| Some(erf_deriv(x, w)?.neg());
    let x = newton(start, q, erfc_ball, neg_deriv, wp)?;
    // |dx/dq| = sqrt(pi)/2 e^(x^2): about 1.443 x^2 bits, which the target's radius is multiplied by
    let slope = (start * start).min(700.0).exp() * 1.01 + 1.0;
    prove(&x, q, slope, false, erfc_ball, wp)
}

/// inverse_erf on an argument with |y| <= 1/2 (by the midpoint), any sign.
fn inv_erf_signed(y: &Arg, wp: u64) -> Option<Ball> {
    if y.is_exact_zero() {
        return Some(int(0));
    }
    if y.approx() < 0.0 {
        return Some(inv_erf_signed(&y.neg(), wp)?.neg());
    }
    inv_erf_core(&y.ball(wp), wp)
}

/// inverse_erfc on an argument with q <= 1/2 (by the midpoint).
fn inv_erfc_small(q: &Arg, wp: u64) -> Option<Ball> {
    inv_erfc_core(&q.ball(wp), wp)
}

/// The inverse error function: erf(x) = y for -1 < y < 1.
pub fn inverse_erf(y: &Real, p: u64) -> Option<CBall> {
    let y = Arg::of(y)?;
    let v = y.approx();
    refine(p, 24, |wp| {
        let r = if v.abs() <= 0.5 {
            inv_erf_signed(&y, wp)?
        } else if v > 0.0 {
            inv_erfc_small(&y.rsub(1), wp)?
        } else {
            inv_erfc_small(&y.neg().rsub(1), wp)?.neg()
        };
        Some(real(r))
    })
}

/// The inverse complementary error function: erfc(x) = q for 0 < q < 2.
pub fn inverse_erfc(q: &Real, p: u64) -> Option<CBall> {
    let q = Arg::of(q)?;
    let v = q.approx();
    refine(p, 24, |wp| {
        let r = if v <= 0.5 {
            inv_erfc_small(&q, wp)?
        } else if v >= 1.5 {
            inv_erfc_small(&q.rsub(2), wp)?.neg()
        } else {
            inv_erf_signed(&q.rsub(1), wp)?
        };
        Some(real(r))
    })
}
