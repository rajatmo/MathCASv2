//! inverse_bilateral_laplace_transform of a product that expands into several terms: inverted term by term with
//! one strip (or the principal-value convention of each term without one); the result transforms back. Time-bounded.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn box_from_a_difference_of_delays() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    let v = ev(&mut e, "vv = inverse_bilateral_laplace_transform((exp(s) - 1)/s, s, t); vv");
    assert!(!v.contains("laplace"), "{v}");
    // the indicator of (-1, 0): 1 inside, 0 outside
    for (t, want) in [("-3/2", "0"), ("-1/2", "1"), ("1/2", "0"), ("3", "0")] {
        assert_eq!(ev(&mut e, &format!("rewrite(vv, t -> {t})")), want, "t = {t}");
    }
    let w = ev(&mut e, "ww = inverse_bilateral_laplace_transform(conditional_expression((exp(s) - 1)/s, re(s) > 0), s, t); ww");
    assert!(!w.contains("laplace"), "{w}");
    for (t, want) in [("-3/2", "0"), ("-1/2", "1"), ("1/2", "0")] {
        assert_eq!(ev(&mut e, &format!("rewrite(ww, t -> {t})")), want, "t = {t}");
    }
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}
