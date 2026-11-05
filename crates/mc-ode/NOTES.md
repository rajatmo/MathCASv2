# Package N3: `mc-ode` (`n_dsolve`, `n_dsolve_value`)

Status: merged (branch `lib/r6-n3`, commits 6ffa24e..2118718, from main 6376b35). L3 passes at t = 10, 100, 1000;
L4 at 10^2, 10^5, 10^11; L3h agrees to 30 digits; Van der Pol mu = 1000 runs to t = 3000 in about 4 ms (2920
steps). On the branch: full suite 712 passed, 0 failed, 6 ignored (peak 2.75 GB); clippy clean for mc-ode, mc-engine,
mc-lsp.

## Modules

| module | contents |
|---|---|
| `system` | equations to a first-order system `Y' = F(t, Y)` in fresh state symbols: differential equations sorted from initial conditions, solved for the highest derivatives (N3.1); unknowns inferred for `n_dsolve_value`; `in_state` rewrites expressions into the state (events) |
| `model` | F through `mc_numfn::compile_expr` on `Var::scalar(t)` and the state symbols; Jacobian `d(F, [s1..sn])` and `dF/dt` from `d`, built lazily when the stiff method first needs them (forward differences otherwise) |
| `dopri` | Dormand-Prince 5(4) after Hairer's DOPRI5: FSAL, Lund-stabilised step control, order-4 dense output, the DOPRI5 stiffness test |
| `rodas` | Rodas4 (Hairer-Wanner coefficients): six stages, one LU and six solves per step, error estimate from k6, order-3 dense output |
| `lu` | dense LU with partial pivoting, factor once, solve many |
| `step`, `driver` | stepper trait, Hairer's error norm and `hinit`; the loop: last-step landing, minimum step, step limit, up to 8 retries on a non-numeric stage, events, automatic switch to Rodas4; fuel on every attempted step |
| `dense` | one format for every step: degree-4 monomial coefficients in theta (values and derivatives of any order, shortening at an event, joining backward and forward runs) |
| `events` | sign change at step ends, crossing located on the dense output by the Illinois method |
| `taylor` | the right-hand side as a tape with automatic-differentiation recurrences for +, *, /, integer powers, sqrt, real powers, exp, log, sin/cos, tan, sinh/cosh/tanh, atan; Jorba-Zou step size; ball arithmetic at the working precision |
| `solution` | the session's store of solutions keyed by a content hash; evaluation on machine reals and balls |
| `natives`, `options` | `n_dsolve`, `n_dsolve_value`, the curried `interpolating_function(...)(t)`, options |

## API

- `parametric_n_dsolve_value(eqs, expr, [t, t0, t1], [p1, ..., pm])` gives `parametric_function(...)`; `f(v1, ..., vm)`
  is the value of `expr` for those parameter values (N3.12).

- `n_dsolve(eqs, y | [y, z, ...], [t, t0, t1], options)` gives
  `[y -> interpolating_function("y", [lo, hi], steps, "key"), ...]`, plus `events -> [[t_e, k], ...]` with `events:`.
- `n_dsolve_value(eqs, expr, [t, t0, t1], options)`: `expr` may use `y(x)`, `y'(x)`, ...; unknowns inferred from the
  equations; a closure `t => ...` when `expr` contains t, else the number or list. A highest derivative is computed
  from the right-hand side along the solution (as accurate as the solution).
- `interpolating_function(...)(x)`: the value at x (a list for a list of times), `failure(out_of_range, x, [lo, hi])`
  outside, unevaluated for symbolic x; the name `"y'"` (more primes for higher orders) selects a derivative.
- Options: `method: auto | rk45 | stiff | taylor`, `accuracy_goal`, `precision_goal`, `max_steps` (default 10^6),
  `max_step_size`, `digits` (up to 2000), `events: [g == 0, ...]` or a bare `g`, `event_action: stop | record`,
  `compile: auto | true | false`.
