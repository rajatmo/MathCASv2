//! Round 25: definite integrals with special-function antiderivatives, closed forms for numeric powers.
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
fn r24_01_numeric_power_to_infinity() {
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
