//! Round 36: fixed wrong answers in numerics and statistics, one test per cause. Every value is
//! checked by a route other than the one that computes it (a closed form, the defining equation, a direct evaluation).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread with a big stack (one worker thread, fresh session); `TIMEOUT` after 120 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn find_root_of_a_surd() {
    // surd(x, 2) == 1.5 has the root 9/4; the root found satisfies the equation
    assert_eq!(ev("r = find_root(surd(x, 2) == 1.5, [x, 1]); abs(surd(rewrite(x, r), 2) - 1.5) < 1e-9 and abs(rewrite(x, r) - 9/4) < 1e-9"), "true");
    assert_eq!(ev("r = find_root(surd(x, 3) - 1.5, [x, 1]); abs(rewrite(x, r) - 27/8) < 1e-9"), "true");
}

#[test]
fn median_of_numeric_constants_and_of_weighted_data() {
    // 2 < e < pi: the middle element, compared by value
    assert_eq!(ev("median([pi, e, 2])"), "e");
    assert_eq!(ev("median([pi, 1, e])"), "e");
    assert_eq!(ev("median([pi, 4, e, 5])"), "(1/2)*(4 + pi)");
    // the cumulative weight reaches half the total (7/2) at the value 3
    assert_eq!(ev("median(weighted_data([1, 2, 3], [1, 1, 5]))"), "3");
    // equal weights agree with the median of the data
    assert_eq!(ev("median(weighted_data([4, 1, 3, 2], [1, 1, 1, 1])) == median([4, 1, 3, 2])"), "true");
    // exactly half the weight below the value 2: the mean of it and the next
    assert_eq!(ev("median(weighted_data([1, 2, 3], [1, 1, 2]))"), "5/2");
}

#[test]
fn centered_interval_of_a_function_is_the_tight_image() {
    // sin over [0.99, 1.01]: midpoint sin(1) cos(0.01), half-width cos(1) sin(0.01) (the angle-sum formulas)
    let out = ev("r = sin(centered_interval(1, 1/100)); c = r[1]; w = r[2]; [abs(c - sin(1) * cos(0.01)) < 1e-14, w >= cos(1) * sin(0.01), w < cos(1) * sin(0.01) * (1 + 1e-12)]");
    assert_eq!(out, "[true, true, true]");
    // exp is monotone: the image of [1.99, 2.01]
    let out = ev("r = exp(centered_interval(2, 1/100)); abs(r[1] - (exp(2.01) + exp(1.99)) / 2) < 1e-13 and r[2] >= (exp(2.01) - exp(1.99)) / 2 and r[2] < (exp(2.01) - exp(1.99)) / 2 * (1 + 1e-12)");
    assert_eq!(out, "true");
}

#[test]
fn centered_interval_products_are_enclosures() {
    // log(x) sin(x) over [10, 30] takes values up to +-log(30) = 3.40119738166...: the first-order estimate gave a
    // smaller radius (not an enclosure)
    let out = ev("r = (&(log(&1) * sin(&1)))(centered_interval(20, 10)); r[2] >= 3.4 and abs(r[1]) < 1e-9 and r[2] < 3.4012");
    assert_eq!(out, "true");
    // the exact ball product: (c1 + r1)(c2 + r2) at the ends
    let out = ev("p = centered_interval(2, 1/100) * centered_interval(3, 1/20); p[1] == 6.0 and p[2] >= 2*(1/20) + 3*(1/100) + (1/100)*(1/20)");
    assert_eq!(out, "true");
}

#[test]
fn midpoint_of_a_centered_interval_and_a_ball_literal() {
    assert_eq!(ev("normal(centered_interval(0.5, 0.1))"), "0.5");
    assert_eq!(ev("normal(around(2.5, 0.1))"), "2.5");
    // a number of 20 digits is a midpoint with a radius of its own rounding
    assert_eq!(ev("r = centered_interval(1.23~20); r[1] == 1.23 and r[2] < 1e-19"), "true");
    assert_eq!(ev("a = centered_interval(1.234); normal(a + pi) == 1.234 + n(pi)"), "true");
}

