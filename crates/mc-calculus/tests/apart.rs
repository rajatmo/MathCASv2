//! `apart`: partial fraction decomposition (package 2C, item 1).
mod common;
use common::Sys;
use std::time::Duration;

/// Inputs whose decomposition must recombine to the input, with every numerator of degree below its factor's.
const CORPUS: &[&str] = &[
    "1/(x^2 - 1)",
    "(x^3 + 1)/(x^2 + x)",
    "1/(x*(x^2 + 1))",
    "1/(x - 1)^3/(x + 2)",
    "(x^4 + 1)/(x^2 - 4)^2",
    "1/(x^3 - 1)",
    "1/((x^2 + 1)^2*(x - 1))",
    "(3*x^5 - 2*x + 7)/(x^3*(x + 1)^2)",
    "(x^2 + 2*x + 3)/((x^2 + x + 1)*(x^2 + 2))",
    "1/(2*x^2 - 2)",
    "(5*x - 3)/(6*x^2 + x - 1)",
    "x^6/(x^4 - 1)",
    "1/(x^4 + 1)",
    "(x + 1)/(x^2*(x^2 + 2*x + 5)^2)",
    "1/((x - a)*(x - b))",
    "1/(x^2 - a^2)",
    "sin(y)/(x^2 - 1)",
    "(a*x + 1)/((x + 1)*(x - a)^2)",
    "1/(x^3 + x^2 + x + 1) + 1/(x - 3)",
    "(x^3 - 1)/(x - 1)^4",
];

/// `[degree of the numerator, degree of the irreducible factor of the denominator]` for each term of `r`.
fn degrees(s: &mut Sys, r: &str) -> Vec<(i64, i64)> {
    let src = format!(
        "rr = {r}; map(t => [degree(numerator(t), x), degree(denominator(t), x) - \
         degree(polynomial_gcd(denominator(t), d(denominator(t), x)), x)], apply(list, rr + zzz))"
    );
    let out = s.run(&src);
    let nums: Vec<i64> = out
        .split(|c: char| !(c.is_ascii_digit() || c == '-'))
        .filter(|w| !w.is_empty())
        .map(|w| w.parse().unwrap_or_else(|_| panic!("{src}: {out}")))
        .collect();
    assert!(nums.len().is_multiple_of(2), "{src}: {out}");
    nums.chunks(2).map(|c| (c[0], c[1])).collect()
}

#[test]
fn exact_outputs() {
    common::check(&[
        ("apart(1/(x^2 - 1), x)", "(-1/2)/(1 + x) + (1/2)/(-1 + x)"),
        ("apart(1/(x^2 - 1))", "(-1/2)/(1 + x) + (1/2)/(-1 + x)"),
        ("apart((x^3 + 1)/(x^2 + x), x)", "-1 + x + x^-1"),
        ("apart(1/(x*(x^2 + 1)), x)", "-1*x/(1 + x^2) + x^-1"),
        ("apart(1/(x - 1)^3/(x + 2), x)", "(-1/9)*(-1 + x)^-2 + (-1/27)/(2 + x) + (1/27)/(-1 + x) + (1/3)*(-1 + x)^-3"),
        ("apart(1/(x^3 - 1), x)", "(1/3)/(-1 + x) + (-2/3 + (-1/3)*x)/(1 + x + x^2)"),
        ("apart(1/((x - a)*(x - b)), x)", "(a - b)^-1/(x - a) + -1/(a - b)/(x - b)"),
        ("apart(sin(y)/(x^2 - 1), x)", "(-1/2)/(1 + x)*sin(y) + (1/2)/(-1 + x)*sin(y)"),
        ("apart(1/(x*y + 1), x)", "(1 + x*y)^-1"),
    ]);
}

#[test]
fn unchanged_and_declined() {
    common::check(&[
        // polynomials and single terms come back as they are
        ("apart(x^2 + 1, x)", "1 + x^2"),
        ("apart(1/(x + 1), x)", "(1 + x)^-1"),
        ("apart((x + 1)^-2, x)", "(1 + x)^-2"),
        ("apart(y/(y + 1), x)", "y/(1 + y)"),
        // a cancelled denominator gives the expanded quotient
        ("apart((x^2 - 1)/(x - 1), x)", "1 + x"),
        // not rational in x, inexact, several variables or none: unevaluated
        ("apart(sin(x)/(x + 1), x)", "apart((1 + x)^-1*sin(x), x)"),
        ("apart(x^(1/2)/(x + 1), x)", "apart(x^(1/2)/(1 + x), x)"),
        ("apart(1.5/(x^2 - 1), x)", "apart(1.5/(-1 + x^2), x)"),
        ("apart(1/(x*y + 1))", "apart((1 + x*y)^-1)"),
        ("apart(3)", "apart(3)"),
        // a malformed variable
        ("apart(1/(x^2 - 1), 2)", "failure(argument, apart, \"a number is not a variable\")"),
    ]);
}

#[test]
fn corpus_recombines() {
    let mut s = Sys::new();
    for e in CORPUS {
        let r = s.run(&format!("apart({e}, x)"));
        assert!(!r.contains("apart("), "apart({e}, x) unevaluated: {r}");
        let diff = s.run(&format!("together(apart({e}, x) - ({e}))"));
        assert_eq!(diff, "0", "apart({e}, x) = {r} does not recombine");
        let ds = degrees(&mut s, &format!("apart({e}, x)"));
        assert!(ds.iter().any(|&(_, dp)| dp > 0), "apart({e}, x) = {r}: no fraction found");
        for (dn, dp) in ds {
            if dp > 0 {
                assert!(dn < dp, "apart({e}, x) = {r}: a numerator of degree {dn} over a factor of degree {dp}");
            }
        }
    }
}

#[test]
fn interruption() {
    let mut s = Sys::new();
    // a large denominator: many factors of high multiplicity
    let src = "apart(1/((x - 1)^60*(x + 2)^60*(x^2 + 3)^40*(x^3 - 5)^30), x)";
    let (r, took) = s.interrupted(src, Duration::from_millis(200));
    // uninterrupted it takes several seconds
    assert!(r.as_ref().map_or(true, |out| out.contains("interrupted")), "not interrupted: {r:?}");
    assert!(took < Duration::from_secs(2), "took {took:?}");
    assert_eq!(s.run("apart(1/(x^2 - 1), x)"), s.run("(1/2)/(x - 1) - (1/2)/(x + 1)"));
}
