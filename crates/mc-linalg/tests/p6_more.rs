//! Phase 6 (package p6-numeric): matrix functions and constructors defined by formulas.
use mc_bench::{Engine, StopFlag};
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
fn matrix_functions() {
    check(&[
        // queue G3
        ("conjugate_transpose([[1 + 2*im, 3], [0, im]])", "[[complex(1, -2), 0], [3, -1im]]"),
        ("hilbert_matrix(3)", "[[1, 1/2, 1/3], [1/2, 1/3, 1/4], [1/3, 1/4, 1/5]]"),
        ("hilbert_matrix([2, 3])", "[[1, 1/2, 1/3], [1/2, 1/3, 1/4]]"),
        ("matrix_power([[1, 1], [1, 0]], 10)", "[[89, 55], [55, 34]]"),
        ("matrix_power([[1, 2], [3, 4]], -1)", "[[-2, 1], [3/2, -1/2]]"),
        ("matrix_exp([[0, 1], [-1, 0]])", "[[cos(1), sin(1)], [-sin(1), cos(1)]]"),
        ("matrix_exp([[1, 1], [0, 1]])", "[[e, e], [0, e]]"),
        ("matrix_function(&(gamma(2, &1)), [[7/2, 0], [0, 13/2]])", "[[(9/2)*exp(-7/2), 0], [0, (15/2)*exp(-13/2)]]"),
        ("rotation_matrix(pi/3)", "[[1/2, (-1/2)*3^(1/2)], [(1/2)*3^(1/2), 1/2]]"),
        (
            "matrix_power([[2, 1], [1, 2]], 1/2)",
            "[[(1/2)*(1 + 3^(1/2)), (1/2)*(-1 + 3^(1/2))], [(1/2)*(-1 + 3^(1/2)), (1/2)*(1 + 3^(1/2))]]",
        ),
    ]);
}
