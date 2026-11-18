//! Primitives called without arguments are written out at the placement their docs pages state; size directives
//! written as rules among the primitives; the unit ball of two dimensions in a picture.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))
}

#[test]
fn defaults_are_written_out() {
    assert_eq!(ev("[disk(), circle(), rectangle()]"), "[disk([0, 0]), circle([0, 0]), rectangle([0, 0])]");
    assert_eq!(ev("[sphere(), cuboid()]"), "[sphere([0, 0, 0]), cuboid([0, 0, 0])]");
    assert_eq!(ev("[cone(), cylinder()]"), "[cone([[0, 0, -1], [0, 0, 1]]), cylinder([[0, 0, -1], [0, 0, 1]])]");
    // the unit ball of space, the ring between the radii 1/2 and 1 (the size written out)
    assert_eq!(ev("[ball(), annulus()]"), "[ball([0, 0, 0]), annulus([0, 0], [1/2, 1])]");
    // given arguments stay as written; ball(n) is the unit ball of n dimensions
    assert_eq!(ev("[ball([1, 2, 3]), ball(2), annulus(3), annulus([1, 1])]"), "[ball([1, 2, 3]), ball(2), annulus(3), annulus([1, 1])]");
    // the same regions as before
    assert_eq!(ev("[region_measure(disk()), region_measure(sphere()), volume(cone()), volume(cylinder()), area(rectangle())]"), "[pi, 4*pi, (2/3)*pi, 2*pi, 1]");
    assert_eq!(ev("[volume(ball()), region_measure(annulus()), region_member(annulus(), [3/4, 0]), region_dimension(ball())]"), "[(4/3)*pi, (3/4)*pi, true, 3]");
    assert!(ev("graphics3d(ball())").starts_with("image(") && ev("graphics(annulus())").starts_with("image("));
}

#[test]
fn rule_directives_and_ball_of_the_plane_draw() {
    assert!(ev("graphics([point_size -> large, point([[0, 0.2], [1, 0.8]])], frame: true)").starts_with("image("));
    assert!(ev("graphics([thickness -> 0.02, line([[0, 0], [1, 1]])])").starts_with("image("));
    assert!(ev("graphics([edge_form([thick, dashed, blue]), brown, ball(2)])").starts_with("image("));
}
