# mc-plot: plots (`plot`, `list_plot`, log variants)

Since round 9 every plot builds a `graphics` term (`src/figure.rs`) and renders it through `mc-graphics`: the value is
`image(path)` naming a 640x400 PNG with the same plot as SVG and vector PDF next to it (`.svg`, `.pdf`), in
`temp/mathcas-plots/`. Labels are drawn from STIX Two Math outlines, so they look the same on every system, in every
format.

## Labels, legends, colours and dashes

Every plot takes `x_label`, `y_label` (drawn upwards), `legend: [...]` (a framed box with each curve's line, or a dot
for point plots), `legend_position: top_right | top_left | bottom_left | bottom_right`, `colors: ["#rrggbb", ...]` and
`line_style: [solid | dashed | dotted | dot_dashed, ...]`. Labels and legend entries accept the TeX subset of
mc-graphics between `$` signs (`"$\\theta$ (rad)"`); log and signed-log axes label powers as `10` with a superscript.
Minor and major grid lines are drawn as styled `grid_lines`.

Point plots of many points: the point radius shrinks as 1/sqrt(n), from 3 px at 100 points to 1 px at 900 and more,
so dense clouds (MCMC chains) stay readable.

## API for other crates

`image_of(h, t)` (an `image(path)` value's path and pixel size), `png_size(path)`, and `render_to_files(h, t, stop)`
(a graphics value rendered like a plot, used by the engine to show graphics values as pictures).

## Tests

Unit 5; `tests/plot.rs` 7 (SVG labels and colours, `.svg`/`.pdf` siblings, formatted labels, 10^5-point plots).
10^5-point `list_plot` end to end 0.42 s.

## Round 10 (package plots): Phase 5B

New plots: `list_line_plot` (and several data sets in every list plot), `parametric_plot`, `polar_plot`
(`src/curves.rs`: adaptive in the parameter, equal units, the page shaped like the curves; polar plots with axes through
the origin), `density_plot`, `contour_plot`, `list_density_plot` (`src/fields.rs`), `histogram`, `bar_chart`
(`src/charts.rs`). New options: `plot_range`, `filling` (axis, bottom, top, a value, between curves i and j),
`color_function` (a gradient name or colours), `color_bar`, `plot_points`, `contours`, `data_range`, `bins`,
`normalized`, `labels`. Values stay `image(path)` with SVG and PDF siblings.

- Jumps: a step still larger than 5 % of the visible range after every halving breaks the curve (`floor(x)` has no
  vertical risers); poles as before.
- Density plots sample an n x n grid (100) and draw a raster three times finer, bilinear between grid points, coloured
  through the gradient; points without a value are transparent.
- Contour plots (grid 80): for each level c the region f >= c is drawn over the band below in the colour of the band's
  middle value; the region is built per cell (whole cells merged into row runs, cut cells as their part above c,
  saddles decided by the cell mean), all rings anticlockwise in one path, so the union fills without seams. Level
  lines by marching squares, segments joined at shared grid edges.
- Histograms: Freedman-Diaconis width rounded up to 1, 2, 2.5 or 5 times a power of ten; bins aligned to multiples of
  the width; `[lo, hi, w]` explicit.
- **Data sidecar** `plot-N.json` next to every plot (format version 1 in `src/sidecar.rs`): page size, plot area in PNG
  pixels, axis ends in data values with their scale, series in data coordinates (at most 20000 points per entry;
  lines split where they break; bars as points at bin centres). Graphics values shown as pictures get one with no
  series. Density and contour plots have none.

Tests: unit 14; `tests/plot.rs` 13 (sidecar mapped back to PNG pixels of the curve's colour, every new plot, fillings,
ranges, jumps, failures).

## Round 12 (package io-gfx): plots take what other functions give

- `src/values.rs`: data values read as numbers. Quantities give their magnitudes, converted to the first one's unit
  on that axis, and the unit becomes the axis label. `around(v, e)` and `around(v, [e-, e+])` give v with an error
  bar. `interval([a, b])` gives its middle with a bar over the interval. `quantity(list, unit)` is spread into a
  list of quantities. A dataset gives its columns (one column: values; two: x and y, named on the axes; more: data
  sets against the first). An association gives its values, with the keys as legend entries or labels.
- Wrappers around curves and data sets (`unwrap_all`, `Wraps::apply`):
  - `legended(f, l)` gives a legend entry;
  - `labeled(f, l)`, `callout(f, l)` and `callout(f)` give a label at the end of the curve;
  - `style(f, ...)` gives that curve's directives.
  They work in plot, list plots, discrete_plot, parametric_plot, polar_plot and list_polar_plot. 3D plots take
  them off. Lists of directives are spliced into a curve's style. Curve labels get room right of the data.
- Plot functions that are quantities, and iterator ends with units, are read as numbers, with the units as axis
  labels. n_dsolve solutions are named after their function (`y(t)`) in legends.
- discrete_plot evaluates at the exact integer where the compiled function has no value (prime, moebius_mu, sums
  to n).
- histogram(data, bins, height): height is `count`, `probability`, `pdf`, `cdf` or `cumulative_count`.
- Charts skip values that are not real numbers.
- pie_chart reads associations, `legended` and `labeled` values and quantities, and draws several lists as rings.
- The epilog and prolog are made numeric as `graphics` does (`mc_graphics::numeric_content`).
- Sampling: a step still large after the usual halvings is bisected on (at most 60 more) until it is small. Only a
  step that stays large is a jump, so a sign change through a signed-log axis's linear band no longer breaks the
  curve. Steps wholly outside the coarse range widened by its size are not resolved.
- `src/roundtrip.rs`: export_data's PNG (an `iTXt` chunk) and SVG (`<metadata>`) carry the printed graphics term,
  up to 16 MB. `mc_plot::import_graphics(h, path)` and `embedded_text(bytes)` read it back; wiring import_data to
  them is gap G61. Plot display files carry no term, so documentation images stay unchanged.
- The first plot of a process sweeps `plot-*` files older than 24 hours from the plots folder.

Tests: unit 18; `tests/interop.rs` 11; `tests/plot.rs` 14 (sign changes never break a curve).
