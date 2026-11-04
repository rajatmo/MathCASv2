# MathCAS v2: numerics roadmap (root finding, integration, differential equations)

This document plans numeric solvers that are fast at machine precision through the numeric compiler (`mc-jit`) and
correct at any precision through balls (semantics 2.3). It refines phase 4 of `06-roadmap.md`. It is a plan: every
speed claim below is a hypothesis until the class L benchmarks (`02-benchmarks.md`) measure it.

## 1. Starting point (November 2026)

| Piece | State |
|---|---|
| Numeric compiler `mc-jit` (Cranelift) | Compiles functions of `Int`/`Real` parameters: arithmetic, comparisons, `if`, `while`/`for`, `exp` ... `tanh`, `sqrt`, `abs`, `min`, `max`; inlines calls; falls back to the interpreter on domain errors (decision 1b.3) |
| Not compiled | lists/vectors, closures, global constants (`pi`), complex numbers, conditional or multi-clause functions; Rust code cannot compile an *expression in given variables*, only named functions |
| `mc-roots` | Newton, secant, Brent, Broyden, BFGS; verified ball roots up to 10^5 digits; functions are seen through a private `NumFn` trait, "so a compiled backend can be added later" |
| Balls | Real balls at any precision with the elementary functions. Complex balls exist as numbers (`complex(ball, ball)`), but **no elementary function evaluates on them**: `n(exp(1 + 2im), 30)` stays unevaluated (machine complex numbers work) |
| Random numbers | `mc-random`: Philox streams, splittable; compiled code cannot draw yet |
| `n_integrate`, `n_dsolve` | not started |
| Benchmarks | class L is specified (L3 Lorenz, L4 Robertson, L5 Newton solves, L6 quadrature, L10 Monte Carlo); no case files exist |

## 2. Two regimes

Solvers call their function thousands to millions of times. Their cost is roughly

    T = T_compile + N_calls * t_call

At **machine precision** `t_call` is dominated by interpretive overhead (about 1 µs per call in the evaluator, a few
ns compiled), so the compiler is the lever. At **high precision** one ball multiplication at 1000 digits already
costs microseconds; the arithmetic dominates and compilation buys little (plan section 4: symbolic and big-number
work is not dispatch-bound).

The algorithms change too. A method of fixed order p needs about 10^(d/p) steps for d digits: Dormand–Prince (p = 5)
at 100 digits would need 10^20 steps. High precision needs methods whose order grows with the digits.

| Problem | Machine precision (compiled `f64`) | Any precision (balls) |
|---|---|---|
| Roots | Newton, Brent, BFGS | Newton with precision doubling, interval Newton verification (done in `mc-roots`) |
| Integrals | adaptive Gauss–Kronrod G7-K15; tanh-sinh for endpoint singularities | tanh-sinh (digits double per level); rigorous Gauss–Legendre with error bounds from complex-ball evaluation (Petras; Johansson) |
| ODEs | Dormand–Prince 5(4) with dense output; Radau IIA or Rosenbrock for stiff systems | Taylor series method of order about d, coefficients by automatic differentiation recurrences; verified enclosures (Lohner) later |

Two meanings of "arbitrary precision" are kept apart: **d digits, heuristically** (tanh-sinh, Taylor series with
error estimates) and **d digits, guaranteed** (balls with rigorous bounds). Guaranteed results are planned for roots
and integrals; for ODEs heuristic high precision comes first and verified enclosures are a later, separate package.

## 3. Decisions

| # | Decision | Reason |
|---|---|---|
| NUM.1 | One public trait `NumFn` over the number types `f64`, `Ball(p)` and complex balls, in a new crate `mc-numfn`; every solver is written once, generic over it (as `mc-roots` already is over its `Scalar`) | One algorithm per problem, not one per precision |
| NUM.2 | Backends of `NumFn`: **compiled** (`mc-jit`, `f64`), **interpreted** (the evaluator, any number type), **batch** (many points per call). `compile_expr(expr, vars)` picks compiled and falls back to interpreted automatically when compilation is refused or declines at run time | Correctness never depends on the compiler (decision 1b.3) |
| NUM.3 | Every solver runs in two tiers: solve with compiled `f64`, then refine with balls at the requested `digits:` (the `mc-roots` pattern) | Most of the work happens at machine speed |
| NUM.4 | Precision-dependent method choice is internal: `n_integrate(f, [x, a, b]; digits: 50)` switches to tanh-sinh or rigorous Gauss–Legendre; `n_dsolve(...; digits: 50)` to the Taylor method | The user states the goal, not the method (methods stay selectable with `method:`) |
| NUM.5 | Compiled ball code (the compiler emitting calls into the ball runtime) is not planned until measurements show interpreter overhead matters below about 100 digits | Section 2 |
| NUM.6 | Results: `n_integrate` returns a number (a ball under `digits:`); `n_dsolve` returns an `interpolating_function` term that evaluates by dense output and differentiates by its derivative data | Plots and further solvers consume solutions as functions |
| NUM.7 | Failures follow `mc-roots`: `failure(not_converged, f, estimate, error_estimate)`, `failure(not_numeric, f, point)`, `failure(divergent, ...)`, limits and interrupts as everywhere | Uniform with existing packages |

## 4. Packages

### N0 Foundation (sequential: everything else depends on it)

