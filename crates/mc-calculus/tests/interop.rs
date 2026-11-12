//! Values of other functions flowing into this crate's: truncated series into `d` and
//! `integrate`, piecewise functions and functions defined by cases into `d` and definite integrals, delta
//! factors, iterated integrals, the `assumptions:` option, `d_solve` solutions into derivatives, and special
//! functions into `limit` and the function properties.
mod common;
use common::check;

#[test]
fn series_into_d_and_integrate() {
    check(&[
        ("s = series(tan(x), x, 0, 5); d(s, x)", "1 + x^2 + 2*x^4/3 + O(x^5)"),
        ("s = series(tan(x), x, 0, 5); integrate(s, x)", "x^2/2 + x^4/12 + x^6/45 + O(x^7)"),
        ("d(series(exp(a*x), x, 0, 2), a)", "x + a*x^2 + O(x^3)"),
        ("d(series(sin(x), x, 0, 5), [x, 2])", "-x + x^3/6 + O(x^4)"),
        // a function the series kernels do not know joins a series through its derivatives
        ("series(sin(x), x, 0, 3) + g(x)", "g(0) + (1 + g'(0))*x + (1/2)*g''(0)*x^2 + (-1/6 + (1/6)*g'''(0))*x^3 + O(x^4)"),
    ]);
}

#[test]
fn piecewise_into_d_and_integrate() {
    check(&[
        ("d(piecewise([[x^2, x < 1]], 2 - x), x)", "piecewise([[2*x, x < 1]], -1)"),
        ("integrate(piecewise([[x^2, x < 1]], 2 - x), [x, 0, 3])", "1/3"),
        ("integrate(abs(x^2 - 4), [x, 0, 3])", "23/3"),
        ("integrate(abs(sin(x)), [x, 0, 2*pi])", "4"),
        ("integrate(max(sin(x), cos(x)), [x, 0, pi])", "1 + 2^(1/2)"),
        ("integrate(unit_step(x - 1)*exp(-x), [x, 0, inf])", "exp(-1)"),
        ("integrate(sign(x - 1)*x, [x, 0, 3])", "7/2"),
        ("integrate(if x > 0 then x else -x, [x, -1, 2])", "5/2"),
        ("integrate(abs(x), [x, 1, -1])", "-1"),
    ]);
}

#[test]
fn delta_iterated_and_options() {
    check(&[
        ("integrate(dirac_delta(x - 1)*x^3, [x, 0, 2])", "1"),
        ("integrate(dirac_delta(x - 3)*x^3, [x, 0, 2])", "0"),
        ("integrate(dirac_delta(x^2 - 1)*x^2, [x, -inf, inf])", "1"),
        ("integrate(dirac_delta(2*x - 1)*exp(x), x)", "(1/2)*heaviside_theta(-1/2 + x)*exp(1/2)"),
        ("integrate(dirac_delta(x - a)*g(x), [x, -inf, inf], assumptions: element(a, reals))", "g(a)"),
        ("integrate(x*y^2, [x, 0, 2], [y, 0, 1])", "2/3"),
        ("integrate(x*sin(y), x, y)", "(-1/2)*cos(y)*x^2"),
        ("integrate(exp(-k*t), [t, 0, inf], assumptions: k > 0)", "k^-1"),
        ("integrate(x^2, [x, 0, 1], generate_conditions: false)", "1/3"),
        ("limit(exp(-k*t), t -> inf, assumptions: k > 0)", "0"),
        ("limit(1/x, x -> 0, direction: -1)", "inf"),
        ("d(x^2*y^3, [[x, y], 2])", "[[2*y^3, 6*x*y^2], [6*x*y^2, 6*y*x^2]]"),
    ]);
}

#[test]
fn solutions_special_functions_and_properties() {
    check(&[
        ("ds = d_solve(y'(x) == 2*y(x), y, x); rewrite(y'(x) - 2*y(x), ds[1])", "0"),
        ("derivative(2)(x => x^4)", "&(12*&1^2)"),
        ("limit(airy_bi(x), x -> inf)", "inf"),
        ("limit(fresnel_s(x), x -> -inf)", "-1/2"),
        ("limit(bessel_k(2, x), x -> inf)", "0"),
        ("function_analytic(fresnel_c(x), x)", "true"),
        ("function_domain(hermite_h(n, x), x)", "true"),
        ("function_analytic(floor(x), x)", "false"),
        ("function_analytic(log(x), x, assumptions: x > 0)", "true"),
    ]);
}

#[test]
fn cases_limits_numbers_and_relations() {
    check(&[
        ("limit(piecewise([[x^2, x < 0]], x + 1), x -> 0, direction: from_above)", "1"),
        ("limit(piecewise([[x^2, x < 0]], x + 1), x -> 0, direction: from_below)", "0"),
        ("limit(piecewise([[x^2, x < 0]], 2*x), x -> 0)", "0"),
        ("limit(max(x, x^2)/x^2, x -> inf)", "1"),
        ("apart(1 < (x + 1)/(x - 1) and (x + 1)/(x - 1) < 2)", "1 < 1 + 2/(-1 + x) and 1 + 2/(-1 + x) < 2"),
        ("d(factorial(x), x)", "digamma(1 + x)*factorial(x)"),
        ("d(laguerre_l(n, a, x), x)", "-laguerre_l(-1 + n, 1 + a, x)"),
        ("function_injective(fractional_part(x), x)", "false"),
    ]);
}
