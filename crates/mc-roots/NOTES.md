# Item R: `mc-roots` (numerical root finding and minimisation)

Status: done. Every form of section R of the round-5 plan works with tests,
including verified ball roots under `digits:`; `find_minimum`/`find_maximum` done too. At completion the full suite: 621
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
- **R.2** Damped Newton by backtracking (sufficient decrease 1 - 2e-4 t, at most 40 halvings, none once the step no
  longer moves x: at a root f is at its noise level and no halving decreases it); a singular Jacobian
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

## Item N0: on `mc-numfn`

`NumFn`, `TermFn`, `Machine`, `CentralGradient` and `Scalar` moved to `mc-numfn` (public there); `func.rs` and
`scalar.rs` re-export them, the tests are untouched and pass. The machine-real root finding (Newton, secant, Brent,
with the symbolic derivative or Jacobian) runs each function through `mc_numfn::Auto` (`natives::auto`): compiled by
`mc-jit` after 32 evaluations by default (`MC_NUMFN=interpreted|compiled|auto:N` for measurements), the interpreter
for refused and declined calls, so results are unchanged; all tests also pass with `MC_NUMFN=compiled`. Complex
Newton, minimisation and the ball refinement stay interpreted. Gap 3 above is closed for those paths; L5 measurements
are in `crates/mc-numfn/NOTES.md`.

## Item N1

Root finding and minimisation run on compiled code through `mc-numfn`'s family cache (one compilation per family of equations differing only in machine-real constants); `compile: auto | true | false` option; minimisation on `Auto`; `digits: d` results print exactly d digits (at most d; d unless within half a unit of a rounding boundary). Details, decisions N1.1-N1.8 and measurements: `crates/mc-numfn/NOTES.md`, section "Item N1".

## Round 18: optimisation and integer relations

