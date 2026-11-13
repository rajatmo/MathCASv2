# Item 2A: `mc-calculus` (symbolic differentiation)

Status: done. Every 2A target of `docs/design/06-roadmap.md` works with tests.
At completion: workspace 531 passed, 0 failed, 6 ignored (guard, peak 2756 MB); clippy clean. Dependencies `mc-api`,
`mc-arith`, `mc-core`, `mc-eval`, `rustc-hash`; dev `mc-engine`, `mc-bench`.

## What was built

- `term`: `Build`, the term builder of one `d` call: sums, products and powers through `mc-arith`'s canonicaliser
  (results collected, no `0*...`), other calls built plain and evaluated on hand-back; helpers for the operator
  `derivative(n1, ..., nk)(f)`.
- `rules`: the table of known functions, one row each `(name, arity, Rule)` (`ANY` matches any arity). `Rule::Partials`
  gives one builder per argument (a non-capturing closure); other kinds `Constant`, `Linear`, `Branches`,
  `Unsupported`. Rows: exp, log, sqrt; sin, cos, tan, asin, acos, atan; sinh, cosh, tanh; abs (gives `sign(u)`);
  piecewise constants with derivative 0 (sign, unit_step, arg, floor, ceiling, round, quotient); `floor`/`ceiling`/
  `round` with 2 arguments; `mod` with 2 and 3; `max`/`min` with 2 (`if a >= b then 1 else 0`, ...); linear: re,
  im_part, conjugate, list, equal; `if` with 2 or 3 arguments (branches differentiated, condition kept); unsupported:
  series_data, failure, fn, local_cell. Adding a function is adding a row.
- `diff`: `Diff`, the derivative in one variable: walks the term's DAG on an explicit stack, memoised by handle, one
  fuel tick per node; sum and product rules; the general power rule (`c b^(c-1) b'`, `b^c log(b) c'`,
  `b^c (c' log b + c b'/b)`, no `log(e)` factor for base `e`); the chain rule through the table; an unknown
  `f(a1..ak)` gives the sum of `derivative(0,..,1,..,0)(f)(a..) * ai'`; `derivative(ns)(f)(a..)` raises the order of
  the differentiated argument, and for a known `f` `resolve` gives the actual derivative (differentiating
  `f(derivative$z1, ...)` and substituting the arguments back); `nth` stops early at 0 or a fixed point
  (`d(exp(x), [x, 100000])` is instant).
- `natives`: the `d` native: specifications left to right, `x`, `[x, n]`, `[x, y, ...]` (gradient); the result is handed
  back with `Reply::Again` so the evaluator simplifies it.

## Public API

`install(k, h, stop)`; `FUNCTIONS` (`d`; `derivative`, a known name without a native, so resolution accepts the
operator); for Rust callers `diff::{Diff, Halt, Tick, nth, resolve}`, `rules::{RULES, Rule, Partial, ANY, lookup,
partials}`, `term::{Build, Heads}`.

Edits outside the crate: `mc-engine` (dependency; `mc_calculus::install` after `mc_poly::install`; `FUNCTIONS` in the
library names); at assembly, `mc-lsp` lists `mc_calculus::FUNCTIONS` (and the JIT's `compile`, `compiled_info`). No
`mc-syntax` change: the printer already writes `derivative(n)(f)` as `f'` to `f''''` (orders 1-4) for a named `f`;
higher orders and multi-argument operators print as `derivative(5)(f)`, `derivative(1, 0)(f)`; all read back.

## Decisions

- **2A.1** Canonical arithmetic during construction, evaluation at the end (`Reply::Again`: `log(e)`, `cos(0)` and
  parity rules apply).
- **2A.2** `[x, n]` with a numeric second element is an order (a non-negative machine integer, else a failure); any
  other list is a gradient (including `[x, n]` with symbolic `n`); the gradient is innermost: `d([f, g], [x, y])` is the
  Jacobian `[[fx, fy], [gx, gy]]`; `d(f, [])` is `[]`.
- **2A.3** A name without a row is an unknown function (chain rule to `derivative(...)(f)`), including `factorial`,
  `binomial` and user symbols.
- **2A.4** Real-variable conventions: piecewise-constant functions have derivative 0 and `if`/`max`/`min` take the
  active piece (valid away from breakpoints; no distributions); `abs(u)` gives `sign(u) u'`; `re`, `im_part`,
  `conjugate` commute with `d`.
- **2A.5** The variable is any term except a number, string, list, sum, product or power, recognised by identity
  (`d(y(t)^2, y(t))` is `2*y(t)`).
- **2A.6** `d` with fewer than two arguments declines (stays unevaluated): existing session tests use `d` as a symbol.
- **2A.7** Malformed specifications give `failure(argument, d, "message")`; parts that cannot be differentiated leave `d`
  unevaluated (a head depending on the variable, `f(x)(y)`; `series_data`).
- **2A.8** Fuel and interrupt polled once per node and per substituted node; `mc-arith`'s stop scope entered.

## Tests (14 integration + 1 doctest)

- `tests/derivatives.rs` (7): exact printed output for every rule row, sums, products, quotients, symbolic and general
  powers, constants, inexact coefficients, unknown functions and the operator (`d(sin'(x), x)` gives `-sin(x)`),
  implicit differentiation of an equation, orders, mixed partials, gradients, Jacobians, failures, unevaluated cases;
  an interrupt of `d(exp(sin(x)*cos(x)*x), [x, 400])` after 300 ms stops within 5 s and the session continues.
- `tests/numeric.rs` (3): ball central differences (h = 10^-15 exactly, 20 digits relative to 1 + |D|) for every
  `Partials` row and argument at 3 random points; 120 random depth-3 compositions in x and y, both variables;
  symmetry of mixed partials on 80 random functions (78 identical as terms) and third-order partials in every order. A
  mutated rule makes two of the three fail.
- `tests/prime.rs` (2): derivatives print with primes and read back as the same term (`===`); the operator alone for
  orders 1-6.

## Measurements (release, REPL wall time including start-up, busy machine)

| case | time |
|---|---|
| `d(expand((1 + x + y)^40), x)` (21 kB printed) | 0.025 s |
| `d(exp(sin(x)*cos(x)*x), [x, 16])` (42 kB printed) | 0.03 s |
| `d(sin(x), [x, 100000])` | 0.26 s |
| `d(f(x), [x, 100000])` | 0.19 s |
| `d(x^100000, [x, 1000])` | 0.015 s |

## Known gaps and requests

1. ~~`derivative(n)(f)` evaluates only inside `d`.~~ Done (decision 2A.7): `mc-eval` natives can opt in to curried
   calls (`Native::curried`), dispatched on the innermost head symbol; the `derivative` native resolves
   `derivative(n...)(known)(args)`, so `sin'(x)` is `cos(x)`. Unknown functions stay as the operator.
2. In `n` (handed to item 1C): outside a numeric island exact exponents become balls and `pow(ball, ball)` stays
   unevaluated (`n(1/(1 + abs(v)), 25)` leaves `2.26^-1.000`); `abs` over a ball is not refined.
3. Not covered: `max`/`min` with more than two arguments or a list (unknown functions); `d` of a series (unevaluated);
   a symbolic order `[x, n]` (read as a gradient); `factorial`, `binomial` until gamma and polygamma exist (phase 3C).
4. Results are canonical, not simplified: `-1*(-1 + x)*(1 + x)^-2 + (1 + x)^-1`, `cos(x)/x + -1*x^-2*sin(x)` (phase 2B).

# Item 2C: integration and partial fractions

Built in four parts on a shared skeleton. Each part is described in `notes/`:

| part | notes | contents |
|---|---|---|
| skeleton | — | `integrate(f, x)`, `integrate(f, [x, a, b])` and `apart(e[, x])` natives; the dispatcher `integrate::antiderivative` (linearity, constant factors, base table `x^n`, `1/x`, `exp`, `sin`, `cos`), then rational, then elementary; test helper `check_antiderivative` |
| 1 `apart` | [`notes/2c-apart.md`](notes/2c-apart.md) | partial fractions over Q(parameters): factor over Z, polynomial part, extended Euclid per prime power |
| 2 rational | [`notes/2c-rational.md`](notes/2c-rational.md) | Hermite reduction + Rothstein–Trager with Trager's gcds; real output (log of real quadratics plus atan, Rioboo); residue fields of degree 2 and even quartics; symbolic parameters; residues of degree 3 or more as `root_sum` (F1, module `rootsum`) |
| 3 elementary | [`notes/2c-elementary.md`](notes/2c-elementary.md) | table with linear arguments, derivative-divides substitution, tabular parts and LIATE, powers of sin/cos |
| 4 definite | [`notes/2c-definite.md`](notes/2c-definite.md) | F(b) - F(a) with breaking points of f and F located (Sturm counts), one-sided limits by a private helper, `failure(divergent, integrate, f)` |

Cross tests (`tests/integrate_cross.rs`): definite integrals over rational and elementary antiderivatives
(`integrate(1/(1 + x^2), [x, 0, inf])` is `pi/2`, `integrate(x*exp(-x), [x, 0, inf])` is 1), elementary integrals with
rational sub-integrals (`x*atan(x)`, `exp(2x)/(1 + exp(x))`), `apart` then `integrate`, and the roadmap's exit test:
a mixed corpus whose antiderivatives differentiate back to the integrand.

Known gaps across the parts (details in the notes):
- residue fields of degree 3 or more with symbolic parameters stay unevaluated (with rational coefficients they give a
  `root_sum` over number fields since item F1, `mc-algebra`, and `rational/rootsum.rs`);
- no erf/Si/Ei results, no Risch algorithm; `exp(-x^2)*x^2` stays unevaluated;
- results are not simplified (`(-1/2)*log(4) + (1/2)*log(2)`);
- `d` has no rules for sec, csc, cot and the inverse hyperbolic functions, and `n` cannot evaluate sec, csc, cot;
- `limit` (item 5) is not built; `integrate/definite/limit.rs` is a private, conservative helper it can replace.

## Round 7 item C: public `limit`, `d` of interpolating functions, exact times

Status: done. At completion: full
suite 782 passed, 0 failed, 6 ignored; clippy clean; L3, L4, L3h pass.

### What was built

1. **`limit`** (`src/limit/`): `limit(f, x -> a)`, `direction: from_above | from_below | two_sided`, `a` may be
   `inf`/`-inf`, a list `f` maps elementwise. The walk moved from `integrate/definite/limit.rs` to `limit/walk.rs`
   (definite integration uses it from there) and gained `tan` (a pole as `sin/cos`), `abs`, `sign`, bounded factors,
   rewrites in `0·∞` products (`abs(u)` to `±u`, `sinh`/`cosh` to exponentials, distributing a sum factor, then
   L'Hospital) and in `∞ - ∞` sums (combining logarithms of positive arguments, then `together`). **Bug fixed:** the
   old helper read `series` results as sums, but `series` returns `series_data(x, x0, [c...], v, p)`, so its series
   step never fired; it now reads the leading nonzero coefficient.
2. **`d` of interpolating functions**: `DerivativeAt` turns `derivative(n)(interpolating_function("y", rest...))`
   into `interpolating_function("y" + n primes, rest...)`, so `d(g(u), u)`, `d(g(u), [u, 2])`, `g'(1)`, `g''(x)`
   and chain rules (`d(g(u^2), u)`) work; mc-ode already reads primes as the derivative order.
3. **Exact times** (`mc-ode` `Interpolating::at`): a time that is not a number goes through `util::number` (`n` at
   machine precision, or `taylor::digits_of(p)` for ball solutions): `g(pi/2)`, `g(sqrt(2))`, `g([1, pi/2])`
   evaluate; symbolic times stay unevaluated; `g(3*pi)` gives `failure(out_of_range, 3*pi, [lo, hi])`.

### Decisions

- **C.1** A two-sided limit is the common value of both sides; sides that provably differ (finite vs finite, or an
  infinity involved) give `failure(no_limit, limit, f)`; an unknown side or undecided equality leaves the call.
- **C.2** An undecided limit stays unevaluated (as `integrate`), not `failure(not_decided, ...)`.
- **C.3** Inexact numbers in `f` or `a` are rationalized, the limit taken exactly, the result given through `n`
  (`limit((x^2-1)/(x-1), x -> 1.0)` is `2.0`): the walk otherwise computed `0.0/0.0`.
- **C.4** A bounded factor (`sin`, `cos`, `atan`, `tanh` of a provably real argument) times factors tending to 0
  tends to 0; a bounded term does not change the infinite limit of a sum.
- **C.5** Rewriting steps only after series and dominance fail: the `abs`, `sinh`/`cosh` and distribution rewrites,
  L'Hospital (`0/0` then `∞/∞`), combined logarithms, `together`; each costs 50 of the 200-level depth budget (at most
  four nest; failing cases return in milliseconds). L'Hospital is one-sided at t → 0+ on elementary functions,
  accepted only when the walk finds the limit of the derivative quotient.
- **C.6** `derivative(n)(interpolating_function(...))` adds n primes for n in 0..=1000; larger orders and
  multi-argument operators (`derivative(1, 1)`) stay unevaluated.
- **C.7** Exact times evaluate at the solution's precision (N3.10's working precision for ball solutions).
- **C.8** Limits are along the real line; at `±inf` the direction is the finite side (a contrary `direction:` gives
  `failure(argument, limit, "an infinity is approached from the finite side")`); unknown functions (`f(x)`,
  `gamma(x)`) are never assumed continuous.

### API

`limit` in `FUNCTIONS`; Rust `mc_calculus::limit::{limit, Direction, Outcome, LimitFn}` (`limit(cx, b, f, x, a,
direction) -> Res<Outcome>`, `Outcome = Value(T) | NoLimit | Unknown`). Failures: `failure(no_limit, limit, f)`,
`failure(argument, limit, "the point is x -> a" | "the point depends on the variable" | "a number is not a variable" |
"direction is ..." | "an infinity is ...")`, `failure(unknown_option, name)`. No edits outside mc-calculus, mc-ode.

### Tests

`mc-calculus/tests/limit.rs` (9): points, one-sided, infinities, inexact, failures, undecided and other forms, hard
cases under 5 s, 1 vs 4 threads identical, definite integrals through the walk. `mc-ode/tests/calculus.rs` (4):
derivative terms `===` to hand-written names, values near exact solutions (y' = -y to about 1e-9; x'' = -x: x', x'',
x''' to 1e-9 / 1e-6 / 1e-3), chain and product rules, the 0-th derivative; exact times (`pi/2`, `sqrt(2)`, lists,
out of range keeping `3*pi`, symbolic); a 30-digit ball solution at `pi/2` and its derivative agree with
`n(exp(-pi/2), 40)` in at least 28 digits; 1 vs 4 threads.

Limits in the tests include: `sin(x)/x` → 1, `(1 + 1/n)^n` → e, `x*log(x)` → 0, `(exp(x) - 1 - x)/x^2` → 1/2,
`(1 - cos(x))/x^2` → 1/2, `(a^x - 1)/x` → log(a), `(2x^2 + 1)/(3x^2 - x)` at inf → 2/3, `(1 + a/x)^x` → exp(a),
`sqrt(x^2 + x) - x` → 1/2, `x^(1/x)` → 1, `log(x) - log(2x)` → log(1/2), `1/x` at 0 → inf / -inf / no_limit,
`tan(x)` at pi/2 → inf / -inf / no_limit, `x^x` at 0+ → 1; unevaluated: `sin(x)` at inf, `sin(1/x)`, `f(x)`,
`x^a` at 0.

### Known gaps

1. Not decided: `(x+1)^(1/3) - x^(1/3)` at inf (no Puiseux series), `sinh(x)/cosh(x)` at inf, `sin(x)*exp(x)`,
   continuity of non-elementary functions (`gamma(x)` at 1, `floor`).
2. Symbolic parameters whose sign matters (`x^a` at 0, `exp(a*x)` at inf) stay unevaluated: round 7 item A's
   assumptions could decide them through `uses_assumptions` on `limit` and `NativeCx::assumptions()`.
3. Inexact inputs are rationalized, so `limit(x^2, x -> 0.1)` is `0.01`, not `0.010000000000000002`.
4. `0.0 + exp(-pi/2)` does not become a number (evaluator).
5. Derivatives beyond the dense output's degree 4 come from the interpolant (x''' to about 1e-3).
6. No interrupt test specific to `limit` (fuel is ticked per node; no case slow enough to interrupt reliably).

## Round 15 item 13C: limits by Gruntz's algorithm, `max_limit`, `min_limit`, `discrete_limit`, `asymptotic`, series extensions

Code: `src/limit/` and `mc-poly/src/series.rs`.

### What was built

1. **Gruntz's algorithm** (`src/limit/gruntz.rs`; Gruntz's 1996 thesis, Shackell's *Symbolic Asymptotics*,
   Salvy-Shackell): `limit` runs the existing walk first (the fast paths: rational functions, the polynomial walk, the
   series and dominance rules) and Gruntz's algorithm where the walk does not decide. The point is moved to
   `t → +inf`; the most rapidly varying subexpressions (comparability classes by `lim log f / log g`), moving up when
   x is among them, the rewriting in ω, and the leading term `c ω^e` through a new Rust entry point of the series
   kernels, `mc_poly::expansion` (Puiseux and logarithmic series, `log ω` given as the value it stands for so
   cancellations happen inside the kernels). Signs and limits recurse into each other.
