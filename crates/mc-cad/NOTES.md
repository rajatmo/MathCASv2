# Item 11A/11B (first part): `mc-cad`

Round 17. Cylindrical algebraic decomposition over the reals, quantifier elimination,
`reduce` in several real variables, `resolve`, `exists`, `for_all`, `cylindrical_decomposition`, real
`find_instance` in several variables, and the decision service's last tier in several variables. Dependencies:
`mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-poly`, `mc-algebra`, `mc-decide`, `rustc-hash` (dev: `mc-bench`,
`mc-engine`). No new external dependency.

## What was built (`src/`)

| module | contents |
|---|---|
| `formula` | relations, quantifiers, formulas over atom indices, atoms as signed products of basis factors, negation normal form, three-valued evaluation from partial sign vectors |
| `poly` | levels, factors, evaluation of an integer polynomial at a sample point in one number field, Lazard's valuation-based residue at points where a polynomial vanishes identically |
| `project` | the basis of irreducible factors by level; Lazard's projection (default) and McCallum's with Brown's improvement; McCallum's restricted projection for an equational constraint |
| `cad` | preprocessing (negation normal form, lexicographic Groebner basis of the equations of a conjunction, factorisation, equational constraint), lifting with certified sample points, truth propagation for quantifiers, witnesses; `Limits` (factors, cells), progress callback |
| `vs` | linear virtual substitution (Loos-Weispfenning) for innermost quantified variables that occur linearly |
| `sos` | a sum-of-squares certificate (Gram matrix, exact LDL^T) for `for_all` of one polynomial inequality |
| `complex` | complex quantifier elimination of one variable at a time (each disjunct with at most one equation in it), closed sentences of equations and `!=` by Groebner bases (Rabinowitsch) |
| `output` | solution formulas: cylindrical form with cell combination, sign-condition form with greedy widening and set-cover pruning |
| `front` | terms to problems: connectives, comparison chains, `element(x, reals)`, quantifiers, Boolean variables, prenex form with renaming apart, square roots as new variables |
| `natives` | `resolve`, `exists`, `for_all`, `cylindrical_decomposition`, the `find_instance` wrapper, the `not` wrapper for quantifiers, `reduce_reals` for `mc-solve`, the internal `cad$stats` |
| `reals` | `CadReals`, the `mc_decide::Reals` hook, and its factory |

## API

- `install(k, h, stop)` after `mc-decide` (it wraps `find_instance` of `mc-calculus` and `not`, and registers the
  decision service's hook); `FUNCTIONS`: `resolve`, `exists`, `for_all`, `cylindrical_decomposition`.
- `mc_cad::reduce_reals(cx, stop, cond, vars)`: the cylindrical form of a real formula, or `None`.
- Rust: `cad::{Problem, Limits, decompose, decompose_witness, Outcome, Cell, Coord}`, `project::{Basis, Method}`,
  `front::Front::problem`, `output::Writer::{cylindrical, sign_formula, root}`, `vs::eliminate`,
  `sos::nonnegative`.

Outside the crate (registrations and owned files):

- `mc-solve/src/reduce.rs`: `several` (real `reduce` in several variables, with parameters, or of quantified
  formulas, to `mc_cad::reduce_reals`); `mc-solve/src/natives.rs`: the call to it (one line); `Cargo.toml` line.
- `mc-decide/src/decide.rs`: `RealsFactory` and `register_reals` (the registration point, about 15 lines), and
  `by_reduce` takes the variables of the question (the facts in those variables), so that questions in up to
  four variables reach the hook. With the default hook (one variable) nothing changes. It is the smallest change
  that lets the service ask about several variables.
- `mc-poly/src/ringfns.rs`: `subresultants` checks the dense degree limit before building coefficient vectors
  (2 lines; the fuzz campaign hit a 120 GB allocation with `x^(2^31)`).
- `mc-solve/tests/numeric.rs`: `reduce(x^2 > a, x, reals)` now has an answer.
- `mc-engine` (install line, `FUNCTIONS`, Cargo), `mc-docs` catalogue topic "Quantifiers and real algebraic
  sets", `mc-lsp` library list, Cargo lines, help index.
- docs: `resolve`, `exists`, `for_all`, `cylindrical_decomposition`.

## Decisions

- **CAD.1** Variables in decomposition order: parameters (symbols not listed), the listed free variables, then
  the bound ones in prefix order. The ring of the conversion orders its kernels canonically, so polynomials are
  permuted into this order on reading and back on writing.
- **CAD.2** McCallum's projection with Brown's improvement (leading coefficients, discriminants, resultants) is the
  default; Lazard's operator (leading and trailing coefficients; complete by McCallum, Parusiński and Paunescu
  2019) is the fallback. Lifting always uses Lazard's valuation-based residue, which is the plain value unless a
  polynomial vanishes identically at the sample point (so it serves as Brown's delineating polynomial over a
  point); a nullification over a cell of positive dimension under Brown's operator reports `NotWellOriented` and
  the decomposition is redone with Lazard's. Lazard as the default was measured (table below): it never helped,
  Brown was smaller on four of eleven problems, so Brown stays the default and Lazard the guarantee.
