//! Number terms and the symbols the canonicaliser recognises: conversion between [`Num`] values and terms of a
//! [`Store`], and the classification of a term by its top-level shape.
use crate::number::{Num, Real, MIN_PREC};
use crate::num::{Ball, Float, Int, Mag, Nat, Rat};
use mc_api::{names, Store, View, T};

/// Symbol names this crate adds to the contract's `names` (reported as a contract addition).
pub mod failure_tags {
    /// `failure(division_by_zero)`: exact division by zero (semantics 2.4).
    pub const DIVISION_BY_ZERO: &str = "division_by_zero";
    /// `failure(limit, memory, partial)`: an exact result beyond the size limit (semantics 10).
    pub const LIMIT: &str = "limit";
    /// The kind of limit in `failure(limit, memory, partial)`.
    pub const MEMORY: &str = "memory";
    /// `failure(interrupted, partial)`: the stop flag was raised (semantics 10).
    pub const INTERRUPTED: &str = "interrupted";
    /// `failure(indeterminate, partial)`: a sum of infinities of opposite signs, or zero times infinity.
    pub const INDETERMINATE: &str = "indeterminate";
}

/// The heads and constants the canonicaliser builds and recognises.
pub(crate) struct Names<'h> {
    pub add: T<'h>,
    pub mul: T<'h>,
    pub pow: T<'h>,
    pub complex: T<'h>,
    pub failure: T<'h>,
    pub division_by_zero: T<'h>,
    pub limit: T<'h>,
    pub memory: T<'h>,
    pub interrupted: T<'h>,
    /// the constant `inf` (positive infinity; `-inf` is `mul(-1, inf)`)
    pub inf: T<'h>,
    /// `complex_infinity` and `indeterminate`, the directionless infinite and undefined values: interned on
    /// first use (an `Arith` is made per arithmetic call; interning on every construction cost 1.7x on recursion)
    /// (raw handles, 0 until interned: a `Cell` of a handle would make the arithmetic invariant in `'h`)
    pub cinf: std::cell::Cell<u32>,
    pub indet: std::cell::Cell<u32>,
    pub zero: T<'h>,
    pub one: T<'h>,
    pub minus_one: T<'h>,
}

impl<'h> Names<'h> {
    pub fn new<S: Store + ?Sized>(st: &'h S) -> Names<'h> {
        let s = |n: &str| st.sym_term(st.sym(n));
        Names {
            add: s(names::ADD),
            mul: s(names::MUL),
            pow: s(names::POW),
            complex: s(names::COMPLEX),
            failure: s(names::FAILURE),
            division_by_zero: s(failure_tags::DIVISION_BY_ZERO),
            limit: s(failure_tags::LIMIT),
            memory: s(failure_tags::MEMORY),
            interrupted: s(failure_tags::INTERRUPTED),
            inf: s("inf"),
            cinf: std::cell::Cell::new(0),
            indet: std::cell::Cell::new(0),
            zero: st.int(0),
            one: st.int(1),
            minus_one: st.int(-1),
        }
    }
}

/// The top-level shape of a term, as far as arithmetic cares.
pub(crate) enum Shape<'h> {
    /// A number (complex numbers normalised by contagion between their parts).
    Number(Num),
    Add(&'h [T<'h>]),
    Mul(&'h [T<'h>]),
    Pow(T<'h>, T<'h>),
    Failure,
    Other,
}

pub(crate) fn shape<'h, S: Store + ?Sized>(st: &'h S, n: &Names<'h>, t: T<'h>) -> Shape<'h> {
    if let Some(x) = real_of(st, t) {
        return Shape::Number(Num::Real(x));
    }
    match st.view(t) {
        View::App { head, args } => {
            if head == n.add {
                Shape::Add(args)
            } else if head == n.mul {
                Shape::Mul(args)
            } else if head == n.pow && args.len() == 2 {
                Shape::Pow(args[0], args[1])
            } else if head == n.failure {
                Shape::Failure
            } else if let Some((re, im)) = complex_parts(st, n, t) {
                Shape::Number(Num::complex(re, im))
            } else {
                Shape::Other
            }
        }
        _ => Shape::Other,
    }
}

/// The parts of a `complex(re, im)` term with real number parts, as written (not normalised).
pub(crate) fn complex_parts<'h, S: Store + ?Sized>(st: &'h S, n: &Names<'h>, t: T<'h>) -> Option<(Real, Real)> {
    match st.view(t) {
        View::App { head, args: [re, im] } if head == n.complex => Some((real_of(st, *re)?, real_of(st, *im)?)),
        _ => None,
    }
}

