# Integration spike I1: generational collector × concurrent store

Code: `spikes/int_gc`. Raw results: `results/full-busy-1.md`, `full-busy-2.md` (two `--full` runs) and
`stress-busy.md` (`--stress`). Builds with no warnings, and `cargo clippy --offline --all-targets` is clean.
`cargo test` passes in debug and release: 5 integration tests, 2 unit tests, and 4 doctests, 3 of them `compile_fail`.

## Question

Can spike 1b's generational collector (decisions 1b.1 to 1b.5) run under spike 4's concurrent store C, its lossy
seqlock memo and its work-stealing evaluator? The known conflict: spike 4's per-thread id chunks break the global
child-before-parent order that 1b's stackless marking and renumbering by rank rely on.

## Setup

- **Heap** (`src/heap.rs`, `src/young.rs`, `src/memo.rs`):
  - **Young region:** spike 4's store C, used as the nursery. It has a segmented arena with per-thread chunks of 256
    ids and 64 KB of argument slots, and 128 shards of open-addressing arrays probed without locks, with a lock only
    on a miss.
  - **Old region:** a dense node array plus an open-addressing table. It does not change between safe points, so every
    thread reads it with plain loads.
  - **Lookup:** old table first, then young table. The old probe is skipped when any part of the new term is young.
    This filter is exact, because old nodes never point to young ones.
  - **Immediates:** symbols and 30-bit integers (1b.3).
  - **Memo:** spike 4's seqlock memo split into an old and a young memo. The slot is chosen by the key's *structural
    hash*, so renumbering rewrites an entry in place instead of moving it.
- **Minor collection** at a safe point once the young region holds about 1 MB. Survivors go to the end of the old
  region; the young table, arena and memo are reset wholesale. Two promotion orders are compared:
  - **D1 (topological):** post-order traversal from the roots with an explicit stack and a forwarding table over young
    indices. The old region keeps 1b's child-before-parent invariant, so major collections stay stackless.
  - **D2 (per-thread chunk order):** a descending pass over young indices, with an explicit stack only for
    *out-of-order* edges (a child index above its parent's). Survivors are renumbered by rank. Out-of-order edges
    survive into the old region, so major marking needs the same stack fallback.
  - `D1s`/`D2s` are the same with a fully sequential collector. `D1`/`D2` run the copy phases on the pool's idle
    workers.
- **Major collection:** 1b's sliding compaction of the old region when it has doubled. When nothing died, every rank is
  the identity, so the slide, table rebuild and memo sweep are skipped.
- **Evaluator** (`src/eval.rs`): spike 4's evaluator ported to branded handles. Its session state (`Defs`: own values,
  output log) is a `Trace` root container.
- **Workloads** (`src/work.rs`), each a session of evaluations with a safe point after each:
  - M6a: 8 × (build 64 expressions in parallel, then evaluate the derivatives), n = 5000.
  - M8: 2·10^6 items in 10 parallel evaluations, plus a parallel canonical-id recheck; after a full collection the heap
    must hold exactly the distinct live terms.
  - M10: 4 × `ptable`/`pmap`, n = 10^5.
  - S3: 10^6 elements, 10^4 replacement evaluations.
  - S8: 10^6 items, 100 parallel evaluations.
  - LS: 1000 parallel evaluations of 10^4-node temporaries, kept results with back pointers, a redefinition every 50th.
- **Configurations:** reference `ref` (no collection, 1 thread, `never` split); `none` (spike 4's store, no
  collection); D1, D2, D1s, D2s at 1, 2 and 4 threads with the `lazy` rule.
- **`--stress`:** collects at every safe point (a major every fourth) and verifies before and after each collection:
  - every node is in exactly one table and is found again by its content;
  - no node, root or memo entry refers to a freed or moved-away id;
  - in D1, every child id is below its parent's;
  - every memo entry has a live key and value and sits in its key's hash slot;
  - the young table and young memo are empty after collection.
- **Baselines**, run unchanged from their crates: spike 1b's C1 on S3/S8/LS (its own LS semantics) and spike 4's
  single-evaluation M6a/M8/M10.

## Results (busy machine: three agents sharing 16 threads; two `--full` runs agree within about 5%)

**Correctness:** every configuration of both full runs matches the sequential non-collecting reference. So do all 72
stress configurations and the tests (both orders, serial and parallel copy, `lazy` and `always`, a 1-byte nursery).

Total time against `none` at the same thread count (run 1), and the time spent in safe points:

| case | none 1/2/4 thr (ms) | D2 1/2/4 thr (× none) | D1s 4 thr (× none) | safe points at 4 thr, D2 (ms) |
|---|---|---|---|---|
| M6a | 1669 / 1123 / 798 | 1.17 / 1.26 / 1.35 | 1.73 | 416 (evaluations 316) |
| M8 | 774 / 416 / 225 | 0.94 / 1.12 / 1.28 | 1.59 | 114 |
| M10 | 1124 / 657 / 408 | 0.92 / 0.95 / 0.95 | 0.93 | 10 |
| S3 | 62 / 61 / 61 | 1.21 / 1.18 / 1.18 | 1.27 | 27 |
| S8 | 256 / 168 / 121 | 1.18 / 1.35 / 1.50 | 1.78 | 91 |
| LS | 1966 / 1687 / 1561 | 0.59 / 0.62 / 0.64 | 0.64 | 129 |

