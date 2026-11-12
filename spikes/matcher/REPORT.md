# Spike 2: matcher

Code: [`spikes/matcher`](.). Raw results: `results/final-*.md` (final) and `results/run1.md` (preliminary, busy machine).

## Question

How should v2 match patterns and dispatch rules (semantics 6, decisions 13.1 and 13.2), and what does it cost?
Specifically:
- naive against constraint-driven matching modulo associativity and commutativity (AC);
- linear against indexed rule dispatch;
- the cost of ordering rules by the instance-of relation (6.3) at definition time;
- whether argument-head rules (6.5) fit indexed dispatch;
- whether read-set tracking (4.3) is cheap enough.

## Setup

- **Store:** a copy of spike 1's `hc+imm` arena, extended with:
  - machine reals;
  - the canonical order of semantics 8;
  - AC canonicalisation: flatten, then sort (semantics 7.3, without identities or number folding).
- **Patterns** (6.1 subset):
  - forms: `_`, `x_`, `x_ : T` (integer, real, number, symbol, head), `xs__`, `xs___`, non-linear variables,
    literals, alternatives, and conditions (closures over the bindings, which may read own values);
  - compilation flattens AC patterns and folds variable-free subpatterns into literal terms;
  - it also puts AC arguments in **class order**: literals, fixed-head patterns, typed variables, other single
    patterns, then sequence variables.
- **Matcher:** backtracking in continuation-passing style. One mechanism handles nested AC, sequences, alternatives and
  conditions. There are three AC strategies, all walking the same enumeration order:
  - **naive:** every pattern argument tries every unused subject argument, and a sequence variable tries every subset;
  - **naive+rest:** naive, except that the last sequence variable takes the remaining arguments;
  - **smart:** naive+rest, plus:
    - a candidate set per single pattern argument: identity for literals, head plus an independent match test for
      compound patterns, type for typed variables;
    - a Hall test (bipartite matching) before the search;
    - a check at each step that every remaining pattern still has a candidate;
    - only the first unused copy of identical arguments.
- **Dispatch:**
  - **linear:** every rule, in rule order;
  - **indexed:** for non-AC heads, a discrimination tree over the preorder of the left sides. For AC heads, a key-set
    filter: each rule is filed under one of the literal arguments or argument heads it requires, and is a candidate
    only if the subject has all of them and an admissible arity. Candidates are tried in rule order.
- **Rule order** (6.3):
  - a new rule goes just before the first rule it is an instance of, or at the end;
  - a rule identical up to renaming is replaced;
  - the first more general rule is found either by a **scan** of all rules, or by a generalisation query on the same
    **index** followed by the exact check;
  - the instance-of check is sound and incomplete. Property-tested: whenever it claims A is an instance of B, B matches
    random instances of A.
- **Evaluator:**
  - innermost, strict, with own values and per-symbol rule sets;
  - argument-head rules are tried before the head's rules;
  - fuel, the interrupt and a time limit are polled in every loop that depends on the input;
  - memoised normal forms with stamps, under three read-set settings:
    - **untracked:** the stamp covers the term's symbols only, as in spike 1;
    - **mask:** each lookup of an own value or a rule set ORs the symbol's bit into a 64-bit read mask; the stamp
      stores it and memo hits propagate it;
    - **exact:** the sorted list of symbols read, stored per stamp in an arena.
- **Correctness:**
  - every variant's results are compared by a store-independent fingerprint (the rule fired and its bindings, or the
    normal form); all agree;
  - unit tests compare the first match *and* the set of all matches of naive, naive+rest and smart on 30 000 random
    pattern/subject pairs, 11 000 of them matching;
  - the untracked variant of R14 is excluded from the comparison and checked to be stale.
- Machine: AMD Ryzen 7 7700X, 31 GB, Windows 11, Rust 1.99.1, release build.

## Results

Final measurement: `--full`, 3 runs per variant (median), 60 s limit per run, on a mostly idle
machine (spikes 1b and 3 ran alongside on other cores; everything single-threaded). Raw results are in
`results/final-*.md`; the preliminary busy-machine run is `results/run1.md`. "limit" gives the measured cost per item
before the limit was hit. Times in ms.

