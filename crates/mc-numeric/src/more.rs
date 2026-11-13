//! More elementary and piecewise functions, defined by formulas ([`crate::formula`]): the
//! reciprocal circular and hyperbolic functions, the inverse hyperbolic functions and the inverse reciprocal
//! functions, `sinc`, `log10`, `log2`, the Gudermannian, integer and fractional parts, the unit box, triangle and
//! ramp, `heaviside_theta`, `dirac_delta`, real absolute value and sign, real roots (`surd`), and the part pairs
//! `re_im` and `abs_arg`.
//!
//! Conventions (decision P6N.5): principal branches as the logarithm formulas give them (asinh(z) = log(z +
//! sqrt(z^2 + 1)), acosh(z) = log(z + sqrt(z + 1) sqrt(z - 1)), atanh(z) = (log(1 + z) - log(1 - z))/2; acot(z) =
//! atan(1/z), asec(z) = acos(1/z), ...); the reciprocal functions keep their own name for symbolic arguments
//! (`sec(x)`) and become the reciprocal of the base function's value where that has one (`sec(pi/3)` is 2).
use crate::formula::{Def, Fx};
use mc_api::T;
use mc_arith::{Num, Real};
use mc_eval::Res;

type R<'h> = Res<Option<T<'h>>>;

/// f(x) = 1/base(x): `Some` for a value of base(x) without base calls, the parity forms `f(-x)`, or `None`.
fn reciprocal<'h>(fx: &mut Fx<'_, '_, '_, 'h>, base: &str, me: &str, x: T<'h>) -> R<'h> {
    if fx.infinity(x).is_none() && fx.num(x).is_none() {
        // symbolic: only parity (cos(-x) = cos(x) makes sec(-x) = sec(x), sin(-x) = -sin(x) makes csc(-x) = -csc(x))
        let Some(y) = fx.negated(x) else {
            let g = fx.eval(fx.call(base, &[x]))?;
            return Ok((!fx.contains(g, base) && !fx.is_int(g, 0)).then(|| fx.pow(g, fx.int(-1))));
        };
        let g = fx.eval(fx.call(base, &[x]))?;
        let by = fx.call(base, &[y]);
        if g == by {
            return Ok(Some(fx.call(me, &[y])));
        }
        if g == fx.neg(by) {
            return Ok(Some(fx.neg(fx.call(me, &[y]))));
        }
        return Ok(None);
    }
    let g = fx.eval(fx.call(base, &[x]))?;
    if fx.contains(g, base) || fx.is_int(g, 0) {
        return Ok(None);
    }
    Ok(Some(fx.pow(g, fx.int(-1))))
}

fn reciprocal_numeric<'h>(fx: &mut Fx<'_, '_, '_, 'h>, base: &str, x: T<'h>) -> R<'h> {
    Ok(Some(fx.pow(fx.call(base, &[x]), fx.int(-1))))
}

macro_rules! recip {
    ($exact:ident, $numeric:ident, $base:literal, $me:literal) => {
        fn $exact<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
            reciprocal(fx, $base, $me, a[0])
        }
        fn $numeric<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
            reciprocal_numeric(fx, $base, a[0])
        }
    };
}

recip!(sec_e, sec_n, "cos", "sec");
recip!(csc_e, csc_n, "sin", "csc");
recip!(cot_e, cot_n, "tan", "cot");
recip!(sech_e, sech_n, "cosh", "sech");
recip!(csch_e, csch_n, "sinh", "csch");
recip!(coth_e, coth_n, "tanh", "coth");

/// A value of `inner` without calls of `inner` (an exact identity), else `None`.
fn closed<'h>(fx: &mut Fx<'_, '_, '_, 'h>, inner: &str, arg: T<'h>) -> R<'h> {
    let v = fx.eval(fx.call(inner, &[arg]))?;
    Ok((!fx.contains(v, inner)).then_some(v))
}

/// i pi q
fn i_pi<'h>(fx: &Fx<'_, '_, '_, 'h>, n: i64, d: i64) -> T<'h> {
    fx.mul(&[fx.i(), fx.rat(n, d), fx.sym("pi")])
}

/// For an odd function: f(-x) = -f(x).
fn odd<'h>(fx: &Fx<'_, '_, '_, 'h>, me: &str, x: T<'h>) -> Option<T<'h>> {
    fx.negated(x).map(|y| fx.neg(fx.call(me, &[y])))
}

