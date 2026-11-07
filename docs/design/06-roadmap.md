# MathCAS v2: implementation roadmap (after M1)

The core is assembled and fast enough to build on: the evaluator runs on an explicit stack, numeric loops compile,
files, modules, notebooks, a kernel protocol and editor support exist. What is thin is the **library**. This document
orders the library work, using a measured picture of how a mature system's library is layered, and fixes how each
step is tested.

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
  solvers (`solve`, `reduce`), linear algebra (inverse, pseudo-inverse, vector calculus), special functions (polylog,
  hypergeometric, Jacobi elliptic), probability distributions, and objects with rich display. They must be developed
  as one coordinated package (phase 3). A refinement of the measurement (formatting rules separated from computational
  ones) is expected to split this cycle further.
- Visualisation stands on numerics: its most depended-on foundations include numerical integration and minimisation
  (plots sample, integrate and optimise), graphics primitives and colour handling.

### 1.2 Where v2 stands (November 2026)

Matching v2's 170 names against the documented functions in the five scope areas (by concept; approximate):

| Reference level | Documented functions | Covered by v2 |
|---|---|---|
| 0 | 1,118 | 128 (11%) |
| 1 | 216 | 13 (6%) |
| 2-5 | 247 | 2 |
| 6 (the large cycle) | 138 | 2 |
| 7 and above | 240 | 0 |

Of the 40 most depended-on documented functions per area: core language 15 covered, symbolic and numeric 21,
visualisation 1. Some reference foundations are that system's own idioms (increment operators, unset, option
patterns, its typesetting forms); v2 covers those differently (`var`, options after `;`, its printer) and they are not
targets.

## 2. Phases

Each phase lists its targets by **v2 name**, is split into packages that can run in parallel (no two packages own the
same crate), and ends with its exit criteria. Phase numbers are an order of dependency, not of calendar: a phase can
start as soon as what it uses exists.

### Phase 1: foundations (reference levels 0-1)

High fan-out, mostly natives, little algorithmic risk.

| Package | Targets |
|---|---|
| 1A predicates and structure (`mc-lists`) | `is_integer`, `is_rational`, `is_number`, `is_exact`, `is_inexact`, `is_machine_number`, `is_list`, `is_vector`, `is_matrix`, `is_string`, `is_symbol`, `is_atom`, `free_of`, `member_of`, `position`, `all_true`, `any_true`, `none_true`, `thread`, `constant_array`, `riffle`, `catenate`, `partition`, `tally`, `gather`, `gather_by`, `group_by`, `sort_by`, `max_by`, `min_by`, `ordering`, `rotate_left`, `rotate_right`, `pad_left`, `pad_right`, `delete_duplicates_by`, `subsets`, `tuples`, `permutations`, `outer`, `inner`, `table` (comprehension), `array`, `map_indexed`, `map_thread`, `scan`, `select_first`, `count_by` |
| 1B sets and associations (new `mc-assoc`) | `union`, `intersection`, `complement`, `subset_q`; associations (maps keyed by any term, insertion-ordered): literal syntax or `assoc([k -> v, ...])`, `keys`, `values`, `lookup` (with default), `key_exists`, `key_take`, `key_drop`, `key_sort`, `insert`, `delete_key`, `merge` (with a combiner), `map_values`, `select_keys`, `counts`, `group_by` into an association; failure tests `is_failure`, `failure_tag` |
| 1C numbers and text (`mc-numeric`, `mc-arith`, `mc-lists`) | `re`, `im_part`, `arg`, `conjugate`, `precision`, `accuracy`, `set_precision`, `rationalize`, `chop`, `clip`, `rescale`, `unit_step`, `sign`, `n(x, digits: d)` option form; strings: `string_split`, `string_replace`, `string_take`, `string_drop`, `string_contains`, `string_starts`, `string_ends`, `to_upper`, `to_lower`, `characters`, `from_characters`, `to_expression` (read a string as MathCAS) |

Exit: every target with tests and a `FUNCTIONS` entry; foundations' differential tests against the reference system
(section 3) pass or have a recorded, deliberate difference.

### Phase 2: calculus foundation (reference level 2)

| Package | Targets |
|---|---|
| 2A differentiation (new `mc-calculus`) | `d(f, x)`, `d(f, [x, n])`, `d(f, x, y)`, `derivative(n)(f)` and the `f'` notation (already read by the parser), the derivative of every elementary and special function v2 knows, implicit chain rule on unknown functions (`d(f(g(x)), x)`), total derivative of lists (gradient) |
| 2B simplification (new `mc-simplify`) | `simplify` (rational normal form, trigonometric and exponential identities, `sqrt` of squares under assumptions), `trig_expand`, `trig_reduce`, `exp_to_trig`, `trig_to_exp`, `power_expand`, `complex_expand`, `assuming` context (semantics section 13) |
| 2C limits and integration (`mc-calculus`, after 2A) | `limit` (rational and elementary functions, one-sided), `integrate` (polynomials, rational functions by partial fractions, elementary antiderivatives by a table plus substitution and parts, definite integrals by the fundamental theorem with discontinuity checks), `apart` |

Exit: the calculus benchmark cases (new class C in `02-benchmarks.md`) pass; derivatives checked numerically at
random points; antiderivatives checked by differentiation.

### Phase 3: the intertwined core (the large cycle)

One coordinated round: these functions use each other, so they share a test suite and are merged together.

