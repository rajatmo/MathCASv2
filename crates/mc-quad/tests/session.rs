//! Behaviour in a session: identical results at 1 and 4 threads (also under the stress collector), interrupts,
//! and the compiled-versus-interpreted measurement.
mod common;
use common::Sys;
use std::time::{Duration, Instant};

const CASES: &[&str] = &[
    "n_integrate(exp(-x)*cos(300*x), [x, 0, pi])",
    "n_integrate(1/sqrt(x), [x, 0, 1])",
    "n_integrate(exp(-x^2), [x, -inf, inf])",
    "n_integrate(sqrt(x - 0.5), [x, 0, 1])",
    "n_integrate(x*y, [x, 0, 1], [y, 0, x])",
    "sum(map(k => n_integrate(exp(-x)*cos(k*x), [x, 0, pi]), range(40)))",
    "n_integrate(log(1 + x)/(1 + x^2), [x, 0, 1], digits: 40)",
    "n_integrate(log(1 + x)/(1 + x^2), [x, 0, 1], digits: 30, rigorous: true)",
    "n_integrate(1/x, [x, 0, 1])",
];

fn results(threads: usize, stress: bool) -> Vec<String> {
    let mut s = Sys::with_threads(threads, stress);
    CASES.iter().map(|c| s.run(c)).collect()
}

#[test]
fn identical_at_one_and_four_threads() {
    let one = results(1, false);
    assert_eq!(one, results(4, false));
    assert_eq!(one, results(4, true));
}

#[test]
fn interrupts() {
    let mut s = Sys::new();
    for src in [
        "n_integrate(log(1 + x)/(1 + x^2), [x, 0, 1], digits: 20000)",
        "n_integrate(abs(x - y), [x, 0, 1], [y, 0, 1], compile: false, precision_goal: 13)",
    ] {
        let (r, took) = s.interrupted(src, Duration::from_millis(300));
        assert!(r.is_err() || r.as_ref().is_ok_and(|v| v.contains("interrupted")), "{src}: not stopped: {r:?}");
        assert!(took < Duration::from_secs(10), "{src}: stopped after {took:?}");
    }
    assert_eq!(s.run("n_integrate(2, [x, 0, 1])"), "2.0");
}

/// Compiled against interpreted integrands (printed with `--nocapture`); both give the same value.
#[test]
fn compiled_versus_interpreted() {
    let mut s = Sys::new();
    let mut row = vec![];
    for opt in ["compile: true", "compile: false"] {
        let src = format!("sum(map(k => n_integrate(exp(-x)*cos(k*x), [x, 0, pi], {opt}), range(100)))");
        let t = Instant::now();
        let v = s.run(&src);
        row.push((opt, v, t.elapsed()));
    }
    assert_eq!(row[0].1, row[1].1);
    for (opt, v, t) in &row {
        println!("L6 10^2 with {opt}: {v} in {t:?}");
    }
}
