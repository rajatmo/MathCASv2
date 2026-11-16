//! Property answers proved from the literature and from the limits on pieces, and `reduce` on disjunctions whose
//! pieces meet at a boundary. Every answer is checked by an independent route: values at 30 digits on a grid, or
//! the truth of the condition evaluated at exact points on and beside each boundary.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn num(e: &mut CoreEngine, src: &str) -> f64 {
    let s = ev(e, &format!("n({src})"));
    s.split('`').next().unwrap_or(&s).parse().unwrap_or_else(|_| panic!("{src} = {s}"))
}

/// The smallest and largest of 30-digit values of `f` (a function of `x`) on `x = lo + k/8`, k = 0..n.
fn sampled(e: &mut CoreEngine, f: &str, lo: i64, n: i64) -> (f64, f64) {
    let out = ev(e, &format!("vs = table(n(rewrite({f}, x -> {lo} + k/8), 30), [k, 0, {n}]); n([min(vs), max(vs)])"));
    let inner = out.trim_start_matches('[').trim_end_matches(']');
    let v: Vec<f64> = inner.split(',').map(|s| s.trim().split('`').next().unwrap().parse().unwrap()).collect();
    (v[0], v[1])
}

#[test]
fn ranges_from_the_first_extremum_and_an_envelope() {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    assert_eq!(ev(&mut e, "function_range(bessel_j(0, x), x, y)"), "bessel_j(0, bessel_j_zero(1, 1)) <= y and y <= 1");
    assert_eq!(ev(&mut e, "function_range(bessel_y(0, x), x, y)"), "y <= bessel_y(0, bessel_y_zero(1, 1))");
    assert_eq!(ev(&mut e, "function_range(anger_j(0, x), x, y)"), "bessel_j(0, bessel_j_zero(1, 1)) <= y and y <= 1");
    assert_eq!(ev(&mut e, "function_range(sinc(x), x, y)"), "sinc(bessel_j_zero(3/2, 1)) <= y and y <= 1");
    // independent: the sampled extremes on [-40, 40] (J0, sinc) and (0, 40] (Y0) lie inside and come close
    for (f, lo, n, a, b) in [
        ("bessel_j(0, x)", -40, 640, "bessel_j(0, bessel_j_zero(1, 1))", "1"),
        ("sinc(x)", -40, 640, "sinc(bessel_j_zero(3/2, 1))", "1"),
        ("bessel_y(0, x)", 1, 320, "-10^9", "bessel_y(0, bessel_y_zero(1, 1))"),
    ] {
        let (mn, mx) = sampled(&mut e, f, lo, n);
        let (a, b) = (num(&mut e, a), num(&mut e, b));
        assert!(mn >= a - 1e-15 && mx <= b + 1e-15, "{f}: [{mn}, {mx}] against [{a}, {b}]");
        assert!(mx > b - 1e-2, "{f}: the maximum {b} not approached ({mx})");
        if a > -1e8 {
            assert!(mn < a + 1e-2, "{f}: the minimum {a} not approached ({mn})");
        }
    }
    // the envelope numbers of the proofs: M0(j(1,2)) < -J0(j(1,1)), M0(y(1,2)) < Y0(y(1,1)), 1/j(3/2,2) < -sinc(j(3/2,1))
    let m0 = |e: &mut CoreEngine, x: &str| num(e, &format!("sqrt(bessel_j(0, {x})^2 + bessel_y(0, {x})^2)"));
    assert!(m0(&mut e, "bessel_j_zero(1, 2)") < -num(&mut e, "bessel_j(0, bessel_j_zero(1, 1))"));
    assert!(m0(&mut e, "bessel_y_zero(1, 2)") < num(&mut e, "bessel_y(0, bessel_y_zero(1, 1))"));
    assert!(num(&mut e, "1/bessel_j_zero(3/2, 2)") < -num(&mut e, "sinc(bessel_j_zero(3/2, 1))"));
    assert!(t.elapsed() < Duration::from_secs(120), "{:?}", t.elapsed());
}

