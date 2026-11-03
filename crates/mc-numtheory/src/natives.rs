//! The natives: argument decoding, list threading, results and failures around the algorithms of the crate.
//!
//! Conventions (decisions J.3 and J.5):
//!
//! - A call whose arguments are not of the supported kinds declines, so it stays unevaluated (or a rule applies):
//!   `floor(x)` for a symbol x, `factorial(1/2)`.
//! - Exact division by zero gives `failure(division_by_zero)`; an exact result beyond `mc-arith`'s size limit
//!   (2^28 bits) gives `failure(limit, memory, call)`, decided before computing it.
//! - The arithmetic functions are listable (semantics 7.2): a call with list arguments becomes the list of calls
//!   on their elements (non-list arguments broadcast), and lists of unequal length give
//!   `failure(shape_mismatch)`.
use crate::budget::Budget;
use crate::combin;
use crate::count::{nth_prime, prime_pi, PI_MAX};
use crate::digits;
use crate::factor::factor;
use crate::modular;
use crate::prime::{is_prime, next_prime, to_u64};
use mc_api::names;
use mc_arith::num::float::f64_to_dyadic;
use mc_arith::num::{Float, Int, Mag, Nat, Rat};
use mc_arith::{Arith, Num, Real, DEFAULT_MAX_BITS};
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// The body of a native.
type Body = for<'a, 'k, 'h> fn(&mut NativeCx<'a, 'k, 'h>, T<'h>, &[T<'h>]) -> Res<Reply<'h>>;

/// A native function: its body and whether it threads over lists.
struct Fun {
    body: Body,
    listable: bool,
}

impl Native for Fun {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if self.listable {
            if let Some(r) = thread(cx, t, args) {
                return Ok(r);
            }
        }
        (self.body)(cx, t, args)
    }
}

/// List threading: `f([a, b], c)` is `[f(a, c), f(b, c)]`.
fn thread<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Option<Reply<'h>> {
    let lists: Vec<Option<&[T<'h>]>> = args.iter().map(|&a| cx.as_list(a)).collect();
    let len = lists.iter().flatten().map(|l| l.len()).next()?;
    if lists.iter().flatten().any(|l| l.len() != len) {
        return Some(Reply::Done(cx.failure(&["shape_mismatch"])));
    }
    let h = cx.heap();
    let View::App { head, .. } = h.view(t) else { return None };
    let items: Vec<T<'h>> = (0..len)
        .map(|i| {
            let xs: Vec<T<'h>> = args.iter().zip(&lists).map(|(&a, l)| l.map_or(a, |l| l[i])).collect();
            h.app(head, &xs)
        })
        .collect();
    Some(Reply::Again(cx.list(&items)))
}

// ---------------------------------------------------------------- decoding and results

/// An exact integer argument.
fn int_of(h: &Heap, t: T<'_>) -> Option<Int> {
    match h.view(t) {
        View::Int(v) => Some(Int::from_i64(v)),
        View::BigInt { neg, mag } => Some(Int::new(neg, Nat::from_limbs(mag.to_vec()))),
        _ => None,
    }
}

/// A non-negative exact integer argument.
fn nat_of(h: &Heap, t: T<'_>) -> Option<Nat> {
    int_of(h, t).filter(|i| !i.neg).map(|i| i.mag)
}

/// An exact integer or rational argument.
fn rat_of(h: &Heap, t: T<'_>) -> Option<Rat> {
    match h.view(t) {
        View::Rational { num, den } => Some(Rat { num: int_of(h, num)?, den: int_of(h, den)?.mag }),
        _ => int_of(h, t).map(Rat::from_int),
    }
}

/// A word-sized non-negative integer argument.
fn word_of(h: &Heap, t: T<'_>) -> Option<u64> {
    nat_of(h, t).and_then(|n| to_u64(&n))
}

fn int_term<'h>(h: &'h Heap, x: &Int) -> T<'h> {
    h.big_int(x.neg, &x.mag.limbs)
}

fn nat_term<'h>(h: &'h Heap, x: &Nat) -> T<'h> {
    h.big_int(false, &x.limbs)
}

fn rat_term<'h>(h: &'h Heap, q: &Rat) -> T<'h> {
    Arith::new(h).number(&Num::rat(q.clone()))
}

