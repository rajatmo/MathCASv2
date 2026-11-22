//! Winding and crossing polygons, polygon decomposition, Boolean and transformed regions,
//! discretised regions, highlighted meshes, capsules, the dimension of a degenerate line.
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

fn approx(src: &str, want: f64) {
    let mut e = CoreEngine::new();
    let got: f64 = eval(&mut e, src).parse().unwrap_or(f64::NAN);
    assert!((got - want).abs() < 1e-9, "{src}: {got} vs {want}");
}

#[test]
fn enclosed_polygons() {
    // a pentagram: the winding rule fills the pentagon in the middle, the even-odd rule does not
    let star = "[[0.35, 0.2], [0.9, 0.75], [0.1, 0.55], [0.9, 0.35], [0.42, 0.9]]";
    approx(&format!("area(winding_polygon({star}))"), 0.18160082878110226);
    approx(&format!("area(crossing_polygon({star}))"), 0.12595165756220444);
    check(&[
        ("crossing_polygon([[0, 0], [1, 0], [1, 1]])", "polygon([[0, 0], [1, 0], [1, 1]])"),
        ("area(winding_polygon([[0, 0], [1, 0], [1, 1]]))", "1/2"),
    ]);
}

#[test]
fn decomposition() {
    check(&[
        // a convex polygon is its own convex piece; triangles on request
        ("polygon_decomposition(rectangle())", "[polygon([[0, 0], [1, 0], [1, 1], [0, 1]], [1, 2, 3, 4])]"),
        ("polygon_decomposition(rectangle(), triangle)", "[polygon([[0, 1], [0, 0], [1, 0]], [1, 2, 3]), polygon([[1, 0], [1, 1], [0, 1]], [1, 2, 3])]"),
        // a clockwise convex ring comes out anticlockwise
        ("polygon_decomposition(polygon([[0, 0], [0, 2], [3, 0]]))", "[polygon([[3, 0], [0, 2], [0, 0]], [1, 2, 3])]"),
        // the corners of the pieces are evaluated (the parallelogram's corners are sums)
        ("polygon_decomposition(parallelogram(), triangle)", "[polygon([[1, 1], [0, 0], [1, 0]], [1, 2, 3]), polygon([[1, 0], [2, 1], [1, 1]], [1, 2, 3])]"),
        // a box given by bounds on each coordinate, strict or not
        ("polygon_decomposition(implicit_region((0 <= x and x <= 1) and (0 <= y and y <= 1), [x, y]))", "[polygon([[0, 0], [1, 0], [1, 1], [0, 1]], [1, 2, 3, 4])]"),
        ("polygon_decomposition(implicit_region(-1 < x and x < 2 and y >= 0 and 3 > y, [x, y]))", "[polygon([[-1, 0], [2, 0], [2, 3], [-1, 3]], [1, 2, 3, 4])]"),
        // not a box: unevaluated
        ("polygon_decomposition(implicit_region(x + y <= 1 and x >= 0 and y >= 0, [x, y]))", "polygon_decomposition(implicit_region(x + y <= 1 and x >= 0 and y >= 0, [x, y]))"),
        ("length(polygon_decomposition(polygon([[0, 0], [2, 0], [2, 2], [1, 1], [0, 2]])))", "3"),
        ("length(polygon_decomposition(polygon([[0, 0], [2, 0], [2, 2], [1, 1], [0, 2]]), triangle))", "3"),
        ("total(map(area, polygon_decomposition(polygon([[0, 0], [4, 0], [4, 1], [1, 1], [1, 3], [0, 3]]))))", "6"),
    ]);
}

#[test]
fn mesh_cells_by_an_index_pattern() {
    // the vertices of odd index: 1, 3 and 5
    let m = "mesh_region([[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 2.0, 0.0], [0.0, 2.0, 0.0], [1.0, 1.0, 2.0]], [tetrahedron([[1, 2, 3, 5], [1, 3, 4, 5]])])";
    check(&[
        (&format!("mesh_primitives({m}, [0, (t_ when is_odd(t))])"), "[point([0.0, 0.0, 0.0]), point([2.0, 2.0, 0.0]), point([1.0, 1.0, 2.0])]"),
        (&format!("mesh_cells({m}, [3, (t_ when t > 1)])"), "[tetrahedron([1, 3, 4, 5])]"),
        (&format!("mesh_cells({m}, [0, (t_ when t > 9)])"), "[]"),
    ]);
}

