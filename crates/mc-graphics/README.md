# mc-graphics

Graphics without an evaluator: the inert `graphics(content, options)` term read into a model, colours and gradients,
themes, tick and label layout, three-dimensional scenes, and three renderers behind one drawing interface (raster PNG,
SVG, vector PDF).

## Modules
- `model`: reads a `graphics` term into a `Graphic`, free of the heap; `num` reads numbers in graphics terms.
- `color`, `gradient`, `theme`: colour terms and their `Rgba` reading, gradients, chrome colours for light and dark.
- `page`: the plot range, page size, margins, and painting a `Graphic` on a `Surface`.
- `surface`: the `Surface` trait every renderer implements.
- `raster`, `svg`, `pdf`: the renderers (`tiny-skia`; standalone SVG 1.1; PDF 1.4 with Flate streams and subset fonts).
- `label`, `ticks`: typeset labels (a TeX subset between `$` signs, or any typeset expression), automatic ticks.
- `three`, `view3d`: `graphics3d` scenes, and rotating and zooming a 3D picture without evaluating anything again.
- `memo`, `deferred`: the plots of the session, and files of a plot that are drawn when first needed (all of them,
  the PNG and the texts beside it included, when the front end names the files it reads; `ensure_only` writes those
  named, `ensure` all).
- API for other crates: `render`, `render_formats`, `is_graphics`, `page_size`.

## Design
- Terms are read through the store contract of `mc-api` only; there is no evaluator, so front ends use the model, the
  colours, the themes and the 3D viewer without the kernel.
- The functions that make and check graphics terms are in `mc-graphics-kernel`.
- Labels use the bundled math font through `mc-typeset`, `mc-boxes` and `mc-mathfont`.
- Relative sizes are fractions of the plot area's width; absolute sizes are points.

## Tests
- Unit tests in `src`; integration tests in `tests/` for the model, rendering to each format, regions in 2D, themes
  and the 3D view; documentation examples. Rendering tests may read back files with `resvg`.
- Run: `cargo test --release -p mc-graphics`
