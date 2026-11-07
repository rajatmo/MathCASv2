# MathCAS v2: implementation roadmap (November 2026, after 0.4.0)

The core, the notebook and a broad base library exist (phases 1-8 and 10). What remains is depth in the symbolic core
and breadth in the long tail. This document records how the library work was ordered, where v2 stands against the
reference documentation's examples, the phases still ahead, and how each step is tested.

## 1. Method

The reference system's public functions were introspected with private tooling outside this repository (decision in
`02-benchmarks.md` section 3: only numbers enter here). For each function it records whether it is built in or written
in its own language, and which other public functions its definitions refer to directly. Collapsing cycles and
counting levels gives a dependency order: level 0 uses no other public function, level *n* uses functions up to level
*n* - 1.

What this is used for:

- **Order of work**: build what many functions stand on first.
- **Order of testing**: compare the lower layers first, so a disagreement found higher up can be traced down.
- **Packaging**: functions that use each other are built and tested together.

What it is **not** used for: designs. The reference system's layering reflects its implementation; v2's semantics
and algorithms are decided here, independently (`00-plan.md`). The graph orders work and tests, nothing more.

### 1.1 What the layering shows

- About 7,900 public symbols: half built in, half written in the language. Most documented functions in the five scope
  areas sit at level 0 or 1: a broad base of foundations, then a thin tail of higher layers.
- The library is strongly mutually recursive. Even counting only direct references, one cycle joins 449 symbols:
  solvers (`solve`, `reduce`), linear algebra, special functions, probability distributions, and objects with rich
  display. Phase 3 built the base of this cycle; its depth (general solving, simplification, equations,
  integration) is phases 11-14.
- Visualisation stands on numerics: its most depended-on foundations include numerical integration and minimisation,
  graphics primitives and colour handling.

### 1.2 Where v2 stands (November 2026)

At the start (M1) v2 had 170 names, covering 11% of the level-0 documented functions in scope. It now catalogues
about 990 functions and options, each with a help entry, and about 700 of them with a documentation page.

The private harness (phase 6) runs every example cell of the reference documentation pages in the five scope areas,
in both systems, and compares the results by meaning. The last full run (0.3.0 with the graphics round of phase 9):

| Scope | Pages | Evaluable cells | Passing | Rate |
|---|---|---|---|---|
| all pages in scope | 1,958 | 47,450 | 16,412 | 34.6% |
| pages of functions v2 has | 729 | 20,394 | 12,444 | 61.0% |
| the 300 most used pages | 300 | 9,659 | 6,084 | 63.0% |

Evaluable cells exclude noise and deliberate skips (server and curated-data functions, deliberately heavy examples,
reference timeouts). A cell that cannot be translated, has no comparable output, gives a different result, fails or
crashes counts as not passing. Release 0.4.0 is not yet measured, and the run has no split by area: both are exit
items of phase 9.

### 1.3 What parity means here

- **The target is the documented examples** of the five scope areas: core language and structure, data manipulation
  and analysis, visualisation and graphics, symbolic and numeric computation, higher mathematical computation. A
  passing example is the unit of progress; the harness rate is the measure.
- **Robustness on arbitrary input is a separate, open-ended goal.** The reference's symbolic algorithms have been
  hardened over decades; matching its documented examples does not mean matching it on every input.
- **Out of scope:** machine learning and language models, network and cloud services, knowledge-base data, user
  interfaces beyond the notebook.
- **Symbolic integration is a parity target** (phase 14). Today antiderivatives of rational functions are complete
  (Hermite reduction, Rothstein-Trager with real output); everything else is a table, substitution, parts and
  trigonometric rules, and there is no definite integration beyond the fundamental theorem. Other functions call
  `integrate` internally (definite sums, `d_solve`, distributions, expectations, some limits), so its depth raises
  their rates too. Numerical integration (`n_integrate`) has its own plan, `10-integration.md`.

## 2. Phases

Each phase lists its targets by **v2 name**, is split into packages that can run in parallel (no two packages own the
same crate), and ends with its exit criteria. Phase numbers are an order of dependency, not of calendar: a phase can
start as soon as what it uses exists.

### Done: phases 1-8 and 10

