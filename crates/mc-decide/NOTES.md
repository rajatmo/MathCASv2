# Item F3: `mc-decide` (decisions and conditions)

Round 16. One decision service for sign, zero, interval, realness and integrality
questions under the assumption context, used by `refine`, `simplify`'s assumption path, the predicates
(`is_positive`, ...), `power_expand`, `solve`'s conditions, and the sign helpers of `limit` and `integrate`;
conditional results (`conditional_expression`); a condition accumulator for 14D and 11C. Dependencies: `mc-api`,
`mc-arith`, `mc-core`, `mc-eval`, `mc-numeric`, `mc-poly` (public API only); dev `mc-engine`, `mc-bench`.

## What was built

| module | contents |
|---|---|
| `facts` | the first tier, moved from `mc-simplify`: kinds and ranges from the structure of an expression met with the facts about its parts, ball refinement of closed parts; new: integer-valued functions of integers (`euler_phi`, `factorial`, `mod`, `binomial`, ...) and `gamma` of a positive argument |
| `basis` | the equational facts as a degrevlex Groebner basis (internal native `decide$basis`, memoised per set of facts by the evaluator) and the normal form of a candidate over Q |
| `quad` | real polynomials of degree at most two decided exactly by an LDL^T decomposition of the homogenised matrix |
| `sturm` | univariate polynomials over Q decided on an interval: Yun's square-free decomposition and Sturm sequences |
| `decide` | `Decider` (the tiers in order, the memo, the deadline), `Sign`, the `Reals` hook (one-variable `reduce`; several variables later), `sign_in_context` |
| `cond` | `conditional_expression`: its native, the lifting of conditions through arithmetic, comparisons, connectives, functions and operators, condition simplification; `Conditions`, the per-call accumulator |
| `build` | `Bld`, the builder of the service (a subset of `mc-simplify`'s `B`, which this crate does not define) |

## Public API

- `Decider::new(cx, stop, facts)`, `Decider::of_context(cx, stop)`, `Decider::with(cx, bld, syms, facts)`; then
  `info`, `sign`, `compare(cx, "less", l, r)`, `truth(cx, condition)`, `element`, `in_interval`, `is_integer`,
  `is_real`, `normal_form`, `integer_terms`; `set_reals` replaces the last tier's hook (phase 11B).
- `sign_in_context(cx, t)`: one question, for callers without a decider.
- `Conditions`: `record`, `simplified`, `finish(cx, stop, value)` (the value, `undefined`, or one
  `conditional_expression`).
- `install(k, h, stop)` after `mc-solve`; `FUNCTIONS`: `conditional_expression`.

## Decisions

- **F3.1** The tiers run cheapest first and stop at the first decided answer: structure and facts (with ball
  refinement of closed parts, so a value whose enclosure excludes zero is decided before anything is proved), the
  Groebner normal form, ball enclosures over the bounds, quadratic forms, root isolation, `reduce`. Every answer is a
  proof (exact arithmetic) or certified by balls (outward-rounded enclosures); machine reals inside an expression
  disable the ball-enclosure tier, since their arithmetic is not certified. "Unknown" is the answer when no tier
  decides: there is no generic-value guessing (`sign(x^2 + 1)` is unknown for a complex `x`).
- **F3.2** Tiers 2-5 run only at the top of a question (not for every subterm the first tier walks), only when the
  context has facts, and only before a deadline of 3 s per decider. Their answers are written back into the first
  tier's cache, so a later question about a larger expression uses them.
- **F3.3** The equational facts are compiled once per set of facts: `decide$basis(list(d1, ...))` is an ordinary
  evaluation, so the evaluator's memo keeps the basis for as long as the context (a hash-consed term) is asked
  about. Candidates are reduced in a ring that adds their kernels to the basis's (a Groebner basis stays one when
  variables are added, since degrevlex restricts to the old order). An inconsistent basis (`[1]`) is not used.
- **F3.4** The ball-enclosure tier replaces each variable by a ball enclosing its interval (the facts must bound it
  on both sides) and evaluates the expression in ball arithmetic through the evaluator; for one variable the
  interval is also cut into eight pieces, each decided separately, which decides `exp(x) - 3x > 0` on (0, 1/2)
  where the whole interval's enclosure overlaps zero.
- **F3.5** Root isolation needs rational bounds (a machine-real bound is its exact dyadic value); a bound that is a
  closed constant (`pi/2`) is left to the other tiers. Odd-multiplicity parts with a root inside the interval leave
  the sign unknown; even-multiplicity parts with a root, or roots at included endpoints, give a non-strict sign.
