//! Exact printed derivatives through the engine: every rule of the table, the structural rules, the derivative
//! operator, orders, gradients, and the failures.
mod common;
use common::check;

#[test]
fn elementary_functions() {
    check(&[
        ("d(exp(x), x)", "exp(x)"),
        ("d(exp(x^2), x)", "2*x*exp(x^2)"),
        ("d(log(x), x)", "x^-1"),
        ("d(log(x^2 + 1), x)", "2*x/(1 + x^2)"),
        ("d(sqrt(x), x)", "(1/2)*x^(-1/2)"),
        ("d(sqrt(1 - x^2), x)", "-1*x*(1 - x^2)^(-1/2)"),
        ("d(sin(x), x)", "cos(x)"),
        ("d(cos(3x), x)", "-3*sin(3*x)"),
        ("d(tan(x), x)", "cos(x)^-2"),
        ("d(asin(x), x)", "(1 - x^2)^(-1/2)"),
        ("d(acos(x), x)", "-(1 - x^2)^(-1/2)"),
        ("d(atan(x), x)", "(1 + x^2)^-1"),
        ("d(sinh(2x), x)", "2*cosh(2*x)"),
        ("d(cosh(x), x)", "sinh(x)"),
        ("d(tanh(x), x)", "cosh(x)^-2"),
        ("d(log(sin(x)), x)", "cos(x)/sin(x)"),
        ("d(1/(1 + exp(-x)), x)", "exp(-x)*(1 + exp(-x))^-2"),
    ]);
}

#[test]
fn piecewise_and_complex_parts() {
    check(&[
        ("d(abs(x), x)", "sign(x)"),
        ("d(abs(x^2 - 1), x)", "2*x*sign(-1 + x^2)"),
        ("d(sign(x), x)", "0"),
        ("d(unit_step(x), x)", "0"),
        ("d(arg(x), x)", "0"),
        ("d(floor(x) + ceiling(x) + round(x), x)", "0"),
        ("d(quotient(x, 2), x)", "0"),
        ("d(floor(x, a), a)", "floor(x/a)"),
        ("d(ceiling(x, a), a)", "ceiling(x/a)"),
        ("d(round(x, a), a)", "round(x/a)"),
        ("d(mod(x, 3), x)", "1"),
        ("d(mod(x, m), m)", "-floor(x/m)"),
        ("d(mod(a, x, c), x)", "-floor((a - c)/x)"),
        ("d(mod(a, m, x), x)", "0"),
        ("d(max(x, 2), x)", "if x >= 2 then 1 else 0"),
        ("d(min(x^2, y), y)", "if x^2 <= y then 0 else 1"),
        ("d(if x > 0 then x^2 else -x, x)", "if x > 0 then 2*x else -1"),
        ("d(re(x^2), x)", "re(2*x)"),
        ("d(im_part(sin(x)), x)", "im_part(cos(x))"),
        ("d(conjugate(x^3), x)", "conjugate(3*x^2)"),
    ]);
}

#[test]
fn sums_products_powers() {
    check(&[
        ("d(5, x)", "0"),
        ("d(x, x)", "1"),
        ("d(y, x)", "0"),
        ("d(pi*x + e, x)", "pi"),
        ("d(x^3 + sin(x), x)", "cos(x) + 3*x^2"),
        ("d(sin(x)*cos(x), x)", "-sin(x)^2 + cos(x)^2"),
        ("d(sin(x)^2, x)", "2*cos(x)*sin(x)"),
        ("d(1/x, x)", "-x^-2"),
        ("d(x/(1 + x^2), x)", "-2*x^2*(1 + x^2)^-2 + (1 + x^2)^-1"),
        ("d(sin(x)/x, x)", "cos(x)/x + -1*x^-2*sin(x)"),
        ("d(x^(1/3), x)", "(1/3)*x^(-2/3)"),
        ("d(x^n, x)", "n*x^(-1 + n)"),
        ("d(x^x, x)", "(1 + log(x))*x^x"),
        ("d(x^y, y)", "log(x)*x^y"),
        ("d(2^x, x)", "log(2)*2^x"),
        ("d(e^(2x), x)", "2*e^(2*x)"),
        ("d(2.5*x^2, x)", "5.0*x"),
        ("d(x^2.5, x)", "2.5*x^1.5"),
        ("d(atan(1/x), x)", "-1*x^-2/(1 + x^-2)"),
    ]);
}

