//! Round 22: bug fixes in solving (D21-06, D21-11, ui B21-3, engine B21-27 and B21-28 are in
//! `bugs_round21.rs`).
mod common;
use common::check;

#[test]
fn d21_06_both_sides_of_x2_equals_2_to_the_x() {
    check(&[
        (
            "solve(x^2 == 2^x, x)",
            "[[x -> -2/log(2)*product_log(-1, (-1/2)*log(2))], [x -> -2/log(2)*product_log((-1/2)*log(2))], [x -> -2/log(2)*product_log((1/2)*log(2))]]",
        ),
        ("n(solve(x^2 == 2^x, x, reals)[3])", "[x -> -0.766664695962123]"),
    ]);
}

#[test]
fn ui_b21_3_assumptions_apply_to_an_expression_without_equals() {
    check(&[
        ("solve(x^2 - 4, x, assumptions: x < 0)", "[[x -> -2]]"),
        ("solve(x^2 - 1, x, assumptions: x > 0)", "[[x -> 1]]"),
    ]);
}

#[test]
fn d21_11_reduce_with_a_function_of_the_unknown() {
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
