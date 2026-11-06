//! The formula-defined functions of one real argument as library functions of numerical evaluation (gap queue G42):
//! `n`, comparisons and the ODE and root solvers evaluate `sech(y)`, `atanh(y)`, `heaviside_theta(y)` inside their
//! numeric islands, with machine values and ball enclosures composed from the elementary functions.
use crate::cball::CBall;
use crate::elementary as el;
use crate::fun::Mach;
use crate::library::{self, LibraryFn};
use mc_arith::num::Ball;
use mc_arith::{Kind, Real};

type Machine = fn(f64) -> f64;
type BallFn = fn(&Ball, u64) -> Option<Ball>;

/// A function of one real argument with a machine value and a ball enclosure.
pub struct OneArg {
    name: &'static str,
    machine: Machine,
    ball: BallFn,
}

/// Principal values outside the real domain from the logarithm formulas on complex balls.
fn complex_value(name: &str, x: &Ball, p: u64) -> Option<CBall> {
    use crate::cball as cb;
    let w = p + 16;
    let one = CBall::from_i64(1);
    let z = CBall::real(x.clone());
    let acosh = |z: &CBall| -> Option<CBall> {
        let s = cb::sqrt(&z.add(&one, w), w)?.mul(&cb::sqrt(&z.sub(&one, w), w)?, w);
        cb::log(&z.add(&s, w), w)
    };
    let atanh = |z: &CBall| -> Option<CBall> {
        let d = cb::log(&one.add(z, w), w)?.sub(&cb::log(&one.sub(z, w), w)?, w);
        Some(d.mul_real(&Ball::from_ratio(1, 2, w), w))
    };
    let inv = || one.div(&z, w);
    let v = match name {
        "acosh" => acosh(&z)?,
        "atanh" => atanh(&z)?,
        "asech" => acosh(&inv()?)?,
        "acoth" => atanh(&inv()?)?,
        "asec" => cb::acos(&inv()?, w)?,
        "acsc" => cb::asin(&inv()?, w)?,
        "log10" | "log2" => {
            let b = if name == "log10" { 10 } else { 2 };
            cb::log(&z, w)?.mul_real(&Ball::from_i64(1).div(&el::log(&Ball::from_i64(b), w)?, w)?, w)
        }
        _ => return None,
    };
    Some(v.round(p))
}

fn one() -> Ball {
    Ball::from_i64(1)
}
fn recip(b: Option<Ball>, p: u64) -> Option<Ball> {
    let b = b?;
    if b.contains_zero() {
        return None;
    }
    one().div(&b, p)
}
fn b_sec(x: &Ball, p: u64) -> Option<Ball> {
    recip(el::cos(x, p + 8), p)
}
fn b_csc(x: &Ball, p: u64) -> Option<Ball> {
    recip(el::sin(x, p + 8), p)
}
fn b_cot(x: &Ball, p: u64) -> Option<Ball> {
    recip(el::tan(x, p + 8), p)
}
fn b_sech(x: &Ball, p: u64) -> Option<Ball> {
    recip(el::cosh(x, p + 8), p)
}
fn b_csch(x: &Ball, p: u64) -> Option<Ball> {
    recip(el::sinh(x, p + 8), p)
}
fn b_coth(x: &Ball, p: u64) -> Option<Ball> {
    recip(el::tanh(x, p + 8), p)
}
fn b_asinh(x: &Ball, p: u64) -> Option<Ball> {
    // sign(x) log(|x| + sqrt(x^2 + 1))
    let w = p + 16;
    let neg = x.mid.neg;
    let a = if neg { x.neg() } else { x.clone() };
    let s = el::sqrt(&a.mul(&a, w).add(&one(), w), w)?;
    let v = el::log(&a.add(&s, w), w)?;
    Some(if neg { v.neg() } else { v })
}
fn b_acosh(x: &Ball, p: u64) -> Option<Ball> {
    let w = p + 16;
    let s = el::sqrt(&x.mul(x, w).sub(&one(), w), w)?;
    el::log(&x.add(&s, w), w)
}
fn b_atanh(x: &Ball, p: u64) -> Option<Ball> {
    let w = p + 16;
    let a = el::log(&one().add(x, w), w)?;
    let b = el::log(&one().sub(x, w), w)?;
    let d = a.sub(&b, w);
    d.div(&Ball::from_i64(2), w)
}
fn b_acot(x: &Ball, p: u64) -> Option<Ball> {
    el::atan(&recip(Some(x.clone()), p + 8)?, p)
}
fn b_asec(x: &Ball, p: u64) -> Option<Ball> {
    el::acos(&recip(Some(x.clone()), p + 8)?, p)
}
fn b_acsc(x: &Ball, p: u64) -> Option<Ball> {
    el::asin(&recip(Some(x.clone()), p + 8)?, p)
}
fn b_acoth(x: &Ball, p: u64) -> Option<Ball> {
    b_atanh(&recip(Some(x.clone()), p + 8)?, p)
}
fn b_asech(x: &Ball, p: u64) -> Option<Ball> {
    b_acosh(&recip(Some(x.clone()), p + 8)?, p)
}
fn b_acsch(x: &Ball, p: u64) -> Option<Ball> {
    b_asinh(&recip(Some(x.clone()), p + 8)?, p)
}
fn b_log10(x: &Ball, p: u64) -> Option<Ball> {
    el::log(x, p + 8)?.div(&el::log(&Ball::from_i64(10), p + 8)?, p)
}
fn b_log2(x: &Ball, p: u64) -> Option<Ball> {
    el::log(x, p + 8)?.div(&el::log(&Ball::from_i64(2), p + 8)?, p)
}
fn b_sinc(x: &Ball, p: u64) -> Option<Ball> {
    if x.contains_zero() {
        return None;
    }
    el::sin(x, p + 8)?.div(x, p)
}
fn b_gd(x: &Ball, p: u64) -> Option<Ball> {
    let h = x.div(&Ball::from_i64(2), p + 8)?;
    Some(el::atan(&el::tanh(&h, p + 8)?, p + 8)?.mul(&Ball::from_i64(2), p))
}
fn b_igd(x: &Ball, p: u64) -> Option<Ball> {
    let h = x.div(&Ball::from_i64(2), p + 8)?;
    Some(b_atanh(&el::tan(&h, p + 8)?, p + 8)?.mul(&Ball::from_i64(2), p))
}
/// A step-like function on a ball that does not contain its jump.
fn step(x: &Ball, at_pos: i64, at_neg: i64) -> Option<Ball> {
    if x.contains_zero() {
        return None;
    }
    Some(Ball::from_i64(if x.mid.neg { at_neg } else { at_pos }))
}
fn b_heaviside(x: &Ball, _p: u64) -> Option<Ball> {
    step(x, 1, 0)
}
fn b_real_sign(x: &Ball, _p: u64) -> Option<Ball> {
    step(x, 1, -1)
}
fn b_ramp(x: &Ball, _p: u64) -> Option<Ball> {
    if x.contains_zero() {
        return None;
    }
    Some(if x.mid.neg { Ball::from_i64(0) } else { x.clone() })
}
fn b_real_abs(x: &Ball, _p: u64) -> Option<Ball> {
    Some(Ball { mid: x.mid.abs(), rad: x.rad })
}

