//! Intersections of round regions with exact data: the arc of a circle inside a disk, the volume of two
//! intersecting balls, and affine images of triangles.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))
}

#[test]
fn circle_inside_a_disk_is_an_arc() {
    // the unit circle about [0, 1] crosses the unit disk at angles 7 pi/6 and 11 pi/6 (cos alpha = 1/2 below the
    // centre)
    assert_eq!(ev("region_intersection(disk([0, 0], 1), circle([0, 1], 1))"), "circle([0, 1], 1, [(7/6)*pi, (11/6)*pi])");
    assert_eq!(ev("region_intersection(circle([0, 1], 1), disk([0, 0], 1))"), "circle([0, 1], 1, [(7/6)*pi, (11/6)*pi])");
    // its length is the third of the circle; every point of the arc lies in the disk
    assert_eq!(ev("arc_length(region_intersection(circle([1, 0], 1), disk()))"), "(2/3)*pi");
    // ends on the disk's circle, middle inside
    let at = |t: &str| format!("c = region_intersection(disk([0, 0], 1), circle([1, 1], 2^(1/2))); p = [1 + 2^(1/2)*cos({t}), 1 + 2^(1/2)*sin({t})]");
    assert_eq!(ev(&format!("{}; chop(n(p[1]^2 + p[2]^2 - 1))", at("c[3][1]"))), "0");
    assert_eq!(ev(&format!("{}; chop(n(p[1]^2 + p[2]^2 - 1))", at("c[3][2]"))), "0");
    assert_eq!(ev(&format!("{}; region_member(disk([0, 0], 1), n(p))", at("(c[3][1] + c[3][2])/2"))), "true");
    // a circle inside the disk is itself, one outside it is empty
    assert_eq!(ev("region_intersection(disk([0, 0], 3), circle([0, 1], 1))"), "circle([0, 1], 1)");
    assert_eq!(ev("region_intersection(disk([0, 0], 1), circle([5, 1], 1))"), "empty_region(2)");
    assert_eq!(ev("region_intersection(disk([0, 0], 1), circle([0, 0], 3))"), "empty_region(2)");
    // touching circles are not decided by machine numbers: left as they are
    assert_eq!(ev("region_intersection(disk([0, 0], 1), circle([2, 0], 1))"), "region_intersection(disk([0, 0], 1), circle([2, 0], 1))");
}

#[test]
fn volume_of_two_balls() {
    assert_eq!(ev("volume(region_intersection(ball([0, 0, 0], 1), ball([0, 0, 1], 1)))"), "(5/12)*pi");
    // one inside the other: the smaller ball; apart: nothing
    assert_eq!(ev("volume(region_intersection(ball([0, 0, 0], 2), ball([0, 0, 1], 1)))"), "(4/3)*pi");
    assert_eq!(ev("region_measure(region_intersection(ball([0, 0, 0], 1), ball([0, 0, 3], 1)))"), "0");
    // radii 2 and 1 at distance 2 meet in the plane z = 7/4: caps of heights 1/4 and 3/4, each pi h^2 (3 r - h) / 3
    let v: f64 = ev("n(volume(region_intersection(ball([0, 0, 0], 2), ball([0, 0, 2], 1))))").parse().unwrap();
    let h1: f64 = 0.25;
    let h2: f64 = 0.75;
    let caps = std::f64::consts::PI * (h1 * h1 * (3.0 * 2.0 - h1) + h2 * h2 * (3.0 * 1.0 - h2)) / 3.0;
    assert!((v - caps).abs() < 1e-12, "{v} {caps}");
}

#[test]
fn moments_of_a_mapped_disk() {
    // x = u + sqrt 3 v, y = v with det 1: the moment of y^2 is that of v^2 over the unit disk, of x^2 is
    // (1 + 3) pi/4
    let tf = "transformed_region(disk([0, 0], 1), transformation_function([[1, 3^(1/2), 0], [0, 1, 0], [0, 0, 1]]))";
    assert_eq!(ev(&format!("region_moment({tf}, [0, 2])")), "(1/4)*pi");
    assert_eq!(ev(&format!("region_moment({tf}, [2, 0])")), "pi");
    // a scaling by 3 and 2 with a shift: the area 6 pi and the mean of x at the shift
    let sc = "transformed_region(disk(), transformation_function([[3, 0, 1], [0, 2, 0], [0, 0, 1]]))";
    assert_eq!(ev(&format!("[region_moment({sc}, [0, 0]), region_moment({sc}, [1, 0])]")), "[6*pi, 6*pi]");
}

#[test]
fn affine_image_of_a_triangle_is_a_triangle() {
    assert_eq!(ev("transformed_region(triangle(), rotation_transform(pi / 3))"), "triangle([[0, 0], [1/2, (1/2)*3^(1/2)], [(-1/2)*3^(1/2), 1/2]])");
}
