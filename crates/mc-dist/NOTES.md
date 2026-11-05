# mc-dist: probability distributions and data statistics (phase 3D)

## What it holds

| module | contents |
|---|---|
| `family` | the 11 families, parameter checks, reading a distribution term |
| `special` | private machine erf, erfc, erfcx, normal quantile, regularized incomplete gamma and beta (stopgaps, DI.2) |
| `machine` | f64 pdf, cdf as both tails, quantiles |
| `symbolic` | exact and symbolic pdf, cdf, survival, closed-form quantiles, mean/variance/skewness/kurtosis, raw moments, densities for integration |
| `sample` | random variates |
| `data` | list statistics, `histogram_list` |
| `fit` | `estimated_distribution` |
| `expect` | `expectation` |
| `natives` | dispatch |
| `build` | a small term builder |

Distributions (inert terms, constructors check parameters): `normal_distribution(mu, sigma)`,
`log_normal_distribution(mu, sigma)`, `uniform_distribution([a, b])`, `exponential_distribution(l)` (rate),
`gamma_distribution(k, theta)` (shape, scale), `beta_distribution(a, b)`, `chi_square_distribution(k)`,
`student_t_distribution(n)`, `binomial_distribution(n, p)`, `poisson_distribution(m)`, `geometric_distribution(p)`
(failures before the first success, support 0, 1, ...); `normal_distribution()` and `uniform_distribution()` are the
standard members.

Functions (16 natives): `pdf`, `cdf`, `survival`, `quantile`, `mean`, `variance`, `std_dev`, `skewness`, `kurtosis`,
`moment`, `central_moment`, `median`, `random_variate`, `expectation`, `estimated_distribution`, `histogram_list`.
`mean` ... `median` and `quantile` take a distribution or a data list; `pdf`, `cdf`, `survival`, `quantile` map over a
list argument (packed machine arrays stay packed).

`lib/stats.mc`: `covariance(xs, ys)`, `correlation(xs, ys)`; its `mean`, `variance`, `std_dev`, `median` also accept a
distribution (handing it to `moment`, `central_moment`, `quantile`), since module members shadow the global natives.

Outside the crate: registration in `mc-engine` (`mc_dist::install(&mut k, h, rng)`, the generator also handed to the
JIT) and `mc-lsp`; test expectations in `crates/mc-repl/tests/sessions/modules.txt` and
`crates/mc-kernel/tests/protocol.rs` (new names and module members).

## Decisions

- **DI.1** A distribution is the call of its family head; the constructor gives `failure(argument, <family>, msg)`
  for invalid numeric parameters and accepts symbolic ones.
- **DI.2** erf/erfc, the normal quantile and the incomplete gamma and beta functions are private stopgaps in
  `special.rs` (both tails, since callers need the small tail accurately). To switch to mc-special: `erfc_dd`,
  `gamma_pq(_l)`, `beta_i(_l)`.
- **DI.3** Exact results use mc-special's names only: `erfc(...)`, upper incomplete `gamma(a, x)`, `beta(z, a, b)`
  (B_z, not regularized), `inverse_erfc`. Elementary cases avoid them: integer-shape gamma and chi-square (Erlang
  sums), integer-parameter beta (polynomials), Student t with 1 or 2 degrees of freedom, discrete distribution
  functions at a numeric point (finite sums up to 200 terms).
- **DI.4** A machine real anywhere selects the machine path (closed numeric parameters through `n`); otherwise the
  exact or symbolic form goes back to the evaluator; balls evaluate the exact forms in balls.
- **DI.5** A continuous quantile without a closed form at a ball probability: Newton on the exact cdf with doubling
  precision; the radius is the last step (an estimate, not a bound). An exact discrete quantile is a machine candidate
  confirmed by exact cdf comparisons.
- **DI.6** `random_variate` is an effect drawing from mc-random's session generator (a seed reproduces values at any
  thread count): Box-Muller (normal), Marsaglia-Tsang with the k < 1 boost (gamma), gamma ratio (beta), normal over
  chi (t), multiplied uniforms below 10 and PTRS (Hoermann) above (Poisson), Knuth's order-statistic recursion plus
  inversion (binomial), inversion (exponential, geometric, uniform).
