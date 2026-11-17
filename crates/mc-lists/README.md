# mc-lists

The list and structure library: natives of the evaluator for building, taking apart, reshaping, reducing, selecting
and sorting lists and other compound terms, plus grouping, combinatorics, hashing and a few string functions.

## Modules
- `construct`, `arrange`, `reshape`, `access`: `range`, `table`, `fold`, `partition`, `join`, `take`, `part` and kin.
- `reduce`, `sums`, `select`, `search`, `sort`, `measure`: reductions, `sum` and `piecewise`, selection by predicate or
  pattern, searching, sorting, `depth` and `leaf_count`.
- `group`, `groupings`, `combinat`, `subsets`, `tour`: `tally`, `gather`, `subsets`, `permutations`, sub-multiset
  pattern matching and shortest tours.
- `predicates`, `structure`, `symbols`, `strings`, `assocs`: kind tests, parts by position and pattern, symbols by
  name, string functions, associations through the list functions.
- `levels`, `matching`, `arrays`, `more`: level specifications, pattern matching of subjects, array slices and
  further functional operations.
- `hash`, `digests`: message digests of terms.
- `parallel`: `parallel_table`, `parallel_map`, `parallel_sum`.
- `ctx`: the symbols and stop flag every native shares.

## Design
- `install` registers every function of `FUNCTIONS` with a kernel.
- Natives work on any compound term where that makes sense (`length(a + b)` is 2); a symbol where a compound term is
  expected leaves the call unevaluated, other wrong arguments give `failure(argument, f, k)`, and an index outside a
  term gives `failure(index, i, n)`.
- Walks over terms (`depth`, `leaf_count`) use an explicit stack, so any depth is handled.
- `table`, `map`, `array`, `nest`, `nest_list`, `fold` and `fold_list` over many machine numbers compile their
  function once (`mc-jit`, which gives the interpreter's values exactly or declines, and then the interpreter runs).
- Depends on `mc-eval`, `mc-arith`, `mc-jit` (compiled maps), `mc-numfn` and `mc-syntax` (the printer for `to_string`).

## Tests
- Integration tests in `tests/` evaluate source text on the assembled engine (`mc-engine`) and include property tests
  and packed-array cases; `examples/` holds a small benchmark program.
- Run them with `cargo test --release -p mc-lists`.
