//! The natives: list threading, argument kinds, and the dispatch to the exact rules, the machine path and the
//! ball path.
//!
//! - Lists thread (as for the arithmetic functions): `gamma([1, 2, 3])` is `[1, 1, 2]`.
//! - A machine real among the arguments makes the call a machine computation (contagion); outside the real domain
//!   (K_nu at x < 0, a non-integer polygamma order) the call stays.
//! - Otherwise a ball among the arguments gives a ball at the largest ball precision (exact arguments stay exact
//!   for the ball path); no enclosure (a ball touching a pole) leaves the call unevaluated.
//! - All exact: the exact rules ([`crate::exact`]); a pole is `failure(pole, call)` (decision SF.1); no closed
//!   form leaves the call unevaluated, for `n` to evaluate.
//! - Complex numbers and other terms stay (except the symbolic rules: `polygamma(0, x) = digamma(x)` and the
//!   half-integer `bessel_k` and `bessel_i` closed forms).
use crate::exact::{Exact, Rules};
use crate::exact_more::More;
use crate::functions::Special;
use mc_api::{names, Store, View};
use mc_arith::num::{stopped, Rat, StopScope};
use mc_arith::{Arith, Num, Real};
use mc_core::{Heap, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_numeric::cball::CBall;
use mc_numeric::fun::Mach;
use mc_numeric::machine::C64;
use mc_numeric::numeval::{cball_term, mach_term};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Failure tags of this crate.
pub mod failure_tags {
    /// `failure(pole, call)`: the function has a pole at the exact arguments.
    pub const POLE: &str = "pole";
}

struct Fun {
    /// the variants of one name (one per arity)
    fs: Vec<Special>,
    stop: Arc<AtomicBool>,
}

/// `hypergeometric_pfq(as, bs, z)`.
struct Pfq {
    stop: Arc<AtomicBool>,
}

/// `f([a, b], c)` is `[f(a, c), f(b, c)]`; unequal lengths give `failure(shape_mismatch)`.
fn thread<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Option<Res<Reply<'h>>> {
    let lists: Vec<Option<&[T<'h>]>> = args.iter().map(|&a| cx.as_list(a)).collect();
    let len = lists.iter().flatten().map(|l| l.len()).next()?;
    if lists.iter().flatten().any(|l| l.len() != len) {
        return Some(Ok(Reply::Done(cx.failure(&["shape_mismatch"]))));
    }
    if let Err(e) = cx.tick(len as u64) {
        return Some(Err(e));
    }
    let h = cx.heap();
    let View::App { head, .. } = h.view(t) else { return None };
    let items: Vec<T<'h>> = (0..len)
        .map(|i| {
            let xs: Vec<T<'h>> = args.iter().zip(&lists).map(|(&a, l)| l.map_or(a, |l| l[i])).collect();
            h.app(head, &xs)
        })
        .collect();
    Some(Ok(Reply::Again(cx.list(&items))))
}

fn pole<'h>(h: &'h Heap, t: T<'h>) -> T<'h> {
    let tag = h.sym_term(h.sym(failure_tags::POLE));
    h.call(h.sym(names::FAILURE), &[tag, t])
}

fn exact_reply<'h>(h: &'h Heap, t: T<'h>, e: Option<Exact<'h>>) -> Reply<'h> {
    match e {
        Some(Exact::Value(v)) => Reply::Again(v),
        Some(Exact::Pole) => {
            // a pole with a signed limit from the real side is that infinity (P6N.2)
            if let View::App { head, args } = h.view(t) {
                if let View::Sym(s) = h.view(head) {
                    let ar = Arith::new(h);
                    let n = h.sym_name(s);
                    if let Some(f) = Special::find(&n, args.len()) {
                        if let Some(v) = crate::limits::at_pole(h, &ar, f, args) {
                            return Reply::Done(v);
                        }
                    }
                }
            }
            Reply::Done(pole(h, t))
        }
        None => Reply::Decline,
    }
}

