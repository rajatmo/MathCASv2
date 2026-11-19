//! The real domain of the Gauss hypergeometric function in its argument, with symbolic parameters, checked point by
//! point against the evaluator: a point is in the domain exactly when the value at 30 digits is a finite real
//! number. Beyond the cut z > 1 the function is real when Euler's transformation (DLMF 15.8.1) makes it the rational
//! function (1 - z)^(c - a - b) 2F1(c - a, c - b; c; z) with a terminating second factor.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

/// `src` evaluated within 30 s (asserted).
fn ev(e: &mut CoreEngine, src: &str) -> String {
    let t0 = Instant::now();
    let out = e
        .eval(&format!("time_constrained({{ {src} }}, 30)"), &StopFlag::new())
        .map(|r| r.printed)
        .unwrap_or_else(|err| format!("error: {err:?}"));
    assert!(t0.elapsed() < Duration::from_secs(35), "{src}: {:?}", t0.elapsed());
    out
}

/// A printed finite real number (a complex value, an infinity, a failure or an unevaluated call is not one).
fn finite_real(s: &str) -> bool {
    s.trim().parse::<f64>().is_ok_and(f64::is_finite)
}

/// (a, b, c, z, in the domain)
const POINTS: &[(&str, &str, &str, &str, bool)] = &[
    // z = 0: the first term, whatever c (c = 0 included: the value 1 the evaluator gives)
    ("1/2", "1", "-1", "0", true),
    ("1", "1", "0", "0", true),
    ("-3", "1", "-2", "0", true),
    // c a pole the series reaches
    ("1/2", "1", "-1", "1/2", false),
    ("-3", "1", "-2", "1/2", false),
    ("-2", "1/3", "-1", "1/2", false),
    ("1", "1", "-1", "2", false),
    ("2", "1", "0", "3", false),
    // c a pole the series stops before: a polynomial
    ("-1", "1", "-2", "3", true),
    ("1", "-2", "-2", "5", true),
    // a or b 0, -1, ...: a polynomial on the whole line
    ("-2", "1/3", "3", "5", true),
    ("1/3", "-1", "2", "7", true),
    ("0", "1/2", "1/3", "4", true),
    // z < 1
    ("1/2", "1/3", "1/4", "1/2", true),
    ("1/2", "1/3", "1/4", "-5", true),
    ("2", "3", "1/2", "-1", true),
    ("5/2", "1", "-5/2", "-1", true),
    // z = 1: Gauss's sum for c - a - b > 0, divergent otherwise
    ("1/2", "1/3", "3", "1", true),
    ("1", "2", "4", "1", true),
    ("1/2", "1/3", "1/4", "1", false),
    ("1", "1", "2", "1", false),
    // z = 1 with Euler's terminating factor and c - a - b < 0: a pole
    ("1", "1/2", "1/2", "1", false),
    ("5/2", "1", "-5/2", "1", false),
    // z > 1 without a terminating Euler factor: complex
    ("1/2", "1/3", "1/4", "3/2", false),
    ("1", "2", "1/2", "2", false),
    ("1/3", "1/2", "4/3", "3", false),
    ("1", "1", "2", "3", false),
    ("1/2", "1/2", "3", "3", false),
    // c - a or c - b positive, c - a - b an integer
    ("1/3", "2", "4/3", "3", false),
    // c - a or c - b 0, -1, ... but c - a - b not an integer
    ("2", "1/3", "1", "3", false),
    ("1/2", "1/3", "1/2", "2", false),
    ("1/2", "7/2", "5/2", "2", false),
    // c - a = 0, -1, ... and c - a - b an integer: real beyond the cut
    ("1", "1/2", "1/2", "3/2", true),
    ("1/2", "1", "1/2", "3", true),
    ("5/2", "1", "-5/2", "3/2", true),
    ("5/2", "1", "-5/2", "3", true),
    ("1/3", "2", "1/3", "4", true),
    ("1/3", "2", "-2/3", "5", true),
    ("5/2", "1", "1/2", "2", true),
    // c - b = 0, -1, ...
    ("3", "1/4", "1/4", "2", true),
    ("2", "7/2", "5/2", "2", true),
    ("3/2", "5", "1/2", "3", true),
    ("3/2", "1", "1/2", "7/4", true),
];

#[test]
fn real_domain_agrees_with_the_values() {
    let mut e = CoreEngine::new();
    let d = ev(&mut e, "function_domain(hypergeometric_2f1(a, b, c, z), z)");
    assert!(POINTS.len() >= 40);
    for &(a, b, c, z, want) in POINTS {
        let inside = ev(&mut e, &format!("rewrite({d}, [a -> {a}, b -> {b}, c -> {c}, z -> {z}])"));
        assert_eq!(inside, want.to_string(), "domain at {a}, {b}, {c}, {z}: {d}");
        let v = ev(&mut e, &format!("n(hypergeometric_2f1({a}, {b}, {c}, {z}), 30)"));
        assert_eq!(finite_real(&v), want, "value at {a}, {b}, {c}, {z}: {v}");
        // the same from a 30-digit argument (the numerical route, not the closed form)
        if z != "0" && z != "1" {
            let v = ev(&mut e, &format!("hypergeometric_2f1({a}, {b}, {c}, n({z}, 30))"));
            assert_eq!(finite_real(&v), want, "value at {a}, {b}, {c}, n({z}, 30): {v}");
        }
    }
}