- Failures: `failure(argument, n_dsolve, k | option | "message")`, `failure(unknown_option, name)`,
  `failure(not_numeric, n_dsolve, [t -> .., y -> ..])`, `failure(not_converged, n_dsolve, partial_rules, t_reached)`
  (blow-up or step limit).

## Decisions

- **N3.1 Highest derivatives.** The residual is written `A h + b` with `A` from `d`, free of h: a constant numeric `A`
  is inverted exactly (Gauss-Jordan on numbers); otherwise each equation must hold exactly one highest derivative;
  anything else is `failure(argument, n_dsolve, "implicit equation")`.
  The common explicit form (each residual a sum with one term `+-h_j` and no other highest derivative) is read off
  without calling `d`: the same right-hand sides, about 40% less time in the reduction (a small equation solved
  10^4 times: 0.46 s -> 0.38 s).
- **N3.2 Results** are short `interpolating_function` summaries; the data is in a session store keyed by a 64-bit
  content hash (the printer has no hook, and a data term would print thousands of numbers and need decoding on every
  evaluation). Content keys make equal solutions `===`, keep memoised results valid and give identical keys at 1 and 4
  threads; a collision with different data moves to the next salt. Cost: solutions live until the session ends.
- **N3.3 Tolerances.** Machine precision defaults to 8 digits for both goals; `digits: d` at or below machine
  precision sets both to d; the relative tolerance is clamped at 1e-14.
- **N3.4 Dense output**: degree-4 monomial coefficients per step; the DOPRI5 and Rodas extensions reduce to it.
- **N3.5 Explicit method**: DOPRI5 step control (beta = 0.04, safety 0.9, factor in [0.2, 10]), stiffness test on
  every accepted step (h*lambda > 3.25 on 15 steps).
- **N3.6 Stiff method**: Rodas4 with mc-ode's own LU, not mc-roots' `linalg` (its functions are `pub(crate)` and its
  `solve` refactors every call; Rodas needs one factorisation and six solves per step). mc-roots unchanged.
- **N3.7 Events**: sign change between step ends, located on the dense output (Illinois); two crossings within one
  step cancel and are missed; machine precision only.
- **N3.8 Automatic switch**: `method: auto` switches DOPRI5 to Rodas4 once on detected stiffness, never back.
- **N3.9 Step limit**: 10^6 by default.
- **N3.10 Taylor method under `digits: d`**: working precision d + 20; two integrations at tolerances 10^-(d+10) and
  10^-(d+4); the result is the fine one with radius its own plus |fine - coarse| (printed digits correct as an
  estimate, not a guarantee); each step restarts from midpoints (no radius blow-up); results rounded to bits(d + 1);
  initial values and range ends evaluated with `n` at the working precision (`y(0) == e` works); goals default to d.
- **N3.11 Domain**: the range plus the initial point; integration runs from that point to both ends (t1 < t0 and
  conditions inside the range work).
- **N3.12 Parametric solutions**: `parametric_n_dsolve_value(eqs, expr, [t, t0, t1], [p1, ..., pm], options)` reduces
  the equations once with the parameters as symbols (initial values may use them) into the term
  `parametric_function(params, expr, data)`, whose data is the reduced system (no state outlives a native call but
  terms the session holds). A call `f(v1, ..., vm)` with real values substitutes them into the right-hand side, the
  initial values, `expr` and the options and integrates as `n_dsolve_value` (identical results); a symbolic value
  leaves the call unevaluated. A small equation solved 10^4 times: 0.37 s by `n_dsolve_value`, 0.23 s by a
  parametric function.

## Edits outside the crate

`mc-engine` (dependency, `install`, `library_names`), `mc-lsp` (dependency, `FUNCTIONS`), `Cargo.lock`;
`bench/cases/L/L3.mc`, `L4.mc`, `L3h.mc` call `n_dsolve` and apply its rules with `rewrite`
(`rewrite([x(t1), y(t1), z(t1)], s)`), "expected to fail" notes removed. mc-roots unchanged.

