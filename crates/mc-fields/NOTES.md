# mc-fields: finite fields (round 25, roadmap 15B)

## What it is

A new crate: the finite fields F_(p^n) for primes p < 2^62, their elements with exact arithmetic, embeddings,
norms, traces, minimal polynomials, primitive elements, discrete logarithms, roots and polynomials over them.

## Terms (decisions n25f.1-n25f.4)

- **n25f.1 A field is a term that reads back to itself**: `finite_field(17, &(14 + &1 + &1^3), "Polynomial")`: the
  characteristic, the defining polynomial as a pure function of one argument, the element representation
  (`"Polynomial"` or `"Exponential"`). The native rebuilds the canonical form from the parsed polynomial (terms by
  increasing degree, unit coefficients left out), so any spelling of the same field evaluates to one term, and the
  printed field evaluates to itself. The defining polynomial is kept as given modulo p, also when it is not monic
  (`finite_field(509, &(2*&1^4 + ...), "Polynomial")`); arithmetic uses its monic multiple.
- **n25f.2 Elements**: `finite_field_element(F, [c0, c1, ...])` with the coefficients of the residue (no trailing
  zeros; zero is `[]`), and `finite_field_element(F, k)` in an exponential field (0 is zero, k >= 1 is x^(k - 1)).
  `F(k)` and `finite_field_element(F, k)` read k as the index (the base-p digits are the coefficients), `F([..])` as
  coefficients (exact rationals with denominators prime to p allowed; longer lists are reduced modulo f).
- **n25f.3 Properties by call**: `F("FieldSize")`, `a("Index")`, `a(["Field", "Coefficients"])`: Characteristic,
  ExtensionDegree, FieldSize, FieldIrreducible, ElementRepresentation; elements also Field, Index, Coefficients,
  Trace, Norm. The calls are curried natives of `finite_field` and `finite_field_element`.
- **n25f.4 Mixing**: elements of one field combine with each other and with exact numbers whose denominator is prime
  to p; elements of different fields stay apart (`a + b` stays a sum). `0 + x` drops the zero element, `0 x` is
  the zero element, `1 x` keeps the unit (polynomials over F_q show every coefficient as an element). Fractional
  powers stay unevaluated (`sqrt(a)`); `finite_field_root(a, n)` lists the roots.

## Wrapped natives (decision n25f.5)

`add`, `mul`, `pow`, `equal`, `unequal`, `element`, `minimal_polynomial`, `factor`, `factor_square_free`, `expand`,
`polynomial_gcd`, `polynomial_lcm`, `polynomial_quotient`, `polynomial_remainder` are wrapped with
`Kernel::wrap_native` at install time (the install line comes after every library that defines them): a call with no
element among its arguments (the arithmetic and comparisons look at depth one only; the polynomial functions scan
the arguments and the `extension:` option) goes to the wrapped native unchanged. The only cost elsewhere is the scan
of the argument heads.

## Methods and literature

- F_p[x]/(f), inverses by extended Euclid, Rabin's irreducibility test (von zur Gathen and Gerhard, Modern Computer
  Algebra, 3rd ed., sections 4.2 and 14.9); traces from Newton's identities on f, norms as a^((q-1)/(p-1)), minimal
  polynomials as products over the Frobenius conjugates (Lidl and Niederreiter, Finite Fields, 2nd ed., 2.2-2.3).
- Orders and primitivity from the factorisation of q - 1 (`mc-numtheory`'s factorisation, capped at 2e6 work units;
  when it does not finish the functions that need it decline).
- Discrete logarithms: Pohlig and Hellman over the prime powers of the order, each digit by baby-step giant-step
  (Lidl and Niederreiter 9.4); prime factors up to 2^46.
- Factorisation over F_q: Yun's square-free decomposition adapted to characteristic p (p-th roots c^(q/p)),
  distinct-degree factorisation and Cantor-Zassenhaus equal-degree splitting, the trace map in characteristic 2
  (von zur Gathen and Gerhard, 14.3-14.8, exercise 14.16). Over the prime field `mc-poly`'s `factor_mod_p` does it.
- Conway polynomials (definition: Lidl and Niederreiter notes to ch. 2; Heath and Loehr, J. Symbolic Comput. 38,
  2004): for prime n (and n = 1, x minus the least primitive root) a search in Conway order with the constant term
  fixed by the norm condition; for composite n Heath and Loehr's coset enumeration in an auxiliary model of
  F_(p^n): with a primitive g, a = g^k is compatible with C(p, d) for the maximal divisors d exactly when k lies in
  a class modulo lcm(p^d - 1) fixed by the discrete logarithms of one root of each C(p, d) (one root of the first
  fixed by the Galois symmetry); the candidates of the classes are run through with one multiplication each, the
  primitive ones with the least trace kept, then the later coefficients by Newton's identities while k < p, the
  least characteristic polynomial winning. Run time is (p^n - 1)/lcm(p^d - 1) per class.
- Embeddings: Conway fields embed canonically by x -> x^((p^n-1)/(p^m-1)); others by the root of the defining
  polynomial with the least index; `a -> b` by solving x = h(a) over F_p. The projection is the F_p-linear left
  inverse reading the first independent rows (a left inverse on the image, linear elsewhere).

## Fallbacks

