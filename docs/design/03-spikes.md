# MathCAS v2: spike reports

Each spike measures one risky part of the design (plan section 5) against cases from the benchmark suite
([`02-benchmarks.md`](02-benchmarks.md)), and ends with decisions and what it rules out.

## Spike 1: term store

Code: [`spikes/termstore`](../../spikes/termstore). Raw results: `spikes/termstore/results/run1.md` (first
version) and `run2.md` (with immediates).

### Question

Hash-consed terms with normal-form stamps (semantics 1.1, 4.3, 4.4) against a v1-style reference-counted tree: is
hash-consing faster, smaller, or neither, and where does it lose?

### Setup

- **tree**: v1's representation (reference-counted nodes, symbols as shared strings compared by content, structural
  equality and hashing by traversal).
- **hc**: hash-consed arena. A term is a 32-bit id, each distinct term is stored once, its hash and a 64-bit symbol mask
  are cached in the node, and arguments live in a shared argument arena.
- **hc+imm**: as hc, with small integers (31 bits) encoded directly in the handle instead of interned.
- **+stamps**: memoised normal forms stamped with the definition epoch; a stamp stays valid unless a later
  redefinition touched a symbol in the term's mask.
- Every workload is written once against a common `Store` interface, so the stores run identical code. A
  store-independent fingerprint of every result is compared across stores: all results agree.
- Machine: AMD Ryzen 7 7700X, 31 GB RAM, Windows 11; Rust 1.99.1, release build. Median of 3 runs on a fresh store
  each; peak heap above baseline from a counting allocator (it includes the fingerprint's work buffers, so it
  overstates both stores equally).

### Results (largest size of each case)

Times in ms; memory is peak heap in MB.

| Case | Phase | tree | hc | hc+imm | hc+imm+stamps |
|---|---|---|---|---|---|
| R3 derivative, 3·10^5 nodes | build | 21.5 | 22.3 | 21.2 | |
| | derivative | 61.1 | 65.3 | 61.5 | |
| | memory | 62.6 | 43.0 | 43.0 | |
| R8 expand (a+b+c+d)^30 | expand | 94.8 | 62.4 | 59.3 | |
| R9 deep nesting, 10^6 | build | 59.1 | 90.1 | 89.1 | |
| | depth | 58.6 | 24.6 | 24.5 | |
| | replace | 119 | 151 | 152 | |
| | drop | 169 | 0 | 0 | |
| | memory | 185 | 122 | 122 | |
| R10 replace, 10^6 nodes | build | 61.5 | 60.1 | 59.1 | |
| | replace | 126 | 84.3 | 84.0 | |
| | memory | 143 | 47.2 | 47.2 | |
| R11 re-evaluation, 10^6 nodes | eval | 134 | 93.8 | 91.1 | 67.1 |
| | after unrelated change | 134 | 83.3 | 81.6 | **0.000** |
| | after a contained symbol changed | 135 | 83.4 | 82.4 | **20.2** |
| | memory | 249 | 28.8 | 28.8 | 34.7 |
| S3 parts, 1000×1000, 10^4 replacements | build | 6.1 | 58.0 | **2.7** | |
| | replacements | 30.6 | 52.7 | 44.6 | |
| | memory | 24.1 | 175 | **132** | |
| S6 transpose, 1000×1000 | build | 5.8 | 58.7 | **3.0** | |
| | transpose | 16.4 | 7.5 | **2.9** | |
| | memory | 72.2 | 56.3 | **12.3** | |
| S8 allocation, 10^7 items | build | 1319 | 6402 | **4476** | |
| | memory | 2617 | 2215 | 2047 | |
| S8 allocation, 10^6 items | build | 116 | 281 | 223 | |
| EQ two equal 10^6-node terms | 100 × `same` | 6783 | **0** | **0** | |
| | 100 × hash | 3822 | **0** | **0** | |
| | memory (one copy) | 175 | 23.6 | 23.6 | |

### Findings

1. **Equality and hashing become free.** Comparing two equal 10^6-node terms costs 68 ms in the tree and nothing with
   hash-consing; hashing likewise. Every workload that compares or looks up terms gains: polynomial expansion with
   like-term collection (R8) is 1.6 times faster, a rewrite pass (R10) 1.5 times.
