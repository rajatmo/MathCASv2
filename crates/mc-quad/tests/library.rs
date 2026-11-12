//! The numerics library (`lib/numerics.mc`) on the assembled engine: each case is source text
//! and the printed value.
use mc_api::{Engine, StopFlag};

fn run(cases: &[(&str, &str)]) {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    let mut bad = vec![];
    for (src, want) in cases {
        let got = e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"));
        if &got != want {
            bad.push(format!("{src}\n   want {want}\n   got  {got}"));
        }
    }
    assert!(bad.is_empty(), "{} of {} cases differ:\n{}", bad.len(), cases.len(), bad.join("\n"));
}

#[test]
fn ratios_and_linear_recurrence() {
    run(&[
        ("ratios([a, b, c, d, e])", "[b/a, c/b, d/c, e/d]"),
        ("ratios([a, b, c, d, e], 2)", "[a*c*b^-2, b*d*c^-2, c*e*d^-2]"),
        ("ratios(map(i => 2^i, range(0, 10)))", "[2, 2, 2, 2, 2, 2, 2, 2, 2, 2]"),
        ("linear_recurrence([-3, 1], [7, 2], 10)", "[7, 2, 1, -1, 4, -13, 43, -142, 469, -1549]"),
        ("linear_recurrence([a, b], [1, 1], 4)", "[1, 1, a + b, b + a*(a + b)]"),
        ("linear_recurrence([1, 1], [1, 1], [-5, 5])", "[5, -3, 2, -1, 1, 0, 1, 1, 2, 3, 5]"),
        ("linear_recurrence([1, 1], [1, 1], [7])", "[13]"),
        ("linear_recurrence([1, 1], [1, 1, a, b], 6)", "[1, 1, a, b, a + b, a + 2*b]"),
        ("linear_recurrence([[[1, 1], [0, 1]], [[1, 0], [-1, 1]]], [[1, 2], [2, 1]], 6)", "[[1, 2], [2, 1], [4, 2], [8, 1], [13, -1], [20, -8]]"),
    ]);
}

#[test]
fn threshold_and_staircase() {
    run(&[
        ("threshold([1, 1/10, 1/100, 1/10^8, 1/10^11])", "[1, 1/10, 1/100, 1/100000000, 0]"),
        ("threshold([-3, 1, -2, 0, 2, -1, 0, 1, -3, 3, 2], 3/2)", "[-3, 0, -2, 0, 2, 0, 0, 0, -3, 3, 2]"),
        ("threshold([-3, 1, -2, 0, 2, -1, 0, 1, -3, 3, 2], [\"LargestValues\", 3])", "[-3, 0, 0, 0, 0, 0, 0, 0, -3, 3, 0]"),
        ("threshold([[0, 0.2, 0.4], [0.6, 0.8, 1]], 0.5)", "[[0, 0.0, 0.0], [0.6, 0.8, 1]]"),
        ("threshold([-1.0, -0.2, 0.5], [\"Soft\", 0.3])", "[-0.7, 0.0, 0.2]"),
        ("map(cantor_staircase, [1/3, 0.76, 0, 15/17, 1/4, 2])", "[1/2, 0.6875, 0, 3/4, 1/3, 1]"),
        ("cantor_staircase(x)", "cantor_staircase(x)"),
        ("[dirac_comb(1/2), dirac_comb(0), dirac_comb(4, 8, 1/3), dirac_comb(-x), dirac_comb(0.1)]", "[0, dirac_comb(0), 0, dirac_comb(x), 0]"),
    ]);
}

#[test]
fn mandelbrot_and_peaks() {
    run(&[
        ("mandelbrot_set_member_q([0, 1, im/2, im + 1, -2, 1/4, 0.26])", "[true, false, true, false, true, true, false]"),
        ("[mandelbrot_set_member_q(0.2501, max_iterations: 250), mandelbrot_set_member_q(0.2501, max_iterations: 500)]", "[true, false]"),
        ("find_peaks([1, 3, 5, 6, 6, 4, 3, 2, 4, 7, 3, 2, 4, 2, 2, 1])", "[[9/2, 6], [10, 7], [13, 4]]"),
        ("find_peaks([2, 1, 3, 5, 6, 6, 4, 3, 2, 4, 7, 3, 4, 2, 2, 1], 1)", "[[11/2, 6], [11, 7]]"),
        ("find_peaks([2, 1, 3, 5, 6, 6, 4, 3, 2, 4, 7, 2, 3, 4, 3, 2], 0, 3)", "[[11, 7]]"),
        ("find_peaks([2, 1, 3, 5, 6, 6, 4, 3, 2, 4, 7, 1, 4, 2, 2, 1], 0, 0, 5.1)", "[[11/2, 6], [11, 7]]"),
    ]);
}

