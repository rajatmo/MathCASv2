//! The incomplete gamma and beta functions: machine values against 160-bit ball references at the exact binary
//! arguments (max and mean ulps over log-spaced grids), round trips of the machine inverses measured against the
//! condition number, published values and identities at 50 digits, and ball enclosures at random exact rational
//! arguments (50 and 300 digits) that must contain the value at 2p + 64 bits with >= p - 4 correct bits.
// published values are quoted to more digits than a double holds
#![allow(clippy::excessive_precision)]
use mc_arith::num::{Ball, Int, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_special::ball;
use mc_special::ball::incomplete as bi;
use mc_special::machine::incomplete as mi;
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
    fn log_range(&mut self, lo: f64, hi: f64) -> f64 {
        (self.range(lo.ln(), hi.ln())).exp()
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
            None => panic!("{}: no reference at {at}, machine {got:e}", self.name),
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

/// x for a: uniform in [0, 2a + 50], near a (a ± 3 sqrt(a)), and log-spaced small values.
fn x_for(rng: &mut Lcg, a: f64, i: usize) -> f64 {
    match i % 3 {
        0 => rng.range(0.0, 2.0 * a + 50.0),
        1 => (a + rng.range(-3.0, 3.0) * a.sqrt()).max(1e-3),
        _ => rng.log_range(1e-6, 2.0 * a + 50.0),
    }
}

#[test]
fn gamma_pq_grid() {
    let mut rng = Lcg(31);
    let mut tp = Table::new("gamma_p");
    let mut tq = Table::new("gamma_q");
    let n = 900;
    for i in 0..n {
        if i % 150 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        // a up to 10^4, one point in nine above 10^3
        let a = if i % 9 == 0 { rng.log_range(1e3, 1e4) } else { rng.log_range(1e-3, 1e3) };
        let x = x_for(&mut rng, a, i);
        let at = format!("{a:e}, {x:e}");
        tp.check(mi::gamma_p(a, x), bi::gamma_p(&exact(a), &exact(x), P), at.clone());
        tq.check(mi::gamma_q(a, x), bi::gamma_q(&exact(a), &exact(x), P), at);
    }
    tp.report(4.0);
    tq.report(4.0);
}

#[test]
fn gamma_unregularized_grid() {
    let mut rng = Lcg(37);
    let mut tu = Table::new("gamma_upper");
    let mut tun = Table::new("gamma_upper a<=0");
    let mut tl = Table::new("gamma_lower");
    let n = 600;
    for i in 0..n {
        if i % 150 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let a = rng.log_range(1e-3, 200.0);
        let x = x_for(&mut rng, a, i);
        let at = format!("{a:e}, {x:e}");
        if x > 0.0 {
            tu.check(mi::gamma_upper(a, x), bi::gamma_upper(&exact(a), &exact(x), P), at.clone());
        }
        tl.check(mi::gamma_lower(a, x), bi::gamma_lower(&exact(a), &exact(x), P), at);
        // a <= 0: integers (E1 and below) and non-integers
        let an = if i % 3 == 0 { -(rng.below(12) as f64) } else { rng.range(-12.0, 0.0) };
        let xn = rng.log_range(1e-3, 60.0);
        let at = format!("{an:e}, {xn:e}");
        tun.check(mi::gamma_upper(an, xn), bi::gamma_upper(&exact(an), &exact(xn), P), at);
    }
    tu.report(4.0);
    tun.report(4.0);
    tl.report(4.0);
}

#[test]
fn beta_grid() {
    let mut rng = Lcg(41);
    let mut tr = Table::new("beta_reg");
    let mut tc = Table::new("1 - beta_reg");
    let mut ti = Table::new("beta_inc");
    let n = 800;
    for i in 0..n {
        if i % 200 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let a = rng.log_range(0.05, 1000.0);
        let b = rng.log_range(0.05, 1000.0);
        // x: uniform, near the mean, near the ends
        let x = match i % 4 {
            0 => rng.unit(),
            1 => (a / (a + b) + rng.range(-3.0, 3.0) * (a * b / ((a + b).powi(2) * (a + b + 1.0))).sqrt())
                .clamp(1e-12, 1.0 - 1e-12),
            2 => rng.log_range(1e-12, 0.5),
            _ => 1.0 - rng.log_range(1e-12, 0.5),
        };
        if x <= 0.0 || x >= 1.0 {
            continue;
        }
        let at = format!("{x:e}, {a:e}, {b:e}");
        tr.check(mi::beta_reg(x, a, b), bi::beta_reg(&exact(x), &exact(a), &exact(b), P), at.clone());
        // the complement: 1 - I_x(a, b) = I_(1-x)(b, a) where 1 - x is exact
        if x >= 0.5 {
            let y = 1.0 - x;
            tc.check(mi::beta_reg(y, b, a), bi::beta_reg(&exact(y), &exact(b), &exact(a), P), at.clone());
        }
        if i % 2 == 0 {
            let (a2, b2) = (a.min(60.0), if i % 4 == 0 { rng.range(-6.0, 1.0) } else { b.min(60.0) });
            let at = format!("{x:e}, {a2:e}, {b2:e}");
            ti.check(mi::beta_inc(x, a2, b2), bi::beta_inc(&exact(x), &exact(a2), &exact(b2), P), at);
        }
    }
    tr.report(4.0);
    tc.report(4.0);
    ti.report(8.0);
}

/// The relative error of v = F(x) against want, in ulps, over 1 + cond, cond the relative condition number of F
/// at x counted in units of the spacing of the doubles at x (larger than an ulp for subnormal x).
fn rel_over_cond(v: f64, want: f64, cond: f64, x: f64) -> f64 {
    let unit = (spacing(x) / (x.abs() * f64::EPSILON)).max(1.0);
    ((v - want) / want).abs() / f64::EPSILON / (1.0 + cond * unit)
}

#[test]
fn inverse_round_trips() {
    // P(a, inverse_gamma_p(a, p)) = p within (1 + cond) ulps, cond = x P'(x) / p the relative condition number of
    // P at the root (an error of an ulp in x moves p by cond ulps); q likewise, beta the same with I
    let mut rng = Lcg(43);
    let (mut wg, mut wq, mut wb) = (0.0f64, 0.0f64, 0.0f64);
    let mut at = (String::new(), String::new(), String::new());
    let n = 3000;
    for i in 0..n {
        if i % 1000 == 0 {
            println!("[{i}/{n} {}%]", 100 * i / n);
        }
        let a = rng.log_range(1e-3, 1e4);
        let p = if i % 2 == 0 { rng.unit() } else { 10f64.powf(rng.range(-300.0, -1.0)) };
        let x = mi::inverse_gamma_p(a, p);
        let dens = |x: f64| (a * x.ln() - x - mc_special::machine::gamma::log_gamma(a).0).exp();
        // a root below the smallest subnormal gives 0
        let tiny = f64::from_bits(1);
        let e = if x == 0.0 {
            if mi::gamma_p(a, tiny) >= p * (1.0 - 1e-15) {
                0.0
            } else {
                f64::INFINITY
            }
        } else {
            rel_over_cond(mi::gamma_p(a, x), p, dens(x) / p, x)
        };
        if e > wg {
            wg = e;
            at.0 = format!("a {a:e} p {p:e} -> {x:e}");
        }
        let q = p;
        let x = mi::inverse_gamma_q(a, q);
        let e = if x == 0.0 {
            if mi::gamma_q(a, tiny) <= q * (1.0 + 1e-15) {
                0.0
            } else {
                f64::INFINITY
            }
        } else {
            rel_over_cond(mi::gamma_q(a, x), q, dens(x) / q, x)
        };
        if e > wq {
            wq = e;
            at.1 = format!("a {a:e} q {q:e} -> {x:e}");
        }
        let b = rng.log_range(0.05, 1000.0);
        let a = a.clamp(0.05, 1000.0);
        let pb = if i % 2 == 0 { rng.unit() } else { 10f64.powf(rng.range(-200.0, -1.0)) };
        let x = mi::inverse_beta_reg(a, b, pb);
        let lb = mc_special::machine::gamma::log_gamma(a).0 + mc_special::machine::gamma::log_gamma(b).0
            - mc_special::machine::gamma::log_gamma(a + b).0;
        let d = (a * x.ln() + (b - 1.0) * (-x).ln_1p() - lb).exp();
        let e = if x == 0.0 {
            if mi::beta_reg(tiny, a, b) >= pb * (1.0 - 1e-15) {
                0.0
            } else {
                f64::INFINITY
            }
        } else if x >= 1.0 - f64::EPSILON && mi::beta_reg(x, a, b) <= pb {
            // the root lies between the last doubles and 1, where the linear condition number does not hold
            0.0
        } else {
            rel_over_cond(mi::beta_reg(x, a, b), pb, d / pb, x)
        };
        if e > wb {
            wb = e;
            at.2 = format!("a {a:e} b {b:e} p {pb:e} -> {x:e}");
        }
    }
    println!("inverse_gamma_p  round trip: worst {wg:.2} ulps over 1 + cond at {}", at.0);
    println!("inverse_gamma_q  round trip: worst {wq:.2} ulps over 1 + cond at {}", at.1);
    println!("inverse_beta_reg round trip: worst {wb:.2} ulps over 1 + cond at {}", at.2);
    assert!(wg <= 4.0 && wq <= 4.0 && wb <= 4.0);
}

#[test]
fn published_and_special_values() {
    let close = |got: f64, want: f64, u: f64| ((got - want) / want).abs() <= u * f64::EPSILON;
    // Gamma(1/2, 1) = sqrt(pi) erfc(1)
    assert!(close(
        mi::gamma_upper(0.5, 1.0),
        0.278_805_585_280_661_976_499_232_611_077_439_172_088_550_082_497_17,
        2.0
    ));
    // E1(1)
    assert!(close(
        mi::gamma_upper(0.0, 1.0),
        0.219_383_934_395_520_273_677_163_775_460_121_649_031_047_293_406_91,
        2.0
    ));
    assert!(close(mi::gamma_q(30.0, 25.0), 0.817_896_084_022_544_890_197_579_564_466_454_219_735_711_740_529_15, 2.0));
    assert!(close(
        mi::beta_reg(0.3, 2.5, 7.0 / 3.0),
        0.169_424_500_024_749_536_306_766_716_405_847_042_324_460_307_67,
        4.0
    ));
    assert!(close(
        mi::gamma_upper(-2.5, 0.75),
        0.276_234_693_005_122_149_346_446_724_764_086_338_989_788_696_019_2,
        4.0
    ));
    assert!(close(
        mi::gamma_upper(-3.0, 2.0),
        0.003_127_855_151_707_537_729_959_985_376_365_446_564_887_264_665_294,
        4.0
    ));
    // Q(n, x) = e^-x sum_{k<n} x^k / k!
    for (n, x) in [(1, 0.5), (4, 3.0), (10, 7.5), (25, 40.0)] {
        let mut t = 1.0;
        let mut s = 1.0;
        for k in 1..n {
            t *= x / k as f64;
            s += t;
        }
        assert!(close(mi::gamma_q(n as f64, x), (-x).exp() * s, 8.0), "Q({n}, {x})");
    }
    // I_x(a, 1) = x^a, I_x(1, b) = 1 - (1-x)^b
    assert!(close(mi::beta_reg(0.37, 3.25, 1.0), 0.37f64.powf(3.25), 4.0));
    assert!(close(mi::beta_reg(0.37, 1.0, 4.5), 1.0 - 0.63f64.powf(4.5), 4.0));
    // domain edges
    assert_eq!(mi::gamma_p(2.0, 0.0), 0.0);
    assert_eq!(mi::gamma_q(2.0, f64::INFINITY), 0.0);
    assert!(mi::gamma_p(-1.0, 1.0).is_nan() && mi::gamma_p(1.0, -1.0).is_nan());
    assert_eq!(mi::gamma_upper(-1.0, 0.0), f64::INFINITY);
    assert!(close(mi::gamma_upper(4.0, 0.0), 6.0, 1.0));
    assert_eq!(mi::beta_reg(0.0, 2.0, 3.0), 0.0);
    assert_eq!(mi::beta_reg(1.0, 2.0, 3.0), 1.0);
    assert_eq!(mi::inverse_gamma_p(3.0, 0.0), 0.0);
    assert_eq!(mi::inverse_gamma_p(3.0, 1.0), f64::INFINITY);
    assert_eq!(mi::inverse_beta_reg(3.0, 2.0, 1.0), 1.0);
    // tiny tails
    assert!(mi::gamma_q(1.0, 700.0) > 0.0 && close(mi::gamma_q(1.0, 700.0), (-700f64).exp(), 4.0));
    assert!(close(
        mi::gamma_p(1e-3, 1e-300),
        (1e-3 * (1e-300f64).ln()).exp() / mc_special::machine::gamma::gamma(1.001),
        8.0
    ));
}

#[test]
fn identities_at_50_digits() {
    let p = 170;
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    let tight = |b: &Ball| ball::log2_up(b) < -(p as f64) + 12.0;
    for (an, ad, xn, xd) in [(1, 3, 7, 2), (25, 1, 24, 1), (3, 2, 1, 10), (100, 1, 130, 1)] {
        let (a, x) = (q(an, ad), q(xn, xd));
        let pp = bi::gamma_p(&a, &x, p).unwrap().re;
        let qq = bi::gamma_q(&a, &x, p).unwrap().re;
        assert!(tight(&pp.add(&qq, p).sub(&Ball::from_i64(1), p)), "P + Q at {an}/{ad}, {xn}/{xd}");
        // Gamma(a+1, x) = a Gamma(a, x) + x^a e^-x
        let ab = ball::rat_ball(&Rat::new(Int::from_i64(an), Int::from_i64(ad)), p + 20);
        let xb = ball::rat_ball(&Rat::new(Int::from_i64(xn), Int::from_i64(xd)), p + 20);
        let a1 = q(an + ad, ad);
        let g1 = bi::gamma_upper(&a1, &x, p).unwrap().re;
        let g0 = bi::gamma_upper(&a, &x, p).unwrap().re;
        let w = p + 20;
        let lx = mc_numeric::elementary::log(&xb, w).unwrap();
        let xa = mc_numeric::elementary::exp(&ab.mul(&lx, w).sub(&xb, w), w).unwrap();
        let rhs = ab.mul(&g0, w).add(&xa, w);
        let d = g1.sub(&rhs, w);
        assert!(d.contains_zero() || ball::log2_up(&d) - ball::log2_up(&g1) < -(p as f64) + 12.0, "recurrence");
    }
    // Gamma(1/2, 1) = sqrt(pi) erfc(1)
    let g = bi::gamma_upper(&q(1, 2), &q(1, 1), p).unwrap().re;
    let e = mc_special::ball::erf::erfc(&q(1, 1), p).unwrap().re;
    let sp = mc_numeric::elementary::sqrt(&ball::pi(p).unwrap(), p).unwrap();
    assert!(tight(&g.sub(&sp.mul(&e, p), p)));
    // Gamma(0, x) = E1(x), Gamma(-3, 2), Gamma(-5/2, 3/4) against published digits
    let digits = |b: Option<CBall>| b.unwrap().re.mid.to_sci(50);
    assert_eq!(digits(bi::gamma_upper(&q(0, 1), &q(1, 1), p)).0, "21938393439552027367716377546012164903104729340691");
    assert_eq!(
        digits(bi::gamma_upper(&q(-3, 1), &q(2, 1), p)),
        ("31278551517075377299599853763654465648872646652941".into(), -3)
    );
    assert_eq!(digits(bi::gamma_upper(&q(-5, 2), &q(3, 4), p)).0, "27623469300512214934644672476408633898978869601920");
    assert_eq!(digits(bi::gamma_q(&q(30, 1), &q(25, 1), p)).0, "81789608402254489019757956446645421973571174052915");
    assert_eq!(
        digits(bi::beta_reg(&q(3, 10), &q(5, 2), &q(7, 3), p)).0,
        "16942450002474953630676671640584704232446030767343"
    );
    // I_x(a, 1) = x^a and I_x(1, b) = 1 - (1-x)^b
    let x = q(37, 100);
    let i1 = bi::beta_reg(&x, &q(13, 4), &q(1, 1), p).unwrap().re;
    let xb = ball::rat_ball(&Rat::new(Int::from_i64(37), Int::from_i64(100)), p + 20);
    let pw = mc_numeric::elementary::pow(
        &xb,
        &ball::rat_ball(&Rat::new(Int::from_i64(13), Int::from_i64(4)), p + 20),
        p + 20,
    )
    .unwrap();
    assert!(tight(&i1.sub(&pw, p)));
    let i2 = bi::beta_reg(&x, &q(1, 1), &q(9, 2), p).unwrap().re;
    let yb = ball::rat_ball(&Rat::new(Int::from_i64(63), Int::from_i64(100)), p + 20);
    let pw = mc_numeric::elementary::pow(
        &yb,
        &ball::rat_ball(&Rat::new(Int::from_i64(9), Int::from_i64(2)), p + 20),
        p + 20,
    )
    .unwrap();
    assert!(tight(&i2.sub(&Ball::from_i64(1).sub(&pw, p + 20), p)));
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

fn functions() -> Vec<Case> {
    vec![
        ("gamma_p", |a, p| bi::gamma_p(&a[0], &a[1], p), |r| vec![r.point(0, 40), r.point(0, 60)]),
        ("gamma_q", |a, p| bi::gamma_q(&a[0], &a[1], p), |r| vec![r.point(0, 40), r.point(0, 60)]),
        ("gamma_upper", |a, p| bi::gamma_upper(&a[0], &a[1], p), |r| vec![r.point(-10, 30), r.point(0, 40)]),
        ("gamma_lower", |a, p| bi::gamma_lower(&a[0], &a[1], p), |r| vec![r.point(0, 30), r.point(0, 40)]),
        (
            "beta_reg",
            |a, p| bi::beta_reg(&a[0], &a[1], &a[2], p),
            |r| vec![r.point(0, 1), r.point(0, 30), r.point(0, 30)],
        ),
        (
            "beta_inc",
            |a, p| bi::beta_inc(&a[0], &a[1], &a[2], p),
            |r| vec![r.point(0, 1), r.point(0, 20), r.point(-5, 20)],
        ),
    ]
}

fn check_at(digits: u64, per: usize) {
    let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64;
    let mut rng = Lcg(digits + 1);
    let mut checked = 0;
    for (name, f, gen) in functions() {
        let start = Instant::now();
        let mut misses = 0;
        for _ in 0..per {
            let a: Vec<Real> = gen(&mut rng).into_iter().map(Real::Exact).collect();
            let (Some(r), Some(want)) = (f(&a, p), f(&a, 2 * p + 64)) else {
                // a = 0 for P, Q and gamma; x = 1 with b <= 0
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
    let mut rng = Lcg(47);
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
    let args: Vec<(f64, f64)> = (0..4000).map(|_| (rng.log_range(0.1, 100.0), rng.unit())).collect();
    let time = |name: &str, f: &dyn Fn(f64, f64) -> f64| {
        let start = Instant::now();
        let mut acc = 0.0;
        for &(a, u) in &args {
            acc += f(a, u);
        }
        let us = start.elapsed().as_secs_f64() * 1e6 / args.len() as f64;
        println!("{name:<34} {us:>7.2} us per call ({acc:.3e})");
        assert!(us < 500.0, "{name}: {us} us");
    };
    mi::gamma_p(1.0, 1.0);
    time("gamma_p a in [0.1, 100], x ~ a", &|a, u| mi::gamma_p(a, a * (0.5 + u)));
    time("gamma_q a in [0.1, 100], x ~ a", &|a, u| mi::gamma_q(a, a * (0.5 + u)));
    time("gamma_q a = 1e4 x ~ a", &|_, u| mi::gamma_q(1e4, 1e4 + 200.0 * (u - 0.5)));
    time("gamma_upper a in [-10, 10]", &|a, u| mi::gamma_upper(a / 5.0 - 10.0, 3.0 * u + 0.01));
    time("beta_reg a, b in [0.1, 100]", &|a, u| mi::beta_reg(u, a, 101.0 - a));
    time("beta_reg a = b = 3 (t, F style)", &|_, u| mi::beta_reg(u, 3.0, 3.0));
    time("inverse_gamma_p", &|a, u| mi::inverse_gamma_p(a, u));
    time("inverse_beta_reg", &|a, u| mi::inverse_beta_reg(a, 101.0 - a, u));
}

#[test]
fn ball_timings() {
    // one value per function at 50 and 300 digits (printed)
    let q = |n: i64, d: i64| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)));
    type Timed = (&'static str, Box<dyn Fn(u64) -> Option<CBall>>);
    let cases: Vec<Timed> = vec![
        ("gamma_p", Box::new(move |p| bi::gamma_p(&q(7, 3), &q(5, 2), p))),
        ("gamma_q", Box::new(move |p| bi::gamma_q(&q(7, 3), &q(5, 2), p))),
        ("gamma_q (30, 60)", Box::new(move |p| bi::gamma_q(&q(30, 1), &q(60, 1), p))),
        ("gamma_upper (-5/2)", Box::new(move |p| bi::gamma_upper(&q(-5, 2), &q(3, 4), p))),
        ("gamma_lower", Box::new(move |p| bi::gamma_lower(&q(7, 3), &q(5, 2), p))),
        ("beta_reg", Box::new(move |p| bi::beta_reg(&q(3, 10), &q(5, 2), &q(7, 3), p))),
        ("beta_inc", Box::new(move |p| bi::beta_inc(&q(7, 10), &q(5, 2), &q(7, 3), p))),
    ];
    for (name, f) in &cases {
        let mut line = format!("{name:<20}");
        for digits in [50u64, 300] {
            let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64;
            let start = Instant::now();
            let reps = 3;
            for _ in 0..reps {
                assert!(f(p).is_some(), "{name}");
            }
            line += &format!("  {digits} digits {:>7.2} ms", start.elapsed().as_secs_f64() * 1e3 / reps as f64);
        }
        println!("{line}");
    }
}
