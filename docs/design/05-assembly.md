# MathCAS v2: assembly of the core (milestone M1)

The spikes and the integration spike are done (`03-spikes.md`). This document settles what they left open and fixes the
crate layout and the interface contract the core is built against. The goal of milestone M1 is a core that runs the
benchmark suite: every case completes or stops cleanly at a limit. Speed comes after M1, behind the contract.

## 1. Decisions on the open points

| # | Open point | Decision | Reason |
|---|---|---|---|
| A.1 | Memo layout: I1 indexes the memo by structural hash, I2 makes it dense by term id. | **Dense by term id, per region.** The young memo is segmented in parallel with the young arena; the old memo is an array parallel to the old nodes. At a minor collection the young memo is swept over the survivors (it is visited anyway) and each surviving entry is written at its key's new id; at a major collection the old memo slides with the nodes. Entries with an old key and a young value go to a small side table swept at every minor collection (the only remembered set). | I2: the dense memo is about 2 times faster sequentially. I1's reason for hashing (renumbering moves entries) is met by moving entries during passes that already visit every survivor. |
| A.2 | Rule indexes keyed by term ids, which a moving collector renumbers. | **Index edges are keyed by symbol (an immediate, never moves) or by a literal term's structural hash, verified by handle on a hit.** The rule tables are a `Trace` root container; the collector rewrites the handles they hold, and the keys stay valid because structural hashes do not change when a term moves. | No re-keying pass; the structural hash is already stored in every node. |
| A.3 | Number payloads of young terms that die (I1 never visits dead young nodes). | **Each thread keeps a list of the young nodes it created that own boxed payloads.** A minor collection walks the lists and frees the payloads of unmarked nodes. Dead old payloads are freed during the major slide (I3). | Cost proportional to the number of boxed payloads, not to the young region. |
| A.4 | Limb arena placement in the concurrent store. | **Per-thread limb chunks**, like argument slots (I1's segmented arena), for payloads of at most 64 words (I3.2); larger payloads are boxed. | Consistent with I1; I3's sweep shows any threshold from 16 to 256 words is equivalent. |
| A.5 | Fuel consumed by speculative tasks. | **Only the frontier decides a limit failure.** A speculative task that exhausts its share of the fuel stops and leaves a hole; the frontier redoes that element with its own budget. Semantics 18.4 is amended accordingly. | Discarded speculative work must not change the result (semantics 18.1). |
| A.6 | Conditions may be evaluated more than once (AC pre-tests and backtracking). | **A condition may be evaluated any number of times, in any order, and must not perform effects.** An effect reached inside a condition gives `failure(effect_in_condition)`. Semantics 6.1 is amended. | Keeps matching deterministic without restricting the matcher's pruning. |
| A.7 | Serial marking caps parallel speedup (I1 finding 5). | **Deferred past M1.** The collector's interface (safe points, `Trace`) does not change when marking becomes parallel. | Performance, not structure. |
| A.8 | Spike 4's wide-node hash bug and memo fence. | **Fixed in the core from the start:** the parallel combiner carries the combined length (I1.8, I2.7); the seqlock writer issues a release fence (I2.8). | Correctness. |

## 2. Crate layout

```
crates/
  mc-api      the interface contract: handles, views, the Store trait, and a simple reference store
  mc-core     the real store: concurrent hash-consed arena, generational collector, number payloads, memo
  mc-arith    canonical arithmetic (semantics 3), canonical order (semantics 8), numeric folding
  mc-syntax   lexer, parser and printer of the concrete syntax (04-syntax.md), lowering to terms
  mc-eval     matcher, rule indexes and evaluator (from integration piece I2), built after mc-core lands
  mc-bench    benchmark harness: case files, guards, progress, results
  mc-repl     the read-eval-print loop
spikes/       the spike crates, kept for reference and comparison
```

`mc-arith`, `mc-syntax` and `mc-bench` depend only on `mc-api` and are written generically over the `Store` trait, so
they can be built and tested against the reference store before `mc-core` exists. `mc-core` implements the trait.

## 3. The interface contract

The contract is code: `crates/mc-api/src/lib.rs`. In outline:

- **Handles.** `T<'h>` is a 32-bit `Copy` term handle branded with the lifetime `'h` of the store borrow it came from
  (spike 1b decision 1b.2, I1.6). `Sym` is a symbol, an immediate that never moves. Handles never outlive their
  borrow; terms kept across evaluations live in root containers.
- **Views.** `view(t)` returns a `View<'h>`: a machine integer that fits `i64`, a big integer (sign and limbs), a
  rational (numerator and denominator terms), a machine real, a ball (sign and payload words, I3.5), a string, a
  symbol, or a compound term (head and arguments). Views borrow from the store for `'h`.
- **Construction.** `sym`, `int`, `big_int` (canonicalising: a value that fits is always the smaller tier, I3.1),
  `rational` (inputs must already be canonical: reduced, denominator greater than 1), `real`, `ball`, `string`, `app`.
  All take `&self`: construction is safe from many threads (I2.1). Canonicalisation of arithmetic is not the store's
  job; it belongs to `mc-arith`.
- **Identity.** Equal terms are the same handle (`==` on `T`). `structural_hash(t)` is stable across collections.
- **Failures** are ordinary terms with head `failure` (semantics 10); the contract fixes the symbol names in
  `mc_api::names`.

What the contract deliberately leaves out of M1: the safe point, `Trace` and the memo are `mc-core`'s own API, used by
`mc-eval` and the session; the generic crates never see them.

## 4. Work packages

| Package | Crate | Builds | Done when |
|---|---|---|---|
| A | `mc-core` | I1's heap with I3's number layout, decisions A.1 to A.4 and A.8, implementing `Store`; the session and safe point API of I1 | `mc-api`'s conformance tests pass against it; I1's and I3's stress invariants pass |
| B | `mc-arith` | `add`, `mul`, `pow` canonicalisation (semantics 3), canonical order (8), exact rational and integer arithmetic through spike 3's limb layer, number folding with contagion (2.2) | property tests: canonical forms are unique, order is total and stable |
| C | `mc-syntax` | lexer, parser, lowering to terms (04-syntax.md section 12), printer back to the concrete syntax | round trip `print(lower(parse(s)))` on a corpus; error positions |
| D | `mc-bench`, `mc-repl` | case-file format, runner with guards and `[k/N pct%]` progress, the first cases of 02-benchmarks.md in the concrete syntax, a REPL shell with a pluggable evaluator | runs cases end to end against a stub evaluator |
| E | (spike) | numeric compiler (running) | its report |
| F | `mc-eval` | I2's matcher and evaluator on `mc-core` | after A lands |

Changes to `mc-api` go through the coordinator only; packages report needed changes instead of editing it.
