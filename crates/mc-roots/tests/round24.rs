//! Round 24: n_minimize never returns an infeasible point as a minimum (D23-16), unbounded and
//! overflowing cases (D23-17).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn run(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The first number of `[v, [...]]`.
fn value(out: &str) -> f64 {
    out.trim_start_matches('[').split(',').next().unwrap().trim().parse().unwrap_or(f64::NAN)
}

#[test]
fn d23_16_feasible_set_smaller_than_the_absolute_tolerance() {
    // the radius 1e-5 disc: the minimum of x + y is -sqrt(2) 1e-5 (it was -3.3e-5, at a point outside the disc)
    let v = value(&run("n_minimize([x + y, x^2 + y^2 <= 1e-10], [x, y])"));
    assert!((v + 2f64.sqrt() * 1e-5).abs() < 1e-9 * 1e-5, "{v}");
    let v = value(&run("n_maximize([x + y, x^2 + y^2 <= 1e-10], [x, y])"));
    assert!((v - 2f64.sqrt() * 1e-5).abs() < 1e-9 * 1e-5, "{v}");
    // a disc of radius 1e-150: a value of the right size, or no value (never a point outside the disc)
    let out = run("n_minimize([x + y, x^2 + y^2 <= 1e-300], [x, y])");
    let v = value(&out);
    assert!(v.is_nan() || (v + 2f64.sqrt() * 1e-150).abs() < 1e-6 * 1e-150, "{out}");
}

#[test]
fn d23_17_unbounded_and_overflowing_cases() {
    // x y on x + y = 2 is unbounded below (it was -1.5e27 at a far point)
    assert_eq!(run("n_minimize([x*y, x + y == 2], [x, y])"), "[-inf, [x -> indeterminate, y -> indeterminate]]");
    assert_eq!(run("n_maximize([-x*y, x + y == 2], [x, y])"), "[inf, [x -> indeterminate, y -> indeterminate]]");
    // the same on a bounded segment, and a bounded constrained problem, are unchanged
    assert_eq!(run("n_minimize([x*y, x + y == 2, x >= 0, y >= 0], [x, y])"), "[0.0, [x -> 0.0, y -> 2.0]]");
    assert_eq!(run("n_minimize([x^2 + y^2, x + y == 2], [x, y])"), "[2.0, [x -> 1.0, y -> 1.0]]");
    // a start whose objective overflows is dropped
    assert_eq!(run("n_minimize(x^2, [x, 1e300])"), "[0.0, [x -> 0.0]]");
    assert_eq!(run("n_minimize((x - 3)^2, [x, 1e300])"), "[0.0, [x -> 3.0]]");
    // a symbol without a value: a message and the failure, no number
    for src in ["n_minimize(x^2 - a*x, [x])", "n_maximize(x - a*x^2, [x])", "n_minimize([x^2, x >= b], [x])"] {
        let out = run(src);
        assert!(out.starts_with("failure(not_numeric"), "{src}: {out}");
    }
    // pi and e are values
    assert!(run("n_minimize(pi*x^2 - e*x, [x])").starts_with("[-0.588002401464"));
}
