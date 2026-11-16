# mc-ode

Numerical ordinary differential equations: `n_dsolve`, `n_dsolve_value` and the `interpolating_function` results, as
natives of the evaluator.

## Modules
- `system`, `model`: the equations reduced to a first-order system `Y' = F(t, Y)` with initial values, and `F` with its
  Jacobian and `dF/dt` at machine reals.
- `dopri`, `rodas`, `step`, `driver`, `lu`: Dormand-Prince 5(4), the Rosenbrock method RODAS for stiff systems, the
  shared step machinery, the integration loop with automatic switching, and dense LU.
- `dense`, `events`: dense output of every accepted step and zero crossings of event functions.
- `taylor`: the Taylor series method on balls, used under `digits:`.
- `solution`: solutions kept by the session and the `interpolating_function` terms naming them.
- `parametric`: `parametric_n_dsolve_value`, with the system reduced once.
- `options`, `natives`, `util`: options, argument forms, results and failures.

## Design
- The right-hand side is evaluated through `mc-numfn` (compiled by `mc-jit` when possible, interpreted otherwise).
- The machine methods choose step sizes by embedded error estimates; stiff problems switch to the Rosenbrock method.
- With `digits:` the Taylor method on balls gives results with proved enclosures.
- Solutions are stored in the session; an `interpolating_function` term refers to one and evaluates it by dense
  output.
- Depends on `mc-eval`, `mc-numfn`, `mc-numeric` and `mc-special`.

## Tests
- Unit tests in `src`, a documentation example, and integration tests in `tests/` for solutions, high-precision
  digits, parametric solves and sessions, run on the assembled engine (`mc-engine`).
- Run them with `cargo test --release -p mc-ode`.
