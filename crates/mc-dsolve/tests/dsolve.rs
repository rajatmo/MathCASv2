//! Symbolic differential equations (item 13A): every method family, systems, PDEs, delay equations, values and
//! series solutions, and recurrences.
mod common;
use common::check;

#[test]
fn linear() {
    check(&[
        ("d_solve(y'(x) == y(x), y, x)", "[[y -> (x => exp(x)*c(1))]]"),
        ("d_solve(y''(x) + y(x) == 0, y(x), x)", "[[y(x) -> c(1)*cos(x) + c(2)*sin(x)]]"),
        ("d_solve([y''(x) + y(x) == 0, y(0) == 1, y'(0) == 0], y(x), x)", "[[y(x) -> cos(x)]]"),
        ("d_solve(y''(x) - 3*y'(x) + 2*y(x) == 0, y(x), x)", "[[y(x) -> c(1)*exp(x) + c(2)*exp(2*x)]]"),
        ("d_solve(y''(x) + 2*y'(x) + y(x) == 0, y(x), x)", "[[y(x) -> c(1)*exp(-x) + x*c(2)*exp(-x)]]"),
        ("d_solve(y''(x) + y(x) == x, y(x), x)", "[[y(x) -> x + c(1)*cos(x) + c(2)*sin(x)]]"),
        ("d_solve(y'(x) + 2*y(x) == exp(x), y(x), x)", "[[y(x) -> (1/3)*exp(x) + c(1)*exp(-2*x)]]"),
        ("d_solve(y'(x) == x*y(x), y(x), x)", "[[y(x) -> c(1)*exp((1/2)*x^2)]]"),
        ("d_solve(x^2*y''(x) + x*y'(x) - y(x) == 0, y(x), x)", "[[y(x) -> x*c(2) + c(1)/x]]"),
        ("d_solve(y''(x) - 4*y(x) == 0, y(x), x)", "[[y(x) -> c(1)*exp(2*x) + c(2)*exp(-2*x)]]"),
        ("d_solve(y''''(x) - y(x) == 0, y(x), x)", "[[y(x) -> c(1)*exp(x) + c(2)*cos(x) + c(3)*exp(-x) + c(4)*sin(x)]]"),
        ("d_solve(y''(x) + 2*y'(x) + 5*y(x) == 0, y(x), x)", "[[y(x) -> c(1)*exp(-x)*sin(2*x) + c(2)*cos(2*x)*exp(-x)]]"),
        ("d_solve(y''(x) + w^2*y(x) == 0, y(x), x)", "[[y(x) -> c(1)*cos(w*x) + c(2)*sin(w*x)]]"),
        ("d_solve(y''(x) + 4*y(x) == sin(x), y(x), x)", "[[y(x) -> (1/3)*sin(x) + c(1)*cos(2*x) + c(2)*sin(2*x)]]"),
        ("d_solve(y'''(x) - y(x) == x, y(x), x)", "[[y(x) -> -x + c(1)*exp(x) + c(2)*cos((1/2)*x*3^(1/2))*exp((-1/2)*x) + c(3)*exp((-1/2)*x)*sin((1/2)*x*3^(1/2))]]"),
        ("d_solve([y'''(x) + 5*y''(x) + 3*y'(x) + 6*y(x) == 3, y(0) == 1/2, y'(0) == 0, y''(0) == 0], y(x), x)", "[[y(x) -> 1/2]]"),
        ("d_solve(y''(x) - 4*y(x) == 0, y(x), x, generated_parameters: f)", "[[y(x) -> f(1)*exp(2*x) + f(2)*exp(-2*x)]]"),
        ("d_solve([y''(x) + y(x) == 0, y(0) == 0, y(pi/2) == 1], y(x), x)", "[[y(x) -> sin(x)]]"),
        ("d_solve([y'(x) + y(x) == a*sin(x), y(0) == 0], y(x), x)", "[[y(x) -> (-1/2)*a*(-1 + cos(x)*exp(x) + -1*exp(x)*sin(x))*exp(-x)]]"),
        ("d_solve(y'(x) + y(x)*tan(x) == sec(x), y(x), x)", "[[y(x) -> c(1)*cos(x) + sin(x)]]"),
        ("d_solve(y''(x) + y(x) == sec(x), y(x), x)", "[[y(x) -> x*sin(x) + c(1)*cos(x) + c(2)*sin(x) + cos(x)*log(cos(x))]]"),
        ("d_solve(x*y''(x) - (x + 1)*y'(x) + y(x) == 0, y(x), x)", "[[y(x) -> (-1 - x)*c(2) + c(1)*exp(x)]]"),
        ("d_solve(y''(x) - x*y(x) == 0, y(x), x)", "[[y(x) -> airy_ai(x)*c(1) + airy_bi(x)*c(2)]]"),
        ("d_solve(x^2*y''(x) + x*y'(x) + (x^2 - 4)*y(x) == 0, y(x), x)", "[[y(x) -> bessel_j(2, x)*c(1) + bessel_y(2, x)*c(2)]]"),
        ("d_solve((1 - x^2)*y''(x) - 2*x*y'(x) + (3/4)*y(x) == 0, y(x), x)", "[[y(x) -> c(1)*legendre_p(1/2, x) + c(2)*legendre_q(1/2, x)]]"),
        ("d_solve(x*y''(x) + (1/3 - x)*y'(x) - 2*y(x) == 0, y(x), x)", "[[y(x) -> c(1)*hypergeometric_1f1(2, 1/3, x) + c(2)*hypergeometric_u(2, 1/3, x)]]"),
        ("d_solve(y''(x) == (x^2 + 3)*y(x), y(x), x)", "[[y(x) -> x*c(1)*exp((1/2)*x^2) + -1*(x*erf(x)*pi^(1/2) + exp(-x^2))*c(2)*exp((1/2)*x^2)]]"),
        ("d_solve(y' == y)", "[[y -> (FormalX => exp(FormalX)*c(1))]]"),
        ("d_solve(y'(x) == 3*y(x))", "[[y(x) -> c(1)*exp(3*x)]]"),
    ]);
}

