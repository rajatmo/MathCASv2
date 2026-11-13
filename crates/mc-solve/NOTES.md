# mc-solve: equation solving (phase 3B), with Groebner bases and FGLM in mc-poly

## API

`install(k, h, stop)`, `FUNCTIONS`:
- `solve(eqs, xs)` / `solve(eqs)` (all symbols) -> `[[x -> a, ...], ...]`;
- `roots(p == q, x)` -> `x == r1 or ...`, sorted, repeated by multiplicity;
- `n_solve(eqs, xs[, options(digits -> d)])`: one numeric polynomial directly, anything else `n(solve(...))`;
- `eliminate(eqs, xs)` -> `p == 0` equations joined by `and` (`true`/`false` at the extremes);
- `groebner_basis(polys, xs)` (other symbols are parameters);
- `reduce(cond, x[, reals])`;
- `root(x => p, k)` (also `root(p, k)` with a polynomial in one variable).

Rust: `mc_poly::groebner::{groebner, GPoly, MonomialOrder}`, `mc_poly::fglm::fglm`. Outside the crate: a test's user
function in `crates/mc-jit/tests/engine.rs` renamed from `roots` to `radicals` (it collided with the native).

## Modules

- `cx`: per-call context: evaluator fuel and interrupt bridged to mc-poly's `Meter`; substitution and rebuilding
  through `Arith`; containment test; `together`. `is_zero`: exact expansion as a rational function in its kernels
  (written back once, so `(2^(1/2))^2` is 2); for closed numeric terms complex balls, a term whose enclosure at 1024
  bits still contains 0 counts as zero (heuristic); machine terms 1e-9 tolerance.
- `univariate`: factor over Z in all kernels; a factor in `x^g` with reduced degree <= 4 solved for `y = x^g`
  (`y^(1/g) (-1)^(2j/g)`); degrees 1, 2 direct; cubic by Cardano (rational coefficients: casus irreducibilis as
  `2 sqrt(-p/3) cos(acos(3q/(2p) sqrt(-3/p))/3 - 2 pi k/3)`, one real root as a sum of real cube roots with the
  complex pair `-(A+B)/2 +- i sqrt(3)(A-B)/2`; otherwise the principal cube root C and `w^k C - p/(3 w^k C)`, valid on
  every branch); quartic by Ferrari (biquadratic when q = 0; else a root m of the resolvent cubic, rational when one
  exists, `s = sqrt(2m)`, roots `(+-s +- sqrt(-(2p + 2m +- 2q/s)))/2`); degree >= 5 root objects.
- `linear`: Bareiss fraction-free elimination over Z[parameters] with the simplest pivot, fraction-free back
  elimination with row gcds removed; values `(B - sum A_j x_j)/D` in lowest terms; columns without a pivot are free
  variables; a zero row with a nonzero right side means no solution.
- `system`: lex Groebner basis, back-substitution by Gianni-Kalkbrener (per variable from the last: substitute,
  trim leading coefficients that are zero, take the element of least true degree, solve with `univariate`, drop
  duplicates; no usable element = free variable; a nonzero constant = no extension of that branch).
- `solve`: reads lists, `and`, `==` chains, `true`/`false`, bare expressions (`== 0`); each difference as a rational
  function; drops solutions where a denominator vanishes; radical kernels `u^(p/q)` become unknowns with `y^q = u`
  (every candidate checked against the original); exponentials with rational exponent ratios become powers of one
  exponential; one kernel `f(u)` holding the unknown is solved for and inverted (exp, log, six trigonometric and
  hyperbolic functions and inverses, `b^x` with exact integer logarithms, `x^a`), principal values only, checked;
  closed numeric solutions sorted by machine value (real, then imaginary part).
- `realroots`: Descartes (Vincent-Collins-Akritas) isolation with exact rational intervals; a root at a bisection
  midpoint recorded exactly; exact bisection refinement.
- `numroots`: coefficients as exact rationals (machine reals and ball midpoints are dyadic); real roots exact
  (square-free decomposition, isolation, bisection; rigorous at ball precision); non-real roots by Aberth-Ehrlich in
  f64 (the d - r approximations of largest |imaginary part|), refined by Newton in complex balls with doubling
  precision, radius `d |p(z)|/|p'(z)|`; complex coefficients Aberth + Newton.
