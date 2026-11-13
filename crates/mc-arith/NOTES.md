# Item B: `mc-arith`

Status: built and tested. Depends on `mc-api` and `rustc-hash`; `num-bigint`/`num-traits` are
dev-dependencies only. Build, clippy (all targets) and tests pass offline with no warnings in release and debug
(overflow checks on); `#![warn(missing_docs)]` clean.

## What was built

- **`num`, the bare number layer.** From spike 3: `Nat` (schoolbook, Karatsuba, three-prime NTT; Knuth and Newton
  division; isqrt; divide-and-conquer to-decimal), `Int`, `Ball`/`Float`/`Mag` (radius rounded up). From I3: Lehmer
  `gcd`, Knuth/Henrici `Rat`. Added `Nat::from_decimal`, `pow_u64`, `iroot`, `trailing_zeros`; `Int::to_i64`,
  `divrem_floor`; `Rat::recip`, `pow_u64`, `floor_fract`, `cmp_value`. `num::float`: correctly rounded exact-to-machine
  conversion (ties to even, subnormals) and machine-to-dyadic. Stop flag: a per-thread scoped `Arc<AtomicBool>`
  (`StopScope`, `stopped()`) polled by every long kernel loop.
- **`number`, numbers as values.** `Real` is `Exact(Rat)`, `Ball(Ball, prec)` or `Machine(f64)`; `Num` is `Real` or
  `Complex(Real, Real)`. Contagion exact < ball < machine. Three-valued `compare` (`Cmp3::{Less, Equal, Greater,
  Unordered, Undecided}`) and `equal`. `pow`: integer powers (exact, ball, machine, complex), principal machine powers
  (complex for negative bases), ball half-integer powers through sqrt. `cmp_canonical`: the total order on numbers.
- **`radical`, canonical radicals.** Trial division by primes below 4096, leftover cofactors made pairwise coprime by
  gcds and reduced when perfect powers; integer parts of exponents move to the coefficient, primes grouped by
  fractional exponent (`8^(1/2)` → `2*2^(1/2)`, `12^(1/3)` → `2^(2/3)*3^(1/3)`, `2^(1/2)*3^(1/2)` → `6^(1/2)`).
- **`Order`, the canonical order (semantics 8):** explicit stack, views and symbol names only, cached split names.
- **`Arith<'h, S: Store + ?Sized>`, the canonicaliser:** `add`, `mul`, `pow`, `sub`, `div`, `neg`, `complex`; number
  terms `num`, `number`, `int`, `integer`, `int_from_decimal`, `rational`, `ball(t, prec)`,
  `ball_with_radius(mid, rad, prec)`; `cmp`, `compare`, `equal`, `text`; `with_stop(Arc<AtomicBool>)`, `with_max_bits`.
  Per thread (`!Sync`), cheap to create.
- **`text`, number views for printing:** integers, `num/den`, shortest round-trip machine reals, balls showing only
  correct digits (`Digits { neg, digits, exp10 }`, `to_text`, e.g. `3.1415926535897932385`; no correct digit prints as
  `ball(m, r)`); `digits_to_bits`.
- **Failures produced:** `failure(division_by_zero)`, `failure(limit, memory, partial)`, `failure(interrupted,
  partial)`, `failure(indeterminate, partial)` (B.17). A failure argument to add, mul or pow is returned as the result.

## Tests (29 unit, 19 integration; about 2 s in release)

- Unit: the three multiplication algorithms agree with each other and num-bigint; division, isqrt, iroot, decimal
  both ways, NTT primes; Lehmer gcd against Euclid (Fibonacci, harmonic splitting); ball enclosure, Mag rounding, IEEE
  round trips; contagion, three-valued comparison, number order, powers, radicals, ball digits.
- `tests/canonical.rs`: every semantics 2.4 and 3 example.
- `tests/collect.rs`: 400 random polynomials each built 4 ways give one handle equal to a model built from
  monomials collected in a map (a mutation disabling sorting is caught); 2000 random nested expressions expand to the
  same polynomial (num-bigint) and the canonical top level is a fixed point.
