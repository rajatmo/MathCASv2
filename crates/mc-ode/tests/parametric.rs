//! `parametric_n_dsolve_value`: the equations reduced once, then integrated for each set of parameter values, with
//! the results of `n_dsolve_value` on the same equations with the values written in.
mod common;
use common::{close, number, Sys};

#[test]
fn values_are_those_of_n_dsolve_value() {
    let mut s = Sys::new();
    s.run("f = parametric_n_dsolve_value([y'(t) == s - k*y(t), y(0) == 0], y(1), [t, 0, 1], [s, k])");
    for (sv, kv) in [("2.0", "1.0"), ("5.0", "0.3"), ("0.5", "2.9"), ("1", "1/2")] {
        let p = s.run(&format!("f({sv}, {kv})"));
        let d = s.run(&format!("n_dsolve_value([y'(t) == {sv} - {kv}*y(t), y(0) == 0], y(1), [t, 0, 1])"));
        assert_eq!(p, d, "f({sv}, {kv})");
        let (sv, kv) = (number(&s.run(&format!("n({sv})"))), number(&s.run(&format!("n({kv})"))));
        assert!(close(number(&p), sv / kv * (1.0 - (-kv).exp()), 1e-7), "{p}");
    }
    // closed numeric values through n, symbolic ones stay
    assert_eq!(s.run("f(2*pi/pi, 1)"), s.run("f(2.0, 1.0)"));
    assert!(s.run("f(a, 1)").starts_with("parametric_function([s, k], y(1), "));
}

#[test]
fn parameters_in_initial_values_expressions_and_options() {
    let mut s = Sys::new();
    // y = y0 e^(-k t): a closure of t, initial values from the parameters
    s.run("g = parametric_n_dsolve_value([y'(t) == -k*y(t), y(0) == y0], y(t), [t, 0, 2], [k, y0])");
    let v = number(&s.run("g(1.0, 3.0)(1.5)"));
    assert!(close(v, 3.0 * (-1.5f64).exp(), 1e-7), "{v}");
    // an expression with a parameter, a second-order system, and an option
    s.run("h = parametric_n_dsolve_value([x''(t) == -w^2*x(t), x(0) == 1, x'(0) == 0], [w*x(1), x'(1)], [t, 0, 1], [w], precision_goal: 12, accuracy_goal: 12)");
    let r = s.run("h(2.0)");
    let inner = r.trim_start_matches('[').trim_end_matches(']');
    let xs: Vec<f64> = inner.split(", ").map(number).collect();
    assert!(close(xs[0], 2.0 * 2f64.cos(), 1e-10) && close(xs[1], -2.0 * 2f64.sin(), 1e-10), "{r}");
    // high precision under digits
    let d = s.run("parametric_n_dsolve_value([y'(t) == a*y(t), y(0) == 1], y(1), [t, 0, 1], [a], digits: 30)(1)");
    assert!(d.starts_with("2.71828182845904523536028747135"), "{d}");
}

#[test]
fn failures() {
    let mut s = Sys::new();
    let f = "parametric_n_dsolve_value([y'(t) == s - k*y(t), y(0) == 0], y(1), [t, 0, 1], [s, k])";
    assert_eq!(s.run(&format!("{f}(1.0)")), "failure(argument, parametric_n_dsolve_value, \"2 parameter values expected\")");
    assert_eq!(s.run(&format!("{f}(1.0, 2im)")), "failure(argument, parametric_n_dsolve_value, \"the parameter values must be real\")");
    assert_eq!(
        s.run("parametric_n_dsolve_value([y'(t) == s, y(0) == 0], y(1), [t, 0, 1], [y])"),
        "failure(argument, parametric_n_dsolve_value, \"a parameter is also an unknown\")"
    );
    assert_eq!(
        s.run("parametric_n_dsolve_value([y'(t) == s, y(0) == 0], y(1), [t, 0, 1], s)"),
        "failure(argument, parametric_n_dsolve_value, 4)"
    );
    assert_eq!(
        s.run("parametric_n_dsolve_value([y'(t) == s, y(0) == c], y(1), [t, 0, 1], [s])"),
        "failure(argument, parametric_n_dsolve_value, \"initial condition 2 is not numeric\")"
    );
    assert_eq!(
        s.run("parametric_function([s], y(1), [1, 2])(3)"),
        "failure(argument, parametric_function, \"not a parametric function\")"
    );
    // a blow-up for some values: the failure of n_dsolve_value at those values
    let r = s.run("parametric_n_dsolve_value([y'(t) == y(t)^2, y(0) == c], y(2), [t, 0, 2], [c])(1.0)");
    assert!(r.contains("parametric_n_dsolve_value") || r.starts_with("failure("), "{r}");
}
