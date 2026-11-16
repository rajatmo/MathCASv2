//! `series_coefficient(f, [x, 0, n])` with a symbolic n for `x^m/(exp(a x) - 1)` (Bernoulli numbers), `g(x)/(1 - b
//! x)` (the Cauchy product, `exp(a x)/(1 - b x)` by the incomplete gamma function) and `chebyshev_t(k, x)` for every
//! k. Each general term is checked against the coefficients of the series expansion for n = 0..9. Each test has a
//! time bound.
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

/// The general term of `f` agrees with the coefficients of `series(f, [x, 0, 11])` at n = 0..9 (and is 0 at n = -1).
fn check(e: &mut CoreEngine, f: &str) {
    let v = ev(e, &format!("gt = series_coefficient({f}, [x, 0, n]); gt"));
    assert!(!v.contains("series_coefficient"), "{f}: {v}");
    holds(
        e,
        &format!(
            "{{ let sr = series({f}, [x, 0, 11]); rewrite(gt, n -> -1) === 0 and every(range(0, 9), j => abs(n(rewrite(gt, n -> j) - series_coefficient(sr, j), 30)) < 10^-25) }}"
        ),
    );
}

#[test]
fn bernoulli_and_geometric_products() {
    timed("bernoulli, Cauchy products", 90, || {
        let mut e = CoreEngine::new();
        check(&mut e, "x/(exp(x) - 1)");
        check(&mut e, "x^2/(exp(3*x) - 1)");
        holds(&mut e, "series_coefficient(t/(exp(t) - 1), [t, 0, n], assumptions: n >= 0) === bernoulli_b(n)/factorial(n)");
        check(&mut e, "exp(x)/(1 - x)");
        check(&mut e, "x*exp(2*x)/(1 - 3*x)");
        // the method option leaves the general term as it is
        holds(&mut e, "series_coefficient(exp(x), [x, 0, n], method: holonomic) === series_coefficient(exp(x), [x, 0, n])");
    });
}

#[test]
fn chebyshev_of_any_degree() {
    timed("chebyshev_t", 90, || {
        let mut e = CoreEngine::new();
        for k in ["5", "4", "1/3", "-7/2"] {
            check(&mut e, &format!("chebyshev_t({k}, x)"));
        }
        let v = ev(&mut e, "series_coefficient(chebyshev_t(k, x), [x, 0, n])");
        assert!(v.starts_with("piecewise(") && v.contains("pochhammer"), "{v}");
    });
}
