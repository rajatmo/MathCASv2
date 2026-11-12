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
fn b23_9_d_solve_value_past_blow_up() {
    assert_declines(
        "d_solve_value([y'(x) == 1 + y(x)^2, y(0) == 1], y(1), x)",
        "failure",
    );
    assert_declines(
        "d_solve_value([y'(x) == 1 + y(x)^2, y(0) == 1/3], y(2), x)",
        "failure",
    );
    // y' = 1/(1 - y), y(0) = 1/3: the solution ends at x = 2/9 (y = 1, infinite slope); at x = 3 a complex number came back
    assert_declines(
        "d_solve_value([y'(x) == 1/(1 - y(x)), y(0) == 1/3], y(3), x)",
        "failure",
    );
    // before the blow-up the value is right
    assert_rr(
        "n(d_solve_value([y'(x) == 1 + y(x)^2, y(0) == 1], y(1/2), x))",
        (0.5f64 + std::f64::consts::FRAC_PI_4).tan(),
        1e-12,
    );
}

#[test]
fn b23_17_d_solve_value_false_blow_up() {
    assert_rr(
        "n(d_solve_value([y'(x) == 1 + y(x)^2, y(0) == 0], y(1/2), x))",
        0.5463024898437905,
        1e-12,
    );
    assert_rr(
        "n(d_solve_value([y'(x) == 1 + y(x)^2, y(0) == 0], y(1/10), x))",
        0.10033467208545055,
        1e-12,
    );
    assert_rr(
        "n(d_solve_value([y'(x) == 1 + y(x)^2, y(0) == 0], y(-1/2), x))",
        -0.5463024898437905,
        1e-12,
    );
}

#[test]
#[ignore = "B23-13: mellin_transform at a numeric s outside the strip (log(x) at s = im, exp(im x) at s = im) does not return within 10 s; at s = -2 for bessel_j(0, x) 8 s; integrate of a divergent bessel_k(0, x)/x^2 takes 7 s"]
fn b23_13_divergent_transform_and_integral_not_detected() {
    for src in [
        "mellin_transform(log(x), x, im)",
        "mellin_transform(exp(im*x), x, im)",
    ] {
        let got = ev_within(src, 10);
        assert!(got.is_some(), "{src} did not return within 10 s");
    }
    for src in [
        "mellin_transform(bessel_j(0, x), x, -2)",
        "integrate(x^(-2)*bessel_k(0, x), [x, 0, inf])",
    ] {
        let t = std::time::Instant::now();
        let got = ev_within(src, 10).unwrap_or_default();
        assert!(
            t.elapsed().as_secs_f64() < 2.0,
            "{src}: {:.1} s (the answer is a divergence message)",
            t.elapsed().as_secs_f64()
        );
        assert!(got.starts_with("failure") || got.starts_with("mellin_transform("), "{src}\n   got {got}");
    }
}

#[test]
#[ignore = "B23-21: laplace_transform of t*bessel_k(0, t), t^(3/2)*bessel_j(0, t) and floor(t) stays unevaluated (closed forms exist; the numeric integral at s = 3 is 0.10870946505258644 for the first)"]
fn b23_21_laplace_transform_unevaluated_inputs() {
    assert_closed("laplace_transform(t*bessel_k(0, t), t, s)", "laplace_transform");
    assert_rr(
        "n(rewrite(laplace_transform(t*bessel_k(0, t), t, s), s -> 3))",
        0.10870946505258644,
        1e-9,
    );
    assert_closed("laplace_transform(floor(t), t, s)", "laplace_transform");
    assert_rr(
        "n(rewrite(laplace_transform(floor(t), t, s), s -> 3))",
        1.0 / (3.0 * (3f64.exp() - 1.0)),
        1e-9,
    );
    assert_closed("laplace_transform(t^(3/2)*bessel_j(0, t), t, s)", "laplace_transform");
}

#[test]
#[ignore = "B23-27b: integrate(cos(2 x) struve_h(1/2, 3/2 x), [x, 0, 5]) does not return in 10 s (n_integrate of it: 0.4 s)"]
fn b23_27b_definite_integral_struve_slow() {
    let got = ev_within("integrate(cos(2*x)*struve_h(1/2, 3/2*x), [x, 0, 5])", 10);
    assert!(got.is_some(), "no answer within 10 s");
}
