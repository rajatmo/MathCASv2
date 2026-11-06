//! `simplify` and the transforms: exact results, numeric equivalence at random points, idempotence, interruption.
mod common;
use common::{check, Lcg, Sys};
use std::time::Duration;

#[test]
fn rational_forms() {
    check(&[
        ("simplify((x^2 - 1)/(x - 1))", "1 + x"),
        ("simplify(d(x/(1 + x), x))", "(1 + x)^-2"),
        ("simplify(d(log(sqrt(x^2 + 1)), x))", "x/(1 + x^2)"),
        ("simplify([x/x^2, (a^2 - b^2)/(a + b)])", "[x^-1, a - b]"),
        ("simplify(expand((x + y + z)^8)/(x + y + z)^6)", "(x + y + z)^2"),
        ("simplify(expand((1 + x)^20))", "(1 + x)^20"),
        // already simplest: unchanged
        ("simplify((1 + x)^20)", "(1 + x)^20"),
        ("simplify(1 + x)", "1 + x"),
        ("simplify(f(x) + f(x))", "2*f(x)"),
        ("simplify(5)", "5"),
        ("simplify(\"text\")", "\"text\""),
        // inside other functions and equations
        ("simplify(sin((x^2 - 1)/(x - 1)))", "sin(1 + x)"),
        ("simplify(x == (x^2 - 1)/(x - 1))", "x == 1 + x"),
    ]);
}

#[test]
fn trigonometric_and_exponential_forms() {
    check(&[
        ("simplify(sin(x)^2 + cos(x)^2)", "1"),
        ("simplify(1 - sin(x)^2)", "cos(x)^2"),
        ("simplify(cosh(x)^2 - sinh(x)^2)", "1"),
        ("simplify(sec(x)^2 - tan(x)^2)", "1"),
        ("simplify(1/(1 + tan(x)^2))", "cos(x)^2"),
        ("simplify(tan(x)*cos(x))", "sin(x)"),
        ("simplify(2*sin(x)*cos(x))", "sin(2*x)"),
        ("simplify(sin(x)^4 - cos(x)^4)", "-cos(2*x)"),
        ("simplify(sin(x)*cos(y) + cos(x)*sin(y))", "sin(x + y)"),
        ("simplify(cos(a)*cos(b) - sin(a)*sin(b))", "cos(a + b)"),
        ("simplify(sin(y - x) + sin(x - y))", "0"),
        ("simplify((exp(1im*x) + exp(-1im*x))/2)", "cos(x)"),
        ("simplify(exp(x)*exp(y))", "exp(x + y)"),
        ("simplify(d(sin(x)^2 + cos(x)^2, x))", "0"),
    ]);
}

#[test]
fn transforms() {
    check(&[
        ("trig_expand(sin(x + y))", "cos(x)*sin(y) + cos(y)*sin(x)"),
        ("trig_expand(cos(3*x))", "-3*cos(x)*sin(x)^2 + cos(x)^3"),
        ("trig_expand(sinh(x + y))", "cosh(x)*sinh(y) + cosh(y)*sinh(x)"),
        ("trig_expand(tan(2*x))", "2*cos(x)/(-sin(x)^2 + cos(x)^2)*sin(x)"),
        ("trig_expand(sin(x))", "sin(x)"),
        ("trig_reduce(sin(x)^2)", "1/2 + (-1/2)*cos(2*x)"),
        ("trig_reduce(sin(x)^3)", "(-1/4)*sin(3*x) + (3/4)*sin(x)"),
        ("trig_reduce(cos(x)*cos(y))", "(1/2)*cos(x + y) + (1/2)*cos(x - y)"),
        ("trig_reduce(sin(x)*cos(y) + cos(x)*sin(y))", "sin(x + y)"),
        ("trig_to_exp(cosh(x))", "(1/2)*(exp(x) + exp(-x))"),
        ("exp_to_trig(exp(1im*x))", "cos(x) + 1im*sin(x)"),
        ("exp_to_trig(exp(2 + 3im*x))", "cos(3*x)*exp(2) + 1im*exp(2)*sin(3*x)"),
        ("exp_to_trig(trig_to_exp(cos(x)))", "cos(x)"),
        ("exp_to_trig(trig_to_exp(sin(x)))", "sin(x)"),
        ("exp_to_trig(exp(x))", "exp(x)"),
        ("power_expand(sqrt(x^2))", "x"),
        ("power_expand(log(x^2*y))", "log(y) + 2*log(x)"),
        ("power_expand((a*b)^n)", "a^n*b^n"),
        ("power_expand(log(exp(x)))", "x"),
        // one argument only: other calls stay as they are
        ("trig_expand(x, y)", "trig_expand(x, y)"),
        // simplify(e, facts) is simplify under the facts
        ("simplify(x, y)", "x"),
    ]);
}

