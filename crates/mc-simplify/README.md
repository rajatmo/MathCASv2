# mc-simplify

Simplification and assumptions: `simplify`, `full_simplify`, `refine`, `power_expand`, `complex_expand`,
`function_expand`, trigonometric rewriting and `element`.

## Modules
- `simplify`, `cost`, `moves`, `powers`, `boolean`: the bounded search for the cheapest equivalent form, its
  complexity measure, the rewriting moves and boolean candidates.
- `trig`, `trigconst`, `identities`, `gammanorm`, `fexpand`: trigonometric and exponential rewriting, algebraic values
  of trigonometric constants, special-function identities, a zero test for gamma-function expressions and
  `function_expand`.
- `power`, `power_exact`: `power_expand` without and with branch corrections.
- `facts`, `element`, `refine`: the facts of an assumption context (sign, bounds, realness, integrality) and the
  decisions they support, domain membership, and `refine`.
- `complex`: `complex_expand`, writing an expression as `a + b*im` with real `a` and `b`.
- `logic`: equational reasoning under universally quantified equations.
- `verify`, `term`: the spot check of a rewrite, and the term builder shared by the rewriting walks.

## Design
- Every rewrite is an identity under the stated assumptions; a rewrite is spot-checked numerically before it is
  accepted.
- `simplify` minimises a digit-weighted leaf count (or the `complexity_function:` option), cached over hash-consed
  subterms of one call.
- Signs and realness are decided through `mc-decide`; numbers and balls come from `mc-arith` and `mc-numeric`.
- Depends on the evaluator but not on the solver or integration crates.

## Tests
- Unit tests in `src` and integration tests in `tests/` for simplification, assumptions, complex expansion, element
  decisions, parallel use and cost, run on the assembled engine (`mc-engine`).
- Run them with `cargo test --release -p mc-simplify`.
