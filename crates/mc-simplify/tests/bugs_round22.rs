//! Round 22: bug fixes in simplification (D21-19 to D21-22, R20-01, R20-02, R20-07).
mod common;
use common::check;

#[test]
fn d21_19_full_simplify_special_functions() {
    check(&[
        ("full_simplify(hypergeometric2f1(1, 1, 2, -x))", "log(1 + x)/x"),
        ("full_simplify(sin(n*pi + x), element(n, integers))", "(-1)^n*sin(x)"),
        ("simplify(cos(n*pi + x), element(n, integers))", "cos(x)*(-1)^n"),
        ("full_simplify(exp_integral_e(1, x) - gamma(0, x))", "0"),
        // a lone exponential integral keeps its name
        ("full_simplify(exp_integral_e(1, x))", "exp_integral_e(1, x)"),
    ]);
}

#[test]
fn d21_20_relations() {
    check(&[
        ("simplify(x^2 > x^2 + 1)", "false"),
        ("simplify(x > 1 and x > 2)", "x > 2"),
        ("simplify(x < 1 or x < 2)", "x < 2"),
        ("simplify(x > 1 and y < 2)", "x > 1 and y < 2"),
    ]);
}

#[test]
fn d21_21_refine() {
    check(&[
        ("refine(conjugate(x), element(x, reals))", "x"),
        ("refine(floor(n + 1/2), element(n, integers))", "n"),
        ("refine(ceiling(n - 1/3), element(n, integers))", "n"),
    ]);
}

#[test]
fn d21_22_function_expand_half_integer_bessel() {
    check(&[
        ("n(bessel_j(3/2, 1.7)) - n(function_expand(bessel_j(3/2, 1.7)))", "0.0"),
        ("n(bessel_j(5/2, 1.7)) - n(function_expand(bessel_j(5/2, 1.7)))", "0.0"),
        ("n(bessel_i(-5/2, 1.7)) - n(function_expand(bessel_i(-5/2, 1.7)))", "0.0"),
        ("n(bessel_k(5/2, 1.7)) - n(function_expand(bessel_k(5/2, 1.7)))", "0.0"),
        ("n(bessel_y(-5/2, 1.7)) - n(function_expand(bessel_y(-5/2, 1.7)))", "0.0"),
    ]);
}

#[test]
fn r20_01_02_powers_of_positive_constants_and_exponent_forms() {
    check(&[
        ("simplify((pi^-1)^(1/2) - pi^(-1/2))", "0"),
        ("simplify(pi^(1/2)*(pi^-1)^(1/2))", "1"),
        ("simplify(exp((1/4)*(p^2 + s^2)) - exp(p^2/4 + s^2/4))", "0"),
        ("simplify(exp((1/4)*(p^2 + s^2)))", "exp((1/4)*(p^2 + s^2))"),
    ]);
}

#[test]
fn r20_07_nonreal_numbers_are_neither_positive_nor_negative() {
    check(&[("is_positive(im)", "false"), ("is_negative(im)", "false"), ("is_nonnegative(1 + im)", "false")]);
}
