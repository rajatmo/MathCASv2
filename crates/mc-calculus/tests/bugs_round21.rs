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
fn b21_15_laplace_drops_cos_times_bessel() {
    assert_true("abs(n(rewrite(laplace_transform(cos(t)*bessel_j(0, t), t, s), s -> 7/2)) - 0.2573086208546995) < 10^-9");
    assert_true("abs(n(rewrite(laplace_transform(cos(t)*bessel_j(1, t), t, s), s -> 7/2)) - 0.03108780795682236) < 10^-9");
    assert_true("abs(n(rewrite(laplace_transform(cos(t)*bessel_j(2, t), t, s), s -> 7/2)) - 0.0034008568096494975) < 10^-9");
}

#[test]
fn b21_16_laplace_log_times_sine_is_zero() {
    assert_true("abs(n(rewrite(laplace_transform(t*sin(t)*log(t), t, s), s -> 7/2)) - n_integrate(t*sin(t)*log(t)*exp(-7/2*t), [t, 0, inf])) < 10^-8");
    assert_true("abs(n(rewrite(laplace_transform(t^2*exp(-3*t)*sin(t)*log(t), t, s), s -> 1)) - n_integrate(t^2*exp(-3*t)*sin(t)*log(t)*exp(-t), [t, 0, inf])) < 10^-8");
}

#[test]
#[ignore = "B21-17: laplace_transform stays unevaluated for exp(-a t^2), t exp(-t^2), erfc(t), 1/(t + a), exp(-1/t), exp_integral_ei(t)"]
fn b21_17_laplace_gaussian_erfc_shifted_unevaluated() {
    for f in [
        "exp(-3*t^2)",
        "t*exp(-t^2)",
        "erfc(t)",
        "1/(t + 3)",
        "exp(-1/t)",
        "exp_integral_ei(t)",
    ] {
        assert_closed(
            &format!("laplace_transform({f}, t, s)"),
            "laplace_transform",
        );
    }
    // exp(-3 t^2): (1/2) exp(s^2/12) sqrt(pi/3) erfc(s/(2 sqrt(3)))
    assert_true("abs(n(rewrite(laplace_transform(exp(-3*t^2), t, s), s -> 7/2)) - 0.21733861779379648) < 10^-10");
}

#[test]
#[ignore = "B21-18: inverse_laplace_transform of exp(-a s) times a rational function with a pure power or simple pole stays unevaluated"]
fn b21_18_inverse_laplace_shift_unevaluated() {
    assert_closed(
        "inverse_laplace_transform(exp(-s)/s^2, s, t)",
        "inverse_laplace_transform",
    );
    assert_closed(
        "inverse_laplace_transform(exp(-s)/(s + 1), s, t)",
        "inverse_laplace_transform",
    );
    assert_closed(
        "inverse_laplace_transform((6 + 6*s + 3*s^2 + s^3)*exp(-s)/s^4, s, t)",
        "inverse_laplace_transform",
    );
    assert_closed(
        "inverse_laplace_transform((-1 + s + 2*exp(-s))/s^2, s, t)",
        "inverse_laplace_transform",
    );
}

#[test]
#[ignore = "B21-19: fourier_transform of a box (boole(abs(x) < a) or a difference of unit_steps) stays unevaluated; it is sqrt(2/pi) sin(a w)/w"]
fn b21_19_fourier_box_unevaluated() {
    assert_closed(
        "fourier_transform(boole(abs(x) < 3), x, w)",
        "fourier_transform",
    );
    assert_closed(
        "fourier_transform(unit_step(x + 1) - unit_step(x - 1), x, w)",
        "fourier_transform",
    );
    assert_true("abs(n(rewrite(fourier_transform(boole(abs(x) < 3), x, w), w -> 4/5)) - sqrt(2/pi)*sin(12/5)/(4/5)) < 10^-9");
}

#[test]
#[ignore = "B21-20: inverse_fourier_transform of the transforms of 1/(x^2 + 1)^2 and x exp(-x^2) (they contain abs(w) and sign(w)) stays unevaluated"]
fn b21_20_inverse_fourier_abs_unevaluated() {
    assert_closed(
        "inverse_fourier_transform((1/4)*(1 + abs(w))*2^(1/2)*exp(-abs(w))*pi^(1/2), w, x)",
        "inverse_fourier_transform",
    );
    assert_closed(
        "inverse_fourier_transform((im/4)*abs(w)*2^(1/2)*exp((-1/4)*w^2)*sign(w), w, x)",
        "inverse_fourier_transform",
    );
}

#[test]
#[ignore = "B21-21: mellin_transform(1/(1 + x^4), x, s) and exp(-1/x) stay unevaluated; x^2/(x^4 + 1) takes 19 s"]
fn b21_21_mellin_rational_unevaluated_and_slow() {
    use std::time::Instant;
    assert_closed("mellin_transform(1/(1 + x^4), x, s)", "mellin_transform");
    assert_closed("mellin_transform(exp(-1/x), x, s)", "mellin_transform");
    let t = Instant::now();
    assert_closed("mellin_transform(x^2/(x^4 + 1), x, s)", "mellin_transform");
    assert!(t.elapsed().as_secs_f64() < 3.0, "took {:?}", t.elapsed());
}

#[test]
fn b21_22_definite_rational_irreducible_denominator() {
    assert_closed("integrate(1/(x^3 + x + 1), [x, 0, 1])", "integrate");
    assert_closed("integrate(1/(x^5 + 1), [x, 0, 1])", "integrate");
    assert_closed("integrate(x^3/(x^5 + 1), [x, 0, 1])", "integrate");
    assert_real(
        "n(integrate(1/(x^5 + 1), [x, 0, 1]))",
        0.8883135726517887,
        1e-12,
    );
    assert_real(
        "n(integrate(1/(x^3 + x + 1), [x, 0, 1]))",
        0.6303193224124081,
        1e-12,
    );
}

#[test]
fn b21_23_limit_log_at_zero() {
    assert_eq_printed("limit(log(x), x -> 0)", "-inf");
    assert_eq_printed("limit(log(x) + x, x -> 0)", "-inf");
    assert_eq_printed("limit(sin(x)/(3 + log(x)), x -> 0)", "0");
    assert_eq_printed("limit(log(x) + tan(x)^3, x -> 0)", "-inf");
    assert_eq_printed("limit(x*log(x)*sinh(4*x), x -> 0)", "0");
}
