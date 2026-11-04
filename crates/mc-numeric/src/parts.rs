//! Complex parts: `re`, `im_part`, `conjugate`, `arg`, and rules extending `abs` and `sign` (whose natives, in
//! `mc-numtheory`, handle numbers) to closed numeric terms.
//!
//! On numbers the parts are exact operations on the number's own parts (decision 1C.1): `re(2.5) = 2.5`,
//! `im_part(2.5) = 0` (exact: a real number's imaginary part is exactly zero), `conjugate(complex(1.0, 2.0)) =
//! complex(1.0, -2.0)`. `arg` of a real number is exact for every kind (0 for x >= 0, pi for x < 0); of an exact
//! complex number a + bi an exact form (`atan(b/a)`, plus or minus pi, or ±pi/2) evaluated again, so the special
//! values of `atan` apply (`arg(1 + im) = pi/4`); of inexact complex numbers a number of their kind.
//!
//! Beyond numbers, a term is taken apart as a sum of summands c * (-1)^q * r1 * ... with c a number, at most one
//! complex unit (-1)^q (q rational) and real factors: closed numeric terms whose value is real (decided by
//! refinement, `numeval::is_real`) or calls of the real-valued functions `re`, `im_part`, `abs`, `arg`. Then
//! re, im_part and conjugate distribute (decision 1C.2): `re(3*pi + 2im*sqrt(2)) = 3*pi`,
//! `im_part((-1)^(1/3)) = sin(pi/3) = 3^(1/2)/2`. Anything else stays unevaluated (`re(x)`), except the
//! identities valid for every complex x: `re(re(x)) = re(x)`, `im_part(abs(x)) = 0`, `conjugate(conjugate(x)) = x`,
//! `re(conjugate(x)) = re(x)`, `im_part(conjugate(x)) = -im_part(x)`, `arg(abs(x)) = 0`.
//!
//! Derivatives (for package 2A): none of these is holomorphic. For a real variable x: `re(x) = x`, `im_part(x) = 0`,
//! `conjugate(x) = x`, `arg(x) = pi * unit_step(-x)` (x != 0), `abs(x)' = sign(x)`, `sign(x)' = 0` (x != 0).
use crate::numbers::Env;
use crate::numeval;
use mc_api::{Store, Sym, T};
use mc_arith::num::{Ball, Int, Rat};
use mc_arith::{Arith, Num, Real};
use mc_core::Heap;
use mc_eval::pat::{call, cond, var};
use mc_eval::{Binds, Kernel, NativeCx, Reply, Res, Rhs};
use std::cmp::Ordering;
use std::sync::Arc;

/// Which part.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Re,
    Im,
    Conj,
}

/// One summand c * (-1)^q * (product of real factors).
struct Summand<'h> {
    c: Num,
    unit: Option<Rat>,
    real: Vec<T<'h>>,
}

/// The real and imaginary parts of a number.
fn parts(z: &Num) -> (Real, Real) {
    match z {
        Num::Real(r) => (r.clone(), Real::int(0)),
        Num::Complex(a, b) => (a.clone(), b.clone()),
    }
}

fn real_term<'h>(ar: &Arith<'h, Heap>, r: Real) -> T<'h> {
    ar.number(&Num::Real(r))
}

/// The exponent q of a complex unit (-1)^q with q a non-integer rational.
fn unit_exponent<'h>(env: &Env, h: &'h Heap, ar: &Arith<'h, Heap>, t: T<'h>) -> Option<Rat> {
    let (s, &[b, e]) = env.call_of(h, t)? else { return None };
    if s != env.s.pow {
        return None;
    }
    let minus_one = matches!(ar.num(b), Some(Num::Real(Real::Exact(q))) if q == Rat::from_i64(-1));
    match ar.num(e) {
        Some(Num::Real(Real::Exact(q))) if minus_one && !q.is_int() => Some(q),
        _ => None,
    }
}

/// Is `t` a call of a function whose value is real for every argument?
fn real_valued_call(env: &Env, h: &Heap, t: T<'_>) -> Option<Sym> {
    let (s, args) = env.call_of(h, t)?;
    ([env.s.re, env.s.im_part, env.s.abs, env.s.arg].contains(&s) && args.len() == 1).then_some(s)
}

/// Is `t` known to be real: a real-valued call or a closed numeric term with a real value?
fn is_real_factor<'h>(env: &Env, h: &'h Heap, ar: &Arith<'h, Heap>, t: T<'h>) -> bool {
    real_valued_call(env, h, t).is_some() || numeval::is_real(h, ar, &env.nm, t) == Some(true)
}

