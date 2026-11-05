# Package 2B: `mc-simplify` (simplification)

Status: branch `simplify-2b`, first round. `simplify` and five transforms work, with tests, including numeric checks
at random points. `assuming` (semantics 13) is not done yet. Dependencies: `mc-api`, `mc-arith`, `mc-core`, `mc-eval`;
dev `mc-engine`, `mc-bench`. Registration lines in `mc-engine` and `mc-lsp` only. The round also changed two other
crates (below).

## What was built

| module | contents |
|---|---|
| `term` | `B`, the builder (canonical `add`/`mul`/`pow` through `mc-arith`, plain calls otherwise, views of sums, products, powers, exponentials, complex numbers); `rewrite`, a bottom-up walk with a DAG memo and one fuel unit per node; `fixed` (rewrite and evaluate to a fixed point); `apply` (call another package's function through the evaluator); `leaf_count` |
| `simplify` | the search behind `simplify` |
| `trig` | `trig_to_exp`, `exp_to_trig`, `trig_expand`, `trig_reduce`; the moves `pythagoras`, `to_sin_cos`, `odd_even`/`normal_signs`, `exp_combine` |
| `power` | `power_expand` |

## Public API

`install(k, h, stop)`, installed after `mc-poly`. `FUNCTIONS`: `simplify`, `trig_expand`, `trig_reduce`, `trig_to_exp`,
`exp_to_trig`, `power_expand`. Each takes one argument and walks lists like any other term. Any other number of
arguments leaves the call unevaluated.

## Decisions

- **2B.1** `simplify` is a search that minimises a cost, the plan's "rule set plus a cost measure" (00-plan, layer 2).
  - Cost: the leaf count. Every symbol, number and head counts 1; a rational counts 3.
  - Search: breadth first, at most `DEPTH = 3` moves deep, keeping the `BEAM = 6` cheapest new forms per level.
  - Duplicates are recognised by handle, since terms are hash-consed.
  - Ties keep the order in which forms were found, with the input first, so `simplify` never trades a form for one
    of equal size. Example: `sin(x)^6 + cos(x)^6` (11 leaves) stays, because `5/8 + 3/8 cos(4x)` has 12.
  - An e-graph (equality saturation, Willsey et al., POPL 2021) can replace the search later behind the same
    function.
- **2B.2** `simplify` uses only moves that are valid for every value of the variables:
  - package K's `together`, `cancel`, `factor` and `expand`;
  - `trig_reduce`;
  - the Pythagorean substitutions (`cos^2 = 1 - sin^2` and the reverse; `cosh^2 = 1 + sinh^2`), each followed by
    `expand` or `together`;
  - tan, cot, sec, csc and tanh written as sines and cosines, followed by `together`;
  - `exp_to_trig`;
  - `exp(a)^k exp(b) = exp(k a + b)`;
  - the sign convention of 2B.4.

  Moves are applied to the whole term. The arguments of non-arithmetic functions, and list elements, are simplified
  first (inside out).
- **2B.3** `power_expand` applies `(ab)^c = a^c b^c`, `(a^b)^c = a^(bc)`, `log(ab) = log a + log b`,
  `log(a^b) = b log a` and `log(exp u) = u`. These hold only for positive reals, so `power_expand` is a formal tool
  that `simplify` never calls. With `assuming` it can become a conditional move.
- **2B.4** Sign convention for sines and cosines of sums: of `f(u)` and `f(-u)`, keep the one whose angle comes first
  in the canonical order (`sin(y - x)` becomes `-sin(x - y)`). The evaluator already normalises an angle that is a
  product with a negative coefficient and leaves sums alone, so the two conventions never disagree. The negation is
  distributed over the sum; `-1*(x - y)` would be flipped straight back by the evaluator.
- **2B.5** `trig_expand` uses the addition theorems on sums (first term against the rest) and on integer multiples
  `n x` (as `x + (n-1) x`), to a fixed point, then expands.
- **2B.6** `trig_reduce` expands, then repeats one product-to-sum step per product (`sin a sin c`, `cos a cos c`,
  `sin a cos c`, with powers as repeated factors) plus the sign convention, re-expanding after each round, until
  nothing changes.
- **2B.7** `exp_to_trig` reads the real and imaginary parts of the exponent off its terms' complex coefficients;
  symbols count as real, so the split is formal. Then it applies Euler's formula and expands. A real exponent is left
  alone: no `exp(x) = cosh(x) + sinh(x)`.

## Changes outside the crate

- **`mc-eval`, curried calls (2A known gap 1).** A native can opt in with `Native::curried()` (default `false`).
  Calls `f(a..)(b..)...` whose innermost head is its symbol are then dispatched to it, with the whole call as `t`.
  `mc-calculus` registers a curried `derivative` native that resolves `derivative(n...)(known)(args)`, so `sin'(x)`
  is `cos(x)` outside `d` (decision 2A.7). Unknown functions keep the operator.
- **`mc-poly`, stop safety.** Once the stop flag is raised, `mc-arith` returns arbitrary values by design
  (`num/mod.rs`). Six exact divisions in `mc-poly` (`ratfun`, `factor` ×3, `functions`, `resultant`) used `expect`,
  so interrupting `together` in the middle of a gcd panicked (`a gcd divides`). The new `work::exact` turns a failed
  division into `Halt::Stopped` when the flag is up and still panics otherwise. `simplify` was the first caller to
  interrupt these kernels; the interruption test here covers it.

## Tests (6 integration + 1 doctest; `mc-calculus` +1)

- `rational_forms`, `trigonometric_and_exponential_forms`, `transforms`: exact printed results.
- `every_function_keeps_the_value`: each function on a 10-expression corpus, at 3 random rational points each, agrees
  with its input to about 20 digits, checked with balls as `n(1 + (A - B) 10^20, 30)`.
- `simplify_is_idempotent`.
- `interrupt_stops_a_long_simplification`: interruptions at 5 to 400 ms into simplifying `expand((1+x+y+z)^40)` stop
  promptly and leave the session usable.
- `mc-calculus/tests/prime.rs::known_functions_resolve_outside_d`.

## Measurements (REPL wall time including about 15 ms of start-up, best of 5, release)

| input | time |
|---|---|
| `simplify(sin(x)^2 + cos(x)^2)`, `simplify(d(sin(x)/x, x))`, contracting `sin(x + y)` | 13-18 ms |
| `simplify(sin(x)^6 + cos(x)^6)` | 29 ms |
| `simplify` of `expand((1+x+y+z)^20)` (1,771 terms) / `^30` (5,456 terms), giving the power | 0.26 s / 1.6 s |
| `trig_reduce(sin(x)^20)` / `trig_expand(sin(20*x))` | 43 / 24 ms |

## Known gaps and next steps

1. **`assuming` is not done** (semantics 13). It is needed for `sqrt(x^2) = x` with `x > 0` inside `simplify`, and
   for the conditional use of `power_expand`'s laws.
2. Cost is the leaf count only. Some forms that people find simpler lose by one or two leaves (`5/8 + 3/8 cos(4x)`).
   A weighted cost, or an option to pass one, is open.
3. The search is shallow (depth 3, beam 6) and works on the whole term. A move that helps only one subterm of a large
   sum is found only if it also lowers the whole term's cost. An e-graph would remove that limitation.
4. `trig_reduce` and `trig_expand` cover sin and cos. sinh and cosh are covered by `trig_expand` only.
5. `sec`, `csc` and `cot` have no numeric evaluation in `mc-numeric` (`n(sec(1))` stays unevaluated), so the numeric
   corpus uses `1/cos`.
6. Printing: complex coefficients print as `complex(0, -1/2)*(...)`, and `3 + 2im` prints as `complex(3, 2)`; that is
   the printer, not this crate.

## Round 7 package A: assumptions (`assuming`, `refine`, assumption-aware `simplify`)

Status: merged (branch `lib/r7-a`, commits 8a27dae, 4488736, f5ae588, from main 216fdea). On the branch: full suite
779 passed, 0 failed; clippy clean. `assuming(x > 0, simplify(sqrt(x^2)))` is `x`; without facts it stays
`(x^2)^(1/2)`; no memo reuse across contexts; 1 and 4 threads identical. The evaluator side is
`crates/mc-eval/src/assume.rs` (see `crates/mc-eval/NOTES.md`, section "Round 7 package A").

### What was built

- **Evaluator** (`mc-eval/src/assume.rs`): the context is one term `list(facts...)`, flattened (`and` and lists
  split, `true` dropped), sorted by the canonical order and deduplicated (equal fact sets are one handle); held in the
  task context as `Cx::assume: Option<T>`, lexically scoped (restored when the body returns, also on abort), copied
  into speculative tasks. The held form `assuming(facts, body)` (facts evaluated, body held; declares
  `uses_assumptions`; wrong arity declines). A call of a head declaring `uses_assumptions`, or a native reading the
  context, sets the read-set bit of the symbol `assuming`, which travels like any read (up through evaluations,
  through memo hits, back from speculative tasks). Memo: an evaluation in a nonempty context whose read set has the
  bit is stored under `assume$memo(t, context)`, everything else under `t`; inside a nonempty context an entry under
  `t` is accepted only if its read set lacks the bit; outside contexts plain entries are always valid; a bit
  collision only sends more entries to the keyed form (conservative).
- **mc-simplify** `facts.rs`: normalised facts and a decision engine giving each expression a kind (integer,
  rational, real, unknown) and a range (bounds, strictness, nonzero) from its structure (sums add bounds, numeric
  coefficients scale, products multiply signs; rules for powers, `exp`, `log`, `abs`, `sign`, `sin`/`cos`,
  `sinh`/`tanh`/`atan`, `cosh`, `floor`/`ceiling`/`round`), met with the facts about the expression (`e op c`, and
  `a op b` when `a - b` is `±e`); closed numeric expressions decided by mc-numeric's refinement; depth 48, fuel ticks.
  `refine.rs`: the rules, bottom up and evaluated, to a fixed point (at most 16 rounds). `simplify` and
  `power_expand` read the context.

### Refinements (all tested in `refinement_table`, each with a "no facts, unchanged" counterpart)

| form | becomes | when |
|---|---|---|
| `(x^a)^c` | `x^(a c)` | `x >= 0` and `a`, `c` real; or `a` a real number in (-1, 1] |
| `(x^a)^c` | `abs(x)^(a c)` | `x` real, `a` an even integer, `c` real (`sqrt(x^2)` → `abs(x)`) |
| `sqrt(sum)` | factored first, then the rules above | `sqrt(x^2+2x+1)` → `1 + x` for `x > -1` |
| `(p q)^c` | `p^c q^c` for factors `p >= 0` | `c` not an integer |
| `abs(x)^n` | `x^n` | `x` real, `n` even |
| `(-1)^k` | `1` / `-1`; `(-1)^k0` (even integer parts dropped) | `k` even / odd; `(-1)^(3n+4m+2)` → `(-1)^n` |
| `abs(x)`, `sign(x)` | `x`/`-x`; `1`/`-1`/`0` | sign known; known-sign factors pulled out of products |
| `log(x^a)` | `a log x` | `x > 0` and `a` real; or `a` in (-1, 1] |
| `log(p q)` | `Σ log p_i + log(rest)` | factors `p_i > 0` |
| `log(exp(u))` | `u` | `u` real |
| `sin(kπ)`, `tan(kπ)`; `cos(kπ)` | `0`; `(-1)^k` | `k` an integer |
| `cos((k+½)π)`, `sin((k+½)π)` | `0`, `(-1)^k` | `k` an integer |
| `sin(v+kπ)`, `cos(v+kπ)`, `tan(v+kπ)` | `(-1)^k sin v`, `(-1)^k cos v`, `tan v` | `k` an integer |
| comparisons | `true` / `false` | decided by the facts |

### Decisions

- **A.1** The context is a canonical `list` term in the evaluation task, inherited by speculative tasks, restored
  after the body; facts are evaluated in the outer context.
- **A.2** The dependency is the read-set bit of `assuming`, set at dispatch of a `uses_assumptions` head (`with_head`)
  and by `NativeCx::assumptions()` / `eval_assuming`; only evaluations that depended on a nonempty context are keyed
  by it, so context-free work is shared across contexts (tested).
- **A.3** Facts: `<`, `<=`, `>`, `>=`, `==`, `!=` between any expressions; `element(x, D)`, `element([x, y], D)` with
  D in `integers`, `rationals`, `reals`, `complexes`, `positive_`/`negative_`/`nonnegative_`/`nonpositive_integers`,
  `positive_`/`negative_rationals`, `positive_`/`negative_`/`nonnegative_`/`nonpositive_reals`; `and`s and lists. An
  order fact makes both sides real; unknown facts are ignored. `element(x, reals)` because `in` is not an infix
  operator yet.
- **A.4** Assumption-aware decision functions registered in mc-simplify replace the registrations of `is_positive`,
  `is_negative`, `is_real` (mc-numeric) and `is_integer` (mc-lists), keeping their attributes plus
  `uses_assumptions`; outside contexts the answers are unchanged (same `numeval` calls; `is_integer` structural,
  `false` for unknowns per 1A.1); inside, `is_integer` is `true` when the facts prove it. New `is_nonnegative`.
  Undecided calls stay unevaluated. (Chosen over a hook in mc-numeric to edit no crate not owned.)
- **A.5** `refine(e, facts)` is `refine(e)` under the extended context; it runs without facts too (some rules need
  none: `log(x^(1/2))` → `(1/2) log x`).
- **A.6** `simplify` in a nonempty context refines its input, adds `Refine` as a search move, refines the result;
  with an empty context it is unchanged.
- **A.7** `power_expand` in a context refines first, then applies its formal laws except to bases proved negative;
  outside contexts unchanged.
- **A.8** `eval_assuming` without `mc_eval::assume::install` gives `failure(not_installed, assuming)`;
  mc-simplify's `install` calls it.

### API additions

mc-eval: `pub mod assume` (`install(&mut Kernel, &Heap)`, `ASSUMING`), `NativeCx::assumptions() -> &'h [T<'h>]`,
`NativeCx::eval_assuming(&[T], T) -> Res<T>`, `Attributes::uses_assumptions` (bit `1 << 7`), decorator
`@uses_assumptions`. mc-simplify: modules `facts` (`Facts`, `Info`, `Kind`, `Range`, `Bound`, `Syms`, `DOMAINS`) and
`refine`; `simplify::simplify` and `power::power_expand` take `Option<&mut Facts>`; `FUNCTIONS` gain `assuming`,
`refine`, `is_nonnegative`, `element` and the 14 domain names; depends on mc-numeric.

mc-eval edits: `assume.rs` (new); `lib.rs` (`pub mod assume`); `defs.rs` (the flag and its bit); `eval.rs`
(`Cx::assume`, `Kernel::assume`); `machine.rs` (`enter`: validity closure, `plain_memo_ok`, second lookup under the
context key; `finish`: `memo_key(...)`; `with_head`: the read bit); `syntax.rs` (the decorator).

### Tests

mc-eval `tests/assume.rs` (3): context flattened, sorted, deduplicated, nested merge and restore, body held; counting
natives show no reuse across contexts (also through a rule), reuse within one context, context-free results shared;
`map` over 300 elements at 1 thread lazy, 4 lazy, 4 always-split, 4 always-split with stress collection.
mc-simplify `tests/assume.rs` (7): the 68-row refinement table; about 30 fact and context cases; decision functions
unchanged outside and aware inside (also a rule whose condition calls `is_positive`); no reuse across contexts
(`fn f(u) = simplify(sqrt(u^2))` gives `(x^2)^(1/2)`, `x`, `-x`, `abs(x)`, `(x^2)^(1/2)`); 1 vs 4 threads,
always-split, memo on and off; refinements keep the value at random points satisfying the facts; interrupt.

### Known gaps

1. Package G's safe point must root `Cx::assume` and treat keyed memo terms like any memo entry.
2. `x in D` needs an infix `in` in mc-syntax lowered to `element(x, D)`.
3. `limit`, `integrate`, `solve` can use the context by registering `uses_assumptions` and reading
   `cx.assumptions()` or calling the decision functions (`limit(exp(a*x), x -> inf)` with `a > 0`).
4. Decision engine: bounds are numbers or come from signs (`x > pi` only proves `x > 0`); no transitive reasoning
   between facts beyond the difference match; contradictory facts not detected.
5. `refine` factors a sum under a fractional power on every pass (costly on huge polynomials; fuel-bounded).
6. Inside contexts `is_integer(e)` can be `true` for non-number terms (deliberate extension of 1A.1).
7. The printer shows `-x*y` as `-1*x*y`.
