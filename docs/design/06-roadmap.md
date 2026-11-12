# MathCAS v2: implementation roadmap (November 2026, after 0.9.0)

The core, the notebook and a broad base library exist (phases 1-8 and 10). What remains is depth in the symbolic core
and breadth in the long tail. This document records how the library work is ordered, where v2 stands, the phases
still ahead, and how each step is tested.

## 1. Method

Work is ordered by dependency. A function at level 0 uses no other public function; a function at level *n* uses
functions up to level *n* - 1, with cycles collapsed into one level.

What this is used for:

- **Order of work**: build what many functions stand on first.
- **Order of testing**: test the lower layers first, so a fault found higher up can be traced down.
- **Grouping**: functions that use each other are built and tested together.

The order is about work and tests, nothing more: v2's semantics and algorithms are decided in its own design
documents (`00-plan.md`).

### 1.1 What the layering shows

- Most documented functions in the five scope areas sit at level 0 or 1: a broad base of foundations, then a thin
  tail of higher layers.
- The library is strongly mutually recursive. One large cycle joins solvers (`solve`, `reduce`), linear algebra,
  special functions, probability distributions, and objects with rich display. Phase 3 built the base of this cycle;
  its depth (general solving, simplification, equations, integration) is phases 11-14.
- Visualisation stands on numerics: its most depended-on foundations include numerical integration and minimisation,
  graphics primitives and colour handling.

### 1.2 Where v2 stands (November 2026)

At the start (M1) v2 had 170 names. It now catalogues about 990 functions and options, each with a help entry, and
about 700 of them with a documentation page.

What is still missing falls into three groups: functions v2 does not have yet (the largest clusters: language
internals such as `unevaluated` and the value tables, Heun, Mathieu and spheroidal functions, Weierstrass functions,
integral transforms, asymptotic relations, convex optimisation), results of existing functions that stay unevaluated
or are wrong (missing derivative rules, series, function expansions), and output forms (the same value printed in a
longer or less natural form).

### 1.3 Scope and goals

- **The five scope areas**: core language and structure, data manipulation and analysis, visualisation and graphics,
  symbolic and numeric computation, higher mathematical computation. Every function in them has a documentation page
  whose examples are checked.
- **Robustness on arbitrary input is a separate, open-ended goal.** Correct documented examples do not mean correct
  answers on every input; the testing probes (random inputs checked by substitution, identities and higher
  precision) measure it.
- **Out of scope:** machine learning and language models, network and cloud services, knowledge-base data, user
  interfaces beyond the notebook.
- **Symbolic integration is a main goal** (phase 14). Today antiderivatives of rational functions are complete
  (Hermite reduction, Rothstein-Trager with real output); everything else is a table, substitution, parts and
  trigonometric rules, and there is no definite integration beyond the fundamental theorem. Other functions call
  `integrate` internally (definite sums, `d_solve`, distributions, expectations, some limits), so its depth improves
  them too. Numerical integration (`n_integrate`) has its own plan, `10-integration.md`.

## 2. Phases

Each phase lists its targets by **v2 name**, is split into steps that can proceed independently (no two steps change
the same crate), and ends with its exit criteria. Phase numbers are an order of dependency, not of calendar: a phase
can start as soon as what it uses exists.

### Done: phases 1-8 and 10

| Phase | Content |
|---|---|
| 1 foundations | predicates and structure, sets and associations, numbers and strings (levels 0-1) |
| 2 calculus foundation | `d`, `simplify` and the trigonometric and exponential transforms, `limit` for rational and elementary functions, `integrate` (rational complete, elementary by rules), `apart` |
| 3 the intertwined core | linear algebra (exact, rational, ball and machine), `solve` (radicals to degree 4, linear systems, Groebner bases), `reduce` for one real variable, special functions, distributions |
| 4 numerics | `n_integrate`, minimisation and root finding, `n_dsolve` (explicit and stiff), `interpolation`, all compiled where they can be (`07-numerics.md`) |
| 5 visualisation and data | the graphics model, 2D and 3D plots, the notebook, datasets, CSV and JSON |
| 6 checking | a runner for large sets of example inputs with known answers: per-case tracing, progress, limits and crash diagnosis (section 4) |
| 7 speed | compiled special functions and ODE right-hand sides, packed complex arrays, parallel `table` and `map` |
| 8 notebook maturity | documentation in the editor, typeset input, `manipulate`, 3D rotation, HTML and PDF export, plots as SVG |
| 10 platforms | Windows installer and zip, Linux tarball and AppImage; no Android port |

