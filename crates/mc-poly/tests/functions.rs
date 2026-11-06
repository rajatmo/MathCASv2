//! The behaviour of every function through the evaluator: normal cases, edge cases and failures.
mod common;
use common::Sys;

/// Evaluate each `(source, printed result)` pair in one session.
fn check(cases: &[(&str, &str)]) {
    let mut s = Sys::new();
    for (src, want) in cases {
        assert_eq!(s.run(src), *want, "{src}");
    }
}

#[test]
fn expand() {
    check(&[
        ("expand((x + 1)^2)", "1 + 2*x + x^2"),
        ("expand((x + y)*(x - y))", "-y^2 + x^2"),
        ("expand((x/2 + 1)^3)", "1 + (1/8)*x^3 + (3/4)*x^2 + (3/2)*x"),
        ("expand(sin(x)*(x + 1))", "x*sin(x) + sin(x)"),
        ("expand([(a + b)^2, 3])", "[2*a*b + a^2 + b^2, 3]"),
        ("expand((x + 1/x)^2)", "2 + x^-2 + x^2"),
        ("expand((x^(1/2) + 1)^2)", "1 + x + 2*x^(1/2)"),
        ("expand((1.5 + x)^2)", "2.25 + 3.0*x + x^2"),
        ("expand((x + im)^2)", "-1 + 2im*x + x^2"),
        ("expand((1 + x)^2/(1 + y))", "x^2/(1 + y) + 2*x/(1 + y) + (1 + y)^-1"),
        ("expand(0)", "0"),
        ("expand(x)", "x"),
        ("expand((x + 1)^0)", "1"),
        ("expand((x + y)^-2)", "(x + y)^-2"),
        ("expand(x*(x + 1)^-1*(x + 1))", "x"),
        ("expand(2^(1/2)*(2^(1/2) + x))", "2 + x*2^(1/2)"),
        ("expand((x - 1)*(x + 1) - x^2)", "-1"),
        ("expand()", "failure(argument, expand, \"expand(e) takes one argument\")"),
        ("expand((1 + x)^(10^9))", "failure(limit, memory)"),
    ]);
}

#[test]
fn coefficients_degrees_variables() {
    check(&[
        ("coefficient(expand((1 + x + y)^3), x, 2)", "3 + 3*y"),
        ("coefficient((1 + x)^5, x^2)", "10"),
        ("coefficient(x + 1/x, x, -1)", "1"),
        ("coefficient(x^2 + 3x, x, 0)", "0"),
        ("coefficient(x^2 + 3x, x)", "3"),
        ("coefficient(x^2 + 3x, x, 7)", "0"),
        ("coefficient(x^4 + x^3, x^2, 2)", "1"),
        ("coefficient([x^2, 2x], x, 1)", "[0, 2]"),
        (
            "coefficient(x^2*y + x*y^2, x*y)",
            "failure(argument, coefficient, \"the variable must not be a number, sum or product\")",
        ),
        ("coefficient(x^2 + y, x, 1/2)", "failure(argument, coefficient, \"the power k must be an integer\")"),
        ("coefficient_list((1 + x)^3, x)", "[1, 3, 3, 1]"),
        ("coefficient_list((1 + x + y)^2, [x, y])", "[[1, 2, 1], [2, 2, 0], [1, 0, 0]]"),
        ("coefficient_list(1 + 2x + x^3/2, x)", "[1, 2, 0, 1/2]"),
        ("coefficient_list(0, x)", "[0]"),
        (
            "coefficient_list(x^-1 + 1, x)",
            "failure(argument, coefficient_list, \"the polynomial has negative powers of a variable\")",
        ),
        ("degree((1 + x)^7 * y, x)", "7"),
        ("degree(0, x)", "-inf"),
        ("degree(x^5 + x, x^2)", "5/2"),
        ("degree(1 + 1/x, x)", "0"),
        ("degree(sin(x)^3 + sin(x), sin(x))", "3"),
        ("variables((x + sin(y))^2 + z)", "[x, z, sin(y)]"),
        ("variables(3)", "[]"),
        ("variables(x^(1/2) + 2.5*y)", "[y, x^(1/2)]"),
    ]);
}

