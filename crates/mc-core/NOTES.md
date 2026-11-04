# Package A: `mc-core`

Status: `mc_api::conformance::check` passes against the real store; I1's and I3's stress invariants pass at 1, 2 and 4
threads; the short benchmark runs under the guard with every check matching the reference. `cargo build`, `cargo test`
(debug and release) and `cargo clippy --offline --all-targets` are clean, with `#![warn(missing_docs)]`.

## What was built

- **Heap layout (I1).** The old region is dense arrays (nodes, args, limbs), an open-addressing table, and old boxed
  bytes. The young region is spike 4's store C: per-thread id chunks of 256, a lock-free 128-shard table with retired
  arrays freed at safe points, and a reset at each minor collection so stale young handles panic.
- **Promotion and collection.** Chunk-order promotion (D2): a descending mark with a stack for out-of-order edges, then
  ranks. The minor copy and old-table insertion run on the pool. The major collection is a sequential in-place slide,
  with the table rebuilt in parallel and a fast path when nothing died.
- **Nodes (I3).** 32-byte nodes (asserted at compile time). Kinds App, Word, Rat, Real, Int, Ball, Str. All integers go
  through one canonicalising `big_int`: a 30-bit immediate, else a one-limb Word, else a payload. Word values that fit
  `i64` view as `View::Int`. Payloads are hashed in full when built; their bytes count towards the triggers.
- **Assembly decisions.**
  - **A.1:** the young memo is dense, in lazily zeroed segments parallel to the arena; survivors' entries are written
    at their new ids during the copy. The old memo is a `Vec` parallel to the old nodes and slides with them. A side
    memo holds old-key/young-value entries and is swept at every minor collection.
  - **A.3:** each thread keeps a chunked list of the young boxed nodes it made; the minor collection frees the dead
    ones.
  - **A.4:** payloads of at most 64 words go in per-thread slabs.
  - **A.8:** the wide-hash combiner carries the combined length; the seqlock writer has the release fence. Memo entries
    keep the start epoch and the read set.
- **Session and safe point.** `Session::step` gives a higher-ranked scope; the safe point runs on the session's own
  pool.

## Public API

- `Heap` implements `Store`, plus `Heap::new(Config)`, `config`, `safe_point(&mut self, roots: &mut [&mut dyn Trace])`,
  `collect_minor`, `collect_full`, `memo_get(t) -> Option<MemoHit { value, epoch, deps }>`,
  `memo_put(key, value, epoch, deps)`, `meta(t) -> (hash, mask)`, `mask`, `verify(roots)`, `gc_stats`,
  `reserved_bytes`, `boxed_bytes`, `stored`, `old_len`, `sym_count`.
- `Config { collect, nursery, min_major, stress, par_gc }` with `default`, `no_collect`, `stress` (collect at every
  safe point, a major every fourth, verify before and after).
- `Raw::new(T)` / `Raw::get(&Heap)`, the `Trace` trait (for `Roots`, `Raw`, `Vec<X>`, `Option<X>`), `Roots`.
- `Session<St: Trace + Send>`: `new(cfg, threads, state)`, `step(for<'h> FnOnce(&'h Heap, &mut St) -> R)`, `read`,
  `collect_full`, `verify`, timing fields.
- `GcStats`, `GcEvent`, `GcKind`, `SMALL_MIN`/`SMALL_MAX`, re-exports of `Store`, `Sym`, `View`, `T`.
- `shapes`: S3, S8, LS (with a small memoised rewriter), M8 and NUM, plus `run`, a store-generic `fingerprint`, `Rng`.
- Binary `mc-core-bench` (`--shapes`, `--threads` with lists joined by `+`, `--quick`, `--stress`): `[k/N pct%]`
  progress, exit status 1 on a mismatch.

## Tests (all pass, debug and release)