- `tests/powers.rs`: 1500 random products of symbolic and numeric powers built 3 ways give one handle with the value of
  the product of principal powers (a mutation is caught).
- `tests/numbers.rs`: integers of 1–60 limbs, NTT-size products, 3^20000 against num-bigint; canonical rationals;
  equal numbers built different ways are one term; balls through the canonicaliser enclose the exact result and print
  the exact value correctly rounded.
- `tests/order.rs`: 300 random terms of every kind (NaN, -0.0, raw `complex(1, 0)`, balls, qualified symbols): total,
  antisymmetric, `Equal` only for the same handle, transitive, agrees with decided numeric comparisons; stable across
  stores built in reverse order; 200k-deep comparisons and flattening without stack overflow.

## Measurements (busy machine, `RefStore`, release)

| Operation | Time |
|---|---|
| Add 10^4 distinct monomials | 12 ms |
| Sum of two such sums (collected) | 19 ms |
| 10^4 terms with 100 distinct rests | 4.4 ms |
| 2000! by a loop of `mul` | 5.6 ms |
| Square of a 10^6-digit integer | 21 ms |
| Decimal of the 2×10^6-digit result | 345 ms |
| 10^5 small-integer adds | 50 ms (mostly the RefStore lock) |
| H(10^4) by a loop | 81 ms |

## Decisions

- **B.1** Arguments must already be canonical; only the top level is canonicalised; nested `add`/`mul` are flattened
  with an explicit stack.
- **B.2** A ball's working precision is the bit width of its midpoint mantissa (stored widened to exactly p bits; a
  zero midpoint keeps p in the exp word), within the I3.5 word layout. Equal balls at the same precision have equal
  words.
- **B.3** Ball operations work at the largest precision among their ball operands.
- **B.4** Contagion applies between the parts of a complex number (`complex(1, 2.0)` → `complex(1.0, 2.0)`); only an
  exact zero imaginary part is dropped.
- **B.5** Number folding combines exact operands first, then inexact ones in canonical order, so inexact results do not
  depend on operand order.
- **B.6** Identities are exact only (`1.0*x`, `x + 0.0` stay). A zero of any kind absorbs a product; an inexact zero
  coefficient in a sum turns the term into that zero.
- **B.7** Number order at equal value: reals before complex; kind exact < ball < machine; machine reals by IEEE total
  order; balls by radius, then precision; non-canonical payloads by words. Symbols compare by the name after the last
  dot, then by module path.
- **B.8** Radical form `c * (-1)^u * prod m_i^f_i` with u in (0, 1/2), distinct f_i in (0, 1), m_i the product of the
  primes whose total exponent has fractional part f_i.
- **B.9** `pow` rewrites valid for all complex values: `(x^a)^n = x^(an)` for integer n or real a in (-1, 1];
  `(xy)^n = x^n y^n` for integer n; `(c x)^a = c^a x^a` for positive real c; like factors merge by adding exponents.
- **B.10** Nested canonicaliser calls are capped at depth 64 (raw beyond), bounding recursion on exponent towers.
- **B.11** A ball prints the largest d such that both ends round to the same d digits (zero radius = half an ulp at
  p); no correct digit prints as `ball(m, r)`, r rounded up.
- **B.12** `x^0 = 1` for every x (so `0^0 = 1`); `1^x = 1`; `0^r` is 0 for r > 0 and a division by zero for r < 0.
- **B.13** No numeric value means the term stays unevaluated: division by a ball that may contain zero, a ball to a
  non-half-integer or ball exponent, an exact complex number to a non-integer exponent.
- **B.14** An inexact coefficient does not evaluate exact radicals numerically (`2.0*2^(1/2)` stays); that is `n()`'s
  job.