Release 0.4.0 also shipped steps 1-2 of `10-integration.md`: cubature, quasi-Monte Carlo and VEGAS in
`n_integrate`.

### Phase 9: coverage (in progress)

Fix failing examples of functions v2 has, and add the missing functions that the most examples need. Done so far:
regions, new plot types, multivariate `factor` and `minimize`, 150 documentation pages, and a breakdown by area
(9A, section 1.2).

Remaining:

| Step | Targets |
|---|---|
| 9B core and data (first part done in round 15: throw and catch, sow and reap, repeated and key-value patterns, one matcher, 31 functions) | the failing examples of core language and data pages (semantics edge cases, formatting, associations and datasets, import and export formats) |
| 9C visualisation | the remaining plot types and options, graph layout, mesh regions |
| 9D numerics | `10-integration.md` steps 3-5: 1D robustness (endpoint and interior singularities, oscillatory integrands), PSLQ and constant recognition, niche methods as packages |
| 9E documentation | the remaining documentation pages, about 290 functions |

Exit: every function v2 has with a documentation page whose examples check (`mc-docs-pages check`); the failing
examples of core language, data and visualisation fixed or recorded as design decisions.

### Phase F: algebraic foundations (before 11 and 14)

The polynomial core is solid (sparse multivariate polynomials over the integers, modular gcd, univariate and
multivariate factorisation, Groebner bases with FGLM, resultants, real root isolation, rational integration), but
phases 11 and 14 need one algebraic layer that today exists only in pieces: `Q[t]/(f)` in `mc-linalg`, quadratic
fields in the rational integrator, radicals in `mc-arith`. Without this phase, CAD and Risch would each build their
own algebraic numbers. It runs alongside phase 9.

