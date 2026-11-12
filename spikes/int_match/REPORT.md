# Integration piece I2: matcher and evaluator on the concurrent store

Crate: `spikes/int_match`. Raw results are in `results/` (`full.md`, `quick.md`, `locks.md`). **All numbers were
taken on a busy machine** (three other jobs on 16 hardware threads, at most 4 threads per process). Only conclusions that
hold within about 1.5 times noise are drawn. All 96 rows of the full run (78 s) give the same result as the baseline
evaluator (sequential, one thread, no memo); on R4, R5, R13, R14 and S7 the results also match spike 2's own
sequential evaluator, run in the same process. 8 `cargo test` tests pass; no build or clippy warnings.

## Question

Do spike 2's matcher and dispatch (decisions 2.1–2.9) work on spike 4's concurrent store C and work-stealing evaluator
(4.1–4.9), including semantics 6.1–6.3? Specifically:
1. Can conditions evaluate terms during matching?
2. Do the reads made inside matching enter the read set?
3. Can rule sets and indexes be shared across threads?

## Setup

- **Heap:** spike 4's store C, plus spike 2's machine reals, canonical order and AC canonicalisation. Every operation
  takes `&self`; symbol names can be read without a lock.
- **Matcher:** spike 2's CPS matcher with the pruning AC strategy. It runs inside a task and holds the task's context,
  so a condition can call the evaluator.
- **Rule index:** spike 2's discrimination tree, AC key-set filter and rule order through the index, rebuilt on
  single-writer structures that readers use without locks (`conc.rs`, `index.rs`).
- **Evaluator:** spike 4's core (holes, lazy granularity, read-set validation, seqlock memo stamped with the start
  epoch) with spike 2's dispatch: argument-head rules, then the head's rules.
- **Spike 2's cases:** R4, R5, R13, R14 and S7 are ported with spike 2's generators and the same symbol creation order.
  The checks are bit-comparable with spike 2.
- **New cases:**
  - **R14e:** `f(x_ : integer) /; is_prime(g(x)) := hi(x)`, where `g`'s rule reads the global `c`. Three variants: the
    condition computed natively, the condition built as a term and evaluated, and evaluated with its reads not
    recorded ("untracked").
  - **MR:** `map(F, xs)` for a rule-heavy `F`. At fixed points in the map, elements redefine `c` (which the conditions
    read), add rules to `F`'s index during the run (including a deeper tree path and a new argument-head rule set),
    replace a rule, and print.
  - **MR fuzz:** 20 random effect layouts.
- **Configurations:**
  - ref: sequential, no memo.
  - seq: sequential, memo.
  - lazy at 1, 2 and 4 threads.
  - Stress modes: split every group (always), a tiny memo of 16 entries, spike 4's hashed memo, and a locked index.

## Results

Times in ms, median of 3 runs.

| Case | spike 2 (1 thread) | ref | seq | lazy 2 | lazy 4 | speed-up at 4 threads |
|---|---|---|---|---|---|---|
| R4, 10^4 AC rules | 120.7 | 126.4 | 126.7 | 66.0 | 33.5 | 3.78 |
| R5, 5·10^4 rules, 10^6 calls | 186.9 | 188.4 | 199.3 | 101.2 | 52.3 | 3.81 |
| R13, 1000 kinds | 222.1 | 225.3 | 227.6 | 111.5 | 78.7 | 2.89 |
| R14, 10^6, 4 phases | 311.7 (memo) | 763 (no memo) | 256.6 | 155.7 | 92.1 | 2.79 (first evaluation 3.1) |
| S7, 10^6 | 81.3 | 101.4 | 100.9 | 54.3 | 29.6 | 3.41 |
| R14e native, 2·10^5 | — | 192 | 98.0 | 63.8 | 28.4 | 3.45 |
| R14e eval, 2·10^5 | — | 1475 | 883.9 | 463.8 | 256.9 | 3.44 |
| MR, 2·10^5 | — | 150.7 | 34.9 | 39.9 | 28.9 | 1.21 (150 000 elements redone) |
| MR fuzz, 20 × 3000 | — | 43.1 | 41.7 | 39.5 | 36.1 | 1.15 |

