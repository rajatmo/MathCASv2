#![allow(clippy::neg_cmp_op_on_partial_ord, clippy::manual_is_multiple_of)]
//! Special functions added for coverage, defined by formulas (`mc_numeric::formula`) or by
//! series evaluated in the canonical arithmetic:
//!
//! - theta functions: `elliptic_theta`, `elliptic_theta_prime`, the Neville theta functions, `modular_lambda`,
//!   `klein_invariant_j`, `inverse_elliptic_nome_q`, `dedekind_eta`;
//! - q-analogues: `q_pochhammer`, `q_factorial`, `q_binomial`, `q_gamma`;
//! - the inverse Jacobi functions not in the core set, `jacobi_amplitude`, `jacobi_zn`, `jacobi_epsilon`;
//! - Bessel relatives: `spherical_hankel_h1`/`_h2`, the Kelvin functions, `struve_h`, `struve_l`, `anger_j`,
//!   `weber_e`, `lommel_s1`, `lommel_s2`;
//! - confluent relatives: `hypergeometric_u`, `whittaker_m`, `whittaker_w`, `parabolic_cylinder_d`,
//!   `hypergeometric_pfq_regularized`;
//! - `fresnel_f`, `fresnel_g`, `logistic_sigmoid`, `riemann_siegel_theta`, `riemann_siegel_z`, `lerch_phi`,
//!   `mittag_leffler_e`, `barnes_g`, `log_barnes_g`, `inverse_gamma_regularized`, `inverse_beta_regularized`.
//!
//! Decisions (cov-num.2): numeric values come from a **working form** ([`Work`]): the arguments as balls at the
//! target precision plus the bits a series is expected to cancel (machine arguments too, so alternating series of
//! large arguments keep their digits), the result rounded back to the arguments' kind. Real arguments give real
//! values where the function is real there. Series are summed in the canonical arithmetic until a term is below
//! 2^-w of the sum. Symbolic arguments stay, apart from the special values listed with each function.
use mc_api::T;
use mc_arith::num::{Ball, Rat};
use mc_arith::{Kind, Num, Real};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

// ---------------------------------------------------------------- the working form

/// How numeric arguments are evaluated: in balls at `w` bits, the result returned to machine numbers or to balls
/// at `p` bits.
pub(crate) struct Work {
    pub(crate) machine: bool,
    pub(crate) p: u64,
    pub(crate) w: u64,
    pub(crate) real: bool,
}

/// Every argument a number and one inexact: the working form with `extra` bits beyond the target, and the
/// arguments as balls at the working precision.
pub(crate) fn work<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>], extra: u64) -> Option<(Work, Vec<T<'h>>)> {
    let nums: Vec<Num> = a.iter().map(|&t| fx.num(t)).collect::<Option<_>>()?;
    if !nums.iter().any(|n| n.kind() != Kind::Exact) {
        return None;
    }
    let machine = nums.iter().any(|n| n.kind() == Kind::Machine);
    let prec = |r: &Real| r.prec().unwrap_or(0);
    let p = if machine {
        53
    } else {
        nums.iter()
            .map(|n| match n {
                Num::Real(r) => prec(r),
                Num::Complex(x, y) => prec(x).max(prec(y)),
            })
            .max()
            .unwrap_or(53)
            .max(53)
    };
    let w = p + extra.min(20_000) + 24;
    let ball = |r: &Real| match r {
        Real::Machine(v) => Real::Ball(Ball::exact(crate::ball::float_of(*v)), w),
        Real::Ball(b, _) => Real::Ball(b.clone(), w),
        Real::Exact(_) => r.to_kind(Kind::Ball, w),
    };
    let args = nums
        .iter()
        .map(|n| {
            fx.ar.number(&match n {
                Num::Real(r) => Num::Real(ball(r)),
                Num::Complex(x, y) => Num::Complex(ball(x), ball(y)),
            })
        })
        .collect();
    let real = nums.iter().all(|n| matches!(n, Num::Real(_)));
    Some((Work { machine, p, w, real }, args))
}

impl Work {
    /// The value `v` (a number at the working precision) in the arguments' kind; its real part when `real`.
    pub(crate) fn out<'h>(&self, fx: &Fx<'_, '_, '_, 'h>, v: T<'h>, real: bool) -> Option<T<'h>> {
        let n = fx.num(v)?;
        let n = match n {
            Num::Complex(x, _) if real => Num::Real(x),
            // an imaginary part 1e-40 below the real part of a machine value is rounding noise of a real result
            // (hypergeometric_2f1_regularized(1, 2, -3, 4.5) had 2e-65 i)
            Num::Complex(x, y) if self.machine && self.real && x.to_f64() != 0.0 && y.to_f64() != 0.0 && y.to_f64().abs() <= 1e-40 * x.to_f64().abs() => Num::Real(x),
            // a machine value whose imaginary part is a ball around zero: real at the precision of the arguments when
            // they are real; a complex argument keeps a complex value with the imaginary part 0.0 (round 36:
            // elliptic_theta(2, 0, 0.2 + 0.0 im) was real)
            Num::Complex(x, Real::Ball(b, _)) if self.machine && b.contains_zero() && x.to_f64() != 0.0 => {
                if self.real {
                    Num::Real(x)
                } else {
                    Num::Complex(x, Real::Machine(0.0))
                }
            }
            n => n,
        };
        let at = |r: Real| match r {
            Real::Ball(b, _) if !self.machine => Real::Ball(b, self.p),
            r if self.machine => r.to_kind(Kind::Machine, 53),
            r => r,
        };
        Some(fx.ar.number(&match n {
            Num::Real(r) => Num::Real(at(r)),
            Num::Complex(x, y) => Num::Complex(at(x), at(y)),
        }))
    }
}

/// |t| of a number as an `f64` (0 for anything else).
pub(crate) fn mag<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> f64 {
    match fx.num(t) {
        Some(Num::Real(r)) => r.to_f64().abs(),
        Some(Num::Complex(x, y)) => x.to_f64().hypot(y.to_f64()),
        None => 0.0,
    }
}

/// The parts of a number as `f64`s.
pub(crate) fn parts<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(f64, f64)> {
    match fx.num(t)? {
        Num::Real(r) => Some((r.to_f64(), 0.0)),
        Num::Complex(x, y) => Some((x.to_f64(), y.to_f64())),
    }
}

/// Every argument a number and one inexact?
pub(crate) fn numeric<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> bool {
    let nums: Option<Vec<Num>> = a.iter().map(|&t| fx.num(t)).collect();
    nums.is_some_and(|ns| ns.iter().any(|n| n.kind() != Kind::Exact))
}

/// `name(args)` evaluated; a closed form of numeric arguments (a formula with `pi`, say) is then evaluated
/// numerically at the arguments' precision.
pub(crate) fn ev<'h>(fx: &mut Fx<'_, '_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
    let c = fx.call(name, args);
    if name == "n" {
        return fx.eval(c);
    }
    numify(fx, c, args)
}

/// The term `c` evaluated; a closed form (a formula with `pi`, `exp(pi i nu)`, say) is then evaluated numerically at
/// the kind and precision of the numbers among `args`.
pub(crate) fn numify<'h>(fx: &mut Fx<'_, '_, '_, 'h>, c: T<'h>, args: &[T<'h>]) -> Res<T<'h>> {
    let v = fx.eval(c)?;
    if fx.num(v).is_some() {
        return Ok(v);
    }
    let mut prec: Option<u64> = None;
    let mut machine = false;
    for &a in args {
        match fx.num(a) {
            Some(Num::Real(r)) => {
                machine |= r.kind() == Kind::Machine;
                prec = prec.max(r.prec());
            }
            Some(Num::Complex(x, y)) => {
                machine |= x.kind() == Kind::Machine || y.kind() == Kind::Machine;
                prec = prec.max(x.prec()).max(y.prec());
            }
            None => {}
        }
    }
    if machine {
        // through 20 digits: n(e^9) at machine precision is 2 ulp off (the constant is rounded before the power), the
        // value rounded once from 20 digits is not
        let hi = fx.eval(fx.call("n", &[v, fx.int(20)]))?;
        if fx.num(hi).is_some() {
            return fx.eval(fx.call("n", &[hi]));
        }
        return fx.eval(fx.call("n", &[v]));
    }
    match prec {
        Some(p) => fx.eval(fx.call("n", &[v, fx.int(digits(p))])),
        None => Ok(v),
    }
}

/// The decimal digits of p bits (rounded up).
pub(crate) fn digits(p: u64) -> i64 {
    (p as f64 * std::f64::consts::LOG10_2).ceil() as i64 + 1
}

/// An exact number as a ball at w bits (other numbers unchanged).
pub(crate) fn ballify<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, w: u64) -> T<'h> {
    match fx.num(t) {
        Some(n) if n.kind() == Kind::Exact => fx.ar.number(&n.to_kind(Kind::Ball, w)),
        _ => t,
    }
}

/// pi as a ball at w bits.
pub(crate) fn pi_at<'h>(fx: &mut Fx<'_, '_, '_, 'h>, w: u64) -> Res<T<'h>> {
    let d = fx.int(digits(w));
    let pi = fx.sym("pi");
    ev(fx, "n", &[pi, d])
}

/// b^e evaluated.
pub(crate) fn evpow<'h>(fx: &mut Fx<'_, '_, '_, 'h>, b: T<'h>, e: T<'h>) -> Res<T<'h>> {
    let t = fx.pow(b, e);
    fx.eval(t)
}

/// Is the number a non-positive integer (a pole of the gamma function)?
pub(crate) fn gamma_pole<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    match parts(fx, t) {
        Some((x, y)) => y == 0.0 && x <= 0.0 && x == x.floor(),
        None => false,
    }
}

/// Does a ball number (either part) have a radius?
pub(crate) fn has_radius<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    let r = |x: &Real| matches!(x, Real::Ball(b, _) if !b.rad.is_zero());
    match fx.num(t) {
        Some(Num::Real(x)) => r(&x),
        Some(Num::Complex(x, y)) => r(&x) || r(&y),
        None => false,
    }
}

/// A ball number with `f` applied to every ball part; other terms unchanged.
pub(crate) fn map_balls<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, f: impl Fn(&Ball) -> Ball) -> T<'h> {
    let g = |x: Real| match x {
        Real::Ball(b, p) => Real::Ball(f(&b), p),
        x => x,
    };
    match fx.num(t) {
        Some(Num::Real(x)) => fx.ar.number(&Num::Real(g(x))),
        Some(Num::Complex(x, y)) => fx.ar.number(&Num::Complex(g(x), g(y))),
        None => t,
    }
}

/// A ball number as its midpoint (exact); other terms unchanged.
pub(crate) fn mid_term<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> T<'h> {
    map_balls(fx, t, |b| Ball::exact(b.mid.clone()))
}

/// A ball number as its midpoint moved up by the radius (exact at `w` bits); other terms unchanged.
fn shift_term<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, w: u64) -> T<'h> {
    map_balls(fx, t, |b| Ball::exact(Ball::exact(b.mid.clone()).add(&Ball::exact(b.rad.to_float()), w).mid))
}

/// Is the number exactly a non-positive integer (an exact ball or rational: not merely within rounding of one)?
fn exact_gamma_pole<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    match fx.num(t) {
        Some(Num::Real(Real::Exact(q))) => q.is_int() && q.num.neg || q.is_zero(),
        Some(Num::Real(Real::Ball(b, _))) => {
            b.rad.is_zero() && crate::ball::float_int(&b.mid).and_then(|n| n.to_i64()).is_some_and(|n| n <= 0)
        }
        Some(Num::Real(Real::Machine(v))) => v <= 0.0 && v == v.floor(),
        _ => false,
    }
}

/// 1/Gamma(x) for k = 0, 1, ..., n at x0 + k: zeros at the poles, the recurrence 1/Gamma(x + 1) = 1/(x Gamma(x))
/// after the last one.
pub(crate) fn rgamma_seq<'h>(fx: &mut Fx<'_, '_, '_, 'h>, x0: T<'h>, n: usize) -> Res<Option<Vec<T<'h>>>> {
    let mut out = Vec::with_capacity(n + 1);
    let mut x = x0;
    let mut cur: Option<T<'h>> = None;
    for _ in 0..=n {
        let v = match cur {
            None if gamma_pole(fx, x) => fx.int(0),
            None => {
                let g = ev(fx, "gamma", &[x])?;
                if fx.num(g).is_none() {
                    return Ok(None);
                }
                fx.div(fx.int(1), g)
            }
            Some(prev) => {
                let xm = fx.sub(x, fx.int(1));
                fx.div(prev, xm)
            }
        };
        cur = if fx.num(v).is_some_and(|n| n.is_zero()) { None } else { Some(v) };
        out.push(v);
        x = fx.add(&[x, fx.int(1)]);
    }
    Ok(Some(out))
}

/// The most terms a series takes.
pub(crate) const MAX_TERMS: usize = 20_000;

/// Sum terms t_k (k = 0, 1, ...) given by `term` until, after `min` terms, one is below 2^-w of the sum. `None`
/// when the series does not settle within [`MAX_TERMS`].
pub(crate) fn sum_series<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    w: u64,
    min: usize,
    mut term: impl FnMut(&mut Fx<'_, '_, '_, 'h>, usize) -> Res<Option<T<'h>>>,
) -> Res<Option<T<'h>>> {
    let eps = 2f64.powi(-(w.min(1000) as i32));
    let mut s = fx.int(0);
    let mut small = 0;
    for k in 0..MAX_TERMS {
        if k % 64 == 63 {
            fx.tick(1)?;
        }
        let Some(t) = term(fx, k)? else { return Ok(None) };
        s = fx.add(&[s, t]);
        let (mt, ms) = (mag(fx, t), mag(fx, s));
        if k >= min && mt <= eps * ms.max(1e-300) {
            small += 1;
            if small >= 2 {
                return Ok(Some(s));
            }
        } else {
            small = 0;
        }
    }
    Ok(None)
}

macro_rules! def {
    ($name:literal, $ar:expr, $e:expr, $n:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: true, exact: $e, numeric: $n, summary: $s }
    };
}

// ---------------------------------------------------------------- theta functions

/// The terms needed for |q|^(n^2) e^(2 n |Im u|) < 2^-w.
pub(crate) fn theta_terms(q: (f64, f64), im_u: f64, w: u64) -> Option<usize> {
    let aq = q.0.hypot(q.1);
    if !(aq < 1.0) {
        return None;
    }
    if aq == 0.0 {
        return Some(1);
    }
    let lq = -aq.ln();
    let b = 2.0 * im_u.abs();
    let c = w as f64 * std::f64::consts::LN_2;
    let n = (b + (b * b + 4.0 * lq * c).sqrt()) / (2.0 * lq) + 2.0;
    (n < 5000.0).then_some(n as usize)
}

/// theta_a(u, q) (`deriv` false) or its derivative in u, for a in 1..4, at working-precision numbers.
fn theta<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: i64, u: T<'h>, q: T<'h>, w: u64, deriv: bool) -> Res<Option<T<'h>>> {
    let Some(qp) = parts(fx, q) else { return Ok(None) };
    let Some(up) = parts(fx, u) else { return Ok(None) };
    let Some(n) = theta_terms(qp, up.1, w) else { return Ok(None) };
    let mut s = fx.int(0);
    let q2 = fx.mul(&[q, q]);
    if a <= 2 {
        // 2 q^(1/4) sum (+-1)^k q^(k(k+1)) {sin, cos}((2k + 1) u)
        let mut qk = fx.int(1); // q^(k(k+1))
        let mut ratio = q2; // q^(2k + 2)
        for k in 0..=n as i64 {
            let arg = fx.mul(&[fx.int(2 * k + 1), u]);
            let trig = match (a, deriv) {
                (1, false) | (2, true) => ev(fx, "sin", &[arg])?,
                _ => ev(fx, "cos", &[arg])?,
            };
            let mut t = fx.mul(&[qk, trig]);
            if deriv {
                t = fx.mul(&[fx.int(2 * k + 1), t]);
            }
            if a == 1 && k % 2 == 1 {
                t = fx.neg(t);
            }
            s = fx.add(&[s, t]);
            qk = fx.mul(&[qk, ratio]);
            ratio = fx.mul(&[ratio, q2]);
        }
        let q4 = evpow(fx, q, fx.rat(1, 4))?;
        let pre = fx.mul(&[fx.int(if a == 2 && deriv { -2 } else { 2 }), q4]);
        return Ok(Some(fx.mul(&[pre, s])));
    }
    // 1 + 2 sum (+-1)^k q^(k^2) cos(2 k u); the derivative -4 sum (+-1)^k k q^(k^2) sin(2 k u)
    let mut qk = q; // q^(k^2)
    let mut ratio = fx.mul(&[q2, q]); // q^(2k + 1)
    for k in 1..=n as i64 {
        let arg = fx.mul(&[fx.int(2 * k), u]);
        let trig = if deriv { ev(fx, "sin", &[arg])? } else { ev(fx, "cos", &[arg])? };
        let mut t = fx.mul(&[qk, trig]);
        if deriv {
            t = fx.mul(&[fx.int(k), t]);
        }
        if a == 4 && k % 2 == 1 {
            t = fx.neg(t);
        }
        s = fx.add(&[s, t]);
        qk = fx.mul(&[qk, ratio]);
        ratio = fx.mul(&[ratio, q2]);
    }
    Ok(Some(if deriv { fx.mul(&[fx.int(-4), s]) } else { fx.add(&[fx.int(1), fx.mul(&[fx.int(2), s])]) }))
}

fn theta_rule<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], deriv: bool) -> R<'h> {
    // the index may be an integer-valued real: n(elliptic_theta(2, 0, q)) maps n over every argument
    let idx = fx.int_of(a[0]).or_else(|| fx.f64_of(a[0]).filter(|v| v.fract() == 0.0 && v.abs() < 8.0).map(|v| v as i64));
    let Some(k) = idx.filter(|k| (1..=4).contains(k)) else { return Ok(None) };
    let (u, q) = (a[1], a[2]);
    if fx.is_int(q, 0) {
        return Ok(Some(match (k, deriv) {
            (3 | 4, false) => fx.int(1),
            _ => fx.int(0),
        }));
    }
    // zeros at the multiples of pi/2 (u = n pi/2, n even or odd; DLMF 20.2.1-4: theta_1 odd and theta_2 even about 0,
    // each of the four has the symmetry about pi/2 of its period): theta_1 and theta_2' vanish at n pi, theta_2 and
    // theta_1' at pi/2 + n pi, theta_3' and theta_4' at every n pi/2
    if fx.num(u).is_none_or(|n| n.kind() == Kind::Exact) {
        let tw = fx.eval(fx.div(fx.mul(&[fx.int(2), u]), fx.sym("pi")))?;
        let even = fx.is_int(u, 0) || fx.int_of(tw).is_some_and(|n| n % 2 == 0);
        let odd = fx.int_of(tw).is_some_and(|n| n % 2 != 0);
        let zero = match (k, deriv) {
            (1, false) | (2, true) => even,
            (2, false) | (1, true) => odd,
            (3 | 4, true) => even || odd,
            _ => false,
        };
        if zero {
            return Ok(Some(fx.int(0)));
        }
    }
    if !numeric(fx, &a[1..]) {
        return Ok(None);
    }
    // machine arguments: the series in complex machine arithmetic (round 37; the ball path took 1 ms a point and the
    // contour plots of theta_4 timed out), the ball path when it declines
    if let Some((ms, real)) = crate::fast29::machine_args(fx, &a[1..]) {
        if let Some(v) = crate::fast29::theta(k as u8, ms[0], ms[1], deriv) {
            let real = real && (ms[1].re.hi >= 0.0 || k >= 3);
            if let Some(t) = crate::fast29::machine_out(fx, v, real) {
                return Ok(Some(t));
            }
        }
    }
    let im = parts(fx, u).map_or(0.0, |p| p.1.abs());
    let Some((wk, xs)) = work(fx, &a[1..], (im * 3.0) as u64 + 8) else { return Ok(None) };
    // a real nome near 1 (q = e^(-pi t), 0 < t < 1): Jacobi's imaginary transformation (DLMF 20.7.30-33 with tau = i t,
    // tau' = i/t), theta_k(z | tau) = t^(-1/2) e^(-z^2/(pi t)) {-i theta_1, theta_4, theta_3, theta_2}(i z/t | tau'): the
    // series in q' = e^(-pi/t) converges fast and has no cancellation (theta_3(pi/2, e^(-pi/1000)) = 5.09e-340 was a ball
    // around 0 at 50 digits: the q-series cancels to 1e-340)
    let qr = parts(fx, xs[1]).filter(|p| p.1 == 0.0 && p.0 > (-std::f64::consts::PI).exp() && p.0 < 1.0);
    let real_q = matches!(fx.num(xs[1]), Some(Num::Real(_)));
    if !deriv && qr.is_some() && real_q {
        let pi = fx.sym("pi");
        let t = numify(fx, fx.div(fx.neg(fx.call("log", &[xs[1]])), pi), &xs)?;
        let qp = numify(fx, fx.call("exp", &[fx.neg(fx.div(pi, t))]), &xs)?;
        let up = fx.eval(fx.div(fx.mul(&[fx.i(), xs[0]]), t))?;
        let (kk, c) = match k {
            1 => (1, fx.neg(fx.i())),
            2 => (4, fx.int(1)),
            3 => (3, fx.int(1)),
            _ => (2, fx.int(1)),
        };
        if let Some(th) = theta(fx, kk, up, qp, wk.w, false)? {
            let pre = fx.mul(&[c, fx.pow(t, fx.rat(-1, 2)), fx.call("exp", &[fx.neg(fx.div(fx.mul(&[xs[0], xs[0]]), fx.mul(&[pi, t])))])]);
            let v = numify(fx, fx.mul(&[pre, th]), &xs)?;
            if fx.num(v).is_some() {
                let real = wk.real && parts(fx, a[2]).is_some_and(|p| p.0 >= 0.0);
                return Ok(wk.out(fx, v, real));
            }
        }
    }
    let Some(v) = theta(fx, k, xs[0], xs[1], wk.w, deriv)? else { return Ok(None) };
    let real = wk.real && parts(fx, q).is_some_and(|p| p.0 >= 0.0);
    Ok(wk.out(fx, v, real))
}

