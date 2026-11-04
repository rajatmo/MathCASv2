//! Elementary functions on complex balls (roadmap 07, package N0 item 4): every result encloses the function's
//! values on the whole argument region (the rectangle of the two part balls), or the function returns `None` (a
//! point where no value exists, a region touching a branch cut or a pole, a size beyond the real functions' ranges,
//! or a raised stop flag).
//!
//! As in Arb's `acb` module, each function is a ball expression of real ball functions of the parts
//! ([`crate::elementary`]), so the radii are propagated by construction:
//!
//! | function | formula |
//! |---|---|
//! | exp | e^a (cos b + i sin b) |
//! | log | log(a^2 + b^2) / 2 + i arg z |
//! | arg | atan(b / a), atan(b / a) ± pi, or ±pi/2 - atan(a / b), by the side of the axes (pi on the negative axis) |
//! | abs | sqrt(a^2 + b^2) (an enclosure [0, |a| + |b|] when the region may contain 0) |
//! | sqrt | t = sqrt((abs z + a)/2), t + i b/(2t) for a >= 0; t = sqrt((abs z - a)/2), abs(b)/(2t) + i sign(b) t otherwise |
//! | sin, cos | sin a cosh b + i cos a sinh b, cos a cosh b - i sin a sinh b |
//! | sinh, cosh | sinh a cos b + i cosh a sin b, cosh a cos b + i sinh a sin b |
//! | tan | (sin 2a + i sinh 2b) / (cos 2a + cosh 2b) |
//! | tanh | -i tan(iz) |
//! | atan | (log(1 + iz) - log(1 - iz)) / (2i) |
//! | asin | -i log(iz + sqrt(1 - z^2)), reduced to Re z >= 0 >= Im z by asin(-z) = -asin(z), asin(conj z) = conj asin z |
//! | acos | pi/2 - asin z |
//! | z^n | binary powering (n an integer) |
//! | z^w | exp(w log z); z^(k/2) through sqrt; e^w = exp(w); 0^w = 0 for Re w > 0 |
//!
//! The principal branches are those of [`crate::machine::C64`] (the formulas are the same): log and sqrt cut along
//! the negative real axis with values from above (arg in (-pi, pi]); atan along the imaginary axis beyond ±i; asin
//! and acos along the real axis beyond ±1. A ball has no signed zero: an *exact* zero imaginary part is on the
//! upper side (`log(-2) = log 2 + i pi`, as the machine functions give for +0.0). A region that touches a cut with
//! an *inexact* part (an imaginary part 0 ± r with the real part on the cut) gives `None`, Arb's behaviour for
//! analytic evaluation: the function is not analytic there, and integration bounds (Petras) must see that
//! (decision N0C.2).
//!
//! A real argument (an exact zero imaginary part) inside a function's real domain goes to the real ball function,
//! so real results stay real (decision N0C.3).
use crate::consts::Const;
use crate::elementary as el;
use mc_arith::num::{stopped, Ball, Float, Int, Mag, Nat, Rat};
use std::cmp::Ordering;

/// A complex ball: the rectangle re + i im of two real balls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CBall {
    /// The real part.
    pub re: Ball,
    /// The imaginary part.
    pub im: Ball,
}

fn zero() -> Ball {
    Ball::from_i64(0)
}

fn one() -> Ball {
    Ball::from_i64(1)
}

fn is_exact_zero(b: &Ball) -> bool {
    b.mid.is_zero() && b.rad.is_zero()
}

/// b / 2, exactly.
fn half(b: &Ball) -> Ball {
    Ball { mid: Float::new(b.mid.neg, b.mid.man.clone(), b.mid.exp - 1), rad: b.rad.mul_2exp(-1) }
}

/// 2 b, exactly.
fn twice(b: &Ball) -> Ball {
    Ball { mid: Float::new(b.mid.neg, b.mid.man.clone(), b.mid.exp + 1), rad: b.rad.mul_2exp(1) }
}

/// The sign of a ball when it excludes zero: `Some(true)` for positive.
fn positive(b: &Ball) -> Option<bool> {
    (!b.contains_zero()).then_some(!b.mid.neg)
}

/// An upper bound of |x| over the ball.
fn mag_up(b: &Ball) -> Mag {
    Mag::from_float_up(&b.mid).add(&b.rad)
}