- `rootobj`: the `root` native: exact coefficients keep the object (degree 1 evaluates); inexact coefficients or
  index (what `n` produces) give the k-th root at the inputs' precision.
- `reduce`: real roots of every irreducible factor of numerators and denominators, refined until disjoint; each open
  cell decided at a rational sample by exact signs; at a root of f a polynomial is 0 if f divides it, else has the
  sign of the cell to its right; a zero denominator excludes the point; adjacent true pieces merge; roots written as
  rationals, quadratic radicals or `root(x => f, k)`.
- `mc-poly/src/groebner.rs`: `GPoly` over Z[parameters], `MonomialOrder::{Lex, DegRevLex}`, Buchberger with the
  sugar strategy and Gebauer-Moeller criteria, fraction-free reduction, primitive parts; reduced basis sorted by
  increasing leading monomial; every loop charges the `Meter`.
- `mc-poly/src/fglm.rs`: FGLM grevlex -> lex for zero-dimensional ideals without parameters (up to 2000 standard
  monomials), used automatically then (Katsura-3: 0.3 ms grevlex basis vs 0.8 s direct lex).

## Decisions

- **3B.1** `install` takes the heap and the arithmetic stop flag.
- **3B.2** Factor in all kernels; radical kernels act as independent variables while factoring (any factorisation
  found remains valid).
- **3B.3** Real-friendly cubic forms for rational coefficients; principal radicals otherwise.
- **3B.4** Root objects `root(x => p(x), k)`, k 1-based, real roots ascending, then non-real by real then imaginary
  part (the same form mc-linalg produces); `n` evaluates them.
- **3B.5** Linear systems by fraction-free elimination, not Groebner bases.
- **3B.6** Systems by Gianni-Kalkbrener; generic solutions (a nonzero polynomial in the parameters is assumed
  nonzero).
- **3B.7** Real roots isolated by Descartes' rule. **3B.8** Real roots exact or rigorous; complex enclosures with
  radius `d |p/p'|`.
- **3B.9** Solutions where a denominator vanishes are dropped.
- **3B.10** A single polynomial equation repeats roots by multiplicity; systems give distinct solutions.
- **3B.11** Transcendental equations: principal values only.
- **3B.12** `roots` gives the disjunction form.
- **3B.13** `reduce` writes `a < x and x <= b`; rational coefficients and one variable only.
- **3B.14** Failures: `failure(argument, f, msg)`, `failure(unsolved, f, msg)`, `failure(limit, memory)`; interrupt
  is the evaluator's abort.

## Tests (mc-solve 16, mc-poly +5)

Unit: Aberth/Newton on a quintic at machine and 50 digits, multiple roots; isolation incl. Wilkinson-8; Groebner
(circle and line, inconsistent, cyclic-3, parameters); FGLM vs direct lex on Katsura-3 and `None` for a
positive-dimensional ideal. `tests/solve.rs`: exact printed forms (quadratic, casus irreducibilis, quintic, linear,
parametric, inconsistent, underdetermined, radicals, rational equations); transcendental and radical equations;
`roots`, `eliminate`, `groebner_basis`; 60 random integer polynomials of degree 1-6 (count = degree, residual < 1e-8);
eight systems with known solution counts (4, 4, 6, 4, 6, 8, 2, 8 for Katsura-3), residual < 1e-9; parametric
residuals simplify to 0; 50 linear equations checked exactly; failures and interrupt. `tests/numeric.rs`: 15
`reduce` cases with expected answers, `n(reduce(...))`, root objects (40 digits of the first root of
x^3 - 2x - 5, 100 digits of sqrt 2, ordering), `n_solve` of x^7 - 3x + 1 to 20 digits.

## Measurements (busy machine, release)

50x50 linear system solved and checked exactly in about 0.14 s; Katsura-3 `solve` 0.3 s (the grevlex basis 0.3 ms;
most time in back-substituting degree-6/7 root objects); small systems milliseconds.

## Known gaps

- Ordering: `n_solve` lists real roots first, then complex (sorting by real part would be more uniform); casus
  irreducibilis cubics come out as trig forms rather than root objects.
- Zero tests: closed algebraic numbers by the ball heuristic; with symbols, nonzero unless it expands to 0; no
  factoring over algebraic extensions.