2. **Normal-form stamps work as specified.** After a redefinition of a symbol the term does not contain,
   re-evaluating a 10^6-node term takes no measurable time (134 ms in the tree, which has to traverse everything).
   After a change to a symbol the term does contain, only the affected part is re-evaluated (20 ms). The first
   evaluation is also faster (67 ms against 134 ms), because shared subterms are evaluated once.
3. **Memory: 3 to 8.6 times smaller** where terms share structure (R10, R11, EQ), 1.3 to 1.5 times smaller on the
   other construction workloads (R3, R9, S8). The exception is S3, where the arena keeps garbage (finding 7). By a
   size estimate of the two layouts, a term like `f(i, g(i))` needs about 184 bytes in the tree and about 86 in the
   arena.
4. **Interning every small integer is a mistake.** Without immediates, building integer lists is 10 times slower than
   the tree (S3, S6). With 31-bit immediate integers it is 2 times *faster*, and transpose is 5.6 times faster with 6
   times less memory.
5. **Rebuilding wide nodes is slower.** Replacing one element of a 1000-element list rehashes all 1000 arguments to
   intern the new list, where the tree only copies them: the S3 replacements are 1.5 times slower (44.6 against 30.6
   ms). In-place updates of uniquely held nodes, which v1 can do and this generic harness cannot, would widen the gap.
6. **Creating huge numbers of fresh, distinct terms is slower.** Building 10^7 new terms (S8) takes 3.4 times as long
   as plain allocation, and 1.9 times at 10^6. Reserving the table in advance changes nothing (measured: 4.5 s against
   4.3 s), so the cost is not table growth: each new term costs a random access into a table that no longer fits in
   cache. Deep chains (R9 build) show the same effect, 1.5 times slower.
7. **Without collection, the arena keeps all garbage.** Repeatedly replacing parts of a matrix (S3) leaves every
   intermediate list in the arena: 132 MB against the tree's 24 MB. A long session would grow without bound.
8. **The tree's recursive drop is a hazard.** Dropping a 10^6-deep term takes 169 ms and needs a very large stack (the
   harness runs on a 1 GiB stack; v1 needs big-stack threads for the same reason). The arena has no recursive drop.

### Decisions

| # | Decision | From |
|---|---|---|
| 1.1 | Adopt hash-consed terms in an arena, with cached hash and symbol mask per node. | findings 1, 3 |
| 1.2 | Small integers (and later symbols) are immediates encoded in the handle; only larger values are interned. | finding 4 |
| 1.3 | Adopt normal-form stamps with memoisation as specified (semantics 4.3, 4.4); the symbol mask is enough for the validity check. | finding 2 |
| 1.4 | The arena needs a collector: a tracing collection at safe points between evaluations, with roots in definitions, session values and live handles. To be designed and measured in spike 1b. | finding 7 |
| 1.5 | Bulk data that does not need term identity (numeric arrays, large integer lists) uses the packed forms of semantics section 11 rather than one interned term per element; lists updated element by element are packed for the same reason. | findings 5, 6 |
| 1.6 | Recursive data structures in the implementation must never rely on recursive drop or recursive traversal of unbounded depth. | finding 8 |

### What it rules out

- Reference-counted trees as the primary representation: they lose on every comparison-heavy workload and on memory.
- Interning every atom, including small integers.
- Keeping the arena without a collector.

### Follow-ups

- **Spike 1b, arena collection:** mark and sweep from roots, with id reuse through a free list, against generational
  regions; measure pause times and throughput on S3, S8 and a long-running session trace.
- **Lazy interning for bulk construction:** measure whether interning on first comparison, or at normal form, recovers
  the S8 and S3 gaps without losing finding 1.
- **R13 and R14** (argument-head rules, read-set tracking) need the evaluator and the matcher; they move to spike 2.

## Spikes 1b to 4: summaries

The full reports, with setup, every table, findings and follow-ups, live next to the code. Final numbers come from
runs on a mostly idle machine (spikes 1b, 2 and 3 side by side on separate cores, all single-threaded; spike 4 alone,
sweeping 1 to 16 threads), every run under a memory cap and a timeout. Every variant in every spike computes identical
results, checked by fingerprint.

