# mc-numtheory

Number theory as natives of the evaluator: integer and rational functions, rounding, primes, prime counting,
factorisation and modular arithmetic.

## Modules
- `word`, `ring`, `prime`: machine-word Montgomery arithmetic with deterministic primality below 2^64, arithmetic
  modulo big odd numbers, Baillie-PSW and the next prime.
- `sieve`, `count`, `leaves`: segmented sieves, Meissel's prime counting and the k-th prime, and the combinatorial
  method (Lagarias-Miller-Odlyzko, Deleglise-Rivat).
- `factor`: trial division, rho, ECM and SIQS.
- `gcd`, `power`, `modular`: subquadratic half-gcd, exact roots and perfect powers, modular powers, inverses,
  primitive roots, orders and divisors.
- `combin`, `digits`, `cover`: factorials and binomials, digits in a base, and further functions defined by formulas.
- `budget`, `natives`: the work budget, and argument decoding, list threading, results and failures.

## Design
- The algorithms are plain functions on the bare numbers of `mc-arith` (`Nat`, `Int`); each takes a `Budget` that
  charges evaluator fuel and polls the interrupt.
- Results that are claims of primality or factorisation come from algorithms that prove them within the stated range.
- `install` registers the natives listed in `FUNCTIONS`; some searches use `rayon` threads.
- Depends on `mc-arith`, `mc-eval` and `mc-numeric` only.

## Tests
- Unit tests in `src`, and integration tests in `tests/` for the functions, limits of size, property tests against
  big-integer models and measurement cases.
- Run them with `cargo test --release -p mc-numtheory`.
