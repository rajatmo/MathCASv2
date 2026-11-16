# mc-poly

Polynomial algebra: packed sparse polynomials, multiplication, GCD, factorisation, resultants, Groebner bases, root
isolation, series and rational-function normalisation, as natives of the evaluator.

## Modules
- `poly`, `qpoly`, `z`, `convert`: the packed sparse multivariate integer polynomial, rational-coefficient polynomials
  over one common denominator, integers with a machine-word fast path, and conversion from and to terms.
- `mul`, `gcd`, `modp`, `resultant`: multiplication by Kronecker substitution, heap merging or hashing; Brown's modular
  GCD; arithmetic modulo word-sized primes; resultants and discriminants.
- `factor`, `mfactor`: factorisation over the integers; multivariate factorisation by evaluation and Hensel lifting.
- `groebner`, `zgroebner`, `fglm`: Groebner bases over rational functions in parameters and over the integers, and
  change of order for zero-dimensional ideals.
- `descartes`, `decompose`: real root isolation and functional decomposition.
- `unipoly`, `ring`, `ringfns`: dense univariate polynomials over any coefficient ring and their user functions.
- `ratfun`, `series`: `together`, `cancel`, truncated power series.
- `functions`, `natives`, `opts`, `work`: user-level functions, argument checking, the `assumptions:` option and
  cooperative interruption.

## Design
- A packed polynomial is observably its expanded term; terms are converted in the kernels of the input.
- Long kernels poll a work budget so an interrupt stops them.
- Depends on `mc-arith` for numbers, `mc-eval` and `mc-numtheory` (primes for modular methods).

## Tests
- Unit tests in `src`, and integration tests in `tests/` for the functions, large cases, comparisons against a
  big-integer model, GCD algorithm dispatch, interruption and Taylor series, run on the assembled engine.
- Run them with `cargo test --release -p mc-poly`.
