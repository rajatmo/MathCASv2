# MathCAS v2: benchmark suite (draft 1)

The suite turns "simpler and faster" into numbers. Every architectural decision in the plan is phrased as "which option
wins on which class", and every spike reports against cases from this document. Notation follows the semantics
document (abstract syntax; concrete syntax is `04-syntax.md`).

## 1. Measurement protocol

- **Machine:** one fixed machine, recorded with every run (CPU model, cores, RAM, OS, compiler versions).
  Numbers from different machines are never mixed in one table.
- **Threads:** single-threaded for classes R, P, N, L and S. Class M (multicore) measures parallel speed separately
  (section 5, class M).
- **Timing:** wall-clock time of the measured expression only, excluding startup and parsing (startup is case S10).
  Each case runs once to warm up, then at least 5 times or for at least 2 seconds, whichever is longer; the **median**
  is reported, with the minimum and the interquartile range.
- **Memory:** peak resident memory of the measured evaluation above the session's baseline.
- **Limits:** every case runs under a 60 s time limit and a 8 GB memory limit (overridable per case). A case that hits a
  limit is reported as `limit`, never as a time. A case that hangs past the limit without stopping is a bug in itself
  (the semantics require every loop to be interruptible).
- **Fresh state:** each case starts in a new session, so caches from one case never help another. Cases that measure
  caching say so (R11).

## 2. Correctness

A fast wrong answer scores as a failure. Each case has a checker, one of:

- **exact:** the result equals a stored value (or its hash, for huge results);
- **property:** an invariant holds (the factors multiply back to the input, a root satisfies the equation to the
  stated accuracy, a sorted list is sorted and a permutation of the input);
- **digits:** a numeric result agrees with a stored high-precision value to the stated number of digits, and its error
  bound (for balls) contains the true value.

Results are checked after timing, outside the measured region.

## 3. Comparison systems

| System | Role |
|---|---|
| MathCAS v1 | Baseline: what we already have. |
| Symbolica | Modern rewriting and polynomial core; the bar for classes R and P. |
| FLINT / Arb | The bar for exact and ball arithmetic in classes P and N (library calls, no interpreter). |
| FORM | Large-expression manipulation (R8, P1, P2). |
| Julia | Numeric loops in class L (compiled code as the upper bound). |

Not every system runs every case; missing entries are left blank.

## 4. Scoring and targets

- Each measurement gives a **ratio** `t(v2) / t(c)` against the class's comparison system `c` (v1 where it runs the
  case, otherwise the bar named in section 3). Below 1 means v2 is faster.
- Per class: the **geometric mean** of the ratios over its cases (each size counted separately).
- Overall: the geometric mean of the five class means (classes weigh equally, whatever their number of cases).

Targets, by milestone:

| Milestone | Target |
|---|---|
| M1 (core runs the suite) | every case completes or reports `limit` cleanly; no hangs, no crashes |
| M2 | class geometric means at most 2; every exact and property check passes |
| M3 | class geometric means at most 1 for R, P and N; at most 1.5 for L and S |
| M4 | overall geometric mean at most 1, and no single case worse than 10 |

Memory is reported alongside time; a case that is faster but uses more than 4 times the comparison system's memory is flagged.

## 5. The cases

Each case has an id, a description, three sizes (small, medium, large) where it scales, what it stresses, and its
checker. About 70 cases at up to three sizes give roughly 210 measurements. Cases marked **(alg)** are algorithmic
stress cases: they measure whether the right algorithm is used, not constant factors.

### Class R: rewriting and evaluation

