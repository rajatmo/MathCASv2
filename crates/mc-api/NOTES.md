
## Item N4a

Packed real and integer arrays (store node, contract, evaluator hooks, payload fast paths): see `crates/mc-core/NOTES.md`, section "Item N4a".

## Round 43: budgets of work

Decision (November 2026): the same input gives the same answer on every machine, so no method budget is measured in
seconds (semantics 18.7). Every clock read in `crates/*/src` was classified:

- (a) a limit or value the user asked for: stays;
- (b) progress, status, statistics, profiling, benchmarks, tests, housekeeping: stays;
- (c) a method budget that decides whether an answer is given or which one: converted to fuel.

| file | clock reads | class | converted |
|---|---|---|---|
| mc-integrate/src/budget.rs | 3 | c (method budget of the decision procedure and the ansatz) | yes |
| mc-integrate-rules/src/run.rs | 9 | 5 c (deadline of a call, its engine shares), 4 b (dispatch and matching statistics) | the 5 of class c |
| mc-integrate-rules/src/lib.rs | 2 | b (statistics under MC_RULES_STATS) | no |
| mc-integrals/src/portfolio.rs | 2 (and 4 budgets in seconds) | c | yes |
| mc-integrals/src/elementary/mod.rs | 1 | c | yes |
| mc-integrals/src/elementary/algebraic/mod.rs | 2 | c | yes |
| mc-integrals/src/definite/mod.rs | 2 (and the limit budget) | c | yes |
| mc-integrals/src/definite/conv.rs, residue.rs | 0 (budgets of limits in seconds) | c | yes |
| mc-limit/src/lib.rs, more.rs, gruntz.rs | 2, 2, 3 | c (gruntz: 1 c, 2 b trace) | the 5 of class c |
| mc-dsolve/src/lib.rs | 3 | c (2), b (1, the trace) | yes (the trace prints work left) |
| mc-decide/src/decide.rs | 3 | c | yes |
| mc-sums/src/cx.rs | 2 | c | yes (units of polynomial work) |
| mc-simplify/src/simplify.rs | 4 | c | yes |
| mc-simplify/src/lib.rs | 1 | a (`time_constraint:`) | no |
| mc-poly/src/natives.rs | 2 | c (internal `time_budget:`, now `work_budget:`) | yes |
| mc-poly/src/groebner.rs | 2 | b (a test's timing print) | no |
| mc-calculus/src/props.rs | 2 | c (sampling budget of the property functions) | converted (5M units of work for the refuting search) |
| mc-fields/src/conway.rs | 6 | b (the table generator, a test: progress and which entries to tabulate) | no |
| mc-eval/src/constrained.rs | 2 | a (`time_constrained`) | no |
| mc-eval/src/eval.rs, machine.rs, profile.rs | 1, 2, 1 | b (`profile`) | no |
| mc-eval/src/native.rs | 3 | b (the test hook that slows the fuel down) | no |
| mc-eval/src/workloads.rs | 2 | b (benchmark workloads) | no |
| mc-engine/src/history.rs | 6 | a (`timing`) | no |
| mc-cad/src/natives.rs | 2 | b (internal statistics) | no |
| mc-core/src/gc.rs, session.rs, shapes.rs | 4, 3, 2 | b (pause and work statistics) | no |
| mc-jit/src/spec.rs, mc-numfn/src/compiled.rs | 2, 2 | b (compile-time statistics) | no |
| mc-numeric/src/consts.rs, mc-numtheory/src/leaves.rs, mc-nbmodel/src/printed.rs | 2, 2, 2 | b (tests) | no |
| mc-random/src/lib.rs | 1 | a (the default seed when the user gives none) | no |
| mc-plot/src/lib.rs, mc-nbfile/src/embed.rs | 4, 1 | b (removing old cache files) | no |
| mc-kernel/src/server.rs, mc-repl/src/lib.rs | 2, 2 | b (time reported with a result) | no |
| mc-bench/src (child, guard, results, runner) | 12 | a (the benchmark's limits), b (measurements) | no |
| mc-docs/src (app, runner, selftest) | 11 | b (interface, progress) | no |
| mc-notebook/src (13 files) | 43 | b (interface: animation, status, self-test) | no |

**The unit of work.** [`work`] is a per-thread counter of *operations* (about a nanosecond of work each) that the
kernels below the evaluator report: a limb product of `mc-arith`'s natural numbers (school products, the NTT at
`9 n log n` for `n` points, Knuth division at `(m + 1)(n + 4)`, one-limb division at 4 per limb), 32 for every
natural number made (its allocation) and `32 + 8 m n` for a gcd, every term built by `mc-core` (`24 + 2 args`), four
per step of an `mc-poly` meter (polynomial and linear-algebra kernels), the merges, scalings and term products of
`mc-poly`'s sparse polynomials (`terms (variables + 16)`), the terms of the machine series of `mc-special` (`complex::series`, `hyper::series`, the incomplete gamma series and continued
fraction). The evaluator takes the counter after every native call, when a native charges fuel, and around every
element of a group, and charges one unit of fuel per 256 operations to the task that ran the work; the part of a
unit left over stays with the task, and an element of a group is charged its own work rounded up, so its count does
not depend on where its neighbours ran. `NativeCx::fuel_spent` is the reading budgets take; `mc_eval::Allowance` is
a budget of units from a reading, belonging to one task.

