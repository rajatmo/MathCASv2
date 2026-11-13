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

## Round 10: Phase 5B

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

## Round 12: plots take what other functions give

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

## Round 13

- 3D pictures get a scene sidecar `plot-N.scene3d` (swept with the others) for the notebook's rotation fast path.
- `render_shown_to_files` (graphics, or the layout of a labeled/legended non-picture); `export` (export_data) of
  `labeled`/`legended` values writes their layout.
- G83: `line_legend([colours], [labels])` colours the legend's samples (`legend_colors`), `swatch_legend` its
  swatches; curves keep their own colours.
- G88: `signed_log(c)` takes any positive real (exact too). The mapping is sign(v) log10(a + k|v|),
  0 at 0, k = 10^n, n = max(0, ceil(-log10 c)), a = max(1, 4n) (c = 10^-8 gives a = 32,
  k = 10^8; c >= 1 the common log10(1 + |v|)). A sign change is bridged once both sides are within the threshold's
  level. The data sidecar's signed_log axes carry `a` and `k`.
- G90: an end without a value is tried a millionth of a step inside (the curve rises from there, no deeper
  refinement between); steps larger than 5 % of the visible range are halved (up to the usual depth) unless wholly
  off screen; the y range of sampled curves is the 1-99 % quantile range weighted by the x stretch each sample
  stands for (refinement near spikes does not stretch it), cut at the quantile; values next to an end without a
  value always count. Lines break on large steps only when they are seen.
- G64 verified (exact integers substituted where the compiled value fails) and G58 verified.
- Manipulate support (measured, nothing cached): a 2D plot re-evaluated with new parameter values takes 3.3-4.5 ms
  end to end (compile, sampling, three formats, files), so a sampling cache would not pay; a default plot3d takes
  82-95 ms, most of it the SVG and PDF siblings (14 + 32 ms) and printing the term (14 ms). Writing the siblings
  lazily would halve it but every reader of them (export_data, import, the notebook) checks that they exist next
  to the PNG: left for a coordinated change.
- Tests: `tests/round13.rs` 5 (G83, G85, G88, G90, G64); `tests/plot.rs` sidecar mapping updated.

## Round nb-svg

- Grid lines of logarithmic axes are written without a colour (major: the graphic's grid colour; minor:
  `opacity(0.5)`, 0.95 gray on white as before), so they are chrome a front end themes; linear axes used
  `automatic` grid lines already. Plot files are otherwise unchanged (PNG at scale 1, SVG with chrome classes, PDF).
- `tests/options.rs`: the legend-frame check reads paths with a class.

## Round 18: plots

- **Chart data** (`src/chartdata.rs`): one reader for chart data, a tree of numbers and lists where each entry
  keeps the label and styles its wrappers give (`style`, `labeled`, `callout`, `legended`, `tooltip`,
  `popup_window`, ...), `x -> label` (a list of values with a list of labels labels each), associations (keys
  label values), quantities (magnitudes), and entries that are not numbers (`missing()`, symbols, complex values)
  left out. `sector_chart`, `rectangle_chart`, `paired_bar_chart`, `bubble_chart`, `box_whisker_chart`,
  `smooth_histogram`, `bar_chart3d`, the gauges and the new charts read through it (`src/charts3.rs`):
  grouped data, labels (`plot_labels: automatic` / given lists), per-item styles, notched boxes, smooth
  distribution functions (`"CDF"`, `"SF"`, `"HF"`, `"CHF"`), reversed gauge ranges. Data with no numbers draws
  an empty chart with a message; a bare undefined name (or `name -> labels`) as the data stays a failure (there is
  nothing to draw).
- **New plots** (`src/panels.rs`, 2D): the five pairwise plots (a grid of panels, histograms on the diagonal;
  P-P and Q-Q panels), `parallel_axis_plot`, `radial_axis_plot`, `ternary_list_plot`, `stacked_list_plot`,
  `list_fit_plot` (least-squares polynomial), `point_value_plot`, `paired_histogram`, `paired_smooth_histogram`,
  `smooth_density_histogram`, `distribution_chart` (violins), `candlestick_chart`, `renko_chart`, `dendrogram`
  and `clustering_tree` (average-linkage agglomerative clustering; Euclidean or edit distance),
  `feature_space_plot` (principal components by power iteration), `list_curve_path_plot`, `chromaticity_plot`
  (the CIE 1931 diagram from the CIE 015 spectral locus table, gamut triangles of nine RGB spaces, colours as
  points). Kernel density estimates: Gaussian kernels, Silverman's rule-of-thumb bandwidth.
