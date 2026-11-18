//! sum of log(k) k^-s and (-1)^k log(k) k^-s over k >= k0: -zeta'(s) and eta'(s); every closed form is compared
//! with n_sum, and sums outside the convergence region or with a symbolic exponent stay unevaluated. Time-bounded.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn derivatives_of_zeta_and_eta() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    assert_eq!(ev(&mut e, "sum((-1)^k*log(k)/k, [k, 2, inf])"), "(-1/2)*log(2)^2 + euler_gamma*log(2)");
    for t in [
        "(-1)^k*log(k)/k, [k, 2, inf]",
        "(-1)^k*log(k)/k^2, [k, 1, inf]",
        "3*log(k)/k^2, [k, 3, inf]",
        "(-1)^k*log(k)/k^(1/2), [k, 1, inf]",
        "log(k)/k^(5/2), [k, 4, inf]",
    ] {
        let v = ev(&mut e, &format!("sum({t})"));
        assert!(!v.contains("sum("), "{t}: {v}");
        assert_eq!(ev(&mut e, &format!("abs(n(sum({t})) - n_sum({t})) < 10^-10")), "true", "{t}: {v}");
    }
    for t in ["log(k)/k, [k, 1, inf]", "log(k)/k^s, [k, 1, inf]", "(-1)^k*log(k), [k, 1, inf]"] {
        assert!(ev(&mut e, &format!("sum({t})")).starts_with("sum("), "{t}");
    }
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}