fn elliptic_theta<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    theta_rule(fx, a, false)
}
fn elliptic_theta_prime<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    theta_rule(fx, a, true)
}

/// The Neville theta functions through the Jacobi theta functions at the nome of m: v = pi z/(2 K).
fn neville<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: u8) -> R<'h> {
    let (z, m) = (a[0], a[1]);
    if fx.is_int(m, 0) {
        return Ok(Some(match which {
            0 => fx.call("sin", &[z]),
            1 => fx.call("cos", &[z]),
            _ => fx.int(1),
        }));
    }
    if fx.is_int(z, 0) {
        return Ok(Some(if which == 0 { fx.int(0) } else { fx.int(1) }));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // an inexact m = 1: theta_s = sinh, theta_n = cosh, theta_c = theta_d = 1 (the limits of the exact case m = 1; K(1)
    // is infinite, and the ball path never returned: the plot of neville_theta_d(1, m) over [-10, 1] timed out)
    if fx.f64_of(m) == Some(1.0) {
        let one = fx.mul(&[m, fx.int(1)]);
        let v = match which {
            0 => fx.call("sinh", &[z]),
            3 => fx.call("cosh", &[z]),
            _ => return Ok(Some(fx.eval(one)?)),
        };
        return numify(fx, v, a).map(Some);
    }
    // an exact m with an inexact z (or the reverse): every argument at the working precision, so that K, the nome and the
    // theta constants are numbers and not closed forms (neville_theta_n(2., 1/3) kept theta_4(0, q(1/3)) unevaluated)
    let Some((wk, mut xs)) = work(fx, a, 24) else { return Ok(None) };
    if wk.machine {
        // machine arguments stay machine numbers: the machine K, nome and theta functions take microseconds where the
        // complex ball path takes 30 ms (the plots of neville_theta_n(1, m) over m > 1 timed out)
        xs = a.iter().map(|&t| fx.eval(fx.call("n", &[t]))).collect::<Res<_>>()?;
    }
    let (z, m) = (xs[0], xs[1]);
    let k = ev(fx, "elliptic_k", &[m])?;
    let q = ev(fx, "elliptic_nome_q", &[m])?;
    let v = fx.div(fx.mul(&[fx.sym("pi"), z]), fx.mul(&[fx.int(2), k]));
    let zero = fx.int(0);
    let e = if which == 0 {
        let d = ev(fx, "elliptic_theta_prime", &[fx.int(1), zero, q])?;
        let t1 = ev(fx, "elliptic_theta", &[fx.int(1), v, q])?;
        fx.div(fx.mul(&[fx.int(2), k, t1]), fx.mul(&[fx.sym("pi"), d]))
    } else {
        let i = which as i64 + 1;
        let num = ev(fx, "elliptic_theta", &[fx.int(i), v, q])?;
        let den = ev(fx, "elliptic_theta", &[fx.int(i), zero, q])?;
        fx.div(num, den)
    };
    let val = fx.eval(e)?;
    let real = wk.real && parts(fx, m).is_some_and(|p| p.0 > 0.0 && p.0 < 1.0);
    Ok(wk.out(fx, val, real))
}
fn neville_s<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    neville(fx, a, 0)
}
fn neville_c<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    neville(fx, a, 1)
}
fn neville_d<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    neville(fx, a, 2)
}
fn neville_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    neville(fx, a, 3)
}

/// The theta constants theta_2, theta_3, theta_4 at u = 0 of a nome given by a function of the argument.
fn theta_consts<'h>(fx: &Fx<'_, '_, '_, 'h>, q: T<'h>) -> [T<'h>; 3] {
    [2, 3, 4].map(|i| fx.call("elliptic_theta", &[fx.int(i), fx.int(0), q]))
}

/// The nome e^(i pi tau) of a point of the upper half-plane.
fn nome_of<'h>(fx: &Fx<'_, '_, '_, 'h>, tau: T<'h>) -> Option<T<'h>> {
    let (_, im) = parts(fx, tau)?;
    (im > 0.0).then(|| fx.call("exp", &[fx.mul(&[fx.i(), fx.sym("pi"), tau])]))
}

fn modular_lambda<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if !numeric(fx, a) {
        return Ok(None);
    }
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        if let Some(v) = crate::fast33::modular_lambda(ms[0]) {
            return Ok(crate::fast29::machine_out(fx, v, real && v.im == 0.0));
        }
        // a machine point below Im tau = 1 that the reduction cannot take (a value beyond 1e308, Im tau < 1e-8): the nome
        // series at |q| near 1 in machine numbers is noise (modular_lambda(0.3 + 1e-5 i) gave -2.6e-105 in 35 ms)
        if ms[0].im.hi < 1.0 {
            return Ok(None);
        }
    }
    let Some(q) = nome_of(fx, a[0]) else { return Ok(None) };
    let [t2, t3, _] = theta_consts(fx, q);
    Ok(Some(fx.pow(fx.div(t2, t3), fx.int(4))))
}

fn klein_j<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a[0] == fx.i() {
        return Ok(Some(fx.int(1)));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        if let Some(v) = crate::fast33::klein_j(ms[0]) {
            return Ok(crate::fast29::machine_out(fx, v, real && v.im == 0.0));
        }
        // a machine point below Im tau = 1 that the reduction cannot take (a value beyond 1e308, Im tau < 1e-8): the nome
        // series at |q| near 1 in machine numbers is noise (modular_lambda(0.3 + 1e-5 i) gave -2.6e-105 in 35 ms)
        if ms[0].im.hi < 1.0 {
            return Ok(None);
        }
    }
    let Some(q) = nome_of(fx, a[0]) else { return Ok(None) };
    let [t2, t3, t4] = theta_consts(fx, q);
    let p8 = |t: T<'h>| fx.pow(t, fx.int(8));
    let num = fx.pow(fx.add(&[p8(t2), p8(t3), p8(t4)]), fx.int(3));
    Ok(Some(fx.div(num, fx.mul(&[fx.int(54), p8(fx.mul(&[t2, t3, t4]))]))))
}

fn inverse_nome<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let q = a[0];
    if fx.is_int(q, 0) {
        return Ok(Some(fx.int(0)));
    }
    // q = 1, the end of the real domain -1 < q <= 1: m(q) = theta_2^4/theta_3^4 -> 1 as q -> 1- (the nome q(m) =
    // exp(-pi K'(m)/K(m)) -> 1 as m -> 1-, DLMF 22.2.1, 19.12.1); the theta series diverge there (round 37: the call
    // stayed theta_2(0, 1.)^4/theta_3(0, 1.)^4). An inexact 1 gives 1 of its own kind.
    if fx.is_int(q, 1) || fx.f64_of(q).is_some_and(|v| v == 1.0) && numeric(fx, a) && !has_radius(fx, q) {
        return Ok(Some(q));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let [t2, t3, _] = theta_consts(fx, q);
    Ok(Some(fx.pow(fx.div(t2, t3), fx.int(4))))
}

fn dedekind_eta<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a[0] == fx.i() {
        // gamma(1/4)/(2 pi^(3/4))
        let g = fx.call("gamma", &[fx.rat(1, 4)]);
        return Ok(Some(fx.div(g, fx.mul(&[fx.int(2), fx.pow(fx.sym("pi"), fx.rat(3, 4))]))));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let tau0 = a[0];
    if parts(fx, tau0).is_none_or(|p| p.1 <= 0.0) {
        return Ok(None);
    }
    // eta(tau + 24) = eta(tau): the real part reduced mod 24 exactly first (a machine real is an exact dyadic
    // number: its integer part by floor, reduced exactly, plus its fractional part), so a huge real part keeps its
    // digits; a ball's real part is left alone
    let re = ev(fx, "re", &[tau0])?;
    let im = ev(fx, "im_part", &[tau0])?;
    let re2 = match fx.num(re).map(|n| n.kind()) {
        Some(Kind::Exact) => Some(ev(fx, "mod", &[re, fx.int(24)])?),
        // a ball too when its floor is one definite integer
        Some(Kind::Machine | Kind::Ball) => {
            let ip = ev(fx, "floor", &[re])?;
            if fx.num(ip).is_some_and(|n| n.kind() == Kind::Exact) {
                let m = ev(fx, "mod", &[ip, fx.int(24)])?;
                let frac = fx.eval(fx.sub(re, ip))?;
                Some(fx.eval(fx.add(&[m, frac]))?)
            } else {
                None
            }
        }
        _ => None,
    };
    let tau = match re2 {
        Some(r) if r != re => fx.eval(fx.add(&[r, fx.mul(&[fx.i(), im])]))?,
        _ => tau0,
    };
    // e^(i pi tau/12) (q; q)_inf with q = e^(2 i pi tau), evaluated at the argument's precision (balls: with pi as
    // a ball, so the result carries its own radius and the caller's `n` judges it)
    let ball = match fx.num(tau0) {
        Some(Num::Complex(x, y)) if x.kind() == Kind::Ball || y.kind() == Kind::Ball => Some(x.prec().max(y.prec())),
        Some(Num::Real(x)) if x.kind() == Kind::Ball => Some(x.prec()),
        _ => None,
    };
    let pi = match ball {
        Some(w) => pi_at(fx, w.unwrap_or(53) + 16)?,
        None => fx.sym("pi"),
    };
    let pre = fx.call("exp", &[fx.mul(&[fx.rat(1, 12), fx.i(), pi, tau])]);
    let q = fx.call("exp", &[fx.mul(&[fx.int(2), fx.i(), pi, tau])]);
    let v = fx.mul(&[pre, fx.call("q_pochhammer", &[q, q])]);
    Ok(Some(if ball.is_some() { fx.eval(v)? } else { numify(fx, v, &[tau0])? }))
}

// ---------------------------------------------------------------- q-analogues

/// (a; q)_inf at working-precision numbers, |q| < 1.
fn q_product<'h>(fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>, q: T<'h>, w: u64) -> Res<Option<T<'h>>> {
    let aq = mag(fx, q);
    if !(aq < 1.0) {
        return Ok(None);
    }
    let ax = mag(fx, x).max(1e-300);
    let n = if aq == 0.0 { 1.0 } else { ((w as f64 * std::f64::consts::LN_2 + ax.ln().max(0.0)) / -aq.ln()).ceil() + 2.0 };
    if n > MAX_TERMS as f64 {
        // q near 1 with |x| < 1: log (x; q)_inf = -sum_{m>=1} x^m / (m (1 - q^m)) (expand each log(1 - x q^k) and
        // sum the geometric series in k), whose terms fall like |x|^m / (m (1 - |q|)): M terms with the tail below
        // 2^-w
        if !(ax < 0.95) {
            return Ok(None);
        }
        let lx = ax.ln();
        let floor = (1.0 - aq).ln() + (1.0 - ax).ln();
        let mut m_max = 1usize;
        while (m_max as f64) * lx - (m_max as f64).ln() + -floor > -(w as f64) * std::f64::consts::LN_2 {
            m_max += 1;
            if m_max > MAX_TERMS {
                return Ok(None);
            }
        }
        let mut s = Vec::with_capacity(m_max);
        let (mut xm, mut qm) = (x, q);
        for m in 1..=m_max {
            if m % 64 == 63 {
                fx.tick(1)?;
            }
            let den = fx.mul(&[fx.int(m as i64), fx.sub(fx.int(1), qm)]);
            s.push(fx.div(xm, den));
            xm = fx.mul(&[xm, x]);
            qm = fx.mul(&[qm, q]);
        }
        let total = fx.add(&s);
        let mt = fx.neg(total);
        return Ok(Some(ev(fx, "exp", &[mt])?));
    }
    let mut p = fx.int(1);
    let mut t = x;
    for k in 0..n as usize {
        if k % 64 == 63 {
            fx.tick(1)?;
        }
        p = fx.mul(&[p, fx.sub(fx.int(1), t)]);
        t = fx.mul(&[t, q]);
    }
    Ok(Some(p))
}

fn q_pochhammer<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (x, q) = match a.len() {
        1 => (a[0], a[0]),
        _ => (a[0], a[1]),
    };
    if a.len() == 3 {
        if let Some(n) = fx.int_of(a[2]).filter(|n| n.unsigned_abs() <= 1000) {
            // (x; q)_n = prod_{k<n} (1 - x q^k); negative n: 1/prod_{k=1}^{-n} (1 - x q^-k)
            let fs: Vec<T<'h>> = if n >= 0 {
                (0..n).map(|k| fx.sub(fx.int(1), fx.mul(&[x, fx.pow(q, fx.int(k))]))).collect()
            } else {
                (1..=-n).map(|k| fx.pow(fx.sub(fx.int(1), fx.mul(&[x, fx.pow(q, fx.int(-k))])), fx.int(-1))).collect()
            };
            return Ok(Some(fx.mul(&fs)));
        }
    }
    if fx.num(x).is_some_and(|n| n.is_exact_zero()) {
        return Ok(Some(fx.int(1)));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // machine arguments: the product in complex machine arithmetic with its cancellation bound (round 37; the
    // contour plots of q_pochhammer and q_factorial timed out on the ball path), else the ball path
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        let (x, q) = if a.len() == 1 { (ms[0], ms[0]) } else { (ms[0], ms[1]) };
        let v = crate::fast29::q_product(x, q).and_then(|v| {
            if a.len() < 3 {
                return Some(v);
            }
            // (x; q)_n = (x; q)_inf/(x q^n; q)_inf with the principal q^n
            let qn = (ms[2] * q.log()).exp();
            let d = crate::fast29::q_product(x * qn, q)?;
            (d.re != 0.0 || d.im != 0.0).then(|| v / d)
        });
        if let Some(t) = v.and_then(|v| crate::fast29::machine_out(fx, v, real)) {
            return Ok(Some(t));
        }
    }
    let Some((wk, xs)) = work(fx, a, 8) else { return Ok(None) };
    let (x, q) = if a.len() == 1 { (xs[0], xs[0]) } else { (xs[0], xs[1]) };
    let Some(mut v) = q_product(fx, x, q, wk.w)? else { return Ok(None) };
    if a.len() == 3 {
        // (x; q)_n = (x; q)_inf/(x q^n; q)_inf
        let qn = evpow(fx, q, xs[2])?;
        let xn = fx.mul(&[x, qn]);
        let Some(d) = q_product(fx, xn, q, wk.w)? else { return Ok(None) };
        v = fx.div(v, d);
    }
    Ok(wk.out(fx, v, wk.real))
}

/// `q_factorial(n, q)` (the count first).
fn q_factorial<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (n, q) = (a[0], a[1]);
    if fx.int_of(n).is_some_and(|n| (0..=1000).contains(&n)) {
        let n = fx.int_of(n).unwrap_or(0);
        // [k]_q! = prod (1 - q^k)/(1 - q) = prod (1 + q + ... + q^(k-1))
        let fs: Vec<T<'h>> = (1..=n).map(|k| fx.add(&(0..k).map(|j| fx.pow(q, fx.int(j))).collect::<Vec<_>>())).collect();
        return Ok(Some(fx.mul(&fs)));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // |q| > 1: [n]_q! = q^(n (n - 1)/2) [n]_(1/q)! (each [k]_q = q^(k - 1) [k]_(1/q)), the product converging in 1/q
    if mag(fx, q) > 1.0 {
        let e = fx.mul(&[fx.rat(1, 2), n, fx.sub(n, fx.int(1))]);
        return Ok(Some(fx.mul(&[fx.pow(q, e), fx.call("q_factorial", &[n, fx.div(fx.int(1), q)])])));
    }
    let qq = fx.call("q_pochhammer", &[q, q, n]);
    Ok(Some(fx.div(qq, fx.pow(fx.sub(fx.int(1), q), n))))
}

fn q_binomial<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (n, k, q) = (a[0], a[1], a[2]);
    if let (Some(nn), Some(kk)) = (fx.int_of(n), fx.int_of(k)) {
        if (0..=1000).contains(&nn) {
            if kk < 0 || kk > nn {
                return Ok(Some(fx.int(0)));
            }
            let f = |m: i64| fx.call("q_factorial", &[fx.int(m), q]);
            let v = fx.div(f(nn), fx.mul(&[f(kk), f(nn - kk)]));
            return Ok(Some(fx.eval(fx.call("cancel", &[v]))?));
        }
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let p = |m: T<'h>| fx.call("q_pochhammer", &[q, q, m]);
    Ok(Some(fx.div(p(n), fx.mul(&[p(k), p(fx.sub(n, k))]))))
}

fn q_gamma<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (z, q) = (a[0], a[1]);
    if let Some(n) = fx.int_of(z).filter(|n| (1..=1000).contains(n)) {
        return Ok(Some(fx.call("q_factorial", &[fx.int(n - 1), q])));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // (q; q)_inf/(q^z; q)_inf (1 - q)^(1 - z)
    let num = fx.call("q_pochhammer", &[q, q]);
    let den = fx.call("q_pochhammer", &[fx.pow(q, z), q]);
    Ok(Some(fx.mul(&[fx.div(num, den), fx.pow(fx.sub(fx.int(1), q), fx.sub(fx.int(1), z))])))
}

// ---------------------------------------------------------------- Jacobi relatives

