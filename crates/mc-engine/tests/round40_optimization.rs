//! Round 40: least-squares residual norms in `minimize`, rational roots of high degree in `reduce`, vector
//! variables of unknown length in the convex optimizers.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn minimize_residual_norms() {
    // the minimizer of |m x - b| solves the normal equations m^T m x = m^T b
    let cases = [
        ("[[1, 1], [1, 2], [1, 3]]", "[7, 7, 8]", "[x, y]"),
        ("[[2, -1], [1, 4], [3, 3], [0, 1]]", "[1, 2, -1, 5]", "[x, y]"),
        ("[[1, 0, 2], [0, 3, 1], [1, 1, 1], [2, 0, 5]]", "[1, -2, 4, 3]", "[x, y, z]"),
    ];
    for (m, b, v) in cases {
        let got = ev(&format!("m = {m}; b = {b}; r = minimize(norm(dot(m, {v}) - b), {v}); [r[1]^2 == expand(norm(dot(m, rewrite({v}, r[2])) - b)^2), rewrite({v}, r[2]) == linear_solve(dot(transpose(m), m), dot(transpose(m), b))]"));
        assert_eq!(got, "[true, true]", "{m} {b}");
    }
    assert_eq!(ev("m = [[1, 1], [1, 2], [1, 3]]; b = [7, 7, 8]; minimize(norm(dot(m, [x, y]) - b), [x, y])"), "[(1/6)*6^(1/2), [x -> 19/3, y -> 1/2]]");
    // abs of a real polynomial squared is the square; a complex one keeps its abs
    assert_eq!(ev("minimize(abs(x + y - 7)^2 + abs(x + 2*y - 7)^2 + abs(x + 3*y - 8)^2, [x, y])"), "[1/6, [x -> 19/3, y -> 1/2]]");
    assert_eq!(ev("minimize(abs(x + y)^2 + abs(x - 1), [x, y])"), "[0, [x -> 1, y -> -1]]");
}

#[test]
fn rational_roots_of_high_degree() {
    assert_eq!(ev("reduce(1/2 - 6*x + 10*x^2 - x^99/2 + x^100 == 0, x, rationals)"), "x == 1/2");
    // roots built in, others irrational: each listed root makes the polynomial 0
    assert_eq!(ev("reduce(expand((3*x - 2)*(5*x + 7)*(x^80 - 2)) == 0, x, rationals)"), "x == -7/5 or x == 2/3");
    assert_eq!(ev("reduce(expand((4*x + 1)^2*(x^2 + 1)*(x^61 + x + 1)) == 0, x, rationals)"), "x == -1/4");
    assert_eq!(ev("p = expand((7*x - 3)*(2*x + 5)*x^3*(x^70 - 3)); rewrite(p, x -> 3/7) == 0 and rewrite(p, x -> -5/2) == 0"), "true");
    assert_eq!(ev("reduce(expand((7*x - 3)*(2*x + 5)*x^3*(x^70 - 3)) == 0, x, rationals)"), "x == -5/2 or x == 0 or x == 3/7");
    // none, and conditions at the roots
    assert_eq!(ev("reduce(4*x^200 - 1 == 0, x, rationals)"), "false");
    assert_eq!(ev("reduce(x^2 == 1/4 and x > 0, x, rationals)"), "x == 1/2");
}

#[test]
fn vector_variables_of_unknown_length() {
    // indexed(x, [2]) with no list giving the length of x: unevaluated at once
    let sdp = "semidefinite_optimization(indexed(x, [1]) + 2*indexed(x, [2]), vector_greater_equal([[[indexed(x, [1]), 1], [1, indexed(x, [2])]], 0], [\"SemidefiniteCone\", 2]), x)";
    assert_eq!(ev(sdp), sdp);
    let conic = sdp.replace("semidefinite_optimization", "conic_optimization");
    assert_eq!(ev(&conic), conic);
    // the length fixed by a dot product: solved (x1 x2 >= 1 with x1, x2 >= 0: x1 = sqrt(2), x2 = 1/sqrt(2))
    assert_eq!(
        ev("semidefinite_optimization(dot([1, 2], x), vector_greater_equal([[[indexed(x, [1]), 1], [1, indexed(x, [2])]], 0], [\"SemidefiniteCone\", 2]), x)"),
        "[x -> [2^(1/2), (1/2)*2^(1/2)]]"
    );
}
