# mc-help

The help index: every documented function with its area, topic, one-line description and usage lines, in one embedded
table shared by the notebook (completion, usage hints, hover, F1), the language server and the documentation window.

## Modules
- `lib`: the index and its lookups, built from the catalogue table and the usage lines of the function pages.
- `scan`: text scanning for editors: the name around or before a position, the call a position is inside (with the
  argument number), the parameters of a usage line.
- `data/index.txt`: the catalogue table; `build.rs`: reads the usage lines of `docs/functions/<name>.mcn` at build time.

## Design
- The crate has no dependencies, so it stays small and cheap for every front end to link. The library crates cannot be
  its dependencies; the table is therefore written by `mc-docs-pages index` from the catalogue, and a test in `mc-docs`
  fails when the table is out of date.
- Table format: lines starting with `#` are comments; every other line is `name TAB area TAB topic TAB summary`, with
  tabs and line breaks inside texts written as spaces.
- Usage lines come from the pages themselves, so they are always the pages' current ones.

## Tests
- Unit tests in `src` for the table reader and the scanning functions.
- Run: `cargo test --release -p mc-help`
