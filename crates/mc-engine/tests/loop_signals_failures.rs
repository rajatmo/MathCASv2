//! Language behaviour fixed before 1.0. A failure in head position is that failure; the loop signals
//! `break()` and `continue()` are read by every iterating function; an option of a `fn` read in its condition is its
//! value. Each test carries the documented input and variations of its own, checked by a second route (the same
//! computation written with an explicit loop or a smaller list, or the value the call must equal).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Duration;

/// Evaluate every statement in one fresh session (`threads` worker threads, big stack): the last value printed.
fn ev_on(src: &str, threads: usize) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

fn ev(src: &str) -> String {
    ev_on(src, 1)
}

fn same(a: &str, b: &str) {
    assert_eq!(ev(a), ev(b), "{a}  vs  {b}");
}

/// A failure called with arguments is the failure itself, whatever the arguments, and they are not evaluated.
#[test]
fn a_called_failure_is_the_failure() {
    let f = "f = integrate(x^x, [x, 0, 1, 2])\n";
    same(&format!("{f}f(\"Values\")"), &format!("{f}f"));
    assert!(ev(&format!("{f}f(\"Values\")")).starts_with("failure("));
    // a failure stored in a variable, called with several arguments and curried
    assert_eq!(ev("g = failure(boom)\ng(1, 2)(3)"), "failure(boom)");
    assert_eq!(ev("g = failure(boom)\nis_failure(g(x))"), "true");
    // the arguments of the call are not evaluated: nothing is sown
    assert_eq!(ev("g = failure(boom)\nreap(g(sow(1), sow(2)))"), "[failure(boom), []]");
    // a function whose value is a failure, called again: the failure of the first call
    assert_eq!(ev("h(x_) := failure(no_value, x)\nh(1)(2)"), "failure(no_value, 1)");
    // as a strict argument (a function to map, an operator form) the failure ends the call; a list of results
    // keeps it as an element
    assert_eq!(ev("g = failure(boom)\nmap(g, [1, 2])"), "failure(boom)");
    assert_eq!(ev("g = failure(boom)\nselect(g)([1, 2])"), "failure(boom)");
    assert_eq!(ev("g = failure(boom)\nmap(x => g, [1, 2])"), "[failure(boom), failure(boom)]");
}

/// `break()` ends a map with the values before it, `continue()` leaves the element out; the same lists come from an
/// explicit `for` loop that appends.
#[test]
fn map_reads_break_and_continue() {
    assert_eq!(ev("map(i => if i == 3 then break() else i, range(5))"), "[1, 2]");
    assert_eq!(ev("map(i => if i == 3 then continue() else i, range(5))"), "[1, 2, 4, 5]");
    let looped = |cond: &str, sig: &str| {
        format!("{{ var s = []; for i in range(20) {{ if {cond} then {sig}(); s = append(s, i^2) }}; s }}")
    };
    same("map(i => if i^2 > 50 then break() else i^2, range(20))", &looped("i^2 > 50", "break"));
    same("map(i => if is_odd(i) then continue() else i^2, range(20))", &looped("is_odd(i)", "continue"));
    // the elements after the break are not evaluated: nothing after it is sown
    assert_eq!(ev("reap(map(i => if i == 3 then break() else sow(i), range(6)))"), "[[1, 2], [[1, 2]]]");
    // an inner map is its own loop
    assert_eq!(ev("map(i => map(j => if j > i then break() else j, range(4)), range(3))"), "[[1], [1, 2], [1, 2, 3]]");
    // outside any iteration the value stays
    assert_eq!(ev("break()"), "failure(break)");
    assert_eq!(ev("continue()"), "failure(continue)");
    // other failures stay elements
    assert_eq!(ev("map(i => if i == 2 then failure(boom) else i, range(3))"), "[1, failure(boom), 3]");
}

/// table, sum and product over explicit ranges read the signals; checked against the shorter range written out.
#[test]
fn table_sum_product_read_break_and_continue() {
    same("table(if k^2 > 20 then break() else k, [k, 1, 10])", "range(4)");
    same("table(if is_odd(k) then continue() else k, [k, 1, 10])", "range(2, 10, 2)");
    same("sum(if k^3 > 1000 then break() else k^3, [k, 1, 100])", "sum(k^3, [k, 1, 10])");
    same("sum(if is_odd(k) then continue() else k, [k, 1, 100])", "sum(2*k, [k, 1, 50])");
    same("product(if k == 6 then break() else k, [k, 1, 30])", "factorial(5)");
    same("sum(if k > 6 then break() else k, [k, [5, 1, 9, 2]])", "5 + 1");
    assert_eq!(ev("sum(if k > 2 then break() else k, [k, [5, 1, 9, 2]])"), "0");
    // with several iterators each inner table is its own loop
    assert_eq!(ev("table(if j > i then break() else [i, j], [i, 1, 3], [j, 1, 3])"), "[[[1, 1]], [[2, 1], [2, 2]], [[3, 1], [3, 2], [3, 3]]]");
    // a count iterator
    assert_eq!(ev("{ var c = 0; table({ c = c + 1; if c > 3 then break() else c }, 10) }"), "[1, 2, 3]");
}