/// The inverse Jacobi functions as F(phi | m) of the angle each one's value fixes (DLMF 22.15): `which` names the
/// function (the letters after inverse_jacobi_).
fn inverse_jacobi<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: &str) -> R<'h> {
    let (x, m) = (a[0], a[1]);
    let one = fx.int(1);
    // the points where the inverse is 0
    let zero_at_one = matches!(which, "cn" | "dn" | "cd" | "dc" | "nc" | "nd");
    if (zero_at_one && fx.is_int(x, 1)) || (matches!(which, "sc" | "sd") && fx.is_int(x, 0)) {
        return Ok(Some(fx.int(0)));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // an inexact m = 0 or 1: the functions are circular or hyperbolic (DLMF 22.5.3-4), K or K' is infinite and the
    // formulas below gave elliptic_f(indeterminate, 0.) or never returned (a plot over m through 1 timed out); the
    // functions constant at that m (dn and nd at 0, cd and dc at 1) have no inverse and stay unevaluated
    if let Some(mv) = fx.f64_of(m).filter(|&v| v == 0.0 || v == 1.0) {
        let inv = |x: T<'h>| fx.div(one, x);
        let e = match (mv == 0.0, which) {
            (true, "cn" | "cd") => fx.call("acos", &[x]),
            (true, "sd") => fx.call("asin", &[x]),
            (true, "dc" | "nc") => fx.call("acos", &[inv(x)]),
            (true, "sc") => fx.call("atan", &[x]),
            (true, "ns" | "ds") => fx.call("asin", &[inv(x)]),
            (true, "cs") => fx.call("atan", &[inv(x)]),
            (false, "cn" | "dn") => fx.call("acosh", &[inv(x)]),
            (false, "nc" | "nd") => fx.call("acosh", &[x]),
            (false, "sc" | "sd") => fx.call("asinh", &[x]),
            (false, "ns") => fx.call("atanh", &[inv(x)]),
            (false, "ds" | "cs") => fx.call("asinh", &[inv(x)]),
            _ => return Ok(None),
        };
        return numify(fx, e, a).map(Some);
    }
    // a real m > 1: the reciprocal-modulus transformation (DLMF 22.17.2-4 with k = sqrt(m)): sn(u | m) = sn(k u | 1/m)/k,
    // cn(u | m) = dn(k u | 1/m), dn(u | m) = cn(k u | 1/m), so each inverse at m is an inverse at 1/m < 1 divided by k,
    // and the real range of the function on the real line at m (dn and dc fill [-1, 1], cd and nd |v| >= 1) is carried
    // to the real range at 1/m, where the principal inverse is real. The formulas of DLMF 22.15 below take |v| only for
    // m > 1 (asin of sqrt((1 - v^2)/m) for dn), and the inverse came out complex on half the range (round 37:
    // inverse_jacobi_dn(-0.3, 3.) was 0.787 + 2.343i, the real inverse is 1.2149); inverse_jacobi_nd(-1, m) =
    // 2 K(1/m)/sqrt(m) for m > 1 is the documented value at the end point.
    if fx.f64_of(m).is_some_and(|v| v > 1.0) {
        let mu = fx.div(one, m);
        let k = fx.sqrt(m);
        let (other, arg) = match which {
            "cn" => ("dn", x),
            "dn" => ("cn", x),
            "cd" => ("dc", x),
            "dc" => ("cd", x),
            "nc" => ("nd", x),
            "nd" => ("nc", x),
            "sd" => ("sc", fx.mul(&[k, x])),
            "sc" => ("sd", fx.mul(&[k, x])),
            "ns" => ("ns", fx.div(x, k)),
            "ds" => ("cs", fx.div(x, k)),
            "cs" => ("ds", fx.div(x, k)),
            _ => return Ok(None),
        };
        let arg = numify(fx, arg, a)?;
        let mu = numify(fx, mu, a)?;
        let v = fx.eval(fx.call(&format!("inverse_jacobi_{other}"), &[arg, mu]))?;
        if fx.num(v).is_none() {
            return Ok(None);
        }
        return numify(fx, fx.div(v, k), a).map(Some);
    }
    let x2 = fx.mul(&[x, x]);
    let asin = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.call("asin", &[t]);
    let phi = match which {
        "cn" => fx.call("acos", &[x]),
        "dn" => asin(fx, fx.sqrt(fx.div(fx.sub(one, x2), m))),
        "cd" => asin(fx, fx.sqrt(fx.div(fx.sub(one, x2), fx.sub(one, fx.mul(&[m, x2]))))),
        "sd" => asin(fx, fx.div(x, fx.sqrt(fx.add(&[one, fx.mul(&[m, x2])])))),
        "nd" => asin(fx, fx.sqrt(fx.div(fx.sub(x2, one), fx.mul(&[m, x2])))),
        "dc" => asin(fx, fx.sqrt(fx.div(fx.sub(x2, one), fx.sub(x2, m)))),
        "nc" => fx.call("acos", &[fx.div(one, x)]),
        "sc" => fx.call("atan", &[x]),
        "ns" => asin(fx, fx.div(one, x)),
        "ds" => asin(fx, fx.div(one, fx.sqrt(fx.add(&[x2, m])))),
        "cs" => fx.call("atan", &[fx.div(one, x)]),
        _ => return Ok(None),
    };
    let u = fx.eval(fx.call("elliptic_f", &[phi, m]))?;
    // the formulas are those of the real case (DLMF 22.15) and the square roots and arcs of a complex argument may fall on the
    // wrong sheet: u is kept when the function maps it back to x, else the images under the symmetries u -> -u and
    // u -> 2K - u (each function of the list changes by a sign or not under them) are tried (round 32: cd(u) = -x for
    // x = 0.2 + i, m = 0.1 - 0.3 i)
    let forward = format!("jacobi_{which}");
    let back = |fx: &mut Fx<'_, '_, '_, 'h>, c: T<'h>| -> Res<Option<f64>> {
        let v = fx.eval(fx.call(&forward, &[c, m]))?;
        if fx.num(v).is_none() {
            return Ok(None);
        }
        let d = fx.eval(fx.sub(v, x))?;
        Ok(Some(mag(fx, d) / mag(fx, x).max(1.0)))
    };
    if fx.num(u).is_none() || back(fx, u)?.is_none_or(|e| e < 1e-6) {
        return Ok(Some(u));
    }
    let k2 = fx.eval(fx.mul(&[fx.int(2), fx.call("elliptic_k", &[m])]))?;
    // K' only when a shift by 2iK' is tried: for m < 0 or m > 1 it is a complex value from the ball path (20 ms)
    let mut kp2i: Option<T<'h>> = None;
    // the half periods 2K and 2iK' span the shifts (the functions have periods among 2K, 4K, 2iK', 4iK', 2K + 2iK')
    for (neg, a, b) in [(true, 1, 0), (true, 0, 0), (false, -1, 0), (true, -1, 0), (false, 1, 0), (false, 0, 1), (false, 0, -1), (true, 0, 1), (true, 0, -1), (false, 1, 1), (false, 1, -1), (false, -1, 1), (false, -1, -1), (true, 1, 1), (true, 1, -1), (true, -1, 1), (true, -1, -1)] {
        let base = if neg { fx.neg(u) } else { u };
        let mut terms = vec![base];
        if a != 0 {
            terms.push(fx.mul(&[fx.int(a), k2]));
        }
        if b != 0 {
            let kp = match kp2i {
                Some(k) => k,
                None => {
                    let k = fx.eval(fx.mul(&[fx.int(2), fx.i(), fx.call("elliptic_k", &[fx.sub(one, m)])]))?;
                    kp2i = Some(k);
                    k
                }
            };
            terms.push(fx.mul(&[fx.int(b), kp]));
        }
        let c = fx.eval(fx.add(&terms))?;
        if back(fx, c)?.is_some_and(|e| e < 1e-6) {
            return Ok(Some(c));
        }
    }
    Ok(Some(u))
}

macro_rules! inverse_jacobi_fns {
    ($($f:ident, $w:literal);*) => {
        $(fn $f<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
            inverse_jacobi(fx, a, $w)
        })*
    };
}
inverse_jacobi_fns!(ij_cn, "cn"; ij_dn, "dn"; ij_cd, "cd"; ij_sd, "sd"; ij_nd, "nd"; ij_dc, "dc"; ij_nc, "nc"; ij_sc, "sc"; ij_ns, "ns"; ij_ds, "ds"; ij_cs, "cs");

/// am(u | m) for real u and m < 1: the angle of (cn, sn), continued by 2 pi per period 4 K.
fn jacobi_amplitude<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (u, m) = (a[0], a[1]);
    if fx.is_int(u, 0) {
        return Ok(Some(fx.int(0)));
    }
    if fx.is_int(m, 0) {
        return Ok(Some(u));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    if fx.f64_of(m).is_some_and(|v| v >= 1.0) {
        return Ok(None);
    }
    if fx.f64_of(u).is_none() || fx.f64_of(m).is_none() {
        // complex arguments: am = -i log(cn + i sn), continued by 2 pi per period 4 K along
        // the real part
        let k = ev(fx, "n", &[fx.call("elliptic_k", &[m])])?;
        let sn = ev(fx, "jacobi_sn", &[u, m])?;
        let cn = ev(fx, "jacobi_cn", &[u, m])?;
        let turns = ev(fx, "round", &[fx.call("re", &[fx.div(u, fx.mul(&[fx.int(4), k]))])])?;
        let log = fx.call("log", &[fx.add(&[cn, fx.mul(&[fx.i(), sn])])]);
        return Ok(Some(fx.add(&[fx.mul(&[fx.neg(fx.i()), log]), fx.mul(&[fx.int(2), turns, fx.sym("pi")])])));
    }
    // K numerically: an exact m (2/3) leaves elliptic_k(m) unevaluated
    let k = ev(fx, "n", &[fx.call("elliptic_k", &[m])])?;
    let (Some(uf), Some(kf)) = (fx.f64_of(u), fx.f64_of(k)) else { return Ok(None) };
    let turns = (uf / (4.0 * kf)).round();
    let sn = fx.call("jacobi_sn", &[u, m]);
    let cn = fx.call("jacobi_cn", &[u, m]);
    let angle = fx.call("arg", &[fx.add(&[cn, fx.mul(&[fx.i(), sn])])]);
    Ok(Some(fx.add(&[angle, fx.mul(&[fx.int(2 * turns as i64), fx.sym("pi")])])))
}

fn jacobi_zn<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (u, m) = (a[0], a[1]);
    if fx.is_int(u, 0) || fx.is_int(m, 0) {
        return Ok(Some(fx.int(0)));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // Z(u | m) = E(am(u) | m) - E(m) u/K(m)
    let am = fx.call("jacobi_amplitude", &[u, m]);
    let e = fx.call("elliptic_e", &[am, m]);
    let ratio = fx.div(fx.call("elliptic_e", &[m]), fx.call("elliptic_k", &[m]));
    Ok(Some(fx.sub(e, fx.mul(&[ratio, u]))))
}

fn jacobi_epsilon<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (u, m) = (a[0], a[1]);
    if fx.is_int(u, 0) {
        return Ok(Some(fx.int(0)));
    }
    if fx.is_int(m, 0) {
        return Ok(Some(u));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    Ok(Some(fx.call("elliptic_e", &[fx.call("jacobi_amplitude", &[u, m]), m])))
}

// ---------------------------------------------------------------- Bessel relatives

fn hankel<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], sign: i64) -> R<'h> {
    if !numeric(fx, a) {
        return Ok(None);
    }
    let j = fx.call("spherical_bessel_j", a);
    let y = fx.call("spherical_bessel_y", a);
    Ok(Some(fx.add(&[j, fx.mul(&[fx.int(sign), fx.i(), y])])))
}
fn spherical_hankel_h1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    hankel(fx, a, 1)
}
fn spherical_hankel_h2<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    hankel(fx, a, -1)
}

/// The Kelvin functions of order nu: ber + i bei = J_nu(x e^(3 pi i/4)), ker + i kei = e^(-nu pi i/2) K_nu(x
/// e^(pi i/4)) (DLMF 10.61.1-2), each part as the half sum (difference) with the conjugate-direction value, which
/// extends them analytically off the positive axis (K_nu is the principal Bessel K).
fn kelvin<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: u8) -> R<'h> {
    let (nu, x) = if a.len() == 1 { (fx.int(0), a[0]) } else { (a[0], a[1]) };
    if fx.is_int(x, 0) && fx.is_int(nu, 0) && which < 2 {
        return Ok(Some(fx.int(if which == 0 { 1 } else { 0 })));
    }
    // ber_nu(0) = bei_nu(0) = 0 for nu > 0 (the factor x^nu of the series, DLMF 10.61.5); ker(0) = +inf and
    // kei(0) = -pi/4 for nu = 0 (DLMF 10.65.9, 10.65.10); exact arguments only (D23-26)
    if fx.is_int(x, 0) && fx.rat_of(nu).is_some_and(|q| !q.num.neg && !q.is_zero()) && which < 2 {
        return Ok(Some(fx.int(0)));
    }
    // a negative integer order: J_(-n) = (-1)^n J_n vanishes at 0 as well, so ber_(-n)(0) = bei_(-n)(0) = 0
    if fx.is_int(x, 0) && which < 2 && fx.int_of(nu).is_some_and(|n| n < 0) {
        return Ok(Some(fx.int(0)));
    }
    if which >= 2 && fx.num(x).is_some_and(|n| n.is_zero()) {
        if let Some(v) = kelvin_k_at_zero(fx, nu, which == 3) {
            // an inexact zero gives the value its kind and precision (v + 0. is 0.5, inf + 0. is inf); an inexact order
            // with an exact zero, a machine value
            let out = if !fx.is_int(x, 0) {
                fx.eval(fx.add(&[v, x]))?
            } else if numeric(fx, a) {
                fx.eval(fx.call("n", &[v]))?
            } else {
                v
            };
            return Ok(Some(out));
        }
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // ker and kei decay like e^(-x/sqrt(2)): a machine argument beyond 1100 underflows to 0 (the machine value of
    // the true number, below 1e-324), as exp(-1100.) does
    if which >= 2 && fx.machine_of(x).is_some_and(|v| v > 1100.0) && fx.f64_of(nu).is_some_and(|v| v.abs() <= 100.0) {
        return Ok(Some(fx.real(0.0)));
    }
    // ber and bei of a machine x > 1000: |value| ~ e^(x/sqrt 2)/sqrt(2 pi x) overflows; the sign is that of cos(alpha) (ber)
    // or sin(alpha) (bei), alpha = x/sqrt 2 + (nu/2 - 1/8) pi (DLMF 10.67.1), alpha reduced in double-double; a sign too
    // near zero to trust leaves the call unevaluated (round 27, B26-22: kelvin_ber(2, 75069.9) took 13 s and gave up)
    if which < 2 && fx.f64_of(nu).is_some_and(|v| v.abs() <= 100.0) {
        if let Some(xm) = fx.machine_of(x).filter(|v| *v > 1100.0) {
            use crate::dd::{sin_cos_pi_any, DD, PI};
            use std::ops::Mul;
            let nm = fx.f64_of(nu).unwrap_or(0.0);
            // alpha/pi = x/(sqrt 2 pi) + nu/2 - 1/8, with 1/(sqrt 2 pi) in double-double
            let inv = DD::new(2.0).sqrt().mul(PI).recip();
            let half_turns = inv.mul_f(xm).add_f(nm * 0.5 - 0.125);
            let (sn, cs) = sin_cos_pi_any(half_turns);
            let v = if which == 0 { cs.to_f64() } else { sn.to_f64() };
            if v.abs() > 1e-3 {
                return Ok(Some(fx.real(f64::INFINITY.copysign(v))));
            }
            return Ok(None);
        }
    }
    let pi = fx.sym("pi");
    let rot = |fx: &Fx<'_, '_, '_, 'h>, k: i64| fx.call("exp", &[fx.mul(&[fx.rat(k, 4), fx.i(), pi])]);
    let (p, m) = if which < 2 {
        // ber_nu + i bei_nu = (z/2)^nu sum e^((3 nu/4 + k/2) pi i) (z^2/4)^k/(k! gamma(nu + k + 1)) (DLMF 10.61.5)
        // is J_nu(z e^(3 pi i/4)) when the principal power of the rotated argument is (z/2)^nu e^(3 nu pi i/4),
        // i.e. for arg z in (-3 pi/4, pi/4]; elsewhere the principal power of z e^(3 pi i/4) differs by
        // e^(-2 pi i nu), so J_nu is multiplied by e^(2 pi i nu k) (k = 1 for arg z > pi/4), and likewise for the
        // conjugate direction (k = 1 for arg z <= -pi/4, factor e^(-2 pi i nu k)). Integer orders need none.
        let theta = match fx.num(x) {
            Some(mc_arith::Num::Real(r)) => if r.to_f64() < 0.0 { std::f64::consts::PI } else { 0.0 },
            Some(mc_arith::Num::Complex(re, im)) => im.to_f64().atan2(re.to_f64()),
            None => 0.0,
        };
        let phase = |fx: &Fx<'_, '_, '_, 'h>, s: i64| fx.call("exp", &[fx.mul(&[fx.int(2 * s), nu, fx.i(), pi])]);
        let integer_order = fx.int_of(nu).is_some();
        let kp = !integer_order && theta > std::f64::consts::FRAC_PI_4;
        let km = !integer_order && theta <= -std::f64::consts::FRAC_PI_4;
        let jp = fx.call("bessel_j", &[nu, fx.mul(&[x, rot(fx, 3)])]);
        let jm = fx.call("bessel_j", &[nu, fx.mul(&[x, rot(fx, -3)])]);
        (
            if kp { fx.mul(&[phase(fx, 1), jp]) } else { jp },
            if km { fx.mul(&[phase(fx, -1), jm]) } else { jm },
        )
    } else {
        let e = |fx: &Fx<'_, '_, '_, 'h>, s: i64| fx.call("exp", &[fx.mul(&[fx.rat(s, 2), nu, fx.i(), pi])]);
        // the principal K_nu(w) (analytic off the negative real axis, so ker and kei continue to Re x < 0 with
        // their cut where x e^(+-pi i/4) is a negative real number)
        // beyond |arg x| = 3 pi/4 the rotated argument crosses the cut of the principal K: continue by DLMF 10.34.2
        // with m = +-2: K(w e^(2 pi i)) = e^(-2 nu pi i) K(w) - 2 pi i cos(nu pi) I(w)
        let theta = match fx.num(x) {
            Some(mc_arith::Num::Real(r)) => if r.to_f64() < 0.0 { std::f64::consts::PI } else { 0.0 },
            Some(mc_arith::Num::Complex(re, im)) => im.to_f64().atan2(re.to_f64()),
            None => 0.0,
        };
        let k = |fx: &Fx<'_, '_, '_, 'h>, w: T<'h>, corr: i64| {
            let kp = fx.call("bessel_k", &[nu, w]);
            if corr == 0 {
                return kp;
            }
            let ph = fx.call("exp", &[fx.mul(&[fx.int(-corr), nu, fx.i(), pi])]);
            let cs = fx.call("cos", &[fx.mul(&[nu, pi])]);
            let corr_term = fx.mul(&[fx.int(-corr), pi, fx.i(), cs, fx.call("bessel_i", &[nu, w])]);
            fx.add(&[fx.mul(&[ph, kp]), corr_term])
        };
        let (cf, cg) = (
            if theta > 0.75 * std::f64::consts::PI { 2 } else { 0 },
            if theta < -0.75 * std::f64::consts::PI { -2 } else { 0 },
        );
        (
            fx.mul(&[e(fx, -1), k(fx, fx.mul(&[x, rot(fx, 1)]), cf)]),
            fx.mul(&[e(fx, 1), k(fx, fx.mul(&[x, rot(fx, -1)]), cg)]),
        )
    };
    let v = if which % 2 == 0 {
        fx.mul(&[fx.rat(1, 2), fx.add(&[p, m])])
    } else {
        fx.div(fx.sub(p, m), fx.mul(&[fx.int(2), fx.i()]))
    };
    // ber, bei, ker and kei of a real x are real for x > 0 and, for ber and bei, at every real x when the order is an
    // integer (J_n(x e^(3 pi i/4)) with x < 0 is the conjugate direction, whose value is real up to the factor (-1)^n)
    let real = [nu, x].iter().all(|&t| fx.f64_of(t).is_some())
        && fx.f64_of(x).is_some_and(|v| v > 0.0 || (which < 2 && fx.int_of(nu).is_some()));
    // the rotation phases carry pi: evaluate the closed form at the precision of the arguments (D23-26)
    let v = numify(fx, v, a)?;
    // a real value computed as a complex ball: its imaginary part is 0 +- 2^-p and cannot reach any relative precision,
    // so `n` reports precision_not_reached with the best value; the real part of that value is the answer (B26-17)
    let v = match fx.call_of(v, "failure") {
        Some(&[tag, best]) if real && tag == fx.sym("precision_not_reached") && fx.num(best).is_some() => best,
        _ => v,
    };
    // an overflow of the Bessel functions of the rotated argument (ber(1e5) is about e^70710): the sign of the
    // value is not known, so the call stays unevaluated rather than giving `indeterminate`
    let out = if real { fx.call("re", &[v]) } else { v };
    if fx.machine_of(x).is_some() {
        let value = fx.eval(out)?;
        if value == fx.sym("indeterminate") || fx.machine_of(value).is_some_and(|v| v.is_nan()) {
            return Ok(None);
        }
        // the parts p and m are machine values with an error relative to their size; a value far below them (near x = 0
        // the terms of size x^(-n) cancel) has lost that many bits (round 37: kelvin_ker(2, 1e-6) was 0.49988, the value
        // is 0.4999999999999018, kelvin_ber(2, 1e-6) was 0.2 % off): then the closed form again with the arguments'
        // binary values as balls, at the digits the loss asks for, until the value has the 53 bits of a double
        let size = |fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>| -> Res<f64> {
            let e = fx.call("n", &[t]);
            let v = fx.eval(e)?;
            Ok(mag(fx, v))
        };
        let parts_size = size(fx, p)?.max(size(fx, m)?);
        let vm = mag(fx, value);
        let loss = if vm > 0.0 { (parts_size / vm).log2() } else { f64::INFINITY };
        if fx.num(value).is_some() && loss.is_finite() && loss <= 4.0 {
            return Ok(Some(value));
        }
        let first = if loss.is_finite() { ((53.0 + loss) / 3.3) as i64 + 8 } else { 40 };
        for d in [first, 2 * first, 4 * first] {
            if d > 1000 {
                break;
            }
            let mut ad = Vec::with_capacity(a.len());
            for &t in a {
                let exact = fx.num(t).is_some_and(|n| n.kind() == Kind::Exact);
                ad.push(if exact { t } else { fx.eval(fx.call("set_precision", &[t, fx.int(d)]))? });
            }
            let Some(hi) = kelvin(fx, &ad, which)? else { return Ok(None) };
            let hi = fx.eval(hi)?;
            let hi = match fx.call_of(hi, "failure") {
                Some(&[tag, best]) if tag == fx.sym("precision_not_reached") => best,
                _ => hi,
            };
            if fx.num(hi).is_none() {
                return Ok(None);
            }
            if has_bits(fx, hi, 57) {
                let v = fx.eval(fx.call("n", &[hi]))?;
                return Ok(fx.num(v).is_some().then_some(v));
            }
            fx.tick(1)?;
        }
        return Ok(None);
    }
    Ok(Some(out))
}
/// ker_n(0) or kei_n(0) (`kei`) at an integer order n, as the limit x -> 0+ (round 37: ker_2(0.) was -inf, kei_0(0.)
/// was 0). ker_n + i kei_n = e^(-n pi i/2) K_n(x e^(pi i/4)) and K_(-n) = K_n give the factor (-1)^n for a negative
/// order; for n >= 1, K_n(w) = (1/2) sum_(k<n) ((n - k - 1)!/k!) (w/2)^(2k - n) (-1)^k + O(w^n log w) (DLMF 10.31.1), so
/// the term k contributes (1/2) ((n - k - 1)!/k!) (2/x)^(n - 2k) e^(i pi m/4) with m = 6k - 3n: the first k < n/2 whose
/// real (ker) or imaginary (kei) part of e^(i pi m/4) is not 0 gives the infinity of its sign; when none is, the term
/// k = n/2 (n even, m = 0) gives ker the value 1/n and kei 0. Order 0: ker_0(0) = inf, kei_0(0) = -pi/4 (DLMF
/// 10.65.9-10). `None` for an order that is not an integer.
fn kelvin_k_at_zero<'h>(fx: &Fx<'_, '_, '_, 'h>, nu: T<'h>, kei: bool) -> Option<T<'h>> {
    let n = match fx.int_of(nu) {
        Some(n) => n,
        None => {
            let v = fx.f64_of(nu)?;
            if v.fract() != 0.0 || v.abs() > 1e9 {
                return None;
            }
            v as i64
        }
    };
    if n == 0 {
        return Some(if kei { fx.neg(fx.mul(&[fx.rat(1, 4), fx.sym("pi")])) } else { fx.sym("inf") });
    }
    let sign = if n < 0 && n % 2 != 0 { -1 } else { 1 };
    let n = n.abs();
    // the sign of cos(pi m/4) (ker) or sin(pi m/4) (kei); 0 where it vanishes
    let part = |m: i64| -> i64 {
        let r = m.rem_euclid(8);
        if kei {
            match r {
                1..=3 => 1,
                5..=7 => -1,
                _ => 0,
            }
        } else {
            match r {
                7 | 0 | 1 => 1,
                3..=5 => -1,
                _ => 0,
            }
        }
    };
    // 6k mod 8 has period 4 in k: four terms decide
    let mut k = 0;
    while 2 * k < n && k < 4 {
        let s = part(6 * k - 3 * n);
        if s != 0 {
            return Some(if s * sign > 0 { fx.sym("inf") } else { fx.neg(fx.sym("inf")) });
        }
        k += 1;
    }
    Some(if kei || n % 2 != 0 { fx.int(0) } else { fx.rat(sign, n) })
}

