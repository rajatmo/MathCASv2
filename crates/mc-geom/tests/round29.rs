//! Round 29: winding and crossing polygons, polygon decomposition, Boolean and transformed regions,
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
        ("polygon_decomposition(rectangle())", "[polygon([[0, 1], [0, 0], [1, 0]], [1, 2, 3]), polygon([[1, 0], [1, 1], [0, 1]], [1, 2, 3])]"),
        ("length(polygon_decomposition(polygon([[0, 0], [2, 0], [2, 2], [1, 1], [0, 2]]), \"Triangle\"))", "3"),
        ("total(map(area, polygon_decomposition(polygon([[0, 0], [4, 0], [4, 1], [1, 1], [1, 3], [0, 3]]))))", "6"),
    ]);
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
