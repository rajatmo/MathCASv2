# mc-diffalg: notes (round 31)

Symmetry methods for differential equations and differential algebra (roadmap 3.4, track D). The library is
MathCAS source, `lib/diffalg.mc` (helpers prefixed `dg_`, pattern variables `dg_...`), loaded after the other
libraries; it calls `d`, `solve`, `d_solve`, `integrate`, `coefficient_rules` and `simplify` through the evaluator.
Decisions are numbered g31.N.

## Naming and representation

- g31.1 A Lie point symmetry generator `sum xi_i d/dx_i + sum eta_k d/du_k` is the list of its coefficients
  `[xi_1, ..., xi_p, eta_1, ..., eta_q]`, over the variables `[x_1, ..., x_p, u_1, ..., u_q]` (the independent ones
  first, then the dependent ones as plain symbols). Lists compose with `symmetry_commutator`, `canonical_coordinates`
  and the reductions, need no new head, and print compactly. In the determining equations the infinitesimals are
  `xi(x, u)`, `eta(x, u)` (`xi1, ...`, `eta1, ...` when there are several). Commutator tables are written in the
  basis `v(1), v(2), ...` (the i-th generator of the list). Arbitrary functions left in a generator are `c(1)(...)`,
  ..., as `d_solve` names its arbitrary functions, inside `conditional_expression(gen, equations)`.
- Function names follow the language's style: `lie_point_symmetries`, `symmetry_determining_equations`,
  `symmetry_commutator`, `lie_commutator_table`, `canonical_coordinates`, `symmetry_reduce_order`,
  `symmetry_d_solve`, `similarity_reduction`.
