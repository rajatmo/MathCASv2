# mc-libraries

The library registry: one crate that depends on every library crate, so that the engine, the language server and the
documentation do not each list them.

## Modules
- `install`: registers every library's natives in a kernel, in a fixed order, and returns the numeric compiler handle.
- `SOURCES`: the libraries written in MathCAS, in load order; the engine evaluates them after the prelude.
- `FUNCTIONS`: every library's `FUNCTIONS` table, the names a session knows from the libraries.
- The library crates themselves, re-exported (`mc_libraries::mc_lists`, ...), for the catalogue's topic tables and the
  language server.

## Design
- Adding a library means adding it here only; the consumers (`mc-engine`, `mc-docs`, the language server) read these
  three items and need no change.
- The install order is part of the behaviour: a later library may build on, or wrap, a native an earlier one registers.
- The crate contains no algorithms of its own.

## Tests
- No tests of its own: the registry is exercised by the engine's integration tests and by the documentation checker,
  which run every function of every installed library.
- Run: `cargo test --release -p mc-engine` and `cargo test --release -p mc-docs`