- **CAD.3** Sample points: sectors take the simplest rational between their bounds; sections the real algebraic
  root (minimal polynomial and isolating interval) found by `mc_algebra::realroot::real_roots` over the number
  field of the base point (ball arithmetic first, exact arithmetic over the field as the fallback). Coordinates
  of one point live in one field built by primitive elements (`adjoin`) only when a section's cylinder is lifted
  further. Signs are exact: elements of the field for sectors, `sign_at_algebraic` for sections, so every cell's
  truth is certified.
- **CAD.4** Preprocessing: the matrix in negation normal form; two or more equations of a top-level conjunction
  are replaced by their lexicographic Groebner basis in the decomposition order (degrevlex + FGLM when
  zero-dimensional), so triangular equations appear before projection and an inconsistent system is false at once.
- **CAD.5** Cell combination in the cylindrical form: adjacent cells whose descriptions above use the same roots of
  the same factors (labels intersect) are combined; a section joins the sector before it, and the sector after it
  only when nothing is described above them, as hand-written answers do. A single point's
  coordinate is substituted into the description above it. Conjunctions of levels are flat, intervals one
  conjunct.
- **CAD.6** Sign-condition formulas (`resolve` with free variables): the true and false free cells' signatures over
  the projection factors; if no false signature is covered by a true one, each true signature is widened greedily
  (drop a condition, else relax it) while it excludes every false one, then implicants whose true cells the others
  cover are removed; ordered by the conditions on the lowest factors. Otherwise the cylindrical form.
- **CAD.7** Every `reduce`, `resolve` and `cylindrical_decomposition` answer with free variables is evaluated at the
  rational sample points of up to 64 free cells and must give each cell's truth; a mismatch makes the call
  decline rather than return a wrong formula.
- **CAD.8** Linear virtual substitution eliminates the innermost quantified variables while every atom is linear
  in them and the formula stays below 400 atoms; the decomposition then has fewer variables.
- **CAD.9** `for_all` of one polynomial inequality `p >= 0` is first tried with a sum-of-squares certificate.
- **CAD.10** An atom whose denominator vanishes is false at that point (in negation normal form), so an
  implication `cond -> ineq` with a denominator that vanishes outside `cond` is still true there.
- **CAD.11** `find_instance` asks for a witness: the search goes past a true cell with an irrational sample, for
  up to 300 cells, looking for a rational one; a variable an equation determines linearly is put last, so its
  sections over rational points are rational.
- **CAD.12** Square roots of rational functions become variables: `sqrt(t)` is `r` with `r^2 == t` and `r >= 0`,
  quantified like the innermost bound variable of `t` (`exists` when `t` has only free variables) and placed
  after it.
- **CAD.13** Over the complex numbers (`resolve` without an inequality, or with `complexes`): one variable at a
  time, `exists x` of a disjunct with only `q != 0` in `x` (no `q` identically zero in `x`) or exactly one equation
  `p == 0` in `x` (`p` not a nonzero constant in `x`; true outright when the coefficients of the positive powers
  have no common zero, by a Groebner basis). Closed sentences outside that by Groebner bases of each disjunct.
- **CAD.14** An equational constraint nullified over a point cuts nothing: all factors cut that stack; nullified
  over a cell of positive dimension, the decomposition is redone without the constraint.