#[test]
fn interpolating_polynomials() {
    run(&[
        ("interpolating_polynomial([1, 4, 9, 16], x)", "1 + (-1 + x)*(1 + x)"),
        ("interpolating_polynomial([[-1, 4], [0, 2], [1, 6]], x)", "4 + (-2 + 3*x)*(1 + x)"),
        ("expand(interpolating_polynomial([4, 7, 2, [8, 0], 9], x) - (4 + (-1 + x)*(3 + (-4 + (-3 + x)*(19/6 + (-4 + x)*(-107/36 + (109/72)*(-4 + x))))*(-2 + x))))", "0"),
        ("interpolating_polynomial([[-1, automatic, 0], [0, 1, 1], [1, automatic, 0]], x)", "1 + x + (-1/3)*x^3"),
        ("expand(interpolating_polynomial([[0, 1], [a, 0], [b, 0], [c, 0]], x)*a*b*c + (x - a)*(x - b)*(x - c))", "0"),
    ]);
}

#[test]
fn discrete_fourier() {
    run(&[
        ("chop(fourier([1, 1, 2, 2, 1, 1, 0, 0]) - n([2*sqrt(2), -1/2 + (1 + 1/sqrt(2))/sqrt(2)*im, 0, 1/2 - (1 - 1/sqrt(2))/sqrt(2)*im, 0, 1/2 + (1 - 1/sqrt(2))/sqrt(2)*im, 0, -1/2 - (1 + 1/sqrt(2))/sqrt(2)*im]), 10^-12)", "[0, 0, 0, 0, 0, 0, 0, 0]"),
        ("chop(inverse_fourier(fourier([1, 2, 3, 4, 5, 6])) - [1, 2, 3, 4, 5, 6], 10^-12)", "[0, 0, 0, 0, 0, 0]"),
        ("chop(fourier([1, 0, 1, 0, 0, 1, 0, 0, 0, 1], fourier_parameters: [1, 1])[1] - 4, 10^-12)", "0"),
        ("chop(fourier([[1, 0], [0, 1]]) - [[1, 0], [0, 1]], 10^-12)", "[[0, 0], [0, 0]]"),
        ("precision(fourier(n([1, 0, 0, 1], 30))[2]) > 25", "true"),
    ]);
}

#[test]
fn fixed_points() {
    run(&[
        ("d_fixed_points(x'(t) == 2*x(t) + 1, x(t), t)", "[[-1/2]]"),
        ("d_fixed_points(x'(t) == x(t)^2 + 3*x(t), x, t)", "[[-3], [0]]"),
        ("d_fixed_points(y'''(x) + 5*y''(x) + 3*y'(x) + 6*y(x) == 3, y, x)", "[[1/2, 0, 0]]"),
        ("d_fixed_points([x'(t) == b*x(t) + y(t), y'(t) == -2*x(t) + a*y(t)], [x, y], t)", "[[0, 0]]"),
        ("r_fixed_points(a(n + 1) == -(a(n) + 15)/(a(n) + 7), a(n), n)", "[[-5], [-3]]"),
        ("r_fixed_points(a(n + 2) == a(n + 1)*a(n)^2, a(n), n)", "[[-1, -1], [0, 0], [1, 1]]"),
        ("r_fixed_points(a(n + 2) == b^n*a(n + 1)*a(n)^2, a(n), n)", "[[0, 0]]"),
        ("r_fixed_points([y(n + 1) + z(n) == 3, y(n) + 2*z(n + 1) == 1], [y(n), z(n)], n)", "[[5, -2]]"),
    ]);
}

