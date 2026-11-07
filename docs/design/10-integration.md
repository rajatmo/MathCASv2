# MathCAS v2: plan for numerical integration

This plan sets out how `n_integrate` should improve as a general tool. It is ordered by how much it helps the most
users, with the literature each piece rests on. Field-specific machinery (loop integrals, phase-space generators,
Dirac algebra, PDF sets) belongs in packages built on these pieces, not in the core.

## 1. Where it stands

`n_integrate` (crate `mc-quad`) has:
- adaptive Gauss–Kronrod (7–15 points, the QUADPACK rule) and double-exponential (tanh-sinh) quadrature;
- splitting at given points and infinite ranges;
- arbitrary-precision results with `digits:` and rigorous enclosures with `rigorous: true`;
- integrands compiled to machine code (including special functions, interpolations and ODE solutions);
- multiple integrals as nested one-dimensional integrals (`[y, c(x), d(x)]`).

What is missing:
- **Dimension:** nested one-dimensional rules cost (points per dimension)^d, so they are useless beyond about 3
  dimensions, and there is no Monte Carlo or quasi-Monte Carlo.
- **Singularities and oscillation:** no extrapolation for endpoint singularities in the Gauss–Kronrod path, no
  principal values, and no method for oscillatory or Fourier-type integrals.
- **Throughput:** the integrand is called one point at a time, on one core.
- **Error reporting:** an error estimate is not part of the returned value unless `digits:` is given.

## 2. Plan

### 2.1 Batched, parallel integrand evaluation (the base for everything below)

Each rule asks for whole blocks of points: a packed array in, a packed array of values out. A compiled integrand runs
a block as one loop, and blocks are spread over the session's worker threads. Results stay bit-for-bit
deterministic: a fixed block order and a fixed reduction order. This is how vectorised cubature libraries get their
speed (the `cubature` library's vectorised interface; Cuba's `nvec`; Hahn 2005), and it makes every method below
faster.

### 2.2 Adaptive cubature in moderate dimension (2 to about 7)

- Genz–Malik degree-7 rules on hyper-rectangles, with the error estimate from an embedded degree-5 rule, and
  subdivision along the dimension with the largest fourth difference (Genz and Malik 1980; Berntsen, Espelid and
  Genz 1991, DCUHRE).
- A heap of regions ordered by error, a global error goal, and an optional cap on evaluations.
- Non-rectangular regions through the existing variable bounds `[y, c(x), d(x)]`, by mapping to the unit cube.

This becomes the default for `n_integrate` with 2 to 7 variables.

### 2.3 Monte Carlo and quasi-Monte Carlo (high dimension, rough integrands)

- **Adaptive importance sampling:** VEGAS (Lepage 1978) with the VEGAS+ adaptive stratification (Lepage 2021). Its
  result carries the χ² per degree of freedom of the iterations, so inconsistent estimates show.
- **Quasi-Monte Carlo:**
  - randomised rank-1 lattice rules (Dick, Kuo and Sloan 2013), whose error falls close to 1/N for smooth periodised
    integrands, with an unbiased error from random shifts;
  - scrambled Sobol sequences (Owen 1995; Joe and Kuo 2008 direction numbers).
- **Results:** `around(value, error)`, so the error carries through later arithmetic (this already exists for
  `around`).
- **Reproducibility:** counter-based random streams (Salmon et al. 2011, Philox), one per block, so parallel results
  do not depend on the thread count.
- **Selection:** `method: vegas | lattice | sobol | monte_carlo`; the default above 7 dimensions is lattice
  quasi-Monte Carlo for smooth integrands and VEGAS otherwise.

Learned importance sampling with normalising flows (Gao et al. 2020; Deutschmann and Götz 2020) is promising but needs
a training framework; it is a package candidate, not core.

### 2.4 One-dimensional robustness

- **Endpoint singularities:** extrapolation with Wynn's ε-algorithm over the bisection sequence (Wynn 1956; the QAGS
  strategy, Piessens et al. 1983). For example `∫ log(x)/sqrt(x)` on [0, 1].
- **Principal values and weight functions:** Cauchy principal values (`principal_value: true`, the QAWC approach),
  and algebraic–logarithmic endpoint weights (QAWS).
