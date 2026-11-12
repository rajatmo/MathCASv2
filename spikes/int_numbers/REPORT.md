# I3: number payloads in the collected arena

Code: `spikes/int_numbers`. Results: `results/final-full.md` (`--full`, median of 3; a run over 1 s is not repeated),
`results/final-sweep.md` (threshold sweep and trigger ablation), `results/final-quick-stress.md` (stress correctness,
9 configurations), `results/final-quick.md`. **All numbers were taken on a busy machine** (three other jobs running,
single-threaded). Only conclusions that hold within about 1.5× noise are drawn.

## Question

Spike 3's numbers are own 64-bit-limb naturals (schoolbook, Karatsuba, NTT) and balls with an own-mantissa midpoint
and a 32-bit-mantissa radius. How do they live as terms in spike 1b's store, which is generational, moving, keeps
creation order, has a weak memo, and makes symbols and 30-bit integers immediate? What does it cost? The five
sub-questions are:
1. Where the payloads live.
2. Hash-consing numbers.
3. Immediates alongside big numbers.
4. Collection with payloads.
5. Exact and ball arithmetic through the store, compared with spike 3's numbers used bare.

## Setup

- **Store** (`src/store.rs`): spike 1b's generational scheme C. It has a 1 MB nursery, a young table, sliding promotion
  and major compaction on doubling. The memo and stamps are left out: numbers are their own normal forms with an empty
  symbol mask.
- **Node layout:** 32 bytes (kind 16, hash 8, mask 8), asserted by a test. The kinds are:
  - `App`;
  - `Word {neg, mag}`: a one-limb integer stored inline, with no payload;
  - `Rat {num, den}`: two integer terms;
  - `Arena {start, len}`: payload words in a limb arena that slides in the same ascending pass as the nodes and
    argument slots;
  - `Boxed {Box<Vec<u64>>}`: a thin pointer, owned by the node and freed during the sliding pass when the node dies.
- **Payload words:** an integer stores its limbs. A ball stores `[exp, rad.m, rad.e, mantissa limbs…]`, with the sign
  in the node.
- **Placements compared:** `arena` (all payloads in the arena), `boxed` (all boxed), `hybrid` (at most 64 words in the
  arena, larger boxed). The sweep adds hybrid at 16, 256 and 4096 words.
- **Hashing modes compared:**
  - eager: full 4-lane hash at construction, intern by value;
  - `hyb-sampled`: hash of length, end words and 8 sampled words, full compare on a match;
  - `hyb-lazy`: payloads above 64 words are not interned at all; they are compared by value inside the table and in
    `same`, and hashed on first use as a child.
- **Ablations:** `hyb-nogc` (no collection) and `boxed-nocount` (boxed bytes not counted towards the nursery trigger).
- **Bare reference:** spike 3's `Nat`/`Int`/`Ball` (copied unchanged apart from three slice helpers made public),
  plus two new modules:
  - `gcd.rs`: Lehmer's algorithm with 63-bit leading parts (spike 3 had no gcd);
  - `rat.rs`: canonical rationals with Knuth/Henrici sum and product.
- **Workloads** (`src/work.rs`): each is written twice with the same algorithm, once on terms with safe points and
  roots, once bare.
  - N5: product tree for 10^4!, 10^5! and 10^6!; the naive loop to 3·10^4!.
  - N10: H(10^4) and H(10^5), by binary splitting and by the loop.
  - EXP: (x + 2^100)^n for n = 100 and 400, plain (coefficients and the expanded sum are terms after every step) or
    packed (bare coefficients, one term at the end).
  - ZETA: sum of 1/k² with balls at 3400 bits, checked against π²/6 with decided ball comparisons.
  - PI/E: balls at 10^6 digits (computed once, then interned and read back; first 50 digits checked).
  - LS churn: 300 evaluations, 10^3–10^5 digits.
  - LS keep: 10^6-digit numbers kept, 5000 compound terms per evaluation.
  - LS big: payload-dominated; 52k-limb numbers in a ring of 8.
  - HASH hazard: 2000 numbers of 1000 limbs that differ only in one unsampled limb.
