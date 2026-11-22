# mc-transforms

Integral transforms: Laplace (one- and two-sided), Mellin, Fourier (exponential, sine and cosine), their inverses, and
the discrete pair `z_transform` and `inverse_z_transform`.

## Modules
- `laplace`, `inverse`: the Laplace transform by linearity, shift, scaling, derivative, integral and multiplication
  rules, tables of special functions and the definite integral; the inverse by partial fractions and residues, rules
  and tables.
- `mellin`, `meijer_inv`: the Mellin transform and its inverse by Mellin-Barnes residue series, and inverse Mellin
  transforms of gamma ratios as Meijer G-functions.
- `fourier`: the exponential, sine and cosine Fourier transforms, with `fourier_parameters:`.
- `ztrans`: the Z transform by its rules and `sum`, and its inverse for rational functions.
- `pattern`: the matcher of the transform tables, which are written in the language itself.
- `verify`: the numeric check of a transform found by rules or tables.

## Design
- One front end (`Transform`) serves every transform: a list, equation or rule maps; several variables iterate one
  after the other (a step that fails is tried again with the later variables in their ranges, and the half-line
  transforms are simplified on positive variables at the end); a non-symbol target is computed with a symbol and
  substituted; a numeric one falls back to numerical integration, and the inverse Laplace transform at a number
  `t > 0` to two fixed Talbot contours that must agree. Options are `generate_conditions:` and `fourier_parameters:`.
- When no method gives a value the call stays unevaluated; a value that the numeric check refutes is not returned.
  The check compares a transform at a sample point with its defining integral (`n_integrate`), and an inverse with
  the forward integral of its value; it refutes only on clear evidence.
- Built on `mc-calculus` (the transform names and derivatives), `mc-integrals` for definite integrals, `mc-limit` and
  `mc-syntax` (for the tables).

## Tests
- Integration tests in `tests/` evaluate transforms on the assembled engine (`mc-engine`), including Meijer
  G-function cases.
- Run them with `cargo test --release -p mc-transforms`.