- **F3.6** The `reduce` tier asks the evaluator for `reduce(facts on x and t <= 0, x, reals)` (and the other three
  cases); an empty answer proves the sign. Only order and equality facts in the one variable are passed (dropping
  facts is sound: fewer hypotheses). The variable must be known real: `x^2 < 1` alone does not bound `x`, since
  `x = i/2` satisfies it.
- **F3.7** `mc-simplify`'s `facts.rs` stays as an adapter with its old API (`Facts`, `Syms`, `Info`, ...), so
  `refine`, `simplify`, `power_expand` and the predicates use the service unchanged. `Facts` now holds its builder by
  `Rc` (the decider owns it) and its symbols by `Arc`.
- **F3.8** `conditional_expression` lifts through the natives of arithmetic, comparisons, `not`, the elementary and
  special functions (by wrapping their natives, so that `add` and `mul`, which canonicalise before any rule, see
  the conditions first; a cheap symbol-mask test keeps the common path unchanged) and through `and` and `or` (whose
  evaluated forms are lifted). A function without a native gets an argument-head rule instead. Operators
  (`simplify`, `refine`, `expand`, `factor`, `d`, `integrate`, `limit`, `n`, ...) evaluate on the value inside
  `assuming(condition, ...)` and keep the condition; `normal` gives the value; `reduce` conjoins the condition.
  Lists keep conditional elements, as the documentation pages show.
- **F3.9** A conjunction is simplified when a conditional result is built: conjuncts implied by the others are
  dropped, a refuted one makes the result `undefined`, and two or more order relations in one variable are put in
  `reduce`'s interval form (`0 < x and x < 1`); a single relation is kept as written. The native does not read the
  assumption context; `Conditions::finish` does (a condition the context implies is dropped).
- **F3.10** `limit`'s power rule asks the service for the sign of a symbolic exponent (it asked only for numbers):
  `assuming(a > 0, integrate(x^a, [x, 0, 1]))` is now `1/(1 + a)`.

- **F3.11** The variables of a polynomial order fact are real: `x^2 + y^2 <= 1` speaks of real `x` and `y`, as
  inequalities in the documentation always do. A non-polynomial order fact (`exp(x) < 1`) makes only its sides
  real.
- **F3.12** Closed terms are refined to at most 256 bits in the decider (`mc-numeric`'s comparisons go to 8,192):
  an exact zero not written as 0 is never decided by refinement, and `limit` asks about such terms often (a zero
  of 8,192 bits cost 130 ms, which made `discrete_limit` pages slow). Callers fall back to their old path for what
  stays unknown. The symbols of the service are interned once per heap and thread.
- **F3.13** A quadratic on the set of one quadratic fact is decided by the S-procedure: `e - lambda g` checked
  for a fixed set of positive rationals `lambda`, exactly; a strict fact makes a nonnegative certificate strict.
  `abs(u)` against a nonnegative number is compared as `u^2` against its square.

## Guarantees

- Sign decisions are proved or certified: a machine evaluation, which can be fooled by cancellation, never
  decides (balls or exact arithmetic only), and an undecided question stays unknown.
- Quadratic forms in several real variables are decided exactly (`a^2 - a b + b^2 >= 0`, `x^2 - x y + y^2 + 1 > 0`)
  without a cylindrical decomposition.
- The basis of the equational facts is memoised per context by the evaluator instead of per call.

## Tests

`tests/decide.rs` (the tiers, unknowns, the refine rules of this crate, `reduce` domains, conditional results,
the Rust API through test natives), unit tests of `sturm` and `quad`, the doc test of `lib.rs`, and the page
`docs/functions/conditional_expression.mcn`.

## Round 19

- `facts.rs`, `exp`: a numeric lower or upper bound `c` of the argument gives the bound `exp(c)` of `exp(u)`,
  outward as a dyadic rational with denominator `2^30` from the machine exponential widened by a relative `10^-6`
  (the machine error is below an ulp), for `|c| <= 600`. `refine(exp(x) < 7, x < 3/2)` is `true`; `exp(x) < 4`
  stays undecided (`exp(3/2) = 4.48`).

## Round 29
- Domains `nonnegative_rationals` and `nonpositive_rationals` added to `DOMAINS` (kind rational, sign >= 0 / <= 0), so
  `element`, `refine` and `simplify` decide them like the other signed domains.

## Round 36

- `normal` removes `conditional_expression` nested inside lists, rules and sums (`strip_all`), not only a top-level one.
- `refine(conditional_expression(v, c), facts)` is `refine(v, facts)` when refine(c, facts) is true (round 36).
