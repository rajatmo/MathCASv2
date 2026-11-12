//! Round 25: reduce by a sign chart for atoms mixing x with exp, log and atan; Lambert forms with a shift
//! and with log(x).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn run(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn b23_15_sign_chart_forms() {
    assert_eq!(run("reduce(x*exp(x) > 1, x, reals)"), "product_log(1) < x");
    assert_eq!(run("reduce(exp(-x^2) > 1/2, x, reals)"), "-log(2)^(1/2) < x and x < log(2)^(1/2)");
    assert_eq!(run("reduce(exp(x) - x > 1, x, reals)"), "x < 0 or 0 < x");
    assert_eq!(run("reduce(atan(x) < 1/100, x, reals)"), "x < tan(1/100)");
    assert_eq!(run("reduce(x*log(x) > 3, x, reals)"), "exp(product_log(3)) < x");
    // two zeros, both Lambert branches
    assert_eq!(run("reduce(exp(x) <= 3*x, x, reals)"), "-product_log(-1/3) <= x and x <= -product_log(-1, -1/3)");
    // a statement with no solution and one that holds everywhere
    assert_eq!(run("reduce(exp(x) + x^2 < 0, x, reals)"), "false");
    assert_eq!(run("reduce(exp(x) + x^2 > 0, x, reals)"), "true");
}

#[test]
fn b23_16_lambert_with_a_shift_and_log() {
    assert_eq!(run("solve(x*log(x) == 2, x)"), "[[x -> exp(product_log(2))]]");
    assert_eq!(run("solve(log(x)/x == -1/10, x)"), "[[x -> exp(-product_log(1/10))]]");
    assert_eq!(run("solve((x + 1)*exp(x) == 3, x, reals)"), "[[x -> -1 + product_log(3*e)]]");
}
