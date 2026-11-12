//! Integration of rational functions (item 2C, part 2): polynomial part, Hermite reduction, Rothstein–Trager
//! with real log/atan output, parameters, declines and interruption.
mod common;
use common::Sys;
use std::time::Duration;

/// Points inside every domain used below (no pole of the integrands at these).
const PTS: [&str; 3] = ["(3/7)", "(13/5)", "(-29/11)"];

#[test]
fn antiderivatives_check_by_differentiation() {
    let mut s = Sys::new();
    let cases = [
        "1/(x^2 + 1)",
        "1/(x^2 + 2*x + 5)",
        "x/(x^2 + 1)",
        "1/(x^2 - 1)",
        "(x^3 + 1)/(x^2 - 4)",
        "1/(x^3 + 1)",
        "1/(x^4 + 1)",
        "x^2/(x^4 + 1)",
        "(2*x + 3)/(x^2 + 3*x + 2)^2",
        "1/(x - 1)^3",
        "x^5/(x^2 + 1)^2",
        "1/(x^2 + 1)^2",
        "1/(x^2 - 2)",
        "1/(x^2 + 2)",
        "3*x^2/(x^3 - 2)",
        "(x + 1)^3",
        "1/(x^4 + x^2 + 1)",
        "1/(x^4 - 2)",
        "1/(x^4 + 1)^2",
        "(x^4 + 3*x - 7)/((x - 2)^2*(x^2 + x + 1))",
        "(6*x^2 + 1)/(2*x^3 + x + 7)^2",
        "(x^2 + 1)/(x^3 + 3*x - 1)^4",
        "1/(x^6 - 1)",
        "(x^7 - 2*x^3 + 1)/(x^4 + 5*x^2 + 6)",
        "1/(4*x^2 + 4*x + 5)^3",
        "(x + 2)/(x^2 + x + 1/4)",
    ];
    for f in cases {
        if let Err(e) = s.check_antiderivative(f, &PTS) {
            panic!("{e}");
        }
    }
}

#[test]
fn exact_results() {
    let mut s = Sys::new();
    let cases = [
        ("integrate(1/(x^2 + 1), x)", "atan(x)"),
        ("integrate(1/(x^2 + 2*x + 5), x)", "(1/2)*atan((1/2)*(1 + x))"),
        ("integrate(x/(x^2 + 1), x)", "(1/2)*log(1 + x^2)"),
        ("integrate(1/(x^2 - 1), x)", "(-1/2)*log(1 + x) + (1/2)*log(-1 + x)"),
        ("integrate((x^3 + 1)/(x^2 - 4), x)", "(1/2)*x^2 + (7/4)*log(2 + x) + (9/4)*log(-2 + x)"),
        ("integrate((2*x + 3)/(x^2 + 3*x + 2)^2, x)", "-(2 + 3*x + x^2)^-1"),
        ("integrate(1/(x - 1)^3, x)", "(-1/2)*(-1 + x)^-2"),
        ("integrate(x^5/(x^2 + 1)^2, x)", "-log(1 + x^2) + (-1/2)/(1 + x^2) + (1/2)*x^2"),
        ("integrate(3*x^2/(x^3 - 2), x)", "log(-2 + x^3)"),
        ("integrate(1/(x^2 + a^2), x)", "atan(x/a)/a"),
        ("integrate(5/(x^2 + 1), x)", "5*atan(x)"),
    ];
    for (src, want) in cases {
        assert_eq!(s.run(src), want, "{src}");
    }
    let r = s.run("integrate(1/(x^3 + 1), x)");
    assert!(r.contains("(1/3)*log(1 + x)") && r.contains("(-1/6)*log(1 - x + x^2)") && r.contains("atan("), "{r}");
}

/// `d(F, x) - f` with the parameters given values, at the points.
fn check_with_params(s: &mut Sys, f: &str, values: &str) {
    let shown = s.run(&format!("ff = {f}; FF = integrate(ff, x); FF"));
    assert!(!shown.contains("integrate("), "integrate({f}, x) not found: {shown}");
    for p in PTS {
        let out = s.run(&format!("n(1 + rewrite(rewrite((d(FF, x) - ff)*10^20, {values}), [x -> {p}]), 30)"));
        let ok = out.trim_end_matches('.').parse::<f64>().is_ok_and(|v| (v - 1.0).abs() < 1.0);
        assert!(ok, "integrate({f}, x) = {shown}: d(F, x) - f at x = {p}, {values} is {out}");
    }
}

#[test]
fn parameters() {
    let mut s = Sys::new();
    for (f, vals) in [
        ("1/(x^2 + a^2)", "[a -> 3/2]"),
        ("1/(x^2 - a^2)", "[a -> 5]"),
        ("1/((x - a)*(x - b))", "[a -> 5, b -> -7]"),
        ("1/(x^2 + a)", "[a -> 3]"),
        ("x/(x^2 + a*x + b)", "[a -> 1, b -> -6]"),
        ("1/(x + a)^2", "[a -> 9]"),
        ("1/(x^2 + sqrt(2))", "[]"),
        ("1/(x^4 + a^4)", "[a -> 7/3]"),
        ("(x^2 + 1)/(a*x^2 + b)", "[a -> 2, b -> 3]"),
        ("1/(x^2 + y^2 + 1)", "[y -> 1/2]"),
        ("x^3/(x^2 + a^2)^2", "[a -> 3]"),
    ] {
        check_with_params(&mut s, f, vals);
    }
}