/// select, select_first, the quantifiers, scan, fold, nest and nest_while read the signals.
#[test]
fn selections_folds_and_nests_read_break_and_continue() {
    assert_eq!(ev("select(range(10), x => if x == 7 then break() else is_even(x))"), "[2, 4, 6]");
    same("select(range(10), x => if x == 4 then continue() else is_even(x))", "[2, 6, 8, 10]");
    assert_eq!(ev("select_first(range(10), x => if x == 3 then continue() else x > 2)"), "4");
    assert_eq!(ev("select_first(range(10), x => if x == 3 then break() else x > 5, none)"), "none");
    assert_eq!(ev("all_true(range(10), x => if x == 3 then break() else x < 3)"), "true");
    assert_eq!(ev("any_true(range(10), x => if x == 3 then break() else x > 5)"), "false");
    // fold: the sum so far, checked by the shorter fold
    same("fold((a, b) => if b > 4 then break() else a + b, 0, range(10))", "fold(add, 0, range(4))");
    same("fold_list((a, b) => if is_odd(b) then continue() else a + b, 0, range(6))", "fold_list(add, 0, [2, 4, 6])");
    assert_eq!(ev("nest_list(x => if x > 10 then break() else 2*x, 1, 10)"), "[1, 2, 4, 8, 16]");
    same("nest(x => if x > 100 then break() else 3*x, 1, 50)", "3^5");
    same("nest_while(x => if x > 10 then break() else 2*x, 1, x => true)", "2^4");
    assert_eq!(ev("nest_while_list(x => x + 1, 1, x => if x == 4 then break() else true)"), "[1, 2, 3, 4]");
    assert_eq!(ev("fixed_point_list(x => if x > 3 then break() else x + 1, 0)"), "[0, 1, 2, 3, 4]");
    // scan prints up to the break
    assert_eq!(ev("reap(scan(x => if x == 3 then break() else sow(x), range(5)))"), "[null, [[1, 2]]]");
    assert_eq!(ev("reap(scan(x => if x == 3 then continue() else sow(x), range(5)))"), "[null, [[1, 2, 4, 5]]]");
    // the other iterating maps
    assert_eq!(ev("map_indexed((x, i) => if i == 2 then continue() else x, [a, b, c])"), "[a, c]");
    assert_eq!(ev("map_thread((x, y) => if x == 2 then break() else x + y, [[1, 2, 3], [4, 5, 6]])"), "[5]");
    assert_eq!(ev("array(i => if i == 4 then break() else i^2, 6)"), "[1, 4, 9]");
}

/// The parallel iterators give the serial result: the values before the first break, no effect after it, on one
/// thread and on several.
#[test]
fn parallel_iterators_stop_like_serial_ones() {
    for threads in [1, 4] {
        let ev = |s: &str| ev_on(s, threads);
        assert_eq!(ev("length(parallel_map(i => if i == 300 then break() else sin(1.0*i), range(1000)))"), "299");
        assert_eq!(ev("parallel_table(if is_odd(i) then continue() else i, [i, 1, 10])"), "[2, 4, 6, 8, 10]");
        assert_eq!(ev("length(table(if i == 3000 then break() else sin(i*1.0), [i, 1, 100000]))"), "2999");
        assert_eq!(ev("reap(parallel_map(i => if i == 3 then break() else sow(i), range(40)))"), "[[1, 2], [[1, 2]]]");
        assert_eq!(ev("length(map(i => if i == 500 then break() else i, range(10000), parallel: true))"), "499");
    }
}

/// An option read in the condition of a `fn` is its value (the default, or the value given), as in the body; the
/// same definition written with the bound in the body agrees.
#[test]
fn options_in_conditions_are_their_values() {
    let def = "fn ff(x; lim = 3) when x < lim = x\n";
    assert_eq!(ev(&format!("{def}ff(2)")), "2");
    assert_eq!(ev(&format!("{def}ff(5)")), "failure(no_match, ff, [5])");
    assert_eq!(ev(&format!("{def}ff(5, lim: 10)")), "5");
    // checked against the bound written into the body
    let body = "fn gg(x; lim = 3) = if x < lim then x else none\n";
    for (x, opt) in [("2", ""), ("7", ", lim: 10"), ("2.5", ", lim: 2")] {
        let a = ev(&format!("{def}is_failure(ff({x}{opt}))"));
        let b = ev(&format!("{body}gg({x}{opt}) === none"));
        assert_eq!(a, b, "x = {x}{opt}");
    }
    // a later option's default sees an earlier option, in the condition too
    let k2 = "fn k2(x; a = 2, b = a + 1) when x < b = [a, b]\n";
    assert_eq!(ev(&format!("{k2}k2(2)")), "[2, 3]");
    assert_eq!(ev(&format!("{k2}k2(4, a: 5)")), "[5, 6]");
    assert_eq!(ev(&format!("{k2}k2(10, a: 5)")), "failure(no_match, k2, [10, options(a -> 5)])");
    // with an optional parameter missing and an option
    let k3 = "fn k3(x, y = 1; s = 2) when x + y < s = x\n";
    assert_eq!(ev(&format!("{k3}k3(0)")), "0");
    assert_eq!(ev(&format!("{k3}k3(3, 4, s: 100)")), "3");
    assert_eq!(ev(&format!("{k3}is_failure(k3(3))")), "true");
    // a symbolic argument keeps the call unevaluated (the condition is undecided)
    assert_eq!(ev(&format!("{def}ff(y, lim: 10)")), "ff(y, lim: 10)");
}
