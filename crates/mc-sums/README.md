# mc-sums

Sums, products and recurrences: closed forms of definite, indefinite and infinite sums, products of rational functions,
numeric sums, `sum_convergence`, `generating_function`, `r_solve` and `r_solve_value`.

## Modules
- `rf`, `hyper`, `shift`: rational functions over the integers in the kernels of the input, hypergeometric terms and
  their ratio, and shifted values written through unshifted ones.
- `gosper`, `zeil`: Gosper's algorithm and Zeilberger's creative telescoping.
- `recur`, `rsolve`, `rclassic`: linear recurrences with polynomial coefficients (rational and hypergeometric
  solutions), `r_solve`, and classical recurrence families.
- `sumfn`, `prod`, `alternate`, `nested`, `special`, `gf`: closed forms of sums, products of rational functions,
  alternating series, nested harmonic sums, sums of special-function terms and generating functions.
- `conv`, `numsum`, `series`: convergence verdicts, numeric sums and products, and series accelerations.
- `verify`: checks of closed forms before they are returned.
- `natives`, `cx`: the natives and the context of one call.

## Design
- The crate wraps the list library's `sum` and `product` (installed before it) and the `r_solve` of the calculus crate:
  the inner natives keep their fast paths, and the methods here run when they decline.
- Closed forms are checked at sample points (`verify`) before they are returned.
- Algorithms work on rational functions over the integers (`mc-poly`) and simplify through `mc-simplify`; numeric
  summation uses `mc-numfn`.

## Tests
- Unit tests in `src`, a documentation example, and integration tests in `tests/` for closed forms, recurrences and
  the precision contract, run on the assembled engine (`mc-engine`).
- Run them with `cargo test --release -p mc-sums`.
