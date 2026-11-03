//! Every public number operation under a raised stop flag: it must return (any value) without panicking or looping.
//!
//! Once the flag is up, long products return early with wrong limbs, so every later step of an operation sees
//! values that break the invariants of exact arithmetic. Two ways of raising the flag are tested: through a real
//! [`StopScope`] before the operation starts, and through a per-thread countdown that makes [`super::stopped`]
//! answer `true` from its n-th poll on, which raises the flag at every point of the operation where a kernel looks
//! at it (the deterministic version of an interrupt arriving at an arbitrary moment).
use super::*;
use std::cell::Cell;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

thread_local! {
    /// Polls left before the simulated flag goes up (`u64::MAX`: never).
    static LEFT: Cell<u64> = const { Cell::new(u64::MAX) };
    /// Polls made while a countdown is armed.
    static POLLS: Cell<u64> = const { Cell::new(0) };
}

/// The simulated flag, consulted by [`super::stopped`] in test builds: up once the countdown reaches zero.
pub(super) fn countdown() -> bool {
    LEFT.with(|l| {
        let v = l.get();
        if v == u64::MAX {
            return false;
        }
        POLLS.with(|p| p.set(p.get() + 1));
        if v == 0 {
            return true;
        }
        l.set(v - 1);
        false
    })
}

/// Run `f` with the simulated flag going up at poll `n`; returns how many polls `f` made.
fn with_countdown(n: u64, f: &dyn Fn()) -> u64 {
    LEFT.with(|l| l.set(n));
    POLLS.with(|p| p.set(0));
    f();
    LEFT.with(|l| l.set(u64::MAX));
    POLLS.with(|p| p.get())
}

/// A reproducible natural number of `limbs` limbs with its top limb nonzero.
fn nat(rng: &mut Rng, limbs: usize) -> Nat {
    let mut v: Vec<u64> = (0..limbs).map(|_| rng.next_u64()).collect();
    if let Some(t) = v.last_mut() {
        *t |= 1 << 63;
    }
    Nat::from_limbs(v)
}