fn kelvin_ber<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    kelvin(fx, a, 0)
}
fn kelvin_bei<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    kelvin(fx, a, 1)
}
fn kelvin_ker<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    kelvin(fx, a, 2)
}
fn kelvin_kei<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    kelvin(fx, a, 3)
}

/// Is a Maclaurin series in z too long to sum (about 2|z| terms at 1.5|z| extra bits, a cost growing like |z|^2)? The
/// Struve, Anger, Weber and Lommel sums then stay unevaluated rather than allocate without bound (round 32).
pub(crate) fn series_too_long<'h>(fx: &Fx<'_, '_, '_, 'h>, z: T<'h>) -> bool {
    mag(fx, z) > 2500.0
}

/// The bits a series of size e^|z| loses to cancellation, plus a margin.
pub(crate) fn cancel_bits<'h>(fx: &Fx<'_, '_, '_, 'h>, z: T<'h>, scale: f64) -> u64 {
    (mag(fx, z) * scale * std::f64::consts::LOG2_E) as u64 + 8
}

/// Struve H (`l` false) or L: sum (-+1)^k (z/2)^(2k+nu+1)/(Gamma(k + 3/2) Gamma(k + nu + 3/2)).
fn struve<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], l: bool) -> R<'h> {
    let (nu, z) = (a[0], a[1]);
    if fx.is_int(z, 0) && fx.f64_of(nu).is_some_and(|v| v > -1.0) {
        return Ok(Some(fx.int(0)));
    }
    // no limit at complex infinity: the functions grow like e^|Im z| or oscillate along different rays
    if z == fx.sym("complex_infinity") && fx.num(nu).is_some() {
        return Ok(Some(fx.sym("indeterminate")));
    }
    if !numeric(fx, a) {
        return Ok(struve_half_integer(fx, nu, z, l));
    }
    // H_nu(x) at a machine real x >= 40 + nu^2: Y_nu(x) + (1/pi) sum_k Gamma(k + 1/2) (x/2)^(nu - 2k - 1)/Gamma(nu + 1/2 - k)
    // (DLMF 11.6.1 and 11.2.5; the sum of the series of the Maclaurin form cancels e^x-like terms: H_(1/3)(309994.9)
    // did not return, round 27, B26-22), terms until below 1e-17 or growing
    if !l {
        if let (Some(nm), Some(xm)) = (fx.f64_of(nu), fx.machine_of(z)) {
            if xm >= 40.0 + nm * nm && nm.abs() <= 30.0 {
                let w = nm + 0.5;
                let gw = crate::machine::gamma::gamma(w);
                let y = crate::machine::bessel_j::bessel_y(nm, xm);
                if gw.is_finite() && gw != 0.0 && y.is_finite() {
                    let inv_h2 = 4.0 / (xm * xm);
                    let mut t = std::f64::consts::PI.sqrt() * (0.5 * xm).powf(nm - 1.0) / gw / std::f64::consts::PI;
                    let mut sum = t;
                    let mut ok = false;
                    for k in 0..300 {
                        let kf = k as f64;
                        let next = t * (kf + 0.5) * (w - kf - 1.0) * inv_h2;
                        if next.abs() > t.abs() && k > 2 {
                            break;
                        }
                        t = next;
                        sum += t;
                        if t.abs() <= 1e-17 * (sum.abs() + y.abs()) || t == 0.0 {
                            ok = true;
                            break;
                        }
                    }
                    if ok {
                        return Ok(Some(fx.real(y + sum)));
                    }
                }
            }
        }
    }
    if let Some(v) = struve_machine(fx, a, l)? {
        return Ok(Some(v));
    }
    if series_too_long(fx, z) {
        return Ok(None);
    }
    let extra = cancel_bits(fx, z, 1.0);
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let (nu, z) = (xs[0], xs[1]);
    let h = fx.div(z, fx.int(2));
    let h2 = fx.mul(&[h, h]);
    let lead = evpow(fx, h, fx.add(&[nu, fx.int(1)]))?;
    let n0 = (mag(fx, z) * 2.0) as usize + 8;
    let three_halves = ballify(fx, fx.rat(3, 2), wk.w);
    let Some(r1) = rgamma_seq(fx, three_halves, n0 + 4)? else { return Ok(None) };
    let Some(r2) = rgamma_seq(fx, fx.add(&[nu, three_halves]), n0 + 4)? else { return Ok(None) };
    let (mut r1, mut r2) = (r1, r2);
    let mut p = fx.int(1);
    let s = sum_series(fx, wk.w, n0, |fx, k| {
        while r1.len() <= k {
            // extend the reciprocal gamma sequences by their recurrences
            let x1 = fx.add(&[three_halves, fx.int(r1.len() as i64 - 1)]);
            let x2 = fx.add(&[nu, three_halves, fx.int(r2.len() as i64 - 1)]);
            r1.push(fx.div(r1[r1.len() - 1], x1));
            r2.push(fx.div(r2[r2.len() - 1], x2));
        }
        let t = fx.mul(&[p, r1[k], r2[k]]);
        p = fx.mul(&[p, h2]);
        if !l {
            p = fx.neg(p);
        }
        Ok(Some(t))
    })?;
    let Some(s) = s else { return Ok(None) };
    let v = fx.mul(&[lead, s]);
    let real = wk.real && parts(fx, z).is_some_and(|p| p.0 >= 0.0);
    Ok(wk.out(fx, v, real))
}
/// H_nu(z) or L_nu(z) at machine arguments (round 37): s_(nu,nu)(z)/c or t_(nu,nu)(z)/c with c = sqrt(pi) Gamma(nu +
/// 1/2) 2^(nu - 1) (DLMF 11.9.7), the Lommel series in the double-double kernels of `fast29` and c from a 40-digit
/// ball; the ball path took 0.3-1 ms a value and the plots of Lommel derivatives (Struve terms) timed out. `None` when
/// a kernel declines (cancellation, a pole) or c is not a finite nonzero number.
fn struve_machine<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], l: bool) -> R<'h> {
    let Some((ms, real)) = crate::fast29::machine_args(fx, a) else { return Ok(None) };
    let (nu, z) = (ms[0], ms[1]);
    let s = if l { crate::fast29::lommel_t1_cd(nu, nu, z) } else { crate::fast29::lommel_s1_cd(nu, nu, z) };
    let Some(s) = s else { return Ok(None) };
    // c depends on the order only: the last one is kept (a plot evaluates one order at many points)
    type OrderKey = (f64, f64, f64, f64);
    thread_local! {
        static LAST_C: std::cell::Cell<Option<(OrderKey, crate::cdd::CD)>> = const { std::cell::Cell::new(None) };
    }
    let key = (nu.re.hi, nu.re.lo, nu.im.hi, nu.im.lo);
    if let Some((k, cd)) = LAST_C.with(|c| c.get()) {
        if k == key {
            let v = (s / cd).to_c64();
            return Ok(crate::fast29::machine_out(fx, v, real && z.re.hi >= 0.0));
        }
    }
    // c at 40 digits from the exact (or exactly converted machine) order
    let nu_t = fx.eval(fx.call("set_precision", &[a[0], fx.int(40)]))?;
    let c = fx.eval(fx.mul(&[fx.pow(fx.sym("pi"), fx.rat(1, 2)), fx.call("gamma", &[fx.add(&[nu_t, fx.rat(1, 2)])]), fx.pow(fx.int(2), fx.sub(nu_t, fx.int(1)))]))?;
    let cd = match fx.num(c) {
        Some(Num::Real(Real::Ball(b, _))) if !b.contains_zero() => crate::cdd::CD::real(crate::fast29::dd_of_ball(&b)),
        Some(Num::Complex(Real::Ball(x, _), Real::Ball(y, _))) => crate::cdd::CD::new(crate::fast29::dd_of_ball(&x), crate::fast29::dd_of_ball(&y)),
        _ => return Ok(None),
    };
    if cd.abs() == 0.0 || !cd.finite() {
        return Ok(None);
    }
    LAST_C.with(|c| c.set(Some((key, cd))));
    let v = (s / cd).to_c64();
    Ok(crate::fast29::machine_out(fx, v, real && z.re.hi >= 0.0))
}

/// H_nu(z) (`l` false) or L_nu(z) at a half-integer order |nu| <= 21/2 in elementary functions: H_(1/2) =
/// sqrt(2/(pi z)) (1 - cos z), H_(-1/2) = sqrt(2/(pi z)) sin z, L_(1/2) = sqrt(2/(pi z)) (cosh z - 1), L_(-1/2) =
/// sqrt(2/(pi z)) sinh z (DLMF 11.4.5-11.4.8), then the recurrences H_(nu+1) = (2 nu/z) H_nu - H_(nu-1) + c_nu and
/// L_(nu+1) = L_(nu-1) - (2 nu/z) L_nu - c_nu with c_nu = (z/2)^nu/(sqrt(pi) Gamma(nu + 3/2)) (DLMF 11.4.23, 11.4.25),
/// run downwards for negative orders.
fn struve_half_integer<'h>(fx: &mut Fx<'_, '_, '_, 'h>, nu: T<'h>, z: T<'h>, l: bool) -> Option<T<'h>> {
    let q = fx.rat_of(nu)?;
    let two_nu = q.mul(&Rat::from_i64(2));
    if !two_nu.is_int() || q.is_int() {
        return None;
    }
    let k2 = fx.int_of(fx.q(&two_nu))?; // 2 nu, odd
    if k2.abs() > 21 || fx.num(z).is_some_and(|n| n.is_zero()) {
        return None;
    }
    // sqrt(2/pi) z^(-1/2), not (2/(pi z))^(1/2): the two differ in sign on the negative axis, and the principal branch
    // H_nu(z) = (z/2)^(nu + 1) (an even series) takes the power z^(-1/2) there, as the terms c_nu below do (round 37:
    // H_(5/2)(-17/10) was -2.18i, the series gives -0.0944i)
    let pre = fx.mul(&[fx.pow(fx.int(2), fx.rat(1, 2)), fx.pow(fx.sym("pi"), fx.rat(-1, 2)), fx.pow(z, fx.rat(-1, 2))]);
    let (c, s) = if l { (fx.call("cosh", &[z]), fx.call("sinh", &[z])) } else { (fx.call("cos", &[z]), fx.call("sin", &[z])) };
    let p_half = if l { fx.mul(&[pre, fx.sub(c, fx.int(1))]) } else { fx.mul(&[pre, fx.sub(fx.int(1), c)]) };
    let m_half = fx.mul(&[pre, s]);
    // 1/Gamma(nu + 3/2) vanishes at nu = -3/2, -5/2, ...: the term is 0 there, not a pole (H_(-7/2) stepped down through
    // nu = -3/2 gave failure(pole, gamma(0)))
    let cnu = |fx: &Fx<'_, '_, '_, 'h>, v2: i64| {
        if v2 <= -3 {
            return fx.int(0);
        }
        let v = fx.rat(v2, 2);
        let g = fx.call("gamma", &[fx.add(&[v, fx.rat(3, 2)])]);
        fx.div(fx.pow(fx.div(z, fx.int(2)), v), fx.mul(&[fx.pow(fx.sym("pi"), fx.rat(1, 2)), g]))
    };
    // (f_(v-1), f_v) at v = 1/2, stepped to the order
    let (mut lo, mut hi) = (m_half, p_half);
    if k2 > 0 {
        let mut v2 = 1; // 2 v
        while v2 < k2 {
            let t = fx.mul(&[fx.rat(v2, 1), fx.pow(z, fx.int(-1)), hi]);
            let next = if l { fx.sub(fx.sub(lo, t), cnu(fx, v2)) } else { fx.add(&[fx.sub(t, lo), cnu(fx, v2)]) };
            lo = hi;
            hi = next;
            v2 += 2;
        }
        Some(fx.call("expand", &[hi]))
    } else {
        // downwards: f_(v-1) from f_v and f_(v+1): H_(v-1) = (2v/z) H_v - H_(v+1) + c_v, L_(v-1) = L_(v+1) + (2v/z) L_v + c_v
        let (mut up, mut cur) = (p_half, m_half); // f_(1/2), f_(-1/2)
        let mut v2 = -1; // 2 v of cur
        while v2 > k2 {
            let t = fx.mul(&[fx.rat(v2, 1), fx.pow(z, fx.int(-1)), cur]);
            let next = if l { fx.add(&[up, t, cnu(fx, v2)]) } else { fx.add(&[fx.sub(t, up), cnu(fx, v2)]) };
            up = cur;
            cur = next;
            v2 -= 2;
        }
        Some(fx.call("expand", &[cur]))
    }
}

fn struve_h<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    struve(fx, a, false)
}
fn struve_l<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    struve(fx, a, true)
}

/// Anger J (`weber` false) or Weber E of order nu (DLMF 11.10.1-4 rearranged):
/// S1 = sum (-1)^k (z/2)^(2k)/(Gamma(k + nu/2 + 1) Gamma(k - nu/2 + 1)),
/// S2 = sum (-1)^k (z/2)^(2k+1)/(Gamma(k + nu/2 + 3/2) Gamma(k - nu/2 + 3/2)),
/// J = cos(pi nu/2) S1 + sin(pi nu/2) S2, E = sin(pi nu/2) S1 - cos(pi nu/2) S2.
fn anger_weber<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], weber: bool) -> R<'h> {
    if a.len() != 2 || !numeric(fx, a) {
        return Ok(None);
    }
    if series_too_long(fx, a[1]) {
        return Ok(None);
    }
    let extra = cancel_bits(fx, a[1], 1.0);
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let (nu, z) = (xs[0], xs[1]);
    let h = fx.div(z, fx.int(2));
    let h2 = fx.neg(fx.mul(&[h, h]));
    let half = fx.div(nu, fx.int(2));
    let n0 = (mag(fx, z) * 2.0) as usize + 8;
    let mut sums = vec![];
    for (shift, lead) in [(ballify(fx, fx.int(1), wk.w), fx.int(1)), (ballify(fx, fx.rat(3, 2), wk.w), h)] {
        let n = n0 + 2 * wk.w as usize / 4 + 8;
        let Some(ra) = rgamma_seq(fx, fx.add(&[half, shift]), n)? else { return Ok(None) };
        let Some(rb) = rgamma_seq(fx, fx.sub(shift, half), n)? else { return Ok(None) };
        let mut p = lead;
        let s = sum_series(fx, wk.w, n0, |fx, k| {
            if k >= ra.len() {
                return Ok(None);
            }
            let t = fx.mul(&[p, ra[k], rb[k]]);
            p = fx.mul(&[p, h2]);
            Ok(Some(t))
        })?;
        let Some(s) = s else { return Ok(None) };
        sums.push(s);
    }
    let pi = pi_at(fx, wk.w)?;
    let ang = fx.mul(&[pi, half]);
    let (c, s) = (ev(fx, "cos", &[ang])?, ev(fx, "sin", &[ang])?);
    let v = if weber {
        fx.sub(fx.mul(&[s, sums[0]]), fx.mul(&[c, sums[1]]))
    } else {
        fx.add(&[fx.mul(&[c, sums[0]]), fx.mul(&[s, sums[1]])])
    };
    Ok(wk.out(fx, v, wk.real))
}
fn anger_j<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    anger_weber(fx, a, false)
}
fn weber_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    anger_weber(fx, a, true)
}

