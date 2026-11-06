//! Exact global optimization and instances.
mod common;
use common::check;

#[test]
fn minima_and_maxima() {
    check(&[
        ("minimize(x^2 - 4*x + 1, x)", "[-3, [x -> 2]]"),
        ("maximize(-x^4 + 2*x^2, x)", "[1, [x -> -1]]"),
        ("minimize(x^3 - 3*x, x)", "[-inf, [x -> -inf]]"),
        ("minimize([x^3 - 3*x, x >= -3 and x <= 3], x)", "[-18, [x -> -3]]"),
        ("maximize([x*(1 - x), 0 <= x and x <= 1], x)", "[1/4, [x -> 1/2]]"),
        ("minimize(x^2 + y^2 - 2*x + 4*y, [x, y])", "[-5, [x -> 1, y -> -2]]"),
        ("arg_min(x^2 + 2*x, x)", "-1"),
        ("min_value(x^2 + 2*x, x)", "-1"),
        // the infimum is not attained
        ("minimize(exp(x), x)", "minimize(exp(x), x)"),
    ]);
}

#[test]
fn instances() {
    check(&[
        ("find_instance(x^2 == 4 and x > 0, x)", "[[x -> 2]]"),
        ("find_instance(x^2 < 4 and x > 1, x)", "[[x -> 3/2]]"),
        ("find_instance(x + y == 3 and x - y == 1, [x, y])", "[[x -> 2, y -> 1]]"),
        ("find_instance(x^2 < 0, x)", "[]"),
    ]);
}

#[test]
fn ranges() {
    check(&[
        ("function_range(x^2, x, y)", "y >= 0"),
        ("function_range(x^3, x, y)", "true"),
        ("function_range([x^2 + 1, -1 <= x and x <= 2], x, y)", "1 <= y and y <= 5"),
        ("function_range(-x^2 + 4, x, y)", "y <= 4"),
    ]);
}
