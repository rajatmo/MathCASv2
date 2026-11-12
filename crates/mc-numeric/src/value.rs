//! Exact values behind inexact numbers, shared by the number functions (`rationalize`, `chop`, ...): the binary
//! value of a machine real or a ball midpoint as a rational, the interval of reals a number stands for, decimal
//! logarithms of magnitudes far outside the machine range, and the simplest rational in an interval.
use mc_arith::num::float::f64_to_dyadic;
use mc_arith::num::{Ball, Float, Int, Mag, Nat, Rat};
use mc_arith::Real;
use mc_eval::Res;
use std::cmp::Ordering;

/// log10(2).
pub const LOG10_2: f64 = std::f64::consts::LOG10_2;

/// The digits of a machine real: 53 log10(2).
pub const MACHINE_DIGITS: f64 = 53.0 * LOG10_2;

/// The accuracy given to a machine zero: the digits after the point down to half the smallest subnormal,
/// 1075 log10(2).
pub const MACHINE_ZERO_ACCURACY: f64 = 1075.0 * LOG10_2;

/// The exact value of a binary floating-point number.
pub fn float_rat(f: &Float) -> Rat {
    if f.is_zero() {
        return Rat::from_i64(0);
    }
    let num = Int::new(f.neg, f.man.clone());
    if f.exp >= 0 {
        return Rat::from_int(num.shl(f.exp as u64));
    }
    // man / 2^-exp, reduced by the common powers of two
    let k = f.man.trailing_zeros().min(f.exp.unsigned_abs());
    let man = f.man.shr(k);
    Rat { num: Int::new(f.neg, man), den: Nat::pow2(f.exp.unsigned_abs() - k) }
}

/// The exact value of a finite machine real.
pub fn machine_rat(v: f64) -> Option<Rat> {
    let (neg, man, exp) = f64_to_dyadic(v)?;
    Some(float_rat(&Float::new(neg, man, exp)))
}

/// The exact value a real number stands for: itself, a machine real's binary value, a ball's midpoint.
pub fn point(x: &Real) -> Option<Rat> {
    match x {
        Real::Exact(q) => Some(q.clone()),
        Real::Machine(v) => machine_rat(*v),
        Real::Ball(b, _) => Some(float_rat(&b.mid)),
    }
}

/// The closed interval of reals a number stands for: an exact point; the reals that round to a machine real
/// (half a unit in the last place on either side); a ball's reals. `None` for infinities and NaN.
pub fn interval(x: &Real) -> Option<(Rat, Rat)> {
    match x {
        Real::Exact(q) => Some((q.clone(), q.clone())),
        Real::Ball(b, _) => {
            let r = float_rat(&b.rad.to_float());
            let m = float_rat(&b.mid);
            Some((m.sub(&r), m.add(&r)))
        }
        Real::Machine(v) => {
            let m = machine_rat(*v)?;
            let a = v.abs();
            let bits = a.to_bits();
            // the neighbours of |v| (the largest finite value has the next power of two above it)
            let next = if a == f64::MAX { None } else { Some(f64::from_bits(bits + 1)) };
            let prev = if a == 0.0 { None } else { Some(f64::from_bits(bits - 1)) };
            let a_rat = machine_rat(a)?;
            let half = Rat::new(Int::from_i64(1), Int::from_i64(2));
            let up = match next {
                Some(n) => machine_rat(n)?.sub(&a_rat).mul(&half),
                None => Rat::from_int(Int::from_nat(Nat::pow2(970))),
            };
            // below zero the gap mirrors the one above
            let down = match prev {
                Some(p) => a_rat.sub(&machine_rat(p)?).mul(&half),
                None => up.clone(),
            };
            let (lo, hi) = if v.is_sign_negative() && a != 0.0 { (up, down) } else { (down, up) };
            Some((m.sub(&lo), m.add(&hi)))
        }
    }
}

