# mc-solve

Equation solving: `solve`, `roots`, `n_solve`, `eliminate`, `groebner_basis`, `reduce` and root objects
`root(x => p(x), k)`, as natives of the evaluator.

## Modules
- `solve`, `front`, `domain`, `order`, `natives`, `cx`: reading equations, choosing the method, domains and conditions,
  the order in which solutions are listed, argument checking and the bridge from evaluator fuel to a work budget.
- `univariate`, `linear`, `system`, `modular`, `bounds`: roots of one-variable polynomials (also with parameters),
  linear systems by fraction-free elimination, polynomial systems by Groebner bases, solving modulo an integer, and
  bounds for integer unknowns.
- `reduce`, `reduce_cx`, `reduce_fn`, `reduce_forms`, `reduce_steps`, `chart`: `reduce` of real and complex conditions,
  conditions on functions of the unknown, step functions and sign charts.
- `plog`: equations with the unknown inside and outside an exponential, through the product-log form.
- `rootobj`, `realroots`, `numroots`, `interval`, `nsolve`, `transwrap`: root objects, real root isolation, numeric
  roots at machine precision or as balls, roots on an interval, numeric answers and root objects of analytic functions.

## Design
- Polynomial equations are solved by radicals up to degree four and by root objects beyond; solutions may carry
  symbolic parameters (generic solutions).
- Beyond radicals, roots are root objects evaluated numerically by `n` (Aberth iteration, exact real roots, complex
  balls); `reduce` handles real polynomial and rational inequalities by exact real root isolation.
- Exact real root isolation uses `mc-poly`; cylindrical decomposition `mc-cad`; number fields `mc-algebra`; signs of
  constants `mc-decide`; numeric refinement `mc-numeric`.

## Tests
- Unit tests in `src` and integration tests in `tests/` for solving, domains, numeric solving, interruption and
  interoperation with other libraries, run on the assembled engine (`mc-engine`).
- Run them with `cargo test --release -p mc-solve`.