- **Correctness:**
  - Every configuration's fingerprint equals the bare one on every case and size.
  - `--stress` runs a minor collection at every safe point and a major every fourth. `Heap::verify` runs after each
    collection and checks: tables consistent, children before parents, arena payloads contiguous in id order, boxed
    bytes add up, and numbers canonical (a Word is never an immediate value, payload integers have at least 2 limbs
    with a nonzero top limb, rationals are reduced with denominator > 1).
  - `cargo test`: 20 unit tests plus 3 composition tests running every workload across 6 configurations under stress.
    All pass; no build or clippy warnings.
- **Bug found by structured input:** binary splitting of H(n) exposed a cofactor-sign bug in the Lehmer gcd that random
  operands had missed. It is fixed, and the H(4000) splitting is now a permanent test that checks every gcd against
  Euclid.

## Results (busy machine, ms; ratio against bare)

| Case | bare | arena | boxed | hybrid | hyb-sampled | hyb-lazy | hyb-nogc |
|---|---|---|---|---|---|---|---|
| N5 tree 10^4! | 1.64 | 0.65× | 0.64× | 0.68× | 0.63× | 0.59× | 0.58× |
| N5 tree 10^5! | 26.9 | 0.91× | 0.95× | 0.89× | 0.89× | 0.90× | 0.89× |
| N5 tree 10^6! | 538 | 1.04× | 1.14× | 1.04× | 1.04× | 1.03× | 1.01× |
| N5 seq 3·10^4! | 89.6 | 1.32× | 1.57× | 1.40× | 1.11× | 1.11× | 3.05× (717 MB) |
| N10 split H(10^5) | 158 | 1.23× | 1.24× | 1.23× | 1.23× | 1.24× | 1.26× |
| N10 seq H(10^5) | 2395 | 1.06× | 1.11× | 1.12× | 1.07× | 1.10× | 1.22× (1009 MB) |
| EXP plain n=400 | 45.2 | 2.02× | 1.68× | 1.63× | 1.46× | 1.57× | 2.83× |
| EXP packed n=400 | 45.3 | 1.01× | 1.00× | 1.00× | 1.00× | 1.00× | 0.99× |
| ZETA balls n=2·10^4 | 18.7 | 1.35× | 1.47× | 1.37× | 1.36× | 1.36× | 1.74× |
| PI/E 10^6 digits: intern+GC / read back / re-intern | compute 935 | 0.078 / 0.020 / 0.030 | 0.045 / 0.016 / 0.030 | 0.043 / 0.015 / 0.028 | 0.017 / 0.015 / 0.015 | 0.016 / 0.015 / 0.008 | |
| LS churn | 117 | 1.01× | 1.01× | 1.01× | 1.00× | 1.00× | 1.04× |
| LS big | 17.4 | 1.86× | 1.45× | 1.51× | 1.16× | 1.16× | 2.05× |
| HASH hazard | 0.46 (bare build) | 4.44 ms | 1.20 ms | 1.30 ms | **161 ms** | 0.56 ms | 1.04 ms |

**Collection with payloads.** Reserved = store reservation after the last collection; peak = peak heap above baseline.

| Case | arena | boxed | hybrid |
|---|---|---|---|
| LS big: moved MB | 143 | 0 | 0 |
| LS big: pause max / mean (ms) | 0.168 / 0.034 | 0.081 / 0.019 | 0.079 / 0.019 |
| LS big: reserved / peak (MB) | 13.7 / 14.2 | 7.1 / 8.8 | 7.1 / 8.8 |
| EXP plain 400: moved MB | 124 | 0 | 2.2 |
| N5 tree 10^6!: pause max (ms) | 11.1 | 24.6 (483k boxes freed) | 11.1 |
| N10 seq H(10^5): pause max (ms) | 0.10 | 2.80 | 1.94 |
| LS keep 10^6-digit: pause max (ms) | 20.7 | 19.2 | 18.3 (node work dominates) |
| N5 seq 3·10^4!: reserved (MB) | 8.65 | 2.09 | 2.22 |

