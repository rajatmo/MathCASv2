//! The error functions: machine values against 160-bit ball references at the exact binary arguments (max and mean
//! ulps per function over grids dense near 0 and in the tails), round trips of the machine inverses, published
//! values, identities at 50 digits, and ball enclosures at random exact rational arguments (50 and 300 digits) that
//! must contain the value at 2p + 64 bits with >= p - 4 correct bits.
// published values are quoted to more digits than a double holds
#![allow(clippy::excessive_precision)]
use mc_arith::num::{Ball, Int, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::machine::C64;
use mc_special::ball;
use mc_special::ball::erf as be;
use mc_special::machine::erf as me;
use std::time::Instant;

const P: u64 = 160;

/// A small deterministic generator (the tests are reproducible).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
    fn below(&mut self, n: i64) -> i64 {
        ((self.next() >> 33) % n as u64) as i64
    }
    /// A rational in [lo, hi] with denominator 97.
    fn point(&mut self, lo: i64, hi: i64) -> Rat {
        let k = lo * 97 + self.below((hi - lo) * 97 + 1);
        Rat::new(Int::from_i64(k), Int::from_i64(97))
    }
}

fn exact(v: f64) -> Real {
    Real::Ball(Ball::exact(ball::float_of(v)), P)
}

/// The distance from v to the next double away from zero (the subnormal spacing at the bottom).
fn spacing(v: f64) -> f64 {
    let a = v.abs();
    f64::from_bits(a.to_bits() + 1) - a
}

/// The error of a machine value in ulps of the ball value.
fn ulps(got: f64, r: &Ball) -> f64 {
    let want = ball::f64_of(&r.mid);
    if !want.is_finite() || !got.is_finite() {
        return if got == want { 0.0 } else { f64::INFINITY };
    }
    if want == 0.0 {
        return if got == 0.0 { 0.0 } else { f64::INFINITY };
    }
    let d = ball::float_of(got).sub_exact(&r.mid);
    ball::f64_of(&d).abs() / spacing(want)
}

struct Table {
    name: &'static str,
    worst: f64,
    at: String,
    count: usize,
    total: f64,
}

impl Table {
    fn new(name: &'static str) -> Table {
        Table { name, worst: 0.0, at: String::new(), count: 0, total: 0.0 }
    }
    fn add(&mut self, e: f64, at: String) {
        self.count += 1;
        self.total += e;
        if e > self.worst {
            self.worst = e;
            self.at = at;
        }
    }
    fn check(&mut self, got: f64, r: Option<CBall>, at: String) {
        match r {
            Some(r) => self.add(ulps(got, &r.re), at),
            None => panic!("{}: no reference at {at}, machine {got}", self.name),
        }
    }
    fn report(&self, bound: f64) {
        println!(
            "{:<14} points {:>5}  max {:>6.2} ulp  mean {:>5.2}  worst at {}",
            self.name,
            self.count,
            self.worst,
            self.total / self.count.max(1) as f64,
            self.at
        );
        assert!(self.worst <= bound, "{}: {} ulps at {}", self.name, self.worst, self.at);
    }
}

/// Arguments in [-hi, hi]: uniform, dense near 0 (log-spaced down to 1e-300) and near the ends.
fn grid(rng: &mut Lcg, n: usize, hi: f64) -> Vec<f64> {
    let mut v = Vec::new();
    for i in 0..n {
        let x = match i % 4 {
            0 => rng.range(-hi, hi),
            1 => rng.range(-3.0, 3.0),
            2 => 10f64.powf(rng.range(-300.0, 0.0)) * if rng.below(2) == 0 { 1.0 } else { -1.0 },
            _ => rng.range(hi * 0.7, hi) * if rng.below(2) == 0 { 1.0 } else { -1.0 },
        };
        v.push(x);
    }
    v
}

