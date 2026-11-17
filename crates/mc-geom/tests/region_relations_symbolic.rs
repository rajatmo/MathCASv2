//! Relations of regions of the line given by a radius, and of regions with symbolic parameters (as conditions).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))
}

#[test]
fn ball_of_the_line_from_its_radius() {
    assert_eq!(ev("region_disjoint(ball(1), interval([2, 3]))"), "true");
    assert_eq!(ev("region_disjoint(ball(2), interval([2, 3]))"), "false");
    assert_eq!(ev("region_equal(ball(1), interval([-1, 1]))"), "true");
    assert_eq!(ev("region_within(interval([-2, 0]), ball(1))"), "false");
    assert_eq!(ev("region_within(interval([-2, 2]), ball(1))"), "true");
}

#[test]
fn symbolic_regions_give_conditions() {
    // the unit square at (x, y) misses the square with corners (+-1/sqrt 2, +-1/sqrt 2) iff it is far enough in
    // one coordinate; the condition is checked at points on each side
    let src = "c = region_disjoint(rectangle([x, y]), regular_polygon(4)); \
               [rewrite(c, [x -> 3, y -> 0]), rewrite(c, [x -> 0, y -> 0]), rewrite(c, [x -> -3, y -> 5]), rewrite(c, [x -> 1/2, y -> 1/2])]";
    assert_eq!(ev(src), "[true, false, true, false]");
    // the same relation, when the second region is the first shifted: equal exactly at the shift of zero
    assert_eq!(ev("region_equal(rectangle([0, 0], [a, b]), rectangle([0, 0], [1, 1]))"), "a == 1 and b == 1");
}

#[test]
fn symbolic_polygon_membership_is_not_guessed() {
    // an edge of symbolic length is not known to be an edge: the membership stays unevaluated
    let r = ev("region_member(polygon([[r, 0], [0, r], [-r, 0], [0, -r]]), [u, v])");
    assert!(r.starts_with("region_member("), "{r}");
}