/// `failure(limit, memory, t)`: a result beyond the size limit.
fn too_large<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> T<'h> {
    let h = cx.heap();
    let s = |n: &str| h.sym_term(h.sym(n));
    h.app(s(names::FAILURE), &[s(names::failure::LIMIT), s(names::failure::MEMORY), t])
}

fn div_zero<'h>(cx: &NativeCx<'_, '_, 'h>) -> Reply<'h> {
    Reply::Done(cx.failure(&[names::failure::DIVISION_BY_ZERO]))
}

fn done(t: T<'_>) -> Res<Reply<'_>> {
    Ok(Reply::Done(t))
}

/// The exact value of a dyadic number (-1)^neg man 2^exp.
fn dyadic(neg: bool, man: Nat, exp: i64) -> Rat {
    if exp >= 0 {
        Rat::from_int(Int::new(neg, man.shl(exp as u64)))
    } else {
        Rat::new(Int::new(neg, man), Int::from_nat(Nat::pow2((-exp) as u64)))
    }
}

fn float_rat(f: &Float) -> Rat {
    dyadic(f.neg, f.man.clone(), f.exp)
}

fn mag_rat(m: &Mag) -> Rat {
    dyadic(false, Nat::from_u64(m.m), m.e)
}

// ---------------------------------------------------------------- rounding

#[derive(Clone, Copy)]
enum Rounding {
    Floor,
    Ceiling,
    /// to the nearest integer, ties to even
    Nearest,
}

/// The rounded value of an exact rational.
fn round_rat(q: &Rat, mode: Rounding) -> Int {
    let (fl, fr) = q.floor_fract();
    match mode {
        Rounding::Floor => fl,
        Rounding::Ceiling if fr.is_zero() => fl,
        Rounding::Ceiling => fl.add(&Int::from_i64(1)),
        Rounding::Nearest => {
            let half = Rat::new(Int::from_i64(1), Int::from_i64(2));
            match fr.cmp_value(&half) {
                std::cmp::Ordering::Less => fl,
                std::cmp::Ordering::Greater => fl.add(&Int::from_i64(1)),
                std::cmp::Ordering::Equal if fl.mag.bit(0) => fl.add(&Int::from_i64(1)),
                std::cmp::Ordering::Equal => fl,
            }
        }
    }
}

/// The rounded value of a real number as an exact integer: `None` for infinities, NaN, and balls whose ends round
/// differently.
fn round_real(x: &Real, mode: Rounding) -> Option<Int> {
    match x {
        Real::Exact(q) => Some(round_rat(q, mode)),
        Real::Machine(v) => {
            let r = match mode {
                Rounding::Floor => v.floor(),
                Rounding::Ceiling => v.ceil(),
                Rounding::Nearest => v.round_ties_even(),
            };
            let (neg, man, exp) = f64_to_dyadic(r)?;
            let q = dyadic(neg, man, exp);
            Some(q.num)
        }
        Real::Ball(b, _) => {
            let (m, r) = (float_rat(&b.mid), mag_rat(&b.rad));
            let lo = round_rat(&m.sub(&r), mode);
            let hi = round_rat(&m.add(&r), mode);
            (lo == hi).then_some(lo)
        }
    }
}

fn rounding<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], mode: Rounding) -> Res<Reply<'h>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    match args {
        [x] => {
            let Some(v) = ar.num(*x) else { return Ok(Reply::Decline) };
            match v {
                Num::Real(r) => match round_real(&r, mode) {
                    Some(i) => done(int_term(h, &i)),
                    None => Ok(Reply::Decline),
                },
                // componentwise for complex numbers
                Num::Complex(re, im) => match (round_real(&re, mode), round_real(&im, mode)) {
                    (Some(a), Some(b)) => done(ar.complex(int_term(h, &a), int_term(h, &b))),
                    _ => Ok(Reply::Decline),
                },
            }
        }
        // floor(x, a) = a floor(x / a), and likewise
        [x, a] => {
            let (Some(Num::Real(xv)), Some(Num::Real(av))) = (ar.num(*x), ar.num(*a)) else {
                return Ok(Reply::Decline);
            };
            if av.is_exact_zero() {
                return Ok(div_zero(cx));
            }
            let Ok(q) = xv.div(&av) else { return Ok(Reply::Decline) };
            match round_real(&q, mode) {
                Some(i) => done(ar.number(&Num::Real(av.mul(&Real::Exact(Rat::from_int(i)))))),
                None => Ok(Reply::Decline),
            }
        }
        _ => Ok(Reply::Decline),
    }
}

