# Spike 4: parallel evaluation

Code: [`spikes/parallel`](.). Final results: `results/final-*.md` (`--full`, alone on an idle machine). Development
results from a busy machine: `results/run2-busy.md` (the default mode, all cases),
`results/regression-1e6-busy.md` (single-thread regression at 10^6 nodes), `results/run1-busy.md` (an earlier
version, before parallel hashing, holes and the effect-aware rule; kept for the comparisons cited below).

## Question

Can v2 evaluate in parallel inside one shared hash-consed term store, deterministically (semantics 18.1), with
speedups that justify it, and with what design: which concurrent store, which granularity rule, and is speculative
effect ordering (semantics 18.2 item 3) practical?

## Setup

- **Store.** Spike 1's `hc+imm` (32-bit handles, 31-bit immediate integers, cached hash, symbol mask and tree size per
  node) with every operation on `&self`. Nodes live in a segmented arena (fixed 2^16-node segments behind a directory
  of atomic pointers, so nodes never move and reads take no lock); each thread reserves ids in chunks of 256 and
  bump-allocates argument slices from its own 64K-slot chunks. Three interning tables, all giving canonical ids
  (look-up and insertion of a hash happen under one critical section):
  - **A, locked**: one hashbrown table behind one lock (baseline);
  - **B, sharded**: 128 hashbrown tables, chosen by hash bits 25..31, each with its own lock;
  - **C, optimistic**: 128 shards, each an open-addressing array (8-byte slots: 32-bit tag and id, at most half
    full) probed **without any lock or shared write**; only a miss takes the shard lock, re-checks from where the
    lock-free probe stopped, and inserts. Growing publishes a new array; old arrays stay readable.
- **Memo** (semantics 4.4, 18.5): direct-mapped, 2^20 entries, each a seqlock over (key, normal form, epoch, read
  set). A busy entry is skipped by writers (lossy) and missed by readers (never wrong).
- **Evaluator.** A small rule language (arithmetic, `plus`, the R3 derivative as a native kernel `d`, rewrite
  chains `loop`/`loopc`/`walk`, `print`, `define`, `random`, `map`, `pmap`, `ptable`, `table`, `stats`), innermost and
  left to right. The strict arguments of a call form a group that may be split on a rayon pool: the left part stays on
  the task, the right part is a stealable *speculative* task.
  - **Effects.** Only the task on the *sequential frontier* (everything before it complete) may print or define; the
    frontier is handed down the leftmost path of every split, so one task holds it at a time. A speculative task that
    reaches an effect does not wait: it leaves a *hole* for that element and carries on with the next (speculating
    past the effect). After the join, the frontier fills the holes in order, and re-evaluates right-part elements
    whose *read set* (symbols whose own values were consulted, a 64-bit mask, decision 13.1) a definition in the left
    part touched; the memo answers the elements it did not touch.
  - **Random streams.** SplitMix64; `ptable(f, n)`/`pmap(f, xs)` draw one seed from the enclosing stream, and
    element `i` gets the stream `split(seed, i)`. A task owns a stream only on the frontier or as an element of
    these constructs; a draw anywhere else is a hole of its own kind, filled by the owner in order.
  - **Fuel and interrupt** (18.4): per-task counters drawn from one shared budget every 4096 units, polling the stop
    flag at the same points.
- **Granularity rules** compared: `never` (the sequential evaluator, the baseline), `always` (every group split to
  single elements), `static` (split while the cached tree size of a range reaches the grain), `lazy` (decision 4.4),
  and `lazy-stop` (lazy, but a speculative task stops at its first effect instead of speculating past it). Grain 1024
  fuel units, poll 512.
