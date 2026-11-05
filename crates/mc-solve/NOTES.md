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
eight systems with reference solution counts (4, 4, 6, 4, 6, 8, 2, 8 for Katsura-3), residual < 1e-9; parametric
residuals simplify to 0; 50 linear equations checked exactly; failures and interrupt. `tests/numeric.rs`: 15
`reduce` cases matching reference output, `n(reduce(...))`, root objects (40 digits of the first root of
x^3 - 2x - 5, 100 digits of sqrt 2, ordering), `n_solve` of x^7 - 3x + 1 to 20 digits.

## Measurements (busy machine, release)

50x50 linear system solved and checked exactly in about 0.14 s; Katsura-3 `solve` 0.3 s (the grevlex basis 0.3 ms;
most time in back-substituting degree-6/7 root objects); small systems milliseconds.

## Known gaps

- Ordering: `n_solve` lists real roots first, then complex (the reference sorts by real part); casus irreducibilis
  cubics come out as trig forms (the reference keeps root objects).
- Zero tests: closed algebraic numbers by the ball heuristic; with symbols, nonzero unless it expands to 0; no
  factoring over algebraic extensions.
- Quartic results are large; exponentials of different bases are not unified (`4^x - 2^(x+1) - 3` unsolved); no
  Lambert-W type equations; transcendental and inverted equations only for one unknown and one equation.
- Root numerics: many-digit real roots by exact bisection (O(p) evaluations); clustered complex roots rely on the
  f64 Aberth starts; coefficients beyond the f64 range are not scaled.
- FGLM only without parameters (with parameters lex directly, can be slow); no F4.
- Parametric degree >= 5: root objects that `n` cannot evaluate until the parameters have values.
- `reduce`: rational coefficients only.