/// The Lommel functions in closed form at symbolic arguments (round 37), for `which` 0 = s, 1 = S, 2 = t, 3 = T:
///
/// - mu = nu: s = c H_nu, S = c (H_nu - Y_nu), t = c L_nu, T = c (I_(-nu) - L_nu) with c = pi (1/2)_nu 2^(nu - 1)
///   (DLMF 11.9.7 for s, with 11.9.5 for S; the modified functions alike, T by I_(-nu) = I_nu + 2 sin(nu pi)/pi K_nu,
///   DLMF 10.27.2); t depends on nu^2 only, so mu = -nu gives t the same value;
/// - mu = 0 and mu = -1 (s and S) at a symbolic order: s_(0,nu) = (pi/2)(tan(nu pi/2) J_nu - E_nu) and s_(-1,nu) = -(pi/(2 nu))(cot(nu
///   pi/2) J_nu + E_nu) with the Anger and Weber functions (Watson, *Theory of Bessel functions*, 10.71), S by
///   DLMF 11.9.5; not where these are poles (nu an odd integer, resp. an even one);
/// - S with mu + nu or mu - nu an odd positive integer: the terminating expansion z^(mu-1) sum_k (-1)^k prod_(i<=k)
///   ((mu - 2i + 1)^2 - nu^2) z^(-2k) (DLMF 11.9.9, Watson 10.75), exact for every z.
///
/// `None` otherwise (the numerical paths take numbers).
pub(crate) fn lommel_exact<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: u8) -> R<'h> {
    let (mu, nu, z) = (a[0], a[1], a[2]);
    if numeric(fx, a) {
        return Ok(None);
    }
    let pi = fx.sym("pi");
    let same = fx.eval(fx.sub(mu, nu))? == fx.int(0);
    let opposite = fx.eval(fx.add(&[mu, nu]))? == fx.int(0);
    let c = |fx: &Fx<'_, '_, '_, 'h>| fx.mul(&[pi, fx.call("pochhammer", &[fx.rat(1, 2), nu]), fx.pow(fx.int(2), fx.sub(nu, fx.int(1)))]);
    if same || (which == 2 && opposite) {
        let order = if same { nu } else { mu };
        let c = if same { c(fx) } else { fx.mul(&[pi, fx.call("pochhammer", &[fx.rat(1, 2), mu]), fx.pow(fx.int(2), fx.sub(mu, fx.int(1)))]) };
        let f = |n: &str, o: T<'h>| fx.call(n, &[o, z]);
        let body = match which {
            0 => f("struve_h", order),
            1 => fx.sub(f("struve_h", order), f("bessel_y", order)),
            2 => f("struve_l", order),
            _ => fx.sub(f("bessel_i", fx.neg(order)), f("struve_l", order)),
        };
        return Ok(Some(fx.mul(&[c, body])));
    }
    // exact parameters: the terminating expansion of S
    if which == 1 {
        if let (Some(m), Some(v)) = (fx.rat_of(mu), fx.rat_of(nu)) {
            let one = Rat::from_i64(1);
            let odd_pos = |s: &Rat| s.is_int() && !s.num.neg && s.num.mag.bit(0);
            if odd_pos(&m.add(&v)) || odd_pos(&m.sub(&v)) {
                let v2 = v.mul(&v);
                let mut coef = one.clone();
                let mut terms = vec![];
                for k in 0..10_000i64 {
                    if k % 64 == 63 {
                        fx.tick(1)?;
                    }
                    let pw = fx.pow(z, fx.q(&m.sub(&one).sub(&Rat::from_i64(2 * k))));
                    terms.push(fx.mul(&[fx.q(&coef), pw]));
                    let b = m.sub(&Rat::from_i64(2 * k + 1));
                    let f = b.mul(&b).sub(&v2);
                    if f.is_zero() {
                        return Ok(Some(fx.add(&terms)));
                    }
                    coef = coef.mul(&f).neg();
                }
            }
        }
    }
    // (a symbolic order only: at a numeric order the series of s is the faster and the more accurate evaluation, and a
    // closed form through E_nu and J_nu would make every later numerical value slower)
    if which <= 1 && fx.num(nu).is_none() {
        let int_parity = |t: T<'h>, odd: bool| fx.int_of(t).is_some_and(|n| (n % 2 != 0) == odd);
        let half = fx.mul(&[fx.rat(1, 2), nu, pi]);
        let (aj, we) = (fx.call("anger_j", &[nu, z]), fx.call("weber_e", &[nu, z]));
        let s = if fx.is_int(mu, 0) && !int_parity(nu, true) {
            Some(fx.mul(&[fx.rat(1, 2), pi, fx.sub(fx.mul(&[fx.call("tan", &[half]), aj]), we)]))
        } else if fx.is_int(mu, -1) && !int_parity(nu, false) {
            let k = fx.neg(fx.div(pi, fx.mul(&[fx.int(2), nu])));
            Some(fx.mul(&[k, fx.add(&[fx.mul(&[fx.call("cot", &[half]), aj]), we])]))
        } else {
            None
        };
        if let Some(s) = s {
            if which == 0 {
                return Ok(Some(s));
            }
            // S = s + 2^(mu - 1) Gamma((mu - nu + 1)/2) Gamma((mu + nu + 1)/2) (sin((mu - nu) pi/2) J_nu - cos((mu - nu) pi/2) Y_nu)
            let g = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.call("gamma", &[fx.mul(&[fx.rat(1, 2), t])]);
            let pre = fx.mul(&[fx.pow(fx.int(2), fx.sub(mu, fx.int(1))), g(fx, fx.add(&[fx.sub(mu, nu), fx.int(1)])), g(fx, fx.add(&[mu, nu, fx.int(1)]))]);
            let ang = fx.mul(&[fx.rat(1, 2), pi, fx.sub(mu, nu)]);
            let bes = fx.sub(fx.mul(&[fx.call("sin", &[ang]), fx.call("bessel_j", &[nu, z])]), fx.mul(&[fx.call("cos", &[ang]), fx.call("bessel_y", &[nu, z])]));
            return Ok(Some(fx.add(&[s, fx.mul(&[pre, bes])])));
        }
    }
    Ok(None)
}

/// Lommel's s_(mu, nu)(z) = sum_k (-1)^k z^(mu + 1 + 2k)/prod_{j<=k} ((mu + 2j + 1)^2 - nu^2).
fn lommel_s1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if let Some(v) = lommel_exact(fx, a, 0)? {
        return Ok(Some(v));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // machine arguments: the series in double-double numbers (round 37), the ball path when it declines
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        if let Some(v) = crate::fast29::lommel_s1(ms[0], ms[1], ms[2]) {
            return Ok(crate::fast29::machine_out(fx, v, real && ms[2].re.hi >= 0.0));
        }
    }
    if series_too_long(fx, a[2]) {
        return Ok(None);
    }
    let extra = cancel_bits(fx, a[2], 1.0);
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let (mu, nu, z) = (xs[0], xs[1], xs[2]);
    let z2 = fx.neg(fx.mul(&[z, z]));
    let nu2 = fx.mul(&[nu, nu]);
    let den = |fx: &Fx<'_, '_, '_, 'h>, j: usize| {
        let b = fx.add(&[mu, fx.int(2 * j as i64 + 1)]);
        fx.sub(fx.mul(&[b, b]), nu2)
    };
    let d0 = den(fx, 0);
    if fx.num(d0).is_none_or(|n| n.is_zero()) {
        return Ok(None);
    }
    let zp = evpow(fx, z, fx.add(&[mu, fx.int(1)]))?;
    let mut t = fx.div(zp, d0);
    let n0 = mag(fx, z) as usize + 4;
    let s = sum_series(fx, wk.w, n0, |fx, k| {
        let cur = t;
        let d = den(fx, k + 1);
        if fx.num(d).is_none_or(|n| n.is_zero()) {
            return Ok(None);
        }
        t = fx.div(fx.mul(&[t, z2]), d);
        Ok(Some(cur))
    })?;
    let Some(s) = s else { return Ok(None) };
    let real = wk.real && parts(fx, xs[2]).is_some_and(|p| p.0 >= 0.0);
    Ok(wk.out(fx, s, real))
}

/// S_(mu, nu)(z) = s_(mu, nu)(z) + 2^(mu - 1) Gamma((mu - nu + 1)/2) Gamma((mu + nu + 1)/2) (sin((mu - nu) pi/2)
/// J_nu(z) - cos((mu - nu) pi/2) Y_nu(z)) (DLMF 11.9.5).
fn lommel_s2<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if let Some(v) = lommel_exact(fx, a, 1)? {
        return Ok(Some(v));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let (mu, nu, z) = (a[0], a[1], a[2]);
    let g = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.call("gamma", &[fx.mul(&[fx.rat(1, 2), t])]);
    let pre = fx.mul(&[
        fx.pow(fx.int(2), fx.sub(mu, fx.int(1))),
        g(fx, fx.add(&[fx.sub(mu, nu), fx.int(1)])),
        g(fx, fx.add(&[mu, nu, fx.int(1)])),
    ]);
    let ang = fx.mul(&[fx.rat(1, 2), fx.sym("pi"), fx.sub(mu, nu)]);
    let bes = fx.sub(
        fx.mul(&[fx.call("sin", &[ang]), fx.call("bessel_j", &[nu, z])]),
        fx.mul(&[fx.call("cos", &[ang]), fx.call("bessel_y", &[nu, z])]),
    );
    Ok(Some(fx.add(&[fx.call("lommel_s1", a), fx.mul(&[pre, bes])])))
}

// ---------------------------------------------------------------- confluent relatives

/// U(a, b, z) at working-precision reals (z > 0): Gamma(1-b)/Gamma(a-b+1) M(a,b,z) + Gamma(b-1)/Gamma(a)
/// z^(1-b) M(a-b+1, 2-b, z) (DLMF 13.2.42), each 1/Gamma zero at its poles; at an integer b the mean of the
/// values at b +- eps (an even function of eps about the limit, so the error is eps^2).
///
/// `p` is the target precision and `w` the working precision of the arguments: eps = 2^-(p/2 + 32) (the error of
/// the mean is eps^2 times the second derivative in b; the 1/eps terms of the two halves cancel, costing p/2 + 32
/// bits), carried at `w` bits so that b + eps is not rounded back to b.
fn tricomi<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: T<'h>, b: T<'h>, z: T<'h>, p: u64, w: u64) -> Res<Option<T<'h>>> {
    let one = fx.int(1);
    let form = |fx: &mut Fx<'_, '_, '_, 'h>, b: T<'h>| -> Res<Option<T<'h>>> {
        let rg = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>| -> Res<Option<T<'h>>> {
            if exact_gamma_pole(fx, x) {
                return Ok(Some(fx.int(0)));
            }
            let g = ev(fx, "gamma", &[x])?;
            Ok(fx.num(g).is_some().then(|| fx.div(fx.int(1), g)))
        };
        let a1 = fx.add(&[fx.sub(a, b), one]);
        let (Some(r1), Some(r2)) = (rg(fx, a1)?, rg(fx, a)?) else { return Ok(None) };
        let g1 = ev(fx, "gamma", &[fx.sub(one, b)])?;
        let g2 = ev(fx, "gamma", &[fx.sub(b, one)])?;
        let m1 = ev(fx, "hypergeometric_1f1", &[a, b, z])?;
        let m2 = ev(fx, "hypergeometric_1f1", &[a1, fx.sub(fx.int(2), b), z])?;
        let zp = evpow(fx, z, fx.sub(one, b))?;
        let v = fx.add(&[fx.mul(&[g1, r1, m1]), fx.mul(&[g2, r2, zp, m2])]);
        Ok(fx.num(v).is_some().then_some(v))
    };
    if fx.f64_of(b).is_some_and(|v| v == v.round()) {
        let eps = fx.ar.number(&Num::Real(Real::Ball(
            Ball::exact(mc_arith::num::Float::new(false, mc_arith::num::Nat::from_u64(1), -((p / 2 + 32) as i64))),
            w,
        )));
        let (Some(u1), Some(u2)) = (form(fx, fx.add(&[b, eps]))?, form(fx, fx.sub(b, eps))?) else { return Ok(None) };
        return Ok(Some(fx.mul(&[fx.rat(1, 2), fx.add(&[u1, u2])])));
    }
    form(fx, b)
}

/// U(a, b, z) for large |z| in the right half plane by the asymptotic series
/// `z^-a sum_k (a)_k (a - b + 1)_k / k! (-z)^-k` (DLMF 13.7.3, valid for |arg z| < 3 pi/2; Olver 1974, ch. 7 and
/// 13): the terms are added until one is below 2^-(p + 6) of the sum, and the first one omitted, doubled, widens the
/// result (an estimate of the remainder as for the other confluent functions here; for real parameters and z > 0
/// DLMF 13.7.2 bounds it by that term in absolute value times a factor near 1). `None` when |z| is not large against
/// the parameters or the terms do not fall below that level (they grow after about |z| terms: the smallest term is
/// about e^-|z|), so the caller keeps the connection formula. The connection formula at z = 1e4 needs `e^z` and
/// thousands of guard bits (`hermite_h(2.5, 100.)` is `2^2.5 U(-1.25, 1/2, 1e4)`: it did not return).
fn u_asymptotic<'h>(fx: &mut Fx<'_, '_, '_, 'h>, xs: &[T<'h>], p: u64, w: u64) -> Res<Option<T<'h>>> {
    let (a, b, z) = (xs[0], xs[1], xs[2]);
    let (Some(pa), Some(pb), Some(pz)) = (parts(fx, a), parts(fx, b), parts(fx, z)) else { return Ok(None) };
    let size = |q: (f64, f64)| q.0.hypot(q.1);
    let zm = size(pz);
    // Re z < 0: the series (DLMF 13.7.3, |ph z| < 3 pi/2) leaves exponentially small terms of relative size e^(Re z),
    // negligible only when -Re z > 0.7 (p + 10) (U(2, 2, -8085.6) took 23 s through the connection formula, B26-22)
    if (pz.0 < 0.0 && -pz.0 < 0.7 * (p as f64 + 10.0)) || p > 900 || zm < (p as f64).max(53.0) * 0.7 + 4.0 * (size(pa) + size(pb)) || zm.is_nan() {
        return Ok(None);
    }
    let one = fx.int(1);
    let a1 = fx.add(&[fx.sub(a, b), one]);
    let mz = fx.neg(z);
    let mut term = one;
    let mut sum = one;
    let mut prev = 1.0f64;
    let mut last = 0.0f64;
    let mut done = false;
    for k in 0..4000i64 {
        let num = fx.mul(&[fx.add(&[a, fx.int(k)]), fx.add(&[a1, fx.int(k)])]);
        let den = fx.mul(&[fx.int(k + 1), mz]);
        let next = fx.div(fx.mul(&[term, num]), den);
        let Some(pn) = parts(fx, next) else { return Ok(None) };
        let nm = size(pn);
        if !nm.is_finite() || (k >= 3 && nm > prev) {
            return Ok(None);
        }
        if nm == 0.0 {
            done = true;
            break;
        }
        term = next;
        sum = fx.add(&[sum, next]);
        let sm = parts(fx, sum).map_or(f64::NAN, size);
        prev = nm;
        last = nm;
        if nm < sm * 2f64.powi(-(p as i32) - 6) {
            done = true;
            break;
        }
    }
    if !done {
        return Ok(None);
    }
    // an integer a: an exact integer power (a real value at a negative z, not a phase with rounding noise)
    let zp = match pa {
        (ar, 0.0) if ar == ar.floor() && ar.abs() < 1e6 => evpow(fx, z, fx.int(-(ar as i64)))?,
        _ => evpow(fx, z, fx.neg(a))?,
    };
    let v = fx.mul(&[zp, sum]);
    let Some(pv) = parts(fx, v) else { return Ok(None) };
    if !pv.0.is_finite() || !pv.1.is_finite() {
        return Ok(None);
    }
    let rad = 2.0 * last * parts(fx, zp).map_or(0.0, size);
    if rad == 0.0 || !rad.is_finite() {
        return Ok(Some(v));
    }
    let ball = Ball { mid: mc_arith::num::Float::zero(), rad: mc_arith::num::Mag::pow2(rad.log2().ceil() as i64) };
    let wide = match fx.num(v) {
        Some(Num::Real(Real::Ball(b, _))) => Num::Real(Real::Ball(Ball { mid: b.mid.clone(), rad: b.rad.add(&ball.rad) }, w)),
        Some(Num::Complex(Real::Ball(x, _), Real::Ball(y, _))) => Num::Complex(
            Real::Ball(Ball { mid: x.mid.clone(), rad: x.rad.add(&ball.rad) }, w),
            Real::Ball(Ball { mid: y.mid.clone(), rad: y.rad.add(&ball.rad) }, w),
        ),
        _ => return Ok(Some(v)),
    };
    Ok(Some(fx.ar.number(&wide)))
}

fn hypergeometric_u<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (x, b, z) = (a[0], a[1], a[2]);
    if fx.is_int(x, 0) {
        return Ok(Some(fx.int(1)));
    }
    // a = -n, a non-positive integer: the polynomial U(-n, b, z) = (-1)^n sum_k C(n, k) (b + k)_(n - k) (-z)^k
    // (DLMF 13.2.7), for any b and z (the general formula has 1/Gamma(a) = 0 against a pole there)
    let minus_n = fx.int_of(x).or_else(|| fx.machine_of(x).filter(|v| v.fract() == 0.0 && v.abs() < 1e6).map(|v| v as i64));
    if let Some(mn) = minus_n.filter(|&v| (-300..=0).contains(&v)) {
        let n = -mn;
        let mut terms = Vec::with_capacity(n as usize + 1);
        let mut binom = mc_arith::num::Int::from_i64(1);
        for k in 0..=n {
            let mut f = vec![fx.integer(&binom), fx.pow(fx.neg(z), fx.int(k))];
            for j in k..n {
                f.push(fx.add(&[b, fx.int(j)]));
            }
            terms.push(fx.mul(&f));
            // C(n, k + 1) = C(n, k) (n - k) / (k + 1)
            binom = binom.mul(&mc_arith::num::Int::from_i64(n - k)).divrem_floor(&mc_arith::num::Nat::from_u64((k + 1) as u64)).0;
        }
        let sum = fx.add(&terms);
        let sum = if n % 2 == 1 { fx.neg(sum) } else { sum };
        let exact_x = fx.machine_of(x).is_none();
        return Ok(Some(if fx.num(sum).is_some() || !exact_x { sum } else { fx.call("expand", &[sum]) }));
    }
    // z = 0 with Re b < 1 (a not a non-positive integer, above): U(a, b, 0) = Gamma(1 - b)/Gamma(a - b + 1), the limit
    // of DLMF 13.2.20-13.2.22 (0 when a - b + 1 is a pole of Gamma; round 37: hypergeometric_u(1/3, 1/2, 0.) stayed
    // unevaluated and U(1, 1/2, 0) gave 0 through the rule e^x x^(1 - b) Gamma(b - 1, x)); Re b >= 1 has no finite
    // value there (z^(1 - b) or log z)
    if fx.num(z).is_some_and(|n| n.is_zero()) && fx.num(x).is_some() && parts(fx, b).is_some_and(|p| p.0 < 1.0) {
        let c = fx.eval(fx.add(&[fx.sub(x, b), fx.int(1)]))?;
        let pole = match fx.num(c) {
            Some(Num::Real(r)) => {
                let v = r.to_f64();
                v <= 0.0 && v.fract() == 0.0 && !has_radius(fx, c)
            }
            _ => false,
        };
        let out = match (pole, numeric(fx, a)) {
            (true, false) => fx.int(0),
            (true, true) => {
                if fx.num(z).is_some_and(|n| n.kind() != Kind::Exact) {
                    z
                } else {
                    fx.real(0.0)
                }
            }
            (false, num) => {
                let v = fx.div(fx.call("gamma", &[fx.sub(fx.int(1), b)]), fx.call("gamma", &[c]));
                if num && fx.num(z).is_some_and(|n| n.kind() != Kind::Exact) {
                    // v + 0. takes the kind and precision of the inexact zero (Gamma(3/2)/Gamma(7/2) = 4/15 stayed exact)
                    fx.eval(fx.add(&[v, z]))?
                } else if num {
                    numify(fx, v, a)?
                } else {
                    fx.eval(v)?
                }
            }
        };
        return Ok(Some(out));
    }
    // U(a, a + 1, z) = z^-a
    if fx.num(x).is_some() && fx.num(b).is_some() && fx.num(fx.sub(b, x)).is_some_and(|n| n.is_exact_one()) {
        return Ok(Some(fx.pow(z, fx.neg(x))));
    }
    // numbers, one inexact (any complex arguments, z negative real included): Tricomi's formula
    // U = Gamma(1-b)/Gamma(a-b+1) M(a,b,z) + Gamma(b-1)/Gamma(a) z^(1-b) M(a-b+1,2-b,z) (DLMF 13.2.42) with the
    // principal power (the value above the cut on the negative real axis), at an integer b the mean of the values
    // at b +- eps
    if !numeric(fx, a) || parts(fx, z).is_some_and(|(x, y)| x == 0.0 && y == 0.0) {
        return Ok(None);
    }
    let p0 = a
        .iter()
        .filter_map(|&t| match fx.num(t) {
            Some(Num::Real(r)) => r.prec(),
            Some(Num::Complex(x, y)) => x.prec().max(y.prec()),
            None => None,
        })
        .max()
        .unwrap_or(53)
        .max(53);
    // large |z| in the right half plane: the asymptotic series (no guard bits for e^z)
    if let Some((wk0, xs0)) = work(fx, a, 64) {
        if let Some(v) = u_asymptotic(fx, &xs0, wk0.p, wk0.w)? {
            let real_value = wk0.real && parts(fx, z).is_some_and(|(x, _)| x > 0.0);
            return Ok(wk0.out(fx, v, real_value));
        }
    }
    let integer_b = parts(fx, b).is_some_and(|(x, y)| y == 0.0 && x == x.round());
    let extra = cancel_bits(fx, z, 1.0) + if integer_b { 2 * p0 + 16 } else { 0 };
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    // the terms of the logarithmic case are of the order 1/eps^2 and cancel: arguments with a radius (a 133-bit
    // z is one) would multiply their radius by that, so the sum is taken at the midpoints and at the midpoints
    // moved by the radii, and the value widened by twice the difference (an estimate, as for meijer_g)
    let radii = xs.iter().any(|&t| has_radius(fx, t));
    let mids: Vec<T<'h>> = xs.iter().map(|&t| mid_term(fx, t)).collect();
    let Some(v) = tricomi(fx, mids[0], mids[1], mids[2], wk.p, wk.w)? else { return Ok(None) };
    // a value at the midpoints with fewer correct bits than asked is not returned (the cancellation exceeded the
    // guard bits); the widening for the arguments' own radii comes after this test
    if !has_bits(fx, v, wk.p) {
        return Ok(None);
    }
    let v = if radii {
        let moved: Vec<T<'h>> = xs.iter().map(|&t| shift_term(fx, t, wk.w)).collect();
        let Some(vs) = tricomi(fx, moved[0], moved[1], moved[2], wk.p, wk.w)? else { return Ok(None) };
        let Some(wide) = widen_by_difference(fx, v, vs, wk.w) else { return Ok(None) };
        wide
    } else {
        v
    };
    let real_value = wk.real && parts(fx, z).is_some_and(|(x, _)| x > 0.0);
    Ok(wk.out(fx, v, real_value))
}