1. **Class L case files first**: L3, L4, L5, L6 and L10 in `bench/cases/L/`, plus three high-precision cases
   (root at 10^3 digits, an integral at 50 and 500 digits, an ODE solution value at 30 digits) with reference values.
2. **`mc-numfn`**: the `NumFn` trait and the three backends; `compile_expr`; `mc-roots` moved onto it unchanged in
   behaviour (its tests must pass untouched).
3. **Compiler extensions** (`mc-jit`): global constants folded to machine reals (`pi`, `e`); **fixed-size real
   vectors** as arguments and results (right-hand sides `f(t, y)` of systems, gradients, Jacobians); closures
   (`x => ...`) as compilable functions.
4. **Complex balls** (`mc-numeric`): `exp`, `log`, `sin`, `cos`, `sqrt`, powers and `atan` on complex balls, needed
   for rigorous integration bounds.

Exit: compiled and interpreted backends agree on random programs (extending `mc-jit`'s agreement tests to vectors
and closures); the L cases run (interpreted where compilation is refused) and their speedups are recorded.

### N1 Roots on the compiler (`mc-roots`)

The compiled backend for `f`, `f'` and Jacobians from `d`; Newton steps fully compiled. Exit: L5 measured against the
interpreted backend; the existing ball refinement unchanged.

### N2 `n_integrate` (new crate or `mc-numeric`)

Adaptive Gauss–Kronrod G7-K15 with a global error heap (QUADPACK's `qag` strategy); tanh-sinh for endpoint
singularities and as the high-precision method; infinite ranges by substitution; batch calls of 15 nodes; rigorous
mode under `digits:` with error bounds from complex-ball evaluation on ellipses (Petras; Johansson 2018). The error
estimate drives refinement:

    err ≈ |K15 - G7|,   split the interval with the largest err until  sum(err) < tolerance

Exit: L6 passes; 50- and 500-digit integrals agree with reference values; the rigorous mode's balls contain them.

### N3 `n_dsolve` (new crate `mc-ode`)

Explicit Dormand–Prince 5(4) with step control `h_new = h * min(5, 0.9 * (tol/err)^(1/5))` and dense output; a stiff
method (Radau IIA order 5 or a Rosenbrock method) with the Jacobian from `d`, compiled, and the dense LU of
`mc-roots` (no dependency on package 3A); stiffness detection to switch methods; events (zero crossings);
`interpolating_function` results. The high-precision method is a Taylor series integrator: coefficients come from
recurrences generated from the right-hand side, for example

    y' = y^2:   (y^2)_k = sum_{j=0..k} y_j y_{k-j},   y_{k+1} = (y^2)_k / (k + 1)

with order about the number of digits and step size from the coefficients' decay. Exit: L3 and L4 pass; the
30-digit ODE case agrees with its reference.

### N4 Scale (after N1–N3)

Packed real arrays (semantics 11; `mc-random` measured lists of reals to be heap-bound), parallel batch evaluation
of effect-free compiled code (semantics 18.2 (2)), random draws from compiled code (design in `mc-random`'s NOTES),
compiled complex numbers (L9).

### N5 Optional

Compiled ball code (decision NUM.5) if measurements call for it; verified ODE enclosures (Lohner's method).

## 5. Order and parallelism

    N0 (benchmarks, mc-numfn, compiler vectors/closures/constants, complex balls)
       ├── N1 roots on the compiler      (small)
       ├── N2 n_integrate                (medium)
       └── N3 n_dsolve                   (largest)
              └── N4 packed arrays, parallel batches, random draws in compiled code
                     └── N5 optional

N0 is the critical path; its four items are developed together because the trait, the compiler extensions and the
benchmarks constrain each other. N1, N2 and N3 own different crates and run in parallel. Each package follows the
established rules: its own branch, tests and a `NOTES.md` report, guarded runs with time limits, and merges only
after the full suite passes.

## 6. Reading list

- R. Piessens, E. de Doncker-Kapenga, C. W. Überhuber, D. K. Kahaner, *QUADPACK: a subroutine package for automatic
  integration*, Springer, 1983.
- H. Takahasi, M. Mori, *Double exponential formulas for numerical integration*, Publ. RIMS 9, 1974.
- D. H. Bailey, K. Jeyabalan, X. S. Li, *A comparison of three high-precision quadrature schemes*, Experimental
  Mathematics 14(3), 2005.
- K. Petras, *Self-validating integration and approximation of piecewise analytic functions*, J. Comput. Appl. Math.
  145, 2002.
- F. Johansson, *Numerical integration in arbitrary-precision ball arithmetic*, ICMS 2018.
- J. R. Dormand, P. J. Prince, *A family of embedded Runge–Kutta formulae*, J. Comput. Appl. Math. 6, 1980.
- E. Hairer, S. P. Nørsett, G. Wanner, *Solving Ordinary Differential Equations I*, Springer, 1993; E. Hairer,
  G. Wanner, *Solving Ordinary Differential Equations II: Stiff and Differential-Algebraic Problems*, Springer, 1996.
- À. Jorba, M. Zou, *A software package for the numerical integration of ODEs by means of high-order Taylor
  methods*, Experimental Mathematics 14(1), 2005.
- R. J. Lohner, *Enclosing the solutions of ordinary initial and boundary value problems*, in *Computer Arithmetic*,
  Teubner, 1987.
- J. Bezanson et al., *Julia: a fresh approach to numerical computing*, SIAM Review, 2017 (type-specialising
  compilation).