fn floor<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    if let [x] = args {
        if let View::Int(_) | View::BigInt { .. } = cx.heap().view(*x) {
            return done(*x);
        }
    }
    rounding(cx, args, Rounding::Floor)
}

fn ceiling<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    rounding(cx, args, Rounding::Ceiling)
}

fn round<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    rounding(cx, args, Rounding::Nearest)
}

/// a - m floor((a - d) / m) for reals (exact or inexact).
fn mod_real(a: &Real, m: &Real, d: &Real) -> Option<Real> {
    let q = a.sub(d).div(m).ok()?;
    let f = round_real(&q, Rounding::Floor)?;
    Some(a.sub(&m.mul(&Real::Exact(Rat::from_int(f)))))
}

fn mod_<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // machine integers: the loop case (benchmark R12)
    if let [a, m] = args {
        if let (View::Int(x), View::Int(y)) = (h.view(*a), h.view(*m)) {
            if y == 0 {
                return Ok(div_zero(cx));
            }
            if let Some(r) = x.checked_rem_euclid(y) {
                // floor convention: the result has the sign of m
                let r = if y < 0 && r != 0 { r + y } else { r };
                return done(h.int(r));
            }
        }
    }
    let ar = Arith::new(h);
    let reals: Option<Vec<Real>> = args.iter().map(|&x| ar.num(x).and_then(|v| v.as_real().cloned())).collect();
    let Some(r) = reals else { return Ok(Reply::Decline) };
    let (a, m, d) = match r.as_slice() {
        [a, m] => (a, m, Real::int(0)),
        [a, m, d] => (a, m, d.clone()),
        _ => return Ok(Reply::Decline),
    };
    if m.is_exact_zero() {
        return Ok(div_zero(cx));
    }
    match mod_real(a, m, &d) {
        Some(v) => done(ar.number(&Num::Real(v))),
        None => Ok(Reply::Decline),
    }
}

fn quotient<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, b] = args else { return Ok(Reply::Decline) };
    if let (View::Int(x), View::Int(y)) = (h.view(*a), h.view(*b)) {
        if y == 0 {
            return Ok(div_zero(cx));
        }
        if let Some(q) = x.checked_div_euclid(y) {
            // div_euclid rounds towards the sign that keeps the remainder non-negative; floor for y < 0 differs
            let q = if y < 0 && x.rem_euclid(y) != 0 { q - 1 } else { q };
            return done(h.int(q));
        }
    }
    if let (Some(x), Some(y)) = (int_of(h, *a), int_of(h, *b)) {
        if y.is_zero() {
            return Ok(div_zero(cx));
        }
        cx.charge(x.mag.len() as u64 / 64 + 1)?;
        return done(int_term(h, &modular::floor_divrem(&x, &y).0));
    }
    let ar = Arith::new(h);
    let (Some(Num::Real(x)), Some(Num::Real(y))) = (ar.num(*a), ar.num(*b)) else { return Ok(Reply::Decline) };
    if y.is_exact_zero() {
        return Ok(div_zero(cx));
    }
    match x.div(&y).ok().and_then(|q| round_real(&q, Rounding::Floor)) {
        Some(i) => done(int_term(h, &i)),
        None => Ok(Reply::Decline),
    }
}