/// Does the ball number have at least `p - 4` correct bits (radius against the larger part)?
fn has_bits<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, p: u64) -> bool {
    let log = |b: &Ball| {
        let m = mc_arith::num::Mag::from_float_up(&b.mid);
        (if m.is_zero() { f64::NEG_INFINITY } else { m.log2() }, if b.rad.is_zero() { f64::NEG_INFINITY } else { b.rad.log2() })
    };
    let parts: Vec<(f64, f64)> = match fx.num(t) {
        Some(Num::Real(Real::Ball(b, _))) => vec![log(&b)],
        Some(Num::Complex(Real::Ball(x, _), Real::Ball(y, _))) => vec![log(&x), log(&y)],
        _ => return true,
    };
    let mag = parts.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max);
    let rad = parts.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max);
    rad == f64::NEG_INFINITY || mag - rad >= p as f64 - 4.0
}

/// `v` with both radii widened by twice the distance to `vs` (an estimate of the effect of the arguments' radii).
fn widen_by_difference<'h>(fx: &Fx<'_, '_, '_, 'h>, v: T<'h>, vs: T<'h>, w: u64) -> Option<T<'h>> {
    use mc_arith::num::Mag;
    let ball_of = |n: &Num| -> Option<(Ball, Option<Ball>)> {
        match n {
            Num::Real(Real::Ball(b, _)) => Some((b.clone(), None)),
            Num::Complex(Real::Ball(x, _), Real::Ball(y, _)) => Some((x.clone(), Some(y.clone()))),
            _ => None,
        }
    };
    let (bv, bs) = (ball_of(&fx.num(v)?)?, ball_of(&fx.num(vs)?)?);
    let dist = |a: &Ball, b: &Ball| {
        let d = a.sub(b, w);
        Mag::from_float_up(&d.mid).add(&d.rad)
    };
    let mut e = dist(&bv.0, &bs.0);
    if let (Some(yv), Some(ys)) = (&bv.1, &bs.1) {
        e = e.add(&dist(yv, ys));
    }
    let e = e.mul_2exp(1);
    let wide = |b: &Ball| Ball { mid: b.mid.clone(), rad: b.rad.add(&e) };
    let num = match &bv.1 {
        None => Num::Real(Real::Ball(wide(&bv.0), w)),
        Some(y) => Num::Complex(Real::Ball(wide(&bv.0), w), Real::Ball(wide(y), w)),
    };
    Some(fx.ar.number(&num))
}

fn whittaker_m<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if !numeric(fx, a) {
        return Ok(None);
    }
    let (k, m, z) = (a[0], a[1], a[2]);
    let half = fx.rat(1, 2);
    let pre = fx.mul(&[fx.call("exp", &[fx.mul(&[fx.rat(-1, 2), z])]), fx.pow(z, fx.add(&[m, half]))]);
    let f = fx.call("hypergeometric_1f1", &[fx.add(&[fx.sub(m, k), half]), fx.add(&[fx.int(1), fx.mul(&[fx.int(2), m])]), z]);
    Ok(Some(fx.mul(&[pre, f])))
}

fn whittaker_w<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if !numeric(fx, a) {
        return Ok(None);
    }
    let (k, m, z) = (a[0], a[1], a[2]);
    let half = fx.rat(1, 2);
    let pre = fx.mul(&[fx.call("exp", &[fx.mul(&[fx.rat(-1, 2), z])]), fx.pow(z, fx.add(&[m, half]))]);
    let u = fx.call("hypergeometric_u", &[fx.add(&[fx.sub(m, k), half]), fx.add(&[fx.int(1), fx.mul(&[fx.int(2), m])]), z]);
    Ok(Some(fx.mul(&[pre, u])))
}

/// D_nu(z) = 2^(nu/2) e^(-z^2/4) sqrt(pi) (M(-nu/2, 1/2, z^2/2)/Gamma((1 - nu)/2) - sqrt(2) z M((1 - nu)/2, 3/2,
/// z^2/2)/Gamma(-nu/2)) (DLMF 12.7.14 with 12.4.1), entire in z.
fn parabolic_d<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (nu, z) = (a[0], a[1]);
    if fx.is_int(nu, 0) {
        return Ok(Some(fx.call("exp", &[fx.mul(&[fx.rat(-1, 4), z, z])])));
    }
    // a non-negative integer order (exact, or a machine integer): D_n(z) = 2^(-n/2) e^(-z^2/4) H_n(z/sqrt 2)
    // (DLMF 12.7.2), expanded
    let order = fx.int_of(nu).or_else(|| fx.machine_of(nu).filter(|v| v.fract() == 0.0 && v.abs() < 1e4).map(|v| v as i64));
    if let Some(n) = order.filter(|&n| (1..=60).contains(&n)) {
        let mut zz = z;
        if fx.machine_of(nu).is_some() && fx.num(z).is_some() {
            zz = fx.eval(fx.call("n", &[z]))?;
        }
        let x = fx.div(zz, fx.sqrt(fx.int(2)));
        let herm = fx.eval(fx.call("hermite_h", &[fx.int(n), x]))?;
        let v = fx.mul(&[fx.pow(fx.int(2), fx.rat(-n, 2)), fx.call("exp", &[fx.mul(&[fx.rat(-1, 4), zz, zz])]), herm]);
        return Ok(Some(if fx.num(zz).is_some() { v } else { fx.call("expand", &[v]) }));
    }
    // D_nu(0) = 2^(nu/2) sqrt(pi)/Gamma((1 - nu)/2) (DLMF 12.2.6 with 12.7.2)
    if fx.is_int(z, 0) && fx.num(nu).is_none_or(|n| n.kind() == Kind::Exact) {
        let g = fx.call("gamma", &[fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), nu)])]);
        return Ok(Some(fx.div(fx.mul(&[fx.pow(fx.int(2), fx.mul(&[fx.rat(1, 2), nu])), fx.sqrt(fx.sym("pi"))]), g)));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // machine reals, x >= 30 and |nu| <= x^2/8: D_nu(x) = e^(-x^2/4) x^nu sum_s (-1)^s (-nu)_(2s)/(s! (2 x^2)^s)
    // (DLMF 12.9.1 with D_nu = U(-nu - 1/2, x)); the exponential underflows to 0 as the value does. The Maclaurin
    // route below needs 1.1 x^2 extra bits: D_(0.35)(278.9) did not return (round 27, B26-22)
    if let (Some(nm), Some(xm)) = (fx.f64_of(nu), fx.machine_of(z)) {
        if xm >= 30.0 && nm.abs() <= xm * xm / 8.0 {
            let (mut term, mut sum) = (1.0f64, 1.0f64);
            let mut ok = false;
            for k in 0..200 {
                let kf = k as f64;
                term *= -((-nm + 2.0 * kf) * (-nm + 2.0 * kf + 1.0)) / ((kf + 1.0) * 2.0 * xm * xm);
                sum += term;
                if term.abs() <= 1e-17 * sum.abs() {
                    ok = true;
                    break;
                }
            }
            if ok {
                return Ok(Some(fx.real((-xm * xm / 4.0 + nm * xm.ln()).exp() * sum)));
            }
        }
    }
    let mz = mag(fx, z);
    let extra = (mz * mz * 0.75 * std::f64::consts::LOG2_E) as u64 + 8;
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let (nu, z) = (xs[0], xs[1]);
    let half = fx.rat(1, 2);
    let y = fx.mul(&[half, z, z]);
    let rg = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>| -> Res<Option<T<'h>>> {
        if gamma_pole(fx, x) {
            return Ok(Some(fx.int(0)));
        }
        let g = ev(fx, "gamma", &[x])?;
        Ok(fx.num(g).is_some().then(|| fx.div(fx.int(1), g)))
    };
    let a1 = fx.mul(&[fx.rat(-1, 2), nu]);
    let a2 = fx.mul(&[half, fx.sub(fx.int(1), nu)]);
    let (Some(r1), Some(r2)) = (rg(fx, a2)?, rg(fx, a1)?) else { return Ok(None) };
    let m1 = ev(fx, "hypergeometric_1f1", &[a1, half, y])?;
    let m2 = ev(fx, "hypergeometric_1f1", &[a2, fx.rat(3, 2), y])?;
    // sqrt(2) as a ball: an exact radical times a ball stays a product
    let sqrt2 = ev(fx, "sqrt", &[ballify(fx, fx.int(2), wk.w)])?;
    let inner = fx.sub(fx.mul(&[m1, r1]), fx.mul(&[sqrt2, z, m2, r2]));
    let p1 = evpow(fx, fx.int(2), fx.mul(&[half, nu]))?;
    let p2 = ev(fx, "exp", &[fx.mul(&[fx.rat(-1, 4), z, z])])?;
    let pi = pi_at(fx, wk.w)?;
    let p3 = ev(fx, "sqrt", &[pi])?;
    let pre = fx.mul(&[p1, p2, p3]);
    let v = fx.mul(&[pre, inner]);
    if fx.num(v).is_none() {
        return Ok(None);
    }
    Ok(wk.out(fx, v, wk.real))
}

fn pfq_regularized<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (Some(_), Some(bs)) = (fx.as_list(a[0]), fx.as_list(a[1])) else { return Ok(None) };
    if bs.iter().any(|&b| gamma_pole(fx, b)) {
        return Ok(None);
    }
    let f = fx.eval(fx.call("hypergeometric_pfq", a))?;
    if fx.contains(f, "hypergeometric_pfq") && !numeric(fx, &[a[2]]) {
        return Ok(None);
    }
    let gs: Vec<T<'h>> = bs.iter().map(|&b| fx.call("gamma", &[b])).collect();
    Ok(Some(fx.div(f, fx.mul(&gs))))
}

// ---------------------------------------------------------------- others

fn fresnel_fg<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], g: bool) -> R<'h> {
    let z = a[0];
    if fx.is_int(z, 0) {
        return Ok(Some(fx.rat(1, 2)));
    }
    // f, g -> 0 as z -> inf (DLMF 7.12.2, 7.12.3)
    if fx.infinity(z) == Some(0) {
        return Ok(Some(fx.int(0)));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // a machine real of size >= 6: the asymptotic series (fast; S and C at high precision are slow there)
    if let Some(x) = fx.machine_of(z).filter(|v| v.abs() >= 6.0 && v.is_finite()) {
        if let Some(v) = fresnel_fg_asymptotic(x, g) {
            return Ok(Some(fx.real(v)));
        }
    }
    // a smaller machine real: from machine S and C (the values are at least 1/(pi |x|)/2 there, so at most a digit
    // or two of the differences 1/2 - S, 1/2 - C is lost)
    if let Some(x) = fx.machine_of(z).filter(|v| v.abs() < 6.0) {
        let sv = fx.eval(fx.call("fresnel_s", &[z]))?;
        let cv = fx.eval(fx.call("fresnel_c", &[z]))?;
        if let (Some(sm), Some(cm)) = (fx.machine_of(sv), fx.machine_of(cv)) {
            let ang = 0.5 * std::f64::consts::PI * x * x;
            let (fs, fc) = (0.5 - sm, 0.5 - cm);
            let v = if g { fc * ang.cos() + fs * ang.sin() } else { fs * ang.cos() - fc * ang.sin() };
            return Ok(Some(fx.real(v)));
        }
    }
    // f = (1/2 - S) cos(pi z^2/2) - (1/2 - C) sin(pi z^2/2), g = (1/2 - C) cos + (1/2 - S) sin
    let extra = (mag(fx, z) * mag(fx, z) * 0.5) as u64 + 8;
    let Some((wk, xs)) = work(fx, a, extra.min(200)) else { return Ok(None) };
    let z = xs[0];
    let pi = ev(fx, "n", &[fx.sym("pi"), fx.int((wk.w as f64 / 3.3) as i64 + 2)])?;
    let ang = fx.mul(&[fx.rat(1, 2), pi, z, z]);
    let (c, s) = (ev(fx, "cos", &[ang])?, ev(fx, "sin", &[ang])?);
    let half = fx.rat(1, 2);
    let sv = ev(fx, "fresnel_s", &[z])?;
    let cv = ev(fx, "fresnel_c", &[z])?;
    let fs = fx.sub(half, sv);
    let fc = fx.sub(half, cv);
    let v = if g { fx.add(&[fx.mul(&[fc, c]), fx.mul(&[fs, s])]) } else { fx.sub(fx.mul(&[fs, c]), fx.mul(&[fc, s])) };
    if fx.num(v).is_none() {
        return Ok(None);
    }
    Ok(wk.out(fx, v, wk.real))
}
/// f(x) or g(x) for a machine real |x| >= 6 by the asymptotic expansions f ~ 1/(pi x) sum (-1)^m (4m-1)!!/(pi x^2)^(2m)
/// and g ~ 1/(pi^2 x^3) sum (-1)^m (4m+1)!!/(pi x^2)^(2m) (DLMF 7.12.2, 7.12.3), summed to the smallest term (below
/// 2^-60 of the sum from |x| = 6 on); negative x by f(-x) = sqrt(2) cos(pi/4 - pi x^2/2) - f(x) and g(-x) =
/// sqrt(2) sin(pi/4 - pi x^2/2) - g(x) (DLMF 7.4.10).
fn fresnel_fg_asymptotic(x: f64, g: bool) -> Option<f64> {
    use std::f64::consts::{FRAC_PI_4, PI, SQRT_2};
    let a = x.abs();
    let w = PI * a * a;
    let w2 = w * w;
    let mut term: f64 = 1.0;
    let mut sum: f64 = 1.0;
    let mut m: f64 = 0.0;
    loop {
        // (4m+1)(4m+3) for f's (4m+3)!!/(4m-1)!!, (4m+3)(4m+5) for g
        let ratio = if g { (4.0 * m + 3.0) * (4.0 * m + 5.0) } else { (4.0 * m + 1.0) * (4.0 * m + 3.0) } / w2;
        if ratio >= 1.0 || m > 200.0 {
            return None;
        }
        let next = -term * ratio;
        if next.abs() < 1e-18 * sum.abs() {
            break;
        }
        term = next;
        sum += term;
        m += 1.0;
    }
    let v = if g { sum / (PI * PI * a * a * a) } else { sum / (PI * a) };
    if x > 0.0 {
        return Some(v);
    }
    let ang = FRAC_PI_4 - 0.5 * PI * a * a;
    Some(if g { SQRT_2 * ang.sin() - v } else { SQRT_2 * ang.cos() - v })
}

fn fresnel_f<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    fresnel_fg(fx, a, false)
}
fn fresnel_g<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    fresnel_fg(fx, a, true)
}

fn logistic_sigmoid<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let z = a[0];
    if fx.is_int(z, 0) {
        return Ok(Some(fx.rat(1, 2)));
    }
    if let Some(i) = fx.infinity(z) {
        match i {
            0 => return Ok(Some(fx.int(1))),
            2 => return Ok(Some(fx.int(0))),
            _ => {}
        }
    }
    if !numeric(fx, a) {
        // e^-z exact (z an exact multiple of i pi/2, say): 1/(1 + e^-z), unless that is a pole
        if fx.contains(z, "pi") || fx.num(z).is_some() {
            let e = fx.eval(fx.call("exp", &[fx.neg(z)]))?;
            if let Some(n) = fx.num(e).filter(|n| n.kind() == Kind::Exact) {
                if n.add(&Num::int(1)).is_exact_zero() {
                    return Ok(None);
                }
                return Ok(Some(fx.div(fx.int(1), fx.add(&[fx.int(1), e]))));
            }
        }
        return Ok(None);
    }
    Ok(Some(fx.div(fx.int(1), fx.add(&[fx.int(1), fx.call("exp", &[fx.neg(z)])]))))
}

/// theta(t) = (log_gamma(1/4 + i t/2) - log_gamma(1/4 - i t/2))/(2 i) - t log(pi)/2.
fn rs_theta<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> T<'h> {
    let q = fx.rat(1, 4);
    let it = fx.mul(&[fx.rat(1, 2), fx.i(), t]);
    let d = fx.sub(fx.call("log_gamma", &[fx.add(&[q, it])]), fx.call("log_gamma", &[fx.sub(q, it)]));
    fx.sub(fx.div(d, fx.mul(&[fx.int(2), fx.i()])), fx.mul(&[fx.rat(1, 2), t, fx.call("log", &[fx.sym("pi")])]))
}

fn riemann_siegel_theta<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let t = a[0];
    if fx.is_int(t, 0) {
        return Ok(Some(fx.int(0)));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let v = rs_theta(fx, t);
    Ok(Some(if fx.f64_of(t).is_some() { fx.call("re", &[v]) } else { v }))
}

fn riemann_siegel_z<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let t = a[0];
    if fx.is_int(t, 0) {
        return Ok(Some(fx.call("zeta", &[fx.rat(1, 2)])));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let e = fx.call("exp", &[fx.mul(&[fx.i(), rs_theta(fx, t)])]);
    let z = fx.call("zeta", &[fx.add(&[fx.rat(1, 2), fx.mul(&[fx.i(), t])])]);
    let v = fx.mul(&[e, z]);
    Ok(Some(if fx.f64_of(t).is_some() { fx.call("re", &[v]) } else { v }))
}

