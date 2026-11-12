//! Round 22: the Meijer G stage of definite integrals, inverse Mellin transforms of gamma ratios,
//! Mellin transforms of Bessel products, and the transform bugs D21-09, D21-12, D21-14, D21-15, D21-16 and
//! ui B21-2, B21-5 (`kronecker_delta`). Values are checked against numeric integrals or identities.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn run(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn session() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() })
}

fn is_true(e: &mut CoreEngine, src: &str) {
    assert_eq!(run(e, src), "true", "{src}");
}

#[test]
fn definite_integrals_by_meijer_g() {
    let mut e = session();
    for (f, v) in [
        ("bessel_j(0, x)*exp(-2*x)", "1/sqrt(5)"),
        ("x*exp(-x^2)*bessel_j(0, x)", "exp(-1/4)/2"),
        ("x*bessel_k(1, 2*x)*bessel_j(1, x)", "1/10"),
        ("bessel_j(1, x)*exp(-x)/x", "sqrt(2) - 1"),
    ] {
        is_true(&mut e, &format!("abs(n(integrate({f}, [x, 0, inf]) - ({v}), 20)) < 10^-15"));
    }
    // a value only after the numeric check: J_2(x)/(1 + x^2) against n_integrate
    is_true(&mut e, "abs(n(integrate(bessel_j(2, x)/(1 + x^2), [x, 0, inf])) - n((1/6)*(2 - 3*pi*bessel_i(2, 1) + 3*pi*struve_l(2, 1)))) < 10^-12");
}

#[test]
fn inverse_mellin_of_gamma_ratios() {
    let mut e = session();
    for (f, v) in [
        ("gamma(s)^2", "2*bessel_k(0, 2*sqrt(x))"),
        ("gamma(s)*gamma(s + 1/2)", "sqrt(pi)*exp(-2*sqrt(x))"),
        ("gamma(s/2)/(1 - s)", "-sqrt(pi)*erfc(x)/x"),
        ("gamma(1/2 + s/2)*gamma(s/2)*2^(-2 + 2*s)/(gamma(1 - s/2)*gamma(3/2 - s/2))", "bessel_j(1, 2*sqrt(x))/sqrt(x)"),
        ("(1/2)*pi*csc(pi*s/2)", "1/(1 + x^2)"),
    ] {
        is_true(&mut e, &format!("abs(n(rewrite(inverse_mellin_transform({f}, s, x) - ({v}), x -> 7/10), 20)) < 10^-15"));
    }
    // a double pole family with a scale: gamma(s/3)^2 is 6 K_0(2 x^(3/2))
    is_true(&mut e, "abs(n(rewrite(inverse_mellin_transform(gamma(s/3)^2, s, x) - 6*bessel_k(0, 2*x^(3/2)), x -> 7/10), 20)) < 10^-15");
}

#[test]
fn mellin_of_bessel_products() {
    let mut e = session();
    is_true(&mut e, "abs(n(rewrite(mellin_transform(bessel_j(0, x)*bessel_j(1, x), x, s), s -> 1/2)) - n_integrate(x^(-1/2)*bessel_j(0, x)*bessel_j(1, x), [x, 0, 2000])) < 10^-6");
    is_true(&mut e, "abs(n(rewrite(mellin_transform(bessel_k(0, x)^2, x, s), s -> 3/2)) - n_integrate(x^(1/2)*bessel_k(0, x)^2, [x, 0, inf])) < 10^-10");
}

#[test]
fn transform_bugs_round21() {
    let mut e = session();
    // D21-09
    assert_eq!(run(&mut e, "simplify(fourier_transform(d(f(t), t), t, w) + im*w*fourier_transform(f(t), t, w))"), "0");
    // D21-12
    is_true(&mut e, "abs(n(rewrite(laplace_transform(exp(-t^2), t, s), s -> 3/2)) - n_integrate(exp(-t^2 - 3/2*t), [t, 0, inf])) < 10^-12");
    is_true(&mut e, "abs(n(rewrite(laplace_transform(1/(t + 1), t, s), s -> 3/2)) - n_integrate(exp(-3/2*t)/(t + 1), [t, 0, inf])) < 10^-12");
    assert_eq!(run(&mut e, "inverse_laplace_transform(exp(-s)/s, s, t)"), "heaviside_theta(-1 + t)");
    // D21-15
    assert_eq!(run(&mut e, "free_of(fourier_transform(t*exp(-t^2), t, w), sign)"), "true");
    // D21-16
    assert_eq!(run(&mut e, "z_transform(fibonacci(n), n, z)"), "z/(-1 - z + z^2)");
    assert_eq!(run(&mut e, "inverse_z_transform(z^2, z, n)"), "discrete_delta(2 + n)");
    assert_eq!(run(&mut e, "inverse_z_transform(exp(1/z), z, n)"), "factorial(n)^-1");
    is_true(&mut e, "abs(n(rewrite(z_transform(harmonic_number(n), n, z), z -> 3)) - n(sum(harmonic_number(k)*3^-k, [k, 0, 200]))) < 10^-12");
    // ui B21-2, B21-5
    assert_eq!(run(&mut e, "[kronecker_delta(1.0, 1), kronecker_delta(pi, 3), kronecker_delta(x, x + 1), kronecker_delta(x, y)]"), "[1, 0, 0, kronecker_delta(x, y)]");
}
