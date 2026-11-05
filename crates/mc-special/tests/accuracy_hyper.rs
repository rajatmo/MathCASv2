//! Hypergeometric functions: machine values against 160-bit ball references at the exact binary arguments (max
//! and mean ulps per table), and ball enclosures at random exact rational arguments (50 and 300 digits) that must
//! contain the value at 2p + 64 bits with >= p - 4 correct bits.
//!
//! The "positive" tables (parameters where the series terms do not cancel badly) are measured relative to the
//! value. The "mixed" tables (parameters of any sign in [-5, 5]) are measured relative to
//! max(|value|, S 2^-50), S the sum of the absolute values of the terms of the series the machine algorithm sums
//! (after its transformation): near the real zeros and for alternating Laguerre-type sums the problem is
//! ill-conditioned, and S 2^-106 is the absolute accuracy double-double guarantees there.
use mc_arith::num::{Ball, Int, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_special::ball;
use mc_special::ball::hyper as bh;
use mc_special::machine::hyper as mh;
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
    /// A rational in [lo, hi) with denominator 97, not an integer (integers b are poles, and |z| = 1 for 3F2 has
    /// no value).
    fn point(&mut self, lo: i64, hi: i64) -> Rat {
        let mut k = lo * 97 + self.below((hi - lo) * 97);
        if k % 97 == 0 {
            k += 1;
        }
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
            "{:<18} points {:>5}  max {:>8.2} ulp  mean {:>6.2}  worst at {}",
            self.name,
            self.count,
            self.worst,
            self.total / self.count.max(1) as f64,
            self.at
        );
        assert!(self.worst <= bound, "{}: {} ulps at {}", self.name, self.worst, self.at);
    }
}

/// sum_k |t_k| of the series prod (a)_k / prod (b)_k x^k / k! (finite sums stop at the zero term).
fn abs_sum(a: &[f64], b: &[f64], x: f64) -> f64 {
    let (mut t, mut s) = (1.0f64, 1.0f64);
    for k in 0..200_000 {
        let kf = k as f64;
        let mut r = x.abs() / (kf + 1.0);
        for &ai in a {
            r *= (ai + kf).abs();
        }
        for &bj in b {
            r /= (bj + kf).abs();
        }
        t *= r;
        s += t;
        if t == 0.0 || (t < 1e-20 * s && kf > 50.0) || !s.is_finite() {
            break;
        }
    }
    s
}

/// The floor of the mixed 2F1 table: the absolute sum of the series the machine evaluates.
fn floor_2f1(a: f64, b: f64, c: f64, z: f64) -> f64 {
    if z.abs() <= 0.5 {
        abs_sum(&[a, b], &[c], z)
    } else if z < 0.0 {
        let w = z / (z - 1.0);
        let pre = (1.0 - z).powf(-a);
        if w <= 0.5 {
            pre * abs_sum(&[a, c - b], &[c], w)
        } else {
            0.0
        }
    } else {
        0.0
    }
}

/// The floor of the mixed 1F1 table.
fn floor_1f1(a: f64, b: f64, z: f64) -> f64 {
    if z < 0.0 && !(a <= 0.0 && a == a.floor()) {
        z.exp() * abs_sum(&[b - a], &[b], -z)
    } else {
        abs_sum(&[a], &[b], z)
    }
}

fn param(rng: &mut Lcg, lo: f64, hi: f64) -> f64 {
    match rng.below(8) {
        0 => rng.below((hi - lo) as i64 + 1) as f64 + lo,
        1 => (rng.below(2 * (hi - lo) as i64 + 1) as f64 + 2.0 * lo) / 2.0,
        _ => rng.range(lo, hi),
    }
}

const BIG: f64 = 1.0 / 1125899906842624.0; // 2^-50

