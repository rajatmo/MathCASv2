# Package 2A: `mc-calculus` (symbolic differentiation)

Status: merged (branch `lib/r4-calc`, commit 3901356). Every 2A target of `docs/design/06-roadmap.md` works with tests.
On the branch: workspace 531 passed, 0 failed, 6 ignored (guard, peak 2756 MB); clippy clean. Dependencies `mc-api`,
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
library names); at the merge, `mc-lsp` lists `mc_calculus::FUNCTIONS` (and the JIT's `compile`, `compiled_info`). No
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
2. In `n` (handed to package 1C): outside a numeric island exact exponents become balls and `pow(ball, ball)` stays
   unevaluated (`n(1/(1 + abs(v)), 25)` leaves `2.26^-1.000`); `abs` over a ball is not refined.
3. Not covered: `max`/`min` with more than two arguments or a list (unknown functions); `d` of a series (unevaluated);
   a symbolic order `[x, n]` (read as a gradient); `factorial`, `binomial` until gamma and polygamma exist (phase 3C).
4. Results are canonical, not simplified: `-1*(-1 + x)*(1 + x)^-2 + (1 + x)^-1`, `cos(x)/x + -1*x^-2*sin(x)` (phase 2B).

# Package 2C: integration and partial fractions

Built by four work packages in parallel on a shared skeleton (commit `051f448`), merged on branch `calculus-2c`.
Each package's full report is in `notes/`:

| item | report | contents |
|---|---|---|
| skeleton | — | `integrate(f, x)`, `integrate(f, [x, a, b])` and `apart(e[, x])` natives; the dispatcher `integrate::antiderivative` (linearity, constant factors, base table `x^n`, `1/x`, `exp`, `sin`, `cos`), then rational, then elementary; test helper `check_antiderivative` |
| 1 `apart` | [`notes/2c-apart.md`](notes/2c-apart.md) | partial fractions over Q(parameters): factor over Z, polynomial part, extended Euclid per prime power |
| 2 rational | [`notes/2c-rational.md`](notes/2c-rational.md) | Hermite reduction + Rothstein–Trager with Trager's gcds; real output (log of real quadratics plus atan, Rioboo); residue fields of degree 2 and even quartics; symbolic parameters |
| 3 elementary | [`notes/2c-elementary.md`](notes/2c-elementary.md) | table with linear arguments, derivative-divides substitution, tabular parts and LIATE, powers of sin/cos |
| 4 definite | [`notes/2c-definite.md`](notes/2c-definite.md) | F(b) - F(a) with breaking points of f and F located (Sturm counts), one-sided limits by a private helper, `failure(divergent, integrate, f)` |

Cross tests (`tests/integrate_cross.rs`): definite integrals over rational and elementary antiderivatives
(`integrate(1/(1 + x^2), [x, 0, inf])` is `pi/2`, `integrate(x*exp(-x), [x, 0, inf])` is 1), elementary integrals with
rational sub-integrals (`x*atan(x)`, `exp(2x)/(1 + exp(x))`), `apart` then `integrate`, and the roadmap's exit test:
a mixed corpus whose antiderivatives differentiate back to the integrand.

Known gaps across the package (details in the reports):
- residue fields of degree 3 or non-even quartics (`1/(x^3 - 2)`) need a root-sum result;
- no erf/Si/Ei results, no Risch algorithm; `exp(-x^2)*x^2` stays unevaluated;
- results are not simplified (`(-1/2)*log(4) + (1/2)*log(2)`);
- `d` has no rules for sec, csc, cot and the inverse hyperbolic functions, and `n` cannot evaluate sec, csc, cot;
- `limit` (item 5) is not built; `integrate/definite/limit.rs` is a private, conservative helper it can replace.

## Round 7 package C: public `limit`, `d` of interpolating functions, exact times

Status: merged (branch `lib/r7-c`, commits 5f4f9f4, 9693969, from main 216fdea; merge 02483a9). On the branch: full
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
2. Symbolic parameters whose sign matters (`x^a` at 0, `exp(a*x)` at inf) stay unevaluated: round 7 package A's
   assumptions could decide them through `uses_assumptions` on `limit` and `NativeCx::assumptions()`.
3. Inexact inputs are rationalized, so `limit(x^2, x -> 0.1)` is `0.01`, not `0.010000000000000002`.
4. `0.0 + exp(-pi/2)` does not become a number (evaluator).
5. Derivatives beyond the dense output's degree 4 come from the interpolant (x''' to about 1e-3).
6. No interrupt test specific to `limit` (fuel is ticked per node; no case slow enough to interrupt reliably).
7. Flaky test outside the package: `mc-calculus/tests/apart.rs::interruption` failed twice in guarded runs with a
   panic in `mc-poly` (`poly.rs:407`, "division by the zero polynomial") while `apart` is interrupted; it passed in 5
   later runs. Likely a race in `mc-poly` when the stop flag is raised mid-computation: needs an owner.
