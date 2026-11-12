//! MathCAS v2 regions: the graphics primitives that are regions (`point`, `line`, `polygon`, `triangle`,
//! `tetrahedron`, `simplex`, `disk`, `circle`, `ball`, `sphere`, `ellipsoid`, `rectangle`, `cuboid`), `interval`,
//! `implicit_region(cond, vars)` and `parametric_region(fs, specs)`, and the functions on them: dimension, measure
//! (length, area, volume), centroid, bounds, membership, distance (also signed) and nearest point, Boolean
//! operations on plane polygons, and integrals over regions.
//!
//! Results are exact where the region's coordinates are exact: the formulas are built as terms from the
//! coordinates and evaluated once. Choices that need comparisons (which edge is nearest, whether a point is
//! inside) are made with machine numbers, and the exact term of the chosen case is the result. Boolean operations
//! on polygons work with machine numbers (module [`clip`]); coordinates that come out as small-denominator
//! rationals of exact input are given back exactly.
//!
//! **Integrals over a region**: `region_integrate(f, [x, y], reg)` (also `region_integrate(f, element([x, y],
//! reg))`) writes the integral as iterated integrals (`integrate`) over the region's parametrisation: boxes
//! directly, disks and balls in polar and spherical coordinates, simplices and polygons (triangulated) over the
//! standard simplex, curves and surfaces with their length or area element. The symbolic integrator can hand
//! `integrate(f, element(vars, reg))` to it.
#![warn(missing_docs)]

pub mod clip;
pub mod construct;
pub mod coords;
pub mod derived;
pub mod ex;
pub mod flat;
pub mod mesh;
pub mod shapes;
pub mod metric;
pub mod props;
pub mod region;
mod natives;

