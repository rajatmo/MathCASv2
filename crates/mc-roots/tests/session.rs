//! Behaviour in a session: identical results at 1 and 4 threads (also under the stress collector), interrupts of
//! long refinements.
mod common;
use common::Sys;
use std::time::Duration;

/// Problems touching every path: Newton, secant, Brent, constrained Newton, complex Newton, systems (with the
/// finite-difference batch of Broyden's method), minimisation in one and several variables, balls.
const CASES: &[&str] = &[
    "find_root(cos(x) == x, [x, 1])",
    "find_root(x^3 - 2*x - 5, [x, 2, 3])",
    "find_root(x^3 - 2*x - 5, [x, 2, 2, 3])",
    "find_root((x - 1)^2 - 0.0001, [x, 0.6, 0.5, 1.5])",
    "find_root(x^3 - 1, [x, -1 + im])",
    "find_root([x^2 + y^2 - 4, x*y - 1], [[x, 2], [y, 0.5]])",
    "find_root([x^2 + y^2 - 4, x*y - 1], [[x, 2], [y, 0.5]], method: broyden)",
    "find_root([abs(x) - 1, abs(y) - 2, z - x*y], [[x, 0.5], [y, 1], [z, 0]])",
    "find_minimum((1 - x)^2 + 100*(y - x^2)^2, [[x, -1.2], [y, 1]])",
    "rule h2(x_ : Real, y_ : Real) := (x - 1)^2 + (y + 2)^2; find_minimum(h2(x, y), [[x, 0], [y, 0]])",
    "find_maximum(sin(x), [x, 1])",
    "find_root(x*exp(x) == 1, [x, 1], digits: 50, verify: true)",
    "find_root(x^2 + 1, [x, 1])",
];

fn results(threads: usize, stress: bool) -> Vec<String> {
    let mut s = Sys::with_threads(threads, stress);
    CASES.iter().map(|c| s.run(c)).collect()
}

#[test]
fn identical_at_one_and_four_threads() {
    let one = results(1, false);
    assert_eq!(one, results(4, false));
    assert_eq!(one, results(4, true));
    assert_eq!(one, results(1, true));
}

#[test]
fn interrupts() {
    let mut s = Sys::new();
    // 10^5 digits take seconds; the stop comes after 300 ms
    let (r, took) = s.interrupted("find_root(sin(x) - 1/2, [x, 0], digits: 100000)", Duration::from_millis(300));
    assert!(r.is_err() || r.as_ref().is_ok_and(|v| v.contains("interrupted")), "not stopped: {r:?}");
    assert!(took < Duration::from_secs(10), "stopped after {took:?}");
    assert_eq!(s.run("find_root(x - 2, [x, 0])"), "[x -> 2.0]");
    // a minimisation refined to 20000 digits (about 3 s)
    let (r, took) = s.interrupted(
        "find_minimum(x^2 + y^2 + sin(30*x)*cos(30*y), [[x, 3], [y, 2]], max_iterations: 1000000, digits: 20000)",
        Duration::from_millis(300),
    );
    assert!(r.is_err() || r.as_ref().is_ok_and(|v| v.contains("interrupted")), "not stopped: {r:?}");
    assert!(took < Duration::from_secs(10), "stopped after {took:?}: {r:?}");
    assert_eq!(s.run("find_root(x - 3, [x, 0])"), "[x -> 3.0]");
}
