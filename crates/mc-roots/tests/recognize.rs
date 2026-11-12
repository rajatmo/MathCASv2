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
