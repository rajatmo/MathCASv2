# mc-numeric

Numerics for MathCAS: exact constants, elementary functions on exact, ball and machine numbers, and numerical
evaluation `n`, with the number functions (`re`, `rationalize`, `chop`, ...) and numbers with uncertainty.

## Modules
- `consts`: `pi`, `e`, `euler_gamma`, `catalan`, `glaisher` as balls (binary splitting; Glaisher's constant from zeta'(2) by
  the Euler-Maclaurin formula with a proved remainder), cached; `series` for hypergeometric-type
  sums.
- `elementary`, `cball`, `machine`, `fun`: elementary functions on real balls, on complex balls (principal branches),
  and on machine numbers, with one enumeration of the functions.
- `exact`, `inverse_pairs`: exact identities such as `sin(pi) = 0`, and inverse functions at numeric arguments.
- `numeval`, `library`, `value`: numerical evaluation `n(e, d)` with precision doubling and comparison by refinement,
  the registry through which other crates' real functions become numeric nodes, and exact values behind inexact numbers.
- `parts`, `approx`, `piecewise`, `numbers`, `natives`: complex parts, precision and approximation, `clip` and
  `unit_step`, and the natives and comparison rules.
- `formula`, `formula_lib`, `more`: functions defined by formulas.
- `uncertain`, `uncertain_boxes`, `uncertain_cball`, `enclose`: `around`, `interval`, `centered_interval` and proved
  images of intervals.
- `spline`, `quantity`, `series_hook`: cubic splines, physical quantities, functions of truncated series.

## Design
- Every ball result encloses the function's values on the whole input ball; evaluation increases precision until the
  requested accuracy is proved.
- `pi`, `e` and similar are ordinary symbols whose numerical values are computed on demand.
- `install` and `install_with_stop` register the natives with an `mc-eval` kernel.
- Depends on `mc-arith` for numbers and balls and `mc-eval`; it does not depend on the special-function crates (they
  register through `library`).

## Tests
- Unit tests in `src`, documentation examples, integration tests in `tests/` (enclosures, complex balls, number
  functions, limits, sessions), and benchmark programs in `examples/`.
- Run them with `cargo test --release -p mc-numeric`.
