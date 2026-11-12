//! The numeric plausibility check of a limit (item 13C, decision 13C.4): `f` is evaluated in ball arithmetic
//! at three points approaching the limit point (x = a ± 10^-3, 10^-4, 10^-6, or ±10^2, 10^3, 10^4; farther points
//! overflow the numeric special functions), with generic values for other symbols when no assumptions are in
//! force. A value whose ball is wider than 10^-9 relative is not used. Functions other than the elementary ones, gamma
//! and erf are not checked (their values far out are slow).
//!
//! The limit is rejected only on evidence against it: the three values settle (each step at most half the one
//! before) at a value farther from the claimed limit than the remaining steps can go (for an infinite limit:
//! settle at all, or all have the wrong sign). Functions that converge too slowly or too late to show it at
//! these points (`log(log(x))/log(x)`, Gruntz's nested examples, whose values still grow at 10^4) pass, since
//! their values do not settle; a wrong value from any method on a function that converges normally fails.
use super::walk::Lim;
use crate::bounds::{subst, Pt};
use mc_calculus::common::{apply, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_arith::number::{Num, Real};
use mc_eval::{NativeCx, Res};

/// The digits of the evaluations.
const DIGITS: i64 = 50;

/// Generic values of the other symbols (positive, not special).
const GENERIC: [(i64, i64); 6] = [(13, 21), (17, 23), (19, 29), (23, 31), (29, 37), (31, 41)];

/// A number as an `f64` when it is real and known to 10^-9 relative.
fn value<'h>(b: &Build<'h>, t: T<'h>) -> Option<f64> {
    match b.ar.num(t)? {
        Num::Real(Real::Ball(ball, _)) => {
            let rad = ball.rad.log2();
            let ok = if ball.mid.is_zero() { rad < -30.0 } else { rad - (ball.mid.top() as f64) < -30.0 };
            ok.then(|| Real::Ball(ball.clone(), 64).to_f64())
        }
        Num::Real(r) => Some(r.to_f64()),
        Num::Complex(..) => None,
    }
}

/// Functions whose ball values far out (10^4) are fast; the check is skipped for others (`dawson_f(10^4)` at 50
/// digits takes minutes by its series).
const CHEAP: &[&str] = &[
    "exp", "log", "sin", "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh", "coth", "sech", "csch", "asin",
    "acos", "atan", "acot", "asinh", "acosh", "atanh", "abs", "real_abs", "sign", "sqrt", "floor", "ceiling", "round",
    "gamma", "log_gamma", "factorial", "binomial", "erf", "erfc", "max", "min",
];

/// Is every call in `f` arithmetic or one of [`CHEAP`]?
fn cheap<'h>(b: &Build<'h>, f: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![f];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !seen.insert(t) {
            continue;
        }
        if let View::App { head, args } = h.view(t) {
            let ok = match b.sym_of(head) {
                Some(s) if s == b.hd.add || s == b.hd.mul || s == b.hd.pow || s == b.hd.list => true,
                Some(s) => CHEAP.contains(&h.sym_name(s).as_str()),
                None => false,
            };
            if !ok {
                return false;
            }
            stack.extend_from_slice(args);
        }
    }
    true
}

/// The symbols of `f` other than `x` that are not numeric constants.
fn parameters<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Vec<T<'h>>> {
    let h = b.heap();
    let mut out = vec![];
    let mut seen = rustc_hash::FxHashSet::default();
    let mut stack = vec![f];
    while let Some(t) = stack.pop() {
        if !seen.insert(t) {
            continue;
        }
        match h.view(t) {
            View::App { args, .. } => stack.extend_from_slice(args),
            View::Sym(_) if t != x => {
                let v = apply(cx, b, "n", &[t])?;
                if b.ar.num(v).is_none() {
                    out.push(t);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// `t` with the parameters replaced by their generic values.
fn generic<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, mut t: T<'h>, params: &[T<'h>]) -> Res<T<'h>> {
    for (i, &p) in params.iter().enumerate() {
        let (n, d) = GENERIC[i % GENERIC.len()];
        t = subst(cx, b, t, p, b.rat(n + i as i64 / GENERIC.len() as i64, d))?;
    }
    Ok(t)
}

/// Is the limit `l` of `f` as `x → pt` (from `side`) consistent with the values of `f` near the point? `false`
/// only on evidence against it (module comment).
pub(crate) fn plausible<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    pt: Pt<'h>,
    side: i8,
    l: Lim<'h>,
) -> Res<bool> {
    if !cheap(b, f) {
        return Ok(true);
    }
    let params = parameters(cx, b, f, x)?;
    if params.len() > 12 || (!params.is_empty() && !cx.assumptions().is_empty()) {
        return Ok(true);
    }
    let fp = generic(cx, b, f, &params)?;
    let digits = b.int(DIGITS);
    let ten = b.int(10);
    let points: Vec<T<'h>> = match pt {
        Pt::Fin(c) => {
            let c = generic(cx, b, c, &params)?;
            if !is_free(b, c, x) {
                return Ok(true);
            }
            [3, 4, 6].iter().map(|&k| b.add(&[c, b.mul(&[b.int(side.into()), b.powi(ten, -k)])])).collect()
        }
        Pt::PosInf => [2, 3, 4].iter().map(|&k| b.powi(ten, k)).collect(),
        Pt::NegInf => [2, 3, 4].iter().map(|&k| b.neg(b.powi(ten, k))).collect(),
    };
    let mut vals = vec![];
    for p in points {
        // the point as a ball: an exact point would make exact powers like 2^(10^8)
        let p = apply(cx, b, "n", &[p, b.int(DIGITS + 20)])?;
        let v = subst(cx, b, fp, x, p)?;
        let v = apply(cx, b, "n", &[v, digits])?;
        if let Some(v) = value(b, v) {
            if v.is_finite() {
                vals.push(v);
            }
        }
    }
    if vals.len() < 2 {
        return Ok(true);
    }
    // the values settle (each step at most half the last) at a point farther from the claimed limit than the
    // steps can still go: a limit elsewhere
    let settles_away = |target: Option<f64>| -> bool {
        if vals.len() < 3 {
            return false;
        }
        let (a, c, d) = (vals[0], vals[1], vals[2]);
        let (s1, s2) = ((c - a).abs(), (d - c).abs());
        if s2 > 0.5 * s1 {
            return false;
        }
        match target {
            Some(l) => (d - l).abs() > 4.0 * s2 + 1e-12 * (1.0 + l.abs()),
            None => true,
        }
    };
    match l {
        Lim::Fin(v, _) => {
            let lv = generic(cx, b, v, &params)?;
            let lv = apply(cx, b, "n", &[lv, digits])?;
            let Some(lv) = value(b, lv) else { return Ok(true) };
            Ok(!settles_away(Some(lv)))
        }
        Lim::Inf(s) => {
            let s = f64::from(s);
            if vals.iter().all(|&v| s * v < 0.0) && vals.len() >= 3 {
                return Ok(false);
            }
            Ok(!settles_away(None))
        }
    }
}
