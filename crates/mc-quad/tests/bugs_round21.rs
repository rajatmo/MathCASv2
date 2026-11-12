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
    // the printed form `complex(re, im)`
    if let Some(inner) = t.strip_prefix("complex(").and_then(|r| r.strip_suffix(')')) {
        let (a, b) = inner.split_once(',')?;
        return Some((a.parse().ok()?, b.parse().ok()?));
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
fn b21_10_n_integrate_step_on_infinite_range() {
    assert_real(
        "n_integrate(heaviside_theta(t - 1)*exp(-t), [t, 0, inf])",
        0.36787944117144233,
        1e-9,
    );
    assert_real(
        "n_integrate(heaviside_theta(t - 1)*exp(-7/2*t), [t, 0, inf])",
        0.008627823834948287,
        1e-9,
    );
    assert_real(
        "n_integrate(unit_step(t - 3)*cosh(t)*exp(-7/2*t), [t, 0, inf])",
        1.1076920281582105e-4,
        1e-8,
    );
}

#[test]
fn b21_11_n_integrate_complex_integrand_times_step() {
    assert_c(
        "n_integrate(exp(im*x)*unit_step(x), [x, 0, 5])",
        -0.9589242746631385,
        0.7163378145367738,
        1e-9,
    );
    assert_c(
        "n_integrate((1 + im)*exp(-x)*unit_step(x), [x, -inf, inf])",
        1.0,
        1.0,
        1e-9,
    );
    assert_c(
        "n_integrate(exp((-1 + im)*x)*unit_step(x), [x, 0, inf])",
        0.5,
        0.5,
        1e-9,
    );
}

#[test]
fn b21_12_n_integrate_midpoint_singularity() {
    assert_real(
        "n_integrate(sin(x)/x, [x, -1, 1])",
        1.8921661407343662,
        1e-10,
    );
    assert_real("n_integrate(x/(exp(x) - 1), [x, -1, 1])", 2.0539, 1e-3);
    assert_c(
        "n_integrate(x^x, [x, -1, 1])",
        0.8278536894984987,
        -0.855,
        1e-1,
    );
}

#[test]
fn b21_13_n_integrate_kink_high_precision() {
    assert_true("abs(n_integrate(abs(x), [x, -1, 2], working_precision: 30) - 5/2) < 10^-28");
    assert_true("abs(n_integrate(abs(x - 1/2), [x, 0, pi], working_precision: 30) - (1/8 + (pi - 1/2)^2/2)) < 10^-28");
    assert_true("abs(n_integrate(sin(x)/x, [x, -1, 1], working_precision: 30) - 2*n(sin_integral(1), 40)) < 10^-28");
    assert_true(
        "abs(n_integrate(log(sin(x)), [x, 0, pi], working_precision: 40) + pi*log(2)) < 10^-36",
    );
}

#[test]
#[ignore = "B21-13 rest: log(sin(x)) at 60 digits stops at 38: sin of a tiny ball (x ~ 1e-38) keeps no relative radius, so the tail nodes are not numeric (mc-numeric)"]
fn b21_13_log_sin_at_sixty_digits() {
    assert_true(
        "abs(n_integrate(log(sin(x)), [x, 0, pi], working_precision: 60) + pi*log(2)) < 10^-55",
    );
}

#[test]
fn b21_14_n_integrate_dirac_delta_silent_zero() {
    assert_real(
        "n_integrate(dirac_delta(t - 2)*exp(-t), [t, 0, inf])",
        0.1353352832366127,
        1e-9,
    );
}

#[test]
fn b21_30_n_integrate_sqrt_of_negative_sign() {
    assert_c(
        "n_integrate(sqrt(-x), [x, 1, 2])",
        0.0,
        1.2189514164974602,
        1e-9,
    );
    assert_c(
        "n_integrate(sqrt(1 - x^2), [x, 1, 2])",
        0.0,
        1.073571859106469,
        1e-9,
    );
    assert_c(
        "n_integrate(1/sqrt(1 - x^2), [x, 1, 3])",
        0.0,
        -1.7627471740390899,
        // an endpoint singularity 1/sqrt(x - 1) at the machine rule: the nodes closer to the end than one unit of the
        // last place cannot be told apart from it, which loses about 2 sqrt(2^-53) = 2e-8 of the integral (the sign
        // was the bug; the default precision goal is 8 digits, see NOTES)
        1e-7,
    );
}