Evaluation itself is faster with collection: M6a 1012 against 1323 ms at 1 thread, LS 989 against 1915 ms, S8 141
against 211 ms. The overhead is entirely in the safe points.

**Pauses with parallel workers** (D2 / D1, 4 threads; minor mean / max, major mean / max, ms):

| case | minor | major |
|---|---|---|
| M6a (4.1M promoted nodes) | 16.3 / 47.8 · D1 22.0 / 55.9 | 29.5 / 80 |
| M8 | 10.5 / 21.9 | 4.3 / 6.4 |
| S8 | 0.82 / 9.1 | 2.4 / 5.7 |
| LS | 0.43 / 7.4 | 9.2 / 15.5 |
| S3 | 0.20 / 1.6 · D1 0.26 / 2.0 | 1.1 / 1.5 |
| M10 | 2.6 (almost all young-region reset after about 10^7 garbage terms) | 0.02 |

**Phases:**
- Minor marking: D2 is 2.5 to 4 times cheaper than D1's traversal (M6a 3.1 against 7.6 ms per minor, M8 1.4 against
  2.0, S3 53 against 114 µs).
- The copy phase dominates promotion-heavy minors: about 40 to 75 ns per survivor serially. Parallel copy brings it
  from 20.7 to 10.7 ms per minor on M6a and from 14.5 to 8.5 ms on M8 at 4 threads.

**Memory:**

| case | peak MB, D2 | peak MB, none | after final collection MB, D2 | after, none |
|---|---|---|---|---|
| M10 | 85 | 150 | 19.5 | 149 |
| S3 | 30 | 117 | 29 | 117 |
| LS | 71 | 522 | 71 | 522 |
| M6a (everything kept) | 534 | 548 | 399 | 521 |

**Against spike 1b C1** (1 thread; its numbers / D2s): S3 48.6 / 75 ms, S8 365 / 310 ms, LS 1886 / 1147 ms (different
evaluator, so only roughly comparable).

**Against spike 4 without collection:** the `none` mode reproduces spike 4's store. Its original single-evaluation M8
(2·10^6 items) takes 308 / 131 ms at 1 / 4 threads; here the same work is spread over 10 evaluations plus a recheck.

**Instrumentation:**
- Stress M6a at 4 threads: 21.7k young-table hits on nodes another worker created (about 4.6% of the 476k promoted).
  Each would be a duplicate under private per-thread tables.
- Cross-worker edges among survivors: about 1.3%.
- Out-of-order edges (D2): about 0.1% on M6a, up to 8.6% of edges on S8, where 10k-wide lists point into other
  workers' chunks.

## Findings

1. **The designs compose correctly.** The safe point needs `&mut Heap`; evaluations get `&'h Heap` in a higher-ranked
   scope (`Session::step`), and branded handles move freely between rayon tasks inside it. Three things fail to
   compile, each checked for the right error: keeping a handle across `safe_point`, returning one from a step, and
   moving one into `rayon::spawn`. Scoped rayon tasks end before `step` returns, so the end of an evaluation is the
   global barrier, with no extra synchronisation.
2. **Private per-thread young tables are ruled out by canonicity.** Workers rebuild each other's fresh terms all the
   time (see the instrumentation above), so private tables would lose hash-consing between threads. "Per-thread young
   regions" can only mean per-thread allocation chunks with one shared table, which is D2.
3. **Both promotion orders work, and global order is not worth restoring.** Out-of-order edges are rare, so D2's
   fallback stack stays small; D2 marks faster, and majors cost the same. D1's only advantage is the clean invariant
   in the old region.
4. **Retired lock-free arrays and the memo are handled cleanly at the minor collection.**
   - Retired arrays are freed there, and the table returns to its initial 512 KB (test
     `retired_table_arrays_are_freed_at_the_safe_point`).
   - The young memo is swept: entries whose key and value survive move to the old memo (weak memo, 1b.5). Routing
     every entry that mentions a young term to the young memo replaces 1b's write barrier.
   - Indexing by structural hash makes renumbering free. Memo stamps and read sets (4.8) are unaffected.
5. **A serial collector caps parallel speedup.** At 4 threads M6a spends 416 ms in safe points against 316 ms in
   evaluations. Parallel copy helps (D1s 1.73 → D2 1.35 against none on M6a; S8 1.78 → 1.50), but marking is still
   serial.
6. **Parallel compaction into fresh arrays is a bad trade.** In a measured, then removed, version the peak heap nearly
   doubled (M6a 1000 against 536 MB) and the slide was 2.7 times slower on 1 thread. Majors now slide in place
   sequentially and rebuild the table and memo on the pool.
