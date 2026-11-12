//! Definite integration (item 14D): conditions, closed forms, residues, convergence first, the numeric check.
//! Every value here also passed the driver's check against `n_integrate` (or at sample parameter values).
mod common;
use common::{check, Sys};
use std::time::{Duration, Instant};

/// Conditions of convergence on parameters, recorded and attached as one conditional result; assumptions and `generate_conditions: false`.
#[test]
fn conditions() {
    check(&[
        ("integrate(x^n, [x, 0, 1])", "conditional_expression((1 + n)^-1, re(n) > -1)"),
        ("integrate(exp(-c*x^2), [x, -inf, inf])", "conditional_expression(c^(-1/2)*pi^(1/2), re(c) > 0)"),
        ("integrate(exp(-a*x), [x, 0, inf])", "conditional_expression(a^-1, re(a) > 0)"),
        ("integrate(x^(a - 1)*exp(-x), [x, 0, inf])", "conditional_expression(gamma(a), re(a) > 0)"),
        ("integrate(x^a, [x, 1, inf])", "conditional_expression(-(1 + a)^-1, re(a) < -1)"),
        ("integrate(x^(s - 1)/(1 + x), [x, 0, inf])", "conditional_expression(pi/sin(pi*s), re(s) > 0 and re(s) < 1)"),
        ("integrate(exp(x^a), [x, 0, 1])", "conditional_expression((gamma(a^-1) - gamma(a^-1, -1))*(-1)^-a^-1/a, re(a) > 0)"),
        ("integrate(exp(-abs(t))*exp(im*w*t), [t, -inf, inf])", "conditional_expression(2/(1 + w^2), im_part(w) < 1 and im_part(w) > -1)"),
        ("assuming(re(n) > -1, integrate(x^n, [x, 0, 1]))", "(1 + n)^-1"),
        ("integrate(x^n, [x, 0, 1], assumptions: n > 0)", "(1 + n)^-1"),
        ("integrate(exp(-c*x^2), [x, -inf, inf], generate_conditions: false)", "c^(-1/2)*pi^(1/2)"),
    ]);
}

/// Gaussian, gamma (with logarithms: derivatives in the exponent), incomplete gamma, Bose-Einstein and Fermi-Dirac, Beta integrals.
#[test]
fn closed_forms() {
    check(&[
        ("integrate(exp(-x^2), [x, -inf, inf])", "pi^(1/2)"),
        ("integrate(x^2*exp(-x^2), [x, -inf, inf])", "(1/2)*pi^(1/2)"),
        ("integrate(exp(-x^2 - 2*x), [x, -inf, inf])", "e*pi^(1/2)"),
        ("integrate(exp(-x^2)*cos(x), [x, -inf, inf])", "exp(-1/4)*pi^(1/2)"),
        ("integrate(log(x)*exp(-x^2), [x, 0, inf])", "(1/4)*(-euler_gamma - log(4))*pi^(1/2)"),
        ("integrate(exp(-x)*log(x), [x, 0, inf])", "-euler_gamma"),
        ("integrate(sqrt(x)*exp(-x), [x, 0, inf])", "(1/2)*pi^(1/2)"),
        ("integrate(exp(-x^2), [x, 0, 1])", "(1/2)*erf(1)*pi^(1/2)"),
        ("integrate(exp(-x)*x^(1/2), [x, 0, 2])", "-gamma(3/2, 2) + (1/2)*pi^(1/2)"),
        ("integrate(x^3/(exp(x) - 1), [x, 0, inf])", "(1/15)*pi^4"),
        ("integrate(1/(exp(x) + 1), [x, 0, inf])", "log(2)"),
        ("integrate(sqrt(x)*(1 - x)^(1/3), [x, 0, 1])", "(1/2)*gamma(4/3)*pi^(1/2)/gamma(17/6)"),
        ("integrate(1/sqrt(x*(1 - x)), [x, 0, 1])", "pi"),
    ]);
}