#[test]
fn erf_erfc_grid() {
    let mut rng = Lcg(7);
    let mut t = [Table::new("erf"), Table::new("erfc")];
    let xs = grid(&mut rng, 2400, 30.0);
    for (i, &x) in xs.iter().enumerate() {
        if i % 600 == 0 {
            println!("[{i}/{} {}%]", xs.len(), 100 * i / xs.len());
        }
        let ex = exact(x);
        t[0].check(me::erf(x), be::erf(&ex, P), format!("{x:e}"));
        t[1].check(me::erfc(x), be::erfc(&ex, P), format!("{x:e}"));
    }
    for t in &t {
        t.report(2.0);
    }
}

#[test]
fn erfc_subnormal_tail() {
    // the last decades before underflow: relative accuracy while normal, the subnormal spacing below
    let mut t = Table::new("erfc tail");
    let n = 400;
    for i in 0..=n {
        let x = 25.0 + 2.3 * i as f64 / n as f64;
        t.check(me::erfc(x), be::erfc(&exact(x), P), format!("{x:e}"));
    }
    t.report(2.0);
}

#[test]
fn erfi_grid() {
    let mut rng = Lcg(11);
    let mut t = Table::new("erfi");
    let xs = grid(&mut rng, 1600, 26.0);
    for (i, &x) in xs.iter().enumerate() {
        if i % 400 == 0 {
            println!("[{i}/{} {}%]", xs.len(), 100 * i / xs.len());
        }
        t.check(me::erfi(x), be::erfi(&exact(x), P), format!("{x:e}"));
    }
    t.report(2.0);
}

/// erf(x + iy) by the Maclaurin series in complex balls at 200 + 3 (x^2 + y^2) bits, the tail after the terms fall
/// below 2^-w bounded by twice the last term (the ratio is then below 1/2): the check of the complex machine path.
fn erf_complex_ref(x: f64, y: f64) -> (Ball, Ball) {
    let r2 = x * x + y * y;
    let w = 200 + (3.0 * r2) as u64;
    let z = CBall::new(Ball::exact(ball::float_of(x)), Ball::exact(ball::float_of(y)));
    let mz2 = z.square(w).neg();
    let mut t = z.clone();
    let mut s = z.clone();
    let mut k = 1i64;
    loop {
        t = t.mul(&mz2, w).mul_real(&Ball::from_ratio(1, k, w), w);
        let v = t.mul_real(&Ball::from_ratio(1, 2 * k + 1, w), w);
        s = s.add(&v, w);
        let size = ball::mag_up(&v.re).add(&ball::mag_up(&v.im));
        if k as f64 > 2.0 * r2 && size.log2() < -(w as f64) {
            let tail = size.mul_2exp(1);
            s = CBall::new(ball::widen(s.re, &tail), ball::widen(s.im, &tail));
            break;
        }
        k += 1;
    }
    let c = Ball::from_i64(2).div(&mc_numeric::elementary::sqrt(&ball::pi(w).unwrap(), w).unwrap(), w).unwrap();
    let r = s.mul_real(&c, w);
    (r.re, r.im)
}

