# mc-graphics: the graphics term and its PNG, SVG and PDF renderers (round 9; Phase 5A)

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

Wiring: `export_data` of a graphics term goes through mc-io's exporter hook
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
9. Beyond the plan: `font_size`, `scaled`, `plot_range_padding`.
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

## Round 10

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

## Round 12

- `show` combines 3D graphics (and 3D plots) into one `graphics3d`, with the box around all of them unless `show`
  gives a range. Mixing 2D and 3D is `failure(argument, show, k)`.
- `numeric_content(cx, t)` (public): closed numeric expressions in coordinates become reals, as `graphics` does. A
  complex value whose imaginary part is a rounding error (at most 1e-10 relative) becomes its real part.
- `labeled` and `legended` lay out only pictures (and `labeled` also text). Around anything else they stay inert,
  so plots and charts can read them.
- Tests: `tests/interop.rs` 4.

## Round 13

- **3D fast path** (`three::Prepared`, `view3d`): the view-independent work of a 3D graphic (box, clipping to the
  plot range, scaling, face normals, smooth-shading vertex normals, outward normals of closed solids) is done once;
  `Prepared::graphic(&View3d, page, stop)` projects, lights, depth-sorts and gives the 2D graphic. The static
  renderer goes through the same code, so a fast-path frame at the picture's own view equals the PNG.
  `view3d::Scene3d` (`sidecar_of`, `exists_for`, `load`, `cached`, `view`, `page_size`, `render`, `try_render`,
  `render_png`), `View3d { view_point, view_vertical, zoom }`, `orbit_drag`, `zoom_wheel` (interface agreed with
  the notebook in gap G79). The sidecar `plot-N.scene3d` is `mathcas-scene3d 1` and the printed term.
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

## Round 18 (phase 9C): colour models, solids, unbounded objects, mesh pictures, axes, transforms

- **Colour models**: `cmyk_color` (with an opacity fifth), `lab_color`, `lch_color` (hue in turns), `luv_color`,
  `xyz_color` are colour heads everywhere a colour is read. XYZ is relative to the D50 white (sRGB's D65 adapted
  with the Bradford transform, Lam 1985, matrices as tabulated by Lindbloom); L*a*b* and L*u*v* (CIE 1976) with L
  from 0 to 1 and the chromatic axes divided by 100, so components are of order one
  (`color_convert(red, "LAB")` gives 0.5429, 0.8081, 0.6988). `color_convert` takes
  model names as strings (`"RGB"`, `"HSB"`, `"Grayscale"`, `"CMYK"`, `"LAB"`, `"XYZ"`, `"LUV"`, `"LCH"`) or heads,
  colours as component lists (`[g]`, `[r, g, b]`, `[c, m, y, k]`) and lists of colours. `rgb_color("#f00")`
  (three hex digits). `is_color`, `color_negate` (in the model the colour is written in), `color_distance` (CIE 1976
  difference). Named colours: `dark_*` (10), `standard_*` (11, softer hues chosen to read on light and dark pages),
  `transparent`; `light_dark_switched(a, b)` is `a` in pictures (files are written for a light page; chrome follows
  the theme).
- **Exact colour arithmetic** (decision p9c.1): `blend`, `lighter` and `darker` of exact colours (named colours with
  components 0 and 1, `rgb_color`/`gray_level` of integers and rationals) with exact fractions give exact components
  (`blend([red, blue], 1/3)` is `rgb_color(2/3, 0, 1/3)`): computed in machine numbers, then recognised as small
  rationals by continued fractions (denominator at most 10^6, within 1e-12). Fractions outside [0, 1] are clamped
  (they were a failure). An opacity given as an exact `rgb_color` fourth component is kept as given.
- **Solids** in `graphics3d`: `tetrahedron`, `pyramid` (quadrilateral base and apex), `prism`, `hexahedron`,
  `parallelepiped(p, [u, v, w])`, `parallelogram(p, [u, v])` (also 2D), `torus(c, [r_in, r_out])` and
  `filled_torus` (tube radius (r_out - r_in)/2), `b_spline_surface(net)` (cubic, clamped uniform knots, de Boor's
  recurrence); the polyhedra are closed convex solids (back faces skipped). Lists of corner lists give several.
- **Unbounded objects** (`infinite_line`, `half_line`, `infinite_plane`, `half_plane`, in 2D and 3D, through two
  points or a point and directions): cut to the plot range once the extent of the other items is known (decision
  p9c.2: they never widen the range; with nothing else the range is their base point widened by 1; in 2D the range is
  then fixed, in 3D the box). Planes are cut by Sutherland-Hodgman, lines by Liang-Barsky clipping.
