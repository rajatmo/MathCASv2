//! Round 26: engine bugs found in this round that concern this crate, one test each (ids B26-k); a test that still
//! fails is kept ignored until its bug is fixed, and removing the `ignore` is the proof of a fix.
#![allow(dead_code)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread (stop flag raised after `secs`); `None` when it did not return within
/// `secs + 5` seconds (a hang that ignores the stop flag cannot block the test run).
fn ev_within(src: &str, secs: u64) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let flag = stop.clone();
        let (dtx, drx) = std::sync::mpsc::channel::<()>();
        let dog = std::thread::spawn(move || {
            if drx.recv_timeout(std::time::Duration::from_secs(secs)).is_err() {
                flag.raise();
            }
        });
        let mut e = CoreEngine::with_config(EngineConfig {
            threads: 1,
            ..EngineConfig::default()
        });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = dtx.send(());
        let _ = dog.join();
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs + 5)).ok()
}

#[test]
fn b26_9_primitive_element_of_a_large_quadratic_field() {
    let out = ev_within(
        "F = finite_field(1000003, 2); g = finite_field_primitive_element(F); is_finite_field_element_primitive(g)",
        30,
    );
    assert_eq!(out.as_deref(), Some("true"));
    for src in ["finite_field(1000000007, 2)", "finite_field(2^31 - 1, 2)", "finite_field(2^61 - 1, 2)"] {
        let out = ev_within(
            &format!("F = {src}; g = finite_field_primitive_element(F); is_finite_field_element_primitive(g)"),
            30,
        );
        assert_eq!(out.as_deref(), Some("true"), "{src}");
    }
}

#[test]
fn b26_10_finite_field_of_a_huge_prime_power() {
    // a message and an unevaluated result is the expected behaviour
    let out = ev_within("finite_field(2^100000)", 5);
    assert!(out.is_some(), "no answer: the call ignored the stop flag");
}

#[test]
#[ignore = "B26-11: finite_field(7, 2000) and finite_field(2^61 - 1, 100) take more than 20 s (the least irreducible polynomial is cheap)"]
fn b26_11_finite_field_of_large_degree() {
    for src in ["finite_field(7, 2000)", "finite_field(2^61 - 1, 100)"] {
        let out = ev_within(&format!("F = {src}; F(\"FieldSize\") > 1"), 20);
        assert_eq!(out.as_deref(), Some("true"), "{src}");
    }
}