/// `t` as summands, or `None` when some factor is neither a number, a unit nor known to be real.
fn decompose<'h>(env: &Env, h: &'h Heap, ar: &Arith<'h, Heap>, t: T<'h>) -> Option<Vec<Summand<'h>>> {
    let summands: Vec<T<'h>> = match env.call_of(h, t) {
        Some((s, args)) if s == env.s.add => args.to_vec(),
        _ => vec![t],
    };
    let mut out = Vec::with_capacity(summands.len());
    for s in summands {
        let factors: Vec<T<'h>> = match env.call_of(h, s) {
            Some((m, args)) if m == env.s.mul => args.to_vec(),
            _ => vec![s],
        };
        let mut sm = Summand { c: Num::int(1), unit: None, real: vec![] };
        for f in factors {
            if let Some(n) = ar.num(f) {
                sm.c = sm.c.mul(&n);
            } else if let Some(q) = unit_exponent(env, h, ar, f) {
                if sm.unit.replace(q).is_some() {
                    return None;
                }
            } else if is_real_factor(env, h, ar, f) {
                sm.real.push(f);
            } else {
                return None;
            }
        }
        out.push(sm);
    }
    Some(out)
}

/// The term c * r1 * r2 * ... (to be evaluated).
fn times<'h>(env: &Env, h: &'h Heap, c: T<'h>, real: &[T<'h>]) -> T<'h> {
    let mut f = Vec::with_capacity(real.len() + 1);
    f.push(c);
    f.extend_from_slice(real);
    h.call(env.s.mul, &f)
}

/// cos(q pi) or sin(q pi), to be evaluated.
fn trig<'h>(env: &Env, h: &'h Heap, ar: &Arith<'h, Heap>, f: Sym, q: &Rat) -> T<'h> {
    let angle = h.call(env.s.mul, &[real_term(ar, Real::Exact(q.clone())), h.sym_term(env.s.pi)]);
    h.call(f, &[angle])
}

/// The part of a decomposed term, to be evaluated.
fn distribute<'h>(env: &Env, h: &'h Heap, ar: &Arith<'h, Heap>, which: Part, summands: &[Summand<'h>]) -> T<'h> {
    let terms: Vec<T<'h>> = summands
        .iter()
        .map(|sm| {
            let (a, b) = parts(&sm.c);
            let (a, b) = (real_term(ar, a), real_term(ar, b));
            let coefficient = match (which, &sm.unit) {
                (Part::Re, None) => a,
                (Part::Im, None) => b,
                (Part::Conj, None) => ar.number(&conj(&sm.c)),
                // (a + bi)(cos t + i sin t): real part a cos t - b sin t, imaginary part a sin t + b cos t
                (Part::Re, Some(q)) => h.call(
                    env.s.add,
                    &[
                        h.call(env.s.mul, &[a, trig(env, h, ar, env.s.cos, q)]),
                        h.call(env.s.mul, &[ar.neg(b), trig(env, h, ar, env.s.sin, q)]),
                    ],
                ),
                (Part::Im, Some(q)) => h.call(
                    env.s.add,
                    &[
                        h.call(env.s.mul, &[a, trig(env, h, ar, env.s.sin, q)]),
                        h.call(env.s.mul, &[b, trig(env, h, ar, env.s.cos, q)]),
                    ],
                ),
                // conjugate((-1)^q) = (-1)^(-q) on the principal branch
                (Part::Conj, Some(q)) => {
                    let unit = h.call(env.s.pow, &[h.int(-1), real_term(ar, Real::Exact(q.neg()))]);
                    h.call(env.s.mul, &[ar.number(&conj(&sm.c)), unit])
                }
            };
            times(env, h, coefficient, &sm.real)
        })
        .collect();
    h.call(env.s.add, &terms)
}

fn conj(z: &Num) -> Num {
    match z {
        Num::Real(r) => Num::Real(r.clone()),
        Num::Complex(a, b) => Num::Complex(a.clone(), b.neg()),
    }
}

