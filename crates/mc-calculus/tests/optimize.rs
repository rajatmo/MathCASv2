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
        // the infimum is a limit at an infinity
        ("minimize(exp(x), x)", "[0, [x -> -inf]]"),
        ("min_value(exp(x), x)", "0"),
        // not attained at an open end
        ("minimize([x, x > 0], x)", "minimize([x, x > 0], x)"),
    ]);
}

#[test]
fn constrained_several_variables() {
    check(&[
        ("minimize([x - 2*y, x^2 + y^2 <= 1], [x, y])", "[-5^(1/2), [x -> (-1/5)*5^(1/2), y -> (2/5)*5^(1/2)]]"),
        ("maximize(x + y, element([x, y], disk()))", "[2^(1/2), [x -> (1/2)*2^(1/2), y -> (1/2)*2^(1/2)]]"),
        ("minimize([x + 2*y, x^2 + 2*y^2 <= 3 and x + y == 2 and x >= 1], [x, y])", "[7/3, [x -> 5/3, y -> 1/3]]"),
        ("minimize([x*y, x^2 + y^2 <= 1 or (x + 1)^2 + (y - 1)^2 <= 2], [x, y])", "[-4, [x -> -2, y -> 2]]"),
        // a linear program and a linear-fractional one: vertices
        (
            "maximize([2*x + 3*y - z, 1 <= x + y + z and x + y + z <= 2 and 1 <= x - y + z and x - y + z <= 2 and x - y - z == 3], [x, y, z])",
            "[15/2, [x -> 5/2, y -> 1/2, z -> -1]]",
        ),
        (
            "min_value([(2*x + y - z)/(5*x - 7*y + 3), 0 <= x + y + z and x + y + z <= 1 and 1 <= x - y + z and x - y + z <= 2 and x - y - z == 3], [x, y, z])",
            "1/7",
        ),
        // a curve of minima: the free variable set to 0
        ("arg_min((x^2 - 2*y)^2 - x^2 + 2*y - 1, [x, y])", "[0, -1/4]"),
        // a boundary point found from the bounds alone
        (
            "arg_min([exp(x) + log(y), x*log(y) == 2 and -10 <= x and x <= 10 and 1/10 <= y and y <= 10], [x, y])",
            "[-2/log(10), 1/10]",
        ),
        // unbounded
        ("min_value([x, x*y <= 1], [x, y])", "-inf"),
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
