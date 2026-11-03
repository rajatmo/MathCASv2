//! Elementary functions on balls (semantics 2.3): every result encloses the function's values on the whole
//! argument ball, or the function returns `None` when it cannot (an argument outside the domain or one that may
//! touch its boundary, a size beyond the supported range, or a raised stop flag).
//!
//! The primitives work on an exact midpoint and add the argument's radius through a derivative bound:
//!
//! - **exp**: exp(m) = e^k exp(f) with k = floor(m), f in [0, 1); exp(f) by the bit-burst method: f is cut into
//!   chunks of 8, 8, 16, 32, ... bits, each chunk's exponential summed by binary splitting (a chunk of b bits at
//!   position 2^j needs about p / 2^j terms of b-bit numbers), the chunks multiplied. Quasi-linear in p.
//! - **sin, cos**: reduction by k pi/2, then the same bit-burst on the sine series with the cosine from
//!   sqrt(1 - sin^2) per chunk and the addition theorems between chunks.
//! - **log**: Newton's iteration y += x exp(-y) - 1 at doubling precisions; the last step is the enclosure:
//!   log x = y + log(1 + t) with t = x exp(-y) - 1 a ball and |log(1 + t) - t| <= t^2 for |t| <= 1/2.
//! - **atan**: Newton on tan with the same final step, atan x = y + atan(z) with z = (x - tan y)/(1 + x tan y) and
//!   |atan z - z| <= |z|^3; |x| > 1 through atan x = ±pi/2 - atan(1/x).
//!
//! The others are ball expressions of these, enclosures by construction: tan = sin/cos,
//! asin x = atan(x / sqrt(1 - x^2)), acos = pi/2 - asin, sinh, cosh and tanh from exp, x^y = exp(y log x).
//! Decision L.4: algorithms and their bounds; L.5: the supported ranges.
use crate::consts::Const;
use crate::series::{log2_factorial_lower, partial_sum, widen, Ratio};
use mc_arith::num::float::{dyadic_to_f64, f64_to_dyadic};
use mc_arith::num::{stopped, Ball, Float, Int, Mag, Nat, StopScope};
use std::cmp::Ordering;

/// exp is supported for |x| < 2^EXP_TOP (beyond that the result's exponent is not a sensible number).
const EXP_TOP: i64 = 40;
/// sin and cos are supported for |x| < 2^TRIG_TOP (the reduction needs pi to that many extra bits).
const TRIG_TOP: i64 = 1 << 22;

// ---------------------------------------------------------------- helpers

/// The ball with its midpoint rounded to p bits (the rounding error joins the radius).
pub fn round(b: Ball, p: u64) -> Ball {
    let (mid, err) = Float::round(b.mid.neg, b.mid.man, b.mid.exp, p);
    Ball { mid, rad: b.rad.add(&err) }
}

fn one() -> Ball {
    Ball::from_i64(1)
}

/// A ball containing y (1 + t) for every y in b and |t| <= tau.
fn widen_rel(b: Ball, tau: &Mag) -> Ball {
    let extra = Mag::from_float_up(&b.mid).add(&b.rad).mul(tau);
    widen(b, &extra)
}

/// An upper bound of a non-negative machine real as a radius.
fn mag_from_f64_up(v: f64) -> Option<Mag> {
    let (_, man, exp) = f64_to_dyadic(v)?;
    Some(Mag::from_float_up(&Float::new(false, man, exp)))
}

/// The radius as a machine real (exact for radii in the machine range).
fn mag_to_f64(m: &Mag) -> f64 {
    dyadic_to_f64(false, &Nat::from_u64(m.m), m.e)
}

/// An upper bound of e^r - 1 for a radius r, or `None` beyond r = 256.
fn expm1_up(r: &Mag) -> Option<Mag> {
    if r.is_zero() {
        return Some(Mag::ZERO);
    }
    if r.log2() < -10.0 {
        // e^r - 1 <= r e^r <= 2r for r <= 2^-10
        return Some(r.mul_2exp(1));
    }
    let v = mag_to_f64(r);
    if v > 256.0 {
        return None;
    }
    // a machine exp_m1 is within an ulp or two; the factor covers it
    mag_from_f64_up(v.exp_m1() * (1.0 + 1e-9))
}

