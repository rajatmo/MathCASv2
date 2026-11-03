# MathCAS v2: design plan

This document fixes how v2's architecture is decided and what the starting hypothesis is. It is a plan, not a spec:
the semantics document (01) and the benchmark suite (02) come next, and the spikes (section 5) decide between the
options below.

## 1. Method

Architecture is decided by measurement against stated goals, top down:

1. **Goals as measurable targets.**
   - *Simplicity:* number of core primitives; size of the semantics document; every evaluation rule written down.
   - *Speed:* time and memory on the benchmark suite (section 3), per workload class.
   - *Predictability:* a reader of the spec can say what an expression evaluates to; every computation is bounded by
     time, memory and fuel limits and reports when it hits one.
2. **Benchmark suite before code** (02-benchmarks.md). Every architectural choice is phrased as "which option wins on
   which class".
3. **Semantics before implementation** (01-semantics.md): terms, numbers, evaluation order, holding, rules,
   strategies. v1's hardest problems came from semantics that were inherited instead of specified: hidden holds,
   attributes that disagree with behaviour, evaluate-until-nothing-changes.
4. **Spikes for the risky parts**, each measured in isolation (section 5); commit to a design only after them.
5. **Build bottom-up behind the spec**, with the benchmark and conformance harness running from day one.

## 2. Starting architecture (hypothesis)

### Layer 0: term store
- Hash-consed terms: each distinct term is stored once, so equality is a pointer comparison and per-term caches are
  cheap.
- Small integers (and later symbols) are immediates encoded in the term handle, not stored (spike 1, decision 1.2).
- A collector reclaims unreachable terms at safe points between evaluations (spike 1, decision 1.4; spike 1b).
- Per-term metadata: hash, depth, free-symbol set, and a *normal-form flag* (the term is known to be fully evaluated
  under the current rule set version).
- Arena allocation; no per-node reference counting on the hot path.

### Layer 1: numbers and packed domains
- Exact integers and rationals, machine floats, and arbitrary-precision **balls** (midpoint plus error radius) as
  distinct types. Ball arithmetic gives results with provable error bounds and is simpler to specify than
  significance arithmetic; at high precision it costs about the same as plain floats.
- Packed internal forms with their own algorithms: sparse multivariate polynomials (heap-based multiplication for huge
  sparse inputs), rational functions, truncated series, and numeric arrays stored as flat machine-number arrays.
  Results convert back to ordinary terms only when a computation leaves the domain.

### Layer 2: rewriting engine
- Rule sets compiled before use into indexing and decision structures (discrimination trees, matching automata), not
  tried one rule at a time.
- Matching modulo associativity and commutativity for Plus, Times and similar operators via bipartite matching, with
  many-to-one matching for large rule sets.
- Innermost-first rewriting to a normal form, using the normal-form flag instead of re-evaluating until nothing
  changes.
- Simplification (as opposed to evaluation) by equality saturation over e-graphs: explore equivalent forms, extract
  the cheapest under a stated cost function. A rule set plus a cost measure replaces piles of heuristics.

### Layer 3: evaluation control
A few explicit primitives instead of an attribute zoo:
- quote / hold;
- strict versus lazy argument positions, declared per function and compiled into the matcher;
- rule-application strategies (once, repeat, innermost, outermost, fixed point with fuel).

### Layer 4: surface language
- v2's own syntax with gradual types.
- Any compatibility syntax is a front end translating into the core, judged against a conformance corpus.

## 3. Benchmark suite (outline; detailed in 02)

About 200 cases in five classes, each with reference timings and other systems as comparison points:

1. **Rewriting:** large rule sets, simplification, rule-based integration.
2. **Polynomial and rational algebra:** expand, gcd, factor, resultants, huge sparse polynomials (degree 10^6 with a
   handful of terms).
3. **Big numbers:** 10^4 to 10^6 digit elementary and special functions; exact integer algorithms (prime counting,
   primitive roots of huge prime powers).
4. **Numeric loops:** functions sampled by plotting, integrated by ODE solvers, root finders, table construction.
5. **Structural operations:** lists, parts, mapping, sorting, association operations.

