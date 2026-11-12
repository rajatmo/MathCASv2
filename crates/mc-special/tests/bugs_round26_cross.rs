//! Round 26: bugs found in this round where several crates meet, the tests for mc-special; a test that still fails is kept
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
fn x26_6_meijer_g_three_gamma_large_z() {
    for z in [1.0e3_f64, 1.0e4, 1.0e5, 1.0e6] {
        let want = 2.0 * std::f64::consts::PI / 3f64.sqrt() * (-3.0 * z.cbrt()).exp();
        let got = ev(&format!("meijer_g([[], []], [[0, 1/3, 2/3], []], {z:e})"));
        assert!(close(&got, want, 1e-6), "z = {z}: {got}, want {want:e}");
    }
}

#[test]
fn x26_7_gegenbauer_high_degree() {
    let got = ev("gegenbauer_c(300, 2.5, 0.3)");
    assert!(close(&got, -767.7487779550515, 1e-8), "{got}");
    let g = ev("gegenbauer_c(400, 1.5, 0.3)");
    assert!(!g.contains("indeterminate") && !g.contains("nan"), "{g}");
}

#[test]
fn x26_8_spherical_harmonic_high_degree() {
    let r = ev("spherical_harmonic_y(500, 300, 0.7, 0.4)");
    assert!(!r.contains("indeterminate") && !r.contains("nan"), "{r}");
}
