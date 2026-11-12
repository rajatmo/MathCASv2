//! Round 25: algebraic tails beyond the machine range.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn number(src: &str) -> f64 {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let s = match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    };
    s.parse().unwrap_or_else(|_| panic!("{src} gave {s}"))
}

#[test]
fn r24_02_slow_algebraic_tails() {
    // 50 16^-0.02 and 100 16^-0.01: the exp-sinh range ends at 1e308 with 3e-5 and 8e-3 of the integral beyond
    let v = number("n_integrate(x^-1.02, [x, 16, inf], precision_goal: 13)");
    assert!((v - 47.30288233627979).abs() < 1e-10, "{v}");
    let v = number("n_integrate(x^(-101/100), [x, 16, inf], precision_goal: 13)");
    assert!((v - 97.26549474122855).abs() < 1e-9, "{v}");
    // the default goals
    let v = number("n_integrate(x^-1.02, [x, 16, inf])");
    assert!((v - 47.30288233627979).abs() < 1e-6, "{v}");
    // a left half line and a sum with a fast part
    let v = number("n_integrate(abs(x)^-1.5, [x, -inf, -4], precision_goal: 13)");
    assert!((v - 1.0).abs() < 1e-10, "{v}");
    let v = number("n_integrate(x^-1.02 + exp(-x), [x, 1, inf], precision_goal: 12)");
    assert!((v - (50.0 + (-1.0f64).exp())).abs() < 1e-9, "{v}");
}