| Package | Targets |
|---|---|
| 3A linear algebra (new `mc-linalg`) | `det`, `inverse`, `pseudo_inverse`, `rank`, `null_space`, `row_reduce`, `linear_solve`, `eigenvalues`, `eigenvectors` (exact for small, numeric otherwise), `characteristic_polynomial`, `identity_matrix`, `diagonal_matrix`, `cross`, `outer`, `grad`, `div`, `curl`, `laplacian`, `kronecker_product`; exact, rational, ball and machine arithmetic |
| 3B solving (`mc-poly`, `mc-calculus`) | `solve` (univariate polynomials by radicals up to degree 4, roots otherwise; linear systems; polynomial systems by Groebner bases), `reduce` for real polynomial inequalities in one variable, `roots`, `n_solve`, `eliminate` |
| 3C special functions (`mc-numeric`) | `gamma` family, `log_gamma`, `beta`, `polylog`, `zeta`, `hypergeometric_pfq` (numeric, with exact special values), `erf` family, Bessel functions, Jacobi elliptic functions and their inverses, elliptic integrals: exact special values, derivatives (for 2A), balls to any precision |
| 3D distributions (`stats` module and `mc-numeric`) | `normal_distribution`, `binomial_distribution`, `poisson_distribution`, `chi_square_distribution` and a small family; `pdf`, `cdf`, `mean`, `variance`, `quantile`, `random_variate` (session random generator) |

Exit: one shared suite of identities across the packages (for example `d(polylog(2, x), x)`, `inverse(m) . m`,
`solve` results substituted back), plus the differential tests.

### Phase 4: numerics for visualisation

| Package | Targets |
|---|---|
| 4A (`mc-numeric`, `mc-jit`) | `n_integrate` (adaptive Gauss-Kronrod, double exponential for endpoint singularities, balls on request), `n_minimize` / `n_maximize` / `find_minimum` (Nelder-Mead, BFGS with numeric gradients, bounds), `find_root` (Newton with bracketing fallback), `n_dsolve` (explicit Runge-Kutta with error control, implicit method for stiff systems), `interpolation`; integrands and right-hand sides compiled with the JIT when compilable |

Prerequisite: the JIT inlines powers and sums of machine reals (in progress; Mandelbrot test case in
`tools-private` quick comparison).

The detailed plan for this phase, including arbitrary precision (balls) and the compiler extensions it needs, is
[`07-numerics.md`](07-numerics.md) (packages N0-N5).

Exit: the L-class benchmark cases (`02-benchmarks.md`) pass and are compiled where they can be.

### Phase 5: visualisation and data

| Package | Targets |
|---|---|
| 5A graphics model | `graphics` and `graphics3d` terms; primitives `point`, `line`, `polygon`, `circle`, `disk`, `rectangle`, `text`, `arrow`; directives (colour, thickness, opacity, dashing), colours and `color_convert`, `scaled` coordinates, options (axes, frame, plot range, labels); an SVG renderer |
| 5B plots | `plot` (adaptive sampling, discontinuity and singularity detection), `list_plot`, `list_line_plot`, `parametric_plot`, `contour_plot`, `density_plot`, `histogram`, `bar_chart`, `plot3d` (later) |
| 5C notebook UI | the notebook window on the kernel protocol (`crates/mc-kernel`), plain-text output first, graphics as SVG, typeset output when printing settles |
| 5D data | datasets over associations: `dataset`, queries (select, group, aggregate), import and export of CSV and JSON |

Exit: every plot of the visualisation foundations list renders; notebooks show graphics.

### Status (November 2026)

Phases 1-5 are done. Later rounds added:
- **Phase 6:** a private harness that runs the reference documentation's examples.
- **Phase 7:** speed: compiled special functions and ODE right-hand sides, packed complex arrays, parallel evaluation.
- **Phase 8:** notebook maturity: documentation in the editor, `manipulate`, 3D rotation, HTML and PDF export.
- **Phase 9 (in progress):** coverage against the harness, plus regions, new plots, multivariate `factor` and
  `minimize`.
- **Phase 10:** platforms. Linux packages are built (tarball and AppImage); an Android port is not planned.

### Scope check

All five phases fall within the five scope areas (core, data, visualisation, symbolic/numeric, higher mathematics).
Network services, cloud, user interfaces beyond the notebook, and the reference system's knowledge-base functions are
out of scope and do not appear.

## 3. Testing against the reference system

- **Differential tests, bottom-up.** For each phase's targets, example inputs (written for v2, translated privately)
  are run in both systems; the outputs are compared by meaning (exact equality, numeric agreement to the stated
  digits, or a property such as "substituting the solution gives zero"). Only the v2 side and the expected values
  enter the repository, as ordinary tests; the translation and the reference runs stay in the private tooling.
- **Lower layers first.** A failing higher-level comparison is investigated by checking its dependencies' comparisons
  first (the graph names them).
- **Deliberate differences** (semantics `01-semantics.md`: no sequence splicing, read-time names, ball contagion, AC
  rest variables binding the head) are recorded per function and compared by meaning, never by exact text.
- **Coverage is re-measured after each phase**: the table of section 1.2, and the top-40 foundations per area.

## 4. Order and parallelism

- Phase 1 packages (1A, 1B, 1C) and phase 2A run in parallel: they touch different crates.
- 2B and 2C follow 2A; phase 3 needs 1A-1C and 2A; phase 4 needs the JIT work and 1C; phase 5 needs 4A for plots,
  1B for data, and the kernel protocol (done) for the notebook.
- Each package follows the established rules (`agents` briefs, kept outside the repository): its own worktree and
  branch, offline builds, guards and progress for heavy runs, a report saved as the crate's `NOTES.md`, every new
  function with a `FUNCTIONS` entry and tests.
