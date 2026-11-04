//! Elementary functions on complex balls (package N0C): containment against independent references at three times
//! the precision, agreement with the machine complex functions, identities, branch cuts, exact values through the
//! evaluator, and interruption.
mod common;
use common::Engine;
use mc_arith::num::float::{dyadic_to_f64, f64_to_dyadic};
use mc_arith::num::{Ball, Float, Int, Mag, Nat, Rat};
use mc_numeric::cball::{self, CBall};
use mc_numeric::consts::Const;
use mc_numeric::fun::Mach;
use mc_numeric::machine::C64;
use mc_numeric::numeval::first_precision;
use mc_numeric::Fun;
use std::cmp::Ordering;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

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
    let (neg, man, exp) = f64_to_dyadic(v).expect("finite");
    Float::new(neg, man, exp)
}

fn exact(v: f64) -> Ball {
    Ball::exact(float(v))
}

fn f64_of(x: &Float) -> f64 {
    dyadic_to_f64(x.neg, &x.man, x.exp)
}

/// Is the ball `inner` inside `outer`? (exactly: |m_i - m_o| + r_i <= r_o)
fn inside(inner: &Ball, outer: &Ball) -> bool {
    let d = inner.mid.sub_exact(&outer.mid).abs().add_exact(&inner.rad.to_float());
    d.cmp_exact(&outer.rad.to_float()) != Ordering::Greater
}

/// Is the reference consistent with the result: inside it, or, for an exact result part (an exact zero imaginary
/// part of a real value, say), containing it?
fn agrees(reference: &Ball, result: &Ball) -> bool {
    if result.rad.is_zero() {
        return inside(result, reference);
    }
    inside(reference, result)
}

fn inside_c(inner: &CBall, outer: &CBall) -> bool {
    agrees(&inner.re, &outer.re) && agrees(&inner.im, &outer.im)
}

/// Does the ball contain the exact point?
fn contains(outer: &CBall, z: &CBall) -> bool {
    inside(&z.re, &outer.re) && inside(&z.im, &outer.im)
}

fn scale(b: &Ball, k: i64) -> Ball {
    Ball { mid: Float::new(b.mid.neg, b.mid.man.clone(), b.mid.exp + k), rad: b.rad.mul_2exp(k) }
}

/// e^z by its Taylor series, independent of `cball::exp` and of the real ball functions: z/2^k with |z/2^k| < 1/4,
/// N terms with a tail below 2 |x|^N / N! <= 2^(1 - 2N) / N!, then k squarings.
fn exp_series(z: &CBall, p: u64) -> CBall {
    let size = |x: &CBall| {
        f64_of(&x.re.mid).abs() + f64_of(&x.im.mid).abs() + 2f64.powf(x.re.rad.log2()) + 2f64.powf(x.im.rad.log2())
    };
    let mut x = z.clone();
    let mut k = 0;
    while size(&x) > 0.2 {
        x = CBall::new(scale(&x.re, -1), scale(&x.im, -1));
        k += 1;
    }
    let w = p + k + 32;
    let mut n = 1u64;
    let mut log2_fact = 0.0f64;
    while 2.0 * n as f64 + log2_fact < (w + 4) as f64 {
        n += 1;
        log2_fact += (n as f64).log2();
    }
    let (mut term, mut sum) = (CBall::from_i64(1), CBall::from_i64(1));
    for j in 1..n {
        term = term.mul(&x, w).div(&CBall::from_i64(j as i64), w).unwrap();
        sum = sum.add(&term, w);
    }
    let tail = Mag::pow2(-(w as i64));
    let real = x.is_real();
    let widen = |b: Ball| Ball { mid: b.mid, rad: b.rad.add(&tail) };
    sum = if real { CBall::real(widen(sum.re)) } else { CBall::new(widen(sum.re), widen(sum.im)) };
    for _ in 0..k {
        sum = sum.square(w);
    }
    sum
}

