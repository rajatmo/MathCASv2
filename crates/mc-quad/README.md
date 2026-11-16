# mc-quad

Numerical integration: `n_integrate` as a native of the evaluator, for single, multiple, infinite-range and oscillatory
integrals, at machine and arbitrary precision.

## Modules
- `rules`, `machine`, `gk`, `de`, `nested`: Gauss-Kronrod rules, shared range handling, adaptive Gauss-Kronrod with a
  global error heap, double-exponential rules, and multiple integrals by nesting.
- `hp`, `rigorous`: double-exponential rules on balls under `digits:`, and guaranteed enclosures by Clenshaw-Curtis
  with Bernstein-ellipse error bounds under `rigorous: true`.
- `osc`, `oscb`: oscillatory integrals over half-lines (Longman's method with Wynn's epsilon), at machine and
  arbitrary precision.
- `domain`, `multi`, `cubature`, `mc`, `qmc`, `qmc_tables`: multidimensional integration on the unit cube, with the
  integrand evaluated in parallel blocks: Genz-Malik cubature, VEGAS and plain Monte Carlo, randomised lattice rules.
- `fft`: the fast Fourier transform of long machine vectors.
- `regions`, `options`, `natives`: symbolic preparation of integrands, options, argument forms, results and failures.

## Design
- The integrand is seen through `mc-numfn`, compiled when possible.
- Infinite ranges are mapped onto finite ones by substitution.
- Monte Carlo and lattice methods draw from `mc-random`, so results follow the session seed.
- With `rigorous: true` the result is an enclosure with a proved error bound.
- Depends on `mc-eval`, `mc-arith`, `mc-numeric`, `mc-numfn`, `mc-random` and `rayon`.

## Tests
- Unit tests in `src`, a documentation example, and integration tests in `tests/` for machine precision, high-precision
  digits, multiple integrals, oscillatory integrands, the precision contract and the library functions.
- Run them with `cargo test --release -p mc-quad`.