fn abs<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let ar = Arith::new(h);
    let Some(v) = ar.num(*x) else { return Ok(Reply::Decline) };
    match v {
        Num::Real(Real::Exact(q)) => done(rat_term(h, &Rat { num: Int::from_nat(q.num.mag), den: q.den })),
        Num::Real(Real::Machine(f)) => done(h.real(f.abs())),
        Num::Real(Real::Ball(b, p)) => {
            // |x| over [m - r, m + r] lies in [|m| - r, |m| + r]
            let ball = mc_arith::num::Ball { mid: b.mid.abs(), rad: b.rad };
            done(ar.number(&Num::Real(Real::Ball(ball, p))))
        }
        Num::Complex(re, im) => {
            // sqrt(re^2 + im^2): exact parts give a canonical radical
            let (a, b) = (ar.number(&Num::Real(re)), ar.number(&Num::Real(im)));
            let s = ar.add(&[ar.mul(&[a, a]), ar.mul(&[b, b])]);
            done(ar.pow(s, rat_term(h, &Rat::new(Int::from_i64(1), Int::from_i64(2)))))
        }
    }
}

fn sign<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let ar = Arith::new(h);
    let Some(v) = ar.num(*x) else { return Ok(Reply::Decline) };
    match v {
        Num::Real(r) => {
            if let Real::Machine(f) = r {
                if f.is_nan() {
                    return Ok(Reply::Decline);
                }
            }
            if r.is_positive() {
                done(h.int(1))
            } else if r.is_negative() {
                done(h.int(-1))
            } else if r.is_zero() {
                done(h.int(0))
            } else {
                // a ball containing zero
                Ok(Reply::Decline)
            }
        }
        Num::Complex(..) => {
            // z / |z|
            let a = abs(cx, *x, args)?;
            let Reply::Done(m) = a else { return Ok(Reply::Decline) };
            done(ar.div(*x, m))
        }
    }
}

/// The numerator and denominator of a term: of numbers exactly; of products by the sign of exponents (factors
/// b^e with e a negative number go to the denominator).
fn split_fraction<'h>(h: &'h Heap, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let ar = Arith::new(h);
    let one = h.int(1);
    if let Some(q) = rat_of(h, t) {
        return Some((int_term(h, &q.num), nat_term(h, &q.den)));
    }
    if ar.num(t).is_some() {
        return Some((t, one));
    }
    let neg_power = |f: T<'h>| -> Option<T<'h>> {
        let View::App { head, args: [b, e] } = h.view(f) else { return None };
        if head != h.sym_term(h.sym(names::POW)) {
            return None;
        }
        let Some(Num::Real(ev)) = ar.num(*e) else { return None };
        ev.is_negative().then(|| ar.pow(*b, ar.neg(*e)))
    };
    let factors: Vec<T<'h>> = match h.view(t) {
        View::App { head, args } if head == h.sym_term(h.sym(names::MUL)) => args.to_vec(),
        _ => vec![t],
    };
    let (mut num, mut den) = (vec![], vec![]);
    for f in factors {
        if let Some(q) = rat_of(h, f) {
            num.push(int_term(h, &q.num));
            den.push(nat_term(h, &q.den));
        } else if let Some(d) = neg_power(f) {
            den.push(d);
        } else {
            num.push(f);
        }
    }
    Some((ar.mul(&num), ar.mul(&den)))
}

fn numerator<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [x] = args else { return Ok(Reply::Decline) };
    match split_fraction(cx.heap(), *x) {
        Some((n, _)) => done(n),
        None => Ok(Reply::Decline),
    }
}

fn denominator<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [x] = args else { return Ok(Reply::Decline) };
    match split_fraction(cx.heap(), *x) {
        Some((_, d)) => done(d),
        None => Ok(Reply::Decline),
    }
}

// ---------------------------------------------------------------- gcd and lcm

fn gcd_nat(a: &Nat, b: &Nat, cx: &mut dyn Budget) -> Res<Nat> {
    crate::gcd::gcd(a, b, cx)
}

fn lcm_nat(a: &Nat, b: &Nat, cx: &mut dyn Budget) -> Res<Nat> {
    if a.is_zero() || b.is_zero() {
        return Ok(Nat::zero());
    }
    let g = gcd_nat(a, b, cx)?;
    Ok(a.div(&g).mul(b))
}

