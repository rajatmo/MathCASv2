//! Round 21: engine bugs found in this round, one test each (ids B21-k); a test that still fails is kept ignored until
//! its bug is fixed, and removing the `ignore` is the proof of a fix.
//! Expected values come from identities, 30 against 45 digits, numeric integrals or independent evaluations;
//! tolerances are stated in each assertion.
#![allow(dead_code)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn session() -> CoreEngine {
    CoreEngine::with_config(EngineConfig {
        threads: 1,
        ..EngineConfig::default()
    })
}

/// Evaluate with a 30 s watchdog (the stop flag is raised when the evaluation has not returned by then).
fn ev(e: &mut CoreEngine, src: &str) -> String {
    let stop = StopFlag::new();
    let flag = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let dog = std::thread::spawn(move || {
        if rx.recv_timeout(std::time::Duration::from_secs(30)).is_err() {
            flag.raise();
        }
    });
    let out = match e.eval(src, &stop) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    };
    let _ = tx.send(());
    let _ = dog.join();
    out
}

/// A printed real or complex number (`a`, `a + b*im`, `a - bim`, `bim`) as (re, im).
fn cnum(s: &str) -> Option<(f64, f64)> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let Some(body) = t.strip_suffix("im") else {
        return t.parse::<f64>().ok().map(|r| (r, 0.0));
    };
    let body = body.strip_suffix('*').unwrap_or(body);
    let b = body.as_bytes();
    let mut pos = None;
    for i in (1..b.len()).rev() {
        if (b[i] == b'+' || b[i] == b'-') && b[i - 1] != b'e' {
            pos = Some(i);
            break;
        }
    }
    let coef = |s: &str| -> Option<f64> {
        match s {
            "" | "+" => Some(1.0),
            "-" => Some(-1.0),
            _ => s.parse().ok(),
        }
    };
    match pos {
        Some(i) => Some((body[..i].parse().ok()?, coef(&body[i..])?)),
        None => Some((0.0, coef(body)?)),
    }
}

/// `src` prints a number within `tol` (relative to max(1, |want|)) of `re + im*i`.
fn assert_c(src: &str, re: f64, im: f64, tol: f64) {
    let mut e = session();
    let got = ev(&mut e, src);
    let ok = match cnum(&got) {
        Some((r, i)) => {
            (r - re).abs() <= tol * re.abs().max(1.0) && (i - im).abs() <= tol * im.abs().max(1.0)
        }
        None => false,
    };
    assert!(
        ok,
        "{src}\n   got  {got}\n   want {re} + {im}im (tol {tol})"
    );
}

fn assert_real(src: &str, want: f64, tol: f64) {
    assert_c(src, want, 0.0, tol);
}

/// `src` prints exactly `want`.
fn assert_eq_printed(src: &str, want: &str) {
    let mut e = session();
    let got = ev(&mut e, src);
    assert_eq!(got, want, "{src}");
}

/// `src` evaluates to `true`.
fn assert_true(src: &str) {
    assert_eq_printed(src, "true");
}

/// The printed value of `src` is an answer: no failure, and the unevaluated head `head` does not remain.
fn assert_closed(src: &str, head: &str) {
    let mut e = session();
    let got = ev(&mut e, src);
    assert!(
        !got.starts_with("failure") && !got.starts_with("error") && !got.contains(head),
        "{src}\n   got  {got}"
    );
}

#[test]
fn b21_24_sum_alternating_from_two() {
    assert_eq_printed("sum((-1)^k/k, [k, 2, inf])", "1 - log(2)");
    assert_eq_printed("sum(binomial(n, k), [k, 1, n])", "-1 + 2^n");
}

#[test]
fn b21_25_n_sum_cosine_series() {
    assert_real("n_sum(cos(k)/k, [k, 1, inf])", 0.04201950582536895, 1e-9);
    assert_real("n_sum(cos(k)/k^2, [k, 1, inf])", 0.3241377400533296, 1e-9);
}

#[test]
fn b21_26_n_product_high_precision_slow() {
    use std::time::Instant;
    let t = Instant::now();
    let mut e = session();
    let got = ev(
        &mut e,
        "n_product(1 - 1/(k + 1)^2, [k, 2, inf], working_precision: 60)",
    );
    assert!(cnum(&got).is_some(), "{got}");
    assert!(t.elapsed().as_secs_f64() < 6.0, "took {:?}", t.elapsed());
}

#[test]
fn b21_31_n_sum_alternating_slow_terms_silently_wrong() {
    assert_real(
        "n_sum((-1)^k/sqrt(k), [k, 1, inf])",
        -0.604898643421630,
        1e-10,
    );
    assert_real(
        "n_sum((-1)^k/k^(1/3), [k, 1, inf])",
        -0.5717528338252765,
        1e-10,
    );
    assert_real(
        "n_sum((-1)^k/log(k + 1), [k, 1, inf])",
        -0.9242998972229388,
        1e-10,
    );
}
