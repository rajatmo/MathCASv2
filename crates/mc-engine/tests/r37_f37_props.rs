//! Round 37: the function properties of the special functions (the table of
//! `mc-calculus/src/props/table.rs`). Every test has an example input, variations of its own, and a check of the
//! answer by an independent route (numeric values at points, the derivative, a period, a witness pair).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The value as a machine number (a complex value with a zero imaginary part counts as real).
fn real(src: &str) -> Option<f64> {
    let v = ev(&format!("chop(n({src}), 1.0e-12)"));
    v.parse::<f64>().ok()
}

#[test]
fn cosine_integral_domain_composes_and_matches_the_values() {
    // documented, a composition, the plane
    assert_eq!(ev("function_domain(cos_integral(x), x)"), "x > 0");
    assert_eq!(ev("function_domain(cos_integral(x^2 - 1), x)"), "x < -1 or x > 1");
    assert_eq!(ev("function_domain(cos_integral(z), z, complexes)"), "z != 0");
    // Ci is real for x > 0 and has the imaginary part pi for x < 0 (DLMF 6.4.4)
    assert!(real("cos_integral(0.5)").is_some());
    assert!(real("cos_integral(-0.5)").is_none());
    assert!(real("cos_integral(1.5^2 - 1)").is_some() && real("cos_integral(0.5^2 - 1)").is_none());
}

