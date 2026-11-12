# mc-mechanics: notes (round 31)

The analytical mechanics toolkit of roadmap 3.5 (P1), written entirely in MathCAS source (`lib/mechanics.mc`,
loaded last, after every other library) on top of `d`, `solve`, `simplify`, `det`, `null_space`, `linear_solve`,
`n_dsolve` and `stream_plot`. Helpers are prefixed `m31_`; the only rule patterns (in `m31_first`) use `m31`-prefixed
pattern variables, so a user's global value cannot shadow them (test `user_values_do_not_reach_the_library`).
Decisions are numbered p31.N.

Literature: H. Goldstein, C. Poole, J. Safko, *Classical Mechanics*, 3rd ed. (2002), chapters 1, 2, 6, 8, 9;
L. D. Landau, E. M. Lifshitz, *Mechanics*, 3rd ed. (1976), sections 2-7, 21-24, 40-42; P. J. Olver, *Applications of
Lie Groups to Differential Equations*, 2nd ed. (1993), 4.4 (Noether's theorem, prolongation); M. V. Ostrogradsky
(1850) for the higher-derivative Euler-Lagrange equations; I. M. Gelfand, S. V. Fomin, *Calculus of Variations*
(1963), chapter 7 (field Lagrangians).

## Naming and representation

- p31.1 Coordinates are functions of the independent variables, `x(t)` (or a field `u(x, t)`), or their bare names
  `x`, which are applied to the variables. Derivatives are the language's `derivative(k)(x)(t)` (`x'(t)`) and
  `derivative(i, j)(u)(x, t)`. The independent variable is a symbol, or a list of symbols for fields. A single
  coordinate may stand alone; several are a list. The functions take `(L, q, t)` in this order throughout.
- p31.2 Names: `euler_lagrange_equations`, `generalized_momenta`, `energy_function`, `legendre_transform`,
  `hamilton_equations`, `poisson_bracket`, `noether_charge`, `equilibrium_points`, `normal_modes`, `phase_portrait`,
  `n_dsolve_motion` (grouped with `n_dsolve` in completion), `energy_drift`. Options are snake_case:
  `constraints:`, `multipliers:`, `time_shift:`, `gauge:`, `at:`, `points:`.
- p31.3 Equations are returned as `expr == 0` with `expr = dL/dq - d/dt dL/dq' + ...` expanded (the sign of the
  variational derivative), one per coordinate, always in a list.
- p31.4 Momenta are plain names (`p`, `[pr, pth]`) in `legendre_transform`; the coordinates keep the form they have
  in `L` (so `H` is in `x(t)` and `p`). `hamilton_equations` and `poisson_bracket` accept either names or functions
  `x(t)` for both, by replacing them with the internal placeholders `m31_q(i)`, `m31_p(i)` before differentiating
  and putting them back afterwards; `hamilton_equations` writes the result in functions of `t`.

## Methods

- p31.5 Euler-Lagrange: the variational derivative `sum over multi-indices k of (-1)^|k| D^k (dL/du_k)`, with
  `dL/du_k` the partial derivative with respect to the derivative term itself (`d` differentiates with respect to
  `x'(t)` and `derivative(1, 1)(u)(x, t)` directly) and `D^k` the total derivative (`d(e, [x, k])`, which applies the
  chain rule to every function of the variables). The derivatives present are found by `free_of` over all
  multi-indices up to order 6 (one variable) or total order 4 (several): cheap, and higher orders are not physical.
- p31.6 Constraints: a constraint free of velocities is holonomic and contributes `lambda dg/dq` (the Euler-Lagrange
  equations of `L + lambda g`); one in the velocities contributes `lambda dg/dq'`, the d'Alembert (non-holonomic)
  force, not the vakonomic equations of `L + lambda g` (Goldstein 2.4, Bloch, *Nonholonomic Mechanics and Control*,
  1.3). For fields only holonomic constraints. Default multipliers `lambda(k)(t)`; `multipliers:` names them.
