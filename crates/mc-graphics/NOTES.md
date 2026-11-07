# mc-graphics: the graphics term and its PNG, SVG and PDF renderers (round 9, package gfx; Phase 5A)

## Modules

| module | contents |
|---|---|
| `model` | `graphics(content, options)` read into a heap-free `Graphic` (items, styles, options); content walked with an explicit stack; a directive applies to what follows it in its list and nested lists |
| `natives` | primitives and directives (kept when fine or symbolic, `failure(argument, f, k)` when numerically wrong); `graphics` (numeric constants to reals, then the whole term checked); `show`; colour functions |
| `label` | labels as `mc_boxes` rows laid out with STIX Two Math metrics; a TeX subset between `$` signs; `subscript`/`superscript` as real scripts; other expressions printed in traditional form |
| `page` | the painter: ranges (2 % padding; given ranges clip), page size and aspect ratio, margins from tick labels, background, grid lines, primitives, frame with inward ticks or axes through the origin, labels, legend |
| `surface` | one `Surface` trait (fill, stroke, dots, text, clip) and three implementations: `raster` (tiny-skia, PNG, scale up to 16, at most 40 M pixels), `svg` (SVG 1.1, glyphs as path defs with `<use>`, labels' text in `aria-label`), `pdf` (PDF 1.4, Flate on every stream, opacity through ExtGState) |
| `ticks`, `num` | nice ticks, tick labels (`m x 10^k` for very large or small), number reading |

Primitives (2D): `point`, `line`, `polygon`, `rectangle`, `circle`, `disk` (with ellipses, arcs, sectors), `arrow`,
`bezier_curve`, `text(expr, pos[, offset])`, positions also `scaled([u, v])`; packed n x 2 arrays read without
unpacking. Directives: `rgb_color` (3 or 4 numbers or `"#rrggbb"`), `hue`, `gray_level`, 15 named colours,
`opacity`, `thickness`, `absolute_thickness`, `point_size`, `absolute_point_size`, `dashing` (list or
`solid`/`dashed`/`dotted`/`dot_dashed`), `absolute_dashing`, `edge_form`, `face_form`, `font_size`. Options:
`plot_range`, `plot_range_padding`, `axes`, `frame`, `axes_label`, `frame_label`, `plot_label`, `aspect_ratio`,
`image_size`, `background`, `ticks`, `grid_lines`, `legend`, `legend_position`, `font_size`. Colour functions:
`color_convert`, `blend`, `lighter`, `darker`.

TeX subset in label strings: `^`, `_`, groups, alias-table commands (`\alpha`, `\le`, ...), `\frac`, `\sqrt`,
`\mathrm`/`\text`, operator names (`\sin`), spacing; math letters italic, `-` a minus sign; `\$` a dollar; nesting
bounded at 16. In MathCAS strings the backslash is written twice (`"$\\alpha$"`).

## API

`render(h, t, Format::{Png { scale }, Svg, Pdf}) -> Result<Vec<u8>, Failure>`; `render_formats(h, t, &[Format],
stop)` reads once for several formats; `is_graphics`; `page_size` (points = PNG pixels at scale 1); `Failure { tag,
detail }` (`not_graphics`, `not_numeric`, `option`, `unknown_option`, `too_large`, `argument`, `interrupted`, `io`).

Wiring (coordinator, at the merge): `export_data` of a graphics term goes through mc-io's exporter hook
(`mc_engine::export_graphics`: PNG at `dpi`/72 pixels per point, SVG, PDF); a graphics **value** is shown as its
picture: the engine renders it like a plot (`mc_plot::render_to_files`: PNG with SVG and PDF siblings) and shows
`image("path")`, so the REPL, kernel, notebook and documentation display it; the value itself stays the term.

## Decisions (GFX)

1. PDF fonts are Type 3 subsets from STIX Two Math outlines (at most 256 glyphs per font) with ToUnicode maps
   (surrogate pairs for astral characters); text extracts correctly.
2. Every label uses STIX Two Math (no sans font bundled); all renderers draw glyphs from outlines, so PNG, SVG and PDF
   look alike and PNGs do not depend on system fonts.
3. Units: 1 point = 1 SVG unit = 1 PNG pixel at scale 1; default width 360; `aspect_ratio: automatic` is equal axis
   units (ratio clamped to [0.02, 50]).
4. `thickness`, `point_size`, `dashing` are fractions of the plot-area width; `absolute_*` in points; defaults 1 pt
   lines, point size 0.012, solid; dashed lines with round caps.
5. Faces fill with the current colour without an edge unless `edge_form`; default background white (`none` is
   transparent) so pictures stay readable in dark themes.
6. Named colours are inert symbols that are colours wherever one is expected; `color_convert` targets are the head
   names.
7. `graphics` evaluates numeric constants in coordinates to reals; `render` reads literal numbers only (a symbolic
   coordinate gives `not_numeric`).
8. One `ticks` option for frame and axes; a pair of specs is one per axis.
9. Beyond the brief: `font_size`, `scaled`, `plot_range_padding`.
10. Plot PNGs stay at scale 1 (640x400); sharper pictures come from the SVG sibling or `Png { scale }`.

## Tests and measurements

11 unit tests; `tests/model.rs` 5 (about 90 cases; terms printed through `to_string`, since a graphics value shows
as its picture); `tests/render.rs` 13 (each primitive in each format with PNG pixel probes; each SVG rasterised with
resvg within a mean of 3 per channel of our PNG; PDF structure: xref, stream lengths, Flate, MediaBox, Type 3 with
CharProcs and ToUnicode; decorations, legends, scientific ticks, sizes, scale 2, `too_large`, failures,
`interrupted`; 4 PNG hashes and 1 SVG snapshot). 29 documentation pages with pictures.

10^5 points plus a 10^5-point line to PNG + SVG + PDF from one reading: 0.59 s (161 KB, 2.8 MB, 0.79 MB).

## Dependencies

`miniz_oxide` 0.8 (MIT OR Zlib OR Apache-2.0), `tiny-skia` 0.11 (BSD-3-Clause), `mc-boxes`/`mc-mathfont`; dev:
`resvg` 0.49 (Apache-2.0 OR MIT).

## Known gaps

No `graphics3d`, `arrowheads`, separate frame ticks, polar coordinates, raster/image primitive or `inset`; the TeX
subset lacks `\left...\right`, accents, matrices and `\over`; serif text only.

## Round 10 (package plots)

- `raster(cells, [[x0, y0], [x1, y1]], gradient)`: cells are numbers in [0, 1] (gray or a gradient), colours or
  `none`; rows from the bottom; packed `rows x cols` or `rows x cols x 3|4` arrays read directly; at most 4 M cells.
  Surfaces gained `image(rect, cols, rows, rgba)`: tiny-skia nearest-neighbour pixmap; SVG `<image>` with a base64 PNG
  and `image-rendering` pixelated; PDF image XObject (`/Interpolate false`, `/SMask` when some cell is transparent).
- `gradient` module: named gradients `viridis` (default), `thermal`, `cool_warm`, `ocean`, `earth`, `rainbow`, `gray`,
  or a list of colours (colour terms or `"#rrggbb"`); `gradient_color(g, t)`.
- `color_bar: [gradient, [lo, hi]]` or `[gradient, [lo, hi], label]`: a bar right of the plot area with ticks and a
  label read upwards; the margin is reserved by the planner.
- `arrowheads(s)` / `arrowheads([s0, s1])` / `arrowheads(automatic)`: head lengths as fractions of the plot width.
- `render_with_layout` gives the `Layout` (page size, plot area, ranges) used by the plot sidecar.
- Polygons with holes were not needed: contour regions are unions of non-overlapping anticlockwise rings in one path.
- Test dependency: `resvg` now with its `raster-images` feature (pure Rust: `zune-jpeg`, `gif`, `image-webp`).

## Round 12 (package io-gfx)

- `show` combines 3D graphics (and 3D plots) into one `graphics3d`, with the box around all of them unless `show`
  gives a range. Mixing 2D and 3D is `failure(argument, show, k)`.
- `numeric_content(cx, t)` (public): closed numeric expressions in coordinates become reals, as `graphics` does. A
  complex value whose imaginary part is a rounding error (at most 1e-10 relative) becomes its real part.
- `labeled` and `legended` lay out only pictures (and `labeled` also text). Around anything else they stay inert,
  so plots and charts can read them.
- Tests: `tests/interop.rs` 4.

## Round 13 (package gfx-int)

- **3D fast path** (`three::Prepared`, `view3d`): the view-independent work of a 3D graphic (box, clipping to the
  plot range, scaling, face normals, smooth-shading vertex normals, outward normals of closed solids) is done once;
  `Prepared::graphic(&View3d, page, stop)` projects, lights, depth-sorts and gives the 2D graphic. The static
  renderer goes through the same code, so a fast-path frame at the picture's own view equals the PNG.
  `view3d::Scene3d` (`sidecar_of`, `exists_for`, `load`, `cached`, `view`, `page_size`, `render`, `try_render`,
  `render_png`), `View3d { view_point, view_vertical, zoom }`, `orbit_drag`, `zoom_wheel` (interface agreed with
  nb-mat in gap G79). The sidecar `plot-N.scene3d` is `mathcas-scene3d 1` and the printed term.
- `graphics3d` takes `view_vertical`; faces of closed convex solids (sphere, cuboid, capped cylinder and cone)
  turned away from the eye are not drawn when opaque and unclipped.
- `compose::shown_layout(h, t)`: the picture of `labeled`/`legended` around a non-picture (gap G80); the kernel's
  shown value stays the term (the notebook draws it), `export_data` uses the layout.
- Layout pages use `image_padding: none` (a new `graphics` option: the blank margin in points, default 4); text
  grids keep a 2-point margin, so framed columns fit their text (G85). Styled labels (`style(x, colour)`) are drawn
  in their colour in layouts.
- TeX labels: `~` is a space in math; `\rm`, `\it`, `\bf` switch letters to the end of the group (G84).
- Frames get unlabelled minor ticks between automatic ticks: a quarter of a step of 2, a fifth of 1 or 5 (G89).
- `render_read` gives the graphic read with the files (plots keep it for layouts without a second read).
- Tests: `tests/view3d.rs` 7 (+1 ignored stage timing), `tests/interop.rs` 5, label and ticks unit tests.
- Timings (release): plot3d 61 x 61 end to end 82-95 ms (render PNG 13, SVG 14, PDF 32 ms after a 7 ms read; print
  14 ms). Fast path at a 640-point page and scale 1.5: plot3d 61 x 61 load 50 ms once, 28 ms a frame; 100 x 100
  load 130 ms, 55 ms a frame; 16 spheres (12800 faces) 90 ms a frame (193 ms before back faces were skipped).

## Round nb-svg: chrome marked for themes

- `surface::Role` (`Data`, `Background`, `Frame`, `Text`, `Grid`, `Panel`, `Ink`) and `Surface::role` (default: ignored).
  The painter sets it before each call: the background (only when no `background` option was given:
  `Opts::themed_background`), grid lines without a colour (a styled grid line keeps `Role::Grid` until a colour
  directive), the frame, axes, ticks and colour-bar frame, every label it typesets, the legend box (`Panel`), frame
  and title rule; primitives take their style's role: `Style::uncoloured()` (the content's first style) is
  `Role::Ink` until a colour directive makes it `Data`, so `graphics([circle()])` draws in the theme's text colour.
  Faces drawn in the current colour follow the style's role; `face_form` colours, edges, rasters and insets are data.
  3D: the box, ticks and labels are chrome; lit faces are data.
