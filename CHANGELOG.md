# Changelog

## 0.13.0 (2026-11-13)

MathCAS runs on macOS, about sixty new functions, and more answers where there were none.

**macOS (Apple Silicon).** A disk image with the application `MathCAS Notebook.app`: the notebook, the
documentation window, the REPL, the kernel and the language server, with the `.mcn` document type. It is built and
tested with the other packages on every change. The application is not notarized: the first launch needs Open
from the context menu (see the README in the image). Intel Macs are not built yet.

**New functions.**
- **Wavelets** (a new library): `discrete_wavelet_transform`, `stationary_wavelet_transform`,
  `continuous_wavelet_transform`, the packet transform and the inverses, wavelet plots and scalograms, and the
  Haar, Daubechies, Mexican hat, Morlet and other families. The discrete transforms reconstruct their data exactly.
- **Series:** `inverse_series` and `compose_series`, also for series in fractional powers.
- **Algebra and numbers:** `solve_always`, `find_linear_recurrence`, `mathieu_characteristic_b`, `pauli_matrix`,
  `permute`, `polynomial_q`, bit operations (`bit_set`, `bit_clear`, `bit_shift_left`), distance functions.
- **Lists and arrays:** `array_filter`, `array_reduce`, `subset_count`, `find_shortest_tour` (exact, up to 20
  points), `unitize`, `interval_intersection`, `find_divisions`.
- **Language:** `break()` and `continue()` in `while` and `for` loops; `symbol`, `symbol_name`, `with_cleanup`; the
  pattern heads `verbatim` and `hold_pattern`.
- **Uncertain values:** `vector_around`, `around_replace`.

**More answers.**
- **Transforms:** Hankel and inverse Hankel transforms through Mellin products; Mellin convolution; Radon
  transforms of disks and point masses with their inverses.
- **Sums:** `generate_conditions: true` gives the convergence condition (`sum(1/n^a, [n, inf])` is `zeta(a)` when
  `re(a) > 1`); Bertrand-type series; divisor sums with a symbolic argument; Dirichlet series and Euler products.
- **Limits and values at infinity** of the special functions; `to_radicals` options; Pell equation families.
- **Lists and numerics:** vector constraints in `n_minimize` and its siblings; `heads: true` in the mapping
  functions; ordering functions with `up_to`; permuted `transpose`; gradient, divergence, curl and Laplacian in
  polar, cylindrical and spherical coordinates; `log(b, x)`.
- **Algebra:** `trig:` and `modulus:` options of `together`, `cancel` and `expand`; bitwise laws on symbols;
  Gaussian gcd and lcm of rationals; inverse Jacobi function properties for `m > 1`.
- `abs`, `re`, `im_part`, `arg`, `conjugate` and `sign` all take numeric factors out (`abs(-2 k) = 2 abs(k)`).

**Fixed.**
- **A freeze:** plotting the solution of a differential equation that had been solved again after its session
  ended could stop the kernel for good, and the interrupt did nothing. Fixed (a lock was taken twice).
- `box_matrix(0, [n, n])` and its siblings divided by zero.
- Compiled numerics on 64-bit ARM processors.

**Notebook.**
- 3D plots in the PDF export are laid out at the width of the text column, as 2D plots are.
- Typing an opening bracket at the top of a very long cell no longer lays the whole cell out again (about six
  times faster in a 2,000-line cell).
- A closing bracket inserted automatically is kept when the caret moves inside it (a fraction filled with the
  arrow keys no longer ends in `))`); completion does not open on `2a`.

**Under the hood.** Output typesetting has its own crate (`mc-typeset`); the compiler version is pinned in
`rust-toolchain.toml`; the speed limit of the documentation examples can be set per machine
(`MC_DOCS_SLOW_SECS`).

## 0.12.0 (2026-11-12)

Answers where there were none: this release works on inputs MathCAS left unevaluated, failed on or timed out on.
Every fix carries tests on the input and on variations of it, each checked by an independent route.

**Function properties know the special functions.** `function_domain`, `function_singularities`,
`function_analytic`, `function_range`, `function_sign`, `function_monotonicity`, `function_convexity`,
`function_injective`, `function_surjective` and their siblings read one table of what each special function is
(about 160 entries with their sources: domain, singular points, range, bounds, sign, monotonicity, convexity,
periodicity), and combine it through sums, products, powers and compositions. Many inputs that stayed
unevaluated now answer.

