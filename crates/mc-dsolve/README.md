# mc-dsolve

Symbolic differential equations and recurrences: `d_solve`, `d_solve_value`, `asymptotic_d_solve_value`, `r_solve` and
`r_solve_value`.

## Modules
- `linear`: linear ordinary equations: constant coefficients of any order, Euler equations, first order by integrating
  factor, order reduction, and second order with variable coefficients by ansatz and reduction of order.
- `special`, `kovacic`: second-order equations solved by special-function bases (Airy, Bessel, hypergeometric, Hermite,
  Laguerre, Legendre) after a change of variables, and Kovacic's algorithm for Liouvillian solutions.
- `first`: nonlinear first order (separable, Bernoulli, Riccati, homogeneous, exact, Clairaut, symmetry-based
  integrating factors) and second order reduced to first order.
- `system`: linear systems with constant coefficients through the matrix exponential.
- `pde`: first-order linear equations by characteristics, and second-order homogeneous equations with constant
  coefficients.
- `delay`: delay equations by the method of steps.
- `series`: Taylor and Frobenius series, regular perturbation.
- `recur`, `rsystem`: linear recurrences and systems of recurrences with constant coefficients.
- `verify`: the check applied to every solution.

## Design
- Every explicit solution is substituted back into the equation; the residual is simplified to 0 or, failing that,
  evaluated at random points. A solution that fails the check is dropped, and when every method fails the call stays
  unevaluated.
- Each call has a time budget and polls the stop flag.
- Constants are `c(1)`, `c(2)`, ...; the head can be changed with `generated_parameters`.
- Depends on `mc-api`, `mc-arith`, `mc-core`, `mc-eval` and `mc-calculus`.

## Tests
- Integration tests in `tests/` solve equations through an engine session and check the results.
- Run: `cargo test --release -p mc-dsolve`
