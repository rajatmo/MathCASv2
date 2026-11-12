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


#[test]
#[ignore = "B23-31: expand((x + y + z)^300) takes more than 25 s (^100: 0.3 s, ^150: 3 s, ^200: 7.7 s; 45451 terms, a multinomial expansion should take well under a second)"]
fn b23_31_expand_trinomial_power_slow() {
    let t = std::time::Instant::now();
    let got = ev_within("length(expand((x + y + z)^300))", 25);
    let secs = t.elapsed().as_secs_f64();
    let got = got.expect("expand((x + y + z)^300) did not return within 25 s");
    assert_eq!(got, "45451");
    assert!(secs < 5.0, "took {secs:.1} s");
}