| Phase | Content |
|---|---|
| 1 foundations | predicates and structure, sets and associations, numbers and strings (reference levels 0-1) |
| 2 calculus foundation | `d`, `simplify` and the trigonometric and exponential transforms, `limit` for rational and elementary functions, `integrate` (rational complete, elementary by rules), `apart` |
| 3 the intertwined core | linear algebra (exact, rational, ball and machine), `solve` (radicals to degree 4, linear systems, Groebner bases), `reduce` for one real variable, special functions, distributions |
| 4 numerics | `n_integrate`, minimisation and root finding, `n_dsolve` (explicit and stiff), `interpolation`, all compiled where they can be (`07-numerics.md`) |
| 5 visualisation and data | the graphics model, 2D and 3D plots, the notebook, datasets, CSV and JSON |
| 6 harness | the private runner of the reference documentation's examples (section 3) |
| 7 speed | compiled special functions and ODE right-hand sides, packed complex arrays, parallel `table` and `map` |
| 8 notebook maturity | documentation in the editor, typeset input, `manipulate`, 3D rotation, HTML and PDF export, plots as SVG |
| 10 platforms | Windows installer and zip, Linux tarball and AppImage; no Android port |

Release 0.4.0 also shipped steps 1-2 of `10-integration.md`: cubature, quasi-Monte Carlo and VEGAS in
`n_integrate`.

### Phase 9: coverage (in progress)

Raise the harness rate by fixing failing examples of functions v2 has, and adding the missing functions that the
most examples need. Done so far: regions, new plot types, multivariate `factor` and `minimize`, 150 documentation
pages.

Remaining:

| Package | Targets |
|---|---|
| 9A measurement | a full harness run on the current release, split by the five areas; the table of section 1.2 updated; the failing cells of each area ranked by the number of examples they block |
| 9B core and data | the failing examples of core language and data pages (semantics edge cases, formatting, associations and datasets, import and export formats) |
| 9C visualisation | the remaining plot types and options, graph layout, mesh regions |
| 9D numerics | `10-integration.md` steps 3-5: 1D robustness (endpoint and interior singularities, oscillatory integrands), PSLQ and constant recognition, niche methods as packages |
| 9E documentation | the remaining documentation pages, about 290 functions |

Exit: core language and data at 90% or more of their evaluable cells, visualisation at 80% or more, every function
v2 has with a documentation page.

### Phase 11: solving (a critical path)

`reduce`, `solve` and the exact optimisers rest on one algorithm: cylindrical algebraic decomposition (CAD). It is
one of the two hardest pieces left (with phase 14), and everything in phases 12-14 that reasons about conditions
depends on it.

| Package | Targets |
|---|---|
| 11A CAD (`mc-poly`) | projection (McCallum's, with Brown's improvement), real root isolation of polynomials with algebraic-number coefficients, lifting with sample points, cell adjacency where needed; quantifier elimination over the reals; virtual substitution as a fast path for low-degree cases |
| 11B `reduce` and friends | `reduce` for several real variables, quantifiers (`for_all`, `exists`), integers and complexes where decidable; `find_instance`; `resolve`; `eliminate` over the reals; region membership and emptiness through `reduce` |
| 11C general `solve` | transcendental equations through inverse functions with branch conditions and generated parameters; equations with parameters (cases as conditional results); `solve` over the reals and integers through 11B; verification of solutions by substitution |
| 11D exact optimisation | `minimize` and `maximize` with polynomial constraints through CAD; `region_bounds` and `region_measure` of semialgebraic sets |

Exit: the harness rate of `reduce`, `solve`, `find_instance`, `minimize` and `maximize` pages at 80% or more; every
`solve` result checked by substitution in the shared suite.

Literature: Collins (1975); McCallum (1988); Brown (2001); Weispfenning (1988) for virtual substitution; Strzebonski's
papers on CAD with algebraic coefficients; Basu, Pollack and Roy, *Algorithms in Real Algebraic Geometry*.

### Phase 12: simplification (after 11B)

