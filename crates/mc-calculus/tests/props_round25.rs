//! Round 25: discontinuities, continuity, meromorphy, poles, periods, surjectivity and
//! bijectivity, piecewise expansion, and the sign, monotonicity and convexity with parameters.
mod common;
use common::check;

#[test]
fn discontinuities_and_singularities() {
    check(&[
        ("function_discontinuities(tan(x), x)", "cos(x) == 0"),
        ("function_discontinuities(log(x), x, complexes)", "x == 0 or im_part(x) == 0 and re(x) <= 0"),
        ("function_discontinuities(beta(x, y), [x, y])", "sin(pi*x) == 0 and x <= 0 or sin(pi*y) == 0 and y <= 0"),
        ("function_discontinuities(gamma(x), x)", "sin(pi*x) == 0 and x <= 0"),
        ("function_discontinuities(floor(1/(x^2 - 1)), x)", "x^2 == 1 or sin(pi/(-1 + x^2)) == 0"),
        ("function_discontinuities(sqrt(x), x)", "x <= 0"),
        ("function_discontinuities(atan(z^2), z, complexes)", "-1im + z^2 == 0 or 1im + z^2 == 0 or re(z^2) == 0 and im_part(z^2) >= 1 or re(z^2) == 0 and im_part(z^2) <= -1"),
        ("function_discontinuities(x + cos(x)*unit_step(sin(x)), x)", "sin(x) == 0"),
        ("function_discontinuities(max(log(abs(x) + 1), x*sin(x)), x)", "abs(x) <= -1"),
        ("function_discontinuities(log(z^3), z, complexes)", "z == 0 or im_part(z^3) == 0 and re(z^3) <= 0"),
        ("function_discontinuities(g(x), x)", "false"),
        ("function_discontinuities(1/x + 1/(x - 1) + log(x + 5), x)", "x <= -5 or x == 0 or x == 1"),
        ("function_singularities(max(log(abs(x) + 1), x*sin(x)), x)", "x == 0 or abs(x) <= -1 or log(1 + abs(x)) + -1*x*sin(x) == 0"),
        ("function_singularities(atanh(z^4), z, complexes)", "z^4 == -1 or z^4 == 1 or im_part(z^4) == 0 and re(z^4) >= 1 or im_part(z^4) == 0 and re(z^4) <= -1"),
        ("function_singularities(1/x + log(x - 1), x)", "x <= 1"),
        ("function_singularities([tanh(x - y), coth(x + y)], [x, y], complexes)", "cosh(x - y) == 0 or sinh(x + y) == 0"),
    ]);
}

#[test]
fn continuity_and_meromorphy() {
    check(&[
        ("function_continuous(sin(x), x)", "true"),
        ("function_continuous(sqrt(x), x, complexes)", "false"),
        ("function_continuous([log(x), x > 0], x)", "true"),
        ("function_continuous(floor(x), x)", "false"),
        ("function_continuous([tan(x), -pi/2 < x and x < pi/2], x)", "true"),
        ("function_continuous([log(z), im_part(z) != 0], z)", "true"),
        ("function_continuous(erf(sin(x + y)/(x^2 + y^2 - 1)), [x, y])", "false"),
        ("function_continuous(erf(sin(x + y)/(x^2 + y^2 + 1)), [x, y], complexes)", "false"),
        ("function_continuous([erf(sin(x + y)/(x^2 + y^2 + 1)), x^2 + y^2 != -1], [x, y], complexes)", "true"),
        ("function_continuous(gamma(x^2 + a), x)", "conditional_expression(true, a > 0)"),
        ("function_continuous(log(x^2 + a), x, generate_conditions: false)", "true"),
        ("function_continuous(cos(x*y)/a, [x, y])", "conditional_expression(true, element(a, reals) and a != 0)"),
        ("function_continuous(cos(x*y)/a, [x, y], generate_conditions: automatic)", "conditional_expression(true, element(a, reals))"),
        ("function_continuous(sin(x + y)/(exp(x*y) + a), [x, y], assumptions: a > 0)", "true"),
        ("function_meromorphic(tan(x), x)", "true"),
        ("function_meromorphic(log(x), x)", "false"),
        ("function_meromorphic(pochhammer(x, y), [x, y])", "true"),
        ("function_meromorphic([gamma(x, y), re(y) > 0], [x, y])", "true"),
        ("function_meromorphic([log(z + a), re(z) > 0], z)", "conditional_expression(true, re(a) >= 0)"),
        ("function_meromorphic(sin(z)^a, z, assumptions: element(a, positive_integers))", "true"),
        ("function_meromorphic(sin(z)^a, z)", "function_meromorphic(sin(z)^a, z)"),
        ("function_meromorphic(acsch(z), z)", "false"),
        ("function_analytic(laguerre_l(exp(x + z), sin(y*z)), [x, y, z], complexes)", "true"),
    ]);
}

