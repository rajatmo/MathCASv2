//! Machine values against high-precision values from the ball path (at 160 bits, at the exact binary value of each
//! machine argument): the relative error in ulps of the ball value, over fixed grids. Each test prints its
//! table (`--nocapture`) and asserts the maximum.
use mc_arith::num::Ball;
use mc_arith::Real;
use mc_special::ball;
use mc_special::machine::{bessel, gamma, zeta};

const P: u64 = 160;

/// A small deterministic generator (the tests are reproducible).
struct Lcg(u64);

impl Lcg {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn exact(v: f64) -> Real {
    Real::Ball(Ball::exact(ball::float_of(v)), P)
}

/// The error of a machine value in ulps of the 160-bit ball's midpoint (0 when both are the same
/// non-finite value or both round to zero).
fn ulps(got: f64, r: &Ball) -> f64 {
    let want = ball::f64_of(&r.mid);
    if want == 0.0 || !want.is_finite() {
        return if got == want { 0.0 } else { f64::INFINITY };
    }
    if !got.is_finite() {
        return f64::INFINITY;
    }
    let ulp = spacing(want);
    let d = ball::float_of(got).sub_exact(&r.mid);
    ball::f64_of(&d).abs() / ulp
}

/// The distance from v to the next double away from zero.
fn spacing(v: f64) -> f64 {
    let a = v.abs();
    f64::from_bits(a.to_bits() + 1) - a
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
    fn report(&self, bound: f64) {
        println!(
            "{:<12} points {:>5}  max {:>6.2} ulp  mean {:>5.2}  worst at {}",
            self.name,
            self.count,
            self.worst,
            self.total / self.count.max(1) as f64,
            self.at
        );
        assert!(self.worst <= bound, "{}: {} ulps at {}", self.name, self.worst, self.at);
    }
}

#[test]
fn gamma_family() {
    let mut rng = Lcg(7);
    let mut tg = Table::new("gamma");
    let mut tl = Table::new("log_gamma");
    let mut td = Table::new("digamma");
    let mut tp = Table::new("polygamma");
    let n = 1500;
    for i in 0..n {
        if i % 300 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let x = -20.0 + 190.0 * rng.unit();
        if x <= 0.0 && (x - x.round()).abs() < 1e-6 {
            continue;
        }
        let r = ball::gamma::gamma(&exact(x), P).expect("gamma reference");
        tg.add(ulps(gamma::gamma(x), &r.re), format!("{x}"));
        let l = ball::gamma::log_gamma(&exact(x), P).expect("log_gamma reference");
        tl.add(ulps(gamma::log_gamma(x).0, &l.re), format!("{x}"));
        let xd = -20.0 + 120.0 * rng.unit();
        if let Some(d) = ball::gamma::digamma(&exact(xd), P) {
            td.add(ulps(gamma::digamma(xd), &d.re), format!("{xd}"));
        }
        let k = (rng.unit() * 6.0).floor() + 1.0;
        let xp = 0.01 + 60.0 * rng.unit();
        let p = ball::gamma::polygamma(&exact(k), &exact(xp), P).expect("polygamma reference");
        tp.add(ulps(gamma::polygamma(k, xp), &p.re), format!("{k}, {xp}"));
    }
    // near the zeros of log gamma and the poles of gamma
    for x in [1.0 + 2f64.powi(-30), 2.0 - 2f64.powi(-30), -3.0 + 1e-9, -1.0 - 1e-12, 1e-300, 1.5, 0.5] {
        let r = ball::gamma::gamma(&exact(x), P).unwrap();
        tg.add(ulps(gamma::gamma(x), &r.re), format!("{x}"));
        let l = ball::gamma::log_gamma(&exact(x), P).unwrap();
        tl.add(ulps(gamma::log_gamma(x).0, &l.re), format!("{x}"));
    }
    tg.report(4.0);
    tl.report(4.0);
    td.report(4.0);
    tp.report(4.0);
}

#[test]
fn zeta_grid() {
    let mut rng = Lcg(11);
    let mut t = Table::new("zeta");
    let n = 800;
    for i in 0..n {
        if i % 200 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let s = -20.0 + 80.0 * rng.unit();
        let r = ball::zeta::zeta(&exact(s), P).expect("zeta reference");
        t.add(ulps(zeta::zeta(s), &r.re), format!("{s}"));
    }
    for s in [1.0 + 1e-8, 1.0 - 1e-8, 0.0, -1.0, -19.0, 59.5, 0.5, 2.0] {
        let r = ball::zeta::zeta(&exact(s), P).unwrap();
        t.add(ulps(zeta::zeta(s), &r.re), format!("{s}"));
    }
    t.report(4.0);
}

#[test]
fn bessel_grid() {
    let nus = [0.0, 1.0 / 3.0, 0.5, 1.0, 2.0, 2.5, 5.0, 10.0, 30.0];
    let mut tk = Table::new("bessel_k");
    let mut ti = Table::new("bessel_i");
    let mut rng = Lcg(3);
    let per = 60;
    for (j, &nu) in nus.iter().enumerate() {
        println!("[{j}/{} {}%]", nus.len(), 100 * j / nus.len());
        for i in 0..per {
            // log-spaced in [1e-6, 700] with jitter
            let x =
                (1e-6f64.ln() + (700f64.ln() - 1e-6f64.ln()) * (i as f64 + rng.unit()) / per as f64).exp().min(700.0);
            let r = ball::bessel::bessel_k(&exact(nu), &exact(x), P).expect("K reference");
            tk.add(ulps(bessel::bessel_k(nu, x), &r.re), format!("{nu}, {x}"));
            let r = ball::bessel::bessel_i(&exact(nu), &exact(x), P).expect("I reference");
            ti.add(ulps(bessel::bessel_i(nu, x), &r.re), format!("{nu}, {x}"));
        }
    }
    // K at orders up to 3 and x in [2, 600] takes the plain double path (gap G94, decision cov-num.1): a few ulps
    tk.report(6.0);
    ti.report(4.0);
}
