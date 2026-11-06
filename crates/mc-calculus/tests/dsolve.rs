//! Symbolic differential equations and recurrences.
mod common;
use common::check;

#[test]
fn differential_equations() {
    check(&[
        ("d_solve(y'(x) == y(x), y, x)", "[[y -> (x => c(1)*exp(x))]]"),
        ("d_solve(y''(x) + y(x) == 0, y(x), x)", "[[y(x) -> c(1)*cos(x) + c(2)*sin(x)]]"),
        ("d_solve([y''(x) + y(x) == 0, y(0) == 1, y'(0) == 0], y(x), x)", "[[y(x) -> cos(x)]]"),
        ("d_solve(y''(x) - 3*y'(x) + 2*y(x) == 0, y(x), x)", "[[y(x) -> c(1)*exp(x) + c(2)*exp(2*x)]]"),
        ("d_solve(y''(x) + 2*y'(x) + y(x) == 0, y(x), x)", "[[y(x) -> c(1)*exp(-x) + x*c(2)*exp(-x)]]"),
        ("d_solve(y''(x) + y(x) == x, y(x), x)", "[[y(x) -> x + c(1)*cos(x) + c(2)*sin(x)]]"),
        ("d_solve(y'(x) + 2*y(x) == exp(x), y(x), x)", "[[y(x) -> (1/3)*exp(x) + c(1)*exp(-2*x)]]"),
        ("d_solve(y'(x) == x*y(x), y(x), x)", "[[y(x) -> c(1)*exp((1/2)*x^2)]]"),
        ("d_solve(y'(x) == y(x)^2, y(x), x)", "[[y(x) -> -(x + c(1))^-1]]"),
        ("d_solve(x^2*y''(x) + x*y'(x) - y(x) == 0, y(x), x)", "[[y(x) -> x*c(2) + c(1)/x]]"),
        ("d_solve_value([y'(x) == -2*y(x), y(0) == 3], y(x), x)", "3*exp(-2*x)"),
        ("d_solve(y'(x) == sin(y(x)*x), y(x), x)", "d_solve(y'(x) == sin(x*y(x)), y(x), x)"),
    ]);
}

#[test]
fn recurrences() {
    check(&[
        ("r_solve(a(n + 1) == 2*a(n), a(n), n)", "[[a(n) -> c(1)*2^n]]"),
        ("r_solve_value([a(n) == 3*a(n - 1), a(0) == 2], a(n), n)", "2*3^n"),
        ("expand(r_solve_value([a(n + 2) == a(n + 1) + a(n), a(0) == 0, a(1) == 1], a(n), n) /. n -> 10)", "55"),
    ]);
}
