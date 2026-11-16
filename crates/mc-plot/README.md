# mc-plot

Plots and charts built as `graphics` terms and rendered by `mc-graphics`: function and data plots, parametric and polar
curves, density and contour plots, 3D plots, charts, vector fields, graphs and animations.

## Modules
- `lib.rs`, `figure`, `held`: `plot` and `list_plot` with their logarithmic forms, the plot as a `graphics` term, and
  plots that hold their function arguments.
- `curves`, `more`, `dates`, `fields`, `regions`, `vectors`: parametric and polar plots, discrete plots, plots against
  dates, density and contour plots, region plots, vector fields.
- `plots3d`, `space`, `slices`: plots in three dimensions, level surfaces and slices.
- `charts`, `charts2`, `charts3`, `chartdata`, `panels`: histograms and bar, sector, bubble and box-whisker charts,
  and the chart data read once into a tree.
- `graphs`: graph (network) drawings.
- `anim`: `export_animation`, graphics as the frames of an animated GIF.
- `reaxis`, `roundtrip`, `sidecar`, `values`: axes of combined pictures, graphics read back from exported files, the
  `plot-N.json` sidecar with what a front end needs to map pixels to data, and values read as plot data.

## Design
- A plot evaluates to `image("path")`: a PNG in the temporary folder `mathcas-plots/`, with the same plot as SVG and
  vector PDF beside it. The PDF and the print twins are drawn when a reader first asks for them; with
  `set_files_on_demand(true)` (the kernel, for a client that names the files it reads) every file waits until it is
  asked for, the picture's size known from its layout. Readers in the session (`mc-io`, animations, charts of
  pictures) ask through `mc_graphics::deferred` before they read.
- Functions are sampled adaptively (64 even points, refined where the curve bends) through `mc-numfn`; a non-finite
  value breaks the curve, and so does a jump larger than the visible range.
- The y range uses the 1% and 99% quantiles of the samples, widened when no value lies far outside them.
- Function plots hold their arguments: the variables of the ranges are the plot's own (an outside value of the
  variable does not enter), and a domain `element([x, y, ...], region)` gives the region's box with the function
  taken only in the region (also in `contour_plot3d`, `density_plot3d` and `region_plot3d`).
- A second range may have ends that depend on the first variable (`plot3d`, the region form of `parametric_plot`).
- Depends on `mc-graphics`, `mc-graphics-kernel`, `mc-geom`, `mc-io`, `mc-numfn` and `mc-syntax`.

## Tests
- Integration tests in `tests/` evaluate plot calls on the assembled engine and check options, interop with other
  libraries, hover data, cleanup of files and chart layouts.
- Run them with `cargo test --release -p mc-plot`.
