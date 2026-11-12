//! The polylogarithm: machine values against 160-bit ball references at the exact binary arguments (max and mean
//! ulps per order over z in [-10, 1]), and ball enclosures at random exact rational arguments (50 and 300 digits)
//! that must contain the value at 2p + 64 bits with >= p - 4 correct bits.
//!
//! Li_-n near the zeros of its Eulerian polynomial (and Li_s for s <= 0 near its real zeros) is measured relative
//! to the sum of the absolute values of the terms of the polynomial, z A_n(|z|)/(1-z)^(n+1) (the absolute
//! accuracy the double-double evaluation guarantees); every other value relative to itself.
use mc_arith::num::{Ball, Int, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_special::ball;
use mc_special::ball::polylog as bp;
use mc_special::machine::polylog as mp;
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
    /// A rational in [lo, hi) with denominator 97.
    fn point(&mut self, lo: i64, hi: i64) -> Rat {
        let k = lo * 97 + self.below((hi - lo) * 97);
        Rat::new(Int::from_i64(k), Int::from_i64(97))
    }
}

fn exact(v: f64) -> Real {
    Real::Ball(Ball::exact(ball::float_of(v)), P)
}

/// The distance from v to the next double away from zero.
fn spacing(v: f64) -> f64 {
    let a = v.abs();
    f64::from_bits(a.to_bits() + 1) - a
}

/// The error of a machine value in ulps of max(|reference|, floor).
fn ulps(got: f64, r: &Ball, floor: f64) -> f64 {
    let want = ball::f64_of(&r.mid);
    if !want.is_finite() || !got.is_finite() {
        return if got == want { 0.0 } else { f64::INFINITY };
    }
    if want == 0.0 && floor == 0.0 {
        return if got == 0.0 { 0.0 } else { f64::INFINITY };
    }
    let d = ball::float_of(got).sub_exact(&r.mid);
    ball::f64_of(&d).abs() / spacing(want.abs().max(floor))
}

struct Table {
    name: String,
    worst: f64,
    at: String,
    count: usize,
    total: f64,
}

impl Table {
    fn new(name: String) -> Table {
        Table { name, worst: 0.0, at: String::new(), count: 0, total: 0.0 }
    }
    fn check(&mut self, got: f64, r: Option<CBall>, floor: f64, at: String) {
        match r {
            Some(r) => {
                let e = ulps(got, &r.re, floor);
                self.count += 1;
                self.total += e;
                if e > self.worst {
                    self.worst = e;
                    self.at = at;
                }
            }
            None => assert!(!got.is_finite(), "{}: no reference at {at}, machine {got}", self.name),
        }
    }
    fn report(&self, bound: f64) {
        println!(
            "{:<16} points {:>5}  max {:>6.2} ulp  mean {:>5.2}  worst at {}",
            self.name,
            self.count,
            self.worst,
            self.total / self.count.max(1) as f64,
            self.at
        );
        assert!(self.worst <= bound, "{}: {} ulps at {}", self.name, self.worst, self.at);
    }
}

/// The size of the terms of Li_-n(z) = z A_n(z)/(1-z)^(n+1): |z| A_n(|z|)/|1-z|^(n+1) (n = -s >= 0).
fn eulerian_scale(n: u64, z: f64) -> f64 {
    let mut row = vec![1.0f64];
    for m in 2..=n as usize {
        let mut next = vec![0.0; m];
        for (k, r) in next.iter_mut().enumerate() {
            let a = if k < m - 1 { row[k] * (k + 1) as f64 } else { 0.0 };
            let b = if k > 0 { row[k - 1] * (m - k) as f64 } else { 0.0 };
            *r = a + b;
        }
        row = next;
    }
    let za = z.abs();
    let p = if n == 0 { 1.0 } else { row.iter().rev().fold(0.0, |acc, c| acc * za + c) };
    za * p / (1.0 - z).abs().powi(n as i32 + 1)
}