/// log2 |f| (approximately; -inf for zero), for any exponent.
pub fn log2_float(f: &Float) -> f64 {
    if f.is_zero() {
        return f64::NEG_INFINITY;
    }
    let bits = f.man.bits();
    let shift = bits.saturating_sub(64);
    let top = f.man.shr(shift).to_u128().unwrap_or(0) as f64;
    top.log2() + shift as f64 + f.exp as f64
}

/// log10 |m| of a ball midpoint.
pub fn log10_float(f: &Float) -> f64 {
    log2_float(f) * LOG10_2
}

/// log10 of a radius.
pub fn log10_mag(m: &Mag) -> f64 {
    m.log2() * LOG10_2
}

/// The decimal accuracy of a ball: min(p log10(2) - log10|m|, -log10 r), with a zero midpoint and radius at
/// p log10(2) (decision 1C.4).
pub fn ball_accuracy(b: &Ball, p: u64) -> f64 {
    let work = p as f64 * LOG10_2 - if b.mid.is_zero() { 0.0 } else { log10_float(&b.mid) };
    if b.rad.is_zero() {
        work
    } else {
        work.min(-log10_mag(&b.rad))
    }
}

/// The decimal precision of a ball: accuracy + log10|m|, at least 0 (0 for a zero midpoint).
pub fn ball_precision(b: &Ball, p: u64) -> f64 {
    if b.mid.is_zero() {
        return 0.0;
    }
    (ball_accuracy(b, p) + log10_float(&b.mid)).max(0.0)
}

/// Compare two rationals.
pub fn cmp(a: &Rat, b: &Rat) -> Ordering {
    a.cmp_value(b)
}

