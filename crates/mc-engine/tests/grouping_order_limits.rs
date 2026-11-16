//! The order of the trees of `groupings` within one multiset of heads (canonical forms in the order of
//! their prefix words, then the orders of the parts), and the limit `[h -> k, m]` on the calls of a head. Positions
//! are checked against a separate enumeration of the canonical forms and their orders; counts against trees
//! counted by their heads.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn positions_of_a_value_among_sums_and_products() {
    let mut e = CoreEngine::new();
    // the trees of seven ones whose value is 12
    assert_eq!(
        ev(&mut e, "flatten(position(groupings(table(1, 7), [add -> 2, mul -> 2]), 12))"),
        "[905, 906, 907, 908, 909, 910, 911, 912, 913, 914, 915, 916, 917, 918, 919, 920, 921, 922, 923, 924, 2877, 2878, 2879, 2880, 2881, 2882, 2883, 2884, 2901, 2902, 2903, 2904]"
    );
    // six ones with value 8
    assert_eq!(
        ev(&mut e, "flatten(position(groupings(table(1, 6), [add -> 2, mul -> 2]), 8))"),
        "[239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 671, 672]"
    );
    // the count is unchanged: Catalan(6) shapes times 2^6 head choices
    assert_eq!(ev(&mut e, "length(groupings(table(1, 7), [add -> 2, mul -> 2]))"), "8448");
}

#[test]
fn heads_of_two_and_three_arguments() {
    let mut e = CoreEngine::new();
    assert_eq!(
        ev(&mut e, "take(groupings(5, [f -> 2, g -> 3]), 9)"),
        "[g(g(1, 2, 3), 4, 5), g(1, g(2, 3, 4), 5), g(1, 2, g(3, 4, 5)), f(f(g(1, 2, 3), 4), 5), f(1, f(g(2, 3, 4), 5)), f(f(1, g(2, 3, 4)), 5), f(1, f(2, g(3, 4, 5))), f(f(1, 2), g(3, 4, 5)), f(g(1, 2, 3), f(4, 5))]"
    );
    assert_eq!(ev(&mut e, "length(groupings(5, [f -> 2, g -> 3]))"), "38");
    // the earlier documented orders stay
    assert_eq!(
        ev(&mut e, "groupings([a, b, c, d], [foo -> 3, bar -> 2])"),
        "[foo(bar(a, b), c, d), foo(a, bar(b, c), d), foo(a, b, bar(c, d)), bar(foo(a, b, c), d), bar(a, foo(b, c, d)), bar(bar(bar(a, b), c), d), bar(a, bar(bar(b, c), d)), bar(bar(a, bar(b, c)), d), bar(a, bar(b, bar(c, d))), bar(bar(a, b), bar(c, d))]"
    );
    assert_eq!(
        ev(&mut e, "groupings([a, a, a, b], [h1 -> [2, orderless], h2 -> 3])"),
        "[h1(h2(a, a, a), b), h2(h1(a, a), a, b), h2(a, h1(a, a), b), h2(a, a, h1(a, b)), h1(h1(h1(a, a), a), b), h1(h1(a, a), h1(a, b))]"
    );
}

#[test]
fn limits_on_the_calls_of_a_head() {
    let mut e = CoreEngine::new();
    // at most two calls of each: one f and two g (f + 2 g = 5 for six leaves)
    assert_eq!(ev(&mut e, "length(groupings(table(o, 6), [[f -> 2, 2], [g -> 3, 2]]))"), "28");
    assert_eq!(
        ev(&mut e, "groupings(table(o, 6), [[f -> 2, 2], [g -> 3, 2]]) == take(groupings(table(o, 6), [f -> 2, g -> 3]), 28)"),
        "true"
    );
    // at most one f among the trees of seven leaves: f + 2 g = 6 leaves no odd f, so only the twelve trees of three g
    assert_eq!(ev(&mut e, "length(groupings(7, [[f -> 2, 1], g -> 3]))"), "12");
    assert_eq!(ev(&mut e, "length(groupings(7, [f -> 2, g -> 3]))"), "654");
    // with a styled head
    assert_eq!(ev(&mut e, "length(groupings(table(o, 6), [[style(f, blue) -> 2, 2], [style(g, red) -> 3, 2]]))"), "28");
}
