//! Phase 6 (package p6-numeric): values at infinities and signed poles, the generalized arities, and the functions
//! defined by formulas (combinatorial functions, orthogonal polynomials, Lambert W, scaled Bessel functions, ...).
mod common;
use common::check;

#[test]
fn infinities_and_poles() {
    check(&[
        ("erf(inf)", "1"),
        ("erfc(-inf)", "2"),
        ("erfi(im*inf)", "1im"),
        ("exp_integral_ei(im*inf)", "1im*pi"),
        ("zeta(inf)", "1"),
        ("cos_integral(0)", "-inf"),
        ("inverse_erf(-1)", "-inf"),
        ("log_gamma(-2)", "inf"),
        ("polylog(1, 1)", "inf"),
        ("gamma(0)", "failure(pole, gamma(0))"),
        ("erf(-inf, inf)", "2"),
    ]);
}

#[test]
fn combinatorial() {
    check(&[
        ("fibonacci(10)", "55"),
        ("fibonacci(-6)", "-8"),
        ("fibonacci(5, x)", "1 + 3*x^2 + x^4"),
        ("lucas_l(10)", "123"),
        ("pochhammer(a, 3)", "a*(1 + a)*(2 + a)"),
        ("pochhammer(1/2, 2)", "3/4"),
        ("harmonic_number(10)", "7381/2520"),
        ("harmonic_number(3, 2)", "49/36"),
        ("factorial2(7)", "105"),
        ("factorial2(8)", "384"),
        ("subfactorial(5)", "44"),
        ("multinomial(1, 2, 3)", "60"),
        ("hyperfactorial(3)", "108"),
    ]);
}

#[test]
fn orthogonal_polynomials() {
    check(&[
        ("chebyshev_t(3, x)", "-3*x + 4*x^3"),
        ("chebyshev_u(3, x)", "-4*x + 8*x^3"),
        ("chebyshev_t(3, 0.3)", "-0.792"),
        ("legendre_p(3, x)", "(-3/2)*x + (5/2)*x^3"),
        ("legendre_p(2, 1, x)", "-3*x*(1 - x^2)^(1/2)"),
        ("legendre_q(1, x)", "-1 + (1/2)*x*(log(1 + x) - log(1 - x))"),
        ("hermite_h(3, x)", "-12*x + 8*x^3"),
        ("laguerre_l(3, x)", "1 + -3*x + (-1/6)*x^3 + (3/2)*x^2"),
        ("zernike_r(4, 2, r)", "-3*r^2 + 4*r^4"),
        ("chebyshev_t(n, x)", "chebyshev_t(n, x)"),
    ]);
}

#[test]
fn numeric_values() {
    let mut s = common::Sys::new();
    let close = |got: &str, want: f64| {
        let v = common::number(got);
        assert!((v - want).abs() <= 1e-12 * want.abs().max(1.0), "{got} vs {want}");
    };
    close(&s.run("fibonacci(2.5)"), 1.4893065462657091);
    close(&s.run("legendre_p(1/2, 0.3)"), 0.7009385309696553);
    close(&s.run("product_log(1.)"), 0.5671432904097838);
    close(&s.run("product_log(-1, -0.2)"), -2.5426413577735265);
    close(&s.run("dawson_f(1.)"), 0.5380795069127684);
    close(&s.run("fresnel_s(1.)"), 0.4382591473903548);
    close(&s.run("spherical_bessel_j(2, 1.5)"), 0.1273492836884083);
    // queue G14: no underflow in the ratio of large-argument K
    close(&s.run("bessel_k_scaled(1, 800.0)/bessel_k_scaled(2, 800.0)"), 0.9981279260279697);
    close(&s.run("bessel_k_scaled(0, 2.0)"), 0.8415682150707714);
}