#[test]
fn ranges_from_the_table_with_affine_maps() {
    assert_eq!(ev("function_range(gudermannian(x), x, y)"), "(-1/2)*pi < y and y < (1/2)*pi");
    assert_eq!(ev("function_range(2 * gudermannian(3 * x) + 1, x, y)"), "1 - pi < y and y < 1 + pi");
    assert_eq!(ev("function_range(sech(x), x, y)"), "0 < y and y <= 1");
    assert_eq!(ev("function_range(coth(x), x, y)"), "y < -1 or y > 1");
    // the bounds are approached: gd(20) is pi/2 to 17 digits, sech(0) = 1, coth(1/1000) is large
    let g = real("gudermannian(40.)").unwrap();
    assert!((g - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
    assert_eq!(real("sech(0.)"), Some(1.0));
    assert!(real("coth(0.001)").unwrap() > 999.0);
}

#[test]
fn polylogarithm_range_is_bounded_by_zeta() {
    assert_eq!(ev("function_range(polylog(2, x), x, y)"), "y <= (1/6)*pi^2");
    assert_eq!(ev("function_range(polylog(3, x), x, y)"), "y <= zeta(3)");
    // Li_2(1) = zeta(2) is taken; Li_3 increases (its derivative Li_2(x)/x > 0 at a sample)
    let z2 = real("polylog(2, 1)").unwrap();
    assert!((z2 - std::f64::consts::PI.powi(2) / 6.0).abs() < 1e-14);
    assert!(real("polylog(2, 0.4) / 0.4").unwrap() > 0.0);
}

#[test]
fn bounded_special_functions_are_not_onto() {
    assert_eq!(ev("function_surjective(airy_ai(x), x)"), "false");
    assert_eq!(ev("function_surjective(dawson_f(2 * x + 1), x)"), "false");
    assert_eq!(ev("function_surjective(airy_ai_prime(x), x)"), "true");
    // |Ai| < 1 at samples; Ai' oscillates with growing amplitude (|Ai'(-1000)| reaches ~ 1000^(1/4)/sqrt(pi))
    for x in ["-20.3", "-1.02", "0.", "3.7"] {
        assert!(real(&format!("airy_ai({x})")).unwrap().abs() < 1.0);
    }
    let big = (0..40).map(|k| real(&format!("airy_ai_prime(-1000. - {k}/10)")).unwrap().abs()).fold(0.0, f64::max);
    assert!(big > 2.0);
}

#[test]
fn jacobi_poles_jointly_and_for_a_number() {
    assert_eq!(ev("function_singularities(jacobi_sc(x, m), [x, m])"), "m < 1 and cos((1/2)*pi*x/elliptic_k(m)) == 0");
    assert_eq!(ev("function_singularities(jacobi_sc(x, 1 / 2), x)"), "cos((1/2)*pi*x/elliptic_k(1/2)) == 0");
    assert_eq!(ev("function_analytic(jacobi_sc(x, m), x, assumptions: m >= 1)"), "true");
    // the pole is a zero of cn: cn(K(1/2) | 1/2) = 0 to 30 digits
    assert_eq!(ev("chop(n(jacobi_cn(elliptic_k(1 / 2), 1 / 2), 30))"), "0");
}

#[test]
fn regularized_gamma_is_entire_for_an_integer_order() {
    assert_eq!(ev("function_analytic(gamma_regularized(a, x), x, assumptions: element(a, integers) and a > 0)"), "true");
    assert_eq!(ev("function_analytic(gamma_regularized(1 / 2, x), x)"), "false");
    assert_eq!(ev("function_analytic(gamma_regularized(-2, x), x)"), "true");
    // Q(3, x) = e^-x (1 + x + x^2/2) is real for x < 0; Q(1/2, x) = erfc(sqrt(x)) is not
    let q = real("gamma_regularized(3, -1.5)").unwrap();
    assert!((q - (1.5f64).exp() * (1.0 - 1.5 + 1.125)).abs() < 1e-12);
    assert!(real("gamma_regularized(1/2, -1.5)").is_none());
}

#[test]
fn inverse_jacobi_domain_is_the_range_of_the_function() {
    assert_eq!(ev("function_domain(inverse_jacobi_sc(x, 3), x)"), "(-1/2)*2^(1/2) <= x and x <= (1/2)*2^(1/2)");
    assert_eq!(ev("function_domain(inverse_jacobi_sc(x, 1 / 3), x)"), "true");
    assert_eq!(ev("function_domain(inverse_jacobi_sn(x, 3), x)"), "(-1/3)*3^(1/2) <= x and x <= (1/3)*3^(1/2)");
    // values: real inside, complex beyond; sc(x|3) never exceeds 1/sqrt(2) (Jacobi's real transformation)
    assert!(real("inverse_jacobi_sc(0.6, 3)").is_some() && real("inverse_jacobi_sc(0.8, 3)").is_none());
    assert!(real("inverse_jacobi_sn(0.5, 3)").is_some() && real("inverse_jacobi_sn(0.6, 3)").is_none());
    let peak = (0..200).map(|k| real(&format!("jacobi_sc({k}/100, 3)")).unwrap_or(0.0).abs()).fold(0.0, f64::max);
    assert!(peak <= 0.5f64.sqrt() + 1e-12);
}

#[test]
fn elliptic_integrals_monotonic_with_the_derivative() {
    assert_eq!(ev("function_monotonicity([elliptic_k(m), m < 1], m)"), "1");
    assert_eq!(ev("function_monotonicity([elliptic_e(m), m < 1], m)"), "-1");
    assert_eq!(ev("function_monotonicity([product_log(x), x >= -1 / e], x, strict_inequalities: true)"), "1");
    // the derivatives (E - (1 - m) K)/(2 m (1 - m)) and (E - K)/(2 m) at m = 0.3 and W'(x) = W/(x (1 + W)) at 1
    assert!(real("rewrite(d(elliptic_k(m), m), m -> 0.3)").unwrap() > 0.0);
    assert!(real("rewrite(d(elliptic_e(m), m), m -> 0.3)").unwrap() < 0.0);
    assert!(real("rewrite(d(product_log(x), x), x -> 1.)").unwrap() > 0.0);
}

#[test]
fn periodic_jacobi_functions_are_not_injective() {
    assert_eq!(ev("function_injective(jacobi_cn(x, m), x, assumptions: m < 1)"), "false");
    assert_eq!(ev("function_injective(jacobi_sn(x, m), x, assumptions: m > 1)"), "false");
    assert_eq!(ev("function_surjective(jacobi_sc(x, m), x, assumptions: m <= 1)"), "true");
    assert_eq!(ev("function_surjective(jacobi_sc(x, m), x, assumptions: m > 1)"), "false");
    // the period 4 K(m): cn(0.7 + 4 K(1/3) | 1/3) = cn(0.7 | 1/3) to 30 digits
    assert_eq!(ev("chop(n(jacobi_cn(0.7 + 4 * elliptic_k(1 / 3), 1 / 3) - jacobi_cn(0.7, 1 / 3), 30))"), "0");
}

#[test]
fn several_variables_sign_and_convexity_by_witnesses() {
    assert_eq!(ev("function_sign(jacobi_cn(x, m), [x, m])"), "indeterminate");
    assert_eq!(ev("function_convexity(jacobi_sn(x, m), [x, m])"), "indeterminate");
    assert_eq!(ev("function_sign(jacobi_cn(x, m), x, assumptions: element(m, positive_integers))"), "1");
    // witnesses: cn(2 | 0.4) < 0 < cn(0.1 | 0.4); cn(x | 3) = dn(sqrt(3) x | 1/3) > 0
    assert!(real("jacobi_cn(2.0, 0.4)").unwrap() < 0.0);
    assert!(real("jacobi_cn(0.1, 0.4)").unwrap() > 0.0);
    assert!((0..60).all(|k| real(&format!("jacobi_cn({k}/7, 3)")).unwrap() > 0.0));
}

#[test]
fn complex_ranges() {
    assert_eq!(ev("function_range(sin(z), z, y, complexes)"), "true");
    assert_eq!(ev("function_range(chebyshev_t(2, z), z, y, complexes)"), "true");
    assert_eq!(ev("function_range(exp(2 * z + 1), z, y, complexes)"), "y != 0");
    // sin takes 5 at asin(5); e^(2 z + 1) takes -3 at (log(-3) - 1)/2
    assert!((real("sin(asin(5.))").unwrap() - 5.0).abs() < 1e-12);
    assert!((real("exp(2 * (log(-3.) - 1)/2 + 1)").unwrap() + 3.0).abs() < 1e-12);
}

#[test]
fn hypergeometric_series_by_their_order() {
    assert_eq!(ev("function_analytic(hypergeometric_pfq([1, 1, 2], [3, 3], z), z)"), "false");
    assert_eq!(ev("function_analytic(hypergeometric_pfq([1, 1], [3, 3, 3], z), z)"), "true");
    assert_eq!(ev("function_domain(hypergeometric_2f1(1 / 2, 1 / 3, 1 / 4, x), x)"), "x < 1");
    assert_eq!(ev("function_domain(hypergeometric_2f1(1 / 2, 1 / 3, 3, x), x)"), "x <= 1");
    // beyond 1 the value is complex; at 1 Gauss's sum Gamma(c) Gamma(c - a - b)/(Gamma(c - a) Gamma(c - b)) for c = 3
    assert!(real("hypergeometric_2f1(1 / 2, 1 / 3, 1 / 4, 1.5)").is_none());
    let gauss = real("gamma(3) * gamma(13/6) / (gamma(5/2) * gamma(8/3))").unwrap();
    assert!((real("hypergeometric_2f1(1/2, 1/3, 3, 0.999999999)").unwrap() - gauss).abs() < 1e-6);
}

#[test]
fn kelvin_functions_at_zero() {
    assert_eq!(ev("function_domain(kelvin_kei(0, x), x)"), "x >= 0");
    assert_eq!(ev("function_domain(kelvin_ker(0, x), x)"), "x > 0");
    assert_eq!(ev("function_domain(kelvin_ker(-1 / 2, x), x)"), "x > 0");
    assert_eq!(ev("function_domain(kelvin_ber(-1 / 2, x), x)"), "x > 0");
    // kei_0(0) = -pi/4, ker_0 grows like -log(x/2)
    assert_eq!(ev("kelvin_kei(0, 0)"), "(-1/4)*pi");
    assert!(real("kelvin_ker(0, 0.001)").unwrap() > 7.0);
    assert!(real("kelvin_ber(-1/2, 0.0001)").unwrap().abs() > 10.0);
}

#[test]
fn steps_and_gaps_by_the_samples_of_the_function() {
    assert_eq!(ev("function_injective(cantor_staircase(x), x)"), "false");
    assert_eq!(ev("function_monotonicity(cantor_staircase(x), x)"), "1");
    assert_eq!(ev("function_monotonicity([inverse_jacobi_ns(x, 1 / 3), x <= -1 or x >= 1], x)"), "indeterminate");
    assert_eq!(ev("function_monotonicity(acoth(x), x)"), "indeterminate");
    // a flat piece of the Cantor function; ns^-1 decreases on each piece but f(-1.5) < f(1.5)
    assert_eq!(ev("n(cantor_staircase(0.4)) == n(cantor_staircase(0.5))"), "true");
    assert!(real("inverse_jacobi_ns(-1.5, 1/3)").unwrap() < real("inverse_jacobi_ns(1.5, 1/3)").unwrap());
    assert!(real("inverse_jacobi_ns(2.5, 1/3)").unwrap() < real("inverse_jacobi_ns(1.5, 1/3)").unwrap());
}

#[test]
fn scorer_hi_positive_increasing_convex() {
    assert_eq!(ev("function_monotonicity(scorer_hi(x), x)"), "1");
    assert_eq!(ev("function_convexity(scorer_hi(x), x)"), "1");
    assert_eq!(ev("function_sign(scorer_hi_prime(x), x)"), "1");
    // Hi = (1/pi) int_0^inf exp(-t^3/3 + x t) dt: samples increase with increasing slopes
    let v: Vec<f64> = ["-3.", "0.", "2.", "4."].iter().map(|x| real(&format!("scorer_hi({x})")).unwrap()).collect();
    assert!(v.windows(2).all(|w| w[1] > w[0] && w[0] > 0.0));
    assert!((v[2] - v[1]) / 2.0 > (v[1] - v[0]) / 3.0);
}

#[test]
fn parameter_conditions_in_the_templates() {
    assert_eq!(ev("function_domain(gamma(z, a), [z, a], complexes)"), "a != 0 or re(z) > 0");
    assert_eq!(ev("function_domain(beta(a, b), [a, b])"), "(a > 0 or not_element(a, integers)) and (b > 0 or not_element(b, integers))");
    assert_eq!(ev("function_monotonicity(surd(x, 2 * n_u), x, nonnegative_reals, assumptions: element(n_u, integers) and n_u > 0)"), "1");
    assert_eq!(ev("function_range(surd(x, -3), x, y)"), "y < 0 or y > 0");
    // Gamma(1/2, 0) = sqrt(pi); beta(-1, 1/2) is not finite; surd(x, 4) increases at samples
    assert!((real("gamma(1/2, 0)").unwrap() - std::f64::consts::PI.sqrt()).abs() < 1e-14);
    assert!(real("beta(-1, 1/2)").is_none());
    assert!(real("surd(2., 4)").unwrap() > real("surd(1.5, 4)").unwrap());
}

#[test]
fn non_real_arguments_and_partial_singular_sets() {
    // re(J(x + i)) is real, but J's facts are about a real argument: not decided
    assert!(ev("function_sign(re(klein_invariant_j(x + im)), x)").starts_with("function_sign("));
    // the pole of p at 0 decides analyticity; the other lattice points are not listed, so no singular set
    assert_eq!(ev("function_analytic(weierstrass_p(x, [g1, g2]), x, assumptions: element([g1, g2], reals))"), "false");
    assert_eq!(ev("function_singularities(weierstrass_p(x, [1, 2]), x)"), "function_singularities(weierstrass_p(x, [1, 2]), x)");
    assert_eq!(ev("function_analytic(weierstrass_sigma(x, [1, 2]), x)"), "true");
    // p(x) ~ 1/x^2 near 0
    assert!(real("weierstrass_p(0.001, [1, 2])").unwrap() > 9.9e5);
}
