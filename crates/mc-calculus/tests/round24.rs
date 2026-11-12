//! Round 24: wrong values of the transforms outside their region of convergence, the unilateral
//! inverse Z transform, products of undefined functions, and the derivative of a pFq with list parameters.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn run(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|x| format!("error: {x}"))
}

#[test]
fn d23_14_no_value_outside_the_region_of_convergence() {
    for src in [
        "laplace_transform(exp(t), t, 0.5)",
        "laplace_transform(sinh(t)/t, t, 1/2)",
        "mellin_transform(1/(1 + x^2), x, 3)",
        "mellin_transform(1/(1 + x^2), x, -1)",
        "z_transform(2^n, n, 1.5)",
    ] {
        let got = run(src);
        assert!(got.starts_with(src.split('(').next().unwrap()), "{src} gave {got}");
    }
    // inside: the values stay
    assert_eq!(run("laplace_transform(exp(t), t, 3)"), "1/2");
    assert_eq!(run("mellin_transform(1/(1 + x^2), x, 1)"), "(1/2)*pi");
    assert_eq!(run("z_transform(2^n, n, 3)"), "3");
    assert_eq!(run("laplace_transform(sin(t)/t, t, 2)"), "atan(1/2)");
}

#[test]
fn d23_01_divergent_gaussian_has_no_transform() {
    assert!(run("laplace_transform(exp(t^2), t, s)").starts_with("laplace_transform("));
    assert!(run("laplace_transform(exp(-t^2), t, s)").contains("erfc"));
    assert!(run("laplace_transform(t^(-1/2)*exp(1/t), t, 2)").starts_with("laplace_transform("));
}

#[test]
fn d23_04_mellin_strips_of_the_table() {
    assert_eq!(
        run("mellin_transform(1/(1 + x), x, s, generate_conditions: true)"),
        "conditional_expression(pi*csc(pi*s), 0 < re(s) and re(s) < 1)"
    );
}

#[test]
fn d23_13_unilateral_inverse_z_is_zero_for_negative_n() {
    assert_eq!(run("inverse_z_transform(z/(z - 1), z, -1)"), "0");
    assert_eq!(run("inverse_z_transform(z/(z - 1), z, 2)"), "1");
}

#[test]
fn d23_15_z_transform_with_a_pole_term_at_zero_is_not_returned() {
    assert!(run("z_transform(1/n, n, z)").starts_with("z_transform("));
}

#[test]
fn d23_20_products_of_undefined_functions_stay_unevaluated() {
    for src in [
        "z_transform(f(n)*g(n), n, z)",
        "inverse_laplace_transform(F(s)*G(s), s, t)",
        "inverse_z_transform(F(z)*G(z), z, n)",
        "fourier_transform(f(t)*g(t), t, w)",
        "mellin_transform(f(x)*g(x), x, s)",
    ] {
        let got = run(src);
        assert!(!got.starts_with("error") && got.starts_with(src.split('(').next().unwrap()), "{src} gave {got}");
    }
}

#[test]
fn d23_12_d_solve_value_tan_is_finite_before_pi_over_two() {
    let v = run("n(d_solve_value([y'(x) == y(x)^2 + 1, y(0) == 0], y(1/2), x))");
    assert!(v.starts_with("0.5463024898"), "{v}");
    assert!(run("d_solve_value([y'(x) == y(x)^2 + 1, y(0) == 0], y(2), x)").starts_with("failure(blow_up"));
    assert!(run("d_solve_value([y'(x) == y(x)^2, y(0) == 1], y(2), x)").starts_with("failure(blow_up"));
}

#[test]
fn r22_03_derivative_of_pfq_and_meijer_g_with_list_parameters() {
    assert_eq!(
        run("d(hypergeometric_pfq([1/2, 2/3, 4/5], [1, 5/7], x), x)"),
        "(28/75)*hypergeometric_pfq([3/2, 5/3, 9/5], [2, 12/7], x)"
    );
    assert!(!run("d(meijer_g([[1/2, 1/3], [1/5]], [[0], [0, 2/7]], x), x)").contains("shape_mismatch"));
}

#[test]
fn x23_huge_exponents_are_declined_at_once() {
    for src in [
        "integrate(x^(2^31)*exp(-x), x)",
        "integrate(sqrt(x^(2^31)), x)",
        "fourier_transform(exp(-u^(2^31)), u, x)",
    ] {
        let t = std::time::Instant::now();
        let got = run(src);
        assert!(t.elapsed().as_secs_f64() < 2.0 && !got.starts_with("error"), "{src}: {got}");
    }
    assert_eq!(run("integrate(x^(2^31), x)"), "(1/2147483649)*x^2147483649");
}

#[test]
fn derivative_of_an_unevaluated_transform_is_not_a_false_value() {
    // it was -f(t)/s^2 (the transform treated as f(t)/s)
    for src in ["d(laplace_transform(f(t), t, s), s)", "d(laplace_transform(g(t)*exp(-s*t), t, s), s)"] {
        let got = run(src);
        assert!(!got.contains("/s^2") && !got.contains("/(2*s^2)"), "{src} gave {got}");
    }
    // a transform with a closed form still differentiates
    assert_eq!(run("d(laplace_transform(sin(t), t, s), s)"), "-2*s*(1 + s^2)^-2");
}