#[test]
fn gcds_and_division() {
    check(&[
        ("gcd(12, 18)", "6"),
        ("gcd(1/2, 3/4)", "1/4"),
        ("gcd(0, 0)", "0"),
        ("gcd(x^2 - 1, x^2 + 2x + 1)", "1 + x"),
        ("gcd(6x^2 + 6, 4x^2 + 4)", "2 + 2*x^2"),
        ("gcd(x/2, x/3)", "(1/6)*x"),
        ("gcd(x, 1.5)", "failure(argument, gcd, \"gcd takes exact numbers or polynomials\")"),
        ("gcd()", "0"),
        ("gcd(-12)", "12"),
        ("gcd(2^100, 6^50)", "1125899906842624"),
        ("lcm(4, 6)", "12"),
        ("lcm(1/2, 3/4)", "3/2"),
        ("lcm()", "1"),
        ("lcm(x^2 - 1, x + 1)", "-1 + x^2"),
        ("lcm(2x, 3x^2)", "6*x^2"),
        ("lcm(x, 1.5)", "failure(argument, lcm, \"lcm takes exact numbers or polynomials\")"),
        ("polynomial_gcd(expand((x + y)^2*(x - 1)), expand((x + y)*(x + 1)))", "x + y"),
        ("polynomial_gcd(x^2 - 1, 0)", "-1 + x^2"),
        ("polynomial_lcm(x^2 - 1, x + 1)", "-1 + x^2"),
        ("polynomial_lcm(x/2, x^2/3)", "x^2"),
        ("polynomial_lcm(x, 0)", "0"),
        ("polynomial_quotient(x^3 + 1, x + 1, x)", "1 - x + x^2"),
        ("polynomial_remainder(x^3 + 2, x + 1, x)", "1"),
        ("polynomial_quotient(x^2 + y*x + 1, x + y, x)", "x"),
        ("polynomial_remainder(x^2 + y*x + 1, x + y, x)", "1"),
        ("polynomial_quotient(x^2, 2x + 1, x)", "-1/4 + (1/2)*x"),
        ("polynomial_quotient(x, 0, x)", "failure(division_by_zero)"),
        ("polynomial_quotient(x^2, y*x + 1, x)", "polynomial_quotient(x^2, 1 + x*y, x)"),
    ]);
}

#[test]
fn factorisation() {
    check(&[
        ("factor(x^4 - 1)", "(-1 + x)*(1 + x)*(1 + x^2)"),
        ("factor(expand(-6*(x + 1)^2*(2x - 3)))", "-6*(-3 + 2*x)*(1 + x)^2"),
        ("factor(2x^2 - 2)", "2*(-1 + x)*(1 + x)"),
        ("factor(x^2*y^2 - x^2)", "(-1 + y)*(1 + y)*x^2"),
        ("factor(-x^3 + x)", "-1*x*(-1 + x)*(1 + x)"),
        ("factor(x^5 - x)", "x*(-1 + x)*(1 + x)*(1 + x^2)"),
        ("factor(x^2 + 1)", "1 + x^2"),
        ("factor(6)", "6"),
        ("factor((x^2 - 1)/(x^2 + 2x + 1))", "(-1 + x)/(1 + x)"),
        ("factor(3/x - 3)", "-3*(-1 + x)/x"),
        ("factor(sin(y)^2 - 1)", "(-1 + sin(y))*(1 + sin(y))"),
        ("factor(x^2 - y^2)", "(x + y)*(x - y)"),
        ("factor(x^4 - y^4)", "(x + y)*(x - y)*(x^2 + y^2)"),
        ("factor(expand((x + y + 1)*(x*y - 3)*(x - y)^2))", "(-3 + x*y)*(1 + x + y)*(x - y)^2"),
        ("factor(x^3*y + x*y^3 + 2*x^2*y^2)", "x*y*(x + y)^2"),
        ("factor(expand((x + y)^5*(x^2 + y^2 + z)))", "(z + x^2 + y^2)*(x + y)^5"),
        ("factor((x^2 - 1)/0)", "failure(division_by_zero)"),
        ("factor_square_free(expand((x + 1)^2*(x + 2)))", "(2 + x)*(1 + x)^2"),
        ("factor_square_free(x^5 - x^3)", "(-1 + x^2)*x^3"),
    ]);
}