- **Workloads** (class M): M6a `plus` of 64 derivatives of random 5000-node expressions (cost visible in the size);
  M6b `plus` of 64 rewrite chains of 10^4 steps on 3-node terms (cost invisible in the size); M7 `map(inc, list)` of
  10^5 and 10^6 elements; M8 concurrent interning of `g(i)` and `f(i, g(i))`, half the items from a shared pool that
  every worker rebuilds, half distinct; M10 `stats` of `ptable` and `pmap` of 8-step random walks, under two seeds;
  M11 *prints* (64 heavy arguments, five wrapped in `print`), *defines* (odd arguments read the global `c`;
  `define(c, ...)` at positions 20 and 45, `print(c)` at 46), *nested* (a random depth-3 tree of `plus` over heavy,
  reading, printing, defining and random-drawing leaves, including `ptable` with printing elements and `table`
  drawing from the session stream), and *fuzz* (20 more such trees with cheap leaves).
- **Checks.** Every run's value, output log and final definitions are fingerprinted (spike 1's structural
  fingerprint) and compared with the sequential baseline (`never`, one thread); M8 checks that rebuilding every item
  afterwards returns the id first returned and that the table holds exactly the distinct terms; M10 also checks that
  the two seeds differ. 475 class-M configurations in the final run, every one identical to the baseline.
- **Machine.** AMD Ryzen 7 7700X (8 cores, 16 hardware threads), 31 GB, Windows 11, Rust 1.99.1, release build.
  Final numbers come from a `--full` run, alone on an idle machine, sweeping 1, 2, 4, 8 and 16
  threads. The preliminary numbers taken while other spikes ran (at most 4 threads; `results/run2-busy.md`) agree
  with them to within about 10% up to 4 threads.

## Results (final: idle machine, 16 hardware threads)

These numbers were run alone on an idle machine (`results/final-*.md`, one `--full` run per case): medians
of 5 runs, or 3 when a sequential run took over 2 s. Speedup is t(1)/t(k) for the same store and rule; efficiency
(eff.) is speedup/k. "vs seq" is t(16) over the sequential baseline. Each case is shown at its largest size.

| Case | Size | Config | t(1) ms | 2 | 4 | 8 | 16 | eff. 8 | eff. 16 | vs seq at 16 |
|---|---|---|---|---|---|---|---|---|---|---|
| M6a deriv | 64 × 20000 nodes | C lazy | 415 | 1.77 | 3.09 | 4.26 | 4.87 | 0.53 | 0.30 | 0.20 |
| | | C static | 422 | 1.81 | 3.26 | 5.27 | **6.93** | 0.66 | 0.43 | 0.14 |
| | | C always | 452 | 1.85 | 3.39 | 5.58 | 7.14 | 0.70 | 0.45 | 0.15 |
| | | B lazy | 402 | 1.65 | 2.74 | 3.40 | 3.71 | 0.43 | 0.23 | 0.25 |
| | | A lazy | 419 | 1.06 | 0.95 | 0.61 | 0.44 | 0.08 | 0.03 | 2.24 |
| M6b loop | 64 × 50000 steps | C lazy | 328 | 1.51 | 2.44 | 3.70 | **4.84** | 0.46 | 0.30 | 0.20 |
| | | C static | 329 | 1.03 | 1.03 | 1.05 | **1.01** | 0.13 | 0.06 | 0.96 |
| | | C always | 315 | 1.48 | 2.30 | 3.75 | 5.47 | 0.47 | 0.34 | 0.17 |
| | | A lazy | 276 | 0.74 | 0.52 | 0.28 | 0.18 | 0.03 | 0.01 | 4.47 |
| M7 map | 10^7 | C lazy | 45.6 | 1.37 | 2.10 | 2.82 | **2.92** | 0.35 | 0.18 | 0.34 |
| | | C static | 51.5 | 1.65 | 2.49 | 3.26 | 3.32 | 0.41 | 0.21 | 0.34 |
| M7 map | 10^6 | C always | **29.4** | 1.95 | 3.52 | 5.96 | 6.55 | 0.75 | 0.41 | 0.93 |
| M8 intern | 10^7 terms | A | 810 | 0.80 | 0.57 | 0.28 | **0.15** | 0.04 | 0.01 | 5.96 |
| | | B | 802 | 1.40 | 2.15 | 3.31 | 4.10 | 0.41 | 0.26 | 0.22 |
| | | C | 883 | 1.60 | 2.14 | 3.78 | **6.32** | 0.47 | 0.40 | 0.16 |
| M8 intern | 10^8 terms | B | 16356 | 1.71 | 2.84 | 4.42 | 5.52 | 0.55 | 0.35 | 0.26 |
| | | C | **11340** | 1.71 | 2.60 | 4.57 | **6.86** | 0.57 | 0.43 | 0.14 |
| M10 ptable | 10^6 elements | C lazy | 6041 | 1.82 | 3.25 | 5.60 | **7.75** | **0.70** | 0.48 | 0.13 |
| M10 ptable | 10^5 elements | C always | 754 | 1.89 | 3.63 | 6.63 | 8.07 | 0.83 | 0.50 | 0.20 |
| | | A lazy | 426 | 1.28 | 0.91 | 0.29 | 0.17 | 0.04 | 0.01 | 5.47 |
| M11 prints | k = 20000 | C lazy | 124 | 1.51 | 2.48 | 3.68 | **4.72** | 0.46 | 0.29 | 0.21 |
| | | C lazy-stop | 126 | 1.69 | 2.56 | 3.68 | 4.08 | 0.46 | 0.26 | 0.25 |
| | | C always | 104 | 1.57 | 2.53 | 3.73 | 4.77 | 0.47 | 0.30 | 0.17 |
| M11 defines | k = 20000 | C lazy | 83.5 | 1.29 | 1.55 | 2.04 | 2.16 | 0.26 | 0.14 | 0.45 |
| | | C always | 95.3 | 1.24 | 1.51 | 1.52 | **0.91** | 0.19 | 0.06 | 1.22 |
| M11 nested | k = 20000 | C lazy | 29.6 | 1.07 | 1.08 | 0.98 | 1.07 | 0.12 | 0.07 | 0.93 |
| | | C always | 33.1 | 1.48 | 1.89 | 2.06 | 1.61 | 0.26 | 0.10 | 0.69 |
| M11 fuzz | 20 trees, k = 1000 | C lazy | 57.9 | 1.01 | 1.01 | 0.97 | 1.00 | 0.12 | 0.06 | 1.00 |
| | | C always | 92.9 | 1.38 | 1.55 | 1.24 | 0.95 | 0.16 | 0.06 | **1.69** |