#[test]
fn poles_and_periods() {
    check(&[
        ("function_poles((x^3 - 2)/((x - 1)*(x^2 - 1)), x)", "[[-1, 1], [1, 2]]"),
        ("function_poles(tan(x) + 1, x)", "[[conditional_expression((1/2)*pi + pi*c(1), element(c(1), integers)), 1]]"),
        ("function_poles((1 - cos(x))/x^2, x)", "[]"),
        ("function_poles(log(x) + 1/(x - 1), x)", "function_poles(log(x) + (-1 + x)^-1, x)"),
        ("function_poles([log(x) + 1/(x - 1), re(x) > 0], x)", "[[1, 1]]"),
        ("function_poles([1/(x - a), re(x) > 0], x, assumptions: re(a) <= 0)", "[]"),
        ("function_poles((exp(x) - 1 - x)/x^2 + sin(x - 1)/(x^2 - 1)^2, x)", "[[-1, 2], [1, 1]]"),
        ("function_poles([tan(x)^3 + x, abs(x) < 2], x)", "[[(-1/2)*pi, 3], [(1/2)*pi, 3]]"),
        ("function_poles([1/(exp(x) - 2), abs(x) < 2], x)", "[[log(2), 1]]"),
        ("function_period(sin(x), x)", "2*pi"),
        ("function_period(mod(n/2 + 7, 11), n, integers)", "22"),
        ("function_period(((-1)^(2/7))^n, n, integers)", "7"),
        ("function_period(cosh(n*2*pi*(im/10)), n, integers)", "10"),
        ("function_period(tan(n*pi/10 + 1), n)", "10"),
        ("function_period(exp(z), z, complexes)", "[2im*pi]"),
        ("assuming(a > 0, function_period(sin(a*x), x))", "2*pi/a"),
        ("function_period([tan(x + y), cos(x - 2*y)], [x, y])", "[2*pi, pi]"),
        ("function_period(sin(x)^2, x)", "pi"),
        ("function_period(sin(x) + sin(sqrt(2)*x), x)", "0"),
    ]);
}

#[test]
fn surjective_bijective_injective() {
    check(&[
        ("function_surjective(tan(x), x)", "true"),
        ("function_surjective(tan(x), x, complexes)", "false"),
        ("function_surjective([x^3 - x, x > 0], x)", "false"),
        ("function_surjective([x^3 - x, x > 0, y > 0], x, y)", "true"),
        ("function_surjective([exp(x), true, y != 0], x, y, complexes)", "true"),
        ("function_surjective([x/2 + 1, element(x, integers), element(y, integers)], x, y)", "true"),
        ("function_surjective(x^2 + x*y + y^2, [x, y])", "false"),
        ("function_surjective([x + y^2, y - x^2], [x, y], complexes)", "true"),
        ("function_surjective(a*x^2 + b*x + c, x)", "conditional_expression(false, element(a | b | c, reals) and a != 0)"),
        ("function_surjective(log(x^2), x)", "true"),
        ("function_surjective(csch(x), x)", "false"),
        ("function_bijective(sinh(x), x)", "true"),
        ("function_bijective(sinh(x), x, complexes)", "false"),
        ("function_bijective([log(x), true, -pi < im_part(y) and im_part(y) <= pi], x, y, complexes)", "true"),
        ("function_bijective(x^3 + a*x + b, x)", "conditional_expression(true, element(a | b, reals) and a > 0)"),
        ("function_bijective(a*x, x, complexes)", "conditional_expression(true, a != 0)"),
        ("function_bijective([x + 2*y + 3*z, 4*x + 5*y + 6*z, 7*x + 8*y + 9*z], [x, y, z])", "false"),
        ("function_injective([x^2 + 2*x, x^3 - x], x)", "true"),
        ("function_injective([x^2 + x, x^3 - x], x)", "false"),
        ("function_injective(exp(x), x, complexes)", "false"),
        ("function_injective([x^2, re(x) >= 0, im_part(y) != 0], x, y, complexes)", "true"),
        ("function_injective([x^2, re(x) >= 0], x, y, complexes)", "false"),
    ]);
}

