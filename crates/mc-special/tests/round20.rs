//! Round 20: terminating hypergeometric series with symbolic parameters are polynomials.
mod common;
use common::check;

#[test]
fn terminating_series_with_symbolic_parameters() {
    check(&[
        ("together(hypergeometric_2f1(-2, b, c, z) - (1 - 2*b*z/c + b*(1 + b)*z^2/(c*(1 + c))))", "0"),
        ("together(hypergeometric_1f1(-1, b, z) - (1 - z/b))", "0"),
        ("together(hypergeometric_2f1(-1, 2, c, 1/2) - (1 - 1/c))", "0"),
        // a lower parameter reached first: no polynomial
        ("hypergeometric_2f1(-3, b, -1, z)", "hypergeometric_2f1(-3, b, -1, z)"),
    ]);
}

#[test]
fn q_pochhammer_near_one() {
    // (x; q)_inf by its logarithmic series when q is near 1; (0.5; 0.999)_10 by the quotient of two products
    check(&[
        ("abs(q_pochhammer(0.5, 0.999)/q_pochhammer(0.5*0.999^10, 0.999) - q_pochhammer(0.5, 0.999, 10)) < 10^-12", "true"),
        ("abs(q_pochhammer(0.5, 0.999)/1.2943568161858832e-253 - 1) < 10^-11", "true"),
    ]);
}