/// gcd and lcm of integers and rationals: gcd(p/q, r/s) = gcd(p, r) / lcm(q, s), lcm(p/q, r/s) = lcm(p, r) /
/// gcd(q, s). Declines unless every argument is an exact number.
pub(crate) fn gcd_lcm<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], is_gcd: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    if is_gcd {
        if let [a, b] = args {
            if let (View::Int(x), View::Int(y)) = (h.view(*a), h.view(*b)) {
                return done(nat_term(h, &Nat::from_u64(crate::word::gcd_u64(x.unsigned_abs(), y.unsigned_abs()))));
            }
        }
    }
    let rats: Option<Vec<Rat>> = args.iter().map(|&a| rat_of(h, a)).collect();
    let Some(rats) = rats else { return Ok(Reply::Decline) };
    let (mut num, mut den) =
        if is_gcd { (Nat::zero(), Nat::from_u64(1)) } else { (Nat::from_u64(1), Nat::from_u64(1)) };
    let mut first = true;
    for q in &rats {
        if is_gcd {
            num = gcd_nat(&num, &q.num.mag, cx)?;
            den = lcm_nat(&den, &q.den, cx)?;
        } else if first {
            num = q.num.mag.clone();
            den = q.den.clone();
        } else {
            num = lcm_nat(&num, &q.num.mag, cx)?;
            den = gcd_nat(&den, &q.den, cx)?;
        }
        first = false;
    }
    if !is_gcd && rats.is_empty() {
        return done(h.int(1));
    }
    done(rat_term(h, &Rat::new(Int::from_nat(num), Int::from_nat(den))))
}

fn extended_gcd<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, b] = args else { return Ok(Reply::Decline) };
    let (Some(x), Some(y)) = (int_of(h, *a), int_of(h, *b)) else { return Ok(Reply::Decline) };
    let (g, s, t) = modular::extended_gcd(&x, &y, cx)?;
    let st = cx.list(&[int_term(h, &s), int_term(h, &t)]);
    done(cx.list(&[nat_term(h, &g), st]))
}

// ---------------------------------------------------------------- factorials and binomials

fn factorial<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let Some(n) = int_of(h, *x) else { return Ok(Reply::Decline) };
    if n.neg {
        // the poles of the gamma function: no value (decision J.5)
        return Ok(Reply::Decline);
    }
    let Some(n) = to_u64(&n.mag) else { return done(too_large(cx, t)) };
    if combin::log2_factorial(n) > DEFAULT_MAX_BITS as f64 {
        return done(too_large(cx, t));
    }
    let f = combin::factorial(n, cx)?;
    done(nat_term(h, &f))
}

/// C(n, k) for integers as (negative, n', k') with C(n, k) = +-C(n', k'), `Some(None)` when it is zero, `None` when
/// the indices do not fit words. Negative n follow C(n, k) = (-1)^k C(k - n - 1, k) for k >= 0 and
/// (-1)^(n-k) C(-k - 1, n - k) for k <= n < 0.
fn binomial_int(n: &Int, k: &Int) -> Option<Option<(bool, u64, u64)>> {
    let one = Int::from_i64(1);
    let (neg, top, low) = if !n.neg {
        if k.neg || k.cmp_int(n).is_gt() {
            return Some(None);
        }
        (false, n.clone(), k.clone())
    } else if !k.neg {
        (k.mag.bit(0), k.sub(n).sub(&one), k.clone())
    } else if k.cmp_int(n).is_le() {
        let d = n.sub(k);
        (d.mag.bit(0), k.neg().sub(&one), d)
    } else {
        return Some(None);
    };
    Some(Some((neg, to_u64(&top.mag)?, to_u64(&low.mag)?)))
}

fn binomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, b] = args else { return Ok(Reply::Decline) };
    if let (Some(n), Some(k)) = (int_of(h, *a), int_of(h, *b)) {
        let (neg, n, k) = match binomial_int(&n, &k) {
            None => return done(too_large(cx, t)),
            Some(None) => return done(h.int(0)),
            Some(Some(v)) => v,
        };
        if combin::log2_binomial(n, k) > DEFAULT_MAX_BITS as f64 {
            return done(too_large(cx, t));
        }
        let c = combin::binomial(n, k, cx)?;
        return done(int_term(h, &Int::new(neg, c)));
    }
    // a rational upper index and a small non-negative integer k: the falling factorial over k!
    let (Some(q), Some(k)) = (rat_of(h, *a), word_of(h, *b)) else { return Ok(Reply::Decline) };
    if k > 100_000 {
        return Ok(Reply::Decline);
    }
    let mut r = Rat::from_i64(1);
    for i in 0..k {
        r = r.mul(&q.sub(&Rat::from_i64(i as i64))).mul(&Rat::new(Int::from_i64(1), Int::from_u64(i + 1)));
        if i % 256 == 255 {
            cx.charge(r.num.mag.len() as u64 + 1)?;
        }
    }
    done(rat_term(h, &r))
}

