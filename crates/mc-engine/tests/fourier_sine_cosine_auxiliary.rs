//! Sine and cosine transforms of `1/(t + a)` (the auxiliary functions of the sine and cosine integrals) and the
//! sine transform of `g(t)/t^2` (the integral of the cosine transform of `g/t`). The first are checked against the
//! Laplace forms `f(z) = integrate(exp(-z u)/(u^2 + 1), [u, 0, inf])`, `g(z) = integrate(u exp(-z u)/(u^2 + 1), [u, 0,
//! inf])` of the auxiliary functions, the second against its defining integral, each at two points. Each test has
//! a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

#[test]
fn reciprocal_of_shifted_variable() {
    timed("1/(t + a)", 60, || {
        let mut e = CoreEngine::new();
        let v = ev(&mut e, "vs = fourier_sin_transform(1/(a + t), t, w); vs");
        assert!(!v.contains("fourier"), "{v}");
        let v = ev(&mut e, "vc = fourier_cos_transform(1/(a + t), t, w); vc");
        assert!(!v.contains("fourier"), "{v}");
        for (a, w) in [("3/2", "2/3"), ("1/3", "5/2")] {
            holds(&mut e, &format!("abs(n(rewrite(vs, [a -> {a}, w -> {w}]), 20) - n(sqrt(2/pi)*n_integrate(exp(-{a}*{w}*u)/(u^2 + 1), [u, 0, inf], working_precision: 20), 20)) < 10^-12"));
            holds(&mut e, &format!("abs(n(rewrite(vc, [a -> {a}, w -> {w}]), 20) - n(sqrt(2/pi)*n_integrate(u*exp(-{a}*{w}*u)/(u^2 + 1), [u, 0, inf], working_precision: 20), 20)) < 10^-12"));
        }
        // the inverse is the same integral
        assert!(!ev(&mut e, "inverse_fourier_sin_transform(1/(a + w), w, t)").contains("fourier"));
        // a pole on the half-line: no transform
        assert!(ev(&mut e, "fourier_sin_transform(1/(t - 1), t, w)").starts_with("fourier_sin_transform("));
    });
}

#[test]
fn sine_transform_over_square() {
    timed("(exp(-t) - exp(-2 t))/t^2", 60, || {
        let mut e = CoreEngine::new();
        let v = ev(&mut e, "vv = fourier_sin_transform((exp(-t) - exp(-2*t))/t^2, t, w); vv");
        assert!(!v.contains("fourier") && v.contains("atan"), "{v}");
        for w in ["1/2", "3"] {
            holds(&mut e, &format!("abs(n(rewrite(vv, w -> {w}), 20) - n(sqrt(2/pi)*n_integrate((exp(-t) - exp(-2*t))/t^2*sin({w}*t), [t, 0, inf], working_precision: 20), 20)) < 10^-12"));
        }
    });
}
