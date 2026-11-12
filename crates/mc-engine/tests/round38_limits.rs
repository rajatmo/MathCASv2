//! Round 38: limits of special functions as the argument tends to an infinity, from their leading asymptotic
//! terms. Each test has an example input, variations, and numeric values at large arguments as the check.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn real(src: &str) -> f64 {
    let v = ev(&format!("re(n({src}))"));
    v.parse::<f64>().unwrap_or_else(|_| panic!("{src}: {v}"))
}

fn absval(src: &str) -> f64 {
    real(&format!("abs({src})"))
}

#[test]
fn struve_h_at_infinity_by_the_order() {
    assert_eq!(ev("limit([struve_h(0, x), struve_h(1, x), struve_h(2, x)], x -> inf)"), "[0, 2/pi, inf]");
    // variations: a fractional order on each side of 1, a symbolic order declines
    assert_eq!(ev("limit(struve_h(1/3, x), x -> inf)"), "0");
    assert_eq!(ev("limit(struve_h(5/2, x), x -> inf)"), "inf");
    assert_eq!(ev("limit(struve_h(nu, x), x -> inf)"), "limit(struve_h(nu, x), x -> inf)");
    // H_1(x) - 2/pi -> 0 like Y_1; H_(5/2) grows like x^(3/2)
    assert!((real("struve_h(1, 4000.)") - 2.0 / std::f64::consts::PI).abs() < 0.02);
    assert!(real("struve_h(5/2, 400.)") > 100.0 && real("struve_h(1/3, 4000.)").abs() < 0.05);
}

#[test]
fn decaying_special_functions_at_infinity() {
    assert_eq!(ev("limit(spherical_hankel_h1(n_u, x), x -> inf)"), "0");
    assert_eq!(ev("limit(weber_e(nu, x), x -> inf)"), "0");
    assert_eq!(ev("limit(anger_j(nu, x), x -> inf)"), "0");
    assert_eq!(ev("limit(scorer_gi_prime(x), x -> inf)"), "0");
    assert_eq!(ev("limit(parabolic_cylinder_d(n_u, x), x -> inf)"), "0");
    // variations: the other side, other orders
    assert_eq!(ev("limit(scorer_hi(x), x -> -inf)"), "0");
    assert_eq!(ev("limit(spherical_hankel_h2(3, 2 * x), x -> inf)"), "0");
    for (f, tol) in [("spherical_hankel_h1(2, 2000.)", 1e-3), ("weber_e(3/4, 400.)", 0.05), ("anger_j(1/3, 400.)", 0.05), ("scorer_hi(-500.)", 1e-2), ("parabolic_cylinder_d(5/2, 12.)", 1e-10)] {
        assert!(absval(f) < tol, "{f}");
    }
}

#[test]
fn growing_special_functions_and_zeros() {
    assert_eq!(ev("limit(scorer_hi(x), x -> inf)"), "inf");
    assert_eq!(ev("limit(subfactorial(x), x -> inf)"), "inf");
    assert_eq!(ev("limit(airy_ai_zero(x), x -> inf)"), "-inf");
    assert_eq!(ev("limit(bessel_j_zero(2, x), x -> inf)"), "inf");
    assert_eq!(ev("limit(bessel_y_zero(2, x), x -> inf)"), "inf");
    // the zeros: a_k ~ -(3 pi k/2)^(2/3), j_(2,k) ~ (k + 3/4) pi
    let a = real("airy_ai_zero(200)");
    assert!((a / -(1.5 * std::f64::consts::PI * 200.0).powf(2.0 / 3.0) - 1.0).abs() < 1e-3);
    let j = real("bessel_j_zero(2, 100)");
    assert!((j / (100.75 * std::f64::consts::PI) - 1.0).abs() < 1e-3);
    assert!(real("subfactorial(30.)") > 1e30 && real("scorer_hi(20.)") > 1e20);
}

#[test]
fn confluent_hypergeometric_at_infinity_by_the_sign_of_the_gamma_factor() {
    assert_eq!(ev("limit(hypergeometric_1f1(1/2, 2^(1/2), x), x -> inf)"), "inf");
    assert_eq!(ev("limit(hypergeometric_1f1_regularized(1/2, 2^(1/2), x), x -> inf)"), "inf");
    // variations: Gamma(a) < 0 for -1 < a < 0 (minus infinity), a polynomial case, a symbolic parameter
    assert_eq!(ev("limit(hypergeometric_1f1(-1/2, 2, x), x -> inf)"), "-inf");
    assert_eq!(ev("limit(hypergeometric_1f1(-2, 3, x), x -> inf)"), "inf");
    assert_eq!(ev("limit(hypergeometric_1f1(a, 2, x), x -> inf)"), "limit(hypergeometric_1f1(a, 2, x), x -> inf)");
    assert!(real("hypergeometric_1f1(-1/2, 2, 60.)") < -1e20);
    assert!(real("hypergeometric_1f1(1/2, 2^(1/2), 60.)") > 1e20);
}