- **Single-thread cost against spike 2 (ref/seq ÷ spike 2):** R4 1.05; R5 1.01–1.07; R13 1.01–1.02; R14 with memo
  0.82; S7 1.24.
- **Memo kind, sequential:**
  - R14: hashed 480 ms against dense 257 ms.
  - R14e native: 191 ms against 98 ms.
  - MR fuzz: 125 ms against 42 ms.
- **Index lock, 4 threads, 7 runs:**
  - R5: locked 65.0 ms against lock-free 50.2 ms.
  - R4: 32.5 against 32.2 ms.
  - R13: 60.6 against 59.3 ms.
- **Cost of a condition that evaluates (R14e):**
  - 1.17 µs per condition on one thread; 0.40 µs wall at 4 threads.
  - It interns about 9.6 terms per condition (1.93 M terms for 2·10^5 conditions).
  - It scales like the native condition (3.44 against 3.45 at 4 threads).

## Findings

1. **Conditions that evaluate are safe and scale.**
   - All term construction takes `&self`. The evaluation, its tasks and its conditions share one `&'h Heap` scope.
   - The only operation that frees or moves anything is the safe point (`Heap::safe_point`, `Eval::safe_point`), and
     it takes `&mut`. The borrow checker therefore proves that no condition is running when it happens.
   - Spike 2 could not allow this: its matcher held `&Store` while allocating needed `&mut Store`.
   - Concurrent allocation inside conditions costs nothing measurable in scaling at 4 threads.
2. **Read sets through matching are necessary and sufficient.**
   - The untracked variant goes stale in every memoised configuration, including sequential.
   - The tracked variant is correct in every configuration, through:
     - a top-level redefinition of `c`, which is read only inside the evaluation a condition starts;
     - a rule added to `g`, which is consulted only inside a condition;
     - mid-run redefinitions (MR, MR fuzz);
     - effects reached inside a speculative condition (a test checks the output order is sequential).
3. **The rule index needs no snapshot consistency, only memory safety and a before-or-after view per definition.**
   - Every reader of `f`'s rules records `f` in its read set, and a definition of `f` bumps `f`'s epoch.
   - So any speculative work that overlapped a definition is discarded by validation, and its memo entries fail their
     stamps.
   - A test runs one writer (insertions, replacements, a growing tree depth) against 3 readers. No reader ever missed
     an old rule or saw a wrong order.
4. **Spike 4's hashed memo is the main single-thread cost.** Its random accesses cost about 100 ns each once the table
   outgrows cache. A dense memo, with one entry per term id in segments parallel to the arena, brings the matcher level
   with spike 2.
5. **Bug in spike 4's wide-node hash.** Nodes of 2^15 or more arguments are hashed in parallel, but the reduce dropped
   the combined length (`..., 0)`). The hash then depended on the schedule, and one term was interned under two ids.
   R14's stress run found it as "equal results with different ids". It is fixed and tested here; `spikes/parallel`
   still has it.
6. **Spike 4's memo writer relied on x86 store ordering.** Under the formal model a reader could see new data with an
   old sequence number. A release fence fixes it.
7. **Speculation past a definition of the mapped function is wasted.** MR redoes 75% of its elements because every
   element reads `F`. Elements dispatched by argument-head rules survive, since they never consult `F`'s own rules.
8. **The always-split mode costs up to 2 times on tiny elements** (S7 at 0.49). The lazy rule avoids it.

## Decisions

