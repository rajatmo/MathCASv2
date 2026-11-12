//! The benchmark cases of this crate (bench/cases/N, R) through source text, at the sizes that do not need the
//! language features of item H (blocks, lambdas, `sum`). The largest sizes are ignored in debug builds; the
//! timings of the release runs are in the crate notes.
mod common;
use common::Nt;
use num_bigint::BigUint;

/// N5: factorial(m) with its property (Legendre's formula for 2 and 5, and the digit count), the digit sums taken
/// here as `sum` is not available.
fn n5(m: u64, digits: u64) {
    let mut nt = Nt::new();
    nt.eval(&format!("r = factorial({m})"));
    let sum = |base: u32| BigUint::from(m).to_radix_be(base).iter().map(|&d| d as u64).sum::<u64>();
    assert_eq!(nt.eval("integer_exponent(r, 2)"), (m - sum(2)).to_string());
    assert_eq!(nt.eval("integer_exponent(r, 5)"), ((m - sum(5)) / 4).to_string());
    assert_eq!(nt.eval("integer_length(r)"), digits.to_string());
    assert_eq!(nt.eval(&format!("integer_digits({m}, 2)")).matches(", ").count() as u32 + 1, 64 - m.leading_zeros());
}

#[test]
fn n5_exact_factorial() {
    n5(10_000, 35660);
}

#[test]
#[cfg_attr(debug_assertions, ignore = "large: run in release")]
fn n5_exact_factorial_large() {
    n5(100_000, 456574);
    n5(1_000_000, 5565709);
}

#[test]
fn n6_prime_counting() {
    let mut nt = Nt::new();
    assert_eq!(nt.eval("[prime_pi(10^8), prime(10^8)]"), "[5761455, 2038074743]");
}

#[test]
#[cfg_attr(debug_assertions, ignore = "large: run in release")]
fn n6_prime_counting_large() {
    let mut nt = Nt::new();
    assert_eq!(nt.eval("[prime_pi(10^10), prime(10^10)]"), "[455052511, 252097800623]");
    assert_eq!(nt.eval("[prime_pi(10^12), prime(10^12)]"), "[37607912018, 29996224275833]");
}

/// N7: the setup and the property, with `floor` and `next_prime` from this crate.
fn n7(d: u32) {
    let mut nt = Nt::new();
    nt.eval(&format!("d = {d}\nh = floor(d/2)\np = next_prime(2*10^(h - 1))\nq = next_prime(5*10^(d - h - 1))"));
    assert_eq!(nt.eval("factor_integer(p*q) == [[p, 1], [q, 1]]"), "true", "{d} digits");
}

#[test]
fn n7_semiprimes() {
    n7(30);
}

#[test]
#[cfg_attr(debug_assertions, ignore = "large: run in release")]
fn n7_semiprimes_large() {
    n7(45);
    n7(60);
}

/// N9 with the block written as statements.
fn n9(d: u32) {
    let mut nt = Nt::new();
    nt.eval(&format!("a = 7^{d}\nb = 11^{d} + 1\nc = a*b"));
    assert_eq!(nt.eval("[quotient(c + 5, b), gcd(c, a^2)] == [a, a]"), "true");
}

#[test]
fn n9_quotient_and_gcd() {
    n9(10_000);
}

#[test]
#[cfg_attr(debug_assertions, ignore = "large: run in release")]
fn n9_quotient_and_gcd_large() {
    n9(100_000);
}

/// N10's property on a harmonic number built here (`range`, `map` with lambdas and `sum` are not available).
#[test]
fn n10_property() {
    let mut nt = Nt::new();
    let m = 1000;
    let terms: Vec<String> = (1..=m).map(|k| format!("1/{k}")).collect();
    nt.eval(&format!("result = {}", terms.join(" + ")));
    let src = format!(
        "integer_exponent(denominator(result), 2) == integer_length({m}, 2) - 1 and mod(numerator(result), 2) == 1"
    );
    assert_eq!(nt.eval(&src), "true");
}

/// The v1 failure case: the primitive root of a prime power with millions of digits.
#[test]
#[cfg_attr(debug_assertions, ignore = "large: run in release")]
fn primitive_root_of_a_huge_prime_power() {
    let mut nt = Nt::new();
    let small = nt.eval("primitive_root(prime(10^6)^2)");
    assert_eq!(nt.eval("primitive_root(prime(10^6)^(10^6))"), small);
}

#[test]
fn primitive_root_of_a_large_prime_power() {
    let mut nt = Nt::new();
    let small = nt.eval("primitive_root(prime(10^6)^2)");
    assert_eq!(nt.eval("primitive_root(prime(10^6)^(10^4))"), small);
    // modulo 2 p^k the root is g or g + p^k for the root g modulo p^k
    assert_eq!(
        nt.eval("mod(primitive_root(2*prime(10^6)^(10^4)), prime(10^6)^(10^4))"),
        nt.eval("mod(primitive_root(2*prime(10^6)^2), prime(10^6)^2)")
    );
}

/// R12's loop body: `mod` on machine integers.
#[test]
fn r12_collatz_steps() {
    let mut nt = Nt::new();
    nt.eval("fn steps(1) = 0");
    nt.eval("fn steps(c: Int) = 1 + steps(if mod(c, 2) == 0 then quotient(c, 2) else 3c + 1)");
    assert_eq!(nt.eval("steps(837799)"), "524");
}