/// b for x = b i (b exact real).
fn imaginary<'h>(fx: &Fx<'_, '_, '_, 'h>, x: T<'h>) -> Option<T<'h>> {
    match fx.num(x)? {
        Num::Complex(re, im @ Real::Exact(_)) if re.is_exact_zero() => Some(fx.ar.number(&Num::Real(im))),
        _ => None,
    }
}

fn asinh_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if fx.is_int(x, 0) {
        return Ok(Some(x));
    }
    match fx.infinity(x) {
        Some(0) => return Ok(Some(fx.inf())),
        Some(2) => return Ok(Some(fx.neg(fx.inf()))),
        _ => {}
    }
    if let Some(v) = odd(fx, "asinh", x) {
        return Ok(Some(v));
    }
    // asinh(b i) = i asin(b)
    if let Some(b) = imaginary(fx, x) {
        if let Some(v) = closed(fx, "asin", b)? {
            return Ok(Some(fx.mul(&[fx.i(), v])));
        }
    }
    Ok(None)
}

fn asinh_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if let Some(v) = fx.machine_of(x) {
        return Ok(Some(fx.real(v.asinh())));
    }
    // the logarithm form, on the right half plane (no cancellation): asinh(-z) = -asinh(z)
    let left = matches!(fx.num(x), Some(n) if n.as_real().map_or_else(|| re_negative(&n), |r| r.is_negative()));
    let z = if left { fx.neg(x) } else { x };
    let s = fx.sqrt(fx.add(&[fx.pow(z, fx.int(2)), fx.int(1)]));
    let v = fx.call("log", &[fx.add(&[z, s])]);
    Ok(Some(if left { fx.neg(v) } else { v }))
}

fn re_negative(n: &Num) -> bool {
    match n {
        Num::Complex(re, _) => re.is_negative(),
        Num::Real(r) => r.is_negative(),
    }
}

fn acosh_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if fx.is_int(x, 1) {
        return Ok(Some(fx.int(0)));
    }
    if fx.is_int(x, 0) {
        return Ok(Some(i_pi(fx, 1, 2)));
    }
    if fx.is_int(x, -1) {
        return Ok(Some(i_pi(fx, 1, 1)));
    }
    Ok(fx.infinity(x).map(|_| fx.inf()))
}

fn acosh_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if let Some(v) = fx.machine_of(x).filter(|v| *v >= 1.0) {
        return Ok(Some(fx.real(v.acosh())));
    }
    let s = fx.mul(&[fx.sqrt(fx.add(&[x, fx.int(1)])), fx.sqrt(fx.add(&[x, fx.int(-1)]))]);
    Ok(Some(fx.call("log", &[fx.add(&[x, s])])))
}

fn atanh_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if fx.is_int(x, 0) {
        return Ok(Some(x));
    }
    if fx.is_int(x, 1) {
        return Ok(Some(fx.inf()));
    }
    if fx.is_int(x, -1) {
        return Ok(Some(fx.neg(fx.inf())));
    }
    match fx.infinity(x) {
        Some(0) => return Ok(Some(i_pi(fx, -1, 2))),
        Some(2) => return Ok(Some(i_pi(fx, 1, 2))),
        _ => {}
    }
    if let Some(v) = odd(fx, "atanh", x) {
        return Ok(Some(v));
    }
    // atanh(b i) = i atan(b)
    if let Some(b) = imaginary(fx, x) {
        if let Some(v) = closed(fx, "atan", b)? {
            return Ok(Some(fx.mul(&[fx.i(), v])));
        }
    }
    Ok(None)
}

fn atanh_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if let Some(v) = fx.machine_of(x).filter(|v| v.abs() < 1.0) {
        return Ok(Some(fx.real(v.atanh())));
    }
    let l1 = fx.call("log", &[fx.add(&[fx.int(1), x])]);
    let l2 = fx.call("log", &[fx.sub(fx.int(1), x)]);
    Ok(Some(fx.mul(&[fx.rat(1, 2), fx.sub(l1, l2)])))
}

