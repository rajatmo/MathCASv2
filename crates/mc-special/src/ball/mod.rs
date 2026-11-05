//! Rigorous enclosures at any precision (decision SF.4): every function here returns a ball containing the exact
//! value for every point of its argument balls, or `None` (a pole or a region touching one, an argument outside the
//! real domain, a raised stop flag). Truncation errors are bounded and added to the radius; rounding is tracked by
//! `mc-arith`'s ball arithmetic.
//!
//! The entry points take [`Real`] arguments (exact rationals or balls) and a target precision p, and work at a
//! higher precision, doubling it while the result has fewer than about p correct bits ([`refine`]): cancellation
//! (the K_nu series for large x, sin(pi x) near integers) is absorbed by the retries, and exact arguments are
//! rounded at each working precision rather than once.
pub mod bessel;
pub mod bessel_j;
pub mod elliptic;
pub mod erf;
pub mod expint;
pub mod gamma;
pub mod hyper;
pub mod incomplete;
pub mod polylog;
pub mod zeta;

use mc_arith::num::{stopped, Ball, Float, Int, Mag, Nat, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::consts::Const;
use mc_numeric::elementary as el;

/// A ball around an exact rational at precision p (exact when it fits).
pub fn rat_ball(q: &Rat, p: u64) -> Ball {
    let num = Ball::exact(Float::new(q.num.neg, q.num.mag.clone(), 0));
    if q.den.bits() == 1 {
        return el::round(num, p);
    }
    num.div(&Ball::from_nat(q.den.clone()), p).unwrap_or_else(|| Ball::from_i64(0))
}

/// A real argument as a ball at working precision p (`None` for machine reals: they take the machine path).
pub fn real_ball(r: &Real, p: u64) -> Option<Ball> {
    match r {
        Real::Exact(q) => Some(rat_ball(q, p)),
        Real::Ball(b, _) => Some(b.clone()),
        Real::Machine(_) => None,
    }
}

/// The exact integer value of a real (an exact integer, or an exact ball with an integral midpoint).
pub fn exact_int(r: &Real) -> Option<Int> {
    match r {
        Real::Exact(q) if q.is_int() => Some(q.num.clone()),
        Real::Ball(b, _) if b.rad.is_zero() => float_int(&b.mid),
        _ => None,
    }
}

/// The value of a float if it is an integer.
pub fn float_int(f: &Float) -> Option<Int> {
    if f.is_zero() {
        return Some(Int::from_i64(0));
    }
    if f.exp >= 0 {
        return Some(Int::new(f.neg, f.man.shl(f.exp as u64)));
    }
    let s = (-f.exp) as u64;
    (!f.man.low_bits_nonzero(s)).then(|| Int::new(f.neg, f.man.shr(s)))
}

/// An exact ball of a small integer.
pub fn int(v: i64) -> Ball {
    Ball::from_i64(v)
}

/// The lower end m - r (exact).
pub fn lower(b: &Ball) -> Float {
    b.mid.sub_exact(&b.rad.to_float())
}

/// The upper end m + r (exact).
pub fn upper(b: &Ball) -> Float {
    b.mid.add_exact(&b.rad.to_float())
}

/// The ball [lo, hi] for floats lo <= hi.
pub fn hull(lo: &Float, hi: &Float, p: u64) -> Ball {
    let sum = lo.add_exact(hi);
    let mid = Float::new(sum.neg, sum.man, sum.exp - 1);
    let b = el::round(Ball::exact(mid.clone()), p);
    let half = hi.sub_exact(lo);
    let half = Float::new(false, half.man, half.exp - 1);
    let shift = b.mid.sub_exact(&mid).abs();
    Ball { rad: b.rad.add(&Mag::from_float_up(&half)).add(&Mag::from_float_up(&shift)), mid: b.mid }
}

/// The nearest double to a float (±0 or ±inf outside the range).
pub fn f64_of(f: &Float) -> f64 {
    mc_arith::num::float::dyadic_to_f64(f.neg, &f.man, f.exp)
}

/// A float with the exact value of a double.
pub fn float_of(v: f64) -> Float {
    match mc_arith::num::float::f64_to_dyadic(v) {
        Some((neg, man, exp)) => Float::new(neg, man, exp),
        None => Float::zero(),
    }
}

/// An upper bound of |x| over the ball.
pub fn mag_up(b: &Ball) -> Mag {
    Mag::from_float_up(&b.mid).add(&b.rad)
}

/// log2 of an upper bound of |x| (-inf for an exact zero).
pub fn log2_up(b: &Ball) -> f64 {
    let m = mag_up(b);
    if m.is_zero() {
        f64::NEG_INFINITY
    } else {
        m.log2()
    }
}

/// The ball widened by `extra`.
pub fn widen(b: Ball, extra: &Mag) -> Ball {
    Ball { mid: b.mid, rad: b.rad.add(extra) }
}

/// Is the ball provably positive?
pub fn is_pos(b: &Ball) -> bool {
    !b.mid.neg && !b.contains_zero()
}

/// Does the ball contain an integer <= `max`? (`max` = 0 checks for the poles of the gamma function.)
pub fn contains_int_at_most(b: &Ball, max: i64) -> bool {
    let lo = lower(b);
    let hi = upper(b);
    let hi_int = floor_float(&hi);
    let Some(hi_int) = hi_int else { return true };
    let top = hi_int.min(max);
    // is there an integer k <= top with k >= lo?
    float_of(top as f64).cmp_exact(&lo) != std::cmp::Ordering::Less
}

/// floor of a float as an i64 (`None` beyond 2^62).
pub fn floor_float(f: &Float) -> Option<i64> {
    if f.top() > 62 {
        return None;
    }
    if f.is_zero() {
        return Some(0);
    }
    if f.exp >= 0 {
        let v = (f.man.to_u128()? << f.exp) as i64;
        return Some(if f.neg { -v } else { v });
    }
    let s = (-f.exp) as u64;
    let q = f.man.shr(s).to_u128()? as i64;
    Some(if !f.neg {
        q
    } else if f.man.low_bits_nonzero(s) {
        -q - 1
    } else {
        -q
    })
}

/// The integer nearest the midpoint and x minus it (an exact subtraction up to the ball's own rounding).
pub fn split_int(x: &Ball, p: u64) -> Option<(i64, Ball)> {
    let half = Float::new(false, Nat::from_u64(1), -1);
    let n = floor_float(&x.mid.add_exact(&half))?;
    Some((n, x.sub(&int(n), p + x.mid.top().max(0) as u64)))
}

/// sin(pi x) with the reduction x = n + f, |f| <= 1/2, so that the relative accuracy near integers is that of f.
pub fn sin_pi(x: &Ball, p: u64) -> Option<Ball> {
    let (n, f) = split_int(x, p)?;
    // the sine's error is absolute: small reduced arguments need their magnitude in extra bits
    let w = p + 8 + (-log2_up(&f)).clamp(0.0, 1e6) as u64;
    let s = el::sin(&pi(w)?.mul(&f, w), w)?;
    let s = el::round(s, p);
    Some(if n.rem_euclid(2) == 1 { s.neg() } else { s })
}

/// cos(pi x) with the same reduction.
pub fn cos_pi(x: &Ball, p: u64) -> Option<Ball> {
    let (n, f) = split_int(x, p)?;
    let w = p + 8 + (-log2_up(&(Ball::from_ratio(1, 2, p).sub(&mag_ball(&f), p)))).clamp(0.0, 1e6) as u64;
    let c = el::cos(&pi(w)?.mul(&f, w), w)?;
    let c = el::round(c, p);
    Some(if n.rem_euclid(2) == 1 { c.neg() } else { c })
}

/// |x| as a ball (the midpoint's absolute value).
fn mag_ball(b: &Ball) -> Ball {
    Ball { mid: b.mid.abs(), rad: b.rad }
}

/// pi at precision p.
pub fn pi(p: u64) -> Option<Ball> {
    Const::Pi.ball(p)
}

/// Euler's constant at precision p.
pub fn euler_gamma(p: u64) -> Option<Ball> {
    Const::EulerGamma.ball(p)
}

/// Correct bits of a ball relative to its magnitude (infinite for an exact ball, 0 when it contains zero).
pub fn accuracy(b: &Ball) -> f64 {
    if b.rad.is_zero() {
        return f64::INFINITY;
    }
    if b.contains_zero() {
        return 0.0;
    }
    b.rel_bits()
}

fn caccuracy(z: &CBall) -> f64 {
    // the imaginary parts here are multiples of pi or exact zeros: the real part decides, unless it is tiny
    let re = accuracy(&z.re);
    if z.im.mid.is_zero() && z.im.rad.is_zero() {
        return re;
    }
    re.min(accuracy(&z.im))
}

/// The working precisions of a function of target precision p: p + guard, doubled while the result has fewer than
/// p - 2 correct bits, up to max(16 p, p + 4096) bits. The rounds stop early when the radius no longer shrinks (the
/// arguments' own radii limit the result), returning the best enclosure found; `f` returns `None` for no value.
pub fn refine(p: u64, guard: u64, mut f: impl FnMut(u64) -> Option<CBall>) -> Option<CBall> {
    let cap = (16 * p).max(p + 4096);
    let mut wp = p + guard.max(16);
    let mut best: Option<CBall> = None;
    loop {
        if stopped() {
            return None;
        }
        let r = f(wp);
        if stopped() {
            return None;
        }
        if let Some(z) = r {
            let acc = caccuracy(&z);
            if acc >= p as f64 + 2.0 {
                return Some(z.round(p));
            }
            if let Some(b) = &best {
                // no progress from doubling: the inputs limit the accuracy
                if acc < caccuracy(b) + 4.0 {
                    return Some(if acc > caccuracy(b) { z } else { b.clone() }.round(p));
                }
            }
            best = Some(z);
        }
        if wp * 2 > cap {
            return best.map(|z| z.round(p));
        }
        wp *= 2;
    }
}

/// A real ball as a complex ball with an exact zero imaginary part.
pub fn real(b: Ball) -> CBall {
    CBall::real(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        let b = rat_ball(&Rat::new(Int::from_i64(-7), Int::from_i64(2)), 64);
        assert_eq!(floor_float(&b.mid), Some(-4));
        assert!(contains_int_at_most(&Ball { mid: float_of(-2.0), rad: Mag::pow2(-10) }, 0));
        assert!(!contains_int_at_most(&Ball { mid: float_of(-2.5), rad: Mag::pow2(-10) }, 0));
        assert!(!contains_int_at_most(&Ball { mid: float_of(2.0), rad: Mag::pow2(-10) }, 0));
        assert!(contains_int_at_most(&Ball { mid: float_of(0.25), rad: Mag::pow2(-1) }, 0));
        let s = sin_pi(&Ball::exact(float_of(1.0 + 2f64.powi(-40))), 100).unwrap();
        assert!(s.rel_bits() > 90.0 && s.mid.neg);
        let h = hull(&float_of(1.0), &float_of(3.0), 64);
        assert!(h.cmp3(&int(2)).is_none() && h.rad.log2() >= 0.0);
        assert_eq!(float_int(&float_of(-6.0)), Some(Int::from_i64(-6)));
        assert_eq!(float_int(&float_of(0.5)), None);
    }
}
