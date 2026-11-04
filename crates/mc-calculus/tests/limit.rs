//! `limit(f, x -> a)`: known limits at points and infinities, one-sided and two-sided, the rewriting steps of the
//! walk (series, dominance, bounded factors, L'Hospital's rule, common denominators), failures and unevaluated
//! cases, and identical results at 1 and 4 threads.
mod common;
use common::{check, Sys};
use std::time::{Duration, Instant};

/// Limits at finite points (two-sided unless a direction is given).
const AT_POINTS: &[(&str, &str)] = &[
    ("limit(sin(x)/x, x -> 0)", "1"),
    ("limit((exp(x) - 1 - x)/x^2, x -> 0)", "1/2"),
    ("limit((1 - cos(x))/x^2, x -> 0)", "1/2"),
    ("limit(sin(x)^2/x^2, x -> 0)", "1"),
    ("limit(tan(x)/x, x -> 0)", "1"),
    ("limit(log(1 + x)/x, x -> 0)", "1"),
    ("limit((sqrt(1 + x) - 1)/x, x -> 0)", "1/2"),
    ("limit((x^2 - 4)/(x - 2), x -> 2)", "4"),
    ("limit(1/x - 1/sin(x), x -> 0)", "0"),
    ("limit(x*sin(1/x), x -> 0)", "0"),
    ("limit(exp(-1/x^2)/x^10, x -> 0)", "0"),
    ("limit(1/x^2, x -> 0)", "inf"),
    ("limit(x^2 + 3, x -> 1)", "4"),
    ("limit(x^2, x -> a)", "a^2"),
    ("limit((a^x - 1)/x, x -> 0)", "log(a)"),
    ("limit(x*log(x), x -> 0)", "0"),
];

/// One-sided limits.
const ONE_SIDED: &[(&str, &str)] = &[
    ("limit(1/x, x -> 0, direction: from_above)", "inf"),
    ("limit(1/x, x -> 0, direction: from_below)", "-inf"),
    ("limit(x*log(x), x -> 0, direction: from_above)", "0"),
    ("limit(x^x, x -> 0, direction: from_above)", "1"),
    ("limit(x^x^x, x -> 0, direction: from_above)", "0"),
    ("limit(log(x), x -> 0, direction: from_above)", "-inf"),
    ("limit(sqrt(x), x -> 0, direction: from_above)", "0"),
    ("limit(abs(x)/x, x -> 0, direction: from_above)", "1"),
    ("limit(abs(x)/x, x -> 0, direction: from_below)", "-1"),
    ("limit(exp(1/x), x -> 0, direction: from_below)", "0"),
    ("limit(exp(1/x), x -> 0, direction: from_above)", "inf"),
    ("limit(tan(x), x -> pi/2, direction: from_below)", "inf"),
    ("limit(tan(x), x -> pi/2, direction: from_above)", "-inf"),
    ("limit(sign(x), x -> 0, direction: from_below)", "-1"),
    ("limit(x, x -> 0, direction: two_sided)", "0"),
];

/// Limits at infinities.
const AT_INFINITY: &[(&str, &str)] = &[
    ("limit((1 + 1/n)^n, n -> inf)", "e"),
    ("limit((1 + a/x)^x, x -> inf)", "exp(a)"),
    ("limit((2*x^2 + 1)/(3*x^2 - x), x -> inf)", "2/3"),
    ("limit((x^3 + 1)/(x^2 + 1), x -> inf)", "inf"),
    ("limit((x^3 + 1)/(x^2 + 1), x -> -inf)", "-inf"),
    ("limit(x/(x^2 + 1), x -> inf)", "0"),
    ("limit((5*x^4 - x)/(1 - 2*x^4), x -> -inf)", "-5/2"),
    ("limit(sin(x)/x, x -> inf)", "0"),
    ("limit((3*x + sin(x))/x, x -> inf)", "3"),
    ("limit(log(log(x))/log(x), x -> inf)", "0"),
    ("limit(log(x)/x, x -> inf)", "0"),
    ("limit(x^2*exp(-x), x -> inf)", "0"),
    ("limit(exp(x)/x^10, x -> inf)", "inf"),
    ("limit(x*exp(x), x -> -inf)", "0"),
    ("limit(sqrt(x^2 + x) - x, x -> inf)", "1/2"),
    ("limit(x - sqrt(x), x -> inf)", "inf"),
    ("limit(x^(1/x), x -> inf)", "1"),
    ("limit(atan(x), x -> -inf)", "(-1/2)*pi"),
    ("limit(exp(x) - x^100, x -> inf)", "inf"),
    ("limit(1/x, x -> inf, direction: from_below)", "0"),
];