/// Residues: rational functions over the real line, Fourier-type integrands with indented contours, trigonometric integrands over a period, x = log(u).
#[test]
fn residues() {
    check(&[
        ("integrate(x^2/(x^4 + 1), [x, -inf, inf])", "(1/2)*pi*2^(1/2)"),
        ("integrate(1/(x^6 + 1), [x, -inf, inf])", "(2/3)*pi"),
        ("integrate(1/(1 + x^2)^2, [x, -inf, inf])", "(1/2)*pi"),
        ("integrate(1/(x^4 + 1), [x, 0, inf])", "(1/4)*pi*2^(1/2)"),
        ("integrate(cos(x)/(x^2 + 1), [x, -inf, inf])", "pi/e"),
        ("integrate(cos(2*x)/(1 + x^2)^2, [x, -inf, inf])", "(3/2)*pi*exp(-2)"),
        ("integrate(x*sin(x)/(x^2 + 1), [x, -inf, inf])", "pi/e"),
        ("integrate(sin(x)/x, [x, -inf, inf])", "pi"),
        ("integrate(sin(x)/x, [x, 0, inf])", "(1/2)*pi"),
        ("integrate((1 - cos(x))/x^2, [x, -inf, inf])", "pi"),
        ("integrate(sin(x)^3/x^3, [x, -inf, inf])", "(3/4)*pi"),
        ("integrate(sin(x)/(x*(x^2 + 1)), [x, -inf, inf])", "pi + -1*pi/e"),
        ("integrate(1/(2 + cos(x)), [x, 0, 2*pi])", "(2/3)*pi*3^(1/2)"),
        ("integrate(1/(5 - 4*sin(x)), [x, 0, 2*pi])", "(2/3)*pi"),
        ("integrate(cos(x)^4, [x, 0, 2*pi])", "(3/4)*pi"),
        ("integrate(1/(5 + 3*cos(x)), [x, 0, pi])", "(1/4)*pi"),
        ("integrate(1/cosh(x), [x, -inf, inf])", "pi"),
        ("integrate(exp(-x)*sin(x), [x, 0, inf])", "1/2"),
        ("integrate(exp(-2*x)*cos(3*x), [x, 0, inf])", "2/13"),
        ("integrate(sin(x^2), [x, 0, inf])", "(1/4)*2^(1/2)*pi^(1/2)"),
        ("integrate(cos(x)/sqrt(x), [x, 0, inf])", "(1/2)*2^(1/2)*pi^(1/2)"),
    ]);
}

/// Newton-Leibniz with trigonometric poles, one infinite bound with a symbolic one, tidied values, principal values.
#[test]
fn newton_leibniz_and_tidy() {
    check(&[
        ("integrate((2*x + 3)/(x^2 + 5*x + 6), [x, 0, 2])", "log(125/54)"),
        ("integrate(x^2 + 3*exp(x), [x, 0, 1])", "-8/3 + 3*e"),
        ("integrate(sec(x)^2, [x, 0, pi/4])", "1"),
        ("integrate(tan(x), [x, 0, pi])", "failure(divergent, integrate, tan(x))"),
        ("integrate(exp(x), [x, -inf, a])", "exp(a)"),
        ("integrate(1/x, [x, -1, 1], principal_value: true)", "0"),
        ("integrate(1/(x - 2), [x, 0, 3], principal_value: true)", "-log(2)"),
    ]);
}

/// Divergence decided before any method: poles on the line, Dirichlet's test for oscillating integrands, powers at infinity.
#[test]
fn divergence_first() {
    check(&[
        ("integrate(cos(x)/x, [x, -inf, inf])", "failure(divergent, integrate, cos(x)/x)"),
        ("integrate(x*exp(im*x), [x, 0, inf])", "failure(divergent, integrate, x*exp(1im*x))"),
        ("integrate(sin(x), [x, -inf, inf])", "failure(divergent, integrate, sin(x))"),
        ("integrate(x/(x^2 + 1), [x, -inf, inf])", "failure(divergent, integrate, x/(1 + x^2))"),
    ]);
}

/// Iterated integrals, nested lists, regions.
#[test]
fn iterated_lists_regions() {
    check(&[
        ("integrate(exp(-x^2 - y^2), [x, -inf, inf], [y, -inf, inf])", "pi"),
        ("integrate([[x, sqrt(x)], [x^(1/3), x^(1/4)]], [x, 0, 5])", "[[25/2, (10/3)*5^(1/2)], [(15/4)*5^(1/3), 4*5^(1/4)]]"),
        ("integrate(x*y^2, element([x, y], rectangle()))", "1/6"),
    ]);
}

/// No value rather than a wrong one: a parameter whose sign the poles depend on, no antiderivative.
#[test]
fn honest_unevaluated() {
    check(&[
        ("integrate(1/(x^2 + a^2), [x, -inf, inf])", "integrate((a^2 + x^2)^-1, [x, -inf, inf])"),
        ("integrate(sin(x)/x, [x, 0, 1])", "sin_integral(1)"),
    ]);
}

