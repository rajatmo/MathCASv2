//! Round 30: listing definitions (`down_values`, `up_values`, `sub_values`) and the core-language
//! fixes of the round, one test per item.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread with a big stack (one worker thread, fresh session); `TIMEOUT` after 60 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn down_values_list_the_rules_as_written() {
    assert_eq!(ev("rule f(1) := 1; rule f(x_) := 2 * f(x - 1); down_values(f)"), "[f(1) :> 1, f(x_) :> 2*f(x - 1)]");
    // a condition and an optional argument are listed as written, once
    assert_eq!(ev("rule p(x_) when x > 0 := x; down_values(p)"), "[p(x_) when x > 0 :> x]");
    assert_eq!(ev("rule oa(x_, optional(y_, 2)) := x + y; down_values(oa)"), "[oa(x_, y_ = 2) :> x + y]");
    // by name, and nothing for a symbol without rules or with built-in code only
    assert_eq!(ev(r#"rule f(1) := 1; down_values("f")"#), "[f(1) :> 1]");
    assert_eq!(ev("down_values(never_defined)"), "[]");
    // rules without pattern variables first, in canonical order
    assert_eq!(ev("rule a(z) := 1; rule a(1) := 2; rule a(x_) := 3; down_values(a)"), "[a(1) :> 2, a(z) :> 1, a(x_) :> 3]");
    // a redefinition replaces the rule
    assert_eq!(ev("rule f(x_) := 1; rule f(y_) := 2; down_values(f)"), "[f(y_) :> 2]");
    // the listed rules apply
    assert_eq!(ev("rule f(1) := 1; rule f(x_) := 2 * f(x - 1); rewrite(quote(f(5)), down_values(f))"), "quote(2*f(5 - 1))");
}

#[test]
fn up_values_and_sub_values() {
    assert_eq!(ev("@on(g) rule area(g(a_)) := a^2; [up_values(g), down_values(area)]"), "[[area(g(a_)) :> a^2], []]");
    assert_eq!(ev("@on(x1) rule f(x1) = 1; up_values(x1)"), "[f(x1) :> 1]");
    assert_eq!(ev("rule k(x_)(y_) := x + y; [sub_values(k), down_values(k), k(1)(2)]"), "[[k(x_)(y_) :> x + y], [], 3]");
}

#[test]
fn quantity_arrays_are_lists_of_quantities() {
    assert_eq!(ev(r#"quantity_array([1, 2], "Meters")"#), r#"[quantity(1, "Meters"), quantity(2, "Meters")]"#);
    assert_eq!(
        ev(r#"quantity_array([[1, 2], [3, 4]], ["Meters", "Seconds"])"#),
        r#"[[quantity(1, "Meters"), quantity(2, "Seconds")], [quantity(3, "Meters"), quantity(4, "Seconds")]]"#
    );
    assert_eq!(ev(r#"total(quantity_array([1, 2, 3], "Meters"))"#), r#"quantity(6, "Meters")"#);
}

#[test]
fn subscript_is_an_inert_indexed_name() {
    assert_eq!(ev("subscript(a, 1 + 2)"), "subscript(a, 3)");
    assert_eq!(ev("solve(subscript(x, 1) + 2 == 0, subscript(x, 1))"), "[[subscript(x, 1) -> -2]]");
}

#[test]
fn longest_and_shortest_in_term_patterns() {
    let r = |p: &str| ev(&format!("rewrite([a, b, c, d, e, f, g], {p} :> [x, y])"));
    assert_eq!(r("[x__, longest(y__)]"), "[[a], [b, c, d, e, f, g]]");
    assert_eq!(r("[longest(x__), y__]"), "[[a, b, c, d, e, f], [g]]");
    assert_eq!(r("[x__, shortest(y__)]"), "[[a, b, c, d, e, f], [g]]");
    // priorities: the higher first
    assert_eq!(r("[longest(x__, 1), longest(y__, 2)]"), "[[a], [b, c, d, e, f, g]]");
    assert_eq!(r("[shortest(x__, 1), shortest(y__, 2)]"), "[[a, b, c, d, e, f], [g]]");
    // a typed marked run between unmarked ones
    assert_eq!(ev("rewrite([x, 3, 5, 7, y, 1, 2, 3, 4, z], [a___, longest(b__: Int), c___] :> b)"), "[1, 2, 3, 4]");
    assert_eq!(ev("rewrite([1, 2, 3, 4, 5], [x__, longest(repeated(_: Int, [1, 2]))] :> [x])"), "[[1, 2, 3]]");
}
