//! Special functions added for coverage (package cov-num), defined by formulas (`mc_numeric::formula`) or by
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
struct Work {
    machine: bool,
    p: u64,
    w: u64,
    real: bool,
}

/// Every argument a number and one inexact: the working form with `extra` bits beyond the target, and the
/// arguments as balls at the working precision.
fn work<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>], extra: u64) -> Option<(Work, Vec<T<'h>>)> {
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
    fn out<'h>(&self, fx: &Fx<'_, '_, '_, 'h>, v: T<'h>, real: bool) -> Option<T<'h>> {
        let n = fx.num(v)?;
        let n = match n {
            Num::Complex(x, _) if real => Num::Real(x),
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
fn mag<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> f64 {
    match fx.num(t) {
        Some(Num::Real(r)) => r.to_f64().abs(),
        Some(Num::Complex(x, y)) => x.to_f64().hypot(y.to_f64()),
        None => 0.0,
    }
}

/// The parts of a number as `f64`s.
fn parts<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(f64, f64)> {
    match fx.num(t)? {
        Num::Real(r) => Some((r.to_f64(), 0.0)),
        Num::Complex(x, y) => Some((x.to_f64(), y.to_f64())),
    }
}

/// Every argument a number and one inexact?
fn numeric<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> bool {
    let nums: Option<Vec<Num>> = a.iter().map(|&t| fx.num(t)).collect();
    nums.is_some_and(|ns| ns.iter().any(|n| n.kind() != Kind::Exact))
}

/// `name(args)` evaluated; a closed form of numeric arguments (a formula with `pi`, say) is then evaluated
/// numerically at the arguments' precision.
fn ev<'h>(fx: &mut Fx<'_, '_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
    let c = fx.call(name, args);
    let v = fx.eval(c)?;
    if fx.num(v).is_some() || name == "n" {
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
        return fx.eval(fx.call("n", &[v]));
    }
    match prec {
        Some(p) => fx.eval(fx.call("n", &[v, fx.int(digits(p))])),
        None => Ok(v),
    }
}

/// The decimal digits of p bits (rounded up).
fn digits(p: u64) -> i64 {
    (p as f64 * std::f64::consts::LOG10_2).ceil() as i64 + 1
}

/// An exact number as a ball at w bits (other numbers unchanged).
fn ballify<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, w: u64) -> T<'h> {
    match fx.num(t) {
        Some(n) if n.kind() == Kind::Exact => fx.ar.number(&n.to_kind(Kind::Ball, w)),
        _ => t,
    }
}

/// pi as a ball at w bits.
fn pi_at<'h>(fx: &mut Fx<'_, '_, '_, 'h>, w: u64) -> Res<T<'h>> {
    let d = fx.int(digits(w));
    let pi = fx.sym("pi");
    ev(fx, "n", &[pi, d])
}

/// b^e evaluated.
fn evpow<'h>(fx: &mut Fx<'_, '_, '_, 'h>, b: T<'h>, e: T<'h>) -> Res<T<'h>> {
    let t = fx.pow(b, e);
    fx.eval(t)
}

/// Is the number a non-positive integer (a pole of the gamma function)?
fn gamma_pole<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    match parts(fx, t) {
        Some((x, y)) => y == 0.0 && x <= 0.0 && x == x.floor(),
        None => false,
    }
}

/// 1/Gamma(x) for k = 0, 1, ..., n at x0 + k: zeros at the poles, the recurrence 1/Gamma(x + 1) = 1/(x Gamma(x))
/// after the last one.
fn rgamma_seq<'h>(fx: &mut Fx<'_, '_, '_, 'h>, x0: T<'h>, n: usize) -> Res<Option<Vec<T<'h>>>> {
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
const MAX_TERMS: usize = 20_000;