- Internally a jet variable u_J is the symbol `dg_j_<u>_<j1>_..._<jp>`; the user's `derivative(J)(u)(x, ...)` terms are
  rewritten to jets and back. The infinitesimals are unknown functions `dg_xi_i(zs)`, `dg_phi_k(zs)` of the base
  variables zs (the x's and the order-0 jets).

## Determining equations

- The prolongation by the recursion phi^(J + e_i) = D_i phi^J - sum_j u_(J + e_j) D_i xi_j (Olver, *Applications of
  Lie Groups to Differential Equations*, 2nd ed., Thm. 2.36 and (2.44)), D_i the total derivative. Each equation is
  solved for one of its highest-order jets (the last in lexicographic order among those of top order, so `u_t ==
  u_xx` is solved for u_xx, an evolution equation of order 3 for u_xxx), the condition pr v(Delta) is evaluated on
  that substitution, cleared of denominators and split by the monomials in the remaining jets of order >= 1
  (`coefficient_rules`). For a single equation of maximal rank this is Olver's Thm. 2.31 condition.
- Gap: equations not polynomial in the remaining derivatives (sin(y')) are not split further (the coefficient keeps
  the jet); systems whose leading jets are derivatives of one another (not passive) would need their integrability
  conditions first (the Rosenfeld-Groebner part of this crate is the route).

## Solving the determining equations (g31.2)

A linear homogeneous system of PDEs in unknown functions of subsets of the base variables, solved by the steps that
hand calculations use (Bluman and Anco, *Symmetry and Integration Methods for Differential Equations*, 2002, 2.3;
Hereman, *Review of symbolic software for Lie symmetry analysis*, 1997, for the strategy), tried in this order on
the equations sorted by size:

1. equations in the free constants alone: split by the base variables when polynomial, else solved for a constant;
2. a single term c D^a F: F = 0, a polynomial of degree a_z - 1 in the one variable z with new unknown functions of
   the other arguments, or for F_xy = 0 a sum of functions of fewer variables;
3. splitting by the powers of a base variable z no unknown in the equation depends on (`coefficient_list`);
4. an undifferentiated unknown F solved for and eliminated, when the rest depends only on F's arguments;
5. c D_z^k F + rest with F only in that term and rest free of unknowns that depend on z: F is the k-fold integral
   of -rest/c plus a polynomial in z (with `integrate`);
6. equations whose unknowns are functions of one variable: `d_solve` (one unknown alone first, else the subsystem);
   the constants c(k) become new constants;
7. separation by differentiation: e = P + Q with Q free of z and containing unknowns, P depending on z: the
   consequence d(e, z) == 0 is added (the derivative with the fewest terms).

Every constant left in the infinitesimals gives one generator (its coefficient); unknown functions that remain give
the infinite-dimensional part with the equations they satisfy (for the heat equation, a solution of the equation
itself, as in Olver, Example 2.41). Normalisation divides each equation by the factors and the powers of base
variables free of the unknowns. At most 400 steps.

Checked against the literature: y'' = 0 (8, sl(3,R), Olver Ex. 2.43), y'' + y = 0 (8), Ermakov-Pinney y'' = y^-3
(sl(2), Bluman-Anco 3.4), Blasius y''' + y y'' = 0 (2), heat equation (6 + infinite, Olver Ex. 2.41), Burgers (5,
Olver Ex. 2.45), KdV (4, Olver Ex. 2.44). The tests substitute every generator back into the determining equations
(`dg_check_generator`) and check that the commutators close.

- g31.4 `lie_point_symmetries(..., degree: k)` instead takes the infinitesimals as polynomials of total degree <= k
  in all variables with unknown coefficients and solves the split linear equations: the route for first-order
  ordinary differential equations, whose single determining equation has infinitely many solutions and is not
  solved by the steps above.

## The symmetry algebra (g31.3)

`symmetry_commutator` by [v, w]_i = sum_j v_j d_j w_i - w_j d_j v_i. `lie_commutator_table` writes each commutator in
the basis: for rational fields from exact linear equations at r + 3 rational points; otherwise (trigonometric
fields) by least squares at machine values of the same points, rationalised to 10^-9; in both cases the
coordinates are then checked symbolically (`possible_zero_q` of every component). A commutator outside the span is
returned by its components.

## Reduction and integration (g31.5)

- `canonical_coordinates`: r is a first integral of dy/dx = eta/xi (from `d_solve`, each branch tried), s the
  integral of dx/xi along the characteristic, with the constant replaced by r (Bluman-Anco 2.2.4). xi = 0 or eta = 0
  by one integration. The principal branch is taken where `solve` attaches conditions, and sqrt(a^2) is a.
- `symmetry_reduce_order`: x, y written in terms of r, s(r) (solving the coordinate map, an equation in one
  variable first), the derivatives by the chain rule; the invariance makes the result free of s, which is checked;
  v = ds/dr. `symmetry_d_solve`: a first-order equation becomes the quadrature s = integral of G(r) + c(1), a
  second-order one a first-order equation for v solved by `d_solve`, then s = integral of v + c(2); the result is
  solved for y or returned as the implicit equation. Symmetries: `lie_point_symmetries`, else polynomial generators
  of degree 1, then 2.

## Similarity reductions (g31.6)

`similarity_reduction(eq, u, [x, t], [xi, tau, eta], f, z)`: for xi, tau free of u and eta linear in u, the
characteristic system dx/xi = dt/tau = du/eta is solved by `d_solve` with t (or x when tau = 0) as the parameter:
the first integral is the similarity variable z, the second constant becomes f(z). The invariant form substituted
into the equation, with x (or t) written through z, gives the ordinary differential equation (factors free of f
dropped). Checked: KdV travelling waves and the scaling reduction (Olver Ex. 3.4), the heat equation's scaling
(z = x/sqrt(t)), Burgers' Galilean boost.

Gaps: more than two independent variables, several dependent variables, generators with xi or tau depending on u.

## Conservation laws (g31.7)

- `conservation_law_multipliers`: Lambda(x, u, u_x, ..., u_(x^q)) (for an ordinary equation, y, ..., y^(q)) with
  E_u(Lambda Delta) == 0 identically in u (Olver, Thm. 4.7; Bluman, Cheviakov and Anco, *Applications of Symmetry
  Methods to Partial Differential Equations*, 2010, 1.3, the direct method). The Euler operator sum (-D)_J d/du_J
  over jets up to order 2 ord + 1; the condition is split by the jets Lambda does not depend on and solved by the
  g31.2 solver; infinite families are returned as conditional_expression (the heat equation: the backward heat
  equation). Option `degree: d` takes Lambda polynomial of total degree <= d with constant coefficients and solves
  the split linear system (the route for ordinary equations at order >= 1, whose multipliers form an infinite
  family).
- `conservation_laws`: ordinary equations: Phi with Lambda Delta == D_x Phi; evolution equations u_t = F: the density
  T = integral_0^1 u Lambda[s u] ds (homotopy formula; multipliers of evolution equations are variational
  derivatives of densities, Olver 4.3), the flux X with D_x X == -D_t T on solutions (u_t and its x-derivatives
  replaced by F and D_x^k F). Both inversions of D_x integrate the highest jet (the expression is linear in it)
  with respect to the jet one below and subtract the total derivative, then integrate in x.
- Checked: KdV (1, u, t u - x, and u_xx + u^2/2 at order 2; Olver Ex. 4.36), each law on the soliton
  3 sech^2((x - t)/2) numerically (an independent route); y'' + y (cos x, sin x), y'' + y^3 (the energy).
- Gaps: densities of non-evolution equations (the homotopy operator in several variables), several dependent
  variables.

## Test notes

- The core engine's printed form shows `mul(-1, t, x)` inside a list as `-1*t*x` where the REPL prints `-t*x`
  (the KdV/Burgers generator test keeps the printed form); not in this crate.

## Differential polynomials and rankings (g31.8)

- Differential polynomials are ordinary expressions in the user's notation (`y'(x)`, `derivative(1, 0)(u)(x, t)`),
  over the base field of rational functions of the independent variables; inside they are polynomials in the jet
  symbols. A ranking is `orderly` (total order, then the earlier function of the list, then the multi-index
  lexicographically) or `elimination` (the earlier function first); each jet gets an integer weight, so a ranking
  compares by one number. Leader, initial (coefficient of the top power), separant (derivative by the leader).
- `differential_reduce` is Ritt's full reduction (Kolchin, *Differential Algebra and Algebraic Groups*, I.9): the
  highest proper derivative of a leader is eliminated by pseudo-division with the corresponding derivative of the
  divisor (linear in it, with the separant as leading coefficient), then the leaders' degrees by pseudo-division by
  the polynomials themselves; the remainder's content in the base field (the gcd of its coefficients) is removed
  and its leading numerical coefficient made positive. Delta-polynomials s_q D p - s_p D q at the least common
  derivative (Rosenfeld, 1959; Boulier et al. 1995, 2.4).

## Rosenfeld-Groebner (g31.9)

The algorithm of Boulier, Lazard, Ollivier and Petitot (*Representation for the radical of a finitely generated
differential ideal*, ISSAC 1995), in the form with three-way splitting: from a branch [P, A, H] the lowest element
of P is fully reduced by A; a nonzero remainder p with leader v gives (1) A with p, the elements of A whose leaders
are derivatives of v moved back to P, the Delta-polynomials of p with A added, H with init(p) and sep(p); (2) P with
p and sep(p); (3) P with p and init(p). A remainder in the base field ends the branch (a nonzero number) or is
dropped (a function of the independent variables is a unit of the base field). At the end the inequations are
reduced by the chain and the branch is dropped if one reduces to 0. Components may be redundant (no inclusion test
between them; the 1995 paper allows that). Checked: y'^2 = 4 y (the general component with y' != 0 and the
singular y = 0, the paper's example), inconsistent PDE systems (u_x = u, u_t = x u and u_x^2 = 4 u, u_t = u give
u = 0 by hand computation of the integrability conditions), an elimination ranking decoupling a linear system.

Gaps: no Lazard-lemma regularity test of the output chains (they are regular when the separants and initials are
invertible modulo the chain, which the final reduction checks only by nonvanishing of their remainders), no
redundancy removal, order bound of 3 above the input (derivatives beyond it are not formed).

## Thomas decomposition (g31.10)

For one independent variable: Thomas (*Differential systems*, 1937; *Systems and roots*, 1962) as described by
Baechler, Gerdt, Lange-Hegermann and Robertz (*Algorithmic Thomas decomposition of algebraic and differential
systems*, J. Symb. Comp. 47, 2012). A reduced equation r with leader v of degree k splits disjointly into
init(r) = 0 (r without its top term goes back) and init(r) != 0; then the discriminant resultant(r, r_v, v) = 0
(r replaced by its square-free part when it has a repeated factor; otherwise the discriminant joins the
equations) or != 0, when r joins the chain. Initials and discriminants are made square-free in their leaders. With
a single derivation all derivatives are multiplicative, so a triangular square-free system is passive.
Gap: several independent variables (Janet division, involutive completion) are not implemented; in the
discriminant-zero branch the square-free part is the generic one (not recomputed under the new equation).

## The Painleve test (g31.11)

Ablowitz, Ramani and Segur, *A connection between nonlinear evolution equations and ordinary differential
equations of P-type I, II* (J. Math. Phys. 21, 1980); Conte and Musette, *The Painleve Handbook* (2008), 2.
The equation (numerator, polynomial in the jets) is split into terms; a term c(x) y^e0 y'^e1 ... has the exponent
alpha + beta p under y = a tau^p; every pair with different beta gives a candidate p < 0 at which that pair is
minimal (dominant balance, the dominant terms all those at the minimum). a solves the dominant terms' leading
coefficient (coefficients at x0); Q(r) the linearisation of the dominant terms (falling factorials p (p - 1) ...);
at integer p and integer resonances the truncated Laurent series a0 tau^p + sum_(j <= rmax) c_j tau^(p + j), with
x = x0 + tau in the coefficients, is substituted, multiplied by tau^(-m) and expanded by `series`; the coefficient
of tau^j is solved for c_j, and at a resonance its remainder is a compatibility condition (reported when nonzero).
Checked: P-I, P-II (two branches), y'' = 6 y^2 + a x^2 (condition a = 0), y'' = y^4 (p = -2/3), Chazy III
(resonances -1, -2, -3), Duffing.
Gaps: negative resonances are not examined further (the perturbative Fuchsian test); balances where a coefficient
vanishes at x0, and non-polynomial equations; systems of equations.

## Checks

The crate offers Lie point symmetries (determining equations, symmetry algebras, canonical coordinates, similarity
reductions), conservation-law multipliers, differential-algebraic decompositions (Rosenfeld-Groebner, Thomas) and
the Painleve test as user functions. Each is checked against worked examples of the cited literature and by an independent route (generators substituted into the determining equations, commutators
closing, solutions substituted into the equation, conservation laws evaluated on the KdV soliton).

## Speed

Every documentation input runs in under 0.4 s (the slowest: multipliers of order 2 for KdV, 0.3 s). The solver's
cost grows with the number of determining equations; equations of order 4 and higher in two variables, or systems,
may take seconds.

## Guide

`docs/guides/14-symmetries-and-differential-algebra.mcn` (the number may need renumbering when the round's other
guides are merged).