pub use region::{read, Radius, Region, Ring};

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("region", "region(r): the region r shown as a region (it stays as it is; the region functions read through it)"),
    ("implicit_region", "implicit_region(cond, [x, y, ...]): the points whose coordinates satisfy cond, e.g. implicit_region(x^2 + y^2 <= 1, [x, y])"),
    ("parametric_region", "parametric_region([f1, f2, ...], [[u, a, b], ...]): the points (f1, f2, ...) for the parameters in their ranges"),
    ("simplex", "simplex([p1, ..., pk]): the convex hull of k affinely independent points; simplex(n): the standard simplex of n dimensions"),
    ("tetrahedron", "tetrahedron([p1, p2, p3, p4]): the solid tetrahedron with these corners; tetrahedron(): the regular one with edge 1, centred at the origin"),
    ("empty_region", "empty_region(n): the empty region of n dimensions (the result of an empty intersection)"),
    ("region_member", "region_member(reg, p): whether the point p lies in the region (true or false; a condition for a symbolic point); region_member(reg, [p1, p2, ...]) for several"),
    ("region_measure", "region_measure(reg): the measure of the region in its own dimension: length of a curve, area of a surface, volume of a solid (0 for points)"),
    ("area", "area(reg): the area of a two-dimensional region"),
    ("volume", "volume(reg): the volume of a three-dimensional region"),
    ("arc_length", "arc_length(reg): the length of a one-dimensional region"),
    ("region_centroid", "region_centroid(reg): the centroid (centre of mass of uniform density) of the region"),
    ("region_bounds", "region_bounds(reg): the ranges [[xmin, xmax], [ymin, ymax], ...] of the region's coordinates (numerical for implicit and parametric regions)"),
    ("region_dimension", "region_dimension(reg): the region's own dimension (0 points, 1 curves, 2 surfaces, 3 solids)"),
    ("region_embedding_dimension", "region_embedding_dimension(reg): the number of coordinates of the region's points"),
    ("is_bounded_region", "is_bounded_region(reg): whether the region lies inside a ball of finite radius"),
    ("region_distance", "region_distance(reg, p): the distance from the point p to the region (0 inside)"),
    ("signed_region_distance", "signed_region_distance(reg, p): the distance from p to the region's boundary, negative inside a solid region"),
    ("region_nearest", "region_nearest(reg, p): the point of the region nearest to p"),
    ("region_union", "region_union(r1, r2, ...): the union of plane polygons (rectangles, triangles, disks as fine polygons) as a polygon"),
    ("region_intersection", "region_intersection(r1, r2, ...): the intersection of plane polygons as a polygon (empty_region(2) when empty)"),
    ("region_difference", "region_difference(r1, r2): the part of the plane polygon r1 outside r2, as a polygon (with holes where r2 lies inside r1)"),
    ("region_integrate", "region_integrate(f, [x, y, ...], reg): the integral of f over the region, as iterated integrals of its parametrisation (also region_integrate(f, element([x, y], reg)))"),
    ("pyramid", "pyramid([p1, p2, p3, p4, p5]): the solid pyramid with base p1 p2 p3 p4 and apex p5 (a region and a graphics primitive)"),
    ("prism", "prism([p1, ..., p6]): the solid triangular prism with ends p1 p2 p3 and p4 p5 p6"),
    ("hexahedron", "hexahedron([p1, ..., p8]): the solid with bottom face p1 p2 p3 p4 and top face p5 p6 p7 p8"),
    ("parallelepiped", "parallelepiped(p, [v1, v2, ...]): the points p + t1 v1 + t2 v2 + ... with each t in [0, 1]"),
    ("parallelogram", "parallelogram(p, [v1, v2]): the parallelogram with corners p, p + v1, p + v1 + v2, p + v2; parallelogram() the one at the origin with v1 = [1, 0], v2 = [1, 1]"),
    ("torus", "torus(c, [r_inner, r_outer]): the surface of a torus about the z axis through c (torus() has radii 1/2 and 1)"),
    ("filled_torus", "filled_torus(c, [r_inner, r_outer]): the solid torus"),
    ("infinite_line", "infinite_line([p1, p2]) or infinite_line(p, v): the line through two points, or through p in the direction v"),
    ("half_line", "half_line([p1, p2]) or half_line(p, v): the ray from p1 through p2, or from p in the direction v"),
    ("infinite_plane", "infinite_plane([p1, p2, p3]) or infinite_plane(p, [v1, v2]): the plane through three points, or through p spanned by v1 and v2"),
    ("half_plane", "half_plane([p1, p2], w) or half_plane(p, v, w): the half-plane bounded by the line through p1 and p2 (or through p along v), on the side of w"),
    ("conic_hull_region", "conic_hull_region([p, q1, ..., qk]): the points p + sum t_i (q_i - p) with t_i >= 0; conic_hull_region([p], [v1, ...]) a cone of directions"),
    ("mesh_region", "mesh_region(points, cells): a region made of cells (point, line, polygon, triangle, tetrahedron, hexahedron, ...) over indexed points; evaluates to its canonical form"),
    ("boundary_mesh_region", "boundary_mesh_region(points, cells): the solid region bounded by the cells (points in 1D, lines in 2D, polygons in 3D)"),
    ("mesh_cells", "mesh_cells(m, d): the cells of dimension d of a mesh region as index primitives (point(1), line([1, 2]), ...); mesh_cells(m, all), mesh_cells(m, [d, i])"),
    ("mesh_primitives", "mesh_primitives(m, d): the cells of dimension d of a mesh region as primitives with coordinates"),
    ("mesh_cell_count", "mesh_cell_count(m): the number of cells of each dimension of a mesh region; mesh_cell_count(m, d) of dimension d"),
    ("mesh_coordinates", "mesh_coordinates(m): the coordinates of a mesh region's vertices"),
    ("is_mesh_region", "is_mesh_region(m): whether m is a valid mesh region"),
    ("is_boundary_mesh_region", "is_boundary_mesh_region(m): whether m is a valid boundary mesh region"),
    ("dimensional_mesh_components", "dimensional_mesh_components(m): the cells of each dimension of a mesh region as mesh regions, lowest dimension first"),
    ("connected_mesh_components", "connected_mesh_components(m): the connected pieces of a mesh region as mesh regions"),
    ("sss_triangle", "sss_triangle(a, b, c): the triangle with sides a, b, c, placed with corners (0, 0) and (c, 0)"),
    ("sas_triangle", "sas_triangle(a, gamma, b): the triangle with sides a and b and the angle gamma between them"),
    ("asa_triangle", "asa_triangle(alpha, c, beta): the triangle with the side c between the angles alpha and beta"),
    ("aas_triangle", "aas_triangle(alpha, beta, a): the triangle with angles alpha and beta and the side a opposite alpha"),
    ("perimeter", "perimeter(reg): the length of the boundary of a plane region"),
    ("circumsphere", "circumsphere([p1, ..., pk]): the sphere (in the plane a circle) through the points, centred in their affine hull"),
    ("region_moment", "region_moment(reg, [i, j, ...]): the integral of x^i y^j ... over the region"),
    ("is_constant_region", "is_constant_region(reg): whether the region is given by numbers only, without free parameters"),
    ("is_simple_polygon", "is_simple_polygon(p): whether a plane polygon does not cross itself"),
    ("is_convex_polygon", "is_convex_polygon(p): whether a plane polygon is convex"),
    ("canonicalize_polygon", "canonicalize_polygon(p): the polygon with its distinct corners sorted and the boundary as indices, anticlockwise"),
    ("outer_polygon", "outer_polygon(p): the outer boundary of a polygon (its holes filled), indexed"),
    ("inner_polygon", "inner_polygon(p): the holes of a polygon as a polygon (empty_region(2) when there are none)"),
    ("bounding_region", "bounding_region(points, form): a region holding the points: \"MinRectangle\" (default; also \"MinCuboid\", \"MinInterval\"), \"MinDisk\" or \"MinBall\" (the smallest ball), \"MinConvexPolygon\" (the convex hull)"),
    ("winding_count", "winding_count(curve, p): how many times a closed curve (points, a line or a polygon) turns about the point p, anticlockwise positive"),
    ("crossing_count", "crossing_count(curve, p): the least number of crossings of a ray from p with a closed curve (plus 1 when p is on it)"),
    ("region_equal", "region_equal(r1, r2, ...): whether the regions are the same set of points (regions of the line and plane polygons)"),
    ("region_within", "region_within(r1, r2): whether r2 lies inside r1"),
    ("region_disjoint", "region_disjoint(r1, r2, ...): whether no two of the regions have a point in common"),
    ("winding_polygon", "winding_polygon(curve): the region a closed curve (points, a line, a polygon) encloses by the non-zero winding rule, as a polygon"),
    ("crossing_polygon", "crossing_polygon(curve): the region a closed curve encloses by the even-odd rule, as a polygon"),
    ("polygon_decomposition", "polygon_decomposition(reg): a polygon without holes (or a rectangle, triangle) as a list of triangles polygon([p1, p2, p3], [1, 2, 3])"),
    ("boolean_region", "boolean_region(f, [r1, r2, ...]): the plane region of the points whose membership in r1, r2, ... satisfies the Boolean function f (and, or, xor, or a function of true/false)"),
    ("transformed_region", "transformed_region(reg, tf): the region reg under the transformation_function tf (points, lines, polygons, triangles, rectangles, cuboids, disks under similarities)"),
    ("discretize_region", "discretize_region(reg): a triangle mesh_region of a plane region (disks as fine polygons), a line mesh of lines"),
    ("highlight_mesh", "highlight_mesh(m, style(d, directives...), ...): the mesh region m shown with its cells of dimension d drawn with the directives (style(d) or d alone: red)"),
    ("capsule_shape", "capsule_shape([p1, p2], r): the points within r of the segment p1 p2 (cylinder with hemispherical ends); capsule_shape() has [[-1, 0, 0], [1, 0, 0]] and radius 1; its volume is known"),
    ("polygon_coordinates","polygon_coordinates(p): the distinct corners of a polygon, sorted"),
];

