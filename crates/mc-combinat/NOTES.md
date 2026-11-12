# mc-combinat notes (round 28)

New crate: number sequences, representations of integers, angular momentum symbols and a set of polynomial/expression
helpers. Natives in Rust (`src/seq.rs`): `bernoulli_b`, `euler_e`, `bell_b` for integer index. Everything else is
written in the language (`lib/combinat.mc`, evaluated by the engine after the prelude and the language library);
rules there handle the forms the natives decline (symbolic second argument: Bernoulli, Euler and Bell polynomials).

## Decisions

- a28n.1 Bernoulli and Euler numbers: tangent and secant number recurrences (Brent and Harvey, "Fast computation of
  Bernoulli, Tangent and Secant numbers", 2011), integer multiplications by words only, O(n^2) steps for B_2n.
  Index limit 40000 (larger calls stay unevaluated). Bell: Bell's triangle (Aitken's array), O(n^2) additions.
- a28n.2 `rule` definitions with `when` everywhere (an unmatched call stays unevaluated). Pitfall found: in
  `rule f(..) := if c then a else b when g` the `when` binds into the else branch; wrap the body in braces.
- a28n.3 Symbolic conditions use `===` (`cs[i] !== 0`), never `!=`, since `x != 0` is not boolean for symbols.
- a28n.4 `powers_representations` for two squares of an integer above 10^7 uses the factorisation over the Gaussian
  integers (Hermite-Serret for p = a^2 + b^2), so the large-number cases are exact and fast.
- a28n.5 `squares_r`: Jacobi (d = 2), Eisenstein/Liouville (d = 4, 6, 8) divisor formulas; other d by convolution
  (k <= 3000). `squares_r(0, 0) = 1` (the empty sum).
- a28n.6 Angular momentum: Racah's single-sum formulas (Varshalovich et al.; DLMF 34.2, 34.3) with exact rational
  square roots. `wigner_d([j, m1, m2], ...)` equals exp(i m1 psi + i m2 phi) d^j_{m2, m1}(theta) with the standard
  (Sakurai) small d.
- a28n.7 `riemann_r`: Gram's series with zeta(k + 1) at the precision of the argument plus 8 digits.
  `dirichlet_l`: Conrey characters built from the smallest primitive root of each odd prime power and the generators
  -1, 5 of 2^e; values through the Hurwitz zeta function `zeta(s, a)`.
- a28n.8 `finite_group_count`: table for k <= 64 (OEIS A000001) and Hoelder's formula for squarefree k (verified for
  k = 6, 30, 2310 = 2*3*5*7*11: 36).
- a28n.9 `mersenne_prime_exponent`: the 52 known exponents (as of 2024); `mersenne_prime_exponent_q` tests membership.

## Gaps

- Not done: champernowne_number, stieltjes_gamma, partial_fraction_elements, apart_square_free, trig_factor,
  symmetric_reduction, coefficient_arrays, non_negative_rationals, non_positive_rationals (domains are registered in
  mc-decide / mc-simplify).
- `dirichlet_l` and `riemann_r` need precision beyond machine only as far as `zeta(s, a)` supplies it: the Hurwitz zeta
  function of two arguments is machine precision only in mc-special, so `dirichlet_l` above machine precision stays
  unevaluated.
- `number_expand` of machine reals is exact on the rational value, not on the printed decimal digits.
- `finite_group_count` above 64 only for squarefree orders; `continued_fraction_k` only finite upper limits.

## Round 30

- `polynomial_extended_gcd(x, (x - a)^2, x)` fixed through `polynomial_quotient` with a symbolic leading coefficient
  (mc-poly); the result `[1, [(-x + 2 a)/a^2, 1/a^2]]` is checked by expansion in the test.
- `horner_form`: a polynomial whose lowest coefficient (in the current variable) is zero lost the x power of the
  nested tail (`horner_form(x y + 2 x y^2, [x, y])` gave `3 x y`); the tail is now `x^(first - 1) (c + tail)`. The
  documented example gives the expected nested form.

## Round 32

- `polynomial_extended_gcd` keeps the Euclidean remainders and cofactors in lowest terms and expanded: with factored remainders the quotient step returned 0 and the loop ended early (a wrong gcd) or never ended; a step limit guards a stall.

## Round 33

