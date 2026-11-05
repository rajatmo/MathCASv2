//! Enclosures of the ball functions and constants, checked against values computed at much higher precision, against
//! independent algorithms, and against known digits.
use mc_arith::num::{Ball, Float, Mag, Nat};
use mc_numeric::consts::Const;
use mc_numeric::elementary as el;
use mc_numeric::Fun;
use std::cmp::Ordering;

/// SplitMix64: reproducible inputs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn float(v: f64) -> Float {
    let (neg, man, exp) = mc_arith::num::float::f64_to_dyadic(v).expect("finite");
    Float::new(neg, man, exp)
}

/// Is the ball `inner` inside `outer`? (exactly: |m_i - m_o| + r_i <= r_o)
fn inside(inner: &Ball, outer: &Ball) -> bool {
    let d = inner.mid.sub_exact(&outer.mid).abs().add_exact(&inner.rad.to_float());
    d.cmp_exact(&outer.rad.to_float()) != Ordering::Greater
}

/// A random argument of f: a machine real in the function's natural range plus 64 random lower bits, and a radius
/// that is zero, about one unit at p, or a relative 2^-20.
fn argument(rng: &mut Rng, f: Fun, p: u64) -> Ball {
    let u = rng.unit();
    let v = match f {
        Fun::Exp => 100.0 * u - 50.0,
        Fun::Log => (30.0 * u - 15.0).exp(),
        Fun::Sin | Fun::Cos | Fun::Tan => 200.0 * u - 100.0,
        Fun::Atan => (2.0 * u - 1.0) * 10f64.powf(6.0 * rng.unit() - 3.0),
        Fun::Asin | Fun::Acos => 1.98 * u - 0.99,
        Fun::Sinh | Fun::Cosh | Fun::Tanh => 60.0 * u - 30.0,
        Fun::Sqrt => (30.0 * u - 15.0).exp(),
        // small arguments are where the stable forms matter
        Fun::Log1p => (1.8 * u - 0.9) * 10f64.powf(-12.0 * rng.unit()),
        Fun::Expm1 => (2.0 * u - 1.0) * 10f64.powf(1.0 - 13.0 * rng.unit()),
    };
    let mid = float(v);
    let mid = mid.add_exact(&Float::new(rng.below(2) == 1, Nat::from_u64(rng.next()), mid.top() - 53 - 64));
    let rad = match rng.below(3) {
        0 => Mag::ZERO,
        1 => Mag::pow2(mid.top() - p as i64 - rng.below(8) as i64),
        _ => Mag::pow2(mid.top() - 20),
    };
    Ball { mid, rad }
}

#[test]
fn every_function_encloses_its_values_on_the_argument_ball() {
    let mut rng = Rng(7);
    let trials = if cfg!(debug_assertions) { 25 } else { 120 };
    let mut checks = 0;
    for f in Fun::ALL {
        for _ in 0..trials {
            let p = [53u64, 100, 256, 700, 2000][rng.below(5) as usize];
            let x = argument(&mut rng, f, p);
            let Some(r) = f.ball(&x, p) else {
                // only a radius that reaches the domain's boundary may leave a function undefined
                assert!(!x.rad.is_zero() || matches!(f, Fun::Tan), "{f:?} undefined at an exact point");
                continue;
            };
            let rf = x.rad.to_float();
            for pt in [x.mid.sub_exact(&rf), x.mid.clone(), x.mid.add_exact(&rf)] {
                let reference =
                    f.ball(&Ball::exact(pt.clone()), 2 * p + 64).unwrap_or_else(|| panic!("{f:?} at a point"));
                checks += 1;
                assert!(inside(&reference, &r), "{f:?} at {:?} (p = {p}): {r:?} misses {reference:?}", pt.to_sci(30));
            }
            // tightness for exact arguments: an absolute error of a few units at p relative to max(|f|, 1)
            if x.rad.is_zero() && f != Fun::Tan {
                let scale = r.mid.top().max(1);
                assert!(r.rad.log2() < (scale - p as i64 + 12) as f64, "{f:?}: radius 2^{} at p = {p}", r.rad.log2());
            }
        }
    }
    assert!(checks > 12 * trials, "{checks}");
}