#[test]
fn real_values_beyond_the_cut_by_their_closed_forms() {
    let mut e = CoreEngine::new();
    // 2F1(a, b; b; z) = (1 - z)^-a (DLMF 15.4.6): -2 at 3/2 and -1/2 at 3
    assert_eq!(ev(&mut e, "hypergeometric_2f1(1, 1/2, 1/2, 3/2)"), "-2");
    assert_eq!(ev(&mut e, "hypergeometric_2f1(1/2, 1, 1/2, 3)"), "-1/2");
    // Euler: (1 - z)^-6 sum_(k <= 5) (-5)_k (-7/2)_k/((-5/2)_k k!) z^k
    let closed = "(1 - z)^-6*sum(pochhammer(-5, k)*pochhammer(-7/2, k)/(pochhammer(-5/2, k)*factorial(k))*z^k, [k, 0, 5])";
    assert_eq!(ev(&mut e, &format!("rewrite({closed}, z -> 3/2)")), "-22574");
    assert_eq!(ev(&mut e, "hypergeometric_2f1(5/2, 1, -5/2, 3/2)"), "-22574");
    // z = 1: Gauss's sum, Chu-Vandermonde for a terminating series ((c - b)_n/(c)_n, DLMF 15.4.24), unbounded when
    // c - a - b <= 0
    assert_eq!(ev(&mut e, "hypergeometric_2f1(-1, 1, 1, 1)"), "0");
    assert_eq!(ev(&mut e, "hypergeometric_2f1(-2, 1/3, 3, 1)"), "22/27");
    for p in ["1, 1, 2", "1/2, 1/3, 1/4", "5/2, 1, -5/2"] {
        assert_eq!(ev(&mut e, &format!("hypergeometric_2f1({p}, 1)")), "complex_infinity", "{p}");
    }
    // at 30 digits, numerically from a 30-digit argument, against the closed forms
    for (f, z, g) in [
        ("hypergeometric_2f1(1, 1/2, 1/2, z)", "3/2", "1/(1 - z)"),
        ("hypergeometric_2f1(1/2, 1, 1/2, z)", "3", "1/(1 - z)"),
        ("hypergeometric_2f1(5/2, 1, -5/2, z)", "3/2", closed),
        ("hypergeometric_2f1(5/2, 1, -5/2, z)", "31/20", closed),
        ("hypergeometric_2f1(1/3, 2, -2/3, z)", "5", "(1 - z)^-3*(1 - 4*z)"),
    ] {
        let v = ev(&mut e, &format!("rewrite({f}, z -> n({z}, 30))"));
        assert!(finite_real(&v), "{f} at {z}: {v}");
        let err = ev(&mut e, &format!("abs(rewrite({f}, z -> n({z}, 30)) - rewrite({g}, z -> {z})) < 10^-25*abs(rewrite({g}, z -> {z}))"));
        assert_eq!(err, "true", "{f} at {z}: {v}");
        // machine precision too: real, relative error near rounding
        let m = ev(&mut e, &format!("rewrite({f}, z -> n({z}))"));
        assert!(finite_real(&m), "{f} at {z}: {m}");
        let mf: f64 = m.parse().unwrap();
        let exact: f64 = ev(&mut e, &format!("n(rewrite({g}, z -> {z}))")).parse().unwrap();
        assert!((mf - exact).abs() <= 1e-13 * exact.abs(), "{f} at {z}: {m} vs {exact}");
    }
}

#[test]
fn real_singular_set_leaves_the_rational_cases() {
    let mut e = CoreEngine::new();
    let s = ev(&mut e, "function_singularities(hypergeometric_2f1(a, b, c, z), z)");
    for (a, b, c, z, want) in [
        ("1", "1/2", "1/2", "3/2", "false"),
        ("1", "1/2", "1/2", "1", "true"),
        ("1/2", "1/3", "1/4", "3/2", "true"),
        ("1/2", "1/3", "1/4", "1", "true"),
        ("1/2", "1/3", "1/4", "1/2", "false"),
        ("5/2", "1", "-5/2", "2", "false"),
        ("2", "1/3", "1", "3", "true"),
    ] {
        assert_eq!(ev(&mut e, &format!("rewrite({s}, [a -> {a}, b -> {b}, c -> {c}, z -> {z}])")), want, "{s} at {a}, {b}, {c}, {z}");
    }
    // the complex domain is unchanged; the cut is no discontinuity in the rational cases
    let sc = ev(&mut e, "function_singularities(hypergeometric_2f1(a, b, c, z), z, complexes)");
    assert_eq!(ev(&mut e, &format!("rewrite({sc}, [a -> 1, b -> 1/2, c -> 1/2, z -> 3])")), "false");
    assert_eq!(ev(&mut e, &format!("rewrite({sc}, [a -> 1, b -> 1/2, c -> 1/2, z -> 1])")), "true");
    assert_eq!(ev(&mut e, &format!("rewrite({sc}, [a -> 1/2, b -> 1/3, c -> 1/4, z -> 3])")), "true");
}
