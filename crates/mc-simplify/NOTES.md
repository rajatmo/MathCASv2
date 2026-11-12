# Item 2B: `mc-simplify` (simplification)

Status: first round. `simplify` and five transforms work, with tests, including numeric checks
at random points. `assuming` (semantics 13) is not done yet. Dependencies: `mc-api`, `mc-arith`, `mc-core`, `mc-eval`;
dev `mc-engine`, `mc-bench`. Registration lines in `mc-engine` and `mc-lsp` only. The round also changed two other
crates (below).

## What was built

| module | contents |
|---|---|
| `term` | `B`, the builder (canonical `add`/`mul`/`pow` through `mc-arith`, plain calls otherwise, views of sums, products, powers, exponentials, complex numbers); `rewrite`, a bottom-up walk with a DAG memo and one fuel unit per node; `fixed` (rewrite and evaluate to a fixed point); `apply` (call another crate's function through the evaluator); (`leaf_count` moved to `cost` in round 15) |
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
  - item K's `together`, `cancel`, `factor` and `expand`;
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
2. (Round 15: the measure is now digit-weighted with tie-breaks and a `complexity_function:` option.) Cost is the leaf count only. Some forms that people find simpler lose by one or two leaves (`5/8 + 3/8 cos(4x)`).
   A weighted cost, or an option to pass one, is open.
3. The search is shallow (depth 3, beam 6) and works on the whole term. A move that helps only one subterm of a large
   sum is found only if it also lowers the whole term's cost. An e-graph would remove that limitation.
4. `trig_reduce` and `trig_expand` cover sin and cos. sinh and cosh are covered by `trig_expand` only.

## Round 7 item A: assumptions (`assuming`, `refine`, assumption-aware `simplify`)

Status: done. At completion: full suite
779 passed, 0 failed; clippy clean. `assuming(x > 0, simplify(sqrt(x^2)))` is `x`; without facts it stays
`(x^2)^(1/2)`; no memo reuse across contexts; 1 and 4 threads identical. The evaluator side is
`crates/mc-eval/src/assume.rs` (see `crates/mc-eval/NOTES.md`, section "Round 7 item A").

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

1. Item G's safe point must root `Cx::assume` and treat keyed memo terms like any memo entry.
2. `x in D` needs an infix `in` in mc-syntax lowered to `element(x, D)`.
3. `limit`, `integrate`, `solve` can use the context by registering `uses_assumptions` and reading
   `cx.assumptions()` or calling the decision functions (`limit(exp(a*x), x -> inf)` with `a > 0`).
4. Decision engine: bounds are numbers or come from signs (`x > pi` only proves `x > 0`); no transitive reasoning
   between facts beyond the difference match; contradictory facts not detected.
5. `refine` factors a sum under a fractional power on every pass (costly on huge polynomials; fuel-bounded).
6. Inside contexts `is_integer(e)` can be `true` for non-number terms (deliberate extension of 1A.1).

## Round 15: the complexity measure

The measure `simplify` minimises is new (`src/cost.rs`), cached over
the hash-consed subterms of one call, and replaceable by the user (`complexity_function:`).

### What was built

- `cost.rs`: `Cost` (the measure of one call with its caches), `Score` (what candidates are compared by), `Measure`
  (`Default` or `User(f)`), `int_size`, `big_size`. `term::leaf_count` is gone; `simplify`, its search and its
  preparation (the expanded-exponent check) all price through one `Cost`.
- `simplify(e, complexity_function: f)` (and `automatic` for the default); passed through when `simplify` re-enters
  itself under extra facts (`simplify(e, facts, complexity_function: f)`).
- A new search move, `LogCombine` (decision s-cost.6).

### Decisions

- **s-cost.1 Default measure.** A leaf count in which integers weigh by their digits: 0 counts 1, a nonzero integer
  its decimal digits plus 1 if negative; a rational or complex number 1 plus its parts; other atoms 1; a call its
  head plus its arguments. Consequences: `-x` (`mul(-1, x)`) costs 4, `x - y` 6; long coefficients
  make expanded forms expensive.
  - Big integers: the digit count is read off the bit length, `floor((bits - 1) log10 2) + 1` with log10 2 as a
    64-bit fixed-point constant, made exact up to 128 bits by one comparison with a power of ten. Beyond 128 bits it
    can be one short; no big arithmetic and no formatting, O(1) after finding the top limb.
- **s-cost.2 Cache.** Terms are hash-consed, so the size of a subterm is a map entry keyed by its handle, kept for
  the whole `simplify` call (every level of the search, every argument searched inside out, the preparation and
  the final choice). A candidate shares most subterms with the input and the other candidates, so pricing it walks
  only its new nodes: the cost is incremental without any bookkeeping in the moves. Handles cannot move during a
  native call (a collection needs the heap exclusively), so the map stays valid. A user measure's values are cached
  per whole candidate.
- **s-cost.3 Tie-breaks.** Scores compare by size, then fewer powers with a negative numeric exponent, then fewer
  distinct function heads (a 64-bit set with a bit per head from an FNV hash of its name, so the same in every
  session whatever order symbols were made in; `add`, `mul`, `pow`, `list`, `complex` not counted). Beyond them the
  order found decides (the input first). A user measure has no tie-breaks: its value alone decides, ties keep the
  order found. A value that is not a real number (after `n`) counts as infinitely complex, so such a form never
  replaces the input. The growth cap of the search (`GROWTH`, `SMALL`) always uses the default size, so a user
  measure cannot make the search blow up.
- **s-cost.4 Measurements** (below): the measure's share of `simplify` is small at these sizes; the cache makes
  pricing a large candidate about 11 times cheaper.
- **s-cost.5 Assumptions (refines A.6).** Inside `assuming`, the search still starts from the refined input and the
  result is refined again, but that refined result is the answer only if neither the unrefined result, nor the
  (prepared) input, nor the input with only its `abs` and `sign` of known-sign arguments decided, is strictly
  simpler (the last because `refine` also rewrites inside: `abs((q p^2)^(1/4))` for q > 0 refines to
  `q^(1/4) (-p)^(1/2)`, 14, while dropping the `abs` gives `(q p^2)^(1/4)`, 8). So `simplify(abs(x), x < 0)` is `abs(x)` (2 leaves, against 4 for `-x`),
  while an equal-size refinement is still taken (`simplify(log(x^p), x > 0 and element(p, reals))`
  is `p*log(x)`, 4 against 4). With `complexity_function: u => string_length(to_string(u))` the first is `-x`.
  `refine` still always rewrites.
- **s-cost.6 LogCombine.** The terms `k log(q)` of a sum, k a positive integer and q a positive rational, become
  one `log` of the product of the `q^k` (valid for every value: the arguments are positive numbers); not tried when
  the product would pass 4,096 bits. `log(2) + log(3)` is `log(6)`; `100 log(2)` stays under the default measure
  (the 31-digit argument costs more), and is `log(1267650600228229401496703205376)` with
  `complexity_function: leaf_count`. Negative multiples are left alone (`log(8) - log(2)` stays).

### Tests

- Unit (`cost.rs`, 5): integer sizes (0, signs, `i64::MAX`/`MIN`); big integers exact up to 128 bits including
  10^20 - 1, 10^20, 10^38 - 1, 10^38, and exact-or-one-short against the exact count for 2^64..2^999; atoms,
  rationals, complex numbers, calls, shared subterms, `x - y`; incremental pricing (a candidate differing in one of
  199 terms walks at most 3 new nodes, a repeated term none); tie-breaks.
- Integration (`tests/simplify.rs::complexity_measure_and_option`, 17 cases): digit weighting, sign weighting,
  `LogCombine`, `complexity_function:` with `leaf_count`, lambdas, a rule-defined measure, a non-numeric measure,
  `automatic`, inside and outside `assuming`. `tests/element.rs`: `simplify(abs(x), x < 0)` is now `abs(x)` (with
  `refine(abs(x), x < 0)` = `-x` and `simplify(abs(x) + x, x < 0)` = `0` added).
- `tests/cost_bench.rs` (2, ignored): the timings below.

### Measurements

`cargo test --release -p mc-simplify --test cost_bench -- --ignored --nocapture`, each input in a fresh session,
best of 3, before and after:

| input | before ms | after ms |
|---|---|---|
| `simplify(sin(x)^2 + cos(x)^2)` | 0.43 | 0.40 |
| `simplify(sin(x)^6 + cos(x)^6)` | 3.51 | 3.57 |
| `simplify(d(sin(x)/x, x))` | 1.08 | 1.07 |
| `simplify(sin(x)*cos(y) + cos(x)*sin(y))` | 0.50 | 0.49 |
| `simplify((x^3 - y^3)/(x - y) - (x^2 + x*y + y^2))` | 1.12 | 1.16 |
| `simplify(1/(x + 1) + 1/(x - 1) + 1/(x + 2) + 1/(x - 2))` | 3.86 | 3.87 |
| `simplify(expand((1 + x + y + z)^12))` | 35.37 | 35.20 |
| `simplify(expand((1 + x + y + z)^20))` | 218.53 | 208.94 |
| `simplify(expand((3 + 1234567*x)^15))` | 1.58 | 1.47 |
| `simplify(expand((x - 1)^8*(x + 2)^5)/(x^2 + x - 2)^3)` | 8.94 | 8.41 |
| `simplify(sum(k*x^k, [k, 1, 30])/(x - 1))` | 14.58 | 13.90 |
| `simplify(trig_expand(sin(10*x)*cos(7*x)))` | 82.54 | 78.88 |
| `simplify(tan(x)^4 + 2*tan(x)^2 + 1)` | 3.48 | 3.34 |
| `simplify(d(atan(x/sqrt(1 - x^2)), x))` | 2.24 | 2.08 |
| `simplify(d(d(log((1 + x)/(1 - x))*exp(x)/(1 + x^2), x), x))` | 52.92 | 51.01 |
| `simplify(expand((1 + x + y + z)^30))` | 986.31 | 920.76 |
| `simplify(sum(1/(x + k), [k, 1, 12]))` | 8.76 | 7.88 |
| `simplify(trig_reduce(sin(x)^12*cos(x)^8))` | 12.54 | 10.52 |
| `simplify(expand((a + b)^10*(a - b)^10))` | 2.80 | 2.48 |
| `simplify(expand((123456789*x^2 + 987654321*y - 1)^8))` | 5.89 | 5.40 |
| `simplify(integrate(x^3*exp(x)*sin(x), x))` | 3.81 | 2.84 |
| `simplify(integrate(1/(x^4 + 1), x))` | 2.68 | 2.40 |
| `simplify(integrate(x^2/(x^3 - 2*x + 1), x))` | 1.81 | 1.65 |
| `integrate(x^5*exp(2*x)*cos(3*x), x)` | 0.75 | 0.65 |
| `integrate((x^4 + 3*x^2 + 1)/(x^5 - x + 7), x)` | 1.42 | 1.33 |
| total | 1457.5 | 1369.7 |

Results are unchanged except `1/(x + 1) + 1/(x - 1) + 1/(x + 2) + 1/(x - 2)`, now one fraction (26 against 27; the
old measure preferred the four reciprocals, 21 against 22). Times per call are 0 to 25% lower (6% in total); the
polynomial moves dominate, so the measure's own share was small at these sizes. The measure alone
(`measure_timings`): pricing the input and 42 candidates of a 1,771-term polynomial with big coefficients takes
8.78 ms with a fresh memo per candidate (the old way) and 0.80 ms with one cache per call (2,166 nodes priced
instead of about 43 x 9,000), 11 times less. `integrate` does not call `simplify`, so its times are unchanged.

### Known gaps and proposals

1. **Cross-call cache (proposal for mc-core, not done):** the size could be a field of the node, computed at
   intern time from the arguments' fields (hash-consing builds bottom-up, so it is O(arguments) once per node,
   for ever). Every `simplify` call, `leaf_count` (mc-lists) and any other cost-driven choice would then price a
   term in O(1). The per-call map here gets most of the benefit inside one call; across calls (nested `simplify`
   from `dsolve`, the evaluator's repeated calls) it starts empty.
2. The measure is not yet used by other crates that choose between forms (`integrate`'s or `solve`'s
   post-processing); `mc_simplify::cost::Cost` is public for them.
3. Distinct-function tie-break is a 64-bit set: two names hashing to one bit count once (rare, deterministic).

## Round 16: the decision service

- `facts.rs` is now an adapter over `mc-decide` (decision F3.7): the engine of the first tier (structure, facts,
  ball refinement) moved there unchanged, and every question of `refine`, `simplify`, `power_expand` and the
  predicates is answered by all of `mc-decide`'s tiers (Groebner normal form modulo the equational facts, ball
  enclosures over the bounds, quadratic forms and the S-procedure, root isolation, `reduce` for one variable).