/// The machine real nearest the midpoint, also for midpoints outside the machine range (as ±0 or ±inf).
fn mid_f64(f: &Float) -> f64 {
    dyadic_to_f64(f.neg, &f.man, f.exp)
}

fn from_f64(v: f64) -> Float {
    match f64_to_dyadic(v) {
        Some((neg, man, exp)) => Float::new(neg, man, exp),
        None => Float::zero(),
    }
}

/// The integer nearest f (halves away from zero).
fn round_to_int(f: &Float) -> Int {
    if f.exp >= 0 {
        return Int::new(f.neg, f.man.shl(f.exp as u64));
    }
    let s = (-f.exp) as u64;
    let q = f.man.add(&Nat::pow2(s - 1)).shr(s);
    Int::new(f.neg, q)
}

/// floor(f) for |f| < 2^63.
fn floor_i64(f: &Float) -> i64 {
    if f.exp >= 0 {
        let v = (f.man.to_u128().unwrap_or(0) << f.exp) as i64;
        return if f.neg { -v } else { v };
    }
    let s = (-f.exp) as u64;
    let q = f.man.shr(s).to_u128().unwrap_or(0) as i64;
    if !f.neg {
        q
    } else if f.man.low_bits_nonzero(s) {
        -q - 1
    } else {
        -q
    }
}

/// Precisions for Newton's iteration ending at w: each about half the next, from about 64 bits.
fn newton_steps(w: u64) -> Vec<u64> {
    let mut v = vec![w];
    loop {
        let q = *v.last().expect("nonempty") / 2 + 16;
        if q < 64 || q >= *v.last().expect("nonempty") {
            break;
        }
        v.push(q);
    }
    v.reverse();
    v
}

/// The fraction bits 1..=w of x (0 <= x < 1) as an integer X with x ~ X / 2^w, and whether bits were cut off
/// (then 0 < x - X/2^w < 2^-w).
fn fraction_bits(x: &Float, w: u64) -> (Nat, bool) {
    let shift = x.exp + w as i64;
    if shift >= 0 {
        (x.man.shl(shift as u64), false)
    } else {
        let s = (-shift) as u64;
        (x.man.shr(s), x.man.low_bits_nonzero(s))
    }
}

/// The bit-burst chunks of X / 2^w: (a, m) with value a / 2^m, at fraction positions (0, 8], (8, 16], (16, 32],
/// (32, 64], ...: chunk j > 0 is below 2^-(its start) and has as many bits as its start.
fn chunks(x: &Nat, w: u64) -> Vec<(Nat, u64)> {
    let mut out = vec![];
    let (mut lo, mut hi) = (0u64, 8u64.min(w));
    while lo < w {
        let a = x.shr(w - hi).low_bits(hi - lo);
        if !a.is_zero() {
            out.push((a, hi));
        }
        lo = hi;
        hi = (2 * hi).min(w);
    }
    out
}

// ---------------------------------------------------------------- exp

/// The ratio a / (k 2^m) of the exponential series at a / 2^m.
struct ExpRatio<'a> {
    a: &'a Int,
    m: u64,
}

impl Ratio for ExpRatio<'_> {
    fn p(&self, _: u64) -> Int {
        self.a.clone()
    }
    fn q(&self, k: u64) -> Nat {
        Nat::from_u64(k)
    }
    fn shift(&self) -> u64 {
        self.m
    }
}

/// exp(a / 2^m) for a < 2^m, at w bits: N terms with x^N/N! <= 2^-(w+4) (x < 2^(b-m) for b-bit a), tail below
/// twice the first omitted term.
fn exp_chunk(a: Nat, m: u64, w: u64) -> Ball {
    let gap = m - a.bits();
    let mut n = 2u64;
    while n * gap + log2_factorial_lower(n) < w + 4 {
        n += 1;
    }
    let tail = Mag::pow2(1 - (n * gap + log2_factorial_lower(n)) as i64);
    let a = Int::from_nat(a);
    widen(partial_sum(&ExpRatio { a: &a, m }, n, w), &tail)
}