/// The reference functions built from [`exp_series`].
fn ref_trig(z: &CBall, p: u64) -> (CBall, CBall) {
    let w = p + 16;
    let (a, b) = (exp_series(&z.mul_i(), w), exp_series(&z.mul_i().neg(), w));
    let sin = a.sub(&b, w).div(&CBall::new(Ball::from_i64(0), Ball::from_i64(2)), w).unwrap();
    let cos = a.add(&b, w).div(&CBall::from_i64(2), w).unwrap();
    (sin, cos)
}

fn ref_hyp(z: &CBall, p: u64) -> (CBall, CBall) {
    let w = p + 16;
    let (a, b) = (exp_series(z, w), exp_series(&z.neg(), w));
    (a.sub(&b, w).div(&CBall::from_i64(2), w).unwrap(), a.add(&b, w).div(&CBall::from_i64(2), w).unwrap())
}

/// The functions under test, by name.
const NAMES: [&str; 18] = [
    "exp",
    "log",
    "sqrt",
    "sin",
    "cos",
    "tan",
    "sinh",
    "cosh",
    "tanh",
    "atan",
    "asin",
    "acos",
    "pow_int",
    "pow_third",
    "pow_neg_half",
    "pow_complex",
    "abs",
    "arg",
];

fn w_exponent() -> CBall {
    CBall::new(exact(0.75), exact(-1.25))
}

fn third(p: u64) -> CBall {
    CBall::real(Ball::from_i64(1).div(&Ball::from_i64(3), p).unwrap())
}

fn eval(name: &str, z: &CBall, p: u64) -> Option<CBall> {
    let fun = |f: &str| Fun::from_name(f).unwrap();
    match name {
        "pow_int" => cball::pow_int(z, &Int::from_i64(7), p),
        "pow_third" => cball::pow_rat(z, &Rat::new(Int::from_i64(1), Int::from_i64(3)), p),
        "pow_neg_half" => cball::pow_rat(z, &Rat::new(Int::from_i64(-5), Int::from_i64(2)), p),
        "pow_complex" => cball::pow(z, &w_exponent(), p),
        "abs" => cball::abs(z, p).map(CBall::real),
        "arg" => cball::arg(z, p).map(CBall::real),
        f => fun(f).cball(z, p),
    }
}

