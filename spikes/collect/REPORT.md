# Spike 1b: arena collection

Code: `spikes/collect`. Final results: `spikes/collect/results/final-{stress,s3,s8,r10,r11,long}.md` (mostly idle
machine). Development results from a busy machine: `results/prelim-*.md`.

## Question

Spike 1 adopted a hash-consed arena with immediate integers and normal-form stamps (decisions 1.1 to 1.3) and found
that it keeps all garbage (finding 7: 132 MB against a tree's 24 MB on S3). Decision 1.4 requires a collector at safe
points between evaluations, with roots in definitions, session values and live handles. Which collection scheme should
v2 use, and what root and handle discipline does it impose on the evaluator?

## Setup

- **hc+imm+stamps**: spike 1's store, copied unchanged (`src/hc.rs`), never collects. Reference for the layout change
  below.
- **none**: the collecting store (`src/gc.rs`) with collection off. Same layout as spike 1 except that **symbols are
  immediates** too (tag bit in the handle; small integers shrink from 31 to 30 bits): every collector below moves or
  frees nodes, and symbol handles are held everywhere (rule tables, workload structs), so they must not live in the
  arena. The two stores run within 7 % of each other on every case at the largest size (final run).
- **A, non-moving mark-sweep:** mark from the roots with an explicit stack, free dead ids onto a free list for reuse,
  remove them from the interning table, clear their memo entries. Dead argument slots are reclaimed by compacting the
  argument arena alone once dead slots outnumber live ones (nodes keep their ids, only their `start` changes).
- **A-fl:** as A, with free lists of argument ranges keyed by exact length instead of compaction.
- **B, sliding compaction:** terms are immutable and a child is always interned before its parent, so a child's id is
  smaller than its parent's, and sliding keeps that order. Hence marking is one descending pass with no stack, the
  new id of a node is its rank in the mark bitmap (prefix count per 64-bit word plus a popcount; no forwarding
  table), the argument arena slides in the same ascending pass, and the interning table is renumbered in place (hashes
  are stored in the nodes and do not change).
- **C, generational:** nodes allocated since the last minor collection form a young region at the tail of the arena,
  with their own interning table (lookups probe the old table, then the young one). A minor collection is B applied to
  the tail only: survivors slide down and are inserted into the old table (promotion), the young table is cleared
  wholesale. Old nodes cannot point to young ones (immutability), so the only old-to-young references are roots and
  memo entries; memo writes of that kind are recorded (a write barrier on `memo_put`, none on term construction). The
  old region is compacted as in B when it has doubled since the last major collection. Variants by nursery size:
  **C** (minor once the young region holds 8 MB), **C1** (1 MB), **C0** (at every safe point).
- **Trigger** (A, B, C's major): at a safe point, once the bytes allocated (or promoted) since the last collection
  exceed both the live bytes after it and 4 MB, i.e. when the heap has doubled. After a collection, arrays are shrunk
  only beyond twice the live size plus 4 MB (finding 6).
- **Interactions.** The memo is weak: an entry survives a collection only if its key and its value survive (B and C
  remap both; A clears entries of freed or dead ids, which matters because A reuses ids). Stamps, the epoch and the
  change log are untouched: a moved node keeps its mask, so a stamp means the same after the move. Immediates are
  never marked, moved or freed. Every collection loop is bounded by the arena size.
- **Workloads** (`src/work.rs`), each a sequence of evaluations with a safe point after each, at which the workload
  passes every handle it still needs as a root and continues with the handles written back; it holds no other handle
  across a safe point:
  - **S3** parts: 1000 × n/1000 integer matrix, 10^4 element replacements, safe point after each (root: the matrix).
  - **S8** alloc: n distinct `f(i, g(i))`, built in 100 evaluations of n/100 each, kept as one list per evaluation in
    the session; nothing ever dies, so every collection is pure overhead.
  - **R10** replace: spike 1's rewrite pass, with a forced collection after the build (tree live) and after the pass
    (tree dead, result live). "Forced" is a full collection for A and B and a minor one for C, whose major collection
    follows its own trigger.
  - **R11** re-evaluation with stamps: spike 1's case with a forced collection before each re-evaluation; the memo
    entries of the live term must survive it (checked: the re-evaluation after an unrelated change is one memo hit,
    0 misses, in every store).
  - **LS** long session: 1000 evaluations; each builds a random temporary expression of n nodes over the R11 rules
    plus five symbols with own values, evaluates it with the memo, and keeps a small result `out(k, piece of the normal
    form, result k/2)` in a session table (results share structure and point backwards; about 2500 nodes each at
    n = 5·10^4). Every 50th evaluation redefines one of the five symbols, making its old value garbage and invalidating
    dependent memo entries through the stamps.
- **Correctness:** a store-independent fingerprint (spike 1's) of everything each workload keeps is compared across all
  eight stores; all agree on every case and size. LS also checks every kept back pointer by identity. `--stress`
  collects at every safe point (C: a minor collection every time and a major one every fourth) and runs
  `GcStore::verify` after each collection: every stored node is in exactly one interning table and is found again by
  its content, no live node or memo entry refers to a freed node. A freed node panics when viewed, so a handle that A
  freed would crash rather than give a wrong answer. All stress runs agree (release `--quick --stress`, and a debug
  build, which also checks the child-before-parent order on every marked edge).
- **Metrics:** median of the runs on a fresh store each; pause = wall time of one collection; peak heap above
  baseline from a counting allocator (as spike 1); "reserved after GC" = capacities of the store's arrays and tables
  after each collection; overhead = total time against `none`.
- Machine: AMD Ryzen 7 7700X, 31 GB RAM, Windows 11; Rust 1.99.1, release build.

## Results

Final run: mostly idle machine (spikes 2 and 3 ran on other cores, all single-threaded), `--full --runs 3 --trace`
per workload, median of 3 runs on a fresh store each (`results/final-{s3,s8,r10,r11,long}.md`). Correctness:
`--quick --stress --runs 1` (`results/final-stress.md`) and every full run agree across all eight stores. Times in
ms, memory in MB. Pause: maximum and mean over all collections of the run.

| Case | | none | A | A-fl | B | C | C1 | C0 |
|---|---|---|---|---|---|---|---|---|
| **S3** parts 10^6, 10^4 replacements | total | 51.0 | 55.8 | **35.9** | 46.3 | 40.6 | 46.0 | 71.5 |
| | overhead | | +9 % | **−30 %** | −9 % | −20 % | −10 % | +40 % |
| | collections | 0 | 20 | 20 | 20 | 15 | 86 | 10021 |
| | pause max / mean | | 1.24 / 1.04 | 0.63 / 0.44 | 1.14 / 0.98 | 1.22 / 0.80 | 1.13 / 0.23 | 1.54 / 0.004 |
| | peak heap | 132 | 28.7 | 8.4 | **8.3** | 16.6 | 16.6 | 9.8 |
| **S3** parts 10^5 | overhead | | −17 % | −33 % | −31 % | −3 % | −31 % | +25 % |
| **S8** alloc 10^7, all live | total | 4417 | 4655 | 4355 | 5281 | 5067 | **3876** | **3847** |
| | overhead | | +5 % | −1 % | +20 % | +15 % | **−12 %** | **−13 %** |
| | pause total | | 141 | 141 | 528 | 2056 | 1654 | 1636 |
| | pause max / mean | | 72 / 20 | 71 / 20 | 288 / 75 | 421 / 37 | 416 / 15 | 424 / 15 |
| | peak heap | 1514 | 1514 | 1514 | 1514 | 1517 | 1516 | 1516 |
| **S8** alloc 10^6, all live | total | 234 | 251 | 247 | 289 | 302 | 286 | 282 |
| | overhead | | +7 % | +6 % | +24 % | +29 % | +22 % | +21 % |
| **R10** replace 10^6 | collection, tree live | | 5.4 | 5.9 | 8.5 | 21.3 | 21.6 | 21.2 |
| | collection, tree dead | | 11.6 | 13.1 | 18.4 | 19.0 | 19.0 | 18.1 |
| | total overhead | | +3 % | +10 % | +14 % | +18 % | +18 % | +15 % |
| **R11** re-eval 10^6 | collection after eval | | 8.4 | 9.1 | 12.2 | 34.2 | 34.8 | 34.5 |
| | misses after unrelated change | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| **LS** session, n = 10^4 | total | 1999 | 1374 | 1396 | 1391 | 1168 | 1113 | **1078** |
| | overhead | | −31 % | −30 % | −30 % | −42 % | −44 % | **−46 %** |
| | pause max / mean | | 19.0 / 7.0 | 19.2 / 7.1 | 18.4 / 7.4 | 12.4 / 1.9 | 15.8 / 0.31 | 10.3 / 0.05 |
| | peak heap | 461 | 79.7 | 72.9 | 68.1 | 67.8 | 52.8 | 52.6 |
| **LS** session, n = 5·10^4 | total | 12149 | 10988 | 12001 | 11276 | 7968 | **6238** | **5958** |
| | overhead | | −10 % | −1 % | −7 % | −34 % | **−49 %** | **−51 %** |
| | pause max / mean | | 114 / 33 | 141 / 32 | 131 / 37 | 65 / 2.2 | 48 / 0.28 | 47 / 0.28 |
| | peak heap | 1869 | 480 | 452 | 326 | 184 | 212 | 212 |
| | memo misses (10^6) | 19.7 | 21.8 | 21.8 | 21.8 | 22.6 | 24.0 | 24.0 |

The spike 1 store (`hc+imm+stamps`) runs within 7 % of `none` on every case at the largest size (S3 −1 %, S8 +3 %,
R10 −7 %, R11 0 %, LS 0 %), so making symbols immediates costs nothing measurable.

The R10 and R11 collections of C are a minor collection promoting the whole 10^6-node young region, plus (R11 and the
first R10 one) the major collection that the doubled old region triggers. With a 10^6-node young region the nursery
size makes no difference (C, C1, C0 within 3 %).

Memory reserved after each collection, LS n = 5·10^4 (every 20th collection, MB). It follows the live set, which
grows by about 2500 nodes per kept result (2.5·10^6 live nodes after 1000 evaluations):

| | |
|---|---|
| A | 15.9 16.6 19.9 32.5 36.1 60.8 104.5 117.2 132.6 230.3 261.1 406.7 |
| A-fl | 14.3 14.1 16.9 30.2 33.7 60.3 111.3 120.8 151.7 242.1 269.8 444.9 |
| B | 10.0 15.1 17.9 33.0 35.7 65.8 79.6 131.8 159.5 264.1 285.6 318.1 |
| C1 | 3.3 14.7 29.2 31.5 52.4 58.0 62.6 104.6 115.8 125.0 208.8 208.8 |

Cost per node of one collection, from the forced collections of 10^6 nodes: A's sweep about 5 ns per arena node, B's
compaction about 8 ns, C's promotion about 12 to 17 ns per survivor (an insertion into the old table at a random
position), C's major collection and B's full one about 20 to 22 ns per live node at 10^7 (B: 288 ms with 1.28·10^7
live nodes).

## Findings

1. **Collection removes the garbage problem.** On S3 the peak heap drops from 132 MB to 8.3 MB (B, A-fl) and the live
   set after each collection is the matrix alone (1001 nodes). On the long session it drops from 1.9 GB to 184 to 480
   MB, and the memory reserved after each collection tracks the live set.
2. **Collecting does not slow down garbage-heavy workloads, and the generational scheme makes them much faster.** On
   the long session every collector is at least as fast as `none`. At n = 10^4: A, A-fl and B −30 %, C variants −42 to
   −46 %. At n = 5·10^4: A −10 %, B −7 %, A-fl −1 %, C variants −34 to −51 %. On S3 all but A and C0 beat it. The arena
   without collection keeps touching fresh memory and a table that grows without bound; a collected arena reuses
   memory that is still in cache and keeps the interning table small.
3. **The generational scheme wins clearly where evaluations make garbage.** On the long session at n = 5·10^4, C1
   and C0 take 6.0 to 6.2 s against 11.0 s for A and 11.3 s for B, with a mean pause of 0.28 ms against 33 to 37 ms.
   Two effects add up: a minor collection costs only the survivors of the last evaluations (most of a temporary dies
   young), and new terms are interned in a small young table. The second effect shows even where nothing dies: on S8
   at 10^7, C1 spends 1.65 s in collections and still finishes 12 % faster than `none`. With an 8 MB nursery (C) the
   same workload is 15 % slower, probably because a young table of that size no longer stays in the faster caches (not
   measured).
4. **Its costs.** When everything survives and the table fits in cache anyway (S8 at 10^6), the generational scheme
   is 21 to 29 % slower than no collection, against 6 to 7 % for A and 24 % for B. Every term is copied once by
   promotion and again by the major collections of the doubling old region, and promoting a survivor costs about twice
   as much as compacting it (an insertion into the old table). Collecting at every safe point regardless of size (C0)
   is wrong for small evaluations. On S3 each step's result survives one minor collection and then dies in the old
   region, so C0 makes 10^4 tiny collections and is 40 % slower than no collection. With a nursery, S3 is 20 % faster
   (C, 8 MB) at 10^6. At 10^5, C is only 3 % faster against 31 % for C1. A nursery of about 1 MB (C1) is the best or
   close to it on every case. 8 MB is worse on S3 at 10^5, on S8 at 10^7 and on the long session (−34 % against
   −49 %).
5. **Non-moving mark-sweep has no advantage that matters.** It needs no handle updates, and its sweep is the cheapest
   collection per node, which makes it the cheapest scheme when nothing dies (S8). But it loses locality: on the long
   session it is about as fast as B (−10 % against −7 % at n = 5·10^4) and holds 1.5 times its memory; the argument arena must still be compacted (A) or it
   fragments (A-fl keeps 445 MB reserved against B's 318); and id reuse breaks the child-before-parent order that
   makes B's and C's marking stackless and their renumbering free. Length-keyed free lists shine only when the same
   lengths recur (S3: −30 % at 10^6, the fastest variant there), and A-fl is the slowest collector on the long session
   (−1 %).
6. **Shrinking after a collection must leave room to regrow.** Shrinking arrays to 1.5 times their live size made B
   12 % slower than no collection on S3 at 10^5 (development measurement; the final run has only the retained policy);
   keeping twice the live size plus the 4 MB trigger slack made it 31 % faster. The difference is the page faults of regrowing into fresh memory.
7. **The memo survives collection where its entries are live; a weak memo loses some useful entries.** R11's
   re-evaluation after an unrelated change is a single memo hit in every store, after a collection that moved every
   node (B, C). On the long session the weak memo costs 11 % (A, B) to 22 % (C0) more evaluation misses than keeping
   every entry, because normal forms that only the memo references (for example the value of a definition's
   right-hand side) are dropped and recomputed. The collected stores are faster all the same.
8. **Major pauses grow with the live heap.** Minor pauses stay below 1 ms on average, but a major collection (or any
   full collection of A or B) takes about 20 ns per live node: 47 ms with 2.5·10^6 live nodes (C1, long session), 420 ms with 2·10^7 (C, S8),
   288 ms for B's full collection with 1.28·10^7.
   Collections do not poll for interrupts (they are bounded by the arena size), so this is the latency added after an
   evaluation in a session with a very large live heap.
9. **Moving requires that every handle crossing a safe point is a registered root, and nothing checks it at run
   time.** The driver meets this by construction (workloads hand over their roots explicitly). A forgotten handle
   crashes under A (freed node) but silently refers to a different term under B and C. The discipline has to be
   enforced by types, not by care (next section).

## Recommended root and handle API

Collection happens only at safe points between evaluations (decision 1.4), so evaluator and built-in code need no
rooting, no barriers and no handle scopes on the hot path: within one evaluation a term handle is a plain `Copy` 32-bit
value. What must be enforced is that no such handle survives into a safe point unless it is in a root. The borrow
checker does this at no run-time cost when handles are branded with the lifetime of the evaluation's access to the
heap:

```rust
/// A term handle, valid while the evaluation scope 'h that produced it lives. Copy, 32 bits, no rooting.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct T<'h> { raw: Raw, _scope: PhantomData<&'h ()> }

/// An unbranded handle: only root containers, which the collector visits and rewrites, may hold one.
#[derive(Clone, Copy)]
pub struct Raw(u32);

/// Holders of terms that outlive an evaluation: definitions and own values, the session history, attached data,
/// caches of compiled matchers. At a safe point the collector calls `trace` and may rewrite every handle.
pub trait Trace {
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw));
}

/// One evaluation's access to the heap. It borrows the heap mutably (a parallel version borrows it shared), so while
/// it lives no safe point can run, and every T<'h> it hands out is dead once it is dropped.
pub struct Scope<'h> { heap: &'h mut Heap, defs: &'h mut Defs }

impl<'h> Scope<'h> {
    pub fn app(&mut self, head: Sym, args: &[T<'h>]) -> T<'h> { ... }
    pub fn view(&self, t: T<'h>) -> View<'_, T<'h>> { ... }
    /// Rule right-hand sides and own values come out branded for this scope.
    pub fn own_value(&self, s: Sym) -> Option<T<'h>> { ... }
    /// A definition made during evaluation (an effect) goes into a root container at once.
    pub fn set_own_value(&mut self, s: Sym, t: T<'h>) { ... }
    /// Keep a term beyond this evaluation, e.g. for the front end: a slot in the heap's pin table, which is a root.
    pub fn pin(&mut self, t: T<'h>) -> Pin { ... }
    pub fn get(&self, p: &Pin) -> T<'h> { ... }
}

impl Session {
    pub fn evaluate(&mut self, input: &Pin) -> Pin {
        let out = {
            let mut cx = Scope::new(&mut self.heap, &mut self.defs);
            let t = cx.get(input);
            let r = eval(&mut cx, t); // the whole evaluator works on T<'_>: plain integers
            cx.pin(r)
        }; // every T<'_> is dead here; keeping one is a compile error
        self.history.push(out.clone());
        // The only place a collection can happen. Roots: the pin table (inside the heap), the definitions and the
        // history. The memo and the interning tables are the heap's own business (weak memo, remembered set).
        self.heap.safe_point(&mut [&mut self.defs, &mut self.history]);
        out
    }
}
```

Properties: zero cost inside an evaluation; the roots are a few long-lived structures (definitions, history, pins),
so a minor collection scans them quickly; symbols are immediates and never move, so rule tables, the matcher's
discrimination trees and built-in code hold `Sym` freely; a parallel evaluation (spike 4) uses shared `&'h Heap`
scopes on several threads while the safe point still needs `&mut Heap`, so the same compile-time guarantee covers it.
The spike's `Store::safe_point(roots, force)` is the untyped core of this API: the slices it receives are what
`Trace` visits.

## Decisions

| # | Decision | From |
|---|---|---|
| 1b.1 | Adopt the generational scheme C: a young region at the tail of the arena with its own interning table, promoted by sliding compaction at a safe point once it holds about 1 MB (tunable; 8 MB measured worse on S3, S8 and the session); the old region is compacted by sliding compaction when it has doubled since the last major collection. | findings 2, 3, 4 |
| 1b.2 | Collections move terms, so roots are precise and updated in place. Handles inside an evaluation are raw and branded with the lifetime of the evaluation's heap scope; only root containers implementing `Trace` (definitions, session history, pins, attached caches) hold handles across safe points. No barrier on term construction; the only write barrier is on memo writes from old keys to young values. | findings 5, 9; API above |
| 1b.3 | Symbols are immediates (decision 1.2 made concrete): permanent, never moved or collected. Small integers are 30-bit immediates. | setup; finding 9 |
| 1b.4 | The arena keeps creation order (no id reuse), so a child's id is always below its parent's: marking is one stackless descending pass and new ids are ranks in the mark bitmap. | finding 5 |
| 1b.5 | The memo is weak (an entry lives only while its key and value live); stamps, masks and immediates are unaffected by collection. | finding 7 |
| 1b.6 | After a collection, keep about twice the live size plus the trigger slack reserved; shrink only beyond that. | finding 6 |
| 1b.7 | Collection runs only at safe points between evaluations and is bounded by the arena size; it does not poll for interrupts. Major collections may be deferred to idle time between evaluations; incremental major collection waits until a live heap of 10^7 terms is a real use case. | finding 8 |

## What it rules out

- An arena without collection (finding 1; it is also slower on garbage-heavy workloads, finding 2).
- Non-moving mark-sweep with free-list reuse as the main scheme: it fragments, needs argument compaction anyway,
  breaks the child-before-parent order, and loses to compaction on the session workload (finding 5).
- Collecting at every safe point regardless of how much was allocated (C0 on S3, finding 4).
- Conservative or ad-hoc rooting of handles held by evaluator code: a moving collector needs the type discipline of
  1b.2 (finding 9).

## Follow-ups

- **Ephemeron memo:** keep the value of a memo entry alive while its key lives (a fixpoint over memo entries during
  marking), and measure whether the 11 to 22 % extra misses of the weak memo are worth the memory (finding 7).
- **All-survivor workloads:** adaptive nursery size, or pretenuring (allocate in the old region while recent minor
  collections promote almost everything), to recover S8's 21 to 29 % at 10^6 (finding 4).
- **Spike 4 (parallel evaluation):** concurrent interning with per-thread young regions breaks the global
  child-before-parent order across regions; check whether marking per region, with a stack only for cross-region
  references, keeps minor collections cheap. The safe point is a global barrier, which the end of an evaluation
  provides.
- **Payloads outside the arena** (big integers, packed arrays, strings): dead nodes that own memory must be finalised
  during the sliding pass, which already visits every dead node; measure with spike 3's representations.
- **Root containers with many handles** (large rule tables) are scanned in full by every minor collection; if that
  shows up, track per container whether it holds young handles.
- **Memory of the memo:** the memo array costs 8 bytes per node even for terms never evaluated; a separate young memo
  or a sparse table may pay off.
