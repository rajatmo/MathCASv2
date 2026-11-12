//! Round 28: the functions of `lib/calculus.mc` and the derivative rows added for them.
mod common;
use common::check;

/// The unit-period waves at exact and inexact points, on lists and with ranges.
#[test]
fn waves() {
    check(&[
        ("triangle_wave(0.2)", "0.8"),
        ("triangle_wave(1/4)", "1"),
        ("triangle_wave(-1)", "0"),
        ("table(triangle_wave(x), [x, 0, 2, 2/3])", "[0, -2/3, 2/3, 0]"),
        ("triangle_wave([[2/3, -1/3], [1/2, -2/3]])", "[[-2/3, -2/3], [0, 2/3]]"),
        ("triangle_wave(x)", "triangle_wave(x)"),
        ("square_wave(0.2)", "1"),
        ("square_wave([0, 2], 1.6)", "0"),
        ("square_wave([0, 5], [0.4, 1.2, 3.6])", "[5, 5, 0]"),
        ("table(square_wave(x), [x, 0, 2, 2/3])", "[1, -1, 1, 1]"),
        ("sawtooth_wave(-1)", "0"),
        ("sawtooth_wave(1/4)", "1/4"),
        ("table(sawtooth_wave(x), [x, 0, 2, 2/3])", "[0, 2/3, 1/3, 0]"),
        ("d(sawtooth_wave(x), x)", "1"),
        ("d(square_wave(x), x)", "0"),
    ]);
}

/// The Heaviside triangle and box, their products and derivatives.
#[test]
fn heaviside_shapes() {
    check(&[
        ("heaviside_lambda(1/4)", "3/4"),
        ("heaviside_lambda([-3, -1, 0, 1/3, 1])", "[0, 0, 1, 2/3, 0]"),
        ("heaviside_lambda(1, pi, 5.3)", "0.0"),
        ("heaviside_lambda(x)", "heaviside_lambda(x)"),
        ("heaviside_pi([-3, -1, 0, 1/3, 1])", "[0, 0, 1, 1, 0]"),
        ("heaviside_pi(-1/2)", "heaviside_pi(1/2)"),
        ("heaviside_pi(1, pi, 5.3)", "0"),
        ("d(heaviside_lambda(x), [x, 2])", "dirac_delta(-1 + x) + dirac_delta(1 + x) + -2*dirac_delta(x)"),
    ]);
}

/// Alternating harmonic numbers and factorials, hyperharmonic and multiple harmonic numbers.
#[test]
fn harmonic_type() {
    check(&[
        ("table(alternating_harmonic_number(k), [k, 6])", "[1, 1/2, 5/6, 7/12, 47/60, 37/60]"),
        ("alternating_harmonic_number(0)", "0"),
        ("alternating_harmonic_number(inf)", "log(2)"),
        ("alternating_harmonic_number(inf, r, s)", "-polylog(r, -s)"),
        ("alternating_harmonic_number(2.0)", "0.5"),
        ("table(alternating_factorial(k), [k, 8])", "[1, 1, 5, 19, 101, 619, 4421, 35899]"),
        ("alternating_factorial(0)", "0"),
        ("abs(alternating_factorial(3.0) - 5) < 10^-12", "true"),
        ("hyper_harmonic_number(2, 4)", "77/12"),
        ("hyper_harmonic_number(2, 0)", "0"),
        ("hyper_harmonic_number(3, 4, 2)", "571/48"),
        ("abs(hyper_harmonic_number(2.0, 4.0) - 77/12) < 10^-12", "true"),
        ("multiple_harmonic_number(10, [2, 3], [1/4, 1/6])", "3562366081828274275871/1207910491447571251200000"),
        ("multiple_harmonic_number(10, [2, -4])", "32563619/75600"),
        ("multiple_harmonic_number(4, [1, 0])", "23/12"),
    ]);
}

/// The asymptotic relations, decided by limits of the ratio.
#[test]
fn asymptotic_relations() {
    check(&[
        ("asymptotic_less_equal(x, x^2, x -> inf)", "true"),
        ("asymptotic_less_equal(x^2, x, x -> inf)", "false"),
        ("asymptotic_less_equal(sin(x), x*unit_step(x), x -> 0, direction: from_above)", "true"),
        ("asymptotic_equal(x^2 + x*sin(x), 2*x^2 + 5, x -> inf)", "true"),
        ("asymptotic_equal(x, -x^2, x -> inf)", "false"),
        ("asymptotic_equal(x^2 + 3*y^2, 2*x^2 + 6*y^2 + x, [x, y] -> inf)", "true"),
        ("asymptotic_greater(x^2 + y^2, x + y, [x, y] -> inf)", "true"),
        ("asymptotic_equal(x, y, [x, y] -> inf)", "false"),
        ("asymptotic_equivalent(x^2 + 3*x + 2, x^2, x -> inf)", "true"),
        ("asymptotic_greater(1/x^2, 1/x, x -> 0)", "true"),
        ("asymptotic_less(x^2, 2*x^2, x -> inf)", "false"),
    ]);
}

/// Difference quotients, Wronskians, Casoratians and implicit derivatives.
#[test]
fn determinants_and_quotients() {
    check(&[
        ("difference_quotient(x^3, [x, h])", "3*x^2 + 3*h*x + h^2"),
        ("difference_quotient(c, [x, h])", "0"),
        ("wronskian([exp(x), exp(2*x)], x)", "exp(3*x)"),
        ("wronskian([sin(x), cos(x), x*sin(x), x*cos(x)], x)", "4"),
        ("wronskian(y''(x) - x*y(x) == 0, y, x)", "1"),
        ("casoratian([m, m^2, m^3], m)", "2*m*(1 + m)*(2 + m)"),
        ("casoratian([2^m, 2^(m + 3)], m)", "0"),
        ("implicit_d(2*y, x^2 + y^2 == 1, y, x)", "-2*x/y"),
    ]);
}

