//! Injectivity, periods, analyticity and surjectivity of inverse and hyperbolic functions, a list objective of
//! min_value and the Z transform of a high power.
//! A test that still fails is kept ignored with the reason in its `ignore`; removing the `ignore` is the proof of a fix.
//! Expected values come from identities.
#![allow(dead_code)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread (one worker thread, fresh session) and give up after `secs` seconds: a hang that
/// ignores the stop flag cannot block the test run. `None` when the evaluation did not finish in time.
fn ev_within(src: &str, secs: u64) -> Option<String> {
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
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

fn ev(src: &str) -> String {
    ev_within(src, 30).unwrap_or_else(|| "TIMEOUT".to_string())
}

/// Relative closeness of a printed real to `want`.
fn close(got: &str, want: f64, rel: f64) -> bool {
    got.trim().parse::<f64>().is_ok_and(|g| (g - want).abs() <= rel * want.abs())
}

#[test]
fn injective_reciprocal_hyperbolics() {
    // injective: decreasing on x < 0 and on x > 0 (the sign of f'), with the disjoint images (-inf, 0), (0, inf) for
    // csch and (-inf, -1), (1, inf) for coth (the limits at the ends of the pieces)
    for f in ["csch(x)", "coth(x)", "jacobi_cs(x, 1)", "jacobi_ds(x, 1)", "jacobi_ns(x, 1)"] {
        assert_eq!(ev(&format!("function_injective({f}, x)")), "true", "{f}");
    }
}

#[test]
fn period_of_large_powers() {
    assert_eq!(ev("function_period(sin(x)^8000, x)"), "pi");
    assert_eq!(ev("function_period(sin(x)^8001, x)"), "2*pi");
    assert_eq!(ev("function_period(sin(x)^(10^6), x)"), "pi");
    assert_eq!(ev("function_period(cos(x)^(10^7 + 1), x)"), "2*pi");
}

#[test]
fn min_value_list_objective() {
    let r = ev("min_value([[1] + y, x*y >= 1], [x, y])");
    assert!(!r.contains("internal"), "{r}");
}

#[test]
fn analytic_inverse_functions() {
    for f in ["acsch(x)", "asech(x)", "acoth(x)", "acsc(x)"] {
        assert_eq!(ev(&format!("function_analytic({f}, x)")), "false", "{f}");
    }
}

#[test]
fn surjective_x_sin_x() {
    // onto, proved: continuous, with the value x at x = pi/2 + 2 k pi and -x at x = 3 pi/2 + 2 k pi (unbounded both
    // ways, intermediate values); values growing along samples alone would prove nothing
    assert_eq!(ev("function_surjective(x*sin(x), x)"), "true");
}

#[test]
fn z_transform_of_a_high_power() {
    let r = ev_within("z_transform(2^n*n^50, n, z)", 5).expect("within 5 s");
    assert!(!r.starts_with("z_transform"), "{r}");
}