#[test]
fn an_indeterminate_argument_makes_an_interval_result_indeterminate() {
    assert_eq!(ev("interval([0, 1]) + indeterminate"), "indeterminate");
}

#[test]
fn rationalize_with_a_tolerance_takes_the_nearest_of_the_simplest() {
    // the integers 11 to 18 lie within 0.1 m = 3.937 in of 14.2 in: the nearest is 14
    assert_eq!(ev("rationalize(14.2, 3.937)"), "14");
    assert_eq!(ev("rationalize(quantity(14.2, \"Inches\"), quantity(1 / 10, \"Meters\"))"), "quantity(14, \"Inches\")");
    // 0 in the interval: the simplest is 0
    assert_eq!(ev("rationalize(0.3, 0.5)"), "0");
    assert_eq!(ev("rationalize(3.14159, 0.01)"), "22/7");
}

#[test]
fn lasso_fit_is_the_minimiser_of_the_penalised_residual() {
    // |y - c0 - c4 x^4|^2 + |c0| + |c4| is smallest at c0 = 1/2, c4 = 0 (objective 7/4); checked on a grid around it
    let out = ev("data = [[0., 0.], [0.001, 1], [0.01, 1], [0.1, 0], [1, 1]]; f = fit(data, [1, x, x^2, x^3, x^4], x, fit_regularization: [\"LASSO\", 1]); f == 0.5");
    assert_eq!(out, "true");
    let out = ev("ys = [0, 1, 1, 0, 1]; xs = [0., 0.001, 0.01, 0.1, 1]; obj = (a, b) => total(map(k => (ys[k] - a - b*xs[k]^4)^2, range(5))) + abs(a) + abs(b); [obj(0.5, 0) <= obj(0.4999389948918362, 0.0002293846183425472), obj(0.5, 0) == 1.75]");
    assert_eq!(out, "[true, true]");
}

#[test]
fn variance_of_complex_data_is_the_sum_of_squared_moduli() {
    // mean 2 + i/2, deviations -1 + i/2 and 1 - i/2: |d|^2 = 5/4 each, 5/2 in all (divisor n - 1 = 1)
    assert_eq!(ev("variance([1 + im, 3])"), "5/2");
    assert_eq!(ev("std_dev([1 + im, 3]) == sqrt(5/2)"), "true");
    // 1.5 + i: deviations -3/4 + i/2 and 3/4 - i/2, |d|^2 = 13/16 each, 13/8 in all
    assert_eq!(ev("abs(variance([1.5 + im, 3]) - 1.625) < 1e-12"), "true");
}

#[test]
fn real_exponent_of_inexact_numbers() {
    // the machine zero has the accuracy 1075 log10(2): the real exponent is minus that
    assert_eq!(ev("abs(real_exponent(0.) + 1075 * log(2) / log(10)) < 1e-9"), "true");
    // a 17-digit number: log10(123456789.987654321)
    assert_eq!(ev("abs(real_exponent(1.23456789987654321e8~17) - 8.091514980643623) < 1e-12"), "true");
    // an inexact zero: the real exponent is minus its accuracy, so that the precision is real_exponent + accuracy
    assert_eq!(ev("z = 0.~40; real_exponent(z) == -accuracy(z)"), "true");
    assert_eq!(ev("real_exponent(0)"), "-inf");
}

#[test]
fn solve_with_a_function_of_the_unknown_and_no_variable() {
    assert_eq!(ev("solve(sqrt(x) == 3/2)"), "[[x -> 9/4]]");
    assert_eq!(ev("solve(cube_root(x) == 3/2)"), "[[x -> 27/8]]");
    assert_eq!(ev("solve(cube_root(x) == -2, x)"), "[[x -> -8]]");
    // the solution satisfies the equation
    assert_eq!(ev("cube_root(27/8) == 3/2"), "true");
}