// ---------------------------------------------------------------- digits

/// The base argument (default 10) as a word >= 2.
fn base_arg(h: &Heap, args: &[T<'_>], i: usize) -> Option<u64> {
    match args.get(i) {
        None => Some(10),
        Some(&b) => word_of(h, b).filter(|&b| b >= 2),
    }
}

fn integer_digits<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if args.is_empty() || args.len() > 3 {
        return Ok(Reply::Decline);
    }
    let Some(n) = int_of(h, args[0]) else { return Ok(Reply::Decline) };
    let Some(base) = base_arg(h, args, 1) else { return Ok(Reply::Decline) };
    let mut ds = digits::digits(&n.mag, base, cx)?;
    if let Some(&l) = args.get(2) {
        let Some(len) = word_of(h, l) else { return Ok(Reply::Decline) };
        let len = len as usize;
        if ds.len() > len {
            ds.drain(..ds.len() - len);
        } else {
            let mut padded = vec![0u64; len - ds.len()];
            padded.extend(ds);
            ds = padded;
        }
    }
    let items: Vec<T<'h>> = ds.iter().map(|&d| nat_term(h, &Nat::from_u64(d))).collect();
    done(cx.list(&items))
}

fn from_digits<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if args.is_empty() || args.len() > 2 {
        return Ok(Reply::Decline);
    }
    let Some(list) = cx.as_list(args[0]) else { return Ok(Reply::Decline) };
    let base = match args.get(1) {
        None => Nat::from_u64(10),
        Some(&b) => match nat_of(h, b) {
            Some(b) => b,
            None => return Ok(Reply::Decline),
        },
    };
    let ds: Option<Vec<Nat>> = list.iter().map(|&d| nat_of(h, d)).collect();
    let Some(ds) = ds else { return Ok(Reply::Decline) };
    done(nat_term(h, &digits::from_digits(&ds, &base, cx)?))
}

fn integer_length<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if args.is_empty() || args.len() > 2 {
        return Ok(Reply::Decline);
    }
    let Some(n) = int_of(h, args[0]) else { return Ok(Reply::Decline) };
    let Some(base) = base_arg(h, args, 1) else { return Ok(Reply::Decline) };
    cx.charge(n.mag.len() as u64 / 16 + 1)?;
    done(h.int(digits::length(&n.mag, base) as i64))
}

fn integer_exponent<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if args.is_empty() || args.len() > 2 {
        return Ok(Reply::Decline);
    }
    let Some(n) = int_of(h, args[0]) else { return Ok(Reply::Decline) };
    let base = match args.get(1) {
        None => Nat::from_u64(10),
        Some(&b) => match nat_of(h, b).filter(|b| b.bits() >= 2) {
            Some(b) => b,
            None => return Ok(Reply::Decline),
        },
    };
    if n.is_zero() {
        // every power divides zero: no finite value
        return Ok(Reply::Decline);
    }
    done(h.int(digits::exponent(&n.mag, &base, cx)? as i64))
}

// ---------------------------------------------------------------- primes

fn is_prime_<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let r = match int_of(h, *x) {
        Some(n) => is_prime(&n.mag, cx)?,
        None => false,
    };
    done(cx.bool(r))
}

/// The largest prime below n, if any.
fn previous_prime(n: &Nat, b: &mut dyn Budget) -> Res<Option<Nat>> {
    if n.bits() <= 1 || *n == Nat::from_u64(2) {
        return Ok(None);
    }
    let mut c = n.sub_u64(1);
    // bounded by the prime gaps; every candidate is charged
    loop {
        if is_prime(&c, b)? {
            return Ok(Some(c));
        }
        if c.bits() <= 1 {
            return Ok(None);
        }
        c = c.sub_u64(1);
        b.charge(1)?;
    }
}

