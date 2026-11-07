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
pub mod ex;
pub mod metric;
pub mod props;
pub mod region;
mod natives;

pub use region::{read, Radius, Region, Ring};

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("region", "region(r): the region r shown as a region (it stays as it is; the region functions read through it)"),
    ("implicit_region", "implicit_region(cond, [x, y, ...]): the points whose coordinates satisfy cond, e.g. implicit_region(x^2 + y^2 <= 1, [x, y])"),
    ("parametric_region", "parametric_region([f1, f2, ...], [[u, a, b], ...]): the points (f1, f2, ...) for the parameters in their ranges"),
    ("simplex", "simplex([p1, ..., pk]): the convex hull of k affinely independent points; simplex(n): the standard simplex of n dimensions"),
    ("tetrahedron", "tetrahedron([p1, p2, p3, p4]): the solid tetrahedron with these corners; tetrahedron(): the standard one"),
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
