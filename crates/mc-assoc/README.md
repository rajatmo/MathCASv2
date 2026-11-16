# mc-assoc

Sets, associations and datasets as natives of the evaluator: set operations on lists, insertion-ordered maps from any
term to any term, tables with named columns, and the data containers `weighted_data`, `event_data` and `time_series`.

## Modules
- `sets`: `union`, `intersection`, `complement`, `is_subset`.
- `assoc`: `assoc`, `keys`, `values`, `lookup`, `insert`, `merge`, `key_sort` and the other association functions.
- `index`: per-term key indexes, so a lookup is not a linear scan.
- `group`: `counts`, `group_by` on lists, `is_failure`, `failure_tag`.
- `dataset`, `table`, `aggregate`, `text`: datasets, their representation, grouping and aggregation, printed table form.
- `series`: weighted data, event data, time series.
- `ctx`: the symbols and caches every native shares.

## Design
- An association is the term `assoc(k1 -> v1, ...)`: keys distinct, compared by identity, in insertion order; it prints
  as that call and reads back to the same term.
- A dataset is an association from column names to columns of one length; numeric columns are packed.
- `missing()` marks an absent value and aggregates skip it.
- A symbol where an association, list or set is expected leaves the call unevaluated; any other wrong argument gives
  `failure(argument, f, k)`.
- `install` registers the natives and the argument-head rules that make `a[k]` a lookup.
- Depends on `mc-api`, `mc-core` and `mc-eval`; `mc-engine`, `mc-lists` and `mc-syntax` are used by tests only.

## Tests
- Integration tests in `tests/` for associations, datasets, list-function interaction, limits and properties of
  datasets.
- A documentation example on the crate page and an example program in `examples/`.
- Run: `cargo test --release -p mc-assoc`
