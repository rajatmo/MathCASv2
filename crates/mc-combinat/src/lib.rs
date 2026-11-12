//! MathCAS v2 number sequences and number representations (round 28).
//!
//! Natives of this crate: `bernoulli_b`, `euler_e` and `bell_b` for integer arguments (exact, integer-only
//! recurrences, see [`seq`]). The rest of the functions are written in MathCAS itself (`lib/combinat.mc`, [`SOURCE`]),
//! evaluated by the engine after the prelude; a rule of that source handles the forms (symbolic second argument,
//! lists) that the natives decline.
//!
//! Literature: Brent and Harvey, "Fast computation of Bernoulli, Tangent and Secant numbers" (2011) for the
//! tangent/secant recurrences; Bell's triangle (Aitken's array); the DLMF chapters 24 (Bernoulli and Euler
//! polynomials), 26 (partitions and Bell numbers) and 34 (3j, 6j symbols); Varshalovich, Moskalev, Khersonskii,
//! "Quantum theory of angular momentum" for the Racah formulas.
//!
//! Decisions are numbered a28n.1, a28n.2, ... in `NOTES.md`.
#![warn(missing_docs)]
pub mod seq;

use mc_core::Heap;
use mc_eval::Kernel;

/// The library's source text.
pub const SOURCE: &str = include_str!("../lib/combinat.mc");

/// The name of the source in messages.
pub const ORIGIN: &str = "lib/combinat.mc";

/// The public functions of this crate (natives and source) with a one-line description each.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("bell_b", "bell_b(n), bell_b(n, x): the Bell number (Bell polynomial) of integer n >= 0"),
    ("bernoulli_b", "bernoulli_b(n), bernoulli_b(n, x): the Bernoulli number (polynomial); B_1 = -1/2"),
    ("euler_e", "euler_e(n), euler_e(n, x): the Euler number (polynomial)"),
    ("polygonal_number", "polygonal_number(k), polygonal_number(r, k): the k-th triangular (r-gonal) number"),
    ("thue_morse", "thue_morse(k): the parity of the number of 1 bits of k"),
    ("golden_angle", "golden_angle: pi (3 - sqrt(5)), the golden angle in radians"),
    ("bit_length", "bit_length(k): the number of bits of k (of -k - 1 for negative k)"),
    ("bit_shift_right", "bit_shift_right(k, s): k shifted right by s bits, floor(k / 2^s)"),
    ("bit_flip", "bit_flip(k, i): k with bit i flipped"),
    ("roman_numeral", "roman_numeral(k): the Roman numeral of k as a string"),
    ("from_roman_numeral", "from_roman_numeral(s): the integer of a Roman numeral"),
    ("norlund_b", "norlund_b(m, a), norlund_b(m, a, x): the Norlund (generalised Bernoulli) number (polynomial)"),
    ("chinese_remainder", "chinese_remainder(rs, ms), (rs, ms, d): the least x with x = r_i mod m_i"),
    ("farey_sequence", "farey_sequence(k), farey_sequence(k, j): the Farey sequence of order k (its j-th term)"),
    ("convergents", "convergents(x), convergents(x, k), convergents(terms): the convergents of a continued fraction"),
    ("mantissa_exponent", "mantissa_exponent(x), (x, b): [m, e] with x = m b^e and 1/b <= |m| < 1"),
    ("number_digit", "number_digit(x, k), (x, k, b): the digit of x at position k in base b"),
    ("number_expand", "number_expand(x), (x, b), (x, b, len): the digits of x times their place values"),
    ("number_compose", "number_compose(digits, bases): the sum of digit times base"),
    ("de_bruijn_sequence", "de_bruijn_sequence(alphabet, m): the lexicographically least de Bruijn sequence"),
    ("mersenne_prime_exponent", "mersenne_prime_exponent(k): the k-th known Mersenne prime exponent"),
    ("mersenne_prime_exponent_q", "mersenne_prime_exponent_q(p): whether p is the exponent of a known Mersenne prime"),
    ("finite_abelian_group_count", "finite_abelian_group_count(k): the number of abelian groups of order k"),
    ("finite_group_count", "finite_group_count(k): the number of groups of order k (k <= 64 or squarefree)"),
    ("frobenius_solve", "frobenius_solve(coins, k), (coins, k, m): the non-negative integer solutions of coins.x = k"),
    ("frobenius_number", "frobenius_number(coins): the largest integer that is not a combination of the coins"),
    ("squares_r", "squares_r(d, k): the number of representations of k as a sum of d squares"),
    ("powers_representations", "powers_representations(m, k, p): the representations of m as a sum of k p-th powers"),
    ("continued_fraction_k", "continued_fraction_k(f, g, [i, imin, imax]): the finite continued fraction K of f(i)/g(i)"),
    ("clebsch_gordan", "clebsch_gordan([j1, m1], [j2, m2], [j, m]): the Clebsch-Gordan coefficient"),
    ("three_j_symbol", "three_j_symbol([j1, m1], [j2, m2], [j3, m3]): Wigner's 3j symbol"),
    ("six_j_symbol", "six_j_symbol([j1, j2, j3], [j4, j5, j6]): Wigner's 6j symbol"),
    ("wigner_d", "wigner_d([j, m1, m2], theta), (.., theta, phi), (.., psi, theta, phi): Wigner's d and D functions"),
    ("apply_sides", "apply_sides(f, r): the relation r with f applied to both sides"),
    ("numerator_denominator", "numerator_denominator(e): [numerator(e), denominator(e)]"),
    ("rational_expression_q", "rational_expression_q(e, x): whether e is a quotient of polynomials in x"),
    ("expand_numerator", "expand_numerator(e): the numerators of all fractions in e expanded"),
    ("expand_denominator", "expand_denominator(e): the denominators of all fractions in e expanded"),
    ("expand_all", "expand_all(e): e expanded everywhere, inside function arguments and denominators too"),
    ("factor_square_free_list", "factor_square_free_list(p): the square-free factors of p with their multiplicities"),
    ("factor_terms_list", "factor_terms_list(p), (p, x): p as the list of its content and primitive part"),
    ("polynomial_quotient_remainder", "polynomial_quotient_remainder(p, q, x): [quotient, remainder] of p by q"),
    ("polynomial_extended_gcd", "polynomial_extended_gcd(p, q, x): [g, [s, t]] with g = s p + t q the monic gcd"),
    ("horner_form", "horner_form(p), horner_form(p, x): the nested (Horner) form of a polynomial"),
    ("coefficient_rules", "coefficient_rules(p, vars): the rules {exponents} -> coefficient of the monomials of p"),
    ("monomial_list", "monomial_list(p, vars): the terms of p in decreasing lexicographic order"),
    ("symmetric_polynomial", "symmetric_polynomial(k, xs): the elementary symmetric polynomial of degree k"),
    ("residue", "residue(f, [x, a]): the residue of f at x = a"),
    ("riemann_r", "riemann_r(x): Riemann's prime counting function R(x), for inexact x"),
    ("dirichlet_l", "dirichlet_l(k, j, s): the Dirichlet L-function of the character with modulus k and Conrey label j"),
];

/// Register the natives with `k`.
pub fn install(k: &mut Kernel, h: &Heap) {
    seq::install(k, h);
}
