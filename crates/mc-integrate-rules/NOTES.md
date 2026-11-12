# Item 14C: `mc-integrate-rules` (the integration rule table; `mc-rules` until round 34)

Round 18. Roadmap phase 14, item 14C: a rule-based integrator for the forms the tower
engine (14A) does not reach. Dependencies `mc-api`, `mc-arith`, `mc-core`, `mc-eval` (the matcher), `mc-integrate`
(the engine, for exponential integrands a substitution leads to), `rustc-hash`. Hooked into
`mc-calculus/src/integrate/portfolio.rs` after the engine and before the elementary rules, when the engine found
nothing; every answer is checked there by differentiation.

## Source and licence

The rules are translated from Rubi, Rich's rule-based integrator (A. D. Rich, P. Scheibe, N. Abbasi,
*Rule-based integration: An extensive system of symbolic integration rules*, JOSS 3(32), 2018;
github.com/RuleBasedIntegration/Rubi), which is published under the MIT licence (checked: its `LICENSE` file, MIT,
copyright 2018 Rule-Based-Integration Organization). MIT is GPL-3.0-compatible; the notice is reproduced in
`packaging/licenses/embedded-data.txt`, which `tools/notices.ps1` copies into `THIRD-PARTY-NOTICES.txt`. The rules
are rewritten in the notation below, never in their original syntax; each file and section heading names the Rubi
section it comes from. The reduction formulas themselves are classical (Gradshteyn and Ryzhik 2.5-2.8, 2.41-2.42;
Abramowitz and Stegun 15.3 for the hypergeometric forms). Rubi's test suite (MIT) was used privately as the
benchmark (below); the tests in this crate are written here.

## What was built (`src/`, `rules/`)