#[test]
fn parametric_solutions() {
    run(&[
        ("sol = parametric_nd_solve([y'(t) == a*y(t), y(0) == 1], y, [t, 0, 1], [a]); abs(rewrite(y(1), sol)(1) - e) < 10^-6", "true"),
        ("f = parametric_nd_solve_value([y''(t) + a*y(t) == 0, y(0) == 1, y'(0) == 0], y(10), [t, 0, 10], [a]); abs(rewrite(a, find_root(f(a), [a, 1])) - 1.20902653) < 10^-6", "true"),
        ("fourier([1, 2])", "[2.121320343559643, -0.7071067811865476]"),
        ("is_real(fourier([1, 2, 3])[1])", "is_real(complex(3.4641016151377544, 0.0))"),
        ("inverse_fourier(fourier([1, 1, 0, 0, 0]))[1] - 1 < 10^-12", "true"),
    ]);
}

#[test]
fn fourier_coefficients_and_convolutions() {
    run(&[
        ("fourier_coefficient(t^2 + t, t, 5)", "complex(-2/25, -1/5)"),
        ("fourier_coefficient(exp(3*im*t), t, n)", "discrete_delta(-3 + n)"),
        ("fourier_coefficient(1, t, n)", "discrete_delta(n)"),
        ("simplify(fourier_coefficient(t^2 + t, t, n) - piecewise([[pi^2/3, n == 0]], (2 + im*n)*(-1)^n/n^2))", "0"),
        ("fourier_sin_series(t, t, 2)", "-sin(2*t) + 2*sin(t)"),
        ("unilateral_convolve(tau, exp(tau), tau, t)", "-1 - t + exp(t)"),
        ("unilateral_convolve(dirac_delta(tau), f(tau), tau, t)", "f(t)"),
        ("unilateral_convolve(a*r(tau), s(tau), tau, t)", "a*unilateral_convolve(r(tau), s(tau), tau, t)"),
        ("abs(mellin_convolve(exp(-x), 1/(x + 1), x, 0.3) - 1.2225356050805858) < 10^-10", "true"),
        ("mellin_convolve(unit_box(x - 1), x, x, y)", "(4/3)*y"),
    ]);
}

#[test]
fn interpolation_and_means() {
    run(&[
        ("f = list_interpolation([1, 2, 3, 5, 8, 5]); f(2.5)", "2.4375"),
        ("g = list_interpolation([1, 2, 3, 5, 8, 5], [[0, 1]]); abs(g(0.5) - 3.875) < 10^-12", "true"),
        ("k = list_interpolation([0, 0.3, 0.6, -0.2, 3], [[0, 0.1, 0.5, 1, 2]]); abs(k(0.3) - 0.6073333333333333) < 10^-12", "true"),
        ("h = list_interpolation([[1, 2, 3, 4], [2, 2, 6, 4], [3, 6, 3, 12], [4, 4, 12, 4]]); h(2.5, 2.5)", "3.953125"),
        ("fi = function_interpolation(sin(x)*y^2, [x, 0, 2], [y, 0, 2]); abs(fi(1.0, 1.5) - sin(1.0)*2.25) < 10^-6", "true"),
        ("mean_around([1, 2, 3, 4, 3, 2, 1])", "around(2.2857142857142856, 0.420560041253707)"),
        ("mean_around([[1, 1], [0, 2], [3, 1]])", "vector_around([4/3, 4/3], [[7/9, -2/9], [-2/9, 1/9]])"),
        ("fourier_cos_coefficient(x^2*y, [x, y], [3, 5])", "(16/225)/pi"),
        ("fourier_sin_coefficient(sin(3*t), t, n)", "discrete_delta(-3 + n)"),
        ("fourier_coefficient(exp(im*a*t), t, n, assumptions: element(a, integers))", "discrete_delta(n - a)"),
    ]);
}

#[test]
fn random_polygons() {
    run(&[
        ("seed(7); p = random_polygon(5); [head(p), length(p[1]), every(flatten(p[1]), x => 0 <= x and x <= 1)]", "[polygon, 5, true]"),
        ("dimensions(random_polygon(8, [2, 2]))", "[2, 2]"),
        ("length(random_polygon(3 -> 10)[1][1])", "3"),
        ("q = random_polygon([\"Convex\", 9])[1]; length(q) == length(nl_hull(q))", "true"),
        ("f = list_interpolation(map(x => [sin(8*x), 8*cos(8*x)], range(0, 3, 0.5)), [[0, 3]]); abs(f(1.0) - sin(8.0)) < 10^-12", "true"),
    ]);
}
