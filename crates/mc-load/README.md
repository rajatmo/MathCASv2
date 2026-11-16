# mc-load

Source loading: files, modules, imports, read-time name resolution, the prelude and the standard modules.

## Modules
- `loader`: `Loader`, the reader state of a session, and the reading of one input (REPL text, `-e` source, a script).
- `modules`: the module table and import lists.
- `resolve`: name resolution of a lowered statement, rewriting its symbols to qualified ones.
- `source`: `Origin` and the `SourceProvider` trait with `FileSystem` and `InMemory` providers.
- `stdlib`: the embedded prelude and standard modules.
- `sites`: definition sites and name resolutions collected for editor support.
- `error`: `LoadError` with `file:line:column`, the source line and a caret.

## Design
- `Loader::read` checks the `#! mathcas 2` header, parses and lowers with `mc-syntax`, loads the module files the
  imports need and resolves every name of every statement before anything is evaluated.
- The result is a list of plain statements on qualified symbols: evaluation never consults the module table.
- The module table is reader state, independent of any term store.
- `Loader::read_recorded` additionally collects definition sites and resolutions (used by the language server).
- The standard modules are embedded in the binary from the repository's `lib/` folder.
- Depends only on `mc-api` and `mc-syntax`; it does not depend on the evaluator.

## Tests
- Unit tests in `src`, a documentation example, and integration tests in `tests/` for resolution and collected sites.
- Run them with `cargo test --release -p mc-load`.
