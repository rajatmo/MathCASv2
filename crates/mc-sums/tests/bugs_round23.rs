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


/// Number of digits in the first number of a printed value.
fn digits_of(s: &str) -> usize {
    let t = s.trim_start_matches('-');
    t.chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .filter(|c| c.is_ascii_digit())
        .count()
}

#[test]
fn b23_7_divergent_geometric_log_series_get_values() {
    // sum 2^k/k diverges: no value, a message
    assert_declines("sum(2^k/k, [k, 1, inf])", "sum(");
    assert_declines("sum((-2)^k/k, [k, 1, inf])", "sum(");
    assert_declines("sum(2^k/(k + 1), [k, 0, inf])", "sum(");
    assert_declines("sum(5^k/k, [k, 2, inf])", "sum(");
    assert_declines("n_sum(2^k/k, [k, 3, inf], working_precision: 30)", "n_sum(");
    // the convergent neighbours stay right
    assert_rr("n(sum(1/(2^k*k), [k, 1, inf]))", 2f64.ln(), 1e-12);
}

#[test]
fn b23_8_n_product_long_finite_range_divergent_to_zero() {
    // partial products by explicit loops: 1000 terms 0.0539215856, 10^5 terms 0.00531596318, 10^6 terms 0.00167923869
    assert_rr("n_product(1 + (-1)^k/sqrt(k), [k, 2, 10^5])", 0.005315963183663204, 1e-6);
    assert_rr("n_product(1 + (-1)^k/sqrt(k), [k, 2, 10^6])", 0.001679238687750119, 1e-6);
    assert_rr("n_product(1 + (-1)^k/sqrt(k), [k, 2, 10^7])", 0.000530840380830824, 1e-6);
}

#[test]
fn b23_24_n_sum_silent_machine_precision() {
    for (src, want) in [
        (
            "n_sum((-1)^k*log(k + 1)/k^(1/2), [k, 3, inf], working_precision: 20)",
            20usize,
        ),
        (
            "n_sum(cos(k)/k, [k, 1, inf], working_precision: 30)",
            28usize,
        ),
    ] {
        let got = ev_within(src, 120).unwrap_or_default();
        assert!(
            got.starts_with("failure") || got.starts_with("n_sum(") || digits_of(&got) >= want,
            "{want} digits asked
   {src}
   got  {got}"
        );
    }
}

#[test]
fn b23_30_n_sum_log_power_series() {
    // true values: partial sum to 10^8 plus the integral tail 2/sqrt(log N) and 4/log(N)^(1/4)
    for (src, want) in [
        ("n_sum(1/(k*log(k)^(3/2)), [k, 2, inf])", 2.93766363796447),
        ("n_sum(1/(k*log(k)^(5/4)), [k, 2, inf])", 4.86286146888),
    ] {
        let mut e = session();
        let got = ev(&mut e, src);
        let ok = got.starts_with("failure")
            || got.starts_with("n_sum(")
            || cnum(&got).is_some_and(|(r, _)| (r - want).abs() < 2e-3 * want);
        assert!(ok, "{src}
   got  {got}
   want {want} or a message");
    }
    assert_declines("n_sum(1/(k*log(k)*log(log(k))), [k, 3, inf])", "n_sum(");
}

#[test]
fn b23_26_n_sum_exp_sqrt_high_precision_slow() {
    let t = std::time::Instant::now();
    let got = ev_within(
        "n_sum(exp(-sqrt(k)), [k, 1, inf], working_precision: 30)",
        10,
    );
    let secs = t.elapsed().as_secs_f64();
    let got = got.expect("n_sum(exp(-sqrt(k)), ..., 30 digits) did not return within 10 s");
    // the speed limit is for a development machine; a slower or shared one sets MC_TEST_TIME_FACTOR (the call
    // still has to return within the 10 s above)
    let factor = std::env::var("MC_TEST_TIME_FACTOR").ok().and_then(|v| v.trim().parse::<f64>().ok()).filter(|f| *f >= 1.0).unwrap_or(1.0);
    assert!(secs < 3.0 * factor, "took {secs:.1} s");
    assert!(
        got.starts_with("1.6704068179663397212396278"),
        "got {got}"
    );
}

#[test]
fn b23_27_n_product_million_terms_slow() {
    let got = ev_within("n_product(1 - 1/(2*k), [k, 1, 10^6])", 10);
    let got = got.expect("n_product over 10^6 terms did not return within 10 s");
    let (r, _) = cnum(&got).unwrap_or((f64::NAN, 0.0));
    assert!(
        (r - 0.000564189513024062751).abs() < 1e-9 * 5.64e-4,
        "got {got}"
    );
}

// ---- round 24: D23-08, D23-09, D23-19, D23-27 and the n_sum bounds

#[test]
fn d23_08_09_n_sum_power_near_one() {
    // zeta(1.01) and zeta(1.0001) = 1/0.0001 + Euler's constant + ...
    assert_rr("n_sum(1/k^1.01, [k, 1, inf])", 100.57794333849678, 1e-9);
    assert_rr("n_sum(1/k^1.02, [k, 1, inf])", 50.578670041015556, 1e-6);
    assert_rr("n_sum(1/k^1.0001, [k, 1, inf])", 10000.577222947539, 1e-9);
    // p = 1 and 1 + tiny stay divergent, with a message
    assert_declines("n_sum(1/k, [k, 1, inf])", "n_sum(");
    assert_declines("n_sum(1/k + 1/k^1.5, [k, 1, inf])", "n_sum(");
    // a logarithm that only just separates the terms from 1/k: a message, not a number
    assert_declines("n_sum(1/(k*log(k + 1)^1.01), [k, 1, inf])", "n_sum(");
}

#[test]
fn d23_19_n_sum_several_iterators_no_message_per_outer_value() {
    let mut e = session();
    let stop = StopFlag::new();
    let out = e.eval("n_sum(1/(i^2*j^2), [i, 1, 30], [j, 1, 30])", &stop).unwrap();
    assert!(out.output.len() <= 1, "{:?}", out.output);
}

#[test]
fn d23_27_n_sum_and_product_corner_cases() {
    assert_rr("n_sum(exp(-k^2), [k, -inf, inf])", 1.7726372048266523, 1e-12);
    assert_rr("n_sum(1/k^2, [k, 1, 5.5])", 1.4636111111111112, 1e-14);
    assert_rr("n_sum(1/k^2, [k, 1.5, 5.5])", 0.7685176647947211, 1e-12);
    assert_rr("n_sum(k, [k, 1.5, 4])", 7.5, 1e-14);
    assert_rr("n_sum(k, [k, 1, 4.5])", 10.0, 1e-14);
    assert_rr("n_product(0, [k, 1, 5])", 0.0, 1e-300);
    assert_closed("n_product(1 - 1/k, [k, 1, 5])", "n_product(");
    assert_rr("n_product(1 - 1/k, [k, 2, 5])", 0.2, 1e-14);
}
