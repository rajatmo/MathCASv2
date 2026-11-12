//! Round 26: engine bugs found in this round that concern this crate, one test each (ids B26-k); a test that still
//! fails is kept ignored until its bug is fixed, and removing the `ignore` is the proof of a fix.
//! Expected values come from the definitions (properties checked by sampling), identities, or a closed form
//! evaluated in the engine itself.
#![allow(dead_code, clippy::excessive_precision)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate in a fresh single-thread session with a 30 s watchdog (the stop flag is raised at the limit).
fn ev(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig {
        threads: 1,
        ..EngineConfig::default()
    });
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

/// The machine value of a printed real number.
fn num(src: &str) -> f64 {
    let s = ev(&format!("n({src})"));
    s.trim().parse::<f64>().unwrap_or(f64::NAN)
}

fn assert_close(src: &str, want: f64, tol: f64) {
    let got = num(src);
    assert!(
        (got - want).abs() <= tol * want.abs().max(1e-300),
        "{src}: got {got}, want {want}"
    );
}

const TWO_PI: f64 = std::f64::consts::TAU;

#[test]
fn b26_1_period_of_floor_of_sine() {
    assert_close("function_period(floor(sin(x)), x)", TWO_PI, 1e-12);
    assert_close("function_period(sign(sin(x)), x)", TWO_PI, 1e-12);
    assert_close("function_period(ceiling(sin(x)), x)", TWO_PI, 1e-12);
    assert_close("function_period(floor(sin(7*x)), x)", TWO_PI / 7.0, 1e-12);
    assert_close("function_period(floor(sin(pi*x)), x)", 2.0, 1e-12);
}

#[test]
fn b26_2_meromorphic_at_essential_singularity() {
    for f in ["exp(1/x)", "sin(1/x)", "cos(1/x^2)", "erf(1/x)", "sinh(1/x)", "exp(tan(x))"] {
        assert_eq!(ev(&format!("function_meromorphic({f}, x)")), "false", "{f}");
    }
}

#[test]
fn b26_3_convexity_with_a_cusp() {
    for (f, _) in [("x^(2/3)", 0), ("(x^2)^(1/4)", 0), ("-(x^2)^(1/3)", 0), ("sqrt(sqrt(x^2))", 0)] {
        assert_eq!(ev(&format!("function_convexity({f}, x)")), "indeterminate", "{f}");
    }
}

#[test]
fn b26_4_injective_inverse_and_mobius() {
    for f in ["1/x", "-1/x", "1/x^3", "x/(x + 1)", "(2*x + 1)/(x - 3)", "exp(1/x)"] {
        assert_eq!(ev(&format!("function_injective({f}, x)")), "true", "{f}");
    }
}

#[test]
fn b26_5_monotonicity_across_a_pole() {
    assert_eq!(ev("function_monotonicity(x/(x^2 - 1), x)"), "indeterminate");
    assert_eq!(
        ev("function_monotonicity(x/(x^2 + a), x, assumptions: a <= -1)"),
        "indeterminate"
    );
    assert_eq!(ev("function_monotonicity(max(4, 3/(2*x)), x)"), "indeterminate");
}

#[test]
fn b26_6_continuity_of_composites() {
    assert_eq!(ev("function_continuous(sqrt(sqrt(x^2)), x)"), "true");
    assert_eq!(ev("function_continuous(tanh(log(x)), x)"), "true");
}

#[test]
fn b26_7_fourier_cos_sin_parity() {
    let c1 = num("fourier_cos_transform(1/(t^2 + 1), t, 1)");
    let cm = num("fourier_cos_transform(1/(t^2 + 1), t, -1)");
    assert!((c1 - 0.46106850444789454).abs() < 1e-9, "cos at 1: {c1}");
    assert!((cm - c1).abs() < 1e-9, "cos transform is even: {cm} vs {c1}");
    let s1 = num("fourier_sin_transform(1/(t^2 + 1), t, 1)");
    let sm = num("fourier_sin_transform(1/(t^2 + 1), t, -1)");
    assert!((sm + s1).abs() < 1e-9, "sin transform is odd: {sm} vs {s1}");
    let c4 = num("fourier_cos_transform(1/(t^2 + 4), t, -5/2)");
    let c4p = num("fourier_cos_transform(1/(t^2 + 4), t, 5/2)");
    assert!((c4 - c4p).abs() < 1e-9 * c4p.abs(), "{c4} vs {c4p}");
}

#[test]
fn b26_8_fourier_sin_of_log() {
    // sqrt(2/pi) * integral_0^inf log(t) sin(w t) dt = -sqrt(2/pi) (euler_gamma + log w)/w
    assert_close("fourier_sin_transform(log(t), t, 5)", -0.3489394258360913, 1e-9);
    assert_close("fourier_sin_transform(log(t), t, 1/10)", 13.766456283562858, 1e-9);
}

#[test]
fn b26_23_period_of_compositions() {
    for f in ["exp(sin(x))", "sin(cos(x))", "cos(sin(x))", "sin(sin(x))", "exp(cos(2*x))"] {
        // cos(sin(x)) is even in sin(x) and sin(x + pi) = -sin(x): its smallest period is pi (the list said 2 pi)
        let want = if f == "exp(cos(2*x))" || f == "cos(sin(x))" { std::f64::consts::PI } else { TWO_PI };
        assert_close(&format!("function_period({f}, x)"), want, 1e-12);
    }
    assert_close("function_period(sin(cos(x/2)), x)", 2.0 * TWO_PI, 1e-12);
}

#[test]
fn b26_24_piecewise_expand_of_a_plain_expression() {
    assert_eq!(ev("piecewise_expand(x)"), "x");
    assert_eq!(ev("piecewise_expand(x^2 + 1)"), "1 + x^2");
    assert_eq!(ev("piecewise_expand(sin(x))"), "sin(x)");
    assert_eq!(ev("piecewise_expand(7)"), "7");
}

#[test]
fn b26_25_injective_of_gamma_of_a_cube() {
    let out = ev("function_injective(gamma(x^3), x)");
    assert_eq!(out, "false");
}
