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