#[test]
fn hyp_2f1_grid() {
    let mut rng = Lcg(31);
    let mut tp = Table::new("2f1 positive");
    let mut tm = Table::new("2f1 mixed");
    let mut td = Table::new("2f1 degenerate");
    let n = 900;
    for i in 0..n {
        if i % 150 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let z = match i % 6 {
            0 => rng.range(0.5, 1.0),
            1 => rng.range(-100.0, -1.0),
            _ => rng.range(-10.0, 1.0),
        };
        let (a, b, c) = (rng.range(0.0, 5.0), rng.range(0.0, 5.0), rng.range(0.1, 5.0));
        let r = bh::hyp_2f1(&exact(a), &exact(b), &exact(c), &exact(z), P);
        tp.check(mh::hyp_2f1(a, b, c, z), r, 0.0, format!("{a}, {b}, {c}, {z}"));
        let (a, b, c) = (param(&mut rng, -5.0, 5.0), param(&mut rng, -5.0, 5.0), param(&mut rng, -5.0, 5.0));
        let r = bh::hyp_2f1(&exact(a), &exact(b), &exact(c), &exact(z), P);
        let fl = floor_2f1(a, b, c, z) * BIG;
        tm.check(mh::hyp_2f1(a, b, c, z), r, fl, format!("{a}, {b}, {c}, {z}"));
        // c - a - b (z > 1/2) or b - a (z < -1) an integer
        let (a, b) = (rng.range(0.0, 4.0), rng.range(0.0, 4.0));
        let m = rng.below(5) as f64 - 2.0;
        let (b, c) = if z < -1.0 { (a + m, rng.range(0.5, 5.0)) } else { (b, a + b + m) };
        if c > 0.0 {
            let r = bh::hyp_2f1(&exact(a), &exact(b), &exact(c), &exact(z), P);
            td.check(mh::hyp_2f1(a, b, c, z), r, 0.0, format!("{a}, {b}, {c}, {z}"));
        }
    }
    tp.report(4.0);
    tm.report(4.0);
    td.report(8.0);
}

#[test]
fn hyp_1f1_grid() {
    let mut rng = Lcg(37);
    let mut tp = Table::new("1f1 positive");
    let mut tm = Table::new("1f1 mixed");
    let n = 900;
    for i in 0..n {
        if i % 150 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let z = match i % 5 {
            0 => rng.range(-300.0, 300.0),
            1 => rng.range(-1.0, 1.0),
            _ => rng.range(-30.0, 30.0),
        };
        let (a, b) = (rng.range(0.0, 5.0), rng.range(0.1, 5.0));
        let r = bh::hyp_1f1(&exact(a), &exact(b), &exact(z), P);
        tp.check(mh::hyp_1f1(a, b, z), r, 0.0, format!("{a}, {b}, {z}"));
        let (a, b) = (param(&mut rng, -5.0, 5.0), param(&mut rng, -5.0, 5.0));
        let r = bh::hyp_1f1(&exact(a), &exact(b), &exact(z), P);
        tm.check(mh::hyp_1f1(a, b, z), r, floor_1f1(a, b, z) * BIG, format!("{a}, {b}, {z}"));
    }
    tp.report(4.0);
    tm.report(4.0);
}

#[test]
fn hyp_0f1_grid() {
    let mut rng = Lcg(41);
    let mut tp = Table::new("0f1 z >= 0");
    let mut tn = Table::new("0f1 z < 0");
    let n = 900;
    for i in 0..n {
        if i % 150 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let b = param(&mut rng, -5.0, 5.0);
        let z = if i % 2 == 0 { rng.range(0.0, 1000.0) } else { -(10f64.powf(rng.range(-2.0, 3.5))) };
        let r = bh::hyp_0f1(&exact(b), &exact(z), P);
        // near the zeros (z < 0) relative to the envelope of the oscillation, |Gamma(b)| (x/2)^(1/2-b) / sqrt(pi/2)
        let fl = if z < 0.0 {
            let x = 2.0 * (-z).sqrt();
            let g = mc_special::machine::gamma::gamma(b).abs();
            (g * (x / 2.0).powf(0.5 - b) / std::f64::consts::FRAC_PI_2.sqrt() * BIG * 1048576.0).min(1e300)
        } else {
            0.0
        };
        if z >= 0.0 {
            tp.check(mh::hyp_0f1(b, z), r, 0.0, format!("{b}, {z}"));
        } else {
            tn.check(mh::hyp_0f1(b, z), r, fl, format!("{b}, {z}"));
        }
    }
    tp.report(4.0);
    tn.report(4.0);
}