/// An independent reference value at an exact point, at p bits, or `None` when f has no value there. For the
/// inverse functions the high-precision value is checked to invert the [`exp_series`] references (and to lie in the
/// principal range).
fn reference(name: &str, z: &CBall, p: u64) -> Option<CBall> {
    let w = p + 16;
    let pi = Const::Pi.ball(w).unwrap();
    let check = |back: CBall| assert!(contains(&back, z), "{name}: the reference does not invert at {z:?}");
    let le = |a: &Ball, b: &Ball| a.mid.cmp_exact(&b.mid) != Ordering::Greater;
    let r = match name {
        "exp" => exp_series(z, p),
        "sin" => ref_trig(z, p).0,
        "cos" => ref_trig(z, p).1,
        "tan" => {
            let (s, c) = ref_trig(z, p);
            s.div(&c, p)?
        }
        "sinh" => ref_hyp(z, p).0,
        "cosh" => ref_hyp(z, p).1,
        "tanh" => {
            let (s, c) = ref_hyp(z, p);
            s.div(&c, p)?
        }
        "log" => {
            let v = cball::log(z, w)?;
            check(exp_series(&v, w));
            assert!(le(&pi.neg(), &v.im) && le(&v.im, &pi), "log range at {z:?}");
            v
        }
        "sqrt" => {
            let v = cball::sqrt(z, w)?;
            check(v.square(w));
            assert!(!v.re.mid.neg, "sqrt range at {z:?}");
            v
        }
        "atan" => {
            let v = cball::atan(z, w)?;
            let (s, c) = ref_trig(&v, w);
            check(s.div(&c, w)?);
            let hp = scale(&pi, -1);
            assert!(le(&hp.neg(), &v.re) && le(&v.re, &hp), "atan range at {z:?}");
            v
        }
        "asin" => {
            let v = cball::asin(z, w)?;
            check(ref_trig(&v, w).0);
            let hp = scale(&pi, -1);
            assert!(le(&hp.neg(), &v.re) && le(&v.re, &hp), "asin range at {z:?}");
            v
        }
        "acos" => {
            let v = cball::acos(z, w)?;
            check(ref_trig(&v, w).1);
            assert!(!v.re.mid.neg && le(&v.re, &pi), "acos range at {z:?}");
            v
        }
        "pow_int" => {
            let mut r = CBall::from_i64(1);
            for _ in 0..7 {
                r = r.mul(z, w);
            }
            r
        }
        "pow_third" => {
            let v = cball::pow(z, &third(w + 64), w)?;
            check(v.mul(&v, w).mul(&v, w));
            let a = cball::arg(&v, w)?;
            let lim = pi.div(&Ball::from_i64(3), w).unwrap();
            assert!(le(&lim.neg(), &a) && le(&a, &lim), "pow_third range at {z:?}");
            v
        }
        "pow_neg_half" => {
            let s = cball::sqrt(z, w + 16)?;
            let s5 = s.square(w + 16).square(w + 16).mul(&s, w + 16);
            s5.recip(w)?
        }
        "pow_complex" => {
            let l = cball::log(z, w + 16)?;
            exp_series(&l.mul(&w_exponent(), w + 16), w)
        }
        "abs" => {
            let v = cball::abs(z, w)?;
            assert!(
                contains(&CBall::real(v.mul(&v, w)), &norm(z, w + 64))
                    || norm(z, w + 64).re.cmp3(&v.mul(&v, w)).is_none()
            );
            CBall::real(v)
        }
        "arg" => {
            let v = cball::arg(z, w)?;
            check(cball::exp(&CBall::new(Ball::from_i64(0), v.clone()), w)?.mul_real(&cball::abs(z, w)?, w));
            CBall::real(v)
        }
        _ => unreachable!(),
    };
    Some(r)
}

/// a^2 + b^2.
fn norm(z: &CBall, w: u64) -> CBall {
    CBall::real(z.re.mul(&z.re, w).add(&z.im.mul(&z.im, w), w))
}

/// A random argument: parts (2u - 1) 10^v with v in [-1.5, 1] and 64 random lower bits; one in eight on an axis
/// (an exact zero part); radius zero, about one unit at p, or a relative 2^-20.
fn argument(rng: &mut Rng, p: u64) -> CBall {
    let part = |rng: &mut Rng| {
        let v = (2.0 * rng.unit() - 1.0) * 10f64.powf(2.5 * rng.unit() - 1.5);
        let mid = float(v);
        mid.add_exact(&Float::new(rng.below(2) == 1, Nat::from_u64(rng.next()), mid.top() - 53 - 64))
    };
    let (mut re, mut im) = (part(rng), part(rng));
    match rng.below(16) {
        0 => im = Float::zero(),
        1 => re = Float::zero(),
        _ => {}
    }
    let rad = |rng: &mut Rng, m: &Float| {
        if m.is_zero() {
            return Mag::ZERO;
        }
        match rng.below(4) {
            0 | 1 => Mag::ZERO,
            2 => Mag::pow2(m.top() - p as i64 - rng.below(8) as i64),
            _ => Mag::pow2(m.top() - 20),
        }
    };
    let (rr, ri) = (rad(rng, &re), rad(rng, &im));
    CBall::new(Ball { mid: re, rad: rr }, Ball { mid: im, rad: ri })
}

