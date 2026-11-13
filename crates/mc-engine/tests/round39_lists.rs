//! Round 39: argument forms of list and structure functions: the plane cross vector, heads through a function,
//! counted and levelled positions, ragged transposes, stepped subset numbers, whole-list subset maps, numerical
//! orders of associations, replacement at all levels, set operations under a test, key tests, alternating
//! geometric ranges, cube roots of uncertain numbers.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn plane_cross_vector() {
    assert_eq!(ev("cross([1, sqrt(3)])"), "[-3^(1/2), 1]");
    assert_eq!(ev("cross([x, y])"), "[-y, x]");
    // orthogonal and of the same length; twice is the negative
    assert_eq!(ev("dot([x, y], cross([x, y]))"), "0");
    assert_eq!(ev("v = [3, -4]; [norm(cross(v)), cross(cross(v)) == -v]"), "[5, true]");
    // the space cross product is unchanged
    assert_eq!(ev("cross([1, 2, 3], [4, 5, 6])"), "[-3, 6, -3]");
}

#[test]
fn heads_positions_and_transposes() {
    assert_eq!(ev("head([1, 2, 3], f)"), "f(list)");
    assert_eq!(ev("head(g(x)(y), h)"), "h(g(x))");
    assert_eq!(ev("position([a, b, a, a, b, c, b, a, b], b, 1, 2)"), "[[2], [5]]");
    // the first n of all positions
    assert_eq!(ev("position([a, b, a, b], b, [1], 5) == position([a, b, a, b], b, [1])"), "true");
    assert_eq!(ev("first_position([[1, 2], [2, 3], [3, 1]], 3, missing(\"NotFound\"), [1])"), "missing(\"NotFound\")");
    assert_eq!(ev("first_position([[1, 2], [2, 3], [3, 1]], 3, missing(\"NotFound\"), [2])"), "[2, 2]");
    assert_eq!(ev("flatten([[1], [2, 3], [3, 4, 5], [4, 5, 6, 7]], [[2], [1]])"), "[[1, 2, 3, 4], [3, 4, 5], [5, 6], [7]]");
    // of lists of one length the ragged transpose is the transpose
    assert_eq!(ev("m = [[1, 2, 3], [4, 5, 6]]; flatten(m, [[2], [1]]) == transpose(m)"), "true");
}

#[test]
fn subsets_and_subset_maps() {
    assert_eq!(ev("subsets([a, b, c, d], all, [15, 1, -2])"), "[[b, c, d], [a, b, d], [c, d], [b, c], [a, c], [d], [b], []]");
    // a step of 1 is the span form
    assert_eq!(ev("subsets([a, b, c, d], all, [3, 7, 1]) == subsets([a, b, c, d], all, [3, 7])"), "true");
    assert_eq!(ev("subsets([a, b, c], 2, [3, 1, -1]) == reverse(take(subsets([a, b, c], 2), 3))"), "true");
    assert_eq!(ev("subset_map(accumulate, [a, b, c], all)"), "[a, a + b, a + b + c]");
    assert_eq!(ev("subset_map(reverse, [1, 2, 3, 4], all) == reverse([1, 2, 3, 4])"), "true");
}

#[test]
fn orders_and_replacements() {
    assert_eq!(ev("numerical_sort(assoc(a -> 0, b -> 1, c -> inf, d -> -inf))"), "assoc(d -> -inf, a -> 0, b -> 1, c -> inf)");
    // the values in the order numerical_sort gives them
    assert_eq!(ev("s = assoc(p -> pi, q -> e, r -> 3); values(numerical_sort(s)) == numerical_sort(values(s))"), "true");
    assert_eq!(ev("replace(f(f(f(f(x)))), f(x_) :> g(x), all)"), "g(g(g(g(x))))");
    assert_eq!(ev("replace(f(1, 2, f(3)), f -> g, all)"), "f(1, 2, f(3))");
    assert_eq!(ev("replace([1, [2, [3]]], n_: Int :> n^2, all) == rewrite([1, [2, [3]]], n_: Int :> n^2)"), "true");
}

#[test]
fn set_operations_and_key_tests() {
    assert_eq!(ev("symmetric_difference([1, 2, 3], [2., 3., 4.], same_test: equal)"), "[1, 4.0]");
    assert_eq!(ev("symmetric_difference()"), "[]");
    // with the identity as the test it is the plain operation
    assert_eq!(ev("symmetric_difference([1, 2, 5], [2, 3], [3, 7], same_test: (p, q) => p === q) == symmetric_difference([1, 2, 5], [2, 3], [3, 7])"), "true");
    assert_eq!(ev("symmetric_difference([1, 2, 3], [11, 12], same_test: (a, b) => mod(a, 10) == mod(b, 10))"), "[3]");
    assert_eq!(ev("hamming_distance(\"abc\", \"AbC\", ignore_case: true)"), "0");
    assert_eq!(ev("hamming_distance(\"abc\", \"AbC\")"), "2");
    assert_eq!(ev("hamming_distance(\"abcd\", \"ABxd\", ignore_case: true) == hamming_distance(\"abcd\", \"abxd\")"), "true");
    assert_eq!(
        ev("key_complement([assoc(\"a\" -> 1, \"A\" -> 2, \"b\" -> 3), assoc(\"a\" -> 4)], same_test: &(to_lower(&1) == to_lower(&2)))"),
        "assoc(\"b\" -> 3)"
    );
    assert_eq!(ev("key_complement([assoc(\"a\" -> 1, \"A\" -> 2, \"b\" -> 3), assoc(\"a\" -> 4)])"), "assoc(\"A\" -> 2, \"b\" -> 3)");
    assert_eq!(ev("key_complement([assoc(1 -> x, 2 -> y, 3 -> z), assoc(4 -> w)], same_test: (p, q) => mod(p, 3) == mod(q, 3))"), "assoc(2 -> y, 3 -> z)");
}

#[test]
fn alternating_ranges_and_uncertain_roots() {
    assert_eq!(ev("power_range(10, 10000, -5)"), "[10, -50, 250, -1250, 6250]");
    assert_eq!(ev("power_range(1, 1/1000, -1/10)"), "[1, -1/10, 1/100, -1/1000]");
    // the sizes are those of the positive ratio
    assert_eq!(ev("abs(power_range(3, 500, -2)) == power_range(3, 500, 2)"), "true");
    assert_eq!(ev("power_range(1, 10, -1)"), "failure(argument, power_range, 3)");
    // the cube root carried through the uncertainty: x^(1/3) and its slope x^(-2/3)/3 times the spread
    assert_eq!(ev("c = cube_root(around(2/3, 0.001)); abs(c[1] - (2/3)^(1/3)) < 10^-15 and abs(c[2] - 0.001*(2/3)^(-2/3)/3) < 10^-15"), "true");
    assert_eq!(ev("c = cube_root(around(-8., 0.03)); abs(c[1] + 2) < 10^-15 and abs(c[2] - 0.03/12) < 10^-15"), "true");
    assert_eq!(ev("cube_root(8)"), "2");
}