- `refine.rs` gained rules: `log(x)` for a negative `x`; `floor`, `ceiling`, `round` of an integer; `re` and
  `im_part` of sums of real and imaginary terms; `atan(tan(u))`, `asin(sin(u))`, `acos(cos(u))` in the principal
  range (for `atan`, also by `re(u)`); `mod(a, m)` by an integer term of the facts; the exponents of `(x^a)^c` and
  `log(x^a)` shown to lie in `(-1, 1]` by the facts; `conditional_expression(v, c)` refined under `c`.

## Round 17 fixes

- **r17fix.1 Powers of one base with symbolic exponents.** The canonical arithmetic keeps a number times a power of a
  number apart (`2*2^(1 + n)` stays, by design of `mc-arith`), so `2*2^(1 + n) - 2^(2 + n)` did not cancel. New
  move `PowerSplit` (`src/powers.rs`): every `b^(k + u)` with an integer constant `k` (`|k| <= 64`) of the exponent
  and a nonzero number or symbol base becomes `b^k b^u`; the evaluator then collects like terms (`4*2^n - 4*2^n`).
  As a move of the search it is only kept when it is cheaper, so `simplify(2^(n + 1))` stays. Checking the
  solutions of recurrences by substituting them back and simplifying now gives `true`.
- **r17fix.2 Gamma ratios.** Move `GammaRatio`: in a product, a numerator `gamma(z + k)` and a denominator
  `gamma(z)` (also `factorial(u)` as `gamma(u + 1)`) with an integer difference `k` become the rising factorial
  `z (z + 1) ... (z + k - 1)` (DLMF 5.5.1 applied k times), its reciprocal for `k < 0`, `pochhammer(z, k)` past four
  factors (which the evaluator expands for integer `k`, so then the cost keeps the ratio). The difference is taken
  term by term (the canonical `-1*(1 + n)` is not distributed). Convention: these identities
  are `simplify`'s, not the canonical evaluation's (`gamma(m + 1)/gamma(m)` stays as typed); `binomial(m, 0)`,
  `binomial(m, 1)`, `binomial(m, m)`, `binomial(m, m - 1)` evaluate at once (in `mc-numtheory`).