**Calculus.** Hilbert and inverse Hilbert transforms by a general method (exponentials times rational functions,
multiple poles, compact piecewise polynomials); asymptotic relations in several variables; optimisation of periodic
objectives and of vector variables; `fourier_sequence_transform` of exponentials and periodic sequences; series of
the inverse trigonometric and hyperbolic functions at infinity and at branch points; integrals over regions given
by `boole`; `coefficient` of monomials and with `modulus:`. A limit that oscillates is `failure(no_limit)`.

**Algebra and number theory.** `modulus:` in the expand and polynomial-division families; `gaussian_integers:` in
the factoring and divisor functions; `groebner_basis` with weight-matrix, graded and elimination orders and over
finite fields; `cellular_automaton` with rule lists, function rules and ranges; `find_sequence_function` recognises
the library's integer functions.

**Special functions.** Derivatives of functions without a derivative rule are computed numerically with a
convergence check (for `find_root` and plots), or stay unevaluated; Weierstrass functions in closed form on the
degenerate lattice and at machine precision; the hypergeometric function at complex machine arguments outside the
unit disk; Lommel functions; `mathieu_c_prime` (new).

**Lists, numerics, linear algebra.** List operations on expressions with any head; `n` with real precisions and
accuracy goals; `solve` inverts `csch`, `sech`, `gudermannian` and `logistic_sigmoid`; eigenvalues and
characteristic polynomials of matrix pencils; `rank` and `row_reduce` with `modulus:`; alphabetical order for
Slovak, Spanish, Swedish and Serbian.

**Wrong values fixed.** `struve_h` and `struve_l` at half-integer order and negative argument; Kelvin functions at
and near 0 (`kelvin_ker(2, 0.)` is 1/2; values near 0 had lost twelve digits); `hyperfactorial` and `barnes_g` at
negative arguments; inverse Jacobi functions for `m > 1` are real where the real inverse exists;
`hypergeometric_u(a, b, 0)`; `series` dropped a term at negative order; options given to several library
functions were ignored when read from an `options(...)` argument. `scorer_hi` at large arguments and
`alternating_factorial` end; a library matrix of more than 2^26 entries declines at once.

**Notebook.** Plots in the notebook's PDF export are laid out for the page: their numbers print at the size of the
code beside them (the print layout is stored with the plot, about 5 KB each; old notebooks export as before). A
list of plots exports to a PDF with a print page each. Typing in a very long cell is about three times faster. The
language server knows every library's functions.

## 0.11.1 (2026-11-12)

**Exports with readable text.**
- `export_data` of a plot to PDF or SVG lays a default-sized plot out on a print page (360 by 225 points) with the
  same text size, where it used the screen's 640 by 400 page: the numbers were about 5 points once the figure was
  placed in a paper column. A plot with `image_size:` or `aspect_ratio:` keeps its own page.
- The notebook's PDF export uses larger text (code and output 10.5 pt, prose 11.5 pt, messages 9.5 pt) and sets
  code in the regular weight, as the notebook shows it.

## 0.11.0 (2026-11-12)

A correctness release: three passes over every known wrong answer and every answer that could not be checked.
No new functions.

**Wrong answers fixed (selection).**
- **Equations:** `roots` of a cubic with a negative intermediate in Cardano's formula returned values that are not
  roots; `solve` honours order conditions (`x^2 == -1 and x > 0` has no solution) and reads
  `element(point, region)`, so a line meeting a circle gives the two points; Pell equations over sign-restricted
  integers; `find_instance` checks every candidate against the whole condition.
- **Calculus:** one-sided series of step functions at a jump; `asymptotic_integrate` at infinity by Laplace's method;
  derivatives of inactive transforms; limits at `complex_infinity`; `product` at integer gamma pairs; Pade
  approximants in fractional powers; the perimeter of a curve whose speed is `sqrt(a (1 - cos u))`.
- **Intervals:** sums, products and powers of `centered_interval`s (complex ones too) are rigorous boxes; machine
  ends of computed intervals round outward; functions of several centered intervals no longer return a first-order
  estimate.
- **Linear algebra:** an arbitrary-precision symmetric eigensolver (a 100 by 100 Hilbert matrix at 200 digits);
  `matrix_exp` of inexact matrices by scaling and squaring with guard digits; `row_reduce`, `null_space`, `rank` and
  `least_squares` on matrices of quantities; `curl` of rank-2 arrays.
- **Special functions:** Neville theta and Jacobi functions at complex arguments and at an inexact `m` of 0 or 1;
  theta functions with a nome near 1; Bessel-zero derivatives; `find_root` at a multiple root no longer claims
  digits it does not have; `hypergeometric_2f1` enclosures.
