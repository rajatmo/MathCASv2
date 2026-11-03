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
