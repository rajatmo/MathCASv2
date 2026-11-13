# mc-geom: regions and the region functions

Module docs in `src/lib.rs` describe the crate (regions read from graphics primitives, exact measures,
centroids, bounds, membership, distances, Boolean operations on plane polygons, integrals over regions).

## Round 18 (geometry)

### New regions (read by every region function)

| module | contents |
|---|---|
| `shapes` | `pyramid`, `prism`, `hexahedron` (convex polyhedra with the usual corner orders), `parallelepiped` and `parallelogram` (polygon, polyhedron or segment by the number of vectors), `torus`, `filled_torus`, `infinite_line`, `half_line`, `infinite_plane`, `half_plane`, `conic_hull_region`, `cylinder` and `cone` as solids, indexed `polygon(points, indices)` |
| `flat` | distances for tori (the tube around the centre circle), flats and cones (active sets of the non-negative directions, exact projection), unions; cylinders and cones through their meridian section (a rectangle or triangle in the (axial, radial) half-plane); ellipses and ellipsoids (Eberly's root of the secular equation, with the degenerate case in a symmetry plane, exact when the multiplier is rational); linear relations in normal form for symbolic membership |
| `mesh` | `mesh_region` and `boundary_mesh_region` (cells held, evaluated below the cell heads; canonical form: real coordinates, cells by kind in decreasing dimension, segments, unused vertices dropped, tetrahedra turned positively, plane boundary loops anticlockwise with clockwise holes, `method: ["SeparateBoundaries" -> false]` on boundary meshes), `mesh_cells`, `mesh_primitives`, `mesh_cell_count`, `mesh_coordinates`, `is_mesh_region`, `is_boundary_mesh_region`, `dimensional_mesh_components`, `connected_mesh_components`; a mesh as a region is the union of its cells (a boundary mesh the solid it bounds) |
| `construct` | `sss_triangle`, `sas_triangle`, `asa_triangle`, `aas_triangle`, `perimeter`, `circumsphere`, `region_moment` (through `region_integrate`), `is_constant_region`, `canonicalize_polygon`, `polygon_coordinates`, `outer_polygon`, `inner_polygon`, `is_simple_polygon`, `is_convex_polygon`, `region_equal`, `region_within`, `region_disjoint` (line regions as interval unions, plane polygons through the clipper), `bounding_region` (boxes, the smallest disk or ball, the convex hull), `winding_count`, `crossing_count` |

### Decisions

1. Region heads stay inert and are not registered here (they are graphics primitives; `mc-graphics` draws them);
   they are listed in `FUNCTIONS` so the names exist. Only `simplex` is a native: a simplex of one point is that
   point.
2. Lower cells of a mesh are the faces of its cells in order, each once with its first orientation; the faces of a
   tetrahedron (a, b, c, d) are (c, b, a), (b, c, d), (c, a, d), (a, b, d).
3. `region_member` of a symbolic point is `element(coordinates, reals) and conditions` (the coordinates' real
   domain stated, as region membership is over the reals); a box's conditions are one
   pair per coordinate, ellipsoids with integer radii have denominators cleared, triangles and half-planes give
   linear relations (first coefficient positive, positive terms left).
4. The measure of finitely many points is their number (counting measure).
5. A box with symbolic sides has the absolute value of the product as its measure.
6. Parametric regions: the Jacobian's absolute value is dropped when its sign at a sample point inside the
   parameter box is definite (integrals then evaluate).

### Closed forms

- Torus nearest points are given in closed form (not as root objects of degree 4 in two variables).
- Ellipsoid distances are exact whenever the Lagrange multiplier is rational.

### Tests

`tests/round18.rs` 3 (about 45 cases); `tests/regions.rs` updated for decisions 3 and 4.

### Gaps

Symbolic membership of polyhedra, cylinders, cones and tori; `transformed_region` (needs the transformation
functions); `boolean_region`, `polygon_decomposition`, `winding_polygon`, `region_member(reg)` with one argument, `crossing_polygon`;
`discretize_region`; `highlight_mesh`; one-dimensional boundary meshes regrouped into components.

## Round 19 fixes

- **f19.1** `area`/`volume` of an implicit region of at most three variables (Q19-28): the cells of its
  cylindrical decomposition integrated symbolically (nested definite integrals of 1), accepted only when every
  integral has a closed form and the sum agrees with the numeric measure to 1e-4: `area(implicit_region(x^2 + y^2
  <= 1, [x, y]))` is `pi` (it was 3.1415939). The three-variable case still needs the numeric measure (missing), so
  it stays as before.

## Round 28

- `is_bounded_region` of an unevaluated `region_union` (all parts bounded) or `region_difference` (its first region)
  decides instead of answering false (1D mesh set operations stay unevaluated; an unevaluated intersection still answers
  false).
- `region_distance`, `signed_region_distance`, `region_nearest` of an `annulus` (numeric points).
- By design at that point: `crossing_count` casts one ray (the count depends on its direction on non-convex
  lines); `winding_count` of a polygon with repeated vertices is a signed count (-2); `region_moment` of an intersection of disks uses the polygon approximation (1e-4).

## Round 29

- New module `derived`: `winding_polygon` (non-zero winding rule) and `crossing_polygon` (even-odd rule) through the
  edge arrangement of `clip` with a new `Fill` parameter (`clip::boolean_fill`); `polygon_decomposition` (ear clipping,
  Meisters 1975; polygons without holes); `boolean_region(f, regions)` for up to five plane regions: the union, over
  the truth assignments for which `f` is true, of "inside exactly these regions"; `transformed_region` for points,
  lines, polygons, triangles, tetrahedra, rectangles (to polygons), cuboids (to hexahedra) and disks/balls under a
  similarity, by applying the `transformation_function` to the corners; `discretize_region` (triangle mesh of plane
  polygons, disks as 256-gons, line meshes); `highlight_mesh(m, style(d, ...))` (the mesh with the cells of dimension
  d drawn); `capsule_shape` (volume, region_measure only).
- `region_dimension` of a line whose points all coincide is 0 (Line 35).
- Gaps: `transformed_region` under nonlinear maps and of disks under general affine maps (an ellipse) stays
  unevaluated; polygons with holes are not decomposed or discretised; capsules are not read as regions (membership,
  distance, centroid); `boolean_region` of meshes, 3D regions and lines; `boolean_region(and, ...)` as a bare
  function name is impossible (`and`, `or` are keywords: use `(a, b) => a and b`, or `xor`).

## Round 30

- crossing_count is the least number of crossings of a ray from p with the curve over 720 directions (off the
  vertices), plus 1 when p lies on the curve (0 below the arrowhead's notch where an upward
  ray would cross twice, 1 for the five-pointed star from inside its pentagon point where rays to the right cross
  three times, 1 on the curve). winding_count of a polygon treats it as a region: a vertex cycle repeated k times
  is taken once and oriented positively (the triangle traced twice clockwise has count 1); lines
  and point lists keep direction and multiplicity.
- mc-plot color_data("VisibleSpectrum"): the range [380, 750] nm and a piecewise linear spectrum through violet,
  blue, cyan, green (550 nm pure green), yellow, red. mc-graphics is_color: light_dark_switched(...) is not a colour
  value (like theme_color).

## Round 33

- `coords.rs`: `arc_length(map, [t, a, b])`, `area(map, [u, a, b], [v, c, d])`, `volume(map, ranges...)` with an optional
  coordinate system (`"Polar"`, `"Cylindrical"`, `"Spherical"`; the map is converted to cartesian first) are the measures of
  the parametric region of the map; `perimeter(map, ranges...)` and `perimeter(parametric_region(...))` of a rectangle of
  two parameters in the plane sum the lengths of the four edges, an edge that coincides with the opposite one (the seam of
  a full turn) not counted: the annulus `[r sin t, r cos t]`, `r` in 1..2, `t` in 0..2 pi has perimeter 6 pi (checked: two
  circles, 2 pi + 4 pi), the sector 2 + pi/3.
- Implicit regions: `region_integrate` (hence `region_moment`) over an implicit region of at most three variables uses the
  cells of the cylindrical decomposition (as the exact measure did), accepted only when the measure of the same cells agrees
  with the numeric measure; `volume` of a solid given by inequalities has a numeric check now (Gauss-Legendre on panels in
  x and y, bisection along z, good to about 1e-3), used to confirm the exact value, never returned by itself for exact
  input. `region_nearest` and `region_distance` of an implicit region minimise the squared distance under the condition
  (`minimize`): the point (3, 4) has nearest point (3/5, 4/5) and distance 4 from the unit disk.
- `winding_polygon` and `crossing_polygon` take a list of closed curves (the rule applies to all the edges together).
- mc-graphics draws `conic_hull_region` in the plane (one apex with rays: a ray or the wedge between the extreme rays; a
  cone of half the plane or more is not drawn). Not done: the multi-curve results print as rings, not as one
  `polygon(points, [index lists])`.

## Round 36, later fixes

- region_union/intersection/difference with a disk stay unevaluated (a 256-gon made the lens area 1.228159 against
  (4 pi - 3 sqrt(3))/6 = 1.228370); discretize_region meshes them; area and region_moment(., [0, 0]) of two
  intersecting disks are the exact lens (lib/numerics.mc). Other moments of such regions stay unevaluated (gap).
- perimeter of a parametric region whose boundary curve is run k times over (astroid on [0, 4 pi]) counts one period
  (periodicity tested numerically at three points: simplify does not reduce cos(t + 2 pi)).
- discretize_region with max_cell_measure gives failure(unsupported_option, max_cell_measure) (the option is not
  honoured).

- `region_intersection` of two exact disks stays whole; `area`/`region_measure` give the exact lens (two circular segments), `region_member` the conjunction, `discretize_region` the mesh of the 256-gon intersection. Was the area of the polygon, off by 2e-4.

## Round 36, symbolic fixes

- `region_member(infinite_line(...), [x, y])` with a symbolic point in the plane: the line's equation n.(p - q) == 0 with the normal (-d2, d1), so `solve(element([x, y], infinite_line(...)) and element([x, y], circle()), [x, y])` gives the two points (was a one-parameter family: the line's condition stayed unevaluated).
## Round 36, later fixes

- One lens implementation: `area`, `region_measure` and `region_moment(., [0, 0])` of the intersection of two solid
  plane disks all use `lens_area` (natives.rs; the two circular segments, simplified). The library copy `nl_lens` in
  `lib/numerics.mc` is gone. Machine data are accepted for the measures (`two_disks`, which keeps the exact disks as
  an intersection in Boolean operations, still takes exact data only); radii must be positive.

## Round 39: areas from vertex differences

- Plane areas (and the vector area in space) are the fan of triangles from the first vertex, i.e. the shoelace sum
  after translating that vertex to the origin, expanded: the area of `parallelogram(p, [u, v])` is
  `abs(u_x v_y - u_y v_x)` without the coordinates of p, which the shoelace sum of the vertices themselves only
  cancelled after an expansion that was not done (a long sum under `abs`). The centroid of a plane ring is computed
  the same way and translated back, each coordinate the first vertex's plus a quotient in lowest terms.
