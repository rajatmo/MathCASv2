//! MathCAS v2 number theory (package J of the library round): integer and rational functions, rounding, primes,
//! prime counting, factorisation and modular arithmetic, as natives of the evaluator.
//!
//! The algorithms are plain functions on `mc-arith`'s bare numbers ([`Nat`](mc_arith::num::Nat),
//! [`Int`](mc_arith::num::Int)), each taking a [`budget::Budget`] that charges fuel and polls the interrupt:
//!
//! - [`word`]: machine-word Montgomery arithmetic, deterministic primality below 2^64, roots, Jacobi symbols;
//! - [`sieve`]: segmented sieves and the table of pi(z); [`count`]: Meissel's prime counting and the k-th prime;
//! - [`ring`]: arithmetic modulo big odd numbers (Montgomery on fixed limbs, Newton reciprocals above);
//! - [`prime`]: Baillie-PSW and the next prime; [`gcd`]: the subquadratic half-gcd;
//! - [`power`]: exact roots, valuations and perfect-power detection; [`factor`]: trial division, rho, ECM, SIQS;
//! - [`modular`]: modular powers and inverses, primitive roots, orders, phi, divisors;
//! - [`combin`]: factorials and binomials; [`digits`]: digits, lengths and exponents in a base.
//!
//! [`install`] registers the natives listed in [`FUNCTIONS`]. Decisions of this package are numbered J.1, J.2, ...
//! in the comments where they are applied.
#![warn(missing_docs)]
pub mod budget;
pub mod combin;
pub mod count;
pub mod cover;
pub mod digits;
pub mod factor;
pub mod gcd;
pub mod modular;
mod natives;
pub mod power;
pub mod prime;
pub mod ring;
pub mod sieve;
pub mod word;

use mc_core::{Heap, T};
use mc_eval::{Kernel, NativeCx, Reply, Res};

/// The functions [`install`] registers, with a one-line description each.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("mod", "mod(a, m) and mod(a, m, d): the remainder a - m floor((a - d)/m), with the sign of m"),
    ("quotient", "quotient(a, b): floor(a / b)"),
    ("extended_gcd", "extended_gcd(a, b): [g, [s, t]] with g = gcd(a, b) = s a + t b"),
    ("floor", "floor(x), floor(x, a): the greatest integer (multiple of a) at most x"),
    ("ceiling", "ceiling(x), ceiling(x, a): the least integer (multiple of a) at least x"),
    ("round", "round(x), round(x, a): the nearest integer (multiple of a), ties to even"),
    ("abs", "abs(x): absolute value of a real or complex number"),
    ("sign", "sign(x): -1, 0 or 1 for reals, z / abs(z) for complex numbers"),
    ("numerator", "numerator(x): of a rational, or the factors of a product with positive exponents"),
    ("denominator", "denominator(x): of a rational, or the factors of a product with negative exponents"),
    ("factorial", "factorial(n): n! for integers n >= 0"),
    ("binomial", "binomial(n, k): the binomial coefficient, for integers and rational n"),
    ("integer_digits", "integer_digits(n), (n, b), (n, b, len): digits of |n| in base b (default 10)"),
    ("from_digits", "from_digits(digits), (digits, b): the integer with these digits in base b"),
    ("integer_length", "integer_length(n), (n, b): the number of digits of |n| in base b"),
    ("integer_exponent", "integer_exponent(n), (n, b): the largest k with b^k dividing n"),
    ("is_prime", "is_prime(n): primality (deterministic below 2^64, Baillie-PSW above)"),
    ("next_prime", "next_prime(x), (x, k): the next (k-th next, k-th previous for k < 0) prime"),
    ("prime", "prime(k): the k-th prime"),
    ("prime_pi", "prime_pi(x): the number of primes up to x"),
    ("factor_integer", "factor_integer(n): [[p, e], ...] for integers and rationals"),
    ("power_mod", "power_mod(a, b, m): a^b mod m, b < 0 by the inverse of a"),
    ("primitive_root", "primitive_root(n): the smallest primitive root modulo n"),
    ("euler_phi", "euler_phi(n): Euler's totient"),
    ("divisors", "divisors(n): the positive divisors of n in increasing order"),
    ("divisor_sigma", "divisor_sigma(k, n): the sum of the k-th powers of the divisors of n"),
    ("moebius_mu", "moebius_mu(n): the Moebius function"),
    ("jacobi_symbol", "jacobi_symbol(a, n): the Jacobi symbol for odd n > 0"),
    ("multiplicative_order", "multiplicative_order(a, n): the order of a modulo n"),
    ("divisible", "divisible(n, m): true when n/m is an integer (Gaussian integers included)"),
    ("is_coprime", "is_coprime(a, b, ...): true when the integers are pairwise coprime"),
    ("is_square_free", "is_square_free(n): true when no square of a prime divides n"),
    ("is_prime_power", "is_prime_power(n): true when |n| is a power p^k of a prime, k >= 1"),
    ("is_composite", "is_composite(n): true for integers n > 1 that are not prime"),
    ("prime_nu", "prime_nu(n): the number of distinct primes dividing n"),
    ("prime_omega", "prime_omega(n): the number of prime factors of n counted with multiplicity"),
    ("liouville_lambda", "liouville_lambda(n): (-1)^prime_omega(n)"),
    ("mangoldt_lambda", "mangoldt_lambda(n): log p when n is a power of the prime p, else 0"),
    ("carmichael_lambda", "carmichael_lambda(n): the exponent of the multiplicative group modulo n"),
    ("divisor_sum", "divisor_sum(n, f): the sum of f(d) over the divisors d of n; divisor_sum(n, f, cond) over those with cond(d) true"),
    ("kronecker_symbol", "kronecker_symbol(a, n): the Kronecker symbol, the Jacobi symbol extended to all integers n"),
    ("modular_inverse", "modular_inverse(k, n): the r with k r = 1 mod n"),
    ("quotient_remainder", "quotient_remainder(m, n): [quotient(m, n), mod(m, n)]"),
    ("catalan_number", "catalan_number(n): the Catalan number binomial(2 n, n)/(n + 1)"),
    ("stirling_s1", "stirling_s1(n, k): the signed Stirling number of the first kind"),
    ("stirling_s2", "stirling_s2(n, k): the Stirling number of the second kind"),
    ("partitions_p", "partitions_p(n): the number of partitions of n"),
    ("partitions_q", "partitions_q(n): the number of partitions of n into distinct parts"),
    ("ramanujan_tau", "ramanujan_tau(n): the Ramanujan tau function, the coefficients of q prod (1 - q^k)^24"),
    ("bit_and", "bit_and(n1, n2, ...): the bitwise and of integers (two's complement)"),
    ("bit_or", "bit_or(n1, n2, ...): the bitwise or of integers"),
    ("bit_xor", "bit_xor(n1, n2, ...): the bitwise exclusive or of integers"),
    ("bit_not", "bit_not(n): the bitwise complement -1 - n"),
    ("integer_reverse", "integer_reverse(n, b): the integer with the base-b digits of n reversed (b = 10 by default)"),
    ("mixed_radix", "mixed_radix([b1, ..., bk]): a mixed-radix base for integer_digits and from_digits"),
    ("real_digits", "real_digits(x), real_digits(x, b), real_digits(x, b, len): [digits, e] with |x| = 0.d1 d2 ... b^e; a rational's repeating digits form a final sublist"),
    ("continued_fraction", "continued_fraction(x), continued_fraction(x, n): the continued fraction terms of x (a quadratic irrational's period as a final sublist)"),
    ("from_continued_fraction", "from_continued_fraction(terms): the number with these continued fraction terms (a final sublist repeats)"),
];