/// re, im_part and conjugate.
fn part<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, which: Part, args: &[T<'h>]) -> Res<Reply<'h>> {
    let &[x] = args else { return Ok(Reply::Decline) };
    let h = cx.heap();
    let ar = env.arith(h);
    if let Some(z) = ar.num(x) {
        let (a, b) = parts(&z);
        return Ok(Reply::Done(match which {
            Part::Re => real_term(&ar, a),
            // a real number of any kind has the exact imaginary part 0 (decision 1C.1)
            Part::Im => real_term(&ar, b),
            Part::Conj => ar.number(&conj(&z)),
        }));
    }
    if real_valued_call(env, h, x).is_some() {
        return Ok(Reply::Done(if which == Part::Im { h.int(0) } else { x }));
    }
    if let Some((s, &[y])) = env.call_of(h, x) {
        if s == env.s.conjugate {
            return Ok(match which {
                Part::Re => Reply::Again(h.call(env.s.re, &[y])),
                Part::Im => Reply::Again(h.call(env.s.mul, &[h.int(-1), h.call(env.s.im_part, &[y])])),
                Part::Conj => Reply::Done(y),
            });
        }
    }
    cx.tick(1)?;
    Ok(match decompose(env, h, &ar, x) {
        Some(sms) => Reply::Again(distribute(env, h, &ar, which, &sms)),
        None => Reply::Decline,
    })
}

/// `re(z)`.
pub(crate) fn re<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    part(env, cx, Part::Re, args)
}

/// `im_part(z)`.
pub(crate) fn im_part<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    part(env, cx, Part::Im, args)
}

/// `conjugate(z)`.
pub(crate) fn conjugate<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    part(env, cx, Part::Conj, args)
}

/// The sign of a real number: `None` for NaN and balls containing zero.
fn real_sign(r: &Real) -> Option<Ordering> {
    if let Real::Machine(v) = r {
        if v.is_nan() {
            return None;
        }
    }
    if r.is_positive() {
        Some(Ordering::Greater)
    } else if r.is_negative() {
        Some(Ordering::Less)
    } else if r.is_zero() {
        Some(Ordering::Equal)
    } else {
        None
    }
}

/// q pi as a term (to be evaluated).
fn pi_times<'h>(env: &Env, h: &'h Heap, ar: &Arith<'h, Heap>, n: i64, d: i64) -> T<'h> {
    let q = Rat::new(Int::from_i64(n), Int::from_i64(d));
    h.call(env.s.mul, &[real_term(ar, Real::Exact(q)), h.sym_term(env.s.pi)])
}

/// The principal argument of a complex ball a + bi at precision p ([`crate::cball::arg`]): `None` when a ball
/// containing zero puts the point on the branch cut or at the origin.
fn ball_arg(a: &Ball, b: &Ball, p: u64) -> Option<Ball> {
    crate::cball::arg(&crate::cball::CBall::new(a.clone(), b.clone()), p)
}

/// `arg(z)`.
pub(crate) fn arg<'h>(env: &Env, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let &[x] = args else { return Ok(Reply::Decline) };
    let h = cx.heap();
    let ar = env.arith(h);
    let pi = h.sym_term(env.s.pi);
    match ar.num(x) {
        Some(Num::Real(r)) => {
            // exact for every kind of real (decision 1C.1)
            return Ok(match real_sign(&r) {
                Some(Ordering::Less) => Reply::Done(pi),
                Some(_) => Reply::Done(h.int(0)),
                None => Reply::Decline,
            });
        }
        Some(Num::Complex(Real::Exact(a), Real::Exact(b))) => {
            let (sa, sb) = (real_sign(&Real::Exact(a.clone())), real_sign(&Real::Exact(b.clone())));
            if sa == Some(Ordering::Equal) {
                let up = sb == Some(Ordering::Greater);
                return Ok(Reply::Again(pi_times(env, h, &ar, if up { 1 } else { -1 }, 2)));
            }
            let ratio = b.mul(&a.recip().expect("a nonzero real part"));
            let t = h.call(env.s.atan, &[real_term(&ar, Real::Exact(ratio))]);
            return Ok(Reply::Again(match (sa, sb) {
                (Some(Ordering::Greater), _) => t,
                (_, Some(Ordering::Greater)) => h.call(env.s.add, &[t, pi]),
                _ => h.call(env.s.add, &[t, h.call(env.s.mul, &[h.int(-1), pi])]),
            }));
        }
        Some(Num::Complex(Real::Machine(a), Real::Machine(b))) => return Ok(Reply::Done(h.real(b.atan2(a)))),
        Some(Num::Complex(Real::Ball(a, pa), Real::Ball(b, pb))) => {
            let p = pa.max(pb);
            cx.tick(1 + p / 1024)?;
            return Ok(match ball_arg(&a, &b, p) {
                Some(v) => Reply::Done(ar.number(&Num::Real(Real::Ball(v, p)))),
                None => Reply::Decline,
            });
        }
        Some(Num::Complex(..)) => return Ok(Reply::Decline),
        None => {}
    }
    if let Some(s) = real_valued_call(env, h, x) {
        return Ok(if s == env.s.abs { Reply::Done(h.int(0)) } else { Reply::Decline });
    }
    // c * r1 * ... with every real factor of decided, nonzero sign: arg(+-c)
    cx.tick(1)?;
    let Some(sms) = decompose(env, h, &ar, x) else { return Ok(Reply::Decline) };
    let [sm] = sms.as_slice() else { return Ok(Reply::Decline) };
    if sm.unit.is_some() {
        return Ok(Reply::Decline);
    }
    let Some(negative) = factor_signs(env, h, &ar, &sm.real) else { return Ok(Reply::Decline) };
    let c = if negative { sm.c.neg() } else { sm.c.clone() };
    Ok(Reply::Again(h.call(env.s.arg, &[ar.number(&c)])))
}

