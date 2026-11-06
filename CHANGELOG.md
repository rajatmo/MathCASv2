# Changelog

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

**Examples:** 25 scripts in `examples/`, among them ports of the author's research notebooks on phase transitions,
gravitational waves, reheating and leptogenesis, which run faster than the originals.

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
