# mc-integrals

The dispatcher of `integrate`, definite and indefinite, and the base methods every other integration method relies on.

## Modules
- `lib`: `antiderivative`, which tries linearity, then a base table, rational functions, and elementary functions.
- `rational`: rational functions of `x` (Hermite reduction, logarithmic part, root sums, radicals).
- `elementary`: a table, substitution and integration by parts, with trigonometric and algebraic submodules.
- `definite`: definite integrals (residues, Meijer G forms, delta functions, substitutions, conversion).
- `portfolio`: the integration portfolio that combines the methods, including the tower-based engine of
  `mc-integrate` and the rule table of `mc-integrate-rules`.
- `natives`: the native `integrate`.

## Design
- Every method returns `Ok(None)` when it does not apply or fails; the call then stays as it is. Methods may recurse
  through `antiderivative` and give up beyond `MAX_DEPTH`.
- No constant of integration is added.
- Results are built with `Build` and handed back to the evaluator, which evaluates the calls in them.
- Answers are checked by differentiation before they are returned.
- Limits come from `mc-limit`, derivatives and shared helpers from `mc-calculus`, signs from `mc-decide`, algebraic
  numbers from `mc-algebra`; partial fractions (`apart`) are `mc-calculus`'s.

## Tests
- Integration tests in `tests/`: base rules, rational and elementary integrands, algebraic integrands, definite
  integrals, cross-method checks and engine-level calls.
- A documentation example on the crate page.
- Run: `cargo test --release -p mc-integrals`