- **Algebra and number theory:** Groebner bases with parameters; `modulus:` and `trig:` options of `collect`,
  `exponent` and `variables`; `continued_fraction` of machine reals gives only the terms the number determines;
  residues at root objects; Gaussian primes and prime powers; simplification of `nand`, `nor`, `xor` and `implies`.
- **Lists, data and geometry:** `partition` and `block_map` on deeper arrays; dataset queries such as
  `ds[total, "a"]`; vector-valued and two-variable `interpolation`; the exact area of two intersecting disks.

**Declines instead of a wrong value.** Where the right answer needs a method that is not there yet, the function now
returns unevaluated or a failure that says so: `asymptotic_solve` where fractional-power (Puiseux) expansions are
needed, `reduce` over the integers with symbolic parameters, stability conditions that depend on `n`,
`discretize_region` with `max_cell_measure:`, the continued fraction of Champernowne's number.

**Interrupts and speed.** A stop request ends polynomial GCD and resultant computations at once (one loop ignored
it); `full_simplify` gives the same answer on every run under a short time limit; faster `truncate_sum`, Gaussian
`is_prime_power`, `mathieu_s` examples.

## 0.10.0 (2026-11-11)

A language and editor release: a space multiplies, the input editor is new, and the documentation's plots follow
the theme.

**Language: a space means multiplication.** `m c^2`, `2 pi r`, `1 2` and `[a b, c]` are products, also across a
line end inside brackets, at the level of `*` (`a/b c` is `(a/b)*c`; `a -b` is a subtraction). **Changed meaning:**
with white space before it, a bracket starts a factor, so `f (x)`, `a [1]` and `(a+b) (c+d)` are products;
`f(x)`, `a[1]` and `(a+b)(c+d)` without the space stay a call or an index. Strings do not multiply. Results still
print `*`. `x ∈ s` reads as `element(x, s)`, a trailing `°` as degrees, and `fn f(x: numeric)` defines `f` only
for numeric arguments.

**Notebook: the input editor is new.** One editor for every code cell (crate `mc-edit`): a cell holds its formula
as a tree, each edit is an exact reversible step, and undo restores the selection. The caret is one text line tall
at the size of the row it is in.
- A faint multiplication sign shows in every space that multiplies; `Esc * Esc` types an explicit `×`.
- Warnings with a quick fix: `sin x` is the product `sin*x` (for the function write `sin(x)`), and `(a+b)(c+d)`
  without a space is a call.
- A power whose base is not one operand is drawn in parentheses (`(2m)²`); brackets that do not match yet stay in
  their fraction or root; Backspace in an empty pair removes both; an opener typed over a selection wraps it.
- Insert menu items for the box keys, with Greek letter and Symbol submenus that show each alias.
- Input and Code cells use the regular weight of the code font. The dark theme is near-black.
- Plots hold their functions: `plot(f(x), [x, a, b])` with an `f` that needs numbers (a numeric integral) draws
  without a guard.

**Documentation.** Plots are vector drawings in the window's theme, sharp at every magnification (the PNG files are
gone: `docs/functions` is 21 MB, was 63 MB; the documentation program is half its size). A page opened as a notebook
keeps themed plots, and saving it embeds them.

**Fixes (rounds 32 and 33, selection).** Hangs and memory exhaustion end cleanly (`expand` of large powers, Kelvin
functions at 0, Mathieu characteristic values at large q, Struve and Weber functions, `matrix_exp`, `permanent`,
`real_digits`, `bit_not`, `find_root` at an exact zero); `minimize` with `abs` finds the kink; eigensystems with
radicals stay exact; `inverse_jacobi_cd` picks the inverting branch; `zeta_zero(k, t0)`; `root_approximant` reads
the input's precision; constrained `find_minimum`; `piecewise_expand` at a closed end;
`polynomial_extended_gcd`; `continued_fraction_k` of constant terms; `time_series` spacing; sums display in order
of power. Faster: Jacobi functions at complex arguments, `euler_e` and `bell_b` of large index, `sequence_position`,
`fourier` (a native FFT), spheroidal plots.

**Under the hood.** The workspace is arranged by responsibility, with rules a tool checks
(`tools/depcheck.ps1`): the notebook, editor and export crates no longer depend on the evaluator; `mc-calculus` is
split into limits, integrals, transforms and differential equations; one registry crate lists the libraries. No
behaviour changed with it.

## 0.9.0 (2026-11-11)

A coverage and depth release: about 500 new functions (1,746 in all, each with a documentation page).