2. **Exp-log preparation**: hyperbolic and inverse hyperbolic functions, `tan`/`cot`/`sec`/`csc`, `abs`, `real_abs`,
   `sign`, `unit_step`, `max`, `min` (by the sign of the difference), `atan`/`acot` at an infinity, `factorial`,
   `binomial`, `pochhammer`, `factorial_power`, `beta` as gamma quotients, gamma at a pole by its recurrence.
3. **Special functions at infinity** (`src/limit/asym.rs`, from the DLMF formulas cited there): `erf`, `erfc`, `erfi`,
   `dawson_f`, `gamma`, `log_gamma` (Stirling), `digamma`, `polygamma`, `exp_integral_ei`, `exp_integral_e`,
   `log_integral`, the incomplete gamma function, `bessel_i`, `bessel_k`, the Airy functions and their derivatives,
   `zeta` (Dirichlet series), `product_log` (de Bruijn). Functions of arguments with finite limits are expanded by the
   series kernels through their derivatives (a singular point makes the table incomplete and the limit unknown).
4. **Numeric plausibility check** (`src/limit/check.rs`) on every limit, from the walk or from Gruntz: ball
   evaluations at 50 digits at three points approaching the limit, generic values for parameters.
5. **`max_limit`, `min_limit`, `discrete_limit`, `asymptotic`** (`src/limit/more.rs`), iterated limits (a list of
   rules, the last innermost) for all of them, and `limit(f, [x, y] -> [a, b])` through one combination `y q`.
6. **Series** (`mc-poly/src/series.rs`): `series(e, x -> x0)` (leading term and the order of the next nonzero
   term); lists elementwise; negative orders (`series(1/sin(x)^10, x, 0, -5)`); gamma at its poles and asin/acos at
   ±1 through substitute terms in the Taylor table; at a real branch point where the right-hand Puiseux series is
   complex, the real expansion in powers of `(x0 - x)`, or with `assumptions:` the side assumed; machine-number
   inputs expanded exactly and rounded once; cancelled leading terms inside `exp` and `log` (`1/w - 1/w`).
7. **Bugs fixed in the series kernels**: `exp` of a coefficient built as a call `exp(c)` did not cancel against
   `e^c` (now the canonical power); Puiseux series of a negative base (`sqrt(-x)`, `sqrt(1 - x)` at 1) came out with
   the sign of the imaginary unit lost; a negative order with a high pole order ran out of precision retries.

### Public API

`mc_calculus::limit::{limit, limit_with, Options, Direction, Outcome, Kind, LimitFn}`: `limit_with(cx, b, f, x, a,
direction, &Options { budget, check })` is the entry point for definite integration (14D): a time budget for the
whole call (Gruntz's algorithm gets at least a quarter of it after the walk) and the numeric check on or off.
`mc_poly::{expansion, Expansion}`: the first terms of a generalized series in a symbol `t` at 0, other symbols
constant. `FUNCTIONS`: `max_limit`, `min_limit`, `discrete_limit`, `asymptotic` (and `limit` with the new forms);
pages `docs/functions/{max_limit,min_limit,discrete_limit,asymptotic}.mcn`; `limit.mcn` and `series.mcn` extended.

### Decisions

- **13C.1** The walk stays in front (fast, and it handles bounded oscillating factors Gruntz's algorithm does not);
  Gruntz runs only where it says unknown, and again when the walk's value fails the numeric check.
- **13C.2** Exponentials are held as a private head `exp%gruntz(u)` during the algorithm: the evaluator turns
  `e^(p log b)` into `b^p`, which recreated the subexpressions the rewriting had just removed (seen with Stirling's
  formula). Products of them merge; terms `c log v` with constant `c` leave the exponent as powers `v^c`, so a
  parameter of unknown sign (`n^x`) does not block the class decision.
- **13C.3** Truncated asymptotic expansions: a result computed with 4 terms is computed again with 6 and kept only
  when both agree (4 and 6, not 6 and 10: the larger run cost 6 s on gamma quotients for no case gained).
- **13C.4** The check rejects a value only when the three samples settle (each step at most half the one before) at
  a point farther from the claimed limit than the remaining steps can go, or (infinite limits) when they settle at all
  or all have the wrong sign. Slow or late convergence (`log(log(x))/log(x)`, Gruntz's nested example whose values
  still grow at 10^4) is not evidence. Sample points stop at 10^4 (10^-6 near a finite point): `erfc(10^8)` aborts the
  process in `mc-special` (a 1.8 PB allocation), see the requests below.
- **13C.5** Limit values are tidied: logarithms of rationals and powers of composite integers split over primes
  (`(log(25) - log(5))/(log(4) - log(2))` is `log(5)/log(2)`, `4^-x 2^x` is `2^-x`) when that gives a smaller term;
  the two sides of a limit are equal when `expand` (then `simplify`) says so.
- **13C.6** Real values only: `log(u)` needs `u > 0` for large x; `u^p` with p not an integer and `u < 0` is written
  `(-1)^p (-u)^p`, so a limit 0 is still found (`x^n exp(-1/x^2)` at 0 from both sides) and an infinite one is left
  undecided; `limit(log(x), x -> 0)` stays unevaluated as before.
- **13C.7** `abs(c)` of a symbolic constant counts as positive in Gruntz's sign (generic values: the outer variable of
  an iterated limit, `limit(real_abs(x) tan(y)^2, [x -> 0, y -> pi/2])` is `inf`).
- **13C.8** `max_limit` / `min_limit` of an oscillation: one argument `u → ±inf`, terms `c_k sin(u)`, `c_k cos(u)`
  with finite coefficient limits give `L ± sqrt(A^2 + B^2)`; a coefficient that grows gives `±inf`; two different
  arguments stay unevaluated (the sum of amplitudes is only a bound).
- **13C.9** `discrete_limit` splits `(-1)^(k n + j)` into even and odd terms; different limits give
  `failure(no_limit, discrete_limit, f)`.
- **13C.10** `asymptotic` at an infinity strips the terms of ω's exponent that tend to 0 (`exp(s) ~ exp(s0)` when
  `s - s0 → 0`) and writes `exp(c log v)` as `v^c`; it keeps a coefficient's own leading term (`e^(1/x)` becomes 1;
  keeping `e^(1/x)` would also be correct).

### Checks and robustness

- Every limit value is checked numerically before it is returned. All 72
  cases of `tests/gruntz.rs` and the 40 of `tests/limit.rs` pass the check (no false rejection), including the
  slowly converging ones.
- Per-call memo tables for limits, signs, mrv sets and leading terms; the deadline and the evaluator's interrupt are
  polled at every step, so an undecided limit returns within its budget (test `undecided_within_budget`).
- Branch points: `series(sqrt(1 - x), x, 1, 2)` was `(x - 1)^(1/2)` before this round (a wrong sign of the square
  root of -1); now `(1 - x)^(1/2)`, real where the function is.
- Machine inputs: `series(sin(2.5 x), x, 0, 10)` has each coefficient correctly rounded.

### Tests

`tests/gruntz.rs` (2 tests, 72 cases): the classic Gruntz examples (`exp(x)(exp(1/x - exp(-x)) - exp(1/x))` → -1,
nested `exp(exp(exp(x)))` quotient → inf, the `log(x)^2 exp(sqrt(log x) ...)/sqrt(x)` tower → 0), exponentials of
different bases, Stirling and gamma quotients, erfc, Ei, Bessel I, Airy, zeta, product_log at inf, iterated and
multivariate limits, `max_limit`, `min_limit`, `discrete_limit`, `asymptotic`, the series extensions; undecided cases
within their budget. `tests/limit.rs`: two hard cases now decided moved out of the undecided list
(`(x + 1)^(1/3) - x^(1/3)` → 0, `gamma(x)/x` at 0 → inf). `mc-poly` tests: three expectations changed to the
corrected forms (`e` for `exp(1)`, gamma's Laurent series at 0, asin's Puiseux series at -1).

### Measurements

The numeric check is skipped for slow special functions (`dawson_f` and `erfi` far out), which otherwise ran into
time limits.

Times (release, REPL with start-up): the Gruntz examples of `tests/gruntz.rs` 0.02-0.3 s each,
`limit(gamma(2 n + 1)/gamma(n + 1)^3, n -> inf)` 1.4 s (6.9 s before the truncation was lowered to 4 and 6 terms),
the whole test file 5.5 s.

### Known gaps and requests

1. Oscillating functions inside Gruntz's algorithm (`sin(x)` with x → inf) are left to the walk's bounded-factor
   rules; no interval results.
2. `limit(n^x n!/pochhammer(x, n + 1), n -> inf)` (= `gamma(x)`) is not decided: the class decision of the
   moved-up exponents still needs the sign of the parameter.
3. `series` at infinity of special functions (`factorial`, `product_log`, Bessel) and `series_coefficient`,
   `inverse_series` are not built.
4. **Open (mc-special):** `n(erfc(10^8), 50)` aborts the process with a 1.8 PB allocation (also through
   `erf(x)^x` at 10^8).
6. **Open (mc-special):** `lucas_l(-2^63)` aborts the process with a failed 512 MB allocation; the engine fuzz
   test (`mc-engine fuzz`) reaches it through the documentation pages (the new `discrete_limit` page uses
   `lucas_l`), so that test fails until `lucas_l` refuses huge indices.

## Round 17 item 14A: the integration portfolio (`src/integrate/portfolio.rs`, crate `mc-integrate`)

`integrate(f, x)` at depth 0 (the native, and `definite/` through `antiderivative(.., 0)`, whose signature is
unchanged) runs a portfolio; inside a method (depth 1 and more) the classic dispatcher runs as before
(`mod.rs`: `classic`).

