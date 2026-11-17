//! Convex optimizers with equations that bind parameters (`a == [[..]]`, `b == [..]`) among the constraints, a
//! vector variable next to a scalar one, and the cone constraints of `convex_optimization`.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn second_order_cone_with_parameter_equations() {
    // min t with |a v + b| <= t: v solves the normal equations a^T a v = -a^T b and t is the residual norm
    let cases = [
        ("[[1, 2], [3, 4], [5, 6]]", "[1, 1, 1]"),
        ("[[1, 0], [0, 1], [1, 1]]", "[1, 2, 0]"),
        ("[[2, -1], [1, 3], [0, 1], [1, 1]]", "[1, 0, 2, -1]"),
    ];
    for (a, b) in cases {
        let check = format!(
            "a0 = {a}; b0 = {b}; v0 = linear_solve(dot(transpose(a0), a0), -dot(transpose(a0), b0)); \
             t0 = norm(dot(a0, v0) + b0); "
        );
        let pairs = "peqns = [a == a0, b == b0]";
        // scalar variables
        let r = ev(&format!("{check}{pairs}; r = second_order_cone_optimization(t, [peqns, norm(dot(a, [x, y]) + b) <= t], [x, y, t]); \
             [rewrite([x, y], r) == v0, simplify(rewrite(t, r) - t0) == 0]"));
        assert_eq!(r, "[true, true]", "{a} {b}");
        // a vector variable and a scalar one
        let r = ev(&format!("{check}{pairs}; r = second_order_cone_optimization(t, [peqns, norm(dot(a, x) + b) <= t], [x, t]); \
             [rewrite(x, r) == v0, simplify(rewrite(t, r) - t0) == 0]"));
        assert_eq!(r, "[true, true]", "{a} {b}");
    }
}

#[test]
fn parameter_equations_are_not_threaded() {
    // the equations bind a and b, and the box constraints keep their meaning
    assert_eq!(
        ev("second_order_cone_optimization(t, [a == [[1, 2], [3, 4], [5, 6]], b == [1, 1, 1], norm(dot(a, [x, y]) + b) <= t, -1 <= x and x <= 1, -1 <= y and y <= 1], [x, y, t])"),
        "[x -> 1, y -> -1, t -> 0]"
    );
    // a box that excludes the unconstrained minimizer: the minimum lies on the boundary, compared with a grid
    let r = ev(
        "r = second_order_cone_optimization(t, [a == [[1, 2], [3, 4], [5, 6]], b == [1, 1, 1], norm(dot(a, [x, y]) + b) <= t, \
         0 <= x and x <= 1/2, -1/2 <= y and y <= 1/2], [x, y, t]); \
         f = (u, w) => sqrt(total((dot([[1, 2], [3, 4], [5, 6]], [u, w]) + [1, 1, 1])^2)); \
         grid = min(flatten(table(f(i/20, j/20), [i, 0, 10], [j, -10, 10]))); \
         [0 <= rewrite(x, r) and rewrite(x, r) <= 1/2 and -1/2 <= rewrite(y, r) and rewrite(y, r) <= 1/2, \
          n(rewrite(t, r)) <= grid + 10^-9, n(rewrite(t, r)) >= grid - 1/10, abs(n(f(rewrite(x, r), rewrite(y, r)) - rewrite(t, r))) < 10^-9]",
    );
    assert_eq!(r, "[true, true, true, true]");
}

#[test]
fn conic_and_convex_with_parameter_equations() {
    for f in ["conic_optimization", "convex_optimization"] {
        let r = ev(&format!(
            "parEqs = [a == [[1, 2], [1, 0]], b == [3, -1]]; {f}(dot([1, 1], x), [parEqs, vector_greater_equal([dot(a, x) + b, 0])], x)"
        ));
        // x1 + 2 x2 + 3 >= 0 and x1 - 1 >= 0: the vertex (1, -2), where the objective is -1
        assert_eq!(r, "[x -> [1, -2]]", "{f}");
    }
    // a scalar parameter
    assert_eq!(ev("convex_optimization(x + y, [p == 3, x >= p, y >= 2*p, x + y <= 20], [x, y])"), "[x -> 3, y -> 6]");
}

#[test]
fn cone_constraints_in_convex_optimization() {
    // norm cone: |(x, y)| <= 3, minimum of x + 2 y is -3 sqrt(5) at -(1, 2) 3/sqrt(5)
    assert_eq!(
        ev("r = convex_optimization(x + 2*y, vector_greater_equal([[x, y, 3], 0], [norm_cone, 3]), [x, y]); \
            [simplify(rewrite(x + 2*y, r) + 3*sqrt(5)) == 0, simplify(rewrite(x^2 + y^2, r)) == 9]"),
        "[true, true]"
    );
    // semidefinite cone: [[x1, 1], [1, x2]] >= 0 means x1 >= 0, x1 x2 >= 1; min x1 + 2 x2 = 2 sqrt(2) at (sqrt 2, sqrt(2)/2)
    assert_eq!(
        ev("r = convex_optimization(dot([1, 2], x), vector_greater_equal([[[indexed(x, [1]), 1], [1, indexed(x, [2])]], 0], [semidefinite_cone, 2]), x); \
            v = rewrite(x, r); [simplify(v[1] + 2*v[2] - 2*sqrt(2)) == 0, simplify(v[1]*v[2]) == 1, v[1] > 0]"),
        "[true, true, true]"
    );
}

#[test]
fn least_squares_fit_accepts_an_optimizer_name() {
    let r = ev(
        "data = [[1.0, 12.0], [1.9, 10.0], [2.6, 8.2], [3.4, 6.9], [5.0, 5.9]]; \
         f0 = find_fit(data, a*exp(-k*t), [a, k], t); \
         f1 = find_fit(data, a*exp(-k*t), [a, k], t, method: quasi_newton); \
         [abs(rewrite(a, f1) - rewrite(a, f0)) < 10^-6, abs(rewrite(k, f1) - rewrite(k, f0)) < 10^-8]",
    );
    assert_eq!(r, "[true, true]");
}