/// f(x) = inverse(1/x) for the inverse reciprocal functions; `at_zero` is f(0).
fn inv_recip<'h>(fx: &mut Fx<'_, '_, '_, 'h>, inverse: &str, x: T<'h>, at_zero: Option<T<'h>>) -> R<'h> {
    if fx.is_int(x, 0) {
        return Ok(at_zero);
    }
    if fx.infinity(x).is_some() {
        return closed(fx, inverse, fx.int(0));
    }
    if fx.num(x).is_none() {
        return Ok(None);
    }
    closed(fx, inverse, fx.pow(x, fx.int(-1)))
}

fn acot_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if let Some(v) = odd(fx, "acot", a[0]) {
        return Ok(Some(v));
    }
    let half_pi = fx.mul(&[fx.rat(1, 2), fx.sym("pi")]);
    inv_recip(fx, "atan", a[0], Some(half_pi))
}
fn asec_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    inv_recip(fx, "acos", a[0], None)
}
fn acsc_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if let Some(v) = odd(fx, "acsc", a[0]) {
        return Ok(Some(v));
    }
    inv_recip(fx, "asin", a[0], None)
}
fn acoth_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if let Some(v) = odd(fx, "acoth", a[0]) {
        return Ok(Some(v));
    }
    let z = i_pi(fx, 1, 2);
    inv_recip(fx, "atanh", a[0], Some(z))
}
fn asech_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let inf = fx.inf();
    inv_recip(fx, "acosh", a[0], Some(inf))
}
fn acsch_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if let Some(v) = odd(fx, "acsch", a[0]) {
        return Ok(Some(v));
    }
    inv_recip(fx, "asinh", a[0], None)
}

macro_rules! inv_recip_n {
    ($name:ident, $inverse:literal) => {
        fn $name<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
            if fx.num(a[0]).is_some_and(|n| n.is_zero()) {
                return Ok(None);
            }
            Ok(Some(fx.call($inverse, &[fx.pow(a[0], fx.int(-1))])))
        }
    };
}
inv_recip_n!(acot_n, "atan");
inv_recip_n!(asec_n, "acos");
inv_recip_n!(acsc_n, "asin");
inv_recip_n!(acoth_n, "atanh");
inv_recip_n!(asech_n, "acosh");
inv_recip_n!(acsch_n, "asinh");

fn sinc_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if fx.is_int(x, 0) {
        return Ok(Some(fx.int(1)));
    }
    if fx.infinity(x).is_some_and(|d| d % 2 == 0) {
        return Ok(Some(fx.int(0)));
    }
    if let Some(y) = fx.negated(x) {
        return Ok(Some(fx.call("sinc", &[y])));
    }
    let s = fx.eval(fx.call("sin", &[x]))?;
    Ok((!fx.contains(s, "sin")).then(|| fx.div(s, x)))
}

fn sinc_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if fx.num(a[0]).is_some_and(|n| n.is_zero()) {
        return Ok(Some(fx.int(1)));
    }
    Ok(Some(fx.div(fx.call("sin", &[a[0]]), a[0])))
}

/// log_b(x): an exact power of the base is its exponent; otherwise log(x)/log(b).
fn log_base<'h>(fx: &mut Fx<'_, '_, '_, 'h>, b: i64, x: T<'h>) -> R<'h> {
    if let Some(q) = fx.rat_of(x) {
        if !q.num.neg && !q.is_zero() {
            // x = b^k exactly?
            let (num, den) = (q.num.mag.clone(), q.den.clone());
            let one = mc_arith::num::Nat::from_u64(1);
            let (target, sign) = if den == one { (num, 1) } else if num == one { (den, -1) } else { (one.clone(), 0) };
            if sign != 0 {
                let mut k = 0i64;
                let mut p = one.clone();
                while p.bits() <= target.bits() {
                    if p == target {
                        return Ok(Some(fx.int(sign * k)));
                    }
                    p = p.mul_u64(b as u64);
                    k += 1;
                }
            }
        }
    }
    if fx.num(x).is_some_and(|n| n.kind() == mc_arith::Kind::Machine) {
        return machine_log_quotient(fx, fx.int(b), x).map(Some);
    }
    let l = fx.eval(fx.call("log", &[x]))?;
    // the logarithm of an infinity is inf, and so is inf divided by log(b) > 0
    if l == fx.sym("inf") {
        return Ok(Some(l));
    }
    Ok(Some(fx.div(l, fx.call("log", &[fx.int(b)]))))
}