**The budgets** (units of fuel; chosen from a run of the example pages with the old clock budgets deciding and
the work counted beside them: every method that finished in time finished within the new budget, which is the old
seconds times at least 500 units per millisecond, above the rates measured under load):

| budget | was | now |
|---|---|---|
| integration: decision procedure, alone, ansatz | 1.5 s, 6 s, 2.5 s | 300k, 1.2M, 500k (measured rate of the tower methods: about 200 units per ms) |
| elementary rules (nothing else answered / after an answer) | 4 s / 0.6 s | 2M / 300k |
| algebraic stage | 2 s | 1M |
| rule table: call, its decision procedure, its ansatz | 1.5 s, 0.3 s, 0.4 s | 750k, 60k, 80k |
| definite integral; its limits (Newton-Leibniz, residues, convergence) | 20 s; 3 s, 2 s, 0.6 s | 10M; 1.5M, 1M, 300k |
| limit (Gruntz gets at least a quarter) | 8 s | 8M |
| `d_solve` | 5 s | 2.5M |
| decision service (tiers after the first) | 3 s | 1.5M |
| `simplify` call, `full_simplify` call, search of one term | 4 s, 5 s, 2 s | 2M, 2.5M, 1M |
| polynomial move of `simplify` | 0.25 s | 16k (the only budget that ran out in the sample: successes up to 12.7k) |
| closed-form sums (units of polynomial work, 16k steps each) | 20 s, 5 s | 160k, 40k |

Parts of a parallel run (`d_solve`'s cascades) get the allowance left at the split, counted on their own task; the
search of `simplify` reads its allowance between levels on its own task.

The work of the tower methods of integration was counted too low at first (a nested integrand of 40 functions spent
13 s in `risch_integrate` for 330k units): the allocation of every number, gcds and the merges of sparse polynomials
were not reported. With them it reports about 200 units per ms, and the budgets of those methods use that rate.

## Round 45: walks over shared terms

Terms are hash-consed, so a term is a graph: `nest(x => [x, x], 1, 60)` is 61 distinct terms and a tree of 2^60
leaves. A walker that recurses into every argument without remembering what it has seen takes time exponential in
the depth and, without a poll of the stop flag, cannot be interrupted. `walk` makes visiting the graph the default:

- `Walk`: the `vec![t]` + `pop` stack of a predicate or collector, handing out each item once (items may carry a
  context, `(T, bool)`); the first 48 pops are not remembered, so small terms cost no set.
- `fold`, `Memo`, `cached`: bottom-up folds and recursive predicates with a memo keyed by the term (`cached` is the
  step of a recursive `-> bool` walker: `f(t) = cached(memo, t, |memo| body)` with the recursion passing `memo` on).
- `any`, `free_of`, `symbols`, `leaf_count`, `tree_size`, `depth`, `distinct_count`, `map`.

Counts over the tree (sizes, leaf counts) stay the same numbers, saturating at `u64::MAX`.

What moved onto it (each a mechanical change: the same answer, each distinct subterm visited once):
- 117 stack walkers that are predicates or collectors with a duplicate test (free-of and contains tests, symbol
  and head collection, pattern variables, step, jump and piecewise detection) in 50 files of 25 crates;
- 32 recursive predicates (`-> bool`) memoized with `cached`, 8 recursive collectors with a set of terms seen, and
  the recursive rewrites `shift::reduce` (sums), `reduce::regions` (solve), `boolean::improve` (simplify),
  `tidy_cond_with` (calculus properties) with a memo of results;
- the size measures of `solve` (`size`, `not_larger`) as memoized folds; the groundness test of a `fn`'s catch-all
  (every call of a library function with no matching clause walked its arguments as a tree);
- equation lists flattened by `d_solve` and read by `solve`/`eliminate` stop at 65536 equations (a shared list stands
  for exponentially many; past the bound the call stays unevaluated or reports the number);
- labels and messages of graphics and plots print with `Printer::brief` (at most 4 KiB) instead of the whole text.

Already graph walks before (left as they were): about 95 walkers with their own set or memo (the evaluator's
substitution and rewriting, the derivative, list measures, the simplifier's `rewrite`, polynomial reading).

Left (the catalogue test `mc-engine/tests/round45_shared_terms.rs` lists them in `LEFT`): recursive transformations
of the algebra, calculus and solving crates (about 260 recursive functions over terms returning `Res`, `Option` or a
term; the scan is `recursive functions whose body calls themselves and walks args`), stack walkers whose visits are
capped (capped work, not exponential), collectors that keep multiplicity, and functions that do exponential work on
purpose with a meter that does not reach the stop flag (layout of nested grids, Hyper on large recurrences).