/// exp(x) for exact 0 <= x < 1 at w bits, by the bit-burst method.
fn exp_unit(x: &Float, w: u64) -> Ball {
    if x.is_zero() {
        return one();
    }
    let fw = w + 10;
    let (bits, cut) = fraction_bits(x, fw);
    let parts = chunks(&bits, fw);
    let wp = w + 8 + 64 - (parts.len() as u64).leading_zeros() as u64;
    let mut r = one();
    for (a, m) in parts {
        if stopped() {
            break;
        }
        r = r.mul(&exp_chunk(a, m, wp), wp);
    }
    // exp(x) = exp(x_cut) e^d with 0 < d < 2^-fw, and e^d - 1 < 2^(1 - fw)
    if cut {
        r = widen_rel(r, &Mag::pow2(1 - fw as i64));
    }
    r
}

/// x^n for n >= 0 by squaring, at w bits.
fn pow_nat(x: &Ball, n: u64, w: u64) -> Ball {
    let mut r = one();
    let mut base = x.clone();
    let mut n = n;
    while n > 0 {
        if stopped() {
            break;
        }
        if n & 1 == 1 {
            r = r.mul(&base, w);
        }
        n >>= 1;
        if n > 0 {
            base = base.mul(&base, w);
        }
    }
    r
}

/// exp(m) for an exact m at w bits: e^floor(m) exp(m - floor(m)).
fn exp_mid(m: &Float, w: u64) -> Option<Ball> {
    if m.is_zero() {
        return Some(one());
    }
    if m.top() > EXP_TOP {
        return None;
    }
    let k = floor_i64(m);
    let f = m.sub_exact(&Float::from_i64(k));
    let r = exp_unit(&f, w + 4);
    if k == 0 {
        return Some(r);
    }
    // the relative error of e grows k-fold in e^k
    let g = 64 - k.unsigned_abs().leading_zeros() as u64 + 8;
    let ek = pow_nat(&Const::E.ball(w + g)?, k.unsigned_abs(), w + g);
    let ek = if k < 0 { one().div(&ek, w + g)? } else { ek };
    Some(r.mul(&ek, w + 4))
}

/// exp(x) for a ball.
pub fn exp(x: &Ball, p: u64) -> Option<Ball> {
    let r = exp_mid(&x.mid, p + 8)?;
    let r = if x.rad.is_zero() { r } else { widen_rel(r, &expm1_up(&x.rad)?) };
    (!stopped()).then(|| round(r, p))
}

// ---------------------------------------------------------------- sin, cos

/// The ratio -a^2 / (2k (2k + 1) 2^(2m)) of the sine series sin x = x sum (-x^2)^k / (2k + 1)!.
struct SinRatio {
    a2: Int,
    m: u64,
}

impl Ratio for SinRatio {
    fn p(&self, _: u64) -> Int {
        self.a2.clone()
    }
    fn q(&self, k: u64) -> Nat {
        Nat::from_u64(2 * k * (2 * k + 1))
    }
    fn shift(&self) -> u64 {
        2 * self.m
    }
}

/// (sin, cos) of a / 2^m (a < 2^m) at w bits. The series alternates with decreasing terms, so its tail is below the
/// first omitted term, x^(2N) / (2N + 1)!; cos = sqrt(1 - sin^2) since 0 <= x < 1.
fn sin_cos_chunk(a: Nat, m: u64, w: u64) -> Option<(Ball, Ball)> {
    let gap = m - a.bits();
    let mut n = 1u64;
    while 2 * n * gap + log2_factorial_lower(2 * n + 1) < w + 4 {
        n += 1;
    }
    let tail = Mag::pow2(-((2 * n * gap + log2_factorial_lower(2 * n + 1)) as i64));
    let x = Float::new(false, a.clone(), -(m as i64));
    let ratio = SinRatio { a2: Int::new(true, a.square()), m };
    let s = widen(partial_sum(&ratio, n, w), &tail).mul(&Ball::exact(x), w);
    let c = sqrt(&one().sub(&s.mul(&s, w), w), w)?;
    Some((s, c))
}

