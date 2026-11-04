//! Package 2C across its parts: definite integrals of antiderivatives found by the rational and elementary methods,
//! elementary integrals whose sub-integrals are rational, `apart` then `integrate`, and the exit test of the roadmap
//! (antiderivatives checked by differentiation) on a mixed corpus.
mod common;
use common::{check, Sys};

#[test]
fn definite_over_rational_and_elementary() {
    check(&[
        ("integrate(1/(1 + x^2), [x, 0, inf])", "(1/2)*pi"),
        ("integrate(1/(1 + x^2), [x, -inf, inf])", "pi"),
        ("integrate(1/(x^2 + 4*x + 5), [x, -inf, inf])", "pi"),
        ("integrate(x/(x^4 + 1), [x, 0, inf])", "(1/4)*pi"),
        ("integrate(exp(-x), [x, 0, inf])", "1"),
        ("integrate(x*exp(-x), [x, 0, inf])", "1"),
        ("integrate(log(x), [x, 0, 1])", "-1"),
        ("integrate((1 - x^2)^(-1/2), [x, -1, 1])", "pi"),
        ("integrate(sin(x)^2, [x, 0, pi])", "(1/2)*pi"),
        ("integrate(1/x, [x, -1, 1])", "failure(divergent, integrate, x^-1)"),
    ]);
}

#[test]
fn values_by_number() {
    // log(3/2)/2 in whatever form the logs come out
    let mut s = Sys::new();
    let v = s.run("n(1 + (integrate(1/(x^2 - 1), [x, 2, 3]) - log(3/2)/2)*10^20, 30)");
    assert!(v.trim_end_matches('.').parse::<f64>().is_ok_and(|v| (v - 1.0).abs() < 1.0), "{v}");
}

/// Mixed integrands: each method alone, and elementary integrals with rational sub-integrals.
const CORPUS: &[&str] = &[
    "x*atan(x)",
    "exp(2*x)/(1 + exp(x))",
    "1/(sin(x)*cos(x))",
    "log(1 + x^2)",
    "x^2*atan(x)",
    "(x^3 + 1)/(x^2 - 4)",
    "1/(x^3 + 1)",
    "x*exp(x)*sin(x)",
    "sin(x)^3*cos(x)^2",
    "x^2/((x - 1)*(x^2 + 1))",
    "1/(x*log(x))",
    "exp(x)*(1 + exp(x))^-2",
    "atan(x)/(1 + x^2)",
    "x^3/(x^2 + 1)^2",
];

#[test]
fn exit_test_by_differentiation() {
    let mut s = Sys::new();
    for f in CORPUS {
        s.check_antiderivative(f, &["(7/5)", "(11/5)"]).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn apart_then_integrate() {
    let mut s = Sys::new();
    for e in ["x^2/((x - 1)*(x^2 + 1))", "1/(x^2 - 1)", "(x^3 + 1)/(x^2 + x)"] {
        // integrating the partial fractions term by term gives an antiderivative of the original
        let out = s.run(&format!(
            "ee = {e}; FF = integrate(apart(ee, x), x); n(1 + rewrite((d(FF, x) - ee)*10^20, [x -> 7/3]), 30)"
        ));
        assert!(out.trim_end_matches('.').parse::<f64>().is_ok_and(|v| (v - 1.0).abs() < 1.0), "{e}: {out}");
    }
}

#[test]
fn unsolved_stay() {
    check(&[
        ("integrate(exp(-x^2)*x^2, x)", "integrate(exp(-x^2)*x^2, x)"),
        ("integrate(sin(x)/x, x)", "integrate(x^-1*sin(x), x)"),
    ]);
}
