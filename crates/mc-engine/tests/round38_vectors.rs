//! Round 38: vector constraints in optimization (inactive sums, parameter equations, named constraints, integer
//! variables among real ones), vector calculus in orthogonal coordinates, Gaussian derivative kernels.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn vector_constraints() {
    // x1 + 2 x2 - 3 >= 0, x1 + 1 >= 0: the minimum of x1 + x2 is 1 at the vertex (-1, 2), checked against the
    // scalar form of the same program
    let v = "vector_greater_equal([inactive(add)(dot([[1, 2], [1, 0]], x), [-3, 1]), 0])";
    assert_eq!(ev(&format!("n_min_value([dot([1, 1], x), {v}], x)")), "1.0");
    assert_eq!(ev(&format!("n_arg_max([dot([-1, -1], x), {v}], x)")), "[-1.0, 2.0]");
    assert_eq!(ev("n_min_value([x1 + x2, x1 + 2 * x2 >= 3, x1 >= -1], [x1, x2])"), "1.0");
    // parameter equations fix a and b
    let p = "[[a == [[1, 2], [1, 0]], b == [3, -1]], vector_greater_equal([dot(a, x) + b, 0])]";
    assert_eq!(ev(&format!("n_min_value([dot([1, 1], x), {p}], x)")), "-1.0");
    assert_eq!(ev("n_min_value([x1 + x2, x1 + 2 * x2 + 3 >= 0, x1 - 1 >= 0], [x1, x2])"), "-1.0");
    assert_eq!(ev(&format!("n_maximize([dot([-1, -1], x), {p}], x)")), "[1.0, [x -> [1.0, -2.0]]]");
    // a constraint named by a symbol
    assert_eq!(ev("c = vector_greater_equal([dot([[1, 2], [1, 0]], v), [3, -1]], [\"NonNegativeCone\", 2]); n_min_value([dot([1, 1], v), c], v)"), "1.0");
    // an integer variable among real ones
    assert_eq!(ev("n_arg_max([x + y, x + 2 * y <= 3, x <= 1], [x, element(y, integers)])"), "[1.0, 1]");
    assert_eq!(ev("n_arg_min([x + y, x + 2 * y >= 3, x >= -2], [x, element(y, integers)])"), "[-2.0, 3]");
    assert_eq!(ev("n_min_value([x + y, x + 2 * y >= 3, x >= -2], [x, element(y, integers)])"), "1.0");
}

#[test]
fn vector_calculus_in_coordinates() {
    // documented: the polar curl of [r sin t, -r cos t]
    assert_eq!(ev("curl([r * sin(t), -r * cos(t)], [r, t], \"Polar\")"), "-3*cos(t)");
    // the same field in Cartesian form is [y, -x] at (x, y) = r (cos t, sin t) rotated: its curl -2 ... plus the
    // radial part; independent route: the divergence of r e_r is 2 in the plane and 3 in space
    assert_eq!(ev("div([r, 0], [r, t], \"Polar\")"), "2");
    assert_eq!(ev("div([r, 0, 0], [r, t, p], \"Spherical\")"), "3");
    assert_eq!(ev("div([x, y, z], [x, y, z])"), "3");
    assert_eq!(ev("div([0, 0, 1 / r], [r, t, z], \"Cylindrical\")"), "0");
    assert_eq!(ev("curl([f(r), 0, 0], [r, t, p], \"Spherical\")"), "[0, 0, 0]");
    // a rigid rotation v_t = r has curl 2 along z, as [-y, x, 0] has in Cartesian coordinates
    assert_eq!(ev("curl([0, r, 0], [r, t, z], \"Cylindrical\")"), "[0, 0, 2]");
    assert_eq!(ev("curl([-y, x, 0], [x, y, z])"), "[0, 0, 2]");
    // harmonic functions: 1/r in space, log r in the plane
    assert_eq!(ev("laplacian(1 / r, [r, t, p], \"Spherical\")"), "0");
    assert_eq!(ev("laplacian(log(r), [r, t], \"Polar\")"), "0");
    assert_eq!(ev("laplacian(r^2, [r, t, z], \"Cylindrical\")"), "4");
    assert_eq!(ev("grad(r^2 * sin(t), [r, t], \"Polar\")"), "[2*r*sin(t), r*cos(t)]");
    // an unknown system stays as it is
    assert_eq!(ev("grad(r, [r, t], \"Elliptic\")"), "grad(r, [r, t], \"Elliptic\")");
}

#[test]
fn gaussian_derivative_kernels() {
    // order k: the correlation with t^k is k!; the lower moments vanish
    assert_eq!(ev("g = gaussian_matrix(3, [1]); abs(total(map(i => g[i] * (4 - i), range(7))) - 1) < 10^-12"), "true");
    assert_eq!(ev("g = gaussian_matrix(3, [1]); abs(total(g)) < 10^-12"), "true");
    assert_eq!(ev("g = gaussian_matrix(4, [2]); abs(total(map(i => g[i] * (5 - i)^2, range(9))) - 2) < 10^-12"), "true");
    assert_eq!(ev("g = gaussian_matrix(4, [2]); abs(total(g)) < 10^-12"), "true");
    assert_eq!(ev("dimensions(gaussian_matrix([20, 20], [0, 1]))"), "[41, 41]");
    assert_eq!(ev("dimensions(gaussian_matrix([[2, 4]], [1, 3]))"), "[5, 9]");
    assert_eq!(ev("dimensions(gaussian_matrix([[20], [2]]))"), "[41]");
    // order 0 in two dimensions is the plain kernel
    assert_eq!(ev("max(abs(flatten(gaussian_matrix(3, [0, 0]) - gaussian_matrix(3)))) < 10^-12"), "true");
}
