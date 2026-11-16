//! Implicit regions recognised as disks, circles, balls, spheres and axis-aligned ellipses (measures, centroids,
//! perimeters), the perimeter of the intersection of two disks, circles under affine maps, regions under affine
//! functions that drop a dimension, and meshes of implicit regions (`discretize_region`). Independent checks: the
//! exact values against numerical integrals of the arc length element at 30 digits, the polygon of a projection
//! against the images of points of the solid, and the vertices of meshes against the defining equations. Each test
//! has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

#[test]
fn quadric_implicit_regions() {
    timed("implicit quadrics", 120, || {
        let mut e = CoreEngine::new();
        for (src, want) in [
            ("arc_length(implicit_region(x^2 + y^2 == 1, [x, y]))", "2*pi"),
            ("arc_length(implicit_region(x^2 + 2 * y^2 == 1, [x, y]))", "4*elliptic_e(1/2)"),
            ("area(implicit_region(x^2 + y^2 + z^2 == 1, [x, y, z]))", "4*pi"),
            ("volume(implicit_region(x^2 + y^2 + z^2 <= 1, [x, y, z]))", "(4/3)*pi"),
            ("perimeter(implicit_region(x^2 + y^2 <= 1, [x, y]))", "2*pi"),
            ("region_centroid(implicit_region((x - 1)^2 + 4*(y + 2)^2 <= 3, [x, y]))", "[1, -2]"),
            ("area(implicit_region((x - 1)^2 + 4*(y + 2)^2 <= 3, [x, y]))", "(3/2)*pi"),
            ("volume(implicit_region(2 - x^2 - y^2 - z^2 + 2*z >= 0, [x, y, z]))", "4*3^(1/2)*pi"),
        ] {
            assert_eq!(ev(&mut e, src), ev(&mut e, want), "{src}");
        }
        // the ellipse's length against the integral of its speed at 30 digits
        holds(&mut e, "abs(n(n_integrate(sqrt(sin(t)^2 + cos(t)^2/2), [t, 0, 2*pi], working_precision: 30) - 4*elliptic_e(1/2), 30)) < 10^-25");
        // the outside of a sphere, a hyperboloid and a cross term are not quadrics of this kind
        holds(&mut e, "head(volume(implicit_region(x^2 + y^2 + z^2 >= 1, [x, y, z]))) === volume");
        holds(&mut e, "head(arc_length(implicit_region(x^2 - y^2 == 1, [x, y]))) === arc_length");
        holds(&mut e, "head(arc_length(implicit_region(x^2 + x*y + y^2 == 1, [x, y]))) === arc_length");
    });
}

#[test]
fn lens_perimeter_and_mapped_circles() {
    timed("lens and mapped circles", 120, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "perimeter(region_intersection(disk([0, 0], 1), disk([0, 1], 1)))"), "(4/3)*pi");
        // the lens from its corners: the circles' crossing points by solve, each arc the angle they subtend at its
        // centre (both arcs are the minor ones: each centre lies outside the other disk's arc)
        holds(&mut e, "{ let s = solve(x^2 + y^2 == 1 and x^2 + (y - 1)^2 == 1, [x, y]); let p = rewrite([x, y], s[1]); let q = rewrite([x, y], s[2]); full_simplify(acos(dot(p, q)) + acos(dot(p - [0, 1], q - [0, 1])) - 4*pi/3) == 0 }");
        assert_eq!(ev(&mut e, "perimeter(region_intersection(disk([0, 0], 3), disk([1, 0], 1)))"), "2*pi");
        assert_eq!(ev(&mut e, "perimeter(region_intersection(disk([0, 0], 1), disk([5, 0], 1)))"), "0");
        // a circle under a linear map: an ellipse with the singular values as semi-axes
        assert_eq!(ev(&mut e, "arc_length(transformed_region(circle(), transformation_function([[3/2, 1/2, 0], [1/2, 3/2, 0], [0, 0, 1]])))"), "8*elliptic_e(3/4)");
        holds(&mut e, "abs(n(n_integrate(sqrt((-3/2*sin(t) + 1/2*cos(t))^2 + (-1/2*sin(t) + 3/2*cos(t))^2), [t, 0, 2*pi], working_precision: 30) - 8*elliptic_e(3/4), 30)) < 10^-25");
        holds(&mut e, "abs(arc_length(transformed_region(circle(), transformation_function([[1.5, 0.5, 0], [0.5, 1.5, 0], [0, 0, 1]]))) - 9.688448220547675) < 1e-12");
        assert_eq!(ev(&mut e, "area(transformed_region(disk([1, 2], 2), transformation_function([[1, 2, 5], [0, 3, 1], [0, 0, 1]])))"), "12*pi");
    });
}