/// Register the natives of [`FUNCTIONS`] with `k` (strict arguments; the arithmetic ones thread over lists).
///
/// The heap names the symbols (decision J.1: a kernel alone cannot make a symbol, so the signature is that of
/// `mc-engine`'s `arith::install`).
pub fn install(k: &mut Kernel, h: &Heap) {
    natives::install(k, h);
    mc_numeric::formula::install(k, h, cover::DEFS);
}

/// `gcd(a, ...)` (or `lcm(a, ...)` when `lcm`) of exact numbers by the half-gcd: gcd(p/q, r/s) = gcd(p, r) / lcm(q, s),
/// lcm(p/q, r/s) = lcm(p, r) / gcd(q, s), gcd() = 0, lcm() = 1. Declines unless every argument is an integer or a
/// rational.
///
/// This package does not register `gcd` and `lcm`: `mc-poly` owns both names (they also apply to polynomials) and
/// sends exact numbers here, so integer gcds get the subquadratic algorithm without two natives competing for one
/// name (decision 9.1 of the library-gaps package). Call it inside `mc-arith`'s stop scope so that single big
/// products stop too.
pub fn number_gcd_lcm<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], lcm: bool) -> Res<Reply<'h>> {
    natives::gcd_lcm(cx, args, !lcm)
}

/// The thread pool of the unit tests: 4 threads (the shared-machine rule), where the parallel sieves and prime
/// counting run.
#[cfg(test)]
pub(crate) fn test_pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| rayon::ThreadPoolBuilder::new().num_threads(4).build().expect("a pool"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_table_lists_every_native() {
        let mut names: Vec<&str> = super::natives::TABLE.iter().map(|e| e.0).collect();
        names.extend(super::cover::DEFS.iter().map(|d| d.name));
        let listed: Vec<&str> = super::FUNCTIONS.iter().map(|e| e.0).collect();
        assert_eq!(names, listed);
    }
}