- **B.15** Exact powers larger than `max_bits` (default 2^28 bits) give `failure(limit, memory, pow(b, e))`.
- **B.16** A raised stop flag gives `failure(interrupted, <raw term>)`.
- **B.17** Signed infinities: `inf` and `-inf` (`mul(-1, inf)`) are extended reals. In a sum, `inf` absorbs the finite
  real numbers (`inf + 1`, `-3.0 + inf` are `inf`; a machine infinity or NaN is not absorbed; other terms stay:
  `inf + x`); infinities of opposite signs give `failure(indeterminate, partial)`. In a product a real coefficient
  keeps only its sign (`2*inf` is `inf`, `-0.5*inf` is `-inf`, positive radicals go, a complex coefficient stays);
  `inf^a` is `inf` for real a > 0 and 0 for a < 0; `0*inf` (any zero, an exception to B.6) and `inf/inf` are
  `failure(indeterminate, partial)`. `compare` and `equal` decide between real numbers and signed infinities (a
  machine infinity equals the infinity of its sign), so `1.0 < inf` is `true` and `x < -inf` is `false` for every
  real x. The tag `indeterminate` is interned only when such a failure is built (interning it in every
  canonicaliser shifted the numbering of later session symbols, which `mc-assoc`'s fuel test noticed).

## Contract changes (applied to the shared interface)

1. Failure tag names `division_by_zero`, `limit`, `memory`, `interrupted` in `mc_api::names` (defined locally in
   `mc_arith::failure_tags` until then).
2. B.2 documented on `View::Ball`.

## Known gaps

- Radical uniqueness is complete only for primes below 4096 or large primes that gcd refinement separates.
- `2*2^x` and `2^(x+1)` are different forms (no merging of numeric coefficients with numeric-base symbolic powers).
- Inexact values cannot have unique forms (floating-point addition is not associative); results are deterministic
  under operand permutation.
- No exp/log for balls: ball powers beyond integer and half-integer exponents stay unevaluated.
- Perfect-power and trial-division work repeats on every product with radicals; gcd is Lehmer only.

## What integration needs from this crate

- Every `add`/`mul`/`pow`/`complex` and arithmetic result is built through `Arith`, not raw `app`; pass the session's
  stop flag with `with_stop`.
- `mc-syntax`: number literals through `int_from_decimal`, `rational`, `ball(t, prec)` (with `text::digits_to_bits`)
  and `ball_with_radius`; printing through `Arith::text` / `text`; `sub`, `div`, `neg` lowering.
- `mc-eval`: `Arith::cmp` (or `Order`) for sorting and match enumeration; `compare`/`equal` for three-valued
  comparisons (`Undecided` stays unevaluated).
- Re-run these tests against `mc-core` (so far only against `RefStore`).

## Item 9 (library and kernel gaps)

- **Clean returns after a stop (9.2):** `num::after_stop(what)` panics as before without a pending stop and lets the
  caller return any (discarded) value after one; applied at every panic site a stopped computation reaches (`Nat::sub`
  underflow, division by zero in `divrem_u64`/`divrem`/`Divisor::new`/`divrem`, the `isqrt` correction loop,
  `Mag::div_up`, `Float::div`, `Float::sqrt` of a negative, `Float::to_sci`, `Rat::new`, `rat_to_ball`); the debug
  assertions in `div_exact` and `Divisor` relaxed the same way. Tests `num::stop_tests` run every public operation under
  a raised flag and under a simulated flag raised at 12 points.
- **Faster decimal conversion (9.4):** no reciprocal for the top power of ten (only compared); each power's reciprocal
  seeded from the square of the previous plus one Newton step (`recip_seeded`, `Divisor::from_root`); an exact Newton
  correction; from 256 limbs a `Divisor` keeps the NTT transforms of its reciprocal and itself (`ntt::Spectrum`), with
  the remainder computed as a half-length cyclic product modulo B^m - 1 (`ntt::mul_cyclic`); the NTT split into
  `forward`/`backward`/`reconstruct`. 10^6 digits 0.176 -> 0.117 s, 10^7 digits 3.18 -> 1.85 s (`from_decimal` 0.08 s
  and 1.1 s unchanged). Tool: `examples/decimal.rs`. Tests: `ntt::spectrum_products_match`,
  `nat::divisors_match_division`, `nat::decimal_matches_reference` up to 9000 limbs and 10^120000 +- small.
- **Negative powers of machine reals:** `Num::pow_int` took the reciprocal and negated the exponent (`2.0^-1` gave 2.0,
  `3.0/7.0` gave 21.0); a machine real now takes the signed exponent. Test `number::negative_integer_powers` (machine
  reals, rationals, balls, complex). Also fixes `mc-jit`'s compiled code (it goes through `Num::pow`).
- Gap: `mc-numeric`'s L.11 workaround (square roots with the stop flag off) can now be lifted.

## Round 19

- `-1` times a single sum distributes over it (`-(a + b)` is `-a - b`, also for products whose
  only other factor is a sum), so `s - s` is 0 for every sum `s`. The bench sessions
  showed no measurable change (the path runs only for products `-1 * sum`).
- Roots of unity: `(-1)^u` is normalised with `u` in (0, 1) and a real sign (`(-1)^(-1/3)` is `-(-1)^(2/3)`), a
  half-integer `u` gives `+-i`; a pure imaginary exact coefficient joins the unit (`im (-1)^(1/6)` is `(-1)^(2/3)`;
  it was `i (-1)^(1/6)` with `u` in [0, 1/2)).
- `complex_infinity` and `indeterminate` (decision i19.ci): `indeterminate` absorbs sums, products and powers;
  `complex_infinity` absorbs finite terms and nonzero factors, is `indeterminate` with another infinity in a sum or
  times 0; `complex_infinity^k` is `complex_infinity`, 0 or `indeterminate` for k > 0, < 0, = 0. (`inf - inf` keeps
  its `failure(indeterminate, ...)`.)

## Round 20

- Deferred: the signed machine zero in `x - 2.5 im` (`complex(-0.0, -2.5)`). Unsigning it in `Num::complex` makes the
  scalar path disagree with packed complex arrays (mc-engine `tests/packed_complex.rs` compares them exactly), so
  both paths must change together.


## Round 27

- **X26-16 `text.rs::ball_digits`**: the printed digits of a ball no longer follow a radius far below the last bit of its precision:
  `1 - 10.0~25^-100000000` is the ball 1 +- 3e-(10^8) and printing computed m - r exactly (3.3e8 bits, 2.8 GB; the evaluation itself
  was instant and honoured the time limit). The radius is taken as at least 2^(top - p - 64).

## Round 36

- A machine infinity (an overflow) in a sum with the symbol inf is that infinity: of the other sign the sum is indeterminate, the same sign stays beside it (a machine infinity is not absorbed: tests/canonical.rs). Was left as inf - inf, and divided by inf gave 0.

## Round 39: balls of very different magnitudes

- `n(1 + exp(-10^12), 30)` and `limit(exp(-1/x^2) + 1, x -> 0)` (whose numeric check meets that sum) asked for 180
  GB and aborted the process: the comparison of real numbers formed the exact rational ends `mid +- rad` of a ball
  (a denominator of 10^12 bits), `has_digits` formed `|mid| - rad` exactly, and `cmp3` and `cmp_exact` aligned
  mantissas whose exponents are 10^12 bits apart. Now `compare` takes two balls (an exact operand as a ball 64 bits
  finer) by `cmp3`; `cmp3` decides midpoints more than two binades apart by the larger one; `cmp_exact` decides
  different signs and different binades by the exponents; `has_digits` tests `r (10^d + 1) <= |m|`. The verdicts
  are the same; no exact alignment of far-apart numbers is formed.
- `cmp3` of midpoints more than two binades apart compared the radius sum with half the larger midpoint and gave up
  above it, where the exact comparison decided: the 30-digit Coulomb value against the 60-digit one, a difference
  of radius 8.1e-31 below the bound 1.59e-30, stayed undecided. The modulus of the difference exceeds |big| -
  2^top(small) >= |big| - 2^e, e = max(top(small), exp(big) - 64), and that bound is formed exactly with an
  alignment of at most 64 bits beyond the mantissa. Test: `tests/numbers.rs`.
