//! Round 36, algebra: regression tests for fixed wrong answers. Values are checked by identities.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn variables_of_roots_reciprocals_and_exponentials() {
    assert_eq!(ev("variables((a - b) / (x + y) - 2 / z)"), "[a, b, x, y, z]");
    assert_eq!(ev("variables(sqrt(x + y - z^2) + (-2 * t)^(2 / 3))"), "[t, x, y, z]");
    assert_eq!(ev("variables(e^x)"), "[]");
}

#[test]
fn number_fields_discriminants() {
    // cubic field of discriminant -31; Q(sqrt 2, i) has discriminant 256
    assert_eq!(ev("number_field_discriminant(algebraic_number(root(&(&1^3 + &1 + 1), 3), [1, 2, 1]))"), "-31");
    assert_eq!(ev("number_field_discriminant(to_number_field([sqrt(2), im], all)[1, 1])"), "256");
}

#[test]
fn algebraic_number_arithmetic_matches_the_values() {
    // (9 + sqrt 82)^2 = 163 + 18 sqrt 82, and the norm 9^2 - 82 = -1 is preserved by powers up to sign
    assert_eq!(ev("map(last, algebraic_number(sqrt(82), [9, 1])^range(1, 3))"), "[[9, 1], [163, 18], [2943, 325]]");
    assert_eq!(ev("chop(n(algebraic_number(sqrt(82), [9, 1])^3 - (9 + sqrt(82))^3, 30))"), "0");
}

#[test]
fn factor_over_a_prime_field() {
    assert_eq!(ev("factor(x^6 + 3 * x^4 + x, modulus: 3)"), "x*(1 + x)*(1 + 2*x + 2*x^3 + x^2 + x^4)");
}

#[test]
fn gaussian_gcd_is_monic() {
    assert_eq!(ev("polynomial_gcd(x^2 + 1, x^3 + 3 * im * x^2 - 3 * x - im)"), "1im + x");
}

#[test]
fn digits_at_machine_precision() {
    assert_eq!(ev("real_digits(123.55555)"), "[[1, 2, 3, 5, 5, 5, 5, 5, 0, 0, 0, 0, 0, 0, 0, 0], 3]");
    assert_eq!(ev("number_expand(1111 / 2, 10, 9)"), "[500, 50, 5, 1/2, 0, 0, 0, 0, 0]");
    assert_eq!(ev("number_expand(10993/7000, 10, 5)"), "[1, 1/2, 7/100, 0, 3/7000]");
}

#[test]
fn bell_y_of_a_square_matrix_and_polygonal_numbers() {
    assert_eq!(ev("table(bell_y(constant_array(1, [n, n])), n, range(1, 5))"), "[1, 2, 12, 154, 3455]");
    assert_eq!(ev("polygonal_number(regular_polygon(5), 3)"), "12");
    assert_eq!(ev("polygonal_number(range(10, 20, 2), range(5, 10))"), "[85, 156, 259, 400, 585, 820]");
}

#[test]
fn integer_reverse_pads_to_a_length_and_threads() {
    // the bit reversal permutation of 0..3 in two bits is 0, 2, 1, 3
    assert_eq!(ev("integer_reverse([0, 1, 2, 3], 2, 2)"), "[0, 2, 1, 3]");
    assert_eq!(ev("integer_reverse(6, 2, 4)"), "6");
}
