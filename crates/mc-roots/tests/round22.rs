//! Round 22: unbounded objectives and constrained optima at working precision.
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
fn unbounded_objectives_are_infinite() {
    for (src, want) in [
        ("n_minimize(1/x, x)", "[-inf, [x -> indeterminate]]"),
        ("n_maximize(1/x, x)", "[inf, [x -> indeterminate]]"),
        ("n_minimize(x^3 - 3*x, x)", "[-inf, [x -> indeterminate]]"),
        ("n_maximize(exp(x), x)", "[inf, [x -> indeterminate]]"),
        ("n_minimize(-x^2, x)", "[-inf, [x -> indeterminate]]"),
    ] {
        assert_eq!(run(src), want, "{src}");
    }
    // bounded problems keep their optimum
    assert_eq!(run("n_minimize(x^4 - 3*x^2 - x, x)"), "[-3.5139050389347886, [x -> 1.300839565941577]]");
    assert_eq!(run("n_minimize(x^2 + y^2 - 1e6, [x, y])"), "[-1000000.0, [x -> 0.0, y -> 0.0]]");
}

#[test]
fn boundary_optima_at_working_precision() {
    let v = run("n_minimize([x + y, x^2 + y^2 <= 1], [x, y], working_precision: 30)");
    assert!(v.starts_with("[-1.41421356237309504880168872421"), "{v}");
    assert!(v.contains("x -> -0.70710678118654752440084436210"), "{v}");
    let v = run("n_maximize([x + y, x^2 + y^2 <= 1], [x, y], working_precision: 30)");
    assert!(v.starts_with("[1.41421356237309504880168872421"), "{v}");
    let v = run("n_minimize([x^2 + y^2, x + y == 1], [x, y], working_precision: 30)");
    assert!(v.starts_with("[0.5000000000000000000000000000"), "{v}");
    let v = run("n_minimize([(x - 2)^2, x <= 1], x, working_precision: 30)");
    assert!(v.starts_with("[1.00000000000000000000000000"), "{v}");
}
