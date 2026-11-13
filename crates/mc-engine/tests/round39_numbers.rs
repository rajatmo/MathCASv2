//! Round 39: number theory and integer equations. Integer roots of sparse polynomials of high degree, expansions in
//! a base that is not an integer, threading of gcd, lcm, extended_gcd and factor_integer over lists, modular roots in
//! power_mod, Gaussian integers (coprimality, divisors, powers modulo an integer), determinants and inverses modulo
//! m, the parity predicates on non-integers. Each answer is checked by substitution or by a second computation.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn integer_roots_of_sparse_polynomials() {
    let mut e = CoreEngine::new();
    let p = "(x^999997 - 25 * x^12345 + 24 * x) * (x - 7) * (x^2 - 9)";
    assert_eq!(ev(&mut e, &format!("reduce({p} == 0, x, integers)")), "x == -3 or x == -1 or x == 0 or x == 1 or x == 3 or x == 7");
    assert_eq!(ev(&mut e, &format!("reduce({p} == 0, x, negative_integers)")), "x == -3 or x == -1");
    assert_eq!(ev(&mut e, &format!("reduce({p} == 0, x, positive_integers)")), "x == 1 or x == 3 or x == 7");
    assert_eq!(ev(&mut e, &format!("reduce({p} == 0, x, nonnegative_integers)")), "x == 0 or x == 1 or x == 3 or x == 7");
    // variations: a root of size 2 in a sparse factor (2^1000 - 2^1000), no integer root, and the same answers as the
    // dense method at a low degree
    assert_eq!(ev(&mut e, "reduce(x^1000 - 2^1000 == 0, x, integers)"), "x == -2 or x == 2");
    assert_eq!(ev(&mut e, "reduce(x^777 + x + 5 == 0, x, integers)"), "false");
    assert_eq!(ev(&mut e, "reduce(x^61 - x^60 - 4*x + 4 == 0, x, integers)"), "x == 1");
    // independent check: each root substituted gives 0
    for r in ["-3", "-1", "0", "1", "3", "7"] {
        assert_eq!(ev(&mut e, &format!("rewrite({p}, x -> {r})")), "0", "{r}");
    }
}

#[test]
fn expansions_in_a_base_that_is_not_an_integer() {
    let mut e = CoreEngine::new();
    // the parts add up to the number and each digit is below the base
    for (x, b, n) in [("9", "6.5", ""), ("1111/7", "7.5", ", 8"), ("1.23", "3.5", ", 10"), ("1.23", "7/2", ", 10"), ("1532", "10.", "")] {
        let l = ev(&mut e, &format!("l = number_expand({x}, {b}{n}); l"));
        assert!(l.starts_with('['), "{x} {b}: {l}");
        assert_eq!(ev(&mut e, &format!("abs(total(l) - {x}) < 10^-9")), "true", "{x} {b}: {l}");
    }
    assert_eq!(ev(&mut e, "take(number_expand(9, 6.5), 3)"), "[6.5, 2.0, 0.46153846153846156]");
    assert_eq!(ev(&mut e, "take(number_expand(1532, 10.), 5)"), "[1000.0, 500.0, 30.0, 2.0, 0.0]");
    assert_eq!(ev(&mut e, "length(number_expand(1532, 10.))"), "16");
    assert_eq!(ev(&mut e, "take(number_expand(1.23, 7/2, 10), 5)"), "[1, 0, 8/49, 16/343, 48/2401]");
}

#[test]
fn threading_over_lists() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "gcd(12, [3, 7, 40])"), "[3, 1, 4]");
    assert_eq!(ev(&mut e, "lcm(12, [3, 7, 40])"), "[12, 84, 120]");
    assert_eq!(ev(&mut e, "extended_gcd(3, [5, 15])"), "[[1, [2, -1]], [3, [1, 0]]]");
    assert_eq!(ev(&mut e, "factor_integer([11, 101, 1001])"), "[[[11, 1]], [[101, 1]], [[7, 1], [11, 1], [13, 1]]]");
    // variations: two lists, and the Bezout identity of each pair
    assert_eq!(ev(&mut e, "gcd([4, 9], [6, 12])"), "[2, 3]");
    let bezout = "l = extended_gcd(3, [5, 15]); [l[1][2][1]*3 + l[1][2][2]*5, l[2][2][1]*3 + l[2][2][2]*15] == [l[1][1], l[2][1]]";
    assert_eq!(ev(&mut e, bezout), "true");
}

#[test]
fn modular_roots_and_gaussian_integers() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "power_mod(2, 1/2, 7)"), "3");
    assert_eq!(ev(&mut e, "power_mod(3, 1/2, 2)"), "1");
    // variations: a cube root, a negative rational power, no root (the call stays); each checked by powering back
    assert_eq!(ev(&mut e, "r = power_mod(5, 1/3, 13); mod(r^3, 13)"), "5");
    assert_eq!(ev(&mut e, "s = power_mod(2, -1/2, 7); mod(s^2*2, 7)"), "1");
    assert_eq!(ev(&mut e, "power_mod(3, 1/2, 7)"), "power_mod(3, 1/2, 7)");
    assert_eq!(ev(&mut e, "power_mod(2 + im, 2, 3)"), "1im");
    assert_eq!(ev(&mut e, "power_mod(2 + im, 5, 7) == mod(re(expand((2 + im)^5)), 7) + im*mod(im_part(expand((2 + im)^5)), 7)"), "true");
    assert_eq!(ev(&mut e, "is_coprime(5 + im, 1 - im)"), "false");
    assert_eq!(ev(&mut e, "is_coprime(2 + im, 2 - im)"), "true");
    assert_eq!(ev(&mut e, "is_coprime(a, b)"), "false");
    assert_eq!(ev(&mut e, "length(divisors(6 + 4 * im))"), "6");
    // every divisor divides: the quotients have integer parts
    assert_eq!(ev(&mut e, "every(divisors(6 + 4 * im), d => is_integer(re(expand((6 + 4*im)/d))) and is_integer(im_part(expand((6 + 4*im)/d))))"), "true");
}