#[test]
fn shapes_with_parameters() {
    check(&[
        ("function_sign(x^2 + a*x + b, x)", "conditional_expression(1, -4*b + a^2 <= 0)"),
        ("function_sign(x^2 + a*x*y + y^2, [x, y])", "conditional_expression(1, -2 <= a and a <= 2)"),
        ("function_sign(x*y, [x, y])", "indeterminate"),
        ("function_sign(x^2/a^2, x, generate_conditions: automatic)", "conditional_expression(1, element(a, reals))"),
        ("function_sign(sin(x) + 1, x, strict_inequalities: true)", "indeterminate"),
        ("function_sign(log(x)^2, x)", "indeterminate"),
        ("function_monotonicity(a*x + 1, x)", "conditional_expression(1, a > 0)"),
        ("function_monotonicity(exp(a*x), x)", "conditional_expression(1, a > 0)"),
        ("function_monotonicity(x/a^2, x)", "conditional_expression(1, element(a, reals) and a != 0)"),
        ("function_monotonicity(floor(x), x, strict_inequalities: true)", "indeterminate"),
        ("function_monotonicity(x^α, x, positive_reals, assumptions: α > 0)", "1"),
        ("function_convexity(-exp(x + y), [x, y])", "-1"),
        ("function_convexity(x^2 - y^2, [x, y])", "indeterminate"),
        ("function_convexity(x - sqrt(x^2), x)", "-1"),
        ("function_convexity(a*x^2 + b*y^2, [x, y])", "conditional_expression(1, a >= 0 and b >= 0)"),
        ("function_convexity(x^4 + a*x^2, x, assumptions: a < 0)", "indeterminate"),
        ("map(&(function_convexity(&1, [x, y], strict_inequalities: true)), [x^2, x^2 + y^2])", "[indeterminate, 1]"),
    ]);
}

#[test]
fn piecewise_expansion() {
    check(&[
        ("piecewise_expand(piecewise([[piecewise([[1, x < 1]], 2), x > 0]], 3))", "piecewise([[1, 0 < x and x < 1], [2, x >= 1]], 3)"),
        ("piecewise_expand(if x > 0 then max(x, y) else unit_step(y))", "piecewise([[1, x <= 0 and y >= 0], [x, x > 0 and x - y >= 0], [y, x > 0 and x - y < 0]], 0)"),
        ("piecewise_expand(floor(x), 0 < x and x < 4)", "piecewise([[1, 1 <= x and x < 2], [2, 2 <= x and x < 3], [3, x >= 3]], 0)"),
        ("piecewise_expand(prime_pi(x), 0 < x and x < 6)", "piecewise([[1, 2 <= x and x < 3], [2, 3 <= x and x < 5], [3, x >= 5]], 0)"),
        ("assuming(0 < x and x < 3, piecewise_expand(floor(x)))", "piecewise([[1, 1 <= x and x < 2], [2, x >= 2]], 0)"),
        ("piecewise_expand(unit_step(x + 1) - unit_step(x - 1)*(sin(x)^2 + cos(x)^2) + 1)", "piecewise([[1, x < -1], [2, -1 <= x and x < 1]], 2 - cos(x)^2 - sin(x)^2)"),
        ("piecewise_expand(piecewise([[piecewise([[piecewise([[4, a]], 2), b]], 3), c]], 1))", "piecewise([[4, c and b and a], [2, c and b], [3, c]], 1)"),
        ("piecewise_expand(if a then b else c)", "piecewise([[b, a]], c)"),
        ("piecewise_expand(clip(x, [a, b]))", "piecewise([[a, a - x > 0], [b, b - x < 0 and a - x <= 0]], x)"),
        ("piecewise_expand(-1 <= max(x, y) and max(x, y) <= 1)", "piecewise([[-1 <= x and x <= 1, x - y >= 0]], -1 <= y and y <= 1)"),
        // a step function of an unbounded variable stays whole
        ("piecewise_expand(floor(x) + round(y), assumptions: y^2 < 2)", "piecewise([[-1 + floor(x), y < -1/2], [floor(x), -1/2 <= y and y <= 1/2]], 1 + floor(x))"),
    ]);
}
