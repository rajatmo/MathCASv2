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