#[test]
fn identities_hold_at_high_precision() {
    let p = 10_000;
    let x = Ball::from_ratio(7, 10, p);
    let (s, c) = el::sin_cos(&x, p).unwrap();
    let one = Ball::from_i64(1);
    let check = |b: Ball, what: &str| {
        assert!(b.cmp3(&one).is_none(), "{what} excludes 1");
        assert!(b.rel_bits() > (p - 32) as f64, "{what}: {} bits", b.rel_bits());
    };
    check(s.mul(&s, p).add(&c.mul(&c, p), p), "sin^2 + cos^2");
    check(el::exp(&el::log(&x, p).unwrap(), p).unwrap().div(&x, p).unwrap(), "exp(log x) / x");
    check(el::tan(&el::atan(&x, p).unwrap(), p).unwrap().div(&x, p).unwrap(), "tan(atan x) / x");
    let ch = el::cosh(&x, p).unwrap();
    let sh = el::sinh(&x, p).unwrap();
    check(ch.mul(&ch, p).sub(&sh.mul(&sh, p), p), "cosh^2 - sinh^2");
    check(el::sin(&el::asin(&x, p).unwrap(), p).unwrap().div(&x, p).unwrap(), "sin(asin x) / x");
    // large arguments: the reduction by pi/2 and the e^k factor
    let big = Ball::exact(Float::new(false, Nat::from_u64(123_456_789), 20));
    let (s, c) = el::sin_cos(&big, 2000).unwrap();
    assert!(s.mul(&s, 2000).add(&c.mul(&c, 2000), 2000).cmp3(&one).is_none());
    assert!(s.rel_bits() > 1900.0);
    let e = el::exp(&Ball::from_i64(-700), 2000).unwrap().mul(&el::exp(&Ball::from_i64(700), 2000).unwrap(), 2000);
    assert!(e.cmp3(&one).is_none() && e.rel_bits() > 1900.0);
}

/// e by the continued fraction e = [2; 1, 2, 1, 1, 4, 1, 1, 6, ...] (an algorithm independent of the series):
/// the convergents p/q bracket e alternately, |e - p/q| < 1/q^2.
fn e_by_continued_fraction(bits: u64) -> (Nat, Nat) {
    let coeff = |i: u64| match i {
        0 => 2,
        _ if i % 3 == 2 => 2 * i.div_ceil(3),
        _ => 1,
    };
    let (mut p0, mut q0, mut p1, mut q1) = (Nat::from_u64(1), Nat::zero(), Nat::from_u64(coeff(0)), Nat::from_u64(1));
    let mut i = 1;
    while q1.bits() < bits / 2 + 8 {
        let a = coeff(i);
        let p2 = p1.mul_u64(a).add(&p0);
        let q2 = q1.mul_u64(a).add(&q0);
        (p0, q0, p1, q1) = (p1, q1, p2, q2);
        i += 1;
    }
    (p1, q1)
}

#[test]
fn constants_agree_with_independent_algorithms() {
    let p = 8_000;
    // pi = 4 atan(1): Newton on sin and cos at |y| < 1, which needs no pi
    let pi = Const::Pi.ball(p).unwrap();
    let pi2 = el::atan(&Ball::from_i64(1), p).unwrap().mul(&Ball::from_i64(4), p);
    assert!(pi.cmp3(&pi2).is_none() && pi.rel_bits() > (p - 4) as f64);
    // pi = 6 asin(1/2) and 2 acos(0)
    assert!(el::asin(&Ball::from_ratio(1, 2, p), p).unwrap().mul(&Ball::from_i64(6), p).cmp3(&pi).is_none());
    // e: the continued fraction, and exp(1/2)^2 (the bit-burst series alone)
    let e = Const::E.ball(p).unwrap();
    let (num, den) = e_by_continued_fraction(p);
    let cf = Ball::exact(Float::from_nat(num)).div(&Ball::exact(Float::from_nat(den)), p).unwrap();
    let cf = Ball { rad: cf.rad.add(&Mag::pow2(-(p as i64))), ..cf };
    assert!(e.cmp3(&cf).is_none());
    let h = el::exp(&Ball::from_ratio(1, 2, p), p).unwrap();
    assert!(h.mul(&h, p).cmp3(&e).is_none());
    assert!(el::log(&e, p).unwrap().cmp3(&Ball::from_i64(1)).is_none());
    // gamma and G at two precisions (different series lengths and Brent-McMillan parameters)
    for c in [Const::EulerGamma, Const::Catalan] {
        let a = c.ball(1000).unwrap();
        let b = c.ball(3000).unwrap();
        assert!(
            a.cmp3(&b).is_none()
                && inside(&mc_numeric::elementary::round(b, 3000), &Ball { rad: a.rad.add(&Mag::pow2(-990)), ..a })
        );
    }
}

#[test]
fn ten_thousand_digits_of_pi_match_the_reference() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/cases/N/ref/N1-pi-1e4.txt"))
        .expect("the N1 reference");
    let want: String = text.chars().filter(|c| c.is_ascii_digit()).take(10_000).collect();
    let b = Const::Pi.ball(mc_arith::text::digits_to_bits(10_010)).unwrap();
    assert!(b.has_digits(10_005));
    let (got, e) = b.mid.to_sci(10_005);
    assert_eq!(e, 0);
    assert_eq!(&got[..10_000], want);
}