#[test]
fn projections_of_solids() {
    timed("projections", 60, || {
        let mut e = CoreEngine::new();
        ev(&mut e, "r = transformed_region(cuboid([0, 0, 0], [1, 1, 1]), &([indexed(&1, 1) + indexed(&1, 2), indexed(&1, 1) + indexed(&1, 3)]));");
        assert_eq!(ev(&mut e, "r"), "polygon([[0, 0], [1, 0], [2, 1], [2, 2], [1, 2], [0, 1]])");
        assert_eq!(ev(&mut e, "[region_dimension(r), region_embedding_dimension(r), region_centroid(r), area(r)]"), "[2, 2, [1, 1], 3]");
        assert_eq!(ev(&mut e, "[signed_region_distance(r, [1, 1]), signed_region_distance(r, [0, 2])]"), "[(-1/2)*2^(1/2), (1/2)*2^(1/2)]");
        // the images of points of the cube lie in the polygon
        holds(&mut e, "every(tuples(range(0, 1, 1/4), 3), p => region_member(r, [p[1] + p[2], p[1] + p[3]]))");
        // an affine map of the same dimension is a transformation function
        assert_eq!(ev(&mut e, "transformed_region(rectangle(), p => [p[1] + 1, 2*p[2]])"), "polygon([[1, 0], [2, 0], [2, 2], [1, 2]])");
        // a map that is not affine stays as it is
        holds(&mut e, "head(transformed_region(disk([1, 1], 4), p => [p[1]*p[2], p[1] + p[2]])) === transformed_region");
    });
}

#[test]
fn joined_curves() {
    timed("joined curves", 120, || {
        let mut e = CoreEngine::new();
        // a parabola arc and two segments back to its start (the join from (1, 0) to (0, -2/3) is part of the curve)
        ev(&mut e, "c = joined_curve([bezier_curve([[-1, 0], [0, 1], [1, 0]]), line([[0, -2/3], [-1, 0]])]);");
        // the length against the integral of the speed at 30 digits, piece by piece
        holds(&mut e, "abs(n(arc_length(c) - (n_integrate(sqrt(4 + (2 - 4*t)^2), [t, 0, 1], working_precision: 30) + 2*sqrt(13)/3), 30)) < 10^-25");
        holds(&mut e, "region_measure(c) === arc_length(c)");
        // the centroid: on the axis of symmetry; its height against the moments by numerical integration
        holds(&mut e, "{ let g = region_centroid(c); g[1] === 0 and abs(n(g[2]*arc_length(c) - (n_integrate((2*t - 2*t^2)*sqrt(4 + (2 - 4*t)^2), [t, 0, 1], working_precision: 30) - 2*sqrt(13)/3*(1/3)), 30)) < 10^-25 }");
        // pieces of other kinds stay as they are
        holds(&mut e, "head(arc_length(joined_curve([bezier_curve([[0, 0], [1, 1], [2, 0], [3, 1], [4, 0]])]))) === arc_length");
    });
}

#[test]
fn meshes_of_implicit_regions() {
    timed("implicit meshes", 120, || {
        let mut e = CoreEngine::new();
        // a disk: triangles whose area approaches pi
        ev(&mut e, "m = discretize_region(implicit_region(x^2 + y^2 <= 1, [x, y]));");
        holds(&mut e, "head(m) === mesh_region and abs(area(m) - n(pi)) < 2e-3");
        // a circle: segments with every vertex on it
        ev(&mut e, "c = discretize_region(implicit_region(x^2 + y^2 == 1, [x, y]));");
        holds(&mut e, "abs(arc_length(c) - n(2*pi)) < 1e-3 and max(map(p => abs(p[1]^2 + p[2]^2 - 1), mesh_coordinates(c))) < 1e-9");
        // a sphere: triangles with every vertex on it, area near 4 pi
        ev(&mut e, "s = discretize_region(implicit_region(x^2 + y^2 + z^2 == 1, [x, y, z]));");
        holds(&mut e, "abs(area(s) - n(4*pi)) < 0.05 and max(map(p => abs(norm(p) - 1), mesh_coordinates(s))) < 1e-9");
        // a curve of space (a cone cut by a plane, an ellipse) in a box: segments near both surfaces
        ev(&mut e, "k = discretize_region(implicit_region(x^2 + y^2 == z^2 and x + y == 2*z - 1, [x, y, z]), [[-3, 3], [-3, 3], [-3, 3]]);");
        holds(&mut e, "head(k) === mesh_region and max(map(p => abs(p[1]^2 + p[2]^2 - p[3]^2) + abs(p[1] + p[2] - 2*p[3] + 1), mesh_coordinates(k))) < 0.05");
        // with inequalities given approximately: a filled region
        holds(&mut e, "head(discretize_region(implicit_region(x^4 - x*y + y^4 <= 1., [x, y]))) === mesh_region");
        // a condition mixing equations and inequalities is not meshed
        holds(&mut e, "head(discretize_region(implicit_region(x^2 + y^2 <= 1 or x == y, [x, y]))) === discretize_region");
    });
}