### Spike 1b: arena collection

Report: [`spikes/collect/REPORT.md`](../../spikes/collect/REPORT.md).

- Collection removes the garbage problem of spike 1: S3's peak heap 132 → 8.3 MB; a 1000-evaluation session
  1.87 GB → 184 to 480 MB.
- It also makes garbage-heavy work faster. On the long session at n = 5·10^4, the generational collector with a 1 MB
  nursery takes 6.2 s against 12.1 s without collection and 11.0 to 11.3 s for mark-sweep and compaction, with a
  mean pause of 0.28 ms against 33 to 37 ms.
- Costs: 21 to 29 % when nothing dies (S8 at 10^6); major pauses of about 20 ns per live node.
- An unrooted handle silently points at the wrong term once terms move, so rooting is enforced by types.

| # | Decision |
|---|---|
| 1b.1 | Generational collection: young region with its own interning table, promoted at a safe point once it holds about 1 MB; old region compacted when it has doubled. |
| 1b.2 | Precise, moving roots; handles branded with the lifetime of an evaluation's heap scope; only `Trace` containers hold handles across safe points; the only write barrier is on memo writes. |
| 1b.3 | Symbols are immediates; small integers are 30-bit immediates. |
| 1b.4 | Creation order is kept (no id reuse): stackless marking, renumbering by rank. |
| 1b.5 | The memo of normal forms is weak. |
| 1b.6 | Keep about twice the live size reserved after a collection. |
| 1b.7 | Collection only at safe points between evaluations; major collections may be deferred to idle time. |

### Spike 2: matcher

Report: [`spikes/matcher/REPORT.md`](../../spikes/matcher/REPORT.md).

- Naive AC matching is exponential in one place, sequence variables: 31 s against 125 ms for the pruning matcher at
  10^4 rules; a 10-term pigeonhole case takes 36 s against 2.5 ms with a Hall test.
- Indexed dispatch stays at 0.12 to 0.28 µs per call from 500 to 50 000 rules; linear dispatch reaches 176 µs.
- Argument-head rules are 3.7 times faster than putting every kind's rule on the generic function (indexed), 31 times
  faster than linear.
- Untracked normal-form stamps return a stale result after a global that a condition reads is redefined; 64-bit read
  masks fix it at no measurable cost (within ±5 %), at the price of about 1 % false invalidation.
- Ordering rules by specificity through the index: 41 ms for 50 000 rules, against 13.4 s for a scan.

| # | Decision |
|---|---|
| 2.1 | Backtracking matcher in continuation-passing style with the pruning AC strategy (class order, last sequence variable takes the rest, candidate sets, Hall test, first copy only). |
| 2.2 | The enumeration order of semantics 6.2 made precise (proposed text). |
| 2.3 | Under an associative head, `x_` matches exactly one argument (proposed for 6.2). |
| 2.4 | Rule sets are always indexed: discrimination trees, and key-set filters for AC heads. |
| 2.5 | Rule order maintained through generalisation queries on the index; pairs the sound but incomplete instance check cannot order stay in definition order (proposed for 6.3). |
| 2.6 | Semantics decision 13.2 (argument-head rules) settled as written. |
| 2.7 | Semantics decision 13.1 (read-set tracking) settled: 64-bit masks propagated through memo hits. |
| 2.8 | A condition inside a pattern sees only the variables bound inside its own subpattern (proposed for 6.1). |
| 2.9 | An AC rule with no indexable key is accepted with a diagnostic. |

### Spike 3: numbers

Report: [`spikes/numbers/REPORT.md`](../../spikes/numbers/REPORT.md).

- Own multiplication with a three-prime NTT: 137 ms at 10^7 digits, against 1.9 s for num-bigint (14 times) and
  14.8 s for v1 (108 times). Below about 30 000 digits own and num-bigint are within 15 %.
- pi to 10^6 digits in 1.0 s (num-bigint 3.5 s; v1 beyond the 60 s limit). Digits of e at offset 10^6 in 242 ms by
  modular extraction.