| Step | Targets |
|---|---|
| F1 number fields (new `mc-algebra`) | one number-field type for the whole system: algebraic numbers as elements of `Q[t]/(f)` with arithmetic, inverses, minimal polynomials, primitive elements of towers, embeddings (which real or complex root is meant); gcd and factorisation of polynomials over a number field (Trager's norm method); a `root_sum` result (a sum over the roots of a polynomial), so rational integration no longer gives up on residues of degree 3 or more, and `sum` and `r_solve` can use it; `mc-linalg` and the rational integrator moved onto this layer. Policy for answers: roots of linear and quadratic factors become radicals, factors of degree 3 or more stay as `root_sum` (cubic and quartic formulas only when asked), which keeps antiderivatives and sums small |
| F2 polynomial rings (`mc-poly`) | polynomials over generic coefficient rings (integers, rationals, rational functions in parameters, number fields, integers modulo a prime, and rings of functions for 14A); principal subresultant coefficients exposed, not only the resultant; real root isolation of polynomials with algebraic-number coefficients and exact sign decision of algebraic numbers; functional decomposition of univariate polynomials (`f = g∘h`), which 14A uses to find substitutions; sample points as validated floating intervals first, with exact algebraic numbers as the fallback when the intervals cannot separate |
| F3 decisions and conditions (`mc-eval`, `mc-solve`) | one service answering sign, zero, interval and integrality questions about an expression under the assumption context, used by `refine`, `simplify`, `limit`, `integrate` and `solve` (today these live in three places); it decides with exact arithmetic and root isolation now, and calls `reduce` when 11B is there; conditional results (a value with the condition under which it holds) as a term with printing, simplification and `normal`; condition accumulation: a per-call context in which methods record the conditions they need (for example `re(a) > 0` for a convergent integral), simplified and reduced once by the caller at the end and attached as one conditional result; the assumption context compiled once per call (equations into a Groebner basis in degree-reverse-lexicographic order, against which candidates are reduced; inequalities into interval bounds for sign questions) |

Exit: the existing suite unchanged on the new layer; rational integrals with residues of any degree answered (as
`root_sum` when no radicals exist); factorisation over number fields checked by expansion; the sign service used by
every caller listed in F3.

Literature: Trager, "Algebraic factoring and rational function integration" (1976); Cohen, *A Course in
Computational Algebraic Number Theory*; von zur Gathen and Gerhard, *Modern Computer Algebra*; Ducos (2000) on
subresultant chains; Rouillier and Zimmermann (2004) on real root isolation.

### Phase 11: solving (a critical path)

`reduce`, `solve` and the exact optimisers rest on one algorithm: cylindrical algebraic decomposition (CAD). It is
one of the two hardest pieces left (with phase 14), and everything in phases 12-14 that reasons about conditions
depends on it.

| Step | Targets |
|---|---|
| 11A CAD (`mc-poly`) | before CAD: equations solved first where they are triangular (Groebner preprocessing), input factored; projection (McCallum's, with Brown's improvement, and equational constraints) on F2's subresultant coefficients, resultants and discriminants reduced to a square-free basis by factoring; lifting with sample points (F2's validated intervals first, exact root isolation over number fields as the fallback), cell combination, cell adjacency where needed; quantifier elimination over the reals; fast paths before CAD: linear and quadratic virtual substitution (Loos-Weispfenning, Weispfenning), simplex for linear systems, and a sum-of-squares decision for `for_all` of one polynomial |
| 11B `reduce` and friends | `reduce` for several real variables, quantifiers (`for_all`, `exists`), integers and complexes where decidable; `find_instance`; `resolve`; `eliminate` over the reals; region membership and emptiness through `reduce`; over the integers: bounds first, then binary quadratic forms (Pell equations through the fundamental unit), Thue equations and bounded search |
| 11C general `solve` | `solve` and `reduce` on one front end (input validation, domains and quantifiers normalised, vector equations to scalars), with `solve` as the generic-solution mode of that machinery and `reduce` the complete one; polynomial systems: a linear fast path first, factoring into a disjunction, lex Groebner bases with the Rabinowitsch variable for `≠` conditions, triangular root solving for zero-dimensional bases, and splitting on leading coefficients for parameters (conditional results); transcendental equations by substituting a new variable for the transcendental kernel, solving, and inverting with branch conditions and generated integer parameters; real transcendental equations through 11B: a `product_log` normal form for `a·e^b = c`, monotone substitutions, and exact real root isolation for exp-log functions (roots that have no closed form stay as isolated transcendental roots); `solve` over the reals and integers through 11B; verification of solutions by substitution |
| 11D exact optimisation | `minimize` and `maximize` with polynomial constraints through CAD; `region_bounds` and `region_measure` of semialgebraic sets |

Exit: the documentation pages of `reduce`, `solve`, `find_instance`, `minimize` and `maximize` check; every `solve`
result checked by substitution in the shared suite.

Literature: Collins (1975); McCallum (1988); Brown (2001); Weispfenning (1988) for virtual substitution; Strzebonski's
papers on CAD with algebraic coefficients, on validated sample points and on real root isolation of exp-log
functions; Basu, Pollack and Roy, *Algorithms in Real Algebraic Geometry*; Weispfenning (1992) and Kapur, Sun and
Wang (2010) on comprehensive Groebner systems.

### Phase 12: simplification (after 11B)

| Step | Targets |
|---|---|
| 12A `full_simplify` (`mc-simplify`) | a small fixed search: each subexpression, bottom up, is rewritten by a short list of transformations (`together`, `factor` and square-free factoring, `apart`, power rewriting, then the trigonometric, exponential and logarithmic rewritings and `function_expand`), and a candidate is kept only when the complexity measure drops; a time limit and recursion guards. The breadth comes from 12C's rule tables, not from a cleverer search |
| 12B assumptions | `assuming`, `refine` and the `assumptions:` option decided through `reduce` (sign, interval and integrality facts), on F3's assumption context compiled once per call (equational assumptions as a Groebner basis that candidates are reduced against, inequalities as interval bounds); simplification of `sqrt`, `log` and powers under assumptions |
| 12C special-function identities | contiguous relations, reflection and duplication formulas, hypergeometric reduction to named functions, as a priority-ordered table of guarded rules (each guarded by assumption predicates such as positivity or integrality) plus one simplifier per family (gamma, polygamma, hypergeometric, Bessel, zeta, polylog, algebraic values of trigonometric constants), used by `full_simplify` and `function_expand`; rules taken from the literature (DLMF; Prudnikov, Brychkov and Marichev) |