/// (sin, cos) of an exact |r| < 1 at w bits, by the bit-burst method with the addition theorems.
fn sin_cos_unit(r: &Float, w: u64) -> Option<(Ball, Ball)> {
    if r.is_zero() {
        return Some((Ball::from_i64(0), one()));
    }
    let fw = w + 10;
    let (bits, cut) = fraction_bits(&r.abs(), fw);
    let parts = chunks(&bits, fw);
    let wp = w + 8 + 64 - (parts.len() as u64).leading_zeros() as u64;
    let (mut s, mut c) = (Ball::from_i64(0), one());
    for (a, m) in parts {
        if stopped() {
            return None;
        }
        let (sj, cj) = sin_cos_chunk(a, m, wp)?;
        let s2 = s.mul(&cj, wp).add(&c.mul(&sj, wp), wp);
        c = c.mul(&cj, wp).sub(&s.mul(&sj, wp), wp);
        s = s2;
    }
    // sin and cos are 1-Lipschitz: the cut-off bits (below 2^-fw) widen both
    if cut {
        let d = Mag::pow2(-(fw as i64));
        s = widen(s, &d);
        c = widen(c, &d);
    }
    Some((if r.neg { s.neg() } else { s }, c))
}

/// (sin, cos) of an exact m at w bits: m = r + k pi/2 with |r| <= pi/4 (about), then the quadrant.
fn sin_cos_mid(m: &Float, w: u64) -> Option<(Ball, Ball)> {
    if m.is_zero() || m.top() <= 0 {
        return sin_cos_unit(m, w);
    }
    if m.top() > TRIG_TOP {
        return None;
    }
    let g = m.top() as u64 + 12;
    let half_pi = Const::half_pi(w + g)?;
    let (q, _) = m.div(&half_pi.mid, m.top() as u64 + 16);
    let k = round_to_int(&q);
    let kb = Ball::exact(Float::new(k.neg, k.mag.clone(), 0));
    let r = Ball::exact(m.clone()).sub(&kb.mul(&half_pi, w + g), w + g);
    if r.mid.top() > 0 {
        return None; // cannot happen for a correct quotient; stay safe
    }
    let (s, c) = sin_cos_unit(&r.mid, w)?;
    let (s, c) = (widen(s, &r.rad), widen(c, &r.rad));
    let quadrant = k.mag.limbs.first().map_or(0, |l| l & 3);
    let quadrant = if k.neg { (4 - quadrant) & 3 } else { quadrant };
    Some(match quadrant {
        0 => (s, c),
        1 => (c, s.neg()),
        2 => (s.neg(), c.neg()),
        _ => (c.neg(), s),
    })
}

/// (sin x, cos x) for a ball.
pub fn sin_cos(x: &Ball, p: u64) -> Option<(Ball, Ball)> {
    let (s, c) = sin_cos_mid(&x.mid, p + 8)?;
    if stopped() {
        return None;
    }
    Some((round(widen(s, &x.rad), p), round(widen(c, &x.rad), p)))
}

/// sin(x) for a ball.
pub fn sin(x: &Ball, p: u64) -> Option<Ball> {
    sin_cos(x, p).map(|(s, _)| s)
}

/// cos(x) for a ball.
pub fn cos(x: &Ball, p: u64) -> Option<Ball> {
    sin_cos(x, p).map(|(_, c)| c)
}

/// tan(x) = sin/cos; `None` when the cosine ball contains zero.
pub fn tan(x: &Ball, p: u64) -> Option<Ball> {
    let (s, c) = sin_cos(x, p + 4)?;
    Some(round(s.div(&c, p + 4)?, p))
}

// ---------------------------------------------------------------- log

