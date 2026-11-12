//! Round 27: function-property fixes of bugs found in round 26 (D26-xx, R25-09) and the ranges.
#![allow(dead_code)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn d26_01_11_12_ranges_with_open_ends() {
    assert_eq!(ev("function_range(exp(x), x, y)"), "y > 0");
    assert_eq!(ev("function_range(1/x, x, y)"), "y != 0");
    assert_eq!(ev("function_range(1/(1 + x^2), x, y)"), "0 < y and y <= 1");
    assert_eq!(ev("function_range(atan(x), x, y)"), "(-1/2)*pi < y and y < (1/2)*pi");
    assert_eq!(ev("function_range(tanh(x), x, y)"), "-1 < y and y < 1");
    assert_eq!(ev("function_range(x^2, x, y)"), "y >= 0");
}

#[test]
fn d26_16_missing_ranges() {
    assert_eq!(ev("function_range(abs(x), x, y)"), "y >= 0");
    assert_eq!(ev("function_range(sqrt(x), x, y)"), "y >= 0");
    assert_eq!(ev("function_range(log(x), x, y)"), "true");
    assert_eq!(ev("function_range(x^2 + a, x, y)"), "y >= a");
    assert_eq!(ev("function_range([x^2, x > 0], x, y)"), "y > 0");
    assert_eq!(ev("function_range(sin(x) + cos(x), x, y)"), "-2^(1/2) <= y and y <= 2^(1/2)");
    assert_eq!(ev("function_range(3*sin(2*x) + 4*cos(2*x) + 1, x, y)"), "-4 <= y and y <= 6");
}

#[test]
fn d26_10_17_periods() {
    assert_eq!(ev("function_period(sin(x)^2 + cos(x)^2, x)"), "0");
    assert_eq!(ev("function_period(square_wave(x), x)"), "1");
    assert_eq!(ev("function_period(x - floor(x), x)"), "1");
    assert_eq!(ev("function_period(floor(x), x)"), "0");
    assert_eq!(ev("function_period(floor(sin(x)), x)"), "2*pi");
}

#[test]
fn d26_13_essential_singularities_and_r25_09() {
    assert_eq!(ev("function_meromorphic(exp(1/z), z)"), "false");
    assert_eq!(ev("function_meromorphic(sin(x)/x, x)"), "true");
    assert_eq!(ev("function_meromorphic(exp(sin(x)/x), x)"), "true");
    assert_eq!(ev("function_meromorphic(a*sqrt(z), z)"), "conditional_expression(false, a != 0)");
}

#[test]
fn d26_14_injective_moebius() {
    assert_eq!(ev("function_injective(1/x, x)"), "true");
    assert_eq!(ev("function_injective(tanh(1/x), x)"), "true");
    assert_eq!(ev("function_injective(1/x^2, x)"), "false");
    assert_eq!(ev("function_injective(x + 1/x, x)"), "false");
}

#[test]
fn d26_05_18_poles_and_piecewise() {
    assert_eq!(ev("function_poles(zeta(x), x)"), "[[1, 1]]");
    assert_eq!(ev("function_poles(gamma(x), x)"), "[[conditional_expression(-c(1), element(c(1), integers) and c(1) >= 0), 1]]");
    assert_eq!(ev("function_discontinuities(piecewise([[1, x < 0], [2, x >= 0]]), x)"), "x == 0");
    assert_eq!(ev("function_continuous(piecewise([[1, x < 0], [2, x >= 0]]), x)"), "false");
    assert_eq!(ev("function_continuous(piecewise([[x, x < 0], [x^2, x >= 0]]), x)"), "true");
    assert_eq!(ev("function_continuous(1/(x - a), x, assumptions: a > 0)"), "false");
}

#[test]
fn d26_03_surjective_with_codomain_condition() {
    assert_eq!(ev("function_surjective([exp(x), true, y > 0], x, y)"), "true");
    assert_eq!(ev("function_surjective(exp(x), x)"), "false");
}