- **Oscillatory integrals:**
  - Levin collocation for `f(x)·exp(i·g(x))` and Bessel-type kernels (Levin 1982; Olver 2010);
  - Filon and Clenshaw–Curtis rules for `sin(ωx)` and `cos(ωx)` weights (QAWO);
  - Longman's method with Wynn-ε acceleration for oscillatory integrals over infinite ranges (QAWF);
  - the double-exponential transformation for Fourier integrals (Ooura and Mori 1991).
- **Automatic choice:** the method follows the integrand's form, as the existing `method:` choice does.

### 2.5 Error and precision control

- **One goal language:** `precision_goal`, `accuracy_goal` and `max_points` for every method, and a report of the
  achieved error.
- **Warnings:** a message when the error goal was not met, saying which region or which iteration failed.
- **High precision:** Gauss–Legendre with nodes computed in ball arithmetic and tanh-sinh at any precision (Bailey,
  Jeyabalan and Li 2005). High-precision results feed `find_integer_relation` (below).

### 2.6 Helpers that turn numbers into exact results (general, not physics-specific)

- **Integer relations:** `find_integer_relation` (PSLQ; Ferguson, Bailey and Arno 1999), and a `recognize` built on
  it that matches a number against a basis (π, logs, ζ values, polylogarithms at simple points).
- **Series acceleration:** Richardson, Wynn ε and Levin u-transforms for slowly converging series and sequences;
  these are used by 2.4 and are useful on their own.

## 3. Packages, not core

These build on the pieces above and go into separate packages (`.mc` modules, or crates loaded on demand):

| Package | Contents | Literature |
|---|---|---|
| Loop integrals | one-loop scalar functions A₀–D₀ (complex masses), Passarino–Veltman reduction, Feynman parameters, sector decomposition with ε expansion, Mellin–Barnes contours | Passarino and Veltman 1979; 't Hooft and Veltman 1979; Binoth and Heinrich 2000; Czakon 2006 |
| Iterated integrals | harmonic and multiple polylogarithms with fast numerics and shuffle algebra | Remiddi and Vermaseren 2000; Vollinga and Weinzierl 2005 |
| Phase space | RAMBO, n-body massive phase space, multichannel sampling on top of VEGAS | Kleiss, Stirling and Ellis 1986; Kleiss and Pittau 1994 |
| Dirac and colour algebra | d-dimensional traces, γ₅ schemes, SU(N) colour factors | — |
| Thermal functions | the bosonic and fermionic thermal integrals and their derivatives, tabulated with interpolation | — |
| Physics inputs | parton-distribution grids, running couplings, natural-unit conversions | — |
| Learned sampling | normalising-flow importance sampling | Gao et al. 2020 |

### Status

- **2.1 done** (package quad-1): the multidimensional methods evaluate the integrand in blocks of up to 2^15 points
  through `mc_numfn::Par` (compiled code of the expression's cached family split over the session's threads,
  the interpreter for what does not compile or `compile: false`); points are generated in parallel blocks keyed by
  position and every sum is added in point order, so results are identical at any thread count. The
  one-dimensional rules keep their small batches (30 or 122 points).
- **2.5 partly done**: `precision_goal`, `accuracy_goal` and `max_points` apply to every machine-precision method
  (for the nested rules `max_points` caps the subdivisions); `error_estimate: true` returns `around(value, error)`
  for any method; a message says when the goal was not met, how many evaluations were spent, and (cubature) where
  the largest error is left or (VEGAS) whether the iterations agree. High-precision Gauss–Legendre is not done.
- **2.2 done**: Genz–Malik degree-7/5 cubature with a region heap, fourth-difference splitting and rounds of
  regions evaluated together; variable inner bounds, infinite ranges and split points through a piecewise map of
  the unit cube. Default for 2 to 7 variables (in 2 or 3 variables a missed goal also tries the nested rules).
- **2.3 done**: VEGAS with VEGAS+ stratification (χ² per degree of freedom reported), randomised rank-1 lattice
  sequence with the tent transformation, Owen-scrambled Sobol points, plain Monte Carlo; results `around(value,
  error)`; Philox streams per block, `seed:` option (default 0). The lattice generating vector and the Sobol
  parameters come from this project's own searches (64 dimensions). Default above 7 variables: lattice for
  integrands of analytic functions, VEGAS otherwise.