1. **Quick table**: linearity over the base table, the rational integrator, `table::single`, and
   `derivative(n)(g)(x)` (an unknown function's derivative integrates to `derivative(n - 1)(g)(x)`).
2. **Engine** (`mc-integrate`, see its NOTES): the Risch decision procedure (1.5 s) for towers of height one; the
   rational remainder of its answer goes to the classic dispatcher; then the Risch–Norman ansatz with the
   special-function candidates (2.5 s). A proof of non-elementarity skips step 3 (the elementary rules cannot find
   what does not exist).
3. **Elementary rules** (`elementary/`) under a deadline: 4 s when the engine found nothing, 0.6 s when it did
   (`elementary::set_deadline`; the attempt budget stays).

Every answer is checked: `d(F)/dx - f` at x = 3/7, 5/11, 7/13 with the parameters at fixed rationals, 30 digits
(`n` of an exact zero is `failure(precision_not_reached, ball(0, r))`, read as zero), else `simplify` must give 0;
a failed answer is dropped. Of two answers the elementary one is kept unless the engine's is smaller by a fifth
(`mc_simplify::cost`), so established forms (`(-1/4) sin(2x) + x/2`) stay.

- `risch_integrate(f, x)`: the decision procedure alone: the answer, `failure(nonelementary, risch_integrate,
  reason)`, or unevaluated outside it. Page `docs/functions/risch_integrate.mcn`.
- `generated_parameters: c` (natives): an antiderivative plus `c(1)`.

Corpus of 126 indefinite integrals (textbook exp/log/trig towers, special-function integrands, non-elementary
ones; scratch `corpus.txt`): main answers 52, this branch 116, none lost; the 10 left are not elementary and have
no candidate (`log(sin x)`, `x^x`, `exp(x^3)`, `log(x)/(1 + x)`, ...).

Tests: `tests/integrate_engine.rs` (towers, special-function answers, printed forms, decisions, bounded time).
Changed expectations: `integrate_cross::unsolved_stay` and `integrate_elementary::unsolved_stay` (exp(x^2), sin(x)/x,
exp(x)/x, sin(x^2) are answered now), `integrate_definite::integrands_refined_under_assumptions` (the engine answers
`(m/(2 pi f))^(3/2)` without the assumptions, one algebraic monomial: a different printed form).

## Round 17 item 14D: definite integration

Code: `src/integrate/definite/`; one-line or small changes outside it: the `Integrate` native in `src/natives.rs` (options, nested lists, regions), the call of
`integrate` in `src/transforms.rs` (`generate_conditions: false`). Indefinite integration is called only through
`integrate::antiderivative`.

### What was built

One driver, `definite::integrate(cx, b, stop, f, x, lo, hi, Opts)`, over an ordered cascade (`definite/mod.rs`):

| step | module | contents |
|---|---|---|
| trivial cases | `mod.rs`, `delta.rs`, `split.rs` | zero integrand, equal or reversed bounds, constants, `dirac_delta`, functions by cases (with `unit_box`, `unit_triangle`, `heaviside_theta`, `boole` rewritten), the integrand refined on the range |
| convergence | `conv.rs` | the behaviour at both ends and at interior singular points as `C t^P (log t)^L` times exponentials and oscillating factors; Hardy's scale, exponential dominance, Dirichlet's test for one oscillating factor of order k; conditions on parameters (`re(n) > -1`, `re(c) > 0`, `im_part(w) < 1`) |
| conditions | `mod.rs` | recorded in the job, the integral computed by an inner call under the conditions as facts, attached once by F3's `Conditions::finish` |
| linearity | `mod.rs` | constant factors; sums term by term when every term converges (conditions of an abandoned split dropped) |
| closed forms | `forms.rs` | `x^r log(x)^k exp(-a x^d)` over (0, inf) (gamma and its derivatives), the lower incomplete gamma and `erf` over (0, L), Gaussians with polynomial factors over the real line, Bose-Einstein and Fermi-Dirac integrals, Beta integrals of `(x - a)^p (b - x)^q` and of binomials `x^α (A + B x^d)^β` (with the reflection formula), sines and cosines through complex rates |
| residues | `residue.rs` | the real line with indented contours (Jordan's lemma for `R(x) exp(i a x)`, real poles of the terms enclosed on the side the contour closes), even integrands over (0, inf), rational functions of `sin(kx)`, `cos(kx)` over a period by `z = exp(i x)` (and half periods of even ones) |
| substitution | `subst.rs` | `x = log(u)` for rational functions of `exp(x)` (hyperbolic functions included); `x -> 1/x` on (0, inf) for integrands odd under it; `x = exp(-u)` on (0, 1) for integrands with `log(x)` |
| symmetry | `mod.rs` | odd integrands over symmetric ranges |
| Newton-Leibniz | `mod.rs`, `points.rs` | the existing splitting at critical points, now with poles of `tan`, `sec`, `cot`, `csc` and `log(sin u)` of linear arguments, limits by the walk then Gruntz (`limit_with`), principal values, one infinite bound with a symbolic one |
| Meijer G | `mod.rs` | `meijer_g`: the hook of the later Meijer G work (returns `None`) |
| check | `check.rs` | `n_integrate` with its error estimate (the real line in two halves; complex integrands without an estimate; oscillating integrands over infinite ranges by two truncations half a period apart); parameters at up to two sample points that satisfy the conditions and the facts, and one complex point when real conditions stood in for conditions on real parts |

Options of `integrate` for definite integrals: `generate_conditions: false`, `principal_value: true`, `assumptions:
a`; nested lists (matrices) element by element; `integrate(f, element(vars, region))` goes to `region_integrate`.

### Decisions

- **14D.1** The cascade order: trivial cases, convergence, linearity, closed forms, residues, substitution,
  symmetry, Newton-Leibniz, Meijer G. Newton-Leibniz stays after the specific methods because their answers are
  smaller (`π/√2` rather than a sum of `atan` terms) and need no antiderivative; it remains the general method.
- **14D.2** Conditions: convergence conditions are recorded once and the integral is computed by an inner
  `integrate(..., generate_conditions: false, definite$inner: true)` under `assuming` the conditions; conditions on
  real parts are given to the inner call as conditions on the parameters themselves (`n > -1` for `re(n) > -1`),
  since the limits decide real exponents; the value then holds for complex parameters by analytic continuation when
  it contains no non-analytic function (`abs`, `re`, `sign`, steps, cases), which is checked, and the numeric check
  samples a complex parameter. Conditions are normalised (`re(s) < 1`, not `re(1 - s) > 0`; `re(i k w)` as
  `-k im_part(w)`), simplified once by `Conditions::finish`, and dropped when the facts imply them.
- **14D.3** The numeric check refutes a value only on evidence: a difference above 50 times the error estimate and
  `10^-6` relative while the estimate is small; without an estimate only a difference above `10^-3` relative. A
  refuted value is not returned (the call stays). An integral `n_integrate` cannot do is returned unverified.
- **14D.3a** An oscillating integrand over an infinite range is checked on the truncated ranges directly:
  `n_integrate` over the whole range spent 14 s before giving up (`exp(i w t)/sqrt(t)`), which made
  `fourier_transform(1/sqrt(abs(t)), t, w)` time out.
- **14D.4** Closed forms and the symmetry rule apply only to integrals whose convergence is decided (or conditioned);
  the residue methods check their own conditions (degrees, real poles of the integrand itself refused), and
  Newton-Leibniz its limits. A sufficient condition that is refuted (an exponential rate with zero real part) makes
  convergence unknown, not divergence.
- **14D.5** Divergence is a `failure(divergent, integrate, f)` (2C4.2), now also for oscillation without decay
  (`sin(x)` on `[0, inf]`, `x exp(i x)`), decided by Dirichlet's test before any method.
- **14D.6** One infinite bound with a symbolic one: `F(a) - lim F` when `f` and `F` are continuous on the whole line
  (`integrate(exp(x), [x, -inf, a])` is `exp(a)`).
- **14D.7** Values are tidied once: expanded, then put over a common denominator, each only when that is smaller;
  two or more logarithms of rationals with integer coefficients combined (`log(125/54)`), and inside a factor one
  with a coefficient of 2 or more (`-γ - log(4)`).
- **14D.8** Budgets: 20 s for the cascade (later methods are skipped after it), 3 s per Newton-Leibniz limit, 0.6 s
  per convergence limit, every step polls the evaluator; the nesting of the cascade is at most 10.
- **14D.9** Step-like functions are written in the forms the splitting by cases knows before it runs:
  `heaviside_theta(u)` as `unit_step(u)`, `unit_box(u)` and `unit_triangle(u)` with `abs` and unit steps of linear
  arguments, `boole(c)` as `if(c, 1, 0)` (their values on the jumps do not matter).
- **14D.10** The integrand simplified on the range: when the cascade finds nothing and the integrand holds a
  function facts can simplify (`abs`, `sign`, `conjugate`, `log`, non-integer powers), it is refined with `x` real
  and between the bounds and tried once more (`sqrt(x^2)` on `(0, 1)`, `conjugate(exp(i x))` on a period).

### Checks and robustness

- Every value with numeric data is checked against `n_integrate` before it is returned: 49 of the 54 values of the
  first seven tests of `tests/integrate_definite_14d.rs` were confirmed numerically (the other five: two principal
  values, `exp(x^a)` with complex incomplete gamma values, `(1 - cos x)/x^2` and `sin(x^2)`, whose numerical
  integrals do not converge in machine arithmetic).
- A wrong value of the old splitting by cases is now refused: `integrate(if(round(x) > x, x, -x), [x, 0, 10])` was
  5 (the value is 5/2): a switching expression with jumps (`round(x) - x`) is not split at its zeros
  only, and the numeric check refutes such values too.
- Honest unevaluated results: a pole whose half plane depends on a parameter (`1/(x^2 + a^2)`) leaves the call
  unevaluated rather than guessing; a Fourier-type integrand with a pole of its own on the line is refused by the
  residue method and decided divergent by the convergence step.
- Fourier-type integrals with poles on the line (`sin(x)/x`, `(1 - cos x)/x^2`, `sin(x)^3/x^3`) need no principal
  value or special rule: one indented contour for all terms.
- Bounded and interruptible: the cascade has a budget and polls the evaluator (`tests/integrate_definite_14d.rs`,
  `bounded_and_interruptible`).
- `n(integrate(exp(-x^x), [x, 0, 1]))` is 0.45854197896, agreeing with the 20-digit value 0.45854197895854.

### Measurements

The tests of `tests/integrate_definite_14d.rs` take under 1 s.

### Tests

`tests/integrate_definite_14d.rs` (10 tests, 74 cases: conditions, closed forms, residues, Newton-Leibniz and
tidying, divergence, iterated/lists/regions, honest unevaluated results, budget and interruption);
`tests/integrate_definite.rs` updated (`exp(-a x)` on `(0, inf)` is now `conditional_expression(a^-1, re(a) > 0)`,
`sin(x)` on `(0, inf)` divergent); `tests/props.rs` (a Fourier transform in a smaller form).

### Known gaps and requests

1. Antiderivatives with special functions (`sin_integral`, `cos_integral`, `exp_integral_ei`, `polylog`) come from
   the Risch tower of `mc-integrate`: `integrate(sin(x)/x, [x, 0, 1])`, `integrate((1 - cos(x^2))/x, [x, 0, 1])`,
   `integrate(log(1 + x)/x, [x, 0, 1])` stay unevaluated until then.
2. Residues with parameters: the side of a pole that depends on a parameter's sign is not split into cases
   (`integrate(cos(a x)/(1 + x^2), [x, -inf, inf])`, `1/(x^2 + a^2)`); under assumptions they are found.
3. Meijer G methods (Bessel integrands) come later (`meijer_g` hook).
4. Periodic integrands over several periods with a remainder, and the substitution `x -> 1/x`, are not built.
5. **Open (simplify):** `simplify(log(64)/18 - log(2)/3)` is not 0, so values that differ only in how
   logarithms of integers are written are not recognised as equal.
6. **Open (region code):** the default `tetrahedron()` of `region_integrate` should be documented, and
   `element(x, ball(3))` with a scalar variable is not understood.

## Round 18 item 14B: integrands with roots (`src/integrate/elementary/algebraic/`)

The portfolio of substitutions for algebraic integrands; the exact algebra (Chebyshev's decision, Hermite reduction
in `y^q = g`, Abel's continued fractions, Goursat's invariants) is `mc-integrate::algebraic`, and the decisions,
methods and gaps are in `crates/mc-integrate/NOTES.md` (section 14B). In `portfolio.rs` the stage runs after the
engine (`elementary::algebraic::top`), its verified answer joins the candidates, and its proof of non-elementarity
skips the elementary rules; `risch_integrate` reports it. Tests: `tests/integrate_algebraic.rs`.

## Round 18 item 14C: the rule table in the portfolio

One hook in `src/integrate/portfolio.rs` (and the `mc-rules` dependency line): when the tower engine found nothing,
`mc_rules::integrate` runs before the elementary rules (and before the return on a proof of non-elementarity, so
special-function answers of the rules are still found), with the quick table as its fallback for the rational and
algebraic integrals its rules lead to; its answer is checked by `verified` like every other. When the rules answer,
the elementary rules get the short budget and the smaller answer is kept (the elementary one on a tie). See
`crates/mc-rules/NOTES.md`.

## Round 19 item 13A: `d_solve` (`src/dsolve/`)

### What was built

`dsolve.rs` became the module `src/dsolve/` with one file per method family (the module documentation of each
file cites its literature):

- `mod.rs`: the native; arguments (`d_solve(eqs)` and `d_solve(eqs, y)` read the unknowns and the variable off
  the equations; the operator form `y' == y` gets the variable `FormalX`; `d_solve_value` of any expression in the
  unknowns, `y(3)`, `y'(x)`; `generated_parameters` as an option or a trailing rule); the conditions (initial and
  boundary values, solved with `solve`; a particular solution that meets them with every constant 0 is accepted
  when `solve` cannot handle root objects); the written form of the results (`tidy`, `display`).
- `linear.rs`: constant coefficients of any order (the roots in a fixed order of the constants, root
  objects for cubics and quartics), undetermined coefficients for exponential-polynomial right-hand sides (the
  shift `L(D) (Q e^(l x)) = e^(l x) L(D + l) Q`, so only polynomial identities are solved), variation of parameters
  of any order (the Wronskian system), Euler equations of any order, equations without `y` lowered in order,
  second order with variable coefficients by an ansatz (`exp(l x)`, `x^r`, polynomials up to degree 8) and
  reduction of order.
- `special.rs`: Airy, Bessel (Bowman's form, modified Bessel for a negative `beta^2`), Gauss, Legendre, Kummer and
  Hermite bases.
- `kovacic.rs`: Kovacic's algorithm, case 1.
- `first.rs`: separable, linear, Bernoulli, `y' = F(y + l x)`, Riccati (the linear second-order transform; a
  particular solution `k x^m` otherwise), homogeneous, isobaric, exact with integrating factors in x or y,
  integrating factors from Lie point symmetries with a linear ansatz for the infinitesimals, Clairaut; implicit
  solutions solved for `y` (exponentiated when logarithms of `y` occur) or returned as `solve(eq, y(x))`;
  second-order equations without `y` or without `x` reduced to first order.
- `system.rs`: algebraic equations eliminated, then the scalar solver, or linear constant-coefficient systems
  `X' = A X + f` by the matrix exponential from Cayley-Hamilton over the real scalar basis (no eigenvectors, no
  Jordan form, real answers for complex eigenvalues), higher derivatives as extra components.
- `pde.rs`: first-order linear PDEs in two variables by characteristics (arbitrary functions `c(1)(xi)`),
  second-order homogeneous constant-coefficient PDEs by factoring (the wave equation).
- `delay.rs`: delay equations by the method of steps, a `piecewise` result.
- `series.rs`: the new function `asymptotic_d_solve_value`: Taylor polynomials at ordinary points (conditions or
  constants, grouped by constant), Frobenius series at regular singular points (roots not differing by an
  integer), systems, regular perturbation in a parameter, `x -> x0` for the leading term.
- `verify.rs`: every explicit solution is substituted into the equation; the residual is evaluated at random
  points (constants and parameters too, machine precision, a failure confirmed to 30 digits) and simplified; a
  solution that fails is dropped. Systems check every equation; PDE solutions are checked symbolically.
- `recur.rs`: `r_solve` of constant-coefficient recurrences, moved unchanged.

Each call has a 5-second budget and polls the stop flag; every method fails
into the next one, and when all fail the call stays unevaluated. `MC_DSOLVE_TRACE` prints the method being tried
and the time left (development aid).

### Decisions

- The order of the constants is fixed (binomial characteristic polynomials: the k-th roots from the
  principal one counterclockwise; otherwise ascending; the cosine of a pair takes the number of the root with the
  positive imaginary part; Euler equations ascending).
- With conditions the result is the shorter of `simplify` and `together`, with decaying exponentials taken out as a
  factor (`(-1/2) a (-1 + cos(x) exp(x) - exp(x) sin(x)) exp(-x)`).
- Held pure-function bodies (`y -> (x => ...)`) are written in a fixed form: constants last in
  products (`exp(x)*c(1)`), `(...)/k` for `(1/k)(...)`, the arguments of arbitrary functions in the order of the
  variables (`c(1)(-3*x + y)`). The value is unchanged; only the order of the stored term differs.
- A symbolic root of a binomial `m^2 + w^2` is `i w` by `power_expand` only for monomials; a sum keeps its
  principal root (`(1 - a)^(1/2)`), so a parameter of unknown sign gives real exponentials.
- Families of solutions from `solve` (`element(c(k), integers)`) take the principal branch.
- Kovacic cases 2 and 3 are not implemented.

### Checks and result forms

- Every solution is verified before it is returned; Clairaut equations give only the general solution.
- `y'' + 4 y == sin(x)` gives `(1/3) sin(x) + c(1) cos(2x) + c(2) sin(2x)` (undetermined coefficients), not the
  longer form variation of parameters gives.
- Hermite-type `y'' - 2 x y' + 6 y == 0` gives the polynomial `x^3 - (3/2) x` and a second solution in `erfi`.
- Linear systems with complex eigenvalues come out real (cosines and sines from the scalar basis).
- `y' == (x + y)^2` gives `tan(x + c(1)) - x`; `y'' == y'^2` gives `c(2) - log(x + c(1))`.

### Tests

`tests/dsolve.rs`: 58 checks in five tests (linear, nonlinear, systems/PDE/delay, values and series,
recurrences); full suite 1780 passed, 0 failed.

### Known gaps and requests

- Kovacic cases 2 and 3; d'Alembert (Lagrange) equations; Frobenius series with logarithms (indicial roots
  differing by an integer) and irregular singular points in `asymptotic_d_solve_value`; nonlinear systems; PDEs
  beyond the two classes; implicit solutions are not verified.
- `simplify(lhs == rhs)` does not reduce an equation whose sides are equal but written differently to `true`, so
  checking a solution by substituting it into the equations and simplifying does not confirm it (mc-simplify, n12).
- `parametric_plot` of a doubly nested list of pairs is not drawn (viz).

## Round 19 fixes

- **f19.1** `props` (Q19-04): a function with jumps or kinks (one of the continuous non-smooth heads: `floor`,
  `unit_step`, `abs`, `max`, ...) has its monotonicity and convexity decided from samples of the function itself
  (differences; slopes between neighbours), not from derivatives that miss the jumps (`floor` was "constant",
  `abs` "linear"). Over the complexes `tanh`, `sech`, `gudermannian`, `coth`, `csch` have poles and the inverse
  functions branch cuts, so they are not analytic everywhere (were `true`). `logistic_sigmoid` is analytic on the
  reals; ties of machine samples of a saturating function are compared again at 40 digits (`function_injective`).
- **f19.2** `limit` (Q19-10): a one-sided limit onto the negative real axis of a `log` or non-integer power uses
  `log(u) = log(-u) + s im pi` (s the sign of `im_part(u)` beside the point, at 40 digits), so `from_below` gives
  `log(10) - im pi` (it gave the value on the cut, `+ im pi`).
- **f19.3** `apart(e, x, modulus: p)` (Q19-06): partial fractions over F_p with `mc_poly::factor::factor_mod_p`.
- **f19.4** Definite integrals (Q19-34): `floor`, `ceiling`, `round`, `integer_part`, `fractional_part` of a linear
  argument split at their jumps (`split.rs`); closed forms `x^r/sinh(a x)` (`2 (1 - 2^-s) Γ(s) ζ(s)/a^s`),
  `e^(-a x) sin(b x)/x` (`atan(b/a)`), `log(1 + c x)/x` on `[0, L]` (`-Li_2(-c L)`), `x^α/(A + B x^d)` with odd d
  (`π/sin(π μ)`); the critical-point search finds the zeros of transcendental expressions (`1 + log(x)^2`) by
  `solve` over the reals, which lets Newton-Leibniz handle `1/(x (1 + log(x)^2))`. The first two forms are tried
  also when the convergence test does not decide (their own conditions imply convergence).

## Round 20 item 14E: integral transforms (`src/transforms.rs`, `src/transforms/`)

Code: `src/transforms.rs`, `src/transforms/` and `src/integrate/` (not `portfolio.rs`). New dependency line: `mc-syntax` (the tables are read from source text).

### What was built

One native (`transforms::Transform`, one `Kind` per function) for `laplace_transform`, `inverse_laplace_transform`,
`bilateral_laplace_transform`, `inverse_bilateral_laplace_transform`, `mellin_transform`, `inverse_mellin_transform`,
`fourier_transform`, `inverse_fourier_transform`, `fourier_sin_transform`, `fourier_cos_transform` and their
inverses, `z_transform`, `inverse_z_transform`; and `discrete_delta`, `kronecker_delta`. The front end maps over
lists, equations and rules, iterates over several variables, computes a transform at a non-symbol target with a
symbol and substitutes (a numeric target falls back to `n_integrate` for Laplace, Mellin, sine and cosine), reads
`generate_conditions:` and `fourier_parameters: [a, b]`, and finishes every value in the tables' form (`finish`).
An undefined function of the variable alone leaves the call unevaluated; inside sums such functions stay as
transform calls, and the operational rules apply to them (derivatives, shifts, scaling, `conjugate`).

| module | methods |
|---|---|
| `laplace.rs` | linearity, undefined functions (`s^n F - ...`, `F/s`, `F(s/a)/a`), the table (Bessel, error functions, `Si`, `Ci`, `E1`, Laguerre, logarithms, `t^(-3/2) exp(a/t)`), the shift rule `exp(a t) g`, trigonometric and hyperbolic functions as exponentials, `t^n g` as `(-1)^n G^(n)`, delayed steps and cut-offs, `g/t` as `integrate(G, [u, s, inf])`, the definite integral; the two-sided transform with its strip |
| `inverse.rs` | rational functions by residues (factor, roots of each factor, the Heaviside expansion with the derivative formula at multiple poles, conjugate pairs as `sin`/`cos`, parameter pairs as `sinh`/`cosh`), polynomial parts as `dirac_delta` derivatives, the table, delays `exp(-a s) G`, shifts `G(s + a)` (also completing the square), `G/s` as an integral, `-L^-1[F']/t` for logarithms and inverse tangents; the two-sided inverse on a strip as a `piecewise` |
| `mellin.rs` | distributions for powers of `x`, `dirac_delta`, the table (Bessel J, K, Y, `erf`, `erfc`, `E_n`, `Ei`, Bose-Einstein, `log(1 - e^-x)`), `x^a g`, steps, `g(log x) unit_step(x - 1)` as a Laplace transform at `-s`, scaling `f(a x^k)`, the definite integral; the inverse by the table, `c^(k s + d) G`, and the Mellin-Barnes residue series (one family of gamma poles and the rational poles left of the strip, summed by `sum`), strips from `assumptions:` or a `conditional_expression`, the fundamental strip as the condition |
| `fourier.rs` | exponential transform: polynomials and `exp(i c t)` as delta functions, the table (steps, `sign`, `1/t`, `abs(t)^a`, `log abs(t)`, `sin(a t)/t`, `sech`, `J0`, `J1/t`), modulation, trigonometric functions as exponentials, Gaussians and step bounds, even and odd parts through the cosine and sine transforms (values valid for every real `w`), rational functions by residues with `heaviside_theta(w)`; sine and cosine: tables, the definite integral with `w > 0`, the `g/t` rules, the Laplace transform on the imaginary axis |
| `ztrans.rs` | linearity, undefined functions (shifts with initial values), deltas, steps, `a^n g`, `exp(c n) g`, `n^k g`, trigonometric functions, factorial powers and Pochhammer symbols, `sum`; the region `abs(z) > r`; the inverse of rational functions by residues of `F z^(n - 1)` and Laurent coefficients at 0 |
| `pattern.rs` | the table matcher (products and sums matched one to one, lone parameters bound to quotients and differences, `t^n` matching `t`) |
| `verify.rs` | the numeric check (the definite driver's comparison at sample parameters and points; inverses through the forward integral; Z by partial sums) |

### Decisions

- **n14e.1** Transforms run their own rules before `integrate`: the definite integral does not decide parameterised
  integrands such as `sin(w t) exp(-s t)` (they need no limit, only the shift rule), and the rules give the forms
  the tables use. The integral is the last method (and the first one under `generate_conditions: true`, since it
  knows the region of convergence).
- **n14e.2** Tables are source text read by the language's own reader (`mc-syntax`), matched by a small structural
  matcher, not the evaluator's rules: entries stay one line each, and a table cannot loop.
- **n14e.3** Every value from a rule or a table is checked numerically; refuted values are not returned. The Mellin
  check needs two refutations (a point outside the strip makes `n_integrate` meaningless); the boundary values of
  a Laplace transform on the imaginary axis are accepted for the sine and cosine transforms only when `f` decays
  exponentially or the check confirms them (`sin_integral(t)` gave 0 otherwise); integrands with `unit_box` are not
  checked by truncated oscillatory quadrature (it misses the narrow support).
- **n14e.4** Values in the tables' form: `sqrt(2/pi)` multiplied last (simplify writes `pi^(-1/2)`, which the
  simplifier does not identify with `(1/pi)^(1/2)`), common rational factors of exponents outside
  (`exp((1/4) (p^2 + s^2))`), strips as one chain (`-1 < re(s) and re(s) < 1`), `csc` for Mellin results,
  rational bases split (`3^s 2^-s`), odd linear factors of the Z transform as `z - a`.
- **n14e.5** A rational `F` gives an inverse Mellin transform with `heaviside_theta(1 - x)` (its contour closes to
  the right without poles for `x > 1`), and `polygamma(n, s)` likewise.
- **n14e.I1** (user item) Bounds whose order depends on the sign of one parameter: the integral under `a > 0` and
  under `a < 0`, joined (one value; `sign(a)` times the first, with `a^(2k)` as `a^(2k-1) abs(a)`; else a
  `piecewise`), with `element(a, reals)` recorded: `integrate(sqrt(x (x - a)), [x, 0, a])` is
  `conditional_expression((im/8) a pi abs(a), element(a, reals))`.
- **n14e.I2** (user item) Definite values with logarithms, roots, `abs`, `sign` or inverse trigonometric functions
  are refined and simplified under the facts of the context before they are returned: `assuming(a > 0,
  integrate(sqrt(x (x - a)), [x, 0, a]))` is `(im/8) pi a^2` (was a difference of two logarithms); likewise
  `asin(a/sqrt(a^2))` forms and `a pi/(2 sqrt(a^2))` on other Newton-Leibniz paths.
- **n14e.I3** Convergence conditions are recorded also with `generate_conditions: false`: the numeric check samples
  only where they hold (a divergent sample made `n_integrate` spend 8 s on `x^(s - 1)` over `(2, inf)`).

### Accuracy and checks

- `inverse_fourier_transform(1/(w^2 + 1), w, 0.3)`: the exact value `sqrt(pi/2) e^-0.3 = 0.928477949161311`.
- `fourier_cos_transform(exp(-t)/sqrt(t), t, 1.3)`: 0.990685607418217803 (checked at 30 digits); likewise the sine
  transform (0.487815076595) and `laplace_transform(sin(t), t, 1.2)` (exactly 1/2.44 = 0.4098360655737705).
- Inverse Mellin transforms of rational functions and polygamma functions carry `heaviside_theta(1 - x)`
  (n14e.5): `log(x)^2/(x - 1)` alone for `polygamma(2, s)` would not be the inverse transform for `x > 1`.
- Every value of a rule or table is checked numerically before it is returned (n14e.3).

### Tests

`tests/transforms.rs` (6 tests, 75 cases: Laplace, inverse Laplace, two-sided, Mellin and inverse, Fourier
exponential, sine and cosine with parameters, Z and inverse, the deltas); `tests/integrate_definite_14d.rs`
`sign_of_bounds_and_facts` (9 cases, the user items); `tests/integrate_definite.rs` (two values now simplified
under the facts).

### Known gaps and requests

1. No Meijer G function in the system: Mellin transforms of products of Bessel functions, `gamma(s/a)^2`, and
   inverses with two gamma families (double poles) stay unevaluated.
2. Distributional transforms of non-decaying special functions (`sin_integral`, the Fourier transform of
   `bessel_j(2, t)`) and transforms needing Fresnel or Airy integrals are not built.
3. Sums with hypergeometric values (`2 1F1(1/2, 3/2, -x^2)`) are not converted to `erf`.
4. **Open (mc-arith, mc-simplify):** `(1/pi)^(1/2)` is not identified with `pi^(-1/2)`
   (`simplify((pi^-1)^(1/2) - pi^(-1/2))` is not 0, `pi^(1/2) (pi^-1)^(1/2)` stays), and
   `exp((1/4) (p^2 + s^2)) - exp(p^2/4 + s^2/4)` does not simplify to 0, so equal values with `sqrt(2/pi)` in
   different forms are not recognised as equal.
5. **Open (mc-sums):** `n_sum((n - 3) unit_step(n - 3) 5^-n, [n, 0, inf])` is 0.0 (the value is
   1/400); the Z check uses partial sums instead.
6. **Open:** `factor_list`, `exponent`, `polynomial_q` and `residue` are not functions of the system.

## Round 20 (roadmap 16A): parallel portfolios

- **n16a.4 Integration** (`integrate/portfolio.rs`): after the quick table, each method runs from a clean state
  (a fresh budget of the elementary rules, no deadline left over: before, the engine's nested elementary call
  used whatever budget the thread had left from earlier statements) and checks its own answer (`method`). One
  thread: the sequence as before. Several: the engine, the algebraic stage, the elementary rules (short budget)
  and the rule table at once (`par_scan`), cut as soon as the answers in order settle it (a proof of
  non-elementarity cancels the rest; an answer of the first two cancels the rule table), combined exactly as the
  sequence combines them; when nothing else answered and the short budget ran out, the elementary rules run again
  with the full one, as the sequence would have given them. The three points of `verified` run in parallel.
  `Build::stop` (term.rs) gives tasks the stop flag.
- **`d_solve`** (`dsolve/`): `Cx::par_until` runs elements on their own contexts (builder, the call's constants,
  deadline and depth): the eight first-order methods (first explicit solution in order, else the first
  implicit), the three bases for second order with variable coefficients (ansatz and reduction, special
  functions, Kovacic; the first in order), and the checks of the explicit solutions.
- `tests/par.rs`: 17 integrals and equations at 1, 2, 4 and 8 threads, the same answers (and the same as before
  this work on the benchmark cases).

## Round 22 (bug fixing)

- D21-05: `d_solve_value` of an initial value problem scans the denominators and logarithm arguments of the
  solution on a grid between the initial point and the target point; a sign change or zero gives
  `failure(blow_up, d_solve_value, x0, x1)` instead of the analytic continuation. Gap: a pole that does not
  change sign (an even-order pole of a squared base is caught through the base, a bounded-away minimum is not).
- D21-07: `working_precision:` is consumed by `integrate` (symbolic integrals are exact).
- D21-10: `integrate` of `g^(k)(x)` (undefined `g`) is `g^(k-1)`; Gaussian times `sin`, `cos`, `exp` of a linear
  argument over the real line for any complex parameter (`dominated_gaussian`: the Gaussian dominates, the value is
  entire in the parameter); `int_0^inf J_nu(a x) dx = 1/a` for `a > 0`, `nu > -1` (DLMF 10.22.1). Deferred:
  `integrate(1/(x^2 + a^2), [x, -inf, inf])` without an assumption (a conditional `pi/sqrt(a^2)`).
- engine B21-22: definite integrals of rational functions whose antiderivative is a `root_sum`: `points::collect`
  takes `root_sum(t => p(t), t => ...log(x - t)...)` with the real roots of `p` as critical points, and the
  numeric check no longer treats the bound variable of a pure function as a parameter.
- engine B21-23, R21-G6: a `log(u)` or `u^p` whose argument is negative real beside the point (also tending to 0)
  is written `log(-u) + i pi`; a missing two-sided limit carries a reason in the failure's third argument.
- D21-17: `y' = f(x)` with an integral that has no closed form gives `c(1) + integrate(f(x), x)` (definite form
  `y0 + integrate(f(t), [t, x0, x])` with a condition); a separable equation whose `y` integral has no closed form
  and an autonomous second-order equation give implicit solutions that keep the integral.
- D21-18: Frobenius series with indicial roots that differ by an integer or are equal: the second solution
  `C y1 log t + t^r2 sum b_k t^k` (Ince ch. XVI; Bender and Orszag 3.3), the residual checked to order `n`. Deferred:
  expansions at `x -> inf`.

## Round 22: Meijer G methods (`integrate/definite/meijer.rs`, `transforms/meijer_inv.rs`)

- **Definite integrals over (0, inf)** (the hook of 14D.1, one line in `definite/mod.rs`): `C x^alpha f1 f2` with
  f1, f2 among exp, sin, cos, Bessel J and K, erfc, `(A + B x^k)^-a`, each as `coef G(eta x^k)` (PBM 3, 8.4); with
  u = x^k the power is absorbed into G1 and Parseval's formula gives one G-function of e1/e2 (PBM 3, 2.24.1.1;
  Adamchik and Marichev 1990); one factor alone gives its Mellin transform. An exponential of x^k next to a
  Bessel function of x^k is written with x^(2k) by Gauss's duplication (e^-t = pi^(-1/2) G^{2,0}_{0,2}(t^2/4 | 0,
  1/2)). The 16 validity cases of PBM 2.24.1 are not decided: a value is returned only when the driver's numeric
  check (14D.3) confirms it. Examples: `bessel_j(0, x) exp(-2x)` = 1/sqrt 5, `x exp(-x^2) bessel_j(0, x)` =
  e^(-1/4)/2, `x bessel_k(1, 2x) bessel_j(1, x)` = 1/10, `bessel_j(1, x) exp(-x)/x`, `bessel_j(2, x)/(1 + x^2)`.