New modules `lp` (two-phase dense simplex, Bland's rule, vertex re-solved from its active rows; branch and bound),
`optim` (augmented Lagrangian with BFGS inner solves, active-set KKT Newton polish with a central-difference
Hessian of the Lagrangian, Halton multistart, Nelder-Mead after BFGS for nonsmooth unconstrained objectives,
nonlinear branch and bound), `nopt` (the natives and the constraint reader), `pslq` (PSLQ on `Float`s with exact
128-bit integer matrices; `root_approximant`).

New functions: `n_minimize`, `n_maximize`, `n_min_value`, `n_max_value`, `n_arg_min`, `n_arg_max` (global, hold
their arguments so a vector variable is substituted before evaluation), `find_min_value`, `find_max_value`,
`find_arg_min`, `find_arg_max` (from `find_minimum`/`find_maximum`), `vector_greater_equal`, `vector_greater`,
`vector_less_equal`, `vector_less`, `find_integer_null_vector`, `root_approximant`. `find_minimum` and
`find_maximum` hand constraints, bare variables and regions to the constrained solver; accept `working_precision`
(as `digits`), `gradient`, monitors, and method names of other systems (as `auto`).

Decisions:
- **R.20** Affine objective and constraints: a linear program, solved exactly (the vertex from its active rows).
- **R.21** Global search: Halton sample of the box (bounds read from single-variable affine constraints, else 10
  around the start), 8 + 4n separated starts, the best feasible result.
- **R.22** The KKT polish: Newton on (grad L, c_A) = 0; accepted when it converges, inequality multipliers are
  non-negative and every constraint holds. This is where results beat an interior-point answer: `x + y` on the
  disk gives -1.414213562373095 (-sqrt 2 to machine precision); `x^2 - (y - 1)^2` on the disk of radius 2
  gives exactly -9 at (0, -2).
- **R.23** Integers: branch and bound (LP or nonlinear relaxations); integer values printed as integers.
- **R.24** Messages: infeasible (`[inf, [x -> indeterminate]]`), unbounded (`-inf`), constraints not met, first-order
  conditions not verified.
- **R.25** PSLQ: relations checked again; a message when a relation is as large as Dirichlet's bound for any
  numbers at that precision (an artifact), or when none exists below PSLQ's norm bound.

Gaps: semidefinite cones; region symbols for transformed or implicit regions; `evaluation_monitor`/`step_monitor`
are accepted but not called.

## Round 20 (the precision contract)

- `working_precision: d` is read by the option parser as `digits: d` (machine_precision keeps the machine method),
  so `find_root(cos(x) == x, [x, 1], working_precision: 50)` gives 50 digits (P19-03).
- `n_minimize`/`n_maximize` and their value and argument forms honour `working_precision:`/`digits:` (P19-04): an
  optimum that is an interior point of scalar real variables (no equations, no integer variables, inequalities
  strictly satisfied) is refined by `find_minimum(..., digits: d)` from the machine point; optima on the boundary
  of the constraints keep machine precision (open).

## Round 22: unbounded objectives, constrained optima at working precision

- **Unbounded** (D21-23): an unconstrained nonlinear objective is unbounded when, from the optimum found, it falls
  along the rays `x s^k` (`s` = 0.1 or 10, k = 1..4) by more than a millionth each step, or the next point overflows
  while the value is already below -1e150 (`x^3 - 3x`, `exp(x)` maximised, `-x^2`, `1/x`): `[-inf, [x -> indeterminate]]`
  with the message of the linear case instead of a value near 1e308. A bounded problem has higher values near its
  minimum; `exp(-x^2)` (infimum 0, not attained) keeps its value 0.
- **Constrained optima** (D21-24, R20-13): with `working_precision: d` an optimum on the boundary is refined by the
  Karush-Kuhn-Tucker system (Nocedal and Wright, ch. 12): the active constraints at the machine point, the multipliers
  by least squares (refused when the multiplier of an inequality is negative), then `find_root` on `grad F - sum
  lambda_i grad e_i = 0, e_i = 0` at d digits. Interior optima keep the earlier path (`find_minimum`).

## Round 24

- **Constraints judged against their own size (D23-16).** The feasibility tolerance is absolute (1e-7 (1 + |x|)), so
  `x^2 + y^2 <= 1e-10` (a disc of radius 1e-5) accepted a point 4e-10 outside it and reported -3.3e-5 for a minimum
  of -1.4e-5. `rescale`: when a constraint is smaller than 1 at the origin, the violation is judged as `viol / |c(0)|`,
  and the problem is solved again with the constraints multiplied by `min(1/|c(0)|, 1e15)` (`optim::Scaled`), the better
  point kept. A point still outside its constraints by more than the tolerance (the disc of radius 1e-150 is beyond
  double precision for this solver) is no minimum: the value and position are `indeterminate` with the message
  "the constraints could not be met", where the point used to be returned with the message.
- **Unbounded constrained objectives (D23-17).** `runs_away`: a point that is not a verified optimum (the polish did not
  converge) from which the local solver, restarted at 10 and 100 times the point, finds feasible points that are each
  lower by more than a millionth and at least five times further away, is a descent with no end: `-inf` with the message
  (`x*y` on `x + y == 2` gave -1.5e27 at x = 3.9e13).
- **Starts and parameters.** A start at which the problem is not numeric (`x^2` at 1e300) is dropped for the default
  one (a point moved in until the overflow ends is at 1e113, from where BFGS does not come down). A symbol without
  a value in the objective or a constraint (`x^2 - a*x`) gives the message and `failure(not_numeric, ...)` (the
  objective was taken as constant and `n_maximize(x - a*x^2, x)` returned 0). A start beyond 1e7 gives the search box
  `c +- |c|`, which holds the origin.

## Round 33

- `root_approximant`: a ball number made by `n(x, d)` carries 16 guard bits (`GUARD_BITS`); for inputs up to 80 bits
  the precision used is p - 16, so `root_approximant(n(pi, 4))` is 22/7 and not 355/113 (which the guard bits
  supported). The acceptance cost is `0.9 digits + 2 (1 - digits/10)` (the second term only for very short inputs).
  `holds` no longer allows a slack of `8 + 2 len` bits at low precision (it was larger than the precision itself and
  accepted anything): the slack is at most p/4.
- `find_minimum`/`find_maximum` with constraints and no starting values also run from the other sign patterns of the
  default start (1, ..., 1), for n <= 4, and keep the lowest feasible point (sin(x) sin(2y) on a disk: -0.99966,
  not the boundary point -0.0886).
- Not done: `find_minimum` of a 2-norm residual in `minimize` (least squares of a 2-norm residual: mc-calculus), `surd` in `find_root`
  (mc-numfn's prep step does not rewrite it).

## Round 37

- `element(x, cylinder([p, q], r))` (and `cylinder()`, the unit cylinder about the z axis from -1 to 1) as a
  constraint of the optimisers: with a = q - p and d = x - p, 0 <= d.a <= a.a and |d|^2 |a|^2 - (d.a)^2 <= r^2 |a|^2
  (the distance to the axis by Lagrange's identity), smooth polynomial constraints in any dimension.
  `find_maximum([z, element([x, y, z], cylinder([[-1, 0, 1], [1, 0, -1]], 1))], [x, y, z])` is 1 + 1/sqrt 2 to
  machine precision.
## Round 37

- `find_integer_null_vector` of complex numbers: integers c with sum c_i Re x_i = 0 and sum c_i Im x_i = 0. PSLQ runs on
  the real numbers Re x_i + sqrt(2) Im x_i (every relation of both parts is one of these); a relation is reported only
  when `holds` confirms it for the real parts and for the imaginary parts separately, otherwise `not_found`. The
  precision of a complex input is that of its parts. (Used for the Weierstrass eta values: eta1 + eta2 + eta3 = 0.)

## Round 38: vector constraints

- `prepare_constraints`: a constraint named by a symbol is its value; nested lists of constraints are flattened;
  `inactive(g)` heads become `g` (the held arguments are substituted before evaluation, so a vector term plus a list
  cannot thread too early); an equation `a == c` with a symbol a that is not a variable and a numeric c (number,
  vector, matrix) fixes a everywhere (two different values: an argument failure). `[x, element(y, integers)]` is two
  variables, not x with a start. Tests: `mc-engine/tests/round38_vectors.rs`.