Exit: the documentation pages of `simplify`, `full_simplify`, `refine` and `function_expand` check, and every
simplification in the suite is checked at random points. Simplification quality is open-ended: after this phase it
improves case by case from failing examples.

### Phase 13: equations, sums and limits

13B and 13C do not need phase 11 and can start at once; 13A needs 11C for solving conditions. 13C is also a
prerequisite of 14D: definite integration is mostly limit calls (convergence at the endpoints, values of the
antiderivative at the endpoints, degenerate cases of the Meijer G formulas), so 13C should land before 14D starts. The quadratures in
`d_solve` results and the definite sums of 13B use phase 14 as it grows; until then they stay as unevaluated
integrals.

| Step | Targets |
|---|---|
| 13A `d_solve` (`mc-calculus`) | beyond today's four classes (constant coefficients, Euler, first-order linear, separable): exact, Bernoulli, Riccati and homogeneous equations; Kovacic's algorithm for second-order linear equations; Lie symmetry methods for first order; series solutions at ordinary and regular singular points; linear systems with constant coefficients (matrix exponential); boundary-value conditions |
| 13B sums and recurrences | Gosper's algorithm for indefinite hypergeometric sums; Zeilberger's algorithm for definite sums; Petkovšek's Hyper for recurrences with polynomial coefficients; `r_solve` beyond constant coefficients; `product` in closed form |
| 13C limits and series | Gruntz's algorithm (comparability classes, the most rapidly varying subexpression) for `limit` of exp-log functions; `series` of special functions; asymptotic expansions (`asymptotic` series at infinity) |

Exit: the documentation pages of `d_solve`, `r_solve`, `sum`, `product`, `limit` and `series` check; every `d_solve`
result checked by substitution.

Literature: Kovacic (1986); Bronstein, Mulders and Weil on Kovacic's algorithm; Cheb-Terrab and others on symmetry
methods; Gosper (1978); Zeilberger (1991); Petkovšek, Wilf and Zeilberger, *A = B*; Gruntz (1996).

### Phase 14: symbolic integration

The rational case is the base of the Risch algorithm and is done. This phase adds the decision procedure for
elementary integrands, the special functions that non-elementary answers need, and definite integration. 14A needs
F1 and F2 (residues in number fields, polynomials over rings of functions); 14D can start after F3 and 13C; 14C needs
14A; the conditions of 14D use the sign service (F3), and `reduce` (11B) when it is there.

Integration is planned as a portfolio behind one driver, not as one algorithm. The tracks are tried in a fixed
order, each with its own time budget, each failing silently into the next: a quick table of common forms, the
tower-based engine (14A, which covers rational and exp-log integrands and special-function answers), the
substitution portfolio for algebraic integrands (14B), and the rule table (14C). Every antiderivative is verified by
differentiation before it is returned.

