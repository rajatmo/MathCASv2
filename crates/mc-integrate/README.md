# mc-integrate

Indefinite integration over a differential tower: the Risch decision procedure, the parallel Risch-Norman ansatz, and
the algebra of integrands with roots. It is a library of exact methods; the driver that checks answers is
`mc-integrals`.

## Modules
- `tower`: the differential field of an integrand, a tower C(x)(t_1)...(t_n) over the constants; `field`, `uni`,
  `write`: its rational-function field, polynomials in a tower variable, and field elements back to terms.
- `risch`: the Risch decision procedure for towers of height one (one exponential or one logarithm; trigonometric
  integrands through `exp(i u)`). It finds the antiderivative, proves that none is elementary
  (`Outcome::NonElementary`), or declines. `rde`: the Risch differential equation.
- `norman`: the parallel Risch-Norman ansatz over any tower; `special`: special functions as candidates (`erf`, `Ei`,
  `li`, `Si`, `Ci`, Fresnel, incomplete gamma, elliptic).
- `algebraic`: Chebyshev's theorem on binomial differentials, Hermite reduction in a simple radical extension,
  Abel's continued fractions, invariants of Goursat's involutions.
- `linsolve`: sparse Gaussian elimination over a field; `budget`: the work budget of a method; `terms`: helpers.

## Design
- All methods are exact. Each runs under its own `Budget`: the evaluator's fuel, the interrupt and a deadline.
- The crate returns candidate antiderivatives; the caller differentiates every answer before returning it.
- Depends on `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-poly` and `rustc-hash`; not on the calculus crates.
- Literature: Bronstein, *Symbolic Integration I*; Bronstein, "Parallel integration"; Norman and Moore (1977).

## Tests
- Unit tests in `src` for the tower, the linear solver, the Risch differential equation and the algebraic methods.
- End-to-end tests of `integrate` are in `mc-integrals`.
- Run: `cargo test --release -p mc-integrate`