- Quartic results are large; exponentials of different bases are not unified (`4^x - 2^(x+1) - 3` unsolved); no
  Lambert-W type equations; transcendental and inverted equations only for one unknown and one equation.
- Root numerics: many-digit real roots by exact bisection (O(p) evaluations); clustered complex roots rely on the
  f64 Aberth starts; coefficients beyond the f64 range are not scaled.
- FGLM only without parameters (with parameters lex directly, can be slow); no F4.
- Parametric degree >= 5: root objects that `n` cannot evaluate until the parameters have values.
- `reduce`: rational coefficients only.

## Round 16

- `domain.rs`: the conditions of `solve`'s solutions are decided by the decision service (`mc-decide`, one decider
  per call, so its memo is shared across solutions), with `refine` as the fallback for what only its rewritings
  decide.
- `reduce` over the signed real domains (`positive_reals`, `nonnegative_reals`, ...: `reals` and a sign
  condition per variable), and over the rationals and complexes for equations in one variable (through `solve` with
  the domain).

## Round 18 (roadmap 11C)

### Modules

- `front`: options (`cubics`, `quartics`, `generated_parameters`, `max_roots`, `inverse_functions`, `modulus`,
  `multiplicity`, `equated_to`, `working_precision`/`digits`, ...) parsed once into `Settings`, kept on a
  thread-local stack for the dynamic extent of the call so the domain layer's inner `solve` calls solve with the
  same settings (11C.1); vector and matrix equations to scalar ones; `solve_values` / `n_solve_values`.
- `order`: the order of solutions (products by their factors from the last backwards with the numeric
  coefficient last, sums by terms from the last, powers by base then exponent, numbers first; `i (-1)^r` compared as
  `(-1)^(r+1/2)`), applied after the values are evaluated (11C.5).
- `modular`: `modulus: n` by trying every residue point (at most 10^6), any modulus (11C.8).
- `plog`: `alpha(x) B^g(x) + beta(x) == 0` with g linear and R linear, or g = c x^n and R a monomial, through
  `product_log` (family `product_log(c(1), z)` over the complexes, W_0 / W_-1 over the reals) (11C.11).
- `nsolve`: n_solve's post-processing: W_k numerics (Halley), `c(1.0)` back to `c(1)`, families listed (16 members, in a
  fixed enumeration; `max_roots: inf` keeps them), real-part order, conjugate pairs (11C.13).
- `bounds`: integer bounds from equations `sum a_i m_i == C` of nonnegative monomials, for the box search.

### Decisions

- **11C.1** settings by dynamic scope (above). **11C.2** `Cfg` in `Env` (cubics, quartics, periodic, reals).
- **11C.3** a polynomial in x^g is solved for y = x^g first (roots of unity times radicals).
- **11C.4** numeric irreducible cubics and quartics give root objects unless `cubics`/`quartics` (biquadratic
  quartics and symbolic coefficients by radicals); `roots` defaults to radicals; Cardano in a fixed printed form (`univariate.rs`).
- **11C.5** order (above). **11C.6** every solution is checked by substitution (exact expansion, complex balls at
  256/1024 bits, else at three samples of the parameters and generated parameters); a solution is dropped only when
  every possible check refutes it (generic solutions on a branch cut at one sample stay).
- **11C.7** `roots` order: factors of degree > 1 first, then linear factors by their root; binomial blocks as
  generated; inexact coefficients by real part.
- **11C.9** inversion with generated parameters (`c(k)`, k one more than the largest in the value), conditions
  `element(c(k), integers)`; over the reals real branches only; duplicate families removed numerically.
- **11C.10** inverse functions: value under the range condition (`-pi < im(a) <= pi` for `log(x) == a`), decided
  numerically for numbers (a condition rather than a warning).
- **11C.12** over the reals with square roots of parameters: `conditional_expression(expand(v), c)`, c from
  `reduce(radicands > 0, params, reals)` (generic: strict).
- **11C.14** depth-two denesting of square roots. **11C.15** Rabinowitsch variable for `!=` in underdetermined
  systems (eliminated first).