- p31.7 Legendre transform: the velocities are replaced by placeholders, `p_i == dL/dv_i` is solved by `solve`; a
  system without a unique solution (singular Lagrangian, Dirac's constrained case) leaves the call unevaluated.
  `legendre_transform(f, x, p)` takes the first solution of `p == f'(x)` with integer constants set to 0 (the
  principal branch, `log(p)` for `exp(x)`).
- p31.8 Noether: generator `X = xi d/dt + sum eta_i d/dq_i`, prolongation `eta_i^(1) = D_t eta_i - q_i' D_t xi`;
  condition `pr X(L) + L D_t xi = D_t F` (Olver Thm. 4.12 and 4.29 for divergence symmetries), with the explicit
  `dL/dt` taken with coordinates and velocities frozen as placeholders. The residual is simplified; a non-zero
  residual gives `failure(not_a_symmetry, noether_charge, residual)` instead of a wrong charge. Charge
  `Q = sum p_i (eta_i - q_i' xi) + L xi - F`: `time_shift: 1` gives `-E` (the standard sign of the theorem, kept so
  that every charge follows one formula). Tested on-shell independently: `dQ/dt` with the accelerations substituted
  vanishes (the dilatation of conformal mechanics).
- p31.9 Small oscillations: `M = d^2L/dv dv`, `K = -d^2L/dq dq`, `A = d^2L/dv dq` at the equilibrium with `v = 0`,
  `G = A - A^T`; the linearised equations `M q'' + G q' + K q = 0` (Goldstein 6.2; Landau-Lifshitz 23 and 39 for the
  gyroscopic case). `det(K - w^2 M + i w G)` is even in `w` (its transpose is the same matrix at `-w`), so it is
  solved as a polynomial in `w^2`; frequencies are `sqrt(w^2)` (simplified), modes the null vectors. `eigensystem`
  of `M^-1 K` failed on the double pendulum (symbolic radicals); the determinant route works there and handles the
  gyroscopic case too. When `null_space` cannot see the rank of a symbolic matrix with radicals (it returns `[]`),
  the null vector with last entry 1 is solved from the first n - 1 rows (`m31_null`; rank n - 1 assumed).
  Degenerate frequencies are deduplicated before the null spaces, so each appears once per mode.
- p31.10 Numerical motion: the accelerations are solved symbolically from the Euler-Lagrange equations (the mass
  matrix may depend on the coordinates) so `n_dsolve` gets an explicit system (its implicit-equation path refuses
  coupled accelerations); if that solve fails the implicit equations are passed as they are. `energy_drift` samples
  the energy function at points + 1 times. Measured drift with the default `n_dsolve` tolerances: pendulum over
  t in [0, 20] 7.3e-8, double pendulum over [0, 10] 7.3e-7, Kepler over [0, 30] 5.1e-8, angular momentum in Kepler
  3.8e-8.
- p31.11 `phase_portrait` draws `stream_plot([v, acc(x, v)], [x, a, b], [v, c, d])` for one coordinate, with the
  acceleration from `m31_accels`; a time-independent acceleration is needed (the Caldirola-Kanai Lagrangian
  `e^(t/5) L0` works because the factor cancels in the solved acceleration).

- p31.12 Results (equations, energies, Hamiltonians, brackets, charges) are expanded; when the expanded form holds a
  power with a non-integer exponent it is simplified instead (`m31_tidy`): the relativistic energy becomes
  `m/(1 - x'(t)^2)^(1/2)`, the geodesic equation of `sqrt(1 + y'^2)` becomes `-y''/(1 + y'^2)^(3/2) == 0`. The
  relativistic Hamiltonian stays `(m^2 + p^2)*((m^2 + p^2)^-1)^(1/2)` even under `assuming(m > 0, ...)`, a gap of
  `simplify` (open there: `a*(1/a)^(1/2)` to `a^(1/2)` for `a > 0`).
- p31.13 The guide is `docs/guides/15-analytical-mechanics.mcn` (numbers 13 and 14 left to the other new areas
  of round 31); its slug is registered in `crates/mc-docs/tests/guides.rs`.

## Scope

- Besides Euler-Lagrange equations and first integrals, the toolkit has the Legendre transform, Hamilton's
  equations, Noether charges from given generators with a symmetry check, multiplier constraints, normal modes and
  energy-checked integration. It is checked by its own tests and the guide's examples.
- Noether charges are verified: a generator that is not a symmetry gives a failure with the residual (gravity and a
  vertical translation) instead of a charge that is not conserved.
- Normal modes with gyroscopic terms (a charge in a trap and a magnetic field: frequencies 1 and 2, circular modes)
  and with symbolic radicals (the double pendulum, unequal masses on springs), where an eigensystem of `M^-1 K`
  fails.

## Gaps

- Noether charges for field Lagrangians and for higher-derivative Lagrangians; generators in the format of
  `mc-diffalg`'s `lie_point_symmetries` (to be connected once that format is settled: the generator `[xi, eta]` maps to
  `time_shift: xi` and `eta`).
- Dirac-Bergmann constraint analysis of singular Lagrangians (`legendre_transform` stays unevaluated).
- `normal_modes` returns frequencies in the order of `solve` (not sorted for symbolic parameters) and assumes the
  equilibrium is stable (an unstable one gives imaginary frequencies, as the formula says).
- `phase_portrait` covers one coordinate; Hamiltonian phase portraits (H, x, p) and Poincare sections are not done.
- `n_dsolve_motion` has no constraint option (index-reduced DAEs) and passes no event handling.