**R4.** AC rules on `add`/`mul`, applied once at the top of 10^4 sums or products of 4 to 16 arguments. Most rules
need two literals, or a literal and a head; one in a thousand has no key the index can use.

| Rules | linear/naive | linear/smart | indexed/naive | indexed/naive+rest | indexed/smart |
|---|---|---|---|---|---|
| 100 | 25.9 | 9.3 | 4.4 | 4.0 | 4.0 |
| 1000 | 4687 | 66.7 | 4381 | 23.0 | 22.2 |
| 10^4 | 31 118 | 335 | 30 754 | 127 | 125 |

A first version of R4 (preliminary, busy machine, not rerun) had 1% of rules with no usable key (three symbol
variables and a condition). At 10^4 rules, indexed/smart then took 1537 ms against 1561 ms for linear. Those 100 rules
were tried on every subject, enumerated every triple of symbol arguments, and accounted for about 90% of the time.

**R4h, the pigeonhole case.** `add(f(x1_), ..., f(xn_), rest___)` against 1000 sums, half of which have only n-1 `f`
terms:

| n | naive+rest | smart |
|---|---|---|
| 6 | 12.4 | 1.3 |
| 8 | 501 | 1.8 |
| 10 | 36 376 | 2.5 |

**R5.** `g(k)` rules with literal arguments, plus a general `g(x_)`, 10^6 calls (half hit a literal rule):

| Rules | linear | indexed | define (indexed ordering) |
|---|---|---|---|
| 500 | 1250 | 120 | 0.3 |
| 5000 | 18 827 | 231 | 4.4 |
| 50 000 | limit (176 µs/call) | 279 | 68 |

**R13.** `area` extended by many kinds, 10^6 calls. 10% of the calls go to a kind without a rule and fall through to
the general rule.

| Kinds | arg-head | all on `area`, linear | all on `area`, indexed |
|---|---|---|---|
| 10 | 182 | 181 | 225 |
| 100 | 264 | 1163 | 316 |
| 1000 | 346 | 10 722 | 1271 |

**R14.** `list(f(0), ..., f(n-1))`, where `f(x_ : integer) /; x mod 7 > g[x mod 100]` reads one of 100 globals.
Evaluated once, then again without a change, after redefining an unrelated symbol, and after redefining `g3`
(n = 10^6):

| Phase | no memo | memo, untracked | memo, mask | memo, exact |
|---|---|---|---|---|
| eval | 264 | 280 | 267 | 308 |
| re-eval, no change | 207 | 0.000 | 0.000 | 0.000 |
| re-eval, unrelated change | 276 | 0.000 | 11.2 (10^4 rule steps) | 0.001 |
| re-eval, `g3` changed | 282 | 0.000, **stale** | 27.9 (2·10^4 rule steps) | 35.3 (10^4 rule steps) |
| peak heap (MB) | 269 | 300 | 365 | 398 |

First evaluation at the smaller sizes, in the same column order:
- n = 10^4: 1.54 / 1.77 / 1.75 / 2.00 ms;
- n = 10^5: 16.0 / 17.6 / 18.5 / 21.7 ms.

**S7.** `cases` with 5 patterns over 10^6 elements: 83.5 ms naive, 83.0 ms smart, about 17 ns per element and pattern.
At 10^4 and 10^5 elements: 0.81 / 0.79 ms and 8.3 / 8.0 ms.

**Definition-time cost of rule order:**

