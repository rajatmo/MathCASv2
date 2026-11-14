//! Round 40: tables of systems of recurrences in several sequences, run forward from the initial values. The values
//! are checked against closed forms of the same sequences (Fibonacci numbers, powers) computed independently.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn systems_of_recurrences() {
    let mut e = CoreEngine::new();
    let sys = "[x(n_u + 2) == y(n_u), y(n_u + 1) == -x(n_u), x(0) == 1, x(1) == 0, y(0) == 0]";
    assert_eq!(ev(&mut e, &format!("recurrence_table({sys}, x, [n_u, 0, 3], dependent_variables: [x, y])")), "[1, 0, 0, -1]");
    assert_eq!(ev(&mut e, &format!("recurrence_table({sys}, y, [n_u, 0, 3], dependent_variables: [x, y])")), "[0, -1, 0, 0]");
    // variation: the Fibonacci numbers as a first-order system, both sequences listed
    let r = ev(&mut e, "recurrence_table([x(n + 1) == x(n) + y(n), y(n + 1) == x(n), x(1) == 1, y(1) == 0], [x, y], [n, 1, 12])");
    assert_eq!(r, ev(&mut e, "table([fibonacci(k), fibonacci(k - 1)], [k, 1, 12])"));
    // variation: three sequences with a coefficient n + 1 and a step of 3 (the values worked out by hand: u = 1, 2,
    // 2, 4, 12, 16, 40, 144, 224, 640)
    let r = ev(&mut e, "recurrence_table([u(n + 1) == 2*v(n), v(n + 1) == (n + 1)*w(n), w(n + 1) == u(n), u(0) == 1, v(0) == 1, w(0) == 1], u, [n, 0, 9, 3], dependent_variables: [u, v, w])");
    assert_eq!(r, "[1, 4, 40, 640]");
    // an equation with two occurrences at the highest shift is not run
    let r = ev(&mut e, "recurrence_table([x(n + 1) + y(n + 1) == x(n), y(n) == 1, x(0) == 1], [x, y], [n, 0, 2])");
    assert!(r.starts_with("recurrence_table("), "{r}");
}