| Step | Targets |
|---|---|
| 14A tower-based engine (`mc-calculus`) | the differential field of an integrand: each subexpression classified as an exponential, logarithmic (primitive), algebraic or trigonometric extension (trigonometric and hyperbolic functions through `exp`, rewritten back to real form at the end), with polynomial decomposition (F2) to find inner functions; rational parts by Hermite reduction and Lazard-Rioboo-Trager (F1's `root_sum`); one engine for the transcendental part with two modes: the Risch decision procedure (Risch differential equation and its parametric version; polynomial, reduced and simple parts; a proof of non-elementarity) and the parallel Risch-Norman ansatz over the same tower; the tower admits special functions as extensions with known derivatives (`erf`, `erfc`, `erfi`, exponential and logarithmic integrals, sine and cosine integrals and their hyperbolic forms, Fresnel integrals, incomplete gamma, the elliptic integrals `elliptic_f`, `elliptic_e`, `elliptic_pi`), so special-function antiderivatives come out of the engine and not out of tables |
| 14B algebraic integrands | roots and algebraic functions: a portfolio of substitutions tried in turn and scored (derivative-divides; rationalising and Euler's substitutions for linear, linear-fractional and quadratic radicals; Chebyshev's theorem for binomial differentials; Goursat's pseudo-elliptic quartic cases; lowering the radicand's degree; undetermined coefficients for the rational part and a Groebner step for the logarithmic part; Trager's method for simple radical extensions), each answer verified by differentiation; elliptic integrals in Legendre form for the rest |
| 14C rule table | a rule-based integrator for the forms the engines do not reach (powers and products of trigonometric functions, mixed algebraic-transcendental forms, special-function answers), written in v2's pattern language; the candidate source is Rubi (Rich's rule-based integrator, about 7,000 rules and a test suite of about 70,000 integrals), ported from its public sources if its licence is confirmed GPL-compatible; its test suite becomes the integration benchmark. Prerequisite: the matcher handles Rubi's optional and default arguments, conditions and utility predicates; `hypergeometric_pfq`, incomplete beta and `polylog` answers through the rules |
| 14D definite integration | one driver: integrand simplified on the range, convergence checked first (limits at the endpoints, series at finite points, a locus of interior singularities and branch-cut crossings), conditions recorded in F3's context, then an ordered cascade: trivial cases and linearity; rational integrands over the real line by Hermite reduction and residues in the upper half plane as a `root_sum`; trigonometric integrands over a period by `z = e^{ix}` and residues; Fourier-type integrands by Jordan's lemma; Gaussian and exponential closed forms; Beta-function forms of binomials; antiderivative and limits (Newton-Leibniz) only after splitting at the interior singularities and cut crossings; periodic integrands; substitutions (`x → 1/x`, `x → -log t`); Meijer G-function methods (products of two G-functions over (0, ∞), finite ranges through a step-function G) as the broad last method, with each formula's validity conditions recorded; results with conditions on parameters (`assumptions:` and conditional results); principal values; multiple integrals iterated. The first items (residues, `z = e^{ix}`, Jordan) need only F1 and are an early win |
| 14E transforms | `laplace_transform`, `fourier_transform`, `mellin_transform` and their inverses through 14D, with the tables they need |

Exit: the documentation pages of `integrate` and of the transforms check; every antiderivative checked by
differentiation and every definite integral checked against `n_integrate` in the shared suite.

Literature: Bronstein, *Symbolic Integration I: Transcendental Functions*; Risch (1969, 1970); Davenport, *On the
Integration of Algebraic Functions*; Trager (1984); Norman and Moore (1977) and Bronstein (2007, "pmint") on parallel
integration, including special functions in the tower; Rich, Scheibe and Abbasi (2018) on Rubi; Goursat (1887) and
Chebyshev (1853) for the algebraic cases; Adamchik and Marichev (1990) and Roach (1996, 1997) on Meijer G integration; Prudnikov, Brychkov and
Marichev, *Integrals and Series*, for tables.

### Phase 16: parallel symbolic engines and the precision contract

Phase 7 made the numeric layer fast: the compiler runs automatically inside `n_integrate`, `find_root`, `n_minimize`,
`n_dsolve`, plots and regions, and `table`, `map`, number theory and `n_integrate` batches run in parallel. The
engines built in phases F and 11-14 are sequential, and some of their numeric paths stop at machine precision. This
phase brings them under the same two promises as the rest of the system.

| Step | Targets |
|---|---|
| 16A parallel symbolic engines | the independent work of the symbolic engines run on the session's thread pool, with results identical for any number of threads (semantics 18.1) and fuel and interrupts honoured on every thread: CAD's lifting over the cells of each level, the candidate search of `simplify` and `full_simplify`, the method portfolios of `integrate` and `d_solve` (methods raced under their time budgets, the first verified answer in the fixed order kept), the verification of solutions and antiderivatives at random points, `reduce` over the disjuncts of a formula; a measured speed-up on multi-core machines and no slowdown on one core |
| 16B the precision contract | every numeric path of the new engines honours arbitrary precision: `n` of `root_sum` and other exact answers to the digits asked; `working_precision:` accepted and honoured by every numeric function (`find_root`, `n_minimize`, the oscillatory stages of `n_integrate`); special functions at complex and negative arguments to any precision; property tests that a result at 30 digits agrees with the result at 60 and that its ball contains the true value |

Exit: 16A, a speed-up of at least 2 on four cores on the slowest symbolic cases of the benchmark set, with identical
results; 16B, the precision probes of the testing method passing for every function in the new engines.

### Phase 15: breadth (alongside 11-14)

Independent steps, each in its own crate, that can proceed alongside the critical path.

| Step | Targets |
|---|---|
| 15A special functions | every special function in scope to arbitrary precision across the complex plane: regions, asymptotic expansions, branch cuts, exact special values and derivatives |
| 15B higher mathematics | number theory, finite fields, polynomial algebra over them, group theory, discrete mathematics and graph algorithms within the scope areas |
| 15C geometry and regions | computational geometry, meshes and their measures, region operations beyond what 11D decides |
| 15D data | the remaining dataset queries, time series, import and export formats in scope |

Exit: per step, every function with a documentation page whose examples check, and tests.

### Out of the plan

- The complete algebraic case of Risch integration (Trager's and Bronstein's general algorithms): 14B covers the
  common cases; the rest is decided after phase 14.
- Partial differential equations beyond what `d_solve`'s documented uses need: decided after phase 13.

## 3. Order of the remaining work

### 3.1 Critical chains

Two chains are sequential and set the pace: 11A → 11B → 11C → 12 → 13A, and 14A → 14D, with 14B, 14C, 14E, phase 9
and phase 15 proceeding beside them. The first estimates of this roadmap, in weeks and months, were far too slow:
phase F was estimated at 3-5 weeks and took one day.

Since round 20 rounds alternate: **A rounds** add features and fix bugs, **B rounds** write documentation and find
bugs; a B round that finds more than about five wrong values is followed by a round of fixes only.

### 3.2 Quality goals (10 November 2026)

Quality means three things, measured separately:

1. **Documented examples.** Every example on every documentation page evaluates to its stored output
   (`mc-docs-pages check`).
2. **Robustness on undocumented input.** Measured by each B round's count of wrong values (26 in round 23); the goal
   is that count near zero across the probes of the testing method.
3. **Algorithmic depth and speed** (hard integrals, simplification quality, solving in several variables, special
   functions everywhere in the complex plane, the benchmark set). Open-ended; judged per workload, not as a whole.

Stages:

| Stage | Work |
|---|---|
| A. Coverage | the most needed missing functions in batches of about 50 (as the visualisation batch of round 18, 178 functions at once), each in its own crate or module, with pages and tests |
| B. Depth | the wrong and unevaluated results of existing functions, grouped by engine (integration, simplification, solving, special functions, data), in rounds of fixes |
| C. Long tail and forms | the remaining functions and output forms |

Robustness (goal 2) runs through the B rounds and speed (goal 3) through the benchmark checks, alongside all three
stages.

Progress (11 November 2026): stage A added 1,709 functions in two rounds (rounds 28 and 29); stage B starts with
round 30.

### 3.3 Specialist areas (from November 2026)

Areas that are usually served by specialist systems: a separate track, one or two steps per round alongside stage B,
each in its own crate, with its own tests and documentation pages (and a guide per area). Exit for each: the listed
functions with their pages, checked against the literature's worked examples and by internal consistency (orders by
the orbit-stabiliser theorem, solutions by substitution, identities at random points).