/// The top binary exponent of the larger part (0 for zero).
fn top(z: &CBall) -> i64 {
    let t = |b: &Ball| if b.mid.is_zero() { i64::MIN } else { b.mid.top() };
    let m = t(&z.re).max(t(&z.im));
    if m == i64::MIN {
        0
    } else {
        m
    }
}

/// (sinh x, cosh x) from one exponential.
fn sinh_cosh(x: &Ball, w: u64) -> Option<(Ball, Ball)> {
    if is_exact_zero(x) {
        return Some((zero(), one()));
    }
    let e = el::exp(x, w)?;
    let r = one().div(&e, w)?;
    Some((half(&e.sub(&r, w)), half(&e.add(&r, w))))
}

impl CBall {
    /// re + i im.
    pub fn new(re: Ball, im: Ball) -> CBall {
        CBall { re, im }
    }
    /// A real ball (exact zero imaginary part).
    pub fn real(re: Ball) -> CBall {
        CBall { re, im: zero() }
    }
    /// An integer.
    pub fn from_i64(v: i64) -> CBall {
        CBall::real(Ball::from_i64(v))
    }
    /// The imaginary unit, exactly.
    pub fn i() -> CBall {
        CBall { re: zero(), im: one() }
    }
    /// Is the imaginary part an exact zero?
    pub fn is_real(&self) -> bool {
        is_exact_zero(&self.im)
    }
    /// Are both parts exact zeros?
    pub fn is_exact_zero(&self) -> bool {
        is_exact_zero(&self.re) && is_exact_zero(&self.im)
    }
    /// May the region contain 0?
    pub fn contains_zero(&self) -> bool {
        self.re.contains_zero() && self.im.contains_zero()
    }
    /// -z, exactly.
    pub fn neg(&self) -> CBall {
        CBall { re: self.re.neg(), im: self.im.neg() }
    }
    /// The conjugate, exactly.
    pub fn conj(&self) -> CBall {
        CBall { re: self.re.clone(), im: self.im.neg() }
    }
    /// i z, exactly.
    pub fn mul_i(&self) -> CBall {
        CBall { re: self.im.neg(), im: self.re.clone() }
    }
    /// -i z, exactly.
    pub fn div_i(&self) -> CBall {
        CBall { re: self.im.clone(), im: self.re.neg() }
    }
    /// z + o.
    pub fn add(&self, o: &CBall, p: u64) -> CBall {
        CBall { re: self.re.add(&o.re, p), im: add_part(&self.im, &o.im, p) }
    }
    /// z - o.
    pub fn sub(&self, o: &CBall, p: u64) -> CBall {
        self.add(&o.neg(), p)
    }
    /// z x for a real ball x.
    pub fn mul_real(&self, x: &Ball, p: u64) -> CBall {
        CBall { re: self.re.mul(x, p), im: mul_part(&self.im, x, p) }
    }
    /// z o.
    pub fn mul(&self, o: &CBall, p: u64) -> CBall {
        if o.is_real() {
            return self.mul_real(&o.re, p);
        }
        if self.is_real() {
            return o.mul_real(&self.re, p);
        }
        let (a, b, c, d) = (&self.re, &self.im, &o.re, &o.im);
        CBall { re: a.mul(c, p).sub(&b.mul(d, p), p), im: a.mul(d, p).add(&b.mul(c, p), p) }
    }
    /// z^2 (one product fewer, and a^2 - b^2 as (a + b)(a - b)).
    pub fn square(&self, p: u64) -> CBall {
        if self.is_real() {
            return CBall::real(self.re.mul(&self.re, p));
        }
        let (a, b) = (&self.re, &self.im);
        CBall { re: a.add(b, p).mul(&a.sub(b, p), p), im: twice(&a.mul(b, p)) }
    }
    /// z / o: `None` when the divisor may be zero.
    pub fn div(&self, o: &CBall, p: u64) -> Option<CBall> {
        if o.is_real() {
            return Some(CBall { re: self.re.div(&o.re, p)?, im: div_part(&self.im, &o.re, p)? });
        }
        let w = p + 8;
        let (a, b, c, d) = (&self.re, &self.im, &o.re, &o.im);
        let n = c.mul(c, w).add(&d.mul(d, w), w);
        let re = a.mul(c, w).add(&b.mul(d, w), w).div(&n, p)?;
        let im = b.mul(c, w).sub(&a.mul(d, w), w).div(&n, p)?;
        Some(CBall { re, im })
    }
    /// 1 / z.
    pub fn recip(&self, p: u64) -> Option<CBall> {
        CBall::from_i64(1).div(self, p)
    }
    /// Both midpoints rounded to p bits (the rounding errors join the radii).
    pub fn round(self, p: u64) -> CBall {
        CBall { re: el::round(self.re, p), im: el::round(self.im, p) }
    }
    /// At least d correct significant digits relative to |z|: each part has d digits itself, or its radius is
    /// below 10^-d times a lower bound of the other part, which has d digits (a tiny part next to a large one, like
    /// the imaginary part of exp(1 + pi i) computed from inexact pi).
    pub fn has_digits(&self, d: u32) -> bool {
        let ok = |x: &Ball, y: &Ball| {
            if x.has_digits(d) {
                return true;
            }
            if !y.has_digits(d) {
                return false;
            }
            let lo = y.mid.abs().sub_exact(&y.rad.to_float());
            let lhs = x.rad.to_float().mul_exact(&Float::from_nat(Nat::pow(10, d as u64)));
            lhs.cmp_exact(&lo) != Ordering::Greater
        };
        ok(&self.re, &self.im) && ok(&self.im, &self.re)
    }
}

