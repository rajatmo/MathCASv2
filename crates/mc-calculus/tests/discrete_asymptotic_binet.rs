//! Leading terms of Fibonacci and Lucas sequences in `discrete_asymptotic` (Binet's formula).
mod common;
use common::check;

/// The leading form, and its values rounded to the nearest integer against the known terms
/// F(40) = 102334155, F(71) = 308061521170129, L(30) = 1860498 (the neglected term is below 1/2).
#[test]
fn binet_leading_terms() {
    check(&[
        ("discrete_asymptotic(fibonacci(n), n -> inf)", "(1/5)*5^(1/2)*golden_ratio^n"),
        ("discrete_asymptotic(lucas_l(n), n -> inf)", "golden_ratio^n"),
        ("discrete_asymptotic(fibonacci(2*n)/n^2, n -> inf)", "(1/5)*5^(1/2)*golden_ratio^(2*n)*n^-2"),
        ("round(n(rewrite(discrete_asymptotic(fibonacci(n), n -> inf), n -> 40), 30))", "102334155"),
        ("round(n(rewrite(discrete_asymptotic(fibonacci(n), n -> inf), n -> 71), 40))", "308061521170129"),
        ("round(n(rewrite(discrete_asymptotic(lucas_l(n), n -> inf), n -> 30), 30))", "1860498"),
        // a difference whose leading terms cancel is not reduced to 0
        ("head(discrete_asymptotic(fibonacci(n + 1) - golden_ratio*fibonacci(n), n -> inf))", "asymptotic"),
    ]);
}