- **In space** (`src/slices.rs`): `slice_density_plot3d`, `slice_contour_plot3d`, `slice_vector_plot3d` and
  their `list_` forms (named plane sets, equations, regions as level sets triangulated by marching tetrahedra,
  `element(vars, region)` domains), `list_density_plot3d`, `histogram3d`, `smooth_histogram3d`,
  `rectangle_chart3d`, `sector_chart3d`, `bubble_chart3d`, `array_plot3d`, `list_surface_plot3d`,
  `list_fit_plot3d`, `feature_space_plot3d`, `chromaticity_plot3d` (RGB gamut solids in XYZ, xyY, Lab, Luv).
  Faces are one packed polygon array with `vertex_colors` (one colour per face).
- **Dates** (`src/dates.rs`): `date_list_plot`, `date_list_log_plot`, `date_list_step_plot`,
  `stacked_date_list_plot`. v2 has no date values, so dates are read as `[y, m, d, h, min, s]` lists or strings
  ("August 15, 2006", "2006-08-15"); the axis is in days (civil-day arithmetic after H. Hinnant) with ticks at
  round years, months or days.
- Fixes: complex ranges `[z, r]` (the square of half-width r) in every complex plot; `parametric_plot` with two
  parameters fills the region the map covers; `revolution_plot3d` with a function of both parameters;
  `list_vector_plot` of a 2D table of `[point, vector]` pairs; `list_contour_plot3d` thins arrays to 48 points a
  side (it ran out of memory at 101^3); region plots unwrap labelled conditions (they timed out);
  `highlighted(f)` (`style(f, background -> c)`) in plots draws the curve thicker instead of failing;
  `color_data` metadata (`"Range"`, `"Name"`, `"ParameterCount"`, `"Image"`, `"Panel"`, `"ColorFunction"`, the
  category lists) and the CSS named colours as `color_data("HTML", name)` (W3C table).
- Tests: `tests/round18.rs` 3 (about 70 calls).
- Open: compressed data literals, datasets, time series, weighted data and numeric arrays are not values yet
  (translator and data packages); financial data by symbol, colour profiles and images in `chromaticity_plot` are
  out of scope; only the named colour schemes and indexed palettes listed in `color_data` exist.

## Round 18 (parent): graph constructors

- `petersen_graph()` and `kary_tree(n[, k])` (complete k-ary tree, vertex v's parent (v - 2)/k + 1).

## Round 20: `rasterize` properties

`rasterize(x, p)` gives a property of the picture, never the image term (before, every second argument was
ignored and the value named a file in the temporary folder): `"RasterSize"` and `"ImageSize"` (`[w, h]`, one
pixel a point), `"BoundingBox"` (`[w, h, h]`: a picture's baseline is its bottom edge, as for an image set in a
line of text), `"Data"` (rows of `[r, g, b]`, the top row first; from the rendered pixels, or from the PNG of an
image argument), `"Regions"` (`[tag, "Region"] -> [[left, top], [right, bottom]]` for each
`annotation(prim, tag, "Region")` of a graphic, pixels with y up from the bottom; through
`mc_graphics::region_on_page`; regions outside graphics give `failure(unsupported, ...)`), `"Image"` /
`"Graphics"` (the picture), `"Properties"`, and a list of properties. An unknown name gives
`failure(unknown_property, name, names)`. Options of `rasterize` (`raster_size`, `image_resolution`) are still not
used. Tests: `tests/round20.rs`.

## Round 22

- ui B21-14: `plot_legends: bar_legend(scheme)` colours density and contour plots (any plot that takes `color_bar`)
  and shows the bar; in a curve plot a scheme is still reported as not used. The page says so.
- R21-G1: not a bug now: every plot function names unknown or misspelt options ("options not used: plot_lable, foo"),
  checked for plot, list_plot, bar_chart, histogram, plot3d, contour_plot, pie_chart, log_plot, parametric_plot.
- R20-15: `write_files` removes a stale `.scene3d` sibling under the new plot's name (process id and count are
  reused across processes), the cause of the flaky 2D plot with a scene file.

## Round 24

- **R23-P1, plot files**: every engine holds a `mc_plot::SessionGuard`; the files a process wrote (`plot-<pid>-<n>.*`)
  are removed when the last session of the process ends; the first session of a process starts the stale sweep (files
  older than 24 h of dead processes, at most once an hour for all processes). Test `tests/session_cleanup.rs`.