- Integer equations without a finite box go to `reduce(..., integers)` (11B's public entry), the answer as rules.
- `MAX_FACTOR_DEGREE` 5000: beyond it `failure(limit, memory)` at once (was a hang on `x^1234567 ...`).

### Checks

- solutions verified before they are returned; inverse-function warnings replaced by conditions; modular roots for
  any modulus; `n_solve` evaluates `product_log(-1, z)` (the W_-1 branch) where the engine cannot.

### Gaps

- root objects print as `root(x => p, k)`, while the slot form `root(&(p), k)` is not read back (3-argument
  `root(f, k, 0)` fails); changing the canonical form touches `mc-algebra`, `mc-linalg`, `mc-cad` and their docs.
- `normal` does not strip `conditional_expression` inside rule lists (mc-decide); Pell equations wait for 11B's
  integer `reduce`; `mc-special` lacks numerics for `product_log(k, z)`, k != 0 (worked around in `nsolve`).
- n_solve: no complex root search in regions, no real-line scan of transcendental equations without closed form.

## Round 19: root objects (decision i19.root)

The canonical form of a root object: it prints as `root(&(p(&1)), k)` (the printer, `mc-syntax`, renames any bound
variable to the slot, so objects from every library print alike and compare equal); `root(f, k, 0)` is accepted
(a third argument 0: the root is exact); with exact rational coefficients the k-th root (root order) is located in its
irreducible factor: degree 1 gives the rational, degree 2 `(-b +- sqrt(b^2 - 4ac))/(2a)`, higher degrees
`root(&(q(&1)), j)` of the primitive factor `q`.

## Round 19 fixes

- **f19.1** `solve` (Q19-18): `abs(u) == v` inverts to `u == ±v` for `v >= 0` (a condition for symbolic v); a sum
  of logarithms with rational coefficients plus a constant becomes one rational equation (`prod u_i^(D a_i) ==
  e^(-D r)`), each solution checked against the given equation (the root of the product with a log on another
  branch is refuted).
- **f19.2** `reduce` (Q19-37, Q19-38, Q19-06): equations alone in one variable without a domain are over the
  complexes (the default domain for equations; `x^2 + 1 == 0` was `false`); one variable with `abs`, radicals or other
  non-rational functions goes to the decomposition's case split (`reduce(abs(x - 1) < 2, x, reals)` was a failure);
  `modulus: n` gives the solutions of `solve(..., modulus: n)` as a disjunction.
- **f19.3** `n_solve(eq, x, max_roots: k)` (Q19-32): fewer exact roots than k are completed by Newton's iteration in
  machine complex arithmetic from rings of starting points, residual-checked, in increasing modulus.

## Round 20 (roadmap 16A)

- `reduce_cx::complex`: the checks of the cases at their points (CAD.22) run in parallel (`par_until`, the first
  failing case declines); each task makes its own `Env` with the call's settings and parameter head (the
  thread-local solver settings are not seen on other threads).

## Round 22 (bug fixing)

- D21-06: `plog` now takes every `mm`-th root of `D` in `x^mm e^(c1 L x^n) = D` (the equation `x^2 == 2^x` is
  `x e^(-x L/2) = +-1`): the sign of the second root gives the negative root; the real branches `W_0` and `W_-1`
  are used over the complexes too (every solution is still checked by substitution).
- ui B21-3: an expression that is no relation or logical formula in the conditions of `solve` (with
  `assumptions:` or inside `assuming`) stands for `expression == 0`, so the assumptions filter it.
- engine B21-27: `x*exp(x) == 0`: the numerator of an equation in `x` and one kernel is factored
  (`mc_poly::factor`) when `plog` does not apply, and the factors are solved one by one, each solution checked
  against the equation (`factored_kernel`); `erf(u) == v` and `erfc(u) == v` invert by `inverse_erf`,
  `inverse_erfc` (a real `v` outside the range has no solution).
- D21-11, engine B21-28: `reduce_fn.rs`, `reduce` with one function of the unknown (`exp`, `b^x`, `log`, `sin`,
  `cos`, `tan` of `x`). Atoms in `x` alone go through the rational algorithm (with a linear fallback for closed
  constants such as `x < 2 pi`), atoms in `y = F(x)` through the same algorithm in `y` and are mapped back through
  the monotone branches of `F` (`k pi + (-1)^k asin(y)` for `sin`, ...); interval sets with exact end points
  ordered by machine value are combined by `and`, `or`, `not`. A periodic function needs bounds from the
  top-level conjuncts (at most 64 pieces). Mixed atoms (`x < sin(x)`) are not handled. Literature: the inverse
  functions of Abramowitz and Stegun 4.4.