All 475 class-M configurations (3 stores, 6 rules, 1 to 16 threads, every size) give the same value, output and
definitions as the sequential baseline. At one thread `lazy` runs the sequential loop: 0.96 to 1.04 of the baseline
everywhere. Over all sizes, `lazy` is never more than 1.09 times the sequential time (fuzz, k = 100, 8 threads).

**M7 criterion** (never more than 1.1 times slower than one thread): met by `lazy` and `static` at every size and
thread count; the worst ratio is 0.89 (`lazy`, 10^5 elements, 2 threads). One task per element (`always`) is 5.9 to
6.1 times slower than sequential at one thread.

**Efficiency targets of class M.** The benchmark's target of 0.7 at 8 threads applies to M1, M2, M3, M5 and M9, none
of which exist in this spike. For reference on the term-level cases at 8 threads: M10 reaches 0.70 (`lazy`, 10^6)
and 0.83 (`always`, 10^5); M6a reaches 0.66 (`static`), 0.70 (`always`) and 0.53 (`lazy`); M8 reaches 0.57 (C,
10^8); M6b and M11 prints reach about 0.46. Every case drops to about 0.5 or below at 16 threads, which are 8 cores
with two hardware threads each.

**Single-thread regression**: spike 1's R10 and R11 on the concurrent stores at one thread, against spike 1's
sequential `hc+imm` (10^6 nodes; `results/final-regression.md`):

| Phase | hc+imm (ms) | A | B | C |
|---|---|---|---|---|
| R10 build | 54.4 | 0.89 | 0.92 | 1.06 |
| R10 replace | 82.5 | 0.92 | 0.91 | 1.12 |
| R11 build | 55.0 | 0.91 | 0.93 | 1.16 |
| R11 eval | 89.8 | 0.99 | 0.94 | 0.98 |
| R11 re-eval (unrelated change) | 81.4 | 1.00 | 0.96 | 1.05 |
| R11 re-eval (contained symbol changed) | 83.2 | 0.98 | 1.02 | 1.05 |

