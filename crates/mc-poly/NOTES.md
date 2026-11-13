# Item K: `mc-poly`

Status: built. Every planned function works
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
- `tests/bigint_model.rs` (`num-bigint`): random products and powers; gcds divide both inputs and are greatest;
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
- `gcd`/`lcm` naming with item J: the evaluator cannot chain natives; install `mc-poly` after `mc-numtheory` (this
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
  result has `binomial(2m + 4, 4)` terms (135751 at m=20). Fixed in the case.
- P11 builds `p` with `range` and lambdas (the crate test builds it directly in Rust).

## Item 9 (library and kernel gaps)

- **`gcd` and `lcm` have one home (9.1):** exact numbers go to `mc_numtheory::number_gcd_lcm` (half-gcd) inside
  `mc-arith`'s stop scope; everything else to the polynomial gcd/lcm; an inexact number gives
  `failure(argument, gcd|lcm, ...)`. The old Lehmer number path is gone. Measured: 104,687 digits 79-87 ms (half-gcd
  direct 73-77, Lehmer 97-106); 418,745 digits 406 ms (410; Lehmer 1.66 s). Test `tests/gcd_dispatch.rs`.
- `series` reads an order term `O((x - x0)^p)` (also `O(x)`, `O(1)`) as a zero series of precision p, so printed series
  read back; `normal` drops `O(...)` terms. `FUNCTIONS` gains `lcm` and `O` (a known name without a native).
- New dependency `mc-numtheory`, dev-dependency `mc-bench`.

## Item F2: polynomial rings over generic coefficients (round 16)

Roadmap phase F, item F2. New modules `ring`, `unipoly`, `decompose`, `descartes` (moved from `mc-solve`),
`ringfns` (natives); `resultant` now runs its multivariate path on the new chain. The real-root and
algebraic-number half of this work is in `mc-algebra` (`realroot`, `countroots`, `minpoly`; see its NOTES).

### API for phases 11A (CAD) and 14A (tower integration)

Coefficient rings (`mc_poly::ring`):

```rust
pub trait CoeffRing {
    type E: Clone + PartialEq + Debug;
    fn zero(&self) -> Self::E;  fn one(&self) -> Self::E;  fn of_i64(&self, n: i64) -> Self::E;
    fn is_zero(&self, a: &Self::E) -> bool;  fn is_one(&self, a: &Self::E) -> bool;
    fn add/sub(&self, a, b) -> Self::E;  fn neg(&self, a) -> Self::E;
    fn mul(&self, a, b, m: &mut Meter) -> WorkResult<Self::E>;
    fn div_exact(&self, a, b, m) -> WorkResult<Option<Self::E>>;   // None: does not divide
    fn inv(&self, a, m) -> WorkResult<Option<Self::E>>;  fn pow(&self, a, e: u64, m) -> WorkResult<Self::E>;
    fn is_field(&self) -> bool;  fn characteristic(&self) -> u64;
    fn gcd(&self, a, b, m) -> WorkResult<Self::E>;  fn size(&self, a) -> u64;
}
```

Instances: `Integers` (`Z`), `Rationals` (`Rat`), `Zp::new(p)` (`u64` residues), `PolyRing { nv }` (`Poly`, an
integral domain with exact division: coefficients that are polynomials in parameters), `RatFunField { nv }`
(`RatFun` in lowest terms), and in `mc-algebra` `NumberField` (`Elem`). A ring of functions for 14A implements the
same trait (exact division and, for gcds over a field, `inv`).

Dense univariate polynomials over any ring (`mc_poly::unipoly`, `Vec<E>`, index = degree, trimmed):
`trim, deg, lc, coef, constant, add, sub, neg, scale, div_scalar, shift_up, mul, pow, divrem (-> Option<QuoRem>),
div_exact, prem, derivative, eval, compose, content, primitive, monic, gcd, square_free` and

```rust
pub struct Chain<E> { pub s: Vec<Vec<E>> }           // S_0 ..= S_q, zero ones empty
impl Chain<E> { fn psc(&self, r, j) -> E; fn principal_coefficients(&self, r) -> Vec<E>;
                fn resultant(&self, r) -> E; fn remainders(&self) -> Vec<Vec<E>>; fn last_nonzero(&self) -> Option<&Vec<E>> }
pub fn subresultant_chain<R: CoeffRing>(r: &R, p: &[R::E], q: &[R::E], m: &mut Meter) -> WorkResult<Chain<R::E>>; // deg p >= deg q, q != 0
pub fn resultant<R: CoeffRing>(r: &R, a: &[R::E], b: &[R::E], m) -> WorkResult<R::E>;
pub fn discriminant<R: CoeffRing>(r: &R, a: &[R::E], m) -> WorkResult<Option<R::E>>;
```

CAD projection reads `principal_coefficients` of `(f, f')` and `(f, g)` over `PolyRing` (coefficients in the
remaining variables) or, at a sample point, over the `NumberField` of the sample. 14A's Rothstein–Trager
resultant `Res_x(a - z d', d)` over a tower is `unipoly::resultant` over the tower's ring.

Decomposition (`mc_poly::decompose`):

```rust
pub fn decompose<R: CoeffRing>(r: &R, f: &[R::E], elements: Option<&[R::E]>, m) -> WorkResult<Option<Vec<Vec<R::E>>>>;
pub fn right_component<R: CoeffRing>(r: &R, f: &[R::E], deg_h: usize, elements: Option<&[R::E]>, m)
    -> WorkResult<Option<Option<(Vec<R::E>, Vec<R::E>)>>>;   // f = g(h), h monic with h(0) = 0
```

14A asks `right_component` for substitutions `u = h(x)`; every answer is an exact identity.

Real roots (`mc_poly::descartes`, used by `mc-solve` and `mc-algebra`): `Interval { lo, hi }`, `isolate(&[Z], m)`,
`refine`, `halve`, `sign_at`.

### User functions

`subresultants(p, q, x)`, `subresultant_polynomials(p, q, x)`, `subresultant_polynomial_remainders(p, q, x)`
(`deg p >= deg q`, else unevaluated), `decompose(p, x)`, all with `modulus: n` (n prime); the
`modulus:` option for `resultant` and `discriminant`. `resultant` and `discriminant` of products are computed
factor by factor and stay products.

### Decisions

- **F2.1** One trait over the coefficient ring, ring values carrying their context; the existing `Poly`/`QPoly`
  kernels (modular gcd, modular resultant, factorisation) stay as they are and stay the fast paths for Z and Q.
- **F2.2** Subresultant chains by Ducos (2000): Lazard's binary powering for `S_e`, Ducos's reduction for `S_(e-1)`
  (`d - e + 1` steps of size `e` instead of a pseudo-remainder of size `d`); the classical step only if a Ducos
  division is not exact (never observed: the test counts fallbacks and asserts zero). The whole chain, defective and
  zero subresultants included, is one object.
- **F2.3** `modulus: p`: inputs reduced mod p first (degrees over F_p), the integer chain or resultant of the
  reduced representatives, the result reduced again (subresultants are integer polynomials in the coefficients).
- **F2.4** Decomposition: `x^k` extraction first (the component kept whole), then right components of increasing
  degree by Kozen–Landau (truncated s-th root by J. C. P. Miller's recurrence, or coefficient by coefficient when
  the characteristic is at most the degree), accepted only when the h-adic expansion has constant digits, so
  `f = g(h)` holds exactly; the wild case (char | s) enumerates at most 4096 candidates over small prime fields, and
  is reported undecided (unevaluated) beyond that, never guessed. Normalisation (inner components monic without
  constant term, a linear outer component kept).
- **F2.9** `resultant`/`discriminant` of products multiplicatively (`res(fg, h) = res(f, h) res(g, h)`,
  `disc(fg) = disc f disc g res(f, g)^2`), so factored inputs give factored answers and
  large expanded products are never formed.

### Checks and speed, with evidence

- Decompositions are verified identities by construction (h-adic expansion), and a wild case is left unevaluated
  rather than answered without proof.
- Multivariate resultants: the Ducos chain replaced the crate's Collins/Brown subresultant algorithm
  (`tests/f2_bench.rs`, release, best of 3; crate before -> Ducos): `Z[y]` degrees 6/5 x 3: 0.97 -> 0.60 ms; 8/7 x 4:
  7.05 -> 4.95 ms; 10/9 x 3: 12.5 -> 9.8 ms; three variables 5/4 x 2: 38.4 -> 6.9 ms. The univariate integer
  resultant keeps the modular algorithm (0.08/0.28/1.14 ms at degrees 20/40/80, against 0.11/0.77/4.25 ms for the
  generic chain over Z), so the fast path did not slow down: the dispatch to it is unchanged.

### Tests

- unit: `unipoly` (2: 64 random and 4 structured pairs whose chains equal Sylvester determinant polynomials computed
  directly by Bareiss, Ducos = classical, zero fallbacks; resultants, discriminants, gcds over Z, F_7, Q, Z[y],
  composition), `decompose` (1: the documented shapes, random 2 x 3 x 2 compositions, mod 3, a wild case mod 2).
- `tests/f2_functions.rs` (3): subresultant, remainder, decomposition and modulus examples with their expected
  values, products of resultants/discriminants compared with the expanded computation.
- `tests/f2_bench.rs` (ignored): the timings above.

### Open gaps

- Ducos's chain is not used for the univariate integer resultant (the modular one is faster); a modular chain
  (images over F_p, CRT) would make `subresultants` of large integer inputs faster.
- `decompose` over number fields works through `CoeffRing` but has no user option (`extension:`).
- Wild decompositions over large prime fields are undecided.

## Round 19 fixes

- **f19.1** `polynomial_gcd`/`polynomial_lcm` options (Q19-05, Q19-06): `modulus: p` (monic gcd/lcm over F_p,
  one variable), `extension:` for the lcm (`p (q / gcd)` with the quotient by the monic gcd in `Q[a][x]`, exact
  modulo the minimal polynomial because the divisor is monic; the gcd stays mc-algebra's), `trig: true`
  (trig-expanded, even powers of cos written through sin). An unknown option is an argument failure; it used to be
  read as a factor (`options(extension -> automatic) x^3 - ...`).
- **f19.2** `factor_mod_p` (`factor.rs`): Yun's square-free decomposition in characteristic p (p-th roots of
  polynomials with zero derivative), distinct- and equal-degree factorisation (the trace map for p = 2).
- **f19.3** `subresultants(p, q, x)` with deg p < deg q (Q19-13): the coefficients of `(q, p)` with the sign
  `(-1)^((deg p - j)(deg q - j))`; the polynomial chains still need deg p >= deg q.
- **f19.4** `coefficient(p, x^s)` with a symbolic exponent (Q19-25): `x^(k s + c)` counts as `(x^s)^k x^c`.

## Round 20

- `gcd`/`lcm` with symbolic arguments that share no polynomial factor stay unevaluated (`gcd(a, m)`: the symbols
  may stand for integers); `polynomial_gcd` keeps the polynomial answer 1.
- `variables` of a list, equation, inequality or logical combination: the variables of its parts in order.
- Nested `gcd`/`lcm` calls flatten: `lcm(lcm(l, m), n)` is `lcm(l, m, n)`.

## Round 22 fix-up

- `decompose` declines polynomials of degree above 2^20: it works on dense coefficient lists, and
  `x^(2^31) + ...` asked for a 16 GB list (an allocation failure aborts the process; found by the fuzz test).

## Round 24

- `factor_univariate`: `x^n - 1` and `x^n + 1` (n >= 4) are factored into cyclotomic polynomials, `Phi_d = (x^d - 1) /
  prod_(e | d, e < d) Phi_e` (Lang, *Algebra*, VI.3); `factor(x^1000 - 1)` took 60 s by Zassenhaus and now takes a
  second or two (the `reduce(x^1000 - 1 > 0)` of D23-22).
- Deferred: B23-31 (`expand((x + y + z)^300)` slow; a multinomial expansion).

## Round 30

- `polynomial_quotient` / `polynomial_remainder` with a divisor whose leading coefficient in x is not a number:
  pseudo-division by the leading coefficient L (Knuth, TAOCP 4.6.1, algorithm R), then each x-coefficient of the
  quotient or remainder is divided by `L^k` and reduced to lowest terms (`symbolic_division`), i.e. division over the
  field of rational functions of the other variables. `polynomial_extended_gcd(x, (x - a)^2, x)` (mc-combinat) now
  terminates: its remainders are x-free rational functions.
- `polynomial_gcd` / `polynomial_lcm` of rational functions with non-constant denominators: gcd of the numerators over
  the lcm of the denominators (lcm: lcm of the numerators over the gcd of the denominators), in lowest terms.
- Printing only: `polynomial_gcd(..., modulus: 2)` answers in expanded form rather than as powers of linear
  factors; some gcds differ from a hand computation by sign only.
- Gaps: `polynomial_quotient` of rational-function arguments; `factor_list` /
  `factor_square_free_list` do not see `exp(2 x)` as `exp(x)^2` (kernels are taken as they stand); `variables` keeps
  `sqrt(x + y)` as one kernel rather than listing x and y (kept: the polynomial machinery relies on it).

## Round 31: series fixes

- **F31.1** `series` with an order below the leading fractional power: `series(x^(-1/4), x, inf, 0)` was `O(x^-1)` and
  `series(x^(1/4), x, 0, 0)` was `O(x)` (the gcd reduction of a ramified series with no coefficient took the order of the
  reduced ramification); now `O(x^(-1/4))` and `O(x^(1/4))`, the honest remainder (no term of the expansion up to the
  order exists, the next power is the leading one). The reduction runs only when some coefficient is nonzero.
- **F31.2** `log(-x)` and every `log(c t ...)` with a negative real c and valuation 1: the coefficient is `log(-c) + log(-t)`
  (c t = (-c)(-t) holds for every real t), where the old `log(c) + log(t)` was valid only for t > 0 (`log(x) + i pi` under
  x < 0 was wrong by 2 i pi). Unramified expansions only (`log_neg_t`; ramified or Gruntz callers with their own log(t)
  keep the old form).
- **F31.3** A symbolic power of the variable (`x^(m + 1)`, `tanh(y)^k`) came out as a constant coefficient with an
  `O(x^p)` that ignored it. The coefficient's `x^s` (s free of x and not a number) is now moved to the exponents and the order
  term: `x^(1 + m) + x^(2 + m) + ... + O(x^(p + m + 1))`; terms with different shifts, or only some, decline (unevaluated).
  At x0 = 0 with den = 1 only.
- Not a bug: unevaluated `derivative(k)(f)(u0)` coefficients of the Taylor table (`series(zeta(x), x, 2, 1)` has
  `zeta'(2)`) are the correct symbolic values, as for a user function; the singular cases (theta functions at q = 0) are
  taken by the d30 wrapper, which sees the unevaluated derivatives. Rejecting every builtin derivative lost `zeta'(2)`.
- **F31.5 Exponentials in `factor`.** `e^(c w)` with an integer c, |c| > 1, is read as the |c|-th power of the kernel
  `e^(w sign c)` (`Conv::splitting`, used by `factor` and so by `factor_list`, `factor_square_free_list`): `factor_list(exp(2 x)
  - 2 exp(x) + 1)` is `[[1, 1], [-1 + exp(x), 2]]`, `factor(exp(2 x) - 1)` is `(-1 + exp(x)) (1 + exp(x))`. Other
  polynomial functions keep exp(2 x) and exp(x) as unrelated kernels (a first version in every conversion broke
  `solve(exp(2 x) - 3 exp(x) + 2 == 0)`, which looks for the substitution itself). The `d_solve` system test and the
  bilateral inverse Laplace test print the fully factored forms now (`(-1 + exp(t)) (1 + exp(t)) (1 + exp(2 t)) exp(-t)`).
  Gap: `x + 2 sqrt(x) + 1` (the kernels `x` and `x^(1/2)` need one common root variable).
- **F31.6** (`lib/calculus.mc`) `polynomial_mod(p, m)` with several variables is the remainder of the multivariate
  division in the lexicographic order with the later variables larger (`x^2 y mod x y - 1` is `x`, it was `1/y`; `x^2 mod x + a`
  is `a^2`; `x^2 + a^2 mod a x - 1` is unchanged); `factor_terms(p, modulus: m)` reduces the coefficients and pulls out
  the leading coefficient when invertible; `factor_terms(p, trig: true)` reduces trigonometric products first.

## Round 32

- Memory cap of products and powers follows the process budget (half of `default_max_young`, at most 1 GB) instead of a fixed 1 GB, and the Kronecker operand limit scales with it: `length(expand((x + y + z)^1000))` fails with `failure(limit, memory)` under a 1000 MB cap instead of aborting in an allocation.
- `Poly::pow` uses a multivariate form of J. C. P. Miller's coefficient recurrence (1962) for few-term bases with large results (`pow_recurrence`): each result term costs (terms of the base) steps, against squaring two huge operands. Verified against repeated multiplication in the unit test.
- `factor` and `factor_square_free` read `x` and `x^(p/q)` as powers of one root kernel `x^(1/L)`: `x + 2 sqrt(x) + 1` is `(1 + sqrt(x))^2`.

## Round 36

- `variables(p)` returns the variables of the expansion, looking through kernels `u^r` with `r` a number (a root or a
  reciprocal stands for the variables of `u`) and dropping `e^u`; the result is sorted
  canonically when a kernel was replaced and for lists/relations. `variables(p, kernels: true)` returns the raw
  kernels (used by library functions that need polynomial structure: `factor_terms_list`, `coefficient_rules`, ...).
  This changes `variables(x^(1/2) + y)` from `[y, x^(1/2)]` to `[x, y]`.
- `factor(p, modulus: q)` and `factor_square_free(p, modulus: q)` for one variable: Cantor-Zassenhaus over `F_q`
  (`factor::factor_mod_p`), leading coefficient times monic factors with coefficients in `0..q-1`.
## Round 36

- `Tick::charge`: a raised stop flag sets the abort (see mc-cad NOTES).

## Round 36, follow-up

- `variables(p, modulus: m)`; `coefficient_list` of a series takes its polynomial part; `resultant` of rational functions in x is extended multiplicatively, res(A/B, C/D) = res(A,C) res(B,D)/(res(A,D) res(B,C)); with finite-field elements the resultant is expanded.
- The resultant's prime loop charges before skipping a prime (same cause as mc-algebra's gcd).
- Series of a step function (floor, ceiling, round, integer_part, fractional_part, unit_step, heaviside_theta, sign) whose argument sits on a jump: no series, unless the side is known (an even order of the argument's first nonzero derivative, or assumptions x < x0 / x > x0), then the one-sided constant (or u - integer part). Was the value at the point (series(floor(4 - t)) = 4).
- A divisor that vanishes identically stops the precision doublings at 512 extra orders (was thousands of terms).

## Round 37

- Groebner bases in any monomial order: `groebner::TermOrder` (the order and the coefficient field) and
  `MatrixOrder`, a weight-matrix order (Robbiano; Becker and Weispfenning, *Groebner Bases* 5.1). Lex, deg-lex,
  degrevlex and block (elimination) orders are matrices; a user matrix is accepted only when it is a well-order
  (full rank, first nonzero entry of each column positive), otherwise `groebner_basis` gives an argument failure.
  The Buchberger code is generic over `TermOrder`; `groebner(input, MonomialOrder, m)` is unchanged for its callers.
- Over `F_p` (`MatrixOrder` with a modulus): the same fraction-free code with every coefficient reduced to `[0, p)`
  after each combination (reduction mod p is a ring map, so the combinations are those over `F_p`; a gcd of two
  nonzero residues is nonzero mod p) and each element made monic at the end. No parameters (numeric kernels) then.
- `expand(e, modulus: n)` (any n > 1) and `polynomial_quotient` / `polynomial_remainder` with `modulus: p` (F_p[x],
  one variable; `division_mod`). `expand(e, patt)` declines to the library rule in mc-combinat.
## Round 37: series of inverse functions at infinity and at branch points

- `atan` of an argument with a pole (val < 0) at infinity (t > 0, x = ±t^-den): atan(a) = s pi/2 - atan(1/a) with s the
  sign of the real leading coefficient (DLMF section 4.24); an undecided sign asks the term ring (NeedTerms).
- `asinh`, `acosh`, `atanh` at infinity, and `atanh`, `acosh` at their branch points ±1, by their logarithmic forms,
  equal on the principal branches in the whole plane (DLMF section 4.37): log(u + (u^2 + 1)^(1/2)),
  log(u + (u + 1)^(1/2) (u - 1)^(1/2)), (log(1 + u) - log(1 - u))/2. Checked against the function at points to 30
  digits (the error is the first omitted term).
- `coefficient(p, [x1, ..., xn], ...)`: the list of coefficients; `coefficient(p, x^a y^b)`: the coefficient in each
  variable in turn (the term x^a y^b with the other variables kept); `coefficient(..., modulus: q)` and
  `coefficient_list(..., modulus: q)`: of `polynomial_mod(p, q)`.
- The Taylor table of `series` also for negative orders, and with the orders a pole of the expression lowers the
  terms by (the negative integer powers of terms in x): bessel_i(3, 2 x)/x^7 to order -4 was O(x^-3) (its x^-4 term
  dropped, a wrong zero coefficient) and to order 0 lost the terms after x^-4.

## Round 38: options handed to the library

- `together`/`cancel` with the options `modulus:` or `trig:` and `expand` with `trig:` decline in the native, and the
  library's rules (mc-combinat) take them; another option of `together` is still an argument failure.

## Round 39: series of the non-analytic functions

- `abs`, `sign`, `arg`, `re`, `im_part`, `conjugate` of an argument u(x) are not expanded through their real-variable
  derivatives any more (abs' = sign, sign' = 0 hold only for a real u: `series(sign(1 - im*t), t, 0, 2)` was
  `1 + O(t^3)`). For the real expansion variable, the Taylor coefficients c_k of u split into real and imaginary
  parts, u = R + i I with R, I real polynomials in x - x0, and abs(u) = sqrt(R^2 + I^2), sign(u) = u/abs(u),
  re(u) = R, im(u) = I, conjugate(u) = R - i I, arg(u) = arg(u0) + atan((I re u0 - R im u0)/(R re u0 + I im u0))
  (the argument of u conj(u0), near |u0|^2 > 0). A real u near u0 != 0 of known sign s gives s u and s.
- Declined (no series): a variable the assumptions make non-real; a coefficient whose real or imaginary part does
  not evaluate (a parameter of unknown realness: `abs(a + x)`); a complex u at a zero of abs, sign, arg; arg across
  its cut (u0 < 0 with I not identically 0); abs and arg of a real u through 0 without a known side. The last was a
  wrong value before: `series(abs(x), x, 0, 3)` gave `O(x^4)`.
- With `derivatives: false` (the limit walk), `non_analytic_table` still gives these forms for arguments built from
  the kernels' own functions only, since their series are known.