**Trigger ablation** (`boxed-nocount`, peak heap compared with `boxed`):
- N5 seq 3·10^4!: 717 MB against 7.2 MB.
- N10 seq H(10^5): 191 MB against 7.0 MB.
- LS big: 106 MB against 8.8 MB.
- EXP plain: 56 MB against 7.8 MB.

**Hybrid threshold sweep** (`final-sweep.md`): 16, 64 and 256 words are within noise on every case. 4096 behaves like
`arena`: EXP plain 2.00× against 1.63×.

Hashing rate: about 0.29 ns per word (eager N5 seq: 89 M words hashed, 126 ms against 100 ms with sampled hashing).

## Findings

1. **The store adds little where the arithmetic is super-linear.**
   - Product trees and binary splitting cost 0.89–1.04× bare at 10^5–10^6 digits. 10^4! is 1.5× *faster* than bare,
     because immediates and Word nodes make the bottom levels allocation-free.
   - H(10^5) by splitting costs 1.23× and by the loop 1.06–1.12×.
   - Interning a 10^6-digit ball costs 0.04–0.08 ms against 935 ms to compute it.
2. **The overhead is per operation and per limb, from three sources:**
   - copying operands out of the store into bare values: about 0.3 µs per operation for small rationals and balls
     (ZETA 1.35×, H split at the leaves);
   - hashing: about 0.3 ns per word;
   - placing the result: an arena copy or a box allocation.

   This matters only in O(n) loops such as r = r·k (N5 seq 1.3–1.6×), where in-place update of a uniquely held bare
   value is impossible for immutable terms. Plain-term polynomial expansion with big coefficients costs 1.6×. Packed
   (bare kernel, one term at the end) costs 1.0×.
