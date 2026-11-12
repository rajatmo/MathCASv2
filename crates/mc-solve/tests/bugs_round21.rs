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
fn b21_27_solve_unknown_times_exp_and_erf() {
    assert_eq_printed("solve(x*exp(x) == 0, x)", "[[x -> 0]]");
    assert_eq_printed("solve(x^2*exp(x) == 0, x)", "[[x -> 0]]");
    assert_eq_printed("solve(erf(x) == 1/2, x)", "[[x -> inverse_erf(1/2)]]");
    assert_eq_printed("solve(erf(x) == 0, x)", "[[x -> 0]]");
}

#[test]
fn b21_28_reduce_sine_inequality_on_interval() {
    assert_eq_printed(
        "reduce(sin(x) > 1/2 and 0 < x and x < 2*pi, x, reals)",
        "(1/6)*pi < x and x < (5/6)*pi",
    );
}