| Track | Crate | Content, in order |
|---|---|---|
| G. Group theory and combinatorics | `mc-groups` | permutations (`cycles`, products, orders, signs); permutation groups by generators with Schreier-Sims (base and strong generating set, order, membership, orbits, stabilisers, cosets, normal closure, centraliser, derived and lower central series, composition series, solvability, transitivity, blocks and primitivity); named groups (symmetric, alternating, cyclic, dihedral, abelian, Mathieu and the small sporadic ones as permutation groups); finitely presented groups (Todd-Coxeter coset enumeration, Knuth-Bendix completion for rewriting systems, abelian invariants, low-index subgroups); character tables (Dixon-Schneider) and conjugacy classes; combinatorics: posets (Möbius function, linear extensions, lattices), matroids (rank, bases, circuits, Tutte polynomial), integer partitions and Young tableaux (Robinson-Schensted, hook length), Pólya counting with cycle indices |
| D. Differential equations and differential algebra | `mc-diffalg` | Lie point symmetries of ODEs and PDEs (determining equations, their solution, the symmetry algebra and its commutator table), reduction of order and similarity reductions by a symmetry, conservation laws (multipliers), equivalence of equations by symmetry; differential algebra: differential polynomials and rankings, differential Gröbner-style reduction, Rosenfeld-Gröbner decomposition into regular differential chains, Thomas decomposition, consistency and the dimension of solution spaces of DE systems; integrability tests (Painlevé test for ODEs) |
| H. Holonomic functions and creative telescoping | `mc-holonomic` | Ore algebras (shift, differential, q-shift operators; noncommutative multiplication, division, gcrd, lclm); holonomic closure: sums, products, compositions with algebraic functions, definite sums and integrals by creative telescoping (Zeilberger and Almkvist-Zeilberger generalised, Chyzak's algorithm, Koutschan's ansatz); guessing recurrences and differential equations from terms; conversion between recurrences, differential equations and series; proofs of special-function identities by annihilator equality plus initial values; asymptotics of holonomic sequences |
| T. Tensor algebra | `mc-tensors` | abstract-index tensors with declared symmetries (symmetric, antisymmetric, Young-projected), canonicalisation of tensor monomials with dummy indices (the Butler-Portugal double-coset algorithm), contraction, metric raising and lowering, covariant derivatives, Christoffel symbols, Riemann, Ricci, Weyl and Einstein tensors from a metric (components and abstract), Bianchi identities, spinors and gamma-matrix algebra (Fierz, traces) |