- **DI.7** One native per name separates data lists from distributions; a list with a machine real is summed in
  machine arithmetic (compensated), else an exact formula. `quantile(xs, q)` is the element of rank
  max(1, ceil(n q)).
- **DI.8** `expectation(f, d)` (a function) and `expectation(e, x, d)` (an expression in x): constant; polynomial by
  moments (exact); binomial up to 10^4 trials by the finite sum; machine parameters by `n_integrate` or a machine
  series (Poisson, geometric); symbolic `integrate`; else unevaluated.
- **DI.9** `estimated_distribution`: maximum likelihood unless `method: moments` (sample variance with n - 1);
  closed forms exact for exact data; gamma, chi-square, beta and t likelihoods numerically (Newton with
  digamma/trigamma; golden section plus secant steps on the score for t).
- **DI.10** Kurtosis is plain (3 for a normal), not excess.
- **DI.11** A moment that does not exist gives `failure(undefined, f, d)`, an infinite one `inf`; with symbolic
  parameters existence is assumed. `std_dev` gives sigma, (b - a)/sqrt(12), 1/l, sqrt(k) theta directly.

## Accuracy and tests

Machine path against reference values: pdf, cdf, survival on 110 points over all families including far tails
(normal at +-37, gamma k = 250, binomial n = 1000, Poisson m = 500): worst relative error 6.1e-15; quantiles at 132
points: 3.0e-15; ball quantiles agree to all printed digits at 40 and 50 digits.

23 tests: unit (erf values, normal quantile round trips to 1e-300, incomplete gamma and beta identities, quantile
inversions, least-count discrete quantiles, masses summing to 1, sampler moments, Stirling numbers, FUNCTIONS);
`tests/reference.rs` (the tables above, cdf(quantile) on packed grids); `tests/session.rs` (147 input/output cases,
51 exact results against reference values to 1e-15, ball quantiles, densities integrating to 1 with the right mean
and variance, discrete masses, exact round trips, the stats module, estimation against reference values, variates
reproducible and within 5 standard errors at 10^5 draws for all 11 families, 1 vs 4 threads and stress collector);
a doctest.

## Measurements (release, busy machine, 10^6-element packed arrays, per element)

pdf normal 0.7 us, gamma 1.4 us; cdf normal 4 us; quantile normal 4 us, Poisson 8 us, gamma 14 us, beta 25 us,
t 43 us. `random_variate` 10^6 draws: normal 64 ms, gamma 74 ms, Poisson(50) 420 ms, binomial(1000, 0.3) 410 ms.

## Known gaps

1. Until mc-special provides them, `n` of `erfc`, `gamma(a, x)`, `beta(z, a, b)`, `inverse_erfc` forms stays
   unevaluated (exact normal/log-normal cdfs, non-integer gamma/chi-square/beta cdfs, t cdf for n > 2, their ball
   quantiles). After that merge: check `gamma(a, x)` is the upper incomplete function and `beta(z, a, b)` is B_z,
   then switch DI.2 to mc-special.
2. Machine paths spend most time in double-double logarithms (normal cdf 4 us, t quantile 43 us could be faster).
3. Incomplete functions accurate to about 1e-14 relative for parameters up to 10^4; above that not measured.
4. Ball quantile radius is an estimate.
5. `expectation` of non-polynomial functions with exact or ball parameters depends on symbolic `integrate`
   (`expectation(x => abs(x), uniform_distribution([-1, 3]))` stays; its machine form works).
6. Symbolic results are not simplified (`binomial(n, 0)` kept in a binomial cdf; long log-normal estimates;
   `central_moment(exponential_distribution(l), 3)` is `2*(l^-2)^(3/2)`).
7. `random_variate` draws sequentially on the session stream (no parallel split streams for big arrays).
8. Univariate only; `covariance`/`correlation` take two data lists, no matrix form.