/// Expressions in x and y for the numeric checks.
const CORPUS: &[&str] = &[
    "sin(x)^2 + cos(x)^2*y",
    "(x^2 - y^2)/(x - y) + 1/(x + 1)",
    "sin(x)^6 + cos(x)^6",
    // (sec has no numeric value yet: 1/cos instead)
    "tan(x)^2 + 1/cos(y)",
    "sin(3*x)*cos(2*y) - cos(x)^3",
    "exp(x)*exp(2*y)*sin(x + y)",
    "cosh(x + y)^2 - sinh(x)*sinh(y)",
    "d(atan(x/(1 - x)), x) + d(sin(x)/x, x)",
    "log(x*y^2) + sqrt(x^2*y^2)",
    "(sin(x) + cos(x))^4 - (sin(x) - cos(x))^4",
];

/// Is the printed `n(1 + (A - B)*10^20, 30)` within 1 of 1 (agreement to about 20 digits)? The 1 keeps the
/// value away from zero, so an exact cancellation still has digits.
fn small(out: &str) -> bool {
    out.trim_end_matches('.').parse::<f64>().is_ok_and(|v| (v - 1.0).abs() < 1.0)
}

#[test]
fn every_function_keeps_the_value() {
    let mut s = Sys::new();
    let mut rng = Lcg(2026);
    for f in ["simplify", "trig_expand", "trig_reduce", "power_expand", "exp_to_trig(trig_to_exp"] {
        let close = if f.contains('(') { "))" } else { ")" };
        for e in CORPUS {
            let src = format!("ee = {e}; ss = {f}(ee{close}; ss");
            let shown = s.run(&src);
            for _ in 0..3 {
                // positive points: power_expand's laws hold there
                let (px, py) = (rng.point(0.1, 0.9), rng.point(0.1, 0.9));
                let out = s.run(&format!("n(1 + rewrite((ee - ss)*10^20, [x -> {px}, y -> {py}]), 30)"));
                assert!(small(&out), "{f}({e}) = {shown} at x = {px}, y = {py}: difference {out}");
            }
        }
    }
}

#[test]
fn simplify_is_idempotent() {
    let mut s = Sys::new();
    for e in CORPUS {
        let once = s.run(&format!("simplify({e})"));
        let twice = s.run(&format!("simplify(simplify({e}))"));
        assert_eq!(once, twice, "{e}");
    }
}

#[test]
fn interrupt_stops_a_long_simplification() {
    let mut s = Sys::new();
    s.run("p = expand((1 + x + y + z)^40)");
    // at several moments, so the stop lands in different kernels (gcds, factorisation, expansion)
    for ms in [5, 20, 60, 150, 400] {
        let (r, took) = s.interrupted("simplify(p)", Duration::from_millis(ms));
        assert!(took < Duration::from_millis(ms) + Duration::from_secs(5), "after {ms} ms: took {took:?}: {r:?}");
    }
    // the session is still usable
    assert_eq!(s.run("simplify(sin(x)^2 + cos(x)^2)"), "1");
}

#[test]
fn medium_size_expressions_stay_within_the_budget() {
    // gap G73: multivariate factoring of these numerators is exponential in the degrees; each polynomial move has
    // a time budget and the search one too, so simplify answers (with the cheapest form found) instead of hanging
    let mut s = Sys::new();
    let e = "-4*lam*p^3/(3*sqrt(lam*p^4/(3*mp^2))*(1 + (a*nc)^5*t^3/(2*f^2)/(1 + 2*nf*nc^4*a^5*t/sqrt(lam*p^4/(3*mp^2)))/(3*sqrt(lam*p^4/(3*mp^2)))))";
    for src in [format!("simplify({e})"), format!("simplify(power_expand({e}))"), format!("simplify({e} /. [t -> 4.0e13, p -> 2.0e19])")] {
        let t0 = std::time::Instant::now();
        let r = s.run(&src);
        let took = t0.elapsed();
        assert!(took < Duration::from_secs(15), "{src}: took {took:?}");
        assert!(!r.starts_with("failure") && !r.starts_with("simplify("), "{src}: {r}");
    }
    // the budget does not stop the moves that finish in time
    assert_eq!(s.run("simplify(x^2 + 2*x + 1)"), "(1 + x)^2");
}
