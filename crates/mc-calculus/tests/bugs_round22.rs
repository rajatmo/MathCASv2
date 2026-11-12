//! Round 22: bug fixes in calculus (D21-05, D21-07, D21-17, R21-G6, engine B21-22 and B21-23 are in
//! `bugs_round21.rs`).
mod common;
use common::check;

#[test]
fn d21_05_d_solve_value_past_a_blow_up_is_not_a_value() {
    check(&[
        ("d_solve_value([y'(x) == y(x)^2, y(0) == 1], y(1/2), x)", "2"),
        ("d_solve_value([y'(x) == y(x)^2, y(0) == 1], y(2), x)", "failure(blow_up, d_solve_value, 0.0, 2.0)"),
        ("d_solve_value([y'(x) == y(x)^2, y(0) == 1], y(-3), x)", "1/4"),
    ]);
}

#[test]
fn d21_07_working_precision_is_consumed_by_a_symbolic_integral() {
    check(&[("integrate(sin(x), [x, a, b], working_precision: 30)", "cos(a) - cos(b)")]);
}

#[test]
fn d21_17_quadrature_of_an_unknown_integrand() {
    check(&[
        ("d_solve(y'(x) == f(x), y(x), x)", "[[y(x) -> c(1) + integrate(f(x), x)]]"),
        ("d_solve_value([y'(x) == f(x), y(0) == 1], y(2), x)", "1 + integrate(f(t), [t, 0, 2])"),
    ]);
}

#[test]
fn r21_g6_a_limit_that_is_unbounded_says_so() {
    let mut s = common::Sys::new();
    let got = s.run("limit(1/x, x -> 0)");
    assert!(got.starts_with("failure(no_limit, limit, x^-1, "), "{got}");
    assert!(got.contains("unbounded"), "{got}");
}

#[test]
fn b21_22_root_sum_antiderivatives_give_definite_integrals() {
    check(&[
        ("n(integrate(1/(x^3 + x + 1), [x, 0, 1]))", "0.6303193224124083"),
        ("integrate(1/(x^3 + x + 1), [x, -3, 1])", "failure(divergent, integrate, (1 + x + x^3)^-1)"),
    ]);
}

#[test]
fn d21_10_integrals_over_unbounded_ranges_and_of_derivatives() {
    check(&[
        ("integrate(d(f(x), x), [x, a, b])", "f(b) - f(a)"),
        ("integrate(f'''(x), [x, 0, 1])", "-f''(0) + f''(1)"),
        ("integrate(exp(-x^2)*cos(w*x), [x, -inf, inf])", "exp((-1/4)*w^2)*pi^(1/2)"),
        ("integrate(exp(-2*x^2)*sin(w*x), [x, -inf, inf])", "0"),
        ("integrate(bessel_j(0, x), [x, 0, inf])", "1"),
        ("integrate(bessel_j(2, 3*x), [x, 0, inf])", "1/3"),
        // order -3 is below -1: no closed form claimed
        ("integrate(bessel_j(-3, x), [x, 0, inf])", "integrate(bessel_j(-3, x), [x, 0, inf])"),
    ]);
}

#[test]
fn d21_17_implicit_solutions_keep_their_integrals() {
    check(&[
        ("d_solve(y'(x) == y(x) + exp(y(x)), y(x), x)", "solve(integrate((y(x) + exp(y(x)))^-1, y(x)) == x + c(1), y(x))"),
        (
            "d_solve(y''(x) == -y(x)^3, y(x), x)",
            "solve(integrate(-1*2^(1/2)*(-4*c(1) - y(x)^4)^(-1/2), y(x)) == x + c(2), y(x))",
        ),
    ]);
}

#[test]
fn d21_18_frobenius_series_with_roots_that_differ_by_an_integer() {
    // order 1 Bessel (roots 1 and -1, a logarithm), order 0 (a double root), order 1/2 (roots differ by 1, no
    // logarithm)
    check(&[
        (
            "asymptotic_d_solve_value(x^2*y''(x) + x*y'(x) + (x^2 - 1/4)*y(x) == 0, y(x), [x, 0, 4])",
            "(1 + (-1/2)*x^2 + (1/24)*x^4)*c(2)*x^(-1/2) + (1 + (-1/6)*x^2 + (1/120)*x^4)*c(1)*x^(1/2)",
        ),
        (
            "asymptotic_d_solve_value(x^2*y''(x) + x*y'(x) + x^2*y(x) == 0, y(x), [x, 0, 4])",
            "(1 + (-1/4)*x^2 + (1/64)*x^4)*c(1) + (1 + (-1/128)*x^4 + (1 + (-1/4)*x^2 + (1/64)*x^4)*log(x))*c(2)",
        ),
    ]);
    // the residual of the logarithmic solution of order 1 is of higher order than the series
    let mut s = common::Sys::new();
    s.run("s = asymptotic_d_solve_value(x^2*y''(x) + x*y'(x) + (x^2 - 1)*y(x) == 0, y(x), [x, 0, 6])");
    let r = s.run("abs(n((x^2*d(s, [x, 2]) + x*d(s, x) + (x^2 - 1)*s) /. [c(1) -> 0.0, c(2) -> 1.0] /. x -> 0.1)) < 1.0e-6");
    assert_eq!(r, "true");
}
