# Package K: `mc-poly`

Status: built and committed (branch `lib/k-poly`: c72c082, 8dcf19c, a477d0b). Every function of the brief works
(`rewrite` belongs to H); all benchmark sizes pass except P12 at m=10^4 (too slow, see gaps). Release and debug tests
pass; clippy, rustdoc and the build are clean. Dependencies: `mc-api`, `mc-core`, `mc-eval`, `mc-arith`, `rustc-hash`;
dev: `mc-engine`, `mc-syntax`, `num-bigint`, `num-traits`.

## What was built (`src/`)

- **`z`**: `Z`, an integer that stays `i64` until it overflows, then becomes an `mc-arith` `Int`.
- **`poly`**: `Poly`, a sparse polynomial in `nv` variables, terms as flat exponent vectors in strictly decreasing lex
  order (equal polynomials have equal structs): ring operations, power, content, derivative, substitution,
  coefficients in one variable, exact multivariate division. **`qpoly`**: `QPoly`, an integer polynomial over one
  reduced common denominator.
- **`mul`**: three algorithms chosen by a calibrated cost estimate (K.3): Kronecker substitution (mixed-radix indices,
  64-bit-aligned slots, balanced signed digits, one big-integer product through `mc-arith`'s NTT; covers P11, and P1/P2
  which are dense enough that it beats the heap); a Monagan–Pearce heap with packed u64 exponent keys and 128-bit
  accumulators spilling to big integers; a hash fallback when degrees do not fit in one word. Memory checked as terms
  are produced, a cap on the Kronecker integers; `pow` checks its estimated size up front.
- **`convert`**: terms to polynomials and back (semantics 11) over kernels (maximal non-polynomial subterms; `b^-n`
  becomes the kernel `b^-1` to the power n). Shared subterms converted once; sums of monomials read with one sort; when
  kernels cannot interact the result is built directly in canonical order (K.4), checked against `Arith` in debug.
- **`modp`**: Fp arithmetic, Miller–Rabin, dense polynomials mod p (gcd, extended gcd, powmod, resultant).
- **`gcd`**: Brown's dense modular gcd (CRT over primes near 2^62, Newton interpolation in the last variable,
  unlucky-image detection by leading monomial, trial division at the end so the result is always correct, K.7).
- **`factor`**: content, monomial content, contents in each variable, Yun square-free decomposition. Univariate:
  Berlekamp–Zassenhaus (best of up to 6 small primes by distinct-degree count, Cantor–Zassenhaus, quadratic Hensel
  lifting down a balanced factor tree, Zassenhaus recombination with a constant-term filter, K.8). Multivariate:
  Kronecker substitution with subset recombination, capped at image degree 4000 and 16 factors (K.13).
- **`resultant`**: univariate over Z by primes plus CRT up to the Hadamard bound; with other variables, the
  subresultant algorithm (Cohen 3.3.7); discriminants (K.9).
- **`ratfun`**: rational functions in lowest terms via gcds (`together`, `cancel`, `factor` of quotients, K.10).
- **`series`**: truncated series with exact precision tracking and automatic precision retries (K.11): exp, log, sin,
  cos, tan, sinh, cosh, tanh, atan, sqrt, rational powers, `b^x`, `e^x`, `series_data` input. Rational coefficients
  when possible, otherwise redone with canonical terms, expanded and returned with `Reply::Again`. Over Q, exp/sin/cos
  use an exponential-generating-function integer recurrence with no gcds in the inner loop (K.12). Result term:
  `series_data(x, x0, [c_v..c_(p-1)], v, p)`.
- **`natives`, `functions`**: one `PolyNative` per function; a `Tick` bridge turns `NativeCx::tick` into the kernels'
  `Work`/`Meter` budget and polls `mc-arith`'s stop flag inside a `StopScope`.

## Public API

- `pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>)`.
- `pub const FUNCTIONS`: `expand`, `coefficient(p, x[, k])`, `coefficient_list(p, x | [x, y..])`, `degree(p, x)`,
  `variables`, `polynomial_gcd`, `polynomial_lcm`, `gcd`, `polynomial_quotient`, `polynomial_remainder`, `factor`,
  `factor_square_free`, `resultant(p, q, x)`, `discriminant(p, x)`, `together`, `cancel`, `series(e, x, x0, n)`,
  `normal`.
