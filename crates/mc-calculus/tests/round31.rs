//! Round 31: derivative rows of the Fox H function.
mod common;
use common::{check, Sys};

#[test]
fn fox_h_derivative_row() {
    check(&[
        // H^(1,0)_(0,1)(z | (0, 1)) = exp(-z)
        ("d(fox_h([[], []], [[[0, 1]], []], z), z)", "-exp(-z)"),
        ("d(fox_h([[], []], [[[0, 1]], []], z), [z, 2])", "exp(-z)"),
    ]);
}

#[test]
fn fox_h_derivative_against_differences() {
    let mut s = Sys::new();
    // a general H function: the derivative row against a central difference, first and second order
    let v = s.run(
        "{g = (w => fox_h([[[1/10, 6/5], [13/10, 1]], [[17/5, 2]]], [[[7/5, 2]], [[1/5, 1]]], w)); \
          a = rewrite(d(fox_h([[[1/10, 6/5], [13/10, 1]], [[17/5, 2]]], [[[7/5, 2]], [[1/5, 1]]], z), z), z -> 0.2); \
          b = (g(0.2 + 1e-5) - g(0.2 - 1e-5))/2e-5; \
          c = rewrite(d(fox_h([[[1/10, 6/5], [13/10, 1]], [[17/5, 2]]], [[[7/5, 2]], [[1/5, 1]]], z), [z, 2]), z -> 0.2); \
          e = (g(0.2 + 1e-4) - 2*g(0.2) + g(0.2 - 1e-4))/1e-8; \
          [abs(a - b) < 1e-7, abs(c - e) < 1e-5]}",
    );
    assert_eq!(v, "[true, true]");
}

#[test]
fn spheroidal_s1_derivative_row() {
    let mut s = Sys::new();
    assert_eq!(s.run("d(spheroidal_s1(2, 1, 1.3, z), z)"), "spheroidal_s1_prime(2, 1, 1.3, z)");
    // against a central difference, for |z| > 1 and |z| < 1 (an imaginary value there)
    let v = s.run(
        "[abs(spheroidal_s1_prime(2, 1, 1.3, 2.5) - (spheroidal_s1(2, 1, 1.3, 2.5 + 1e-5) - spheroidal_s1(2, 1, 1.3, 2.5 - 1e-5))/2e-5) < 1e-8, \
          abs(spheroidal_s1_prime(2, 1, 1.3, 0.4) - (spheroidal_s1(2, 1, 1.3, 0.4 + 1e-5) - spheroidal_s1(2, 1, 1.3, 0.4 - 1e-5))/2e-5) < 1e-8]",
    );
    assert_eq!(v, "[true, true]");
}

#[test]
fn spheroidal_s1_second_derivative_from_the_ode() {
    let mut s = Sys::new();
    let v = s.run(
        "[abs(rewrite(d(spheroidal_s1(2, 1, 1.3, z), [z, 2]), z -> 2.5) - (spheroidal_s1_prime(2, 1, 1.3, 2.5 + 1e-5) - spheroidal_s1_prime(2, 1, 1.3, 2.5 - 1e-5))/2e-5) < 1e-7]",
    );
    assert_eq!(v, "[true]");
}
