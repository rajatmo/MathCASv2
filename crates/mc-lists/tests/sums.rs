//! sum(f, [k, a, b]): exact, closed-form, geometric and zeta sums; piecewise.
use mc_bench::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn sums() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("sum(k, [k, 1, 10])", "55"),
        ("sum(k^2, [k, 100])", "338350"),
        ("sum(1/k^2, [k, 1, 10])", "1968329/1270080"),
        ("sum(k, [k, 5, 1])", "0"),
        ("sum(k, [k, 1, 10^7])", "50000005000000"),
        ("sum(k, [k, 1, n])", "(1/2)*n*(1 + n)"),
        ("sum(k^2, [k, 1, n])", "(1/6)*n*(1 + n)*(1 + 2*n)"),
        ("sum(k^3, [k, 1, n])", "(1/4)*n^2*(1 + n)^2"),
        ("sum(c, [k, 1, n])", "c*n"),
        ("sum(2^k, [k, 0, n])", "-1 + 2^(1 + n)"),
        ("sum((1/2)^k, [k, 0, inf])", "2"),
        ("sum(1/k, [k, 1, n])", "sum(k^-1, [k, 1, n])"),
        ("sum(2^k, [k, 0, inf])", "sum(2^k, [k, 0, inf])"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
    // the polynomial closed form agrees with the exact sum at a point
    assert_eq!(eval(&mut e, "rewrite(sum(k^5 - 3*k, [k, 2, m]), [m -> 30]) == sum(k^5 - 3*k, [k, 2, 30])"), "true");
}

#[test]
fn piecewise() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("piecewise([[1, x > 0], [-1, x < 0]], 0)", "piecewise([[1, x > 0], [-1, x < 0]], 0)"),
        ("map(z => piecewise([[z^2, z < 1], [2*z - 1, z < 3]], 5), [0, 2, 4])", "[0, 3, 5]"),
        ("piecewise([[1, 2 > 3]])", "0"),
        // the branch not taken is never evaluated
        ("(z => piecewise([[1/z, z != 0]], 0))(0)", "0"),
        ("piecewise([[a, 1 > 2], [b, x > 0]])", "piecewise([[b, x > 0]])"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}

/// Failures are kept as list elements; drop_failures removes elements that are or contain one.
#[test]
fn failures_in_lists() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("[1, 1/0]", "[1, failure(division_by_zero)]"),
        ("[1, 1/0] + 1", "[2, failure(division_by_zero)]"),
        ("drop_failures([[1, 2], [3, 1/0], 1/0, [4, [5, 1/0]], 6])", "[[1, 2], 6]"),
        ("drop_failures(table([k, if k == 2 then 1/0 else k], k, 3))", "[[1, 1], [3, 3]]"),
        ("f(1/0)", "failure(division_by_zero)"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}