/// The midpoint and the four corners of the rectangle (exact points).
fn points(z: &CBall) -> Vec<CBall> {
    let mut v = vec![CBall::new(Ball::exact(z.re.mid.clone()), Ball::exact(z.im.mid.clone()))];
    if z.re.rad.is_zero() && z.im.rad.is_zero() {
        return v;
    }
    for (sr, si) in [(false, false), (false, true), (true, false), (true, true)] {
        let corner = |m: &Float, r: &Mag, s: bool| {
            let r = r.to_float();
            Ball::exact(if s { m.sub_exact(&r) } else { m.add_exact(&r) })
        };
        v.push(CBall::new(corner(&z.re.mid, &z.re.rad, sr), corner(&z.im.mid, &z.im.rad, si)));
    }
    v
}

#[test]
fn results_contain_independent_references() {
    let mut rng = Rng(0x0c0b_a115);
    let count = if cfg!(debug_assertions) { 40 } else { 200 };
    let mut none = vec![0usize; NAMES.len()];
    let mut total = 0usize;
    for d in [30u64, 100] {
        let p = first_precision(d);
        for _ in 0..count {
            let z = argument(&mut rng, p);
            let pts = points(&z);
            for (k, name) in NAMES.iter().enumerate() {
                total += 1;
                let Some(r) = eval(name, &z, p) else {
                    none[k] += 1;
                    continue;
                };
                for x in &pts {
                    let reference = reference(name, x, 3 * p);
                    let reference = reference.unwrap_or_else(|| panic!("{name}: a value at {z:?} but none at {x:?}"));
                    assert!(inside_c(&reference, &r), "{name}({z:?}) at {d} digits: {r:?} misses {reference:?}");
                }
                // an exact argument gives about p correct bits relative to |f| (no gross loss)
                if pts.len() == 1 && !matches!(*name, "arg" | "abs") {
                    let m = f64_of(&r.re.mid).abs().max(f64_of(&r.im.mid).abs()).log2();
                    let rad = r.re.rad.log2().max(r.im.rad.log2());
                    assert!(m - rad > p as f64 / 2.0, "{name}({z:?}): {} bits", m - rad);
                }
            }
        }
    }
    // the misses are the poles, points and cuts the random regions touch (and wide regions near them)
    for (k, n) in none.iter().enumerate() {
        assert!(*n * 10 <= 2 * count, "{}: no value for {n} of {} arguments", NAMES[k], 2 * count);
    }
    assert!(total >= 2 * count * NAMES.len());
}

#[test]
fn abs_reference_is_consistent() {
    // |z|^2 = a^2 + b^2 at 3 precisions (the check in `reference` compares v^2 against z itself, which is only
    // meaningful through this one)
    let z = CBall::new(exact(3.0), exact(-4.0));
    for p in [64, 300, 1000] {
        let a = cball::abs(&z, p).unwrap();
        assert!(inside(&Ball::from_i64(5), &a));
        assert!(contains(&CBall::real(a.mul(&a, p)), &CBall::real(norm(&z, p).re)));
    }
}