#[test]
fn unknown_functions_and_the_operator() {
    check(&[
        ("d(f(x), x)", "f'(x)"),
        ("d(f(g(x)), x)", "f'(g(x))*g'(x)"),
        ("d(f(x)*g(x), [x, 2])", "f(x)*g''(x) + g(x)*f''(x) + 2*f'(x)*g'(x)"),
        ("d(f'(x), x)", "f''(x)"),
        ("d(f(x), x, x)", "f''(x)"),
        ("d(f'(g'(x)), x)", "f''(g'(x))*g''(x)"),
        ("d(derivative(4)(f)(x), x)", "derivative(5)(f)(x)"),
        ("d(f(y), x)", "0"),
        ("d(f'(y), x)", "0"),
        ("d(f(x, y), x)", "derivative(1, 0)(f)(x, y)"),
        ("d(f(x, y), x, y)", "derivative(1, 1)(f)(x, y)"),
        ("d(f(x, x), x)", "derivative(0, 1)(f)(x, x) + derivative(1, 0)(f)(x, x)"),
        ("d(derivative(2, 1)(f)(x, y), y)", "derivative(2, 2)(f)(x, y)"),
        ("d(factorial(x), x)", "factorial'(x)"),
        ("d(g(1)(x), x)", "derivative(1)(g(1))(x)"),
        // the operator on a known function resolves when differentiated
        ("d(sin'(x), x)", "-sin(x)"),
        ("d(sin''(x), x)", "-cos(x)"),
        ("d(exp'(2*x), x)", "2*exp(2*x)"),
        ("d(derivative(1)(log)(x^2), x)", "-2*x^-3"),
        // implicit differentiation of an equation
        ("d(x^2 + y(x)^2 == 1, x)", "2*x + 2*y(x)*y'(x) == 0"),
        // a variable that is a call
        ("d(y(t)^2, y(t))", "2*y(t)"),
        ("d(y(t)^2 + t, t)", "1 + 2*y(t)*y'(t)"),
    ]);
}

#[test]
fn orders_mixed_and_gradients() {
    check(&[
        ("d(f(x), [x, 0])", "f(x)"),
        ("d(x^3, [x, 2])", "6*x"),
        ("d(x^10, [x, 11])", "0"),
        ("d(log(x), [x, 3])", "2*x^-3"),
        ("d(sin(x), [x, 4])", "sin(x)"),
        ("d(cos(x), [x, 2])", "-cos(x)"),
        ("d(exp(x), [x, 100000])", "exp(x)"),
        ("d(x^2*y^3, x, y)", "6*x*y^2"),
        ("d(x^2*y^3, [x, 2], [y, 2])", "12*y"),
        ("d(x^y, x, y)", "y*log(x)*x^(-1 + y) + x^(-1 + y)"),
        ("d(x^y, y, x)", "y*log(x)*x^(-1 + y) + x^(-1 + y)"),
        ("d(exp(x*y), x, y)", "exp(x*y) + x*y*exp(x*y)"),
        ("d(x^2*y^3, [x, y])", "[2*x*y^3, 3*x^2*y^2]"),
        ("d(x*y*z, [x, y, z])", "[y*z, x*z, x*y]"),
        ("d([sin(x), cos(x)], x)", "[cos(x), -sin(x)]"),
        // the gradient of a list is its Jacobian
        ("d([x^2, x*y], [x, y])", "[[2*x, 0], [y, x]]"),
        ("d([[x], [x*y]], [x, y])", "[[[1, 0]], [[y, x]]]"),
        // a gradient followed by a variable
        ("d(x^2*y, [x, y], x)", "[2*y, 2*x]"),
        ("d(x, [])", "[]"),
        ("d(x^2 + y^2 == 1, x)", "2*x == 0"),
    ]);
}

#[test]
fn failures_and_unsupported() {
    let arg = |m: &str| format!("failure(argument, d, \"{m}\")");
    let order = arg("the order of a derivative must be a non-negative machine integer");
    check(&[
        // without a variable the call stays (d is also an ordinary name)
        ("d()", "d()"),
        ("d(x)", "d(x)"),
        ("d(d(d(1)))", "d(d(d(1)))"),
        ("d(x, 2)", arg("a number is not a variable").as_str()),
        ("d(x, 1.5)", arg("a number is not a variable").as_str()),
        ("d(x, \"a\")", arg("a string is not a variable").as_str()),
        ("d(x^2, x^2)", arg("a sum, product or power is not a variable").as_str()),
        ("d(x, x + y)", arg("a sum, product or power is not a variable").as_str()),
        ("d(x^3, [x, -1])", order.as_str()),
        ("d(x^3, [x, 1/2])", order.as_str()),
        ("d(x^3, [x, 10^30])", order.as_str()),
        ("d(x, [x, y, 2])", arg("a number is not a variable").as_str()),
        ("d(x^2, [x, [y]])", arg("a gradient takes a list of variables").as_str()),
        // failures in the arguments propagate
        ("d(1/0, x)", "failure(division_by_zero)"),
        // a head depending on the variable, and a series, are not differentiated: d stays unevaluated
        ("d(f(x)(y), x)", "d(f(x)(y), x)"),
        ("d(series(exp(x), x, 0, 3), x)", "d(1 + x + x^2/2 + x^3/6 + O(x^4), x)"),
    ]);
}

/// A derivative whose size explodes stops promptly when interrupted, and the session goes on.
#[test]
fn interrupt() {
    use std::time::Duration;
    let mut s = common::Sys::new();
    let (r, took) = s.interrupted("d(exp(sin(x)*cos(x)*x), [x, 400])", Duration::from_millis(300));
    assert!(r.is_err(), "not stopped: {}", r.map(|t| t.len()).unwrap_or(0));
    assert!(took < Duration::from_secs(5), "stopped after {took:?}");
    assert_eq!(s.run("d(x^2, x)"), "2*x");
}