fn next_prime_<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if args.is_empty() || args.len() > 2 {
        return Ok(Reply::Decline);
    }
    // the floor of a real argument
    let ar = Arith::new(h);
    let Some(Num::Real(x)) = ar.num(args[0]) else { return Ok(Reply::Decline) };
    let Some(n) = round_real(&x, Rounding::Floor) else { return Ok(Reply::Decline) };
    let k = match args.get(1) {
        None => 1,
        Some(&k) => match int_of(h, k).and_then(|k| k.to_i64()) {
            Some(k) if k != 0 => k,
            _ => return Ok(Reply::Decline),
        },
    };
    // below 2 the next prime is 2; negative n count as below 2 (decision J.5)
    let mut cur = if n.neg { Nat::zero() } else { n.mag };
    if k > 0 {
        for _ in 0..k {
            cur = next_prime(&cur, cx)?;
        }
    } else {
        for _ in 0..-k {
            match previous_prime(&cur, cx)? {
                Some(p) => cur = p,
                None => return Ok(Reply::Decline),
            }
        }
    }
    done(nat_term(h, &cur))
}

fn prime<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let Some(k) = word_of(h, *x).filter(|&k| k >= 1) else { return Ok(Reply::Decline) };
    match nth_prime(k, cx)? {
        Some(p) => done(nat_term(h, &Nat::from_u64(p))),
        None => done(too_large(cx, t)),
    }
}

fn prime_pi_<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let Some(Num::Real(v)) = Arith::new(h).num(*x) else { return Ok(Reply::Decline) };
    let Some(n) = round_real(&v, Rounding::Floor) else { return Ok(Reply::Decline) };
    if n.neg {
        return done(h.int(0));
    }
    match to_u64(&n.mag).filter(|&n| n <= PI_MAX) {
        Some(n) => done(h.int(prime_pi(n, cx)? as i64)),
        None => done(too_large(cx, t)),
    }
}

fn factor_integer<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let Some(q) = rat_of(h, *x) else { return Ok(Reply::Decline) };
    let pair = |cx: &NativeCx<'_, '_, 'h>, p: T<'h>, e: i64| cx.list(&[p, h.int(e)]);
    if q.num.is_zero() {
        return done(cx.list(&[pair(cx, h.int(0), 1)]));
    }
    let mut items = vec![];
    if q.num.neg {
        items.push(pair(cx, h.int(-1), 1));
    }
    let mut fs: Vec<(Nat, i64)> = factor(&q.num.mag, cx)?.into_iter().map(|(p, e)| (p, e as i64)).collect();
    fs.extend(factor(&q.den, cx)?.into_iter().map(|(p, e)| (p, -(e as i64))));
    fs.sort_by(|a, b| a.0.cmp(&b.0));
    if fs.is_empty() && items.is_empty() {
        // the factorisation of 1
        return done(cx.list(&[pair(cx, h.int(1), 1)]));
    }
    for (p, e) in fs {
        items.push(pair(cx, nat_term(h, &p), e));
    }
    done(cx.list(&items))
}

// ---------------------------------------------------------------- modular functions

fn power_mod<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, e, m] = args else { return Ok(Reply::Decline) };
    let (Some(a), Some(e), Some(m)) = (int_of(h, *a), int_of(h, *e), int_of(h, *m)) else { return Ok(Reply::Decline) };
    if m.is_zero() {
        return Ok(div_zero(cx));
    }
    let base = if e.neg {
        match modular::inverse_mod(&a, &m.mag, cx)? {
            Some(inv) => inv,
            // no inverse: the call stays unevaluated (decision J.5)
            None => return Ok(Reply::Decline),
        }
    } else {
        a.divrem_floor(&m.mag).1
    };
    let r = modular::pow_mod(&base, &e.mag, &m.mag, cx)?;
    // floor convention: a negative modulus gives a result in (m, 0]
    let r = if m.neg && !r.is_zero() { Int::from_nat(r).sub(&Int::from_nat(m.mag.clone())) } else { Int::from_nat(r) };
    done(int_term(h, &r))
}

