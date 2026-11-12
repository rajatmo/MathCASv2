//! Round 25: oscillating and logarithmic tails at 30 to 40 digits by the Euler-Maclaurin formula.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Instant;

fn run(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The digits two decimal texts share.
fn agreeing(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

#[test]
fn r24_03_n_sum_cos_k_over_k_and_log_k_over_k3() {
    // -log(2 sin(1/2)) = 0.0420195058253689617257983840379020371245...; -zeta'(3) = 0.19812624288563685333068182150328...
    let t = Instant::now();
    let got = run("n_sum(cos(k)/k, [k, 1, inf], working_precision: 30)");
    assert!(agreeing(&got, "0.04201950582536896172579838403790203712") >= 31, "{got}");
    let got = run("n_sum(log(k)/k^3, [k, 1, inf], working_precision: 40)");
    assert!(agreeing(&got, "0.198126242885636853330681821503285796875542") >= 41, "{got}");
    let got = run("abs(n_sum(sin(k)/k, [k, 1, inf], working_precision: 30) - (pi - 1)/2) < 10^-28");
    assert_eq!(got, "true");
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
}
