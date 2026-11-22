//! Filled and joined curves as regions, ellipsoids given by a matrix, transformed disks, polygons with holes by
//! index, canonical polygons of several parts or in space, and enclosed polygons of curves in space.
//!
//! The values are checked against computations that do not use the code under test: areas and centroids of the
//! filled curve by integrating between its two boundary functions, distances by minimising the squared distance
//! along the curve's parametrisation, the ellipse's measures from its semi-axes.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))
}

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::new();
    for (src, want) in cases {
        assert_eq!(eval(&mut e, src), *want, "{src}");
    }
}

fn number(e: &mut CoreEngine, src: &str) -> f64 {
    let s = eval(e, src);
    s.parse::<f64>().unwrap_or_else(|_| panic!("{src} gave {s}"))
}

#[test]
fn filled_curve_area_centroid_membership() {
    // the quadratic B(t) = (2t, 2t - 3t^2) closed by the segment back to the origin: the region between
    // y = x - 3x^2/4 and y = -x/2 for 0 <= x <= 2, whose area is the integral of 3x/2 - 3x^2/4 = 1 and whose
    // centroid is (int x (3x/2 - 3x^2/4) dx, int ((x - 3x^2/4)^2 - x^2/4)/2 dx) = (1, -1/5)
    let r = "filled_curve([b_spline_curve([[0, 0], [1, 1], [2, -1]])])";
    check(&[
        (&format!("region_measure({r})"), "1"),
        (&format!("area({r})"), "1"),
        (&format!("arc_length({r})"), "undefined"),
        (&format!("region_centroid({r})"), "[1, -1/5]"),
        // on the boundary (the start), inside, outside, just above the curve at x = 1 (y = 1/4)
        (&format!("region_member({r}, [[0, 0], [1, 0], [1, 2], [1, 0.26], [1, 0.24]])"), "[true, true, false, false, true]"),
    ]);
    let mut e = CoreEngine::new();
    // the least of |B(t) - (1, 1)| by Newton's method on the derivative of the square (30 digits): 0.70740445713054...
    let d = number(&mut e, &format!("region_distance({r}, [1, 1])"));
    assert!((d - 0.707_404_457_130_541_2).abs() < 1e-12, "{d}");
    assert_eq!(eval(&mut e, &format!("region_distance({r}, [0, 0])")), "0.0");
    let s = number(&mut e, &format!("signed_region_distance({r}, [1, -0.2])"));
    // inside: minus the distance to the nearer of the segment y = -x/2 (0.3/sqrt(5/4)) and the curve
    assert!((s + 0.3 / 1.25f64.sqrt()).abs() < 1e-12, "{s}");
}

#[test]
fn joined_curve_membership_and_distance() {
    // B(t) = (2t - 1, 2t(1 - t)), then the segments to (0, -2/3) and on to (-1, 0)
    let j = "joined_curve([bezier_curve([[-1, 0], [0, 1], [1, 0]]), line([[0, -(2 / 3)], [-1, 0]])])";
    let mut e = CoreEngine::new();
    assert_eq!(eval(&mut e, &format!("[region_member({j}, [1, 0]), region_member({j}, [1, 2]), region_member({j}, [0, 1/2])]")), "[true, false, true]");
    // from (0, 0) the nearest point is the top of the arc (0, 1/2); from (1, 1) the least |B(t) - (1, 1)| is
    // 0.78890341058402558... (Newton's method on the derivative of the square, 30 digits)
    let d0 = number(&mut e, &format!("region_distance({j}, [0, 0])"));
    assert!((d0 - 0.5).abs() < 1e-14, "{d0}");
    let d1 = number(&mut e, &format!("region_distance({j}, [1, 1])"));
    assert!((d1 - 0.788_903_410_584_025_6).abs() < 1e-12, "{d1}");
    // a joined curve has no inside: no signed distance
    assert!(eval(&mut e, &format!("signed_region_distance({j}, [0, 0])")).starts_with("signed_region_distance"));
}

#[test]
fn ellipsoid_of_a_matrix() {
    // m = [[4, sqrt 3], [sqrt 3, 1]] has det 1: the image of the unit disk under the shear (x, y) -> (x + sqrt 3 y, y)
    let el = "ellipsoid([0, 0], [[4, sqrt(3)], [sqrt(3), 1]])";
    check(&[
        (&format!("area({el})"), "pi"),
        (&format!("region_measure({el})"), "pi"),
        (&format!("region_centroid({el})"), "[0, 0]"),
        // m^-1 = [[1, -sqrt 3], [-sqrt 3, 4]]: (1, 0) gives 1 (on the boundary), (0, 1/2) gives 1, (0, 0.51) more
        (&format!("[region_member({el}, [1, 0]), region_member({el}, [0, 1/2]), region_member({el}, [0, 0.51]), region_member({el}, [3, 1])]"), "[true, true, false, false]"),
        // the shear maps the unit disk onto it: the moment of y^2 is that of the disk, pi/4
        (&format!("region_moment({el}, [0, 2])"), "(1/4)*pi"),
        // det m = 3: the volume is 4/3 pi sqrt 3 (the semi-axes are sqrt 3, 1, 1 along (1, 1, 0), (1, -1, 0), z)
        ("volume(ellipsoid([1, 2, 3], [[2, 1, 0], [1, 2, 0], [0, 0, 1]]))", "(4/3)*pi*3^(1/2)"),
        ("region_centroid(ellipsoid([1, 2, 3], [[2, 1, 0], [1, 2, 0], [0, 0, 1]]))", "[1, 2, 3]"),
        // a transformed disk under a map that is not a similarity is that ellipsoid
        ("transformed_region(disk([0, 0], 1), shearing_transform(pi / 3, [1, 0], [0, 1]))", "ellipsoid([0, 0], [[4, 3^(1/2)], [3^(1/2), 1]])"),
        ("transformed_region(disk(), scaling_transform([3, 2]))", "ellipsoid([0, 0], [[9, 0], [0, 4]])"),
        ("area(transformed_region(disk(), scaling_transform([3, 2])))", "6*pi"),
    ]);
    let mut e = CoreEngine::new();
    // the ellipse with semi-axes 3 and 2: perimeter 4 * 3 * E(1 - 4/9) = 15.8654395892906...
    let p = number(&mut e, "n(perimeter(ellipsoid([0, 0], [[9, 0], [0, 4]])))");
    assert!((p - 15.865_439_589_290_595).abs() < 1e-9, "{p}");
}

