//! Interpolating functions with the rest of the system: `d` of a solution gives the interpolating function of its
//! derivative (the name with primes), and exact numeric times are evaluated with `n` (machine precision, or the
//! working precision of a ball solution).
mod common;
use common::{agreeing_digits, close, number, Sys};

/// `y' = -y, y(0) = 1` on [0, 2] as `g`, and the harmonic oscillator `x'' = -x, x(0) = 0, x'(0) = 1` as `h`.
const SETUP: &str = "sol = n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 2], accuracy_goal: 12, precision_goal: 12); \
    g = rewrite(y, sol); \
    osc = n_dsolve([x''(t) == -x(t), x(0) == 0, x'(0) == 1], x, [t, 0, 4], accuracy_goal: 12, precision_goal: 12); \
    h = rewrite(x, osc); null";

#[test]
fn derivatives_of_interpolating_functions() {
    let mut s = Sys::new();
    s.run(SETUP);
    let d1 = s.run("d(g(u), u)");
    assert!(d1.starts_with("interpolating_function(\"y'\", [0.0, 2.0], ") && d1.ends_with(")(u)"), "{d1}");
    assert!(s.run("d(g(u), [u, 2])").starts_with("interpolating_function(\"y''\", "));
    // the derivative term is the one named by hand, and the operator form evaluates the same way
    let by_name = s.run("g").replace("(\"y\"", "(\"y'\"");
    assert_eq!(s.run(&format!("d(g(u), u) === {by_name}(u)")), "true");
    assert_eq!(s.run("g'(1) === rewrite(d(g(u), u), [u -> 1])"), "true");
    let e1 = (-1f64).exp();
    assert!(close(number(&s.run("rewrite(d(g(u), u), [u -> 1])")), -e1, 1e-9));
    assert!(close(number(&s.run("g''(1)")), e1, 1e-6));
    // the chain rule through the solution
    assert!(close(number(&s.run("rewrite(d(g(u^2), u), [u -> 1])")), -2.0 * e1, 1e-9));
    assert!(close(number(&s.run("rewrite(d(u*g(u), u), [u -> 1])")), e1 - e1, 1e-9));
    // a second-order equation: x' and x'' are state components or the right-hand side's order, x''' the dense
    // output's derivative
    assert!(close(number(&s.run("rewrite(d(h(u), u), [u -> 1])")), 1f64.cos(), 1e-9));
    assert!(close(number(&s.run("rewrite(d(h(u), [u, 2]), [u -> 1])")), -1f64.sin(), 1e-6));
    assert!(close(number(&s.run("h'''(1)")), -1f64.cos(), 1e-3));
    // the 0th derivative is the function itself; other derivative forms stay
    assert_eq!(s.run("derivative(0)(g)(1) === g(1)"), "true");
    assert!(s.run("derivative(1, 1)(g)(1, 2)").starts_with("derivative(1, 1)(interpolating_function("));
}

#[test]
fn exact_times() {
    let mut s = Sys::new();
    s.run(SETUP);
    let half_pi = std::f64::consts::FRAC_PI_2;
    assert!(close(number(&s.run("g(pi/2)")), (-half_pi).exp(), 1e-10));
    assert!(close(number(&s.run("g(sqrt(2))")), (-2f64.sqrt()).exp(), 1e-10));
    assert!(close(number(&s.run("g'(pi/2)")), -(-half_pi).exp(), 1e-9));
    assert!(close(number(&s.run("h(pi)")), 0.0, 1e-9));
    let v = s.run("g([1, pi/2, 3/2])");
    assert!(v.starts_with('[') && !v.contains("pi"), "{v}");
    // exact times keep their form in failures; symbolic times leave the call
    assert_eq!(s.run("g(3*pi)"), "failure(out_of_range, 3*pi, [0.0, 2.0])");
    assert!(s.run("g(a + 1)").ends_with(")(1 + a)"));
    assert!(s.run("g(a*pi)").ends_with(")(a*pi)"));
}

#[test]
fn exact_times_of_ball_solutions() {
    let mut s = Sys::new();
    s.run("bs = n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 2], digits: 30); b = rewrite(y, bs); null");
    let v = s.run("b(pi/2)");
    let want = s.run("n(exp(-pi/2), 40)");
    // the time is taken at the working precision: about 30 correct digits
    assert!(agreeing_digits(&v, &want) >= 28, "{v} vs {want}");
    let dv = s.run("rewrite(d(b(u), u), [u -> pi/2])");
    let dv = dv.strip_prefix('-').unwrap_or_else(|| panic!("{dv}"));
    assert!(agreeing_digits(dv, &want) >= 28, "{dv} vs {want}");
}

#[test]
fn one_and_four_threads_agree() {
    let cases = [
        "d(g(u), u)",
        "rewrite(d(g(u), u), [u -> 1])",
        "g(pi/2)",
        "g''(sqrt(2))",
        "h'''(1)",
        "g([1, pi/2])",
        "limit(g(u), u -> 1)",
    ];
    let mut one = Sys::with_threads(1, false);
    let mut four = Sys::with_threads(4, false);
    one.run(SETUP);
    four.run(SETUP);
    for c in cases {
        assert_eq!(one.run(c), four.run(c), "{c}");
    }
}
