# mc-dist

Probability distributions and statistics of data lists: the distribution families, their density, distribution
function, quantile and moments, random variates, descriptive statistics, `estimated_distribution` and `find_fit`.

## Modules
- `family`: the families (normal, log-normal, uniform, exponential, gamma, beta, chi-square, Student t, binomial,
  Poisson, geometric): names, parameters, validity, reading a distribution term.
- `symbolic`: exact and symbolic densities, distribution functions, closed-form quantiles and moments as terms.
- `machine`: machine-real densities, distribution functions and quantiles; `special` holds the machine special
  functions they need (error function family, incomplete gamma and beta).
- `sample`: random variates drawn from the session generator.
- `data`: statistics of data lists; `fit`: parameter estimation by maximum likelihood or moments.
- `expect`: `expectation(f, d)` and `expectation(e, x, d)`; `findfit`: nonlinear least squares.
- `build`, `natives`: a builder of unevaluated terms; the registered functions.

## Design
- A distribution is an inert term, the call of its family head; the family's native checks numeric parameters and
  accepts symbolic ones.
- Exact and symbolic arguments give exact forms; a machine real among the arguments selects the machine path; balls
  give the exact forms evaluated in balls.
- The data functions share their names with the distribution functions; one native per name tells a list from a
  distribution.
- Kurtosis is the fourth standardised moment (3 for a normal distribution); geometric distributions count failures.
- Variates come from the session generator of `mc-random`, so a seed reproduces them at any thread count.
- Depends on `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-numeric`, `mc-numfn`, `mc-random` and `mc-special`.

## Tests
- Unit tests in `src`; integration tests in `tests/` for the families, data statistics, fitting and interaction with
  other libraries; a documentation example on the crate page.
- Run: `cargo test --release -p mc-dist`
