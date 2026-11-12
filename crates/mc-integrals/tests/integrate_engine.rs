//! The tower-based engine (item 14A): integrands the decision procedure, the Risch–Norman ansatz and the
//! special-function candidates answer, each checked by differentiation; non-elementary integrals proven by the
//! decision procedure (`risch_integrate`); a few printed forms pinned.
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

/// Exponential and logarithmic towers with elementary antiderivatives.
#[test]
fn elementary_towers() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "x*exp(x^2)",
            "x^3*exp(x^2)",
            "(exp(x) + 1)/(exp(x) + x)",
            "1/(x*log(x))",
            "1/(x*log(x)^2)",
            "log(x)^2",
            "log(x)^3/x",
            "exp(x)*(x - 1)/x^2",
            "exp(x)/(1 + exp(2*x))",
            "exp(-x^2)*(1 - 2*x^2)",
            "1/(1 + exp(x))",
            "(log(x) + 1)*x^x",
            "exp(x)*sin(x)",
            "exp(2*x)*cos(3*x)",
            "x*exp(x)*sin(x)",
            "tan(x)",
            "exp(x)*log(x) + exp(x)/x",
            "1/(1 + cos(x))",
            "exp(exp(x))*exp(x)",
            "exp(x)*(1 + x)/(x*exp(x) + 1)",
            "x*asin(x)",
            "exp(x)*sin(x)*cos(x)",
        ],
        UNIT,
    );
}

/// Antiderivatives with special functions, from the candidates of the ansatz.
#[test]
fn special_function_answers() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "exp(x^2)",
            "exp(-x^2)",
            "x^2*exp(-x^2)",
            "exp(-x^2 + x)",
            "exp(-c*x^2)",
            "erf(x)",
            "x*erf(x)",
            "exp(x)/x",
            "exp(2*x)/x",
            "exp(x)/x^2",
            "exp(x^3)/x",
            "sin(x)/x",
            "cos(x)/x",
            "sin(2*x)/x",
            "sin(x)/x^2",
            "sinh(x)/x",
            "cosh(x)/x",
            "1/log(x)",
            "x/log(x)",
            "sin(x^2)",
            "cos(x^2)",
            "x^(1/2)*exp(-x)",
            "x^(3/2)*exp(-x)",
            "1/(x*log(x)) + 1/log(x)",
            "exp(x)/x^3",
            "1/log(x)^2",
            "x*exp(-x^4)",
            "x*sin(x^4)",
            "sin_integral(x)",
            "x*exp_integral_ei(x)",
            "exp(x)*log(x)",
            "exp(x)*erf(x)",
            "exp(2*x)*erfc(x)",
            "1/sqrt(1 - 2*sin(x)^2)",
            "sqrt(1 - sin(x)^2/2)",
            "1/((1 - sin(x)^2/2)*sqrt(1 - sin(x)^2/3))",
        ],
        UNIT,
    );
}

/// The printed forms of a few answers.
#[test]
fn printed_forms() {
    let mut s = Sys::new();
    for (f, want) in [
        ("exp(-x^2)", "(1/2)*erf(x)*pi^(1/2)"),
        ("exp(x^2)", "(1/2)*erfi(x)*pi^(1/2)"),
        ("sin(x)/x", "sin_integral(x)"),
        ("cos(x)/x", "cos_integral(x)"),
        ("exp(x)/x", "exp_integral_ei(x)"),
        ("1/log(x)", "log_integral(x)"),
        ("x/log(x)", "exp_integral_ei(2*log(x))"),
        ("(exp(x) + 1)/(exp(x) + x)", "log(x + exp(x))"),
        ("1/(x*log(x))", "log(log(x))"),
        ("exp(x)*(1 + x)/(x*exp(x) + 1)", "log(1 + x*exp(x))"),
        ("x^a*exp(-x)", "-gamma(1 + a, x)"),
        ("x*exp(-x^4)", "(1/4)*erf(x^2)*pi^(1/2)"),
        ("1/sqrt(1 - 2*sin(x)^2)", "elliptic_f(x, 2)"),
    ] {
        assert_eq!(s.run(&format!("integrate({f}, x)")), want, "{f}");
    }
}

/// The decision procedure: answers, proofs of non-elementarity, and integrands outside it.
#[test]
fn risch_decisions() {
    let mut s = Sys::new();
    for f in ["exp(x^2)", "exp(x)/x", "1/log(x)", "sin(x)/x", "log(x)/(1 + x)", "x^2*exp(-x^2)", "sin(x^2)", "exp(-x^2)/x"] {
        let out = s.run(&format!("risch_integrate({f}, x)"));
        assert!(out.starts_with("failure(nonelementary, risch_integrate,"), "{f}: {out}");
    }
    for (f, want) in [
        ("x*exp(x^2)", "(1/2)*exp(x^2)"),
        ("(exp(x) + 1)/(exp(x) + x)", "log(x + exp(x))"),
        ("1/(x*log(x)^2)", "-log(x)^-1"),
        ("exp(x)*(x - 1)/x^2", "exp(x)/x"),
        ("cos(x)^2", "(1/4)*sin(2*x) + (1/2)*x"),
    ] {
        assert_eq!(s.run(&format!("risch_integrate({f}, x)")), want, "{f}");
    }
    // two monomials (a tower of height two) or an algebraic function: outside the procedure
    for f in ["exp(x)*log(x)", "sqrt(x)*exp(x)"] {
        let out = s.run(&format!("risch_integrate({f}, x)"));
        assert!(out.starts_with("risch_integrate("), "{f}: {out}");
    }
    assert_eq!(s.run("risch_integrate(3, x)"), "3*x");
}

/// Large towers fail or succeed in bounded time.
#[test]
fn bounded_time() {
    let mut s = Sys::new();
    for f in [
        "exp(x)^12*log(x)^5/(x^7 + x + 1)",
        "exp(x^2)*sin(x)*log(x)*atan(x)/(1 + x^3)",
        "(exp(x) + log(x) + sin(x) + x)^5/(exp(x) + x)",
        // a constant factor in a denominator of the derivations (the lcm of 2 and x is 2 x)
        "sech(x/2)*exp(x*t)/2",
    ] {
        let start = Instant::now();
        let out = s.run(&format!("integrate({f}, x)"));
        assert!(start.elapsed() < Duration::from_secs(20), "{f} took {:?}: {out}", start.elapsed());
        assert!(!out.contains("failure("), "{f}: {out}");
    }
}

/// An exploration aid: `MC_RUN="src;src"` evaluates and prints (nothing without the variable).
#[test]
fn explore() {
    let mut s = Sys::new();
    if let Ok(src) = std::env::var("MC_RUN") {
        for line in src.split(';') {
            let t = Instant::now();
            println!("{line}  =>  {}   [{:?}]", s.run(line), t.elapsed());
        }
    }
}