/// Register every native with `k`.
pub fn install(k: &mut mc_eval::Kernel, h: &mc_core::Heap) {
    natives::install(k, h);
}

/// A condition on coordinates as a real function that is at least 0 exactly where the condition holds (on the
/// boundary 0): `a <= b` gives `b - a`, `and` the least of its parts, `or` the greatest, `not` the negative, an
/// equation `a == b` gives `-abs(a - b)`. `None` for conditions of other forms.
pub fn margin<'h>(h: &'h mc_core::Heap, t: mc_core::T<'h>) -> Option<mc_core::T<'h>> {
    let e = ex::E { h };
    let (name, args) = region::call_parts(h, t)?;
    let chain = |up: bool| -> Option<mc_core::T<'h>> {
        if args.len() < 2 {
            return None;
        }
        let parts: Vec<mc_core::T<'h>> = args.windows(2).map(|w| if up { e.sub(w[1], w[0]) } else { e.sub(w[0], w[1]) }).collect();
        Some(if parts.len() == 1 { parts[0] } else { e.f("min", &parts) })
    };
    match name.as_str() {
        "less" | "less_equal" => chain(true),
        "greater" | "greater_equal" => chain(false),
        "equal" if args.len() == 2 => Some(e.neg(e.f("abs", &[e.sub(args[0], args[1])]))),
        "and" | "or" => {
            let parts: Vec<mc_core::T<'h>> = args.iter().map(|&a| margin(h, a)).collect::<Option<_>>()?;
            Some(e.f(if name == "and" { "min" } else { "max" }, &parts))
        }
        "not" if args.len() == 1 => Some(e.neg(margin(h, args[0])?)),
        _ => None,
    }
}
