//! Monitors on the functions that lacked them: `find_fit` (evaluation and step monitors with the parameter values),
//! the Taylor method of `n_dsolve` under `digits:` (the times of its expansions and steps), and the function plots,
//! which have no hook: a function there is an unknown option, anything else is `not_a_function`, never accepted and
//! ignored. Each check has a second route: the last step a monitor sees is the fit returned, the step times increase
//! to the end of the range at the working precision, a counter agrees with the collected list.
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
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &StopFlag::new()) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

const DATA: &str = "data = [[0, 1.0], [1, 2.7], [2, 7.4], [3, 20.1]];\n";
const DECAY: &str = "data = table([x, 2*exp(-x/2) + 1/2], x, range(0, 6, 1/2));\n";

/// find_fit: the step monitor sees the parameters after each accepted step, the last of them the fit; a counter of
/// the evaluations agrees with the sown list; a throw ends the fit; the fit with monitors is the fit without.
#[test]
fn find_fit_monitors_see_the_parameters() {
    // the documented input: the last step is the result
    let r = ev(&format!(
        "{DATA}{{ let r = reap(find_fit(data, a*exp(b*x), [[a, 1], [b, 1]], x, step_monitor: ((a, b) => sow([a, b])))); ([a, b] /. r[1]) == last(r[2][1]) }}"
    ));
    assert_eq!(r, "true");
    // variations: three parameters of a decay, and a straight line of one variable
    let r = ev(&format!(
        "{DECAY}{{ let r = reap(find_fit(data, a*exp(-b*x) + c, [a, b, c], x, step_monitor: ((a, b, c) => sow([a, b, c])))); let fit = ([a, b, c] /. r[1]) == last(r[2][1]); [fit, length(r[2][1]) > 1] }}"
    ));
    assert_eq!(r, "[true, true]");
    assert_eq!(
        ev(&format!("{DECAY}find_fit(data, a*exp(-b*x) + c, [a, b, c], x, evaluation_monitor: ((a, b, c) => null))")),
        ev(&format!("{DECAY}find_fit(data, a*exp(-b*x) + c, [a, b, c], x)"))
    );
    let count = ev(&format!("{DATA}{{ var k = 0; find_fit(data, p*x, [p], x, evaluation_monitor: (p => {{ k = k + 1 }})); k }}"));
    let sown = ev(&format!("{DATA}length(last(reap(find_fit(data, p*x, [p], x, evaluation_monitor: (p => sow(p)))))[1])"));
    assert_eq!(count, sown);
    assert!(count.parse::<i64>().is_ok_and(|n| n >= 2), "{count}");
    // a failure from a monitor is the result; a value that is not a function shows the form
    assert_eq!(ev(&format!("{DATA}catch(find_fit(data, a*exp(b*x), [a, b], x, evaluation_monitor: ((a, b) => if a < 1 then throw(\"small a\"))))")), "\"small a\"");
    let f = ev(&format!("{DATA}find_fit(data, a*exp(b*x), [a, b], x, step_monitor: sow([a, b]))"));
    assert!(f.starts_with("failure(not_a_function, step_monitor"), "{f}");
    // another option still leaves the call as it is
    assert!(ev(&format!("{DATA}find_fit(data, p*x, [p], x, bogus: 1)")).starts_with("find_fit("));
}

/// n_dsolve under `digits:`: the Taylor method calls the step monitor at the end of every step, at the working
/// precision, each of its two runs ending at the end of the range; a throw ends the integration.
#[test]
fn taylor_monitors_see_the_times() {
    let ode = "[y''(t) + y(t) == 0, y(0) == 0, y'(0) == 1], y, [t, 0, 12], digits: 30";
    let r = ev(&format!(
        "{{ let ts = last(reap(n_dsolve({ode}, step_monitor: (t => sow(t)))))[1]; [length(ts) > 4, precision(first(ts)) >= 30, first(ts) > 0, abs(last(ts) - 12) < 10^-25] }}"
    ));
    assert_eq!(r, "[true, true, true, true]");
    // two runs (the second estimates the error), each ending at 12
    let r = ev(&format!("{{ let ts = last(reap(n_dsolve({ode}, step_monitor: (t => sow(t)))))[1]; length(select(ts, t => abs(t - 12) < 10^-25)) }}"));
    assert_eq!(r, "2");
    // evaluations: one expansion per step, from the start
    let r = ev(&format!("{{ let ts = last(reap(n_dsolve({ode}, evaluation_monitor: (t => sow(t)))))[1]; first(ts) == 0 }}"));
    assert_eq!(r, "true");
    assert_eq!(ev(&format!("catch(n_dsolve({ode}, step_monitor: (t => if t > 6 then throw(\"half\"))))")), "\"half\"");
    // the solution with a monitor is the one without
    assert_eq!(
        ev(&format!("{{ let s = n_dsolve({ode}, step_monitor: (t => null)); n(y(10) /. s, 20) }}")),
        ev(&format!("{{ let s = n_dsolve({ode}); n(y(10) /. s, 20) }}"))
    );
    assert_eq!(ev("{ let s = n_dsolve([y'(t) == y(t), y(0) == 1], y, [t, 0, 1], digits: 30); abs((y(1) /. s) - e) < 10^-25 }"), "true");
}

/// The function plots call an evaluation monitor at every sample (see `language_roundtrip_random.rs`); a value that is
/// not a function is `not_a_function`, and a step monitor (a plot takes no steps) is an unknown option, as is a
/// monitor on a plot that samples no function of its own; without a monitor the plot is drawn.
#[test]
fn plots_take_function_monitors_only() {
    assert!(ev("plot(x, [x, 0, 1], evaluation_monitor: (x => sow(x)))").starts_with("image("));
    assert!(ev("plot3d(x*y, [x, 0, 1], [y, 0, 1], evaluation_monitor: ((x, y) => null))").starts_with("image("));
    assert_eq!(ev("complex_plot3d(z^2, [z, -1 - im, 1 + im], step_monitor: (z => null))"), "failure(unknown_option, step_monitor)");
    assert_eq!(ev("complex_plot(z^2, [z, -1 - im, 1 + im], evaluation_monitor: (z => null))"), "failure(unknown_option, evaluation_monitor)");
    let f = ev("plot(x, [x, 0, 1], evaluation_monitor: sow(x))");
    assert!(f.starts_with("failure(not_a_function, evaluation_monitor"), "{f}");
    let f = ev("{ var c = 0; contour_plot(x*y, [x, 0, 1], [y, 0, 1], evaluation_monitor: { c = c + 1 }) }");
    assert!(f.starts_with("failure(not_a_function, evaluation_monitor"), "{f}");
    // nothing is evaluated before the refusal
    assert_eq!(ev("{ var c = 0; is_failure(plot({ c = c + 1; x }, [x, 0, 1], evaluation_monitor: sow(x))); c }"), "0");
    assert!(ev("plot(x, [x, 0, 1])").starts_with("image("));
}