/// Sum terms t_k (k = 0, 1, ...) given by `term` until, after `min` terms, one is below 2^-w of the sum. `None`
/// when the series does not settle within [`MAX_TERMS`].
fn sum_series<'h>(
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
fn theta_terms(q: (f64, f64), im_u: f64, w: u64) -> Option<usize> {
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
    let Some(k) = fx.int_of(a[0]).filter(|k| (1..=4).contains(k)) else { return Ok(None) };
    let (u, q) = (a[1], a[2]);
    if fx.is_int(q, 0) {
        return Ok(Some(match (k, deriv) {
            (3 | 4, false) => fx.int(1),
            _ => fx.int(0),
        }));
    }
    if fx.is_int(u, 0) && ((k == 1 && !deriv) || (k != 1 && deriv)) {
        return Ok(Some(fx.int(0)));
    }
    if !numeric(fx, &a[1..]) {
        return Ok(None);
    }
    let im = parts(fx, u).map_or(0.0, |p| p.1.abs());
    let Some((wk, xs)) = work(fx, &a[1..], (im * 3.0) as u64 + 8) else { return Ok(None) };
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
    let k = fx.call("elliptic_k", &[m]);
    let q = fx.call("elliptic_nome_q", &[m]);
    let v = fx.div(fx.mul(&[fx.sym("pi"), z]), fx.mul(&[fx.int(2), k]));
    let th = |fx: &Fx<'_, '_, '_, 'h>, i: i64, u: T<'h>| fx.call("elliptic_theta", &[fx.int(i), u, q]);
    Ok(Some(match which {
        0 => {
            let d = fx.call("elliptic_theta_prime", &[fx.int(1), fx.int(0), q]);
            fx.div(fx.mul(&[fx.int(2), k, th(fx, 1, v)]), fx.mul(&[fx.sym("pi"), d]))
        }
        1 => fx.div(th(fx, 2, v), th(fx, 2, fx.int(0))),
        2 => fx.div(th(fx, 3, v), th(fx, 3, fx.int(0))),
        _ => fx.div(th(fx, 4, v), th(fx, 4, fx.int(0))),
    }))
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
    let tau = a[0];
    if parts(fx, tau).is_none_or(|p| p.1 <= 0.0) {
        return Ok(None);
    }
    // e^(i pi tau/12) (q; q)_inf with q = e^(2 i pi tau)
    let pre = fx.call("exp", &[fx.mul(&[fx.rat(1, 12), fx.i(), fx.sym("pi"), tau])]);
    let q = fx.call("exp", &[fx.mul(&[fx.int(2), fx.i(), fx.sym("pi"), tau])]);
    Ok(Some(fx.mul(&[pre, fx.call("q_pochhammer", &[q, q])])))
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
        return Ok(None);
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
        if let Some(n) = fx.int_of(a[2]).filter(|n| n.abs() <= 1000) {
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

fn q_factorial<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (q, n) = (a[0], a[1]);
    if fx.int_of(n).is_some_and(|n| (0..=1000).contains(&n)) {
        let n = fx.int_of(n).unwrap_or(0);
        // [k]_q! = prod (1 - q^k)/(1 - q) = prod (1 + q + ... + q^(k-1))
        let fs: Vec<T<'h>> = (1..=n).map(|k| fx.add(&(0..k).map(|j| fx.pow(q, fx.int(j))).collect::<Vec<_>>())).collect();
        return Ok(Some(fx.mul(&fs)));
    }
    if !numeric(fx, a) {
        return Ok(None);
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
            let f = |m: i64| fx.call("q_factorial", &[q, fx.int(m)]);
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
        return Ok(Some(fx.call("q_factorial", &[q, fx.int(n - 1)])));
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
    Ok(Some(fx.call("elliptic_f", &[phi, m])))
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
    if !numeric(fx, a) || fx.f64_of(u).is_none() || fx.f64_of(m).is_none_or(|v| v >= 1.0) {
        return Ok(None);
    }
    let k = ev(fx, "elliptic_k", &[m])?;
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
/// extends them analytically off the positive axis.
fn kelvin<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: u8) -> R<'h> {
    let (nu, x) = if a.len() == 1 { (fx.int(0), a[0]) } else { (a[0], a[1]) };
    if fx.is_int(x, 0) && fx.is_int(nu, 0) && which < 2 {
        return Ok(Some(fx.int(if which == 0 { 1 } else { 0 })));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let pi = fx.sym("pi");
    let rot = |fx: &Fx<'_, '_, '_, 'h>, k: i64| fx.call("exp", &[fx.mul(&[fx.rat(k, 4), fx.i(), pi])]);
    let (p, m) = if which < 2 {
        (
            fx.call("bessel_j", &[nu, fx.mul(&[x, rot(fx, 3)])]),
            fx.call("bessel_j", &[nu, fx.mul(&[x, rot(fx, -3)])]),
        )
    } else {
        let e = |fx: &Fx<'_, '_, '_, 'h>, s: i64| fx.call("exp", &[fx.mul(&[fx.rat(s, 2), nu, fx.i(), pi])]);
        // K_nu(w) = (pi/2) i^(nu + 1) (J_nu(i w) + i Y_nu(i w)) for -pi < arg w <= pi/2 (DLMF 10.27.8)
        let k = |fx: &Fx<'_, '_, '_, 'h>, w: T<'h>| {
            let iw = fx.mul(&[fx.i(), w]);
            let h1 = fx.add(&[fx.call("bessel_j", &[nu, iw]), fx.mul(&[fx.i(), fx.call("bessel_y", &[nu, iw])])]);
            let ip = fx.call("exp", &[fx.mul(&[fx.rat(1, 2), fx.add(&[nu, fx.int(1)]), fx.i(), pi])]);
            fx.mul(&[fx.rat(1, 2), pi, ip, h1])
        };
        (
            fx.mul(&[e(fx, -1), k(fx, fx.mul(&[x, rot(fx, 1)]))]),
            fx.mul(&[e(fx, 1), k(fx, fx.mul(&[x, rot(fx, -1)]))]),
        )
    };
    let v = if which % 2 == 0 {
        fx.mul(&[fx.rat(1, 2), fx.add(&[p, m])])
    } else {
        fx.div(fx.sub(p, m), fx.mul(&[fx.int(2), fx.i()]))
    };
    let real = [nu, x].iter().all(|&t| fx.f64_of(t).is_some()) && fx.f64_of(x).is_some_and(|v| v > 0.0);
    Ok(Some(if real { fx.call("re", &[v]) } else { v }))
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

/// The bits a series of size e^|z| loses to cancellation, plus a margin.
fn cancel_bits<'h>(fx: &Fx<'_, '_, '_, 'h>, z: T<'h>, scale: f64) -> u64 {
    (mag(fx, z) * scale * std::f64::consts::LOG2_E) as u64 + 8
}

/// Struve H (`l` false) or L: sum (-+1)^k (z/2)^(2k+nu+1)/(Gamma(k + 3/2) Gamma(k + nu + 3/2)).
fn struve<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], l: bool) -> R<'h> {
    let (nu, z) = (a[0], a[1]);
    if fx.is_int(z, 0) && fx.f64_of(nu).is_some_and(|v| v > -1.0) {
        return Ok(Some(fx.int(0)));
    }
    if !numeric(fx, a) {
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

/// Lommel's s_(mu, nu)(z) = sum_k (-1)^k z^(mu + 1 + 2k)/prod_{j<=k} ((mu + 2j + 1)^2 - nu^2).
fn lommel_s1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if !numeric(fx, a) {
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
fn tricomi<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: T<'h>, b: T<'h>, z: T<'h>, p: u64) -> Res<Option<T<'h>>> {
    let one = fx.int(1);
    let form = |fx: &mut Fx<'_, '_, '_, 'h>, b: T<'h>| -> Res<Option<T<'h>>> {
        let rg = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>| -> Res<Option<T<'h>>> {
            if gamma_pole(fx, x) {
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
            Ball::exact(mc_arith::num::Float::new(false, mc_arith::num::Nat::from_u64(1), -(p as i64) - 8)),
            p,
        )));
        let (Some(u1), Some(u2)) = (form(fx, fx.add(&[b, eps]))?, form(fx, fx.sub(b, eps))?) else { return Ok(None) };
        return Ok(Some(fx.mul(&[fx.rat(1, 2), fx.add(&[u1, u2])])));
    }
    form(fx, b)
}