/// log(x)/log(b) for numbers b and x of which one at least is a machine number, correctly rounded to a machine
/// number: the quotient of the logarithms of their exact binary values is evaluated to 40 digits and rounded once.
/// The quotient of two rounded machine logarithms is off by an ulp or more (log(8.0)/log(2.0) = 2.9999999999999996);
/// at 40 digits only a quotient within 10^-40 of a rounding boundary could round wrongly, and an exact quotient (a
/// power of the base, k/m for b^m = x^k) is an integer or a rational whose nearest machine number is found.
fn machine_log_quotient<'h>(fx: &mut Fx<'_, '_, '_, 'h>, b: T<'h>, x: T<'h>) -> Res<T<'h>> {
    let exact = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.call("set_precision", &[t, fx.inf()]);
    let q = fx.div(fx.call("log", &[exact(fx, x)]), fx.call("log", &[exact(fx, b)]));
    let wide = fx.call("n", &[q, fx.int(40)]);
    fx.eval(fx.call("n", &[wide]))
}
fn log10_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    log_base(fx, 10, a[0])
}
fn log2_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    log_base(fx, 2, a[0])
}

fn gd_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if fx.is_int(a[0], 0) {
        return Ok(Some(a[0]));
    }
    Ok(odd(fx, "gudermannian", a[0]))
}
fn gd_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let t = fx.call("tanh", &[fx.mul(&[fx.rat(1, 2), a[0]])]);
    Ok(Some(fx.mul(&[fx.int(2), fx.call("atan", &[t])])))
}
fn igd_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if fx.is_int(a[0], 0) {
        return Ok(Some(a[0]));
    }
    Ok(odd(fx, "inverse_gudermannian", a[0]))
}
fn igd_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let t = fx.call("tan", &[fx.mul(&[fx.rat(1, 2), a[0]])]);
    Ok(Some(fx.mul(&[fx.int(2), fx.call("atanh", &[t])])))
}

/// The sign (-1, 0, 1) of a real number or a closed real term, when decided.
fn sign_of<'h>(fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>) -> Res<Option<i64>> {
    if let Some(d) = fx.infinity(x) {
        return Ok(match d {
            0 => Some(1),
            2 => Some(-1),
            _ => None,
        });
    }
    match fx.num(x) {
        Some(Num::Real(r)) => {
            if let Real::Machine(v) = r {
                return Ok((!v.is_nan()).then_some(if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 }));
            }
            Ok(if r.is_positive() {
                Some(1)
            } else if r.is_negative() {
                Some(-1)
            } else if r.is_zero() {
                Some(0)
            } else {
                None
            })
        }
        Some(Num::Complex(..)) => Ok(None),
        None => {
            let s = fx.eval(fx.call("sign", &[x]))?;
            Ok(fx.int_of(s))
        }
    }
}

/// The integer part of a real (truncation toward zero), when decided: exact for exact and closed terms.
fn integer_part_of<'h>(fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>) -> R<'h> {
    if let Some(v) = fx.machine_of(x) {
        return Ok(Some(fx.real(v.trunc())));
    }
    let Some(s) = sign_of(fx, x)? else { return Ok(None) };
    let f = fx.eval(fx.call(if s >= 0 { "floor" } else { "ceiling" }, &[x]))?;
    Ok(fx.num(f).is_some().then_some(f))
}

fn integer_part_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if fx.infinity(x).is_some() {
        return Ok(Some(x));
    }
    if let Some(Num::Complex(re, im)) = fx.num(x) {
        let (re, im) = (fx.ar.number(&Num::Real(re)), fx.ar.number(&Num::Real(im)));
        let (Some(a), Some(b)) = (integer_part_of(fx, re)?, integer_part_of(fx, im)?) else { return Ok(None) };
        return Ok(Some(fx.add(&[a, fx.mul(&[fx.i(), b])])));
    }
    integer_part_of(fx, x)
}

fn fractional_part_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if let Some(v) = fx.machine_of(x) {
        return Ok(Some(fx.real(v - v.trunc())));
    }
    // x - trunc(x) has no limit at a real infinity: its values accumulate on [0, 1] as x -> inf and on [-1, 0] as
    // x -> -inf (the interval of the limit points, as for sin(inf))
    match fx.infinity(x) {
        Some(d @ (0 | 2)) => {
            let (lo, hi) = if d == 0 { (0, 1) } else { (-1, 0) };
            return Ok(Some(fx.call("interval", &[fx.list(&[fx.int(lo), fx.int(hi)])])));
        }
        Some(_) => return Ok(None),
        None => {}
    }
    let ip = integer_part_e(fx, a)?;
    Ok(ip.map(|ip| fx.sub(x, ip)))
}