| Package | Targets |
|---|---|
| 12A `full_simplify` (`mc-simplify`) | a search over transformations (rational normal forms, trigonometric, exponential and logarithmic rewritings, special-function identities, `function_expand`), scored by a complexity measure, with a time limit |
| 12B assumptions | `assuming`, `refine` and the `assumptions:` option decided through `reduce` (sign, interval and integrality facts); simplification of `sqrt`, `log` and powers under assumptions |
| 12C special-function identities | contiguous relations, reflection and duplication formulas, hypergeometric reduction to named functions, as rules `full_simplify` and `function_expand` use |

Exit: the harness rate of `simplify`, `full_simplify`, `refine` and `function_expand` pages at 75% or more.
Simplification quality is open-ended: after this phase it improves case by case from failing examples.

### Phase 13: equations, sums and limits

13B and 13C do not need phase 11 and can start at once; 13A needs 11C for solving conditions. The quadratures in
`d_solve` results and the definite sums of 13B use phase 14 as it grows; until then they stay as unevaluated
integrals.

| Package | Targets |
|---|---|
| 13A `d_solve` (`mc-calculus`) | beyond today's four classes (constant coefficients, Euler, first-order linear, separable): exact, Bernoulli, Riccati and homogeneous equations; Kovacic's algorithm for second-order linear equations; Lie symmetry methods for first order; series solutions at ordinary and regular singular points; linear systems with constant coefficients (matrix exponential); boundary-value conditions |
| 13B sums and recurrences | Gosper's algorithm for indefinite hypergeometric sums; Zeilberger's algorithm for definite sums; Petkovšek's Hyper for recurrences with polynomial coefficients; `r_solve` beyond constant coefficients; `product` in closed form |
| 13C limits and series | Gruntz's algorithm (comparability classes, the most rapidly varying subexpression) for `limit` of exp-log functions; `series` of special functions; asymptotic expansions (`asymptotic` series at infinity) |

Exit: the harness rate of `d_solve`, `r_solve`, `sum`, `product`, `limit` and `series` pages at 75% or more; every
`d_solve` result checked by substitution.

Literature: Kovacic (1986); Bronstein, Mulders and Weil on Kovacic's algorithm; Cheb-Terrab and others on symmetry
methods; Gosper (1978); Zeilberger (1991); Petkovšek, Wilf and Zeilberger, *A = B*; Gruntz (1996).

### Phase 14: symbolic integration

The rational case is the base of the Risch algorithm and is done. This phase adds the decision procedure for
elementary integrands, the special functions that non-elementary answers need, and definite integration. 14A and 14D
can start at once; 14C needs 14A; the conditions of 14D use `reduce` (11B) when it is there and simple sign tests
before.