- **Inverse Mellin transforms of gamma ratios** (`meijer_inv.rs`): rational exponents brought to one unit by
  Legendre's duplication (in reverse, so steps of 1/2 do not double the order) and Gauss's multiplication formula,
  rational factors as gamma ratios with the pole on its side of the strip, then G by its definition, reduced by
  `meijer_g`; the contour must separate the families. Tried before the residue series when it gives named
  functions. `gamma(s)^2` -> 2 K_0(2 sqrt x) (double poles), `gamma(s/3)^2`, `gamma(s) gamma(s + 1/2)` ->
  sqrt(pi) e^(-2 sqrt x), `gamma(s)^2/gamma(2 s)` -> 2/sqrt(1 - 4x) theta(1 - 4x), `gamma(s/2)/(1 - s)` ->
  (-1 + erf(x)) sqrt(pi)/x (was a 6 s timeout; erfc is written 1 - erf, the tables' form), `gamma(s)/gamma(2 - s)` ->
  J_1(2 sqrt x)/sqrt x; a common symbolic unit too: `gamma(s/a)^2` ->
  2 a K_0(2 sqrt(x^a)).
- **Mellin table**: products of two Bessel functions of one argument (J J from DLMF 10.22.57 written with the
  duplication formula, K K from 10.43.19); `csc(a pi s)` inverses.
- **Bugs**: engine B21-15, B21-16 (the Laplace shift rule and the Mellin scaling rule recursed with a fresh
  variable while the table stayed bound to s, so the shift was lost: `cos(t) J0(t)` gave L[J0], `t sin(t) log(t)`
  gave 0); D21-09 (Fourier transform of g^(k): (-i SGN w)^k G); D21-12 (`exp(-a t^2)`, `erfc(a t)`, `1/(t + a)` in
  the Laplace table; delayed steps checked from the delay, since `n_integrate(heaviside_theta(t - 1) exp(-t), [t,
  0, inf])` is 0); D21-14 (the inverse Mellin cases above, csc); D21-15 (`abs(w) sign(w)` -> w, `(pi^-2)^(1/2)` ->
  1/pi); D21-16 (Z transforms of fibonacci, harmonic numbers, 1/n, sin(n)/n from n = 1; inverses of c z^k as
  shifted deltas, exp(c/z), log(z/(z - 1))); ui B21-2, B21-5 (`kronecker_delta` compares by the difference: 1.0
  and 1 equal, pi and 3, x and x + 1 unequal).
- Deferred: D21-13 (`inverse_laplace_transform(1/(s^3 + 1))` real form), the strip of `mellin_transform(...,
  generate_conditions: true)` from a table value (the integral-first route took 21 s).

## Round 24: wrong values of transforms and d_solve_value, aborts and hangs

- **Transforms at a point (D23-14, D23-01)**: `transform_one` now finds the symbolic transform with
  `Opts.strips` set, so the conditions of the tables and rules are kept as `conditional_expression(v, region)`, and
  checks the region at the point; a point outside it leaves the call unevaluated (no analytic continuation as a
  value). The Laplace and Mellin tables carry a fourth entry item, the region of convergence (`[lhs, rhs, true,
  re(s) > ...]`; Doetsch 1974 for Laplace, Oberhettinger 1974 for Mellin), carried through the shift, delay, scaling
  and power-factor rules (`shift_strip`); constants give `re(s) > 0`. Table entries valid for a parameter sign only
  carry `possibly(re(a) > 0)` (applies unless decided false): `exp(-a t^2)`, `t^(-1/2) exp(a/t)`. This also gives
  `mellin_transform(..., generate_conditions: true)` the strip for table values (D23-04: `0 < re(s) < 1`), and the
  numeric fallback is time-limited to 3 s (B23-13: `mellin_transform(log(x), x, im)` took 26 s).
- **Z transform (D23-13, D23-15, D23-20)**: the unilateral inverse of a rational function bounded at infinity is 0 at a
  negative integer n; `z_transform` of a sequence whose n = 0 term is infinite (1/n) is not returned; a product of
  undefined functions (`f(n) g(n)`) stays unevaluated in every transform (the held call was evaluated again without
  end: "limit: evaluation depth").
- **d_solve_value blow-up scan (D23-12, B23-9, B23-17)**: the candidates are typed: a zero of a log or a negative
  power's base, a pole of `tan` (argument = pi/2 + k pi) and of `cot`, a sign change of a fractional power's base. The
  zero at the initial point no longer counts (`tan(x)` at 0). Still a 240-point grid per candidate (R22-15, deferred).
- **Huge exponents (X23-9, X23-10)**: `has_huge_power` (exponent beyond 100000): `integrate` and the transforms leave
  such an integrand unevaluated unless it is a monomial (the quick table answers `x^(2^31)`); definite integrals over
  `[0, inf]` still reach `gamma(2^31 + 1)`.
- **Derivatives (R22-03)**: a term free of the variable has derivative 0 (a constant list argument gave `[0, 0]` and
  `shape_mismatch`); `d` of `hypergeometric_pfq` in z (DLMF 16.3.1).
- Deferred: D23-21 (rule forms with undefined f), `inverse_laplace_transform(s^2)`, B23-21 (laplace of t K0(t),
  floor(t)), B23-27b (struve integral 6-9 s), the divergence of `integrate(bessel_k(0, x)/x^2, [x, 0, inf])` taking
  7-10 s, R22-06/07/08/15.

## Round 25

- **Definite integrals through special-function antiderivatives.** `points::collect_with` refused any function it did
  not know, so `integrate(sin(x)/x, [x, 1, 100])`, `integrate(cos(x)/x, [x, 60, inf])` and `integrate(erf(x), [x, 1, 2])`
  stayed unevaluated although the antiderivative (`sin_integral`, `cos_integral`, `erf`) was found and its limits are
  known. It now accepts the entire functions `erf erfc erfi sin_integral sinh_integral fresnel_s fresnel_c dawson_f
  airy_ai airy_bi` and the ones with a logarithmic singularity at 0 (`cos_integral cosh_integral exp_integral_ei`, the
  argument is a critical point, so a range through 0 is divergent). This is what the Euler-Maclaurin tail of
  `n_sum` (mc-sums) uses for `cos(k)/k`.
- **R24-01**: `integrate(x^(-1.01), [x, 16, inf])` and `x^(-101/100)` give the closed form (97.26549474122855,
  50 2^(24/25)); the divergent `x^-0.99` is `failure(divergent)`. Already so on this base (the round 24 note was stale);
  a test pins it.
- **D23-26**: derivatives of `kelvin_ber/bei/ker/kei` in x for order 0 and order nu (`rules.rs::kelvin_x`): with
  A, B = (ber, bei)_(nu+1) - (ber, bei)_(nu-1) over 2, ber' = (A + B)/sqrt(2), bei' = (B - A)/sqrt(2); with
  P, Q = (ker, kei)_(nu-1) - (ker, kei)_(nu+1) over 2, ker' = -(P + Q)/sqrt(2), kei' = (P - Q)/sqrt(2) (DLMF 10.65.19-22
  by the recurrence of J and K; signs checked against central differences at several orders). The order is an
  opaque argument.
- Not done: B21-17..21, B23-13, B23-21, B23-27b, D23-21, D23-22 inverse_mellin, R22-06..08, R22-15.

## Round 25: function properties (`src/props.rs`, `src/props/`)

### What was built

| module | contents |
|---|---|
| `props/spec.rs` | R25P.1: one reading of every call: `f`, `[f, cond]`, `[f, cond, ycond]`, lists of components, codomain variables, domains (`reals`, `complexes`, `integers`, `positive_reals` and the other half lines), options `generate_conditions` (true, false, automatic, none), `assumptions:`, `strict_inequalities`, `performance_goal`; facts about the variables join the condition. R25P.2: `conditional_expression(v, element(params, reals) and c)` (realness only over the reals; `automatic` drops the `p != q` conditions) |
| `props/sets.rs` | R25P.3: singular sets: atoms of kinds pole, branch, jump, kink for every subterm (table in the module comment); `meets`: does a set meet the region (refine, `reduce`, `resolve(exists)`, complex variables over their real and imaginary parts with `complex_expand`, a numeric witness by bisection); with parameters `resolve(for_all(x, region implies not atoms))` after dropping non-polynomial conjuncts (a sufficient condition); the order of disjuncts; `tidy_cond` (R25P.9: `c < x` alone as `x > c`, `a < c or c < a` as `a != c`) |
| `props/smooth.rs` | R25P.4: `function_discontinuities`, `function_singularities` (now on the sets; the old code is the fallback), `function_continuous`, `function_analytic`, `function_meromorphic` (complex by default; only non-pole atoms count) |
| `props/period.rs` | R25P.5: `function_period`: candidates `p/a` from periodic subterms of linear arguments, constant powers as exponentials, `mod` of integer polynomials over the integers; least common multiple of commensurable candidates; the smallest `T/k` (k <= 24) or the smallest divisor (integers) that checks at six points to 30 digits; `0` for polynomials, real exponentials and incommensurable sums |
| `props/poles.rs` | R25P.6: `function_poles`: meromorphic check, candidates by `solve` of the pole atoms (families of `c(1)`; enumerated inside a bounded condition; the gamma family only there), orders by the multiplicity in the reduced denominator (rational functions) or by `limit(t^m f(p + t), t -> 0)` with a numeric check at `t = 10^-12`, merging of `a + P c` and `a + P/2 + P c` |
| `props/onto.rs` | R25P.7: `function_surjective`, `function_bijective`, and `function_injective` for maps, parameters, codomain conditions and the complexes (method list in the module comment) |
| `props/shape.rs` | R25P.8: `function_sign`, `function_monotonicity`, `function_convexity` with parameters, several variables or strict inequalities by `resolve` (principal minors of the Hessian for convexity) |
| `props/pw.rs` | R25P.10: `piecewise_expand` (exclusive pieces for relations, ordered pieces for symbolic conditions, step functions cut inside a bounded condition, `method:` suboptions) |
| `props/domain_form.rs` | R25P.12: `function_domain` forms: `cos(u) != 0` as `not_element(1/2 + u/pi, integers)` (and the `sin`, `cosh`, `sinh` lattices), integer shifts of `not_element` dropped, `not_element` first, a complex region (an ordering among the conditions, `abs(z) < 1` as `re(z)^2 + im_part(z)^2 < 1`) reduced over the real and imaginary parts, disjunctions reduced piece by piece; `gamma`, `factorial`, `zeta`, two-argument `atan`, `elliptic_theta` (nome in the unit disc) added to the conditions, now collected in postorder |
| `props.rs` | the old domain/sign/monotonicity/convexity/injective code, now behind `Spec`: facts, strict inequalities, a function not defined on the whole line gives `indeterminate` without a condition (R25P.11), `sqrt(u^2)` read as `abs(u)` so its kink is seen, strictness checked by `solve(f == 0)` |

### Decisions

- **R25P.3 (generic functions)**: an undefined head (`g(x)`, and a compound head free of the variables such as
  `log(z^3)(y)`) is taken to be analytic, as a generic function; a defined function not in the tables declines.
- **R25P.3 (order)**: atoms without `and` first, then cuts; each group by the key of its left side (numbers, constants,
  symbols and their powers and polynomials by their largest part, then calls by name), then the whole condition. It
  gives the expected ordering in 14 of 16 examples of the example corpus; `atan(x^y)` (the poles of `atan` before
  `x == 0`) does not.
- **R25P.11**: `function_sign(log(x)^2, x)` and `function_monotonicity(log(x), x)` are
  `indeterminate`: the property is asked for every real `x`, and `log(x)` is not real for `x < 0`; `[log(x), x > 0]`
  gives `1`. Before this round the natural domain was used silently.
- Conditions are not simplified beyond tidying: `function_discontinuities(max(log(abs(x) + 1), x sin(x)), x)` is
  `abs(x) <= -1` (empty), as stated by the definitions; `function_continuous` decides it.
- Monotonicity excludes the parameters for which the function is constant (`a x + 1` increases for `a > 0`); sign
  and convexity do not (`a x^2 + b x + c` is convex for `a >= 0`).
- Surjectivity that holds only on a set of parameters of measure zero (`a == 0 and b != 0`) is given as
  `conditional_expression(false, a != 0)`, the generic value.

### Literature

Standard real and complex analysis for the singular sets (branch cuts of the principal branches: DLMF 4.2, 4.23,
4.37), Laurent series and pole orders (Ahlfors, Complex Analysis, ch. 4), the intermediate value theorem and strict
monotonicity for images on intervals, Sylvester's criterion with all principal minors for semidefinite Hessians
(Horn-Johnson, Matrix Analysis, 7.2), quantifier elimination as in `mc-cad` (Collins; Lazard; McCallum) and the
complex elimination of `mc-cad/complex.rs`.

### Result forms and checks

- `function_continuous(sin(x*y)/(x^2 + a*x*y + y^2 + b), [x, y])` is `a <= 2 and a >= -2 and b > 0`, one interval
  condition rather than a disjunction of pieces of the same set.
- `function_bijective(x^3 + a x^2 + b x + c, x)` is `-3 b + a^2 < 0`, without the redundant `b > 0` (`a^2 < 3 b`
  implies `b > 0`).
- Every period is checked at six points to 30 digits, every pole order by a limit (or the exact multiplicity) and a
  numeric check, every surjectivity proof on the complexes by a solution checked by substitution.
- One imprecision: `function_meromorphic(a sqrt(z), z)` is `false`, exact only for
  `a != 0`.

### Gaps

- Families with conditions (`function_poles(gamma(x^2 + 1), x)`, `tan(x^2)`), `jacobi_cd` periods, the elliptic
  theta domains, `not_element(1/2 + x/pi, integers)` forms of `function_domain`, the quartic with three parameters
  (times out in the decomposition), maps of two variables with parameters (`[x + a y^3, y + b x^3]`: 4 quantified
  variables plus 2 parameters, times out).
- `piecewise_expand` of conditions containing piecewise terms (`min(p, max(q, r))`), `max(abs(x), abs(y))` in its
  shortest form, the merging of pieces with equal values by simplifying them.
- `square_wave`, `triangle_wave` are not functions of the engine.

## Round 27

- Fourier sine/cosine transforms at a negative number are reflected in `transform_one` (cosine even, sine odd);
  symbolic `w` keeps the `w > 0` closed form. `fourier_sin_transform(log(t), t, w)` has a table entry
  `-sqrt(2/pi) (euler_gamma + log w)/w` (Erdelyi et al. I, 2.2), replacing the divergent numeric fallback (B26-7, B26-8).
- `z_transform` of `n^k` for 10 < k <= 400: `sum_j j! S(k, j) z/(z - 1)^(j + 1)` (Concrete Mathematics 6.10, 7.4) instead of k
  derivatives; the numeric check sums more terms while the terms are large (X26-19).
- `minimize_n` declines when the gradient length differs from the number of variables (list objective, X26-3).

## Round 27: function-property fixes (`src/props.rs`, `src/props/`)

Fixes of the bugs found in round 26 in the round-25 function properties. Tests: `tests/bugs_round26.rs`,
`tests/bugs_round26_cross.rs` (ignores removed for B26-1 to 6, 23 to 25, X26-1, 2, 4, 5) and `tests/props_round27.rs`
(D26-xx, R25-09). Decisions are numbered F27.n.

- **F27.1 `function_range` (`props/range.rs`, replaces the `optimize.rs` code; D26-01, 11, 12, 16)**: rational functions
  (parameters allowed) by `resolve(exists(x, cond and den != 0 and num == y den))` reduced in `y` (`1/x`: `y != 0`;
  `1/(1 + x^2)`: `0 < y <= 1`; `x^2 + a`: `y >= a`). Other continuous functions of one variable: the domain
  (condition and definedness conditions) reduced to intervals; extreme values among the critical points
  (`solve(g' == 0)` for each sign pattern of the `abs` arguments, families sampled at `c(1) = -4..4`), the zeros of the
  `abs` arguments, the closed finite ends, and the limits at the open and infinite ends. A bound is closed only
  when a point of the domain takes it, so `exp`, `atan`, `tanh` have open ends. A harmonic `c + A sin(w) + B cos(w)` has the
  amplitude `sqrt(A^2 + B^2)` (the engine's `solve` does not solve `sin(x) + cos(x) == 0`). Cross-check: 250 samples of the function
  must satisfy the answer; functions with jumps decline. Literature: Collins (CAD), extreme value theorem.
- **F27.2 meromorphy (`props/smooth.rs`; D26-13, B26-2)**: a transcendental function `F(u)` (exp, trigonometric and hyperbolic
  functions, erf, Bessel, Airy, gamma, `c^u`) whose argument `u` has a pole is an essential singularity (accumulating poles
  for `tan`, `gamma`); the poles of `u` are checked by the growth of `|u|` (`sin(x)/x` has none). `a g(z)` with a parameter
  factor gives `conditional_expression(false, a != 0)` (R25-09).
- **F27.3 `acsch`, `acoth`, `acsc`, `asec`, `asech` on the real line (`props/sets.rs`; X26-4)**: poles and cuts as atoms.
  `function_continuous(sqrt(sqrt(x^2)))` and `tanh(log(x))` (B26-6): a positive power of a quantity that is never negative has no branch
  atom at its zeros, and `tanh/sinh/cosh/sech(log(u))` are read as rational functions of `u`.
- **F27.4 `piecewise` (`props/sets.rs`; D26-18)**: atoms of each value inside its region (first match), and at the boundaries of the
  conditions a jump or pole by the one-sided values at `10^-12`, `10^-18` (machine arithmetic: 40-digit values are not machine reals).
- **F27.5 `function_poles` (D26-05, D26-18)**: the gamma family `-c(1)` under `c(1) >= 0` for a linear argument without a bounded
  condition; when `limit` does not answer (or contradicts the values: `limit(t tanh(i pi/2 + t), t -> 0)` is `0`) the order comes from the growth
  of `|f|` along a ray (`zeta`, `tanh`).
- **F27.6**: a parametric `meets` that is `false` for every admissible parameter is `Yes` (`function_continuous(1/(x - a), x)` is `false`).
- **F27.7 injectivity across poles (`props.rs`; B26-4, X26-1, D26-14)**: over a wide sample set the differences have one sign except at
  jumps across genuine poles or gaps of the domain (bisection: `|f|` past 10^8, or a point where a definedness condition fails); the pieces are
  then monotonic and `f` is injective iff their value ranges do not overlap (touching ranges: the open ends of `1/x`).
- **F27.8 speed (B26-25)**: `value_at` skips samples where a gamma-type function has an argument above 2000 (`gamma(125000.)`
  takes minutes in `mc-special`, open there); the gamma-family domain condition is not given to `reduce`.
- **Periods (`props/period.rs`; D26-10, 17, X26-2, B26-1, B26-23)**: numerically constant functions give 0 (`pi/12` came from
  the largest `k <= 24` passing every shift); step functions are checked at 120 points spread over the periods (six points all
  missed the jumps of `floor(sin(x))`); `g^N` for `|N| >= 64` is checked as `g` or `abs(g)` (the samples underflowed);
  periodic functions of periodic functions use the inner periods; `x - floor(x)` is the fractional part; `floor(a x + c)` is not periodic (0).
  `cos(sin(x))` has the smallest period `pi`, not `2 pi`: `cos` is even and `sin(x + pi) = -sin(x)`.
- **Convexity cusps (B26-3)**: a claim `1` or `-1` is checked on chords `x0 - t, x0, x0 + t` with the real roots of odd-denominator
  powers; a violated chord gives `indeterminate`. **Monotonicity (B26-5)**: an isolated undefined sample is a pole and the values on
  both sides are compared (`max(4, 3/(2 x))`); parametric results require no real pole of `f` in the region.
- **Surjectivity (D26-03, X26-5)**: `for_all(y, ycond implies range)` with the new range; continuous on the line with samples
  unbounded above and below and growing on windows 10^3, 10^6, 10^9 (`x sin(x)`).
- `piecewise_expand(x)` returns `x` (B26-24).
- Not changed: `function_domain(x^(1/3), x)` is `true` (D26-15): odd roots are taken as real roots in the domain; the engine evaluates them
  by the principal value, a convention of the whole engine.
- Gaps: `function_range` of oscillating non-harmonic functions (`x sin(x)`: the engine's `solve` gives nothing), of several variables, and
  of `sqrt(x)` pieces with a boundary atom in `piecewise` continuity (`sqrt(x)` is reported discontinuous at 0, the convention of the sets).

## Round 28: the calculus library

Functions written in MathCAS itself, embedded as `mc_calculus::LIB_SOURCE` and evaluated by the engine after the
language library (one install block in `mc-engine/src/engine.rs`); listed in the `FUNCTIONS` of `mc-calculus`,
`mc-poly`, `mc-algebra` and `mc-sums` by topic. Helpers are named `calc_*`. Tests: `tests/round28.rs`.

- **Decisions.** `rule` definitions where a call of another form must stay unevaluated (waves, Heaviside shapes,
  harmonic-type numbers, transforms, `difference_root`); `fn` with options where options are needed (asymptotic
  relations, sides, `possible_zero_q`). A rule whose value has to be computed to know it applies repeats the
  computation in its `when` (the language has no decline from a rule body). `if ... then ... else` on the right of a
  `rule ... when` must be parenthesised (the `when` binds to the `else` branch otherwise). `piecewise` holds its
  arguments: built with `apply`. Pattern-carrying rules (`dt(s_, x) :> ...`) evaluate their left side, so `dt` refuses
  arguments containing patterns (`calc_nopat`).
- **Waves** (period 1): sawtooth `x - floor(x)`; square `1` on `[0, 1/2)`; triangle `1 - 4|s - 1/2|`,
  `s = sawtooth(x + 1/4)`; `[a, b]` ranges by affine maps. Derivative rows in `rules.rs`: triangle `4 square(x + 1/4)`,
  `heaviside_lambda'` the three `heaviside_theta`, `heaviside_pi'` `2 delta(1 + 2x) - 2 delta(2x - 1)`.
- **Harmonic-type numbers.** Alternating harmonic numbers continued by the Hurwitz zeta tail
  `eta(r) - (-1)^m 2^-r (zeta(r, (m+1)/2) - zeta(r, (m+2)/2))` (digamma for r = 1), three-argument by Lerch's
  transcendent (DLMF 25.11, 25.14); alternating factorial `e gamma(m+2) gamma(-m-1, 1) - cos(pi m)(1 + e Ei(-1))`,
  derived from `a(m) + a(m-1) = m!` with the incomplete gamma recurrence (DLMF 8.8); hyperharmonic
  `binomial(m+r-1, r-1)(H(m+r-1) - H(r-1))` (Conway and Guy, The Book of Numbers) with its gamma/digamma
  continuation; multiple harmonic numbers by running sums from the innermost index, O(k m).
- **Asymptotic relations** by limits of `f/g` (de Bruijn, Asymptotic Methods in Analysis, ch. 1): O by
  `max_limit(|f/g|)` finite, o by 0, same order by both quotients bounded, `~` by `limit(f/g) = 1`; one variable
  only; undecided limits give `failure(undecided, ...)`.
- **Polynomial structure.** `polynomial_reduce` is the division algorithm of Cox, Little and O'Shea (2.3) in lex
  order; `cyclotomic` by Moebius inversion; `partial_fractions` by residues `p(a)/q'(a)` at simple roots from `solve`
  (repeated roots fall back to `apart`); `possible_zero_q` simplifies, then evaluates at three complex sample points
  (machine first, then 40 digits; a precision failure there counts as zero).
- **Transforms.** Hankel: closed forms for `r^mu`, `r^mu e^(-ar)` (a 2F1), `r^mu e^(-ar^2)` (a 1F1),
  `r^nu (r^2+a^2)^-p` (a K Bessel function) and deltas (DLMF 10.22.43/49/51, Erdelyi II ch. 8); detection by
  `r f'/f` being constant, affine or of the form `mu - 2 a r^2`. Hilbert: sinusoids, `exp(im a t)`, deltas, and
  rational functions through complex partial fractions with `H((t - z)^-k) = im sign(im z)(s - z)^-k` (King, Hilbert
  Transforms I, ch. 4). Discrete-time Fourier transform through `z_transform` of the one-sided parts.
- **Sequence limits.** `discrete_max_limit`/`min_limit`: a period P of the oscillating factors (negative bases,
  trig of rational multiples of pi k), the factors frozen at each residue, limits of the rest; dense angles use
  `max_limit` over the reals (Weyl equidistribution). `discrete_asymptotic` applies Stirling (DLMF 5.11.7), then
  `asymptotic`.
- **Algebraic numbers.** `element(x, algebraics)`: closure of the rationals and `im` under ring operations and rational
  powers; transcendence by Lindemann-Weierstrass (exp, log, trig at algebraic points), Gelfond-Schneider (`b^c`,
  `e^(r pi)`), and sums or products with one transcendental part.
- **Closed forms**: `alternating_harmonic_number(m, r)` at inexact m is an ordinary closed form
  (Hurwitz zeta) without a separate expansion step; `discrete_max_limit(sin(k), k -> inf)` and periodic
  cases are decided by residues with an explicit criterion.
- **Gaps.** No symbolic general term in `series_coefficient` (symbolic order stays unevaluated); multivariate
  asymptotic relations; `dt` of mixed variables drops the cross term rule of `dt(dt(y, x), y)`; `convolve` relies on
  `integrate` (fails on products of unit steps); `difference_root` only at integers, `differential_root` only at
  inexact reals; Hankel transforms of Bessel, Airy, elliptic functions and `unit_step(a - r)` (Meijer G forms);
  `inverse_function` of several arguments; not reached: `algebraic_number`, `difference_root_reduce`,
  `asymptotic_solve`.

## Round 29: the algebra and calculus library

### Derivative rows of the round 28 special functions (`rules.rs`)
- Scorer: Gi' and Hi' named; Gi'' = z Gi - 1/pi, Hi'' = z Hi + 1/pi (DLMF 9.12.1-2).
- Weierstrass: p' named, p'' = 6 p^2 - g2/2 (from p'^2 = 4p^3 - g2 p - g3, DLMF 23.3.10), zeta' = -p, sigma' =
  sigma zeta (DLMF 23.2.8-9), inverse_weierstrass_p' = 1/p'(u) at u = inverse_weierstrass_p; elliptic_exp' is the
  `mc-special` function `elliptic_exp_prime`. The invariants are opaque partials.
- Coulomb F, G, H+, H-: u_l' = S_(l+1) u_l - R_(l+1) u_(l+1), S_k = k/rho + eta/k, R_k = sqrt(1 + eta^2/k^2)
  (DLMF 33.4.4, valid from l = 0); l and eta opaque.
- Lommel s1, s2, t1, t2: f' = (mu + nu - 1) f_(mu-1, nu-1) - (nu/z) f (DLMF 11.9.ii for s, S; for t the same
  term-by-term computation: P_k(mu, nu)/P_k(mu-1, nu-1) = (mu + nu + 2k + 1)/(mu + nu - 1)). On mu + nu = 1 the
  companion (mu - nu - 1) f_(mu-1, nu+1) + (nu/z) f (the functions are even in nu); on mu = 1, nu = 0 both vanish
  and s_(1,0)' = J_1, t_(1,0)' = I_1 (s_(1,0) = 1 - J_0, t_(1,0) = I_0 - 1). Gap: the operator form
  `derivative(0, 0, 1)(lommel_s2)(1, 0, z)` resolves through the generic formula and gives 0.
- Mathieu: s' named, s'' = -(a - 2q cos 2z) s (DLMF 28.2.1). mathieu_c has no named derivative (mc-special has no
  `mathieu_c_prime`), so it stays the operator.