**New functions:** waves and pulse functions; asymptotic relations; Hankel, Hilbert and Radon transforms;
`wronskian`, `series_coefficient`, `pade_approximant`, Riccati and Lyapunov solvers, stability conditions,
`green_function`; `factor_list`, `collect`, `cyclotomic`, `partial_fractions`, `horner_form` and the polynomial list
functions; the `*_sides` equation functions; array and matrix construction (`array_pad`, Toeplitz and Hankel
matrices, `band`, special matrices, `minors`, `adjugate`, `drazin_inverse`, principal components); number forms
(`decimal_form`, `engineering_form`, `padded_form`, `base_form`); `cellular_automaton`, `integer_partitions`,
`find_sequence_function`, Roman numerals, Bell, Bernoulli and Euler numbers and polynomials, Frobenius problems,
sums of squares, 3j/6j/Clebsch-Gordan/Wigner D; the Fourier coefficient and series family, discrete Fourier
transforms, `find_peaks`, interpolating polynomials; time series and weighted data; polygon and region functions
(winding and crossing polygons, decomposition, Boolean and transformed regions).

**Special functions:** Scorer, Lommel T, Coulomb, Weierstrass (with the lattice functions), Heun, Appell, Mathieu,
Carlson, spheroidal, Fox H and Siegel theta functions; zeros of Bessel, Airy and zeta functions; q- and bilateral
hypergeometric series; series and asymptotic expansions of special functions near 0, at poles and at infinity;
double-double machine paths that make their plots fast; high-precision 2F1 fixed (a slow precision band and
growing radii).

**Optimisation:** linear, quadratic, conic, second-order-cone and semidefinite optimisation with exact optima,
integer variables by branch and bound, complex variables, and matrix and vector constraint forms.

**Language:** string patterns (blanks, runs, character classes, anchors, repetition, alternatives, conditions,
`longest`/`shortest`, regular expressions) in `string_cases`, `string_replace`, `string_split` and the other string
functions; `x[k] = v` assigns a part; `up_values`, `down_values`, `sub_values`; `quantity_array`, `subscript`.

**Wrong answers fixed (selection):** derivatives of `arg`, `abs` and `sign` at complex arguments; `series` at the
branch points of `asin` and `acos`; `is_irreducible_polynomial` over number fields; Gaussian `is_composite`,
`is_prime_power` and `is_square_free`; `find_instance` with integer conjuncts; `to_number_field` of rationals;
`dedekind_eta` at large real parts; `bin_counts` of quantities; spherical Bessel functions on the negative axis;
`polynomial_quotient` with a symbolic leading coefficient; `horner_form`; `free_of` and `is_ordered` on
associations; a plot crash at poles.

**Robustness:** `simplify`'s numeric spot check has a time budget (an exact zero of special functions no longer
climbs precision for minutes); plot memory is bounded; Appell functions of machine arguments are fast.

## 0.8.0 (2026-11-09)

A quality release: rounds 21 and 22 probed the system systematically (about 90,000 random and property-checked
inputs) and fixed what they found.

**Meijer G-function:** new `meijer_g` to any precision across the complex plane, with exact reductions to named
functions; definite integrals over (0, inf) of products of two special functions; inverse Mellin transforms of gamma
ratios (double poles included) and Mellin transforms of Bessel products.

**Wrong answers fixed:** complex `polygamma`; `hermite_h` of non-integer order; machine values of `cot`, `sec`,
`csc` and the inverse and hyperbolic functions at complex arguments (they gave nan); `kelvin` functions at negative
arguments; `beta` at negative integers; `kronecker_delta(1.0, 1)`; divergent `n_sum` and `n_product` now say so instead
of returning a number; slowly converging alternating and oscillating sums; finite `n_product`s exact; `n_integrate`
with steps and deltas; `d_solve_value` past a blow-up; `solve(x^2 == 2^x)` finds all three roots; `solve` with
assumptions on an expression; Laplace and Mellin transforms that lost a shift or a factor.

**More answers:** `reduce` with exp, log and trigonometric functions of x; implicit and logarithmic series solutions
of `d_solve`; more definite integrals and transforms (Gaussians, `erfc`, Fourier transforms of derivatives, more
z-transforms); 30-digit special functions at complex arguments; `full_simplify`, `refine` and `function_expand` gaps;
unbounded minimisation gives inf; `working_precision:` for constrained minimisation; 30-digit sums 15 times faster.

**Robustness:** `time_constrained` and `time_constraint:` stop exact arithmetic and `simplify`'s checks; inputs with
huge exponents or orders (`sin(x)^(2^31)`, `decompose` of degree 2^31, `bessel_j` of order 2^53) return at once
instead of aborting on a 16 to 64 GB allocation or hanging.

