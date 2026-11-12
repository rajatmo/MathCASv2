# mc-algebra: algebraic numbers and number fields (roadmap phase F, item F1)

One number-field type for the whole system: elements of `Q[t]/(f)` with embeddings, polynomials over number fields,
and sums over the roots of a polynomial. Dependencies: `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-numeric`
(complex balls), `mc-poly` (integer factorisation, resultants, arithmetic modulo primes), `rustc-hash` (dev:
`mc-engine`, `mc-bench`). No new external dependency.

## API

`install(k, h, stop)` (after `mc-poly`: it wraps `factor`) and `FUNCTIONS`:

- `minimal_polynomial(a, x)`, `minimal_polynomial(a)` (a function `x => p(x)`): primitive integer polynomial with
  positive leading coefficient;
- `to_number_field(a, theta)` -> `[c0, ..., c(d-1)]` with `a = sum c_i theta^i`, `failure(not_in_field, ...)`
  when `a` is not in `Q(theta)`; `to_number_field([a, b, ...])` -> `[gamma, [coefficient lists]]`;
- `root_sum(x => p(x), x => g(x))`: the sum of `g` over the roots of `p` with multiplicity;
- `to_radicals(e)`: `root_sum` over irreducible cubics and quartics written with the radicals `solve` gives;
- `root_reduce(a)`: a rational, `u + v sqrt(D)` for degree 2, else `root(x => p(x), k)` with `p` the minimal
  polynomial; `algebraic_number_norm(a)`, `algebraic_number_trace(a)` over Q in `Q(a)` (all three and
  `minimal_polynomial` thread over lists);
- the `extension:` option (`a` a number, a list, or `automatic`) of `factor`, `factor_square_free` and
  `polynomial_gcd`: univariate polynomials over `Q(a, algebraic coefficients)`.

Rust (`pub`): `field::{NumberField, Elem, Alg}` (arithmetic, `inv`, `pow`, `trace`, `norm`, `charpoly`,
`minpoly`, `compose`), `embed::{Embedding, roots, enclose, locate, eval_ball}`, `algnum::{AlgNum, adjoin, Tower,
common_field, simple, roots_of, choose_factor}` (`AlgNum::sign`, `enclose`, `minpoly`), `kpoly::{gcd, gcd_euclid,
norm, square_free, factor_square_free, factor, Factored, rational_reconstruct}`, `qpoly` (dense `Q[x]`).

Outside the crate (all registrations or the integration this work needed):
- `mc-linalg/src/algebraic.rs`: `Alg` is now `mc_algebra::Alg` (with `NumberField`), the module only implements the
  elimination's `Field` trait; `eigen.rs` builds the field with `NumberField::new` (2 lines).
- `mc-calculus/src/integrate/rational/`: new module `rootsum.rs`; `logpart.rs` sends irreducible residue factors of
  degree 3 or more (and even quartics that do not split over a real quadratic field) to it when the coefficients
  are rational, instead of giving up.
