# Package N2: `mc-quad` (`n_integrate`)

Status: merged (branch `lib/r6-n2`, commits 5d0a151..11a78b2, from main 6376b35). L6 passes at all three sizes
(compiled); L6h50 and L6h500 agree with their references; the new rigorous case L6h500r passes (its ball contains
the reference). On the branch: full workspace suite passed (170 test binaries, 0 failed; 324 s, peak 2.76 GB); clippy
clean for mc-quad. Dependencies: `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-numfn`, `mc-numeric`.

## Modules

| module | contents |
|---|---|
| `rules` | Gauss-Kronrod rules computed once per process: Gauss nodes by Newton; Kronrod nodes as zeros of the Stieltjes polynomial (Legendre-basis coefficients from a closed form for triple-product integrals); Kronrod weights from a symmetric exactness system. G7-K15 and G30-K61 |
| `gk` | adaptive Gauss-Kronrod, global error heap (QUADPACK `qag`); error `resasc*min(1,(200|K-G|/resasc)^1.5)`, at least `50*eps*resabs`; both halves of a split in one `eval_flat` call (30 or 122 points); subdivision and depth limits; running sums recomputed every 256 steps |
| `de` | double-exponential rules at machine precision (f64 and complex): tanh-sinh on [a, b] with end distances computed directly, exp-sinh on half-lines, sinh-sinh on the line; levels halve h; Bailey's error estimate |
| `machine` | pieces of a range; infinite pieces by `x = a + t/(1-t)`, `x = b - t/(1-t)`, `x = t/(1-t^2)` |
| `nested` | single and multiple integrals at machine precision; inner bounds compiled functions of the outer variables |
| `hp` | tanh-sinh family on balls under `digits:`; integrand on the interpreted backend, one speculative group per level |
| `rigorous` | guaranteed enclosures: Clenshaw-Curtis on balls, bounds from a complex-ball evaluation on a box containing a Bernstein ellipse |
| `options`, `natives` | options, argument forms, method choice, results, failures |

## API

- `FUNCTIONS`: `n_integrate`; `install(k, h, stop)`.
- `n_integrate(f, [x, a, b])` (reals, `±inf`, numeric expressions; b < a gives the negative), `[x, a, s1, ..., b]`
  splits at the listed points, further ranges `[y, c(x), d(x)]`, ... for multiple integrals.
- Options: `digits`, `accuracy_goal`, `precision_goal`, `max_subdivisions` (alias `max_steps`, default 8192),
  `max_recursion` (default 50), `method: auto | gauss_kronrod | double_exponential`, `rigorous: true | false`,
  `compile: auto | true | false` (as in N1; absent: `MC_NUMFN` applies).
- Failures: `failure(not_converged, n_integrate, estimate, error)`, `failure(not_numeric, f, [x -> p, ...])`,
  `failure(argument, n_integrate, k | option)`, `failure(unknown_option, name)`,
  `failure(not_verified, n_integrate, estimate)`. A call without ranges stays unevaluated.

## Decisions

- **N2.1 Precision tiers.** Default machine precision through `mc_numfn::compile_expr` (Auto policy, interpreter for
  refused or declined calls). A complex value (a real point not numeric but its complex value finite) reruns the
  whole integral on complex machine numbers (interpreted: gap 4). `digits: d`: balls through the interpreted backend
  (NUM.3, NUM.4).
- **N2.2 Rules are computed, not typed tables**; exactness tested, qk15 checked against QUADPACK to 2e-16. Weights
  rescaled to sum exactly 2 in f64, so constants integrate exactly (`n_integrate(1, [x, 0, 7])` is `7.0`).
- **N2.3 Heap strategy**: QUADPACK `qag` with the error formula above.
- **N2.4 Rule escalation.** A 15-point interval with `|K-G| > 1e-3*resasc` is re-evaluated with G30-K61 instead of
  being split, and its descendants keep that rule: with bisection alone L6 at 10^4 (about 3000 oscillations) needs
  about 2e9 evaluations; escalation cuts that about 4x.
- **N2.5 Infinite ranges**: rational maps for Gauss-Kronrod, exp-sinh / sinh-sinh for double-exponential; signed
  `x'(t)`, so reversed pieces give the negative.
- **N2.6 Goals.** Machine precision: `precision_goal` about 8 digits (half the machine digits), `accuracy_goal`
  `inf`: the customary choice for integrals (it deviates from the shared convention of half the working precision
  for both). The error goal is never below `200*eps*∫|f|`, so heavily cancelling integrals converge at what machine
  arithmetic can reach.
- **N2.7 Automatic method choice.** Not numeric at a finite end (`1/sqrt(x)`, `log(x)`, `sin(x)/x` at 0):
  double-exponential first, else Gauss-Kronrod first; if the first does not converge the other runs and the
  converged or better estimate is kept. In a double-exponential tail a value that stops being numeric ends the range
  (`x^3 exp(-x)` overflowing far out).
- **N2.8 Multiple integrals**: nested adaptive integration, x outermost, at most 6 ranges; inner integrals aim 10x
  tighter; each inner error estimate, weighted by the rule, joins the outer interval's error.
