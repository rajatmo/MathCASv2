# mc-diffalg

Differential equations and differential algebra, as a library written in MathCAS: Lie point symmetries of ordinary
and partial differential equations, the symmetry algebra, reduction and integration by a symmetry, conservation laws
by multipliers, differential polynomials under rankings, the Rosenfeld-Groebner and Thomas decompositions, and the
Painleve test.

## Modules
- `lib/diffalg.mc`: the whole library, exposed to Rust as `SOURCE`; the engine evaluates it after the prelude.
- `src/lib.rs`: `SOURCE`, `ORIGIN` (its name in messages) and `FUNCTIONS` (names with one-line descriptions).

## Design
- The crate has no Rust dependencies and no natives. Its source calls `d`, `solve`, `d_solve`, `integrate` and the
  polynomial functions through the evaluator, so it improves whenever those do.
- Because the functions are ordinary MathCAS definitions, they are defined in every session after the prelude.

## Tests
- An integration test in `tests/` runs the library functions through a full engine session (`mc-engine` is a
  development dependency only).
- Run: `cargo test --release -p mc-diffalg`