/// x + y keeping an exact zero exact.
fn add_part(x: &Ball, y: &Ball, p: u64) -> Ball {
    if is_exact_zero(x) {
        return y.clone();
    }
    if is_exact_zero(y) {
        return x.clone();
    }
    x.add(y, p)
}

/// x y keeping an exact zero exact.
fn mul_part(x: &Ball, y: &Ball, p: u64) -> Ball {
    if is_exact_zero(x) || is_exact_zero(y) {
        return zero();
    }
    x.mul(y, p)
}

/// x / y keeping an exact zero exact.
fn div_part(x: &Ball, y: &Ball, p: u64) -> Option<Ball> {
    if is_exact_zero(x) {
        return (!y.contains_zero()).then(zero);
    }
    x.div(y, p)
}

// ---------------------------------------------------------------- abs, arg

/// a^2 + b^2.
fn norm(z: &CBall, w: u64) -> Ball {
    let sq = |x: &Ball| if is_exact_zero(x) { zero() } else { x.mul(x, w) };
    sq(&z.re).add(&sq(&z.im), w)
}

/// |z|: sqrt(a^2 + b^2); a ball [0 ± (|a| + |b|)] when the region may contain 0 (the bound still encloses).
pub fn abs(z: &CBall, p: u64) -> Option<Ball> {
    if z.is_real() {
        return Some(Ball { mid: z.re.mid.abs(), rad: z.re.rad });
    }
    if z.re.mid.is_zero() && z.re.rad.is_zero() {
        return Some(Ball { mid: z.im.mid.abs(), rad: z.im.rad });
    }
    if stopped() {
        return None;
    }
    let w = p + 8;
    match el::sqrt(&norm(z, w), w) {
        Some(r) => Some(el::round(r, p)),
        None if !stopped() => Some(Ball { mid: Float::zero(), rad: mag_up(&z.re).add(&mag_up(&z.im)) }),
        None => None,
    }
}

/// |z| for a region that excludes 0 (`None` otherwise).
fn abs_positive(z: &CBall, w: u64) -> Option<Ball> {
    if z.is_real() {
        return (!z.re.contains_zero()).then(|| Ball { mid: z.re.mid.abs(), rad: z.re.rad });
    }
    el::sqrt(&norm(z, w), w)
}