/// A number term as written: real numbers, and complex numbers without normalisation (for the order).
pub(crate) fn num_of_raw<'h, S: Store + ?Sized>(st: &'h S, n: &Names<'h>, t: T<'h>) -> Option<Num> {
    if let Some(x) = real_of(st, t) {
        return Some(Num::Real(x));
    }
    complex_parts(st, n, t).map(|(a, b)| Num::Complex(a, b))
}

pub(crate) fn int_of<S: Store + ?Sized>(st: &S, t: T<'_>) -> Option<Int> {
    match st.view(t) {
        View::Int(v) => Some(Int::from_i64(v)),
        View::BigInt { neg, mag } => Some(Int::new(neg, Nat::from_limbs(mag.to_vec()))),
        _ => None,
    }
}

/// A real number term's value.
pub(crate) fn real_of<S: Store + ?Sized>(st: &S, t: T<'_>) -> Option<Real> {
    Some(match st.view(t) {
        View::Int(v) => Real::Exact(Rat::from_i64(v)),
        View::BigInt { neg, mag } => Real::Exact(Rat::from_int(Int::new(neg, Nat::from_limbs(mag.to_vec())))),
        View::Rational { num, den } => Real::Exact(Rat { num: int_of(st, num)?, den: int_of(st, den)?.mag }),
        View::Real(v) => Real::Machine(v),
        View::Ball { neg, words } => {
            let (b, p) = decode_ball(neg, words)?;
            Real::Ball(b, p)
        }
        _ => return None,
    })
}

/// A ball from its payload words `[exp, radius mantissa, radius exponent, mantissa limbs...]` (I3.5). The working
/// precision is the mantissa's bit width; a zero midpoint keeps it in the exponent word (decision B.2).
pub(crate) fn decode_ball(neg: bool, words: &[u64]) -> Option<(Ball, u64)> {
    let [exp, rm, re, man @ ..] = words else { return None };
    let man = Nat::from_limbs(man.to_vec());
    let rad = Mag { m: *rm, e: *re as i64 };
    if man.is_zero() {
        return Some((Ball { mid: Float::zero(), rad }, (*exp).max(MIN_PREC)));
    }
    let p = man.bits().max(MIN_PREC);
    Some((Ball { mid: Float::new(neg, man, *exp as i64), rad }, p))
}

/// The canonical payload of a ball at precision p: the midpoint rounded to p bits (the rounding error joins the
/// radius) and widened to exactly p bits, so that equal balls at equal precision have equal words.
pub(crate) fn encode_ball(b: &Ball, p: u64) -> (bool, Vec<u64>) {
    let p = p.max(MIN_PREC);
    let (mid, err) = Float::round(b.mid.neg, b.mid.man.clone(), b.mid.exp, p);
    let rad = b.rad.add(&err);
    let rad = if rad.is_zero() { Mag::ZERO } else { rad };
    if mid.is_zero() {
        return (false, vec![p, rad.m, rad.e as u64]);
    }
    let shift = p - mid.man.bits();
    let man = mid.man.shl(shift);
    let mut words = vec![(mid.exp - shift as i64) as u64, rad.m, rad.e as u64];
    words.extend_from_slice(&man.limbs);
    (mid.neg, words)
}

pub(crate) fn int_term<'h, S: Store + ?Sized>(st: &'h S, x: &Int) -> T<'h> {
    match x.to_i64() {
        Some(v) => st.int(v),
        None => st.big_int(x.neg, &x.mag.limbs),
    }
}

pub(crate) fn rat_term<'h, S: Store + ?Sized>(st: &'h S, q: &Rat) -> T<'h> {
    if q.is_int() {
        return int_term(st, &q.num);
    }
    st.rational(int_term(st, &q.num), int_term(st, &Int::from_nat(q.den.clone())))
}

pub(crate) fn real_term<'h, S: Store + ?Sized>(st: &'h S, x: &Real) -> T<'h> {
    match x {
        Real::Exact(q) => rat_term(st, q),
        Real::Machine(v) => st.real(*v),
        Real::Ball(b, p) => {
            let (neg, words) = encode_ball(b, *p);
            st.ball(neg, &words)
        }
    }
}

pub(crate) fn num_term<'h, S: Store + ?Sized>(st: &'h S, n: &Names<'h>, x: &Num) -> T<'h> {
    match x {
        Num::Real(r) => real_term(st, r),
        Num::Complex(a, b) => st.app(n.complex, &[real_term(st, a), real_term(st, b)]),
    }
}

