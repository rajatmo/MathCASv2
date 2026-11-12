//! Definite integrals: the fundamental theorem with discontinuity checks, limits at infinities and at integrable
//! endpoint singularities, divergence (item 2C, part 4).
mod common;
use common::check;

#[test]
fn fundamental_theorem() {
    check(&[
        ("integrate(x^2, [x, 0, 1])", "1/3"),
        ("integrate(sin(x), [x, 0, pi])", "2"),
        ("integrate(cos(x), [x, 0, pi/2])", "1"),
        ("integrate(1/x, [x, 1, e])", "1"),
        ("integrate(x^-1, [x, 1, 3])", "log(3)"),
        ("integrate(x^2 + 3*exp(x), [x, 0, 1])", "-8/3 + 3*e"),
        ("integrate(x^100, [x, 0, 1])", "1/101"),
        ("integrate(x^2, [x, 0, 2^(1/2)])", "(2/3)*2^(1/2)"),
        ("integrate(x^(-1/2), [x, 1, 4])", "2"),
        ("integrate([x, x^2], [x, 0, 1])", "[1/2, 1/3]"),
    ]);
}

#[test]
fn bounds_reversed_equal_numeric() {
    check(&[
        ("integrate(x^2, [x, 1, 0])", "-1/3"),
        ("integrate(x^-2, [x, 2, 1])", "-1/2"),
        ("integrate(x^-2, [x, inf, 1])", "-1"),
        ("integrate(x, [x, 2, 2])", "0"),
        ("integrate(x^2, [x, 0, 0.5])", "0.041666666666666664"),
    ]);
}

#[test]
fn symbolic_bounds() {
    check(&[
        ("integrate(x, [x, a, b])", "(-1/2)*a^2 + (1/2)*b^2"),
        ("integrate(cos(x), [x, a, b])", "-sin(a) + sin(b)"),
        ("integrate(x^2, [x, 0, c])", "(1/3)*c^3"),
        // F = log(x) is not continuous on the whole line: the call stays
        ("integrate(1/x, [x, a, b])", "integrate(x^-1, [x, a, b])"),
        ("integrate(x^-2, [x, 1, a])", "integrate(x^-2, [x, 1, a])"),
        ("integrate(x, [x, a, inf])", "integrate(x, [x, a, inf])"),
    ]);
}

#[test]
fn infinite_bounds() {
    check(&[
        ("integrate(x^-2, [x, 1, inf])", "1"),
        ("integrate(x^-2, [x, -inf, -1])", "1"),
        ("integrate(exp(x), [x, -inf, 0])", "1"),
        ("integrate(x^-3, [x, -inf, -1])", "-1/2"),
        ("integrate(x^(-3/2), [x, 1, inf])", "2"),
    ]);
}

#[test]
fn endpoint_singularities() {
    check(&[
        ("integrate(x^(-1/2), [x, 0, 1])", "2"),
        ("integrate(x^(-2/3), [x, 0, 8])", "6"),
        // principal powers on the negative axis
        ("integrate(x^(1/2), [x, -1, 0])", "complex(0, 2/3)"),
    ]);
}

#[test]
fn logarithms_of_negative_arguments_stay_real() {
    check(&[("integrate(1/x, [x, -2, -1])", "-log(2)"), ("integrate(1/x, [x, -e, -1])", "-1")]);
}

#[test]
fn divergent() {
    check(&[
        ("integrate(1/x, [x, -1, 1])", "failure(divergent, integrate, x^-1)"),
        ("integrate(x^-2, [x, -1, 1])", "failure(divergent, integrate, x^-2)"),
        ("integrate(1/x, [x, 0, 1])", "failure(divergent, integrate, x^-1)"),
        ("integrate(1/x, [x, 1, inf])", "failure(divergent, integrate, x^-1)"),
        ("integrate(x, [x, 0, inf])", "failure(divergent, integrate, x)"),
        ("integrate(x^2 - x^3, [x, 0, inf])", "failure(divergent, integrate, -x^3 + x^2)"),
        ("integrate(x^(-1/2), [x, 0, inf])", "failure(divergent, integrate, x^(-1/2))"),
        ("integrate(x^-2, [x, -inf, inf])", "failure(divergent, integrate, x^-2)"),
        ("integrate(exp(x), [x, 0, inf])", "failure(divergent, integrate, exp(x))"),
        ("integrate(x^-2, [x, 0, pi])", "failure(divergent, integrate, x^-2)"),
        ("integrate(x^-2, [x, 1, 0])", "failure(divergent, integrate, x^-2)"),
    ]);
}