#[test]
fn polylog_grid() {
    let orders: [f64; 10] = [-3.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 2.5, 0.5, -1.5];
    let mut rng = Lcg(11);
    let n = 300;
    for (i, &s) in orders.iter().enumerate() {
        println!("[{i}/{} {}%]", orders.len(), 100 * i / orders.len());
        let mut t = Table::new(format!("polylog s={s}"));
        for j in 0..n {
            let z = match j % 10 {
                0 => 1.0 - 2f64.powi(-(rng.below(50) as i32) - 1),
                1 if j % 20 == 1 => -1.0,
                1 => -1.0 + rng.range(-1e-3, 1e-3),
                2 => rng.range(-1e-3, 1e-3) * 2f64.powi(-(rng.below(40) as i32)),
                3 => 1.0,
                _ => rng.range(-10.0, 1.0),
            };
            // for s <= 0 integers: relative to the size of the terms (equal to |value| away from the zeros)
            let floor = if s <= 0.0 && s == s.floor() { eulerian_scale(-s as u64, z) } else { 0.0 };
            let r = bp::polylog(&exact(s), &exact(z), P);
            t.check(mp::polylog(s, z), r, floor, format!("{z}"));
        }
        t.report(4.0);
    }
}

#[test]
fn polylog_large_orders() {
    // s in [-20, 60], z in [-10, 1)
    let mut rng = Lcg(23);
    let mut t = Table::new("polylog s any".into());
    let n = 400;
    for j in 0..n {
        if j % 100 == 0 {
            println!("[{j}/{n} {}%]", 100 * j / n);
        }
        let s = if j % 3 == 0 { rng.below(60) as f64 - 10.0 } else { rng.range(-10.0, 60.0) };
        let z = rng.range(-10.0, 1.0);
        if s <= 0.0 && s == s.floor() {
            continue;
        }
        let r = bp::polylog(&exact(s), &exact(z), P);
        t.check(mp::polylog(s, z), r, 0.0, format!("{s}, {z}"));
    }
    t.report(8.0);
}

// ---------------------------------------------------------------- enclosures

/// Is ball `inner` inside ball `outer`?
fn inside(inner: &Ball, outer: &Ball) -> bool {
    let d = inner.mid.sub_exact(&outer.mid).abs();
    let need = d.add_exact(&inner.rad.to_float());
    need.cmp_exact(&outer.rad.to_float()) != std::cmp::Ordering::Greater
}

fn check_at(digits: u64, per: usize) {
    let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64;
    let mut rng = Lcg(digits);
    let orders = [(-3, 1), (0, 1), (1, 1), (2, 1), (3, 1), (5, 2), (1, 2), (-3, 2), (7, 1), (10, 3)];
    let start = Instant::now();
    let mut checked = 0;
    for k in 0..per {
        let (sn, sd) = orders[k % orders.len()];
        let s = Real::Exact(Rat::new(Int::from_i64(sn), Int::from_i64(sd)));
        let z = Real::Exact(rng.point(-10, 1));
        let (Some(r), Some(want)) = (bp::polylog(&s, &z, p), bp::polylog(&s, &z, 2 * p + 64)) else {
            panic!("no value at {s:?}, {z:?}");
        };
        assert!(inside(&want.re, &r.re), "polylog({s:?}, {z:?}) at {digits} digits");
        assert!(ball::accuracy(&r.re) >= p as f64 - 4.0 || r.re.contains_zero(), "polylog({s:?}, {z:?})");
        checked += 1;
    }
    println!(
        "polylog {digits} digits: {checked} enclosures, {:.2} ms per value pair",
        start.elapsed().as_secs_f64() * 1e3 / per as f64
    );
}

#[test]
fn enclosures_at_50_digits() {
    check_at(50, 60);
}

#[test]
fn enclosures_at_300_digits() {
    check_at(300, if cfg!(debug_assertions) { 2 } else { 20 });
}

#[test]
fn ball_arguments() {
    // a ball argument's enclosure contains the values at its two ends and its midpoint
    let p = 200;
    let mut rng = Lcg(21);
    for s in [(2i64, 1i64), (5, 2), (-1, 1), (1, 2)] {
        for _ in 0..6 {
            let z = rng.point(-5, 1);
            let rad = mc_arith::num::Mag::pow2(-40);
            let sq = Rat::new(Int::from_i64(s.0), Int::from_i64(s.1));
            let sr = Real::Exact(sq);
            let zb = Real::Ball(Ball { rad, ..ball::rat_ball(&z, p) }, p);
            let Some(r) = bp::polylog(&sr, &zb, p) else { continue };
            for corner in [-1i64, 0, 1] {
                let m = ball::rat_ball(&z, p);
                let e = Ball::exact(ball::float_of(corner as f64 * 2f64.powi(-40)));
                let zc = Real::Ball(Ball { mid: m.add(&e, p + 64).mid, rad: mc_arith::num::Mag::ZERO }, p);
                if let Some(v) = bp::polylog(&sr, &zc, p) {
                    assert!(inside(&v.re, &r.re), "polylog({s:?}, {z:?}) corner {corner}");
                }
            }
        }
    }
}

