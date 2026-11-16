# mc-arith

Canonical arithmetic: the number tower (big integers, rationals, balls, machine reals, complex numbers), the canonical
order on terms, and the canonicaliser of `add`, `mul` and `pow`, written generically over `mc_api::Store`.

## Modules
- `num`: the bare number layer outside the store: `nat` (limb naturals), `int`, `rat`, `ball` (ball arithmetic),
  `float` (exact/machine conversions), `gcd` (Lehmer), `ntt` (large multiplication).
- `number`: numbers as values with contagion `exact < ball < machine`, three-valued comparisons, numeric powers.
- `radical`: canonical radicals, exact powers of rationals with non-integer rational exponents.
- `order`: the canonical total order on terms, which sorts the arguments of `add` and `mul`.
- `canon`: `Arith`, the canonicaliser of sums, products and powers.
- `term`: conversion between `Num` values and number terms, and the failure tags the canonicaliser recognises.
- `text`: decimal views of numbers for printing.

## Design
- Kernels compute on bare values outside the store and build terms once at the end.
- Every public number operation under a raised stop flag returns without panicking or looping.
- `Arith` carries a size limit in bits (`DEFAULT_MAX_BITS` by default) and an optional stop flag.
- Infinities (semantics 3.1): a directed infinity is `c*inf` with one coefficient per direction (a sign, a coprime
  Gaussian integer, or `c/abs(c)` for inexact `c`); two directions in a sum are indeterminate.
- The only dependencies are `mc-api` and `rustc-hash`; the evaluator is not a dependency (`mc-core` and the big-number
  libraries are used by tests only).

## Tests
- Unit tests in `src`, including a test that every number operation returns under a raised stop flag.
- Integration tests in `tests/` cover canonical forms, collection of terms, powers, number kinds and the order.
- An example program in `examples/`.
- Run: `cargo test --release -p mc-arith`