**Notebook and documentation:** eight topic guides on the documentation window's home page; 47 function pages
rewritten; the call-of-a-variable warning no longer fires on `y'(x)`; typeset cells convert to text cleanly; undo
keeps cell labels; trailing blank lines are saved; `circle_points`, `bar_legend` on density and contour plots;
overflow prints as `inf` and `indeterminate`.

## 0.7.0 (2026-11-09)

**Integral transforms:** `laplace_transform`, `mellin_transform`, `fourier_transform` (also `fourier_sin_transform`
and `fourier_cos_transform`), the two-sided Laplace transform and `z_transform`, each with its inverse, plus
`discrete_delta` and `kronecker_delta`. Shift, scaling, derivative and convolution rules and tables come first, then
definite integration; rational inverses by residues. Every value is checked numerically before it is returned.

**Definite integrals:** bounds whose order depends on a parameter give a conditional result
(`integrate(sqrt(x*(x - a)), [x, 0, a])`), and every value is simplified under the assumptions in force.

**Parallel engines:** cylindrical decomposition, the search of `simplify` and `full_simplify`, the methods of
`integrate` and `d_solve` and the checks of their answers run on all cores, with the same answers on any number of
threads (2.2 to 6.3 times faster on the heaviest cases); `MC_THREADS` sets the pool size.

**Precision:** `working_precision:` in `find_root`, `n_minimize`, `n_sum`, `n_product` and the oscillatory stages of
`n_integrate`; `n` of exact integrals gives the digits asked for; digamma and polygamma, Bessel, Airy and
hypergeometric functions at complex arguments to any precision.

**Fixes:** `gcd` of symbols stays unevaluated; periodic indefinite sums in closed form; exact comparison of algebraic
numbers; terminating hypergeometric series as polynomials; new `minus`; `numerator` and `denominator` of complex
numbers; `rasterize` properties; `+ -` and `1im` no longer printed.

**Notebook:** a call of a variable such as `x(x - a)` is underlined with a fix to `x*(x - a)`, noted under the result
and printed unambiguously; faster typing in long cells; typeset values in the documentation window can be copied; F1
inside an option opens that option's section.

## 0.6.0 (2026-11-09)

**Symbolic integration:** a tower-based Risch engine (`mc-integrate`) for exponential, logarithmic and algebraic
integrands, with special-function answers (erf, Ei, Si/Ci, Fresnel, incomplete gamma, elliptic integrals); integrands
with roots through Chebyshev's, Hermite's and Abel's methods and Euler substitutions; a rule table of 162 rules from
Rubi; definite integration that checks convergence first, splits at singularities, uses residues and closed forms,
records conditions on parameters and checks every value numerically. Every antiderivative is verified by
differentiation.

**Solving:** cylindrical algebraic decomposition and real quantifier elimination (`mc-cad`); `reduce` over several
real variables, the integers and the complexes; one front end for `solve` with transcendental families and verified
solutions, `solve_values`; exact `minimize` and `maximize` through the decomposition; `modulus:` in `reduce`,
`apart`, `polynomial_gcd` and `polynomial_lcm` (factoring modulo a prime).

**Simplification:** new `full_simplify` (a deeper search with `function_expand`), `transformation_functions:`,
`excluded_forms:` and `time_constraint:`; `simplify(lhs == rhs)` proves equations; a table of guarded
special-function identities; exact radicals for sin and cos of many rational multiples of pi. Every new rewrite is
checked numerically at 30 digits before it is returned.

**Differential equations:** `d_solve` rewritten by method family: linear equations of any order (undetermined
coefficients, variation of parameters, Euler equations, reduction of order), Airy, Bessel, Legendre, Hermite, Kummer
and hypergeometric bases, Kovacic's algorithm, nonlinear first-order classes (Bernoulli, Riccati, homogeneous,
exact, Lie symmetries, Clairaut), linear systems through the matrix exponential with real answers, first-order and
wave PDEs, delay equations; new `asymptotic_d_solve_value` (Taylor and Frobenius series). Every explicit solution
is verified by substitution.

**Numerics:** constrained `n_minimize` (augmented Lagrangian and KKT Newton steps), oscillatory and rough
`n_integrate`, integrands with conditions integrated over decomposition cells, PSLQ constant recognition; much faster
`fresnel_f`, `harmonic_number` and `partitions_p`; more special-function values (confluent U, Whittaker W, complex
Bessel Y and K, Kelvin, parabolic cylinder D at every order, Struve).

