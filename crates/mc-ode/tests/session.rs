//! Behaviour in a session: identical results at 1 and 4 threads (also under the stress collector), compiled and
//! interpreted right-hand sides giving identical solutions, interrupts of long integrations, and the measurement
//! of the compiled backend against the interpreted one.
mod common;
use common::Sys;
use std::time::{Duration, Instant};

/// Problems touching every path: both machine methods and the switch, events, n_dsolve_value, balls; the last
/// statements evaluate solutions inside a parallel map.
const CASES: &[&str] = &[
    "s1 = n_dsolve([y'(t) == -y(t) + sin(t), y(0) == 1], y, [t, 0, 10]); s1",
    "map(k => rewrite(y(k/3), s1), range(30))",
    "n_dsolve([x'(t) == y(t), y'(t) == 100*(1 - x(t)^2)*y(t) - x(t), x(0) == 2, y(0) == 0], [x, y], [t, 0, 300])",
    "n_dsolve([y''(t) == -y(t), y(0) == 0, y'(0) == 1], y, [t, 0, 10], events: [y(t) == 0], event_action: record)",
    "n_dsolve_value([x'(t) == 10*(y(t) - x(t)), y'(t) == x(t)*(28 - z(t)) - y(t), z'(t) == x(t)*y(t) - 8/3*z(t), x(0) == 1, y(0) == 1, z(0) == 1], [x(5), y(5), z(5)], [t, 0, 5])",
    "map(k => n_dsolve_value([y'(t) == -k*y(t), y(0) == 1], y(1), [t, 0, 1]), range(8))",
    "n_dsolve_value([y'(t) == 1 + y(t)^2, y(0) == 0], y(1), [t, 0, 1], digits: 30)",
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
fn compiled_and_interpreted_agree() {
    // decision N0.2: the compiled code gives the interpreter's values bit for bit, so the solutions are the same
    // terms (their keys hash the data)
    let mut s = Sys::new();
    for eq in [
        "n_dsolve([x'(t) == 10*(y(t) - x(t)), y'(t) == x(t)*(28 - z(t)) - y(t), z'(t) == x(t)*y(t) - 8/3*z(t), x(0) == 1, y(0) == 1, z(0) == 1], [x, y, z], [t, 0, 2]",
        "n_dsolve([x'(t) == y(t), y'(t) == 1000*(1 - x(t)^2)*y(t) - x(t), x(0) == 2, y(0) == 0], [x, y], [t, 0, 10]",
        "n_dsolve([y'(t) == exp(-t)*sqrt(y(t)) + atan(t)*cos(y(t)), y(0) == 1], y, [t, 0, 5], method: stiff",
    ] {
        let a = s.run(&format!("{eq}, compile: true)"));
        let b = s.run(&format!("{eq}, compile: false)"));
        assert_eq!(a, b, "{eq}");
    }
}

#[test]
fn interrupts() {
    let mut s = Sys::new();
    // Lorenz with an interpreted right-hand side over a long range takes minutes; the stop comes after 300 ms
    let (r, took) = s.interrupted(
        "n_dsolve([x'(t) == 10*(y(t) - x(t)), y'(t) == x(t)*(28 - z(t)) - y(t), z'(t) == x(t)*y(t) - 8/3*z(t), x(0) == 1, y(0) == 1, z(0) == 1], [x, y, z], [t, 0, 10^5], compile: false, max_steps: 10^8)",
        Duration::from_millis(300),
    );
    assert!(r.is_err() || r.as_ref().is_ok_and(|v| v.contains("interrupted")), "not stopped: {r:?}");
    assert!(took < Duration::from_secs(5), "stopped after {took:?}");
    // compiled, and a Taylor integration at 2000 digits
    let (r, took) = s.interrupted(
        "n_dsolve([x'(t) == 10*(y(t) - x(t)), y'(t) == x(t)*(28 - z(t)) - y(t), z'(t) == x(t)*y(t) - 8/3*z(t), x(0) == 1, y(0) == 1, z(0) == 1], [x, y, z], [t, 0, 10^7], max_steps: 10^8)",
        Duration::from_millis(300),
    );
    assert!(r.is_err() || r.as_ref().is_ok_and(|v| v.contains("interrupted")), "not stopped: {r:?}");
    assert!(took < Duration::from_secs(5), "stopped after {took:?}");
    let (r, took) = s.interrupted("n_dsolve_value([y'(t) == cos(t)*y(t), y(0) == 1], y(100), [t, 0, 100], digits: 2000)", Duration::from_millis(300));
    assert!(r.is_err() || r.as_ref().is_ok_and(|v| v.contains("interrupted")), "not stopped: {r:?}");
    assert!(took < Duration::from_secs(5), "stopped after {took:?}");
    assert_eq!(s.run("n_dsolve_value([y'(t) == 0, y(0) == 3], y(1), [t, 0, 1])"), "3.0");
}

#[test]
fn compiled_against_interpreted_timing() {
    // the measurement reported in the notes (busy machine; printed with --nocapture)
    let mut s = Sys::new();
    let lorenz = "[x'(t) == 10*(y(t) - x(t)), y'(t) == x(t)*(28 - z(t)) - y(t), z'(t) == x(t)*y(t) - 8/3*z(t), x(0) == 1, y(0) == 1, z(0) == 1]";
    let vdp = "[x'(t) == y(t), y'(t) == 1000*(1 - x(t)^2)*y(t) - x(t), x(0) == 2, y(0) == 0]";
    for (what, src) in [
        ("Lorenz to t = 20, tol 1e-10", format!("n_dsolve({lorenz}, [x, y, z], [t, 0, 20], accuracy_goal: 10, precision_goal: 10")),
        ("Van der Pol mu = 1000 to t = 3000", format!("n_dsolve({vdp}, [x, y], [t, 0, 3000]")),
    ] {
        let mut times = vec![];
        for c in ["true", "false"] {
            let start = Instant::now();
            let r = s.run(&format!("{src}, compile: {c})"));
            times.push(start.elapsed());
            assert!(r.starts_with('['), "{r}");
        }
        println!("{what}: compiled {:?}, interpreted {:?}", times[0], times[1]);
        // compiled is not slower (generous: the machine is shared)
        assert!(times[0] < times[1] * 2, "{what}: {times:?}");
    }
}
