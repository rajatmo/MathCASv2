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