/// Phi(z, s, a) = sum z^k/(k + a)^s.
fn lerch_phi<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (z, s, x) = (a[0], a[1], a[2]);
    if fx.is_int(z, 0) {
        return Ok(Some(fx.pow(x, fx.neg(s))));
    }
    if fx.is_int(z, 1) {
        return Ok(Some(fx.call("zeta", &[s, x])));
    }
    if fx.is_int(x, 1) && fx.num(z).is_some() {
        return Ok(Some(fx.div(fx.call("polylog", &[s, z]), z)));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    if mag(fx, z) > 0.95 {
        if parts(fx, z) == Some((1.0, 0.0)) {
            return Ok(Some(fx.call("zeta", &[s, x])));
        }
        // a positive integer s and a: the series is z^-a (Li_s(z) - sum_(i < a) z^i/i^s), which continues it
        // beyond the unit circle on the principal branch of the polylogarithm (DLMF 25.14.1, 25.12.10)
        if let (Some(n), Some(m)) = (fx.int_of(s), fx.int_of(x)) {
            if n >= 1 && (1..=10_000).contains(&m) {
                let mut terms = vec![fx.call("polylog", &[s, z])];
                for i in 1..m {
                    terms.push(fx.neg(fx.div(fx.pow(z, fx.int(i)), fx.pow(fx.int(i), s))));
                }
                return Ok(Some(fx.mul(&[fx.pow(z, fx.int(-m)), fx.add(&terms)])));
            }
        }
        return Ok(None);
    }
    let Some((wk, xs)) = work(fx, a, 8) else { return Ok(None) };
    let (z, s, x) = (xs[0], xs[1], xs[2]);
    let ns = fx.neg(s);
    let mut zk = fx.int(1);
    let v = sum_series(fx, wk.w, 4, |fx, k| {
        let base = fx.add(&[x, fx.int(k as i64)]);
        if fx.num(base).is_some_and(|n| n.is_zero()) {
            return Ok(None);
        }
        let bp = evpow(fx, base, ns)?;
        let t = fx.mul(&[zk, bp]);
        zk = fx.mul(&[zk, z]);
        Ok(Some(t))
    })?;
    let Some(v) = v else { return Ok(None) };
    let real = wk.real && parts(fx, xs[2]).is_some_and(|p| p.0 > 0.0);
    Ok(wk.out(fx, v, real))
}

/// E_(a, b)(z) = sum z^k/Gamma(a k + b), a > 0.
fn mittag_leffler<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (al, b, z) = if a.len() == 2 { (a[0], fx.int(1), a[1]) } else { (a[0], a[1], a[2]) };
    if fx.is_int(z, 0) {
        return Ok(Some(fx.div(fx.int(1), fx.call("gamma", &[b]))));
    }
    if fx.is_int(b, 1) {
        if fx.is_int(al, 1) {
            return Ok(Some(fx.call("exp", &[z])));
        }
        if fx.is_int(al, 2) {
            return Ok(Some(fx.call("cosh", &[fx.sqrt(z)])));
        }
    }
    if fx.is_int(al, 0) && fx.num(b).is_some() && fx.num(z).is_some() {
        return Ok(Some(fx.div(fx.int(1), fx.mul(&[fx.call("gamma", &[b]), fx.sub(fx.int(1), z)]))));
    }
    // closed forms at an exact or symbolic z (round 37). A positive integer a = m and an integer b: E_(m,b)(z) =
    // (1/m) z^((1-b)/m) sum_(k<m) w^(k(1-b)) exp(w^k z^(1/m)), w = exp(2 pi i/m) (the sum keeps the powers j = b - 1
    // mod m of the exponential series, and 1/Gamma vanishes at the poles for b <= 0; Gorenflo, Kilbas, Mainardi and
    // Rogosin, *Mittag-Leffler functions*, 2014, 4.2). a = 1 and any b: E_(1,b)(z) = e^z z^(1-b) (Q(b - 1, 0) -
    // Q(b - 1, z)) with the regularized incomplete gamma function Q (ibid. 4.1, by the series of the lower one).
    // (a symbolic z, or an exact one off the real line: real exact values keep the series, a real number)
    if fx.num(z).is_none_or(|n| n.kind() == Kind::Exact && matches!(n, Num::Complex(..))) {
        let m = fx.int_of(al).filter(|&m| m >= 1);
        if let (Some(m), Some(bi)) = (m, fx.int_of(b)) {
            let w = |fx: &Fx<'_, '_, '_, 'h>, k: i64| fx.call("exp", &[fx.mul(&[fx.rat(2 * k, m), fx.i(), fx.sym("pi")])]);
            let root = fx.pow(z, fx.rat(1, m));
            let mut terms = Vec::with_capacity(m as usize);
            for k in 0..m {
                if k % 64 == 63 {
                    fx.tick(1)?;
                }
                let c = fx.pow(w(fx, k), fx.int(1 - bi));
                terms.push(fx.mul(&[c, fx.call("exp", &[fx.mul(&[w(fx, k), root])])]));
            }
            let pre = fx.mul(&[fx.rat(1, m), fx.pow(z, fx.rat(1 - bi, m))]);
            return Ok(Some(fx.mul(&[pre, fx.add(&terms)])));
        }
        if m == Some(1) && fx.num(b).is_none() {
            let a1 = fx.sub(b, fx.int(1));
            let q = fx.sub(fx.call("gamma_regularized", &[a1, fx.int(0)]), fx.call("gamma_regularized", &[a1, z]));
            return Ok(Some(fx.mul(&[fx.call("exp", &[z]), fx.pow(z, fx.sub(fx.int(1), b)), q])));
        }
    }
    let args = [al, b, z];
    if !numeric(fx, &args) {
        return Ok(None);
    }
    let (Some(af), Some(bf)) = (fx.f64_of(al), fx.f64_of(b)) else { return Ok(None) };
    // a < 0: the series in 1/z, E_(a,b)(z) = -sum_(k >= 1) z^-k/Gamma(b - a k) = 1/Gamma(b) - E_(-a,b)(1/z) (the
    // continuation the function's properties use, E_a(z) = 1 - E_(-a)(1/z))
    if af < 0.0 {
        let inv = fx.div(fx.int(1), z);
        let e = if a.len() == 2 {
            fx.call("mittag_leffler_e", &[fx.neg(al), inv])
        } else {
            fx.call("mittag_leffler_e", &[fx.neg(al), b, inv])
        };
        let v = fx.eval(fx.sub(fx.div(fx.int(1), fx.call("gamma", &[b])), e))?;
        return Ok(fx.num(v).map(|_| v));
    }
    if af <= 0.0 {
        return Ok(None);
    }
    // the largest term (log2), from the machine log gamma
    let lz = mag(fx, z).max(1e-300).ln();
    let mut big: f64 = 0.0;
    let mut kmax = 0usize;
    for k in 0..MAX_TERMS {
        let x = af * k as f64 + bf;
        let lt = k as f64 * lz - crate::machine::gamma::log_gamma(x.max(1e-300)).0;
        big = big.max(lt);
        if k > 10 && lt < big - 800.0 {
            kmax = k;
            break;
        }
    }
    if kmax == 0 {
        return Ok(None);
    }
    let extra = (big.max(0.0) * std::f64::consts::LOG2_E) as u64 + 8;
    let Some((wk, xs)) = work(fx, &args, extra) else { return Ok(None) };
    let (al, b, z) = (xs[0], xs[1], xs[2]);
    let mut zk = fx.int(1);
    let v = sum_series(fx, wk.w, 4, |fx, k| {
        let x = fx.add(&[fx.mul(&[al, fx.int(k as i64)]), b]);
        let r = if gamma_pole(fx, x) {
            fx.int(0)
        } else {
            let g = ev(fx, "gamma", &[x])?;
            if fx.num(g).is_none() {
                return Ok(None);
            }
            fx.div(fx.int(1), g)
        };
        let t = fx.mul(&[zk, r]);
        zk = fx.mul(&[zk, z]);
        Ok(Some(t))
    })?;
    let Some(v) = v else { return Ok(None) };
    Ok(wk.out(fx, v, wk.real))
}

/// log G(z) at working precision: the asymptotic series for log G(v + 1) at v = z + n with Re v large (DLMF
/// 5.17.5), shifted back by log G(z + n + 1) = log G(z) + sum_{j=0..n} log Gamma(z + j). `constant` is zeta'(-1)
/// (None computes it from log G(1) = 0).
fn log_g<'h>(fx: &mut Fx<'_, '_, '_, 'h>, z: T<'h>, w: u64, constant: Option<T<'h>>) -> Res<Option<T<'h>>> {
    let Some((re, _)) = parts(fx, z) else { return Ok(None) };
    let r = 0.12 * w as f64 + 8.0;
    let n = (r - re).max(0.0).ceil() as i64;
    if n > 100_000 {
        return Ok(None);
    }
    let v = fx.add(&[z, fx.int(n)]);
    let lv = ev(fx, "log", &[v])?;
    let v2 = fx.mul(&[v, v]);
    let pi = ev(fx, "n", &[fx.sym("pi"), fx.int((w as f64 / 3.3) as i64 + 4)])?;
    let two_pi = fx.mul(&[fx.int(2), pi]);
    let l2pi = ev(fx, "log", &[two_pi])?;
    let mut s = fx.add(&[
        fx.mul(&[fx.rat(1, 2), v2, lv]),
        fx.mul(&[fx.rat(-3, 4), v2]),
        fx.mul(&[fx.rat(1, 2), v, l2pi]),
        fx.mul(&[fx.rat(-1, 12), lv]),
    ]);
    // sum_k B_(2k+2)/(4 k (k + 1) v^(2k))
    let inv2 = fx.div(fx.int(1), v2);
    let mut vk = inv2;
    let eps = 2f64.powi(-(w.min(1000) as i32));
    for k in 1..400u64 {
        let Some(bk) = crate::bernoulli::bernoulli(2 * k + 2) else { return Ok(None) };
        let c = bk.mul(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64((4 * k * (k + 1)) as i64)));
        let t = fx.mul(&[fx.q(&c), vk]);
        s = fx.add(&[s, t]);
        if mag(fx, t) < eps * mag(fx, s).max(1e-300) {
            break;
        }
        vk = fx.mul(&[vk, inv2]);
    }
    let mut shift = vec![];
    for j in 0..=n {
        if j % 64 == 63 {
            fx.tick(1)?;
        }
        let lg = ev(fx, "log_gamma", &[fx.add(&[z, fx.int(j)])])?;
        shift.push(fx.neg(lg));
    }
    shift.push(s);
    let without = fx.add(&shift);
    let c = match constant {
        Some(c) => c,
        None => return Ok(Some(without)),
    };
    let v = fx.add(&[without, c]);
    Ok(fx.num(v).is_some().then_some(v))
}

/// zeta'(-1) at working precision (from log G(1) = 0).
fn zeta_prime_m1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, w: u64) -> Res<Option<T<'h>>> {
    let one = fx.ar.number(&Num::Real(Real::Ball(Ball::from_i64(1), w)));
    Ok(log_g(fx, one, w, None)?.map(|v| fx.neg(v)))
}

fn barnes_exact<'h>(fx: &Fx<'_, '_, '_, 'h>, z: T<'h>) -> Option<T<'h>> {
    let n = fx.int_of(z)?;
    if n <= 0 {
        return Some(fx.int(0));
    }
    if n > 2000 {
        return None;
    }
    // G(n) = prod_{k=1}^{n-2} k!
    let mut f = mc_arith::num::Int::from_i64(1);
    let mut g = mc_arith::num::Int::from_i64(1);
    for k in 1..=(n - 2).max(0) {
        f = f.mul(&mc_arith::num::Int::from_i64(k));
        g = g.mul(&f);
    }
    Some(fx.integer(&g))
}

fn barnes_g<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if let Some(v) = barnes_exact(fx, a[0]) {
        return Ok(Some(v));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // G = exp(log G): log G is about |x|^2 log|x|/2, and exp of a value rounded to the target precision loses that many
    // bits (barnes_g(-12.7) was 11 ulp off)
    let Some((re, im)) = parts(fx, a[0]) else { return Ok(None) };
    if !re.is_finite() || !im.is_finite() {
        return Ok(None);
    }
    // a machine integer x <= 0 is a zero of G
    if im == 0.0 && re <= 0.0 && re.fract() == 0.0 {
        return Ok(if fx.machine_of(a[0]).is_some() { Some(fx.real(0.0)) } else { None });
    }
    // log G at the working precision of log_barnes_g (24 + 16 bits beyond the target), not rounded before exp: those
    // guard bits cover the magnification by exp up to |log G| = 2^30; beyond, log2 |log G| - 30 bits more
    // (|log G| ~ |x|^2 log|x|/2)
    if gamma_pole(fx, a[0]) {
        return Ok(None);
    }
    // a machine real: the double-double evaluation (a few microseconds, where the balls below take milliseconds)
    if let Some(x) = fx.machine_of(a[0]) {
        if let Some(v) = crate::machine::gamma::barnes_g(x) {
            return Ok(Some(fx.real(v)));
        }
    }
    let size = re.hypot(im) + 2.0;
    let extra = 16 + ((size * size * size.ln()).log2() - 30.0).max(0.0).ceil() as u64;
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let Some(c) = zeta_prime_m1(fx, wk.w)? else { return Ok(None) };
    let Some(l) = log_g(fx, xs[0], wk.w, Some(c))? else { return Ok(None) };
    // a real x < 0: G is real there (an entire function with real Taylor coefficients, DLMF 5.17), and exp of the
    // complex log G left an imaginary part of rounding size (round 37). |G(x)| = exp(Re log G(x)); the sign: with c =
    // ceil(-x), G(x) = G(x + c)/prod_(k<c) Gamma(x + k), G(x + c) > 0 and Gamma(x + k) of sign (-1)^(c - k), so the sign is
    // (-1)^(c (c + 1)/2), odd for c = 1, 2 mod 4.
    if wk.real && re < 0.0 {
        let c = (-re).ceil();
        let odd = c.rem_euclid(4.0) == 1.0 || c.rem_euclid(4.0) == 2.0;
        let m = fx.call("exp", &[fx.call("re", &[l])]);
        let v = fx.eval(if odd { fx.neg(m) } else { m })?;
        return Ok(wk.out(fx, v, true));
    }
    let v = fx.eval(fx.call("exp", &[l]))?;
    Ok(wk.out(fx, v, wk.real))
}

fn log_barnes_g<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if let Some(n) = fx.int_of(a[0]).filter(|n| (1..=2000).contains(n)) {
        let g = barnes_exact(fx, fx.int(n)).unwrap_or(fx.int(1));
        return Ok(Some(fx.call("log", &[g])));
    }
    if !numeric(fx, a) || gamma_pole(fx, a[0]) {
        return Ok(None);
    }
    let Some((wk, xs)) = work(fx, a, 16) else { return Ok(None) };
    let Some(c) = zeta_prime_m1(fx, wk.w)? else { return Ok(None) };
    let Some(v) = log_g(fx, xs[0], wk.w, Some(c))? else { return Ok(None) };
    let real = wk.real && parts(fx, a[0]).is_some_and(|p| p.0 > 0.0);
    Ok(wk.out(fx, v, real))
}

