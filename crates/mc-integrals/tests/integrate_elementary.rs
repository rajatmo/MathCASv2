//! Integration of elementary functions (item 2C, part 3): the table with linear arguments, derivative-divides
//! substitution, integration by parts, powers and products of trigonometric functions. Every antiderivative is
//! checked by differentiation; a few printed forms are pinned.
mod common;
use common::Sys;
use std::time::{Duration, Instant};

/// Points inside (0, 1), where every integrand of the lists below is defined.
const UNIT: &[&str] = &["(3/7)", "(5/11)", "(2/9)"];

fn all_found(s: &mut Sys, fs: &[&str], points: &[&str]) {
    let mut failures = Vec::new();
    for f in fs {
        if let Err(e) = s.check_antiderivative(f, points) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn table_linear_arguments() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "exp(2*x + 1)",
            "e^(3*x)",
            "sin(3*x - 1)",
            "cos(x/2)",
            "tan(2*x)",
            "cot(x)",
            "sec(x)",
            "1/sin(3*x)",
            "sinh(2*x)",
            "cosh(1 - x)",
            "tanh(x)",
            "2^x",
            "3^(2*x + 1)",
            "sec(x)^2",
            "csc(2*x)^2",
            "sec(x)*tan(x)",
            "log(x)",
            "log(2*x + 1)",
            "atan(x)",
            "asin(x)",
            "acos(x/2)",
            "(2*x + 1)^(1/2)",
            "(3*x + 2)^-1",
            "(1 - x)^7",
            "exp(a*x)",
            "sin(a*x + b)",
        ],
        UNIT,
    );
}

#[test]
fn quadratic_powers() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "1/sqrt(1 - x^2)",
            "1/sqrt(4 - 9*x^2)",
            "1/sqrt(1 + x^2)",
            "1/sqrt(x^2 + 2*x + 5)",
            "1/(1 + x^2)",
            "1/(x^2 + 2*x + 5)",
            "1/(4 - x^2)",
            "sqrt(1 - x^2)",
            "sqrt(x^2 + 1)",
            "(1 - x^2)^(-3/2)",
        ],
        UNIT,
    );
    assert_eq!(s.run("integrate(1/sqrt(1 - x^2), x)"), "asin(x)");
    assert_eq!(s.run("integrate(1/(1 + x^2), x)"), "atan(x)");
    assert_eq!(s.run("integrate(1/sqrt(1 + x^2), x)"), "log(x + (1 + x^2)^(1/2))");
}

#[test]
fn substitution() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "2*x*cos(x^2)",
            "exp(sin(x))*cos(x)",
            "log(x)/x",
            "sin(x)^3*cos(x)",
            "x*exp(x^2)",
            "x^3*exp(x^2)",
            "(2*x + 1)*exp(x^2 + x)",
            "x/sqrt(1 - x^2)",
            "x*(x^2 + 1)^5",
            "exp(x)/(1 + exp(x))",
            "cos(x)/(1 + sin(x)^2)",
            "exp(x)*sin(exp(x))",
            "cos(sqrt(x))/sqrt(x)",
            "log(x)^2/x",
            "sec(x^2)^2*x",
            "sin(log(x))",
        ],
        UNIT,
    );
    s.check_antiderivative("1/(x*log(x))", &["(13/7)", "(5/2)"]).unwrap();
    assert_eq!(s.run("integrate(2*x*cos(x^2), x)"), "sin(x^2)");
    assert_eq!(s.run("integrate(exp(sin(x))*cos(x), x)"), "exp(sin(x))");
    assert_eq!(s.run("integrate(log(x)/x, x)"), "(1/2)*log(x)^2");
    assert_eq!(s.run("integrate(sin(x)^3*cos(x), x)"), "(1/4)*sin(x)^4");
    assert_eq!(s.run("integrate(x*exp(x^2), x)"), "(1/2)*exp(x^2)");
}

#[test]
fn by_parts() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "x*exp(x)",
            "x^2*exp(3*x)",
            "x^3*exp(-x)",
            "(x^2 + 1)*exp(2*x)",
            "x*sin(2*x)",
            "x^2*cos(x)",
            "x*cosh(2*x)",
            "x*2^x",
            "x^3*log(x)",
            "x^(1/2)*log(x)",
            "log(x)/x^2",
            "(x + 1)*log(x)",
            "log(x)^3",
            "x*log(x)^2",
            "x^2*atan(x)",
            "acos(x)",
            "exp(2*x)*sin(3*x)",
            "exp(x)*cos(x)",
            "exp(-x)*sin(x + 1)",
            "x^2*sin(x)*exp(x)",
            "x*sec(x)^2",
            "x*cos(x)^2",
            "x*exp(a*x)",
        ],
        UNIT,
    );
    assert_eq!(s.run("integrate(x*exp(x), x)"), "-exp(x) + x*exp(x)");
    assert_eq!(s.run("integrate(log(x), x)"), "-x + x*log(x)");
    assert_eq!(s.run("integrate(atan(x), x)"), "(-1/2)*log(1 + x^2) + x*atan(x)");
    assert_eq!(s.run("integrate(exp(x)*cos(x), x)"), "(1/2)*(cos(x) + sin(x))*exp(x)");
    // a high degree goes by the tabular method, not by recursion
    s.check_antiderivative("x^40*exp(x)", UNIT).unwrap();
    s.check_antiderivative("x^30*sin(x)*exp(2*x)", UNIT).unwrap();
}