#[test]
fn find_peaks_of_quantities() {
    let src = "data = [quantity(9, \"Feet\"), quantity(86, \"Inches\"), quantity(27, \"Meters\"), quantity(79, \"Feet\"), quantity(90, \"Feet\"), quantity(41, \"Inches\"), quantity(99, \"Inches\"), quantity(6, \"Feet\"), quantity(38, \"Feet\"), quantity(89, \"Feet\"), quantity(53, \"Feet\"), quantity(85, \"Meters\"), quantity(92, \"Meters\"), quantity(95, \"Meters\"), quantity(49, \"Feet\")]; ";
    // above 30 m (98.43 ft) only the 95 m = 118750/381 ft at position 14 is a peak
    assert_eq!(ev(&format!("{src}find_peaks(data, automatic, automatic, quantity(30, \"Meters\"))")), "[[14, quantity(118750/381, \"Feet\")]]");
    // 27 m = 11250/127 ft at position 3 (a peak of the raw data)
    assert_eq!(ev(&format!("{src}find_peaks(data)[2]")), "[3, quantity(11250/127, \"Feet\")]");
}

#[test]
fn global_maximum_of_an_oscillating_function() {
    // the maximum on the unit disk found by a dense sample is 6.69222...; the objective is evaluated again at the
    // point returned
    let src = "f = (x, y) => e^sin(50 * x) + sin(60 * e^y) + sin(70 * sin(x)) + sin(sin(80 * y)) - sin(10 * (x + y)) + (1 / 4) * (x^2 + y^2); r = n_maximize([f(x, y), x^2 + y^2 <= 1], [x, y]); ";
    assert_eq!(ev(&format!("{src}abs(r[1] - 6.6922207863) < 1e-6")), "true");
    assert_eq!(ev(&format!("{src}abs(f(rewrite(x, r[2]), rewrite(y, r[2])) - r[1]) < 1e-9")), "true");
}

/// Optimisation answers that are exact or agree with the closed form to the stated tolerance.
#[test]
fn optimisation_answers_agree_with_closed_forms() {
    // maximum of x - 2y on the unit disk: sqrt(5)
    assert_eq!(ev("abs(find_max_value([x - 2 * y, x^2 + y^2 <= 1], [x, y]) - sqrt(5)) < 1e-12"), "true");
    // maximum of x + y on the disk: sqrt(2), at (1/sqrt(2), 1/sqrt(2))
    assert_eq!(ev("p = n_arg_max([x + y, x^2 + y^2 <= 1], [x, y]); abs(p[1] - 1/sqrt(2)) < 1e-12 and abs(p[2] - 1/sqrt(2)) < 1e-12"), "true");
    // x + 2y on x^2 + y^2 <= 9: 3 sqrt(5) at (3/sqrt(5), 6/sqrt(5))
    assert_eq!(ev("abs(n_max_value([x + 2 * y, x^2 + y^2 <= 9], [x, y]) - 3 * sqrt(5)) < 1e-12"), "true");
    // a linear program: x1 + 2 x2 >= 3, x1 >= -1, maximise -x1 - x2: the vertex (-1, 2), value -1
    // (the vertex (3, 0) has the smaller value -3)
    assert_eq!(ev("r = n_maximize([dot([-1, -1], x), vector_greater_equal([dot([[1, 2], [1, 0]], x) + [-3, 1], 0])], x); abs(r[1] + 1) < 1e-9"), "true");
}

#[test]
fn repeated_constraints_do_not_change_the_optimum() {
    // min x - y with x + 2y + z <= 0, x - 2z == 1, 2x - y >= 2: the vertex (9/11, -4/11, -1/11), value 13/11
    // (the two active constraints meet there; checked against n_minimize); a repeated equality gave (0, -2, -1/2)
    let out = ev("a = linear_optimization(x - y, [x + 2*y + z <= 0, x - 2*z == 1, 2*x - y >= 2, x - 2*z == 1], [x, y, z]); b = robust_convex_optimization(x - y, for_all(θ, -1 <= θ and θ <= 1, [x + 2 * y + z <= 1 + θ, x - 2 * z == 1, 2 * x - y >= 1 + θ]), [x, y, z]); [a == [x -> 9/11, y -> -4/11, z -> -1/11], b == a]");
    assert_eq!(out, "[true, true]");
}