| # | Decision |
|---|---|
| I2.1 | Store API: all construction takes `&self` on a heap borrowed for the evaluation (`&'h Heap`); only safe points take `&mut`. Right sides get `&Heap`; conditions get a `CondCx` (heap, recorded own-value reads, `eval`). There is no `&mut Store` anywhere in matching or evaluation. |
| I2.2 | A condition returns `Res<bool>` and may evaluate. An abort stops the whole match at once, so no later alternative or rule is tried, and in a speculative task it becomes a hole. Dispatch scratch is per frame (a pool of boxed scratch in the task context), because conditions make dispatch re-entrant. |
| I2.3 | Everything a condition reads (own values, rule sets consulted by the evaluations it starts, read sets of memo hits) enters the read set of the evaluation whose dispatch runs it. Confirms 2.7 and 4.5 for evaluating conditions. |
| I2.4 | Rule sets and indexes are updated in place by the single writer (the frontier, or the session) and read without locks or shared writes. Tree nodes, leaves and AC lists are append-only; edges live in single-writer key maps; ranks are an array swapped whole on renumbering; a replacement swaps a pointer. Retired parts are freed at safe points. **Rejected:** rebuilding or copying at the effect point (O(n) per definition, O(n²) for functions that define memo entries as they run), and a reader-writer lock (1.3 times slower on a hot, cheap rule set; held across matching it can deadlock on recursive reads while a writer waits). |
| I2.5 | Definition order is enforced by types: session definitions take `&mut Eval`; inside an evaluation only the frontier defines. |
| I2.6 | The memo is dense, with one seqlock entry per term id in lazily zeroed segments, instead of spike 4's hashed table. It keeps the start-epoch stamp and read set. This amends 4.8. |
| I2.7 | Fix spike 4's parallel wide-node hash: the reduce must carry the combined length. This amends the implementation of 4.3. |
| I2.8 | The seqlock memo writer issues a release fence after marking an entry busy. |
| I2.9 | A reader flattens a subject to the tree depth it loaded, and marks entries at that depth unexpanded so they follow only head and `*` edges. A concurrent deeper insertion can then only be missed (a before view); it can never misalign a query. |

## What it rules out

- Copy-on-write or rebuilt rule sets as the scheme for definitions made during an evaluation.
- Reader-writer locks held across matching.
- `&mut Store` in the matcher, right-side or condition API.
- Condition reads that are not recorded in the read set.
- A hashed lossy memo as the primary memo for large working sets.

## Follow-ups

- **Fuel in speculative tasks.** A speculative task's fuel counts against the shared budget, so discarded work can
  trigger a limit failure. This is inherited from spike 4, and worse now that conditions evaluate. Proposal: only the
  frontier decides a limit failure; a speculative task that exhausts its share leaves a hole.
- **Finer validation after a rule insertion.** Only redo speculative elements whose subject the new rule matches,
  instead of every element that read the symbol (MR).
- **Conditions are evaluated more than once.** The AC candidate pre-test and backtracking both run conditions. Results
  stay deterministic, but the semantics should say a condition may run any number of times, or forbid effects in
  conditions.
- **Lock cost beyond 4 threads** was not measured.
- **S7 is still 1.24 times spike 2** (task and closure overhead per element).

## What the final assembly needs from this piece

1. **Collector interplay (I1):**
   - Index keys (tree edges, AC keys) are term ids, and so is the dense memo. A renumbering or moving collection must
     re-key the indexes, or they must be keyed by structural hash. The memo is weak and can simply be cleared.
   - Safe points must also call `Defs::reclaim` and the interning table's reclaim.
   - Conditions create a lot of short-lived garbage (about 10 terms each in R14e), so per-thread young regions matter.
   - Definitions (patterns with literal terms, own values) must be roots.
2. **Carry the two spike 4 fixes:** the wide-node hash (I2.7) and the memo fence (I2.8). `spikes/parallel` still has
   the hash bug.
3. **Keep the API shape:** heap `&self`; definitions through `&mut` or the frontier only; any native or kernel that
   reads definitions, or calls back into evaluation, goes through a `CondCx`-like context so its reads are recorded.
4. **Keep the index invariants:** a single writer, publication by one release store after a rule is complete, ranks
   swapped as a whole array, retirement until a safe point.

The core files are `src/heap.rs`, `src/conc.rs`, `src/index.rs`, `src/eval.rs` and `src/tests.rs`. The crate depends
on `spikes/matcher` by path, to run spike 2 as the in-process baseline.
