//! Adaptive precision (semantics 2.6, benchmark N11): evaluate with balls at p = d log2(10) + guard bits, accept
//! when the ball has d correct digits, otherwise double p, up to a cap.
use crate::ball::{Ball, Float, Mag};
use crate::consts::pi_fixed;
use crate::int::Int;
use crate::nat::Nat;
use std::cmp::Ordering;

pub const GUARD_BITS: u64 = 16;

/// The outcome of `n(e, d)`.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// the last ball computed (None if the last round was undefined, e.g. a division by a ball containing zero)
    pub ball: Option<Ball>,
    /// whether the ball has the requested digits; false means failure(precision_not_reached, best_so_far)
    pub reached: bool,
    pub rounds: u32,
    pub final_bits: u64,
}

/// n(e, d): `f(p)` evaluates the expression with balls at p bits.
pub fn n_eval(f: &dyn Fn(u64) -> Option<Ball>, d: u32, cap_bits: u64) -> Outcome {
    let mut p = (d as f64 * std::f64::consts::LOG2_10).ceil() as u64 + GUARD_BITS;
    let mut rounds = 0;
    loop {
        rounds += 1;
        let b = f(p);
        let reached = b.as_ref().is_some_and(|b| b.has_digits(d));
        // the cap bounds the loop (semantics 10); the stop flag ends it early
        if reached || 2 * p > cap_bits || crate::stopped() {
            return Outcome { ball: b, reached, rounds, final_bits: p };
        }
        p *= 2;
    }
}

/// A ball containing pi at p bits: pi_fixed is within 2 units of pi 2^bits.
pub fn pi_ball(p: u64) -> Ball {
    let bits = p + 8;
    let x: Int = pi_fixed::<Int>(bits);
    let (m, e) = Float::round(false, x.mag, -(bits as i64), p);
    Ball { mid: m, rad: Mag::pow2(1 - bits as i64).add(&e) }
}

fn int(v: i64) -> Ball {
    Ball::from_i64(v)
}

fn pow10(k: u64) -> Ball {
    Ball::from_nat(Nat::pow(10, k))
}

/// sqrt(10^60 + 1) - 10^30, about 5 10^-31: 60 digits cancel.
pub fn e1_sqrt_cancel(p: u64) -> Option<Ball> {
    let n = pow10(60).add(&int(1), p);
    Some(n.sqrt(p)?.sub(&pow10(30), p))
}

/// Muller's recurrence u_{n+1} = 111 - 1130/u_n + 3000/(u_n u_{n-1}), u_0 = 2, u_1 = -4, at n = 40. The exact
/// sequence tends to 6, but rounding errors are amplified by about (100/6) per step towards the attracting fixed
/// point 100: about 162 bits are lost by n = 40.
pub const MULLER_N: u64 = 40;

pub fn e2_muller(p: u64) -> Option<Ball> {
    let (mut a, mut b) = (int(2), int(-4));
    for _ in 1..MULLER_N {
        let t1 = int(1130).div(&b, p)?;
        let t2 = int(3000).div(&b.mul(&a, p), p)?;
        let c = int(111).sub(&t1, p).add(&t2, p);
        a = b;
        b = c;
    }
    Some(b)
}

/// The exact u_n = (3 6^(n+1) - 4 5^(n+1)) / (3 6^n - 4 5^n) (from the closed form with u_0 = 2, u_1 = -4).
pub fn muller_exact(n: u64) -> (Int, Int) {
    let f = |b: u64, k: u64, c: u64| Int::from_nat(Nat::pow(b, k).mul_u64(c));
    (f(6, n + 1, 3).sub(&f(5, n + 1, 4)), f(6, n, 3).sub(&f(5, n, 4)))
}

/// pi minus its 51-digit decimal truncation, about 5.8 10^-51.
pub const PI51: &str = "314159265358979323846264338327950288419716939937510";

pub fn e3_pi_cancel(p: u64) -> Option<Ball> {
    let c = Ball::from_nat(dec(PI51)).div(&pow10(50), p)?;
    Some(pi_ball(p).sub(&c, p))
}

/// Rump's example 333.75 b^6 + a^2 (11 a^2 b^2 - b^6 - 121 b^4 - 2) + 5.5 b^8 + a / (2b) at a = 77617, b = 33096.
/// The polynomial part is exactly -2, the value -54767/66192. At 53 bits it famously comes out as 1.17...; with
/// balls at p >= 122 bits every integer step is exact.
pub fn e4_rump(p: u64) -> Option<Ball> {
    let a = int(77617);
    let b = int(33096);
    let a2 = a.mul(&a, p);
    let b2 = b.mul(&b, p);
    let b4 = b2.mul(&b2, p);
    let b6 = b4.mul(&b2, p);
    let b8 = b4.mul(&b4, p);
    let inner = int(11).mul(&a2, p).mul(&b2, p).sub(&b6, p).sub(&int(121).mul(&b4, p), p).sub(&int(2), p);
    let t1 = Ball::from_ratio(1335, 4, p).mul(&b6, p);
    let t3 = Ball::from_ratio(11, 2, p).mul(&b8, p);
    let t4 = a.div(&int(2).mul(&b, p), p)?;
    Some(t1.add(&a2.mul(&inner, p), p).add(&t3, p).add(&t4, p))
}