## Tests (20 plus a doctest)

- Unit (2): LU with pivoting and reuse; dense forms, derivative and truncation.
- `tests/ode.rs` (10): exponential decay with all three methods; rules and evaluation (lists, symbolic times, out of
  range, `"y'"`, `===` of equal solutions); dense-output error below 1e-9 between steps (DOPRI5, Rodas); oscillator
  energy drift below 1e-9 over [0, 100] and a third-order equation; three components, a coupled constant matrix,
  backwards integration, conditions in the middle; closures from `n_dsolve_value`; Lorenz against the reference to
  1e-6; stiff: Van der Pol mu = 1000 under 5 s and 20,000 steps, auto agreeing with stiff, Robertson at 10^11, a
  non-autonomous stiff equation; events (stop at the ground, recorded crossings of two functions); 17 failure forms,
  the blow-up partial solution, `max_steps`.
- `tests/digits.rs` (3): nine closed forms at 30, 40, 50, 100 digits with every printed digit correct (tan near its
  pole, sin and cos, e, e^-1 backwards, exp(sin t), exp(e^t), atan, sinh); exact solutions through sqrt, `y^(3/2)`,
  `1/(2y)`; Taylor vs machine agreement, evaluation on both sides of the initial point, `method: taylor` alone.
- `tests/session.rs` (4): identical at 1 and 4 threads with and without the stress collector (also parallel maps over
  solutions); `compile: true` and `false` give identical solution terms; interrupts stop interpreted and compiled
  Lorenz and a 2000-digit Taylor run within 5 s; timing.

## Measurements (release, busy shared machine)

| case | compiled (default) | interpreted |
|---|---|---|
| L3 Lorenz t = 10 / 100 / 1000 | 2.25 ms / 15.0 ms / 144 ms | 239 ms / 3.26 s / 3 GB cap |
| L4 Robertson 10^2 / 10^5 / 10^11 | 13.1 / 13.8 / 15.3 ms | 2.59 / 2.61 / 2.36 s |
| L3h 30 digits (balls) | 19.7 ms | 20.0 ms |
| Van der Pol mu = 1000, t = 3000 | 3.6 ms | 270 ms |
| Lorenz to t = 20, tol 1e-10 | 6.8 ms | 658 ms |

L3 t = 10 max error 1e-8 against the reference; L4 relative errors 7e-11 / 1e-10 / 9.6e-8, components sum to 1 within
1e-9.

## Known gaps

1. `d` of an interpolating function stays unevaluated (`derivative(1)(interpolating_function(...))(u)`): mc-calculus'
   `DerivativeAt` should rewrite `derivative(n)(interpolating_function(name, ...))` to the term with n primes added to
   the name. Meanwhile: `"y'"` names and `n_dsolve_value(..., y'(t), ...)`.
2. Interpreted long runs exhaust memory (terms are not collected during a native call; the evaluator's interpreted
   backend); compiled runs are unaffected.
3. Not supported: implicit equations with a state-dependent non-diagonal coefficient matrix, DAEs, delay equations,
   complex-valued solutions, events under `digits:`.
4. No switch back from Rodas4 to DOPRI5; two crossings in one step are missed.
5. The Taylor method's error is an estimate (no verified enclosures); functions outside its set give a clear failure.
6. The store keeps every solution of a session (no eviction).
7. Vector-valued unknowns written as list arithmetic are not supported (unknowns are scalar functions).

## Round 7 package C

`d` of interpolating functions (primes added to the name, in mc-calculus' `DerivativeAt`) and exact times (`g(pi/2)` evaluated with `n` at the solution's precision, in `Interpolating::at`): see `crates/mc-calculus/NOTES.md`, section "Round 7 package C".