/// A named operation.
type Operation = (&'static str, Box<dyn Fn()>);

/// The operations, each on operands large enough to reach the NTT, Newton division and the divide-and-conquer
/// conversions (the stopping paths), named for the failure message.
fn operations() -> Vec<Operation> {
    let mut rng = Rng(91);
    let a = nat(&mut rng, 3200);
    let b = nat(&mut rng, 1700);
    let c = nat(&mut rng, 400);
    let small = nat(&mut rng, 3);
    let ia = Int::new(true, a.clone());
    let ib = Int::from_nat(b.clone());
    let ra = Rat::new(Int::from_nat(a.clone()), Int::from_nat(c.clone()));
    let rb = Rat::new(Int::new(true, b.clone()), Int::from_nat(small.clone()));
    let p = 64 * 2500;
    let x = Ball::from_nat(a.clone());
    let y = Ball::exact(Float::new(false, b.clone(), -70_000));
    let dec = a.mul(&b).to_decimal();
    let fl = x.mid.clone();
    let fy = y.mid.clone();
    let num_a = Num::rat(ra.clone());
    let num_b = Num::Real(Real::Ball(y.clone(), p));
    macro_rules! ops {
        ($($name:literal => $body:expr),* $(,)?) => {
            vec![$(($name, Box::new($body) as Box<dyn Fn()>)),*]
        };
    }
    ops![
        "Nat::add" => { let (a, b) = (a.clone(), b.clone()); move || { a.add(&b); } },
        "Nat::sub" => { let (a, b) = (a.clone(), b.clone()); move || { a.sub(&b); } },
        "Nat::mul" => { let (a, b) = (a.clone(), b.clone()); move || { a.mul(&b); } },
        "Nat::square" => { let a = a.clone(); move || { a.square(); } },
        "Nat::mul_u64, shl, shr, low_bits" => { let a = a.clone(); move || { a.mul_u64(7).shl(1000).shr(77).low_bits(9000); } },
        "Nat::pow" => || { Nat::pow(10, 200_000); },
        "Nat::pow_u64" => { let s = small.clone(); move || { s.pow_u64(5000); } },
        "Nat::divrem_u64" => { let a = a.clone(); move || { a.divrem_u64(12345); } },
        "Nat::divrem (Newton)" => { let (a, b) = (a.mul(&a), b.clone()); move || { a.divrem(&b); } },
        "Nat::divrem (truncated)" => { let (a, b) = (a.clone(), a.shr(64 * 20).add_u64(1)); move || { a.divrem(&b); } },
        "Nat::divrem (Knuth)" => { let (a, c) = (c.mul(&c), c.shr(64 * 200)); move || { a.divrem(&c); } },
        "Nat::div" => { let (a, b) = (a.clone(), b.clone()); move || { a.div(&b); } },
        "Nat::isqrt" => { let a = a.mul(&a).add_u64(5); move || { a.isqrt(); } },
        "Nat::iroot" => { let a = a.clone(); move || { a.iroot(3); a.iroot(7); } },
        "Nat::to_decimal" => { let a = a.mul(&b); move || { a.to_decimal(); } },
        "Nat::from_decimal" => { let d = dec.clone(); move || { Nat::from_decimal(&d); } },
        "Divisor" => { let (a, b) = (a.mul(&a), b.clone()); move || { Divisor::new(&b).divrem(&a); } },
        "gcd" => { let (a, b) = (a.mul(&c), b.mul(&c)); move || { gcd::gcd(&a, &b); } },
        "gcd::div_exact" => { let (a, c) = (a.mul(&c), c.clone()); move || { gcd::div_exact(&a, &c); } },
        "Int::add, sub, mul" => { let (x, y) = (ia.clone(), ib.clone()); move || { x.add(&y).sub(&y.mul(&x)); } },
        "Int::divrem_floor" => { let (x, b) = (ia.clone(), b.clone()); move || { x.divrem_floor(&b); } },
        "Rat::new" => { let (x, y) = (ia.mul(&ib), ib.mul(&ib)); move || { Rat::new(x.clone(), y.clone()); } },
        "Rat::add, sub, mul" => { let (x, y) = (ra.clone(), rb.clone()); move || { x.add(&y).sub(&x.mul(&y)); } },
        "Rat::recip, pow_u64, floor_fract, cmp_value" => {
            let (x, y) = (ra.clone(), rb.clone());
            move || {
                x.recip();
                y.pow_u64(9).floor_fract();
                x.cmp_value(&y);
            }
        },
        "Float::add, mul, div, sqrt" => {
            let (f, g) = (fl.clone(), fy.clone());
            move || {
                f.add(&g, p);
                f.mul(&g, p);
                f.div(&g, p);
                f.sqrt(p);
            }
        },
        "Float::to_sci" => { let f = fl.clone(); move || { f.to_sci(20_000); } },
        "Ball::add, sub, mul, div, sqrt" => {
            let (x, y) = (x.clone(), y.clone());
            move || {
                x.add(&y, p);
                x.sub(&y, p);
                x.mul(&y, p);
                x.div(&y, p);
                y.sqrt(p);
                Ball::from_ratio(1, 3, p);
            }
        },
        "Ball digits" => { let y = y.clone(); move || { text::ball(&y, p); text::ball_digits(&y.div(&Ball::from_i64(3), p).unwrap_or(y.clone()), p); } },
        "float conversions" => {
            let (a, b) = (a.clone(), b.clone());
            move || {
                float::ratio_to_f64(false, &a, &b);
                float::dyadic_to_f64(true, &a, -200_000);
            }
        },
        "Num::add, mul, recip, pow_int" => {
            let (x, y) = (num_a.clone(), num_b.clone());
            move || {
                x.add(&y);
                x.mul(&y);
                y.recip().ok();
                x.pow_int(&Int::from_i64(-7), 1 << 28).ok();
                y.pow_int(&Int::from_i64(5), 1 << 28).ok();
                x.pow(&Num::rat(Rat::new(Int::from_i64(1), Int::from_i64(2))), 1 << 28).ok();
            }
        },
        "number text" => { let x = ra.clone(); move || { text::rational(&x); text::integer(&ia); } },
    ]
}

use super::super::number::{Num, Real};
use super::super::text;

#[test]
fn every_operation_returns_under_a_raised_flag() {
    let flag = Arc::new(AtomicBool::new(true));
    let _scope = StopScope::enter(Some(&flag));
    for (name, op) in operations() {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(op));
        assert!(r.is_ok(), "{name} panicked under a raised stop flag");
    }
}

#[test]
fn every_operation_returns_when_stopped_midway() {
    for (name, op) in operations() {
        // the number of polls of an uninterrupted run; then the flag goes up at polls spread over that run
        let total = with_countdown(u64::MAX - 1, &*op);
        let points = 12u64.min(total + 1);
        for k in 0..points {
            let n = k * (total + 1) / points;
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| with_countdown(n, &*op)));
            LEFT.with(|l| l.set(u64::MAX));
            assert!(r.is_ok(), "{name} panicked when stopped at poll {n} of {total}");
        }
    }
}
