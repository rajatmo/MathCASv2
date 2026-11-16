//! Inverse Laplace transforms of `(s + c)^(k/2) R(s)` with R rational, `log(s)/(s - a)`, `log(s + a)/s`,
//! `s^(-+n/2) bessel_k(n, a sqrt(s))` and the Legendre functions. Each result is checked by its forward Laplace
//! integral, `n_integrate(f(t) exp(-s t), [t, 0, inf])`, at two values of s. Each test has a time bound.
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

/// `inverse_laplace_transform(big, s, t)` evaluates, and its Laplace integral at each `s` (with the parameters
/// `subs`) agrees with `big` to 8 digits.
fn checked(e: &mut CoreEngine, big: &str, subs: &str, ss: &[&str]) {
    let v = ev(e, &format!("vv = inverse_laplace_transform({big}, s, t); vv"));
    assert!(!v.contains("laplace") && !v.contains("failure"), "{big}: {v}");
    for s in ss {
        holds(
            e,
            &format!(
                "{{ let f = rewrite(vv, {subs}); let g = n(rewrite({big}, join({subs}, [s -> {s}]))); let i = n_integrate(f*exp(-{s}*t), [t, 0, inf]); abs(i - g) < 10^-8*(1 + abs(g)) }}"
            ),
        );
    }
}

#[test]
fn half_integer_powers() {
    timed("square roots over rational functions", 90, || {
        let mut e = CoreEngine::new();
        checked(&mut e, "sqrt(s + b)/(s + a)", "[a -> 1/2, b -> 2]", &["1", "3"]);
        checked(&mut e, "sqrt(s + b)/(s + a)", "[a -> 3, b -> 1]", &["3/2", "4"]);
        checked(&mut e, "sqrt(s)/(s + a)", "[a -> 2]", &["1", "5/2"]);
        checked(&mut e, "1/((s + 2)*sqrt(s + 1))", "[]", &["1", "3"]);
        // s^(1/2) alone is not the transform of a function: unchanged
        assert!(ev(&mut e, "inverse_laplace_transform(sqrt(s + 1)*s/(s + 2), s, t)").starts_with("inverse_laplace_transform("));
    });
}

#[test]
fn logarithms_bessel_and_legendre() {
    timed("logarithms, bessel_k, legendre", 90, || {
        let mut e = CoreEngine::new();
        checked(&mut e, "log(s)/(s - 1)", "[]", &["2", "7/2"]);
        holds(&mut e, "vv === -exp(t)*exp_integral_ei(-t)");
        checked(&mut e, "log(s + 2)/s", "[]", &["1", "3"]);
        checked(&mut e, "bessel_k(4, sqrt(s))/s^2", "[]", &["1", "3"]);
        holds(&mut e, "vv === 8*t^3*exp(-1/(4*t))");
        checked(&mut e, "bessel_k(1, 2*sqrt(s))*sqrt(s)", "[]", &["1", "2"]);
        checked(&mut e, "legendre_p(-3/5, s)", "[]", &["3/2", "3"]);
        checked(&mut e, "legendre_q(3/5, 0, 3, s)", "[]", &["3/2", "3"]);
        // legendre_p of degree outside (-1, 0): the kernel is not integrable at 0 (a polynomial for n = 2)
        assert!(ev(&mut e, "inverse_laplace_transform(legendre_p(1/3, s), s, t)").starts_with("inverse_laplace_transform("));
    });
}