/// Calls with an arity the crate has no variant for but a closed form in terms of one: erf(a, b) = erf(b) -
/// erf(a), gamma(s, a, b) = gamma(s, a) - gamma(s, b), gamma_regularized(s, a, b) likewise.
fn generalized<'h>(h: &'h Heap, name: &str, args: &[T<'h>]) -> Option<T<'h>> {
    let ar = Arith::new(h);
    let call = |args: &[T<'h>]| h.call(h.sym(name), args);
    match (name, args) {
        ("erf", [a, b]) => Some(ar.sub(call(&[*b]), call(&[*a]))),
        ("gamma" | "gamma_regularized", [s, a, b]) => Some(ar.sub(call(&[*s, *a]), call(&[*s, *b]))),
        ("polygamma", [z]) => Some(h.call(h.sym("digamma"), &[*z])),
        ("zeta", [s, a]) => hurwitz(h, &ar, *s, *a),
        _ => None,
    }
}

/// The Hurwitz zeta function zeta(s, a): zeta(s) minus the first terms at positive integers a, (2^s - 1) zeta(s)
/// at a = 1/2, the complex machine algorithm for machine numbers.
fn hurwitz<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, s: T<'h>, a: T<'h>) -> Option<T<'h>> {
    let zeta = |x: T<'h>| h.call(h.sym("zeta"), &[x]);
    let (ns, na) = (ar.num(s), ar.num(a));
    if let (Some(x), Some(y)) = (&ns, &na) {
        if x.kind() == mc_arith::Kind::Machine || y.kind() == mc_arith::Kind::Machine {
            let (zs, za) = (machine_c64(x)?, machine_c64(y)?);
            let v = crate::machine::complex::hurwitz_zeta(zs, za)?;
            let real = matches!(x, Num::Real(_)) && matches!(y, Num::Real(_)) && za.re > 0.0;
            return Some(mach_term(ar, if real { Mach::Real(v.re) } else { Mach::Complex(v) }));
        }
    }
    let q = na.as_ref().and_then(Num::as_rat)?;
    if q.is_int() {
        let n = q.num.to_i64().filter(|n| (1..=10_000).contains(n))?;
        let terms: Vec<T<'h>> = (1..n).map(|k| ar.neg(ar.pow(h.int(k), ar.neg(s)))).collect();
        let mut all = vec![zeta(s)];
        all.extend(terms);
        return Some(ar.add(&all));
    }
    if *q == Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64(2)) {
        return Some(ar.mul(&[ar.sub(ar.pow(h.int(2), s), h.int(1)), zeta(s)]));
    }
    None
}

/// The value at machine arguments from the enclosure at 192 bits (a complex-valued point, a region the machine
/// algorithms do not cover), rounded; `None` without 14 correct digits. `all_complex`: the arguments are complex.
fn machine_by_balls(f: Special, zs: &[C64], all_complex: bool) -> Option<Mach> {
    const P: u64 = 192;
    let ball = |v: f64| mc_arith::num::float::f64_to_dyadic(v).map(|(neg, man, exp)| mc_arith::num::Ball::exact(mc_arith::num::Float::new(neg, man, exp)));
    let args: Option<Vec<CBall>> = zs.iter().map(|z| Some(CBall::new(ball(z.re)?, ball(z.im)?))).collect();
    let args = args?;
    let _ = all_complex;
    // arguments beyond 1e12 need more than the 192-bit enclosures can give in reasonable time (X23-4, X23-6)
    if zs.iter().any(|z| z.re.abs().max(z.im.abs()) > 1e12) && !matches!(f, Special::JacobiSn | Special::JacobiCn | Special::JacobiDn) {
        return None;
    }
    let r = f.ball_complex(&args, P)?;
    if !r.has_digits(14) && !r.is_exact_zero() {
        return None;
    }
    let re = Real::Ball(r.re.clone(), P).to_f64();
    let im = Real::Ball(r.im.clone(), P).to_f64();
    // an imaginary ball that contains 0 and is 1e-40 below the real part (2F1(5, 6; 5; 4.5) = 3.5^-6 on the cut): the
    // value is real to every digit the machine can hold
    let negligible = r.im.contains_zero() && re != 0.0 && im.abs() <= 1e-40 * re.abs();
    Some(if r.is_real() || negligible { Mach::Real(re) } else { Mach::Complex(C64::new(re, im)) })
}