- **conformance:** a fresh heap, no collection, on a 4-thread pool, and with a populated old region.
- **stress:** every shape in stress mode at 1, 2 and 4 threads, parallel and serial copy, against the non-collecting
  single-threaded reference; also a 1-byte nursery.
- **numbers:** integer tiers and demotion, Word views beyond `i64`, rationals, reals by bit pattern, balls, strings
  (empty, non-ASCII, long), the arena/box boundary, A.3 freeing, payload bytes triggering collections.
- **memo:** young entries move with their keys and keep their stamps, weak in values, side-memo entries survive into the
  old memo, entries survive the major slide, a 4-thread stress never returns a wrong pair.
- **handles:** a 200k-wide list built 21 times in parallel gets one handle and the sequential hash; stale young and
  moved-away old handles panic; 200k-deep terms collect without deep recursion; an out-of-order edge built on two
  threads survives minor and major collections.
- **doctests:** three `compile_fail` cases, each failing for the right reason: a handle kept across a safe point
  (E0502), returned from `step` (lifetime error), moved into `rayon::spawn` (E0521).
- Unit tests: seqlock, side-memo drain, zeroed segments, parallel versus sequential hashing.

## Measurements (busy machine, bench scale, ms; reference check ok everywhere)

| shape | none, 1 / 4 threads | gen, 1 / 4 threads | gen, 4 threads: work / safe points | max minor / major pause | MB at end, gen / none |
|---|---|---|---|---|---|
| S3 (10^6 elements, 2000 steps) | 69 / 72 | 74 / 74 | 8.4 / 8.8 | 1.5 / 1.6 | 24 / 22 |
| S8 (10^6 items) | 460 / 322 | 581 / 407 | 56 / 101 | 11.5 / 5.7 | 193 / 145 |
| LS (300 evaluations, 10^4 nodes) | 484 / 164 | 285 / 119 | 106 / 7 | 0.26 / 0.16 | 6.4 / 193 |
| M8 (4·10^5) | 82 / 65 | 90 / 73 | 9.5 / 10.3 | 2.2 / 0.6 | 27 / 18 |
| NUM (20 × 4000 records) | 288 / 122 | 236 / 104 | 56 / 19 | 2.4 / 0.5 | 76 / 148 |

## Decisions

1. **A-core.1 Node layout:** a manual 32-byte layout (`hash`, `mask`, `data`, `head`, `meta` with 3 kind bits and 29
   length bits). I1's tree-size field (granularity estimate) and owner field are dropped; arity and payload length are
   capped at 2^29−1.
2. **A-core.2 Kinds:** reals are nodes identified by bit pattern; strings are payloads (UTF-8 in zero-padded words,
   byte length in `head`).
3. **A-core.3 Placement by length alone:** at most 64 words in the arena, more in a box owned by its node in both
   regions.
4. **A-core.4 One per-thread slab:** a single 64 KB slab per thread for argument slots and arena payload words;
   requests over 1024 words get their own allocation.
5. **A-core.5 Box lists:** per-thread chunks of 64 ids walked at the minor collection; survivors' boxes pass to the old
   region, dead old boxes are freed in the slide, `Drop` frees the rest.