- `euler_e(n)` from index 600 by Euler's formula `|E_2m| = 4^(m+1) (2m)! beta(2m+1)/pi^(2m+1)` (Dirichlet beta) in fixed point (Machin's pi, the beta series cut when `(2k+1)^(n+1)` exceeds `2^q`, one division, rounding): 10000 in 0.9 s (the secant recurrence: 22 s). Checked by `sum_j C(n, 2j) E_2j = 0` across the switch.
- `bell_b(m, x)` for a number x and m >= 40 builds the row of Stirling numbers by the recurrence on whole lists (1000 in 1.7 s); an inexact x is rationalized and the exact value rounded to its precision (the powers leave the machine range). `powers_representations(m, 2, 3)` by the divisors of m (`a^3 + b^3 = s (s^2 - 3ab)`, s = a + b between the cube roots of m and 4m).
- `continued_fraction_k(f, g, [i, 1, inf])` with constant numbers: the root of `K^2 + g K - f = 0` of smaller modulus (equal moduli: `failure(divergent)`), others `failure(infinite_continued_fraction)`.

## Round 36

- `number_expand` rewritten: terms are the digits times their place values over the places of the leading digit,
  the last term holds the rest; inexact numbers are rounded to their digits first (16 for a machine real), complex
  numbers expand both parts over common places; terminating exact numbers expand fully without `len`.
- `number_digit` of an inexact number below its last significant digit is `indeterminate`.
- `polygonal_number` of a regular polygon (its number of sides) and of two lists of equal length (pairwise).

## Round 36, later fixes

- `convergents` of a periodic list ends with the exact value; of a machine real; `factor_terms_list` with modulus:, trig:, Gaussian content and successive variables; `factor_square_free_list` monic over Q(i) and with a field constant over a finite field; `residue` at an algebraic point reduces the constant parts of the arguments first.

## Round 37

- `modulus: m` for `expand_numerator`, `expand_denominator`, `expand_all`, `numerator`, `denominator`,
  `numerator_denominator`, `coefficient_rules`, `monomial_list` (m > 1, prime or not: expansion over Z with the
  coefficients reduced, a ring map; a rational number needs a denominator prime to m, otherwise no answer; a
  fraction is made to have a denominator leading with 1, lex in the canonical variable order, when its leading
  coefficient is a unit), and for `polynomial_quotient_remainder`, `polynomial_extended_gcd` (Euclid in F_p[x], one
  variable), `factor_list` and `factor_square_free_list` (factor over F_p; factor_list sorted by degree).
- `expand(e, patt)`: the maximal subterms of the sum/product/power tree that are free of patt become opaque kernels
  `ex_leaf(i)` for the expansion and are put back after.
- `gaussian_integers: true` for `factor_integer`, `divisors`, `divisor_sigma`, `prime_nu`, `prime_omega`,
  `liouville_lambda`, `moebius_mu`, `is_composite`, `is_prime_power` (and `factor_list`): the factorisation in
  Z[i] from the norm (Hardy and Wright 12.8; Cohen 1.5.1): 1 + i over 2, p inert for p = 3 mod 4, gcd(p, t + i) and
  its conjugate for p = 1 mod 4 with t^2 = -1 mod p; exponents by exact division; the unit left over first (omitted
  when 1). Primes in the first quadrant, ordered by norm then real part. `divisor_sigma` is the product of
  `1 + q^k + ... + q^(e k)` over those primes (so 2 i, not 2, is the divisor (1 + i)^2);
  `divisors` lists the first-quadrant associates.
- `apply_sides(f, e)` through and, or, piecewise and conditional_expression, every side of a chain; true and false
  stay (the side map of mc-calculus's `multiply_sides`).
- Outside the crate (`lib/prelude.mc`): `cellular_automaton` takes neighbourhood rules `[l1, ..., lm] -> v`,
  `[f, [], r]` (f of the neighbourhood and the step), half-integer ranges, symbolic cells (the window compares cells
  as terms, `!==`), periodic rows of any cells, and `[t, [[c]]]` (one column). A rule is `[g, offsets]` inside.
- `divisor_sigma(k, n)` for k not a natural number (negative, rational, symbolic): the product of
  `(p^(k (e + 1)) - 1)/(p^k - 1)` (generic in a symbolic k). Non-real Gaussian
  integers go to the `gaussian_integers: true` rules without the option (`divisor_sigma(1, 3 + i)`,
  `moebius_mu(5 + 6 i)`).
- Mixed radix: `integer_digits(n, mixed_radix(bs))`, `integer_length`, `integer_string` (with a length): the digits
  from the last radix, `inf` unbounded, what is left above the radices one more leading digit. The prelude's
  `cellular_automaton` also takes a function of m cells (`&(xor(&1, &3))`, `boolean_function(n, m)`).
- Outside the crate (`lib/prelude.mc`): `find_sequence_function` tries, after its closed forms, a catalogue of
  the library's integer functions (prime, prime_pi, euler_phi, carmichael_lambda, moebius_mu, liouville_lambda,
  prime_nu, prime_omega, partitions_p/q, bell_b, catalan_number, euler_e, the group counts, divisor_sigma(0..3),
  squares_r(2..4), mod(n, k)): the first whose values at 1..m are all m terms (m >= 6, a period seen twice).

