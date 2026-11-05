//! Exponential, logarithmic, sine and cosine integrals: machine values against 160-bit ball references at the exact
//! binary arguments (max and mean ulps relative to the value, per function over the grids), ball enclosures at
//! random exact rational arguments (50 and 300 digits) that must contain the value at 2p + 64 bits with >= p - 4
//! correct bits, identities and derivatives at 50 digits, published values and zeros.
use mc_arith::num::{Ball, Int, Mag, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;
use mc_special::ball;
use mc_special::ball::expint as be;
use mc_special::machine::expint as me;
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
    /// A log-uniform double in [lo, hi].
    fn log(&mut self, lo: f64, hi: f64) -> f64 {
        (self.range(lo.ln(), hi.ln())).exp()
    }
}

fn exact(v: f64) -> Real {
    Real::Ball(Ball::exact(ball::float_of(v)), P)
}

fn q(n: i64, d: i64) -> Real {
    Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)))
}

/// The distance from v to the next double away from zero (the subnormal spacing at least).
fn spacing(v: f64) -> f64 {
    let a = v.abs();
    f64::from_bits(a.to_bits() + 1) - a
}

/// The error of a machine value in ulps of the reference.
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
    /// One measurement: the machine value against the reference (the reference must exist).
    fn check(&mut self, got: f64, r: Option<CBall>, at: String) {
        let r = r.unwrap_or_else(|| panic!("{}: no reference at {at} (machine {got})", self.name));
        self.add(ulps(got, &r.re), at);
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

/// The doubles nearest a ball's midpoint and `k` ulps around it.
fn around(z: &Ball, k: i64) -> Vec<f64> {
    let c = ball::f64_of(&z.mid);
    (-k..=k).map(|i| f64::from_bits((c.to_bits() as i64 + i) as u64)).collect()
}

/// A zero of f near `guess` by Newton steps at 200 bits (f' given).
fn zero(f: &dyn Fn(&Real) -> Option<CBall>, df: &dyn Fn(&Ball) -> Ball, guess: f64) -> Ball {
    let w = 200;
    let mut z = Ball::exact(ball::float_of(guess));
    for _ in 0..8 {
        let v = f(&Real::Ball(z.clone(), w)).expect("a value near the zero").re;
        z = Ball::exact(z.sub(&v.div(&df(&z), w).expect("f' != 0"), w).mid);
    }
    z
}

#[test]
fn ei_grid() {
    let mut rng = Lcg(11);
    let mut t = Table::new("ei");
    let n = 1500;
    for i in 0..n {
        if i % 500 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let mut x = rng.log(1e-6, 700.0);
        if i % 2 == 1 {
            x = -x;
        }
        if i % 10 == 0 {
            // near the zero 0.3725...
            x = 0.372_507_410_781_366_6 + rng.range(-0.01, 0.01);
        }
        t.check(me::exp_integral_ei(x), be::exp_integral_ei(&exact(x), P), format!("{x}"));
    }
    let z = zero(&|x| be::exp_integral_ei(x, 200), &|x| el::exp(x, 200).unwrap().div(x, 200).unwrap(), 0.3725);
    for x in around(&z, 8) {
        t.check(me::exp_integral_ei(x), be::exp_integral_ei(&exact(x), P), format!("{x} (zero)"));
    }
    t.report(4.0);
}

#[test]
fn en_grid() {
    let mut rng = Lcg(12);
    for nu in [0.0, 1.0, 2.0, 5.0, 0.5, 2.5, -1.5, 30.0, 1.0 + 1e-9, 3.25, 41.5, -7.0, -2000.0, 1e6] {
        let mut t = Table::new("expint_e");
        for i in 0..150 {
            let x = if i % 15 == 0 { 4.0 + rng.range(-0.01, 0.01) } else { rng.log(1e-6, 700.0) };
            t.check(me::exp_integral_e(nu, x), be::exp_integral_e(&exact(nu), &exact(x), P), format!("{nu}, {x}"));
        }
        print!("nu = {nu:<14} ");
        t.report(4.0);
    }
    // random orders
    let mut t = Table::new("expint_e");
    for _ in 0..400 {
        let nu = rng.range(-20.0, 60.0);
        let x = rng.log(1e-4, 200.0);
        t.check(me::exp_integral_e(nu, x), be::exp_integral_e(&exact(nu), &exact(x), P), format!("{nu}, {x}"));
    }
    print!("nu random         ");
    t.report(4.0);
}

#[test]
fn li_grid() {
    let mut rng = Lcg(13);
    let mut t = Table::new("li");
    let n = 1500;
    for i in 0..n {
        if i % 500 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let x = match i % 5 {
            0 => 1.0 + rng.range(-1.0, 1.0) * 2f64.powi(-(rng.below(50) as i32)),
            1 => 1.451_369_234_883_381 + rng.range(-0.02, 0.02),
            2 if i % 50 == 2 => rng.log(1e-320, 1e-300),
            _ => rng.log(1e-300, 1e300),
        };
        if x == 1.0 {
            continue;
        }
        t.check(me::log_integral(x), be::log_integral(&exact(x), P), format!("{x}"));
    }
    let z = zero(&|x| be::log_integral(x, 200), &|x| el::log(x, 200).unwrap().recip_ball(), 1.4513);
    for x in around(&z, 8) {
        t.check(me::log_integral(x), be::log_integral(&exact(x), P), format!("{x} (zero)"));
    }
    t.report(4.0);
}

trait Recip {
    fn recip_ball(&self) -> Ball;
}

impl Recip for Ball {
    fn recip_ball(&self) -> Ball {
        Ball::from_i64(1).div(self, 200).unwrap()
    }
}

#[test]
fn si_ci_grid() {
    let mut rng = Lcg(14);
    let mut ts = Table::new("si");
    let mut tc = Table::new("ci");
    let n = 1500;
    for i in 0..n {
        if i % 500 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let x = if i % 10 == 0 { 4.0 + rng.range(-0.01, 0.01) } else { rng.log(1e-6, 1e6) };
        ts.check(me::sin_integral(x), be::sin_integral(&exact(x), P), format!("{x}"));
        ts.check(me::sin_integral(-x), be::sin_integral(&exact(-x), P), format!("{}", -x));
        tc.check(me::cos_integral(x), be::cos_integral(&exact(x), P), format!("{x}"));
    }
    for x in [1e10, 3.5e13, 1e15, 1e20, 1e100, 1e300, f64::MAX] {
        ts.check(me::sin_integral(x), be::sin_integral(&exact(x), P), format!("{x}"));
        tc.check(me::cos_integral(x), be::cos_integral(&exact(x), P), format!("{x}"));
    }
    // around zeros of Ci: the first twelve, and some near 10^3, 10^5
    let dci = |x: &Ball| el::cos(x, 200).unwrap().div(x, 200).unwrap();
    let mut guesses = vec![0.6165, 3.3842, 6.4270];
    for k in 3..12 {
        guesses.push(k as f64 * std::f64::consts::PI);
    }
    for k in [318.0, 319.0, 31831.0, 31832.0] {
        guesses.push(k * std::f64::consts::PI);
    }
    let mut zc = Table::new("ci (zeros)");
    for g in guesses {
        let z = zero(&|x| be::cos_integral(x, 200), &dci, g);
        for x in around(&z, 6) {
            zc.check(me::cos_integral(x), be::cos_integral(&exact(x), P), format!("{x}"));
        }
    }
    ts.report(4.0);
    tc.report(4.0);
    zc.report(4.0);
}

#[test]
fn shi_chi_grid() {
    let mut rng = Lcg(15);
    let mut ts = Table::new("shi");
    let mut tc = Table::new("chi");
    for i in 0..1000 {
        let x = if i % 10 == 0 { 0.523_822_571_389_864_4 + rng.range(-0.01, 0.01) } else { rng.log(1e-6, 700.0) };
        ts.check(me::sinh_integral(x), be::sinh_integral(&exact(x), P), format!("{x}"));
        tc.check(me::cosh_integral(x), be::cosh_integral(&exact(x), P), format!("{x}"));
    }
    let dchi = |x: &Ball| el::cosh(x, 200).unwrap().div(x, 200).unwrap();
    let z = zero(&|x| be::cosh_integral(x, 200), &dchi, 0.5238);
    for x in around(&z, 8) {
        tc.check(me::cosh_integral(x), be::cosh_integral(&exact(x), P), format!("{x} (zero)"));
    }
    ts.report(4.0);
    tc.report(4.0);
}

#[test]
fn machine_special_values() {
    assert_eq!(me::exp_integral_ei(0.0), f64::NEG_INFINITY);
    assert_eq!(me::exp_integral_ei(1000.0), f64::INFINITY);
    assert_eq!(me::exp_integral_ei(-1000.0), 0.0);
    assert!(me::exp_integral_ei(f64::NAN).is_nan());
    assert_eq!(me::log_integral(0.0), 0.0);
    assert_eq!(me::log_integral(1.0), f64::NEG_INFINITY);
    assert!(me::log_integral(-2.0).is_nan());
    assert_eq!(me::sin_integral(0.0), 0.0);
    assert_eq!(me::sin_integral(f64::INFINITY), std::f64::consts::FRAC_PI_2);
    assert_eq!(me::cos_integral(0.0), f64::NEG_INFINITY);
    assert!(me::cos_integral(-1.0).is_nan() && me::cosh_integral(-1.0).is_nan());
    assert_eq!(me::exp_integral_e(3.0, 0.0), 0.5);
    assert_eq!(me::exp_integral_e(1.0, 0.0), f64::INFINITY);
    assert!(me::exp_integral_e(2.0, -1.0).is_nan() && me::exp_integral_e(-0.5, -1.0).is_nan());
    // integer orders <= 0 are real at x < 0: E_-1(x) = e^-x (1/x + 1/x^2)
    let x = -2.5f64;
    let want = (-x).exp() * (1.0 / x + 1.0 / (x * x));
    assert!((me::exp_integral_e(-1.0, x) - want).abs() <= 4.0 * f64::EPSILON * want.abs());
    // subnormal and huge arguments
    assert!(me::log_integral(5e-324) == 0.0 && me::log_integral(5e-324).is_sign_negative());
    assert!(me::exp_integral_e(2.0, 730.0) > 0.0 && me::exp_integral_e(2.0, 750.0) == 0.0);
}

// ---------------------------------------------------------------- published values, zeros, identities

fn digits(b: &Ball, d: usize) -> String {
    let (s, e) = b.mid.to_sci(d);
    format!("{}{s}e{e}", if b.mid.neg { "-" } else { "" })
}

fn dec(s: &str) -> Real {
    // a decimal string d.ddd... as an exact rational
    let (int_part, frac) = s.split_once('.').unwrap();
    let ten = Int::from_i64(10);
    let mut den = Int::from_i64(1);
    let mut n = Int::from_i64(0);
    for ch in int_part.chars().chain(frac.chars()) {
        n = n.mul(&ten).add(&Int::from_i64(ch.to_digit(10).unwrap() as i64));
    }
    for _ in 0..frac.len() {
        den = den.mul(&ten);
    }
    Real::Exact(Rat::new(n, den))
}

#[test]
fn published_values_50_digits() {
    let p = 180;
    let v = be::exp_integral_ei(&q(1, 1), p).unwrap().re;
    assert_eq!(digits(&v, 50), "18951178163559367554665209343316342690170605817327e0");
    let v = be::exp_integral_e(&q(1, 1), &q(1, 1), p).unwrap().re;
    assert_eq!(digits(&v, 50), "21938393439552027367716377546012164903104729340691e-1");
    let v = be::log_integral(&q(2, 1), p).unwrap().re;
    assert_eq!(digits(&v, 50), "10451637801174927848445888891946131365226155781512e0");
    let v = be::sin_integral(&q(1, 1), p).unwrap().re;
    assert_eq!(digits(&v, 50), "94608307036718301494135331382317965781233795473811e-1");
    // Ci(1) = gamma + sum_k (-1)^k/(2k (2k)!) = 0.33740392290096813466264620388915076999757803258573...
    let v = be::cos_integral(&q(1, 1), p).unwrap().re;
    assert_eq!(digits(&v, 50), "33740392290096813466264620388915076999757803258573e-1");
    // the zeros of li (Ramanujan-Soldner, 54 digits) and Ci (22 digits): the values there are below 1e-52, 1e-21
    let mu = dec("1.45136923488338105028396848589202744949303228364801586");
    let v = be::log_integral(&mu, p).unwrap().re;
    assert!(ball::log2_up(&v) < -52.0 * std::f64::consts::LOG2_10, "li(mu) = {}", digits(&v, 5));
    let z = dec("0.6165054856207162337971");
    let v = be::cos_integral(&z, p).unwrap().re;
    assert!(ball::log2_up(&v) < -21.0 * std::f64::consts::LOG2_10, "Ci(z) = {}", digits(&v, 5));
}

/// |a - b| <= 2^-bits max(|a|, |b|, floor)?
fn agree(a: &Ball, b: &Ball, bits: f64, floor: f64) {
    let d = a.sub(b, 400);
    let scale = ball::log2_up(a).max(ball::log2_up(b)).max(floor);
    assert!(ball::log2_up(&d) < scale - bits, "{} vs {}", digits(a, 30), digits(b, 30));
}

#[test]
fn identities_at_50_digits() {
    let p = 170;
    let bits = 160.0;
    let mut rng = Lcg(31);
    for _ in 0..12 {
        let xq = rng.point(1, 30);
        let x = Real::Exact(xq.clone());
        let xb = ball::rat_ball(&xq, 400);
        // E_(n+1)(x) = (e^-x - x E_n(x))/n
        for n in 1..5i64 {
            let en = be::exp_integral_e(&q(n, 1), &x, p).unwrap().re;
            let en1 = be::exp_integral_e(&q(n + 1, 1), &x, p).unwrap().re;
            let rhs = el::exp(&xb.neg(), 400).unwrap().sub(&xb.mul(&en, 400), 400).div(&Ball::from_i64(n), 400);
            agree(&en1, &rhs.unwrap(), bits - 8.0, -1e9);
        }
        // the same recurrence for a real order
        let nu = Real::Exact(Rat::new(Int::from_i64(7), Int::from_i64(3)));
        let nub = ball::rat_ball(&Rat::new(Int::from_i64(7), Int::from_i64(3)), 400);
        let a = be::exp_integral_e(&nu, &x, p).unwrap().re;
        let nu1 = Real::Exact(Rat::new(Int::from_i64(10), Int::from_i64(3)));
        let b = be::exp_integral_e(&nu1, &x, p).unwrap().re;
        let rhs = el::exp(&xb.neg(), 400).unwrap().sub(&xb.mul(&a, 400), 400).div(&nub, 400).unwrap();
        agree(&b, &rhs, bits - 8.0, -1e9);
        // Ei(x) - Ei(-x) = 2 Shi(x), Ei(x) + Ei(-x) = 2 Chi(x)
        let mx = Real::Exact(Rat::new(xq.num.neg(), Int::from_nat(xq.den.clone())));
        let ep = be::exp_integral_ei(&x, p).unwrap().re;
        let em = be::exp_integral_ei(&mx, p).unwrap().re;
        let shi = be::sinh_integral(&x, p).unwrap().re;
        let chi = be::cosh_integral(&x, p).unwrap().re;
        agree(&ep.sub(&em, 400), &shi.mul(&Ball::from_i64(2), 400), bits, -1e9);
        agree(&ep.add(&em, 400), &chi.mul(&Ball::from_i64(2), 400), bits - 8.0, ball::log2_up(&ep));
        // E1(x) = -Ei(-x)
        let e1 = be::exp_integral_e(&q(1, 1), &x, p).unwrap().re;
        agree(&e1, &em.neg(), bits, -1e9);
        // li(e^x) = Ei(x): li at an exact point near e^x against Ei at its exact logarithm
        let y = el::exp(&xb, 400).unwrap();
        let yr = Real::Ball(Ball::exact(y.mid.clone()), 400);
        let li = be::log_integral(&yr, p).unwrap().re;
        let ly = el::log(&Ball::exact(y.mid), 400).unwrap();
        let ei = be::exp_integral_ei(&Real::Ball(Ball::exact(ly.mid), 400), p).unwrap().re;
        agree(&li, &ei, bits - 8.0, -1e9);
    }
}

#[test]
fn derivatives_by_central_differences() {
    // f'(x) ~ (f(x + h) - f(x - h)) / 2h with h = 2^-80 at 400 bits: the error is about f''' h^2 / 6 ~ 2^-160
    let p = 400;
    let h = Ball::exact(ball::float_of(2f64.powi(-80)));
    let mut rng = Lcg(37);
    type F = Box<dyn Fn(&Real) -> Option<CBall>>;
    type D = Box<dyn Fn(&Ball) -> Ball>;
    let w = 400;
    let cases: Vec<(&str, F, D, (i64, i64))> = vec![
        (
            "ei",
            Box::new(move |x| be::exp_integral_ei(x, p)),
            Box::new(move |x| el::exp(x, w).unwrap().div(x, w).unwrap()),
            (1, 40),
        ),
        (
            "si",
            Box::new(move |x| be::sin_integral(x, p)),
            Box::new(move |x| el::sin(x, w).unwrap().div(x, w).unwrap()),
            (1, 40),
        ),
        (
            "ci",
            Box::new(move |x| be::cos_integral(x, p)),
            Box::new(move |x| el::cos(x, w).unwrap().div(x, w).unwrap()),
            (1, 40),
        ),
        (
            "shi",
            Box::new(move |x| be::sinh_integral(x, p)),
            Box::new(move |x| el::sinh(x, w).unwrap().div(x, w).unwrap()),
            (1, 40),
        ),
        (
            "chi",
            Box::new(move |x| be::cosh_integral(x, p)),
            Box::new(move |x| el::cosh(x, w).unwrap().div(x, w).unwrap()),
            (1, 40),
        ),
        (
            "li",
            Box::new(move |x| be::log_integral(x, p)),
            Box::new(move |x| Ball::from_i64(1).div(&el::log(x, w).unwrap(), w).unwrap()),
            (2, 40),
        ),
        // d/dx E_nu(x) = -E_(nu-1)(x)
        (
            "e_5/2",
            Box::new(move |x| be::exp_integral_e(&q(5, 2), x, p)),
            Box::new(move |x| be::exp_integral_e(&q(3, 2), &Real::Ball(x.clone(), w), p).unwrap().re.neg()),
            (1, 40),
        ),
    ];
    for (name, f, df, (lo, hi)) in &cases {
        for _ in 0..6 {
            let xq = rng.point(*lo, *hi);
            let xb = ball::rat_ball(&xq, w);
            let at = |b: Ball| f(&Real::Ball(b, w)).unwrap().re;
            let num = at(xb.add(&h, w)).sub(&at(xb.sub(&h, w)), w).div(&h.mul(&Ball::from_i64(2), w), w).unwrap();
            let want = df(&xb);
            agree(&num, &want, 150.0, -1e9);
            let _ = name;
        }
    }
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

fn nonzero(r: &mut Lcg, lo: i64, hi: i64) -> Rat {
    loop {
        let v = r.point(lo, hi);
        if !v.is_zero() {
            return v;
        }
    }
}

fn functions() -> Vec<Case> {
    vec![
        ("exp_integral_ei", |a, p| be::exp_integral_ei(&a[0], p), |r| vec![nonzero(r, -50, 50)]),
        ("exp_integral_e", |a, p| be::exp_integral_e(&a[0], &a[1], p), |r| vec![r.point(-10, 10), nonzero(r, 0, 50)]),
        ("log_integral", |a, p| be::log_integral(&a[0], p), |r| vec![nonzero(r, 0, 100)]),
        ("sin_integral", |a, p| be::sin_integral(&a[0], p), |r| vec![r.point(-50, 50)]),
        ("cos_integral", |a, p| be::cos_integral(&a[0], p), |r| vec![nonzero(r, 0, 50)]),
        ("sinh_integral", |a, p| be::sinh_integral(&a[0], p), |r| vec![r.point(-50, 50)]),
        ("cosh_integral", |a, p| be::cosh_integral(&a[0], p), |r| vec![nonzero(r, 0, 50)]),
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
                // li(1) is a pole
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
            "{name:<16} {digits} digits: {:.2} ms per value pair, {misses} without a value",
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
            let rad = Mag::pow2(-40);
            let balls: Vec<Real> = a.iter().map(|q| Real::Ball(Ball { rad, ..ball::rat_ball(q, p) }, p)).collect();
            let Some(r) = f(&balls, p) else { continue };
            for corner in [-1i64, 0, 1] {
                let pts: Vec<Real> = a
                    .iter()
                    .map(|q| {
                        let m = ball::rat_ball(q, p);
                        let e = Ball::exact(ball::float_of(corner as f64 * 2f64.powi(-40)));
                        Real::Ball(Ball { mid: m.add(&e, p + 64).mid, rad: Mag::ZERO }, p)
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
    let xs: Vec<f64> = (0..4000).map(|_| rng.log(1e-3, 100.0)).collect();
    let _ = me::exp_integral_ei(1.0); // the one-time constants
    let time = |name: &str, f: &dyn Fn(f64) -> f64| {
        let start = Instant::now();
        let mut acc = 0.0;
        for &x in &xs {
            acc += f(x);
        }
        let us = start.elapsed().as_secs_f64() * 1e6 / xs.len() as f64;
        println!("{name:<16} {us:>6.2} us per call ({acc:.3e})");
        assert!(us < 200.0, "{name}: {us} us");
    };
    time("exp_integral_ei", &me::exp_integral_ei);
    time("exp_integral_e 1", &|x| me::exp_integral_e(1.0, x));
    time("exp_integral_e 5/2", &|x| me::exp_integral_e(2.5, x));
    time("exp_integral_e 30", &|x| me::exp_integral_e(30.0, x));
    time("log_integral", &|x| me::log_integral(x * 10.0));
    time("sin_integral", &me::sin_integral);
    time("cos_integral", &me::cos_integral);
    time("sinh_integral", &me::sinh_integral);
    time("cosh_integral", &me::cosh_integral);
}

#[test]
fn ball_timings() {
    type Timed = (&'static str, Box<dyn Fn(u64) -> Option<CBall>>);
    let cases: Vec<Timed> = vec![
        ("ei(7/3)", Box::new(|p| be::exp_integral_ei(&q(7, 3), p))),
        ("ei(-25/2)", Box::new(|p| be::exp_integral_ei(&q(-25, 2), p))),
        ("e(5/2, 7/3)", Box::new(|p| be::exp_integral_e(&q(5, 2), &q(7, 3), p))),
        ("e(3, 7/3)", Box::new(|p| be::exp_integral_e(&q(3, 1), &q(7, 3), p))),
        ("li(10^6)", Box::new(|p| be::log_integral(&q(1_000_000, 1), p))),
        ("si(25/2)", Box::new(|p| be::sin_integral(&q(25, 2), p))),
        ("ci(25/2)", Box::new(|p| be::cos_integral(&q(25, 2), p))),
        ("ci(1000)", Box::new(|p| be::cos_integral(&q(1000, 1), p))),
        ("shi(7/3)", Box::new(|p| be::sinh_integral(&q(7, 3), p))),
        ("chi(7/3)", Box::new(|p| be::cosh_integral(&q(7, 3), p))),
    ];
    for (name, f) in &cases {
        let mut line = format!("{name:<14}");
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

#[test]
fn huge_arguments() {
    // no panics or hangs: values or None
    for x in [1e5, 1e6, 1e20, 1e300, -1e300] {
        let start = Instant::now();
        let r = [
            be::exp_integral_ei(&exact(x), P).is_some(),
            be::exp_integral_e(&exact(2.5), &exact(x.abs()), P).is_some(),
            be::log_integral(&exact(x.abs()), P).is_some(),
            be::sinh_integral(&exact(x), P).is_some(),
            be::cosh_integral(&exact(x.abs()), P).is_some(),
        ];
        println!("{x:e}: {r:?} in {:?}", start.elapsed());
        assert!(start.elapsed().as_secs_f64() < 20.0);
    }
    // up to 10^6 everything has a value; e^+-x beyond the exponent range of the floats (|x| > about 6e18) has none,
    // but li(10^300) = Ei(690.8...) does
    for x in [1e5, 1e6] {
        assert!(be::exp_integral_ei(&exact(-x), P).is_some() && be::exp_integral_ei(&exact(x), P).is_some());
    }
    let li = be::log_integral(&exact(1e300), P).unwrap().re;
    assert!(ulps(me::log_integral(1e300), &li) <= 1.0);
}