At 10^8 terms (M8), B at one thread takes 16.4 s against C's 11.3 s (1.44 times). M8 at 10^8 peaked at 4.8 GB.

**Limits**: a fuel budget of 10^5 on a 3 * 10^6-fuel computation stops every task at 1, 2, 4, 8 and 16 threads under
every rule. The root task's overshoot is at most one 4096-unit block per task: it saw 1.0 * 10^5 to 3.6 * 10^5, the
same at 16 threads as at 4. An interrupt stops it the same way.

## Findings

1. **Determinism holds, and it is cheap to get.** All 475 class-M configurations of the final run (3 stores, 6
   rules, 1 to 16 threads, 10 cases including 20 fuzz trees with every effect kind in random positions) produce the
   same value, the same output in the same order and the same definitions as the sequential evaluator. Nothing on the evaluation
   path needed a lock except the output log.
2. **One lock is not an option, and at 8 to 16 threads lock-free lookups win.** The single-lock table gets slower
   with every added thread (M8: 0.80 at 2, 0.15 at 16; M6b: 0.18 at 16, 4.5 times slower than sequential). Sharded
   locks (B) scale to 4 threads like C (M8 at 10^7: 2.15 against 2.14), then flatten: 4.10 against C's 6.32 at 16
   threads at 10^7, and 5.52 against 6.86 at 10^8, where C finishes in 1.65 s against B's 2.96 s. The same holds in
   evaluation: M6a with the lazy rule reaches 4.87 at 16 threads on C against 3.71 on B.
3. **At one thread, sharded locks cost nothing on small and medium stores, and C costs 6 to 16%; at 10^8 terms the
   order reverses.** B runs spike 1's R10/R11 at 0.91 to 1.02 of the sequential `hc+imm`. C costs 6 to 16% on builds
   dominated by new terms (R10 build 1.06, R10 replace 1.12, R11 build 1.16), because its 8-byte slots at most half
   full take 16 to 32 bytes per term against hashbrown's ~5. At 10^8 terms, C is 1.44 times *faster* at one thread
   (11.3 s against 16.4 s). The likely reason, inferred and not measured: growing a hashbrown shard rehashes each
   entry by reading its node's hash, which at 10^8 terms is a random access into 2.6 GB of arena; C re-places entries
   from the slot alone.
4. **Canonical ids are not deterministic ids.** Equal terms always get the same id (verified on M8 by rebuilding every
   item), but which id depends on the schedule (per-thread id chunks). Results stay deterministic because nothing
   observable uses an id's value: hashes are structural and fingerprints traverse structure.
5. **The hash of a wide node was a serial bottleneck.** Spike 1's hash is a sequential fold over the arguments, so
   building the 10^6-element result list of M7 was a serial step that capped M7 at 1.57 times at 4 threads
   (`run1-busy.md`). A polynomial hash, `H(xs ++ ys) = H(xs) * K^|ys| + H(ys)` (wrapping) followed by a finaliser, is
   the same function computed in parallel chunks: M7 went to 2.6 times with no single-thread cost (the regression
   table above already uses it).
6. **Granularity control is necessary, and size alone is not enough.** One task per element (`always`) makes M7 5.9
   to 6.1 times slower than sequential at one thread. The cached tree size (`static`) handles M7 and M6a but cannot see a
   cheap-looking call that is expensive: M6b gains nothing (0.99 at 4 threads). The lazy rule (decision 4.4) does
   both, and is never slower than one thread: at 16 threads M6a reaches 4.87, M6b 4.84 and M7 2.92. It is
   conservative at high thread counts, though: on M6a the size-only rule reaches 6.93 and `always` 7.14 at 16
   threads, and on M7 `static` reaches 3.32. The likely cause is in the code: after a split, the left half gets no
   hint and must wait for the task's poll budget, which is checked only between elements. So the number of tasks
   grows one level per completed element, and reaching 16 tasks takes log2(16) element times.
