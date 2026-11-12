//! Round 26: bugs found in this round where several crates meet, the tests for mc-eval; a test that still fails is kept
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
fn x26_12_threaded_scalar_operand() {
    let r = ev("set_attributes(f, listable); f([[1, 2], [3, 4]], threaded(b))");
    assert!(!r.contains("threaded_level"), "{r}");
    let r = ev("set_attributes(f, listable); a = array(x, [2, 2, 2]); f(a, threaded(b)) === f(a, threaded(b, 1))");
    assert!(!r.contains("threaded_level"), "{r}");
}