| Id | Case | Sizes | Stresses | Check |
|---|---|---|---|---|
| R1 | Naive recursive `fib(n_) := fib(n-1) + fib(n-2)` | n = 20, 25, 30 | call and rule-dispatch overhead | exact |
| R2 | Memoising `fib(n_) := fib(n) = ...` | n = 10^3, 10^4, 10^5 | definition creation, rule-set growth | exact |
| R3 | Symbolic derivative by a rule set of 30 rules on a random expression tree | 10^3, 10^4, 10^5 nodes | rule dispatch, term construction | exact (hash) |
| R4 | AC matching: 1000 rules with commutative and associative patterns on `add`/`mul`, applied to 10^4 terms | 100, 1000, 10^4 rules | AC matching, many-to-one indexing | exact (hash) |
| R5 | Rule indexing: 5000 rules `g(k) := ...` with literal arguments, 10^6 calls | 500, 5000, 50000 rules | discrimination-tree lookup | exact |
| R6 | Rule-based integration: integrate a fixed set of 500 integrands with a large rule set | 50, 200, 500 integrands | large conditional rule sets | property (differentiate back) |
| R7 | Trigonometric simplification by equality saturation | 50, 200 expressions | e-graph growth, extraction | property (numeric equivalence) and size of result |
| R8 | Expansion as plain terms: `expand((a+b+c+d)^n)` with packing disabled | n = 10, 20, 30 | canonicaliser, like-term collection | exact (hash) |
| R9 | Deep nesting: `nest(f, x, n)` built, evaluated and traversed | n = 10^4, 10^5, 10^6 | stack safety, deep terms | exact |
| R10 | `replace_all` over a large tree with 10 rules | 10^4, 10^5, 10^6 nodes | traversal, rebuilding | exact (hash) |
| R11 | Re-evaluation after a small redefinition: evaluate a large expression, redefine one symbol it does not contain, evaluate again | 10^4, 10^5, 10^6 nodes | normal-form stamps (semantics 4.3) | second evaluation is O(1): time ratio second/first |
| R12 | Interpreted loops: Collatz lengths with `while` | up to 10^4, 10^5, 10^6 | interpreter overhead | exact |
| R13 | Argument-head rules: generic `area` extended by 100 object kinds, 10^6 calls | 10, 100, 1000 kinds | dispatch through argument-head rules (semantics 6.5) | exact |
| R14 | Read-set tracking: conditions that read global values, evaluated 10^6 times | 10^4, 10^5, 10^6 | cost of dependency tracking (semantics 13.1) | exact |

### Class P: polynomial and rational algebra

| Id | Case | Sizes | Stresses | Check |
|---|---|---|---|---|
| P1 | Dense multivariate: `f = (1+x+y+z)^n`, expand `f * (f+1)` | n = 10, 20, 30 | multivariate multiplication | exact (hash) |
| P2 | Five variables: `f = (1+x+y+z+t)^n`, expand `f * (f+1)` | n = 10, 15, 20 | large multiplication | exact (hash) |
| P3 | Sparse multiplication (Monagan–Pearce style sparse five-variable inputs) | 3 standard sizes | heap-based sparse multiplication | exact (hash) |
| P4 | Multivariate gcd of two products sharing a large factor | 3 sizes | gcd algorithms | property (divides both, cofactors coprime) |
| P5 | Univariate factorisation, including Swinnerton-Dyer polynomials | degree 32, 64, 128 | factorisation over Z | property (product, irreducibility of factors by degree check) |
| P6 | Multivariate factorisation | 3 sizes | multivariate factorisation | property |
| P7 | Resultant and discriminant of dense polynomials | degree 20, 50, 100 | subresultants or modular methods | exact |
| P8 | **(alg)** Huge sparse polynomials: integer roots and real-root count of `x^(10^6) - 2x^777777 + 3x^12345 + 9x^67 - 10` and products like it | 3 inputs | algorithms that avoid dense representations | exact |
| P9 | Rational-function normalisation: `together` and `cancel` of large sums | 3 sizes | rational arithmetic, gcd | exact (hash) |
| P10 | Gröbner bases: cyclic-n, katsura-n | cyclic 5, 6; katsura 6, 8 | Buchberger / F4 | exact (hash of reduced basis) |
| P11 | Univariate dense multiplication with large integer coefficients | degree 10^4, 10^5, 10^6 | FFT-based multiplication | exact (hash) |
| P12 | Power series: `series(exp(sin(x)), x, 0, n)` | n = 100, 1000, 10000 | truncated series arithmetic | exact |
| P13 | Real-root isolation of a sparse degree-777 polynomial (the v1 case) | degree 200, 777, 2000 | root isolation without coefficient blow-up | exact (interval count and disjointness) |

### Class N: exact and arbitrary-precision numbers

