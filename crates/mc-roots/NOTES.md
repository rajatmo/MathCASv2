# Package R: `mc-roots` (numerical root finding and minimisation)

Status: merged (branch `lib/r5-roots`, commit 726167d). Every form of section R of the round-5 brief works with tests,
including verified ball roots under `digits:`; `find_minimum`/`find_maximum` done too. Main after the merge: 621
passed, 0 failed; clippy clean. Dependencies `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-numeric` (`machine::C64`);
dev `mc-engine`, `mc-bench`. Registration lines in `mc-engine` and `mc-lsp` only.

## Modules

| module | contents |
|---|---|
| `func` | `NumFn<'h, S>` (`eval`, `eval_batch`); `TermFn`: the expression as the closure `fn([x1..xn], body)` applied by the evaluator (a list body gives its leaves row-major, so one closure computes a residual vector or Jacobian); `Machine` (machine-scalar view; batches as a speculative group); `CentralGradient` (one batch) |
| `scalar` | `Scalar` for `f64` and `C64`: conversions to and from `Num`, magnitude, conjugate, `clamp_real`; norms |
| `linalg` | Gaussian elimination with partial pivoting (generic); a Levenberg-Marquardt step for singular Jacobians; elimination on `Num` (balls) |
| `newton` | damped Newton for n equations, real or complex (Armijo backtracking on \|f\|^2, at most 40 halvings); Broyden when no symbolic Jacobian exists or it is not numeric; optional box bounds (projection) |
| `bracket` | damped secant; Brent's `zero` |
| `minimize` | golden-ratio bracketing then Brent's `localmin`; BFGS with Armijo backtracking |
| `refine` | ball Newton refinement under `digits:` and interval Newton verification |
| `options`, `natives` | option parsing, goals, precision; argument forms, method choice, results, failures |

## Public API

`install(k, h, stop)`; `FUNCTIONS`: `find_root`, `find_minimum`, `find_maximum`.

- `find_root(f, [x, x0])`, `[x, x0, x1]`, `[x, x0, a, b]`; systems `find_root([f1, ...], [[x, x0], ...])`; `f` may be
  `lhs == rhs`. Result `[x -> root, ...]` (substitutes with `rewrite`).
- `find_minimum(f, [x, x0])` or `(f, [[x, x0], ...])`; `find_maximum` alike. Result `[f_value, [x -> x_ext, ...]]`.
- Options: `digits: d` (positive integer up to 10^5); `accuracy_goal`, `precision_goal` (digits, non-negative real or
  `inf`); `max_iterations` (synonym `max_steps`); `method: newton | secant | brent | broyden | bfgs`; `verify: true |
  false` (`find_root` with `digits` only). Unknown option `failure(unknown_option, name)`; bad value
  `failure(argument, f, name)`.
- Failures: `failure(argument, f, k)`; `failure(not_numeric, f, [x -> point])`; `failure(not_converged, f, estimate,
  error_estimate)`; `failure(not_verified, find_root, rules)`; `failure(interrupted)`.

## Decisions

- **R.1** `lhs == rhs` stands for `lhs - rhs`; residuals must be scalars.
- **R.2** Damped Newton by backtracking (sufficient decrease 1 - 2e-4 t, at most 40 halvings); a singular Jacobian
  takes a Levenberg-Marquardt step.
- **R.3** Convergence: error estimate is the last step; goals max(10^-accuracy_goal, 10^-precision_goal \|x\|); the
  residual must also be small, which rejects poles and jumps (`tan(x)` on [1, 2]).
- **R.4** Goals default to half the working digits; `max_iterations` 100; under `digits:` the user's goals apply to the
  ball stage.
- **R.5** Derivatives come from `d` (called once through the evaluator); without one, the secant method (one
  variable) or Broyden (systems).
- **R.6** Method choice: `[x, x0]` and systems Newton; `[x, x0, x1]` secant; `[x, x0, a, b]` Brent; mismatched
  combinations are argument failures.
