//! `fourier_sequence_transform` of a piecewise sequence with pieces on half-lines (as one-sided terms), with the
//! region of convergence under `generate_conditions: true`. Each value is checked against the partial sums of the
//! defining series, `sum(f(n) exp(-im n w), [n, -60, 60])`, at two sample points. Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

#[test]
fn half_line_pieces() {
    let t0 = Instant::now();
    let mut e = CoreEngine::new();
    let v = ev(&mut e, "vv = fourier_sequence_transform(piecewise([[a^n, n >= 0], [b^n, n < 0]]), n, w); vv");
    assert!(!v.contains("fourier_sequence_transform"), "{v}");
    for (a, b, w) in [("1/2", "3", "7/10"), ("-1/3", "5/2", "2")] {
        holds(&mut e, &format!(
            "{{ let s = n(sum(piecewise([[({a})^k, k >= 0], [({b})^k, k < 0]])*exp(-im*k*{w}), [k, -60, 60]), 30); abs(n(rewrite(vv, [a -> {a}, b -> {b}, w -> {w}]), 30) - s) < 10^-12 }}"
        ));
    }
    // the region of convergence: |a| < 1 and |b| > 1 for real w
    let c = ev(&mut e, "fourier_sequence_transform(piecewise([[a^n, n >= 0], [b^n, n < 0]]), n, w, generate_conditions: true, assumptions: element(w, reals))");
    assert!(c.starts_with("conditional_expression(") && c.contains("abs(a)") && c.contains("abs(b)"), "{c}");
    // a piece with the default 0 and a strict bound
    holds(&mut e, "{ let v = fourier_sequence_transform(piecewise([[2^-n, n > 1]]), n, w); abs(n(rewrite(v, w -> 1), 30) - n(sum(2^-k*exp(-im*k), [k, 2, 120]), 30)) < 10^-20 }");
    assert!(t0.elapsed() < Duration::from_secs(30), "took {:?}", t0.elapsed());
}