#[test]
fn nonlinear() {
    check(&[
        ("d_solve(y'(x) == y(x)^2, y(x), x)", "[[y(x) -> -(x + c(1))^-1]]"),
        ("d_solve(y'(x) == sin(y(x)*x), y(x), x)", "d_solve(y'(x) == sin(x*y(x)), y(x), x)"),
        ("d_solve(y'(x) + y(x)/x == x*y(x)^2, y(x), x)", "[[y(x) -> (-x^2 + x*c(1))^-1]]"),
        ("d_solve(y'(x) == y(x)^2 - 2/x^2, y(x), x)", "[[y(x) -> (-2*x^3 + 3*c(1))/(3*x*c(1) + x^4)]]"),
        ("d_solve(y'(x) == (x^2 + y(x)^2)/(x*y(x)), y(x), x)", "[[y(x) -> -(c(1)*x^2 + 2*log(x)*x^2)^(1/2)], [y(x) -> (c(1)*x^2 + 2*log(x)*x^2)^(1/2)]]"),
        ("d_solve(y(x) == x*y'(x) + y'(x)^2, y(x), x)", "[[y(x) -> (x + c(1))*c(1)]]"),
        ("d_solve(y'(x) == (x + y(x))^2, y(x), x)", "[[y(x) -> -x + tan(x + c(1))]]"),
        ("d_solve([x'(t) == x(t)^2 - 5*x(t) + 6, x(0) == 5/2], x(t), t)", "[[x(t) -> (3 + 2*exp(t))/(1 + exp(t))]]"),
        ("d_solve(y'(x) == exp(x + y(x)), y(x), x)", "[[y(x) -> -1 - log(-c(1) - exp(-1 + x))]]"),
        ("d_solve(y'(x) == 2*y(x)/x + x^2*y(x)^2, y(x), x)", "[[y(x) -> -5*x^2/(-5*c(1) + x^5)]]"),
        ("d_solve(y''(x) == y'(x)^2, y(x), x)", "[[y(x) -> c(2) - log(x + c(1))]]"),
    ]);
}