fn primitive_root<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let Some(n) = nat_of(h, *x).filter(|n| !n.is_zero()) else { return Ok(Reply::Decline) };
    match modular::primitive_root(&n, cx)? {
        Some(g) => done(nat_term(h, &g)),
        // no primitive root: the call stays unevaluated
        None => Ok(Reply::Decline),
    }
}

fn euler_phi<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let Some(n) = int_of(h, *x) else { return Ok(Reply::Decline) };
    if n.is_zero() {
        return done(h.int(0));
    }
    done(nat_term(h, &modular::euler_phi(&n.mag, cx)?))
}

fn divisors<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let Some(n) = int_of(h, *x).filter(|n| !n.is_zero()) else { return Ok(Reply::Decline) };
    match modular::divisors(&n.mag, cx)? {
        Some(ds) => {
            let items: Vec<T<'h>> = ds.iter().map(|d| nat_term(h, d)).collect();
            done(cx.list(&items))
        }
        None => done(too_large(cx, t)),
    }
}

fn divisor_sigma<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [k, x] = args else { return Ok(Reply::Decline) };
    let (Some(k), Some(n)) = (word_of(h, *k), int_of(h, *x).filter(|n| !n.is_zero())) else {
        return Ok(Reply::Decline);
    };
    if (k as f64) * (n.mag.bits() as f64) * 2.0 > DEFAULT_MAX_BITS as f64 {
        return done(too_large(cx, t));
    }
    done(nat_term(h, &modular::divisor_sigma(k, &n.mag, cx)?))
}

fn moebius_mu<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = args else { return Ok(Reply::Decline) };
    let Some(n) = int_of(h, *x).filter(|n| !n.is_zero()) else { return Ok(Reply::Decline) };
    done(h.int(modular::moebius_mu(&n.mag, cx)? as i64))
}

fn jacobi_symbol<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, n] = args else { return Ok(Reply::Decline) };
    let (Some(a), Some(n)) = (int_of(h, *a), nat_of(h, *n)) else { return Ok(Reply::Decline) };
    if !n.bit(0) {
        return Ok(Reply::Decline);
    }
    cx.charge(crate::budget::limb_work(n.len(), 1))?;
    done(h.int(modular::jacobi(&a, &n) as i64))
}

fn multiplicative_order<'h>(cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, n] = args else { return Ok(Reply::Decline) };
    let (Some(a), Some(n)) = (int_of(h, *a), nat_of(h, *n).filter(|n| n.bits() >= 2)) else {
        return Ok(Reply::Decline);
    };
    let a = a.divrem_floor(&n).1;
    match modular::multiplicative_order(&a, &n, cx)? {
        Some(o) => done(nat_term(h, &o)),
        None => Ok(Reply::Decline),
    }
}

// ---------------------------------------------------------------- registration

/// The natives: name, listable, body (the descriptions are in [`crate::FUNCTIONS`]).
pub(crate) const TABLE: &[(&str, bool, Body)] = &[
    ("mod", true, mod_),
    ("quotient", true, quotient),
    ("extended_gcd", false, extended_gcd),
    ("floor", true, floor),
    ("ceiling", true, ceiling),
    ("round", true, round),
    ("abs", true, abs),
    ("sign", true, sign),
    ("numerator", true, numerator),
    ("denominator", true, denominator),
    ("factorial", true, factorial),
    ("binomial", true, binomial),
    ("integer_digits", false, integer_digits),
    ("from_digits", false, from_digits),
    ("integer_length", true, integer_length),
    ("integer_exponent", true, integer_exponent),
    ("is_prime", true, is_prime_),
    ("next_prime", true, next_prime_),
    ("prime", true, prime),
    ("prime_pi", true, prime_pi_),
    ("factor_integer", false, factor_integer),
    ("power_mod", true, power_mod),
    ("primitive_root", true, primitive_root),
    ("euler_phi", true, euler_phi),
    ("divisors", false, divisors),
    ("divisor_sigma", true, divisor_sigma),
    ("moebius_mu", true, moebius_mu),
    ("jacobi_symbol", true, jacobi_symbol),
    ("multiplicative_order", true, multiplicative_order),
];

/// Register every native of [`TABLE`] with the kernel.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    for &(name, listable, body) in TABLE {
        k.register(h.sym(name), Attributes::default(), Box::new(Fun { body, listable }));
    }
}
