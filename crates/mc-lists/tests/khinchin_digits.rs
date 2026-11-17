//! Khinchin's constant to many digits.
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

#[test]
fn khinchin_agrees_with_the_series_over_exact_zeta_values() {
    // an independent sum: ln K = (1/ln 2) sum_k (zeta(2k) - 1)/k (1 - 1/2 + ... + 1/(2k - 1)), every zeta(2k) from its
    // exact value with the digits lost to the subtraction of 1 added back, 1.7 w + 2 terms (they fall like 4^-k)
    let direct = "{ let w = 312; var s = 0; var alt = 0; \
                  for k in range(533) { alt = alt + 1/(2*k - 1) - (if k > 1 then 1/(2*k - 2) else 0); \
                    s = s + (n(zeta(2*k), w + k + 6) - 1)/k*n(alt, w) }; n(exp(s/log(2)), 300) }";
    let got = ev(&format!("abs(n(khinchin, 300) - {direct}) < 10^-295"));
    assert_eq!(got, "true");
    // the first digits (Bailey, Borwein and Crandall 1997)
    assert!(ev("n(khinchin, 40)").starts_with("2.68545200106530644530971483548179569382"));
}

#[test]
fn continued_fraction_of_khinchin_to_four_hundred_terms() {
    let t = ev_within("length(continued_fraction(khinchin, 400))", 20).unwrap_or_default();
    assert_eq!(t, "400");
}