| Package | Targets |
|---|---|
| 14A transcendental Risch (`mc-calculus`) | the differential field of an integrand (towers of `exp`, `log` and, through them, trigonometric and hyperbolic functions, rewritten back to real form); the Risch differential equation and its parametric version; the polynomial, reduced and simple parts of each extension; a proof that an integral is not elementary, so the call stays as it is or moves on to 14C |
| 14B algebraic integrands | roots and algebraic functions: the cases with an elementary answer that the literature handles well (rationalising substitutions, Euler's substitutions, Chebyshev's theorem for binomial differentials, Trager's method for simple radical extensions); elliptic integrals in Legendre form for the rest |
| 14C beyond elementary | answers in special functions: `erf`, exponential, logarithmic, sine and cosine integrals, `polylog`, incomplete gamma and beta, `hypergeometric_pfq`; the Risch-Norman (parallel Risch) heuristic as a broad fallback; a larger rule table for the forms neither method reaches |
| 14D definite integration | integrals over intervals and half lines by antiderivative and limits, with checks for singularities and branch cuts inside the range; Meijer G-function methods (products of two G-functions over (0, ∞)) for the many closed forms that have no elementary antiderivative; results with conditions on parameters (`assumptions:` and conditional results); principal values; multiple integrals iterated |
| 14E transforms | `laplace_transform`, `fourier_transform`, `mellin_transform` and their inverses through 14D, with the tables they need |

Exit: the harness rate of `integrate` pages at 75% or more and of the transform pages at 70% or more; every
antiderivative checked by differentiation and every definite integral checked against `n_integrate` in the shared
suite.

Literature: Bronstein, *Symbolic Integration I: Transcendental Functions*; Risch (1969, 1970); Davenport, *On the
Integration of Algebraic Functions*; Trager (1984); Norman and Moore (1977) and Bronstein (2007) on parallel
integration; Adamchik and Marichev (1990) and Roach (1996, 1997) on Meijer G integration; Prudnikov, Brychkov and
Marichev, *Integrals and Series*, for tables.

### Phase 15: breadth (alongside 11-14)

Independent packages, each in its own crate, that can run in parallel with the critical path.

| Package | Targets |
|---|---|
| 15A special functions | every special function in scope to arbitrary precision across the complex plane: regions, asymptotic expansions, branch cuts, exact special values and derivatives |
| 15B higher mathematics | number theory, finite fields, polynomial algebra over them, group theory, discrete mathematics and graph algorithms within the scope areas |
| 15C geometry and regions | computational geometry, meshes and their measures, region operations beyond what 11D decides |
| 15D data | the remaining dataset queries, time series, import and export formats in scope |

Exit: per package, the harness rate of its pages at 80% or more.

### Out of the plan

- The complete algebraic case of Risch integration (Trager's and Bronstein's general algorithms): 14B covers the
  cases with documented examples; the rest is decided after phase 14.
- Partial differential equations beyond what the documented examples of `d_solve` need: decided after phase 13.

## 3. Estimates

At the pace of the first week (several packages in parallel, one coordinator merging), and measured by the harness:

| Phase | Time | Can start |
|---|---|---|
| 9 coverage | 2-4 weeks | now |
| 11 solving | 3-5 months | now; 11A first, then 11B-11D |
| 12 simplification | 1-2 months | after 11B |
| 13 equations, sums, limits | 2-3 months | 13B, 13C now; 13A after 11C |
| 14 symbolic integration | 3-5 months | 14A, 14D now; 14C after 14A |
| 15 breadth | 2-4 months, in parallel | now |

Expected harness rate over all pages in scope: about 80% in 3-4 months and about 95% in 9-12 months. Two chains set
that pace and run side by side: 11A → 11B → 11C → 12 → 13A (solving, simplification, equations) and 14A → 14C → 14D
(integration, whose definite part also waits for `reduce`). Each is sequential, so more parallel packages do not
shorten it. The last few percent, and robustness beyond the documented examples, have no date.

## 4. Testing against the reference system

- **Differential tests, bottom-up.** For each phase's targets, example inputs (written for v2, translated privately)
  are run in both systems; the outputs are compared by meaning (exact equality, numeric agreement to the stated
  digits, or a property such as "substituting the solution gives zero"). Only the v2 side and the expected values
  enter the repository, as ordinary tests; the translation and the reference runs stay in the private tooling.
- **Lower layers first.** A failing higher-level comparison is investigated by checking its dependencies' comparisons
  first (the graph names them).
- **Deliberate differences** (semantics `01-semantics.md`: no sequence splicing, read-time names, ball contagion, AC
  rest variables binding the head) are recorded per function and compared by meaning, never by exact text.
- **Coverage is re-measured after each phase**: a full harness run, the table of section 1.2 split by area.
- **Solvers are checked by substitution**: every result of `solve`, `reduce`, `d_solve` and `r_solve` in the shared
  suite is substituted back, so a wrong answer fails even when the reference has no example for it.

## 5. Order and parallelism

- Critical paths: 11A (CAD) → 11B (`reduce`) → 11C (`solve`) → 12 (simplification) and 13A (`d_solve`); and 14A
  (transcendental Risch) → 14C (special-function answers) → 14D (definite integration, with conditions from 11B).
- In parallel with them from the start: phase 9, 13B, 13C and all of phase 15.
- Crates: phase 11 owns `mc-poly`, phase 12 `mc-simplify`, phase 14 the integration modules of `mc-calculus` (a new
  `mc-integrate` crate if 13 and 14 run at the same time, so no two packages own one crate), 15A `mc-numeric` and
  `mc-special`, new crates for 15B-15D where needed.
- Each package follows the established rules (`agents` briefs, kept outside the repository): its own worktree and
  branch, offline builds, guards and progress for heavy runs, a report saved as the crate's `NOTES.md`, every new
  function with a `FUNCTIONS` entry, a documentation page and tests.
