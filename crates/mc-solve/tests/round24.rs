//! Round 24: reduce with powers of one exponential, solve of products of different functions of the
//! unknown, a huge degree declined at once.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn run(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|x| format!("error: {x}"))
}

#[test]
fn d23_25_reduce_exp_and_its_inverse() {
    assert_eq!(run("reduce(exp(-x) > 2, x)"), "x < -log(2)");
    assert_eq!(run("reduce(exp(x) + exp(-x) > 3, x)"), "x < log((1/2)*(3 - 5^(1/2))) or log((1/2)*(3 + 5^(1/2))) < x");
    assert_eq!(run("reduce(exp(2*x) - 3*exp(x) + 2 < 0, x)"), "0 < x and x < log(2)");
}

#[test]
fn d23_25_solve_product_of_sin_and_cos() {
    let got = run("solve(sin(x)*cos(x) == 0, x)");
    assert!(!got.starts_with("failure") && !got.starts_with("error"), "{got}");
    assert!(got.contains("(1/2)*pi") && got.contains("2*pi*c(1)"), "{got}");
}

#[test]
fn d23_23_huge_degree_is_declined_at_once() {
    let t = std::time::Instant::now();
    assert_eq!(run("solve(x^(10^6) == 1, x)"), "solve(x^1000000 == 1, x)");
    assert!(t.elapsed().as_secs_f64() < 3.0);
}

#[test]
fn d23_22_reduce_of_a_binomial_of_degree_1000() {
    let t = std::time::Instant::now();
    assert_eq!(run("reduce(x^1000 - 1 > 0, x, reals)"), "x < -1 or 1 < x");
    assert!(t.elapsed().as_secs_f64() < 10.0, "{:.1} s", t.elapsed().as_secs_f64());
}