**Language and coverage:** more than 220 new functions, including 178 for visualisation (colour models, 3D solids, mesh
regions, 43 plot types), `unevaluated`, `time_constrained`, `memory_constrained`, `pause`, `quote`, `inactivate`,
subset patterns and sequence alignment; definitions with conditions are tried in the documented order; `-1` times a
sum distributes; a documentation page for every function (1314 pages), printed as the notebook prints.

**Notebook:** n-th roots in typeset math (Ctrl+5), Paste as math (Ctrl+Shift+V), pasted code split into cells,
indentation on Return, fast editing of long cells, a forgiving documentation search with near misses, copy buttons
and typeset values in the documentation window, readable error positions; `integrate` of `sinc`, and coefficient
fractions drawn without parentheses.

**Tooling:** the test suite runs on every core with progress and the time left (83 s instead of 415 s).

## 0.5.0 (2026-11-08)

**Algebraic numbers:** a number-field layer for the whole system. `factor`, `factor_square_free` and `polynomial_gcd`
take `extension:`; new `minimal_polynomial`, `to_number_field`, `root_sum`, `to_radicals`, `root_reduce`,
`algebraic_number_norm` and `algebraic_number_trace`. Rational integrals whose residues need roots of degree 3 or
more return a compact `root_sum` instead of staying unevaluated, and `x^n - c` denominators integrate over the real
radical field. Every factorisation is multiplied back and every minimal polynomial checked numerically.

**Polynomials:** subresultant chains (`subresultants`, `subresultant_polynomials`,
`subresultant_polynomial_remainders`), `decompose` (f = g(h)), `count_roots` on intervals and complex rectangles,
`modulus:` on resultants and discriminants, exact real algebraic numbers, and polynomials over parameters, prime
fields and number fields. Multivariate resultants are up to 5 times faster, `minimal_polynomial` about 10 times.

**Limits and series:** Gruntz's algorithm for limits of exp-log functions, limits of special functions at infinity
(erf, gamma, Ei, Bessel, Airy, zeta and others), and new `max_limit`, `min_limit`, `discrete_limit` and
`asymptotic`. Every limit is checked numerically before it is returned. Fixed: series of `sqrt(-x)` and
`sqrt(1 - x)` lost the imaginary unit.

**Sums and recurrences:** Gosper's and Zeilberger's algorithms, Petkovsek's Hyper and Abramov's rational solutions,
closed-form products, and `r_solve` for recurrences with polynomial coefficients. New `n_sum`, `n_product`,
`difference_delta`, `discrete_shift`, `discrete_ratio`, `generating_function`, `sum_convergence` and
`recurrence_table`. Closed forms are verified (certificates, substitution, numeric comparison) before they are
returned.

**Assumptions and conditions:** one decision service answers sign, zero, interval and integrality questions under
assumptions (numerically certified or proved, otherwise unknown); `refine` and `simplify` use it and know more rules.
New `conditional_expression` and `not_element`; `reduce` covers the signed real domains, and
`assuming(a > 0, integrate(x^a, [x, 0, 1]))` gives `1/(1 + a)`.