/// log(m) for exact m > 0 at w bits.
fn log_mid(m: &Float, w: u64) -> Option<Ball> {
    if m.cmp_exact(&Float::from_i64(1)) == Ordering::Equal {
        return Some(Ball::from_i64(0));
    }
    // a machine estimate from the top bits and the exponent
    let t = m.top();
    let frac = dyadic_to_f64(false, &m.man, -(m.man.bits() as i64));
    let mut y = from_f64(frac.ln() + t as f64 * std::f64::consts::LN_2);
    let xb = Ball::exact(m.clone());
    let steps = newton_steps(w);
    for &q in &steps[..steps.len() - 1] {
        let e = exp_mid(&y.neg(), q)?;
        let t = xb.mul(&e, q).sub(&one(), q);
        y = round(Ball::exact(y.add_exact(&t.mid)), q).mid;
        if stopped() {
            return None;
        }
    }
    // the enclosure: log m = y + log(1 + t), t = m exp(-y) - 1, |log(1 + t) - t| <= |t|^2 for |t| <= 1/2
    let e = exp_mid(&y.neg(), w + 4)?;
    let t = xb.mul(&e, w + 4).sub(&one(), w + 4);
    let tau = Mag::from_float_up(&t.mid).add(&t.rad);
    if tau.to_float().cmp_exact(&Float::new(false, Nat::from_u64(1), -1)) == Ordering::Greater {
        return None;
    }
    let r = Ball::exact(y).add(&Ball::exact(t.mid), w);
    Some(widen(r, &t.rad.add(&tau.mul(&tau))))
}

/// log(x) for a ball that is provably positive.
pub fn log(x: &Ball, p: u64) -> Option<Ball> {
    if x.mid.neg || x.mid.is_zero() {
        return None;
    }
    let lo = Mag::from_float_down(&x.mid).sub_down(&x.rad);
    if lo.is_zero() {
        return None;
    }
    let r = log_mid(&x.mid, p + 8)?;
    // |log(m + t) - log(m)| <= r / (m - r) for |t| <= r
    let r = if x.rad.is_zero() { r } else { widen(r, &x.rad.div_up(&lo)) };
    (!stopped()).then(|| round(r, p))
}

// ---------------------------------------------------------------- atan

/// atan(m) for exact |m| <= 1 at w bits.
fn atan_small(m: &Float, w: u64) -> Option<Ball> {
    if m.is_zero() {
        return Some(Ball::from_i64(0));
    }
    let mut y = from_f64(mid_f64(m).atan());
    let xb = Ball::exact(m.clone());
    let steps = newton_steps(w);
    for &q in &steps[..steps.len() - 1] {
        // y - (tan y - m) cos^2 y = y - sin y cos y + m cos^2 y
        let (s, c) = sin_cos_mid(&y, q)?;
        let d = xb.mul(&c, q).mul(&c, q).sub(&s.mul(&c, q), q);
        y = round(Ball::exact(y.add_exact(&d.mid)), q).mid;
        if stopped() {
            return None;
        }
    }
    // the enclosure: atan m = y + atan z, z = (m - tan y) / (1 + m tan y), |atan z - z| <= |z|^3
    let wq = w + 4;
    let (s, c) = sin_cos_mid(&y, wq)?;
    let tn = s.div(&c, wq)?;
    let z = xb.sub(&tn, wq).div(&one().add(&xb.mul(&tn, wq), wq), wq)?;
    let tau = Mag::from_float_up(&z.mid).add(&z.rad);
    if tau.to_float().cmp_exact(&Float::new(false, Nat::from_u64(1), -1)) == Ordering::Greater {
        return None;
    }
    let r = Ball::exact(y).add(&Ball::exact(z.mid), w);
    Some(widen(r, &z.rad.add(&tau.mul(&tau).mul(&tau))))
}

/// atan(m) for exact m at w bits; |m| > 1 through ±pi/2 - atan(1/m).
fn atan_mid(m: &Float, w: u64) -> Option<Ball> {
    if m.abs().cmp_exact(&Float::from_i64(1)) != Ordering::Greater {
        return atan_small(m, w);
    }
    let z = one().div(&Ball::exact(m.clone()), w + 4)?;
    let a = widen(atan_small(&z.mid, w + 4)?, &z.rad);
    let hp = Const::half_pi(w + 4)?;
    let hp = if m.neg { hp.neg() } else { hp };
    Some(hp.sub(&a, w))
}

