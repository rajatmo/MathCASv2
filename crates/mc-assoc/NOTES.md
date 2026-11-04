# Package 1B: `mc-assoc` (sets, associations, failure tests)

Status: merged (branch `lib/r4-assoc`, commit d726af6; merge e4135a0). Workspace clippy and `cargo doc` clean. The
branch's one failing workspace test (`rewrite_ac`: `rewrite(2*a*b*c, [a*b -> q])`) was a latent `mc-eval` bug this
package found; fixed on main in b33430f before the merge (see "Found outside the crate").

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
  `part(a @ assoc(___), k_, ks__) := part(lookup(a, k), ks...)`; they run after `mc-lists`' `part` declines, so until
  `part` declines on associations, integer, string and list indices are positional (`m[3]` is the 3rd entry).
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
- **1B.8** `is_subset(xs, ys)` means xs is a subset of ys (two arguments); the reference system's argument order is the
  reverse: a deliberate difference.
- **1B.9** For `key_take`/`key_drop` a list argument is a list of keys (a list key is written `[[1]]`); `key_take`
  follows the given order, takes a repeated key once, skips missing keys; `delete_key`/`key_drop` of a missing key
  return the association unchanged.
- **1B.10** `merge` takes a list of associations or separate arguments; without a combiner later values win; with `f`,
  `f` gets the list of values of every key (also single values); keys in order of first occurrence.
- **1B.11** `counts`, `group_by` (with optional `g` applied to each group): keys in order of first occurrence. The list
  form of grouping is `mc-lists`' `gather_by` (package 1A asked not to register `group_by`).
- **1B.12** `map_values(f, a)` (like `map`), `select_keys(a, f)` (like `select`; kept only on `true`); a callback
  failure is the result; callbacks run as a speculative group.
- **1B.13** `failure_tag` of a non-failure or `failure()` is `failure(argument, failure_tag, 1)`.
- **1B.14** Fuel: 1 unit per 16 elements, charged in chunks of 4096 (each charge polls the interrupt); sorts charge
  n log n / 16 up front (`cx.sort` does not poll).
- **1B.15** `is_assoc` added (not on the roadmap list).

## Tests (14 + 1 doctest)

- `tests/functions.rs` (8, about 200 cases): normal and edge cases (empty, `1` vs `1.0`, list and compound keys), every
  failure tag, `a[k]`, a large association on the indexed path; expected values for sets, `counts`, `group_by`, `merge`
  with a combiner and `key_take` checked against short reference computations (values only).
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
all-literal AC left side depended on symbol numbering. Fixed on main (b33430f) by taking a literal call apart first.

## Known gaps

- `a[k]` reaches `lookup` only for symbol and compound keys until `part` declines on associations (asked of 1A).
- `insert`/`delete_key` copy (O(n) each; n inserts O(n^2)); build from a list or with `merge` instead (O(n)).
- The index is dropped at every safe point and rebuilt once per statement; one cache lock shared by all tasks.
- Identity comparison only (no custom equality, no custom `key_sort` order); `is_subset` takes two arguments;
  `lookup` does not take lists of rules or of associations; the canonical sort is not interruptible (bounded, charged
  up front).
