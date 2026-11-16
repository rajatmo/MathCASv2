# mc-langlib

Core-language functions written in MathCAS itself and evaluated by the engine after the prelude, so that they are
defined in every session.

## Modules
- `lib/langlib.mc`, `lib/more.mc`: the MathCAS source, concatenated into `SOURCE`; `ORIGIN` is its name in messages.
- `lib.rs`: `FUNCTIONS`, the public functions with a one-line description, read by the documentation catalogue and
  the name tables.

The functions fall into these groups:
- threading: `threaded`, `elementwise`;
- associations and lists: `join_across`, `key_union`, `key_intersection`, `is_disjoint`, `compose_list`,
  `longest_common_subsequence` and others;
- Boolean functions: `boolean_function`, `boolean_minimize`, `logical_expand`, ...;
- graphs and arrays: `nest_graph`, `array_resample`;
- arguments: `arguments_options`, `check_arguments`;
- exceptions over `throw` and `catch`: `exception`, `throw_exception`, `catch_exception`, `register_exception_type`.

## Design
- The crate has no natives; it depends only on `mc-api`.
- The helper functions the source defines for its own use (`bool_*`, `join_*`, `resample_*`, `threaded_*`, `option_*`,
  `logical_*`) are ordinary definitions in the bare namespace and not part of the interface.

## Tests
- Integration tests in `tests/` evaluate each group of functions through an engine session; `mc-engine` is a
  development dependency only. A documentation example on the crate page.
- Run: `cargo test --release -p mc-langlib`