/// atan(x) for a ball (atan is 1-Lipschitz).
pub fn atan(x: &Ball, p: u64) -> Option<Ball> {
    let r = atan_mid(&x.mid, p + 8)?;
    (!stopped()).then(|| round(widen(r, &x.rad), p))
}

// ---------------------------------------------------------------- derived

/// asin(x) = atan(x / sqrt(1 - x^2)); asin(±1) = ±pi/2 for exact ±1; `None` unless |x| < 1 provably otherwise.
pub fn asin(x: &Ball, p: u64) -> Option<Ball> {
    let w = p + 8;
    if x.rad.is_zero() && x.mid.abs().cmp_exact(&Float::from_i64(1)) == Ordering::Equal {
        let hp = Const::half_pi(p)?;
        return Some(if x.mid.neg { hp.neg() } else { hp });
    }
    let root = sqrt(&one().sub(&x.mul(x, w), w), w)?;
    let r = atan(&x.div(&root, w)?, w)?;
    Some(round(r, p))
}

/// acos(x) = pi/2 - asin(x).
pub fn acos(x: &Ball, p: u64) -> Option<Ball> {
    let w = p + 8;
    let r = Const::half_pi(w)?.sub(&asin(x, w)?, w);
    Some(round(r, p))
}

/// sinh(x) = (e^x - e^-x) / 2.
pub fn sinh(x: &Ball, p: u64) -> Option<Ball> {
    let w = p + 8;
    let e = exp(x, w)?;
    let r = e.sub(&one().div(&e, w)?, w).mul(&Ball::from_ratio(1, 2, w), w);
    Some(round(r, p))
}

/// cosh(x) = (e^x + e^-x) / 2.
pub fn cosh(x: &Ball, p: u64) -> Option<Ball> {
    let w = p + 8;
    let e = exp(x, w)?;
    let r = e.add(&one().div(&e, w)?, w).mul(&Ball::from_ratio(1, 2, w), w);
    Some(round(r, p))
}

/// tanh(x) = (e^2x - 1) / (e^2x + 1), with tanh(-x) = -tanh(x) so that e^2x does not overflow the range.
pub fn tanh(x: &Ball, p: u64) -> Option<Ball> {
    if x.mid.neg {
        return tanh(&x.neg(), p).map(|r| r.neg());
    }
    let w = p + 8;
    let e2 = exp(&x.mul(&Ball::from_i64(2), w), w)?;
    let r = e2.sub(&one(), w).div(&e2.add(&one(), w), w)?;
    Some(round(r, p))
}

/// sqrt(x) for a provably positive ball, or an exact zero. The integer square root below cannot be interrupted (L.11)
/// (its correction loop does not survive the garbage of a stopped multiplication), so it runs with the stop flag
/// off; it costs a few multiplications.
pub fn sqrt(x: &Ball, p: u64) -> Option<Ball> {
    if x.mid.is_zero() && x.rad.is_zero() {
        return Some(x.clone());
    }
    if stopped() {
        return None;
    }
    let _uninterruptible = StopScope::enter(None);
    x.sqrt(p)
}

/// x^n for an integer n (a negative power needs a ball excluding zero). The relative error grows n-fold, so the
/// squarings run with log2 n extra bits.
pub fn pow_int(x: &Ball, n: &Int, p: u64) -> Option<Ball> {
    let e = n.to_i64()?;
    let m = e.unsigned_abs();
    // the result's exponent must stay far inside i64
    let top = x.mid.top().unsigned_abs() + 2;
    if m > 0 && top.checked_mul(m).is_none_or(|v| v > 1 << 60) {
        return None;
    }
    let g = 64 - m.leading_zeros() as u64 + 4;
    let r = pow_nat(x, m, p + g);
    let r = if e < 0 { one().div(&r, p + g)? } else { r };
    (!stopped()).then(|| round(r, p))
}

