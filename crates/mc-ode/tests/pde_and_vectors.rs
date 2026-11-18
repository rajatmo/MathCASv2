//! Partial differential equations in one space dimension by the method of lines, and vector-valued unknowns.
mod common;
use common::{close, number, Sys};

/// The heat equation with u(0, x) = sin(pi x) and zero ends: u = exp(-pi^2 t) sin(pi x).
#[test]
fn heat_equation_dirichlet() {
    let mut s = Sys::new();
    let r = s.run("sol = n_dsolve([d(u(t, x), t) == d(u(t, x), x, x), u(0, x) == sin(pi*x), u(t, 0) == 0, u(t, 1) == 0], u, [t, 0, 1], [x, 0, 1]); sol");
    assert!(r.starts_with("[u -> interpolating_function(\"u\", [[0.0, 1.0], [0.0, 1.0]], "), "{r}");
    for (t, x) in [(0.1, 0.5), (0.05, 0.25), (0.3, 0.9)] {
        let v = number(&s.run(&format!("rewrite(u({t}, {x}), sol)")));
        let exact = (-std::f64::consts::PI.powi(2) * t).exp() * (std::f64::consts::PI * x).sin();
        assert!(close(v, exact, 2e-4), "u({t}, {x}) = {v}, exact {exact}");
    }
    // outside the grid: out of range
    assert!(s.run("rewrite(u(0.1, 2), sol)").starts_with("failure(out_of_range"));
}

/// The heat equation on [0, 5] with u(0, x) = 0, u(t, 0) = sin t, u(t, 5) = 0 against its series solution
/// u = sin(t) (1 - x/5) + sum_n b_n(t) sin(n pi x/5), b_n = -(2/(n pi)) (l_n cos t + sin t - l_n e^(-l_n t))/(l_n^2 + 1),
/// l_n = (n pi/5)^2 (the values below: 400,000 terms), including the boundary layer of the first hundredth of time,
/// where the three-point differences were 4% off.
#[test]
fn heat_equation_with_a_moving_end_against_the_series_solution() {
    let mut s = Sys::new();
    s.run("sol = n_dsolve([d(u(t, x), t) == d(u(t, x), x, x), u(0, x) == 0, u(t, 0) == sin(t), u(t, 5) == 0], u, [t, 0, 10], [x, 0, 5]); null");
    let series = [
        (10.0, 1.0, 0.06845657927061878),
        (5.0, 0.5, -0.6904918158486054),
        (2.0, 2.0, 0.23010366013780686),
        (1.0, 0.25, 0.6469879943648084),
        (7.5, 3.0, -0.08758564453508388),
        (7.011849688903379, 1.0416666666666667, 0.0031247235018663664),
        (0.1, 0.3, 0.030072396277534257),
        (0.01, 0.15, 0.0013159516241787995),
    ];
    for (t, x, exact) in series {
        let v = number(&s.run(&format!("rewrite(u({t}, {x}), sol)")));
        assert!((v - exact).abs() < 2e-7, "u({t}, {x}) = {v}, series {exact}");
    }
}

/// The wave equation u_tt = u_xx with u(0, x) = sin(pi x), u_t(0, x) = 0: u = cos(pi t) sin(pi x).
#[test]
fn wave_equation() {
    let mut s = Sys::new();
    s.run("g = n_dsolve_value([d(u(t, x), t, t) == d(u(t, x), x, x), u(0, x) == sin(pi*x), derivative(1, 0)(u)(0, x) == 0, u(t, 0) == 0, u(t, 1) == 0], u, [t, 0, 1], [x, 0, 1]); null");
    for (t, x) in [(0.3, 0.5), (0.7, 0.2)] {
        let v = number(&s.run(&format!("g({t}, {x})")));
        let exact = (std::f64::consts::PI * t).cos() * (std::f64::consts::PI * x).sin();
        assert!(close(v, exact, 2e-4), "u({t}, {x}) = {v}, exact {exact}");
    }
}