- **r17fix.5 The `assumptions:` option replaces the context.** An option is the
  value of one setting, so `simplify(e, assumptions: a)` inside `assuming(b, ...)` uses `a` only, and `assumptions:
  true` means no facts; extending stays available by nesting `assuming`, and positional facts (`simplify(e, f)`,
  `refine(e, f)`) still extend. Implemented with `NativeCx::eval_replacing` here and in `mc-poly::opts` (the shared
  option of `integrate`, `limit`, `solve`, ...; a one-line change outside this crate).
- **More of the same moves** (`full_simplify` and `simplify`): `GammaRatio` also writes `z gamma(z)` as
  `gamma(z + 1)` (kept only when cheaper: `x gamma(x)` alone ties with `gamma(1 + x)` and stays) and
  `exp(k log_gamma(u) + v)` as `gamma(u)^k exp(v)` (valid for every `u`: `log_gamma` differs
  from `log(gamma(u))` by multiples of `2 pi i` only, so no assumption such as `re(x) > 0` is needed); a new move `TrigToExp` (the existing `trig_to_exp` transform) lets `cosh(x) - sinh(x)` become
  `exp(-x)` and `(e^x - e^-x)/sinh(x)` become 2 when that is cheaper.
- **`function_expand`**: Chebyshev polynomials of symbolic degree (DLMF 18.5.1), Catalan numbers and falling
  factorials as gamma functions (DLMF 26.5.1, 5.2.5), the product form of a multivariate `dirac_delta`, the nome
  `elliptic_nome_q(m) = exp(-pi K(1 - m)/K(m))` (DLMF 22.2.1); gamma ratios with integer differences as rising
  factorials afterwards.