#[test]
fn erf_complex_grid() {
    // the error relative to |erf(z)|, in units of 2^-52
    let mut rng = Lcg(29);
    let n = 1500;
    let (mut worst, mut total, mut at) = (0.0f64, 0.0, String::new());
    let mut comp = Table::new("erf_complex parts");
    let mut nan = 0;
    for i in 0..n {
        if i % 300 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let (x, y) = match i % 6 {
            0 | 1 => (rng.range(-6.0, 6.0), rng.range(-6.0, 6.0)),
            5 => {
                // near the first zero 1.4506161632... + 1.8805516357... i
                let d = 10f64.powf(rng.range(-9.0, -1.0));
                (1.450_616_163_223_4 + d * rng.range(-1.0, 1.0), 1.880_551_635_702_1 + d * rng.range(-1.0, 1.0))
            }
            2 => {
                let r = rng.range(6.0, 26.0);
                let t = rng.range(-std::f64::consts::PI, std::f64::consts::PI);
                (r * t.cos(), r * t.sin())
            }
            3 => (rng.range(-8.0, 8.0), 10f64.powf(rng.range(-30.0, 0.0))),
            _ => (10f64.powf(rng.range(-30.0, 0.0)), rng.range(-8.0, 8.0)),
        };
        let w = me::erf_complex(C64::new(x, y));
        let (rre, rim) = erf_complex_ref(x, y);
        let (fre, fim) = (ball::f64_of(&rre.mid), ball::f64_of(&rim.mid));
        if !fre.is_finite() || !fim.is_finite() || fre.abs().max(fim.abs()) > 1e300 {
            continue;
        }
        if w.re.is_nan() || w.im.is_nan() {
            println!("NaN at {x:e} + {y:e} i");
            nan += 1;
            continue;
        }
        let dre = ball::f64_of(&ball::float_of(w.re).sub_exact(&rre.mid));
        let dim = ball::f64_of(&ball::float_of(w.im).sub_exact(&rim.mid));
        let e = dre.hypot(dim) / fre.hypot(fim) / f64::EPSILON;
        // each part that is not tiny against |erf(z)| also to a few ulps of itself
        let norm = fre.hypot(fim);
        if fre.abs() >= norm * 1e-6 {
            comp.add(ulps(w.re, &rre), format!("{x:e} + {y:e} i (re)"));
        }
        if fim.abs() >= norm * 1e-6 {
            comp.add(ulps(w.im, &rim), format!("{x:e} + {y:e} i (im)"));
        }
        total += e;
        if e > worst {
            worst = e;
            at = format!("{x:e} + {y:e} i");
        }
    }
    println!(
        "erf_complex    points {n:>5}  max {worst:>6.2} eps  mean {:>5.2}  NaN {nan}  worst at {at}",
        total / n as f64
    );
    assert!(nan == 0 && worst <= 2.0, "erf_complex: {worst} at {at}");
    comp.report(16.0);
}

#[test]
fn inverse_grid() {
    let mut rng = Lcg(19);
    let mut ti = Table::new("inverse_erf");
    let mut tc = Table::new("inverse_erfc");
    let n = 1200;
    for i in 0..n {
        if i % 300 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let y = match i % 3 {
            0 => rng.range(-1.0, 1.0),
            1 => 10f64.powf(rng.range(-300.0, 0.0)) * if i % 2 == 0 { 1.0 } else { -1.0 },
            _ => (1.0 - 10f64.powf(rng.range(-16.0, -1.0))) * if i % 2 == 0 { 1.0 } else { -1.0 },
        };
        if y.abs() < 1.0 {
            ti.check(me::inverse_erf(y), be::inverse_erf(&exact(y), P), format!("{y:e}"));
        }
        let q = match i % 4 {
            0 => rng.range(0.0, 2.0),
            1 => 10f64.powf(rng.range(-300.0, 0.0)),
            2 => 2.0 - 10f64.powf(rng.range(-16.0, 0.0)),
            _ => 1.0 + rng.range(-1e-6, 1e-6),
        };
        if q > 0.0 && q < 2.0 && q != 1.0 {
            tc.check(me::inverse_erfc(q), be::inverse_erfc(&exact(q), P), format!("{q:e}"));
        }
    }
    // subnormal q
    for k in [1u64, 3, 1000, 1 << 40, 1 << 51] {
        let q = f64::from_bits(k);
        tc.check(me::inverse_erfc(q), be::inverse_erfc(&exact(q), P), format!("{q:e}"));
    }
    ti.report(2.0);
    tc.report(2.0);
}