#[test]
fn identities_at_60_digits() {
    let p = 200;
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    let close = |a: &Ball, b: &Ball| {
        let d = a.sub(b, p);
        d.contains_zero() || ball::f64_of(&d.mid).abs() < 1e-58
    };
    // Li_3(1/2) = 7/8 zeta(3) - pi^2/12 log 2 + log(2)^3/6
    let l = bp::polylog(&q(3, 1), &q(1, 2), p).unwrap().re;
    let z3 = mc_special::ball::zeta::zeta(&q(3, 1), p).unwrap().re;
    let pi = ball::pi(p).unwrap();
    let ln2 = mc_numeric::elementary::log(&Ball::from_i64(2), p).unwrap();
    let want = z3
        .mul(&Ball::from_ratio(7, 8, p), p)
        .sub(&pi.mul(&pi, p).mul(&ln2, p).div(&Ball::from_i64(12), p).unwrap(), p)
        .add(&ln2.mul(&ln2, p).mul(&ln2, p).div(&Ball::from_i64(6), p).unwrap(), p);
    assert!(close(&l, &want));
    // Li_s(1) = zeta(s), s = 5/2
    let l = bp::polylog(&q(5, 2), &q(1, 1), p).unwrap().re;
    assert!(close(&l, &mc_special::ball::zeta::zeta(&q(5, 2), p).unwrap().re));
    // the duplication Li_s(z) + Li_s(-z) = 2^(1-s) Li_s(z^2) at s = 5/2, z = 7/10 and s = -3/2, z = 9/10
    for (s, z) in [((5, 2), (7, 10)), ((-3, 2), (9, 10)), ((1, 3), (19, 20))] {
        let (sr, zr) = (q(s.0, s.1), q(z.0, z.1));
        let a = bp::polylog(&sr, &zr, p).unwrap().re;
        let b = bp::polylog(&sr, &q(-z.0, z.1), p).unwrap().re;
        let c = bp::polylog(&sr, &q(z.0 * z.0, z.1 * z.1), p).unwrap().re;
        let two =
            mc_numeric::elementary::exp(&ln2.mul(&Ball::from_i64(1).sub(&Ball::from_ratio(s.0, s.1, p), p), p), p)
                .unwrap();
        assert!(close(&a.add(&b, p), &two.mul(&c, p)), "duplication at {s:?}, {z:?}");
    }
    // Li_2(z) + Li_2(1 - z) = pi^2/6 - log z log(1 - z) at z = 1/3
    let a = bp::polylog(&q(2, 1), &q(1, 3), p).unwrap().re;
    let b = bp::polylog(&q(2, 1), &q(2, 3), p).unwrap().re;
    let lz = mc_numeric::elementary::log(&Ball::from_ratio(1, 3, p), p).unwrap();
    let lw = mc_numeric::elementary::log(&Ball::from_ratio(2, 3, p), p).unwrap();
    let want = pi.mul(&pi, p).div(&Ball::from_i64(6), p).unwrap().sub(&lz.mul(&lw, p), p);
    assert!(close(&a.add(&b, p), &want));
}

#[test]
fn machine_timings() {
    let mut rng = Lcg(3);
    let zs: Vec<f64> = (0..2000).map(|_| rng.range(-10.0, 1.0)).collect();
    for s in [2.0, 3.0, -2.0, 2.5, 0.5] {
        let start = Instant::now();
        let mut acc = 0.0;
        for &z in &zs {
            acc += mp::polylog(s, z);
        }
        let us = start.elapsed().as_secs_f64() * 1e6 / zs.len() as f64;
        println!("polylog s={s:<5} {us:>7.2} us per call ({acc:.3})");
        assert!(us < 1000.0);
    }
}

#[test]
fn ball_timings() {
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    for (s, z) in [((2, 1), (3, 4)), ((3, 1), (-7, 2)), ((5, 2), (3, 4)), ((1, 2), (-7, 2)), ((5, 2), (1, 3))] {
        let mut line = format!("polylog({}/{}, {}/{})", s.0, s.1, z.0, z.1);
        for digits in [50u64, 300] {
            let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64;
            let start = Instant::now();
            assert!(bp::polylog(&q(s.0, s.1), &q(z.0, z.1), p).is_some());
            line += &format!("  {digits} digits {:>8.2} ms", start.elapsed().as_secs_f64() * 1e3);
        }
        println!("{line}");
    }
}
