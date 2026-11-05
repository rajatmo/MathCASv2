//! Bessel J, Y and the Airy functions: machine values against references from the ball path (at 160 bits, at the
//! exact binary value of each machine argument), and ball enclosures at 50 and 300 digits.
//!
//! The machine error is measured in ulps of max(|value|, envelope), where the envelope is sqrt(J^2 + Y^2) for the
//! Bessel functions and sqrt(Ai^2 + Bi^2) (sqrt(Ai'^2 + Bi'^2) for the derivatives) for the Airy functions at
//! x < 0: near a zero of an oscillating function the value is small while its absolute error is that of the
//! envelope. Separately, the plain relative error in ulps of the value is reported (and bounded) where the
//! functions do not oscillate: for the Bessel functions below the first sign change of each grid row, for the Airy
//! functions at x >= 0. Each test prints its tables (`--nocapture`) and asserts the maxima.
use mc_arith::num::{Ball, Int, Mag, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_special::ball;
use mc_special::ball::bessel_j as bj;
use mc_special::machine::bessel_j as mj;
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

/// The error of a machine value in ulps of `scale` (0 when both are the same non-finite value).
fn ulps_of(got: f64, r: &Ball, scale: f64) -> f64 {
    let want = ball::f64_of(&r.mid);
    if !want.is_finite() || want == 0.0 && scale == 0.0 {
        return if got == want { 0.0 } else { f64::INFINITY };
    }
    if !got.is_finite() {
        return f64::INFINITY;
    }
    let d = ball::float_of(got).sub_exact(&r.mid);
    ball::f64_of(&d).abs() / spacing(scale.max(want.abs()))
}

struct Table {
    name: String,
    worst: f64,
    at: String,
    count: usize,
    total: f64,
}

impl Table {
    fn new(name: &str) -> Table {
        Table { name: name.to_string(), worst: 0.0, at: String::new(), count: 0, total: 0.0 }
    }
    fn add(&mut self, e: f64, at: String) {
        self.count += 1;
        self.total += e;
        if e > self.worst {
            self.worst = e;
            self.at = at;
        }
    }
    fn report(&self, bound: f64) {
        println!(
            "{:<26} points {:>5}  max {:>6.2} ulp  mean {:>5.2}  worst at {}",
            self.name,
            self.count,
            self.worst,
            self.total / self.count.max(1) as f64,
            self.at
        );
        assert!(self.worst <= bound, "{}: {} ulps at {}", self.name, self.worst, self.at);
    }
}

fn f(r: &CBall) -> f64 {
    ball::f64_of(&r.re.mid)
}

#[test]
fn bessel_jy_grid() {
    let nus = [0.0, 1.0 / 3.0, 0.5, 1.0, 2.0, 2.5, 5.0, 10.0, 30.0, -1.0 / 3.0, -2.5];
    let mut tj = Table::new("bessel_j (envelope)");
    let mut ty = Table::new("bessel_y (envelope)");
    let mut pj = Table::new("bessel_j (plain, pre-zero)");
    let mut py = Table::new("bessel_y (plain, pre-zero)");
    let mut rng = Lcg(3);
    let (n_log, n_lin) = (60, 40);
    let mut machine_time = 0.0;
    let mut calls = 0;
    for (i, &nu) in nus.iter().enumerate() {
        println!("[{i}/{} {}%]", nus.len(), 100 * i / nus.len());
        let mut xs: Vec<f64> = (0..n_log)
            .map(|k| {
                let t = (k as f64 + rng.unit()) / n_log as f64;
                (1e-6f64.ln() + (700f64.ln() - 1e-6f64.ln()) * t).exp().min(700.0)
            })
            .collect();
        xs.extend((0..n_lin).map(|k| 700.0 * (k as f64 + rng.unit()) / n_lin as f64).filter(|&x| x > 0.0));
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let (mut j_sign, mut y_sign) = (None, None);
        let (mut j_pre, mut y_pre) = (true, true);
        for x in xs {
            let rj = bj::bessel_j(&exact(nu), &exact(x), P).expect("J reference");
            let ry = bj::bessel_y(&exact(nu), &exact(x), P).expect("Y reference");
            let env = f(&rj).hypot(f(&ry));
            let t0 = Instant::now();
            let (gj, gy) = (mj::bessel_j(nu, x), mj::bessel_y(nu, x));
            machine_time += t0.elapsed().as_secs_f64();
            calls += 2;
            let at = format!("{nu}, {x}");
            tj.add(ulps_of(gj, &rj.re, env), at.clone());
            ty.add(ulps_of(gy, &ry.re, env), at.clone());
            // the rows' first sign changes end the non-oscillating parts
            let sj = f(&rj) > 0.0;
            let sy = f(&ry) > 0.0;
            j_pre &= j_sign.is_none_or(|s| s == sj);
            y_pre &= y_sign.is_none_or(|s| s == sy);
            (j_sign, y_sign) = (Some(sj), Some(sy));
            if j_pre {
                pj.add(ulps_of(gj, &rj.re, 0.0), at.clone());
            }
            if y_pre {
                py.add(ulps_of(gy, &ry.re, 0.0), at);
            }
        }
    }
    println!("machine: {:.2} us per call (average over the grid)", 1e6 * machine_time / calls as f64);
    tj.report(4.0);
    ty.report(4.0);
    pj.report(4.0);
    py.report(4.0);
}

#[test]
fn airy_grid() {
    type M = fn(f64) -> f64;
    type B = fn(&Real, u64) -> Option<CBall>;
    let fns: [(&str, M, B); 4] = [
        ("airy_ai", mj::airy_ai, bj::airy_ai),
        ("airy_bi", mj::airy_bi, bj::airy_bi),
        ("airy_ai_prime", mj::airy_ai_prime, bj::airy_ai_prime),
        ("airy_bi_prime", mj::airy_bi_prime, bj::airy_bi_prime),
    ];
    let mut env_tables: Vec<Table> = fns.iter().map(|e| Table::new(&format!("{} (envelope)", e.0))).collect();
    let mut plain: Vec<Table> = fns.iter().map(|e| Table::new(&format!("{} (plain, x >= 0)", e.0))).collect();
    let mut rng = Lcg(17);
    let n = 400;
    let mut machine_time = 0.0;
    let mut calls = 0;
    for i in 0..n {
        if i % 100 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let x = if i % 2 == 0 {
            -50.0 + 150.0 * rng.unit()
        } else {
            // denser near the origin and in the oscillating region
            -50.0 * rng.unit().powi(2) * if i % 4 == 1 { 1.0 } else { -0.2 }
        };
        let refs: Vec<CBall> = fns.iter().map(|e| (e.2)(&exact(x), P).expect("Airy reference")).collect();
        let env_v = f(&refs[0]).hypot(f(&refs[1]));
        let env_d = f(&refs[2]).hypot(f(&refs[3]));
        for (k, e) in fns.iter().enumerate() {
            let t0 = Instant::now();
            let got = (e.1)(x);
            machine_time += t0.elapsed().as_secs_f64();
            calls += 1;
            let env = if x >= 0.0 {
                0.0
            } else if k < 2 {
                env_v
            } else {
                env_d
            };
            env_tables[k].add(ulps_of(got, &refs[k].re, env), format!("{x}"));
            if x >= 0.0 {
                plain[k].add(ulps_of(got, &refs[k].re, 0.0), format!("{x}"));
            }
        }
    }
    println!("machine: {:.2} us per call (average over the grid)", 1e6 * machine_time / calls as f64);
    for t in env_tables.iter().chain(plain.iter()) {
        t.report(4.0);
    }
}

/// Is ball `inner` inside ball `outer`?
fn inside(inner: &Ball, outer: &Ball) -> bool {
    let d = inner.mid.sub_exact(&outer.mid).abs();
    let need = d.add_exact(&inner.rad.to_float());
    need.cmp_exact(&outer.rad.to_float()) != std::cmp::Ordering::Greater
}

/// A ball function of the test: name, arity, evaluation.
type BallFn = (&'static str, fn(&[Real], u64) -> Option<CBall>);

fn ball_fns() -> Vec<BallFn> {
    vec![
        ("bessel_j", |a, p| bj::bessel_j(&a[0], &a[1], p)),
        ("bessel_y", |a, p| bj::bessel_y(&a[0], &a[1], p)),
        ("airy_ai", |a, p| bj::airy_ai(&a[0], p)),
        ("airy_bi", |a, p| bj::airy_bi(&a[0], p)),
        ("airy_ai_prime", |a, p| bj::airy_ai_prime(&a[0], p)),
        ("airy_bi_prime", |a, p| bj::airy_bi_prime(&a[0], p)),
    ]
}

fn args(name: &str, rng: &mut Lcg) -> Vec<Rat> {
    if name.starts_with("bessel") {
        vec![rng.point(-6, 12), rng.point(0, 60)]
    } else {
        vec![rng.point(-40, 40)]
    }
}

fn check_at(digits: u64, per: usize) {
    let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64;
    let mut rng = Lcg(digits);
    for (name, func) in ball_fns() {
        let (mut checked, mut misses) = (0, 0);
        let t0 = Instant::now();
        for _ in 0..per {
            let a: Vec<Real> = args(name, &mut rng).into_iter().map(Real::Exact).collect();
            if name == "bessel_y" && matches!(&a[1], Real::Exact(q) if q.num.is_zero()) {
                continue;
            }
            let r = func(&a, p).unwrap_or_else(|| panic!("{name}({a:?}) at {digits} digits"));
            let want = func(&a, 2 * p + 64).expect("the reference");
            assert!(inside(&want.re, &r.re), "{name}({a:?}) at {digits} digits");
            if ball::accuracy(&r.re) >= p as f64 - 4.0 {
                checked += 1;
            } else {
                // only next to a zero of the function: the reference is tiny
                misses += 1;
            }
        }
        let ms = 1e3 * t0.elapsed().as_secs_f64() / per as f64 / 2.0;
        println!("{digits} digits {name:<14} {checked} enclosures with >= p-4 bits, {misses} near zeros, {ms:.2} ms each");
        assert!(misses * 10 <= per, "{name}: {misses} of {per} below p-4 bits");
    }
}

#[test]
fn enclosures_at_50_digits() {
    check_at(50, 40);
}

#[test]
fn enclosures_at_300_digits() {
    check_at(300, if cfg!(debug_assertions) { 2 } else { 10 });
}

#[test]
fn ball_arguments() {
    // a ball argument's enclosure contains the values at its two ends and its midpoint
    let p = 200;
    let mut rng = Lcg(5);
    for (name, func) in ball_fns() {
        for _ in 0..6 {
            let a = args(name, &mut rng);
            let rad = Mag::pow2(-40);
            let balls: Vec<Real> = a.iter().map(|q| Real::Ball(Ball { rad, ..ball::rat_ball(q, p) }, p)).collect();
            let Some(r) = func(&balls, p) else { continue };
            for corner in [-1i64, 0, 1] {
                let pts: Vec<Real> = a
                    .iter()
                    .map(|q| {
                        let m = ball::rat_ball(q, p);
                        let e = Ball::exact(ball::float_of(corner as f64 * 2f64.powi(-40)));
                        Real::Ball(Ball { mid: m.add(&e, p + 64).mid, rad: Mag::ZERO }, p)
                    })
                    .collect();
                if let Some(v) = func(&pts, p) {
                    assert!(inside(&v.re, &r.re), "{name}({a:?}) corner {corner}");
                }
            }
        }
    }
}