#[test]
fn inverse_round_trips() {
    // erfc(inverse_erfc(q)) = q to the condition number |q'/q| |x| ulp(x)/|x| = 2|x| e^(-x^2)/(sqrt(pi) q) ulps
    let mut rng = Lcg(23);
    let mut worst: f64 = 0.0;
    for _ in 0..3000 {
        let q = 10f64.powf(rng.range(-307.0, 0.3));
        let x = me::inverse_erfc(q);
        let back = me::erfc(x);
        let cond = 2.0 * x.abs() * (-x * x).exp() / (std::f64::consts::PI.sqrt() * q);
        let err = ((back - q) / q).abs() / f64::EPSILON / (1.0 + cond);
        worst = worst.max(err);
        let y = rng.range(-1.0, 1.0);
        let x = me::inverse_erf(y);
        let cond = 2.0 * x.abs() * (-x * x).exp() / (std::f64::consts::PI.sqrt() * y.abs());
        let err = ((me::erf(x) - y) / y).abs() / f64::EPSILON / (1.0 + cond);
        worst = worst.max(err);
    }
    println!("round trips: worst {worst:.2} (ulps over 1 + condition)");
    assert!(worst <= 2.0);
}

#[test]
fn published_and_special_values() {
    let close = |got: f64, want: f64| ((got - want) / want).abs() <= 2.0 * f64::EPSILON;
    assert!(close(me::erf(1.0), 0.842_700_792_949_714_869_341_220_635_082_609_259_296));
    assert!(close(me::erfc(10.0), 2.088_487_583_762_544_757_000_786_294_957_788_611_560_818_119_321e-45));
    assert!(close(me::inverse_erf(0.5), 0.476_936_276_204_469_873_381_418_353_643_130_559_808_969_749_059_47));
    assert!(close(me::erfi(1.0), 1.650_425_758_797_542_876_025_337_729_561_362_443_895_679_874_874));
    assert!(me::inverse_erf(-1.0) == f64::NEG_INFINITY && me::inverse_erfc(0.0) == f64::INFINITY);
    assert!(me::inverse_erfc(2.0) == f64::NEG_INFINITY && me::inverse_erfc(-0.1).is_nan());
    assert!(me::erf(f64::NAN).is_nan() && me::erf(f64::INFINITY) == 1.0 && me::erfc(f64::NEG_INFINITY) == 2.0);
    assert_eq!(me::erfc(f64::INFINITY), 0.0);
    assert_eq!(me::erfi(-30.0), f64::NEG_INFINITY);
    assert_eq!(me::inverse_erf(-0.0).to_bits(), (-0.0f64).to_bits());
    let z = me::erf_complex(C64::new(0.5, 0.0));
    assert!(close(z.re, me::erf(0.5)) && z.im == 0.0);
    // ball values at 50 digits against the published digits
    let p = 170;
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    let digits = |b: Option<CBall>| {
        let b = b.unwrap().re;
        let (s, e) = b.mid.to_sci(50);
        (s, e)
    };
    assert_eq!(digits(be::erf(&q(1, 1), p)).0, "84270079294971486934122063508260925929606699796630");
    assert_eq!(digits(be::erfc(&q(10, 1), p)), ("20884875837625447570007862949577886115608181193212".into(), -45));
    assert_eq!(digits(be::inverse_erf(&q(1, 2), p)).0, "47693627620446987338141835364313055980896974905947");
}

#[test]
fn identities_at_50_digits() {
    let p = 170;
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    let near = |a: &Ball, b: &Ball| a.sub(b, p).rel_bits() < 0.0 || ball::log2_up(&a.sub(b, p)) < -(p as f64) + 8.0;
    for (n, d) in [(1, 3), (7, 2), (-5, 4), (31, 3)] {
        let x = q(n, d);
        let e = be::erf(&x, p).unwrap().re;
        let c = be::erfc(&x, p).unwrap().re;
        // erf + erfc = 1
        assert!(near(&e.add(&c, p), &Ball::from_i64(1)), "erf + erfc at {n}/{d}");
        // round trips of the inverses
        if n.abs() < 3 * d {
            let back = be::inverse_erf(&Real::Ball(e.clone(), p), p).unwrap().re;
            assert!(back.sub(&ball::rat_ball(&Rat::new(Int::from_i64(n), Int::from_i64(d)), p), p).contains_zero());
        }
        let back = be::inverse_erfc(&Real::Ball(c.clone(), p), p).unwrap().re;
        assert!(back.sub(&ball::rat_ball(&Rat::new(Int::from_i64(n), Int::from_i64(d)), p), p).contains_zero());
    }
    // the tail: inverse_erfc(10^-1000) and back
    let tiny = Real::Exact(Rat::new(Int::from_i64(1), Int::from_nat(mc_arith::num::Nat::pow(10, 1000))));
    let x = be::inverse_erfc(&tiny, p).unwrap().re;
    let back = be::erfc(&Real::Ball(x, p), p).unwrap().re;
    assert!(back
        .sub(&ball::rat_ball(&Rat::new(Int::from_i64(1), Int::from_nat(mc_arith::num::Nat::pow(10, 1000))), p), p)
        .contains_zero());
}