7. **Blocking on an effect deadlocks a work-stealing pool; aborting does not.** If a speculative task waits for its
   left siblings, a worker that is itself waiting in a join can pick it up and block above the frame it waits for
   (frontier task P on thread X waits for a stolen half; X picks up Q, right of P; Q blocks until P finishes; P cannot
   finish until Q returns). Holes plus in-order filling give the semantics of 18.2 item 3 without ever blocking a
   worker.
8. **Speculative effect ordering is practical when effects are sparse.** With five prints among 64 heavy arguments,
   M11 reaches 4.72 times at 16 threads, close to the pure loop case M6b (4.84). Speculating past an effect, rather
   than stopping at it, matters only at high thread counts: equal up to 8 threads, 4.72 against 4.08 at 16. With
   definitions, read sets confine re-evaluation to the readers (44 elements re-evaluated, the memo answering the
   others). The chain of readers after each definition limits the result to 2.16 at 16 threads.
9. **Dense effects in fine-grained code make speculation a loss, and more threads make it worse.** On the fuzz
   trees, maximal speculation is 1.6 to 1.8 times slower than sequential at one thread, roughly break-even at 4
   threads, and 1.69 to 2.09 times slower at 16. On M11 defines, `always` at 16 threads is slower than at one (0.91).
   Counting effect-marked arguments at half weight (an earlier, busy-machine experiment) made `lazy` 1.24 times
   slower. Treating an argument that contains a symbol declared `effect` as not worth a task (decision 4.6) keeps
   `lazy` between 0.96 and 1.09 of sequential on fuzz and nested at every thread count, at the price of the 2.06 that
   `always` finds on the nested tree at 8 threads (whose `print(heavy)` leaves do have a large pure part).
10. **Split streams make random code deterministic and parallel.** M10 is identical at every thread count and rule,
    the two seeds differ, and `lazy` reaches 3.25 at 4 threads, 5.60 at 8 (efficiency 0.70) and 7.75 at 16 on 10^6
    elements. The `ptable` seed is a draw from the enclosing stream,
    so a `ptable` is itself an ordered effect, while its elements own their streams and speculate freely.
11. **The memo must be stamped with the epoch at which the evaluation began.** Speculative tasks run concurrently with
    definitions and may read a new value racily; their memo entries stay harmless because they carry their starting
    epoch and their read set (which contains the symbol they read), so a later reader rejects them. Stamping with the
    epoch at write time would let a stale result pass.
12. **Term-level parallelism pays off on independent heavy arguments.** At 16 threads: 4.8 to 7.8 on M6, M8 and M10
    (C store), 2.9 on M7 (where much of the remaining time is one serial step building the result list), about 2.2 on
    definition chains, and none on effect-dense trees. Efficiency at 8 threads is 0.46 to 0.70; at 16 threads (8
    cores, two hardware threads each) it is 0.3 to 0.5.

## Decisions