/// Whether the product of closed real factors is negative, when each factor's sign is decided and nonzero.
fn factor_signs<'h>(env: &Env, h: &'h Heap, ar: &Arith<'h, Heap>, real: &[T<'h>]) -> Option<bool> {
    let mut negative = false;
    for &f in real {
        match numeval::sign(h, ar, &env.nm, f)? {
            Ordering::Less => negative = !negative,
            Ordering::Greater => {}
            Ordering::Equal => return None,
        }
    }
    Some(negative)
}

/// |z| for a number: exact for exact parts (a canonical radical), of the parts' kind otherwise.
fn abs_num<'h>(ar: &Arith<'h, Heap>, z: &Num) -> T<'h> {
    match z {
        Num::Real(r) => real_term(ar, if r.is_negative() { r.neg() } else { r.clone() }),
        Num::Complex(a, b) => {
            let (a, b) = (real_term(ar, a.clone()), real_term(ar, b.clone()));
            let s = ar.add(&[ar.mul(&[a, a]), ar.mul(&[b, b])]);
            ar.pow(s, ar.number(&Num::Real(Real::Exact(Rat::new(Int::from_i64(1), Int::from_i64(2))))))
        }
    }
}

/// -t, distributed over a sum (the canonical form keeps `-(1 - pi)` as a product).
fn negated<'h>(env: &Env, h: &'h Heap, t: T<'h>) -> T<'h> {
    let neg = |u: T<'h>| h.call(env.s.mul, &[h.int(-1), u]);
    match env.call_of(h, t) {
        Some((s, args)) if s == env.s.add => h.call(env.s.add, &args.iter().map(|&u| neg(u)).collect::<Vec<_>>()),
        _ => neg(t),
    }
}

/// `abs(t)` for a term that is not a number: a closed real term of decided sign gives +-t; c * (-1)^q * r1 * ...
/// with every real factor of decided sign or itself an `abs` call gives |c| * (+-r1) * ...; `abs(conjugate(x)) =
/// abs(x)`.
fn abs_closed<'h>(env: &Env, h: &'h Heap, x: T<'h>) -> Option<T<'h>> {
    let ar = env.arith(h);
    if ar.num(x).is_some() {
        return None;
    }
    if let Some((s, &[y])) = env.call_of(h, x) {
        if s == env.s.conjugate {
            return Some(h.call(env.s.abs, &[y]));
        }
    }
    // a closed real term of decided sign
    match numeval::sign(h, &ar, &env.nm, x) {
        Some(Ordering::Less) => return Some(negated(env, h, x)),
        Some(Ordering::Equal) => return Some(h.int(0)),
        Some(Ordering::Greater) => return Some(x),
        None => {}
    }
    let sms = decompose(env, h, &ar, x)?;
    let [sm] = sms.as_slice() else { return None };
    let mut factors = vec![abs_num(&ar, &sm.c)];
    for &f in &sm.real {
        if env.call_of(h, f).is_some_and(|(s, _)| s == env.s.abs) {
            factors.push(f);
            continue;
        }
        factors.push(match numeval::sign(h, &ar, &env.nm, f)? {
            Ordering::Less => negated(env, h, f),
            Ordering::Equal => h.int(0),
            Ordering::Greater => f,
        });
    }
    Some(h.call(env.s.mul, &factors))
}

