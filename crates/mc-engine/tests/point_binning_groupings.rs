//! Points of several coordinates binned in grids, unit bins by default; groupings with heads written as
//! options, with `construct`, and wrapped in a head. Counts are checked against the lists of the same bins and
//! against cells counted by hand; tree counts against the Fuss-Catalan numbers.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn bins_of_points_in_the_plane() {
    let mut e = CoreEngine::new();
    ev(&mut e, "data = table(mod(i^2, [3, 5]), [i, 100])");
    assert_eq!(ev(&mut e, "bin_counts(data, 1, 1)"), "[[0, 0, 0, 0, 0, 0], [0, 6, 14, 0, 0, 13], [0, 14, 26, 0, 0, 27]]");
    // the lists of the same bins have the counts as their lengths, and every point is in one of them
    assert_eq!(ev(&mut e, "map(c => map(length, c), bin_lists(data, 1, 1)) == bin_counts(data, 1, 1)"), "true");
    assert_eq!(ev(&mut e, "total(flatten(bin_counts(data, 1, 1)))"), "100");
    // explicit edges in each coordinate, counted by hand: (-0.2, 0.1) and (0.5, 0.5) in the first y bin, (7, 0)
    // outside the x edges
    assert_eq!(ev(&mut e, "bin_counts([[0.5, 0.5], [-0.2, 0.1], [7, 0]], [[-1, 0, 1]], [[0, 2, 4]])"), "[[1, 0], [1, 0]]");
    assert_eq!(
        ev(&mut e, "bin_lists([[0.5, 0.5], [-0.2, 0.1], [7, 0]], [-1, 1, 0.5], [[-1, 0, 1]])"),
        "[[[], []], [[], [[-0.2, 0.1]]], [[], []], [[], [[0.5, 0.5]]]]"
    );
}

#[test]
fn unit_bins_by_default() {
    let mut e = CoreEngine::new();
    ev(&mut e, "sp = sparse_array([[1, 1] -> 5, [3, 2] -> 4], [100, 2])");
    assert_eq!(
        ev(&mut e, "bin_counts(sp)"),
        "[[0, 0, 0, 0, 0, 0], [0, 98, 0, 0, 0, 1], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 1, 0, 0, 0, 0]]"
    );
    // the same as width 1 in each coordinate; three coordinates
    assert_eq!(ev(&mut e, "bin_counts(sp) == bin_counts(sp, 1, 1)"), "true");
    assert_eq!(
        ev(&mut e, "bin_counts([[1, 2, 3], [1, 2, 3], [0, 0, 0]])"),
        "[[[0, 0, 0, 0, 0], [0, 0, 0, 0, 0], [0, 0, 0, 0, 0], [0, 0, 0, 0, 0]], [[0, 0, 0, 0, 0], [0, 1, 0, 0, 0], [0, 0, 0, 0, 0], [0, 0, 0, 0, 0]], [[0, 0, 0, 0, 0], [0, 0, 0, 0, 0], [0, 0, 0, 0, 0], [0, 0, 0, 0, 2]]]"
    );
    // quantities: bins of one unit of the first datum
    ev(&mut e, "q = map(v => quantity(v, \"Grams\"), [-1.541, -0.849, -0.477, -0.133, -0.11, -0.054, 0.226, 0.259, 1.4, 1.661])");
    assert_eq!(ev(&mut e, "bin_counts(q)"), "[1, 5, 2, 2]");
    assert_eq!(ev(&mut e, "map(length, bin_lists(q)) == bin_counts(q)"), "true");
    assert_eq!(ev(&mut e, "bin_counts([0.5, 1.5, 1.7, 3.2])"), "[1, 2, 0, 1]");
}

#[test]
fn groupings_with_heads_as_options() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "groupings([a, b, c], circle_times: 2)"), "[circle_times(circle_times(a, b), c), circle_times(a, circle_times(b, c))]");
    assert_eq!(ev(&mut e, "groupings([a], square: 1)"), "[a, square(a)]");
    assert_eq!(ev(&mut e, "groupings(table(s, 4), construct: 2)"), "[s(s)(s)(s), s(s(s)(s)), s(s(s))(s), s(s(s(s))), s(s)(s(s))]");
    assert_eq!(ev(&mut e, "groupings(table(s, 5), construct: 3)"), "[s(s, s)(s, s), s(s(s, s), s), s(s, s(s, s))]");
    // ternary trees: none with 6 leaves; with 7 leaves (3 calls) the Fuss-Catalan number binomial(9, 3)/7 = 12
    assert_eq!(ev(&mut e, "groupings(table(s, 6), construct: 3)"), "[]");
    assert_eq!(ev(&mut e, "length(groupings(table(t, 7), construct: 3)) == binomial(9, 3)/7"), "true");
}

#[test]
fn groupings_wrapped_in_a_head() {
    let mut e = CoreEngine::new();
    assert_eq!(
        ev(&mut e, "groupings([1, 1, 1], [add -> 2, mul -> 2], quote)"),
        "[quote((1 + 1) + 1), quote(1 + (1 + 1)), quote(1*1 + 1), quote(1 + 1*1), quote((1 + 1)*1), quote(1*(1 + 1)), quote((1*1)*1), quote(1*(1*1))]"
    );
    // trees with the same heads are together: the multiset of heads orders before the shape
    assert_eq!(
        ev(&mut e, "groupings(table(1, 7), [add -> 2, mul -> 2], quote)[905]"),
        "quote((((1 + 1) + 1) + 1)*((1 + 1) + 1))"
    );
    // the unwrapped trees evaluate to the values of the wrapped ones; 132*2^6 trees of seven leaves
    assert_eq!(ev(&mut e, "groupings([1, 1, 1], [add -> 2, mul -> 2])"), "[3, 3, 2, 2, 2, 2, 1, 1]");
    assert_eq!(ev(&mut e, "length(groupings(table(1, 7), [add -> 2, mul -> 2], quote))"), "8448");
    assert_eq!(ev(&mut e, "groupings([a, b], f -> 2, g)"), "[g(f(a, b))]");
}