- **X23-12**: a date component beyond any calendar (year over 10^6, month over 1.2 10^7, day over 10^9) is no date
  (the tick loops did not end); the call reports "no numeric data".

## Round 27

- X26-15: the vertices of a graph are indexed with a hash map (a linear search per edge took seconds); the spring
  layout takes a stop poll and its iteration count shrinks with n (2.5e8 pair terms in all, 30 to 400 iterations, the
  cooling schedule stretched to match), so a 2583-vertex graph is laid out in about a second. `nest_graph(n => [n + 1,
  2*n], [1], 15)` has 2583 vertices, not 32768 (the values collide).

## Round 33

- Function plots hold their first argument (`held.rs`): a range with an infinite end is drawn over a window of width 10
  (`[x, -inf, inf]` as `[-10, 10]`, `[x, a, inf]` as `[a, a + 10]`), and a domain `element([x, y], region)` replaces the
  ranges by the region's numerical bounding box, the function being taken only where `region_member(region, point)`
  holds (as `piecewise([[f, cond]], indeterminate)`; for `region_plot` as `f and cond`). A region whose membership has
  no closed form (a polygon) gets its bounding box only. Parametric plots take the bounding box only.
- `discrete_plot3d` reads its axes as `discrete_plot` does (`[i, a, b]`, `[i, a, b, step]`, `[i, imax]`, `[i, [v1, v2,
  ...]]`), several surfaces, wrapped functions (`style`, `tooltip`, `callout`, `legended`), and falls back to numeric
  evaluation of the substituted point for values the compiler cannot reach (a density of a named distribution).
- `list_step_plot` reads gaps (`none`, `missing(...)`: the line is broken), associations (numeric keys are the x values),
  quantities, sets wrapped in `style`/`legended`/..., and a wrapper around the list of sets applies to each set.
- Charts take the values of a data container (`weighted_data`, `event_data`, `time_series`, `temporal_data`).
- Not reached: the `Interpolation` cells with several variables (a tensor-product interpolant needs `mc-io`), `plot_layout`
  for lists of arrays (`list_density_plot`), `region_function` and `exclusions` options (accepted, ignored).

## Round 37: the column twin

- `finish` also renders `plot-N.column.svg` for a plot without `image_size:`/`aspect_ratio:`: the figure on a page of
  `COLUMN_SIZE` (569.4 x 355.9 pt, its 13-point text unchanged), which the notebook's PDF export draws at its picture
  column's 459.9 pt (decision u37.1, design 08, 7.5). `write_files` writes the twins before the PNG and removes twins
  an earlier process left under the same name; the session cleanup knows every sibling (`SIBLINGS`). An interrupt
  while the twins are drawn ends the evaluation. Test `tests/round37.rs` (sin, a log plot and a list plot get
  twins of their sizes; plots with `image_size:` or `aspect_ratio:` get none).

## Round 38: the column twin of 3D pictures

- `finish_graphic` also renders `plot-N.column.svg` for a 3D picture whose `graphics3d` term sets neither `image_size`
  nor `aspect_ratio` (`sized_3d`): the 2D picture `mc-graphics` made of the scene, its page set to `COLUMN_SIZE`'s
  width (569.4 points, the height from the picture's shape), its text and lines at their own sizes. The notebook's
  PDF export draws it at its picture column's 459.9 pt, so the 10-point text prints at 8.1 pt (the window shows 10 px
  beside the code's 13 px), where the screen drawing printed at 270 pt wide with 7.5-point text (design 08, 7.5).
  2D graphics shown as they are (`show(graphics(...))`) still get no twin.
- Test `tests/round38_column.rs`: three 3D plots get twins 569.4 points wide, shaped like their screen pictures,
  with as many paths; a 3D plot with `image_size:` and a 2D graphic get none.

## Round 39: data containers and labelled data

- `pie_chart` (and `pie_chart3d`) of a `weighted_data`, `event_data`, `time_series` or `temporal_data` container: of
  its values, as the other charts.
- `list_point_plot3d`, `list_line_plot3d`, `list_plot3d`: a value may be `around(x, dx)` (its centre is drawn),
  `labeled(v, l)`, `callout(v, l)` or `v -> l` (the value is drawn, the label is not); a whole data set
  `data -> labels` is the data.