- **N2.9 `digits: d`**: working precision d + 10 digits, default precision goal d + 3; the ball's radius is the error
  estimate (heuristic, not a bound) rounded to d + 3 digits, so at least d correct digits print unless the value lies
  within 10^-(d+3) of a rounding boundary (avoids N0 gap 2).
- **N2.10 `rigorous: true`**: Clenshaw-Curtis bound `|I - I_n| <= 4M(1 + 1/(n(n+2))) rho^-n/(rho - 1) |r|` (from
  `|a_k| <= 2M rho^-k`, positive weights summing to 2, `|I(T_k)| <= 2/(k^2 - 1)`); M from one complex-ball evaluation
  of f on a rectangle containing the scaled ellipse; analyticity proven only for bodies of add, mul, pow, exp, log,
  sqrt, the trigonometric and hyperbolic functions and their inverses (the complex-ball versions give no value at
  branch cuts and poles, N0C.2); nodes, weights and values all balls; 7 values of rho and cached node counts per
  interval, else bisection (depth <= 40, at most 4000 intervals); uncertain end balls add `sup|f| * radius`; without
  `digits:` rigorous works at 16 digits; no enclosure: `failure(not_verified, n_integrate, estimate)`.

## Edits outside the crate

`mc-engine` (dependency, `library_names`, `install`), `mc-lsp` (dependency, `FUNCTIONS`), `Cargo.lock`; L6, L6h50,
L6h500 notes no longer say "expected to fail"; new `bench/cases/L/L6h500r.mc` (500-digit rigorous enclosure, against
`ref/L6h-500.txt`).

## Tests (18)

- `rules` unit tests (3): Gauss exactness, Kronrod exactness to 3n+1 with positive weights, QUADPACK values.
- `tests/machine.rs` (7): 16 classic cases against exact values (`1/sqrt(x)`, `log(x)`, `x^-0.9`, `exp(-x^2)` on the
  line, half-lines, `1/(1+x^2)` split at 0, `sin(x)/x` on [0, 100], the peak `1/((x-0.3)^2+1e-8)`, `log|x-0.3|`
  split at 0.3, `exp(-x)cos(1000x)`, reversed ranges, `sqrt(1-x^2)`); methods agree; goals and limits; complex
  integrands (also `sqrt(x-0.5)`, complex on part of the range); multiple integrals (triangle, square, disc, cube,
  2-D Gaussian); compiled, interpreted and default bitwise equal; 17 failure and argument forms, divergent integrals.
- `tests/digits.rs` (4): 50 and 500 digits plain and rigorous against the references; 30 digits for an endpoint
  singularity, a half-line and the line; rigorous e - 1, ∫sin on [0, pi], pi/4, the 16-digit default; a complex
  integrand on balls; rigorous refusals (abs, a kink, an infinite range, a singular end).
- `tests/session.rs` (3): 9 cases identical at 1 and 4 threads and under the stress collector; interrupts
  (`digits: 20000`, a 2-D interpreted kink) stop well within 10 s; compiled vs interpreted timing.
- One doc test.

## Measurements (mc-bench, guarded, memo off, busy shared machine)

| case | compiled (default) | interpreted |
|---|---|---|
| L6 10^2 | 31.9 ms pass | 367 ms pass (11x) |
| L6 10^3 | 476 ms pass | 46.0 s pass (97x) |
| L6 10^4 | 25.4 s pass | memory limit |
| L6h50 | 51 ms pass | |
| L6h500 | 5.57 s pass (500 of 503 printed digits agree) | |
| L6h500r (rigorous) | 10.7 s pass (contains the reference) | |

L6 values differ from the references by about 2e-15 / 1.6e-14 / 2.5e-13. At 10^4 each call compiles afresh
(about 0.25 ms x 10^4); N1's compile cache should remove that.

## Known gaps

1. **Memory on the interpreted backend**: interpreted evaluation allocates heap terms per evaluation and nothing
   collects them until the statement ends (NativeCx has no safepoint for natives): interpreted L6 10^3 peaked at
   2.2 GB, 10^4 hit the limit; a 3-D interpreted kink integral hit 3 GB in 26 s. Compiled integrands are unaffected.
   Needed from mc-eval/mc-core: a GC safepoint or arena reset for long-running natives.
2. Multiple integrals are machine precision only (`digits:`/`rigorous:` with several ranges: argument failure).
3. `method: gauss_kronrod` is machine only (no Kronrod nodes on balls): with `digits:` an argument failure.
4. Complex integrands always run interpreted (N4b's compiled complex numbers not yet used).
5. `rigorous:` needs a finite range, analytic heads only and an integrand numeric at the ends; otherwise
   `not_verified` with the tanh-sinh estimate.
6. Machine precision limits: square-root end zeros (`sqrt(1-x^2)`) pick Gauss-Kronrod and reach only about 3e-11;
   Gauss-Kronrod alone on `1/sqrt(x)` stops at the depth limit (the automatic choice uses double-exponential);
   highly oscillatory integrands can alias.
7. Vector (list) integrands are not supported (`failure(argument, n_integrate, 1)`).
8. Under `digits:` the radius is Bailey's heuristic estimate, not a bound: use `rigorous:` for a guarantee.