Includes algorithmic stress cases, not just micro-benchmarks: in v1 the wins came from algorithm choice (a divisor
test instead of factoring a degree-10^6 polynomial), not constant factors.

## 4. Compilation and JIT

| Where | Expected gain | Approach |
|---|---|---|
| Numeric inner loops (functions passed to plotting, ODE solvers, root finders, tables) | 10 to 1000 times | Type-specialise and compile to machine code (Cranelift or LLVM), automatically for typed numeric functions. |
| Pattern matching | Large; the core of fast rewriting | Compile rule sets to matching automata, optionally to native code; recompile when a definition changes. |
| General symbolic evaluation | Unproven | Tracing JITs and self-specialising interpreters speed up dynamic languages, but symbolic workloads are dominated by matching, allocation and big-number arithmetic, not dispatch. Only if profiling shows interpretive overhead dominates. |

Order: interpreter, then compiled matchers, then the numeric compiler. No general JIT unless the benchmarks call for
it.

## 5. Spikes (decide the risky parts)

1. **Term store:** hash-consed terms with normal-form flags against v1's representation, on rewriting and structural
   benchmarks.
2. **AC matcher:** compiled many-to-one matching for associative-commutative operators against naive matching, on large
   rule sets.
3. **Numbers:** ball arithmetic at 10^4 to 10^6 digits against v1's bigfloat, including elementary and elliptic
   functions.

Each spike ends with a short report: numbers, decision, what it rules out.

## 6. Lessons carried over from v1

- Inheriting another system's semantics drove most of the complexity (layered overrides, hidden holds, attribute
  tables that disagree with behaviour). v2 defines its own.
- Every hang in v1 was a loop that never checked for an interrupt. v2 builds fuel counters, interrupt polling and
  memory accounting into the core.
- Algorithm choice beats constant factors.
- A conformance and performance harness with per-case tracing, live progress and crash diagnosis is worth building
  first.

## 7. Reading list

- J.-C. Filliâtre, S. Conchon, *Type-safe modular hash-consing*, ML Workshop 2006.
- R. Sekar, I. V. Ramakrishnan, A. Voronkov, *Term indexing*, Handbook of Automated Reasoning, 2001.
- S. Eker, *Associative-commutative matching via bipartite graph matching*, Computer Journal 38(5), 1995.
- M. Krebber, *Non-linear associative-commutative many-to-one pattern matching with sequence variables*, MSc thesis,
  2017 (MatchPy).
- M. Clavel et al., *All About Maude*, LNCS 4350, 2007 (rewriting logic, strategies, compiled matching).
- M. Willsey et al., *egg: Fast and extensible equality saturation*, POPL 2021.
- F. Johansson, *Arb: efficient arbitrary-precision midpoint-radius interval arithmetic*, IEEE Trans. Computers, 2017.
- L. Fousse et al., *MPFR: a multiple-precision binary floating-point library with correct rounding*, ACM TOMS, 2007.
- M. Monagan, R. Pearce, work on heap-based sparse polynomial multiplication and division (2007 onwards).
- J. A. M. Vermaseren, *New features of FORM*, 2000 (large expression manipulation).
- B. Ruijl, Symbolica (modern Rust CAS core; polynomial and rewriting performance).
- J. Bezanson et al., *Julia: a fresh approach to numerical computing*, SIAM Review, 2017 (type-specialising JIT).
- C. F. Bolz et al., *Tracing the meta-level: PyPy's tracing JIT compiler*, ICOOOLPS 2009.
- T. Würthinger et al., *One VM to rule them all*, Onward! 2013 (self-optimising AST interpreters).
- R. Fateman, essays on rule-based computer algebra design and its pitfalls.

## 8. Next documents

- [`01-semantics.md`](01-semantics.md): the core semantics (draft 2).
- [`02-benchmarks.md`](02-benchmarks.md): the benchmark suite (draft 1).
- [`03-spikes.md`](03-spikes.md): spike reports (spike 1, term store: done).
- [`04-syntax.md`](04-syntax.md): the concrete syntax (draft 1).