/// The principal argument in (-pi, pi]: `None` when the region touches the negative real axis with an inexact
/// imaginary part, or may contain 0. An exact zero imaginary part gives exact 0 (positive real part) or pi
/// (negative).
pub fn arg(z: &CBall, p: u64) -> Option<Ball> {
    let w = p + 16;
    let (a, b) = (&z.re, &z.im);
    if is_exact_zero(b) {
        return match positive(a)? {
            true => Some(zero()),
            false => Const::Pi.ball(p),
        };
    }
    let pi = Const::Pi.ball(w)?;
    let r = match (positive(a), positive(b)) {
        // right half plane: atan(b/a)
        (Some(true), _) => el::atan(&b.div(a, w)?, w)?,
        // left half plane, above or below the cut: atan(b/a) +- pi
        (Some(false), Some(up)) => {
            let t = el::atan(&b.div(a, w)?, w)?;
            if up {
                t.add(&pi, w)
            } else {
                t.sub(&pi, w)
            }
        }
        // near the imaginary axis: +-pi/2 - atan(a/b)
        (_, Some(up)) => {
            let hp = half(&pi);
            let hp = if up { hp } else { hp.neg() };
            if is_exact_zero(a) {
                hp
            } else {
                hp.sub(&el::atan(&a.div(b, w)?, w)?, w)
            }
        }
        _ => return None,
    };
    Some(el::round(r, p))
}

// ---------------------------------------------------------------- exp, log, sqrt

/// e^z = e^a (cos b + i sin b).
pub fn exp(z: &CBall, p: u64) -> Option<CBall> {
    if z.is_real() {
        return el::exp(&z.re, p).map(CBall::real);
    }
    let w = p + 8;
    let (s, c) = el::sin_cos(&z.im, w)?;
    if is_exact_zero(&z.re) {
        return Some(CBall { re: c, im: s }.round(p));
    }
    let e = el::exp(&z.re, w)?;
    Some(CBall { re: e.mul(&c, w), im: e.mul(&s, w) }.round(p))
}

/// The principal logarithm log|z| + i arg z.
pub fn log(z: &CBall, p: u64) -> Option<CBall> {
    let w = p + 16;
    if z.is_real() {
        return match positive(&z.re)? {
            true => el::log(&z.re, p).map(CBall::real),
            false => Some(CBall { re: el::log(&z.re.neg(), p)?, im: Const::Pi.ball(p)? }),
        };
    }
    let th = arg(z, w)?;
    // log|z| = log(a^2 + b^2) / 2, or log|b| on the imaginary axis
    let lr = if is_exact_zero(&z.re) {
        el::log(&Ball { mid: z.im.mid.abs(), rad: z.im.rad }, w)?
    } else {
        half(&el::log(&norm(z, w), w)?)
    };
    Some(CBall { re: lr, im: th }.round(p))
}

/// The principal square root (real part >= 0; the negative real axis from above).
pub fn sqrt(z: &CBall, p: u64) -> Option<CBall> {
    if z.is_real() {
        if is_exact_zero(&z.re) {
            return Some(z.clone());
        }
        return match positive(&z.re)? {
            true => el::sqrt(&z.re, p).map(CBall::real),
            false => Some(CBall { re: zero(), im: el::sqrt(&z.re.neg(), p)? }),
        };
    }
    let w = p + 16;
    let (a, b) = (&z.re, &z.im);
    let r = abs_positive(z, w)?;
    let c = if !a.mid.neg {
        // t = sqrt((|z| + a) / 2) > 0 off the cut; the imaginary part b / (2t)
        let t = el::sqrt(&half(&r.add(a, w)), w)?;
        let im = b.div(&twice(&t), w)?;
        CBall { re: t, im }
    } else {
        // no cancellation for a < 0: t = sqrt((|z| - a) / 2) = |Im sqrt z|
        let up = positive(b)?;
        let t = el::sqrt(&half(&r.sub(a, w)), w)?;
        let bb = if up { b.clone() } else { b.neg() };
        let re = bb.div(&twice(&t), w)?;
        CBall { re, im: if up { t } else { t.neg() } }
    };
    (!stopped()).then(|| c.round(p))
}

// ---------------------------------------------------------------- trigonometric and hyperbolic

/// (sin z, cos z).
pub fn sin_cos(z: &CBall, p: u64) -> Option<(CBall, CBall)> {
    if z.is_real() {
        let (s, c) = el::sin_cos(&z.re, p)?;
        return Some((CBall::real(s), CBall::real(c)));
    }
    let w = p + 8;
    let (s, c) = el::sin_cos(&z.re, w)?;
    let (sh, ch) = sinh_cosh(&z.im, w)?;
    let sin = CBall { re: mul_part(&s, &ch, w), im: mul_part(&c, &sh, w) };
    let cos = CBall { re: mul_part(&c, &ch, w), im: mul_part(&s, &sh, w).neg() };
    Some((sin.round(p), cos.round(p)))
}