fn hypergeometric_u<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (x, b, z) = (a[0], a[1], a[2]);
    if fx.is_int(x, 0) {
        return Ok(Some(fx.int(1)));
    }
    // U(a, a + 1, z) = z^-a
    if fx.num(x).is_some() && fx.num(b).is_some() && fx.num(fx.sub(b, x)).is_some_and(|n| n.is_exact_one()) {
        return Ok(Some(fx.pow(z, fx.neg(x))));
    }
    if !numeric(fx, a) || a.iter().any(|&t| fx.f64_of(t).is_none()) || fx.f64_of(z).is_some_and(|v| v <= 0.0) {
        return Ok(None);
    }
    let base = if fx.machine_of(z).is_some() || a.iter().any(|&t| fx.machine_of(t).is_some()) { 53 } else { 0 };
    let p0 = match fx.num(z) {
        Some(Num::Real(r)) => r.prec().unwrap_or(base).max(53),
        _ => 53,
    };
    let integer_b = fx.f64_of(b).is_some_and(|v| v == v.round());
    let extra = cancel_bits(fx, z, 1.0) + if integer_b { 2 * p0 + 16 } else { 0 };
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let Some(v) = tricomi(fx, xs[0], xs[1], xs[2], wk.p + 8)? else { return Ok(None) };
    Ok(wk.out(fx, v, true))
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
    if !numeric(fx, a) {
        return Ok(None);
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
    let inner = fx.sub(fx.mul(&[m1, r1]), fx.mul(&[fx.sqrt(fx.int(2)), z, m2, r2]));
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
    if !numeric(fx, a) {
        return Ok(None);
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
    let args = [al, b, z];
    if !numeric(fx, &args) {
        return Ok(None);
    }
    let (Some(af), Some(bf)) = (fx.f64_of(al), fx.f64_of(b)) else { return Ok(None) };
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
    Ok(Some(fx.call("exp", &[fx.call("log_barnes_g", a)])))
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
    def!("q_factorial", &[2], Some(q_factorial), None, "q_factorial(q, n): the q-factorial (q; q)_n/(1 - q)^n"),
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