#[test]
fn agrees_with_machine_complex_functions() {
    let mut rng = Rng(42);
    let n = if cfg!(debug_assertions) { 100 } else { 1000 };
    for _ in 0..n {
        let (re, im) = ((2.0 * rng.unit() - 1.0) * 3.0, (2.0 * rng.unit() - 1.0) * 3.0);
        let (z, m) = (CBall::new(exact(re), exact(im)), C64::new(re, im));
        for f in Fun::ALL {
            let Some(b) = f.cball(&z, first_precision(15)) else { panic!("{}({re}, {im})", f.name()) };
            let Mach::Complex(w) = f.machine(Mach::Complex(m)) else { unreachable!() };
            let scale = w.re.abs().max(w.im.abs()).max(1e-300);
            let (er, ei) = ((f64_of(&b.re.mid) - w.re).abs(), (f64_of(&b.im.mid) - w.im).abs());
            assert!(
                er.max(ei) <= 1e-12 * scale,
                "{}({re}, {im}): ball {:e} {:e}, machine {w:?}",
                f.name(),
                f64_of(&b.re.mid),
                f64_of(&b.im.mid)
            );
        }
    }
    // the cuts, approached exactly (exact zero parts are on the side the machine functions take for +0.0)
    for (re, im) in [(-2.0, 0.0), (2.0, 0.0), (-3.0, 0.0), (0.0, 2.0), (0.0, -2.0), (-0.5, 0.0), (1.5, 0.0)] {
        let (z, m) = (CBall::new(exact(re), exact(im)), C64::new(re, im));
        for f in Fun::ALL {
            let Some(b) = f.cball(&z, 80) else { panic!("{}({re}, {im})", f.name()) };
            let w = f.machine(Mach::Complex(m)).c64();
            let scale = w.re.abs().max(w.im.abs()).max(1.0);
            let (er, ei) = ((f64_of(&b.re.mid) - w.re).abs(), (f64_of(&b.im.mid) - w.im).abs());
            assert!(
                er.max(ei) <= 1e-13 * scale,
                "{}({re}, {im}): {:e} {:e} vs {w:?}",
                f.name(),
                f64_of(&b.re.mid),
                f64_of(&b.im.mid)
            );
        }
    }
}

#[test]
fn identities() {
    let mut rng = Rng(7);
    let n = if cfg!(debug_assertions) { 30 } else { 150 };
    for d in [30, 100] {
        let p = first_precision(d);
        for _ in 0..n {
            let z = argument(&mut rng, p);
            let one = CBall::from_i64(1);
            let point = z.re.rad.is_zero() && z.im.rad.is_zero();
            if let Some(l) = cball::log(&z, p) {
                let e = cball::exp(&l, p).unwrap();
                assert!(if point { contains(&e, &z) } else { overlaps(&z, &e) }, "exp(log z) = z at {z:?}");
            }
            let (s, c) = cball::sin_cos(&z, p).unwrap();
            let sum = s.square(p).add(&c.square(p), p);
            assert!(contains(&sum, &one), "sin^2 + cos^2 = 1 at {z:?}: {sum:?}");
            if let Some(r) = cball::sqrt(&z, p) {
                let sq = r.square(p);
                assert!(if point { contains(&sq, &z) } else { overlaps(&z, &sq) }, "sqrt(z)^2 = z at {z:?}");
                assert!(!r.re.mid.neg || r.re.contains_zero());
            }
            let (sh, ch) = (cball::sinh(&z, p).unwrap(), cball::cosh(&z, p).unwrap());
            let diff = ch.square(p).sub(&sh.square(p), p);
            assert!(contains(&diff, &one), "cosh^2 - sinh^2 = 1 at {z:?}");
            if let (Some(t), Some(q)) = (cball::tan(&z, p), s.div(&c, p)) {
                assert!(overlaps(&t, &q), "tan = sin/cos at {z:?}");
            }
        }
    }
}

/// Do the rectangles overlap?
fn overlaps(a: &CBall, b: &CBall) -> bool {
    let meet = |x: &Ball, y: &Ball| matches!(x.cmp3(y), None | Some(Ordering::Equal));
    meet(&a.re, &b.re) && meet(&a.im, &b.im)
}