fn zs_by_balls(f: Special, nums: &[Option<Num>]) -> Option<Mach> {
    let zs: Option<Vec<C64>> = nums.iter().flatten().map(machine_c64).collect();
    machine_by_balls(f, &zs?, true)
}

impl Native for Fun {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        // a truncated power series: the series of the composition (gap G59)
        if let Some(v) = mc_numeric::series_hook::apply(cx, &self.stop, self.fs[0].name(), args)? {
            return Ok(Reply::Done(v));
        }
        if mc_numeric::uncertain::any(cx.heap(), args) {
            if let Some(v) = mc_numeric::uncertain::apply(cx, t, args)? {
                return Ok(Reply::Again(v));
            }
        }
        let Some(f) = self.fs.iter().copied().find(|f| f.arity() == args.len()) else {
            if let Some(r) = thread(cx, t, args) {
                return r;
            }
            return Ok(match generalized(cx.heap(), self.fs[0].name(), args) {
                Some(v) => Reply::Again(v),
                None => Reply::Decline,
            });
        };
        if let Some(r) = thread(cx, t, args) {
            return r;
        }
        if let Some(xs) = mc_numeric::formula::numeric_arguments(cx, args)? {
            let h = cx.heap();
            let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
            return Ok(Reply::Again(h.app(head, &xs)));
        }
        let h = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        if let Some(v) = crate::limits::at_infinity(h, &ar, f, args) {
            return Ok(Reply::Again(v));
        }
        let nums: Vec<Option<Num>> = args.iter().map(|&a| ar.num(a)).collect();
        let reals: Option<Vec<Real>> = nums
            .iter()
            .map(|n| match n {
                Some(Num::Real(r)) => Some(r.clone()),
                _ => None,
            })
            .collect();
        let rules = Rules::new(h, &ar);
        let more = More::new(h, &ar);
        let Some(reals) = reals else {
            // complex machine numbers: the complex machine algorithms
            if nums.iter().all(Option::is_some)
                && nums.iter().flatten().any(|n| matches!(n, Num::Complex(Real::Machine(_), _) | Num::Real(Real::Machine(_))))
            {
                let zs: Option<Vec<C64>> = nums.iter().flatten().map(machine_c64).collect();
                if let Some(z) = zs.and_then(|zs| f.machine_complex(&zs)) {
                    cx.tick(1)?;
                    return Ok(Reply::Done(mach_term(&ar, Mach::Complex(z))));
                }
                let _scope = StopScope::enter(Some(&self.stop));
                if let Some(z) = zs_by_balls(f, &nums) {
                    cx.tick(1)?;
                    return Ok(Reply::Done(mach_term(&ar, z)));
                }
                return Ok(match complex_formula(h, &ar, f, args) {
                    Some(v) => Reply::Again(v),
                    None => Reply::Decline,
                });
            }
            // complex balls: the complex-ball enclosures
            if nums.iter().all(Option::is_some) && nums.iter().flatten().any(|n| n.kind() == mc_arith::Kind::Ball) {
                let p = nums
                    .iter()
                    .flatten()
                    .filter_map(|n| match n {
                        Num::Real(r) => r.prec(),
                        Num::Complex(a, b) => a.prec().max(b.prec()),
                    })
                    .max()
                    .unwrap_or(64);
                let zs: Vec<CBall> = nums
                    .iter()
                    .flatten()
                    .map(|n| {
                        let ball = |r: &Real| match r.to_kind(mc_arith::Kind::Ball, p) {
                            Real::Ball(b, _) => b,
                            _ => mc_arith::num::Ball::from_i64(0),
                        };
                        match n {
                            Num::Real(r) => CBall::real(ball(r)),
                            Num::Complex(a, b) => CBall::new(ball(a), ball(b)),
                        }
                    })
                    .collect();
                cx.tick(1 + p / 256)?;
                let r = {
                    let _scope = StopScope::enter(Some(&self.stop));
                    f.ball_complex(&zs, p)
                };
                return Ok(match r {
                    Some(z) => Reply::Done(cball_term(&ar, z, p)),
                    None => Reply::Decline,
                });
            }
            if !f.is_original() {
                return Ok(exact_reply(h, t, more.rule(f, args)));
            }
            // indeterminate arguments, and the gamma family at complex_infinity
            let undefined = |n: &str| h.sym_term(h.sym(n));
            if args.contains(&undefined("indeterminate"))
                || (matches!(f, Special::Gamma | Special::LogGamma | Special::Digamma | Special::Polygamma)
                    && args.last() == Some(&undefined("complex_infinity")))
            {
                return Ok(Reply::Done(undefined("indeterminate")));
            }
            // symbolic arguments: the rules that hold for any x
            return Ok(match f {
                // sn, cn, dn at x + a K(m) + b i K(1 - m)
                Special::JacobiSn | Special::JacobiCn | Special::JacobiDn if args.len() == 2 => {
                    let p = match f {
                        Special::JacobiSn => 's',
                        Special::JacobiCn => 'c',
                        _ => 'd',
                    };
                    match crate::jshift::reduce(h, &ar, p, 'n', args[0], args[1]) {
                        Some(v) => Reply::Again(v),
                        None => Reply::Decline,
                    }
                }
                Special::Polygamma if nums[0].as_ref().is_some_and(|n| n.is_exact_zero()) => {
                    Reply::Again(h.call(h.sym("digamma"), &[args[1]]))
                }
                Special::BesselK | Special::BesselI if nums[1].is_none() => {
                    match nums[0].as_ref().and_then(Num::as_rat) {
                        Some(nu) if f == Special::BesselK => exact_reply(h, t, rules.bessel_k(nu, args[1])),
                        Some(nu) => exact_reply(h, t, rules.bessel_i(nu, args[1])),
                        None => Reply::Decline,
                    }
                }
                _ => Reply::Decline,
            });
        };
        if reals.iter().any(|r| matches!(r, Real::Machine(_))) {
            // the 192-bit fallbacks below honour the stop flag (X23-4)
            let _scope = StopScope::enter(Some(&self.stop));
            let xs: Vec<f64> = reals.iter().map(Real::to_f64).collect();
            // outside the real domain: the principal complex value (as the elementary functions, decision L.9)
            // the elliptic integrals beyond m = 1 lie on their cuts: the value from below (Im m = -0), from the
            // complex-ball enclosure rounded to a machine number
            let complex_value = |xs: &[f64]| {
                if f == Special::EllipticF && xs.len() == 2 {
                    if let Some(z) = crate::fast33::elliptic_f_beyond(xs[0], xs[1]) {
                        return Some(z);
                    }
                }
                if f == Special::EllipticK && xs.len() == 1 {
                    if let Some(z) = crate::fast33::elliptic_k_above_one(xs[0]) {
                        return Some(z);
                    }
                }
                let zs: Vec<C64> = xs.iter().map(|&x| C64::real(x)).collect();
                if !matches!(f, Special::EllipticK | Special::EllipticE | Special::EllipticEInc | Special::EllipticF) {
                    if let Some(z) = f.machine_complex(&zs) {
                        return Some(z);
                    }
                }
                match machine_by_balls(f, &zs, false) {
                    Some(Mach::Complex(z)) => Some(z),
                    Some(Mach::Real(r)) => Some(C64::real(r)),
                    None => None,
                }
            };
            if !f.real_domain(&xs) {
                cx.tick(1)?;
                return Ok(match complex_value(&xs) {
                    // a value with an exactly zero imaginary part is real (2F1(5, 6; 5; 4.5) = (-3.5)^-6)
                    Some(z) if z.im == 0.0 => Reply::Done(mach_term(&ar, Mach::Real(z.re))),
                    Some(z) => Reply::Done(mach_term(&ar, Mach::Complex(z))),
                    None => Reply::Decline,
                });
            }
            cx.tick(1)?;
            let v = f.machine_value(&xs);
            if matches!(v, Mach::Real(r) if r.is_nan()) && xs.iter().all(|x| x.is_finite()) {
                if let Some(z) = complex_value(&xs) {
                    let v = if z.im == 0.0 { Mach::Real(z.re) } else { Mach::Complex(z) };
                    return Ok(Reply::Done(mach_term(&ar, v)));
                }
            }
            if !f.is_original() && matches!(v, Mach::Real(r) if r.is_nan()) && !xs.iter().any(|x| x.is_nan()) {
                // no real value (a complex-valued or undefined point): the call stays
                return Ok(Reply::Decline);
            }
            return Ok(Reply::Done(mach_term(&ar, v)));
        }
        if let Some(p) = reals.iter().filter_map(Real::prec).max() {
            cx.tick(1 + p / 256)?;
            let r = {
                let _scope = StopScope::enter(Some(&self.stop));
                f.ball_value(&reals, p)
            };
            if stopped() || cx.stopped() {
                cx.tick(0)?;
                let tag = h.sym_term(h.sym(names::failure::INTERRUPTED));
                return Ok(Reply::Done(h.call(h.sym(names::FAILURE), &[tag, t])));
            }
            return Ok(match r {
                Some(z) => Reply::Done(cball_term(&ar, z, p)),
                None => Reply::Decline,
            });
        }
        let q: Vec<&Rat> = reals
            .iter()
            .map(|r| match r {
                Real::Exact(q) => q,
                // invariant: inexact arguments returned above, so only exact reals remain
                _ => unreachable!("only exact reals remain"),
            })
            .collect();
        let e = match f {
            Special::Gamma => rules.gamma(q[0]),
            Special::LogGamma => rules.log_gamma(q[0]),
            Special::Digamma => rules.digamma(q[0]),
            Special::Polygamma => match q[0].num.to_i64().filter(|_| q[0].is_int()) {
                Some(0) => return Ok(Reply::Again(h.call(h.sym("digamma"), &[args[1]]))),
                Some(n) => rules.polygamma(n, q[1]),
                None => None,
            },
            Special::Beta => rules.beta(q[0], q[1]),
            Special::Zeta => rules.zeta(q[0]),
            Special::BesselK => rules.bessel_k(q[0], args[1]),
            Special::BesselI => rules.bessel_i(q[0], args[1]),
            _ => more.rule(f, args),
        };
        Ok(exact_reply(h, t, e))
    }
}

