# mc-engine

The core assembled: `mc-syntax` reads and prints, `mc-eval` evaluates in an `mc-core` session, `mc-arith` is the
arithmetic, and the libraries are installed, all behind the `mc_api::Engine` trait that the benchmark runner, the REPL
and the kernel drive.

## Modules
- `engine`: `CoreEngine`: reads a text, evaluates it statement by statement with a safe point after each, prints.
- `arith`: `mc-arith`'s canonical arithmetic, comparisons and order as natives of the evaluator.
- `format`: number text for the printer.
- `attrs`: `attributes`, `set_attributes`, `clear_attributes`; `compose`: function composition and threading of heads.
- `history`: the natives behind the notebook protocol (`out`, numbered results) and `FUNCTIONS`.
- `packed`: functions of one argument on packed arrays (parts of complex numbers and the like).
- Re-exports the module loader (`InMemory`, `SourceProvider`, `Module`) and the evaluator's `guard`.

## Design
- This is the one crate that wires the parts together: libraries come through `mc-libraries`, the compiler through
  `mc-jit`, and the prelude and library sources are evaluated at session start.
- A session evaluates one statement per step, so collection happens at safe points between statements.
- `StopFlag` interrupts a running evaluation; a panic inside an evaluation becomes a `failure`, not the end of the
  session.
- Most other crates use `CoreEngine` in their tests, as a development dependency only.

## Tests
- A large set of integration tests in `tests/`: language semantics, modules, the profile and guard functions, parallel
  evaluation, packed arrays, fuzzing of the reader, and cross-library interaction.
- A documentation example on the crate page and an example program.
- Run: `cargo test --release -p mc-engine`
