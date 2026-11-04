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
