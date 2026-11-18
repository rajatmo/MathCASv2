//! Machine numbers beside complex closed numeric factors, function_expand of the Kelvin functions of half-integer
//! order, the confluent hypergeometric functions at infinity, the range of Dawson's integral, and erf and gamma at
//! the imaginary infinities. Every answer is checked against an independent evaluation.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn is_true(e: &mut CoreEngine, src: &str) {
    assert_eq!(eval(e, src), "true", "{src}");
}

/// A product or sum with a machine real is a machine number when its other factors are closed numeric expressions,
/// complex ones included (`exp(im*pi/6)`, `gamma(8 - 3im)`) and `degree`; an exact complex number keeps its exact
/// arithmetic, and a symbol stays.
#[test]
fn machine_reals_make_closed_numeric_factors_numbers() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("is_number(2.5*exp(im*30*degree))", "true"),
        ("is_number(10000.0*cos(50*degree))", "true"),
        ("is_number((1.0 + im)/gamma(8 - 3im))", "true"),
        ("1.0*degree", "0.017453292519943295"),
        ("2.0*f(1)", "2.0*f(1)"),
        ("is_number(2*exp(im))", "false"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
    // the values agree with the exact expressions evaluated by n
    is_true(&mut e, "abs(2.5*exp(im*30*degree) - n(5/2*exp(im*pi/6))) < 1e-14");
    is_true(&mut e, "abs((1.0 + im)/gamma(8 - 3im) - n((1 + im)/gamma(8 - 3im))) < 1e-17");
    is_true(&mut e, "abs(10000.0*cos(50*degree) - n(10000*cos(5*pi/18))) < 1e-10");
}

/// function_expand of ber, bei, ker, kei at half-integer orders: elementary, and equal to the functions at 30 digits
/// at points on both sides of the cut.
#[test]
fn kelvin_functions_of_half_integer_order_expand() {
    let mut e = CoreEngine::new();
    assert_eq!(eval(&mut e, "free_of(function_expand(kelvin_kei(1/2, x)), kelvin_kei)"), "true");
    is_true(
        &mut e,
        "max(flatten(table(abs(n(rewrite(function_expand(f(nu, x)), x -> z) - f(nu, z), 30)), f, [kelvin_ber, kelvin_bei, kelvin_ker, kelvin_kei], nu, [-5/2, -1/2, 1/2, 3/2], z, [7/10, 3, 2 - 3/2*im]))) < 1e-20",
    );
    is_true(
        &mut e,
        "max(flatten(table(abs(n(rewrite(function_expand(f(nu, x)), x -> z) - f(nu, z))), f, [kelvin_ber, kelvin_bei, kelvin_ker, kelvin_kei], nu, [-1/2, 1/2], z, [-0.6 + 0.4im, -2.0 - 0.1im]))) < 1e-12",
    );
}

/// The modulus of 0F1 and 1F1 (a known, not a nonpositive integer) grows without bound at inf for any other
/// parameter: complex_infinity; with a parameter that may make it constant the limit stays.
#[test]
fn confluent_functions_at_infinity() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("limit(hypergeometric_0f1(a, x), x -> inf)", "complex_infinity"),
        ("limit(hypergeometric_0f1_regularized(a, x), x -> inf)", "complex_infinity"),
        ("limit(hypergeometric_1f1(1/2, b, x), x -> inf)", "complex_infinity"),
        ("limit(hypergeometric_1f1_regularized(1, b, x), x -> inf)", "complex_infinity"),
        ("limit(hypergeometric_1f1(a, b, x), x -> inf)", "limit(hypergeometric_1f1(a, b, x), x -> inf)"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
    // the growth: 1F1(1/2, 3/2 + i, x) at x = 40 is already above 10^14 in modulus
    is_true(&mut e, "abs(n(hypergeometric_1f1(1/2, 3/2 + im, 40))) > 10^14");
}

/// Dawson's integral takes exactly the values between -F(c) and F(c), c its one critical point on x > 0.
#[test]
fn range_of_dawsons_integral() {
    let mut e = CoreEngine::new();
    let r = eval(&mut e, "function_range(dawson_f(x), x, y)");
    assert!(r.contains("root(") && r.contains("<= y"), "{r}");
    // the bound is F at the zero of F' = 1 - 2 x F, found independently by find_root
    assert!(r.contains("0.54104422463518"), "{r}");
    eval(&mut e, "c = rewrite(x, find_root(1 - 2*x*dawson_f(x) == 0, [x, 0.9]))");
    is_true(&mut e, "abs(n(root([t => dawson_f(1/(2*t)) - t, 0.541])) - dawson_f(c)) < 1e-12");
}

/// erf(+-i inf) = +-i inf, gamma(+-i inf) = 0, gamma(-inf) has no limit.
#[test]
fn erf_and_gamma_at_imaginary_infinities() {
    let mut e = CoreEngine::new();
    is_true(&mut e, "erf([inf, -inf, im*inf, -im*inf]) === [1, -1, im*inf, -im*inf]");
    is_true(&mut e, "gamma([-inf, inf, im*inf, -im*inf]) === [indeterminate, inf, 0, 0]");
    // |gamma(i y)| = sqrt(pi/(y sinh(pi y))) (DLMF 5.4.3) is below 1e-30 at y = 45
    is_true(&mut e, "abs(n(gamma(45*im))) < 1e-30");
}