- **CAD.15** `resolve` of a formula it cannot eliminate returns the formula with each quantifier's variables as a
  list and nested `and`/`or` flattened (its unevaluated form).

## Checks and answer forms, and the evidence

- **Certified answers.** Every truth value comes from exact signs (CAD.3), and every answer with free variables is
  checked at sample points before it is returned (CAD.7); `tests/cad.rs::answers_hold_at_random_points` checks
  25 random two-variable formulas at 12 random rational points each against `reduce`'s answers.
- **Simpler answers.** `resolve(for_all(x, a x^2 + b x + c > 0), reals)` gives
  `a >= 0 and b == 0 and c > 0 or a > 0 and -b^2 + 4 a c > 0` (two disjuncts, none redundant).
  `resolve(exists(x, a x^2 + b x + c < 0), reals)` contains `c < 0` alone, without splitting on `a`.
  `reduce(x^2 + y^2 < 2 and x - y >= 1, [x, y])` gives the expected cylindrical form, including the substituted
  section.
- **Fewer variables.** Linear virtual substitution removes linear quantified variables before the decomposition
  (measurements below).

## Measurements

`tests/cad.rs::measurements` (ignored; release), `cad$stats`.

Factors / cells / microseconds (one evaluation, loaded machine; `null`: free variables remain):

| problem | Lazard | Brown | Lazard + linear VS |
|---|---|---|---|
| `x^2 + y^2 < 1 and y > x` | 6 / 72 / 3018 | 5 / 56 / 2294 | 6 / 72 / 2940 |
| `x^2 + y^2 + z^2 < 1` | 4 / 43 / 1231 | 4 / 43 / 1013 | 4 / 43 / 1247 |
| `x^4 >= x^2 + y^2` | 4 / 28 / 520 | 4 / 28 / 516 | 4 / 28 / 511 |
| `y^2 == x*(-1 + x^4)` | 5 / 28 / 618 | 5 / 28 / 489 | 5 / 28 / 600 |
| `exists([x, y], x != y and a x^2 + b x + c == 0 and a y^2 + b y + c == 0)` | 11 / 293 / 31463 | 11 / 293 / 35649 | 11 / 293 / 31087 |
| `for_all(x, a x^2 + b x + c > 0)` | 5 / 95 / 1585 | 5 / 95 / 2232 | 5 / 95 / 1457 |
| `exists([x, y], a x + b y >= 1 and (a^2 - b) x - (a - b^2) y == 2)` | 21 / 4191 / 386046 | 21 / 4191 / 364361 | 16 / 330 / 43069 |
| `for_all(c, exists([x, y], a x + b y + c == 0 and c x + a y >= b))` | 13 / 630 / 18897 | 13 / 630 / 17131 | 8 / 79 / 5642 |
| `exists([x, y], x^2 + y^2 <= 11 and x^3 - y^2 == 2 and x y > 3)` | 6 / 23 / 10044 | 6 / 23 / 10247 | 6 / 23 / 9959 |
| `exists([x, y, z], x + y + z <= 1 and x - y >= 0 and z >= 2 x and y + 3 z >= 4)` | 22 / 6 / 571 | 11 / 6 / 316 | 0 / 0 / 144 |
| `exists(y, x y^2 + y + z == 0 and x^2 + z^2 <= 4)` | 8 / 242 / 77465 | 7 / 172 / 65200 | 8 / 242 / 76640 |

Brown's operator is never larger and is smaller on four of eleven problems (up to half the factors and 30% fewer
cells); Lazard's trailing coefficients bring no gain here. Linear virtual substitution cuts the two parametric
linear problems by 9x and 3x in time (13x and 8x in cells) and decides the linear sentence without a
decomposition; on the others it does not apply and costs nothing.

## Tests

`tests/cad.rs` (6 and 1 ignored): documented cylindrical answers, quantifier elimination (real, complex, Boolean,
radicals, unresolved), linear virtual substitution checked at points, random answers checked at random points,
real `find_instance` witnesses checked by substitution, the decision service in two variables; the doc test of
`lib.rs`; the four documentation pages.

## Open gaps (for 11B and 11C)

Round 18 (item 11B, below) closed: integers (Pell, bounded problems, sentences), complex elimination with
several equations per disjunct and complex inequalities, `reduce` of equations without a domain in several
variables, several radicals per formula and the Cauchy-Schwarz problems, `minimize` and friends in several
variables. What is left:

- Quadratic virtual substitution for quantified variables (Weispfenning's test points with square roots and
  infinitesimals) and a simplex fast path are not done; the sign rules of the quadratic substitution are used to
  square radicals out of sentences (CAD.21), not to eliminate quantified variables.
- Over the integers: generalized Pell equations `x^2 - D y^2 == N` with `N != 1` and Pell equations with other
  sign conditions as families (only `N == 1` with `x > 0 and y > 0`, or with no condition), Thue equations,
  linear Diophantine systems as parametric families (Hermite normal form); unbounded problems other than Pell
  only by the bounded search of `find_instance` (no claim beyond the bound); `reduce` of unbounded integer
  problems falls through to `mc-solve`'s solution path, which fails or (without conditions) can return a formula
  that is not restricted to integers (`domain.rs`, outside this crate).
- Comprehensive Groebner systems in full (Kapur-Sun-Wang): the triangular `reduce` splits on leading coefficients
  of lex bases only, which is complete but can give more cases than needed; complex elimination results are
  tidied (square free, factors excluded by `!=`, inconsistent cases dropped) but not minimised
  (`resolve(exists([x, y], a x^2 + b x + c == 0 and a y^2 + b y + c == 0 and x != y))` has four disjuncts where
  two suffice).
- Boundary, interior, closure, exterior and connected components of the set described by
  `cylindrical_decomposition` need cell adjacency; not done.
- `minimize` with parameters (`minimize(a x^2 + b x + c, x)`: a piecewise answer) and transcendental objectives
  stay with `mc-calculus`.
- `region_member` gives conditions without an `element(x | y, reals)` prefix (mc-geom).
- Algebraic-number and transcendental coefficients, transcendental functions.
- The sign-condition formulas are not minimal, and their order follows a fixed rule.

## Item 11B (round 18)

### What was built

| where | contents |
|---|---|
| `optimize` (new) | `minimize`, `maximize`, `min_value`, `max_value`, `arg_min`, `arg_max` through the decomposition, wrapping `mc-calculus`'s natives (which run first) |
| `integers` (new) | `reduce` over the integers in several variables (Pell families, bounded problems), `find_instance(..., integers[, n])`, `resolve(sentence, integers)` |
| `front` | `abs`, `max`, `min` split into cases (in the matrix, or into separate formulas for the optimisation); radicals of nonnegative radicands squared out of sentences; several radicals in one formula (the second was rejected before) |
| `complex` | several equations and `!=` in the eliminated variable (pseudo-remainders, splitting on leading coefficients); results tidied; sentences with inequalities over the real and imaginary parts |
| `natives` | `find_instance(cond, vars[, reals], n)` for finite real sets; `reduce` of a finite set of rational points as a list of points; `resolve(..., integers)` |
| `mc-solve/src/reduce_cx.rs` (new) | `reduce` of equations and `!=` over the complex numbers in several variables or with parameters: the triangular decomposition |
| `mc-solve/src/reduce.rs` | routing: complex equations to `reduce_cx`, quantified complex formulas to complex elimination then `reduce_cx`, integers to `integers`, `element(points, region)` to the region's condition over the reals |

### Decisions

- **CAD.16** Optimisation: `t == f and cond` with `t` the first (base) variable and every listed variable free;
  the lowest base cell with a true cell above it decides: none (infeasible: `inf`, point `indeterminate`), an
  unbounded sector (`-inf`), a section (the minimum, attained at the sample point of a true cell above it, a
  rational one when there is one), a bounded sector (its lower end, an infimum not attained: `indeterminate`).
  The point is checked by substitution (constraints true, objective equal to the value within 10^-20) before it
  is returned; a failed check declines. Parameters decline (left to `mc-calculus`).
- **CAD.17** `abs(u)`, `max(a, b, ...)`, `min(a, b, ...)`: the innermost is split into its cases
  (`u >= 0` with `u`, `u < 0` with `-u`; `a >= max(rest)` with `a`, else `max(rest)`), at most 8 splits along a
  branch and 64 branches. The optimisation decomposes each branch separately and takes the best extreme (exact
  comparison of real algebraic numbers; an attained value wins over an equal infimum): 16 small decompositions
  in 3 variables instead of one that exceeded the factor limit (`TooLarge` after 50 s before, 2.4 s now).