- **Mesh regions as pictures**: `mesh_region(points, cells)` and `boundary_mesh_region` inside `graphics` and
  `graphics3d` draw their `point`, `line`, `polygon`, `triangle` cells (and tetrahedra and other solids as faces);
  one-coordinate points lie on the x axis. The values themselves are mc-geom's.
- **Curves**: `joined_curve` and `filled_curve` of `line`, `bezier_curve`, `b_spline_curve` pieces.
- **`axis_object(path, [t0, t1], axis_label: l)`**: a line with ticks and labels (chrome, so it follows the theme)
  along `line([p, q])`, `infinite_line(...)` or `["horizontal", y, [x0, x1]]` / `["vertical", x, [y0, y1]]`
  (strings or symbols; without the range across the plot range).
- **Transforms** in 2D: `rotate(g, angle[, centre])`, `scale(g, s | [sx, sy][, centre])` (about the centre of g's
  bounding box by default), `translate(g, v | [v1, v2, ...])` (copies),
  `geometric_transformation(g, m | [m, b])`; nested transforms compose. Circles under similarities stay circles,
  otherwise ellipses and rectangles become polygons. `offset(d, pos)` positions read as `pos`.
- **Accepted, not drawn**: the effects `drop_shadowing`, `haloing`, `blurring`; the shadings `gooch_shading`,
  `stipple_shading`, `hatch_shading`, `halftone_shading`, `texture`; `hatch_filling`, `pattern_filling`,
  `clip_planes`; gradient fillings fill faces in the mean of their colours.
- **Interface wrappers**: `pane` (a margin, no border) and `panel` (a light panel with a border) are pictures like
  `framed`; `list_animate` shows its first frame; `highlighted` of a non-picture stays inert (plots read it: it was
  turned into a `style(..., background)` that broke every plot of a highlighted curve); `tab_view`, `slide_view`,
  `flip_view`, `overlay`, `item`, `button`, `popup_menu`, `list_picker`, `table_view`, `short`, `annotation` and
  the grid spans are inert names (printed as they are).
- Tests: `tests/round18.rs` 5 (exact colour arithmetic, colour models with the CIE difference and a L*a*b* round
  trip, 21 pictures with the new primitives rendered, inert wrappers).
- **Colour differences** (`color_distance(a, b, distance_function: f)`): `"CIE76"` (default), `"CIE94"` (graphic arts
  weights, the first colour the standard), `"CIE2000"` (Sharma, Wu and Dalal 2005), `"CMC"` (l = c = 1),
  `"DeltaL"`, `"DeltaC"`, `"DeltaH"`. The XYZ matrix is computed from the sRGB primaries and the D65 white
  (0.31271, 0.32902), adapted to D50 (0.34567, 0.35850) by Bradford; with these chromaticities red is L*a*b*
  0.54288645345387, 0.80811342689126, 0.69884257076298.