- **R.7** A bracket without a sign change runs Newton constrained to [a, b].
- **R.8** Complex starts run Newton or Broyden on `C64`.
- **R.9** The body goes through `n` first (constants become numbers, variables stay symbols); under `digits: d` the
  working precision is d + 10 digits; variables must be symbols.
- **R.10** Ball refinement from the machine root; for a simple real root of one equation with a symbolic derivative,
  interval Newton verification (N = m - f(m)/f'(X) inside X proves exactly one root, and N is returned); `verify:
  true` turns an unverified root into `not_verified`. Double roots, complex roots and systems are refined, not
  verified.
- **R.11** Under `digits:`, missed goals give `not_converged` with the ball estimate.
- **R.12** Minimisation: golden-ratio bracketing then Brent (one variable); BFGS with the symbolic or central-difference
  gradient (several); Newton polishing with a symbolic Hessian; `find_maximum` minimises -f.
- **R.13** `find_minimum` under `digits:` refines with ball Newton on the symbolic gradient and Hessian.
- **R.14** Fuel ticks on every iteration and refinement step; stops give `failure(interrupted)`.

## Tests (20 integration, 2 unit, 1 doctest)

`tests/roots.rs` (Newton, secant, Brent, complex, systems, 28 failure cases), `tests/digits.rs` (verified roots at
20-100 digits checked digit by digit; systems; `not_verified` and `not_converged` cases), `tests/minimize.rs` (one and
several variables, balls, 13 failure cases), `tests/session.rs` (identical at 1 and 4 threads and under the stress
collector; interrupts stop at about 300 ms); unit linear solves.

## Measurements (median of 5 fresh engines, release, busy machine)

| problem | median |
|---|---|
| Newton `cos(x) == x` / secant / Brent on x^3 - 2x - 5 | 0.25 / 0.24 / 0.25 ms |
| complex Newton x^3 - 1 | 0.26 ms |
| system of 3 (Newton) / system of 2 (Broyden) | 0.59 / 0.28 ms |
| Rosenbrock (BFGS + polish) / Brent minimum of a quartic | 1.31 / 0.32 ms |
| verified 100 / 1000 / 10000 digits | 0.73 / 8.09 / 23 ms |
| `sin(x) == 1/2` at `digits: 20000` / `100000` | 1.6 / 15.8 s |

## Known gaps

1. Ball refinement runs every step at full precision.
2. Only simple real roots in one variable are verified.
3. No compiled backend yet (a `mc-jit` backend can be added behind `NumFn`).
4. No per-evaluation fuel test.
5. Brent landing on a pole reports `not_numeric`; no brackets in several variables or for minimisation.
6. BFGS on nonsmooth objectives ends `not_converged` with a good estimate.
7. `n_minimize`/`n_maximize` (global methods) not done.

## Package N0: on `mc-numfn`

`NumFn`, `TermFn`, `Machine`, `CentralGradient` and `Scalar` moved to `mc-numfn` (public there); `func.rs` and
`scalar.rs` re-export them, the tests are untouched and pass. The machine-real root finding (Newton, secant, Brent,
with the symbolic derivative or Jacobian) runs each function through `mc_numfn::Auto` (`natives::auto`): compiled by
`mc-jit` after 32 evaluations by default (`MC_NUMFN=interpreted|compiled|auto:N` for measurements), the interpreter
for refused and declined calls, so results are unchanged; all tests also pass with `MC_NUMFN=compiled`. Complex
Newton, minimisation and the ball refinement stay interpreted. Gap 3 above is closed for those paths; L5 measurements
are in `crates/mc-numfn/NOTES.md`.

## Package N1

Root finding and minimisation run on compiled code through `mc-numfn`'s family cache (one compilation per family of equations differing only in machine-real constants); `compile: auto | true | false` option; minimisation on `Auto`; `digits: d` results print exactly d digits (at most d; d unless within half a unit of a rounding boundary). Details, decisions N1.1-N1.8 and measurements: `crates/mc-numfn/NOTES.md`, section "Package N1".