- **CAD.18** Complex elimination of `exists x` from `p_1 == 0 and ... and q_1 != 0 and ...`: on the leading
  coefficient `c` of the equation of least degree, `c != 0` (other equations replaced by pseudo-remainders; with
  one equation, the pseudo-remainder of `q^deg p` by `p` not identically zero) or `c == 0` (the reductum); depth
  at most 12. Results: `!=` square free, equation factors that a `!=` excludes removed, cases inconsistent by a
  Groebner basis (Rabinowitsch) dropped.
- **CAD.19** Integers: Pell `x^2 - D y^2 == 1` by the continued fraction of `sqrt(D)` (fundamental solution
  checked); `reduce` gives `element(c(1), integers) and c(1) >= 1 and x == ... and y == ...` with `x > 0 and y > 0`, or both signs and every `c(1)` without conditions; `find_instance` walks the
  powers of the unit (every sign) against the whole condition. Bounded problems: the box from the minimum and
  maximum of every variable over the real set (CAD.16, degree at most 12), then every integer point (at most
  400,000; the last variable from the integer roots of an equation in it); a complete search decides `reduce`
  and `resolve`. Otherwise `find_instance` searches the points of maximum norm up to 60 and stays unevaluated
  when it finds none. Every point is checked exactly.
- **CAD.20** An order relation between complex values holds only between real values: a sentence with one is
  decided over the real and imaginary parts (`x = a + i b`, `p < 0` as `im p == 0 and re p < 0`), both parts
  bound by `x`'s quantifier. Free variables decline.
- **CAD.21** In a sentence, a radical `sqrt(B)` with `B` nonnegative everywhere (sum-of-squares certificate) is
  squared out of each atom `P + Q sqrt(B) rel 0` by the sign rules of quadratic virtual substitution (with
  `D = P^2 - Q^2 B`: `> 0` iff `D > 0 and P > 0` or `D < 0 and Q > 0` or `D == 0 and P > 0 and Q >= 0`;
  `== 0` iff `D == 0 and P Q <= 0`), instead of a new variable. The semantics agree with CAD.12 because the
  radical is defined everywhere. Formulas with free variables keep CAD.12 (their cylindrical forms would change).
- **CAD.22** (`reduce_cx`) Complex `reduce`: listed variables after the parameters, later ones solved in terms of
  earlier ones; the lex basis is split on the leading coefficient `l` of its element `g` of least degree in the
  highest leading variable: `l != 0` (`g`'s roots, by Gianni-Kalkbrener) and `l == 0` (added to the equations);
  input polynomials square free, `!=` conditions reduced to normal forms (zero empties a case) and written last,
  leading-coefficient conditions in level order, cases with conditions on lower variables first. Roots: rational
  functions, the quadratic formula (square part of a numeric discriminant taken out), pure powers with roots of
  unity in the solver's order, the solver's formulas for numeric coefficients, root objects for parametric
  degrees 3 and up. A lower variable with one numeric value is substituted. Every case is checked at a rational
  point of its free variables (solved values substituted, the input equations within 10^-15 at 30 digits); a
  failed check declines. 64 cases, depth 16 at most.
- **CAD.23** `reduce` over the reals of a finite set of at least two rational points lists the points
  (`x == 0 and y == 0 and z == 1 or ...`) instead of the combined cylindrical form.

### Checks and answer forms, and the evidence

- **Checked answers.** Optimisation points are substituted back (CAD.16), complex `reduce` cases are checked at
  points (CAD.22), integer points exactly (CAD.19); `tests/reduce11b.rs` re-substitutes the Pell family at
  `c(1) = 1 .. 6` and a `find_instance` point of `x^3 + y^3 == 9 + z^3`.
- **Cauchy-Schwarz and the triangle inequality with square roots** (`for_all` in four variables with two and
  three radicals): `true` in under a second; before, the second radical was rejected and with radical variables
  the problem exceeded the factor limit (CAD.21).
- **Honest bounds.** `find_instance` over the integers states its search bound in the documentation and never
  claims emptiness from a search; `reduce` and `resolve` over the integers answer only when the box is complete.
- **Stable forms**: Pell families, triangular complex systems and the optimisation outputs are printed in fixed
  forms that the tests and the documentation pages check exactly.

### Tests

`tests/reduce11b.rs` (5): optimisation (12 cases), complex `reduce` (7), complex elimination and complex
inequalities (7), radicals squared out (4), integers (8 cases plus the substitution checks).
`mc-solve/tests/domain.rs`: the linear system's expected form changed to the triangular one
(`y == 22 - 29 x and z == 13 - 17 x`). Pages: `minimize`, `resolve`, `find_instance` (new
examples, stored outputs regenerated).

## Round 20 (roadmap 16A)

Parallel work on the session's pool (`mc-eval`'s `par` module), with the same answers at any thread count
(`tests/par.rs`: 17 problems at 1, 2, 4 and 8 threads).