/// Two-sided limits whose sides differ (decision C.1), and argument failures.
const FAILURES: &[(&str, &str)] = &[
    ("limit(1/x, x -> 0)", "failure(no_limit, limit, x^-1)"),
    ("limit(abs(x)/x, x -> 0)", "failure(no_limit, limit, abs(x)/x)"),
    ("limit(tan(x), x -> pi/2)", "failure(no_limit, limit, tan(x))"),
    ("limit(exp(1/x), x -> 0)", "failure(no_limit, limit, exp(x^-1))"),
    ("limit(1/(x - a), x -> a)", "failure(no_limit, limit, (x - a)^-1)"),
    ("limit(x, y)", "failure(argument, limit, \"the point is x -> a\")"),
    ("limit(x, x -> x)", "failure(argument, limit, \"the point depends on the variable\")"),
    ("limit(x, 2 -> 0)", "failure(argument, limit, \"a number is not a variable\")"),
    ("limit(x, x -> 0, direction: up)", DIRECTION),
    ("limit(x, x -> 0, foo: 1)", "failure(unknown_option, foo)"),
    ("limit(1/x, x -> inf, direction: from_above)", FINITE_SIDE),
    ("limit(1/x, x -> -inf, direction: from_below)", FINITE_SIDE),
];

const DIRECTION: &str = "failure(argument, limit, \"direction is from_above, from_below or two_sided\")";
const FINITE_SIDE: &str = "failure(argument, limit, \"an infinity is approached from the finite side\")";

/// Limits this package does not decide stay unevaluated (decision C.2); other forms.
const OTHERS: &[(&str, &str)] = &[
    ("limit(sin(x), x -> inf)", "limit(sin(x), x -> inf)"),
    ("limit(sin(1/x), x -> 0)", "limit(sin(x^-1), x -> 0)"),
    ("limit(f(x), x -> 0)", "limit(f(x), x -> 0)"),
    ("limit(x^a, x -> 0)", "limit(x^a, x -> 0)"),
    // log(x) for x < 0 is complex: the side from below is not decided
    ("limit(log(x), x -> 0)", "limit(log(x), x -> 0)"),
    ("limit(x)", "limit(x)"),
    ("limit(c, x -> 0)", "c"),
    ("limit([x, 1/x, sin(x)/x], x -> 2)", "[2, 1/2, (1/2)*sin(2)]"),
    ("limit([1, sin(x)/x], x -> 0)", "[1, 1]"),
];

#[test]
fn limits_at_points() {
    check(AT_POINTS);
}

#[test]
fn one_sided_limits() {
    check(ONE_SIDED);
}

#[test]
fn limits_at_infinity() {
    check(AT_INFINITY);
}

#[test]
fn different_sides_and_argument_failures() {
    check(FAILURES);
}

#[test]
fn undecided_and_other_forms() {
    check(OTHERS);
}

#[test]
fn undecided_limits_return_quickly() {
    // the rewriting steps nest a bounded number of times: a hard case fails fast
    let mut s = Sys::new();
    for src in [
        "limit(sin(x)*exp(x), x -> inf)",
        "limit(log(x) - log(2*x), x -> inf)",
        "limit(sin(1/x)/x, x -> 0)",
        "limit(x^sin(x), x -> inf)",
        "limit(gamma(x)/x, x -> 0)",
    ] {
        let start = Instant::now();
        let out = s.run(src);
        assert!(out.starts_with("limit("), "{src}: {out}");
        assert!(start.elapsed() < Duration::from_secs(5), "{src}: {:?}", start.elapsed());
    }
}

#[test]
fn one_and_four_threads_agree() {
    let mut one = Sys::with_threads(1);
    let mut four = Sys::with_threads(4);
    for (src, _) in AT_POINTS.iter().chain(ONE_SIDED).chain(AT_INFINITY).chain(FAILURES).chain(OTHERS) {
        assert_eq!(one.run(src), four.run(src), "{src}");
    }
}

#[test]
fn definite_integrals_still_use_the_walk() {
    check(&[
        ("integrate(x^(-1/2), [x, 0, 1])", "2"),
        ("integrate(x*exp(-x), [x, 0, inf])", "1"),
        ("integrate(log(x), [x, 0, 1])", "-1"),
        ("integrate(x*log(x), [x, 0, 1])", "-1/4"),
    ]);
}