Literature: Seress, *Permutation Group Algorithms*; Holt, Eick and O'Brien, *Handbook of Computational Group
Theory*; Sims, *Computation with Finitely Presented Groups*; Olver, *Applications of Lie Groups to Differential
Equations*; Bluman and Anco, *Symmetry and Integration Methods for Differential Equations*; Boulier, Lazard, Ollivier
and Petitot on Rosenfeld-Gröbner; Bächler, Gerdt, Lange-Hegermann and Robertz on Thomas decomposition; Zeilberger
(1990, 1991); Chyzak (2000); Koutschan (2009); Kauers and Paule, *The Concrete Tetrahedron*; Butler and Portugal on
tensor canonicalisation; Penrose and Rindler, *Spinors and Space-Time*.

### 3.4 Applied toolkits built from existing engines (from November 2026)

Features for mathematicians and physicists that combine engines v2 already has, so each is mostly a library of
its own (`.mc` code plus small natives) rather than a new algorithm family. One step per round beside stage B
and section 3.3; each with a guide, pages, and checks by substitution or by an independent route.

| Toolkit | Built from | Content |
|---|---|---|
| M1. Identity prover | Zeilberger and Gosper, difference/differential roots, `simplify`, random-point checks | `prove_identity(lhs == rhs)`: a proof (WZ certificate, annihilator plus initial values, or a simplification chain) or a counterexample |
| M2. Inequality prover | CAD and `reduce`, sums of squares, ball arithmetic | `prove_inequality`: a proof with a sum-of-squares or CAD certificate, or a counterexample point |
| M3. Certified numerics | balls, interval Newton (Krawczyk), root isolation, ODE enclosures | `verified_` integrals, roots (with uniqueness) and ODE values, each with a guaranteed error bound |
| M4. Constant recognition | high-precision `n`, integer relations (PSLQ), a basis of constants | `identify(x)`: closed forms guessed from 50+ digits, handed to M1 for proof |
| M5. Asymptotics of sums and integrals | Gruntz limits, `series`, Euler-Maclaurin | Laplace's method, saddle points, Euler-Maclaurin expansions, asymptotics of combinatorial sums |
| M6. Exact normal forms | number fields, finite fields, exact linear algebra | Smith and Hermite normal forms, Jordan and rational canonical forms over extensions |
| P1. Analytical mechanics | `d`, variational derivatives, `d_solve`, `n_dsolve`, plots | Euler-Lagrange and Hamilton equations from a Lagrangian, Noether charges, Poisson brackets, phase portraits |
| P2. Perturbation theory | `series`, `d_solve`, `collect` | regular perturbation, Lindstedt-Poincaré, multiple scales, WKB, to any order |
| P3. Units through the pipeline | quantities, `solve`, `integrate`, `d_solve` | calculus with units checked, dimensional analysis, Buckingham pi groups |
| P4. Angular momentum and spin | Clebsch-Gordan, 3j/6j symbols, Wigner D, Pauli matrices, matrix exponentials | coupling of angular momenta, spin Hamiltonians, rotations of states, time evolution |
| P5. Operator algebra | non-commutative products, `rewrite` | commutators, ladder operators, normal ordering, Baker-Campbell-Hausdorff |
| P6. Loop-integral helpers | gamma and hypergeometric series at poles, Mellin-Barnes, Meijer G | epsilon expansions in dimensional regularisation, one-loop scalar integrals |
| P7. Eigenproblems | `nd_eigensystem`, special functions, `green_function` | Sturm-Liouville eigenvalues and eigenfunctions, Green's functions, spectral expansions |