| Id | Case | Sizes | Stresses | Check |
|---|---|---|---|---|
| N1 | Digits of `pi` | 10^4, 10^5, 10^6 digits | binary splitting, multiplication | digits |
| N2 | **(alg)** Digits of `e` starting far after the point (the v1 digits case) | offset 10^4, 10^5, 10^6 | series by binary splitting | digits |
| N3 | Elementary functions `exp`, `log`, `sin`, `atan` at high precision | 10^3, 10^4, 10^5 digits | argument reduction, series | digits |
| N4 | Special functions at high precision: gamma, zeta, Bessel J, Jacobi elliptic and inverse, 2F1 | 10^3, 10^4, 5·10^4 digits | special-function algorithms | digits |
| N5 | Exact factorial and binomial | 10^4!, 10^5!, 10^6! | product trees | exact (hash) |
| N6 | **(alg)** Prime counting and the n-th prime: `prime_pi(10^k)`, `prime(10^k)` | k = 8, 10, 12 | combinatorial prime counting, segmented sieve | exact |
| N7 | Integer factorisation of semiprimes | 30, 45, 60 digits | ECM, quadratic sieve | property |
| N8 | **(alg)** Primitive root of a huge prime power (the v1 case) | p^k with 10^4, 10^5, 10^6 digits | perfect-power detection instead of factorisation | exact |
| N9 | Big-integer multiplication, division, gcd | 10^5, 10^6, 10^7 digits | FFT multiplication, subquadratic division and gcd | exact (hash) |
| N10 | Exact harmonic numbers `H(n)` | n = 10^4, 10^5, 10^6 | rational arithmetic, binary splitting | exact (hash) |
| N11 | Precision refinement on near-cancellation: `n(e, 50)` of expressions with catastrophic cancellation | 3 expressions | adaptive precision (semantics 2.6) | digits |
| N12 | Ball comparisons that need refinement: decide `a < b` for nearly equal exact expressions | 3 pairs | refinement loop, termination | exact |

### Class L: numeric loops

| Id | Case | Sizes | Stresses | Check |
|---|---|---|---|---|
| L1 | Table of a 20-operation machine-real expression | 10^5, 10^6, 10^7 points | numeric compiler | digits (spot values) |
| L2 | Adaptive sampling of an oscillatory function, as a plot would | 3 functions | adaptive refinement, compiled evaluation | property (sample density near features) |
| L3 | Non-stiff ODE: Lorenz system, explicit Runge–Kutta, tolerance 1e-10 | t = 10, 100, 1000 | right-hand-side evaluation | digits (against a reference trajectory point) |
| L4 | Stiff ODE: Robertson problem, implicit method | t = 10^2, 10^5, 10^11 | Jacobians, linear solves | digits |
| L5 | Root finding: Newton solves of a parametrised equation | 10^3, 10^4, 10^5 solves | compiled function and derivative | property (residual) |
| L6 | Numeric integration of smooth and oscillatory integrands | 10^2, 10^3, 10^4 integrals | quadrature, compiled integrand | digits |
| L7 | Dense linear algebra on packed arrays: multiply, solve, eigenvalues | n = 200, 1000, 3000 | packed arrays, native kernels | property (residual) |
| L8 | Sparse linear solve | 10^4, 10^5, 10^6 unknowns | sparse storage, iterative solvers | property (residual) |
| L9 | Mandelbrot escape counts, 256 iterations | 200², 1000², 3000² | compiled loops with complex numbers | exact (checksum) |
| L10 | Monte Carlo estimate with the explicit generator | 10^6, 10^7, 10^8 draws | random generation, compiled loop | exact (seeded checksum) |

### Class S: structural operations