- **n16a.3 Lifting.** `natives::metered` offers an `mc_eval::Budget` on its thread (`par.rs`, a thread-local
  `Offer`); a decomposition that finds one (and does no witness search, whose choices depend on the order of the
  whole traversal) lifts every stack's cells in parallel (`Lifter::stack_par`): each cell (signs, truth, lifting
  above it) is an element on its own lifter (a copy of the lower levels' signs) and meter, the run ending at the
  first cell that decides a quantified stack or fails. The parent takes the cells in order, counting them (and the
  cells their tasks built) against `max_cells` exactly as the sequential traversal does (a task's count is added
  when its cell is taken: the sequential `TooLarge` happens at or before that point), charges each kept cell's fuel
  to its meter, and computes a stopped cell again itself. Nested stacks lift in parallel too.
- **Cases** (`optimize`): the decompositions of the `abs`/`max`/`min` cases (CAD.17) run as `par_until` elements,
  the first case without an answer ending the run, as the loop did; the best extreme is chosen after, in order.
- **Checks and disjuncts**: the answer check at up to 64 sample points (CAD.7), the satisfiability of the
  disjuncts of a closed complex sentence and the per-disjunct elimination (CAD.13, CAD.18), the 2n bounds of an
  integer box and the slices of its first coordinate (CAD.19) run through `par::until` (plain elements with their
  own meters; an element runs its decompositions without the budget, so the fuel does not depend on where it ran).
- **Measured** (medians of 5, fresh process per run, 16-core machine under other load): `minimize`/`maximize` with `max`, `abs` (16 cases) 3.7x on 4 threads, 5.7x on 8; one
  decomposition of 239 cells (`minimize([x - y, -y^2 + 4 x^2 - 4 x^4 > 0])`) 2.9x / 4.1x; the triangle
  inequality sentence (2,808 cells) 2.4x / 3.1x; the integer box of `resolve(exists(...), integers)` 3.2x / 4.5x.

## Round 36

- An interrupt reaches the decomposition. The number kernels' stop flag is raised together with the evaluator's
  interrupt but before the fuel poll sees it, so a metered computation stopped by the flag used to answer "did not
  finish" (`None`) and the caller went on to its next method with the kernels returning arbitrary values: a
  `function_bijective` of a quintic with four parameters never returned after an interrupt at 2 s. Now a stop is the
  abort (`Tick::charge` sets it when `cx.stopped()`, `metered` returns `Abort::Limit`), and a stopped element of a
  parallel `until` or of a lifting level is not recomputed (`par.rs`, `cad.rs`). The same change is in the other `Tick`s
  (mc-solve, mc-poly, mc-algebra, mc-linalg) and `NativeCx::eval` fails when the control block is halted (mc-eval).
  Test: `an_interrupt_stops_function_bijective_systems` (mc-engine/tests/r36_w36_calculus.rs).

## Round 36, Pell families

- Pell families with any sign conditions on x and y (half-lines read from the points -2..2): signs and least power c(1) >= 0 or 1.

## Round 39: resolve over signed reals and booleans

- `resolve(f, positive_reals)` (and negative, nonnegative, nonpositive): every quantifier restricted (exists(v, s(v)
  and c), for_all(v, s(v) implies c)) and the sign condition on every free variable added, then over the reals.
  `resolve(f, booleans)`: each quantifier written out as the disjunction (exists) or conjunction (for_all) of its
  instances at true and false (at most 16 variables).