## Round 24

- **reduce with powers of one exponential (D23-25)**: `exponent_family` writes `b^(c x)` with rational c as integer
  powers of the one kernel `b'^x` (`exp(x) + exp(-x) > 3`, `exp(2x) - 3 exp(x) + 2 < 0`); the power is built as a plain
  call, since the arithmetic folds `(e^x)^-1` back into `e^-x`.
- **solve of products of functions of the unknown (D23-25)**: `factored_kernels` factors the numerator over all
  kernels at once (`sin(x) cos(x) == 0`), solves each factor and keeps the solutions that pass the check.
- **Huge degrees (D23-23)**: a power with an exponent beyond 100000 in `solve`, `roots`, `n_solve`, `reduce` is left
  unevaluated at once (it reached the memory cap). `reduce(x^1000 - 1 > 0)` (D23-22) took 60 s in the factorisation:
  see mc-poly (cyclotomic factors).
- Deferred: B23-15 (`x exp(x) > 1`, `exp(-x^2) > 1/2`: mixed atoms need certified real-root isolation of
  transcendental functions), B23-16 (`x log(x) == c`, `exp(x)/x == c`: product_log normal forms for these shapes),
  `reduce(exp(x) > a)` for a symbolic a, R22-15.

## Round 25

- **B23-15, sign chart (`chart.rs`).** `reduce` of one real unknown with atoms that mix x with `exp`, `log`, `atan`
  and powers (the kernel algorithm of `reduce_fn` takes one function of x on its own) now falls through to a sign
  chart: the real zeros of each atom (`solve` over the reals, and a numerical scan of [-40, 40] whose zeros must be
  exact rationals: the double zero of `exp(x) - x - 1`), the truth of the whole condition between the zeros and at them
  by machine evaluation (an atom that is not real at a point is false: `log` restricts to its domain), written
  as intervals with the exact zeros as end points (`product_log(1) < x` for `x exp(x) > 1`). The claim is
  checked against the condition itself at 400 points around the zeros and at both sides of each zero; a
  mismatch, an atom that is never a real number (a symbolic parameter: `exp(x) > a`), more than 24 zeros or a zero
  with no exact form gives the failure of before. Lambert W leaves (`product_log(z)`, `product_log(-1, z)`)
  are valued by Halley's iteration (`chart::lambert`, Corless et al. 1996 section 5) because the machine
  evaluator does not know the second branch. `Env` carries the stop flag so a second environment with real
  branches can be made.
- **B23-16, Lambert forms.** `x log(x) == c`, `log(x)/x == c`: `x = e^t` turns log(x) into t (`solve.rs`, the
  kernel `log(x)` with x outside); `(a1 x + a0) B^(c1 x + c0) == b` with a0 not 0: `u = a1 x + a0`,
  `u e^(gamma u) = D`, `x = (W(gamma D)/gamma - a0)/a1` (`plog.rs`, the shift made explicit; all branches as for the
  other linear forms). `x exp(x) + x == e` is not a Lambert form and stays unsolved.
- The test of B23-15 had `exp(x) - x > 1` false at -1/10 (it is true: e^-0.1 + 0.1 = 1.0048); corrected.

## Round 27

- `solve` of a disjunction (alone or under `and`): disjunctive normal form (at most 64 branches), each branch solved with the
  domain and the conditions, the union sorted (X26-11, R25-06).
- `reduce(cond in re(z), im_part(z), z)`: the parts become real variables, the region is reduced over the reals and written
  back in `re(z)` and `im_part(z)` (R25-07).

## Round 29
- `reduce(cond, x, d)` for a signed rational domain d (positive, negative, nonnegative, nonpositive rationals):
  `reduce` over the rationals, then the sign condition intersected over the reals (`domain.rs`, `signed_rationals`).

## Round 36

- `Tick::charge` and `reduce_cx` treat a stop flag raised with the interrupt as the abort (see mc-cad NOTES).

## Round 36