/// Complex machine arguments without a complex algorithm of their own: the value as a formula in functions that
/// have one (gap G62): the incomplete gamma and beta functions through the confluent and Gauss hypergeometric
/// functions, E_n(z) through the incomplete gamma function.
fn complex_formula<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, f: Special, a: &[T<'h>]) -> Option<T<'h>> {
    let call = |name: &str, xs: &[T<'h>]| h.call(h.sym(name), xs);
    let one = h.int(1);
    Some(match (f, a) {
        // B_z(a, b) = z^a/a 2F1(a, 1 - b; a + 1; z)
        (Special::BetaInc, &[z, p, q]) => ar.mul(&[
            ar.pow(z, p),
            ar.pow(p, h.int(-1)),
            call("hypergeometric_2f1", &[p, ar.sub(one, q), ar.add(&[p, one]), z]),
        ]),
        (Special::BetaRegularized, &[z, p, q]) => ar.div(call("beta", &[z, p, q]), call("beta", &[p, q])),
        // Gamma(a, z) = Gamma(a) - z^a/a 1F1(a; a + 1; -z)
        (Special::GammaInc, &[p, z]) if !ar.num(p).is_some_and(|n| n.as_rat().is_some_and(|q| q.is_int() && (q.num.neg || q.is_zero()))) => {
            let lower = ar.mul(&[ar.pow(z, p), ar.pow(p, h.int(-1)), call("hypergeometric_1f1", &[p, ar.add(&[p, one]), ar.neg(z)])]);
            ar.sub(call("gamma", &[p]), lower)
        }
        (Special::GammaRegularized, &[p, z]) => ar.div(call("gamma", &[p, z]), call("gamma", &[p])),
        // E_n(z) = z^(n - 1) Gamma(1 - n, z)
        (Special::ExpIntegralE, &[n, z]) => ar.mul(&[ar.pow(z, ar.sub(n, one)), call("gamma", &[ar.sub(one, n), z])]),
        _ => return None,
    })
}

/// A machine-precision complex value of a number (exact parts are rounded).
fn machine_c64(n: &Num) -> Option<C64> {
    Some(match n {
        Num::Real(r) => C64::new(r.to_f64(), 0.0),
        Num::Complex(a, b) => C64::new(a.to_f64(), b.to_f64()),
    })
}

impl Native for Pfq {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if args.len() != 3 {
            return Ok(Reply::Decline);
        }
        let (Some(a), Some(b)) = (cx.as_list(args[0]), cx.as_list(args[1])) else { return Ok(Reply::Decline) };
        if let Some(zs) = cx.as_list(args[2]) {
            // a list of z threads
            cx.tick(zs.len() as u64)?;
            let h = cx.heap();
            let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
            let items: Vec<T<'h>> = zs.iter().map(|&z| h.app(head, &[args[0], args[1], z])).collect();
            return Ok(Reply::Again(cx.list(&items)));
        }
        let h = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let z = ar.num(args[2]);
        if z.as_ref().is_some_and(Num::is_exact_zero) {
            return Ok(Reply::Again(ar.int(1)));
        }
        // a zero numerator parameter: 1
        if a.iter().any(|&x| ar.num(x).is_some_and(|v| v.is_exact_zero())) {
            return Ok(Reply::Again(ar.int(1)));
        }
        // dispatch to the named functions of fixed arity
        let named = match (a.len(), b.len()) {
            (0, 1) => Some(("hypergeometric_0f1", vec![b[0], args[2]])),
            (1, 1) => Some(("hypergeometric_1f1", vec![a[0], b[0], args[2]])),
            (2, 1) => Some(("hypergeometric_2f1", vec![a[0], a[1], b[0], args[2]])),
            _ => None,
        };
        if let Some((name, xs)) = named {
            return Ok(Reply::Again(h.call(h.sym(name), &xs)));
        }
        let all: Vec<T<'h>> = a.iter().chain(b.iter()).copied().chain(std::iter::once(args[2])).collect();
        let reals: Option<Vec<Real>> = all
            .iter()
            .map(|&x| match ar.num(x) {
                Some(Num::Real(r)) => Some(r),
                _ => None,
            })
            .collect();
        let Some(reals) = reals else {
            // complex machine numbers: the series in complex machine arithmetic (round 37), else unevaluated here
            let nums: Option<Vec<Num>> = all.iter().map(|&x| ar.num(x)).collect();
            if let Some(nums) = nums.filter(|ns| ns.iter().any(|n| n.kind() == mc_arith::Kind::Machine)) {
                let cs: Option<Vec<C64>> = nums.iter().map(machine_c64).collect();
                if let Some(cs) = cs {
                    let (ca, rest) = cs.split_at(a.len());
                    let (cb, cz) = rest.split_at(b.len());
                    if let Some(v) = crate::machine::complex::hyp_pfq(ca, cb, cz[0]) {
                        cx.tick(1)?;
                        return Ok(Reply::Done(mach_term(&ar, Mach::Complex(v))));
                    }
                }
                return Ok(Reply::Decline);
            }
            // complex balls (no machine number): the series on complex balls with its rigorous tail
            if let Some(nums) = all.iter().map(|&x| ar.num(x)).collect::<Option<Vec<Num>>>() {
                if nums.iter().any(|n| n.kind() == mc_arith::Kind::Ball) {
                    let p = nums
                        .iter()
                        .filter_map(|n| match n {
                            Num::Real(r) => r.prec(),
                            Num::Complex(x, y) => x.prec().max(y.prec()),
                        })
                        .max()
                        .unwrap_or(64);
                    let ball = |r: &Real| match r.to_kind(mc_arith::Kind::Ball, p) {
                        Real::Ball(b, _) => b,
                        _ => mc_arith::num::Ball::from_i64(0),
                    };
                    let cs: Vec<CBall> = nums
                        .iter()
                        .map(|n| match n {
                            Num::Real(r) => CBall::real(ball(r)),
                            Num::Complex(x, y) => CBall::new(ball(x), ball(y)),
                        })
                        .collect();
                    let (ca, rest) = cs.split_at(a.len());
                    let (cb, cz) = rest.split_at(b.len());
                    cx.tick(1 + p / 256)?;
                    let r = {
                        let _scope = StopScope::enter(Some(&self.stop));
                        crate::ball::cspecial::hyp_pfq(ca, cb, &cz[0], p)
                    };
                    if stopped() || cx.stopped() {
                        cx.tick(0)?;
                        let tag = h.sym_term(h.sym(names::failure::INTERRUPTED));
                        return Ok(Reply::Done(h.call(h.sym(names::FAILURE), &[tag, t])));
                    }
                    if let Some(z) = r {
                        return Ok(Reply::Done(cball_term(&ar, z, p)));
                    }
                }
            }
            return Ok(Reply::Decline);
        };
        let (ra, rest) = reals.split_at(a.len());
        let (rb, rz) = rest.split_at(b.len());
        if reals.iter().any(|r| matches!(r, Real::Machine(_))) {
            // the 192-bit fallbacks below honour the stop flag (X23-4)
            let _scope = StopScope::enter(Some(&self.stop));
            let fa: Vec<f64> = ra.iter().map(Real::to_f64).collect();
            let fb: Vec<f64> = rb.iter().map(Real::to_f64).collect();
            let v = crate::machine::hyper::hyp_pfq(&fa, &fb, rz[0].to_f64());
            if v.is_nan() {
                return Ok(Reply::Decline);
            }
            cx.tick(1)?;
            return Ok(Reply::Done(mach_term(&ar, Mach::Real(v))));
        }
        let Some(p) = reals.iter().filter_map(Real::prec).max() else { return Ok(Reply::Decline) };
        cx.tick(1 + p / 256)?;
        let r = {
            let _scope = StopScope::enter(Some(&self.stop));
            crate::ball::hyper::hyp_pfq(ra, rb, &rz[0], p)
        };
        if stopped() || cx.stopped() {
            cx.tick(0)?;
            let tag = h.sym_term(h.sym(names::failure::INTERRUPTED));
            return Ok(Reply::Done(h.call(h.sym(names::FAILURE), &[tag, t])));
        }
        Ok(match r {
            Some(z) => Reply::Done(cball_term(&ar, z, p)),
            None => Reply::Decline,
        })
    }
}

/// Registers the natives (one per name) and the library functions of numerical evaluation (one per name and
/// arity).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let mut names: Vec<&str> = vec![];
    for f in &crate::functions::ALL {
        mc_numeric::library::register(f);
        if !names.contains(&f.name()) {
            names.push(f.name());
        }
    }
    for name in names {
        let fs: Vec<Special> = crate::functions::ALL.iter().copied().filter(|f| f.name() == name).collect();
        k.register(h.sym(name), Attributes::default(), Box::new(Fun { fs, stop: stop.clone() }));
    }
    k.register(h.sym(crate::functions::PFQ.0), Attributes::default(), Box::new(Pfq { stop: stop.clone() }));
    mc_numeric::formula::install(k, h, crate::more::DEFS);
    mc_numeric::formula::install(k, h, crate::cover::DEFS);
    mc_numeric::formula::install(k, h, crate::meijer::DEFS);
    mc_numeric::formula::install(k, h, crate::cover28::DEFS);
    mc_numeric::formula::install(k, h, crate::weierstrass::DEFS);
    mc_numeric::formula::install(k, h, crate::heun::DEFS);
    mc_numeric::formula::install(k, h, crate::appell::DEFS);
    mc_numeric::formula::install(k, h, crate::zeros::DEFS);
    mc_numeric::formula::install(k, h, crate::qhyper::DEFS);
    mc_numeric::formula::install(k, h, crate::mathieu::DEFS);
    mc_numeric::formula::install(k, h, crate::round29::DEFS);
    for f in &crate::more::SCALED {
        mc_numeric::library::register(f);
    }
    mc_numeric::library::register(&crate::more::ProductLog);
    for f in &crate::more::ERF_FAMILY {
        mc_numeric::library::register(f);
    }
}