/// The simplest rational in the closed interval [lo, hi] (lo <= hi): the one with the smallest denominator, and
/// among those the smallest absolute numerator (found by continued fractions, as in the Stern-Brocot tree).
/// `tick` is called once per partial quotient.
pub fn simplest(lo: &Rat, hi: &Rat, tick: &mut dyn FnMut() -> Res<()>) -> Res<Rat> {
    debug_assert!(cmp(lo, hi) != Ordering::Greater);
    if !hi.num.neg && (lo.num.neg || lo.is_zero()) {
        return Ok(Rat::from_i64(0));
    }
    if hi.num.neg {
        return Ok(simplest(&hi.neg(), &lo.neg(), tick)?.neg());
    }
    // 0 < lo <= hi: the partial quotients shared by both ends, then the smallest integer in the last interval
    let (mut lo, mut hi) = (lo.clone(), hi.clone());
    let mut quotients: Vec<Int> = vec![];
    loop {
        tick()?;
        let (fl, frac) = lo.floor_fract();
        if frac.is_zero() {
            quotients.push(fl);
            break;
        }
        let ceil = fl.add(&Int::from_i64(1));
        if cmp(&Rat::from_int(ceil.clone()), &hi) != Ordering::Greater {
            quotients.push(ceil);
            break;
        }
        // fl < lo <= hi < fl + 1: continue with 1 / (hi - fl) <= 1 / (lo - fl)
        let f = Rat::from_int(fl.clone());
        let (Some(nlo), Some(nhi)) = (hi.sub(&f).recip(), frac.recip()) else {
            // a zero difference is garbage after a stop
            return Ok(lo);
        };
        quotients.push(fl);
        lo = nlo;
        hi = nhi;
    }
    // the convergent p/q of the quotients (already reduced, q > 0)
    let (mut p0, mut q0) = (Int::from_i64(1), Int::from_i64(0));
    let (mut p1, mut q1) = (Int::from_i64(0), Int::from_i64(1));
    for a in &quotients {
        let p = a.mul(&p0).add(&p1);
        let q = a.mul(&q0).add(&q1);
        (p1, q1) = (p0, q0);
        (p0, q0) = (p, q);
    }
    Ok(Rat { num: p0, den: q0.mag })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: i64, d: i64) -> Rat {
        Rat::new(Int::from_i64(n), Int::from_i64(d))
    }

    fn s(lo: Rat, hi: Rat) -> Rat {
        simplest(&lo, &hi, &mut || Ok(())).unwrap()
    }

    #[test]
    fn simplest_rationals() {
        assert_eq!(s(q(314059, 100000), q(314259, 100000)), q(201, 64));
        assert_eq!(s(q(3, 10), q(4, 10)), q(1, 3));
        assert_eq!(s(q(-1, 2), q(1, 3)), q(0, 1));
        assert_eq!(s(q(-7, 2), q(-3, 1)), q(-3, 1));
        assert_eq!(s(q(5, 2), q(5, 2)), q(5, 2));
        assert_eq!(s(q(-5, 2), q(-5, 2)), q(-5, 2));
        assert_eq!(s(q(7, 3), q(8, 3)), q(5, 2));
        assert_eq!(s(q(31415, 10000), q(31416, 10000)), q(333, 106));
        // 22/7 in the interval of 355/113 plus or minus 1/100
        assert_eq!(s(q(355, 113).sub(&q(1, 100)), q(355, 113).add(&q(1, 100))), q(22, 7));
    }

    /// Against brute force: the result lies in the interval and no smaller denominator has a fraction there (nor
    /// the same denominator a smaller numerator), on random intervals.
    #[test]
    fn simplest_against_brute_force() {
        let mut seed = 5u64;
        let mut next = |m: i64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as i64 % m
        };
        for _ in 0..2000 {
            let (a, b) = (next(4000) - 2000, next(300) + 1);
            let (c, d) = (next(400), next(300) + 1);
            let lo = q(a, b);
            let hi = lo.add(&q(c, d * 50));
            let r = s(lo.clone(), hi.clone());
            assert!(cmp(&lo, &r) != Ordering::Greater && cmp(&r, &hi) != Ordering::Greater);
            let rq = r.den.to_u128().unwrap() as i64;
            let rp = r.num.to_i64().unwrap();
            for den in 1..=rq {
                // the integers p with lo <= p/den <= hi
                let (lo_p, hi_p) = (lo.mul(&q(den, 1)), hi.mul(&q(den, 1)));
                let first = {
                    let (f, frac) = lo_p.floor_fract();
                    f.to_i64().unwrap() + i64::from(!frac.is_zero())
                };
                let last = hi_p.floor_fract().0.to_i64().unwrap();
                if den < rq {
                    assert!(first > last, "{a}/{b} + {c}/{}: {first}/{den} beats {rp}/{rq}", d * 50);
                } else {
                    let best = (first..=last).min_by_key(|p| p.abs()).unwrap();
                    assert_eq!(best.abs(), rp.abs(), "{a}/{b} + {c}/{}", d * 50);
                }
            }
        }
    }

    #[test]
    fn exact_values_of_inexact_numbers() {
        assert_eq!(machine_rat(0.5), Some(q(1, 2)));
        assert_eq!(machine_rat(-3.0), Some(q(-3, 1)));
        assert_eq!(machine_rat(0.1).unwrap().den, Nat::pow2(55));
        assert_eq!(machine_rat(f64::NAN), None);
        let (lo, hi) = interval(&Real::Machine(0.1)).unwrap();
        assert_eq!(s(lo.clone(), hi.clone()), q(1, 10));
        assert!(cmp(&lo, &q(1, 10)) == Ordering::Less && cmp(&hi, &q(1, 10)) == Ordering::Greater);
        let (lo, hi) = interval(&Real::Machine(-2.5)).unwrap();
        assert!(cmp(&lo, &q(-5, 2)) == Ordering::Less && cmp(&hi, &q(-5, 2)) == Ordering::Greater);
        // a power of two: the gap below is half the gap above
        let (lo, hi) = interval(&Real::Machine(1.0)).unwrap();
        assert_eq!(q(1, 1).sub(&lo).mul(&q(2, 1)), hi.sub(&q(1, 1)));
        let (lo, hi) = interval(&Real::Machine(0.0)).unwrap();
        assert_eq!(lo.neg(), hi);
        assert!((log2_float(&Float::from_i64(1024)) - 10.0).abs() < 1e-12);
        assert!((MACHINE_DIGITS - 15.954589770191003).abs() < 1e-12);
    }
}
