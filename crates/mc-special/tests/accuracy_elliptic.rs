//! Elliptic integrals and Jacobi functions: machine values against 160-bit ball references at the exact binary
//! arguments (max and mean ulps per function over the grids of the brief), and ball enclosures at random exact
//! rational arguments (50 and 300 digits) that must contain the value at 2p + 64 bits with >= p - 4 correct bits.
//!
//! The Jacobi functions are measured relative to max(|value|, 2^-20): near their zeros the phase error, which is
//! absolute (about |u| 2^-105 in double-double), is what a relative measure would magnify. Every other function is
//! measured relative to its value.
use mc_arith::num::{Ball, Int, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_special::ball;
use mc_special::ball::elliptic as be;
use mc_special::machine::elliptic as me;
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
    /// One measurement: the machine value against the reference (skipped when the reference has no value).
    fn check(&mut self, got: f64, r: Option<CBall>, floor: f64, at: String) {
        match r {
            Some(r) => self.add(ulps(got, &r.re, floor), at),
            None => {
                assert!(!got.is_finite() || got.abs() > 1e300, "{}: no reference at {at}, machine {got}", self.name)
            }
        }
    }
    fn report(&self, bound: f64) {
        println!(
            "{:<18} points {:>5}  max {:>6.2} ulp  mean {:>5.2}  worst at {}",
            self.name,
            self.count,
            self.worst,
            self.total / self.count.max(1) as f64,
            self.at
        );
        assert!(self.worst <= bound, "{}: {} ulps at {}", self.name, self.worst, self.at);
    }
}

const JACOBI_FLOOR: f64 = 1.0 / 1048576.0;

#[test]
fn carlson_grid() {
    let mut rng = Lcg(17);
    let mut t =
        [Table::new("carlson_rf"), Table::new("carlson_rd"), Table::new("carlson_rj"), Table::new("carlson_rc")];
    let n = 400;
    for i in 0..n {
        if i % 100 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let mut v = || 10f64.powf(rng.range(-3.0, 3.0));
        let (mut x, y, z, pp) = (v(), v(), v(), v());
        if i % 7 == 0 {
            x = 0.0;
        }
        let pn = if i % 3 == 0 { -pp } else { pp };
        let at = format!("{x}, {y}, {z}, {pn}");
        t[0].check(me::carlson_rf(x, y, z), be::carlson_rf(&exact(x), &exact(y), &exact(z), P), 0.0, at.clone());
        t[1].check(me::carlson_rd(x, y, z), be::carlson_rd(&exact(x), &exact(y), &exact(z), P), 0.0, at.clone());
        let r = be::carlson_rj(&exact(x), &exact(y), &exact(z), &exact(pn), P);
        t[2].check(me::carlson_rj(x, y, z, pn), r, 0.0, at.clone());
        let yc = if i % 2 == 0 { -y } else { y };
        t[3].check(me::carlson_rc(x, yc), be::carlson_rc(&exact(x), &exact(yc), P), 0.0, format!("{x}, {yc}"));
    }
    for t in &t {
        t.report(4.0);
    }
}

#[test]
fn complete_grid() {
    let mut rng = Lcg(5);
    let mut tk = Table::new("elliptic_k");
    let mut te = Table::new("elliptic_e");
    let mut tp = Table::new("elliptic_pi");
    let n = 600;
    for i in 0..n {
        if i % 150 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let m = if i % 10 == 0 { 1.0 - 2f64.powi(-(rng.below(52) as i32) - 1) } else { rng.range(-10.0, 1.0) };
        tk.check(me::elliptic_k(m), be::elliptic_k(&exact(m), P), 0.0, format!("{m}"));
        te.check(me::elliptic_e(m), be::elliptic_e(&exact(m), P), 0.0, format!("{m}"));
        // n in [-10, 1) and, one point in four, the principal value for n in (1, 10]
        let nn = if i % 4 == 0 { rng.range(1.0, 10.0) } else { rng.range(-10.0, 1.0) };
        tp.check(me::elliptic_pi(nn, m), be::elliptic_pi(&exact(nn), &exact(m), P), 0.0, format!("{nn}, {m}"));
    }
    tk.report(4.0);
    te.report(4.0);
    tp.report(4.0);
}

