# mc-further-functions

Functions whose natural home is another crate but that need only registrations, not changes elsewhere. Most are
MathCAS source calling the other libraries through the evaluator; the machine-number kernels that need speed are
natives of this crate.

## Modules
- `lib/further.mc`: the MathCAS source, exposed as `SOURCE`; the engine evaluates it after the prelude.
- `julia`: `julia_set_iteration_count`, the number of iterations of a rational map until a point is caught by an
  attracting cycle or escapes.
- `foxh`: machine-number values of the Fox H function by its residue series.
- `num`: machine numbers out of terms and back, shared by the natives.
- `lib.rs`: `SOURCE`, `FUNCTIONS` (names with one-line descriptions) and `install`.

## Design
- The Rust side is small: natives decline arguments they do not handle, and the library source answers the rest.
- Library functions are ordinary MathCAS definitions, so they depend on whatever other libraries are installed.
- Depends on `mc-arith`, `mc-core` and `mc-eval`; `mc-api` and `mc-engine` are used by tests only.

## Tests
- Integration tests in `tests/` evaluate the functions through an engine session.
- Run: `cargo test --release -p mc-further-functions`