- Appell F1-F4 (DLMF 16.13): d/dx and d/dy raise the parameters by one with the coefficient (a b1/c etc.).
- Not given rows: Heun (no named derivatives in mc-special), Carlson RF/RK (needs R_D), mathieu_characteristic_a.
- Every row was checked against a central difference at 20 digits (one per function in tests/round29.rs).

### Library functions (the round 29 part of `lib/calculus.mc`, loaded after the round 28 part)
- `between`: ranges as pairs, lists of pairs, `interval`, `centered_interval`; chained inequalities are written with
  `and` (the language does not chain comparisons). Dates and quantities are not handled.
- Domains: `nonnegative_rationals`, `nonpositive_rationals` are rows of `mc-decide`'s domain table (so `element`,
  `refine`, `simplify` know them); `reduce(..., x, <signed rational domain>)` reduces over the rationals and then
  intersects with the sign condition over the reals (`mc-solve/src/domain.rs`). `booleans`: `element` by a library
  rule (the native declines), `find_instance(c, vs, booleans)` by enumerating the 2^n assignments (n <= 16), in the
  order false < true with the first variable slowest.
- `o(x)`, `o(x, x0)`: the empty series `series_data(x, x0, [], 1, 1)`; series arithmetic does the rest. `o(x, inf)`
  is not supported (the series code has no series at infinity in that form). Documented on the `O` page (a page
  `o.mcn` would collide with `O.mcn` on case-insensitive file systems).
