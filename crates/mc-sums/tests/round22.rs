//! Round 22: divergence verdicts, oscillating and alternating series, finite products, closed forms at
//! working precision, options.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn run(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn number(src: &str) -> f64 {
    let s = run(src);
    s.parse().unwrap_or_else(|_| panic!("{src} gave {s}"))
}

#[test]
fn divergent_sums_and_products_stay_unevaluated() {
    for src in [
        "n_sum(1, [k, 1, inf])",
        "n_sum(k, [k, 1, inf])",
        "n_sum(1/k, [k, 1, inf])",
        "n_sum(1/sqrt(k), [k, 1, inf])",
        "n_sum((-1)^k, [k, 1, inf])",
        "n_sum(2^k, [k, 0, inf])",
        "n_sum(1/(k*log(k)), [k, 2, inf])",
        "n_product(1 + 1/k, [k, 1, inf])",
        "n_product(2, [k, 1, inf])",
        "n_product(k, [k, 1, inf])",
        "n_product(1/k, [k, 1, inf])",
        "n_sum(1/k, [k, 1, inf], working_precision: 30)",
    ] {
        let v = run(src);
        assert!(v.starts_with("n_sum(") || v.starts_with("n_product("), "{src} gave {v}");
    }
}

#[test]
fn slowly_convergent_and_oscillating_series() {
    // (pi - 1)/2, 1 - ... , the Fourier series of cos k / k^2
    assert!((number("n_sum(sin(k)/k, [k, 1, inf])") - (std::f64::consts::PI - 1.0) / 2.0).abs() < 1e-14);
    assert!((number("n_sum(cos(k)/k, [k, 1, inf])") - 0.04201950582536895).abs() < 1e-13);
    assert!((number("n_sum(1/k^(11/10), [k, 1, inf])") - 10.58444846495081).abs() < 1e-10);
    // 1/(k log(k)^2) converges (to 2.10974280...), 1/(k log k) does not
    assert!((number("n_sum(1/(k*log(k)^2), [k, 2, inf])") - 2.1097428012).abs() < 1e-6);
}

#[test]
fn finite_products_of_integers_are_exact() {
    assert_eq!(run("n_product(k, [k, 1, 10])"), "3628800.0");
    assert_eq!(run("n_product(k, [k, 1, 20])"), "2.43290200817664e18");
}

#[test]
fn a_finite_range_of_a_divergent_tail() {
    // H(100000)
    assert!((number("n_sum(1/k, [k, 1, 100000])") - 12.090146129863427).abs() < 1e-9);
}

#[test]
fn closed_forms_at_working_precision_are_fast() {
    let t = std::time::Instant::now();
    assert_eq!(run("abs(n_sum(1/k^2, [k, 1, inf], working_precision: 40) - n(pi^2/6, 50)) < 10^-39"), "true");
    assert_eq!(run("abs(n_product(1 - 1/(k + 1)^2, [k, 2, inf], working_precision: 60) - 2/3) < 10^-59"), "true");
    assert_eq!(run("abs(n_product(exp(1/k^2), [k, 1, inf], working_precision: 40) - n(exp(pi^2/6), 50)) < 10^-39"), "true");
    assert!(t.elapsed().as_secs_f64() < 10.0, "took {:?}", t.elapsed());
}

#[test]
fn sums_from_a_small_integer() {
    assert_eq!(run("sum((-1)^k/k, [k, 2, inf])"), "1 - log(2)");
    assert_eq!(run("sum(binomial(n, k), [k, 1, n])"), "-1 + 2^n");
}

#[test]
fn options_are_read() {
    // a known method is used, an unknown option or a symbolic summand leaves a message and the value or the call
    let v = number("n_sum((-1)^k/sqrt(k), [k, 1, inf], method: alternating_signs)");
    assert!((v + 0.604898643421630).abs() < 1e-12, "{v}");
    let v = number("n_sum(1/k^2, [k, 1, inf], method: euler_maclaurin)");
    assert!((v - std::f64::consts::PI.powi(2) / 6.0).abs() < 1e-13, "{v}");
    assert_eq!(run("n_sum(x^k/factorial(k), [k, 0, inf])"), "n_sum(x^k/factorial(k), [k, 0, inf])");
}
