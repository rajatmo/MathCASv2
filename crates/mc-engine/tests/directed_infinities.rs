//! Directed infinities `c*inf` and their constructor `directed_infinity(z)`: the canonical direction, arithmetic,
//! printing that reads back, and limits that give them. Each limit is checked by a second route: the limits of the
//! real and imaginary parts taken separately, or the value of the function far along the ray.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn ev_within(src: &str, limit: Duration) -> String {
    let t = Instant::now();
    let r = ev(src);
    assert!(t.elapsed() < limit, "{src} took {:?}", t.elapsed());
    r
}

#[test]
fn constructor() {
    assert_eq!(ev("directed_infinity(1)"), "inf");
    assert_eq!(ev("directed_infinity(-1)"), "-inf");
    assert_eq!(ev("directed_infinity(-7/3)"), "-inf");
    assert_eq!(ev("directed_infinity(im)"), "1im*inf");
    assert_eq!(ev("directed_infinity()"), "complex_infinity");
    assert_eq!(ev("directed_infinity(0)"), "complex_infinity");
    assert_eq!(ev("directed_infinity(z)"), "directed_infinity(z)");
    assert_eq!(ev("directed_infinity([1, im])"), "[inf, 1im*inf]");
}

#[test]
fn one_coefficient_per_direction() {
    assert_eq!(ev("[2*im*inf, -3*im*inf]"), "[1im*inf, -1im*inf]");
    assert_eq!(ev("[(2 + 2*im)*inf, (1/2 + im/3)*inf, (3 + 4*im)*inf]"), "[complex(1, 1)*inf, complex(3, 2)*inf, complex(3, 4)*inf]");
    assert_eq!(ev("exp(im*pi/4)*inf"), "complex(1, 1)*inf");
    assert_eq!(ev("directed_infinity((1 + im)/sqrt(2)) === directed_infinity(5 + 5*im)"), "true");
    assert_eq!(ev("(0.6 + 0.8*im)*inf === directed_infinity(3.0 + 4.0*im)"), "true");
    // c directed_infinity(z) = directed_infinity(c z)
    assert_eq!(ev("im*directed_infinity(1 + im) === directed_infinity(im*(1 + im))"), "true");
}

#[test]
fn printing_reads_back() {
    for src in ["im*inf", "(1 + im)*inf", "-im*inf", "(3 + 4*im)*inf", "(-1 + 2*im)*inf"] {
        let printed = ev(src);
        assert_eq!(ev(&printed), printed, "{src}");
        assert_eq!(ev(&format!("({printed}) === ({src})")), "true", "{src}");
    }
}

#[test]
fn arithmetic() {
    assert_eq!(ev("1 + im*inf"), "1im*inf");
    assert_eq!(ev("im*inf + 2*im*inf"), "1im*inf");
    assert_eq!(ev("inf + im*inf"), "failure(indeterminate, inf + 1im*inf)");
    assert_eq!(ev("im*inf - im*inf"), "failure(indeterminate, -1im*inf + 1im*inf)");
    assert_eq!(ev("(im*inf)^2"), "-inf");
    assert_eq!(ev("im*inf*(1 + im)*inf"), "complex(-1, 1)*inf");
    assert_eq!(ev("1/(im*inf)"), "0");
    assert_eq!(ev("complex_infinity*im*inf"), "complex_infinity");
    assert_eq!(ev("complex_infinity + im*inf"), "indeterminate");
    assert_eq!(ev("abs((1 + im)*inf)"), "inf");
}

#[test]
fn limits_with_a_direction() {
    let d = Duration::from_secs(20);
    assert_eq!(ev_within("limit(x*im, x -> inf)", d), "1im*inf");
    assert_eq!(ev_within("limit(exp(im*pi/4)*x, x -> inf)", d), "complex(1, 1)*inf");
    assert_eq!(ev_within("limit(x + im*x^2, x -> inf)", d), "1im*inf");
    assert_eq!(ev_within("limit(x - im*x^3, x -> -inf)", d), "1im*inf");
    assert_eq!(ev_within("limit((1 + im)*x^2 - x, x -> inf)", d), "complex(1, 1)*inf");
    assert_eq!(ev_within("limit(im/x, x -> 0, direction: from_above)", d), "1im*inf");
    // the second route: the parts separately
    assert_eq!(ev("[limit(x, x -> inf), limit(x^2, x -> inf), limit(x^2/x, x -> inf)]"), "[inf, inf, inf]");
    // the argument of the function far along the ray tends to the direction's
    assert_eq!(ev("abs(arg(n(exp(im*pi/4)*10^6)) - arg(1 + im)) < 10^-12"), "true");
    assert_eq!(ev("abs(arg(n((1 + im)*10^12 - 10^6)) - arg(1 + im)) < 10^-5"), "true");
    // finite limits of complex functions are unchanged
    assert_eq!(ev_within("limit(im*x, x -> 2)", d), "2im");
    assert_eq!(ev_within("limit(exp(im*x)/x, x -> inf)", d), "0");
}