#[test]
fn branch_cuts_at_the_api() {
    let p = 128;
    let tiny = Ball { mid: Float::zero(), rad: Mag::pow2(-40) };
    let pi = Const::Pi.ball(p).unwrap();
    // exactly on the negative axis: the upper side (arg = pi)
    let l = cball::log(&CBall::real(exact(-2.0)), p).unwrap();
    assert!(inside(&l.im, &Const::Pi.ball(p).unwrap()) || l.im.cmp3(&pi).is_none());
    let s = cball::sqrt(&CBall::real(exact(-9.0)), p).unwrap();
    assert!(s.re == Ball::from_i64(0) && inside(&Ball::from_i64(3), &s.im));
    // a region across the negative axis: no value (not analytic there)
    let across = CBall::new(exact(-2.0), tiny.clone());
    for f in [Fun::Log, Fun::Sqrt] {
        assert!(f.cball(&across, p).is_none(), "{}", f.name());
    }
    assert!(cball::arg(&across, p).is_none());
    assert!(cball::pow_rat(&across, &Rat::new(Int::from_i64(1), Int::from_i64(3)), p).is_none());
    // the same region on the positive axis, or just above the negative one, has values
    assert!(cball::log(&CBall::new(exact(2.0), tiny.clone()), p).is_some());
    let above = CBall::new(exact(-2.0), Ball { mid: float(1e-6), rad: Mag::pow2(-40) });
    let la = cball::log(&above, p).unwrap();
    assert!(la.im.cmp3(&pi) == Some(Ordering::Less) && la.im.mid.top() == pi.mid.top());
    let below = above.conj();
    let lb = cball::log(&below, p).unwrap();
    assert!(lb.im.mid.neg);
    // asin and acos: the real axis beyond +-1; atan: the imaginary axis beyond +-i
    assert!(cball::asin(&CBall::new(exact(2.0), tiny.clone()), p).is_none());
    assert!(cball::acos(&CBall::new(exact(-3.0), tiny.clone()), p).is_none());
    assert!(cball::atan(&CBall::new(tiny.clone(), exact(2.0)), p).is_none());
    assert!(cball::asin(&CBall::new(exact(0.5), tiny.clone()), p).is_some());
    assert!(cball::atan(&CBall::new(tiny.clone(), exact(0.5)), p).is_some());
    // points: log 0, atan(+-i), tan at a pole, 0 to a power with non-positive real part
    assert!(cball::log(&CBall::from_i64(0), p).is_none());
    assert!(cball::atan(&CBall::i(), p).is_none() && cball::atan(&CBall::i().neg(), p).is_none());
    let half_pi = CBall::real(scale(&pi, -1));
    assert!(cball::tan(&half_pi, p).is_none());
    assert!(cball::pow(&CBall::from_i64(0), &CBall::new(exact(-1.0), exact(1.0)), p).is_none());
    assert!(cball::pow(&CBall::from_i64(0), &CBall::new(exact(1.0), exact(1.0)), p).unwrap().is_exact_zero());
    // abs of a region around 0 still encloses
    let a = cball::abs(&CBall::new(tiny.clone(), tiny), p).unwrap();
    assert!(a.contains_zero() && !a.mid.neg);
}

