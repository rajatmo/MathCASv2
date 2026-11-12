//! Phase 6: nonlinear least squares, find_fit.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let mut bad = vec![];
    for (src, want) in cases {
        let got = match e.eval(src, &StopFlag::new()) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        if got != *want {
            bad.push(format!("{src}\n    got  {got}\n    want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn find_fit() {
    // queue G27
    check(&[
        ("find_fit([[1, 2.1], [2, 3.9], [3, 6.2]], p*x, [p], x)", "[p -> 2.0357142857142856]"),
        ("find_fit([1.0, 1.9, 3.2, 3.9], a + b*x, [a, b], x)[2]", "b -> 1.0"),
        ("abs(rewrite(a, find_fit([[0, 1.0], [1, 2.7], [2, 7.4], [3, 20.1]], a*exp(b*x), [[a, 1], [b, 1]], x)) - 0.9983168020298491) < 1e-9", "true"),
    ]);
}
