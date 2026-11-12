//! Regions through the engine: exact measures, centroids, bounds, membership, distances, Boolean operations on
//! polygons and integrals over regions.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::new();
    for (src, want) in cases {
        assert_eq!(eval(&mut e, src), *want, "{src}");
    }
}

fn number(e: &mut CoreEngine, src: &str) -> f64 {
    let s = eval(e, src);
    s.parse().unwrap_or_else(|_| panic!("{src} -> {s}"))
}

#[test]
fn measures_are_exact() {
    check(&[
        ("region_measure(disk([0, 0], r))", "pi*r^2"),
        ("region_measure(ball([0, 0, 0], r))", "(4/3)*pi*r^3"),
        ("region_measure(circle())", "2*pi"),
        ("region_measure(sphere())", "4*pi"),
        ("region_measure(simplex(3))", "1/6"),
        ("region_measure(line([[0, 0], [1, 1], [1, 0]]))", "1 + 2^(1/2)"),
        ("region_measure(disk([0, 0], [2, 3]))", "6*pi"),
        ("region_measure(polygon([[0, 0], [4, 0], [4, 3], [0, 3]]))", "12"),
        ("region_measure(point([[1, 2], [3, 4]]))", "2"),
        ("area(triangle([[0, 0], [4, 0], [0, 3]]))", "6"),
        ("volume(cuboid([0, 0, 0], [1, 2, 3]))", "6"),
        ("arc_length(circle([0, 0], 2))", "4*pi"),
        ("area(line([[0, 0], [1, 1]]))", "0"),
        ("region_measure(polygon([[0, 0], [4, 0], [4, 4], [0, 4]] -> [[[1, 1], [2, 1], [2, 2], [1, 2]]]))", "15"),
        ("region_measure(parametric_region([cos(t), sin(t)], [[t, 0, pi]]))", "pi"),
        ("region_measure(interval([0, 1], [2, 5]))", "4"),
    ]);
}

#[test]
fn centroids_and_bounds() {
    check(&[
        ("region_centroid(disk([x, y], r))", "[x, y]"),
        ("region_centroid(rectangle([a, b], [c, d]))", "[(1/2)*(a + c), (1/2)*(b + d)]"),
        ("region_centroid(polygon([[0, 0], [3, -1], [1, 0], [3, 1]]))", "[4/3, 0]"),
        ("region_centroid(cuboid([1, 2, 3], [5, 5, 5]))", "[3, 7/2, 4]"),
        ("region_centroid(simplex([[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]]))", "[1/4, 1/4, 1/4]"),
        ("region_centroid(line([[0], [1]]))", "[1/2]"),
        ("region_centroid(polygon([[0, 0, 0], [5/3, 2/3, -(4/3)], [8/15, 8/15, -(4/15)], [1, 2, 0]]))", "[28/45, 28/45, -14/45]"),
        ("region_bounds(triangle([[0, 0], [1, 2], [2, 1]]))", "[[0, 2], [0, 2]]"),
        ("region_bounds(disk([1, 2], 3))", "[[-2, 4], [-1, 5]]"),
        ("region_dimension(implicit_region(x^2 + y^2 == 1, [x, y]))", "1"),
        ("[region_dimension(circle()), region_dimension(ball()), region_embedding_dimension(ball())]", "[1, 3, 3]"),
        ("is_bounded_region(disk())", "true"),
    ]);
    let mut e = CoreEngine::new();
    // numerical bounds of an implicit region: the ellipse x^2/4 + y^2 <= 1
    let b = eval(&mut e, "region_bounds(implicit_region(x^2 + 4*y^2 <= 4, [x, y]))");
    let v: Vec<f64> = b.trim_matches(|c| c == '[' || c == ']').split(", ").map(|s| s.trim_matches(|c| c == '[' || c == ']').parse().unwrap()).collect();
    for (got, want) in v.iter().zip([-2.0, 2.0, -1.0, 1.0]) {
        assert!((got - want).abs() < 1e-6, "{b}");
    }
    let a = number(&mut e, "n(region_measure(implicit_region(x^2 + y^2 <= 1, [x, y])))");
    assert!((a - std::f64::consts::PI).abs() < 1e-4, "{a}");
}

