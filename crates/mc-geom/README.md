# mc-geom

Regions: the graphics primitives that are regions (point, line, polygon, disk, ball, cuboid, polyhedra, meshes, ...),
`interval`, `implicit_region` and `parametric_region`, and the functions on them: dimension, measure, centroid,
bounds, membership, distance and nearest point, Boolean operations on plane polygons, and integrals over regions.

## Modules
- `region`: reads region terms into `Region`; `shapes` adds polyhedra and other primitives; `mesh` is `mesh_region`.
- `props`: dimension, measure, centroid and bounds as exact terms where the coordinates allow it.
- `metric`, `flat`: membership, distance, nearest points and signed distance; tori, flats and cones.
- `clip`: union, intersection and difference of plane polygons through the arrangement of their edges.
- `hrep`: membership of a symbolic point as a condition on its coordinates.
- `construct`, `derived`, `coords`: regions built from measurements, regions made from other regions (winding and
  crossing polygons), measures written as a map with ranges.
- `ex`: building unevaluated arithmetic terms and reading numbers; `natives`: the registered functions.

## Design
- Results are exact when the coordinates are exact: formulas are built as terms from the coordinates and evaluated once.
- Choices that need comparisons (which edge is nearest, whether a point is inside) are made with machine numbers, and
  the exact term of the chosen case is the result.
- Polygon Boolean operations use machine numbers; coordinates that come out as small-denominator rationals of exact
  input are given back exactly.
- `region_integrate` writes an integral over a region as iterated integrals over its parametrisation and hands them to
  `integrate`.
- Depends on `mc-api`, `mc-core`, `mc-eval`, `mc-numfn` and `mc-syntax`.

## Tests
- Integration tests in `tests/` for regions, measures, distances, Boolean operations and region integrals.
- Run: `cargo test --release -p mc-geom`