- SVG: chrome elements get `class="mcf-<role>"` (fill) or `class="mcs-<role>"` (stroke) and keep the light colours as
  attributes (other readers see the light look); the root element has `class="mc-graphic"` (`SVG_ROOT_CLASS`).
- `theme::Theme { background, text, frame, grid, panel }`, `Theme::LIGHT` (what files are written with),
  `Theme::color(role, c)` (the theme colour with `c`'s opacity; `None` for a transparent background), `Theme::css()`
  (one rule per role and property: injected by a front end as `usvg::Options::style_sheet`; CSS overrides
  presentation attributes), `Theme::key()` (cache keys), `Theme::contrast` (WCAG ratio).
- The raster surface recolours chrome itself with `Raster::with_theme`; `render_rgba_themed`,
  `Scene3d::render_themed` / `render_draft_themed` (the notebook's turned 3D pictures).
- Decision nb-svg.1: chrome is marked by role where it is drawn, never recognised by colour. Decision nb-svg.2:
  primitives without a colour directive are chrome (`Ink`), like `currentColor`, so default-black drawings stay
  visible on a dark page; a colour given by the user (also white or black) is data.
- Tests: `theme` unit test; `tests/theme.rs` 3 (classes on chrome and none on data, an uncoloured line is ink, a given
  background is not themed; resvg with the dark style sheet: transparent corner, curves unchanged, the theme's frame
  colour present, the light one absent, and the themed raster surface within a mean of 3 per channel of it; the light
  style sheet changes nothing); `tests/render.rs` SVG snapshot and path count updated. PNG snapshots unchanged.
