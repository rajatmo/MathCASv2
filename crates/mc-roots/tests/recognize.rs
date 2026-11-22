//! `find_integer_null_vector` (PSLQ) and `root_approximant`.
mod common;
use common::Sys;

#[test]
fn integer_relations() {
    let mut s = Sys::new();
    assert_eq!(s.run("find_integer_null_vector([1, sqrt(2.0), 2.0])"), "[2, 0, -1]");
    // Machin's formula at 30 digits
    assert_eq!(s.run("find_integer_null_vector(n([pi/4, atan(1/5), atan(1/239)], 30))"), "[1, -4, 1]");
    assert_eq!(s.run("find_integer_null_vector(n([1, log(2), log(3), log(6)], 40))"), "[0, 1, 1, -1]");
    assert!(s.run("find_integer_null_vector(n([zeta(3), pi^3, 1], 50))").starts_with("failure(not_found"));
}

#[test]
fn algebraic_numbers() {
    let mut s = Sys::new();
    assert_eq!(s.run("root_approximant(n(sqrt(67)))"), "67^(1/2)");
    assert_eq!(s.run("root_approximant(3/7)"), "3/7");
    assert_eq!(s.run("simplify(root_approximant(n(sqrt(2) + sqrt(3)), 4) - sqrt(2) - sqrt(3))"), "0");
    assert!(s.run("root_approximant(3.1415926)").starts_with("root(&("));
}

#[test]
fn complex_algebraic_numbers() {
    let mut s = Sys::new();
    // sqrt(2) + i is a root of x^4 - 2 x^2 + 9 (x - i = sqrt(2): x^2 - 2 i x - 3 = 0, times its conjugate); the
    // answer is that root: its value agrees with sqrt(2) + i to 30 digits
    s.run("a = root_approximant(n(sqrt(2) + im), 4)");
    assert_eq!(s.run("expand(minimal_polynomial(a, x) - (x^4 - 2 x^2 + 9))"), "0");
    assert_eq!(s.run("abs(n(a, 30) - n(sqrt(2) + im, 30)) < 10^-25"), "true");
    // a Gaussian rational and a quadratic irrational of degree 2 over Q
    assert_eq!(s.run("root_approximant(n(3/4 - 2 im)) - (3/4 - 2 im)"), "0");
    assert_eq!(s.run("simplify(root_approximant(n((1 + im sqrt(3))/2)) - (1 + im sqrt(3))/2)"), "0");
    // no relation of small height: not found (e + i pi)
    assert!(s.run("root_approximant(n(e + im pi))").starts_with("failure(not_found"));
}