- `groebner_basis(polys, xs)` is the basis of the ideal of Q[xs, other symbols] (other symbols ranked after xs, `parameter_variables:` last), so the elements free of xs generate the elimination ideal (Cox, Little, O'Shea 3.1); without symbolic parameters the old lex path (FGLM). `solve` reads `element(points, region)` as reduce does; families are expanded over a finite strip of re(x) and by scanning c(1) outwards (ends by distance, non-real runs or 20 false conditions); a condition undefined at a solution drops it; numeric roots within 1e-7 of an exact one are duplicates. `reduce` over positive/negative/nonnegative/nonpositive integers adds the sign conditions; integer `false` with symbolic parameters declines.
- An order relation on an unknown (0 < x) makes the inner solve real: only the real branch of acoth(c) + i pi c(1).
- Cardano: 1/C is (S/2)^(-1/3) for a negative real S; (2/S)^(1/3) differs there by a cube root of unity (roots of x^3 - 2x + 7 were no roots).
- Conditions of `solve`: an order relation (`<`, `<=`, `>`, `>=`) whose sides differ by a number with a nonzero imaginary part (machine value, above 1e-10 relative) is false at that solution: the relation makes the unknown real. `solve(x^2 == -1 and x > 0, x)` is `[]`, `solve(x^3 == 1 and x > 0, x)` is `[[x -> 1]]`; `re(x) >= 0` still leaves complex roots. Test: `an_order_relation_filters_non_real_roots` (mc-engine/tests/r36p3_symbolic.rs).

## Round 37

- `groebner_basis(polys, xs, elims, monomial_order: o, modulus: p, coefficient_domain: rationals, method: m)`:
  `groebner_general` builds a `MatrixOrder` (lex default, `degree_lexicographic`, `degree_reverse_lexicographic`,
  `elimination_order` = degrevlex block of elims then degrevlex block of the rest, or a weight matrix on xs with a
  lex block for the other variables) and returns the elements free of `elims` (the elimination ideal).
  `coefficient_domain: integers` (strong bases over Z) and `rational_functions` (its normalisation is not settled yet)
  still give an argument failure. `method:` is ignored: the
  reduced basis does not depend on it.
## Round 37: more inverses in `solve`

- `csch(u) == v`, `sech(u) == v` (sinh and cosh of 1/v: the real branches over the reals, the 2 pi i families
  otherwise; v = 0 has none); `acsch`, `asech` as principal inverse functions with the ranges of asinh and acosh
  (acsch(z) = asinh(1/z), asech(z) = acosh(1/z)); `gudermannian` over the reals (a bijection onto (-pi/2, pi/2)) and
  `inverse_gudermannian` of a real value; `logistic_sigmoid(u) == v` as u = log(v/(1 - v)) (period 2 pi i over
  the complexes; 0 and 1 are not values); `sign(u) == 0` and `real_sign(u) == 0` as u = 0 (their other values
  are taken on half-lines). Every solution is still checked by substitution. Not done: the complex solutions of
  `gudermannian(u) == v` (the call keeps declining without `reals`).

## Round 38: integer families with a parameter

- `solve(..., integers)` takes the answer of `reduce` over the integers also when it is a family with a parameter:
  `common and (d1 or d2 ...)`, each `d_i` the equations `x == v` of every unknown with conditions free of the
  unknowns (the Pell family `x + y sqrt(d) = +-(x1 + y1 sqrt(d))^c(1)`, `element(c(1), integers)`, `c(1) >= 1` under
  positivity). The conditions go on each value as `conditional_expression` (`condition_as_conditional_rules`).

## Round 39: integer roots of sparse polynomials

- `reduce(p == 0 (and conditions), x, integers)` (and the signed integer domains) for a polynomial with few terms of
  any degree: each factor as written alone, x^k divided out (root 0), then the rational root theorem: a root d
  divides the constant term. A candidate is no root when one term outweighs twice the sum of the others (bounds on
  log2 |a_i| + e_i log2 |d|, from the bit lengths), or when the value is not 0 modulo 2^31 - 1 or 2^61 - 1
  (power_mod); otherwise p(d) is computed exactly when it has at most 200000 bits, else no answer. The terms are read
  from the expanded factor directly (coefficient_rules is dense in the degree). Used for degrees above 60, and for
  equations above the degree limit of the other methods (x^999997 - 25 x^12345 + 24 x, a millisecond).