/// A Dirichlet datum that varies in time and a Neumann end: u_t = u_xx, u(t, 0) = t, u_x(t, 1) = 0, u(0, x) = 0
/// has u(t, 0) = t, and u = t + (x^2 - 2 x)/2 plus a transient that decays like exp(-pi^2 t/4).
#[test]
fn boundary_data_in_time_and_neumann_end() {
    let mut s = Sys::new();
    s.run("f = n_dsolve_value([d(u(t, x), t) == d(u(t, x), x, x), u(0, x) == 0, u(t, 0) == t, rewrite(d(u(t, x), x), x -> 1) == 0], u, [t, 0, 2], [x, 0, 1])");
    assert!(close(number(&s.run("f(1.5, 0)")), 1.5, 1e-6));
    // the steady state of the moving datum lags behind it: u(t, 1) < t, and u(t, x) increases towards x = 0
    let (a, b) = (number(&s.run("f(2, 1)")), number(&s.run("f(2, 0.5)")));
    assert!(a < b && b < 2.0, "{a} {b}");
    // u = t + (x^2 - 2 x)/2 + transient: at t = 2 the transient (exp(-pi^2/4 t)) is below 1e-2
    assert!(close(a, 2.0 - 0.5, 2e-2), "{a}");
}

/// Forms that are not supported stay unevaluated (symbols for the conditions, a mixed derivative).
#[test]
fn unsupported_forms_stay() {
    let mut s = Sys::new();
    let r = s.run("n_dsolve_value([pde, ics, bcs], u, [t, 0, 1], [x, -1, 1])");
    assert!(r.starts_with("n_dsolve_value("), "{r}");
    let r = s.run("n_dsolve([derivative(1, 1)(u)(t, x) == u(t, x), u(0, x) == 0, u(t, 0) == 0, u(t, 1) == 0], u, [t, 0, 1], [x, 0, 1])");
    assert!(r.starts_with("n_dsolve("), "{r}");
}

/// A vector unknown z with z(0) = [1, 0] and z' = a.z: the rotation (cos t, -sin t).
#[test]
fn vector_unknown() {
    let mut s = Sys::new();
    let r = s.run("sol = n_dsolve([z'(t) == dot([[0, 1], [-1, 0]], z(t)), z(0) == [1, 0]], z, [t, 0, 10]); sol");
    assert!(r.starts_with("[z -> interpolating_function(\"z\", [0.0, 10.0], "), "{r}");
    let v = s.run("rewrite(z(2), sol)");
    let parts: Vec<f64> = v.trim_matches(['[', ']']).split(", ").map(number).collect();
    assert!(close(parts[0], 2f64.cos(), 1e-6) && close(parts[1], -(2f64.sin()), 1e-6), "{v}");
    // the derivative is the list of the components' derivatives
    let d = s.run("rewrite(z'(2), sol)");
    let parts: Vec<f64> = d.trim_matches(['[', ']']).split(", ").map(number).collect();
    assert!(close(parts[0], -(2f64.sin()), 1e-6) && close(parts[1], -(2f64.cos()), 1e-6), "{d}");
}

/// A second-order vector equation with chained initial conditions y(0) == y'(0) == [1, 1], through
/// `n_dsolve_value`: y'' = -y gives y = cos t + sin t in each component.
#[test]
fn vector_unknown_second_order_value() {
    let mut s = Sys::new();
    s.run("ysol = n_dsolve_value([y''(x) + dot([[1, 0], [0, 1]], y(x)) == 0, y(0) == y'(0) and y'(0) == [1, 1]], y, [x, 0, 8], precision_goal: 12, accuracy_goal: 12)");
    let v = s.run("ysol(5)");
    let parts: Vec<f64> = v.trim_matches(['[', ']']).split(", ").map(number).collect();
    let exact = 5f64.cos() + 5f64.sin();
    assert!(parts.len() == 2 && parts.iter().all(|&p| close(p, exact, 1e-9)), "{v}");
    // a list of times gives a list of vectors
    assert!(s.run("ysol([1, 2])").starts_with("[["));
}
