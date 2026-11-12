//! Round 26: bugs found in this round where several crates meet, the tests for mc-calculus; a test that still fails is kept
//! ignored until its bug is fixed, and removing an `ignore` is the proof of a fix. Expected
//! values come from identities.
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
fn x26_1_injective_reciprocal_hyperbolics() {
    for f in ["csch(x)", "coth(x)", "jacobi_cs(x, 1)", "jacobi_ds(x, 1)", "jacobi_ns(x, 1)"] {
        assert_eq!(ev(&format!("function_injective({f}, x)")), "true", "{f}");
    }
}

#[test]
fn x26_2_period_of_large_powers() {
    assert_eq!(ev("function_period(sin(x)^8000, x)"), "pi");
    assert_eq!(ev("function_period(sin(x)^8001, x)"), "2*pi");
    assert_eq!(ev("function_period(sin(x)^(10^6), x)"), "pi");
    assert_eq!(ev("function_period(cos(x)^(10^7 + 1), x)"), "2*pi");
}

#[test]
fn x26_3_min_value_list_objective() {
    let r = ev("min_value([[1] + y, x*y >= 1], [x, y])");
    assert!(!r.contains("internal"), "{r}");
}

#[test]
fn x26_4_analytic_inverse_functions() {
    for f in ["acsch(x)", "asech(x)", "acoth(x)", "acsc(x)"] {
        assert_eq!(ev(&format!("function_analytic({f}, x)")), "false", "{f}");
    }
}

#[test]
fn x26_5_surjective_x_sin_x() {
    assert_eq!(ev("function_surjective(x*sin(x), x)"), "true");
}

#[test]
fn x26_19_z_transform_of_a_high_power() {
    let r = ev_within("z_transform(2^n*n^50, n, z)", 5).expect("within 5 s");
    assert!(!r.starts_with("z_transform"), "{r}");
}
