//! Round 23: engine bugs found in this round that concern this crate, one test each (ids B23-k); a test that still
//! fails is kept ignored until its bug is fixed, and removing the `ignore` is the proof of a fix.
//! Expected values come from identities, a defining series or integral evaluated in the engine itself,
//! higher precision, or an independent evaluation; each assertion states its tolerance (relative to the modulus
//! of the expected value unless said otherwise).
#![allow(dead_code, clippy::excessive_precision)]
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

/// Evaluate on a detached thread and give up after `secs` seconds (a hang that ignores the stop flag cannot
/// block the test run). Returns `None` when the evaluation did not finish in time.
fn ev_within(src: &str, secs: u64) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = session();
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

/// A printed real or complex number (`a`, `a + b*im`, `a - bim`, `bim`) as (re, im).
fn cnum(s: &str) -> Option<(f64, f64)> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(inner) = t.strip_prefix("complex(").and_then(|r| r.strip_suffix(')')) {
        let (re, im) = inner.split_once(',')?;
        return Some((re.parse().ok()?, im.parse().ok()?));
    }
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

/// `src` prints a number within `rel` (relative to |want|) of `re + im*i`.
fn assert_cr(src: &str, re: f64, im: f64, rel: f64) {
    let mut e = session();
    let got = ev(&mut e, src);
    let want = (re * re + im * im).sqrt();
    let ok = match cnum(&got) {
        Some((r, i)) => ((r - re).powi(2) + (i - im).powi(2)).sqrt() <= rel * want,
        None => false,
    };
    assert!(
        ok,
        "{src}\n   got  {got}\n   want {re} + {im}im (rel {rel})"
    );
}

fn assert_rr(src: &str, want: f64, rel: f64) {
    assert_cr(src, want, 0.0, rel);
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

/// `src` stays unevaluated (starts with `head`) or fails with a message; it must not return a number.
fn assert_declines(src: &str, head: &str) {
    let mut e = session();
    let got = ev(&mut e, src);
    assert!(
        got.starts_with(head) || got.starts_with("failure"),
        "{src}\n   got  {got}"
    );
}


/// `src` evaluates to `true`.
fn assert_true(src: &str) {
    let mut e = session();
    let got = ev(&mut e, src);
    assert_eq!(got, "true", "{src}");
}

#[test]
fn b23_15_reduce_exp_log_atan_of_x() {
    // each region is checked by the original statement at sample points
    for (cond, pts) in [
        ("x*exp(x) > 1", [("1", true), ("1/10", false)]),
        ("exp(-x^2) > 1/2", [("1/2", true), ("2", false)]),
        ("exp(x) + exp(-x) < 5", [("1", true), ("3", false)]),
        // e^x - x - 1 > 0 except at 0 (the first draft of the test had the sign at -1/10 wrong)
        ("exp(x) - x > 1", [("-1/10", true), ("0", false)]),
        ("atan(x) < 1/100", [("-1", true), ("1", false)]),
    ] {
        for (p, truth) in pts {
            let src = format!(
                "rewrite(reduce({cond}, x, reals), x -> {p}) == {}",
                if truth { "true" } else { "false" }
            );
            assert_true(&src);
        }
    }
}

#[test]
fn b23_16_solve_lambert_forms() {
    for eq in [
        "x*log(x) == 2",
        "x*log(x) == -1/10",
        "(x + 1)*exp(x) == 1",
        "exp(x)/x == 3",
        "log(x)/x == -1/10",
    ] {
        let src = format!(
            "max(map(s => abs(n(rewrite(({}) - ({}), s))), solve({eq}, x, reals))) < 10^-9",
            eq.split("==").next().unwrap().trim(),
            eq.split("==").nth(1).unwrap().trim()
        );
        assert_true(&src);
    }
}
