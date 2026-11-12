//! Round 22: oscillatory tails at tight goals, steps and kinks, delta functions, undefined samples.
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
fn oscillatory_tail_at_tight_goals_is_not_a_sliver_sequence() {
    // -Ci(25) = 0.0068485971797025909...; the tight goals once gave 0.00499 with an error estimate of 2e-14
    let v = number("n_integrate(cos(x)/x, [x, 25, inf], accuracy_goal: 17, precision_goal: 15)");
    assert!((v - 0.006848597179702591).abs() < 1e-12, "{v}");
}

#[test]
fn steps_and_deltas() {
    assert!((number("n_integrate(unit_step(t - 3)*exp(-t), [t, 0, inf])") - (-3.0f64).exp()).abs() < 1e-9);
    assert!((number("n_integrate(heaviside_theta(2*t - 1)*exp(-t), [t, 0, 3])") - ((-0.5f64).exp() - (-3.0f64).exp())).abs() < 1e-9);
    // a step in a decreasing range, a delta in a sum, a delta outside the range
    assert!((number("n_integrate(unit_step(t - 3)*exp(-t), [t, 4, 0])") + ((-3.0f64).exp() - (-4.0f64).exp())).abs() < 1e-9);
    assert!((number("n_integrate(dirac_delta(2*t - 2)*t^2 + t, [t, 0, 4])") - (0.5 + 8.0)).abs() < 1e-9);
    assert_eq!(run("n_integrate(dirac_delta(t - 9)*exp(-t), [t, 0, 4])"), "0.0");
}

#[test]
fn kinks_at_working_precision() {
    assert_eq!(run("abs(n_integrate(abs(x - 1/3), [x, 0, 1], working_precision: 30) - 5/18) < 10^-28"), "true");
    assert_eq!(run("abs(n_integrate(abs(sin(x)), [x, 0, pi], working_precision: 20) - 2) < 10^-18"), "true");
}

#[test]
fn undefined_samples() {
    assert!((number("n_integrate(sin(x)/x, [x, -2, 2])") - 3.2108259536).abs() < 1e-7);
    assert!((number("n_integrate(x/(exp(x) - 1), [x, -1, 1])") - 2.0550092683).abs() < 1e-8);
    // an integrable singularity in the middle splits the range (the value is complex: x^(-1/2) for x < 0)
    let v = run("n_integrate(1/sqrt(x), [x, -1, 1])");
    assert!(v.starts_with("complex(1.99999"), "{v}");
}

#[test]
fn sqrt_of_a_negative_real_is_the_principal_root() {
    let v = run("n_integrate(sqrt(-x), [x, 1, 4])");
    // int sqrt(-x) = i (2/3)(4^(3/2) - 1) = 4.6667i
    assert!(v.starts_with("complex(0.0, 4.66666"), "{v}");
}

#[test]
fn far_exponential_tails_at_working_precision() {
    assert_eq!(
        run("abs(n_integrate(1/k^3 + exp(-k), [k, 17, inf], working_precision: 30) - (1/(2*17^2) + n(exp(-17), 40))) < 10^-28"),
        "true"
    );
}

// ---- round 24: a slowly decaying tail beyond the largest machine real

#[test]
fn slow_algebraic_tail_is_not_returned_as_a_value() {
    // 16^-0.01/0.01 = 97.2655: the double-exponential rule stops at 1e308, where 0.09 of the integral is still to come.
    // round 24 made this a not_converged failure; round 25 integrates the tail by its power law (de::algebraic_tail), so
    // the values are returned: 100 16^-0.01 and 1000 16^-0.001
    let v: f64 = run("n_integrate(x^(-1.01), [x, 16, inf], precision_goal: 12)").parse().unwrap();
    assert!((v - 97.26549474122855).abs() < 1e-8, "{v}");
    let v: f64 = run("n_integrate(x^(-1.001), [x, 16, inf], precision_goal: 12)").parse().unwrap();
    assert!((v - 1000.0 * 16f64.powf(-0.001)).abs() < 1e-6, "{v}");
    // tails that are negligible beyond the range keep their values
    let v: f64 = run("n_integrate(x^(-1.5), [x, 16, inf])").parse().unwrap();
    assert!((v - 0.5).abs() < 1e-13, "{v}");
    let v: f64 = run("n_integrate(x^(-1.1), [x, 16, inf])").parse().unwrap();
    assert!((v - 7.57858283255199).abs() < 1e-10, "{v}");
}
