//! Ball enclosures: every function at random exact arguments, at 50 and 300 digits, must contain the value
//! computed at more than twice the precision, and have at least p - 4 correct bits. Ball arguments with radii
//! must contain the values at their ends and midpoints.
use mc_arith::num::{Ball, Int, Mag, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_special::ball;
use mc_special::functions::{Special, ALL};

struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: i64) -> i64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) % n as u64) as i64
    }
    /// A rational in [lo, hi] with denominator 97.
    fn point(&mut self, lo: i64, hi: i64) -> Rat {
        let k = lo * 97 + self.below((hi - lo) * 97 + 1);
        Rat::new(Int::from_i64(k), Int::from_i64(97))
    }
}

/// Is ball `inner` inside ball `outer`?
fn inside(inner: &Ball, outer: &Ball) -> bool {
    let d = inner.mid.sub_exact(&outer.mid).abs();
    let need = d.add_exact(&inner.rad.to_float());
    need.cmp_exact(&outer.rad.to_float()) != std::cmp::Ordering::Greater
}

fn cinside(a: &CBall, b: &CBall) -> bool {
    inside(&a.re, &b.re) && inside(&a.im, &b.im)
}

/// Arguments for a function: avoiding poles, with x > 0 for the Bessel functions.
fn args(f: Special, rng: &mut Lcg) -> Vec<Rat> {
    let r = |rng: &mut Lcg, lo, hi| rng.point(lo, hi);
    match f {
        Special::Gamma | Special::LogGamma | Special::Digamma => vec![r(rng, -15, 40)],
        Special::Polygamma => vec![Rat::from_i64(rng.below(5)), r(rng, -10, 30)],
        Special::Beta => vec![r(rng, -5, 20), r(rng, -5, 20)],
        Special::Zeta => vec![r(rng, -30, 40)],
        Special::BesselK | Special::BesselI => vec![r(rng, -6, 12), r(rng, 0, 60)],
    }
}

fn check_at(digits: u64, per: usize) {
    let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64;
    let mut rng = Lcg(digits);
    let mut checked = 0;
    for f in ALL {
        let mut misses = 0;
        for _ in 0..per {
            let a: Vec<Real> = args(f, &mut rng).into_iter().map(Real::Exact).collect();
            let (Some(r), Some(want)) = (f.ball_value(&a, p), f.ball_value(&a, 2 * p + 64)) else {
                // a pole or a point next to one
                misses += 1;
                continue;
            };
            assert!(cinside(&want, &r), "{}({a:?}) at {digits} digits", f.name());
            assert!(
                ball::accuracy(&r.re) >= p as f64 - 4.0 || r.re.contains_zero(),
                "{}({a:?}): {} bits",
                f.name(),
                ball::accuracy(&r.re)
            );
            checked += 1;
        }
        assert!(misses * 5 <= per, "{}: {misses} of {per} without a value", f.name());
    }
    println!("{digits} digits: {checked} enclosures checked");
}

#[test]
fn enclosures_at_50_digits() {
    check_at(50, 40);
}

#[test]
fn enclosures_at_300_digits() {
    check_at(300, if cfg!(debug_assertions) { 2 } else { 8 });
}

#[test]
fn ball_arguments() {
    // a ball argument's enclosure contains the values at its two ends and its midpoint
    let p = 200;
    let mut rng = Lcg(5);
    for f in ALL {
        for _ in 0..6 {
            let a = args(f, &mut rng);
            let rad = Mag::pow2(-40);
            let balls: Vec<Real> = a.iter().map(|q| Real::Ball(Ball { rad, ..ball::rat_ball(q, p) }, p)).collect();
            let Some(r) = f.ball_value(&balls, p) else { continue };
            for corner in [-1i64, 0, 1] {
                let pts: Vec<Real> = a
                    .iter()
                    .map(|q| {
                        let m = ball::rat_ball(q, p);
                        let e = Ball::exact(ball::float_of(corner as f64 * 2f64.powi(-40)));
                        Real::Ball(Ball { mid: m.add(&e, p + 64).mid, rad: Mag::ZERO }, p)
                    })
                    .collect();
                if let Some(v) = f.ball_value(&pts, p) {
                    assert!(cinside(&v, &r), "{}({a:?}) corner {corner}", f.name());
                }
            }
        }
    }
}