6. **A-core.6 Memo entries:** 24 bytes, key stored as a staleness check; immediates not memoised. The old memo takes 24
   bytes per old node (about 48 MB of S8's 193 MB).
7. **A-core.7 Side memo:** direct-mapped, lossy, resized to 2^12–2^20 entries at each minor; dirty bits per 64-entry
   block; an old-key/young-value put empties that key's old entry; lookup tries old then side; at the minor a side
   entry goes into the old memo unless the old entry has a newer epoch.
8. **A-core.8 Promotion order:** D2 only.
9. **A-core.9 Parallel copy:** on a pool thread with at least 4096 survivors (always in stress mode); memo entries move
   inside the parallel copy.
10. **A-core.10 Session shape:** `Session` is generic over one state type, `step(&'h Heap, &mut St)`; each session owns
    its pool.
11. **A-core.11 Public `Raw`:** so other crates can build `Trace` containers. An untraced `Raw` goes stale: a young one
    panics on use, an old one can name another term after a major collection.
12. **A-core.12 Stats:** bounded totals plus the last event.
13. **A-core.13 Symbols:** hashes in a lock-free segmented array, names under a read-write lock, up to 2^30−1 symbols.

## Contract observations (none blocking)

- `Sym(pub u32)` can be forged; mc-core checks the index in `sym_name` and hashing but not in `sym_term`.
- Conformance does not check that a ball has at least 3 words; mc-core asserts it in debug builds only.

## Known gaps

- Marking and the major slide are sequential (A.7 deferred): on S8 at 4 threads safe points take 101 ms against 56 ms
  of work.
- No safe point inside an evaluation.
- The old memo is a dense array with no lazy segments.
- A rational's reducedness is checked only by `verify`, and only when both parts fit a word.
- Strings are hashed from native-endian words.
- Limits: 2^30 young ids per cycle, 2^30 old nodes.
- The benchmark has no spike or D1 baselines.

## What integration needs from this package

- **mc-eval** uses `Session`, `Heap::memo_get`/`memo_put` with epoch and read set, and `meta`/`mask` for normal-form
  stamps. Its rule tables and indexes implement `Trace` and key on symbols or `structural_hash` (A.2).
- The tree-size field is gone: mc-eval's granularity estimate must come from argument counts or its own measure.
- Hole ids `0x7FFF_FFFE` and `0x7FFF_FFFF` remain free.
- mc-arith, mc-syntax and mc-bench use `Heap` through `Store` unchanged; `shapes::fingerprint` works on any `Store`.

## Package N4a: packed real and integer arrays

Status: merged (branch `lib/r6-n4a`, commits 0ca6016..1d30677, from main 6376b35; merge 2925bcc). On the branch: full
suite 711 passed, 0 failed, 6 ignored (peak 2762 MB); clippy clean in mc-api, mc-core, mc-eval, mc-lists. Spans
`mc-api` (contract), `mc-core` (store), `mc-eval` (evaluator hooks), `mc-lists` (producers and fast paths).

### What was built

- **Store** (`mc-core`): node kind `Packed` (7): one node with a contiguous payload (dimensions, then leaves, as `u64`
  words; the `head` word holds the leaf kind in bit 0 and the rank above). No children, so the collector moves it like
  any payload; long payloads are boxed (dead young boxes freed by the minor collection, dead old ones by the major
  slide). `intern_owned` hands the leaf vector to the young arena as its box (no second copy of an 80 MB payload).
  `verify` checks rank, dimensions, at least `PACK_MIN` leaves, and that hash and mask equal the compound term's.
- **Unpacking cache** (`mc-core/src/unpack.rs`): the first `view` of a packed node in a GC cycle builds its argument
  handles (interned leaves for rank 1, row arrays above); later views return the same slice. 64 locked shards; large
  arrays unpacked in parallel on pool threads; not traced, emptied at every minor and major collection (its handles
  may be young or renumbered); its bytes count toward the nursery trigger and `reserved_bytes`.
- **Contract** (`mc-api`): module `packed` (`Packed<'h>` with `Reals`/`Ints` variants, `Elem`, `PACK_MIN = 64`,
  `MAX_RANK = 16`, `pack_shape` (the reference rule, written against views), `build_nested`, slice conversions).
  `RefStore` packs by the same rule (keeps the compound node, adds a payload copy).
- **Evaluator** (`mc-eval/src/packed.rs` and hooks): while `list` has no definitions, a packed array is its own normal
  form; `eval`, `enter`, `needs_eval`, `args_from`, `estimate`, `first_failure` and the AC `canonical` flattening never
  view (unpack) it. A listable two-operand `add`/`mul` with a packed operand and a packed array of the same shape or a
  machine number is computed on the payloads (`Kernel::thread_packed`): integers exactly (overflow falls back to
  threading), an integer meets a real as its exact `f64` up to 2^53, reals by IEEE (commutative). NaN operands,
  unequal shapes, other heads or arities (including `pow`) use ordinary threading, so `failure(shape_mismatch)` is
  unchanged. Fuel n/16, polling every 65,536 leaves.
- **Library** (`mc-lists`): `range` (integer, and machine-real bounds computing `a + k*step` with the machine
  arithmetic `Real::add`/`mul` use) builds payloads directly; `length` reads the first dimension; `part` reads an
  integer index, a row or a final span from the payload (no view for its association check); `sum` (rank 1) and
  `total` (any rank) on the payload; helpers `packed_arg`, `packed_slice` in `ctx`.

### Decisions

- **N4a.1 Identity.** Packing is canonical, so packed and unpacked forms are the same handle: every construction of a
  packable list (`app(list, ...)`, `real_array`, `int_array`, `reals`, `ints`) yields the packed node and no unpacked
  twin exists. Structural hash and symbol mask equal those of the compound `list(...)`, computed from the leaves
  without building them (`packed_hash`, also the parallel fold): `===`, rule indexes and `structural_hash` need no
  case for packing.
- **N4a.2 Rule.** `list(a1..an)` is packed exactly when rectangular, rank 1 to 16, all leaves machine reals or all
  machine integers (`View::Int`, fitting `i64`), and at least `PACK_MIN` leaves. Mixed kinds, big integers, ragged
  rows, other heads, empty dimensions, rank above 16 stay compound; short compound rows that together reach the
  threshold are packed (16 rows of 4).
- **N4a.3 Threshold 64** (measured, `crates/mc-core/examples/pack_threshold.rs`): building from element terms and
  viewing once costs 0.99-1.08x for 8-48 leaves, 1.11-1.17x for 64-256 (165-199 ns per element), 0.95 at 1024, 1.46
  at 8192; from 64 up retained memory more than halves (about 81 vs 183 bytes per element). Payload producers and fast
  paths offset the generic-path cost.
- **N4a.4 Unpacked arguments** are a per-cycle untraced cache emptied at every collection; a handle from it is an
  ordinary term and must be rooted to survive a safe point.
- **N4a.5 No symbols interned up front** (the heap learns `list`'s bits on first `sym("list")` or interns it at the
  first direct packed construction; `mc-eval` recognises `add`/`mul` by name only with a packed operand): interning at
  startup shifted symbol indices and read-set bits, and broke `mc-assoc`'s `lookups_do_not_scan` fuel test through a
  memo miss on a bit collision.
- **N4a.6 Inert packed arrays** are normal forms; the read of `list` is recorded as evaluating the compound would.
- **N4a.7 Element-wise `add`/`mul` on payloads** under the conditions above; differential tests check them bitwise
  against threading.
- **N4a.8** `length`, `part` read the payload without unpacking.
- **N4a.9** Producers build payloads directly (`range`; `real_array`/`int_array` in the store).
- **N4a.10** `sum`/`total` on payloads: integers an exact 128-bit sum; reals sorted ascending then folded from +0.0,
  exactly `mc-arith`'s `Num::sum` order (B.5; ties cannot change the result, the running sum never becomes -0.0);
  NaN falls back to the general path.
- **N4a.11** `normal(t)` of a packed array is the same handle (no separate unpacked form); `mc-poly`'s function
  unchanged.

### API additions (source-compatible)

- `mc-api`: `Store` default methods `packed`, `real_array`, `int_array`, `reals`, `ints` (existing implementors
  compile unchanged); module `packed` (re-exports `Packed`, `Elem`, `PACK_MIN`); `conformance::check` gains packed
  consistency checks (any store); new `conformance::check_packing` (the exact rule, concurrent construction).
- `mc-core`: `Heap` overrides `packed`, `real_array`, `int_array`; re-exports `Packed`, `PACK_MIN`.

Edits outside: `mc-engine/src/arith.rs` (add/mul listable; merged with main's add/mul/pow listable); `mc-lists`
dev-dependency on `mc-bench` (measurement example). After the merge the prelude's `dot` is `sum(u*v)` (fully packed).

### Tests

- `mc-api` conformance (`check_packing` on `RefStore`): packed equals element-by-element construction at lengths 0-1000
  around the threshold with awkward reals (±0, ±inf, NaN payloads, subnormals, extremes) and integers of every tier;
  matrices and rank-3 arrays equal the list of their rows; mixed, ragged, other-head and big-integer lists unpacked;
  the exact rule, `MAX_RANK`, agreement with `pack_shape`; four threads building the same arrays by different routes
  get the same handles.
- `mc-core`: the four conformance tests run `check_packing` (fresh heap, no collection, pool, old region);
  `tests/packed.rs` (5): payloads and views survive minor and major collections (also stress mode), dead payloads
  freed; memo with packed keys and values (side memo too); canonical construction and racing parallel unpacking (one
  cached slice); 8 bytes per leaf and no heap cell per leaf; integer tiers. Unit test: packed hashes equal `app_hash`
  (sequential and parallel).
- `mc-lists` `tests/packed.rs` (4): element-wise add/mul equal element-by-element evaluation over 60 random rounds of
  every operand-kind combination (NaN, overflow, 2^53), most results stay packed; `sum`/`total` equal the canonical
  `add` of the leaves (40 rounds, reals and integers, all -0.0); producers and access (`length`, `part` index, row,
  span, head, out of range), `range(100) + [1, 2]` is `failure(shape_mismatch)`; fuel limits stop `sum(range(10^6))`
  and `2.0*v`; identical session results at 1 and 4 threads under stress.

### Measurements (release, guarded, shared machine; `crates/mc-lists/examples/packed_measure.rs`, 10^7)

| statement | before | after |
|---|---|---|
| `range(10^7)` (integers) | 47 ms | 26 ms |
| `range(1.0, 10^7)` | 3.43 s | 27 ms |
| `total(v)` / `sum(v)` | 480 / 485 ms at 10^6; out of memory at 10^7 | 24 / 23 ms |
| `v[n/2]` | 0.6 ms | 0.3 ms |
| `v + w` | a symbolic sum (before list threading) | 35 ms, element-wise, packed |
| `2.0 * v` | a symbolic product | 35 ms |
| `random_real(1.0, 10^7)` | 174 ms at 10^6 | 1.57 s at 10^7 (element terms, gap 1) |

Peak memory: `v = range(1.0, 10^7); length(v)` 1839 MB before, 160 MB after (80 MB payload retained; the peak includes
the transient leaf vector); `range(10^7)` of integers 83 MB before, 160 MB after (gap 2); the whole run at 10^7 hit
the 3 GB cap before, peaks at 1197 MB after (dominated by `random_real`). S-class (interleaved medians): S1, S2, S6,
S8 equal; S7 10^6 48-56 ms vs 58-61 ms (about 5-10% slower, noisy). S1 and S8 at 10^7 still stop at the memory limit
(bound by `map`'s evaluator results).

### Known gaps

1. `random_real(n)` (`mc-random`) and other natives build element terms and let `app` pack them (peak about 1 GB at
   10^7); they can switch to `Store::reals` / `real_array`.
2. Packed integers take 8 bytes per leaf vs 4 for immediate small integers in a compound term.
3. `pow`, three-operand sums or products, NaN operands and other listable functions use ordinary threading (unpack).
4. Other `mc-lists` functions (`map`, `select`, `take`, `reverse`, `join`, ...) unpack on view (cached per cycle) and
   repack their results: correct, not payload-fast.
5. Building a packable list from element terms and viewing it once is 10-17% slower than a compound term (N4a.3).
6. S7 may be slightly slower (within noise).
7. Compiled code (`mc-jit`) does not take packed arrays as arguments yet.
