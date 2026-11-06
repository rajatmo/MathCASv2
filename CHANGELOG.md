# Changelog

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
