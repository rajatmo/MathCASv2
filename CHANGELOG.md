# Changelog

## Unreleased

**Plots in the notebook:** pictures are drawn from their SVG at the screen's pixel density, so they stay sharp at
any zoom, and their background, frame, ticks, labels and legend box follow the light or dark theme. Notebooks store
plots as compressed SVG (a typical plot shrinks from about 90 KB to 7 KB). HTML exports inline the SVG and PDF
exports draw plots as vectors.

**Dark exports:** the new option `plot_theme: "dark"` (or `"light"`) writes a picture's chrome in that theme's
colours into every file it is exported to.

**PDF export:** SVG drawings keep their linear and radial gradients (with varying opacity) and their luminance and
alpha masks.

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