## Round 19: phase 12 (full_simplify, assumptions, special-function identities)

Covers `simplify`, `full_simplify`, `refine`,
`function_expand` and the related transforms.

### What was built

| module | contents |
|---|---|
| `simplify` | `Config` (mode and options of one call), the move list built from it, the search with flagged candidates and the spot check, a bottom-up subexpression search in `full_simplify`, the identities marked ALWAYS in `prepare` |
| `identities` | the priority table of guarded rules (`TABLE`, used by `function_expand` with mask `EXPAND` and by the search with `SIMPLIFY`) and the families of sums (gamma and polygamma shifts, dilogarithm reflection, arctangent sums, `I_-nu - I_nu`) |
| `trigconst` | `sin`/`cos` of `p pi/q` in radicals (`q = 2^k m`, `m` in 1, 3, 5, 15), arctangent sums by Gaussian rationals, logarithms of closed complex numbers |
| `moves` | `rationalize`, `power_collect`, `pythagoras_one`, `sqrt_parity`, `im_split`, `im_combine`, `log_primes`, `linear_equation`, `common_factor` |
| `logic` | equations proved from `for_all` axioms: oriented rewrite rules plus commutativity by sorted arguments, innermost normal forms (bounded) |
| `verify` | the ball-arithmetic spot check of a rewrite at points satisfying the facts |
| `refine` | Euler's theorem for `mod(a^(k euler_phi(m)), m)`, `fractional_part` from a fact `mod(a, 1) == c` |
| `fexpand` | the table and families, `degree` as `pi/180`, multiple angles of inverse functions, `sqrt_parity`; facts as a second argument and `assumptions:` |