// ---------------------------------------------------------------- enclosures

/// Is ball `inner` inside ball `outer`?
fn inside(inner: &Ball, outer: &Ball) -> bool {
    let d = inner.mid.sub_exact(&outer.mid).abs();
    let need = d.add_exact(&inner.rad.to_float());
    need.cmp_exact(&outer.rad.to_float()) != std::cmp::Ordering::Greater
}

type BallFn = fn(&Real, u64) -> Option<CBall>;
type ArgGen = fn(&mut Lcg) -> Rat;

fn functions() -> Vec<(&'static str, BallFn, ArgGen)> {
    vec![
        ("erf", be::erf, |r| r.point(-8, 8)),
        ("erfc", be::erfc, |r| r.point(-6, 40)),
        ("erfi", be::erfi, |r| r.point(-10, 10)),
        ("inverse_erf", be::inverse_erf, |r| r.point(-1, 1)),
        ("inverse_erfc", be::inverse_erfc, |r| r.point(0, 2)),
    ]
}

fn check_at(digits: u64, per: usize) {
    let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64;
    let mut rng = Lcg(digits);
    let mut checked = 0;
    for (name, f, gen) in functions() {
        let start = Instant::now();
        let mut misses = 0;
        for _ in 0..per {
            let a = Real::Exact(gen(&mut rng));
            let (Some(r), Some(want)) = (f(&a, p), f(&a, 2 * p + 64)) else {
                misses += 1;
                continue;
            };
            assert!(inside(&want.re, &r.re), "{name}({a:?}) at {digits} digits");
            assert!(
                ball::accuracy(&r.re) >= p as f64 - 4.0 || r.re.mid.is_zero(),
                "{name}({a:?}): {} bits",
                ball::accuracy(&r.re)
            );
            checked += 1;
        }
        println!(
            "{name:<14} {digits} digits: {:.2} ms per value pair, {misses} without a value",
            start.elapsed().as_secs_f64() * 1e3 / per as f64
        );
        // the inverses have no value at the ends of their domains (y = ±1, q = 0, 2)
        assert!(misses * 10 <= per, "{name}: {misses} of {per} without a value");
    }
    println!("{digits} digits: {checked} enclosures checked");
}

#[test]
fn enclosures_at_50_digits() {
    check_at(50, 40);
}

#[test]
fn enclosures_at_300_digits() {
    check_at(300, if cfg!(debug_assertions) { 1 } else { 8 });
}

#[test]
fn ball_arguments() {
    // a ball argument's enclosure contains the values at its two ends and its midpoint
    let p = 200;
    let mut rng = Lcg(21);
    for (name, f, gen) in functions() {
        for _ in 0..6 {
            let a = gen(&mut rng);
            let rad = mc_arith::num::Mag::pow2(-40);
            let Some(r) = f(&Real::Ball(Ball { rad, ..ball::rat_ball(&a, p) }, p), p) else { continue };
            for corner in [-1i64, 0, 1] {
                let m = ball::rat_ball(&a, p);
                let e = Ball::exact(ball::float_of(corner as f64 * 2f64.powi(-40)));
                let pt = Real::Ball(Ball { mid: m.add(&e, p + 64).mid, rad: mc_arith::num::Mag::ZERO }, p);
                if let Some(v) = f(&pt, p) {
                    assert!(inside(&v.re, &r.re), "{name}({a:?}) corner {corner}");
                }
            }
        }
    }
}

