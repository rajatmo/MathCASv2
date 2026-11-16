# mc-roots

Numerical root finding, minimisation and optimisation: `find_root`, `find_minimum`, `find_maximum`, `n_minimize`,
`n_maximize`, integer relations and algebraic recognition, as natives of the evaluator.

## Modules
- `func`, `scalar`, `linalg`: the user's expression evaluated at numeric points behind `NumFn`, real and complex machine
  scalars, and small dense linear algebra for Newton steps.
- `newton`: damped Newton's method for n equations in n unknowns, real or complex, with Broyden's method as fallback.
- `bracket`: the secant method and Brent's method in one real variable.
- `minimize`, `optim`, `lp`, `nopt`: local minimisation (Brent, BFGS), constrained nonlinear minimisation, linear and
  mixed-integer linear programming, and the constrained and global natives.
- `refine`: refinement of a machine root with balls under `digits:`, and interval Newton verification of a simple root.
- `pslq`: `find_integer_null_vector` and `root_approximant`.
- `options`, `natives`: options and goals; argument forms, method choice, results and failures.

## Design
- Iterations run at machine precision; a requested higher precision is reached by refining the machine root with balls.
- A verified root is one proved by interval Newton on balls.
- The function is evaluated through `mc-numfn`, which uses compiled code when it can.
- Depends on `mc-eval`, `mc-arith`, `mc-numeric` and `mc-numfn`.

## Tests
- Integration tests in `tests/` cover root finding, minimisation, optimisation, recognition, precision requests and
  sessions on the assembled engine; `examples/` holds a timing program.
- Run them with `cargo test --release -p mc-roots`.
