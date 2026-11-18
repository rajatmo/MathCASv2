//! Definite integrals with special-function antiderivatives, closed forms for numeric powers.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn run(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn real(src: &str) -> f64 {
    let s = run(src);
    s.parse().unwrap_or_else(|_| panic!("{src} gave {s}"))
}

#[test]
fn numeric_power_to_infinity() {
    // 100 16^-0.01 and 50 2^(24/25)
    let v = real("integrate(x^(-1.01), [x, 16, inf])");
    assert!((v - 97.26549474122855).abs() < 1e-12, "{v}");
    assert_eq!(run("integrate(x^(-101/100), [x, 16, inf])"), "50*2^(24/25)");
    let v = real("integrate(x^(-2.5), [x, 1, inf])");
    assert!((v - 2.0 / 3.0).abs() < 1e-14, "{v}");
    assert!(run("integrate(x^(-0.99), [x, 16, inf])").contains("divergent"));
}

#[test]
fn definite_integrals_through_special_function_antiderivatives() {
    assert_eq!(run("integrate(sin(x)/x, [x, 1, 100])"), "-sin_integral(1) + sin_integral(100)");
    assert_eq!(run("integrate(cos(x)/x, [x, 60, inf])"), "-cos_integral(60)");
    assert_eq!(run("integrate(sin(x)/x, [x, 1, inf])"), "-sin_integral(1) + (1/2)*pi");
    assert_eq!(run("integrate(exp(-x^2), [x, 0, 1])"), "(1/2)*erf(1)*pi^(1/2)");
    assert_eq!(run("integrate(exp(x)/x, [x, 1, 2])"), "exp_integral_ei(2) - exp_integral_ei(1)");
    // across the logarithmic singularity of Ci and Ei at 0 the integral diverges
    assert!(run("integrate(cos(x)/x, [x, -1, 1])").contains("divergent"));
    assert!(run("integrate(exp(x)/x, [x, -1, 1])").contains("divergent") || run("integrate(exp(x)/x, [x, -1, 1])").starts_with("integrate"));
    // numerically
    let v = real("n(integrate(sin(x)/x, [x, 1, 100]))");
    assert!((v - 0.616137).abs() < 1e-4, "{v}");
}

#[test]
fn kelvin_derivative_closed_form() {
    assert_eq!(run("d(kelvin_ber(x), x)"), "(1/2)*(kelvin_bei(1, x) + kelvin_ber(1, x))*2^(1/2)");
}

#[test]
fn theta_quotient_antiderivatives() {
    assert_eq!(run("integrate(elliptic_theta_prime(a, u, q), u)"), "elliptic_theta(a, u, q)");
    assert_eq!(run("integrate(elliptic_theta_prime(a, u, q), [u, 0, 5])"), "elliptic_theta(a, 5, q) - elliptic_theta(a, 0, q)");
    assert_eq!(run("integrate(jacobi_zn(u, m), u)"), "log(neville_theta_n(u, m))");
    assert_eq!(run("integrate(jacobi_epsilon(u, m), u)"), "log(neville_theta_n(u, m)) + (1/2)*elliptic_e(m)*u^2/elliptic_k(m)");
    // the derivatives of the antiderivatives are the integrands
    for f in ["jacobi_zn", "jacobi_epsilon"] {
        for (u, m) in [("0.7", "1/3"), ("-1.9", "0.8"), ("2.5", "-2")] {
            let v = real(&format!("n(rewrite(d(integrate({f}(u, {m}), u), u), u -> {u}) - {f}({u}, {m}))"));
            assert!(v.abs() < 1e-12, "{f} at {u}, {m}: {v}");
        }
    }
    // an argument that is not the variable itself is left alone
    assert_eq!(run("integrate(jacobi_zn(u^2, m), u)"), "integrate(jacobi_zn(u^2, m), u)");
}

#[test]
fn theta_derivative_in_its_argument() {
    assert_eq!(run("d(elliptic_theta(a, u, q), u)"), "elliptic_theta_prime(a, u, q)");
    assert_eq!(run("d(integrate(elliptic_theta_prime(a, u, q), u), u)"), "elliptic_theta_prime(a, u, q)");
    let v = real("n(rewrite(d(elliptic_theta(3, u, 1/3), u), u -> 0.7) - elliptic_theta_prime(3, 0.7, 1/3))");
    assert!(v.abs() < 1e-14, "{v}");
}
