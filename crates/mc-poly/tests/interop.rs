//! Truncated series in arithmetic, and `normal` of associations and of chosen heads.
mod common;
use common::eval;

#[test]
fn series_arithmetic() {
    let cases = [
        ("s = series(tan(x), x, 0, 5); s^2", "x^2 + 2*x^4/3 + 17*x^6/45 + O(x^7)"),
        ("s = series(tan(x), x, 0, 5); 1 + 2*s", "1 + 2*x + 2*x^3/3 + 4*x^5/15 + O(x^6)"),
        ("series(log(1 + x), x, 0, 4)*series(exp(x), x, 0, 3)", "x + x^2/2 + x^3/3 + O(x^5)"),
        ("1/series(cos(x), x, 0, 4)", "1 + x^2/2 + 5*x^4/24 + O(x^5)"),
        ("series(sin(x), x, 0, 5)^2 + series(cos(x), x, 0, 5)^2", "1 + O(x^6)"),
        // series at different points are not combined
        ("series(sin(x), x, 0, 1) + series(sin(x), x, 1, 1)", "(x + O(x^2)) + (sin(1) + cos(1)*(x - 1) + O((x - 1)^2))"),
    ];
    for (src, want) in cases {
        assert_eq!(eval(src), want, "{src}");
    }
}

#[test]
fn normal_of_associations_and_heads() {
    let cases = [
        ("normal(assoc(p -> 1, r -> 2))", "[p -> 1, r -> 2]"),
        ("normal([assoc(p -> 1), series(exp(x), x, 0, 2)], assoc)", "[[p -> 1], 1 + x + x^2/2 + O(x^3)]"),
        ("normal([assoc(p -> 1), series(exp(x), x, 0, 2)], [series, assoc])", "[[p -> 1], 1 + x + (1/2)*x^2]"),
    ];
    for (src, want) in cases {
        assert_eq!(eval(src), want, "{src}");
    }
}

#[test]
fn poles_relations_and_gcd() {
    let cases = [
        ("series(coth(x), x, 0, 5)", "x^-1 + x/3 - x^3/45 + 2*x^5/945 + O(x^6)"),
        ("series(tan(x)^3 + x, x, pi/2, 1)", "-(x - (1/2)*pi)^-3 + (x - (1/2)*pi)^-1 + (1/2)*pi + 11*(x - (1/2)*pi)/15 + O((x - (1/2)*pi)^2)"),
        ("series(exp(x), x, 2*pi, 1)", "e^(2*pi) + e^(2*pi)*(x - 2*pi) + O((x - 2*pi)^2)"),
        ("factor(x^2 - 1 == x^3 - x)", "(-1 + x)*(1 + x) == x*(-1 + x)*(1 + x)"),
        ("cancel((x^2 - 1)/(x - 1) == 2 and x > 1)", "1 + x == 2 and x > 1"),
        ("gcd(a, 12)", "gcd(a, 12)"),
        ("gcd(6*x^2 + 6, 4*x^2 + 4)", "2 + 2*x^2"),
    ];
    for (src, want) in cases {
        assert_eq!(eval(src), want, "{src}");
    }
}