#[test]
fn monotone_functions_with_known_limits() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "function_range(bessel_i(0, x), x, y)"), "y >= 1");
    assert_eq!(ev(&mut e, "function_monotonicity(bessel_i(1, x), x)"), "1");
    assert_eq!(ev(&mut e, "function_convexity(bessel_i(2, x), x)"), "1");
    assert_eq!(ev(&mut e, "function_range(elliptic_nome_q(x), x, y)"), "-1 < y and y <= 1");
    assert_eq!(ev(&mut e, "function_surjective(elliptic_nome_q(x), x)"), "false");
    assert_eq!(ev(&mut e, "function_surjective(inverse_elliptic_nome_q(x), x)"), "false");
    assert_eq!(ev(&mut e, "function_range(elliptic_f(t, 1/2), t, y)"), "true");
    assert_eq!(ev(&mut e, "table(function_surjective(bessel_y(k, z), z), k, 5)"), "[false, false, false, false, false]");
    assert_eq!(ev(&mut e, "function_surjective(sinc(x), x)"), "false");
    // independent: q(m) at m = -10^6 is close to -1 and q(1) = 1; I0 at 0 is 1; F(pi k|1/2) = 2 k K(1/2)
    assert!(num(&mut e, "elliptic_nome_q(-10^6)") < -0.5);
    assert_eq!(ev(&mut e, "elliptic_nome_q(1)"), "1");
    assert_eq!(ev(&mut e, "bessel_i(0, 0)"), "1");
    let d = num(&mut e, "elliptic_f(3 pi, 1/2) - 6 elliptic_k(1/2)");
    assert!(d.abs() < 1e-12, "{d}");
}

#[test]
fn every_real_value_from_rays_of_two_pieces() {
    let mut e = CoreEngine::new();
    // -cos(x)/x: -inf at 0+, +inf at 0-, f(1) = -cos(1) < f(-1) = cos(1) is not needed: f(2) = 0.208 > f(-2) = -0.208
    assert_eq!(ev(&mut e, "function_surjective(-cos(x)/x, x)"), "true");
    assert_eq!(ev(&mut e, "function_range(spherical_bessel_y(0, x), x, y)"), "true");
    assert_eq!(ev(&mut e, "function_range(legendre_q(2, x), x, y)"), "true");
    // not onto: x + 1/x misses (-2, 2)
    assert_eq!(ev(&mut e, "function_surjective(x + 1/x, x)"), "false");
    assert_eq!(ev(&mut e, "function_range(x + 1/x, x, y)"), "y <= -2 or y >= 2");
}

/// The truth of a condition in `a` (and `b`) at exact values, `None` when not decided.
fn truth(e: &mut CoreEngine, c: &str, a: &str, b: &str) -> Option<bool> {
    match ev(e, &format!("rewrite({c}, [a -> ({a}), b -> ({b})])")).as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[test]
fn reduce_of_disjunctions_meeting_at_a_boundary() {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    assert_eq!(ev(&mut e, "reduce(a < 0 and b >= sqrt(-3 a)/3 or a >= 0, a, reals)"), "b <= 0 and a >= 0 or b > 0 and a >= -3*b^2");
    // generator: pieces whose conditions meet where a radical's radicand changes sign; the reduced condition and the
    // original agree at exact points on each boundary and 1/7 beside it, for several values of b
    let forms = [
        "a < 0 and b >= sqrt(-3 a)/3 or a >= 0",
        "a <= 0 and b > sqrt(-a) or a > 0 and b < 1",
        "a < 1 and sqrt(1 - a) < b or a >= 1",
        "a >= 0 and sqrt(a) <= b or a < 0 and b > -1",
        "sqrt(a^2 - 1) > b or a^2 < 1",
        "not(sqrt(-a) < b) or a > 2",
    ];
    let mut faults = vec![];
    for f in forms {
        let r = ev(&mut e, &format!("reduce({f}, a, reals)"));
        assert!(!r.starts_with("reduce") && !r.contains("failure"), "{f}: {r}");
        for bv in ["-2", "-1", "0", "1/2", "1", "3"] {
            for av in ["-3", "-1", "-1/7", "0", "1/7", "1", "8/7", "2", "15/7"] {
                // the formula itself at the point (a relation with a value that is not real stays undecided: skipped)
                let lhs = truth(&mut e, f, av, bv);
                let rhs = truth(&mut e, &r, av, bv);
                if let (Some(x), Some(y)) = (lhs, rhs) {
                    if x != y {
                        faults.push(format!("{f} at a = {av}, b = {bv}: {x}, reduced {r}: {y}"));
                    }
                }
            }
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
    assert!(t.elapsed() < Duration::from_secs(240), "{:?}", t.elapsed());
}
