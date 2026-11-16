//! The regularized generalized hypergeometric function with a lower parameter at a pole of the gamma function:
//! an inexact input gives a number at the input's precision, not a formula in the gamma function of an exact complex number.

use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Duration;

/// Evaluate the statements in one fresh session on a worker thread; `TIMEOUT` after 60 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    let stop = StopFlag::new();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

/// Real and imaginary part of a printed complex number (`complex(re, im)` or `re + im im`).
fn parts(s: &str) -> (f64, f64) {
    let t = s.replace(' ', "");
    if let Some(inner) = t.strip_prefix("complex(").and_then(|r| r.strip_suffix(')')) {
        let (re, im) = inner.split_once(',').expect("pair");
        return (re.parse().expect("re"), im.parse().expect("im"));
    }
    let i = t.rfind(['+', '-']).filter(|&i| i > 0 && !t[..i].ends_with('e')).expect("complex");
    (t[..i].parse().expect("re"), t[i..].trim_end_matches("im").parse().expect("im"))
}

/// Lower parameter -3 (so the series starts at k = 4, DLMF 16.2.5): the machine value has no gamma left and agrees
/// with the 30-digit value of the exact inputs to at least 12 digits.
#[test]
fn machine_inputs_give_a_machine_number() {
    let m = ev("hypergeometric_pfq_regularized([1. - im, 13], [im, -3], 7 + im)");
    assert!(!m.contains("gamma"), "{m}");
    let (re, im) = parts(&m);
    let t = ev("n(hypergeometric_pfq_regularized([1 - im, 13], [im, -3], 7 + im), 30)");
    let (tr, ti) = parts(&t);
    assert!(((re - tr) / tr).abs() < 1e-12 && ((im - ti) / ti).abs() < 1e-12, "{m} vs {t}");
}

/// Other inexact inputs give numbers too.
#[test]
fn other_inexact_inputs_give_numbers() {
    for src in [
        "hypergeometric_pfq_regularized([1, 2], [1/2 + im, -2], 0.3)",
        "hypergeometric_pfq_regularized([1, 2], [1/2 + im, -2], n(3/10, 30))",
        "hypergeometric_pfq_regularized([1.5, 2], [1/2, -2], 2)",
    ] {
        let v = ev(src);
        assert!(!v.contains("gamma") && !v.contains("hypergeometric"), "{src}: {v}");
    }
}