#[test]
fn hyp_pfq_grid() {
    let mut rng = Lcg(43);
    let mut t32 = Table::new("3f2 |z| < 1");
    let mut t22 = Table::new("2f2");
    let mut t13 = Table::new("1f3");
    let n = 300;
    for i in 0..n {
        if i % 100 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let mut v = || rng.range(0.1, 5.0);
        let (a1, a2, a3, b1, b2) = (v(), v(), v(), v(), v());
        let z = rng.range(-0.95, 0.95);
        let ra = [exact(a1), exact(a2), exact(a3)];
        let r = bh::hyp_pfq(&ra, &[exact(b1), exact(b2)], &exact(z), P);
        t32.check(mh::hyp_pfq(&[a1, a2, a3], &[b1, b2], z), r, 0.0, format!("{a1}, {a2}, {a3}; {b1}, {b2}; {z}"));
        let z = rng.range(-20.0, 20.0);
        let fl = abs_sum(&[a1, a2], &[b1, b2], z) * BIG;
        let r = bh::hyp_pfq(&[exact(a1), exact(a2)], &[exact(b1), exact(b2)], &exact(z), P);
        t22.check(mh::hyp_pfq(&[a1, a2], &[b1, b2], z), r, fl, format!("{a1}, {a2}; {b1}, {b2}; {z}"));
        let fl = abs_sum(&[a1], &[b1, b2, a3], z) * BIG;
        let r = bh::hyp_pfq(&[exact(a1)], &[exact(b1), exact(b2), exact(a3)], &exact(z), P);
        t13.check(mh::hyp_pfq(&[a1], &[b1, b2, a3], z), r, fl, format!("{a1}; {b1}, {b2}, {a3}; {z}"));
    }
    t32.report(4.0);
    t22.report(4.0);
    t13.report(4.0);
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

fn functions() -> Vec<(&'static str, BallFn, ArgGen)> {
    vec![
        (
            "2f1",
            |a, p| bh::hyp_2f1(&a[0], &a[1], &a[2], &a[3], p),
            |r| vec![r.point(-5, 5), r.point(-5, 5), r.point(-5, 5), r.point(-10, 1)],
        ),
        ("1f1", |a, p| bh::hyp_1f1(&a[0], &a[1], &a[2], p), |r| vec![r.point(-5, 5), r.point(-5, 5), r.point(-40, 40)]),
        ("0f1", |a, p| bh::hyp_0f1(&a[0], &a[1], p), |r| vec![r.point(-5, 5), r.point(-200, 200)]),
        (
            "3f2",
            |a, p| bh::hyp_pfq(&a[0..3], &a[3..5], &a[5], p),
            |r| vec![r.point(-5, 5), r.point(-5, 5), r.point(-5, 5), r.point(-5, 5), r.point(-5, 5), r.point(-1, 1)],
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
                // a pole (a b parameter at a non-positive integer)
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
            "{name:<6} {digits} digits: {:.2} ms per value pair, {misses} without a value",
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
fn identities_at_50_digits() {
    let p = 180;
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    let close = |a: &Ball, b: &Ball| {
        let d = a.sub(b, p);
        d.contains_zero() || ball::f64_of(&d.mid).abs() < 1e-50 * ball::f64_of(&b.mid).abs()
    };
    // Gauss's theorem against the series near 1: 2F1(1/3, 1/4; 5/2; 1)
    let g = bh::hyp_2f1(&q(1, 3), &q(1, 4), &q(5, 2), &q(1, 1), p).unwrap().re;
    let gm = |x: Ball| mc_special::ball::gamma::gamma_ball(&x, p).unwrap();
    let want = gm(Ball::from_ratio(5, 2, p))
        .mul(&gm(Ball::from_ratio(23, 12, p)), p)
        .div(&gm(Ball::from_ratio(13, 6, p)).mul(&gm(Ball::from_ratio(9, 4, p)), p), p)
        .unwrap();
    assert!(close(&g, &want));
    // the 3F2 Saalschutz sum: 3F2(-n, a, b; c, 1+a+b-c-n; 1) = (c-a)_n (c-b)_n / ((c)_n (c-a-b)_n), n = 3,
    // a = 1/2, b = 1/3, c = 2: (3/2)_3 (5/3)_3 / ((2)_3 (7/6)_3)
    let v = bh::hyp_pfq(&[q(-3, 1), q(1, 2), q(1, 3)], &[q(2, 1), q(-19, 6)], &q(1, 1), p).unwrap().re;
    let poch = |x: f64| x * (x + 1.0) * (x + 2.0);
    let want = poch(1.5) * poch(5.0 / 3.0) / (poch(2.0) * poch(7.0 / 6.0));
    assert!((ball::f64_of(&v.mid) - want).abs() < 1e-14 * want);
    // Kummer's transformation M(a, b, z) = e^z M(b-a, b, -z) at a = 7/3, b = 1/5, z = -12 (checked through the
    // direct alternating series at high precision)
    let v = bh::hyp_1f1(&q(7, 3), &q(1, 5), &q(-12, 1), p).unwrap().re;
    let wp = 600;
    let s =
        bh::pfq_sum(&[Ball::from_ratio(7, 3, wp)], &[Ball::from_ratio(1, 5, wp)], &Ball::from_i64(-12), wp).unwrap();
    assert!(close(&v, &s));
}

#[test]
fn machine_timings() {
    let mut rng = Lcg(3);
    let xs: Vec<(f64, f64, f64, f64)> = (0..2000)
        .map(|_| (rng.range(0.0, 5.0), rng.range(0.0, 5.0), rng.range(0.1, 5.0), rng.range(-10.0, 1.0)))
        .collect();
    let time = |name: &str, f: &dyn Fn(f64, f64, f64, f64) -> f64| {
        let start = Instant::now();
        let mut acc = 0.0;
        for &(a, b, c, z) in &xs {
            acc += f(a, b, c, z);
        }
        let us = start.elapsed().as_secs_f64() * 1e6 / xs.len() as f64;
        println!("{name:<10} {us:>7.2} us per call ({acc:.3e})");
        assert!(us < 2000.0, "{name}: {us} us");
    };
    time("2f1", &|a, b, c, z| mh::hyp_2f1(a, b, c, z));
    time("1f1", &|a, b, _, z| mh::hyp_1f1(a, b, 3.0 * z));
    time("0f1", &|_, b, _, z| mh::hyp_0f1(b, 10.0 * z));
    time("3f2", &|a, b, c, z| mh::hyp_pfq(&[a, b, 0.5], &[c, 1.5], z / 11.0));
}

#[test]
fn ball_timings() {
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    type Timed = (&'static str, Box<dyn Fn(u64) -> Option<CBall>>);
    let cases: Vec<Timed> = vec![
        ("2f1 z=1/3", Box::new(move |p| bh::hyp_2f1(&q(1, 3), &q(5, 2), &q(7, 4), &q(1, 3), p))),
        ("2f1 z=4/5", Box::new(move |p| bh::hyp_2f1(&q(1, 3), &q(5, 2), &q(7, 4), &q(4, 5), p))),
        ("2f1 z=-7", Box::new(move |p| bh::hyp_2f1(&q(1, 3), &q(5, 2), &q(7, 4), &q(-7, 1), p))),
        ("1f1 z=-25", Box::new(move |p| bh::hyp_1f1(&q(1, 3), &q(5, 2), &q(-25, 1), p))),
        ("0f1 z=-100", Box::new(move |p| bh::hyp_0f1(&q(5, 2), &q(-100, 1), p))),
        ("3f2 z=1/2", Box::new(move |p| bh::hyp_pfq(&[q(1, 3), q(5, 2), q(1, 1)], &[q(7, 4), q(3, 1)], &q(1, 2), p))),
    ];
    for (name, f) in &cases {
        let mut line = format!("{name:<12}");
        for digits in [50u64, 300] {
            let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64;
            let start = Instant::now();
            assert!(f(p).is_some(), "{name}");
            line += &format!("  {digits} digits {:>8.2} ms", start.elapsed().as_secs_f64() * 1e3);
        }
        println!("{line}");
    }
}

#[test]
fn hard_cases() {
    // printed: the cases where the brief expects cancellation or degenerate transformations
    let cases: Vec<(&str, f64, Option<CBall>)> = vec![
        (
            "1f1(-20.5, 1.5, 30)",
            mh::hyp_1f1(-20.5, 1.5, 30.0),
            bh::hyp_1f1(&exact(-20.5), &exact(1.5), &exact(30.0), P),
        ),
        ("1f1(-40, 2.5, 60)", mh::hyp_1f1(-40.0, 2.5, 60.0), bh::hyp_1f1(&exact(-40.0), &exact(2.5), &exact(60.0), P)),
        (
            "1f1(30.5, 2.5, -60)",
            mh::hyp_1f1(30.5, 2.5, -60.0),
            bh::hyp_1f1(&exact(30.5), &exact(2.5), &exact(-60.0), P),
        ),
        ("1f1(0.5, 1.5, 800)", mh::hyp_1f1(0.5, 1.5, 800.0), bh::hyp_1f1(&exact(0.5), &exact(1.5), &exact(800.0), P)),
        (
            "1f1(2.5, 1.5, -2000)",
            mh::hyp_1f1(2.5, 1.5, -2000.0),
            bh::hyp_1f1(&exact(2.5), &exact(1.5), &exact(-2000.0), P),
        ),
        (
            "2f1 c-a-b = 1+1e-10",
            mh::hyp_2f1(0.5, 0.25, 1.75 + 1e-10, 0.9),
            bh::hyp_2f1(&exact(0.5), &exact(0.25), &exact(1.75 + 1e-10), &exact(0.9), P),
        ),
        (
            "2f1 c-a-b = -2, z=.999",
            mh::hyp_2f1(1.5, 2.5, 2.0, 0.999),
            bh::hyp_2f1(&exact(1.5), &exact(2.5), &exact(2.0), &exact(0.999), P),
        ),
        (
            "2f1 b-a = 1, z=-1e4",
            mh::hyp_2f1(0.5, 1.5, 2.25, -1e4),
            bh::hyp_2f1(&exact(0.5), &exact(1.5), &exact(2.25), &exact(-1e4), P),
        ),
        (
            "2f1 a=b, z=-1e3",
            mh::hyp_2f1(1.25, 1.25, 3.5, -1e3),
            bh::hyp_2f1(&exact(1.25), &exact(1.25), &exact(3.5), &exact(-1e3), P),
        ),
        (
            "2f1(-7.5, 9.5, 1.5, 0.4)",
            mh::hyp_2f1(-7.5, 9.5, 1.5, 0.4),
            bh::hyp_2f1(&exact(-7.5), &exact(9.5), &exact(1.5), &exact(0.4), P),
        ),
        ("0f1(50.5, -1000)", mh::hyp_0f1(50.5, -1000.0), bh::hyp_0f1(&exact(50.5), &exact(-1000.0), P)),
        ("0f1(1.5, -1e5)", mh::hyp_0f1(1.5, -1e5), bh::hyp_0f1(&exact(1.5), &exact(-1e5), P)),
    ];
    // the ball declines the degenerate case far out (the series in w = z/(z-1) would need about 10^8 terms)
    assert!(bh::hyp_2f1(&exact(0.5), &exact(1.5), &exact(2.25), &exact(-1e6), P).is_none());
    assert!(mh::hyp_2f1(0.5, 1.5, 2.25, -1e6).is_finite());
    for (name, got, r) in cases {
        let r = r.unwrap_or_else(|| panic!("{name}: no reference"));
        let e = ulps(got, &r.re, 0.0);
        println!("{name:<26} machine {got:<24e} {e:>10.2} ulp");
        // the terminating Laguerre-type sum cancels by about 2^61 (documented)
        assert!(e <= if name.contains("(-40") { 400.0 } else { 4.0 }, "{name}");
    }
}
