//! Round 27: fixes of finite-field bugs found in round 26 (D26-06 to D26-09, D26-21).
use mc_api::{Engine, StopFlag};

fn ev(src: &str) -> String {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn d26_06_negative_and_rational_data() {
    // -1 and 1/2 are numbers of the prime field F_3: 2 and 2
    assert_eq!(ev("F = finite_field(3, 2); [F(-1) == F(2), F(1/2) == F(2), F(-1)(\"Coefficients\")]"), "[true, true, [2]]");
    assert_eq!(ev("F = finite_field(3, 2); F(1/3)"), ev("F = finite_field(3, 2); F(1/3)"));
    assert!(ev("F = finite_field(3, 2); F(1/3)").starts_with("finite_field(3, "));
}

#[test]
fn d26_07_expand_of_a_factorisation_equals_the_polynomial() {
    assert_eq!(ev("F = finite_field(3, 2); p = x^2 + 1; q = factor(p, extension: F); [expand(q) == p, p == expand(q), x^2 + 2 == expand(q)]"), "[true, true, false]");
}

#[test]
fn d26_08_factor_over_a_prime_power_modulus() {
    let r = ev("q = factor(x^4 + 1, modulus: 9); is_list(q) or not match_q(q, failure(__))");
    assert_eq!(r, "true");
    assert_eq!(ev("expand(factor(x^4 + 1, modulus: 9)) == x^4 + 1"), "true");
}

#[test]
fn d26_09_multiplicative_order_of_an_element() {
    assert_eq!(ev("F = finite_field(3, 2); [multiplicative_order(F([1, 1])), multiplicative_order(F(2)), multiplicative_order(F(1))]"), "[4, 2, 1]");
    // the zero element has no order
    assert!(ev("F = finite_field(3, 2); multiplicative_order(F(0))").starts_with("multiplicative_order("));
}

#[test]
fn d26_21_embedding_with_an_invalid_image_and_a_named_variable() {
    assert!(ev("finite_field_embedding(finite_field(3, 2)([0, 1]) -> finite_field(3, 4)([1, 1]))").starts_with("failure(argument, finite_field_embedding"));
    assert_eq!(ev("[is_irreducible_polynomial(x^2 + 1, x, modulus: 3), is_irreducible_polynomial(x^2 + 1, x, modulus: 5)]"), "[true, false]");
}
