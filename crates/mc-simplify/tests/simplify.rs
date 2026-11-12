//! `simplify` and the transforms: exact results, numeric equivalence at random points, idempotence, interruption.
mod common;
use common::{check, Lcg, Sys};
use std::time::Duration;

#[test]
fn huge_trig_powers_are_left_alone() {
    // product-to-sum once listed every unit of the power: sin(x)^(2^31) asked for 16 GB and aborted the process
    check(&[
        ("trig_reduce(sin(x)^(2^31))", "sin(x)^2147483648"),
        ("simplify(64*sin(x)^(2^31)*cos(x)^65 + 4*sin(x)*cos(x) + 1)", "1 + 4*cos(x)*sin(x) + 64*cos(x)^65*sin(x)^2147483648"),
        ("trig_reduce(sin(x)^3*cos(x)^2)", "(-1/16)*sin(5*x) + (1/16)*sin(3*x) + (1/8)*sin(x)"),
    ]);
}

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
        ("trig_to_exp(cosh(x))", "(1/2)*exp(x) + (1/2)*exp(-x)"),
        ("trig_to_exp(cos(x) + im*sin(x))", "exp(1im*x)"),
        ("trig_to_exp(asinh(x))", "log(x + (1 + x^2)^(1/2))"),
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

/// The measure of round 15 (decisions s-cost.1 to s-cost.6): digits and signs count, the measure chooses inside
/// `assuming` too, logarithms of positive rationals combine, and `complexity_function:` replaces the measure.
#[test]
fn complexity_measure_and_option() {
    check(&[
        // digits count: the expanded form's long coefficients make the factored form far cheaper
        ("simplify(expand((3 + 1234567*x)^4))", "(3 + 1234567*x)^4"),
        // signs count: four reciprocals with -1 and -2 inside (27) lose to one fraction (26)
        ("simplify(1/(x + 1) + 1/(x - 1) + 1/(x + 2) + 1/(x - 2))", "(-10*x + 4*x^3)/(4 + -5*x^2 + x^4)"),
        // logarithms of positive rationals
        ("simplify(log(2) + log(3))", "log(6)"),
        ("simplify(2*log(2) + log(x))", "log(4) + log(x)"),
        ("simplify(100*log(2))", "100*log(2)"),
        // round 19: the logarithms of rationals over their primes (`LogPrimes`): 3 log(2) - log(2)
        ("simplify(log(8) - log(2))", "log(4)"),
        // a user measure
        ("simplify(100*log(2), complexity_function: leaf_count)", "log(1267650600228229401496703205376)"),
        ("simplify(4*log(10), complexity_function: leaf_count)", "log(10000)"),
        ("simplify((1 + x)^2, complexity_function: u => -leaf_count(u))", "1 + 2*x + x^2"),
        ("simplify(abs(x), x < 0)", "abs(x)"),
        ("simplify(abs(x), x < 0, complexity_function: u => string_length(to_string(u)))", "-x"),
        ("assuming(x < 0, simplify(abs(x), complexity_function: u => string_length(to_string(u))))", "-x"),
        // a measure that gives no number: nothing replaces the input
        ("simplify(sin(x)^2 + cos(x)^2, complexity_function: u => \"no\")", "cos(x)^2 + sin(x)^2"),
        ("simplify(x^2 - 2*x*y + y^2, complexity_function: automatic)", "(x - y)^2"),
        // a rule-defined measure
        ("rule chars(u_) := string_length(to_string(u))\nsimplify(abs(x), x < 0, complexity_function: chars)", "-x"),
        ("simplify(abs(x), x < 0, complexity_function: leaf_count)", "abs(x)"),
    ]);
}