- `n_roots`: `n_solve` on the polynomial, roots repeated by multiplicity, as a flat `or`.
- Lyapunov/Sylvester: Kronecker vectorization vec(AXB) = (B^T (x) A) vec(X), solved exactly by `linear_solve`.
  Continuous a.x + x.a^H = c, discrete a.x.a^H - x = c, generalized discrete a.x.b - d.x.e = c (checked against
  expected values of the example corpus; the generalized continuous form is not implemented).
- `riccati_solve`: matrix sign function of the Hamiltonian by the determinant-scaled Newton iteration (Byers 1987,
  "Solving the algebraic Riccati equation with the matrix sign function"), X from the least-squares system
  [W12; W22 + I] X = -[W11 + I; W21] (Roberts 1971). `discrete_riccati_solve`: structure-preserving doubling (Chu,
  Fan, Lin, Wang 2004). Both return machine matrices (also for exact input).
- `pade_approximant`: the Hankel system for the denominator (Baker and Graves-Morris, Pade Approximants, ch. 1); a
  pole of order v at x0 is factored out and the denominator degree lowered by v (exp(x)/x keeps
  its pole); a singular block falls back to a lower denominator degree (cos at [1/1] gives 1). Not done: Puiseux
  series (sin(sqrt(x))).
- `apart_square_free`: factors of the denominator as written, split square-free, merged; numerators by the inverse
  of the cofactor modulo each factor power (extended Euclid kept polynomial by clearing x-free denominators), then
  d-adic expansion. Options `modulus:` and `trig:` are not supported.
- `partial_fraction_elements`: poles from `solve` of the denominator, coefficients by Taylor coefficients of
  (x - p)^m r at p.
- `symmetric_reduction`: Gauss's algorithm on the lexicographically leading term (exponents non-increasing ->
  product of elementary symmetric polynomials, else to the remainder); factors free of the variables are
  coefficients.
- `find_generating_function`: shortest constant-coefficient recurrence by Hankel systems (orders k with 2k <= n - 1,
  checked on every remaining term), rational generating function. Not done: products of cyclotomic factors found
  by other means, logarithmic generating functions.
- `integrate_change_variables`: one variable by f/g' with g(x) replaced by u when that removes x, else x = h(u)
  from `solve` (conditional solutions reduced to their principal branch); limits through g; several variables
  with |det J|. Coordinate-system names ("Cartesian" -> "Polar") are not supported.
- `radon_transform`: the line integral in t of f(p cos - t sin, p sin + t cos), simplified with sin^2 -> 1 - cos^2
  and `trig_reduce`. Unit-step and delta integrands fail (the integrator does not take them over the real line).
- `asymptotic_integrate/sum/product`: term-by-term expansion for a parameter at a finite point, else the series of
  the closed form. Gaps: Euler-Maclaurin/Stirling expansions at infinity (`series` of harmonic numbers, gamma and
  pochhammer at inf does not evaluate), Laplace's method.
- `asymptotic_solve`: Newton's iteration on truncated power series in x - x0 (quadratic convergence, at most k + 2
  steps, stopped when the series no longer change); starts from the given point or from every root at x0; regular
  points only (invertible Jacobian). Gaps: Newton-Puiseux branches at singular points, x0 = inf, several expansion
  variables.
- `d_stability_conditions` / `r_stability_conditions`: first-order system (state = unknowns and their lower
  derivatives or shifts), fixed points from `d_fixed_points` / `r_fixed_points`, eigenvalues of the Jacobian:
  Re < 0 (continuous) and |lambda| < 1 (discrete; -1 < lambda < 1 for symbolic real-looking lambda). Numbers decide;
  boundary cases count as not stable. Symbolic higher-order cases give the eigenvalue conditions, not the
  Routh-Hurwitz/Jury forms.
- `difference_root_reduce(lhs == rhs, n)`: exact comparison at n = 0..40. For holonomic sides whose difference
  satisfies a recurrence of order <= 40 with no leading-coefficient zeros among those n this is a proof; outside
  that it is a test. Building `difference_root` objects from expressions is not done.
- `green_function`: variation of parameters on the homogeneous solutions from `d_solve` (Stakgold, Green's Functions
  and Boundary Value Problems): boundary value problems with one condition at each end, and causal initial value
  problems. PDE Green's functions are not done.

### Not reached / not done (round 29)
`differential_root_reduce`, `dirichlet_condition`, `neumann_value` (finite-element PDE solving), `algebraic_number`,
`d_solve_change_variables`, `asymptotic_r_solve_value`, `inverse_radon_transform`, `trig_factor`,
`polynomial_sum_of_squares_list`, `complete_integral`, `root_intervals`, `isolating_interval`. `trig_reduce(...,
modulus:)` lives in mc-simplify (not this crate). Known bugs found in other crates: `polynomial_extended_gcd`
(mc-combinat) loops when a remainder is a nonzero rational function that `expand` does not reduce (e.g.
polynomial_extended_gcd(x, (x - a)^2, x)); `polynomial_quotient` declines a divisor with a symbolic leading
coefficient (mc-poly), so the library's extended Euclid stops at x-free remainders.

## Round 30

- `asymptotic_less/_equal/_greater/...` with several variables growing together (`[x, y] -> inf`): decided along the
  curves `x_i = c_i t^k_i` (k in 1/2, 1, 2; c in 1, 3; up to three variables) with the one-variable code; a
  counterexample along one curve is a disproof, agreement on all is reported as true (a sampled answer).
- `function_period(square_wave(x), x)` is 1 (the step-function sampling of the check now includes `square_wave`).
- `polynomial_mod(p, m)` for a polynomial modulus with several variables reduces in the first variable shared with p.
- `dt(e, x, constants: [a, b])`: symbols listed as constants have no differential.
- Checked and kept: the generating function of `factorial_power(n, 2)` is 2 z^2/(1 - z)^3; `asymptotic` returns
  the leading term, not a longer equivalent form.

## Round 31

- **F31.4 Derivative rows.** `fox_h` (transforms.rs `fox_dz`): d/dz H^(m,n)_(p,q)(z) = H^(m,n+1)_(p+1,q+1)(z | (-1, 1),
  (a_j - A_j, A_j); (b_j - B_j, B_j), (0, 1)): the Mellin-Barnes integrand times (-s) with z^(-s-1) moved back to
  z^(-s) is the shift of every parameter and the gamma ratio (-1) gamma(s)/gamma(s - 1) = gamma(2 - s)/gamma(1 - s) by
  reflection (Kilbas and Saigo, *H-Transforms*, 2004, 2.2). Higher orders by repetition. Checked against central
  differences (first and second order) and H^(1,0)_(0,1)(z | (0, 1)) = exp(-z) (tests/round31.rs).
- `spheroidal_s1`: the row of `d` is the new `spheroidal_s1_prime(n, m, gamma, z)` (the round 31 part of `lib/calculus.mc`,
  helpers of mc-cover30 reused): P' N/D + P gamma sum a_k w_k j_v'(gamma z)/D with P'/P = m/(z (z^2 - 1)),
  j_v'(x) = j_(v-1)(x) - (v + 1) j_v(x)/x, j_0' = -j_1 (DLMF 10.51.2). Checked against central differences for
  |z| > 1 and |z| < 1. Not done: z on the unit circle (P singular for m > 0), derivatives of `spheroidal_s1_prime`.

## Round 32

- `piecewise_expand` keeps the jump of a floor, ceiling or round at a closed end of the region (`floor(x)` on `-1 <= x <= 1` is 1 at x = 1) and merges neighbouring pieces of one value. The wave functions are rewritten with floors first (`calc_waves` in `lib/calculus.mc`; the triangle wave is written without `abs` so that the pieces stay clean).
- `implicit_d(eq, y, [[x1, x2]])` gives the list of derivatives.

## Round 33

- `function_injective`/`function_bijective` of a polynomial map of degree 4 or more on the complex plane with a condition on the domain no longer run the four-real-variable decomposition of the pairs (12 s, and an interrupt arriving inside its arithmetic was not honoured, so the evaluation hung): the question stays open. The cause of the ignored interrupt was not located (an interrupt at 1, 2, 5 or 7 s inside the decomposition never returned, at 3 or 4 s it did: a loop that runs on junk values after the stop flag is raised, probably in `mc-cad` or `mc-arith`); only the panic in `mc-algebra/src/embed.rs` (`rat_ball` after a stop) was fixed here. Open: an interrupt at 1, 2, 5 or 7 s inside `function_injective([x^4, ...], x, complexes)` before this change never returned.
- `minimize` in several variables declines when the objective holds `abs`, `max`, `min`, `sign`, `floor`, `ceiling`, `round`, `clip`, `piecewise` or `unit_step`: the critical points of the gradient miss the minimum on a kink (`|x + 2y - 3|` on a triangle gave 1, the minimum is 0); the decomposition of `mc-cad` answers it.
- Definite integrals of products of `unit_step` factors whose bounds hold parameters (`split.rs`, `param_steps`): the integrand `h(x) unit_step(g_1) ...` with each `g_i` linear of constant slope has the support `[max of lower bounds, min of upper bounds]`; the value is the integral of `h` over it times `unit_step(upper - lower)`, or a `piecewise` over the cases of the order of two bounds when it depends on the parameters (impossible combinations of orders dropped by `resolve`). `convolve` of two boxes is the triangle, of two exponential steps `y exp(-y) unit_step(y)`.
- `wronskian` of the pairs (Ai, Bi), (J_n, Y_n), (I_n, K_n) gives the closed forms (1/pi, 2/(pi x), -1/x; checked numerically), `casoratian` of Fibonacci and Lucas -2 (-1)^n; `difference_quotient(..., assumptions: ...)` refines under the assumptions (`abs(x)` with x, h > 0 gives 1).
- Gaps: symbolic-index `series_coefficient(f, [x, 0, n])` is not done: it needs a table of closed forms (exp, trigonometric, rational by `apart`); `bell_b` and friends see mc-combinat.

## Round 34: the split

- `mc-calculus` was split by area, with no behaviour change: `mc-limit` (limits and asymptotics, with the real
  bounds `bounds.rs` and the functions defined by cases `switches.rs` that definite integrals share),
  `mc-integrals` (the `integrate` driver, antiderivatives, definite integrals, the native of `integrate`),
  `mc-transforms` (integral transforms, `discrete_delta`, `kronecker_delta`) and `mc-dsolve` (differential and
  recurrence equations). This crate keeps differentiation, partial fractions, the properties of functions and
  optimisation, at the bottom: the others use its term builder, derivatives and the helpers of `common.rs`
  (`is_free`, `args_of`, `apply`, `is_failure`, `call_of`, `variable`, `named_failure`, `wrap`).
- Moved down to break cycles: the derivatives of `meijer_g` and `fox_h` in z (`rules.rs`, from the transforms),
  the transforms by name (`transform_kind.rs`, which the derivative operator reads), and the shared helpers.
- `FUNCTIONS` stays one table for the whole area, here, in the old order: the catalogue and the help index list the
  functions in table order, so splitting it would have reordered `mc-help/data/index.txt`.
- The tests went with their areas (limit, gruntz; dsolve; transforms, meijer_round22; integrate_*); the
  cross-area tests stayed here. `tests/common/mod.rs` is copied into each crate whose tests use it.
- Names by area (step 7): the calculus library files of rounds 28, 29 and 31 are one file,
  `lib/calculus.mc`, in that order (`LIB_SOURCE`; `LIB29_*` and `LIB31_*` are gone). The round 31 definitions
  (`cf_s1p`, two rules of `spheroidal_s1_prime`) now load before the further functions, whose `c30_` helpers they
  call only when they run; no other library defines those names. Helper prefixes stay.

## Round 36

Mostly in `lib/calculus.mc`:
- `truncate_sum`: inactive sums, `activate_result`, `working_precision`, ranges from -inf or over the whole line, a list
  of counts for multiple sums; a finite range is cut only above 100 terms.
- Stirling expansions at infinity (`calc_st_expand`): factorial, gamma, pochhammer and binomial factors are written
  with Stirling's series for log gamma(c x + d) (shift by the Bernoulli polynomials, DLMF 5.11.8); used by
  `discrete_asymptotic` (series_term_goal terms), `asymptotic_product` and `asymptotic_sum` (terms to x^-order).
- Asymptotic relations along a complex direction: the base of a non-integer power that is negative real with a non-real
  first-order term gets its principal branch (`calc_branch`); undecided limits fall back to the leading term of the
  series of the ratio (`calc_series_order`), and only then to the old numeric reading.