7. **Spike 4 bug found.** Its parallel hash combiner for nodes wider than 2^15 arguments resets the combined length to
   0. The hash then depends on rayon's reduction tree: the same 200 000-wide list built 21 times in spike 4's store got
   3 different ids. This broke canonical ids, and it surfaced here as an M8 mismatch across thread counts. It is fixed
   in this crate (carry `n1 + n2`) and covered by a test. (Piece I2 found the same bug independently.)
8. **Stale handles crash instead of reading a wrong term.** Young cycles start at fresh segment boundaries and their
   directory entries are cleared, so a young id is never reused until the 2^30 young id space wraps. Unit tests cover a
   stale young handle and a moved-away old one.
9. **Garbage inside one evaluation cannot be collected before it ends.** M10 builds about 10^7 young terms in a single
   evaluation; peak memory follows that.

## Decisions

| # | Decision |
|---|---|
| I1.1 | Young region = spike 4's store C, used as the nursery, with one shared young table. The old region is a dense array plus an open-addressing table, immutable between safe points and read without locks. Lookup goes old, then young; the old probe is skipped when the term has a young part. |
| I1.2 | Promotion keeps per-thread chunk order (D2): descending marking pass with an explicit stack only for out-of-order edges, ranks as new ids. This amends 1b.4: "stackless" becomes "stackless except for out-of-order edges", in both minor and major marking. D1 stays the fallback if a later piece needs child-before-parent order in the old region. |
| I1.3 | No private per-thread interning tables (canonicity, finding 2). |
| I1.4 | The memo is split into young and old memos, indexed by structural hash. The old memo never holds a young term, which replaces 1b's write barrier. The old memo is sized to the old region (2^14 to 2^20 entries) and resized at safe points; the young memo is 2^16 entries. |
| I1.5 | At the minor collection: retired young-table arrays are freed, the young table returns to its initial size, arena segments are pooled (4 kept), and the young id space is advanced so stale handles panic. |
| I1.6 | The safe point is the end of `Session::step`. The whole session runs on a pool worker; evaluations use only scoped parallelism; branded `T<'h>` handles exist only inside the higher-ranked scope. |
| I1.7 | The collector's copy phases run on the idle pool: minor copy and table insertion, major table and memo rebuild. Marking and the major slide stay sequential and in place. |
| I1.8 | Spike 4's parallel-hash combiner must carry the combined length (correction to the code behind 4.3). |
| I1.9 | The nursery trigger counts reserved id and argument chunks. It overstates the young size by at most one chunk of each kind per thread, which is acceptable at about 1 MB. |

## What it rules out

- Private per-thread young tables.
- Restoring global creation order across workers as a hard requirement.
- A memo indexed by id (renumbering would move entries).
- Compacting the old region into fresh parallel arrays (memory cost).
- Freeing retired lock-free arrays anywhere other than a safe point.

## Follow-ups

- Parallel marking: D2's per-chunk pass maps naturally onto per-region work with a worklist for cross edges; also
  parallel, in-place, block-wise sliding. Both are needed for promotion-heavy parallel sessions.
- Pretenuring or an adaptive nursery for all-survive sessions (S8/M8: +28 to 50% at 4 threads).
- A safe point inside long evaluations, at task boundaries (finding 9). This needs evaluator-level rooting of
  in-flight work, which the type brand currently forbids by design.
- Finalising off-arena payloads: neither order visits dead young nodes, so payload-owning young nodes need a per-cycle
  list (per thread) to free the unmarked ones; dead old nodes can be finalised during the slide.
- The young memo sweep (2^16 entries, about 0.2 ms) dominates small minors (LS); it could track dirty blocks.

## What the final assembly needs from this piece

- The heap API:
  - `&'h Heap` methods `sym`, `int`, `app`, `view`, `meta`, `memo_get`, `memo_put`, `fingerprint`;
  - the `Session::step(for<'h> FnOnce(&'h Heap, &'h Defs, &mut Roots) -> R)` scope;
  - `Trace` and `Raw` for root containers.
- **Every structure that keeps terms across evaluations must implement `Trace`:** rule tables and matcher indexes with
  compound patterns or right-hand sides, packed/number payload tables, front-end pins. Only symbols (`Sym`,
  immediates) may be held unrooted.
- Number payloads (piece I3) as heap nodes need the finalisation hook described in the follow-ups.
- The wide-node hash fix (I1.8) must go into whatever store code is reused from spike 4.
- The evaluator's holes use ids `0x7FFF_FFFE` and `0x7FFF_FFFF`, reserved as the last young segment.
- Memo entries keep spike 4's start-epoch and read-set stamps unchanged.

## Running

`CARGO_TARGET_DIR=<build dir> cargo build --release --offline -p int_gc` builds `int_gc`.
- `int_gc --quick` takes seconds.
- `int_gc --full` takes about 85 s on the busy machine, one run per configuration; `--runs N` reports medians.
- `int_gc --stress` takes about 35 s.
- Options: `--threads 1+2+4`, `--modes none+D1+D2+D1s+D2s`, `--no-base`, and case filters. Lists take `+` because
  the guard script splits its arguments on commas.
- The program exits with status 1 on any mismatch with the reference.