#[test]
fn b_spline_surface_of_one_span() {
    // the bicubic patch of a 4 x 4 net: its area by an independent 20-digit adaptive quadrature of |S_u x S_v|
    // (tanh-sinh, derivatives by numerical differentiation) is 9.9560480867715569781; its x and y run over [1, 4]
    // (the coordinates are linear in u and v) and z over [0, 1] (the corner (u, v) = (1, 0) has z = 1, the corner
    // (0, 0) z = 0, and no control point lies outside 0 <= z <= 1)
    let net = "[[[1, 1, 0], [1, 2, 0], [1, 3, 1], [1, 4, 0]], [[2, 1, 0], [2, 2, 1], [2, 3, 1], [2, 4, 1]], [[3, 1, 0], [3, 2, 1], [3, 3, 1], [3, 4, 0]], [[4, 1, 1], [4, 2, 1], [4, 3, 1], [4, 4, 0]]]";
    approx(&format!("area(b_spline_surface({net}))"), 9.956_048_086_771_557);
    approx(&format!("region_measure(b_spline_surface({net}))"), 9.956_048_086_771_557);
    check(&[(&format!("region_bounds(b_spline_surface({net}))"), "[[1.0, 4.0], [1.0, 4.0], [0.0, 1.0]]")]);
    // a flat bilinear patch is a parallelogram: area |(3, 0, 0) x (1, 2, 0)| = 6
    approx("area(b_spline_surface([[[0, 0, 0], [1, 2, 0]], [[3, 0, 0], [4, 2, 0]]]))", 6.0);
}

#[test]
fn highlighted_mesh_of_the_line() {
    let m = "mesh_region([[0.0], [1.0], [2.0]], [line([[1, 2]]), point([[3]])])";
    let mut e = CoreEngine::new();
    for spec in ["style(0, red)", "1"] {
        let s = eval(&mut e, &format!("head(highlight_mesh({m}, {spec}))"));
        assert_eq!(s, "graphics", "{spec}");
    }
}

#[test]
fn boolean_and_transformed() {
    check(&[
        ("area(boolean_region(xor, [rectangle(), rectangle([1/2, 1/2])]))", "3/2"),
        ("area(boolean_region((a, b) => a and b, [rectangle(), rectangle([1/2, 1/2])]))", "1/4"),
        ("area(boolean_region((a, b) => not(b) and a, [rectangle(), rectangle([1/2, 1/2])]))", "3/4"),
        ("transformed_region(rectangle(), translation_transform([2, 3]))", "polygon([[2, 3], [3, 3], [3, 4], [2, 4]])"),
        ("area(transformed_region(triangle(), rotation_transform(pi/3)))", "1/2"),
        ("transformed_region(disk(), translation_transform([2, 3]))", "disk([2, 3], 1)"),
        ("region_dimension(line([[1, 2], [1, 2]]))", "0"),
        ("region_dimension(line([[1, 2], [1, 3]]))", "1"),
    ]);
}

#[test]
fn meshes_and_capsules() {
    check(&[
        ("is_mesh_region(discretize_region(rectangle()))", "true"),
        ("area(discretize_region(rectangle()))", "1.0"),
        ("capsule_shape()", "capsule_shape([[-1, 0, 0], [1, 0, 0]], 1)"),
        ("volume(capsule_shape())", "(10/3)*pi"),
        ("volume(capsule_shape([[0, 0, 0], [0, 0, 3]], 2))", "(68/3)*pi"),
    ]);
    let mut e = CoreEngine::new();
    let got = eval(&mut e, "highlight_mesh(mesh_region([[0, 0], [1, 0], [1, 1]], triangle([1, 2, 3])), style(0, red))");
    assert!(got.starts_with("image("), "{got}");
}