#[test]
fn machine_timings() {
    // per-call times of the machine path (printed; the bound only catches gross regressions)
    let mut rng = Lcg(3);
    let xs: Vec<f64> = (0..20000).map(|_| rng.range(-6.0, 6.0)).collect();
    let us: Vec<f64> = (0..20000).map(|_| rng.unit()).collect();
    let time = |name: &str, args: &[f64], f: &dyn Fn(f64) -> f64| {
        let start = Instant::now();
        let mut acc = 0.0;
        for &a in args {
            acc += f(a);
        }
        let ns = start.elapsed().as_secs_f64() * 1e9 / args.len() as f64;
        println!("{name:<22} {ns:>7.1} ns per call ({acc:.3})");
        assert!(ns < 20000.0, "{name}: {ns} ns");
    };
    me::erf(0.3);
    let tail: Vec<f64> = xs.iter().map(|x| 3.0 + x.abs() * 3.0).collect();
    let q: Vec<f64> = us.iter().map(|u| 10f64.powf(-300.0 * u)).collect();
    let y: Vec<f64> = us.iter().map(|u| 2.0 * u - 1.0).collect();
    time("erf [-6, 6]", &xs, &me::erf);
    time("erfc [-6, 6]", &xs, &me::erfc);
    time("erfc [3, 21]", &tail, &me::erfc);
    time("erfi [-6, 6]", &xs, &me::erfi);
    time("inverse_erf (-1, 1)", &y, &me::inverse_erf);
    time("inverse_erfc 1e-300..1", &q, &me::inverse_erfc);
    time("inverse_erfc (0, 2)", &us.iter().map(|u| 2.0 * u).collect::<Vec<_>>(), &me::inverse_erfc);
    let im: Vec<f64> = us.iter().map(|u| 12.0 * u - 6.0).collect();
    let pairs: Vec<(f64, f64)> = xs.iter().copied().zip(im).collect();
    let start = Instant::now();
    let mut acc = 0.0;
    for &(x, y) in &pairs {
        acc += me::erf_complex(C64::new(x, y)).re.abs().min(1e300);
    }
    let ns = start.elapsed().as_secs_f64() * 1e9 / pairs.len() as f64;
    println!("{:<22} {ns:>7.1} ns per call ({acc:.3e})", "erf_complex [-6, 6]^2");
}

#[test]
fn ball_timings() {
    // one value per function at 50 and 300 digits (printed)
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    type Timed = (&'static str, Box<dyn Fn(u64) -> Option<CBall>>);
    let cases: Vec<Timed> = vec![
        ("erf", Box::new(move |p| be::erf(&q(7, 3), p))),
        ("erfc", Box::new(move |p| be::erfc(&q(7, 3), p))),
        ("erfc (x = 30)", Box::new(move |p| be::erfc(&q(30, 1), p))),
        ("erfi", Box::new(move |p| be::erfi(&q(7, 3), p))),
        ("inverse_erf", Box::new(move |p| be::inverse_erf(&q(2, 3), p))),
        ("inverse_erfc", Box::new(move |p| be::inverse_erfc(&q(1, 1000), p))),
    ];
    for (name, f) in &cases {
        let mut line = format!("{name:<16}");
        for digits in [50u64, 300] {
            let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64;
            let start = Instant::now();
            let reps = 5;
            for _ in 0..reps {
                assert!(f(p).is_some(), "{name}");
            }
            line += &format!("  {digits} digits {:>7.2} ms", start.elapsed().as_secs_f64() * 1e3 / reps as f64);
        }
        println!("{line}");
    }
}