`finite_field(p, n)` takes C(p, n) when it is tabulated or computes within 3e6 enumeration steps, otherwise the
least primitive polynomial in Conway order (no compatibility), and when p^n - 1 does not factor within the cap the
least irreducible one. `conway_polynomial` gives a failure in those cases.

## Conway table and costs

`conway_table.rs` holds 87 polynomials (primes p < 110, composite n, p^n < 2^64, not found within 50 ms at run
time), written by `conway::tests::generate_table` (190 s, `MC_FIELDS_STEPS` 3e8 candidates per entry, status file
`fields-conway-table.json`). For composite n the computation picks the cheaper of the compatible search (expected
tries n p^(n-1) / (|S|/2)) and the coset enumeration (|S| = (p^n - 1)/L times the classes); the coset loop
multiplies dense coordinate vectors in machine words (p < 2^20, one reduction per coefficient), about 50 ns a step
at n = 8. 53 entries in that range stay out of reach (for example C(2, 32), C(3, 30), C(23, 14)); those fields take
the fallback polynomial. Checks: C(17, 3), C(17, 6), C(29, 4), C(29, 12), C(97, 4), C(97, 8), C(7, 9), C(71, 5),
C(1009, 3) and the small ones in `conway::tests` agree with Luebeck's published tables.

## Design choices (evidence)

- Embeddings between Conway fields are the compatible ones, x -> x^((p^n-1)/(p^m-1)), so embedding through an
  intermediate field gives the same map (test `conway_embeddings_commute`). Choosing, say, the root of C(29, 4) in
  F_(29^12) whose linear factor sorts first would give embeddings that need not compose.
- Fields and elements print in the language's own syntax and read back to themselves (n25f.1).
- Two spellings of one field evaluate to one term, so `===` finds them equal; elements are terms, not atoms
  (`is_atom` of an element is false).

## Gaps

- Characteristic p >= 2^62 (`finite_field(next_prime(2^100), 3)` stays unevaluated).
- Discrete logarithms need the prime factors of the group order below 2^46 (`finite_field(2, 61)` has none).
- Polynomials over F_q: one variable only (factor, gcd, quotient, remainder, expand, cancel/together); matrices over
  F_q go through `det`, `dot`, `inverse`, `total` with the result evaluated again, the other linear algebra
  functions see elements as symbols.
- `factor_list`, `factor_square_free_list`, `polynomial_quotient_remainder`, `polynomial_extended_gcd` do not exist
  in the language yet.
- A pattern with a head test on an element (`a_finite_field_element`) after `a` has a value meets the evaluator's
  "pattern variable that is not a symbol".

## Round 27

- B26-10: `finite_field(q)` declines for a prime power with exponent above 4096 (the cap of `finite_field(p, n)`); no
  Conway search of degree 100000 (it ignored the stop flag).
- B26-9: `finite_field_primitive_element` starts at the polynomial index p (the element x) for n >= 2: the constants
  of the prime field (indices below p) have order dividing p - 1 and used the whole 10^6-step cap when p > 10^6.
- X26-14 and stop flags: every candidate of the Conway search is charged (it was every 64th), the compatibility and
  primitivity tests (`is_primitive_poly`, now taking the budget) charge before each power of x, the trace-map loop of the
  equal-degree split charges per step, and a polynomial found by the search skips the uninterruptible irreducibility
  re-test (`checked_field_with`). `time_constrained(..., 2)` of the GF(2^60)-to-GF(2^120) embedding returns after 2.06 s
  (was 22 s), of GF(5^300) after 2.03 s (was 4.9 s).
- D26-06, D26-21: element data that is a negative integer or a rational is a number of the prime field.
- D26-07: `p == q` for one-variable polynomials with field elements inside compares them as polynomials over the field.
- D26-08: `factor(p, modulus: q)` for a prime power q is `factor(p, extension: F_q)`.
- D26-09: `multiplicative_order(a)` of a nonzero element.
- D26-21: `finite_field_embedding(a -> b)` with elements whose image is no root of the minimal polynomial gives a failure;
  `is_irreducible_polynomial(f, x, modulus: p)` takes the variable.
- Deferred: B26-11 (`finite_field(7, 2000)`, `finite_field(2^61 - 1, 100)` need fast polynomial arithmetic for the
  irreducibility search at that degree; both honour the time limit now).

## Round 33

No change to the crate. `tests/round33.rs` holds two ignored tests of number-field examples whose fix lies in other
crates (`number_field_discriminant` of an `algebraic_number` term: mc-cover30; `to_number_field([sqrt(2), im], all)`:
mc-algebra), kept as failing tests until those fixes land.

## Round 37

- `is_irreducible_polynomial` threads over a list of polynomials (the options apply to each). Over the rationals
  it accepts complex rational and algebraic-constant coefficients (`2^(1/2)`, `im`) and answers what `factor` (with
  the same `extension:`) gives: one non-constant factor of multiplicity 1. When `factor` gives no factorisation (a
  failure, as for several variables over an extension) the call stays unevaluated instead of reading the failure
  as one factor.
- Irreducibility without factorisation: f = a y + b of degree 1 in a variable y is irreducible over every field
  when gcd(a, b) is a nonzero constant (the gcd is the same over every extension), so `extension: all` and several
  variables are answered for such polynomials (x^7 + 12 x y - 11).