#[test]
fn nearest_point_of_an_annulus_sector() {
    // the sector of radii 1..2 about (1, 2) for angles 0..3 pi/2: (5, 6) lies at angle pi/4, beyond the outer arc,
    // so the nearest point is (1, 2) + 2 (1, 1)/sqrt 2; (1, 0) lies at angle -pi/2, the end of the wedge (3 pi/2), on
    // the edge's line at distance 2 from the centre, so it is that point itself; (3, 1) lies outside the wedge
    // (angle -0.46), nearest to the edge along angle 0 at (3, 2), distance 1
    let s = "annulus([1, 2], [1, 2], [0, 3 * (pi / 2)])";
    check(&[
        (&format!("region_nearest({s}, [5, 6])"), "[1 + 2^(1/2), 2 + 2^(1/2)]"),
        (&format!("region_distance({s}, [5, 6])"), "-2 + 4*2^(1/2)"),
        (&format!("region_nearest({s}, [1, 0])"), "[1, 0]"),
        (&format!("region_nearest({s}, [3, 1])"), "[3, 2]"),
        (&format!("region_distance({s}, [3, 1])"), "1"),
        // (3/2, 2) is in the hole (radius 1/2): the nearest point is on the inner arc
        (&format!("region_nearest({s}, [3/2, 2])"), "[2, 2]"),
        (&format!("region_nearest({s}, [7/4, 2])"), "[2, 2]"),
        (&format!("region_nearest({s}, [5/2, 2])"), "[5/2, 2]"),
    ]);
}

#[test]
fn polygons_by_index_with_holes() {
    let p = "polygon([[0, 0], [5, 0], [2.5, 4], [2, 1], [2, 2], [3, 2], [3, 1]], [1, 2, 3] -> [[4, 5, 6, 7]])";
    check(&[
        // the triangle of area 10 less the unit square
        (&format!("area({p})"), "9.0"),
        (&format!("region_member({p}, [[1, 1/2], [5/2, 3/2]])"), "[true, false]"),
        (&format!("polygon_coordinates(outer_polygon({p}))"), "[[0, 0], [2.5, 4], [5, 0]]"),
        (&format!("polygon_coordinates(inner_polygon({p}))"), "[[2, 1], [2, 2], [3, 1], [3, 2]]"),
    ]);
}

#[test]
fn canonical_polygons_of_parts_and_in_space() {
    let q = "polygon([[[1, 0], [0, sqrt(3)], [-1, 0]], [[2, 2], [3, 3]]])";
    check(&[
        (&format!("canonicalize_polygon({q}, all)"), "polygon([[-1, 0], [0, 3^(1/2)], [1, 0], [2, 2], [3, 3]], [[1, 3, 2], [4, 5]])"),
        (&format!("canonicalize_polygon({q}, full)"), "polygon([[-1, 0], [0, 3^(1/2)], [1, 0], [2, 2], [3, 3]], [1, 3, 2])"),
        (
            "canonicalize_polygon(polygon([[[0, 0, 0], [2, 0, 0], [1, 1, 2]], [[2, 0, 0], [2, 2, 0], [1, 1, 2]], [[2, 2, 0], [0, 2, 0], [1, 1, 2]], [[0, 2, 0], [0, 0, 0], [1, 1, 2]]]))",
            "polygon([[0, 0, 0], [2, 0, 0], [1, 1, 2], [2, 2, 0], [0, 2, 0]], [[1, 2, 3], [2, 4, 3], [4, 5, 3], [5, 1, 3]])",
        ),
    ]);
}

#[test]
fn enclosed_polygons_in_space() {
    // four points of the plane w = 3, x + z = 1 of four-space (a simple quadrilateral there), and a triangle of space
    let c = "[[1, 0, 0, 3], [1, 1, 1, 3], [2/3, 1/3, 2/3, 3], [0, 0, 1, 3]]";
    let want = "polygon([[1, 0, 0, 3], [1, 1, 1, 3], [2/3, 1/3, 2/3, 3], [0, 0, 1, 3]], [1, 2, 3, 4])";
    check(&[
        (&format!("winding_polygon({c})"), want),
        (&format!("crossing_polygon({c})"), want),
        (&format!("winding_polygon({c}, even_odd_rule)"), want),
        ("crossing_polygon([[1, 0, 0], [0, 1, 0], [0, 0, 1]])", "polygon([[1, 0, 0], [0, 1, 0], [0, 0, 1]], [1, 2, 3])"),
        // off a plane: unevaluated
        ("crossing_polygon([[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]])", "crossing_polygon([[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]])"),
        // a mesh region of a mesh region is that mesh region
        ("mesh_region(mesh_region([[0, 0], [1, 0], [0, 1]], [polygon([1, 2, 3])])) == mesh_region([[0, 0], [1, 0], [0, 1]], [polygon([1, 2, 3])])", "true"),
    ]);
}
