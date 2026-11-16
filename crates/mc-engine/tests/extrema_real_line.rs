//! Global extrema of functions of one variable on the whole real line and on closed half-lines: the unbounded
//! domain is reduced with proof to a bounded interval (tail enclosures or monotone tails, reflection symmetry,
//! monotone outer functions), and the extremum is a root object of f'. Every answer is checked independently: the
//! value against a dense machine grid over a wide range (no sample lies below a minimum or above a maximum) and a
//! fine grid around the point, and the point by the sign change of f' across it at 30 digits. Each test has a time
//! bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

/// The minimum (maximum when `max`) of `f` over the line: a root object point, the value at 30 digits not above
/// (below) any grid sample on [lo, hi] and within 10^-9 of the least sample of a fine grid around the point, f'
/// changing sign there, and `minimize` agreeing with `min_value` and `arg_min`.
fn check(e: &mut CoreEngine, f: &str, max: bool, lo: &str, hi: &str) {
    let (mv, am, mz, cmp, pick, s1, s2, slack) = if max {
        ("max_value", "arg_max", "maximize", "<=", "max", ">", "<", "+")
    } else {
        ("min_value", "arg_min", "minimize", ">=", "min", "<", ">", "-")
    };
    let p = ev(e, &format!("{am}({f}, x)"));
    assert!(p.starts_with("root("), "{am}({f}, x) gave {p}");
    holds(e, &format!("{{ let v = n({mv}({f}, x), 30); let s = {pick}(table({f}, x, range({lo}, {hi}, 1/128))); s {cmp} v {slack} 10^-12 }}"));
    holds(e, &format!(
        "{{ let p = n({am}({f}, x), 30); let v = n({mv}({f}, x), 30); let s = {pick}(table({f}, x, range(p - 1/1000, p + 1/1000, 1/10^6))); abs(s - v) < 10^-9 and s {cmp} v {slack} 10^-20 }}"
    ));
    holds(e, &format!(
        "{{ let p = n({am}({f}, x), 40); let g = d({f}, x); n(rewrite(g, x -> p - 10^-10), 20) {s1} 0 and n(rewrite(g, x -> p + 10^-10), 20) {s2} 0 }}"
    ));
    holds(e, &format!("{{ let r = {mz}({f}, x); r[1] === {mv}({f}, x) and r[2] === [x -> {am}({f}, x)] }}"));
}

#[test]
fn exponential_against_logarithm_and_line() {
    timed("exp(2 exp(x)) - log(x^2 + 1) - 20 x", 60, || {
        let mut e = CoreEngine::new();
        check(&mut e, "e^(2 * e^x) - log(x^2 + 1) - 20 * x", false, "-10.0", "3.0");
        check(&mut e, "-e^(2 * e^x) + log(x^2 + 1) + 20 * x", true, "-10.0", "3.0");
        holds(&mut e, "abs(n(min_value(e^(2 * e^x) - log(x^2 + 1) - 20 * x, x) + max_value(-e^(2 * e^x) + log(x^2 + 1) + 20 * x, x), 30)) < 10^-25");
    });
}

#[test]
fn monotone_outer_function_of_dawson_integral() {
    timed("sinh_integral(dawson_f(x))", 60, || {
        let mut e = CoreEngine::new();
        check(&mut e, "sinh_integral(dawson_f(x))", true, "-25.0", "25.0");
        check(&mut e, "sinh_integral(dawson_f(x))", false, "-25.0", "25.0");
        // the maximum of Dawson's integral itself, at the root of 1 - 2 x F(x)
        holds(&mut e, "{ let p = arg_max(sinh_integral(dawson_f(x)), x); abs(n(1 - 2*p*dawson_f(p), 30)) < 10^-25 }");
    });
}

#[test]
fn bessel_of_a_quadratic_with_mirror_symmetry() {
    timed("bessel_j(7/4, x^2 + x + 1)", 90, || {
        let mut e = CoreEngine::new();
        check(&mut e, "bessel_j(7/4, x^2 + x + 1)", true, "-12.0", "11.0");
        check(&mut e, "bessel_j(7/4, x^2 + x + 1)", false, "-12.0", "11.0");
        // the two mirror points x and -1 - x tie: the one on the left of the axis x = -1/2 is given
        holds(&mut e, "n(arg_max(bessel_j(7/4, x^2 + x + 1), x)) < -1/2 and n(arg_min(bessel_j(7/4, x^2 + x + 1), x)) < -1/2");
    });
}

#[test]
fn half_lines_and_declined_cases() {
    timed("half-lines", 60, || {
        let mut e = CoreEngine::new();
        // x <= 4: the left tail grows like -3 x
        holds(&mut e, "{ let r = minimize([exp(x) - 3*x + sin(x), x <= 4], x); let p = r[2][1][2]; abs(n(exp(p) - 3 + cos(p), 30)) < 10^-25 and n(p) > 0.85 and n(p) < 0.86 }");
        holds(&mut e, "{ let v = n(min_value([exp(x) - 3*x + sin(x), x <= 4], x), 30); min(table(exp(x) - 3*x + sin(x), x, range(-20.0, 4.0, 1/128))) >= v - 10^-12 }");
        // sup sin(x)/x = 1 is not attained (x = 0 is outside the domain): unevaluated
        assert_eq!(ev(&mut e, "head(max_value(sin(x)/x, x))"), "max_value");
        // an unbounded oscillation without a decaying envelope: no tail bound, unevaluated
        assert_eq!(ev(&mut e, "head(min_value(x*sin(x), x))"), "min_value");
    });
}

#[test]
fn radical_equation_with_generated_constants() {
    timed("radical equations with c(k)", 60, || {
        let mut e = CoreEngine::new();
        let s = ev(&mut e, "solve(2*(c(1)*u)^(1/2) == x*c(1) + y + c(2), u)");
        assert_eq!(s, ev(&mut e, "[[u -> (y + c(2) + x*c(1))^2/(4*c(1))]]"));
        // substituted back at sample values of the constants and parameters
        holds(&mut e, "{ let u0 = solve(2*(c(1)*u)^(1/2) == x*c(1) + y + c(2), u)[1][1][2]; let r = [c(1) -> 2, c(2) -> 3, x -> 1/2, y -> 1]; n(rewrite(2*(c(1)*u0)^(1/2) - (x*c(1) + y + c(2)), r), 30) == 0 }");
        assert_eq!(ev(&mut e, "solve(sqrt(c(1)*u) == c(2), u)"), ev(&mut e, "[[u -> c(2)^2/c(1)]]"));
        // a spurious root is still dropped
        assert_eq!(ev(&mut e, "solve(sqrt(u) + 1 == 0, u)"), "[]");
    });
}