| Id | Case | Sizes | Stresses | Check |
|---|---|---|---|---|
| S1 | Integer list: build, sum, map, select | 10^5, 10^6, 10^7 | packed arrays, list primitives | exact |
| S2 | Sort mixed terms by the canonical order | 10^4, 10^5, 10^6 | canonical comparison | property |
| S3 | Part extraction and replacement in nested lists | 10^4, 10^5, 10^6 | part access, copy-on-write | exact (hash) |
| S4 | Associations: inserts, lookups, deletions | 10^4, 10^5, 10^6 | hash maps of terms | exact |
| S5 | Strings: split, join, replace, regular expressions | 10^4, 10^5, 10^6 | string library | exact (hash) |
| S6 | Flatten and transpose of nested lists | 10^4, 10^5, 10^6 elements | structural rebuilding | exact (hash) |
| S7 | Pattern-based selection (`cases`) over a list | 10^4, 10^5, 10^6 | matching without rule sets | exact |
| S8 | Allocation stress: build distinct small terms | 10^5, 10^6, 10^7 | hash-consing table, arena | exact (count) |
| S9 | Memory per term: hold the result of P1 (large) and report bytes per term | 1 size | term representation | reported, not timed |
| S10 | Startup: time from launch to the first evaluated result | 1 size | load and initialisation | reported |

### Class M: multicore

Each case runs at 1, 2, 4, 8 and 16 threads. The score is the **speedup** `t(1) / t(k)` and the **efficiency**
`speedup / k`; the result at every thread count must be identical to the one-thread result (semantics 18.1), checked by
fingerprint.

| Id | Case | Sizes | Stresses | Check |
|---|---|---|---|---|
| M1 | Compiled table of a machine-real expression (L1, parallel) | 10^6, 10^7, 10^8 points | parallel compiled loops | identical |
| M2 | `pmap` of a special function over a packed array | 10^4, 10^5, 10^6 elements | data parallelism with uneven cost | identical |
| M3 | Parallel sparse polynomial multiplication (P2 large, P3) | 3 sizes | parallel kernels | identical |
| M4 | Modular gcd and resultant over many primes | 3 sizes | independent modular images | identical |
| M5 | Big-integer multiplication | 10^6, 10^7, 10^8 digits | parallel FFT or NTT | identical |
| M6 | Term-level: a sum of 64 independent heavy subterms (expansions, derivatives) | 3 sizes | speculative parallel arguments (semantics 18.2) | identical |
| M7 | Tiny tasks: `map` of a trivial function over 10^6 elements | 10^5, 10^6, 10^7 | granularity control | identical, and no slower than 1.1 times one thread |
| M8 | Concurrent interning: workers building overlapping and distinct terms | 10^6, 10^7, 10^8 terms | concurrent hash-consing | identical |
| M9 | ODE ensemble: 1000 trajectories | 3 sizes | parallel compiled right-hand sides | identical |
| M10 | Determinism with randomness: `ptable` of random draws, then reductions | 10^4, 10^5, 10^6 | split random streams (semantics 18.2) | identical for every thread count and seed |
| M11 | Speculation with effects: arguments that print or define in the middle | 3 shapes | effect ordering (semantics 18.2) | outputs and value identical to sequential |

Targets: efficiency at least 0.7 at 8 threads for M1, M2, M3, M5 and M9 (large sizes); M7 never more than 1.1 times
slower than one thread; M10 and M11 identical in every run.

## 6. Mapping to the spikes

| Spike (plan section 5) | Decides with |
|---|---|
| Term store | R3, R8, R9, R10, R11, S3, S6, S8, S9 |
| AC matcher | R4, R5, R6, R13, S7 |
| Numbers | N1 to N5, N9 to N12, P11 |

| Parallel evaluation | M6, M7, M8, M10, M11 (and M1, M2 once the compiler exists) |

The read-set tracking question (semantics 13.1) is settled by R11 and R14; the argument-head indexing question (13.2)
by R13.

## 7. Runner

- Cases live in `bench/cases/<class>/<id>.<ext>`, one file per case, holding the setup, the measured expression, the
  sizes and the checker. Comparison-system versions of a case live next to it where they exist.
- The runner (`mc-bench`) runs a case under the protocol of section 1, checks it (section 2), and writes one line of results per
  size: system, case, size, median, min, interquartile range, peak memory, check result.
- Results are stored per machine and date under `bench/results/`; a summary script prints the class tables, the
  geometric means and the milestone status (section 4).
- The runner reuses v1's lessons: a per-case trace (start, end, time, memory, limit events) and live progress output.

## 8. Open points

- The integrand set for R6 and the expression set for R7 need to be chosen and fixed (sources with permissive licences
  only).
- Whether L7 may call an external dense linear-algebra library, or must be native, is a scope decision for the plan.
