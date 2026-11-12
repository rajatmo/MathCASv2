//! Round 30:`trig_reduce(e, modulus: p)`.
mod common;
use common::check;

#[test]
fn trig_reduce_with_a_modulus() {
    check(&[
        // the coefficients of cos(x)^50 reduced modulo 7, the common factor outside
        ("trig_reduce(cos(x)^50, modulus: 7)", "4*(cos(48*x) + cos(50*x))"),
        ("trig_reduce(sin(x)^2, modulus: 3)", "2 + cos(2*x)"),
        // a modulus that is no integer leaves the expression; modulus 0 is none
        ("trig_reduce(sinh(t)^2, modulus: pi)", "sinh(t)^2"),
        ("trig_reduce(sin(x)^2, modulus: 0)", "1/2 + (-1/2)*cos(2*x)"),
    ]);
}