/// sqrt(2)^2 - 2 = 0: no precision gives relative digits for an exact zero, so n() must fail at the cap.
pub fn e5_zero(p: u64) -> Option<Ball> {
    let s = int(2).sqrt(p)?;
    Some(s.mul(&s, p).sub(&int(2), p))
}

pub fn dec(s: &str) -> Nat {
    // short constants only: Horner in base 10^18
    let mut n = Nat::zero();
    for chunk in s.as_bytes().chunks(18) {
        let v: u64 = std::str::from_utf8(chunk).unwrap().parse().unwrap();
        n = n.mul_u64(10u64.pow(chunk.len() as u32)).add_u64(v);
    }
    n
}

fn to_float(x: &Int) -> Float {
    Float::new(x.neg, x.mag.clone(), 0)
}

/// Does the ball contain the rational num / den (den > 0)? |num - m den| <= r den, exactly.
pub fn contains_ratio(b: &Ball, num: &Int, den: &Int) -> bool {
    let (n, d) = (to_float(num), to_float(den));
    let lhs = n.sub_exact(&b.mid.mul_exact(&d)).abs();
    lhs.cmp_exact(&b.rad.to_float().mul_exact(&d)) != Ordering::Greater
}

/// Does the ball contain x?
pub fn contains(b: &Ball, x: &Float) -> bool {
    x.sub_exact(&b.mid).abs().cmp_exact(&b.rad.to_float()) != Ordering::Greater
}

/// The independent check of each expression's ball: true when the exact value lies in the ball.
pub fn check(id: &str, b: &Ball) -> bool {
    match id {
        "E1" => {
            // v = sqrt(N) - 10^30 lies in [lo, hi] iff (lo + 10^30)^2 <= N <= (hi + 10^30)^2 (with hi + 10^30 >= 0)
            let n = Float::from_nat(Nat::pow(10, 60).add_u64(1));
            let t = Float::from_nat(Nat::pow(10, 30));
            let r = b.rad.to_float();
            let lo = b.mid.sub_exact(&r).add_exact(&t);
            let hi = b.mid.add_exact(&r).add_exact(&t);
            let lo_ok = lo.neg || lo.mul_exact(&lo).cmp_exact(&n) != Ordering::Greater;
            !hi.neg && lo_ok && hi.mul_exact(&hi).cmp_exact(&n) != Ordering::Less
        }
        "E2" => {
            let (num, den) = muller_exact(MULLER_N);
            let (num, den) = if den.neg { (num.neg(), den.neg()) } else { (num, den) };
            contains_ratio(b, &num, &den)
        }
        "E3" => {
            // pi lies in [D, D + 1] / 10^199 with D its first 200 digits (checked independently by the constants
            // tests); both ends minus c must lie in the ball
            let d = dec(&crate::consts::pi_digits::<Int>(200));
            let c = dec(PI51).mul(&Nat::pow(10, 149));
            let den = Int::from_nat(Nat::pow(10, 199));
            let lo = Int::from_nat(d.clone()).sub(&Int::from_nat(c.clone()));
            let hi = Int::from_nat(d.add_u64(1)).sub(&Int::from_nat(c));
            contains_ratio(b, &lo, &den) && contains_ratio(b, &hi, &den)
        }
        "E4" => contains_ratio(b, &Int::from_i64(-54767), &Int::from_i64(66192)),
        "E5" => contains(b, &Float::zero()),
        _ => false,
    }
}

pub type Expr = (&'static str, &'static str, fn(u64) -> Option<Ball>);

pub fn expressions() -> Vec<Expr> {
    vec![
        ("E1", "sqrt(10^60 + 1) - 10^30", e1_sqrt_cancel),
        ("E2", "Muller recurrence u_40", e2_muller),
        ("E3", "pi - 51-digit truncation", e3_pi_cancel),
        ("E4", "Rump's polynomial", e4_rump),
        ("E5", "sqrt(2)^2 - 2 (exact zero)", e5_zero),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n11_reaches_50_digits_and_encloses() {
        for (id, _, f) in expressions() {
            let o = n_eval(&f, 50, 1 << 13);
            let b = o.ball.clone().expect("defined at the final precision");
            assert!(check(id, &b), "{id}: exact value outside the ball");
            if id == "E5" {
                assert!(!o.reached, "an exact zero must not reach relative digits");
            } else {
                assert!(o.reached, "{id}: {o:?}");
            }
        }
        // E1, E2 and E3 need more than the first precision
        assert!(n_eval(&e1_sqrt_cancel, 50, 1 << 13).rounds >= 2);
        assert!(n_eval(&e2_muller, 50, 1 << 13).rounds >= 2);
        // at the first precision the Muller ball is honest: it is wide (or undefined), not wrong
        if let Some(b) = e2_muller(182) {
            assert!(check("E2", &b) && !b.has_digits(50));
        }
    }
}