#[test]
fn trigonometric_powers() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "sin(x)^2",
            "cos(3*x)^4",
            "sin(x)^5",
            "sin(x)^2*cos(x)^3",
            "sin(x)^4*cos(x)^2",
            "sin(x)*cos(x)",
            "sin(2*x)*cos(3*x)",
            "sin(x)^2*cos(2*x)",
            "tan(x)^2",
            "tan(x)^3",
            "tan(x)^4",
            "cot(x)^3",
            "sec(x)^3",
            "sec(x)^4",
            "sin(x)^-3",
            "sec(x)^2*tan(x)^2",
            "cos(x)^2/sin(x)",
            "cos(x)^3/sin(x)^2",
            "1/(sin(x)^2*cos(x)^2)",
            "sinh(x)^2",
            "x*sinh(x)*cosh(x)",
        ],
        UNIT,
    );
    assert_eq!(s.run("integrate(sin(x)^2, x)"), "(-1/4)*sin(2*x) + (1/2)*x");
    assert_eq!(s.run("integrate(tan(x), x)"), "-log(cos(x))");
    assert_eq!(s.run("integrate(sec(x), x)"), "log((1 + sin(x))/cos(x))");
}

/// Integrands without an elementary antiderivative (or out of reach) stay unevaluated. (Since item 14A the
/// tower-based engine answers exp(x^2), sin(x)/x, exp(x)/x and sin(x^2) with special functions:
/// tests/integrate_engine.rs.)
#[test]
fn unsolved_stay() {
    let mut s = Sys::new();
    // log(sin(x)) is answered by the rule table since round 18 (dilogarithm; crates/mc-integrate-rules/tests/rules.rs)
    for f in ["x^x", "exp(exp(x)) + 1/log(x)", "f(x)*x", "sin(sin(x))", "exp(x^3)"] {
        let out = s.run(&format!("integrate({f}, x)"));
        assert!(out.starts_with("integrate("), "{f}: {out}");
    }
    assert_eq!(s.run("integrate(x^x, x)"), "integrate(x^x, x)");
}

/// Adversarial inputs (deep nesting, many factors) fail or succeed in bounded time.
#[test]
fn bounded_time() {
    let mut s = Sys::new();
    let mut nested = String::from("x");
    for i in 0..40 {
        nested = format!("{}({nested})", ["sin", "exp", "cos", "log"][i % 4]);
    }
    let many = "exp(x^2)*sin(x)/x*log(x)*atan(x)*cosh(x)*tan(x^3)*asin(x)*(1 + x^5)^(1/3)";
    let deep_power = "sin(x^2 + 1)^7*cos(x^3)^5*exp(sin(x))^3";
    for f in [nested.as_str(), many, deep_power, "(sin(x) + cos(x) + exp(x) + log(x))^6*atan(x)"] {
        let start = Instant::now();
        let out = s.run(&format!("integrate({f}, x)"));
        let took = start.elapsed();
        assert!(took < Duration::from_secs(10), "{f}: {took:?} ({})", out.len());
    }
    // a found chain of nested functions
    s.check_antiderivative("cos(x)*cos(sin(x))*exp(sin(sin(x)))", UNIT).unwrap();
}

/// An interrupt stops a long integration promptly, and the session goes on.
#[test]
fn interrupt() {
    let mut s = Sys::new();
    // many high-degree tabular integrations (about 0.1 s each)
    let items: Vec<String> = (1..=200).map(|k| format!("(x + {k})^150*exp({k}*x)*sin(x)")).collect();
    let src = format!("integrate([{}], x)", items.join(", "));
    let (r, took) = s.interrupted(&src, Duration::from_millis(300));
    assert!(r.is_err(), "not stopped after {took:?}");
    assert!(took < Duration::from_secs(5), "stopped after {took:?}");
    assert_eq!(s.run("integrate(x*exp(x), x)"), "-exp(x) + x*exp(x)");
}