#[test]
fn matrices_modulo_an_integer() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "inverse([[1, 2], [3, 4]], modulus: 3)"), "[[1, 1], [0, 1]]");
    assert_eq!(ev(&mut e, "det([[1, 2, 3], [21, 210, 2100], [2221, 3332, 4443]], modulus: 8)"), "6");
    // variations: the product with the inverse is the identity modulo m; no inverse when gcd(det, m) > 1
    assert_eq!(ev(&mut e, "a = [[2, 3, 1], [1, 4, 5], [7, 1, 2]]; mod(dot(a, inverse(a, modulus: 11)), 11)"), "[[1, 0, 0], [0, 1, 0], [0, 0, 1]]");
    assert_eq!(ev(&mut e, "inverse([[2, 0], [0, 2]], modulus: 4)"), "inverse([[2, 0], [0, 2]], modulus: 4)");
}

#[test]
fn parity_and_rational_expressions() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "is_even(x)"), "false");
    assert_eq!(ev(&mut e, "is_odd(x)"), "false");
    assert_eq!(ev(&mut e, "is_even(2*(golden_ratio - golden_ratio^-1))"), "false");
    assert_eq!(ev(&mut e, "[is_even(4), is_odd(7), is_even(7)]"), "[true, true, false]");
    assert_eq!(ev(&mut e, "rational_expression_q((x^2 - 2 * x * y + y) / (3 * x + 4 * (y / x)), [x, y], is_numeric)"), "true");
    assert_eq!(ev(&mut e, "rational_expression_q((x^2 - a * x * y + y) / (b * x + 4 * (y / x)), [x, y], is_numeric)"), "false");
    assert_eq!(ev(&mut e, "rational_expression_q(sin(x)^2/(1 + cos(x)), [sin(x), cos(x)], is_numeric)"), "true");
    assert_eq!(ev(&mut e, "rational_expression_q(sin(x)^2/(1 + cos(x)), [sin(x), cos(x)], is_integer)"), "true");
    assert_eq!(ev(&mut e, "rational_expression_q(sin(x)^(1/2), [sin(x), cos(x)], is_numeric)"), "false");
}

#[test]
fn partitions_with_parts_that_are_not_positive_stay() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "integer_partitions(5, all, [1, -1])"), "integer_partitions(5, all, [1, -1], all)");
    assert_eq!(ev(&mut e, "length(integer_partitions(5, all, [1, 2]))"), "3");
}

#[test]
fn several_interval_arguments() {
    let mut e = CoreEngine::new();
    // the tight image of the box: the corners, checked against values inside
    let r = ev(&mut e, "r = pochhammer(interval([1.23, 1.24]), interval([2.34, 2.35])); r");
    assert!(r.starts_with("interval(["), "{r}");
    assert_eq!(ev(&mut e, "[min(r) <= pochhammer(1.235, 2.345) and pochhammer(1.235, 2.345) <= max(r), min(r) <= pochhammer(1.23, 2.34), pochhammer(1.24, 2.35) <= max(r)]"), "[true, true, true]");
    for call in [
        "carlson_rf(interval([1.23, 1.24]), interval([2.34, 2.35]), interval([3.45, 3.46]))",
        "bessel_j(interval([0.5, 0.6]), interval([2.4, 2.5]))",
        "coulomb_f(interval([0.234, 0.235]), interval([0.345, 0.346]), interval([0.456, 0.457]))",
    ] {
        let v = ev(&mut e, &format!("v = {call}; v"));
        assert!(v.starts_with("interval(["), "{call}: {v}");
        let mid = call.replace("interval([", "mean([");
        assert_eq!(ev(&mut e, &format!("min(v) <= {mid} and {mid} <= max(v)")), "true", "{call}: {v}");
    }
}

#[test]
fn harmonic_numbers_with_a_power_argument() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "harmonic_number(2, 3 / 2, s)"), "s + (1/4)*2^(1/2)*s^2");
    // the list form and an inexact order, against the finite sums and the polylogarithm tail
    assert_eq!(ev(&mut e, "abs(harmonic_number([2, 3, 5, 6], 1 / 2, 0.3)[3] - sum(0.3^k/k^(1/2), [k, 1, 5])) < 10^-14"), "true");
    assert_eq!(ev(&mut e, "abs(harmonic_number(27, 3 / 2, 0.5 + im) - sum((0.5 + im)^k/k^(3/2), [k, 1, 27])) < 10^-10"), "true");
    assert_eq!(ev(&mut e, "abs(harmonic_number(3., 3 / 2, 0.5) - sum(0.5^k/k^(3/2), [k, 1, 3])) < 10^-12"), "true");
}

#[test]
fn divisibility_of_exact_constants() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "divisible(2 * pi, pi / 2)"), "true");
    assert_eq!(ev(&mut e, "divisible(sqrt(6), sqrt(2))"), "false");
    // variations: a quotient that is a fraction, an integer quotient of radicals, against the quotient itself
    assert_eq!(ev(&mut e, "divisible(pi, 2 * pi)"), "false");
    assert_eq!(ev(&mut e, "divisible(sqrt(12), sqrt(3))"), "true");
    assert_eq!(ev(&mut e, "simplify(sqrt(12)/sqrt(3))"), "2");
}
