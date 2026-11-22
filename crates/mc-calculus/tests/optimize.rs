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
        ("minimize([x, x > 0], x)", "[0, [x -> indeterminate]]"),
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
        // a curve of minima (x^2 - 2 y == 1/2): its point with the free variable at 0, its value proved by the
        // critical values
        ("arg_min((x^2 - 2*y)^2 - x^2 + 2*y - 1, [x, y])", "[0, -1/4]"),
        ("min_value((x^2 - 2*y)^2 - x^2 + 2*y - 1, [x, y])", "-5/4"),
        // a boundary point: the critical points inside the curve x log(y) == 2 (where solve of the Lagrange system
        // fails) from y = exp(x exp(x)) substituted, x^2 exp(x) == 2
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

/// Critical points that solve returns as a family (periodic derivatives: `x -> ... c(1)`) are sampled, and the
/// range is accepted only when a numeric scan agrees; otherwise the call stays unevaluated.
#[test]
fn ranges_from_families_of_critical_points() {
    check(&[
        ("function_range(sin(x), x, y)", "-1 <= y and y <= 1"),
        ("function_range(fresnel_c(x), x, y)", "-fresnel_c(1) <= y and y <= fresnel_c(1)"),
        // continuous, with the values x at x = pi/2 + 2 k pi and -x at x = 3 pi/2 + 2 k pi: unbounded both ways
        ("function_range(x*sin(x), x, y)", "true"),
    ]);
}

/// Angles that enter through cos and sin only become points of the unit circle (cut to the arc of their range):
/// polar coordinates, trigonometric polynomials on boxes, one variable or several. The minimum of the distance from
/// the unit disk to (3, 4) is 16, not the value 20 at the corner r = 1, t = 0.
#[test]
fn angles_as_points_of_the_unit_circle() {
    check(&[
        ("minimize([(r*cos(t) - 3)^2 + (r*sin(t) - 4)^2, 0 <= r and r <= 1 and 0 <= t and t <= 2*pi], [r, t])", "[16, [r -> 1, t -> atan(4/3)]]"),
        ("maximize([(r*cos(t) - 3)^2 + (r*sin(t) - 4)^2, 0 <= r and r <= 1 and 0 <= t and t <= 2*pi], [r, t])", "[36, [r -> 1, t -> pi + atan(4/3)]]"),
        // an arc that does not hold the nearest direction: its end
        ("min_value([(r*cos(t) - 3)^2 + (r*sin(t) - 4)^2, 0 <= r and r <= 1 and pi/2 <= t and t <= pi], [r, t])", "18"),
        ("minimize([(cos(t) - 3)^2 + (sin(t) - 4)^2, 0 <= t and t <= 2*pi], t)", "[16, [t -> atan(4/3)]]"),
        ("max_value([cos(t) + sin(t), 0 <= t and t <= pi], t)", "2^(1/2)"),
        ("max_value([sin(x) + sin(y) + sin(x + y), 0 <= x and x <= 2*pi and 0 <= y and y <= 2*pi], [x, y])", "(3/2)*3^(1/2)"),
        ("arg_min([r*(cos(t) - 2*sin(t)), 1 <= r and r <= 2 and 0 <= t and t <= 2*pi], [r, t])", "[2, pi - atan(2)]"),
        ("min_value([sin(x)*sin(y)*sin(z), 0 <= x and x <= pi and 0 <= y and y <= pi and 0 <= z and z <= 2*pi], [x, y, z])", "-1"),
    ]);
}

/// A face whose critical points form a curve (where solve leaves unknowns free and can leave out isolated
/// solutions) is decided by its critical values; a face that is not solved fails with the reason instead of
/// giving the smallest value found elsewhere.
#[test]
fn every_face_of_the_constraints_is_accounted_for() {
    check(&[
        // the critical values -1, 0 and 1 of sin(x + y): -1 is attained on the line x + y = 3 pi/2
        ("min_value([sin(x + y), 0 <= x and x <= pi and 0 <= y and y <= pi], [x, y])", "-1"),
        (
            "minimize([b*d*f, a^2 + b^2 == 1 and c^2 + d^2 == 1 and g^2 + f^2 == 1 and b >= 0 and d >= 0], [a, b, c, d, g, f])",
            "[-1, [a -> 0, b -> 1, c -> 0, d -> 1, g -> 0, f -> -1]]",
        ),
        (
            "min_value([sin(x*y), 0 <= x and x <= 2 and 0 <= y and y <= 2], [x, y])",
            "failure(unsolved, min_value, \"the critical points on a face of the constraints are not found\")",
        ),
        // a system solved after one unknown is eliminated (exp(-r) cos(t) == 0 gives u == 0)
        ("min_value([exp(-r)*cos(t), 0 <= r and r <= 1 and 0 <= t and t <= 2*pi], [r, t])", "-1"),
        // a face whose critical points form a curve on which g is constant: the edge x = -1 of a linear fraction
        ("min_value([(2 - 3*y + 5*x)/(5 + y + 4*x), x + 2*y <= 4 and x + 3*y >= 6 and y <= 2.5 and x >= -1], [x, y])", "-3.0"),
    ]);
}