/// sin z = sin a cosh b + i cos a sinh b.
pub fn sin(z: &CBall, p: u64) -> Option<CBall> {
    sin_cos(z, p).map(|(s, _)| s)
}

/// cos z = cos a cosh b - i sin a sinh b.
pub fn cos(z: &CBall, p: u64) -> Option<CBall> {
    sin_cos(z, p).map(|(_, c)| c)
}

/// tan z = (sin 2a + i sinh 2b) / (cos 2a + cosh 2b): `None` at a region containing a pole.
pub fn tan(z: &CBall, p: u64) -> Option<CBall> {
    if z.is_real() {
        return el::tan(&z.re, p).map(CBall::real);
    }
    let w = p + 16;
    let (s2, c2) = el::sin_cos(&twice(&z.re), w)?;
    let (sh2, ch2) = sinh_cosh(&twice(&z.im), w)?;
    let d = c2.add(&ch2, w);
    let re = div_part(&s2, &d, w)?;
    let im = sh2.div(&d, w)?;
    Some(CBall { re, im }.round(p))
}

/// (sinh z, cosh z): sinh a cos b + i cosh a sin b, cosh a cos b + i sinh a sin b.
pub fn sinh_cosh_c(z: &CBall, p: u64) -> Option<(CBall, CBall)> {
    if z.is_real() {
        let w = p + 8;
        let (s, c) = sinh_cosh(&z.re, w)?;
        return Some((CBall::real(el::round(s, p)), CBall::real(el::round(c, p))));
    }
    let w = p + 8;
    let (sh, ch) = sinh_cosh(&z.re, w)?;
    let (s, c) = el::sin_cos(&z.im, w)?;
    let sinh = CBall { re: mul_part(&sh, &c, w), im: mul_part(&ch, &s, w) };
    let cosh = CBall { re: mul_part(&ch, &c, w), im: mul_part(&sh, &s, w) };
    Some((sinh.round(p), cosh.round(p)))
}

/// sinh z.
pub fn sinh(z: &CBall, p: u64) -> Option<CBall> {
    if z.is_real() {
        return el::sinh(&z.re, p).map(CBall::real);
    }
    sinh_cosh_c(z, p).map(|(s, _)| s)
}

/// cosh z.
pub fn cosh(z: &CBall, p: u64) -> Option<CBall> {
    if z.is_real() {
        return el::cosh(&z.re, p).map(CBall::real);
    }
    sinh_cosh_c(z, p).map(|(_, c)| c)
}

/// tanh z = -i tan(iz).
pub fn tanh(z: &CBall, p: u64) -> Option<CBall> {
    if z.is_real() {
        return el::tanh(&z.re, p).map(CBall::real);
    }
    tan(&z.mul_i(), p).map(|t| t.div_i())
}

// ---------------------------------------------------------------- inverse functions

/// Extra bits for a small argument (the logarithm formulas cancel to about |z|).
fn small_guard(z: &CBall, p: u64) -> u64 {
    (-top(z)).clamp(0, 4 * p as i64 + 64) as u64
}

/// atan z = (log(1 + iz) - log(1 - iz)) / (2i): `None` at a region containing ±i or touching the cuts
/// (the imaginary axis beyond ±i) with an inexact real part.
pub fn atan(z: &CBall, p: u64) -> Option<CBall> {
    if z.is_real() {
        return el::atan(&z.re, p).map(CBall::real);
    }
    let w = p + 16 + small_guard(z, p);
    let iz = z.mul_i();
    let one = CBall::from_i64(1);
    let d = log(&one.add(&iz, w), w)?.sub(&log(&one.sub(&iz, w), w)?, w);
    let r = CBall { re: half(&d.im), im: half(&d.re).neg() };
    (!stopped()).then(|| r.round(p))
}