3. **Placement.** Arena-only copies every surviving mid-size or large payload on each promotion and major collection:
   124 MB moved in EXP plain (2.0× against hybrid's 1.63×), 143 MB in LS big (1.86× against 1.51×), and about 2× the
   reserved memory (slack of 2× live plus 4 MB, per 1b.6). Boxed-only pays an allocation per small number and a free
   per dead one in the sliding pass: pause max 24.6 ms against 11 ms on N5 tree 10^6, 2.8 ms against 0.1 ms on N10
   seq, and slower small-number churn. Payload copying itself is cheap (memcpy rate), so pauses stay dominated by node
   work, as in 1b.
4. **The trigger must count payload bytes.** If it counts nodes alone, a few nodes can own hundreds of MB: 717 MB
   against 7 MB. Creation order means early long-lived payloads never move (the copy is skipped when a payload is
   already in place).
5. **Hashing on construction is affordable**, at most 21% (N5 seq) and nothing measurable on multiplication-bound
   work. Sampled hashing is **125× slower than full hashing on structured equal-length inputs** (161 ms against 1.3 ms,
   quadratic bucket chains). Lazy non-interning saves the same at most 21% but:
   - breaks handle identity: `===` and every compound lookup must compare big numbers by value;
   - duplicates payloads: H(10^5) by the loop has 1114 collections and 194k boxes, against 727 and 107k;
   - gives no saving when big numbers become children (rationals), which hash them anyway.
6. **Immediates and canonicity compose cleanly.** Every integer result goes through one canonicalising constructor,
   which trims, then demotes to an immediate (30 bits) or a Word (one limb), else makes a payload. Tests check
   promotion on overflow, demotion (2^300 − 2^300 + 7 is the immediate 7), equal big numbers built two ways being one
   term, and f(N) over equal N being one term. `verify` checks canonical forms after every stress collection.

## Decisions

| # | Decision | From |
|---|---|---|
| I3.1 | Three integer tiers, chosen only by a single canonicalising constructor: 30-bit immediate; `Word` node (one limb inline, no payload); payload node (≥ 2 limbs, top limb nonzero). Every arithmetic result passes through it, so a value that fits is always the smaller tier. | findings 1, 6 |
| I3.2 | Hybrid payload placement: payloads of at most 64 words in a limb arena that slides with the nodes; larger payloads in a thin `Box<Vec<u64>>` owned by the node and freed in the sliding pass. Nodes stay 32 bytes. Any threshold from 16 to 256 words is equivalent. | findings 3, 4; sweep |
| I3.3 | Payload bytes (arena and boxed) count towards the nursery and major triggers and towards the live and promoted sizes. | finding 4 |
| I3.4 | Payloads are hashed in full at construction (4-lane, about 0.3 ns/word) and interned by value. Handle equality stays the only identity (1.1, 1b). No sampled hashing, no uninterned big numbers. | finding 5 |
| I3.5 | A rational is a `Rat` node over two canonical integer terms (immediates when small, so small rationals have no payload). Sum and product follow Knuth/Henrici, with own Lehmer gcd. A ball is one payload node `[exp, rad.m, rad.e, mantissa…]` with the sign in the node; midpoint and radius are not separate terms. This was decided by argument, not measured: no sharing to gain, and it saves a node and a lookup per ball. | setup, finding 2 |
| I3.6 | Number kernels (product trees, binary splitting, series, polynomial arithmetic) compute on bare values and intern only results. Plain per-step terms are for small or symbolic work. This matches semantics 11 packing. | findings 1, 2 |
| I3.7 | Collection needs no payload-specific rules beyond I3.2 and I3.3: the stackless descending mark treats Rat as two children and payloads as leaves, the slide moves arena words or frees dead boxes, and pauses remain node-dominated. Large payloads at the bottom of the arena never move. | findings 3, 4 |

## What it rules out

- Sampled or partial hashing of payloads: a quadratic hazard on structured input (125×).
- Not interning large numbers (value comparison in the table and in `===`): it breaks O(1) identity, duplicates
  payloads and saves at most 21%.
- All payloads in the arena: copies large survivors, about 2× the reserved memory, up to 1.24× slower than hybrid.
- All payloads boxed: allocation and free per small number, larger pauses (2.2–28×).
- An allocation trigger that counts nodes only (100× peak memory).
- Per-step interning inside big-number kernels.

## Follow-ups

- **Borrowed payload views for rational and ball arithmetic** (integers already borrow slices): removes the about
  0.3 µs per operation copy-out (ZETA 1.35×, H split 1.23×). Also a word-size rational fast path.
- **Subquadratic gcd** (half-gcd) for rationals beyond about 10^5 digits. Lehmer is quadratic; it is fine at H(10^5).
- **Limb-arena slack:** the 2×-live-plus-4 MB shrink policy of 1b.6 also applies to the limb arena. A smaller slack
  for limbs may suffice.
- **In-place update of uniquely held numbers** cannot exist for immutable terms. If O(n) loops (r = r·k) matter, give
  the evaluator a bare accumulator for numeric loops, as the numeric compiler would.
- **Parallel hashing:** the 4-lane hash is not spike 4's split-combinable polynomial hash (4.3). Hashing 10^6 digits
  takes about 16 µs, so this matters only if parallel hashing of 10^7-limb payloads is ever needed.

## What the final assembly needs from I3

1. **Node kinds and layout.** Take `Kind`/`Node` from `src/store.rs` (`App`, `Word`, `Rat`, `Arena`, `Boxed`; 32
   bytes), the payload word format (integer limbs; ball header plus mantissa), and the `Arrays` split borrow used by
   table lookups and `verify`.
2. **Collector extensions to spike 1b's compaction:**
   - `Rat` children are marked;
   - arena payload words slide with `copy_within` (skipped when in place), and the young region also tracks
     `young_limbs_lo`;
   - dead `Boxed` kinds are dropped in the pass;
   - trigger counters include payload bytes;
   - the extended `verify` invariants.
3. **One canonicalising entry point** (`int_parts` / `from_rat` / `ball`) for every number the evaluator or kernels
   produce. The matcher and canonical order (semantics 8) compare numbers by handle for identity and read values
   through `view()` slices for ordering.
4. **For the concurrent store (spike 4):**
   - the limb arena must be per region/thread chunk, like the argument arena, with the same child-before-parent order
     within a region;
   - boxed payloads are thread-agnostic;
   - interning a payload hashes it before the lock-free probe; equal-hash compares read the words, so payloads must be
     immutable and published before the handle.
5. **Shared code:** `gcd.rs`, `rat.rs` and the fingerprint functions in `lib.rs::fp`, so that store results can be
   checked against bare kernels.