fn m_heaviside(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        0.0
    } else {
        f64::NAN
    }
}
fn m_real_sign(x: f64) -> f64 {
    if x == 0.0 {
        0.0
    } else {
        x.signum()
    }
}
fn m_sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        x.sin() / x
    }
}

/// The library functions of this module.
pub static ONE_ARG: &[OneArg] = &[
    OneArg { name: "sec", machine: |x| 1.0 / x.cos(), ball: b_sec },
    OneArg { name: "csc", machine: |x| 1.0 / x.sin(), ball: b_csc },
    OneArg { name: "cot", machine: |x| 1.0 / x.tan(), ball: b_cot },
    OneArg { name: "sech", machine: |x| 1.0 / x.cosh(), ball: b_sech },
    OneArg { name: "csch", machine: |x| 1.0 / x.sinh(), ball: b_csch },
    OneArg { name: "coth", machine: |x| 1.0 / x.tanh(), ball: b_coth },
    OneArg { name: "asinh", machine: f64::asinh, ball: b_asinh },
    OneArg { name: "acosh", machine: f64::acosh, ball: b_acosh },
    OneArg { name: "atanh", machine: f64::atanh, ball: b_atanh },
    OneArg { name: "acot", machine: |x| (1.0 / x).atan(), ball: b_acot },
    OneArg { name: "asec", machine: |x| (1.0 / x).acos(), ball: b_asec },
    OneArg { name: "acsc", machine: |x| (1.0 / x).asin(), ball: b_acsc },
    OneArg { name: "acoth", machine: |x| (1.0 / x).atanh(), ball: b_acoth },
    OneArg { name: "asech", machine: |x| (1.0 / x).acosh(), ball: b_asech },
    OneArg { name: "acsch", machine: |x| (1.0 / x).asinh(), ball: b_acsch },
    OneArg { name: "log10", machine: f64::log10, ball: b_log10 },
    OneArg { name: "log2", machine: f64::log2, ball: b_log2 },
    OneArg { name: "sinc", machine: m_sinc, ball: b_sinc },
    OneArg { name: "gudermannian", machine: |x| 2.0 * (x / 2.0).tanh().atan(), ball: b_gd },
    OneArg { name: "inverse_gudermannian", machine: |x| 2.0 * (x / 2.0).tan().atanh(), ball: b_igd },
    OneArg { name: "heaviside_theta", machine: m_heaviside, ball: b_heaviside },
    OneArg { name: "real_sign", machine: m_real_sign, ball: b_real_sign },
    OneArg { name: "ramp", machine: |x| x.max(0.0), ball: b_ramp },
    OneArg { name: "real_abs", machine: f64::abs, ball: b_real_abs },
];

impl LibraryFn for OneArg {
    fn name(&self) -> &'static str {
        self.name
    }
    fn arity(&self) -> usize {
        1
    }
    fn ball(&self, args: &[Real], p: u64) -> Option<CBall> {
        let b = match args.first()?.to_kind(Kind::Ball, p) {
            Real::Ball(b, _) => b,
            _ => return None,
        };
        (self.ball)(&b, p).map(CBall::real).or_else(|| complex_value(self.name, &b, p))
    }
    fn machine(&self, args: &[f64]) -> Mach {
        Mach::Real((self.machine)(args[0]))
    }
}

/// Registers the functions with numerical evaluation.
pub fn register() {
    for f in ONE_ARG {
        library::register(f);
    }
}