/// Substitutions: `x -> 1/x` on (0, inf), `x = exp(-u)` on (0, 1); the incomplete gamma function.
#[test]
fn substitutions() {
    check(&[
        ("integrate(log(x)/(1 + x^2), [x, 0, inf])", "0"),
        ("integrate(log(x)^2, [x, 0, 1])", "2"),
        ("integrate(x^a*log(x), [x, 0, 1])", "conditional_expression(-(1 + a)^-2, re(a) > -1)"),
        ("integrate((-log(x))^(s - 1), [x, 0, 1])", "conditional_expression(gamma(s), re(s) > 0)"),
        ("integrate(sqrt(-log(x)), [x, 0, 1])", "(1/2)*pi^(1/2)"),
        ("integrate(exp(-x^3), [x, 0, 1])", "(1/3)*(gamma(1/3) - gamma(1/3, 1))"),
        ("integrate(x^2*exp(-x^3), [x, 0, 1])", "1/3 + (-1/3)/e"),
    ]);
}

/// Step-like functions rewritten for the splitting by cases, the integrand refined on the range, and a switching
/// expression with jumps refused (the old splitting gave 5 for the last one; the value is 5/2).
#[test]
fn steps_and_ranges() {
    check(&[
        ("integrate(unit_box(x), [x, -inf, inf])", "1"),
        ("integrate(unit_triangle(x), [x, -inf, inf])", "1"),
        ("integrate(heaviside_theta(x)*exp(-x), [x, -inf, inf])", "1"),
        ("integrate(boole(x > 1/2), [x, 0, 1])", "1/2"),
        ("integrate(sqrt(x^2), [x, -1, 2])", "5/2"),
        ("integrate(piecewise([[x, round(x) > x]], -x), [x, 0, 10])", "integrate(piecewise([[x, round(x) > x]], -x), [x, 0, 10])"),
    ]);
}

/// An integral no method decides returns unevaluated within the driver's budget, and the stop flag interrupts it.
#[test]
fn bounded_and_interruptible() {
    let mut s = Sys::new();
    let start = Instant::now();
    let r = s.run("integrate(exp(-x^x)*sin(x)^3/(1 + x^7), [x, 0, inf])");
    assert!(r.starts_with("integrate("), "{r}");
    assert!(start.elapsed() < Duration::from_secs(60), "took {:?}", start.elapsed());
    // the numeric check of an oscillating integrand over an infinite range is quick (truncated ranges at once)
    let start = Instant::now();
    let r = s.run("fourier_transform(1/sqrt(abs(t)), t, w)");
    assert!(!r.starts_with("fourier_transform("), "{r}");
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
    let (r, took) = s.interrupted("integrate(exp(-x^x)*cos(x)^5/(2 + x^9), [x, 0, inf])", Duration::from_millis(50));
    assert!(took < Duration::from_secs(20), "{r:?} after {took:?}");
}

/// Bounds whose order depends on the sign of a parameter: both cases joined, for real parameters (decision n14e.I1);
/// values with parameters simplified under the facts (decision n14e.I2).
#[test]
fn sign_of_bounds_and_facts() {
    check(&[
        ("integrate(sqrt(x*(x - a)), [x, 0, a])", "conditional_expression(complex(0, 1/8)*a*pi*abs(a), element(a, reals))"),
        ("integrate(sqrt(x*(x - a)), [x, 0, a], generate_conditions: false)", "complex(0, 1/8)*a*pi*abs(a)"),
        ("integrate(sqrt(x*(a - x)), [x, 0, a])", "conditional_expression((1/8)*a*pi*abs(a), element(a, reals))"),
        ("integrate(abs(x), [x, 0, a])", "conditional_expression((1/2)*a*abs(a), element(a, reals))"),
        ("integrate(sqrt(x*(x - 1)), [x, 0, 1])", "complex(0, 1/8)*pi"),
        ("assuming(a > 0, integrate(sqrt(x*(x - a)), [x, 0, a]))", "complex(0, 1/8)*pi*a^2"),
        ("assuming(a < 0, integrate(sqrt(x*(x - a)), [x, 0, a]))", "complex(0, -1/8)*pi*a^2"),
        ("assuming(a > 0, integrate(sqrt(x*(a - x)), [x, 0, a]))", "(1/8)*pi*a^2"),
        ("assuming(a > 0, integrate(1/sqrt(a^2 - x^2), [x, 0, a]))", "(1/2)*pi"),
    ]);
}