#[test]
fn through_the_evaluator() {
    let mut e = Engine::new();
    let cases: &[(&str, &str)] = &[
        ("n(exp(1 + 2im), 30)", "complex(-1.1312043837568136384312552555107947, 2.471726672004818927616930893551665)"),
        (
            "n(sqrt(1 + 1im), 30)",
            "complex(1.0986841134678099660398011952406784, 0.45508986056222734130435775782246857)",
        ),
        ("n(log(-2 + 1im), 30)", "complex(0.8047189562170501873003796666130938, 2.6779450445889871222483871518182885)"),
        ("n(atan(1 + 1im), 30)", "complex(1.0172219678978513677227889615504829, 0.4023594781085250936501898333065469)"),
        (
            "n((1 + 2im)^(1/3), 30)",
            "complex(1.219616507971757622571496581351432, 0.4717112677893890047819389461677654)",
        ),
        ("n(sin(1 + 2im), 20)", "complex(3.16577851321616814674073, 1.959601041421605897070352)"),
        ("n(log(-2), 20)", "complex(0.693147180559945309417232, 3.14159265358979323846264)"),
        ("n(asin(2), 20)", "complex(1.570796326794896619231322, -1.316957896924816708625046)"),
        ("n((-8)^(1/3), 20)", "complex(1.00000000000000000000000000, 1.73205080756887729352745)"),
        // exact values stay exact (the identities of `exact` run first)
        ("exp(im*pi)", "-1"),
        ("exp(im*pi/2)", "1im"),
        ("log(-1)", "1im*pi"),
        ("sqrt(-4)", "2im"),
        ("n(exp(im*pi), 30)", "-1.000000000000000000000000000000000"),
        // complex balls as arguments of the functions and of powers
        (
            "z = n(1 + 2im, 30); exp(z)",
            "complex(-1.1312043837568136384312552555107947, 2.471726672004818927616930893551665)",
        ),
        (
            "z = n(1 + 2im, 30); sqrt(z)",
            "complex(1.2720196495140689642524224617374915, 0.7861513777574232860695585858429589)",
        ),
        (
            "z = n(1 + 2im, 30); z^(1/2)",
            "complex(1.2720196495140689642524224617374915, 0.7861513777574232860695585858429589)",
        ),
        (
            "z = n(1 + 2im, 30); z^z",
            "complex(-0.22251715680177260839710476114667904, 0.10070913113607538371230775446157579)",
        ),
        (
            "z = n(1 + 2im, 30); exp(log(z))",
            "complex(1.00000000000000000000000000000000, 2.000000000000000000000000000000000)",
        ),
        (
            "z = n(1 + 2im, 30); [n(abs(z), 30), arg(z)]",
            "[2.2360679774997896964091736687312762, 1.107148717794090503017065460178537]",
        ),
        // real balls outside a real domain: the principal complex value (as for machine reals)
        ("log(-2.5~20)", "complex(0.9162907318741550652, 3.141592653589793238)"),
        ("(-2.0~30)^(1/3)", "complex(0.62996052494743658238360530364, 1.09112363597172140356007261419)"),
        ("2^(1.5~30)", "2.82842712474619009760337744842"),
        // machine numbers unchanged
        ("n(exp(1 + 2im))", "complex(-1.1312043837568135, 2.4717266720048188)"),
        ("n((-1)^(1/3))", "complex(0.5000000000000001, 0.8660254037844386)"),
        ("[is_real(log(-2)), is_real(exp(im)), is_real(abs(1 + im))]", "[false, false, true]"),
    ];
    for (src, want) in cases {
        assert_eq!(&e.eval(src), want, "{src}");
    }
    // on a cut with an inexact imaginary part: no value, the call stays
    let r = e.eval("y = n(1/3, 30) - n(1/3, 30); log(-2 + y*im)");
    assert!(r.starts_with("log(complex(-2."), "{r}");
    let r = e.eval("y = n(1/3, 30) - n(1/3, 30); sqrt(-2 + y*im)");
    assert!(r.starts_with("complex(-2.") && r.ends_with("^(1/2)"), "{r}");
    // a value whose imaginary part is only known to be tiny: d digits relative to |z|
    let r = e.eval("n(exp(1 + pi*im), 30)");
    assert!(r.starts_with("complex(-2.71828182845904523536028747135266, ball("), "{r}");
    // high precision
    let r = e.eval("n(exp(1 + 2im), 1000)");
    assert!(
        r.starts_with("complex(-1.13120438375681363843125525551079471062886799582652575021772191") && r.len() > 2000
    );
}

