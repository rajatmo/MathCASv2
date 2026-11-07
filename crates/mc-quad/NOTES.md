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

## Package quad-1 (round 13): multidimensional integration, parallel blocks, goals

Plan 10 (`docs/design/10-integration.md`), steps 1 and 2: sections 2.1, 2.2, 2.3 and most of 2.5. Branch `lib/quad-1`
from main 9391d0f. New dependencies of this crate: `mc-random` (Philox streams) and `rayon 1.12` (MIT/Apache-2.0,
already in the lock).

### Modules

| module | contents |
|---|---|
| `domain` | `Domain`: the ranges as a piecewise map of the unit cube (each range's K pieces on equal u-intervals; affine, or the rational maps of `machine` for infinite pieces; inner ends evaluated for a whole block); `Sampler`: `f(x(u)) w(u)` for a block of points in one `eval_flat` call, weight-0 points skipped, non-finite values reported with their point |
| `cubature` | Genz–Malik degree-7/5 rule (`Rule`, exactness unit test to degree 7 in 1 to 7 dimensions), region heap, fourth-difference split axis, rounds of regions evaluated together |
| `qmc` | rank-1 lattice sequence (radical-inverse order, random shifts, tent transform) and Gray-code Sobol points with hash-based Owen scrambling; 10 replicates, doubling |
| `qmc_tables` | the generating vector (64 dimensions) and the Sobol polynomials and initial direction numbers (64 dimensions), with the `#[ignore]` searches that produced them |
| `mc` | plain Monte Carlo (pass doubling, pairwise moments) and VEGAS+ (map refinement, hypercube allocation, weighted average with χ²) |
| `multi` | `Kind`, automatic choice, default goals, `Estimate`, messages, `around` results |

### Options added

`method: nested | cubature | vegas | lattice | sobol | monte_carlo` (besides `auto`, `gauss_kronrod`,
`double_exponential`); `max_points: n` (every machine method; nested rules: subdivisions capped at n/30);
`points: n` (VEGAS per iteration, otherwise in all), `iterations: k` (averaged VEGAS iterations), `seed: n`
(default 0), `error_estimate: true` (`around(value, error)` from any machine method). Failures:
`failure(argument, n_integrate, max_points | points | iterations | seed | error_estimate | method)` (a sampling method
with `digits:` or `rigorous:` is `method`); more than 64 ranges: `failure(argument, n_integrate, 66)`; more ranges
than a method takes (nested 6, cubature 15): argument `max + 2`. `MC_QUAD_TRACE=<file>` appends one line per
integration (method, variables, evaluations, estimate, error estimate, converged), a measurement aid.

### Decisions

- **quad-1.1 Blocks and threads.** The multidimensional methods evaluate the integrand through `mc_numfn::par_expr`
  (compiled code split over the session's threads, the interpreter for declined points or `compile: false`), in
  blocks of at most 2^15 points. `Par` now takes its code from N1's family cache (`Par::cached`, lifted constants
  appended per point), so a loop of integrals compiles once. Points are generated in parallel blocks keyed by
  their position, and every sum is added in point order on the calling thread: results are identical at 1, 2 and
  4 threads (tested). The one-dimensional rules are unchanged (their batches of 30 or 122 points stay sequential;
  results bit-identical to before).
- **quad-1.2 Method choice.** One range: as before (N2.7). 2 to 7 ranges: cubature; if it meets a point where the
  integrand is not real (complex values), or misses its goal in 2 or 3 variables, the nested rules run too and the
  smaller error estimate is kept. Above 7: the lattice rule when the integrand is built from the analytic heads of
  `rigorous` only, VEGAS otherwise; a lattice run that misses its goal is followed by VEGAS with the same budget,
  and the smaller error wins. `method: nested` gives the previous behaviour for multiple integrals.
- **quad-1.3 Cubature rounds.** Each round pops the regions of largest error until their errors add up to what must
  be removed to meet the goal (at least one; at most 2^14 points of children; never past `max_points`), and
  evaluates all children in one call. Rounds depend on values only. Regions narrower than 1e-13 along the split
  axis are retired.
- **quad-1.4 Default goals.** Cubature: half the machine digits (as one-dimensional integration) up to 3 variables,
  6 digits from 4; lattice and Sobol 4 digits; VEGAS and plain Monte Carlo 3. `accuracy_goal` none. Default
  `max_points` 10^7. The goal is never below 200 eps times the rule applied to |f| (N2.6).
- **quad-1.5 Cubature error estimate**: |I7 - I5| unscaled. Scaling it by the convergence of the degree-3, -5 and -7
  values (as null-rule estimates do) halved the work on smooth integrands but let the continuous and discontinuous
  Genz families converge falsely by factors up to 10^5; reliability first.
- **quad-1.6 Tables from our own searches.** Lattice: component by component, each component the best of 256 random
  odd candidates below 2^19 for P_2 with weights (1 + (j - 1)/8)^-2, summed in logarithm over N = 2^10 ... 2^20
  (an embedded sequence). Sobol: primitive polynomials by degree and coefficients (dimension 2 is x + 1), initial
  direction numbers minimising the sum of squared t-values of the 2-D projections with all earlier dimensions over
  the first 2^1 ... 2^20 points (all candidates up to 128, else 128 random). Both searches run in seconds
  (`cargo test -p mc-quad --release --lib qmc_tables::search -- --ignored`); a test checks the tables' form, the
  (0, m, 2)-net of dimensions 1 and 2, and the one-dimensional stratification of every dimension.
- **quad-1.7 Quasi-Monte Carlo error.** Mean of 10 independent randomisations (shifts, scramblings from `seed`),
  error the standard error of that mean; 2^10 points per replicate, doubling until the goal or `max_points`.
- **quad-1.8 Random numbers.** Philox (`mc_random::Stream`): block b (2048 points) of pass or iteration i uses
  `from_key(seed).split(i).split(b)`. The default seed is 0, not the session generator: `n_integrate` stays free of
  effects (memoisable, usable in speculative evaluation) and repeatable; `seed:` gives other draws.
- **quad-1.9 Results.** Sampling methods give `around(value, error)` (one standard error), with a message when the
  goal was not met or (VEGAS) when χ² per degree of freedom exceeds 1 + 3 sqrt(2/dof). Cubature gives a number; a
  missed goal prints a message (evaluations, error, the variables at the centre of the region with the largest
  error) and returns the estimate if its error is below 10^-3 of it, else `failure(not_converged, ...)` (divergent
  integrals). `error_estimate: true` gives `around` for every machine method.
- **quad-1.10 VEGAS details.** 5 training iterations, then averaged ones (at least 3) until the goal; points per
  iteration default 10^4 (at most an eighth of `max_points`), doubled when more than 10 further iterations would be
  needed; increments per axis clamp(n/10, 50, 1000); hypercubes ns^d with ns = floor((n/4)^(1/d)); smoothing
  (1, 6, 1)/8, compression α = 1/2, allocation β = 3/4.
- **quad-1.11 (mc-jit) Sums and products of up to 64 machine reals are sorted inline.** The compiled code sorted at
  most 6 operands with hand-made networks and called the arithmetic (about 2 µs) for more, so `x1 + ... + x8` cost
  25 times `x1 + ... + x6`. Larger sizes now use Batcher's odd-even merge network for the next power of two with
  the padding comparators dropped (order keys are a total order, so any network gives the arithmetic's order).

### Tests (new: 10 integration, 6 unit; changed: 2)

- `tests/multi.rs` (9): cubature against closed forms in 2 to 7 variables; domains (inner bounds, infinite ranges,
  split points, a corner singularity); every sampling method in 3 and 12 variables within 6 errors, and the lattice
  error below plain Monte Carlo's / 20 at equal points; VEGAS on a narrow 4-D peak and on an 8-D product of end
  singularities; automatic choice (cubature, lattice, VEGAS, nested on request); seeds; options and messages
  (budgets, `error_estimate`, goals, fixed points and iterations); 10 failure forms, the divergent 2-D integral,
  complex fallback; 7 cases identical at 1, 2 and 4 threads.
- `tests/bench.rs` (1, ignored): the benchmark below.
- Unit: Genz–Malik exactness; Sobol and lattice one-dimensional stratification; nested scrambling; VEGAS map
  refinement; table form (plus the two ignored searches).
- `tests/machine.rs`: the 2-D Gaussian over the plane is now by cubature, compared to 1e-10 (was 1e-12; the goal is
  8 digits).
- mc-jit `tests/agree.rs` `sorting_networks_sort`: generated networks sort every 0-1 input up to 20 values and
  random ones up to 64.

### Benchmark (`tests/bench.rs`, release, 3 threads, guarded; busy shared machine)

Genz families with sum a_i = h/d^e (h = 110, 600, 600, 100, 150, 100; e = 1.5, 2, 2, 1, 2, 2), irregular weights,
shifts in [0.1, 0.9]. "Before" is `method: nested`, the only multidimensional method on main (at most 6 ranges),
stopped after 60 s. Errors are relative; "estimated" is the error estimate over the value; the method column
shows every method that ran.

| case | method | time | evaluations | error | estimated | before: time | evaluations | error |
|---|---|---|---|---|---|---|---|---|
| oscillatory 2d | cubature | 0.115 s | 1459399 | 4.5e-16 | 1.1e-8 | 0.002 s | 6320 | 1.5e-14 |
| product peak 2d | cubature | 0.015 s | 151181 | 6.0e-12 | 1.1e-8 | 0.031 s | 320356 | 4.1e-16 |
| corner peak 2d | cubature | 0.004 s | 41259 | 6.4e-13 | 1.1e-8 | 0.003 s | 25413 | 1.5e-16 |
| gaussian 2d | cubature | 0.006 s | 37757 | 5.0e-11 | 1.1e-8 | 0.005 s | 40000 | 1.7e-16 |
| continuous 2d | cubature | 0.006 s | 59177 | 1.5e-6 | 1.1e-8 | 0.184 s | 2754012 | 6.3e-11 |
| discontinuous 2d | cubature | 0.002 s | 8687 | 2.6e-3 | 1.1e-8 | 0.340 s | 5088482 | 1.8e-10 |
| oscillatory 4d | cubature | 0.188 s | 2627301 | 4.1e-11 | 1.0e-6 | 0.009 s | 83521 | 1.8e-15 |
| product peak 4d | cubature | 1.033 s | 9968787 | 4.8e-9 | 1.0e-6 | > 60 s | - | - |
| corner peak 4d | cubature | 0.111 s | 1743117 | 5.3e-9 | 1.0e-6 | 0.571 s | 5842817 | 1.8e-16 |
| gaussian 4d | cubature | 0.257 s | 3240849 | 1.3e-8 | 1.0e-6 | 6.64 s | 50777064 | 3.9e-16 |
| continuous 4d | cubature | 0.040 s | 695115 | 2.5e-7 | 1.0e-6 | > 60 s | - | - |
| discontinuous 4d | cubature | 0.002 s | 7011 | 4.4e-4 | 9.9e-7 | > 60 s | - | - |
| oscillatory 8d | lattice | 0.005 s | 40960 | 7.0e-5 | 8.1e-5 | not available | | |
| product peak 8d | lattice | 0.014 s | 81920 | 1.5e-6 | 3.6e-5 | not available | | |
| corner peak 8d | lattice+vegas | 0.574 s | 5501370 | 4.5e-4 | 8.6e-4 | not available | | |
| gaussian 8d | lattice | 0.023 s | 163840 | 4.3e-5 | 3.7e-5 | not available | | |
| continuous 8d | vegas | 0.016 s | 79129 | 1.4e-4 | 5.0e-4 | not available | | |
| discontinuous 8d | vegas | 0.019 s | 99178 | 1.0e-3 | 8.8e-4 | not available | | |
| oscillatory 12d | lattice | 0.003 s | 10240 | 4.1e-5 | 4.0e-5 | not available | | |
| product peak 12d | lattice | 0.005 s | 10240 | 3.3e-7 | 3.5e-6 | not available | | |
| corner peak 12d | lattice+vegas | 0.883 s | 5622880 | 1.3e-3 | 8.7e-4 | not available | | |
| gaussian 12d | lattice | 0.005 s | 10240 | 1.5e-6 | 9.5e-5 | not available | | |
| continuous 12d | vegas | 0.022 s | 80000 | 3.5e-4 | 5.5e-4 | not available | | |
| discontinuous 12d | vegas | 0.022 s | 80000 | 1.9e-3 | 8.2e-4 | not available | | |
| corner singularity 2d | cubature+nested | 0.446 s | 10004434 | 2.2e-16 | 4.4e-15 | 0.001 s | 4439 | 2.2e-16 |
| log(x + y) 2d | cubature | 0.002 s | 15997 | 5.5e-12 | 1.1e-8 | 0.005 s | 105715 | 3.1e-12 |
| 1/r on the cube 3d | cubature | 0.012 s | 199947 | 5.6e-11 | 1.1e-8 | 0.436 s | 4982335 | 1.9e-16 |
| narrow peak 4d | cubature | 0.459 s | 5871399 | 3.4e-8 | 1.0e-6 | > 60 s | - | - |
| end singularities 8d | lattice+vegas | 0.437 s | 5941391 | 5.6e-4 | 9.0e-4 | not available | | |
| peak 12d | lattice+vegas | 1.678 s | 7862880 | 5.4e-4 | 8.6e-4 | not available | | |

Reading: in 4 variables the product peak, the kinked family and the narrow peak went from more than a minute to
about a second or less, the Gaussian from 6.6 s to 0.26 s; in 8 and 12 variables everything is new. The
statistical estimates track the achieved errors (within a factor of about 2.5). Cubature's estimate is pessimistic
by orders of magnitude on smooth integrands (the price of quad-1.5), and in 2 variables the nested Gauss–Kronrod
rules are cheaper on smooth integrands (oscillatory 2d: 6320 evaluations against 1.46 million, both in
milliseconds). Before quad-1.11 the lattice runs of the 12-term integrands took 20 to 40 s (the compiled sums
called the arithmetic).

### Known gaps

1. **Jumps and kinks inside a region fool cubature** (discontinuous 2d/4d: error 10^-3 against an estimate of
   10^-8; continuous 2d: 10^-6): a discontinuity between a region's outer points and its face is invisible to the
   rule. Give known jumps as split points (`[x, 0, u, 1]`), or use `method: nested` or `vegas`.
2. Cubature's |I7 - I5| is very pessimistic for smooth integrands; a two-level null-rule estimate as in DCUHRE
   would cut the work several-fold without quad-1.5's false convergence, but needs care.
3. The multidimensional methods are real only; complex integrands go to the nested rules (at most 6 ranges); with
   an explicitly named sampling method a complex value is `failure(not_numeric, ...)`.
4. The tables cover 64 dimensions, as does the range limit (`failure(argument, n_integrate, 66)` beyond).
5. The automatic smooth/rough test is syntactic (analytic heads); an analytic integrand with a near singularity
   inside the cube goes to the lattice rule first (VEGAS then runs when the goal is missed, doubling the work).
6. The one-dimensional rules still evaluate in small batches, one interval pair at a time (blocks for one variable
   would change the order of subdivision and so the bits of every result); Gauss–Legendre on balls (2.5) not done.
7. `max_points` for the nested rules caps subdivisions per level, not the total evaluations of a multiple integral.
