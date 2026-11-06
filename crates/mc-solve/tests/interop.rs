//! Equations of other packages' functions with conditions, and the `assumptions:` option (package io-sym).
mod common;
use common::{check, Sys};

#[test]
fn bounded_transcendental_equations() {
    check(&[
        ("solve(sin(x) == 0 and 0 < x and x < 7, x)", "[[x -> pi], [x -> 2*pi]]"),
        ("solve(d(cos(x), x) == 0 and 0 < x and x < 2*pi, x)", "[[x -> pi]]"),
        ("solve(tan(x) == 1 and 0 < x and x < 4, x)", "[[x -> (1/4)*pi], [x -> (5/4)*pi]]"),
        ("solve(erf(x) == 0 and -1 < x and x < 1, x)", "[[x -> 0]]"),
        ("solve(abs(x) == 2, x, reals)", "[[x -> -2], [x -> 2]]"),
        ("solve(x^2 == 4, x, assumptions: x > 0)", "[[x -> 2]]"),
        ("reduce(x^2 == 4, x, assumptions: x > 0)", "x == 2"),
    ]);
    let mut s = Sys::new();
    let r = s.number("rewrite(x, solve(bessel_j(0, x) == 0 and 0 < x and x < 3, x)[1])");
    assert!((r - 2.404_825_557_695_773).abs() < 1e-12, "{r}");
}

#[test]
fn integers_and_inexact_bounds() {
    check(&[
        ("reduce(gcd(a, 12) > gcd(a^2 - 1, 12) and -10 < a and a < 10, a, integers)", "a == -8 or a == -6 or a == -4 or a == 0 or a == 4 or a == 6 or a == 8"),
        ("solve(cos(x) == 0 and 0 < x and x < 4.0, x)", "[[x -> 1.5707963267948966]]"),
    ]);
}