- **Transformation functions** (`transform.rs`): `rotation_transform` (plane, about a point, about an axis in
  space by Rodrigues' formula, in the plane of two vectors), `translation_transform`, `scaling_transform`,
  `shearing_transform`, `reflection_transform`, `affine_transform`, `rescaling_transform` evaluate to
  `transformation_function(m)` with the homogeneous matrix; entries are terms (exact and symbolic), the
  translation column expanded. `transformation_function(m)(p)` maps a point or a list of points (a curried
  native); `transformation_matrix(f)`; `geometric_transformation(g, f)` draws with it.
- `multicolumn(list, n)` (or `[n, rows]`) is a grid picture filled down the columns; `magnify(x)` is x as a picture
  (`magnify(x, k)` of a non-picture stays inert); `graphics_group` draws its content;
  `theme_color("AccentK")` draws in the standard palette but is not a colour value for `is_color`; `raster3d` is accepted in 3D graphics and not drawn (gap); `font_family`, `font_weight`,
  `font_slant`, `image_size_multipliers` are names (style options).
- 2D: `cuboid(p, q)` is a rectangle, `parallelepiped(p, [u, v])` a parallelogram, `annulus(c, [r0, r1], [t0, t1])` a
  sector of the ring, `axis_object` along `half_line`, `bezier_curve` and `b_spline_curve` (ticks by arc length),
  `inset` takes named and `automatic` positions; `joined_curve`/`filled_curve` take a list of curves (filled
  even-odd: inner ones are holes) and `curve_closed`. Primitives whose coordinates are positive integers (indices
  into the points of a `graphics_complex` or mesh) stay as they are instead of failing.
- `list_animate` of values that are not pictures shows the first in a pane; `show` takes regions (mesh regions and
  region primitives) as pictures.

## Round 20

`region_on_page(h, whole, part, stop)`: where the primitives of `part` lie on `whole`'s page, in points (y down),
through the same plan (plot range, area) the page is painted with; `rasterize(g, "Regions")` uses it.

## Round 22

- R20-08: `circle_points([r, theta], n)` and `circle_points(c, [r, theta], n)`: radius r, first point at angle theta.

## Round 24

- **X23-8**: a layout part nested deeper than 200 levels is shown as an ellipsis instead of being typeset (the typesetting
  was quadratic in the depth).

## Round 27

- X26-13: `Label::of` of an integer of more than 1000 limbs (about 20,000 digits) is `≈ d.ddddd×10^k` from its top 128
  bits; printing all digits (seconds for 3^(10^6)) is no use in a drawing. `grid([[2^(10^7)]])` takes 0.6 s.

## Round 28

- `normal` of `translate`, `rotate`, `scale`, `geometric_transformation` of a point, line, polygon, triangle,
  cuboid (translation, scaling), sphere, ball, cylinder (translation, rotation): the primitive with mapped
  coordinates (`transform.rs`, `NormalGeometry`). Anything else is left to the other `normal`.
- `dot(f, g, ...)` of transformation functions is their composition (`DotTransforms`), registered by
  `install_late` after `mc-linalg` (one line in the engine).

## Round 29

- Polylines are cut at a rectangle four page sizes around the page before stroking (`page.rs`, `clipped_runs`,
  Liang and Barsky 1984): a curve with huge values (`plot([weierstrass_p(x, [1, 2]), 2], [x, -6, 6])`, values
  1e6 and beyond near the pole) overflowed the rasterizer's stroker stack. Segments leaving the rectangle end a
  run; non-finite points break the line.
- The remembered terms (`MEMO_BYTES` 24 MB) and graphics (`GRAPHICS_BYTES` 48 MB, by a points-and-cells estimate,
  nothing over a quarter of that kept) are bounded in bytes, not only in entries: eight 3D surface plots in one session
  went from 590 MB to a plateau of 462 MB (a 3D list contour plot ran out of memory under a 700 MB cap).
- `color_convert(c, "XYZ", white_point: w)`: `w` a standard illuminant name (CIE 15: A, B, C, D50, D55, D65, D75, E,
  F1-F12) or an `xyz_color`; XYZ relative to D50 adapted to `w` by the Bradford transform (Lam 1985), so the white of
  sRGB maps to `w`. Other models ignore the option.
- Not bugs / not reached: `color_data` palettes and ranges (`"VisibleSpectrum"` range 380..750, named gradients) are
  data tables not derivable from a formula; `rasterize` region and size values depend on the font's text metrics;
  `is_color(unevaluated(red))` (a held argument is passed unwrapped by design).

## Round 33

- 3D `bezier_curve` and `b_spline_curve` are drawn (the same samplers as the plane's, with three coordinates).
- Plane `conic_hull_region` (an apex and its rays) is an unbounded object cut to the plot range like `half_plane` (kind 4).

## Round 34: the split

- The crate is now the graphics model as plain data, the colours, themes, labels, the PNG, SVG and PDF renderers and
  the 3D viewer. It reads terms through the store contract of `mc-api` only and has no evaluator, so the notebook
  uses it without `mc-eval` and `mc-core` (rule in `tools/depcheck-rules.txt`).
- The natives (`graphics`, `show`, primitives, directives, colour functions), the transformations and the layouts
  of pictures moved to `mc-graphics-kernel` with their `FUNCTIONS`; the plot memos moved from `compose.rs` to
  `memo.rs` here, since the model's insets read them. No behaviour changed.

## Round 37: regions of the plane as primitives

- `sphere(c, r)` of two coordinates is the circle; `ellipsoid(c, sigma)` with a symmetric positive definite 2 x 2
  matrix is the ellipse (x - c)^T sigma^-1 (x - c) = 1 (semi-axes the square roots of the eigenvalues, along the
  eigenvectors; 128 points); `cuboid(p)` the unit square at p; `simplex(2)` the standard triangle and
  `simplex(points)` the polygon, segment or point of up to three points; `empty_region(2)` draws nothing.
  Tests: `tests/regions2d.rs` (every point of the ellipse checked against the quadratic form).
- Not done: positions `scaled(d, p)` and `image_scaled(d, p)` (a scaled offset from a coordinate: the point model
  holds either kind, not a sum of both), `winding_polygon` and `crossing_polygon` (the renderer fills even-odd
  only; drawing another rule's region even-odd would be a wrong picture).
