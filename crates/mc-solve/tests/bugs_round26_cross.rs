//! Round 26: bugs found in this round where several crates meet, the tests for mc-solve; a test that still fails is kept
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
fn x26_11_solve_of_a_disjunction() {
    let r = ev("solve(x == 1 or x == 2, x)");
    assert!(r.contains("x -> 1") && r.contains("x -> 2"), "{r}");
    let r = ev("solve((x^2 == 1 or x == 5) and x > 0, x, reals)");
    assert!(r.contains("x -> 1") && r.contains("x -> 5") && !r.contains("x -> -1"), "{r}");
}

#[test]
fn r25_06_solve_disjunction_under_and_with_bounds() {
    let r = ev("solve((cos(2*sin(x)) == 0 or tan(2*sin(x)) == 0) and -2*pi <= x and x <= 2*pi, x, reals)");
    assert!(r.starts_with("[[x ->"), "{r}");
}

#[test]
fn r25_07_reduce_over_complex_z() {
    let r = ev("reduce(re(z) > 0 and im_part(z) > re(z), z)");
    assert!(!r.contains("failure") && r.contains("re(z)") && r.contains("im_part(z)"), "{r}");
}