/// asin z = -i log(iz + sqrt(1 - z^2)), computed for Re z >= 0 >= Im z where the sum does not cancel.
pub fn asin(z: &CBall, p: u64) -> Option<CBall> {
    if z.is_real() {
        if let Some(r) = el::asin(&z.re, p) {
            return Some(CBall::real(r));
        }
        if stopped() {
            return None;
        }
    }
    // asin(-z) = -asin(z) everywhere (also on the cuts, with the branches above); asin(conj z) = conj asin(z)
    // off the cuts (and a region touching a cut fails below either way)
    if z.re.mid.neg {
        return asin(&z.neg(), p).map(|r| r.neg());
    }
    if !z.im.mid.neg && !z.im.mid.is_zero() {
        return asin(&z.conj(), p).map(|r| r.conj());
    }
    let w = p + 16 + small_guard(z, p);
    let s = sqrt(&CBall::from_i64(1).sub(&z.square(w), w), w)?;
    let l = log(&z.mul_i().add(&s, w), w)?;
    (!stopped()).then(|| l.div_i().round(p))
}

/// acos z = pi/2 - asin z.
pub fn acos(z: &CBall, p: u64) -> Option<CBall> {
    if z.is_real() {
        if let Some(r) = el::acos(&z.re, p) {
            return Some(CBall::real(r));
        }
        if stopped() {
            return None;
        }
    }
    let w = p + 16;
    let a = asin(z, w)?;
    let hp = half(&Const::Pi.ball(w)?);
    Some(CBall { re: hp.sub(&a.re, w), im: a.im.neg() }.round(p))
}

// ---------------------------------------------------------------- powers

/// z^n for an integer n by binary powering (a negative n needs a region excluding 0).
pub fn pow_int(z: &CBall, n: &Int, p: u64) -> Option<CBall> {
    if z.is_real() {
        return el::pow_int(&z.re, n, p).map(CBall::real);
    }
    let e = n.to_i64()?;
    let m = e.unsigned_abs();
    if m == 0 {
        return Some(CBall::from_i64(1));
    }
    // the result's exponent must stay far inside i64
    let t = top(z).unsigned_abs() + 2;
    if t.checked_mul(m).is_none_or(|v| v > 1 << 60) {
        return None;
    }
    let w = p + 64 - m.leading_zeros() as u64 + 8;
    let mut r = CBall::from_i64(1);
    let mut base = z.clone();
    let mut k = m;
    while k > 0 {
        if stopped() {
            return None;
        }
        if k & 1 == 1 {
            r = r.mul(&base, w);
        }
        k >>= 1;
        if k > 0 {
            base = base.square(w);
        }
    }
    let r = if e < 0 { r.recip(w)? } else { r };
    (!stopped()).then(|| r.round(p))
}

/// The ball of a rational at w bits.
fn rat_ball(q: &Rat, w: u64) -> Option<Ball> {
    let num = Ball::exact(Float::new(q.num.neg, q.num.mag.clone(), 0));
    let num = el::round(num, w);
    num.div(&Ball::from_nat(q.den.clone()), w)
}

/// z^q for an exact rational q: integer powers by [`pow_int`], half-integers through [`sqrt`], others as
/// exp(q log z) (principal; a negative real base gives |z|^q (cos q pi + i sin q pi)).
pub fn pow_rat(z: &CBall, q: &Rat, p: u64) -> Option<CBall> {
    if q.is_int() {
        return pow_int(z, &q.num, p);
    }
    if q.den.limbs == [2] {
        return pow_int(&sqrt(z, p + 16)?, &q.num, p);
    }
    let w = p + 16 + (64 - q.num.mag.bits().min(63).leading_zeros() as u64);
    pow(z, &CBall::real(rat_ball(q, w)?), p)
}