fn unit_box_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    // the product of the boxes: 1 for |x| <= 1/2, else 0
    let mut open = vec![];
    for &x in a {
        let d = fx.eval(fx.call("less_equal", &[fx.call("abs", &[x]), fx.rat(1, 2)]))?;
        if d == fx.bool(false) {
            return Ok(Some(fx.int(0)));
        }
        if d != fx.bool(true) {
            open.push(x);
        }
    }
    Ok(if open.is_empty() {
        Some(fx.int(1))
    } else if open.len() < a.len() {
        Some(fx.call("unit_box", &open))
    } else {
        None
    })
}

fn unit_triangle_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let mut vals = vec![];
    for &x in a {
        let ax = fx.eval(fx.call("abs", &[x]))?;
        let d = fx.eval(fx.call("less", &[ax, fx.int(1)]))?;
        if d == fx.bool(false) {
            return Ok(Some(fx.int(0)));
        }
        if d != fx.bool(true) {
            return Ok(None);
        }
        vals.push(fx.sub(fx.int(1), ax));
    }
    Ok(Some(fx.mul(&vals)))
}

fn ramp_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    Ok(match sign_of(fx, x)? {
        Some(s) if s > 0 => Some(x),
        Some(_) => Some(if fx.machine_of(x).is_some() { fx.real(0.0) } else { fx.int(0) }),
        None => None,
    })
}

fn heaviside_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    // the product: 0 when one argument is negative, 1 when all are positive; stays at an exact 0
    let mut open = vec![];
    for &x in a {
        match sign_of(fx, x)? {
            Some(-1) => return Ok(Some(fx.int(0))),
            Some(1) => {}
            _ => open.push(x),
        }
    }
    Ok(if open.is_empty() {
        Some(fx.int(1))
    } else if open.len() < a.len() {
        Some(fx.call("heaviside_theta", &open))
    } else {
        None
    })
}

fn dirac_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    for &x in a {
        if matches!(sign_of(fx, x)?, Some(1 | -1)) {
            return Ok(Some(fx.int(0)));
        }
    }
    Ok(None)
}

fn real_abs_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    Ok(match sign_of(fx, x)? {
        Some(s) if s < 0 => Some(fx.neg(x)),
        Some(_) => Some(x),
        None => None,
    })
}

fn real_sign_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    let machine = fx.machine_of(x).is_some();
    Ok(sign_of(fx, x)?.map(|s| if machine { fx.real(s as f64) } else { fx.int(s) }))
}

/// surd(x, n): the real n-th root of x (negative x with odd n).
fn surd_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (x, n) = (a[0], a[1]);
    let k = fx.int_of(n).or_else(|| fx.machine_of(n).filter(|v| v.fract() == 0.0 && v.abs() < 1e9).map(|v| v as i64));
    let Some(k) = k.filter(|k| *k != 0) else { return Ok(None) };
    let e = fx.rat(1, k);
    match sign_of(fx, x)? {
        Some(s) if s >= 0 => Ok(Some(fx.pow(x, e))),
        Some(_) if k % 2 != 0 => Ok(Some(fx.neg(fx.pow(fx.neg(x), e)))),
        _ => Ok(None),
    }
}

fn re_im_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    Ok(Some(fx.list(&[fx.call("re", &[a[0]]), fx.call("im_part", &[a[0]])])))
}

fn abs_arg_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    Ok(Some(fx.list(&[fx.call("abs", &[a[0]]), fx.call("arg", &[a[0]])])))
}