- `mc-calculus/src/diff.rs`: `d(root_sum(p, t => g), x) = root_sum(p, t => d(g, x))` for `p` free of `x` (the
  summand's body is the only child visited, since `fn` terms are not differentiable); 30 lines. Without it `d` of
  every new antiderivative would stay unevaluated.
- registrations: `mc-engine` (install line, `FUNCTIONS`, Cargo), `mc-docs` catalogue topic "Algebraic numbers",
  `mc-lsp` library list, help index regenerated. Cargo lines in `mc-calculus`, `mc-linalg`, `mc-docs`, `mc-lsp`.
- docs: new pages `minimal_polynomial`, `to_number_field`, `root_sum`, `to_radicals`, `root_reduce`,
  `algebraic_number_norm`, `algebraic_number_trace`; `factor` (the option, five
  examples), `integrate` (one item, three examples). `mc-calculus/src/integrate/rational/radical.rs` (new), and
  `rational/mod.rs` tries the elementary methods before keeping a `root_sum` answer (decision F1.11).

## Modules

- `qpoly`: dense `Q[x]` (index = degree, trimmed); gcd through `mc-poly`'s integer modular gcd; factorisation over
  Q through `mc-poly`.
- `field`: `NumberField` (an `Arc` of the monic irreducible modulus, its primitive integer form, the power traces
  `Tr(t^k)` by Newton's identities, an optional embedding). Elements are trimmed `QPoly`s of degree below `d`, so
  equality is equality of vectors. Inverse by the extended Euclidean algorithm modulo `f`. `charpoly(a) = Res_t(f(t),
  x - a(t))` evaluated at `x = 0..d`, each value one modular integer resultant (`mc-poly`), then interpolated;
  `minpoly` its square-free part (the characteristic polynomial is a power of the minimal polynomial); `norm` a
  single resultant; `trace` a dot product with the power traces.
- `embed`: root isolation of a square-free `f`: Ehrlich–Aberth in ball midpoint arithmetic at a working precision
  doubling from 64 bits, then the inclusion disk `d |f(z)|/|f'(z)|` per approximation with balls (Henrici);
  pairwise disjoint disks hold one root each; a disk whose mirror image meets no other disk holds a real root; a
  conjugate pair is detected the same way and shares one real part. Order of root objects: real ascending, then by
  real and imaginary part. `enclose` refines one root by Newton inside its disk at a requested precision and checks
  that the new disk lies in the old one; `locate` finds the root a numeric enclosure lies in.
- `algnum`: `AlgNum` = field with embedding + element; enclosure by evaluating the element on the generator's ball;
  `sign` refines until the real ball excludes 0 (exact: an element that is not zero in the field is not zero);
  `adjoin` = primitive element of `K(β)`; `common_field` folds it; `simple` rewrites a number as the generator of its
  own field `Q(a)`.
- `kpoly`: polynomials over K; modular gcd (below), Yun's square-free decomposition, the norm by evaluation and
  interpolation, Trager factorisation with the product check.
- `convert`: term -> `AlgNum` (rationals, Gaussian rationals, `+`, `*`, integer powers, radicals of algebraic bases
  as principal values, root objects with multiplicity, `e^(r pi im)` for rational `r` as the root of unity `(-1)^r`), each atom placed by `atom`: factor its defining polynomial,
  choose the factor and the root with ball enclosures.
- `rootsum`: the `root_sum` native and `to_radicals`' helpers.
- `natives`: argument handling, the work budget, `factor`'s wrapper and the term printer for extension elements.

## Decisions

- **F1.1 Reading algebraic numbers** by atoms with certified enclosures: every atom (radical, `im`, root object) is
  placed as one root of one irreducible polynomial by ball enclosures, never by guessing a branch; a combination of
  atoms goes into one field by primitive elements. A term with any other kernel is not an algebraic number (the call
  stays unevaluated).
- **F1.2 Writing**: results keep the user's terms: elements of `Q(a, b, ...)` are written as rational combinations of
  products of the generator terms (a monomial basis chosen greedily by total degree, coordinates by Gaussian
  elimination), never as polynomials in an internal primitive element; factors over an extension are monic up to
  the rational multiple that clears their denominators.
- **F1.3 `root_sum` evaluation**: factor `p` over Q; linear and quadratic factors give radicals; for degree 3 or more
  the sum is exact when the summand is a rational function of the root and at most one other kernel (a trace, or a
  rational function of the other kernel from traces at `need + 1` integer points, interpolated, checked at one more
  point), else it stays as `root_sum` over that irreducible factor with a primitive integer polynomial. With
  parameters in `p` and a summand polynomial in the root, the sum is `sum g_j P_j` with the power sums `P_k` from
  Newton's identities (one fraction). Numeric
  values: the sum over `root(p, k)`; when `p` is real and the summand has no complex constants, an imaginary part
  whose ball contains 0 (machine: below `10^-12` relative) is dropped, since conjugate roots give conjugate values
  away from branch cuts, where the imaginary parts would not cancel and stay visible.
- **F1.4 Integration policy**: residues that are roots of linear and quadratic factors keep the existing real
  `log`/`atan` output; an irreducible residue factor `r` of degree 3 or more gives `root_sum`. With `v = gcd(d, a - α
  d')` over `Q(α)` (modular gcd): if `v` is linear, the sum is written over the roots `β` of the factor `q =
  minpoly(β)` of the denominator with the residue `a(β)/d'(β)` (classical form, `log(x - t)`), as the reduced
  quotient or the polynomial `c(t)` in `Q(β)`, whichever has fewer coefficient bits; otherwise over `r` with `t
  log(v(t, x))`. Cubic and quartic formulas only through `to_radicals`. Integrands with parameters and residues of
  degree 3 or more still stay unevaluated.
- **F1.5 Modular gcd over K** (Encarnación; Langemyr–McCallum): Euclid in `(F_p[t]/(f_p))[x]` for 62-bit primes,
  skipping primes where a leading coefficient is a zero divisor or a denominator vanishes, dropping images of higher
  degree, Chinese remaindering, Wang's rational reconstruction attempted at doubling intervals and accepted only
  when the candidate divides both inputs exactly over K; Euclid over K after 400 primes. Rational inputs go to the
  integer gcd.
- **F1.6 Factorisation** (Trager): shifts `s = 0, 1, -1, 2, ...` (0 skipped for rational inputs, whose norm is a
  power) up to 40; the norm factored over Q by `mc-poly`'s integer factoriser; every result multiplied out
  and compared with the input, `None` (unevaluated) if the check ever failed.
- **F1.7 Embeddings** are certified disks, not floating approximations: identity of a root never depends on a
  tolerance. The order of non-real roots compares machine values of the certified centres (with conjugate pairs
  sharing one real part exactly); two non-conjugate roots with real parts equal to machine precision are ordered by
  imaginary part, as `mc-solve`'s numeric roots are.
- **F1.9 The `extension:` option** is handled by wrapping `mc-poly`'s natives (`Kernel::wrap_native`): calls
  without the option go to them unchanged. Polynomials must have one variable (multivariate factorisation over
  number fields is not done: `failure(not_univariate, ...)`).
- **F1.10 Real radical fields in the integrator**: when the denominator factor behind an irreducible residue factor
  is a binomial `x^n - c`, `n` in 3, 4, 6, with a real root `ρ`, it splits over `Q(ρ)` into linear and quadratic
  factors, and its part is integrated over `Q(ρ)` by the existing Rothstein–Trager code with real `log`/`atan`
  output (`mc-calculus` `rational/radical.rs`): `Q(ρ)` is an exact real field for that code, signs decided by the
  embedding, square roots by factoring `y^2 - a` over it. `∫ 1/(x^3 - 2)` is written with `2^(1/3)` instead of a
  sum over roots.
- **F1.11 Old answers first**: when the rational method's answer contains `root_sum`, the elementary methods are
  tried as before (they ran whenever the rational method gave up), and a closed form they find is kept;
  so no previously evaluated integral changes.
- **F1.12 `normal` of a sum over roots** (a wrapper of `mc-poly`'s `normal`) writes it with root objects, `g(root(p, 1)) + ... + g(root(p, d))`; the one-argument `minimal_polynomial(a)` gives a slot function `&(p(&1))`.
- **F1.8 Bounded work**: every loop charges the evaluator's fuel (`Meter`), precision is capped at 2^15 bits for
  enclosures and choices, the modular gcd at 400 primes, Trager at 40 shifts, `root_sum` interpolation at 4000
  points; failures leave the call unevaluated.

## Checks, and how they were measured

(See the measurements section.) Results are verified: factorisations by expansion over K inside the algorithm;
minimal polynomials by evaluation on a 256-bit ball enclosure of the number before they are returned; antiderivatives
with `root_sum` differentiate back to the integrand exactly (`d` then `root_sum`'s exact summation), which the tests
check for every case. `root_sum` summands are kept small (one reduced quotient or one polynomial, the smaller by
coefficient bits).

## Tests

- unit (10): fields of `sqrt 2` and a cubic (inverse, traces, norms, minimal and characteristic polynomials,
  interpolation), root isolation of a quintic and of three real roots with refinement, gcd over `Q(sqrt 2)` and
  rational reconstruction, Trager on `x^4 + 1`, `x^2 - 2` and a product with multiplicities, primitive elements
  (`sqrt 2 + sqrt 3`, `2^(1/4)` with `sqrt 2`), the sign of `sqrt(10^12 + 1) - 10^6`.
- `tests/fields.rs` (5): 40 random modular gcds over fields of degree 2-5 equal to Euclid's; 18 random products
  over three fields factored and multiplied back, `x^8 - 1` over `Q(i)` with the right degrees; 12 random minimal
  polynomials vanish on ball enclosures and are irreducible; exact signs near a decimal approximation of
  `sqrt 2 + sqrt 3`. A real number in a complex field (`2 cos(144 degrees)` from a fifth root of unity) has an exact sign;
  exact comparison and equality.
- `tests/session.rs` (5): the natives through the engine (minimal polynomials incl. a zero difference of nested
  radicals and the vanishing check of six more at 50 digits; number fields; factorisations over seven extensions,
  each expanded back; `root_sum` exact sums, numerics, the derivative, `to_radicals`). `root_reduce`, norms and traces,
  `polynomial_gcd` and `factor_square_free` over extensions, threading over lists, `normal` of a root sum.
- `mc-calculus/tests/integrate_rational.rs`: `root_sums_check_by_differentiation` (7 integrands checked numerically
  and by `together(d(F) - f) == 0`, the residue formula's form, the bound variable when integrating in `t`); the
  `declines` test expects `root_sum` for `1/(x^3 + x + 1)` and real `log`/`atan` terms in `2^(1/3)` for `1/(x^3 - 2)`;
  `real_radical_fields_check_by_differentiation` (6 integrands over `Q(c^(1/n))`, n = 3, 4, 6).
- doc pages: 7 new (`minimal_polynomial`, `to_number_field`, `root_sum`, `to_radicals`, `root_reduce`,
  `algebraic_number_norm`, `algebraic_number_trace`), 2 extended (`factor`, `integrate`); all checked by the doc test.

## Measurements

Release build, busy machine, per evaluation in milliseconds: the mean of ten evaluations with distinct variable
names (no memo hits) in a fresh session, minus the session start, best of three.

| Input | ms |
|---|---|
| `factor(x^4 - 10x^2 + 1, extension: [sqrt(2), sqrt(3)])` | 5.6 |
| `factor(x^8 - 1, extension: [sqrt(2), im])` | 5.5 |
| `factor(x^6 + 3x^4 + 3x^2 - 1, extension: 2^(1/3))` | 2.9 |
| `factor(x^5 - x - 1, extension: root(x => x^5 - x - 1, 1))` | 8.7 |
| `factor(x^9 - 3, extension: 3^(1/9))` | 45.7 |
| `factor(x^16 - 1, extension: [sqrt(2), im, sqrt(3)])` | 53.4 |
| `minimal_polynomial(sqrt(2) + sqrt(3) + sqrt(5) + sqrt(7), x)` | 26.6 |
| `minimal_polynomial(2^(1/3) + 3^(1/4), x)` | 10.6 |
| `integrate(1/(x^3 + x + 1), x)` | 0.8 |
| `integrate(1/(x^5 - x - 1), x)` | 1.4 |
| `integrate(1/(x^3 + 2x + 5)^2, x)` | 1.3 |
| `integrate((x^4 + 1)/(x^5 + 3x - 1), x)` | 5.7 |
| `integrate(1/(x^8 + x + 1), x)` | 3.0 |
| `integrate(1/((x^3 + x + 1)(x^3 - x + 1)), x)` | 1.7 |
| `integrate(x/(x^4 + x + 1), x)` | 1.9 |
| `integrate((x^2 + 1)/(x^7 + 2x + 3), x)` | 18.3 |

The `root_sum` summands are small: for `(x^2 + 1)/(x^7 + 2x + 3)` one quotient
`(1 + t^2) log(x - t)/(-19 - 7t^2 - ... + 7t^5)`; for `1/(x^8 + x + 1)` a single `log(x - t)/(degree-5)`.
Factorisation over extensions is fastest when rational parts can be factored over Q first (`x^16 - 1` over a
degree-8 field), slowest on the degree-9 radical field and the root-object field (Trager's norm is factored over Q
there). Minimal polynomials are the slow part: each atom is placed by certified root isolation and the tower built
by norms with numeric factor choice; the answers are checked in balls, but the tower construction is the obvious
place to speed up (open gap).

## Item F2: real roots over number fields, real algebraic numbers, faster minimal polynomials (round 16)

New modules `realroot`, `countroots`, `minpoly`; `NumberField` implements `mc_poly::ring::CoeffRing` (so the generic
subresultant chains, gcds and decompositions of `mc-poly` run over number fields). `mc-solve/src/realroots.rs`
re-exports this API (and keeps the integer Descartes names its solvers use, now from `mc_poly::descartes`), so the
phases above can reach it from either crate; the code lives here because the CAD and decision crates should not
need all of `mc-solve` (decision F2.10).

### API (for 11A: sample points and lifting; for F3: exact signs)

```rust
pub struct RealAlg { pub poly: Vec<Z>, pub iv: Interval }   // minimal polynomial over Q + isolating interval
impl RealAlg {
    fn rational(c: &Rat) -> RealAlg;  fn as_rat(&self) -> Option<Rat>;  fn degree(&self) -> usize;
    fn halve(&mut self);  fn refine(&mut self, width: &Rat, m) -> WorkResult<()>;
    fn ball(&mut self, bits: u64, m) -> WorkResult<Ball>;
    fn field(&self, m) -> WorkResult<Option<NumberField>>;      // Q(xi) with the embedding that is xi
    fn alg_num(&self, m) -> WorkResult<Option<AlgNum>>;
    fn sign_of(&mut self, p: &[Z], m) -> WorkResult<i32>;      // exact
    fn compare(&mut self, o: &mut RealAlg, m) -> WorkResult<Ordering>;   // exact
}
pub fn real_roots(k: &NumberField, p: &[Elem], m) -> WorkResult<Option<Vec<(RealAlg, u32)>>>;  // increasing, with multiplicity
pub fn rational_real_roots(p: &[Rat], m) -> WorkResult<Vec<(RealAlg, u32)>>;
pub fn isolate_balls(k: &NumberField, a: &[Elem], m) -> WorkResult<Option<Vec<Interval>>>;    // square-free a
pub fn sign_at_algebraic(k: &NumberField, q: &[Elem], xi: &RealAlg, m) -> WorkResult<Option<i32>>;
pub enum Sample { Gap(Rat), Root(RealAlg) }
pub fn sample_points(roots: &[RealAlg], m) -> WorkResult<Vec<Sample>>;  // gap, root, gap, ..., gap
pub fn simplest_between(a: &Rat, b: &Rat) -> Rat;
// countroots
pub fn real_alg_of(a: &AlgNum, m) -> WorkResult<Option<RealAlg>>;
pub fn count_rectangle(p: &[Rat], xs: (&Rat, &Rat), ys: (&Rat, &Rat), m) -> WorkResult<Option<u64>>;
```

Lifting over a sample `xi`: `let k = xi.field(m)?`, map the coefficients of `p(xi, y)` into `k`, `real_roots(&k, ..)`,
`sample_points` of the result. `None` means a precision or refinement cap was reached (bounded loops everywhere).

### User function

`count_roots(p, x)`, `count_roots(p, [x, a, b])`: real roots with multiplicity, in a closed interval (rational or
real algebraic ends, `-inf`, `inf`), or in a closed rectangle when an end is complex (rational coefficients).
Non-polynomial input stays unevaluated.

### Decisions

- **F2.5** Real roots over K: square-free factors over K, then Descartes' rule in **ball arithmetic** on the
  coefficients' enclosures (a sign variation counts only when every sign is certain; a test with one variation and
  no uncertain sign proves one root; precision doubled from 64 to 4096 bits). Each isolated root is then identified as
  the root of one irreducible factor of the norm `N_{K/Q}(p)` by refining it (exact signs of p at rational points in
  K) together with the candidates' intervals until one candidate meets it. When the balls cannot decide (a root at a
  bisection point, clustered roots), the **exact** method: for each irreducible factor `f` of the norm, `g =
  gcd_K(p, f)` has `deg g` roots among those of `f`; certified complex enclosures of `f`'s roots are refined until
  all others are excluded by ball evaluation of `g`. The two methods are cross-checked in the unit tests.
- **F2.6** Signs at algebraic points: balls at 64, 128, 256 bits; then the exact value in `K(xi)` (a primitive
  element from `algnum::adjoin`), whose sign is exact.
- **F2.7** Sample points: the simplest rational (smallest denominator, by continued fractions) between neighbouring
  roots after refining their intervals apart; integers beyond the ends.
- **F2.8** Rectangle counts: boundary roots exactly (real roots of `gcd(U, V)` with `p(c + it) = U + iV` on each
  side, vertical sides closed, horizontal open), interior roots by certified enclosures refined until only the
  boundary roots' enclosures stay undecided, so no count depends on a tolerance.
- **F2.11** `minimal_polynomial` of sums, products and rational powers by composed resultants (`Res_y(P(y), Q(z -
  y))`, `Res_y(P(y), y^n Q(z/y))`, `Res_x(P(x^q), z - x^p)`) with the factor chosen on a ball enclosure of each
  partial value; the number-field method stays as the fallback (atoms, or a degree above 256). The 256-bit vanishing
  check of the answer is kept.

### Measurements (release, busy machine; mean of ten evaluations with distinct variable names, best of three)

| Input | F1 (round 15 code, same run) | F2 |
|---|---|---|
| `minimal_polynomial(sqrt(2) + sqrt(3) + sqrt(5) + sqrt(7), x)` | 32.9 ms | 2.4 ms |
| `minimal_polynomial(2^(1/3) + 3^(1/4), x)` | 13.7 ms | 1.3 ms |
| `minimal_polynomial(sqrt(2) + sqrt(3), x)` | 2.1 ms | 0.41 ms |
| `minimal_polynomial((1 + sqrt(5))/2 2^(1/3), x)` | 4.0 ms | 0.66 ms |
| `count_roots(x^7 - sqrt(3) x^3 + 2^(1/3) x - 1, x)` | | 17 ms |

The minimal polynomials are 5-14 times faster than round 15 (the times include the session's per-evaluation
overhead). `count_roots` over a degree-6 field spends its time building the
field and the degree-42 norm.

### Tests

- unit `realroot` (2): real algebraic numbers (comparison, equality across refinements, exact signs, simplest
  rationals, sample points); roots over `Q(sqrt 2)` with multiplicities and minimal polynomials, ball method =
  exact method on a quintic, signs at `2^(1/4)` (zero and negative).
- `tests/f2_roots.rs` (1 + 1 ignored): 17 `count_roots` cases (polynomial examples with expected counts, algebraic
  coefficients and ends, rectangles with roots on the boundary and at corners); the timings above (ignored).

### Open gaps

- Multivariate factorisation over number fields (`factor(..., extension: ..)` with several variables) is not done.
- `root_intervals` and `isolating_interval` are not provided as user functions; the Rust API above gives certified
  intervals.
- Counting roots of sparse polynomials of huge degree (`x^1000000 - ...`) needs a sparse Descartes method; the dense
  reader declines degrees above 2000.

## Round 33

- `embed::rat_ball`: after a stop the quotient of the arithmetic kernel may fail; it no longer panics (the value is discarded by the caller, which sees the stop).

## Round 36

- `to_number_field(a, theta)` returns `algebraic_number(theta, coefficients)` (a rational stays rational);
  `to_number_field([a, b, ...])` and `[..., all]` return the list of the numbers as algebraic numbers over one
  primitive element (the generator is tracked as `beta + s*gamma`, the first irrational number generates alone).
- `to_alg` reads `algebraic_number(theta, [c0, ...])` terms; `add`, `mul` and integer `pow` of algebraic numbers of one
  generator are wrapped (`AlgebraicArithmetic`) and give the result in the same field.
- `polynomial_gcd` over an extension is monic (it printed the denominator-cleared multiple before); polynomials with
  an exact complex coefficient (`im`) are taken over `Q(i)` automatically; roots of rationals are not (they need
  `extension: automatic`).
## Round 36

- `to_radicals` of an expression with `root(p, k)` objects and no root sums hands it to `calc_to_radicals`
  (lib/calculus.mc), which replaces cubic and quartic root objects by the radical root nearest to the numeric value.
- `Tick::charge`: a raised stop flag sets the abort (see mc-cad NOTES).

## Round 36, follow-up

- `root_reduce` of an expression that is not an algebraic number reduces its algebraic parts (a polynomial with algebraic coefficients keeps its degree). `algebraic_number_norm`/`trace` and `minimal_polynomial` take `extension: b`: over K = Q(b) the norm is N(a)^[K:Q(a)], the trace [K:Q(a)] Tr(a), the characteristic polynomial p^[K:Q(a)] (Cohen 4.3); a number outside K is `failure(not_in_field)`.
- The prime loop of the modular gcd over a number field (`kpoly::gcd_modular`) polls the stop and charges each prime; after a stop the number kernels return arbitrary values, every prime could be skipped and the loop never ended (an interrupt at 3.0-3.3 s of `function_surjective` of a quintic with four parameters never returned). At most 8 x MAX_PRIMES primes are tried. Test: `an_interrupt_during_the_modular_gcd_of_a_lifting_stops` (mc-engine/tests/r36p2_calculus.rs).

- `to_number_field(a)` of one number is `to_number_field(a, a)`: `algebraic_number(a, [0, 1, 0, ...])` (stayed unevaluated, and `root_reduce` of it with it).

## Round 37

- `factor`, `factor_square_free`, `polynomial_gcd` with `gaussian_integers: true` are the calls with
  `extension: im`; `gaussian_integers: false` is the call without the option.
## Round 37

- `root_reduce` of an atom that is not an algebraic number (true, false, a symbol) is the atom (was unevaluated).