- Kernels for Rust callers: `Poly`, `QPoly`, `Z`, `mul`, `gcd::gcd`, `factor::factor`,
  `resultant::{resultant, discriminant}`, `convert::{Conv, Ring, Terms}`, `work::{Meter, Unlimited, Halt}`.

## Tests

- Unit (9): the three multiplication algorithms against schoolbook (200 random rounds), accumulator spill, `Z` edge
  cases, mod-p operations, gcd (univariate, multivariate, big coefficients), factorisation, resultants.
- `tests/functions.rs`: exact printed output for every function, edge cases and failures.
- `tests/oracle.rs` (`num-bigint`): random products and powers; gcds divide both inputs and are greatest;
  factorisations multiply back; resultants equal Bareiss determinants of the Sylvester matrix (subresultant path too).
- `tests/benchmark_cases.rs`: P1, P2, R8, P4, P5, P7 (exact expected values), P11 at m=2000, P12 at m=100, through an
  `mc-engine` session with this crate installed.
- `tests/interrupt.rs`: a series, a heap product and a factorisation each stop within about 0.4 s of an interrupt with
  `failure(interrupted)`, and the session keeps working.
- `tests/large_cases.rs` (ignored; guarded run ok at 25 s, 2.9 GB peak):

| case | time |
|---|---|
| P1 m=30 | 0.31 s |
| P2 m=15 / m=20 | 0.65 s / 2.5 s |
| R8 m=30 | 0.04 s |
| P4 m=15 | 0.025 s |
| P5 d=128 | 0.36 s |
| P11 m=10^4 / 10^5 / 10^6 | 0.15 s / 1.8 s / 19 s (about 6 s in `expand`; the rest is the evaluator building and squaring the million-term sum) |
| P12 m=1000 | 0.18 s (m=3000: 6.5 s) |

P7 at m=100 takes about 10 ms.

## Decisions

- **K.1** `install` takes the heap and `mc-arith`'s stop flag (mirrors `arith::install`).
- **K.2** Reading a term expands it in its kernels: rationals are coefficients; inexact and complex numbers are kernels.
- **K.3** The multiplication algorithm is chosen by cost estimates measured on this machine (heap about 2.6 ns per unit,
  Kronecker about 7 ns per limb unit).
- **K.4** Direct canonical construction when kernels cannot interact (two ranks per kernel, coefficient-numerator keys);
  otherwise terms go back through `Arith.add`.
- **K.5** `gcd` handles exact numbers (rationals: gcd of numerators over lcm of denominators) as well as polynomials.
- **K.6** Malformed calls return `failure(argument, fname, "message")`; not-applicable calls decline (unevaluated);
  exceeding the memory limit returns `failure(limit, memory)`.
- **K.7–K.13** The algorithm choices above.

## Requests

- `install` needs the heap (or `Kernel` should expose symbol creation).
- `gcd`/`lcm` naming with package J: the evaluator cannot chain natives; install `mc-poly` after `mc-numtheory` (this
  `gcd` handles integers too), or have the engine dispatch.

## Known gaps

- P12 at m=10^4 is O(n^2) on big integers (about 4 minutes extrapolated, so it hits the time limit; it stops cleanly);
  needs Newton iteration with fast multiplication over a common denominator.
- Multivariate factorisation gives up beyond Kronecker image degree 4000 or 16 univariate factors (no Wang/EEZ).
- No parallelism inside the kernels; the heap is about 40 ns per operation.
- Series: no Puiseux or log terms; coefficients in the term ring are only expanded.
- Exact division is O(quotient × remainder) sparse division, not heap division.
- The printer shows `series_data(...)` literally.

## Problems found in the cases

- P2's property `length(result) == binomial(2m + 5, 5)` was wrong: `1 + x + y + z + t` has four variables, so the
  result has `binomial(2m + 4, 4)` terms (135751 at m=20). Fixed by the coordinator.
- P11 builds `p` with `range` and lambdas (the crate test builds it directly in Rust).