- `rules/*.rules`: 162 rules in nine files (sines and cosines; tangents, secants and their reciprocals;
  powers of a linear form times sines and cosines; hyperbolic powers; inverse trigonometric and inverse hyperbolic
  functions; binomials `x^m (a + b x^n)^p`; logarithms of sines and cosines; exponentials times sines,
  polylogarithms, the Hurwitz zeta function and polygamma). One rule per line (continuations indented):
  `int(pattern) := right side when condition`, with v2's pattern variables (`a_`), optional arguments whose default
  is the unit of their place (`a_.`: 1 in a product and as an exponent, 0 in a sum, v2's `optional(a_, 1)`), `x`
  the variable of integration, `subst(int(u), g)` for a substitution (inside `int(u)`, `x` is the new variable).
- `syntax`: the parser of that notation into owned nodes (no heap terms), checked when the table is built (unknown
  functions, predicates or variables are build errors; a unit test builds the table).
- `table`: the table, parsed once per process (`OnceLock`), and its dispatch: a bit per function (43 functions);
  a rule is a candidate only when every function its left side names occurs in the integrand (in a subterm that
  depends on `x`). The distinct masks are grouped, so finding the candidates is one pass over a few dozen masks.
  `run_kinds` marks the variables whose every occurrence is an argument of a sum or a product.
- `run`: the interpreter. `int(u)`: evaluate `u`; memo (and cycle guard); free of `x`; sums term by term;
  constant factors out; fractional powers of products normalised (`(b sec u)^(1/2) = F sec(u)^(1/2)` with `F`
  piecewise constant, the rule set's `FracPart` rules); the caller's quick methods (base table, rational
  integrator, one function of a linear argument); the candidate rules in table order, each lowered once per call
  to the evaluator's pattern (`NativeCx::compile_pattern`) with the condition as a native condition wrapping the
  whole left side, so the matcher backtracks into the next match when the condition fails; the right side built by
  canonical arithmetic, its `int(...)` recursed into; then the substitutions of `methods`; then, below the top, the
  engine with a short budget for exponential-logarithmic integrands. Bounded by depth (24), rule applications
  (400), a deadline (1.5 s), the evaluator's fuel and interrupt.
- `methods`: Bioche's substitutions (`t = cos θ`, `sin θ`, `tan θ`, else `tan(θ/2)`) for rational functions of the
  six trigonometric functions of one linear argument; the substitution `a + b x = t^q` for powers of one linear form
  with rational exponents; tangents, secants and their hyperbolic forms rewritten in sines and cosines; products
  with a sum factor and polynomials times sines and cosines expanded (multiple angles first); the product
  logarithm substitution (written, switched off, see the gaps).
- `cond`, `util`: the predicates of conditions (`free`, `integer`, `rational`, `half_integer`, `odd`, `even`,
  `eq`, `ne`, `pos`, `neg`, `number`, comparisons decided only on numbers or terms of evident sign, as in the
  source rule set) and the term helpers (canonical replacement, free-of, sizes, a plain printer for traces).

## Public API

- `mc_rules::integrate(cx, f, x, Limits, fallback) -> Res<Outcome>`: the answer (unchecked), the rules applied
  (file, line, section) and counters (`Stats`: integrands, candidates, matched, dispatch and matching time).
- `mc_rules::table::{table, bit, Table, Rule}`; `mc_rules::syntax` for tools.
- Environment, for development only: `MC_RULES_TRACE` prints every `int` and rule applied, `MC_RULES_STATS` the
  counters of every call.
- No new user function.

## Decisions

- **14C.1 Rules as text in v2's pattern notation, matched by the evaluator's matcher.** Not as definitions of a
  symbol (they would be loaded into every session and dispatched by the definition index, which files patterns
  with optional arguments below the top), and not as hand-written Rust matchers (thousands of them). The table is
  parsed once per process into owned nodes and lowered to patterns per call, only for the candidates.
- **14C.2 Optional runs for coefficients.** In the source notation `b_. x^n_` means "the rest of the product, or
  1". A single v2 variable takes one argument under an associative head (semantics 6.2), so variables whose every
  occurrence is an argument of a product (a sum) are lowered to optional runs, `optional(b__, 1)` (r18c14.1 in
  `mc-eval`): `-x^-2/c^2` then matches with `b = -c^-2`.
- **14C.3 Conditions are decided natively and conservatively** (a comparison holds only when it is decided, `ne`
  holds unless the difference is zero): a rule may apply where its formula has a removable problem; the portfolio's
  check catches it, and the elementary rules run next.
- **14C.4 The rules run only when the engine found nothing**, before the elementary rules, and the portfolio
  keeps the established elementary form when both answer (smallest by `mc-simplify`'s cost, ties to the later).
  So the engine's and the elementary forms of the documentation stay as they were.
- **14C.5 The caller's quick methods are tried on every integrand below the top** (not on the top one, where the
  portfolio already tried them): the rules reduce trigonometric and inverse-function integrands to rational and
  algebraic ones, which the existing integrators finish.

## Checks and speed

- Every answer is checked by differentiation before it is returned (the portfolio), so a rule with a wrong right
  side cannot give a wrong answer.
- Dispatch is measured and bounded: over the benchmark sample, a call of the rule table takes 0.1-3 ms, of which
  dispatch is 1-7 microseconds and matching 30-300 microseconds; the rest is the algebra of the right sides. The
  checking of the answer (numerical evaluation of special functions to 30 digits) dominates.
- Bioche's substitutions and the linear-radical substitution answer every rational function of the trigonometric
  functions of one linear argument, with symbolic coefficients, without a rule per form.

## Benchmark (private; Rubi's test suite, MIT, outside the repository)

A sample of 1274 integrals (up to 28 evenly spaced per section of the suite, translated to v2 syntax by a script
outside the repository), each run through `integrate` with `timing`, 10 per process under a 3 GB guard. Solved = an answer
without `integrate(` (every answer is checked by the portfolio).

| run | solved | of 1274 | lost |
|---|---|---|---|
| main (683308d) | 307 | 24.1% | |
| first rule files | 348 | 27.3% | 0 |
| + by parts, special functions, inverse substitutions, expansion | 385 | 30.2% | 0 |
| + optional runs (r18c14.1) | 399 | 31.3% | 0 |
| + piecewise constant factors, hyperbolic Bioche | 454 | 35.6% | 0 |

Largest gains by section: trigonometric 4.x 61 → 118 of 196, hyperbolic 6.x 62 → 102 of 196, inverse
trigonometric 5.x 20 → 43 of 168, polylogarithms 0 → 11 of 28. Summed `timing` per integral, measured with two
benchmark processes and a build running on the machine: integrals both versions solve 182 s → 298 s (the rule
table does not run for them, since the engine or the quick table answers first: this is load), integrals neither
solves 177 s → 264 s (0.11 s more each: rules, substitutions and the short engine runs that fail), the 147 gained
39 s → 134 s (mostly the 30-digit check of hypergeometric and elliptic answers).

## Known gaps

- The product logarithm substitution is off: checking its answers evaluates `product_log` with more than about
  1000 bits, where `mc-special`'s ball evaluation (`more.rs`, `ProductLog::ball`) computes a radius of 0 from an
  underflowed `f64` and the process aborts with an allocation of 2^60 bytes (`n(product_log(3/7), 3000)`). To be
  switched on (`run::PRODUCT_LOG`) when that is fixed.
- Most of Rubi's rules for products of two linear forms in sines (`(a + b sin)^m (c + d sin)^n` with fractional
  exponents), for `(a + b tan)^n` with `a^2 + b^2 = 0`, for trinomials and for `x^m trig(a + b x^n)` are not
  ported; integer powers of those are covered by Bioche's substitutions.
- Answers are in the source rule set's forms (reduction formulas: powers of the functions), not in multiple
  angles; the elementary form is kept when both answer.