#[test]
fn a_raised_flag_stops_complex_computations() {
    let mut e = Engine::new();
    let (stop, ctl) = e.stopper();
    stop.store(true, Relaxed);
    let t0 = Instant::now();
    for f in ["exp", "log", "sin", "atan", "asin"] {
        // (other literals than below: a term evaluated under a raised flag keeps its failure in the session)
        let src = format!("n({f}(3 + 5im), 10000)");
        let r = e.eval(&src);
        assert!(r.starts_with("failure(interrupted, "), "{src}: {r}");
    }
    assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
    stop.store(false, Relaxed);
    // raised during a long computation, with the evaluator's interrupt as the engine does
    for src in ["n(atan(1 + 2im), 1000000)", "n(log(-2 + 1im), 1000000)", "n(exp(1 + 2im)^(1/3), 1000000)"] {
        let raiser = {
            let (stop, ctl) = (stop.clone(), ctl.clone());
            std::thread::spawn(move || {
                let t0 = Instant::now();
                while t0.elapsed() < Duration::from_millis(50) {
                    std::thread::yield_now();
                }
                stop.store(true, Relaxed);
                ctl.interrupt();
            })
        };
        let t0 = Instant::now();
        let r = e.eval(src);
        raiser.join().unwrap();
        assert!(r.starts_with("failure(interrupted"), "{src}: {}", &r[..r.len().min(80)]);
        assert!(t0.elapsed() < Duration::from_secs(5), "{src}: {:?}", t0.elapsed());
        stop.store(false, Relaxed);
        ctl.clear_interrupt();
    }
    // and nothing half-computed was cached
    assert_eq!(e.eval("n(exp(1 + 2im), 20)"), "complex(-1.131204383756813638431255, 2.47172667200481892761693)");
    // 10^4 digits in full
    let t0 = Instant::now();
    let r = e.eval("n(exp(1 + 2im), 10000)");
    assert!(r.starts_with("complex(-1.1312043837568136384312552555107947") && r.len() > 20000);
    assert!(t0.elapsed() < Duration::from_secs(5));
}

#[test]
fn expressions_in_variables_on_complex_balls() {
    use mc_arith::Arith;
    use mc_core::{Config, Heap, Store};
    use mc_numeric::numeval::{Graph, Names};
    let h = Heap::new(Config::default());
    let ar = Arith::new(&h);
    let nm = Names::new(&h);
    let x = h.sym("x");
    let xt = h.sym_term(x);
    // exp(x) sin(x) + x^(1/3) + abs(x) pi
    let e = h.call(h.sym("exp"), &[xt]);
    let s = h.call(h.sym("sin"), &[xt]);
    let third = ar.rational(h.int(1), h.int(3));
    let pi = h.sym_term(h.sym("pi"));
    let t = ar.add(&[ar.mul(&[e, s]), ar.pow(xt, third), ar.mul(&[h.call(h.sym("abs"), &[xt]), pi])]);
    let g = Graph::build_with_vars(&h, &ar, &nm, t, &[x]);
    let is = g.island(g.node(t).expect("a numeric expression in x"));
    for (re, im, rad) in [(0.7, -1.2, None), (-2.0, 0.5, Some(-30)), (3.0, 0.0, None)] {
        let p = first_precision(50);
        let r = rad.map_or(Mag::ZERO, Mag::pow2);
        let z = CBall::new(Ball { mid: float(re), rad: r }, Ball { mid: float(im), rad: r });
        let v = g.eval_complex_at(&is, p, std::slice::from_ref(&z)).expect("a value");
        for pt in points(&z) {
            let w = 3 * p;
            let direct = cball::exp(&pt, w)
                .unwrap()
                .mul(&cball::sin(&pt, w).unwrap(), w)
                .add(&cball::pow_rat(&pt, &Rat::new(Int::from_i64(1), Int::from_i64(3)), w).unwrap(), w)
                .add(&CBall::real(cball::abs(&pt, w).unwrap().mul(&Const::Pi.ball(w).unwrap(), w)), w);
            assert!(inside_c(&direct, &v), "at {pt:?}: {v:?} misses {direct:?}");
        }
        if rad.is_none() {
            assert!(v.has_digits(50));
        }
    }
    // without an argument for x there is no value; with the branch cut of x^(1/3) touched, neither
    assert!(g.eval_complex_at(&is, 64, &[]).is_none());
    let across = CBall::new(exact(-2.0), Ball { mid: Float::zero(), rad: Mag::pow2(-20) });
    assert!(g.eval_complex_at(&is, 64, &[across]).is_none());
}