- Balls: no enclosure failure in 2.4 million exact checks; radius bookkeeping costs at most 21 % at 100 digits and
  nothing from 1000 digits. A machine-float radius is ruled out by range.
- Adaptive precision costs 1.6 to 3 times one evaluation at the final precision; an exact zero ends in
  `failure(precision_not_reached)`, never a wrong zero.

| # | Decision |
|---|---|
| 3.1 | Own big-integer layer on 64-bit limbs: schoolbook, Karatsuba from 24 limbs, NTT from about 1500 limbs. |
| 3.2 | Three primes c·2^32 + 1 in (2^61, 2^62), Harvey/Shoup butterflies, recursive transforms, Garner CRT. |
| 3.3 | Division, square root and radix conversion by Newton methods on top of multiplication (2 to 3 multiplications per division is required work). |
| 3.4 | Ball midpoint: sign, own mantissa, i64 exponent. Radius: 32-bit mantissa, i64 exponent, rounded up. |
| 3.5 | Division by a ball and square roots are undefined unless provably non-zero or positive; comparisons decided only for disjoint balls. |
| 3.6 | `n(e, d)` doubles from d·log2(10) + 16 bits up to a cap; a zero-containing ball at the cap is a failure. |
| 3.7 | Digits at an offset come from modular reduction of the binary-splitting fraction. |
| 3.8 | v1's big-number code is not reused (its test vectors and inline small integers are). |
| 3.9 | Long-running number loops poll the stop flag inside themselves. |

### Spike 4: parallel evaluation

Report: [`spikes/parallel/REPORT.md`](../../spikes/parallel/REPORT.md).

- All 475 configurations (3 stores, 6 granularity rules, 1 to 16 threads, 10 cases including random effect-heavy
  trees) give the same value, output order and definitions as sequential evaluation (semantics 18.1).
- One lock collapses (M8: 0.15 at 16 threads). Sharded locks flatten past 4 threads; lock-free lookups reach 6.3 to
  6.9 times at 16 threads, and at 10^8 terms are faster even on one thread (11.3 s against 16.4 s).
- At 16 threads: 4.8 to 7.8 times on independent heavy arguments (M6, M8, M10), 2.9 on a trivial map, about 2.2 on
  definition chains, none on effect-dense trees, which the effect-aware granularity rule keeps at 0.96 to 1.09 of
  sequential. Efficiency at 8 threads is 0.46 to 0.70.
- A speculative task must never block on an effect (it deadlocks a work-stealing pool): it leaves a hole, and the
  task on the sequential frontier fills holes in order.

| # | Decision |
|---|---|
| 4.1 | Concurrent store C: 128 shards of open-addressing arrays probed without locks, a lock only on a miss; segmented arena with per-thread id chunks. Next: compact slots to remove its 6 to 16 % single-thread cost. |
| 4.2 | Ids are canonical but schedule-dependent; nothing observable may depend on an id's value. |
| 4.3 | The structural hash is a polynomial combinable across a split; very wide nodes hash in parallel. |
| 4.4 | The lazy granularity rule (cost estimate from sampled size, learned head cost and measured cost; split only above 1024 fuel when the worker's deque is empty). Proposed amendment: pass half the hint to each half on a split. |
| 4.5 | Speculative effect ordering by holes, in-order filling and read-set validation. |
| 4.6 | Arguments containing effect-declared symbols get no weight in the granularity estimate. |
| 4.7 | SplitMix streams: element i of `ptable`/`pmap` uses `split(seed, i)`. |
| 4.8 | A lossy seqlock memo, stamped with the starting epoch and the read set. |
| 4.9 | Per-task fuel in blocks of 4096 from the shared budget; interrupts polled at the same points. |

### Open items from the spikes

- ~~Semantics edits proposed by spike 2~~: applied in semantics draft 4 (sections 6.1 to 6.3).
- **Interactions not yet measured together:** the collector with the concurrent store (per-thread young regions break
  the global child-before-parent order), the matcher on the concurrent store, and the number layer's payloads in the
  collected arena.
- **Missing benchmark cases:** M1, M2, M3, M5 and M9 need the numeric compiler and the parallel kernels; N3 to N5, N9,
  N10 and N12; R1 to R3 and R6 on the matcher.
