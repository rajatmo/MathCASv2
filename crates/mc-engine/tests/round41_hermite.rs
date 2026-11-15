//! Round 41: interpolation with derivative data (piecewise Hermite in one variable, Hermite in both directions on a
//! grid) and the rectangle of least area around plane points. Interpolants are checked by reproducing polynomials of
//! their degree exactly; rectangles by their area against a hand count and by holding every point.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// |a - b| < tol as the engine prints it
fn close(e: &mut CoreEngine, a: &str, b: &str, tol: &str) -> String {
    ev(e, &format!("abs(({a}) - ({b})) < {tol}"))
}

#[test]
fn hermite_in_one_variable() {
    let mut e = CoreEngine::new();
    ev(&mut e, "f = interpolation(table([[x], sin(8*x), 8*cos(8*x)], [x, 0, 3, 0.5]))");
    assert_eq!(ev(&mut e, "head(f)"), "interpolated");
    // the data at the nodes: values and first derivatives
    assert_eq!(close(&mut e, "f(1.5)", "sin(12.0)", "1e-14"), "true");
    assert_eq!(close(&mut e, "derivative(1)(f)(2.5)", "8*cos(20.0)", "1e-12"), "true");
    // a cubic with its derivative is reproduced between the nodes (degree 3 = 2m - 1 with m = 2)
    ev(&mut e, "p = x => x^3 - 2*x + 1");
    ev(&mut e, "g = interpolation(map(x => [[x], p(x), 3*x^2 - 2], [0, 0.7, 1.5, 3]))");
    assert_eq!(close(&mut e, "g(2.2)", "p(2.2)", "1e-12"), "true");
    assert_eq!(close(&mut e, "derivative(1)(g)(0.3)", "3*0.3^2 - 2", "1e-12"), "true");
    // with second derivatives a quintic is reproduced; items [x, f, f', f'']
    ev(&mut e, "q = x => x^5 - x^4 + 3*x");
    ev(&mut e, "k = interpolation(map(x => [x, q(x), 5*x^4 - 4*x^3 + 3, 20*x^3 - 12*x^2], [0, 1, 2.5]))");
    assert_eq!(close(&mut e, "k(1.7)", "q(1.7)", "1e-11"), "true");
    assert_eq!(close(&mut e, "derivative(2)(k)(0.4)", "20*0.4^3 - 12*0.4^2", "1e-10"), "true");
}

#[test]
fn hermite_on_a_grid() {
    let mut e = CoreEngine::new();
    // the documented data: values and gradients of sin(x y) on a grid; plotted
    ev(&mut e, "s1 = interpolation(flatten(table([[x, y], sin(x*y), [y*cos(x*y), x*cos(x*y)]], [x, 0, 3, 1.0], [y, 0, 3, 1.0]), 1))");
    assert_eq!(close(&mut e, "s1(1.0, 2.0)", "sin(2.0)", "1e-14"), "true");
    assert!(ev(&mut e, "contour_plot(s1(x, y), [x, 0, 3], [y, 0, 3])").starts_with("image("));
    // a polynomial of degree 3 in each variable is reproduced from values and gradients, and from values,
    // gradients and Hessians
    ev(&mut e, "pp = (x, y) => x^3*y^2 - x*y + 2*y^3");
    ev(&mut e, "s2 = interpolation(flatten(table([[x, y], pp(x, y), [3*x^2*y^2 - y, 2*x^3*y - x + 6*y^2]], [x, 0, 3, 1.0], [y, 0, 3, 1.0]), 1))");
    assert_eq!(close(&mut e, "s2(1.3, 2.6)", "pp(1.3, 2.6)", "1e-10"), "true");
    ev(&mut e, "s3 = interpolation(flatten(table([[x, y], pp(x, y), [3*x^2*y^2 - y, 2*x^3*y - x + 6*y^2], [[6*x*y^2, 6*x^2*y - 1], [6*x^2*y - 1, 2*x^3 + 12*y]]], [x, 0, 3, 1.0], [y, 0, 3, 1.0]), 1))");
    assert_eq!(close(&mut e, "s3(0.4, 2.9)", "pp(0.4, 2.9)", "1e-10"), "true");
}

#[test]
fn least_rectangles_around_points() {
    let mut e = CoreEngine::new();
    ev(&mut e, "pts = [[3, 10], [6, 3], [10, 2], [2, 8], [3, 3]]");
    assert_eq!(ev(&mut e, "bounding_region(pts, min_oriented_rectangle)"), "parallelogram([738/113, -117/113], [[-7, 8], [392/113, 343/113]])");
    // its area 49 is less than the box's 64, and it holds the points
    assert_eq!(ev(&mut e, "[area(bounding_region(pts, min_oriented_rectangle)), area(bounding_region(pts))]"), "[49, 64]");
    assert_eq!(ev(&mut e, "region_member(bounding_region(pts, min_oriented_rectangle), pts)"), "[true, true, true, true, true]");
    // a tilted rectangle with corners (0, 0), (4, 3), (1, 7), (-3, 4) and points inside is itself (area 25)
    assert_eq!(ev(&mut e, "area(bounding_region([[0, 0], [4, 3], [1, 7], [-3, 4], [1, 2], [0, 4]], min_oriented_rectangle))"), "25");
    // a right triangle: twice its area
    assert_eq!(ev(&mut e, "area(bounding_region([[0, 0], [4, 0], [0, 3]], min_oriented_rectangle))"), "12");
    // the interval of a mesh on a line
    assert_eq!(ev(&mut e, "bounding_region(mesh_region([[1.0], [2.0], [3.0], [4.0]], [line([[1, 2], [3, 4]])]), min_interval)"), "interval([1.0, 4.0])");
}
