//! Integrals under `digits:` (balls, the tanh-sinh family) and `rigorous: true` (guaranteed enclosures).
mod common;
use common::{agreeing, reference, Sys};

/// The Serret integral pi log(2)/8 of the L6h cases.
const SERRET: &str = "log(1 + x)/(1 + x^2), [x, 0, 1]";

#[test]
fn fifty_digits() {
    let mut s = Sys::new();
    let want = reference("L6h-50.txt");
    for opt in ["", ", rigorous: true"] {
        let got = s.run(&format!("n_integrate({SERRET}, digits: 50{opt})"));
        assert!(agreeing(&got, &want) >= 50, "{opt}: {got}");
    }
}

#[test]
fn five_hundred_digits() {
    let mut s = Sys::new();
    let want = reference("L6h-500.txt");
    for opt in ["", ", rigorous: true"] {
        let got = s.run(&format!("n_integrate({SERRET}, digits: 500{opt})"));
        assert!(agreeing(&got, &want) >= 500, "{opt}: {} digits agree", agreeing(&got, &want));
    }
}

#[test]
fn other_ranges_and_singularities() {
    let mut s = Sys::new();
    let cases: &[(&str, &str)] = &[
        ("n_integrate(1/sqrt(x), [x, 0, 1], digits: 30)", "2.000000000000000000000000000000"),
        ("n_integrate(exp(-x), [x, 0, inf], digits: 30)", "1.000000000000000000000000000000"),
        ("n_integrate(exp(-x^2), [x, -inf, inf], digits: 30)", "1.772453850905516027298167483341"),
        ("n_integrate(exp(x), [x, 0, 1], digits: 30, rigorous: true)", "1.718281828459045235360287471352"),
        ("n_integrate(sin(x), [x, 0, pi], digits: 40, rigorous: true)", "2.0000000000000000000000000000000000000000"),
        ("n_integrate(1/(1 + x^2), [x, 0, 1], digits: 30, rigorous: true)", "0.785398163397448309615660845819"),
        // rigorous at 16 digits by default
        ("n_integrate(exp(x), [x, 0, 1], rigorous: true)", "1.718281828459045235"),
    ];
    for (src, want) in cases {
        let got = s.run(src);
        let n = want.len() - 1;
        assert!(got.len() >= n && got[..n] == want[..n], "{src}: {got}");
    }
    // a complex integrand on balls: the integral of exp(i x) over [0, 1]
    let got = s.run("n_integrate(exp(im*x), [x, 0, 1], digits: 25)");
    assert!(got.starts_with("complex(0.8414709848078965066525023") && got.contains("0.459697694131860282599063"), "{got}");
}

#[test]
fn rigorous_refusals() {
    let mut s = Sys::new();
    // not built from analytic functions (abs is smooth here, but the bound cannot know): the estimate
    let out = s.run("n_integrate(abs(x), [x, 1, 2], digits: 20, rigorous: true)");
    assert!(out.starts_with("failure(not_verified, n_integrate, 1.5000000000000000000"), "{out}");
    // a kink: the estimate has no correct digit
    let out = s.run("n_integrate(abs(x - 0.25), [x, 0, 1], digits: 20, rigorous: true)");
    assert!(out.starts_with("failure(not_verified, n_integrate, "), "{out}");
    // an infinite range has no Bernstein ellipse
    let out = s.run("n_integrate(exp(-x), [x, 0, inf], digits: 20, rigorous: true)");
    assert!(out.starts_with("failure(not_verified, n_integrate, 1.0000000000000000000"), "{out}");
    // a singular end
    let out = s.run("n_integrate(1/sqrt(x), [x, 0, 1], digits: 20, rigorous: true)");
    assert!(out.starts_with("failure(not_verified, n_integrate, 2.000000000000000000"), "{out}");
}
