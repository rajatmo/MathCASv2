//! `asymptotic_d_solve_value` at an irregular singular point of a first-order linear equation: the exponential
//! factor exactly, the rest as a Taylor polynomial.
mod common;
use common::Sys;

#[test]
fn first_order_irregular_point() {
    let mut s = Sys::new();
    // x^2 y' + (x^2 + 1) y = 0: y = c(1) exp(1/x) exp(-x)
    s.run("v = asymptotic_d_solve_value(x^2*y'(x) + (x^2 + 1)*y(x) == 0, y(x), [x, 0, 4]); null");
    assert_eq!(s.run("expand(v/(c(1)*exp(1/x)) - (1 - x + x^2/2 - x^3/6 + x^4/24))"), "0");
    // x^3 y' - (1 + x) y = 0: int = -1/(2 x^2) - 1/x, y = c(1) exp(-1/(2 x^2) - 1/x), no regular part
    s.run("w = asymptotic_d_solve_value(x^3*y'(x) - (1 + x)*y(x) == 0, y(x), [x, 0, 3]); null");
    assert_eq!(s.run("simplify(w/(c(1)*exp(-1/(2*x^2) - 1/x)))"), "1");
    // a residue: x^2 y' + (1 + 2 x) y = 0, y = c(1) x^-2 exp(1/x)
    s.run("u = asymptotic_d_solve_value(x^2*y'(x) + (1 + 2*x)*y(x) == 0, y(x), [x, 0, 3]); null");
    assert_eq!(s.run("simplify(u*x^2/(c(1)*exp(1/x)))"), "1");
}