`full_simplify` is new (`FUNCTIONS`, page `docs/functions/full_simplify.mcn`).

### Decisions

- **n12.1 One machinery.** `simplify` and `full_simplify` share the search; `full_simplify` adds `function_expand` as
  a move, searches every sum, product and power below the top (two moves, up to 400 leaves)
  before the whole, goes 4 moves deep with a beam of 8 (3 and 6 in `simplify`), and its whole call stops after
  5 s (`simplify` 4 s; `time_constraint:` sets it). The breadth comes from the identity table, as the roadmap asks.
- **n12.2 Every new move is spot-checked.** A candidate reached through a round-19 move carries a flag; when the
  answer is flagged, `verify::refuted` evaluates input and answer at up to 3 points (of 12 drawn) where every fact
  is `true`, in balls at 30 digits; a certainly positive `abs(a - c) - 10^-12 (1 + abs(a))` refutes it and the
  best unflagged candidate is returned. Without facts the points are complex (an identity of principal branches can
  fail on a cut, which real points hit: `Li2(z) + Li2(1 - z)` on `z > 1`); with facts they are real. Points where a
  side has no numeric value say nothing. The ALWAYS identities applied before the search are checked the same way.
- **n12.3 ALWAYS rules.** Identities that remove a special function in favour of elementary ones under a proved
  guard (`product_log(u e^u) = u` for `u >= -1`, `W(w log w) = log w` for `w >= 1/e`, `F(phi, 1) =
  log(sec + tan)` for `|phi| < pi/2`) are applied before the search whatever the cost
  (`exp(elliptic_f(x, 1))` is 4 leaves, `sec(x) + tan(x)` 5).
- **n12.4 Ties.** `GammaRatio`'s `z gamma(z) = gamma(z + 1)` is taken at equal cost when the best form so far is
  the input (so `simplify(x gamma(x))` is `gamma(1 + x)`); in `full_simplify` also an
  identity's form.
- **n12.5 `trig: false`** leaves out `PythagorasOne` only (the move that writes a constant as `sin^2 + cos^2`), so
  `simplify(sin(x)^2 + cos(x)^2, trig: false)` stays 1 as before the option was read, and `(1 + 2 cos sin)^2`
  stays with it.