#[test]
fn rational_poles_diverge_without_an_antiderivative() {
    // the poles are located (exactly, or by a Sturm sequence) whether or not an antiderivative is known
    check(&[
        ("integrate(1/(x^2 - 1), [x, 0, 2])", "failure(divergent, integrate, (-1 + x^2)^-1)"),
        ("integrate(1/(x^3 - 2), [x, 0, 2])", "failure(divergent, integrate, (-2 + x^3)^-1)"),
        ("integrate(1/(x^5 - x - 1), [x, 0, 2])", "failure(divergent, integrate, (-1 - x + x^5)^-1)"),
        ("integrate(1/(x - pi), [x, 3, 4])", "failure(divergent, integrate, (x - pi)^-1)"),
    ]);
}

#[test]
fn stays_unevaluated() {
    check(&[
        ("integrate(f(x), [x, 0, 1])", "integrate(f(x), [x, 0, 1])"),
        // no pole in [0, 1], but no antiderivative in this crate yet
        ("integrate(1/(x^5 - x - 1), [x, 0, 1])", "-root_sum(t => -1 - t + t^5, t => log(0 - t)/(-1 + 5*t^4)) + root_sum(t => -1 - t + t^5, t => log(1 - t)/(-1 + 5*t^4))"),
        // oscillating at infinity: no limit (Dirichlet: the amplitude does not decay), item 14D
        ("integrate(sin(x), [x, 0, inf])", "failure(divergent, integrate, sin(x))"),
        ("integrate(x^2, [x, 0, 1, 2])", "failure(argument, integrate, \"the range is [x, a, b]\")"),
    ]);
}

#[test]
fn under_assumptions() {
    check(&[
        ("assuming(a > 0 and b > 0, integrate(f^(-3/2), [f, a, b]))", "-2*b^(-1/2) + 2*a^(-1/2)"),
        ("integrate(f^(-3/2), [f, a, b])", "integrate(f^(-3/2), [f, a, b])"),
        ("assuming(a > 0, integrate(exp(-a*x), [x, 0, inf]))", "a^-1"),
        // the condition of convergence attached (item 14D)
        ("integrate(exp(-a*x), [x, 0, inf])", "conditional_expression(a^-1, re(a) > 0)"),
        // a singular point between the bounds: divergent
        ("assuming(a < 0 and b > 0, integrate(f^(-3/2), [f, a, b]))", "failure(divergent, integrate, f^(-3/2))"),
    ]);
}

#[test]
fn integrands_refined_under_assumptions() {
    check(&[
        ("assuming(a > 0 and b > 0, integrate(c*f^(-3/2), [f, a, b]))", "2*c*(-b^(-1/2) + a^(-1/2))"),
        (
            "assuming(a > 0 and b > 0 and m > 0, integrate((m/(2*pi*f))^(3/2), [f, a, b]))",
            // the engine answers without the assumptions (item 14A): a power of the quotient stays one kernel
            // the value F(b) - F(a) simplified under the facts (decision n14e.I2)
            "(1/2)*(-b^(-1/2) + a^(-1/2))*2^(1/2)*m^(3/2)*pi^(-3/2)",
        ),
        ("assuming(m > 0 and f > 0, integrate((m/(2*pi*f))^(3/2), f))", "(-1/2)*m*2^(1/2)/pi*(m/f/pi)^(1/2)"),
        ("integrate((m/(2*pi*f))^(3/2), f)", "(-1/2)*m*2^(1/2)/pi*(m/f/pi)^(1/2)"),
    ]);
}

/// sinc(u) is integrated as sin(u)/u (the value at 0 changes no integral).
#[test]
fn sinc_integrands() {
    check(&[
        ("integrate(sinc(x), [x, 0, inf])", "(1/2)*pi"),
        ("integrate(sinc(x), x)", "sin_integral(x)"),
        ("integrate(sinc(x)^2, [x, -inf, inf])", "pi"),
        ("integrate(sinc(2*x), [x, 0, inf])", "(1/4)*pi"),
    ]);
}
