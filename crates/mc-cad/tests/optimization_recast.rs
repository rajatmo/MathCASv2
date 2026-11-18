//! Optimization problems recast before the decomposition: regions as constraints, objectives with cases, variables
//! the objective does not hold, a vector variable in a region; principal roots of any small index in the
//! decomposition's front end; non-smooth objectives (a norm) that the critical points alone would get wrong.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn regions_as_constraints() {
    let mut e = CoreEngine::new();
    assert_eq!(
        ev(&mut e, "minimize([1 + x^2 + y^2 + z^2, element([x, y, z], infinite_plane([0, 0, 2], [[1, 0, 0], [0, 1, 0]]))], [x, y, z])"),
        "[5, [x -> 0, y -> 0, z -> 2]]"
    );
    assert_eq!(ev(&mut e, "minimize([x + y, element([x, y], triangle([[0, 0], [1, 0], [0, 1]]))], [x, y])"), "[0, [x -> 0, y -> 0]]");
    assert_eq!(ev(&mut e, "max_value([x + y, element([x, y], polygon([[0, 0], [2, 0], [0, 1]]))], [x, y])"), "2");
}

#[test]
fn objectives_with_cases() {
    let mut e = CoreEngine::new();
    // inside the disk exp(sin(x)) with x in [-1, 1], outside 1
    assert_eq!(ev(&mut e, "minimize(exp(boole(x^2 + y^2 <= 1)*sin(x)), [x, y])"), "[exp(-sin(1)), [x -> -1, y -> 0]]");
    assert_eq!(ev(&mut e, "maximize(exp(boole(x^2 + y^2 <= 1)*sin(x)), [x, y])"), "[exp(sin(1)), [x -> 1, y -> 0]]");
    assert_eq!(ev(&mut e, "min_value(exp(boole(x^2 + y^2 <= 1)*sin(x)), [x, y])"), "exp(-sin(1))");
}

#[test]
fn variables_absent_from_the_objective() {
    let mut e = CoreEngine::new();
    // y only in the constraint: projected out (x in [-1, 1]), then a y for the point
    let r = ev(&mut e, "minimize([exp(sin(x)), x^2 + y^2 <= 1], [x, y])");
    assert_eq!(r, "[exp(-sin(1)), [x -> -1, y -> 0]]");
}

#[test]
fn vector_variable_in_a_region() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "minimize([norm(x), indexed(x, 1) + indexed(x, 2) <= 1], element(x, ball()))"), "[0, [x -> [0, 0, 0]]]");
    assert_eq!(
        ev(&mut e, "maximize(dot(x, [1, 2, 3]), element(x, ball()))"),
        "[14^(1/2), [x -> [(1/14)*14^(1/2), (1/7)*14^(1/2), (3/14)*14^(1/2)]]]"
    );
}

#[test]
fn two_periodic_parts_with_incommensurable_periods() {
    let mut e = CoreEngine::new();
    // dense in the torus: the supremum is the sum of the parts' maxima, not attained (sqrt(3) x = pi/2 + 2 pi k
    // and 3 x = pi + 2 pi m have no common solution)
    assert_eq!(ev(&mut e, "maximize(sin(3^(1/2)*x) - exp(cos(3*x) + 2), x)"), "[1 - e, [x -> indeterminate]]");
    assert_eq!(ev(&mut e, "minimize(sin(3^(1/2)*x) - exp(cos(3*x) + 2), x)"), "[-1 - exp(3), [x -> indeterminate]]");
    // attained at 0
    assert_eq!(ev(&mut e, "maximize(cos(2^(1/2)*x) + cos(x), x)"), "[2, [x -> 0]]");
    assert_eq!(ev(&mut e, "max_value(sin(2^(1/2)*x) + cos(x), x)"), "2");
}

#[test]
fn norms_have_their_minimum_where_no_gradient_vanishes() {
    let mut e = CoreEngine::new();
    // the norm is smallest at the origin, where its gradient is not defined
    assert_eq!(ev(&mut e, "minimize([(x^2 + y^2)^(1/2), x + y <= 1], [x, y])"), "[0, [x -> 0, y -> 0]]");
    assert_eq!(ev(&mut e, "min_value([(a^2 + b^2 + c^2)^(1/2), a + b <= 1 and a^2 + b^2 + c^2 <= 1], [a, b, c])"), "0");
    assert_eq!(ev(&mut e, "minimize([(x^2 + y^2)^(1/2), x + y >= 1], [x, y])"), "[(1/2)*2^(1/2), [x -> 1/2, y -> 1/2]]");
}

#[test]
fn principal_roots_in_the_decomposition() {
    let mut e = CoreEngine::new();
    // the principal cube root is real only for a nonnegative radicand
    assert_eq!(ev(&mut e, "reduce(x^(1/3) + y == 1 and y > 0 and x >= 0, [x, y])"), ev(&mut e, "reduce(x^(1/3) + y == 1 and y > 0, [x, y])"));
    ev(&mut e, "p = find_instance(-3*x + 4*y + (x + 2*y)^(1/2) >= 5 and x + y^(1/3) == 1, [x, y], reals)");
    let p = ev(&mut e, "p");
    assert!(p.starts_with("[[x -> "), "{p}");
    assert_eq!(ev(&mut e, "rewrite(-3*x + 4*y + (x + 2*y)^(1/2) >= 5 and x + y^(1/3) == 1, p[1])"), "true");
}