## Round 38: bits, perfect numbers, linear recurrences

- `bit_set`, `bit_clear` through `bit_or`/`bit_and` with 2^i (two's complement for negative k), `bit_shift_left` as
  k 2^s (a negative s shifts right with the floor), all threading over lists.
- `perfect_number_q(k)`: sigma_1(k) = 2 k for positive integers, false otherwise.
- `find_linear_recurrence(xs)`, `(xs, d)`: the Berlekamp-Massey algorithm (Massey, IEEE Trans. Inform. Theory 15,
  1969) over the exact numbers of the list. Decision: a recurrence of order L counts as found only when the list is
  longer than the 2 L terms that determine it (a generic list of 2 L terms has linear complexity L, so 2 L terms
  check nothing); otherwise `failure(no_recurrence)`.
## Round 38: bit indices, fraction options, Dirichlet series

- `bit_flip(k, i)` with k >= 0 and i < 0 counts from the most significant bit (-1 the top bit of the
  `bit_length(k)` bits); an index past the top leaves k. `bit_shift_right(k)` shifts by one bit.
- `together`, `cancel` with `modulus: m`: the fraction over Z/m (numerator and denominator reduced, the denominator's
  leading coefficient made 1 when it is a unit); for a prime m and one variable the gcd over F_m is cancelled first.
  With several variables no gcd is taken (the value is the same, the form may not be in lowest terms).
- `trig: true` for `expand`, `expand_all`, `expand_numerator`, `expand_denominator`, `numerator`, `denominator`,
  `numerator_denominator`, `together`, `cancel`: tan, cot, sec, csc (and the hyperbolic ones) read as quotients of sin
  and cos, multiple angles and sums of arguments expanded (`trig_expand`), and a fraction's numerator and denominator
  put in the normal form of the ideal (cos(u)^2 + sin(u)^2 - 1) (its Groebner basis for cos > sin: cos of degree at most
  1; likewise cosh^2 = 1 + sinh^2; Cox, Little, O'Shea, *Ideals, Varieties, and Algorithms* 2.6). Not done: `factor`,
  `factor_list`, `factor_square_free(_list)` and `apart_square_free` with `trig: true` (they need the sum-to-product
  factorisation), and `extension:` for `together`/`cancel` (`factor` over an extension takes one variable only, and a
  radical in the coefficients is read as a variable).
- Dirichlet series `sum(a(n)/n^s, [n, inf])` of mu, mu^2, Liouville's lambda, von Mangoldt's Lambda, 2^omega, phi and
  sigma_k, `sum(prime(n)^-s)` = `prime_zeta_p(s)`, and the Euler products over the primes of 1 -+ p^-s and their
  reciprocals (Apostol 11.4-11.6; Hardy and Wright 17.1-17.7). A numeric s outside the half plane of absolute
  convergence (re(s) > 1; > 2 for phi; > max(1, 1 + re(k)) for sigma_k) leaves the sum: `sum(phi(k)/k^2)` diverges.
- Gaussian numbers without the option: `factor_integer(z)` and `prime_nu(z)` of a non-real Gaussian integer factor in
  Z[i]; `gcd` and `lcm` of two exact Gaussian rationals (one not real): z = w/d with d the common denominator of its
  parts, gcd = gcd(w1, w2)/lcm(d1, d2) and lcm = lcm(w1, w2)/gcd(d1, d2) as for rationals, the Gaussian gcd by
  Euclid with the rounded quotient, normalised to the first-quadrant associate (Hardy and Wright 12.8). Not done:
  `divisors` of a Gaussian integer without the option (its order would be by norm), more than two arguments.
