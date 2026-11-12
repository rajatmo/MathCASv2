# Item 1B: `mc-assoc` (sets, associations, failure tests)

Status: done. Workspace clippy and `cargo doc` clean. The one failing workspace test at the time (`rewrite_ac`:
`rewrite(2*a*b*c, [a*b -> q])`) was a latent `mc-eval` bug found here and fixed there (see "Found outside the
crate").

## What was built

- `sets.rs`: `union`, `intersection`, `complement`, `is_subset`.
- `assoc.rs`: `assoc` (also the normal form), `is_assoc`, `keys`, `values`, `to_rules`, `lookup`, `key_exists`,
  `key_take`, `key_drop`, `key_sort`, `insert`, `delete_key`, `merge`, `map_values`, `select_keys`.
- `group.rs`: `counts`, `group_by` (association form), `is_failure`, `failure_tag`.
- `index.rs`: the per-term key index cache (1B.6). `ctx.rs`: shared symbols, failure builders, a fuel meter, the
  canonical sort, calling function-valued arguments as a speculative group.
- `examples/lookup_speed.rs`: the measurement, with `[k/N pct%]` progress.

API: `install(k, h)` (natives plus two argument-head rules on `assoc` for `part`); `FUNCTIONS` (23 entries, generated
from the same table as the registration). Outside the crate: `mc-engine` (dependency, install after `mc_lists`,
`FUNCTIONS` in the library names); at the merge, `mc-lsp` lists `mc_assoc::FUNCTIONS`.

## Decisions

- **1B.1 Representation.** An association is the ordinary term `assoc(rule(k1, v1), ..., rule(kn, vn))`, distinct keys
  in insertion order; it prints as `assoc(a -> 1, "s" -> [1, 2])` and reads back to the identical term; no literal
  syntax this round; `length(a)` (`mc-lists`) already counts keys.
- **1B.2 Normal form** (the `assoc` native): arguments are entries `k -> v`, lists of entries or associations, read in
  order; a later entry replaces the value and keeps the first position (`assoc(a -> 1, b -> 2, a -> 3)` is
  `assoc(a -> 3, b -> 2)`); keys compared by identity (`1` and `1.0` differ); anything else, including `k :> v` and
  symbols, is `failure(argument, assoc, k)`. `mc-lists` structural functions rebuild the term, so their result is
  normalised again (`join(a1, a2)` merges).
- **1B.3** Every function is strict; `is_failure`/`failure_tag` receive failures as values (failures propagate, so
  `is_failure(f(failure(x)))` is `true`).
- **1B.4 Part access.** Argument-head rules `part(a @ assoc(___), k_) := lookup(a, k)` and
  `part(a @ assoc(___), k_, ks__) := part(lookup(a, k), ks...)`; `mc-lists`' `part` declines on associations (1A.16), so every key,
  including integers, strings and lists, is a key: `m[3]` is the value of key 3.
- **1B.5** `lookup` of a missing key without default is `failure(missing_key, k)`; a list is always one key.
- **1B.6 Index.** A cache from association handles to `FxHashMap<key handle, position>` (16 associations, 2^23 keys,
  LRU, `parking_lot::Mutex`), emptied in `Native::trace` at every collection (no handles across a safe point);
  associations of 16 entries or fewer are scanned. `lookup`, `key_exists`, `key_take` build and cache it; `insert`,
  `delete_key` use a cached index or scan (they copy the association, O(n)); `assoc`, `merge`, `counts`, `group_by`,
  `map_values`, `key_take` cache their result's index for free; the `assoc` native treats a cached term as normal in
  O(1). The index is rebuilt once per top-level statement after a safe point. Rejected: a hash-consed sorted form (log n
  comparisons, nowhere to keep it) and a packed term (needs `mc-core`).
- **1B.7 Sets.** Identity comparison; results without repetitions in canonical order (`cx.sort`); O(n log n); any
  shared head (a non-list result is rebuilt and evaluated); a symbol argument leaves the call unevaluated, another head
  or an atom is `failure(argument, f, k)`; `union()` is `[]`, `intersection()` and `complement()` stay unevaluated.
- **1B.8** `is_subset(xs, ys)` means xs is a subset of ys (two arguments): the
  container comes second, a deliberate choice.
- **1B.9** For `key_take`/`key_drop` a list argument is a list of keys (a list key is written `[[1]]`); `key_take`
  follows the given order, takes a repeated key once, skips missing keys; `delete_key`/`key_drop` of a missing key
  return the association unchanged.
- **1B.10** `merge` takes a list of associations or separate arguments; without a combiner later values win; with `f`,
  `f` gets the list of values of every key (also single values); keys in order of first occurrence.
- **1B.11** `counts`, `group_by` (with optional `g` applied to each group): keys in order of first occurrence. The list
  form of grouping is `mc-lists`' `gather_by` (item 1A asked not to register `group_by`).
- **1B.12** `map_values(f, a)` (like `map`), `select_keys(a, f)` (like `select`; kept only on `true`); a callback
  failure is the result; callbacks run as a speculative group.
- **1B.13** `failure_tag` of a non-failure or `failure()` is `failure(argument, failure_tag, 1)`.
- **1B.14** Fuel: 1 unit per 16 elements, charged in chunks of 4096 (each charge polls the interrupt); sorts charge
  n log n / 16 up front (`cx.sort` does not poll).
- **1B.15** `is_assoc` added (not on the roadmap list).

## Tests (14 + 1 doctest)

- `tests/functions.rs` (8, about 200 cases): normal and edge cases (empty, `1` vs `1.0`, list and compound keys), every
  failure tag, `a[k]`, a large association on the indexed path; expected values for sets, `counts`, `group_by`, `merge`
  with a combiner and `key_take` checked against short independent computations (values only).
- `tests/properties.rs` (3, fixed seeds): 300 random cases of the four set operations against `BTreeSet`; random
  `insert`/`delete_key`/`lookup`/`key_exists` sequences against an insertion-ordered vector (8, 30, 120, 400 keys; 1
  and 2 threads; collection stress); the stress test fails if the cache is not cleared at collections.
- `tests/limits.rs` (3): fuel shows lookups do not scan (2000 lookups cost 24,006 with 100 keys and 30,150 with 10^5,
  the difference being the one-time index build; scanning would cost about 1.25e7 more); 15 long native loops stop at
  a 1000-unit budget; an 18-statement session identical at 1, 2 and 4 threads, with and without stress.

## Measurement (release, 2 threads, median of 3, guarded, busy machine)

| n | build `assoc(map(i => i -> 2i, range(n)))` | n x `lookup(a, i)` | n x `b[k(i)]` | n x absent `key_exists` | bare loop |
|---|---|---|---|---|---|
| 10^3 | 1.05 ms | 1.69 ms | 2.44 ms | 1.34 ms | 1.32 ms |
| 10^4 | 10.0 ms | 14.4 ms | 22.9 ms | 13.1 ms | 12.9 ms |
| 10^5 | 107 ms | 176 ms (1.76 us each) | 282 ms | 151 ms | 150 ms |

Lookup time per key is flat from 10^3 to 10^5 keys, about 0.3 us over a bare loop iteration.

## Found outside the crate

`mc_eval::syntax::pattern` lowers a term to one `Pat::Lit` when its symbol mask has no pattern-head bit, and
`lang.rs::compile` added the implicit rest of 08c2f54 only to a `Pat::App` left side, so sub-call rewriting of an
all-literal AC left side depended on symbol numbering. Fixed by taking a literal call apart first.

## Known gaps

- `insert`/`delete_key` copy (O(n) each; n inserts O(n^2)); build from a list or with `merge` instead (O(n)).
- The index is dropped at every safe point and rebuilt once per statement; one cache lock shared by all tasks.
- Identity comparison only (no custom equality, no custom `key_sort` order); `is_subset` takes two arguments;
  `lookup` does not take lists of rules or of associations; the canonical sort is not interruptible (bounded, charged
  up front).

## Datasets (round 10): datasets

Modules `table` (representation: `DataSyms`, `Col`, `Table`, `Columns`), `dataset` (construction, columns, rows,
the list functions on rows, `join_on`), `aggregate` (`group_by` on datasets, `aggregate`, `pivot`), `text` (the
printed table). New functions: `dataset`, `is_dataset`, `missing`, `is_missing`, `column`, `columns`, `rows`,
`add_column`, `aggregate`, `pivot`, `join_on`; `group_by` extended to datasets; internal native `dataset_rows`.
Public Rust API: `dataset_of_columns`, `dataset_columns`, `Column`, `missing_value`, `is_missing_value`,
`table_text` (with `MAX_ROWS`, `MAX_CELL`, `MAX_WIDTH`).

- **DATA.1** A dataset is `dataset(assoc(c1 -> col1, ...))`: columns of one length, numeric ones packed; rows are
  built on demand as associations. The `dataset` native keeps a normal form as it is and builds one from rows
  (associations), a table with a header row, rows under given names, or a dataset.
- **DATA.2** The list functions `length`, `part`, `first`, `last`, `rest`, `most`, `take`, `drop`, `reverse`, `join`,
  `select`, `sort_by` act on rows: `mc-lists` declines them when the first argument is a dataset
  (`mc_lists::DATASET_ROWS`), and argument-head rules on `dataset` rewrite them to the internal `dataset_rows(f, ds,
  ...)`, which gives `f(ds, ...)` back unevaluated when it cannot answer. Same meanings as on lists (bounds failures
  `failure(index, k, n)` as there); no `head`/`tail` names (the head of a term is a different notion).
- **DATA.3** `ds[i]` is a row, `ds[i, c]` a value, `ds[c]` a column (any non-integer index names a column; a symbol
  naming none leaves the call unevaluated), `ds[i:j]` and `ds[[i, ...]]` datasets of rows.
- **DATA.4** `missing()` / `missing(reason)` marks absent values (an inert call, so a user value of a symbol cannot
  clash).
- **DATA.5** Construction failures: `failure(argument, dataset, k)`, `failure(shape_mismatch, dataset)`,
  `failure(duplicate_column, c)`.
- **DATA.6** In `sort_by`, `group_by`, `aggregate` a string or a list of strings names key columns; anything else is a
  function of the row (fast path for columns: no interpreted calls).
- **DATA.7** Aggregates skip missing values; empty `total`/`count` are 0, empty `mean`/`min`/`max` are `missing()`;
  `total`, `mean`, `min`, `max`, `count` read packed payloads (integers summed exactly in 128 bits, reals with Neumaier
  compensation, mean exact for integers); other columns and custom functions go through the evaluator; custom
  functions get the list of present values.
- **DATA.8** `aggregate(ds, specs)`, `aggregate(ds, by, specs)`, `aggregate(groups, specs)`; specs `c -> f`,
  `name -> [c, f]`, `count`; a function key or groups give a key column named `"key"`; duplicate output names fail.
- **DATA.9** `join_on(a, b, key[, how: inner|left|right|outer])`: keys first, then `a`'s, then `b`'s columns (a clash
  gets `_right`); order of `a` then `b`; missing keys match nothing. `join(ds, ...)` stacks rows (column union).
- **DATA.10** Printed form: the engine shows a dataset value (and `print` output) as a table of at most 20 rows
  (15 first, `...`, 5 last), cells cut at 32 characters, 120 characters a line, numeric columns right-aligned, last
  line `dataset: n rows, m columns`; inside other values a dataset prints as its term; `to_string` gives the term.
- `pivot(ds, r, c, v[, f])`: rows and columns in first-occurrence order; without `f` a repeated pair is
  `failure(duplicate_entry, [rv, cv])`.

Tests: `tests/datasets.rs` (7 tests, about 200 cases), `tests/dataset_properties.rs` (4, against independent Rust models),
`tests/limits.rs` (+2: 16 dataset loops stop at a 1000-unit budget; a dataset session identical at 1, 2, 4 threads
and under stress).

## Round 15 item 9B

- **C9B.12** An association applied to a key, `assoc(k -> v, ...)(k)`, is `lookup(assoc, k)`: the native of
  `assoc` also takes curried calls (`Native::curried`). This is the reading of `a(k)` that slot functions such as
  `&(&1("x"))` need on rows, and it gives `failure(missing_key, k)` for an absent key like `lookup`. Test:
  `tests/functions.rs`, `associations_applied_to_keys`.

## Round 19 fixes

- **f19.1** `insert(a, k -> v, pos)` with a key already present replaces its value in place;
  the constructor kept the old value at the first occurrence (Q19-22, the `insert` case).

## Round 29

- **b29.1** `key(k)`: the operator form of a lookup; `key(k)(a)` is `lookup(a, k)` and `a(key(k))` the same (a missing
  key is `failure(missing_key, k)`).
- **b29.2** Containers in `series.rs`: `weighted_data(data, weights)`, `event_data(data, flags)`, `time_series` and
  `temporal_data` (one normal form `time_series(values, times)` from pairs, values alone, or values with a start and
  step). Properties are read by calling the container with a name (`ts("Values")`, `ts("Path")`, `wd("TotalWeight")`).
  `mean`, `median`, `variance`, `std_dev`, `quantile` of a series act on its values and `mean(weighted_data)` is the
  weighted mean (rules attached to the container heads, so they apply after the native declines).
- Gaps: weighted and event-data median, variance and quantiles; dates as times; regular/irregular series resampling.

## Round 33

- `time_series(values, [t0, t1])` spaces the times evenly from t0 to t1 (the last time is t1 itself; exact bounds give
  exact times). The old reading of the pair as a start and a step is gone (a start alone, `time_series(values, t0)`,
  still gives t0, t0 + 1, ...).
- Dataset queries take operator forms: an index that is a call of a symbol other than a list or a span (`ds[select(f)]`,
  `ds[sort_by(g), "b"]`) applies the call to the dataset, and the remaining indices go to the result. A list of
  column names is a sub-dataset (`ds[["b", "c"]]`).
- Not a bug: `key_exists(dataset, "c")` is true for a column name (decision G57: the columns of a dataset are its keys).
- Gap: `select_first`, `maximal_by`, `take_largest_by` and friends are not row functions of a dataset (`mc-lists` answers
  them first; its `DATASET_ROWS` list would need the names), and a query operator that is applied after a column
  (`ds[select(f) /* total, "a"]`) is not read.

## Round 36 (with the linear algebra work)

`insert(a, e, pos)`: `key(k)` names the key to insert before, an integer is a position; a list of entries goes in together; a path `[k, i, ...]` inserts into the value of the key (or position) `k`.

## Round 36, later fixes

- Dataset queries `ds[f, c, ...]` with a defined function name first: f of ds[all, c, ...] (`ds[total, "a"]` is 21).
  A query result stays a plain value, so `ds[select(...), "a"](total)` is a list applied to total (by design).
- `to_rules` of equations and their and/or declines to the library rule (lib/calculus.mc).

## Round 37

- `merge` takes, besides associations, rules (an association of one entry) and lists of rules (an association of
  their entries, a later rule for a key replacing an earlier one): `merge([a -> x, a -> y, b -> z], f)` is
  `assoc(a -> f([x, y]), b -> f([z]))`. `key_sort` of a list of rules sorts the association of its entries.