#[test]
fn systems_pde_delay() {
    check(&[
        ("d_solve([y'(x) == z(x), z'(x) == -y(x)])", "[[y(x) -> c(1)*cos(x) + c(2)*sin(x), z(x) -> c(2)*cos(x) + -1*c(1)*sin(x)]]"),
        ("d_solve([x'(t) == x(t) + y(t), y'(t) == 4*x(t) + y(t), x(0) == 1, y(0) == 0], [x(t), y(t)], t)", "[[x(t) -> (1/2)*(1 + exp(4*t))*exp(-t), y(t) -> (-1 + exp(t))*(1 + exp(t))*(1 + exp(2*t))*exp(-t)]]"),
        ("d_solve([y'(x) - 3*z(x) == sin(x), y(x) + z(x) == 1/5, y(pi/2) == 1/2], [y, z], x)", "[[y -> (x => (2 - cos(x) + 3*sin(x))/10), z -> (x => (cos(x) + -3*sin(x))/10)]]"),
        ("d_solve(d(u(x, y), x) + 3*d(u(x, y), y) + u(x, y) == 1, u, [x, y])", "[[u -> ((x, y) => 1 + exp(-x)*c(1)(-3*x + y))]]"),
        ("d_solve(d(u(x, y), x) - x*d(u(x, y), y) == 0, u(x, y), [x, y])", "[[u(x, y) -> c(1)(y + (1/2)*x^2)]]"),
        ("d_solve(d(u(x, t), [t, 2]) == d(u(x, t), [x, 2]))", "[[u(x, t) -> c(1)(t - x) + c(2)(t + x)]]"),
        ("d_solve([x'(t) == x(t - 1)^2, x(t when t <= 0) == a*t], x(t), [t, 0, 2])", "[[x(t) -> piecewise([[a*t, t <= 0], [(1/3)*(a^2*t^3 + -3*a^2*t^2 + 3*t*a^2), 0 < t and t <= 1], [(1/126)*(-163*a^4 + 42*a^2 + -1176*a^4*t^2 + -553*a^4*t^4 + -28*a^4*t^6 + 2*a^4*t^7 + 168*a^4*t^5 + 686*t*a^4 + 1064*a^4*t^3), 1 < t and t <= 2]], indeterminate)]]"),
    ]);
}

#[test]
fn values_and_series() {
    check(&[
        ("d_solve_value([y'(x) == -2*y(x), y(0) == 3], y(x), x)", "3*exp(-2*x)"),
        ("d_solve_value([y''(x) + y(x) == 0, y(0) == 0, y'(0) == 1], y(3), x)", "sin(3)"),
        ("d_solve_value([y''(x) + y(x) == 0, y(0) == 0, y'(0) == 1], y'(x), x)", "cos(x)"),
        ("d_solve_value([y'(x) == y(x), y(0) == 2], y, x)", "x => 2*exp(x)"),
        ("asymptotic_d_solve_value([y'(x) - y(x) == 0, y(0) == 1], y(x), [x, 0, 4])", "1 + x + (1/24)*x^4 + (1/6)*x^3 + (1/2)*x^2"),
        ("asymptotic_d_solve_value(y''(x) + y(x) == 0, y(x), [x, 0, 4])", "(x + (-1/6)*x^3)*c(2) + (1 + (-1/2)*x^2 + (1/24)*x^4)*c(1)"),
        ("asymptotic_d_solve_value([y''(x) + 2*y(x) == 0, y(3) == 1, y'(3) == 0], y(x), [x, 3, 4])", "1 - (-3 + x)^2 + (1/6)*(-3 + x)^4"),
        ("asymptotic_d_solve_value(2*x^2*y''(x) + 3*x*y'(x) - (x^2 + 1)*y(x) == 0, y(x), [x, 0, 4])", "(1 + (1/616)*x^4 + (1/14)*x^2)*c(1)*x^(1/2) + (1 + (1/40)*x^4 + (1/2)*x^2)*c(2)/x"),
        ("asymptotic_d_solve_value([y'(x) + eps*y(x) == cos(x), y(0) == 0], y(x), x, [eps, 0, 2])", "eps*(-1 + cos(x)) + (x - sin(x))*eps^2 + sin(x)"),
        ("asymptotic_d_solve_value([y''(x) - y(x) == 0, y(0) == 0, y'(0) == 1], y(x), x -> 0)", "x"),
    ]);
}

#[test]
fn recurrences() {
    check(&[
        ("r_solve(a(n + 1) == 2*a(n), a(n), n)", "[[a(n) -> c(1)*2^(-1 + n)]]"),
        ("r_solve_value([a(n) == 3*a(n - 1), a(0) == 2], a(n), n)", "2*3^n"),
        ("expand(r_solve_value([a(n + 2) == a(n + 1) + a(n), a(0) == 0, a(1) == 1], a(n), n) /. n -> 10)", "55"),
    ]);
}