/// `sign(t)` for a term that is not a number: -1, 0 or 1 for a closed real term of decided sign; c * (-1)^q *
/// r1 * ... with every real factor of decided sign gives (c / |c|) * (-1)^q * (+-1).
fn sign_closed<'h>(env: &Env, h: &'h Heap, x: T<'h>) -> Option<T<'h>> {
    let ar = env.arith(h);
    if ar.num(x).is_some() {
        return None;
    }
    if let Some(o) = numeval::sign(h, &ar, &env.nm, x) {
        return Some(h.int(o as i64));
    }
    let sms = decompose(env, h, &ar, x)?;
    let [sm] = sms.as_slice() else { return None };
    let mut negative = false;
    for &f in &sm.real {
        match numeval::sign(h, &ar, &env.nm, f)? {
            Ordering::Less => negative = !negative,
            Ordering::Greater => {}
            Ordering::Equal => return Some(h.int(0)),
        }
    }
    if sm.c.is_zero() {
        return Some(h.int(0));
    }
    let c = if negative { sm.c.neg() } else { sm.c.clone() };
    let mut factors = vec![ar.number(&c), h.call(env.s.pow, &[abs_num(&ar, &c), h.int(-1)])];
    if let Some(q) = &sm.unit {
        factors.push(h.call(env.s.pow, &[h.int(-1), real_term(&ar, Real::Exact(q.clone()))]));
    }
    Some(h.call(env.s.mul, &factors))
}

/// `f(x) /; computed(x) is some := computed(x)` on the head `head`.
fn define_computed(
    k: &mut Kernel,
    h: &Heap,
    env: &Arc<Env>,
    head: Sym,
    f: for<'h> fn(&Env, &'h Heap, T<'h>) -> Option<T<'h>>,
) {
    let env_c = env.clone();
    let lhs = cond(call(h, head, vec![var(0)]), &[0], move |cx| {
        let h = cx.heap();
        let _scope = mc_arith::num::StopScope::enter(Some(&env_c.stop));
        Ok(f(&env_c, h, cx.one(0)).is_some())
    });
    let env_r = env.clone();
    let rhs: Rhs = Arc::new(move |binds: &Binds<'_>, h: &Heap| {
        let x = binds.one(0);
        let _scope = mc_arith::num::StopScope::enter(Some(&env_r.stop));
        // the condition just computed the same value; only a raised stop flag can make it differ
        f(&env_r, h, x).unwrap_or_else(|| env_r.fail(h, mc_api::names::failure::INTERRUPTED, &[h.call(head, &[x])]))
    });
    k.define(h, lhs, rhs).expect("a one-variable pattern is valid");
}

/// The rules on `abs` and `sign` for closed numeric terms (decision 1C.7); numbers are `mc-numtheory`'s natives'.
pub(crate) fn define_abs_sign(k: &mut Kernel, h: &Heap, env: &Arc<Env>) {
    define_computed(k, h, env, env.s.abs, abs_closed);
    define_computed(k, h, env, env.s.sign, sign_closed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::Const;

    #[test]
    fn ball_arguments() {
        let p = 128;
        let one = Ball::from_i64(1);
        let q = ball_arg(&one, &one, p).unwrap();
        let pi4 = Const::Pi.ball(p + 8).unwrap().mul(&Ball::from_ratio(1, 4, p + 8), p + 8);
        assert!(q.cmp3(&pi4).is_none(), "arg(1 + i) encloses pi/4");
        let m = ball_arg(&one.neg(), &one.neg(), p).unwrap();
        assert!(m.mid.neg && m.cmp3(&pi4.mul(&Ball::from_i64(-3), p)).is_none());
        let up = ball_arg(&Ball::from_i64(0), &Ball::from_i64(2), p).unwrap();
        assert!(up.cmp3(&pi4.mul(&Ball::from_i64(2), p)).is_none());
        // a ball straddling the negative axis has no principal argument
        let wide = Ball { mid: Ball::from_i64(0).mid, rad: mc_arith::num::Mag::pow2(-10) };
        assert!(ball_arg(&one.neg(), &wide, p).is_none());
        assert!(ball_arg(&wide, &wide, p).is_none());
    }

    #[test]
    fn real_signs() {
        assert_eq!(real_sign(&Real::Machine(f64::NAN)), None);
        assert_eq!(real_sign(&Real::Machine(-0.0)), Some(Ordering::Equal));
        assert_eq!(real_sign(&Real::int(-3)), Some(Ordering::Less));
    }
}