Order: P1, M4 with M1, P2, M3, P4 with P5 first (the most value for the least new code), then the rest.

## 4. Testing

- **Checks by an independent route, bottom-up.** For each phase's targets, example inputs are written with their
  expected values and checked by meaning (exact equality, numeric agreement to the stated digits, or a property such
  as "substituting the solution gives zero"). They enter the repository as ordinary tests.
- **Lower layers first.** A failing higher-level check is investigated by checking its dependencies first.
- **Design decisions** (`01-semantics.md`: no sequence splicing, read-time names, ball contagion, AC rest variables
  binding the head) are tested as specified, and checked by meaning, never by exact text.
- **Documentation pages are re-checked after each phase**: every page's examples evaluated again and compared with
  their stored outputs (`mc-docs-pages check`).
- **Solvers are checked by substitution**: every result of `solve`, `reduce`, `d_solve` and `r_solve` in the shared
  suite is substituted back, so a wrong answer fails even when no expected value was written for it.
- **Antiderivatives by differentiation, definite integrals against `n_integrate`, numeric values at a higher
  precision**: a result at 30 digits must agree with the result at 60, and its ball must contain the true value.

## 5. Order and parallelism

- Critical paths: F (foundations) → 11A (CAD) → 11B (`reduce`) → 11C (`solve`) → 12 (simplification) and 13A
  (`d_solve`); and F → 14A (tower-based engine) → 14D (definite integration, with conditions from F3 and 11B and
  limits from 13C).
- Alongside them from the start: phase 9, 13B, 13C, 14B, 14C (rule table, once the matcher supports it) and
  all of phase 15. 13C should finish before 14D starts.
- Every method in the solving and integration cascades gets its own time budget under the stop scope, and fails
  into the next method instead of starving it. F1, F2 and F3 proceed independently
  (different crates); F2's root isolation over number fields uses F1's arithmetic as soon as it lands.
- Crates: phase F works on the new `mc-algebra` (F1), `mc-poly` (F2) and the decision code in `mc-eval` and `mc-solve`
  (F3); after it, phase 11 works on `mc-poly`, phase 12 `mc-simplify`, phase 14 the integration modules of `mc-calculus` (a new
  `mc-integrate` crate if 13 and 14 run at the same time, so no two steps change one crate), 15A `mc-numeric` and
  `mc-special`, new crates for 15B-15D where needed.
- Every step: its own branch, offline builds, guards and progress for heavy runs, notes saved as the crate's
  `NOTES.md`, every new function with a `FUNCTIONS` entry, a documentation page and tests.
