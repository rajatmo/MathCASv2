//! Linear algebra with values of other crates (round 12): sparse arrays (rule and compressed forms), quantities
//! and numbers with uncertainty as entries, options, scalars.
use mc_bench::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::new();
    let stop = StopFlag::new();
    for (src, want) in cases {
        let got = e.eval(src, &stop).unwrap_or_else(|err| panic!("{src}: {err}")).printed;
        assert_eq!(got, *want, "{src}");
    }
}

#[test]
fn sparse_arrays_feed_linear_algebra() {
    check(&[
        ("sparse_array([[i_, i_] -> i], 3)", "[[1, 0, 0], [0, 2, 0], [0, 0, 3]]"),
        ("sparse_array([[i_, j_] when abs(i - j) <= 1 -> 1], 3)", "[[1, 1, 0], [1, 1, 1], [0, 1, 1]]"),
        ("sparse_array(automatic, [3, 3], 0, [1, [[0, 1, 2, 3], [[3], [2], [1]]], [1, 2, 3]])", "[[0, 0, 1], [0, 2, 0], [3, 0, 0]]"),
        ("sparse_array(automatic, [4], x, [1, [[0, 1], [[2]]], [5]])", "[x, 5, x, x]"),
        ("det(sparse_array([[1, 1] -> 2, [2, 2] -> 4], [2, 2]))", "8"),
        ("inverse(sparse_array([[i_, i_] -> i], 2))", "[[1, 0], [0, 1/2]]"),
        ("cholesky(sparse_array(automatic, [2, 2], 0, [1, [[0, 2, 4], [[1], [2], [1], [2]]], [4, 2, 2, 3]]))", "[[2, 1], [0, 2^(1/2)]]"),
    ]);
}

#[test]
fn uncertain_and_unit_entries() {
    check(&[
        ("det([[interval([1, 2]), 0], [0, 2]])", "interval([2, 4])"),
        ("dot([around(1.0, 0.1), 2.0], [1.0, around(3.0, 0.2)])", "around(7.0, 0.4123105625617661)"),
        (r#"norm([quantity(3, "Meters"), quantity(4, "Meters")])"#, r#"quantity(5, "Meters")"#),
        ("norm(-3)", "3"),
        (r#"norm(quantity(-2 + im, "Pounds"))"#, r#"quantity(5^(1/2), "Pounds")"#),
    ]);
}

#[test]
fn matrix_functions_accept_a_method() {
    check(&[(
        r#"matrix_function(exp, [[0, 1], [0, 0]], method: "Jordan")"#,
        "[[1, 1], [0, 1]]",
    )]);
}