#[test]
fn declines() {
    let mut s = Sys::new();
    // not rational in x
    assert_eq!(s.run("integrate(sin(x)/(x^2 + 1), x)"), "integrate((1 + x^2)^-1*sin(x), x)");
    // not rational in x: answered since round 18 (the algebraic stage, or the rule table's substitution x = t^2)
    assert!(!s.run("integrate(1/(x^2 + sqrt(x)), x)").contains("integrate("));
    // residues of degree 3 (item F1): a sum over the roots of the cubic
    assert_eq!(s.run("integrate(1/(x^3 + x + 1), x)"), "root_sum(t => 1 + t + t^3, t => log(x - t)/(1 + 3*t^2))");
    // a binomial denominator: real log and atan terms over Q(2^(1/3)) (item F1)
    let r = s.run("integrate(1/(x^3 - 2), x)");
    assert!(r.contains("2^(1/3)") && r.contains("atan(") && !r.contains("root_sum"), "{r}");
    // inexact coefficients: the rational method declines them; since the parts were combined the elementary method's quadratic
    // table integrates them (sqrt(2)*atan(sqrt(2)*x) in machine numbers), so check that result by differentiation
    let out = s.run("FF = integrate(1/(x^2 + 0.5), x); n(rewrite(d(FF, x) - 1/(x^2 + 0.5), [x -> 0.7]))");
    assert!(out.parse::<f64>().is_ok_and(|v| v.abs() < 1e-12), "{out}");
    // the session goes on
    assert_eq!(s.run("integrate(1/(x^2 + 1), x)"), "atan(x)");
}

#[test]
fn interrupt() {
    let mut s = Sys::new();
    let (r, took) = s.interrupted("integrate(1/(x^200 + 7*x + 3), x)", Duration::from_millis(300));
    assert!(took < Duration::from_secs(2), "took {took:?}");
    if let Ok(v) = &r {
        assert!(v.contains("interrupted") || v.contains("integrate("), "{v}");
    }
    assert_eq!(s.run("integrate(1/(x^2 + 1), x)"), "atan(x)");
}

/// Residues of degree 3 or more (item F1): `root_sum` results, checked by differentiation numerically and exactly
/// (`d` of a sum over roots sums the derivative, which `root_sum` adds up to a rational function).
#[test]
fn root_sums_check_by_differentiation() {
    let mut s = Sys::new();
    let cases = [
        "1/(x^3 + x + 1)",
        "(x^2 + 1)/(x^5 - x - 1)",
        "1/((x^3 + x + 1)*(x^3 - x + 1))",
        "1/(x^3 + 2*x + 5)^2",
        "x/(x^4 + x + 1)",
        "1/(x^8 + x + 1)",
        "(x^4 + 1)/(x^5 + 3*x - 1)",
    ];
    for f in cases {
        let r = s.run(&format!("integrate({f}, x)"));
        assert!(r.contains("root_sum") && !r.contains("integrate("), "integrate({f}, x) = {r}");
        if let Err(e) = s.check_antiderivative(f, &PTS) {
            panic!("{e}");
        }
        let back = s.run(&format!("together(d(integrate({f}, x), x) - {f})"));
        assert_eq!(back, "0", "d(integrate({f}, x), x) - {f}");
    }
    // the residue formula: a quotient over the roots of the denominator
    assert_eq!(s.run("integrate(1/(x^3 + x + 1), x)"), "root_sum(t => 1 + t + t^3, t => log(x - t)/(1 + 3*t^2))");
    // the integration variable t: the bound variable is u
    assert_eq!(s.run("integrate(1/(t^3 + t + 1), t)"), "root_sum(u => 1 + u + u^3, u => log(t - u)/(1 + 3*u^2))");
    // a quadratic split stays elementary, as before
    let r = s.run("integrate(1/(x^4 + 1), x)");
    assert!(!r.contains("root_sum"), "{r}");
}

/// Binomial denominators `x^n - c`, `n` in 3, 4, 6, with a real root (item F1): real `log`/`atan` terms over
/// the real radical field, checked by differentiation.
#[test]
fn real_radical_fields_check_by_differentiation() {
    let mut s = Sys::new();
    for f in ["1/(x^3 - 2)", "1/(x^3 + 2)", "x/(x^4 - 3)", "1/(x^6 - 5)", "(x + 1)/(x^3 - 7)", "x^2/(x^4 - 2) + 1/(x^3 + x + 1)"] {
        let r = s.check_antiderivative(f, &PTS).unwrap_or_else(|e| panic!("{e}"));
        assert!(!r.contains("integrate("), "integrate({f}, x) = {r}");
    }
    let r = s.run("integrate(1/(x^6 - 5), x)");
    assert!(!r.contains("root_sum") && r.contains("5^(1/6)"), "{r}");
}
