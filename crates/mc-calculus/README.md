# mc-calculus

Symbolic differentiation, partial fractions, properties of functions (domain, range, continuity, singularities, sign,
monotonicity, period, poles, injectivity, surjectivity) and exact optimisation, plus the helpers and derivative rules
the other calculus crates use.

## Modules
- `rules`: the table of derivative rules of known functions, one row per function (the place to extend).
- `diff`: `Diff`, the derivative of a term in one variable, an iterative memoised walk over the term's DAG.
- `term`: `Build`, canonical construction of derivative terms through `mc-arith`.
- `natives`: the native `d`: variable specifications, orders, gradients over lists.
- `numdiff`: numerical values of derivatives that have no rule.
- `apart`: partial fractions.
- `props`: the function-property natives, with the modules `spec`, `table`, `smooth`, `sets`, `shape`, `range`,
  `period`, `poles`, `inject`, `onto`, `pw`, `interval`, `proof` and `domain_form` beneath it.
- `optimize`: `minimize`, `maximize`, `min_value`, `max_value`.
- `common`, `transform_kind`: helpers shared by the calculus crates; integral transforms by name.

## Design
- The derivative operator is the term `derivative(n1, ..., nk)(f)`, printed with prime marks; differentiating an unknown
  function applies the chain rule.
- A property is answered only when it is proved from enclosures; otherwise the call stays unevaluated.
- Limits, integration, integral transforms and differential equations are separate crates above this one.
- Depends on `mc-api`, `mc-arith`, `mc-core`, `mc-decide`, `mc-eval` and `mc-poly`.

## Tests
- Integration tests in `tests/` for derivatives, partial fractions, optimisation, function properties, numerical
  derivatives, parallel use, and cross-library interaction; property tests live under `tests/props`.
- A documentation example on the crate page.
- Run: `cargo test --release -p mc-calculus`
