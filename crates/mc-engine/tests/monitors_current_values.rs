//! Monitors are functions of the current values. Each test carries the documented input and variations
//! of its own, checked by a second route: the points a monitor collects satisfy what the method promises (they lie
//! in the range, the last iterate is the root, the times increase to the end), and counts agree between a `var`
//! counter and the collected list.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Duration;

/// Evaluate every statement in one fresh session (one worker thread, big stack): the last value printed.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

/// find_root: the step monitor sees the Newton iterates, ending at the root; the evaluation monitor sees every
/// evaluation, as many as a counter counts.
#[test]
fn find_root_monitors_see_the_iterates() {
    assert_eq!(
        ev("reap(find_root(x^2 - 2, [x, 1], step_monitor: (x => sow(x))))"),
        "[[x -> 1.4142135623730951], [[1.5, 1.4166666666666667, 1.4142156862745099, 1.4142135623746899, 1.4142135623730951]]]"
    );
    // the last iterate is the root the call returns
    assert_eq!(ev("{ let r = reap(find_root(cos(x) == x, [x, 1], step_monitor: (x => sow(x)))); (x /. r[1]) == last(r[2][1]) }"), "true");
    // a counter and the sown list agree; every evaluation point is a number near the start or the root
    let count = ev("{ var c = 0; find_root(x^3 - 2x - 5, [x, 2], evaluation_monitor: (x => { c = c + 1 })); c }");
    let sown = ev("length(last(reap(find_root(x^3 - 2x - 5, [x, 2], evaluation_monitor: (x => sow(x)))))[1])");
    assert_eq!(count, sown);
    assert!(count.parse::<i64>().is_ok_and(|n| n >= 3), "{count}");
    // two equations: the monitor takes two arguments, the evaluations are points of the plane
    let pts = ev("last(reap(find_root([x - 1, 10*(y - x^2)], [[x, -1], [y, 1]], evaluation_monitor: ((x, y) => sow(is_number(x) and is_number(y))))))[1]");
    assert!(pts.starts_with("[true") && !pts.contains("false"), "{pts}");
    // a function as a name: print shows every evaluation
    assert_eq!(ev("find_root(x^2 - 2, [x, 1], evaluation_monitor: print)"), "[x -> 1.4142135623730951]");
}

/// A value that is not a function is a failure that shows the form; a failure from the monitor is the result.
#[test]
fn monitor_values_and_failures() {
    let f = ev("find_root(x^2 - 2, [x, 1], evaluation_monitor: sow(x))");
    assert!(f.starts_with("failure(not_a_function, evaluation_monitor, \"evaluation_monitor takes a function"), "{f}");
    assert!(ev("find_root(x^2 - 2, [x, 1], step_monitor: 3)").starts_with("failure(not_a_function, step_monitor"));
    assert!(ev("n_integrate(x, [x, 0, 1], evaluation_monitor: 0)").starts_with("failure(not_a_function"));
    // throw from a monitor ends the computation; catch gets the value
    assert_eq!(ev("catch(find_root(x^2 - 2, [x, 1], step_monitor: (x => if x < 1.42 then throw(x))))"), "1.4166666666666667");
    assert_eq!(ev("catch(n_sum(1/n^2, [n, 1, inf], evaluation_monitor: (n => if n > 5 then throw(n))))"), "6");
    assert_eq!(ev("n_dsolve([x'(t) == x(t), x(0) == 1], x, [t, 0, 1], step_monitor: (t => if t > 0.5 then throw(\"half\")))"), "failure(thrown, \"half\")");
    // a monitor an integrator has no steps for is an unknown option
    assert_eq!(ev("n_integrate(x, [x, 0, 1], step_monitor: (x => x))"), "failure(unknown_option, step_monitor)");
}

/// find_minimum, n_minimize: steps of two variables reach the minimum; evaluations are counted.
#[test]
fn minimizer_monitors() {
    let r = ev("{ let r = reap(find_minimum((x - 1)^2 + 100*(y - sin(x))^2, [[x, -10], [y, 1]], step_monitor: ((x, y) => sow([x, y])))); [r[1][2], last(r[2][1])] }");
    assert!(r.starts_with("[[x -> 1.0, y -> 0.8414709848078965], [0.99999999999"), "{r}");
    assert_eq!(ev("catch(find_minimum(exp(x) + 1/x^2, [x, 1], evaluation_monitor: (x => if x <= 0 then throw(\"negative\"))))"), "[3.6906033362835577, [x -> 0.9254789277509086]]");
    let c = ev("{ var c = 0; n_minimize(x^2 + (y - 1)^2, [x, y], evaluation_monitor: ((x, y) => { c = c + 1 })); c > 0 }");
    assert_eq!(c, "true");
    // the value forms pass the monitor on
    assert_eq!(ev("{ var c = 0; find_min_value(x^4 - 3x^2 + x, [x, 1], evaluation_monitor: (x => { c = c + 1 })); c > 3 }"), "true");
}

/// n_integrate: every evaluation point lies in the range; with a monitor the value is the plain one.
#[test]
fn integration_monitor_points_lie_in_the_range() {
    let inside = ev("all_true(last(reap(n_integrate(sqrt(x*(1 - x)), [x, 0, 1], evaluation_monitor: (x => sow(x)))))[1], x => 0 <= x and x <= 1)");
    assert_eq!(inside, "true");
    assert_eq!(ev("n_integrate(1/x^2, [x, 0.1, 2], evaluation_monitor: (x => null))"), ev("n_integrate(1/x^2, [x, 0.1, 2])"));
    // two variables: points of the square
    let sq = ev("all_true(last(reap(n_integrate(x*y, [x, 0, 1], [y, 0, 2], evaluation_monitor: ((x, y) => sow([x, y])))))[1], p => 0 <= p[1] and p[1] <= 1 and 0 <= p[2] and p[2] <= 2)");
    assert_eq!(sq, "true");
}

/// n_sum, n_product: the monitor sees the indices (integers from the start); the value is unchanged.
#[test]
fn sum_monitors_see_the_indices() {
    assert_eq!(ev("n_sum(1/(n + 1)^2, [n, 0, inf], evaluation_monitor: (n => null))"), ev("n_sum(1/(n + 1)^2, [n, 0, inf])"));
    // the first terms at the integers from the start; the tail method also evaluates at complex points
    let ks = ev("{ let ks = last(reap(n_sum(1/(n + 1)^2, [n, 0, inf], evaluation_monitor: (n => sow(n)))))[1]; [first(ks), all_true(ks, k => is_number(k))] }");
    assert_eq!(ks, "[0, true]");
    assert_eq!(ev("{ var k = 0; n_product(1 + 1/(2*n + 1)^2, [n, 1, inf], evaluation_monitor: (n => { k = k + 1 })); k > 0 }"), "true");
}

/// n_dsolve: the step times increase to the end of the range; evaluations outnumber steps.
#[test]
fn ode_monitors_see_the_times() {
    let ode = "[x''(t) + sin(x(t)) == 0, x(0) == 1, x'(0) == 0], x, [t, 0, 10]";
    let ts = ev(&format!("{{ let ts = last(reap(n_dsolve({ode}, step_monitor: (t => sow(t)))))[1]; [ordered(ts), last(ts)] }}"));
    assert_eq!(ts, "[true, 10.0]");
    let more = ev(&format!(
        "length(last(reap(n_dsolve({ode}, evaluation_monitor: (t => sow(t)))))[1]) > length(last(reap(n_dsolve({ode}, step_monitor: (t => sow(t)))))[1])"
    ));
    assert_eq!(more, "true");
}