#[test]
fn incomplete_grid() {
    let mut rng = Lcg(9);
    let mut tf = Table::new("elliptic_f");
    let mut te = Table::new("elliptic_e_inc");
    let mut tp = Table::new("elliptic_pi_inc");
    let mut ti = Table::new("inverse_jacobi_sn");
    let n = 600;
    for i in 0..n {
        if i % 150 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let m = rng.range(-10.0, 1.0);
        let phi = rng.range(-10.0, 10.0);
        // m > 1 with m sin^2 phi < 1 now and then
        let (phi, m) = if i % 8 == 0 {
            let m2 = rng.range(1.0, 10.0);
            (rng.range(-1.0, 1.0) * (1.0 / m2.sqrt()).asin(), m2)
        } else {
            (phi, m)
        };
        let at = format!("{phi}, {m}");
        tf.check(me::elliptic_f(phi, m), be::elliptic_f(&exact(phi), &exact(m), P), 0.0, at.clone());
        te.check(me::elliptic_e_inc(phi, m), be::elliptic_e_inc(&exact(phi), &exact(m), P), 0.0, at.clone());
        let nn = if i % 5 == 0 { rng.range(1.0, 10.0) } else { rng.range(-10.0, 1.0) };
        let r = be::elliptic_pi_inc(&exact(nn), &exact(phi), &exact(m), P);
        tp.check(me::elliptic_pi_inc(nn, phi, m), r, 0.0, format!("{nn}, {phi}, {m}"));
        let v = rng.range(-1.0, 1.0);
        let mv = if i % 8 == 0 { m.min(1.0 / (v * v)) } else { m.min(1.0) };
        let r = be::inverse_jacobi_sn(&exact(v), &exact(mv), P);
        ti.check(me::inverse_jacobi_sn(v, mv), r, 0.0, format!("{v}, {mv}"));
    }
    tf.report(4.0);
    te.report(4.0);
    tp.report(4.0);
    ti.report(4.0);
}

#[test]
fn jacobi_grid() {
    let mut rng = Lcg(13);
    let mut ts = Table::new("jacobi_sn");
    let mut tc = Table::new("jacobi_cn");
    let mut td = Table::new("jacobi_dn");
    let n = 500;
    for i in 0..n {
        if i % 100 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let u = rng.range(-20.0, 20.0);
        let m = match i % 10 {
            0 => 1.0 - 2f64.powi(-(rng.below(40) as i32) - 1),
            1 => rng.range(1.0, 10.0),
            2 => 1.0,
            _ => rng.range(-10.0, 1.0),
        };
        let at = format!("{u}, {m}");
        let (eu, em) = (exact(u), exact(m));
        ts.check(me::jacobi_sn(u, m), be::jacobi_sn(&eu, &em, P), JACOBI_FLOOR, at.clone());
        tc.check(me::jacobi_cn(u, m), be::jacobi_cn(&eu, &em, P), JACOBI_FLOOR, at.clone());
        td.check(me::jacobi_dn(u, m), be::jacobi_dn(&eu, &em, P), JACOBI_FLOOR, at);
    }
    ts.report(4.0);
    tc.report(4.0);
    td.report(4.0);
}

// ---------------------------------------------------------------- enclosures

/// Is ball `inner` inside ball `outer`?
fn inside(inner: &Ball, outer: &Ball) -> bool {
    let d = inner.mid.sub_exact(&outer.mid).abs();
    let need = d.add_exact(&inner.rad.to_float());
    need.cmp_exact(&outer.rad.to_float()) != std::cmp::Ordering::Greater
}