| # | Decision | From |
|---|---|---|
| 4.1 | The concurrent store is **C**: 128 shards chosen by hash bits, each an open-addressing array probed without a lock or a shared write, with the shard lock taken only on a miss (re-check from where the lock-free probe stopped, then insert). Nodes go in a segmented arena (fixed segments behind atomic pointers, never moved), with ids and argument slots reserved in per-thread chunks. The switch condition this decision set is met: B runs at 0.71 of C's speed on M8 at 16 threads (10^7) and 0.56 at 10^8, and at 10^8 C is faster even at one thread. Next step: compact slots (or tag groups) to remove C's 6 to 16% single-thread cost on small and medium builds. | findings 2, 3 |
| 4.2 | Term ids are canonical but schedule-dependent: no observable result, order (semantics 8) or iteration may depend on an id's value. | finding 4 |
| 4.3 | The structural hash of a compound term is a polynomial in its parts, combinable across a split, then finalised; nodes wider than 2^15 arguments hash in parallel. | finding 5 |
| 4.4 | Granularity rule (`lazy`): with one worker, never split. Otherwise a range of a group is considered for splitting at once when its O(1) static estimate (the parent's cached size) reaches the grain, else at most once per 512 fuel units of the task (a per-task budget). It is split in two when its estimated cost reaches the grain (1024 fuel) *and* the worker's deque is empty; the estimate is the larger of a sampled static estimate (per argument: tree size, or the learned cost of its head when larger) and the measured cost of the elements done so far extrapolated to the rest. Static size alone, and one task per argument, are rejected. Proposed amendment, not yet measured: when a range splits, give each half half the hint, so the left half can split again at once instead of after the task's poll budget (finding 6). | findings 6, 9 |
| 4.5 | Adopt speculative effect ordering, implemented as: the frontier passed down the leftmost path; speculative tasks never block, they leave holes and continue past them; the frontier fills holes in order and re-evaluates elements whose read set a definition touched. | findings 1, 7, 8 |
| 4.6 | The granularity rule gives no weight to an argument that contains a symbol declared `effect` (semantics 9); inside `ptable`/`pmap` elements only global effects count. The declarations are used for performance only: an undeclared effect is still ordered correctly. | finding 9 |
| 4.7 | Random streams are SplitMix-style: `ptable`/`pmap` draw one seed from the enclosing stream and element `i` uses `split(seed, i)`; an element owns its stream. | finding 10 |
| 4.8 | The memo of normal forms is a concurrent lossy cache (seqlock entries, readers write nothing), each entry stamped with the epoch at which its evaluation began and with its read set; speculative tasks may write it. | finding 11 |
| 4.9 | Fuel is per-task, drawn from the shared budget in blocks of 4096; the interrupt flag is polled at the same points. The overshoot (at most one block per task) falls within the nondeterminism 18.1 already allows in `failure(limit, ...)`. | limits check |

## What it rules out

- A single lock around the interning table, and a shared id counter touched per node.
- Blocking a worker on an effect (deadlock in a work-stealing pool).
- Granularity by static term size alone, and one task per argument.
- A sequential-fold structural hash (it serialises wide nodes).
- Using ids as order or iteration keys, and stamping memo entries at write time.

## Follow-ups

- **Lazy rule at 8 to 16 threads:** measure the half-hint amendment to 4.4 against `static` on M6a and M7, the cases
  where `lazy` trails by 30% at 16 threads.
- **Compact slots for C** (decision 4.1): 4-byte slots or SIMD tag groups, aiming for B's single-thread cost on
  R10/R11 while keeping lock-free lookups.
- **Collector interaction (spike 1b):** the arena's ids have gaps (per-thread chunks), so a sweep needs each chunk's
  high-water mark; the optimistic table's retired arrays and the memo need the collector's safe points (all tasks
  stopped) to be freed.
- **Effect-dense code:** a better estimate of the pure part of effect-marked arguments (for example the measured cost
  before the first effect, learned per head) could recover the 1.5 times `always` finds on the nested tree without
  the fuzz losses.
- **Cancellation:** a speculative right part keeps running after its left sibling made it useless; cancelling it
  would free the worker sooner.
- **Per-element read sets** after a definition (the union over a part re-evaluates more than needed; the memo hides
  most of the cost here).
- **Recursive heads:** the learned head cost is an average over all calls of the head, which a recursive function's
  many small calls drag down; binary recursions whose cost is invisible in the size (fib-like) are not split early.

## Running

    cargo build --release --offline     (CARGO_TARGET_DIR: build/target_parallel)
    parallel --quick                    seconds; smallest sizes, one run, 1/2/4 threads
    parallel                            about 75 s; two sizes, three runs, 1/2/4 threads
    parallel --full                     all sizes, five runs (three when a run exceeds 2 s), 1/2/4/8/16 threads

Options: `--threads 1,2,4`, `--runs N`, `--stores ABC`, `--grain G`, `--poll P`, and case filters (`m6`, `m7`, `m8`,
`m10`, `m11`, `fuzz`, `limit`, `regression`). The exit status is 1 if any check fails.
