//! nest_graph and longest common subsequences stay within their limits.
//! A test that still fails is kept ignored with the reason in its `ignore`; removing the `ignore` is the proof of a fix.
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
fn nest_graph_limit() {
    assert!(ev_within("time_constrained(nest_graph(n => [n + 1, 2*n], [1], 15), 2)", 5).is_some());
}

#[test]
fn longest_common_subsequence_cost() {
    let r = ev_within("time_constrained(longest_common_subsequence(range(1500), range(1500)), 4)", 8).unwrap();
    assert!(!r.contains("limit"), "{r}");
}