#[test]
fn membership_and_distances() {
    check(&[
        ("region_member(disk(), [1/2, 1/2])", "true"),
        ("region_member(disk(), [x, y])", "element(x | y, reals) and x^2 + y^2 <= 1"),
        ("region_member(rectangle(), [2, 0])", "false"),
        ("region_member(rectangle(), [1/2, 1])", "true"),
        ("region_member(polygon([[0, 0], [2, 0], [2, 2], [0, 2]]), [[1, 1], [3, 3], [2, 1]])", "[true, false, true]"),
        ("region_member(implicit_region(x^2 + y^2 <= 1, [x, y]), [1, 1])", "false"),
        ("region_member(line([[0, 0], [2, 2]]), [1, 1])", "true"),
        ("region_distance(disk(), [3, 3])", "-1 + 3*2^(1/2)"),
        ("region_distance(point([[1, 1], [1, 2]]), [3, 4])", "2*2^(1/2)"),
        ("region_distance(line([[0, 0], [1, 1]]), [3, 4])", "13^(1/2)"),
        ("region_distance(line([[0, 0, 0], [1, 1, 1], [0, 0, 1]]), [1, 2, 3])", "5^(1/2)"),
        ("region_distance(rectangle([0, 0], [1, 2]), [3, 4])", "2*2^(1/2)"),
        ("region_distance(cuboid([0, 0, 0], [1, 2, 3]), [4, 5, 6])", "3*3^(1/2)"),
        ("region_distance(simplex(2), [1, 1])", "(1/2)*2^(1/2)"),
        ("region_distance(disk(), [x, y])", "max(0, -1 + (x^2 + y^2)^(1/2))"),
        ("signed_region_distance(rectangle([0, 0], [2, 2]), [1/2, 1])", "-1/2"),
        ("signed_region_distance(triangle(), [1/4, 1/4])", "-1/4"),
        ("region_nearest(disk(), [3, 4])", "[3/5, 4/5]"),
        ("region_nearest(line([[0, 0], [1, 1]]), [3, 4])", "[1, 1]"),
        ("region_nearest(polygon([[0, 0], [2, 0], [2, 2], [0, 2]]), [5, 1])", "[2, 1]"),
    ]);
}

#[test]
fn polygon_boolean_operations() {
    check(&[
        ("region_union(rectangle([0, 0], [2, 2]), rectangle([1, 1], [3, 3]))", "polygon([[0, 0], [2, 0], [2, 1], [3, 1], [3, 3], [1, 3], [1, 2], [0, 2]])"),
        ("region_intersection(rectangle([0, 0], [2, 2]), triangle([[1, 1], [3, 1], [1, 3]]))", "polygon([[1, 1], [2, 1], [2, 2], [1, 2]])"),
        ("area(region_difference(rectangle([0, 0], [4, 4]), rectangle([1, 1], [2, 2])))", "15"),
        ("region_intersection(rectangle([0, 0], [1, 1]), rectangle([2, 2], [3, 3]))", "empty_region(2)"),
        ("area(region_intersection(rectangle([0, 0], [1, 1]), triangle([[0, 0], [3, 0], [0, 3]])))", "1"),
        ("area(region_union(triangle([[0, 0], [2, 0], [0, 2]]), triangle([[0, 0], [2, 0], [2, 2]])))", "3"),
    ]);
}

#[test]
fn integrals_over_regions() {
    check(&[
        ("region_integrate(x^2 + y^2, [x, y], disk())", "(1/2)*pi"),
        ("region_integrate(x*y, [x, y], triangle())", "1/24"),
        ("region_integrate(1, [x, y, z], ball([0, 0, 0], 2))", "(32/3)*pi"),
        ("region_integrate(x, [x, y], polygon([[0, 0], [2, 0], [2, 2], [0, 2]]))", "4"),
        ("region_integrate(1, [x, y], circle([0, 0], 3))", "6*pi"),
        ("region_integrate(x^2, element([x], interval([0, 1])))", "1/3"),
        ("region_integrate(x + y, [x, y], rectangle([0, 0], [1, 2]))", "3"),
        ("region_integrate(f(x, y), [x, y], point([[1, 2], [3, 4]]))", "f(1, 2) + f(3, 4)"),
    ]);
}
