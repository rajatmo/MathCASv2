//! The continued fraction of the golden ratio, a quadratic irrational: the periodic form, its first terms, and the
//! value of the periodic form against the constant at 30 digits. The test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn golden_ratio_continued_fraction() {
    let t0 = Instant::now();
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "continued_fraction(golden_ratio)"), ev(&mut e, "continued_fraction(1/2 + sqrt(5)/2)"));
    assert_eq!(ev(&mut e, "continued_fraction(golden_ratio, 6)"), "[1, 1, 1, 1, 1, 1]");
    // the convergents are ratios of Fibonacci numbers: F(31)/F(30) within 1/F(30)^2 of the constant
    assert_eq!(ev(&mut e, "abs(n(from_continued_fraction(continued_fraction(golden_ratio, 30)) - golden_ratio, 30)) < 1/fibonacci(30)^2"), "true");
    assert_eq!(ev(&mut e, "from_continued_fraction(continued_fraction(golden_ratio, 30)) == fibonacci(31)/fibonacci(30)"), "true");
    assert!(t0.elapsed() < Duration::from_secs(30));
}
