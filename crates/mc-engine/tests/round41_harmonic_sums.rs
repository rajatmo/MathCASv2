//! Round 41: nested harmonic sums in multiple harmonic numbers (the definition, the first index of an inner sum of
//! k, stuffle products, shifted summands, Stirling numbers of order 2). Each closed form is checked against the sum
//! added term by term at a fixed upper bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The closed form of `sum` (upper bound `k`, written `, k]`) agrees with the sum at k = 1, ..., 6 added term by
/// term.
fn agrees(sum: &str) {
    let closed = ev(sum);
    assert!(!closed.contains("sum("), "{sum}: {closed}");
    for j in 1..=6 {
        let direct = sum.replace(", k]", &format!(", {j}]"));
        let src = format!("rewrite({closed}, k -> {j}) - ({direct})");
        assert_eq!(ev(&src), "0", "{sum} at k = {j}: {closed}");
    }
}

#[test]
fn nested_sums_by_definition() {
    assert_eq!(
        ev("sum((1/5)^n2*(1/7)^n4/(n1^2*n2^4*n3^3*n4^6), [n1, 1, n], [n2, 1, n1 - 1], [n3, 1, n2 - 1], [n4, 1, n3 - 1])"),
        "multiple_harmonic_number(n, [2, 4, 3, 6], [1, 1/5, 1, 1/7])"
    );
    assert_eq!(
        ev("sum(1/(n1^2*n2^4*n3^3*n4^6), [n1, 1, k], [n2, 1, n1 - 1], [n3, 1, n2 - 1], [n4, 1, n3 - 1])"),
        "multiple_harmonic_number(k, [2, 4, 3, 6], [1, 1, 1, 1])"
    );
    // variations: other weights and depths, against direct summation
    agrees("sum(2^n1*(-1)^n2/(n1*n2^3), [n1, 1, k], [n2, 1, n1 - 1])");
    agrees("sum((1/3)^n1/(n1^2*n2*n3^2), [n1, 1, k], [n2, 1, n1 - 1], [n3, 1, n2 - 1])");
    // a depth-one sum with a weight is a generalized harmonic number
    assert_eq!(ev("sum(x^j/j^2, [j, 1, m])"), "harmonic_number(m, 2, x)");
}

#[test]
fn bounds_that_are_not_strict() {
    // n1 >= n2 >= ...: every coarsening of the strict chain (letters adding as (r1 + r2, s1 s2))
    let got = ev("sum((1/5)^n2*(1/7)^n4/(n1^2*n2^4*n3^3*n4^6), [n1, 1, k], [n2, 1, n1], [n3, 1, n2], [n4, 1, n3])");
    for part in [
        "harmonic_number(k, 15, 1/35)",
        "multiple_harmonic_number(k, [2, 13], [1, 1/35])",
        "multiple_harmonic_number(k, [6, 9], [1/5, 1/7])",
        "multiple_harmonic_number(k, [2, 4, 3, 6], [1, 1/5, 1, 1/7])",
    ] {
        assert!(got.contains(part), "{got}");
    }
    agrees("sum(1/(n1^2*n2^3), [n1, 1, k], [n2, 1, n1])");
    agrees("sum((-1)^n1*2^n2/(n1*n2*n3^2), [n1, 1, k], [n2, 1, n1], [n3, 1, n2 - 1])");
}

#[test]
fn products_of_harmonic_numbers() {
    // sum H(n)^2 = (k + 1) H(k)^2 - (2 k + 1) H(k) + 2 k
    assert_eq!(
        ev("simplify(sum(harmonic_number(n)^2, [n, 1, k]) - (2*k + (-1 - 2*k + (1 + k)*harmonic_number(k))*harmonic_number(k)))"),
        "0"
    );
    assert_eq!(ev("sum(n^2*harmonic_number(n), [n, m])"), "(1/2)*m*(1 + m) + multiple_harmonic_number(m, [-2, 1], [1, 1])");
    agrees("sum(harmonic_number(n)*harmonic_number(n, 2)/n, [n, 1, k])");
    agrees("sum(harmonic_number(n - 1)^2*(-1)^n/n^2, [n, 1, k])");
    // a summand in k + 1: sum_(j<i<=n) 1/(i j) = (H(n)^2 - H(n, 2))/2
    assert_eq!(ev("simplify(sum(harmonic_number(k)/(k + 1), [k, 1, n - 1]) - (harmonic_number(n)^2 - harmonic_number(n, 2))/2)"), "0");
    agrees("sum(harmonic_number(j, 2)*2^j/(j + 1)^3, [j, 1, k])");
    // an inner sum of k - 2 after the shift is not of this form: left alone
    assert!(ev("sum(harmonic_number(j, 2)/(j + 2)^3, [j, 1, k])").starts_with("sum("));
}

#[test]
fn stirling_numbers_of_order_two() {
    assert_eq!(ev("sum(i^2*stirling_s1(i, 2)/factorial(i - 1), [i, 1, n])"), "multiple_harmonic_number(n, [-2, 1], [-1, 1])");
    assert_eq!(ev("simplify(sum(stirling_s2(i, 2)/i^2, [i, 1, n]) - (1/2)*(harmonic_number(n, 2, 2) - 2*harmonic_number(n, 2)))"), "0");
    agrees("sum(stirling_s1(i, 2)/(i*factorial(i - 1)), [i, 1, k])");
    agrees("sum(stirling_s2(i, 2)*(-1)^i/i^3, [i, 1, k])");
}

#[test]
fn asymptotic_expansion_through_a_nested_sum() {
    // sum_(k=1)^(n-1) H(k)/(k + 1) = (H(n)^2 - H(n, 2))/2 at infinity
    let got = ev("asymptotic_sum(harmonic_number(k)/(k + 1), [k, 1, n - 1], [n, inf, 2])");
    let want = "-1/(8*n^2) - (1/12)*pi^2 + (1/2)*euler_gamma^2 + 1/(2*n) + (1/2)*log(n)^2 + euler_gamma*log(n) - euler_gamma/(12*n^2) - log(n)/(12*n^2) + euler_gamma/(2*n) + log(n)/(2*n)";
    assert_eq!(ev(&format!("simplify(({got}) - ({want}))")), "0");
    // the value at n = 2000 against the sum itself (30 digits)
    assert_eq!(
        ev(&format!("abs(n(rewrite({got}, n -> 2000), 30) - n(sum(harmonic_number(k)/(k + 1), [k, 1, 1999]), 30)) < 10^-9")),
        "true"
    );
}