macro_rules! def {
    ($name:literal, $ar:expr, $e:expr, $n:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: true, exact: $e, numeric: $n, summary: $s }
    };
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("sec", &[1], Some(sec_e), Some(sec_n), "sec(x): the secant 1/cos(x)"),
    def!("csc", &[1], Some(csc_e), Some(csc_n), "csc(x): the cosecant 1/sin(x)"),
    def!("cot", &[1], Some(cot_e), Some(cot_n), "cot(x): the cotangent 1/tan(x)"),
    def!("sech", &[1], Some(sech_e), Some(sech_n), "sech(x): the hyperbolic secant 1/cosh(x)"),
    def!("csch", &[1], Some(csch_e), Some(csch_n), "csch(x): the hyperbolic cosecant 1/sinh(x)"),
    def!("coth", &[1], Some(coth_e), Some(coth_n), "coth(x): the hyperbolic cotangent 1/tanh(x)"),
    def!("asinh", &[1], Some(asinh_e), Some(asinh_n), "asinh(z): the inverse hyperbolic sine log(z + sqrt(z^2 + 1))"),
    def!("acosh", &[1], Some(acosh_e), Some(acosh_n), "acosh(z): the inverse hyperbolic cosine, principal branch"),
    def!("atanh", &[1], Some(atanh_e), Some(atanh_n), "atanh(z): the inverse hyperbolic tangent (log(1 + z) - log(1 - z))/2"),
    def!("acot", &[1], Some(acot_e), Some(acot_n), "acot(z): the inverse cotangent atan(1/z)"),
    def!("asec", &[1], Some(asec_e), Some(asec_n), "asec(z): the inverse secant acos(1/z)"),
    def!("acsc", &[1], Some(acsc_e), Some(acsc_n), "acsc(z): the inverse cosecant asin(1/z)"),
    def!("acoth", &[1], Some(acoth_e), Some(acoth_n), "acoth(z): the inverse hyperbolic cotangent atanh(1/z)"),
    def!("asech", &[1], Some(asech_e), Some(asech_n), "asech(z): the inverse hyperbolic secant acosh(1/z)"),
    def!("acsch", &[1], Some(acsch_e), Some(acsch_n), "acsch(z): the inverse hyperbolic cosecant asinh(1/z)"),
    def!("sinc", &[1], Some(sinc_e), Some(sinc_n), "sinc(x): sin(x)/x, with sinc(0) = 1"),
    def!("log10", &[1], Some(log10_e), None, "log10(x): the base-10 logarithm log(x)/log(10); exact powers of 10 give integers"),
    def!("log2", &[1], Some(log2_e), None, "log2(x): the base-2 logarithm log(x)/log(2); exact powers of 2 give integers"),
    def!("gudermannian", &[1], Some(gd_e), Some(gd_n), "gudermannian(x): the Gudermannian function 2 atan(tanh(x/2))"),
    def!(
        "inverse_gudermannian",
        &[1],
        Some(igd_e),
        Some(igd_n),
        "inverse_gudermannian(x): the inverse Gudermannian function 2 atanh(tan(x/2))"
    ),
    def!("integer_part", &[1], Some(integer_part_e), None, "integer_part(x): x truncated toward zero (componentwise for complex x)"),
    def!("fractional_part", &[1], Some(fractional_part_e), None, "fractional_part(x): x - integer_part(x), with the sign of x"),
    def!("unit_box", &[1, 2, 3, 4], Some(unit_box_e), None, "unit_box(x, ...): 1 for |x| <= 1/2, else 0 (a product over the arguments)"),
    def!(
        "unit_triangle",
        &[1, 2, 3, 4],
        Some(unit_triangle_e),
        None,
        "unit_triangle(x, ...): the triangle 1 - |x| for |x| < 1, else 0 (a product over the arguments)"
    ),
    def!("ramp", &[1], Some(ramp_e), None, "ramp(x): x for x >= 0, else 0"),
    def!(
        "heaviside_theta",
        &[1, 2, 3, 4],
        Some(heaviside_e),
        None,
        "heaviside_theta(x, ...): 1 for x > 0, 0 for x < 0, unevaluated at 0 (a product over the arguments)"
    ),
    def!("dirac_delta", &[1, 2, 3, 4], Some(dirac_e), None, "dirac_delta(x): the Dirac delta, 0 at nonzero reals"),
    def!("real_abs", &[1], Some(real_abs_e), None, "real_abs(x): |x| for real x (its derivative is real_sign)"),
    def!("real_sign", &[1], Some(real_sign_e), None, "real_sign(x): -1, 0 or 1 for real x"),
    def!("surd", &[2], Some(surd_e), None, "surd(x, n): the real n-th root of x, negative for x < 0 and odd n"),
    def!("re_im", &[1], Some(re_im_e), None, "re_im(z): the list [re(z), im_part(z)]"),
    def!("abs_arg", &[1], Some(abs_arg_e), None, "abs_arg(z): the list [abs(z), arg(z)]"),
];