/// Series coefficients and the polynomial-structure functions of the library.
#[test]
fn polynomial_structure() {
    check(&[
        ("series_coefficient(series(exp(sin(x)), x, 0, 10), 8)", "31/5760"),
        ("series_coefficient(exp(x + y), [x, 0, 3], [y, 0, 3])", "1/36"),
        ("factor_list(2*x^3 + 2*x^2 - 2*x - 2)", "[[2, 1], [-1 + x, 1], [1 + x, 2]]"),
        ("factor_terms(3 + 6*x + 3*x^2)", "3*(1 + 2*x + x^2)"),
        ("exponent(1 + x^2 + a*x^3, x)", "3"),
        ("exponent(1 + x^2 + a*x^3, x, list)", "[0, 2, 3]"),
        ("collect(b*x^2 + 5*x + 7*x^2 + 9*a*x + 2, x)", "2 + x*(5 + 9*a) + (7 + b)*x^2"),
        ("cyclotomic(5, x)", "1 + x + x^2 + x^3 + x^4"),
        ("cyclotomic(12, x)", "1 - x^2 + x^4"),
        ("polynomial_mod(3*x^3 + 21*x^2 - 7*x + 55, [2*x^2 - 7, 9])", "7 + 8*x"),
        ("polynomial_mod(x^2, x + a)", "a^2"),
        ("polynomial_reduce(x^3 + y^3, [x - y], [x, y])", "[[x*y + x^2 + y^2], 2*y^3]"),
        ("polynomial_expression_q(x - 1/x, x)", "false"),
        ("possible_zero_q((e + pi)^2 - e^2 - pi^2 - 2*e*pi)", "true"),
        ("possible_zero_q(sqrt(x^2) - x)", "false"),
    ]);
}

/// Sides of relations, repeats, truncated sums, total derivatives and inverse functions.
#[test]
fn sides_repeats_and_more() {
    check(&[
        ("add_sides(x - 3 == 7, 3)", "x == 10"),
        ("divide_sides(3*x == 21, 3)", "x == 7"),
        ("divide_sides(a == b, c == d)", "piecewise([[a/c == b/d, c != 0]], a == b)"),
        ("multiply_sides(x > 1, -2)", "-2*x < -2"),
        ("subtract_sides(a == b)", "a - b == 0"),
        ("find_repeat([1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3])", "[1, 2, 3, 4]"),
        ("find_repeat(\"hellohellohello\")", "\"hello\""),
        ("find_transient_repeat([1, 2, 3, 4, 3, 4, 3, 4], 3)", "[[1, 2], [3, 4]]"),
        ("truncate_sum(sum(f(i), [i, 1, inf]), 3)", "f(1) + f(2) + f(3)"),
        ("dt(x*y)", "x*dt(y) + y*dt(x)"),
        ("dt(x^2*y, [x, 2])", "2*y + dt(y, [x, 2])*x^2 + 4*x*dt(y, x)"),
        ("dt(a*x^2 + b, x, constants: [a, b])", "2*a*x"),
        ("inverse_function(sin)", "asin"),
        ("inverse_function(t => 2*t + 3)(x)", "(1/2)*(-3 + x)"),
    ]);
}

/// Hankel, Hilbert and discrete-time Fourier transforms; upper and lower limits of sequences.
#[test]
fn transforms_and_sequence_limits() {
    check(&[
        ("hankel_transform(1/r^3, r, s)", "-s"),
        ("hankel_transform(1/sqrt(1 + r^2), r, s)", "exp(-s)/s"),
        ("hankel_transform(r^2*dirac_delta(r - 2), r, s)", "8*bessel_j(0, 2*s)"),
        ("hilbert_transform(sin(t), t, s)", "-cos(s)"),
        ("hilbert_transform(1/(1 + t^2), t, s)", "s/(1 + s^2)"),
        ("inverse_hilbert_transform(cos(s), s, t)", "-sin(t)"),
        ("fourier_sequence_transform(discrete_delta(k), k, w)", "1"),
        ("discrete_max_limit((-1)^k, k -> inf)", "1"),
        ("discrete_max_limit((3 + (24/25)^k)*sin(pi*k/20), k -> inf)", "3"),
        ("discrete_min_limit((-1)^k, k -> inf)", "-1"),
    ]);
}

/// Relations inside and, piecewise and conditional expressions; sequence asymptotics; algebraic numbers.
#[test]
fn relations_asymptotics_algebraics() {
    check(&[
        ("add_sides(a == b and b > 5, -5)", "-5 + a == -5 + b and -5 + b > 0"),
        ("add_sides(true, a)", "true"),
        ("expand(multiply_sides(a*x^2 + b*x + c == 0, 4*a, assumptions: a != 0))", "4*a*c + 4*a^2*x^2 + 4*a*b*x == 0"),
        ("discrete_asymptotic(binomial(2*k, k), k -> inf)", "2^(2*k)*k^(-1/2)*pi^(-1/2)"),
        ("element(pi, algebraics)", "false"),
        ("element((6 + im*sqrt(5))^(1/3), algebraics)", "true"),
        ("element(sqrt(2)^sqrt(2), algebraics)", "false"),
        ("element(euler_gamma, algebraics)", "element(euler_gamma, algebraics)"),
    ]);
}
