//! Solving: both sides of x^2 = 2^x, assumptions applied to an expression without an equals sign, reduce
//! with a function of the unknown, Lambert equations.
mod common;
use common::check;

#[test]
fn both_sides_of_x2_equals_2_to_the_x() {
    check(&[
        (
            "solve(x^2 == 2^x, x)",
            "[[x -> -2/log(2)*product_log(-1, (-1/2)*log(2))], [x -> -2/log(2)*product_log((-1/2)*log(2))], [x -> -2/log(2)*product_log((1/2)*log(2))]]",
        ),
        ("n(solve(x^2 == 2^x, x, reals)[3])", "[x -> -0.766664695962123]"),
    ]);
}

#[test]
fn assumptions_apply_to_an_expression_without_equals() {
    check(&[
        ("solve(x^2 - 4, x, assumptions: x < 0)", "[[x -> -2]]"),
        ("solve(x^2 - 1, x, assumptions: x > 0)", "[[x -> 1]]"),
    ]);
}

#[test]
fn reduce_with_a_function_of_the_unknown() {
    check(&[
        ("reduce(exp(x) > 1, x)", "0 < x"),
        ("reduce(log(x) < 2, x)", "0 < x and x < exp(2)"),
        ("reduce(2^x <= 8, x)", "x <= log(8)/log(2)"),
        ("reduce(sin(x) == 0 and 0 < x and x < 4*pi, x, reals)", "x == pi or x == 2*pi or x == 3*pi"),
        ("reduce(cos(x) < 0 and 0 <= x and x <= 2*pi, x)", "(1/2)*pi < x and x < (3/2)*pi"),
        ("reduce(tan(x) > 1 and 0 < x and x < pi, x)", "(1/4)*pi < x and x < (1/2)*pi"),
        ("reduce(not(sin(x) > 0) and 0 < x and x < 2*pi, x)", "pi <= x and x < 2*pi"),
    ]);
}

#[test]
fn lambert_equation_with_a_coefficient_that_does_not_divide() {
    check(&[
        ("solve(x + 2*exp(x) == 1, x, reals)", "[[x -> 1 - product_log(2*e)]]"),
        ("n(solve(x + 2*exp(x) == 1, x, reals))", "[[x -> -0.37482252818362327]]"),
    ]);
}

/// sin(x + e^x - 1) == 1/2: the period of the sine and the branch of W are two independent integers, c(1) and c(2);
/// one constant for both lost every solution with different period and branch. Each solution is substituted back at
/// several (period, branch) pairs.
#[test]
fn period_and_lambert_branch_are_independent_parameters() {
    let mut s = common::Sys::new();
    let r = s.run("r = reduce(sin(x + e^x - 1) == 1/2, x); r");
    assert!(r.contains("c(1)") && r.contains("c(2)"), "{r}");
    let sols = s.run("sols = map(q => q[2], cases(r, equal(x, _), inf)); length(sols)");
    assert_eq!(sols, "2", "{r}");
    for (k1, k2) in [(0, 0), (0, 1), (1, 0), (-1, 2), (2, -1)] {
        for j in 1..=2 {
            let src = format!("abs(n(rewrite(sin(x + e^x - 1) - 1/2, x -> rewrite(sols[{j}], [c(1) -> {k1}, c(2) -> {k2}])), 20)) < 10^-15");
            assert_eq!(s.run(&src), "true", "solution {j} at c(1) = {k1}, c(2) = {k2}");
        }
    }
}