- **n12.6 Options.** `transformation_functions: [automatic, f, ...]`: the user functions are extra moves (not
  spot-checked: the user's identities); without `automatic` they are the only moves. `excluded_forms: [p, ...]`:
  subterms matching a pattern (`match_q`, outermost first) are replaced by fresh symbols `excluded$k` for the
  search and put back after. `time_constraint: s` (or `inf`, capped at a day).
- **n12.7 Equational axioms** (`logic`): an axiom is oriented from the larger side to the smaller (as written on
  equal sizes), reversed when its right side has extra variables, dropped when a variable alone would be a left
  side; `f(a, b) == f(b, a)` makes `f` commutative (sorted arguments). Only a proof is reported (equal normal
  forms); the rules are not completed (Knuth-Bendix), so group-theory axioms and `f(f(a, a), b) == a` stay
  open.
- **n12.8 Trigonometric constants** take the nested radical with the fewest leaves (cos(pi/5) as `-(1/4)(-1 -
  sqrt 5)`, halvings as `sqrt(2 +- 2 cos)/2`); `p/15`
  and `p/30` by the addition theorem on the decomposition `k/6 + j/10` with the smallest `|k| + |j|`.
- **n12.9 function_expand** applies the round-7 reciprocal forms (`sec = 1/cos`, ...) only in a first pass, so a
  `csc` written by an identity (the `K_nu(-x)` form) stays.
- **n12.10 Coprimality.** The evaluator's `gcd(a, m)` of two symbols is 1 (the polynomial gcd), so the fact `gcd(a,
  m) == 1` is `true` before it reaches the context and the decision service would prove it for every `a`, `m`.
  Euler's theorem therefore asks for the fact `coprime_q(a, m)` (or a literal `gcd(...) == 1` that stayed
  unevaluated). Writing `assumptions: gcd(a, m) == 1` stays open (it needs `gcd` of
  symbols to stay unevaluated: `mc-numtheory`/`mc-poly`).
- **n12.11 Decision service** (`mc-decide/src/facts.rs`): a numeric bound `c` of `u` bounds `exp(u)` by `exp(c)`,
  outward as a dyadic rational, so `refine(exp(x) < 7, x < 3/2)` is `true`.
- **n12.12 Changed answers.** `simplify(log(8) - log(2))` is `log(4)` (LogPrimes; was left alone by s-cost.6),
  `simplify(x gamma(x))` is `gamma(1 + x)` (n12.4), `simplify(x == 1 + x)` is unchanged (a linear equation without
  variables after moving the terms is left to the evaluator).

### Checks and results (evidence: tests/full.rs)

- Every new rewrite is spot-checked in ball arithmetic before it is returned.
- `simplify(boole(e^x < 7), x < 3/2)` is `1`, decided by the facts without a `reduce` transformation.
- Machin-type sums are combined exactly with Gaussian rationals for any number of terms; the multiple of `pi` is fixed
  by a machine value whose error is far below the spacing.
- Identities valid off a branch cut (`Li2(z) + Li2(1 - z)`) are kept by `simplify` only when the check at complex
  points passes.

### Known loss

- `n(u == f)` with `f` the nested radical of `sin(pi/128)`: its machine value loses about 22 ulps to cancellation
  in `2 - sqrt(...)`, and `==` on machine reals compares every bit. Needs `mc-eval`'s machine comparison or a ball
  evaluation of radicals in `n` (round 19).

### Tests

`tests/full.rs` (8): fixes on the way (`3 + 4/(3 + sqrt 13)`, `log(64)/18 - log(2)/3`, the Laplacian of `1/r`),
new moves, options, equational axioms, assumptions, 31 function_expand identities with exact results, identities
keep the value at random points (11 inputs x 3 points, 30 digits), user transformations are not checked.

### Known gaps

1. Identities not done: hypergeometric U with half-integer parameters, the Binet form of `fibonacci` of a symbolic
   index, Legendre functions beyond order 1/2, Bessel recurrences between contiguous orders, contiguous relations of
   2F1 in general; the table has 31 rules.
2. n() of Bessel functions at negative arguments and of `airy_ai` at complex ones does not reach 30 digits, and n()
   of `zeta(1 - x)` takes about a minute, so those identities are tested exactly and the spot check says nothing.
3. Equational logic without completion (n12.7).

### Late changes (for d_solve)

- **n12.13 Equations with equal sides.** `simplify(l == r)` is `true` when the search brings `l - r` (formed term by
  term) to 0, for sides that are not lists, strings, booleans or two numbers; otherwise the equation is simplified
  side by side as before. Checking the solutions of differential equations by substituting them back and
  simplifying gives `true`.
- **n12.1 amended.** `function_expand` is a move of `full_simplify` only: `simplify(chebyshev_t(n, x),
  complexity_function: f)` keeps `chebyshev_t`, and `full_simplify` with the same measure gives `cos(n acos(x))`.
- `matrix_function` gives `(1/24) pi (3 + sqrt 3)` for `pi sqrt(3)/(12 (sqrt(3) - 1))`, the same number with a
  rational denominator; `boole(e^x < 7)` is decided under `x < 3/2` also with `transformation_functions:`.

## Round 20 (roadmap 16A)

- **The search** (`simplify.rs`): every (form, move) pair of a level is a `par_until` element (`task_step`: its
  own builder, and its own facts and measure for the four moves that consult them); the results are taken in
  the order of the loop, so seen-set, scores, beam and the deadline behave as before. A move not started before
  the deadline ends the search (the loop broke there). `Config::stop` carries the arithmetic's stop flag.
- **The spot check** (`verify.rs`): the points are drawn first (they never depended on what was found at them);
  the facts are checked in order on the calling task (cheap), and the comparisons at the points that satisfy them
  run in rounds of as many as are still needed (`par_scan`), so no point is evaluated that the sequential loop
  would not evaluate (a numerical evaluation is not cancelled at once: speculating past the needed points made
  `full_simplify` slower). `B::stop` gives tasks the stop flag.
- Measured (4 / 8 threads): `full_simplify(e^elliptic_f(x, 1), -pi/2 < x < pi/2)` 2.5x / 3.0x (its time is the
  check of the ALWAYS identity at three real points), the Catalan identity of `full_simplify(function_expand(..))`
  2.2x / 2.6x (three complex points); `tests/par.rs`: 11 calls at 1, 2, 4 and 8 threads.

## Round 22 (bug fixing)

- D21-19: `hypergeometric2f1`/`1f1` reductions are ALWAYS rules (they remove a special function);
  `exp_integral_difference` writes `E_n(x)` as `x^(n-1) Gamma(1-n, x)` in a sum that holds that gamma, so
  `E_1(x) - Gamma(0, x)` is 0 while a lone `E_1(x)` keeps its name; `has_pi_shift`: in `assuming`, the reduction of
  `sin(x + n pi)` for an integer `n` to `(-1)^n sin(x)` is taken even though it is one leaf larger.
- D21-20: a relation whose difference expands to a number is decided; bounds on one expression in `and`/`or`
  merge (`merge_bounds`). `x == x + 1` is now `false` (two test expectations changed).
- D21-21: `refine(conjugate(u))` is `u` for real `u`; `floor`/`ceiling` of a sum of integer terms and a rational
  number take the integers out.
- D21-22: `function_expand` of `bessel_j|y|i|k(n + 1/2, x)` by the three-term recurrences from the orders +-1/2
  (DLMF 10.6.1, 10.29.1), |n| up to 12.
- R20-01/02: `(x^a)^c = x^(a c)` for a positive real constant `x`; exponents of `exp` terms of a sum that agree
  after `expand` are written alike. R20-07: `is_positive`, `is_negative`, `is_nonnegative` of a non-real number
  are `false`.

## Round 22 fix-up: the numeric spot check

- `verify::compare_at` evaluated `n(abs(a - c) - 1e-12 (1 + abs(a)), 30)`. Round 22 made special functions
  evaluate at complex points, so for a true identity (`E_(1-n)(x) x^n = Gamma(n, x)`) the difference was an exact
  zero of two evaluable sides: `n` asks for 30 digits of zero and climbs to its cap (4000+ bits, each step with
  refinement inside the special functions), 25 s to minutes per point, and `time_constraint` was not honoured
  inside one evaluation. Each side is now evaluated to 30 digits on its own and the gap is taken between the two
  numbers. `simplify(exp_integral_e(1 - n, x)*x^n)` is 0.1 s again.

## Round 28

- `refine` splits on a disjunctive fact of the context (`or`): the refinement under each alternative, taken when all
  agree (`refine(abs(x^2 - 1), x <= -1 or x >= 1)` is `x^2 - 1`; `abs(x)` for `x < -1 or x > 1` stays, the two
  branches differ). The facts engine itself still holds only conjunctions of ranges. Test
  `refine_splits_on_disjunctive_assumptions`.
- `simplify` offers boolean candidates (`boolean.rs`): the `boolean_minimize` form of a propositional term over
  symbols, and the 0/1 arithmetic of `boole(a and b)`, `boole(a or b)`, `boole(not a)`, `min`/`max` of `boole`
  values (expanded), each used only if it scores simpler. `simplify(xor(x, x, y))` is `y`,
  `simplify(boole(a and b) - min(boole(a), boole(b)))` is 0.
- `trig_reduce` rewrites products and squares of `sinh`/`cosh` (the circular family first, then the hyperbolic);
  `trig_expand` collects `c sin^2 + c cos^2` and `c cosh^2 - c sinh^2` pairs; `exp_to_trig(exp(x))` for a real
  exponent is `cosh(x) + sinh(x)` (so `simplify((exp(x) - exp(-x))/2)` is `sinh(x)`). Known loss: `trig_reduce(...,
  modulus: pi)` ignores the option and now reduces `sinh(t)^2` (it should be left as it is).
- Triaged as not bugs: `power_expand` is the formal transformation (documented), so its `assumptions:` argument
  conditions (`sqrt(x y)` with args in (pi/2, pi)) are not modelled; `function_period` of `sin(10 x) + sin(15 x)`
  is `2 pi/5` (correct).

## Round 29

- `simplify` of a closed expression that still holds `golden_ratio` is retried with the constant written
  `(1 + sqrt(5))/2`; an integer or rational result replaces it (`1 + 1/golden_ratio - golden_ratio` is 0,
  `golden_ratio^2 - golden_ratio - 1` is 0). Anything else keeps the symbolic form.
- `refine`/`simplify` under `re(x) > 0`: `(x^a)^c = x^(a c)` for real `a` in [-2, 2] (the argument of `x^a` lies in
  (-pi, pi), DLMF 4.2), so `sqrt(x^2)` is `x` and `possible_zero_q(sqrt(x^2) - x, assumptions: re(x) > 0)` is true.
  Exponents beyond 2 are left (`(x^3)^(1/3)`).
- Not reached: `refine(sqrt((x + y - 2)^2), element([x, y], disk()))` (needs disk bounds in the decision
  service).

## Round 30

- **D30.7** `trig_reduce(e, modulus: p)`: the reduction, then each rational coefficient n/d replaced by
  n d^-1 mod p (a coefficient whose denominator p divides is kept), like terms combined and reduced again, the
  common integer factor of the coefficients written outside: `trig_reduce(cos(x)^50, modulus: 7)` is
  `4*(cos(48*x) + cos(50*x))`. A modulus that is no integer >= 2 (`modulus: pi`)
  returns the expression unchanged; `modulus: 0` is no modulus. Before, the option was
  ignored.

## Round 32

- `denest_symbolic`: `sqrt(a + b sqrt(c))` with symbolic a, c, positive rational b, under facts that give a > 0 and c >= 0, as `sqrt((a + d)/2) + sqrt((a - d)/2)` with `d^2 = a^2 - b^2 c` a perfect square found by `factor`; the sum is symmetric in the sign of d, and both radicands are nonnegative (their sum is a, their product b^2 c/4).
- `exp_to_trig` writes `log(s + w) - log(s - w)` (s a positive number) as `2 atanh(w/s)` (`2 i atan(y)` when `w/s = i y`) and `log(u + sqrt(u^2 + 1))` as `asinh(u)`; `power_expand` cancels `atan(tan(u))` and the other inverse pairs formally; `function_expand` writes `heaviside_pi` and `heaviside_lambda` with `heaviside_theta`; `refine` decides `heaviside_theta(a, b, ...)` (zero when an argument is negative, one when all are positive).

## Round 33

- `power_expand(e, assumptions: facts)` (an explicit option, not the surrounding `assuming` context) keeps the branch corrections of the principal branches, `src/power_exact.rs`: `(a b)^c = a^c b^c exp(2 pi i c k)`, `log(a b) = log a + log b + 2 pi i k`, `arg(a b) = arg a + arg b + 2 pi k` with `k = floor(1/2 - (arg a + arg b)/(2 pi))`; `(z^p)^q = z^(p q) exp(2 pi i q k)` and `log(z^p) = p log z + 2 pi i k` with `k = floor(1/2 - Im(p log z)/(2 pi))`; `log(exp(u))`. The integer `k` is decided from interval bounds (bounds on `arg(x)` or `im_part(x)` read from the facts, reals of known sign, numbers; ends open or closed): `sqrt(x y)` with both arguments in (pi/2, pi) is `-sqrt(x) sqrt(y)`; otherwise the floor stays. The default (no option) stays the formal law (decision 2B.3). The inverse pairs (`atan(tan x)`) are still formal. The flag reaches the inner call as the internal option `exact_branches`.

## Round 36

- boole arithmetic of nand, nor, implies, xor, equivalent and mod(sum of boole, 2), with boole(u)^k = boole(u); the boolean candidates apply element by element to lists.
- The points of the spot check are seeded by a hash of the input's structure, not its handle: the handle varied with allocation order, and so the points and the cost (full_simplify of the dilogarithm reflection 0.15 s to 3 s, a point where polylog converges slowly).

- `time_constraint:`: the first level of moves from a small term (size <= 32) runs whatever the deadline, so a very short limit still gives the identities one move away (`full_simplify(x*gamma(x), time_constraint: 0.0001)` is `gamma(1 + x)`). It was a race with the clock: the preparation of the term could use up the 0.1 ms before the first move started.
