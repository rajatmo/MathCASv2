
## Labels, legends, colours and dashes (port of the reheating notebook)

Every plot takes `x_label`, `y_label` (drawn upwards), `legend: [...]` (a framed box with each curve's line, or a dot
for point plots), `legend_position: top_right | top_left | bottom_left | bottom_right`, `colors: ["#rrggbb", ...]` and
`line_style: [solid | dashed | dotted | dot_dashed, ...]`, in PNG and PDF (legend samples follow colours and dashes).
Rendering goes through a `Look` struct; the PNG uses the system font with rotated raster text; the PDF sets Greek
letters in Symbol (font F2) and Latin-1 in Helvetica. Gaps: labels are plain text (no sub/superscripts or typeset
formulas; a combining dot is not drawn in the PNG); no grid-line option.

Point plots of many points: the point radius shrinks as 1/sqrt(n), from 3 px at 100 points to 1 px at 900 and more, so dense clouds (MCMC chains) stay readable.
