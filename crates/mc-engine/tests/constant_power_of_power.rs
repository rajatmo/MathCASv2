//! `(b^a)^k = b^(a k)` for a numeric constant b when `a arg(b)` is certainly in (-pi, pi]: the principal values
//! of the nested powers, computed independently at 35 digits, against the flattened results.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `|n(e, 30) - v| < 10^-28` for the value v (a complex value as `re + im*i`).
fn close(e: &str, v: &str) {
    assert_eq!(ev(&format!("abs(n({e}, 30) - ({v})) < 10^-28")), "true", "{e} = {}", ev(e));
}

#[test]
fn nested_powers_of_constants() {
    // principal sqrt((pi + 2 i)^2) = 3.14159265358979323846264338328 + 2 i, principal sqrt(pi^2) = pi
    assert_eq!(ev("sqrt(pi^2)"), "pi");
    assert_eq!(ev("sqrt((pi + 2*im)^2)"), "2im + pi");
    assert_eq!(ev("(pi^3)^(1/3)"), "pi");
    assert_eq!(ev("sqrt(exp(2))"), "e");
    close("sqrt((pi + 2*im)^2)", "3.14159265358979323846264338328 + 2*im");
    // (1/(pi + i))^(1/3) = 0.668347499044083107006037159406 - 0.0688971814369251396080840527192 i
    assert_eq!(ev("(1/(pi + im))^(1/3)"), "(1im + pi)^(-1/3)");
    close("(1/(pi + im))^(1/3)", "0.668347499044083107006037159406 - 0.0688971814369251396080840527192*im");
    // the function of the page: 1/(z - sqrt(z^2)) at z with re z > 0 is 1/0
    assert_eq!(ev("w(z_) := 1/(z - sqrt(z^2)); [w(2), w(pi), w(pi + 2*im)]"), "[failure(division_by_zero), failure(division_by_zero), failure(division_by_zero)]");
}

#[test]
fn nested_powers_that_stay() {
    // a base that is negative or off the right half plane, or not a known constant: no rule, the value stays right
    assert_eq!(ev("sqrt((1 - pi)^2)"), "((1 - pi)^2)^(1/2)");
    close("sqrt((1 - pi)^2)", "2.14159265358979323846264338328");
    assert_eq!(ev("sqrt((-pi - 2*im)^2)"), "((-2im - pi)^2)^(1/2)");
    close("sqrt((-pi - 2*im)^2)", "3.14159265358979323846264338328 + 2*im");
    assert_eq!(ev("sqrt(x^2)"), "(x^2)^(1/2)");
    assert_eq!(ev("(pi^4)^(1/2)"), "pi^2");
    assert_eq!(ev("((im*pi)^2)^(1/2)"), "(-pi^2)^(1/2)");
}

#[test]
fn principal_square_roots_of_squares() {
    // a negative, an imaginary and a negative constant base: the principal root, never the base itself
    assert_eq!(ev("sqrt((-3)^2)"), "3");
    assert_eq!(ev("sqrt(im^2)"), "1im");
    assert_eq!(ev("sqrt((-im)^2)"), "1im");
    close("sqrt((-pi)^2)", "3.14159265358979323846264338328");
    assert_ne!(ev("sqrt((-pi)^2)"), "-pi");
    // a positive constant base
    assert_eq!(ev("sqrt(e^2)"), "e");
    assert_eq!(ev("(golden_ratio^2)^(1/2)"), "golden_ratio");
    // a symbol: no rule
    assert_eq!(ev("sqrt(x^2)"), "(x^2)^(1/2)");
    assert_eq!(ev("sqrt((-x)^2)"), "(x^2)^(1/2)");
}