- Benchmark: `crates/mc-quad/tests/bench.rs` (ignored test `genz_benchmark`), results in `crates/mc-quad/NOTES.md`.

## 4. Order of work

1. 2.1 batched, parallel evaluation, together with 2.5 goals and reporting. Everything else uses these.
2. 2.2 adaptive cubature, and 2.3 VEGAS+ and lattice QMC. The largest gain for multi-dimensional integrals.
3. 2.4 one-dimensional robustness (Wynn ε, principal values, oscillatory methods).
4. 2.6 PSLQ and acceleration.
5. Packages, starting with thermal functions and one-loop integrals.

Each step is measured on a benchmark set of the classic test integrals: the Genz test families (Genz 1984), singular
and oscillatory one-dimensional cases, and smooth and peaked integrals in 4 to 12 dimensions. The set records time,
the number of integrand calls, achieved error and the reliability of the error estimate.

## References

- D. H. Bailey, K. Jeyabalan, X. S. Li, *A comparison of three high-precision quadrature schemes*, Exp. Math. 14
  (2005).
- J. Berntsen, T. O. Espelid, A. Genz, *An adaptive algorithm for the approximate calculation of multiple integrals*,
  ACM TOMS 17 (1991).
- T. Binoth, G. Heinrich, *An automatized algorithm to compute infrared divergent multi-loop integrals*, Nucl. Phys.
  B585 (2000).
- M. Czakon, *Automatized analytic continuation of Mellin–Barnes integrals*, Comput. Phys. Commun. 175 (2006).
- C. Gao, J. Isaacson, C. Krause, *i-flow: High-dimensional integration and sampling with normalizing flows*, Mach.
  Learn.: Sci. Technol. 1 (2020).
- J. Dick, F. Y. Kuo, I. H. Sloan, *High-dimensional integration: the quasi-Monte Carlo way*, Acta Numerica 22
  (2013).
- H. R. P. Ferguson, D. H. Bailey, S. Arno, *Analysis of PSLQ, an integer relation finding algorithm*, Math. Comp. 68
  (1999).
- A. Genz, A. A. Malik, *An adaptive algorithm for numerical integration over an n-dimensional rectangular region*,
  J. Comput. Appl. Math. 6 (1980).
- A. Genz, *Testing multidimensional integration routines*, in Tools, Methods and Languages for Scientific and
  Engineering Computation (1984).
- T. Hahn, *Cuba — a library for multidimensional numerical integration*, Comput. Phys. Commun. 168 (2005),
  arXiv:hep-ph/0404043.
- S. Joe, F. Y. Kuo, *Constructing Sobol sequences with better two-dimensional projections*, SIAM J. Sci. Comput. 30
  (2008).
- R. Kleiss, R. Pittau, *Weight optimization in multichannel Monte Carlo*, Comput. Phys. Commun. 83 (1994).
- G. P. Lepage, *A new algorithm for adaptive multidimensional integration*, J. Comput. Phys. 27 (1978).
- G. P. Lepage, *Adaptive multidimensional integration: VEGAS enhanced*, J. Comput. Phys. 439 (2021).
- D. Levin, *Procedures for computing one- and two-dimensional integrals of functions with rapid irregular
  oscillations*, Math. Comp. 38 (1982).
- S. Olver, *Fast, numerically stable computation of oscillatory integrals with stationary points*, BIT 50 (2010).
- T. Ooura, M. Mori, *The double exponential formula for oscillatory functions over the half infinite interval*,
  J. Comput. Appl. Math. 38 (1991).
- A. B. Owen, *Randomly permuted (t,m,s)-nets and (t,s)-sequences*, in Monte Carlo and Quasi-Monte Carlo Methods
  (1995).
- R. Piessens, E. de Doncker-Kapenga, C. W. Überhuber, D. K. Kahaner, *QUADPACK* (Springer, 1983).
- J. K. Salmon, M. A. Moraes, R. O. Dror, D. E. Shaw, *Parallel random numbers: as easy as 1, 2, 3*, SC'11 (2011).
- P. Wynn, *On a device for computing the e_m(S_n) transformation*, MTAC 10 (1956).