/// The principal power z^w = exp(w log z); 0^w = 0 for Re w > 0. An exponent that is an exact integer goes to
/// [`pow_int`].
pub fn pow(z: &CBall, e: &CBall, p: u64) -> Option<CBall> {
    if e.is_real() && e.re.rad.is_zero() && e.re.mid.exp >= 0 {
        let n = Int::new(e.re.mid.neg, e.re.mid.man.shl(e.re.mid.exp as u64));
        return pow_int(z, &n, p);
    }
    if z.is_exact_zero() {
        return (positive(&e.re)? && !e.re.contains_zero()).then(|| CBall::from_i64(0));
    }
    if z.is_real() && e.is_real() && positive(&z.re) == Some(true) {
        return el::pow(&z.re, &e.re, p).map(CBall::real);
    }
    // exp(x) has the relative error of x's absolute error: |w log z| bits more
    let lz = log(z, 64)?;
    let size = top(&lz.mul(e, 64)).max(0) as u64;
    let w = p + 16 + size;
    let l = log(z, w)?;
    exp(&l.mul(e, w), p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::C64;
    use mc_arith::num::float::{dyadic_to_f64, f64_to_dyadic};

    fn b(v: f64) -> Ball {
        let (neg, man, exp) = f64_to_dyadic(v).unwrap();
        Ball::exact(Float::new(neg, man, exp))
    }
    fn c(re: f64, im: f64) -> CBall {
        CBall::new(b(re), b(im))
    }
    fn f(x: &Ball) -> f64 {
        dyadic_to_f64(x.mid.neg, &x.mid.man, x.mid.exp)
    }
    fn near(z: &CBall, w: C64) -> bool {
        let tol = 1e-13 * w.re.abs().max(w.im.abs()).max(1.0);
        (f(&z.re) - w.re).abs() <= tol && (f(&z.im) - w.im).abs() <= tol
    }

    #[test]
    fn principal_values_agree_with_machine() {
        let pts = [
            (1.0, 2.0),
            (-2.0, 1.0),
            (-2.0, -1.0),
            (0.3, -0.7),
            (-0.5, 0.0),
            (2.0, 0.0),
            (-3.0, 0.0),
            (0.0, 2.0),
            (0.0, -3.0),
        ];
        for (re, im) in pts {
            let (z, m) = (c(re, im), C64::new(re, im));
            let p = 128;
            let checks: [(&str, Option<CBall>, C64); 11] = [
                ("exp", exp(&z, p), m.exp()),
                ("sin", sin(&z, p), m.sin()),
                ("cos", cos(&z, p), m.cos()),
                ("tan", tan(&z, p), m.tan()),
                ("sinh", sinh(&z, p), m.sinh()),
                ("cosh", cosh(&z, p), m.cosh()),
                ("tanh", tanh(&z, p), m.tanh()),
                ("sqrt", sqrt(&z, p), m.sqrt()),
                ("asin", asin(&z, p), m.asin()),
                ("acos", acos(&z, p), m.acos()),
                ("log", if re == 0.0 && im == 0.0 { None } else { log(&z, p) }, m.log()),
            ];
            for (name, r, want) in checks {
                let r = r.unwrap_or_else(|| panic!("{name}({re}, {im})"));
                assert!(near(&r, want), "{name}({re}, {im}) = {} {} vs {want:?}", f(&r.re), f(&r.im));
            }
            if (re, im) != (0.0, 2.0) && (re, im) != (0.0, -3.0) {
                let r = atan(&z, p).unwrap();
                assert!(near(&r, m.atan()), "atan({re}, {im})");
            }
        }
    }

    #[test]
    fn cuts_and_points() {
        let p = 100;
        let wide = Ball { mid: Float::zero(), rad: Mag::pow2(-30) };
        // a region across the negative axis has no principal log, sqrt or arg
        let z = CBall::new(b(-2.0), wide.clone());
        assert!(log(&z, p).is_none() && sqrt(&z, p).is_none() && arg(&z, p).is_none());
        // on the positive axis they exist
        let z = CBall::new(b(2.0), wide.clone());
        assert!(log(&z, p).is_some() && sqrt(&z, p).is_some() && arg(&z, p).is_some());
        // atan at +-i, log at 0, tan at a pole
        assert!(atan(&c(0.0, 1.0), p).is_none() && log(&CBall::from_i64(0), p).is_none());
        assert!(asin(&CBall::new(b(2.0), wide.clone()), p).is_none());
        // the exact negative axis: from above
        let l = log(&c(-1.0, 0.0), p).unwrap();
        assert!(is_exact_zero(&l.re) && l.im.cmp3(&Const::Pi.ball(p).unwrap()).is_none());
        let s = sqrt(&c(-4.0, 0.0), p).unwrap();
        assert!(is_exact_zero(&s.re) && s.im.cmp3(&Ball::from_i64(2)).is_none());
        // abs near 0 still encloses
        let a = abs(&CBall::new(wide.clone(), wide), p).unwrap();
        assert!(a.contains_zero());
        // 0^w
        assert!(pow(&CBall::from_i64(0), &c(0.5, 1.0), p).unwrap().is_exact_zero());
        assert!(pow(&CBall::from_i64(0), &c(-0.5, 1.0), p).is_none());
    }
}
