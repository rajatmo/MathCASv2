//! The dispatcher and base rules of `integrate` (linearity, powers, exp, sin, cos), lists, argument failures.
mod common;
use common::Sys;

#[test]
fn base_rules() {
    let mut s = Sys::new();
    for f in ["x^3 + 2*x + 5", "3/x + exp(x) - sin(x)", "a*cos(x)", "x^n", "7", "x^(1/2) + x^-3"] {
        s.check_antiderivative(f, &["(3/7)", "(5/11)"]).unwrap();
    }
    assert_eq!(s.run("integrate([1, x], x)"), "[x, (1/2)*x^2]");
}

#[test]
fn not_found_stays() {
    let mut s = Sys::new();
    assert_eq!(s.run("integrate(f(x), x)"), "integrate(f(x), x)");
    assert_eq!(s.run("integrate(x, y)"), "x*y");
}