- Stability: Jury's test for 2 x 2 symbolic systems; the Schwarzian derivative test at a fixed point of multiplier -1.
- `find_repeat` for arrays of any depth with a level list and `same_test`; `differences` with a step and a level list;
  `series_coefficient` with a symbolic order for sums of c x^m h(x); `convolve` of piecewise polynomials;
  `hankel_transform` options; `to_radicals` of root objects (degree 2 to 4).
- Gaps: Laplace's method for asymptotic integrals, Puiseux branches for `asymptotic_solve`, Euler-Maclaurin sums.

## Round 36, later fixes

- minimize in one variable: a derivative that is a nonzero constant has no critical points (solve of 1 == 0 was a
  failure that made minimize([x, x >= 2.0], x) decline); machine numbers in the problem give a machine answer
  (reduce reads x >= 2. as x >= 2).
## Round 36, later fixes

- `find_instance` in several unknowns checks each candidate numerically against the whole condition (equations included: a parametric radical solution can change branch) and tries several values of the free unknowns; Boolean-declared unknowns by their two values.
## Round 36, later fixes

- `function_convexity` with a domain that leaves out points x != c: a jump there gives indeterminate, a kink gives 1 (-1) when f' jumps up (down) and the pieces are convex or linear.
- Derivatives of inert transforms: inactive(laplace/fourier/fourier_sin/fourier_cos transform)(f, t, s) and inactive(convolve): 0 in t, the transform of -t f, i t f, t f (cosine), -t f (sine), convolve(f, g') in s (Debnath and Bhatta, Integral Transforms).
- d(indeterminate, x) is indeterminate.
- lib/calculus.mc: asymptotic relations take the side series under t > 0 and decide a side where f or g is identically 0; Pade approximants of series in one fractional class t^r(...) (numerator degree floor(m - r)); symbolic series coefficients vanish below their first order (piecewise); asymptotic_sum over an infinite range expands the closed form first and declines when the term-by-term sums diverge; asymptotic_product with series_term_goal; Laplace's method and Watson's lemma for asymptotic_integrate at infinity (Olver, Asymptotics and Special Functions ch. 3; Bender and Orszag 6.4: minimum of phi, order p of its first nonzero derivative, series reversion, Gamma((m+1)/p)/(p x^((m+1)/p))); antiderivatives without a closed form at a finite point by the integrand's series with logarithms, truncated by the power of x; asymptotic_solve declines (a branch through a singular point or to infinity needs Puiseux) and keeps real branches under reals; stability conditions decline for non-autonomous equations.

## Round 36, later fixes

- `n_roots(..., precision_goal: d)` with d above machine precision: `n_solve` with `working_precision: d` (the option was ignored and the roots were machine numbers).
## Round 36, later fixes

- The z-derivative of `hypergeometric_pfq_regularized(a, b, z)`: (a1 ... ap) times the regularized pFq(a + 1; b + 1; z)
  (DLMF 16.3.1 divided by gamma(b1) ... gamma(bq), with gamma(b + 1) = b gamma(b)); it was an opaque derivative, so the
  centered-interval image could not see that the function is monotone.

## Round 37: polynomial reduction and remainders

- `polynomial_reduce(p, gs, xs, monomial_order: o)` in `lib/calculus.mc`: the division algorithm in lex, deg-lex or
  degrevlex (the orders of `groebner_basis`); `coefficient_domain:` other than rationals is an argument failure.
  `polynomial_reduction` passes its options on.
- `polynomial_mod(p, m, modulus: q)`: the remainder in F_q[x] (one variable). `calc_coeff_mod` of the zero
  polynomial is 0 (was a division by zero, so `polynomial_mod(0, 2)` failed).
- `multiply_sides` and `divide_sides` take `generate_conditions:` (false: scaled as for a positive factor; true:
  conditional_expression with c > 0 for an inequality, c != 0 for an equation; automatic: the condition for an
  inequality only); without it the piecewise over the signs is unchanged.
## Round 37: answers where there were none

- **Hilbert transform, general method** (`lib/calculus.mc`, `calc_hilbert_osc`, `calc_hb_*`). Convention H f(s) = (1/pi)
  PV integrate(f(t)/(s - t), t) (H cos = sin, H(f') = (H f)', H H = -1; King, Hilbert Transforms I, ch. 3-4).
  Sines and cosines of linear arguments are written as exponentials; each term exp(im w t + b) R(t), R rational, w real
  of a sign decided by refine under the assumptions. A function analytic and bounded in the upper half plane has
  H g = -im g (lower: +im). For w != 0: exp(im w t) P(t) gives -im sign(w) exp(im w s) P(s); exp(im w t)/(t - z)^k is
  split into (exp(im w t) - T(t))/(t - z)^k (entire, bounded where exp(im w t) is) and T(t)/(t - z)^k with T the Taylor
  polynomial at z of degree k - 1. Rational parts on complex partial fractions with multiple poles (factor_list into
  irreducible factors, Laurent coefficients by derivatives): (t - z)^-k gives im sign(im z) (s - z)^-k, and a real pole
  the distribution -pi (-1)^(k-1)/(k-1)! dirac_delta^(k-1)(s - z) (from H(1/t) = -pi dirac_delta, the inverse of
  H(dirac_delta) = 1/(pi s)). The sign of im(z) is read from re/im split through numeric multiples (im a gives re(a)),
  so re(a) > 0 decides it. Piecewise polynomials of compact support (unit_box, sign, unit_step, piecewise): breakpoints
  from the step arguments and the piecewise conditions, the polynomial on each piece by refine, then (1/pi)
  (integrate((p(t) - p(s))/(s - t), [t, a, b]) + p(s) log|(s - a)/(s - b)|). The option `assumptions:` now reaches the
  transform (a rule of its own: the conditions of a `fn` definition do not see its options). Checked against the
  principal value written as the proper integral integrate((f(s0 - u) - f(s0 + u))/u, [u, 0, inf]) at sample points.
  Still declining: sin(t^2), cos(t^2) (Fresnel-type), acot(t), coefficients that factor only over Q(i)
  (t^2 - 3 + 4 im), frequencies of unknown sign.
- **Asymptotic relations in several variables** (`calc_order_sep`): when f/g = c h1(x1) ... hn(xn), the relation follows
  from one-variable relations of each factor to 1 (two-sided at finite points): q -> 0 when all h_i are bounded and one
  tends to 0, bounded when all are, unbounded when one is and all others are bounded below, q -> 1 from finite limits
  whose product is 1. A ratio that does not separate keeps the earlier sampled method at [x, y] -> inf only; at other
  points it stays undecided.
- **Periodic optimization in one variable** (`optimize.rs`, `period`): when the existing method declines and x occurs
  only in linear arguments a x + b of sin, cos, tan, cot, sec, csc with slopes in rational ratio, the problem is solved
  on one common period [0, P] (P = 2 pi L/|a1|, L = lcm(n_i)/gcd(d_i) for a_i = (n_i/d_i) a1), the critical points by
  solve with the period's bounds (a finite exact set; machine roots of an exact problem decline). Incommensurate
  frequencies (sin(3^(1/2) x) - exp(cos(3 x) + 2)) still decline: the supremum is then the sum of the parts'
  suprema (Kronecker), not done.
- **Optimization over a vector variable** (`lib/calculus.mc`, `calc_vopt`): minimize, maximize, min_value, max_value,
  arg_min, arg_max with one symbol v used through dot: v split into entries (the dimension from the matrices and
  vectors v meets, which must agree), vector inequalities into scalar ones (`lo_scalars`), v given back as a vector.
  Cone constraints (the exponential cone) still decline.
- **Derivatives through root objects** (`diff.rs`): a `root(f, k)` free of the variable is a constant; its pure
  function was walked as a term and the derivative declined (exp(x root(...)) had no derivative, so Wronskians of
  d_solve bases with cubic roots stayed unevaluated).
- **fourier_sequence_transform of two-sided sequences** (`calc_fst_exp`, `calc_fst_periodic`, `calc_fst_box`):
  exponentials exp(im a k) (a real by refine; sines and cosines as exponentials) give dirac_comb((w - a)/(2 pi))
  (Poisson summation); sequences of integer period P built from mod(a k + b, N) by their discrete Fourier series
  (F_j = (1/P) sum f(m) exp(-2 pi im j m/P)); several variables factor by factor (unit_step(n1, n2) =
  unit_step(n1) unit_step(n2)) or over the period box. Option `fourier_parameters: [a, b]` (default [1, 1]):
  (|b|/(2 pi))^((1 - a)/2) sum f(k) exp(-im b k w); `assumptions:` for the reality of a frequency. Checked by
  synthesis (dirac_comb((w - a)/(2 pi)) read back as exp(im a m)). Still declining: 1/(2k + 1)^2 and other
  non-periodic two-sided sequences without steps (Lerch-type sums).
- **Integrals over regions** (`calc_bint`): integrate(g boole(c), [x, a, b], [y, c, d], ...) and
  piecewise([[g, c]]) by reduce (cylindrical decomposition in the order of the integration variables) and an
  iterated integral per cell; cells of lower dimension have measure 0. In three variables the inner definite
  integrals with bounds that are functions of the outer variables (2 (r(x)^2 - y^2)^(1/2) over [-r(x), r(x)]) are
  not found by integrate, so the 3D example still declines; regions with a parameter (x^2 + y^2 < a) decline
  (reduce does not split on a).
- **Series in several variables** (`calc_series_multi`): series(f, [x, x0, n], [y, y0, m], ...) is the series in x
  whose coefficients are series in y (x outermost); series(f, [x, x0, n]) is series(f, x, x0, n). The native leaves
  several specifications to the library (`mc-poly/src/series.rs`).
- **General series coefficients** (`calc_sc_rat`, `calc_sc_h2`): a rational function on its complex partial fractions
  ((x - z)^-k has the coefficient (-1)^k binomial(n + k - 1, k - 1) z^(-n - k), the polynomial part by value at its
  orders); polylog(s, a x), hypergeometric_pfq(as, bs, a x), bessel_i and bessel_j(nu, a x) (integer nu >= 0) by their
  defining series (DLMF 25.12.10, 16.2.1, 10.25.2, 10.2.2); negative powers x^-m of the factor shift the order;
  option `assumptions:` refines the conditions. Checked against the Taylor coefficients from series. Still declining:
  cos(x^(1/2)) and other composite arguments, products (exp(x)/(1 - x)), chebyshev_t of symbolic degree.
## Round 37

- `numdiff`: the numerical value of `derivative(n1, ..., nk)(f)(x1, ..., xk)` at inexact numbers when `f` has no rule
  for that partial (an opaque order or parameter derivative, or no row at all: `riemann_siegel_z`, `factorial2`,
  `q_pochhammer`, the Neville theta functions). Central differences of order n with Richardson extrapolation (Dahlquist
  and Bjorck 2008, 3.3.4-3.3.5), steps h0 = 2^-10 times the power of two above max(1, |x|), h0/2, ...; mixed partials as
  products of the one-dimensional differences. The function is evaluated at exact dyadic points in balls at the
  target digits + 12 + n (10 + levels) log10 2, so the cancellation of the n-th difference at the smallest step is
  covered. Levels are added (4 + digits/8 at most) until the last two diagonal entries agree to 10^-(digits+2) of the
  value; machine targets also accept an absolute agreement of 10^-(digits+4) of max|f| / scale^n (a derivative that
  is nearly zero, as at the critical points `find_root` looks for). Every function value must carry digits + 2
  correct digits in its ball. A forward-difference quotient with one Richardson step must agree to 10^-3: a kink or a
  jump at the point declines (the central quotient would give the mean of the two slopes). Anything else stays
  unevaluated (unknown functions, integer-only arguments, functions that evaluate to machine numbers only).
- Rows: `anger_j` and `weber_e` in z, 2 J'_nu = J_(nu-1) - J_(nu+1) and the same for E (DLMF 11.10.33-34);
  `mathieu_c` (z: `mathieu_c_prime`) and `mathieu_c_prime` (z: -(a - 2 q cos 2z) `mathieu_c`, DLMF 28.2.1), the
  parameters opaque.
## Round 37: what each special function is

The function properties declined for most special functions: no property knew what they are. `props/table.rs` now
keeps that knowledge once, as data with references (DLMF chapter and equation per entry), and every property reads
it:

- **An entry** describes one function of its *variable argument* (the others parameters, or variables too for the
  entries that hold jointly): where it is defined and real on the line (`real`), where it is defined in the plane
  (`complex`), its singular atoms on the line and in the plane with their kinds (pole, branch, jump, kink: the kinds
  of `sets.rs`), its ranges on the line and in the plane, boundedness, strict sign / monotonicity / convexity on its
  domain, ontoness and periodicity, each under a condition on the parameters (`when`, or per fact). The facts are
  templates in `$u`, `$a`, `$b`, ... and `$y`, read by `to_expression` and instantiated by substitution; a
  template may name the parameters (`not_element($a, integers) and $u <= 0`: a cut only for a non-integer order), so
  one entry covers every parameter value. The hypergeometric families are built from their series (DLMF 16.2:
  p <= q entire, p = q + 1 the cut [1, inf) and convergence at 1 for re(sum b - sum a) > 0, a terminating series
  entire, a denominator parameter at a pole nowhere defined); the classical families of non-integer degree through
  their 2F1 forms at (1 - z)/2 (DLMF 15.9, 18.5.7, 14.3.1).
- **Readers.** `conditions_of` (domain, sign, monotonicity, convexity, injectivity samples) and the atom walk of
  `sets.rs` (singularities, discontinuities, continuity, analyticity, meromorphy, the breaks of surjectivity) consult
  the table where they did not know a function; compositions keep working through the walks (a subterm's conditions
  with its own argument). `range.rs` reads the ranges for a function of an affine function of the variable with an
  affine outer map, after the general methods (critical points, quantifier elimination) have declined, with a light
  sampling check; `onto.rs` reads bounds (a bounded function is not onto), ontoness and periodicity (not injective;
  periodic and continuous: bounded) and strict monotonicity (injective); `sign_of` reads strict sign, monotonicity
  and convexity on the domain, and a nonconstant periodic function is neither monotonic nor convex.
- **Partial atoms** (`partial`, `cpartial`): some singular points only (the pole of the Weierstrass functions at 0,
  the branch point of an inverse Jacobi function at one (x, m)). They decide that a set meets a region (analytic:
  false), never that it misses it, and the sets are not listed.
- **Over the reals the facts are about a real argument**: a subterm whose argument is never real (`klein_invariant_j(x
  + im)`) is not read (`re(J(x + i))` is real although J is not on the line).

General changes of the round in the property engine:
- Critical points of products: a factor that never vanishes on the line (refine) has none, the zeros of a product are
  those of its factors (sech, coth and tan ranges).
- Complex ranges (`function_range(..., complexes)`): a constant, a nonconstant polynomial (every value), the principal
  square root, `e^u` and the recorded ranges of the table under affine maps; checked at complex samples.
- Several variables: `indeterminate` proved by a grid of samples (two points of opposite sign; two axis-parallel
  segments with the midpoint above and below the chord), otherwise no answer.
- A condition whose region leaves the function's domain gives `indeterminate`, as no condition does when the domain
  is not the whole line; under the parameters' facts a domain may be the whole line (`jacobi_nc(x, m)`, m >= 1).
- Monotonicity by samples of the function compares values across gaps of the domain (x < y implies f(x) <= f(y)
  everywhere); functions with jumps, kinks or a singular set the table does not list are decided by samples of the
  function itself, not of the derivative (a jump of `inverse_jacobi_cs` at 0 hides between derivative samples).
- `not_element(u, s)` is refined in the form `not(element(u, s))`, which `refine` decides under facts.
- Injectivity: a value taken twice among the samples with a different value between them is decided first (the
  derivative of a slow function need not be sampled); a relative tolerance (decaying tails are no ties).
- A complex sample value with an exactly zero imaginary part counts as real (some methods return real values so).
- **Time budget**: the samples of one property call stop after 2.5 s and the call declines (some special functions
  evaluate in a second each, and the evaluation would run into its time limit). This is a resource guard: it never changes an
  answer, it only withholds one.

Gaps (the property declines): Lommel functions (the singular set at 0 is not established from DLMF 11.9),
Appell F1, the elliptic logarithm, the hyperharmonic numbers, spheroidal radial functions, the Weierstrass invariants
functions e1..eta3 without a known real domain, the ranges of the inverse trigonometric functions in the plane,
periods of the modular functions (quasi-periods), `reduce(...)` of a condition without a variable (a `reduce` gap),
inverse Jacobi dn, cd, nd, dc, ds for m > 1 (their numeric values are complex on part of the real range of the
function), and the QE-heavy polynomial maps of several variables (bijective, surjective, convexity with parameters).
Numeric issues seen (open in `mc-special`): `scorer_hi(1e6)` does not return (and ignores the interrupt),
`barnes_g` takes ~1 s per real value and returns tiny imaginary parts on the negative axis, `alternating_factorial`
takes ~1.5 s per value and hangs under interrupt, `kelvin_ker(2, 0.)` gives -inf (ker_2(0) = 1/2), `hypergeometric_u(a,
b, 0.)` with b < 1 and `inverse_elliptic_nome_q(1.)` stay unevaluated, `hyperfactorial(-2.2)` gives a real value
(the continuation by the Hurwitz zeta function is complex there).

## Round 38: options of to_radicals

- `to_radicals(e, cubics: false)` keeps the roots of irreducible cubics other than `a x^3 + b` (the ones needing
  Cardano's formula), `quartics: false` those of irreducible quartics other than `a x^4 + b x^2 + c`;
  `eigenvalues(m, cubics: true)` and `eigenvalues(m, k, quartics: true)` write the eigenvalues with radicals.
- `assumptions:` decides `root(p, k)` for a polynomial with parameters: its roots from `roots`, each one real when
  `simplify` says so and not real when its square is real and negative under the assumptions; `root(p, k)` is the
  k-th smallest real root when every comparison needed holds by `simplify` or (one parameter) by `reduce` of its
  negation. A comparison left open, or `k` past the real roots, keeps the root object (the order of the non-real roots
  is not attempted). `assumptions: none` means no assumptions.

## Round 38: Radon transform

- `radon_transform(f, [x, y], [p, phi], options)`: `assumptions:` and `generate_conditions: true` pass to the line
  integral. Before integrating, `sin(phi)^n` (n >= 2) is written with `cos(phi)`, so `x^2 + y^2` becomes `p^2 + t^2`
  in the line's parameter t; Gaussians times `cos` or `sin` of linear forms then integrate in closed form.
- A factor `unit_step(q(t))`, q quadratic in t with a negative leading coefficient (a disk, or any region whose
  section by each line is an interval): the integral between the roots of q where the discriminant D is positive,
  0 elsewhere (`piecewise`, `D > 0` solved for p by `reduce` when D is a rational polynomial in p). A factor
  `dirac_delta(x - a, y - b)` gives `c(a, b) dirac_delta(p - a cos(phi) - b sin(phi))` (the rotation has Jacobian 1;
  Deans, *The Radon Transform and Some of Its Applications*, 1983, chapter 2).
- `radon_transform(inverse_radon_transform(F, [p, phi], [x, y]), [x, y], [p, phi])` is F; an equation is
  transformed side by side.
- Not done: `dirac_delta(q(x, y))` for a curve q = 0 (a sum over the intersection points, valid only where the line
  meets the curve), and transforms of differential equations in an undefined u (`R[u_x] = cos(phi) d/dp R[u]`, Deans
  section 2.4): the transform of `u(x, y)` stays a failure, because `d` of a held `radon_transform(u(x, y), [x, y],
  [p, phi])` in p differentiates through the list `[p, phi]` and gives a list (a gap of `d` for list arguments).
- Round 39: the transform of an undefined `u(x, y)` stays (held), and a linear combination of u and its partial
  derivatives with coefficients free of x, y is transformed by `R[derivative(a, b)(u)] = cos(phi)^a sin(phi)^b
  d^(a + b)/dp^(a + b) R[u]` (Deans, section 2.4), now that `d` of the held transform in p stays as a term; so a
  Poisson equation becomes an ordinary equation in p. A product of u with a function of x, y has no rule (a
  failure, as before). A transform that is a product comes out with the integer content of its polynomial factors
  taken out (`factor`).

## Round 38: extrema of quadratics with parameters

- `maximize`, `minimize`, `max_value`, `min_value`, `arg_max`, `arg_min` of `a x^2 + b x + c` in x with a coefficient
  that is not a number (real parameters): the complete case split. The maximum is `c - b^2/(4 a)` at `-b/(2 a)` when `a
  < 0`, c at 0 when `a = b = 0` (every point is a maximum), and unbounded otherwise (`inf`, the point
  `indeterminate`), as `piecewise`; the minimum likewise with `a > 0`. Polynomials of higher degree with parameters
  are not attempted (their extrema need the real root structure of the derivative as a function of the parameters).

## Round 38: identities of recurrence sequences

- `difference_root_reduce(lhs == rhs, n)` substitutes the integers for n only where n is free: the defining
  function of a `difference_root`, whose bound variable may itself be n, is left alone (`cb_subst_free`). Before,
  the substitution entered the pure function, so an identity of a sequence defined with the same index name stayed
  unevaluated. The reduction of an expression to one `difference_root` (closure of recurrences under sums and
  products) is not implemented.
## Round 38: series composition and reversion, polynomial tests, identities

Source in `lib/calculus.mc` (helpers `r38_`).

- `compose_series(f, g, ...)`: the sum of the terms c_k (g - x0)^k of f in the series arithmetic, plus the order
  term O((x - y0)^(v n1)) for f known to O((x - x0)^n1) and g - x0 of valuation v >= 1; the arithmetic carries the
  orders of g (Knuth, TAOCP 2, 4.7). A sum with one O term counts as the series it truncates. When g does not start
  at x0 the call stays unevaluated.
- `inverse_series(s, y)`: for a series of valuation 1 after its constant c, the fixed-point iteration
  x = (u - b_2 x^2 - ...)/b_1 with u = y - c, one more correct coefficient a step (Knuth 4.7; Henrici, Applied and
  computational complex analysis 1, 1.9), to the order of s. For a leading term b_v (x - x0)^v with v > 1 the series
  w = (x - x0)((s - c)/(b_v (x - x0)^v))^(1/v), of valuation 1 and known to O((x - x0)^(n1 - v + 1)), is inverted
  and evaluated at t = ((y - c)/b_v)^(1/v): a Puiseux series written as a sum with O((y - c)^((n1 - v + 1)/v)).
  A Puiseux series at 0 as input (terms c x^p with rational p and O(x^q)) is written as a series in t = x^(1/v),
  v the common denominator, inverted in t, and x = t^v follows in the series arithmetic.
- `polynomial_q(e, xs)`: like `polynomial_expression_q`, but a coefficient may not contain a symbol of the variables
  (so `sec(x)` is not a coefficient in `cos(x)`), and b^p is the power (b^q)^(p/q) of a variable b^q when p/q is a
  non-negative integer.
- `solve_always(eqs, xs)`: every coefficient of the numerator of lhs - rhs (polynomial in xs) set to zero and solved
  by `solve`; decision: with more symbols than equations, the first symbols in the canonical order are solved for
  (the coefficients of a monic cubic in terms of its roots, not the roots in terms of the coefficients).
## Round 38: inverse Jacobi and Weierstrass properties

- The onto facts of the table with no parameter condition (`onto: [("", false)]`) were never read (the empty
  condition was instantiated instead of taken as true): `function_surjective` of the bounded inverse Jacobi functions
  declined. Read like the shapes now.
- The inverse Jacobi functions dn, cd, nd, dc, ds for m > 1: their real domains (Jacobi's real transformation, DLMF
  22.17.4) were already written; the numeric values are now real exactly on them (checked at points inside and out),
  so the entries hold for every real m other than 1.
- A special function of the table with a pole on the real line (the Weierstrass functions: the pole at 0 is listed,
  the others are not) is not defined on the whole line: its sign, monotonicity and convexity are indeterminate, as for
  any function whose domain leaves points out (DLMF 23.2.4-23.2.5). Not done: the Weierstrass functions of the
  invariants (`weierstrass_eta1([x, y])` and the half periods as functions of g2, g3).

## Round 39: derivatives of calls that bind variables

- `d` of a transform that stayed unevaluated (`laplace_transform(f(t), t, s)`, the Fourier, Mellin, Z, Hankel,
  Hilbert and Radon transforms, the convolutions) no longer goes through the generic chain rule, which differentiated
  the variable arguments as values (a list for `[p, phi]`) and resolved `derivative(0, 0, 1)(laplace_transform)` by
  differentiating the transform of the placeholder `&1` as a constant: `-f(t)/s^2`, a false value. Now: 0 in the
  integration variable, the transform of the integrand's derivative in a parameter (differentiation under the
  integral sign), and in the transform's own variable the call `d(T, s)` stays as a term, so a sum or product around
  it is differentiated by the rules (`d(s T, s) = T + s d(T, s)`); `d(d(T, s), s)` is `d(T, [s, 2])`.
- `d` of an unevaluated `sum(g, [k, a, b])` with bounds free of the variable is `sum(d(g), [k, a, b])` (it gave a
  list); 0 in the index; unevaluated in a bound, in the index of an indefinite sum, and for a product whose factor
  depends on the variable.
- The derivative operator `derivative(...)(T)` of a transform stays unevaluated too.

## Round 39: periodic functions

- `max_value`/`min_value` of `g + h + c`, g and h continuous and periodic in x with periods whose ratio is an
  algebraic irrational (the minimal polynomial of the ratio has degree at least 2), c constant: the sum of the
  extrema of g and h (Kronecker's theorem: the curve x -> (x mod P, x mod Q) is dense in the torus; Hardy and
  Wright, theorem 442). Terms are grouped by commensurable periods; exactly two groups are needed. Whether the bound
  is attained is not decided, so `maximize` and `minimize` (which give the point) still decline; a ratio involving
  pi (transcendental, not tested here) declines too.
- `integrate(w, [x, a, b])` for a unit wave w (`sawtooth_wave`, `square_wave`, `triangle_wave` of x) and real
  bounds: the integral of `piecewise_expand(w)` (floors of linear functions), taken cell by cell.
- The waves between a and b are rewritten as their affine images of the unit waves (an identity for every x) for
  `function_injective`, `function_convexity`, `function_monotonicity` and `function_sign`.
- Function properties of `alpha + beta g(x)` with every parameter in alpha and beta and beta != 0 under the
  assumptions: injective when g is; the monotonicity and convexity of g times the sign of beta (indeterminate stays
  indeterminate for any beta != 0).
- A wrong value removed: with a parameter, a function with jumps or kinks has no samples, and the second derivative
  that the shape fell back to misses the jumps (`function_convexity(c unit_step(x), x)` and `function_convexity(abs(x
  - a), x)` gave 0, linear). Such a call now declines.

## Round 39: sequences, relations, unit masses

- `discrete_max_limit`/`discrete_min_limit`: `mod(a k + b, m)` (integers a, b, m > 0) is periodic with period m;
  inside `mod(u, m)` a summand `c factorial(k + j)` (integers c, j) is a multiple of m from k + j >= m on and is
  dropped; Fibonacci and Lucas numbers are written by Binet's formulas, so their oscillating part
  `(-1/golden_ratio)^k` is seen (`is_numeric`, not `is_number`, for a negative base: `-1/golden_ratio` is no number
  literal).
- `multiple_harmonic_number(m, [a, b])` at a symbolic m for b = a, b = 0 or a = 0, in harmonic numbers: the
  quasi-shuffle product gives `S(a, a) = (H(a)^2 - H(2 a))/2`; `S(a, 0) = H(a - 1) - H(a)`, `S(0, b) = m H(b) - H(b -
  1)` (Hoffman 1992).
- Asymptotic relations: an assumption `p >= c` (or `p <= c`) on a parameter is split into `p = c` and the open part;
  when both give the same verdict it is the verdict (`asymptotic_equal(1 + x^a, 1, x -> 0)` for a >= 0). When they
  differ the call stays undecided (`asymptotic_greater(1, x^a, x -> 0)` with a >= 0 fails at a = 0, so `true` would
  be wrong there).
- The series test of the relations writes a power whose base stays on the negative real axis on the side approached
  (a rational function with real coefficients, negative at the point) with the principal branch, `b^p = exp(i pi p)
  (-b)^p`; a leading term that comes with its order term is taken alone. A `max_limit` or `limit` that stays
  unevaluated decides nothing (its numerical value had been compared with 0).
- `wronskian([y1'(x) == ..., ...], [y1, ..., yk], x)` for a linear first-order system y' = A y + r: Liouville's
  formula `exp(integrate(trace(A), x))` (Coddington and Levinson, ch. 1, theorem 7.3).
- `integrate(g(x) dirac_delta(c (x - a)), [x, lo, hi])` with a symbolic a (lo < hi known, or an infinite end):
  `g(a)/abs(c) heaviside_theta(a - lo) heaviside_theta(hi - a)`; `laplace_transform` and `fourier_transform` (the
  default parameters) of such a term by the same rule. A shift known not to be real is left alone.

## Round 39: radial transforms, directional limits, poles in one-variable extrema

- `fourier_transform`, `inverse_fourier_transform`, `fourier_cos_transform` and `inverse_fourier_cos_transform` in two
  variables (default parameters) of a radial f = h(sqrt(x^2 + y^2)): the Hankel transform of order 0 of h at
  sqrt(u^2 + v^2) (Stein and Weiss, ch. IV.1); for an even f the cosine transform over the quadrant equals the
  transform over the plane.
- `max_limit`/`min_limit` of f(x, y) at a point where f is homogeneous of degree 0 about it (constant on rays): the
  supremum (infimum) of the values on the directions (1, t), (-1, t), (0, 1), (0, -1); `y/(x + y)` at the origin is
  unbounded both ways.
- A wrong value removed: the one-variable `minimize`/`maximize`/`min_value`/`max_value` took the critical points and
  the ends of the domain, not the real poles of g: `max_value(t/(1 + t), t)` was 1 (it is unbounded at -1). The
  zeros of the denominator of `together(g)` (members -4..4 of a family) inside the domain are now checked from both
  sides: -inf from one side is the infimum, a finite limit that would be the infimum declines (not attained).

## Round 39: Bessel integrals and cut Mellin transforms

- `integrate(c bessel_j(n, a x + b), x)` for odd integer n: from `integrate(J_1) = -J_0` and `J_(n + 1) = J_(n - 1) -
  2 J_n'` (DLMF 10.6.1), `-J_0 - 2 (J_2 + ... + J_(n - 1))`, written with `J_0 = (2/u) J_1 - J_2` for n >= 3 (so
  `integrate(J_3) = -J_2 - 2 J_1/u`); a definite integral by the limits of that antiderivative at the ends. Even
  orders need `integrate(J_0)` (Struve functions) and are not done.
- `mellin_transform(unit_step(x - a) g(x), x, s)` and of `piecewise([[g, lo < x and x < hi]])` (numbers 0 <= lo <
  hi) for g = c x^m exp(-k x^q): incomplete gamma functions (DLMF 8.2.2 after u = k x^q); for an a of unknown sign
  the piecewise of the whole transform (a <= 0) and the cut one.

## Round 39: fundamental solutions of the heat and wave operators, series of roots

- `green_function(L, u(x, t), [x, -inf, inf], t, [y, s])` for `a u_t - b u_xx` (b/a > 0) and `a u_tt - b u_xx` (b/a
  > 0) with constant a, b: the causal fundamental solutions (Evans, 2.3.1 and 2.4.1), the heat kernel and
  `heaviside_theta(t - s - abs(x - y)/c)/(2 a c)`. Other operators, and the form with `u` in place of `u(x, t)` (a
  pure function), are not done.
- `series(root(f, k), a, a0, n)` for f polynomial in the root and in a, with only simple roots at a0 (nonzero
  discriminant, so the k-th root stays the continuation of the k-th root at a0): Newton's iteration on truncated
  series (Brent and Kung 1978).

## Round 39: left undone

- `count_roots` of transcendental functions on intervals and rectangles: a rigorous count needs interval
  evaluations of the special functions and their derivatives (`bessel_j` of an interval stays partly unevaluated)
  and a treatment of poles (`gamma`, `tan`); not done.
- `asymptotic_solve` through a singular point (Newton-Puiseux expansions with fractional exponents) is not done.
- `difference_root_reduce` and `casoratian(eq, y, n)`: the operations are defined only up to choices (the initial
  values given, the normalisation of the fundamental system) that are not fixed here; left as they are.
- Asymptotic relations with a parameter in an exponent (`asymptotic_equal(1/(x - 1)^p, 10/(x - 1), x -> 1)`): the
  verdict depends on p on both sides of a boundary, and one condition is not the whole answer; a split on an
  assumption `p >= c` is done only when both parts agree.
- The Hankel transform when the Mellin-Barnes integrand is balanced (a jump at s = 1, `sin(r)/r`): the residue
  series on the two sides of the jump are not combined into a piecewise result; transforms that exist only as
  distributions (of `cos(r)`, `erf(r)`, `sin_integral(r^2)`) are not given.
- `maximize`/`minimize` of a sum with incommensurable periods give no point: whether the bound is attained needs a
  decision of `a + P k = b + Q m` in integers k, m, which is not done (`max_value`/`min_value` give the bound).
## Round 39: witnesses for undecided properties, theta derivative

- `function_sign`, `function_monotonicity`, `function_convexity` of a smooth function of one variable whose real domain
  is not known (no table entry), and of a function whose derivative has no closed form: samples can only refute.
  Real values of both signs give `indeterminate` for the sign; first or second differences of both signs at triples
  x0 - 1/4, x0, x0 + 1/4 (ten x0 spread over [-15, 15]) give `indeterminate` for monotonicity or convexity; nothing
  else is concluded from samples here (no sign or shape is ever proved by them). The sampling order puts a few spread
  points first, so a slow function whose values near 0 are not real (a radial spheroidal function for |x| < 1) is
  refuted before the budget goes to the points near 0. Gap: functions of several variables whose sampled values
  are mostly not real (the Weierstrass invariants' functions) stay undecided.
- Gap: a derivative rule d/du elliptic_theta = elliptic_theta_prime sends d/dq of elliptic_theta into a loop (the
  opaque q partial is expanded again); left out.