#[test]
fn resultants() {
    check(&[
        ("resultant(x^2 - 2, x - 1, x)", "-1"),
        ("resultant(x^2 + y, x - y, x)", "y + y^2"),
        ("resultant(x^2 - 1, x^2 - 4, x)", "9"),
        ("resultant(x + 1, 3, x)", "3"),
        ("resultant(0, x, x)", "0"),
        ("resultant(x/2 + 1, x - 1, x)", "-3/2"),
        ("resultant(x^2 + 1, x^2, 1)", "failure(argument, resultant, \"the variable must be a kernel\")"),
        ("discriminant(a*x^2 + b*x + c, x)", "-4*a*c + b^2"),
        ("discriminant(x^3 + p*x + q, x)", "-27*q^2 + -4*p^3"),
        ("discriminant(x^2 + 1, x)", "-4"),
        ("discriminant(3, x)", "0"),
        ("discriminant(x, x)", "1"),
        ("discriminant(x^2/2 + x, x)", "1"),
    ]);
}

#[test]
fn rational_functions() {
    check(&[
        ("together(1/x + 1/y)", "(x + y)/x/y"),
        ("together(x/(x^2 - 1) + 1/(x + 1))", "(-1 + 2*x)/(-1 + x^2)"),
        ("together(1/(x - 1) - 1/(x + 1))", "2/(-1 + x^2)"),
        ("together(x/2 + 1/2)", "(1/2)*(1 + x)"),
        ("together((x^2 - y^2)/(x - y))", "x + y"),
        ("together([1/x + 1, 2])", "[(1 + x)/x, 2]"),
        ("together(1/0)", "failure(division_by_zero)"),
        ("cancel((x^2 - 1)/(x - 1))", "1 + x"),
        ("cancel(x/(x*y) + (x^2 - 1)/(x + 1))", "-1 + x + y^-1"),
    ]);
}