/// x^y = exp(y log x) for a provably positive x.
pub fn pow(x: &Ball, y: &Ball, p: u64) -> Option<Ball> {
    let w = p + 8 + (64 - x.mid.top().unsigned_abs().leading_zeros() as u64);
    let l = log(x, w)?;
    exp(&l.mul(y, w), p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ball(v: f64) -> Ball {
        Ball::exact(from_f64(v))
    }

    fn sci(b: &Ball, d: usize) -> String {
        let (s, e) = b.mid.to_sci(d);
        format!("{}{s}e{e}", if b.mid.neg { "-" } else { "" })
    }

    #[test]
    fn known_values() {
        let p = 200;
        assert_eq!(sci(&exp(&ball(1.0), p).unwrap(), 40), "2718281828459045235360287471352662497757e0");
        assert_eq!(sci(&log(&ball(2.0), p).unwrap(), 40), "6931471805599453094172321214581765680755e-1");
        assert_eq!(sci(&log(&ball(10.0), p).unwrap(), 40), "2302585092994045684017991454684364207601e0");
        assert_eq!(
            sci(&atan(&ball(1.0), p).unwrap().mul(&Ball::from_i64(4), p), 40),
            "3141592653589793238462643383279502884197e0"
        );
        assert_eq!(sci(&sin(&ball(1.0), p).unwrap(), 40), "8414709848078965066525023216302989996226e-1");
        assert_eq!(sci(&cos(&ball(1.0), p).unwrap(), 40), "5403023058681397174009366074429766037323e-1");
        assert_eq!(sci(&sin(&ball(100.0), p).unwrap(), 40), "-5063656411097587936565576104597854320650e-1");
        // exp(-50.5) exp(50.5) = 1, and both have full precision
        let (a, b) = (exp(&ball(-50.5), p).unwrap(), exp(&ball(50.5), p).unwrap());
        assert!(a.rel_bits() > 190.0 && b.rel_bits() > 190.0);
        assert_eq!(a.mul(&b, p).cmp3(&Ball::from_i64(1)), None);
    }

    #[test]
    fn machine_agreement() {
        // the balls at 120 bits must contain the machine results to within a few machine ulps
        let xs = [0.1, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 10.0, 33.3, -0.3, -2.5, 1e-5, 123.456, -700.0, 700.0];
        for &x in &xs {
            let b = ball(x);
            let checks: [(&str, Option<Ball>, f64); 8] = [
                ("exp", exp(&b, 120), x.exp()),
                ("sin", sin(&b, 120), x.sin()),
                ("cos", cos(&b, 120), x.cos()),
                ("atan", atan(&b, 120), x.atan()),
                ("sinh", sinh(&b, 120), x.sinh()),
                ("cosh", cosh(&b, 120), x.cosh()),
                ("tanh", tanh(&b, 120), x.tanh()),
                ("log", log(&ball(x.abs()), 120), x.abs().ln()),
            ];
            for (name, r, want) in checks {
                let r = r.unwrap_or_else(|| panic!("{name}({x})"));
                let got = mid_f64(&r.mid);
                assert!(
                    (got - want).abs() <= 4.0 * f64::EPSILON * want.abs().max(f64::MIN_POSITIVE),
                    "{name}({x}) = {got} vs {want}"
                );
                assert!(r.rel_bits() > 110.0, "{name}({x}): {} bits", r.rel_bits());
            }
        }
    }

    #[test]
    fn domains() {
        assert!(log(&Ball::from_i64(0), 64).is_none());
        assert!(log(&Ball::from_i64(-2), 64).is_none());
        assert!(asin(&Ball::from_i64(2), 64).is_none());
        assert!(sqrt(&Ball::from_i64(-1), 64).is_none());
        let x = Ball { mid: Float::from_i64(1), rad: Mag::pow2(1) };
        assert!(log(&x, 64).is_none());
        let hp = Const::half_pi(64).unwrap();
        // tan at a ball containing pi/2
        assert!(tan(&Ball { mid: hp.mid.clone(), rad: Mag::pow2(-20) }, 64).is_none());
        assert!(exp(&Ball::exact(Float::new(false, Nat::from_u64(1), 45)), 64).is_none());
        let a = asin(&Ball::from_i64(1), 100).unwrap();
        assert_eq!(a.cmp3(&Const::half_pi(100).unwrap()), None);
    }
}