**Language:** `throw`/`catch`, `sow`/`reap`, `repeated(...)`, `key_value_pattern`, `optional(x_, d)` (also under
`+` and `*`, matching at the operation's unit), rules for curried calls `f(x_)(y_)`, `replace_list`, and about 45
more functions (`fit`, `covariance`, `commonest`, `bin_counts`, `take_while`, `fold_while`, `riffle` with positions,
`maximal_by`, `echo`, ...). `eval(x)` written in a held argument now forces that argument, as the semantics state.
`simplify` prefers fewer and shorter integers and takes `complexity_function:`.

**Notebook:** dialogs (unsaved changes, preferences, about, keyboard shortcuts) are modal cards with a clear default
button; menus line up and leave room for shortcuts; tall brackets around fractions and grids are drawn as strokes in
the weight of the text instead of math-font pieces, in the notebook and in PDF exports.

**Plots in the notebook:** pictures are drawn from their SVG at the screen's pixel density, so they stay sharp at
any zoom, and their background, frame, ticks, labels and legend box follow the light or dark theme. Notebooks store
plots as compressed SVG (a typical plot shrinks from about 90 KB to 7 KB). HTML exports inline the SVG and PDF
exports draw plots as vectors.

**Cell brackets** are left-justified: the outermost bracket sits at the left margin and each nested group steps one
column in, while the content stays aligned.

**Dark exports:** the new option `plot_theme: "dark"` (or `"light"`) writes a picture's chrome in that theme's
colours into every file it is exported to.

**PDF export:** SVG drawings keep their linear and radial gradients (with varying opacity) and their luminance and
alpha masks.

**Fixes:**

- Complex numbers print as sums: `3 + 2im`, `1/2 - 3im`, `-im/2*(x + y)` instead of `complex(3, 2)`;
  `full_form` and the exact form still show `complex(a, b)`.
- `-x*y` prints as `-x*y` instead of `-1*x*y`, and a negated product or quotient in an exponent keeps its
  parentheses: `2^(-x*y)` and `2^(-x/y)` printed as `2^-x*y` and `2^-x/y`, which read back differently.
- Indexing through an association inside a list works: `xs[1, "b"]`, `xs[all, "a"]`.
- `min` and `max` with symbolic arguments keep only the extreme number and drop repeats: `min(x, 2, 1, x)` is
  `min(1, x)`, `max(x, x)` is `x`.
- A notebook (`.mcn`) can be loaded as a script: its header `#! mathcas 2 notebook` is accepted.
- The documentation window remembers its theme.
- `lucas_l(-2^63)` and `n(erfc(10^8), 50)` no longer abort the program; `n(e, 10^6)` stops on an interrupt.
- Two documentation examples no longer show a path of the machine they were recorded on (Windows CI is green
  again).

## 0.4.0 (2026-11-07)

**Numerical integration:** `n_integrate` chooses a method by dimension: adaptive cubature (Genz-Malik rules) for 2 to 7
variables; randomised lattice quasi-Monte Carlo or VEGAS with VEGAS+ stratification above; Sobol and plain Monte
Carlo on request. The integrand runs in compiled blocks on all cores, results are the same at any thread count, and
the statistical methods return `around(value, error)`. New options `max_points`, `points`, `iterations`, `seed` and
`error_estimate`; a message when a goal is not met. Long compiled sums and products are much faster.

**Notebook:** math input keeps the cell font's brackets around powers and grows them as the same glyph around
fractions; no shift around boxes; no completion inside comments; keyboard control of `manipulate` sliders; higher
contrast light and dark colour schemes. The documentation window's home page is a grid of topic cards.

**Documentation:** 150 new function pages (operators, predicates, lists, associations, strings, random numbers,
distributions and statistics, colours and plot options).

**Plots:** tick positions given as exact constants (`pi/2`) are placed and labelled.

**Coverage (Phase 9):**
- Regions (`mc-geom`: membership, measure, centroid, distance, polygon boolean operations).
- About 50 new plot types (vector, stream, region, graph and chart plots, Julia and Mandelbrot sets, 3D contour,
  region and density plots).
- About 85 new special and number-theory functions, and about 40 new list and structure functions.
- Multivariate `factor` by lifting (much faster), and `minimize`/`maximize` in several variables with constraints.
- Functions of series, and sums and products with several iterators.
- `max`/`min` of intervals, uncertain values and quantities.
- A faster machine `bessel_k`.
- The REPL no longer hangs when a function writes to standard error.

## 0.3.0 (2026-11-07)

**Notebook:**
- Documentation in the editor: F1 opens a function's page, completion with one-line usage, a usage hint with the current argument, and hover; the documentation is hyperlinked throughout, with an index.
- `manipulate` with sliders, re-evaluated as they move.
- 3D pictures turn and zoom by dragging.
- Export to HTML and PDF (File > Export, `mc-repl --export`).
- Copy and paste of root, fraction and power boxes.
- One output per statement.
- One file per notebook (pictures embedded).
- Calm Ctrl+wheel magnification.
- Output alignment Left or Right (View menu), Left by default.
- `labeled` and `legended` values drawn with their labels.

**Speed:**
- Compiled code calls special functions, step functions, `piecewise`, interpolations and ODE solutions, and large right-hand sides and Jacobians compile, so typical stiff ODE workloads run several times to more than ten times faster.
- Packed complex arrays with elementwise arithmetic and functions.
- `parallel_table`, `parallel_map`, `parallel_sum`, with results identical to the serial run.
- `profile` and `timing(e, breakdown: true)`.

**Functions working together:** solve rules, interpolations, ODE solutions, quantities, uncertain numbers, datasets and distributions are accepted by the functions they naturally feed. New in this area:
- `options(...)` lists merge with later options;
- `%`, `%%`, `%n`;
- `scientific_form`, `number_form`;
- `put`/`get`;
- `domain`;
- `interpolation_order` and derivatives of interpolations;
- `find_root` methods;
- `simplify` with time budgets.

**Graphics:**
- Ticks in data units on log and signed-log axes, minor ticks, and a signed-log scale matching the common definition.
- Row and rounded legends, legend placement, label sizes, TeX `\rm`, `\it`, `\bf` and `~`, fitted framed insets.
- Deeper sampling at steep ends; a fast 3D re-render path.

**Documentation:** 412 function pages (736 functions in the help index).

## 0.2.0 (2026-11-06)

**Notebook:** one kind of cell. Input and output are the same plain text in the cell's own font, with exactly three
two-dimensional forms: square roots (`Ctrl+2`), fractions (`Ctrl+/`) and powers (`Ctrl+6`), edited in place. Outputs
read as written mathematics (`2√2`, `3x²`, `1/√z`). Outputs are right-justified; selection, Find and copying work across
the boxes. All icons, and the new application icon (a calligraphic M inside a calligraphic C), are drawn in TikZ.

**Graphics:** 3D graphics and plots (`graphics3d`, `plot3d`, `parametric_plot3d`, `list_plot3d`, `complex_plot3d`, ...)
with MathCAS's own renderer and smooth shading; `complex_plot`, `re_im_plot`, `abs_arg_plot`, `array_plot`,
`matrix_plot`, `discrete_plot`, `pie_chart`, `list_contour_plot`; layouts (`show`, `graphics_row`, `graphics_grid`,
`inset`, `callout`, `placed`); the common plot options (epilog, filling, grid lines, legend titles, plot styles, ...);
math letter styles in labels; animated GIF export.

**Mathematics:**
- **Symbolic:** assumptions throughout `simplify`, `refine`, `integrate` and `solve`; series through derivatives
  (Puiseux, logarithmic, at infinity); `d_solve`, `r_solve`, `minimize`/`maximize`, `find_instance`; the
  `function_*` properties; Fourier transforms; `complex_expand`; `function_expand`.
- **Numeric and special functions:** 74 functions defined by formulas (reciprocal and inverse trigonometric
  functions, orthogonal polynomials, `product_log`, Fresnel, spherical Bessel, ...); special functions at complex
  arguments with rigorous enclosures; uncertain numbers (`around`, `interval`); units (`quantity`, `unit_convert`);
  `sparse_array`; matrix functions; `find_fit`; boundary-value problems in `n_dsolve`.
- **Language:** recursion and memory limits, level and operator forms, structural and sequence functions,
  `composition`, and packed running sums.

**Examples:** scripts and notebooks in `examples/`.

**Documentation:** 385 function pages, each with examples checked against the engine.

## 0.1.0 (2026-11-06)

The first numbered release of MathCAS v2, under the GNU GPL version 3 or later.

**Language and engine:** functions as rule clauses with typed parameters and guards, rules, lambdas, pipes, `match`,
modules with `import`; exact integers and rationals, machine reals, arbitrary-precision balls, complex numbers;
numeric functions compiled to machine code (Cranelift JIT); packed arrays; raw strings `r"..."` for TeX labels.

**Mathematics:** polynomials (expand, factor, gcd, resultant), `simplify` with assumptions, `d`, `integrate`,
`apart`, `limit`, `series`, `sum`; `solve`, `roots`, `reduce`; linear algebra (exact, ball and machine); number
theory; special functions (gamma family, zeta, Bessel, polylog, hypergeometric, elliptic, error functions); 11
probability distributions with statistics and sampling; `find_root`, `find_minimum`, `n_integrate`, `n_dsolve`
(with events), `interpolation`.

**Data:** import and export of CSV, TSV, JSON, text tables, PNG (with pixel data) and PDF (pages, text; vector
plots); datasets with queries, aggregates, joins and pivots.

**Graphics:** a graphics model (points, lines, polygons, circles, disks, arrows, Bezier curves, text, rasters;
colours, opacity, dashing, axes, frames, legends, colour bars) rendered to PNG, SVG and vector PDF with typeset
labels; `plot` and its log variants, `list_plot`, `list_line_plot`, `parametric_plot`, `polar_plot`,
`density_plot`, `contour_plot`, `histogram`, `bar_chart`.

**Notebook:** tabs, typeset output, typesetting keys in input (fractions, powers, roots, subscripts, templates,
aliases), automatic bracket closing, pictures in cells with coordinate read-outs on plots, large-output summaries,
One Light and One Dark themes, one kernel process per tab; integrated GPU first (Direct3D 12 on Windows).

**Documentation:** a documentation window (`mc-docs`) with pages for 65 functions whose examples are checked against
the engine.

**Tools:** REPL and script runner, `.mcn` notebooks, a JSON-lines kernel, a Jupyter kernel, a language server with a
VS Code extension, a Windows installer; the workspace builds and tests on Linux.