#[test]
fn series_and_normal() {
    check(&[
        ("series(exp(x), x, 0, 5)", "1 + x + x^2/2 + x^3/6 + x^4/24 + x^5/120 + O(x^6)"),
        (
            "normal(series(exp(sin(x)), x, 0, 8))",
            "1 + x + (-1/8)*x^4 + (-1/15)*x^5 + (-1/240)*x^6 + (31/5760)*x^8 + (1/90)*x^7 + (1/2)*x^2",
        ),
        ("series(1/(1 - x), x, 0, 4)", "1 + x + x^2 + x^3 + x^4 + O(x^5)"),
        ("series(sin(x)/x^2, x, 0, 3)", "x^-1 - x/6 + x^3/120 + O(x^4)"),
        ("series(log(1 + x), x, 0, 4)", "x - x^2/2 + x^3/3 - x^4/4 + O(x^5)"),
        ("series(sqrt(1 + x), x, 0, 3)", "1 + x/2 - x^2/8 + x^3/16 + O(x^4)"),
        ("series(sqrt(x^2 + x^3), x, 0, 3)", "x + x^2/2 - x^3/8 + O(x^4)"),
        ("series(exp(a*x), x, 0, 3)", "1 + a*x + (1/2)*a^2*x^2 + (1/6)*a^3*x^3 + O(x^4)"),
        ("series(exp(x), x, 1, 2)", "exp(1) + exp(1)*(x - 1) + (1/2)*exp(1)*(x - 1)^2 + O((x - 1)^3)"),
        ("series(exp(x/2 + x^2/3), x, 0, 4)", "1 + x/2 + 11*x^2/24 + 3*x^3/16 + 115*x^4/1152 + O(x^5)"),
        ("series(cos(x)^2 + sin(x)^2, x, 0, 6)", "1 + O(x^7)"),
        ("series(cosh(x), x, 0, 4)", "1 + x^2/2 + x^4/24 + O(x^5)"),
        ("series(tan(x), x, 0, 7)", "x + x^3/3 + 2*x^5/15 + 17*x^7/315 + O(x^8)"),
        ("series(atan(x), x, 0, 7)", "x - x^3/3 + x^5/5 - x^7/7 + O(x^8)"),
        ("series(1/sin(x), x, 0, 3)", "x^-1 + x/6 + 7*x^3/360 + O(x^4)"),
        ("series(1/(1 + x)^2, x, 0, 4)", "1 - 2*x + 3*x^2 - 4*x^3 + 5*x^4 + O(x^5)"),
        ("series(2^x, x, 0, 3)", "1 + log(2)*x + (1/2)*log(2)^2*x^2 + (1/6)*log(2)^3*x^3 + O(x^4)"),
        ("series(x^x, x, 1, 2)", "1 + (x - 1) + (x - 1)^2 + O((x - 1)^3)"),
        ("series((1 + x)^a, x, 0, 2)", "1 + a*x + ((-1/2)*a + (1/2)*a^2)*x^2 + O(x^3)"),
        ("series(x^3, x, 0, 2)", "O(x^3)"),
        ("series(series_data(x, 0, [1, 1, 1], 0, 3)^2, x, 0, 4)", "1 + 2*x + 3*x^2 + O(x^3)"),
        // the printed form reads back: a sum with an order term, which series reads as the same series
        ("series(1 + x + x^2/2 + O(x^3), x, 0, 5)", "1 + x + x^2/2 + O(x^3)"),
        ("series(x^-1 - x/6 + x^3/120 + O(x^4), x, 0, 3) === series(sin(x)/x^2, x, 0, 3)", "true"),
        ("series(1 + (x - 1) + (x - 1)^2 + O((x - 1)^3), x, 1, 4) === series(x^x, x, 1, 2)", "true"),
        ("series(O(x^3), x, 0, 5)", "O(x^3)"),
        ("series(5 + O(1), x, 0, 5)", "O(1)"),
        ("series(exp(x) + O(x^2), x, 0, 5)", "1 + x + O(x^2)"),
        ("normal(1 + x + O(x^2))", "1 + x"),
        // inconsistent shapes print as plain calls
        ("series_data(x, 0, [1, 2], 0, 1)", "series_data(x, 0, [1, 2], 0, 1)"),
        ("series_data(x, 0, [1, 2], 0, 4)", "1 + 2*x + O(x^4)"),
        ("series_data(x, 0, 7, 0, 1)", "series_data(x, 0, 7, 0, 1)"),
        ("series_data(x, 0, [-3, 1/2, -2/3], -1, 2)", "-3*x^-1 + 1/2 - 2*x/3 + O(x^2)"),
        ("series_data(x, 0, [1/2, 1], -2, 0)", "1/(2*x^2) + x^-1 + O(1)"),
        ("series_data(x, -2, [1, -1], 0, 2)", "1 - (x + 2) + O((x + 2)^2)"),
        // logarithmic and Puiseux series (other functions through their derivatives: tests/taylor.rs)
        ("series(log(x), x, 0, 3)", "log(x) + O(x^4)"),
        ("series(x*log(x) + sin(x), x, 0, 3)", "(1 + log(x))*x - x^3/6 + O(x^4)"),
        ("series(sqrt(x), x, 0, 3)", "x^(1/2) + O(x^(7/2))"),
        ("series(cos(sqrt(x)), x, 0, 3)", "1 - x/2 + x^2/24 - x^3/720 + O(x^4)"),
        // at infinity: powers of 1/x
        ("normal(series(1/(1 + x), x, inf, 3))", "-x^-2 + x^-3 + x^-1"),
        ("normal(series(sqrt(x^2 + 1), x, inf, 3))", "x + (-1/8)*x^-3 + (1/2)/x"),
        // not expandable here: unevaluated
        ("series(exp(x), x, inf, 2)", "series(exp(x), x, inf, 2)"),
        ("series(x, 1, 0, 3)", "failure(argument, series, \"the variable must be a symbol\")"),
        ("series(x, x, 0, 1.5)", "failure(argument, series, \"the order must be a machine integer\")"),
        ("normal(series_data(x, 0, [1, 2, 3], -1, 2))", "2 + 3*x + x^-1"),
        ("normal(series_data(x, 1, [1, 2], 0, 2))", "1 + 2*(-1 + x)"),
        ("normal(x + 1)", "1 + x"),
        ("normal([series(exp(x), x, 0, 2), 3])", "[1 + x + (1/2)*x^2, 3]"),
    ]);
}