type BallFn = fn(&[Real], u64) -> Option<CBall>;
type ArgGen = fn(&mut Lcg) -> Vec<Rat>;
type Case = (&'static str, BallFn, ArgGen);

/// The functions with argument generators (rationals with denominator 97) inside their real domains.
fn functions() -> Vec<Case> {
    vec![
        ("elliptic_k", |a, p| be::elliptic_k(&a[0], p), |r| vec![r.point(-10, 0)]),
        ("elliptic_e", |a, p| be::elliptic_e(&a[0], p), |r| vec![r.point(-10, 1)]),
        ("elliptic_f", |a, p| be::elliptic_f(&a[0], &a[1], p), |r| vec![r.point(-10, 10), r.point(-10, 1)]),
        ("elliptic_e_inc", |a, p| be::elliptic_e_inc(&a[0], &a[1], p), |r| vec![r.point(-10, 10), r.point(-10, 1)]),
        ("elliptic_pi", |a, p| be::elliptic_pi(&a[0], &a[1], p), |r| vec![r.point(-10, 10), r.point(-10, 0)]),
        (
            "elliptic_pi_inc",
            |a, p| be::elliptic_pi_inc(&a[0], &a[1], &a[2], p),
            |r| vec![r.point(-10, 10), r.point(-10, 10), r.point(-10, 0)],
        ),
        ("jacobi_sn", |a, p| be::jacobi_sn(&a[0], &a[1], p), |r| vec![r.point(-20, 20), r.point(-10, 10)]),
        ("jacobi_cn", |a, p| be::jacobi_cn(&a[0], &a[1], p), |r| vec![r.point(-20, 20), r.point(-10, 10)]),
        ("jacobi_dn", |a, p| be::jacobi_dn(&a[0], &a[1], p), |r| vec![r.point(-20, 20), r.point(-10, 10)]),
        ("inverse_jacobi_sn", |a, p| be::inverse_jacobi_sn(&a[0], &a[1], p), |r| vec![r.point(-1, 1), r.point(-10, 1)]),
        (
            "carlson_rj",
            |a, p| be::carlson_rj(&a[0], &a[1], &a[2], &a[3], p),
            |r| vec![r.point(0, 10), r.point(0, 10), r.point(0, 10), r.point(-10, 10)],
        ),
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
            let a: Vec<Real> = gen(&mut rng).into_iter().map(Real::Exact).collect();
            let (Some(r), Some(want)) = (f(&a, p), f(&a, 2 * p + 64)) else {
                // a pole, or a point outside the real domain (n sin^2 phi = 1 cannot happen at rationals)
                misses += 1;
                continue;
            };
            assert!(inside(&want.re, &r.re), "{name}({a:?}) at {digits} digits");
            assert!(
                ball::accuracy(&r.re) >= p as f64 - 4.0 || r.re.contains_zero(),
                "{name}({a:?}): {} bits",
                ball::accuracy(&r.re)
            );
            checked += 1;
        }
        println!(
            "{name:<18} {digits} digits: {:.2} ms per value pair, {misses} without a value",
            start.elapsed().as_secs_f64() * 1e3 / per as f64
        );
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
        for _ in 0..4 {
            let a = gen(&mut rng);
            let rad = mc_arith::num::Mag::pow2(-40);
            let balls: Vec<Real> = a.iter().map(|q| Real::Ball(Ball { rad, ..ball::rat_ball(q, p) }, p)).collect();
            let Some(r) = f(&balls, p) else { continue };
            for corner in [-1i64, 0, 1] {
                let pts: Vec<Real> = a
                    .iter()
                    .map(|q| {
                        let m = ball::rat_ball(q, p);
                        let e = Ball::exact(ball::float_of(corner as f64 * 2f64.powi(-40)));
                        Real::Ball(Ball { mid: m.add(&e, p + 64).mid, rad: mc_arith::num::Mag::ZERO }, p)
                    })
                    .collect();
                if let Some(v) = f(&pts, p) {
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
    let xs: Vec<(f64, f64, f64)> =
        (0..2000).map(|_| (rng.range(-10.0, 10.0), rng.range(-1.0, 0.95), rng.range(-5.0, 0.9))).collect();
    let time = |name: &str, f: &dyn Fn(f64, f64, f64) -> f64| {
        let start = Instant::now();
        let mut acc = 0.0;
        for &(a, b, c) in &xs {
            acc += f(a, b, c);
        }
        let us = start.elapsed().as_secs_f64() * 1e6 / xs.len() as f64;
        println!("{name:<18} {us:>6.2} us per call ({acc:.3})");
        assert!(us < 200.0, "{name}: {us} us");
    };
    time("carlson_rf", &|a, b, c| me::carlson_rf(a.abs(), b.abs(), c.abs()));
    time("carlson_rj", &|a, b, c| me::carlson_rj(a.abs(), b.abs(), c.abs(), 1.5));
    time("elliptic_k", &|_, b, _| me::elliptic_k(b));
    time("elliptic_e", &|_, b, _| me::elliptic_e(b));
    time("elliptic_f", &|a, b, _| me::elliptic_f(a, b));
    time("elliptic_e_inc", &|a, b, _| me::elliptic_e_inc(a, b));
    time("elliptic_pi", &|_, b, c| me::elliptic_pi(c, b));
    time("elliptic_pi_inc", &|a, b, c| me::elliptic_pi_inc(c, a, b));
    time("jacobi_sn", &|a, b, _| me::jacobi_sn(a, b));
    time("inverse_jacobi_sn", &|_, b, c| me::inverse_jacobi_sn(b, c));
}

#[test]
fn ball_timings() {
    // one value per function at 50 and 300 digits (printed)
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    type Timed = (&'static str, Box<dyn Fn(u64) -> Option<CBall>>);
    let cases: Vec<Timed> = vec![
        ("elliptic_k", Box::new(move |p| be::elliptic_k(&q(1, 3), p))),
        ("elliptic_e", Box::new(move |p| be::elliptic_e(&q(1, 3), p))),
        ("elliptic_f", Box::new(move |p| be::elliptic_f(&q(7, 2), &q(1, 3), p))),
        ("elliptic_e_inc", Box::new(move |p| be::elliptic_e_inc(&q(7, 2), &q(1, 3), p))),
        ("elliptic_pi", Box::new(move |p| be::elliptic_pi(&q(-2, 1), &q(1, 3), p))),
        ("elliptic_pi_inc", Box::new(move |p| be::elliptic_pi_inc(&q(-2, 1), &q(7, 2), &q(1, 3), p))),
        ("jacobi_sn", Box::new(move |p| be::jacobi_sn(&q(5, 2), &q(7, 10), p))),
        ("inverse_jacobi_sn", Box::new(move |p| be::inverse_jacobi_sn(&q(3, 5), &q(7, 10), p))),
    ];
    for (name, f) in &cases {
        let mut line = format!("{name:<18}");
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