| Rule set | scan | index |
|---|---|---|
| D5: 500 / 5000 / 50 000 literal rules after a general one | 1.5 / 132 / 13 358 | 0.28 / 3.0 / 41 |
| D4: 100 / 1000 / 10^4 AC rules (R4's) | 0.23 / 17.4 / 1278 | 0.07 / 0.76 / 18.6 |

Both methods produce the same rule order in every case; 798 of the 10^4 AC rules end up out of definition order.

## Findings

1. **Naive AC matching is exponential in exactly one place: sequence variables.**
   - A trailing `rest___` makes the naive strategy enumerate every subset of the remaining arguments, and only one
     subset passes.
   - On R4 this costs 3.1 ms per subject at 10^4 rules (31 s in total), 250 times more than smart, and 200 times more
     at 1000 rules.
   - Letting the last sequence variable take the rest (naive+rest) removes almost all of it: within 4% of smart on R4
     at every size.
2. **Candidate sets and the Hall test matter for the pigeonhole shape.**
   - When several patterns compete for the same arguments and there are too few of them, backtracking costs (n−1)!.
   - At n = 10 that is 36 s for 1000 subjects, against 2.5 ms with the Hall test (14 000 times); at n = 8, 501 ms
     against 1.8 ms.
   - This shape (sums of many like terms) is common in algebra and rare in R4.
3. **Class order gives cheap pruning without changing the semantics.**
   - Putting literal and fixed-head pattern arguments first makes most failing AC matches fail on the first argument,
     in both strategies.
   - Because class order is part of the defined enumeration order, no pruning step changes the first match.
   - This is verified on random pairs: same first match and same set of matches.
4. **Indexing is what makes large rule sets cheap.**
   - With literal arguments (R5), indexed dispatch costs 0.12 to 0.28 µs per call from 500 to 50 000 rules. The slight
     growth is memory locality, as in finding 6.
   - Linear dispatch grows with the rule count, to 176 µs per call at 50 000 rules, 630 times slower.
   - For AC rule sets (R4), the key-set filter gains 2.3 to 3 times. The gain is smaller there for two reasons:
     subjects match an early rule (96% of subjects match some rule at 10^4 rules), and the matching itself, not the
     selection, dominates.
5. **Rules the index cannot see dominate AC dispatch.**
   - One percent of rules with only variables and a condition took about 90% of R4's time at 10^4 rules.
   - They are candidates for every subject, and each of them enumerates argument tuples before the condition can
     reject them.
6. **Argument-head rules fit indexed dispatch (decision 13.2).**
   - Their dispatch is a lookup on the argument's head (one flag test when the symbol holds no argument-head rules),
     followed by an ordinary indexed rule set.
   - The cost grows slowly with the number of kinds: 182, 264 and 346 ms per 10^6 calls at 10, 100 and 1000 kinds.
   - That is 3.7 times lower than putting all 1000 kinds on `area` with indexing (1271 ms), and 31 times lower than
     linear dispatch on `area` (10.7 s).
   - With all rules on `area`, the index returns 1.9 candidates per call at every size (counted with a probe). So the
     growth with the number of kinds (225 to 1271 ms for all on `area`, 1.9 times for argument-head rules) comes from
     memory locality, not selectivity: rule trees and trie nodes are reached in random order.
7. **Read-set tracking with a mask is free; without it, stamps are wrong (decision 13.1).**
   - Untracked stamps return the stale result after `g3` is redefined.
   - Mask tracking costs nothing measurable on the first evaluation: within ±5% of untracked at every size (267 against
     280 ms at 10^6). It invalidates correctly.
   - Its cost is false invalidation. 64 bits for about 110 symbols give collisions, so an unrelated redefinition
     re-evaluated 1% of the calls, and the `g3` change re-evaluated 2% instead of 1%.
   - Exact read sets invalidate precisely (0% and 1%), but cost 10 to 23% more on the first evaluation and 9% more
     memory. The extra time goes to sorting and storing the sets; the root's set is the union of 10^6 children's.
8. **Ordering rules by instance-of is affordable with the index, not with a scan.**
   - A scan is quadratic: 13.4 s to define 50 000 literal rules, 1.3 s for 10^4 AC rules.
   - The generalisation query on the dispatch index brings this to 41 ms and 19 ms (320 and 69 times faster), and
     produces the same order.
9. **The instance-of check needs care to be sound.**
   - The first version treated two anonymous blanks (or two copies of `a | b`) of the more specific pattern as equal;
     the randomised soundness test found the counterexample.
   - Pieces of a pattern are known to stand for the same term only when they are built from literals and the same
     named variables.

## Decisions

| # | Decision | From |
|---|---|---|
| 2.1 | Match by backtracking in continuation-passing style, with the smart AC strategy: class order, the last sequence variable takes the rest, candidate sets, a Hall test, and first-copy-only for identical arguments. Full Eker-style bipartite graphs of sub-matches and many-to-one AC automata are not needed for the measured rule sets. | findings 1 to 3 |
| 2.2 | Define the enumeration order of semantics 6.2 precisely: the arguments of an AC pattern in class order (definition order within a class); each single argument takes the subject's arguments in canonical order; a sequence variable takes subsets in lexicographic order of their positions, a set before its extensions; outside AC, sequence variables take the shortest run first. Proposed as text for semantics 6.2. | finding 3 |
| 2.3 | Under an associative head, `x_` matches exactly one argument; several arguments are matched by sequence variables. Proposed for semantics 6.2. | finding 1 |
| 2.4 | Rule sets are always indexed: a discrimination tree for non-AC heads, a key-set filter for AC heads, candidates tried in rule order. Linear dispatch is not kept as a mode. | finding 4 |
| 2.5 | Rule order (6.3) is maintained by inserting before the first more general rule, found by a generalisation query on the dispatch index. The instance-of check is sound and may be incomplete; semantics 6.3 should say that pairs it cannot order stay in definition order. | findings 8, 9 |
| 2.6 | Decision 13.2 is **settled as written**: argument-head rules at depth one, tried before the head's rules, each held set indexed like any other. Symbols carry a flag "holds argument-head rules", so calls whose arguments hold none pay one test per argument. | finding 6 |
| 2.7 | Decision 13.1 is **settled**: read-set tracking is on, with 64-bit masks propagated through memo hits; no fallback to declared dependencies is needed. Exact read sets are not the default; spend memory on a wider mask instead if collisions matter (follow-up). | finding 7 |
| 2.8 | A condition inside a pattern may use only the variables bound inside its own subpattern; a condition on the whole left side sees all of them. This keeps conditions valid at the points where the matcher tests them independently. Proposed for semantics 6.1. | setup (smart strategy) |
| 2.9 | A rule whose AC left side has no indexable key (no literal argument and no argument with a fixed head) is accepted with a diagnostic at definition time (semantics 10), because it is tried on every subject. | finding 5 |

## What it rules out

- Naive AC matching that enumerates subsets for sequence variables: it is exponential on ordinary rule sets.
- Linear rule dispatch for anything but tiny rule sets.
- Scanning all rules to keep the instance-of order: it is quadratic at definition time.
- Normal-form stamps without read sets: they give wrong answers as soon as a condition reads a global.
- Storing the extensions of a generic function as rules on that function instead of as argument-head rules: 3.7
  times slower with indexing at 1000 kinds, and linear in the number of kinds without it.

## Follow-ups

- **Memory layout of compiled patterns and indexes:** flat arrays instead of pattern trees of boxed vectors, to
  remove the locality cost seen in findings 4 and 6 (indexed time per call grows 2.3 times over R5's sizes, and up to
  5.6 times over R13's, at constant selectivity). Compiling rule sets to matching automata (plan section 4) is the natural next
  step, and should be measured against this index.
- **AC arity beyond 64 arguments:** the spike's argument sets are 64-bit masks and assert past that. Sums of
  polynomials need bit sets, or packed polynomials (decision 1.5), which avoid term-level AC matching altogether.
- **Wider or better-hashed read masks:** 128 or 256 bits, or bits taken from a hash of the symbol instead of its index,
  to reduce the false invalidations of finding 7. Measure on a session with thousands of symbols.
- **Conditions that evaluate terms:** here conditions only read. Real conditions call the evaluator, which needs the
  store to be shared and mutable during matching (interior mutability, or the concurrent table of spike 4).
- **Recursion:** the matcher recurses in pattern depth, which the rule set bounds. The evaluator and the canonical
  order recurse in term depth and need explicit stacks (decision 1.6).
- **R6 (large conditional rule sets)**, and R1 to R3, on this matcher and evaluator once the rule sets exist.
