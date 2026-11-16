# mc-combinat

Number sequences and number representations: Bernoulli, Euler and Bell numbers, and the combinatorial and
number-representation functions written in MathCAS itself.

## Modules
- `seq`: the natives `bernoulli_b`, `euler_e` and `bell_b` for integer arguments, by exact integer-only recurrences.
- `lib/combinat.mc`: the remaining functions, exposed as `SOURCE`; the engine evaluates it after the prelude. A rule in
  that source handles the forms (symbolic second argument, lists) that the natives decline.
- `lib.rs`: `FUNCTIONS` (names with one-line descriptions) and `install`, which registers the natives.

## Design
- Natives are exact and integer-only; anything they decline falls through to the library source.
- The library source is ordinary MathCAS code, so its functions are defined in every session and can be redefined by
  users like any other.
- Algorithms follow Brent and Harvey for the tangent and secant numbers, Bell's triangle, and the DLMF chapters on
  Bernoulli and Euler polynomials, partitions and Bell numbers, and 3j and 6j symbols.
- Depends on `mc-arith`, `mc-core` and `mc-eval`; `mc-api` and `mc-engine` are used by tests only.

## Tests
- Integration tests in `tests/` evaluate the library functions through a full engine session.
- Run: `cargo test --release -p mc-combinat`