/// The inverse of Q(a, x) (`gamma`) or of I_x(a, b): the machine inverse, then Newton steps at working precision.
fn inverse_reg<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], gamma: bool) -> R<'h> {
    let s = if gamma { a[1] } else { a[0] };
    if gamma {
        if fx.is_int(s, 1) {
            return Ok(Some(fx.int(0)));
        }
        if fx.is_int(s, 0) {
            return Ok(Some(fx.inf()));
        }
    } else if fx.is_int(s, 0) || fx.is_int(s, 1) {
        return Ok(Some(s));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let fs: Option<Vec<f64>> = a.iter().map(|&t| fx.f64_of(t)).collect();
    let Some(fs) = fs else { return Ok(None) };
    let x0 = if gamma {
        crate::machine::incomplete::inverse_gamma_q(fs[0], fs[1])
    } else {
        crate::machine::incomplete::inverse_beta_reg(fs[1], fs[2], fs[0])
    };
    if !x0.is_finite() {
        return Ok(None);
    }
    let Some((wk, xs)) = work(fx, a, 8) else { return Ok(None) };
    let mut x = fx.ar.number(&Num::Real(Real::Ball(Ball::exact(crate::ball::float_of(x0)), wk.w)));
    if !wk.machine || wk.p > 53 {
        let steps = (wk.w as f64 / 40.0).log2().ceil().max(1.0) as usize + 2;
        for _ in 0..steps {
            // Newton on f(x) - s with f' = -x^(a-1) e^-x/Gamma(a) for Q, x^(a-1)(1-x)^(b-1)/B(a, b) for I
            let (f, d) = if gamma {
                let f = ev(fx, "gamma_regularized", &[xs[0], x])?;
                let xa = evpow(fx, x, fx.sub(xs[0], fx.int(1)))?;
                let ex = ev(fx, "exp", &[fx.neg(x)])?;
                let ga = ev(fx, "gamma", &[xs[0]])?;
                let d = fx.neg(fx.div(fx.mul(&[xa, ex]), ga));
                (f, d)
            } else {
                let f = ev(fx, "beta_regularized", &[x, xs[1], xs[2]])?;
                let xa = evpow(fx, x, fx.sub(xs[1], fx.int(1)))?;
                let xb = evpow(fx, fx.sub(fx.int(1), x), fx.sub(xs[2], fx.int(1)))?;
                let be = ev(fx, "beta", &[xs[1], xs[2]])?;
                let d = fx.div(fx.mul(&[xa, xb]), be);
                (f, d)
            };
            let step = fx.div(fx.sub(f, if gamma { xs[1] } else { xs[0] }), d);
            if fx.num(step).is_none() {
                return Ok(None);
            }
            x = fx.sub(x, step);
        }
    }
    Ok(wk.out(fx, x, true))
}
fn inverse_gamma_regularized<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    inverse_reg(fx, a, true)
}
fn inverse_beta_regularized<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    inverse_reg(fx, a, false)
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("elliptic_theta", &[3], Some(elliptic_theta), None, "elliptic_theta(a, u, q): the Jacobi theta function theta_a(u, q), a = 1, 2, 3, 4, nome |q| < 1"),
    def!("elliptic_theta_prime", &[3], Some(elliptic_theta_prime), None, "elliptic_theta_prime(a, u, q): the derivative of theta_a(u, q) in u"),
    def!("neville_theta_s", &[2], Some(neville_s), None, "neville_theta_s(z, m): the Neville theta function theta_s(z | m)"),
    def!("neville_theta_c", &[2], Some(neville_c), None, "neville_theta_c(z, m): the Neville theta function theta_c(z | m)"),
    def!("neville_theta_d", &[2], Some(neville_d), None, "neville_theta_d(z, m): the Neville theta function theta_d(z | m)"),
    def!("neville_theta_n", &[2], Some(neville_n), None, "neville_theta_n(z, m): the Neville theta function theta_n(z | m)"),
    def!("modular_lambda", &[1], Some(modular_lambda), None, "modular_lambda(tau): the modular lambda function theta_2^4/theta_3^4 at q = e^(i pi tau), Im tau > 0"),
    def!("klein_invariant_j", &[1], Some(klein_j), None, "klein_invariant_j(tau): Klein's absolute invariant J(tau), Im tau > 0"),
    def!("inverse_elliptic_nome_q", &[1], Some(inverse_nome), None, "inverse_elliptic_nome_q(q): the parameter m whose nome is q"),
    def!("dedekind_eta", &[1], Some(dedekind_eta), None, "dedekind_eta(tau): the Dedekind eta function, Im tau > 0"),
    def!("q_pochhammer", &[1, 2, 3], Some(q_pochhammer), None, "q_pochhammer(a, q, n): the q-Pochhammer symbol (a; q)_n; q_pochhammer(a, q) for n = infinity; q_pochhammer(q) = (q; q)_inf"),
    def!("q_factorial", &[2], Some(q_factorial), None, "q_factorial(n, q): the q-factorial (q; q)_n/(1 - q)^n"),
    def!("q_binomial", &[3], Some(q_binomial), None, "q_binomial(n, k, q): the q-binomial coefficient"),
    def!("q_gamma", &[2], Some(q_gamma), None, "q_gamma(z, q): the q-gamma function (q; q)_inf/(q^z; q)_inf (1 - q)^(1 - z)"),
    def!("inverse_jacobi_cn", &[2], Some(ij_cn), None, "inverse_jacobi_cn(v, m): the inverse of the Jacobi elliptic function cn"),
    def!("inverse_jacobi_dn", &[2], Some(ij_dn), None, "inverse_jacobi_dn(v, m): the inverse of the Jacobi elliptic function dn"),
    def!("inverse_jacobi_cd", &[2], Some(ij_cd), None, "inverse_jacobi_cd(v, m): the inverse of the Jacobi elliptic function cd"),
    def!("inverse_jacobi_sd", &[2], Some(ij_sd), None, "inverse_jacobi_sd(v, m): the inverse of the Jacobi elliptic function sd"),
    def!("inverse_jacobi_nd", &[2], Some(ij_nd), None, "inverse_jacobi_nd(v, m): the inverse of the Jacobi elliptic function nd"),
    def!("inverse_jacobi_dc", &[2], Some(ij_dc), None, "inverse_jacobi_dc(v, m): the inverse of the Jacobi elliptic function dc"),
    def!("inverse_jacobi_nc", &[2], Some(ij_nc), None, "inverse_jacobi_nc(v, m): the inverse of the Jacobi elliptic function nc"),
    def!("inverse_jacobi_sc", &[2], Some(ij_sc), None, "inverse_jacobi_sc(v, m): the inverse of the Jacobi elliptic function sc"),
    def!("inverse_jacobi_ns", &[2], Some(ij_ns), None, "inverse_jacobi_ns(v, m): the inverse of the Jacobi elliptic function ns"),
    def!("inverse_jacobi_ds", &[2], Some(ij_ds), None, "inverse_jacobi_ds(v, m): the inverse of the Jacobi elliptic function ds"),
    def!("inverse_jacobi_cs", &[2], Some(ij_cs), None, "inverse_jacobi_cs(v, m): the inverse of the Jacobi elliptic function cs"),
    def!("jacobi_amplitude", &[2], Some(jacobi_amplitude), None, "jacobi_amplitude(u, m): the amplitude am(u | m), the angle whose sine is sn(u | m)"),
    def!("jacobi_zn", &[2], Some(jacobi_zn), None, "jacobi_zn(u, m): the Jacobi zeta function Z(u | m) = E(am(u) | m) - E(m) u/K(m)"),
    def!("jacobi_epsilon", &[2], Some(jacobi_epsilon), None, "jacobi_epsilon(u, m): the Jacobi epsilon function E(am(u | m) | m)"),
    def!("spherical_hankel_h1", &[2], Some(spherical_hankel_h1), None, "spherical_hankel_h1(n, z): the spherical Hankel function of the first kind j_n(z) + i y_n(z)"),
    def!("spherical_hankel_h2", &[2], Some(spherical_hankel_h2), None, "spherical_hankel_h2(n, z): the spherical Hankel function of the second kind j_n(z) - i y_n(z)"),
    def!("kelvin_ber", &[1, 2], Some(kelvin_ber), None, "kelvin_ber(nu, x): the Kelvin function ber_nu(x), the real part of J_nu(x e^(3 pi i/4)); kelvin_ber(x) for nu = 0"),
    def!("kelvin_bei", &[1, 2], Some(kelvin_bei), None, "kelvin_bei(nu, x): the Kelvin function bei_nu(x), the imaginary part of J_nu(x e^(3 pi i/4)); kelvin_bei(x) for nu = 0"),
    def!("kelvin_ker", &[1, 2], Some(kelvin_ker), None, "kelvin_ker(nu, x): the Kelvin function ker_nu(x), the real part of e^(-nu pi i/2) K_nu(x e^(pi i/4)); kelvin_ker(x) for nu = 0"),
    def!("kelvin_kei", &[1, 2], Some(kelvin_kei), None, "kelvin_kei(nu, x): the Kelvin function kei_nu(x), the imaginary part of e^(-nu pi i/2) K_nu(x e^(pi i/4)); kelvin_kei(x) for nu = 0"),
    def!("struve_h", &[2], Some(struve_h), None, "struve_h(nu, z): the Struve function H_nu(z)"),
    def!("struve_l", &[2], Some(struve_l), None, "struve_l(nu, z): the modified Struve function L_nu(z)"),
    def!("anger_j", &[2], Some(anger_j), None, "anger_j(nu, z): the Anger function J_nu(z), the integral of cos(nu t - z sin t) over [0, pi], over pi"),
    def!("weber_e", &[2], Some(weber_e), None, "weber_e(nu, z): the Weber function E_nu(z), the integral of sin(nu t - z sin t) over [0, pi], over pi"),
    def!("lommel_s1", &[3], Some(lommel_s1), None, "lommel_s1(mu, nu, z): the Lommel function s_(mu, nu)(z)"),
    def!("lommel_s2", &[3], Some(lommel_s2), None, "lommel_s2(mu, nu, z): the Lommel function S_(mu, nu)(z)"),
    def!("hypergeometric_u", &[3], Some(hypergeometric_u), None, "hypergeometric_u(a, b, z): Tricomi's confluent hypergeometric function U(a, b, z)"),
    def!("whittaker_m", &[3], Some(whittaker_m), None, "whittaker_m(k, m, z): the Whittaker function M_(k, m)(z) = e^(-z/2) z^(m + 1/2) 1F1(m - k + 1/2; 1 + 2 m; z)"),
    def!("whittaker_w", &[3], Some(whittaker_w), None, "whittaker_w(k, m, z): the Whittaker function W_(k, m)(z) = e^(-z/2) z^(m + 1/2) U(m - k + 1/2, 1 + 2 m, z)"),
    def!("parabolic_cylinder_d", &[2], Some(parabolic_d), None, "parabolic_cylinder_d(nu, z): the parabolic cylinder function D_nu(z)"),
    Def { name: "hypergeometric_pfq_regularized", arities: &[3], listable: false, exact: Some(pfq_regularized), numeric: None, summary: "hypergeometric_pfq_regularized(as, bs, z): pFq(as; bs; z) divided by the gamma functions of the bs" },
    def!("fresnel_f", &[1], Some(fresnel_f), None, "fresnel_f(z): the Fresnel auxiliary function f(z)"),
    def!("fresnel_g", &[1], Some(fresnel_g), None, "fresnel_g(z): the Fresnel auxiliary function g(z)"),
    def!("logistic_sigmoid", &[1], Some(logistic_sigmoid), None, "logistic_sigmoid(z): the logistic function 1/(1 + e^-z)"),
    def!("riemann_siegel_theta", &[1], Some(riemann_siegel_theta), None, "riemann_siegel_theta(t): the Riemann-Siegel theta function, the phase of zeta on the critical line"),
    def!("riemann_siegel_z", &[1], Some(riemann_siegel_z), None, "riemann_siegel_z(t): the Riemann-Siegel Z function e^(i theta(t)) zeta(1/2 + i t), real for real t"),
    def!("lerch_phi", &[3], Some(lerch_phi), None, "lerch_phi(z, s, a): the Lerch transcendent, the sum of z^k/(k + a)^s over k >= 0"),
    def!("mittag_leffler_e", &[2, 3], Some(mittag_leffler), None, "mittag_leffler_e(a, z): the Mittag-Leffler function, the sum of z^k/gamma(a k + 1); mittag_leffler_e(a, b, z) with gamma(a k + b)"),
    def!("barnes_g", &[1], Some(barnes_g), None, "barnes_g(z): the Barnes G-function, G(z + 1) = gamma(z) G(z), G(1) = 1"),
    def!("log_barnes_g", &[1], Some(log_barnes_g), None, "log_barnes_g(z): the logarithm of the Barnes G-function, analytic off the negative axis"),
    def!("inverse_gamma_regularized", &[2], Some(inverse_gamma_regularized), None, "inverse_gamma_regularized(a, s): the x with gamma_regularized(a, x) = s"),
    def!("inverse_beta_regularized", &[3], Some(inverse_beta_regularized), None, "inverse_beta_regularized(s, a, b): the x with beta_regularized(x, a, b) = s"),
];

// ---------------------------------------------------------------- Hankel functions (round 24, B23-4, B23-23)

/// Are the parts of the number `h` accurate to `d` decimal digits (balls by their radii; machine and exact parts
/// always)?
pub(crate) fn accurate<'h>(fx: &Fx<'_, '_, '_, 'h>, h: T<'h>, d: u32) -> bool {
    use mc_numeric::cball::CBall;
    let ball = |r: &Real| match r {
        Real::Ball(b, _) => Some(b.clone()),
        _ => None,
    };
    match fx.num(h) {
        Some(Num::Real(r)) => ball(&r).is_none_or(|b| b.has_digits(d)),
        Some(Num::Complex(x, y)) => match (ball(&x), ball(&y)) {
            (Some(bx), Some(by)) => CBall::new(bx, by).has_digits(d),
            (None, None) => true,
            (Some(b), None) | (None, Some(b)) => b.has_digits(d) || b.rad.is_zero(),
        },
        None => false,
    }
}

/// H^(1)_nu(z) = J_nu(z) + i Y_nu(z) (`s` = 1) and H^(2)_nu(z) = J_nu(z) - i Y_nu(z) (`s` = -1) at numeric
/// arguments (DLMF 10.4.3). Where the sum cancels (J and Y both grow like e^|Im z| while H decays: Im z > 0 for
/// H1, Im z < 0 for H2) the machine value of the two parts loses every digit, and a ball sum at a fixed precision
/// can be wider than its value. The sum is therefore formed in balls with `extra` bits beyond the target, raised
/// (doubled, at most 20000 bits) until the result has the digits of the arguments' kind; machine arguments try
/// the machine parts first and keep them when less than four digits cancel.
pub(crate) fn hankel_bessel<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], s: i64) -> R<'h> {
    if !numeric(fx, a) {
        return Ok(None);
    }
    // an order that is an exact integer carried as a number of another kind (`n` turns 1 into 1.000...): the
    // integer, so that Y takes its logarithmic branch instead of dividing by sin(nu pi) = 0
    let a0 = match fx.num(a[0]) {
        Some(Num::Real(Real::Ball(b, _))) if b.rad.is_zero() && b.mid.is_zero() => Some(0),
        Some(Num::Real(Real::Ball(b, _))) if b.rad.is_zero() => {
            let v = crate::ball::f64_of(&b.mid);
            (v.fract() == 0.0 && v.abs() < 1e9).then_some(v as i64)
        }
        _ => None,
    };
    let a_buf = [a0.map_or(a[0], |v| fx.int(v)), a[1]];
    let a = &a_buf[..];
    let Some((zr, zi)) = parts(fx, a[1]) else { return Ok(None) };
    if !zr.is_finite() || !zi.is_finite() {
        return Ok(None);
    }
    let i_s = fx.ar.complex(fx.int(0), fx.int(s));
    let machine = a.iter().any(|&t| fx.num(t).is_some_and(|n| n.kind() == Kind::Machine));
    if machine {
        let j = fx.eval(fx.call("bessel_j", a))?;
        let y = fx.eval(fx.call("bessel_y", a))?;
        if fx.num(j).is_some() && fx.num(y).is_some() {
            let h = fx.eval(fx.add(&[j, fx.mul(&[i_s, y])]))?;
            if fx.num(h).is_some() && mag(fx, h) * 1e4 >= mag(fx, j).max(mag(fx, y)) {
                return Ok(Some(h));
            }
        }
    }
    let Some((wk0, _)) = work(fx, a, 0) else { return Ok(None) };
    // the arguments are balls of radius about 2^-p: H cannot be known to more than p bits less log2 of its condition
    // number (about |z|); a result below the digits asked for is refined by the caller at a higher precision
    let d = if wk0.machine {
        15
    } else {
        ((wk0.p as f64 * std::f64::consts::LOG10_2) as u32).saturating_sub(8 + 2 * (zr.hypot(zi) + 1.0).log10() as u32).max(10)
    };
    // A ball z of radius r is evaluated at its midpoint and the radius carried by the derivative bound
    // |H'| r, |H'_nu| <= |H_(nu-1)| + |nu/z| |H_nu| (DLMF 10.6.2): the series of J and Y multiply the radius of
    // their argument by the cancellation e^|z|, which the midpoint evaluation avoids.
    let rad_of = |r: &Real| match r {
        Real::Ball(b, _) => crate::ball::f64_of(&b.rad.to_float()),
        _ => 0.0,
    };
    let (radz, radnu) = match (fx.num(a[1]), fx.num(a[0])) {
        (Some(Num::Real(r)), Some(Num::Real(n))) => (rad_of(&r), rad_of(&n)),
        (Some(Num::Complex(x, y)), Some(Num::Real(n))) => (rad_of(&x) + rad_of(&y), rad_of(&n)),
        _ => (0.0, 1.0),
    };
    let zmod = zr.hypot(zi);
    let use_mid = radnu == 0.0 && radz < 1e-3 * zmod.max(1.0);
    let mut extra = ((zmod * 1.5 + 2.0 * zi.abs() * std::f64::consts::LOG2_E) as u64).min(6_000) + 32;
    for _ in 0..6 {
        let w = wk0.p + extra + 24;
        let (wk, args): (Work, Vec<T<'h>>) = if use_mid {
            let mid = |fx: &Fx<'_, '_, '_, 'h>, r: Real| match r {
                Real::Ball(b, _) => Real::Ball(mc_arith::num::Ball::exact(b.mid.clone()), w),
                Real::Machine(v) => Real::Ball(mc_arith::num::Ball::exact(crate::ball::float_of(v)), w),
                other => {
                    let _ = fx;
                    other.to_kind(Kind::Ball, w)
                }
            };
            let zt = match fx.num(a[1]) {
                Some(Num::Real(r)) => fx.ar.number(&Num::Real(mid(fx, r))),
                Some(Num::Complex(x, y)) => fx.ar.number(&Num::Complex(mid(fx, x), mid(fx, y))),
                None => return Ok(None),
            };
            let nu_t = match fx.num(a[0]) {
                Some(Num::Real(Real::Machine(_))) | Some(Num::Real(Real::Ball(..))) => match fx.num(a[0]) {
                    Some(Num::Real(r)) => fx.ar.number(&Num::Real(mid(fx, r))),
                    _ => a[0],
                },
                _ => a[0],
            };
            let Some((wk, _)) = work(fx, a, extra) else { return Ok(None) };
            (wk, vec![nu_t, zt])
        } else {
            let Some((wk, args)) = work(fx, a, extra) else { return Ok(None) };
            // exact integer orders stay exact (an integer order selects the log-series branch of Y)
            let args: Vec<T<'h>> = a
                .iter()
                .zip(&args)
                .map(|(&orig, &b)| if fx.num(orig).is_some_and(|n| n.kind() == Kind::Exact) && fx.int_of(orig).is_some() { orig } else { b })
                .collect();
            (wk, args)
        };
        let Some(mut h) = hankel_pair(fx, args[0], args[1], i_s)? else { return Ok(None) };
        if use_mid && radz > 0.0 {
            // the derivative bound, doubled
            let Some(hm1) = hankel_pair(fx, fx.sub(args[0], fx.int(1)), args[1], i_s)? else { return Ok(None) };
            let nu_mag = fx.f64_of(args[0]).map_or(0.0, f64::abs);
            let dh = (mag(fx, hm1) + nu_mag / zmod.max(1e-300) * mag(fx, h)) * radz * 2.0;
            if !dh.is_finite() {
                return Ok(None);
            }
            let m = mc_arith::num::Mag::from_float_up(&crate::ball::float_of(dh.max(1e-300)));
            let widen_part = |r: Real| match r {
                Real::Ball(b, p) => Real::Ball(crate::ball::widen(b, &m), p),
                other => other,
            };
            h = match fx.num(h) {
                Some(Num::Real(r)) => fx.ar.number(&Num::Real(widen_part(r))),
                Some(Num::Complex(x, y)) => fx.ar.number(&Num::Complex(widen_part(x), widen_part(y))),
                None => return Ok(None),
            };
        }
        if accurate(fx, h, d) {
            return Ok(wk.out(fx, h, false));
        }
        if extra >= 6_000 {
            break;
        }
        extra = (extra * 2 + 64).min(6_000);
    }
    Ok(None)
}

/// J_nu(z) + i s Y_nu(z) evaluated (`i_s` = i or -i), `None` when a part is not a number.
fn hankel_pair<'h>(fx: &mut Fx<'_, '_, '_, 'h>, nu: T<'h>, z: T<'h>, i_s: T<'h>) -> R<'h> {
    let j = fx.eval(fx.call("bessel_j", &[nu, z]))?;
    let y = fx.eval(fx.call("bessel_y", &[nu, z]))?;
    if fx.num(j).is_none() || fx.num(y).is_none() {
        return Ok(None);
    }
    Ok(Some(fx.eval(fx.add(&[j, fx.mul(&[i_s, y])]))?))
}

/// `num/den` of two functions of `a` (the Jacobi ratios sn/cn, ...) at numeric arguments. Their quotient of
/// balls can have far fewer digits than either (a cancellation inside the theta sums, B23-23: `n(jacobi_cd(5 -
/// 3 i, 1/3), 30)` had 13 digits), and the input radii (2^-p) limit what a ball evaluation can prove. Ball
/// arguments are therefore evaluated at their midpoints, exactly, at p + extra bits (extra from 64, doubled
/// while the quotient lacks the digits, to 4096), and the result is given the radius 2^-(p-8) |q|: the digits the
/// arguments carry less 8 bits for the conditioning (a good enclosure, not a proof; NOTES.md).
pub(crate) fn refined_ratio<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], num: Option<&str>, den: Option<&str>) -> R<'h> {
    let Some((wk0, _)) = work(fx, a, 0) else { return Ok(None) };
    let ev1 = |fx: &mut Fx<'_, '_, '_, 'h>, name: Option<&str>, args: &[T<'h>]| -> Res<Option<T<'h>>> {
        match name {
            Some(n) => {
                let v = fx.eval(fx.call(n, args))?;
                Ok(fx.num(v).map(|_| v))
            }
            None => Ok(Some(fx.int(1))),
        }
    };
    let digits_goal = ((wk0.p as f64 * std::f64::consts::LOG10_2) as u32).saturating_sub(6).max(10);
    let mut extra = 64u64;
    for _ in 0..7 {
        let w = wk0.p + extra + 24;
        let mid = |r: Real| match r {
            Real::Ball(b, _) => Real::Ball(Ball::exact(b.mid.clone()), w),
            other => other.to_kind(Kind::Ball, w),
        };
        let args: Vec<T<'h>> = a
            .iter()
            .map(|&t| match fx.num(t) {
                Some(Num::Real(r)) => fx.ar.number(&Num::Real(mid(r))),
                Some(Num::Complex(x, y)) => fx.ar.number(&Num::Complex(mid(x), mid(y))),
                None => t,
            })
            .collect();
        let (Some(nv), Some(dv)) = (ev1(fx, num, &args)?, ev1(fx, den, &args)?) else { return Ok(None) };
        if fx.num(dv).is_some_and(|n| n.is_zero()) {
            return Ok(None);
        }
        let q = fx.eval(fx.div(nv, dv))?;
        if fx.num(q).is_none() {
            return Ok(None);
        }
        if accurate(fx, q, digits_goal) {
            let widen_to = mc_arith::num::Mag::pow2(-(wk0.p as i64) + 8 + mag(fx, q).max(1e-300).log2().ceil() as i64);
            let wide = |r: Real| match r {
                Real::Ball(b, _) => Real::Ball(crate::ball::widen(b, &widen_to), wk0.p),
                other => other,
            };
            return Ok(fx.num(q).map(|n| {
                fx.ar.number(&match n {
                    Num::Real(r) => Num::Real(wide(r)),
                    Num::Complex(x, y) => Num::Complex(wide(x), wide(y)),
                })
            }));
        }
        extra *= 2;
        if extra > 4096 {
            break;
        }
    }
    Ok(None)
}
