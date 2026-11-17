//! Monomial orders of `coefficient_rules` / `monomial_list`, a single generator of `polynomial_reduction`, blocks of
//! arrays with another head, partitions overhanging by whole runs, repeats with a test or in an association,
//! `inner` on vectors with another head, replacing inside associations, the `all` level, and the distance matrix
//! of nominal and mixed features. Each answer is checked by a second route.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn monomial_orders() {
    // degree reverse lexicographic: x y^2 (degree 3, smaller z power ranks higher) before x^2 z
    assert_eq!(ev("coefficient_rules(a*x*y^2 + b*x^2*z, [x, y, z], \"DegreeReverseLexicographic\")"), "[[1, 2, 0] -> a, [2, 0, 1] -> b]");
    // the weight matrix of that order gives the same list
    assert_eq!(
        ev("coefficient_rules(a*x*y^2 + b*x^2*z, [x, y, z], [[1, 1, 1], [0, 0, -1], [0, -1, 0]])"),
        "[[1, 2, 0] -> a, [2, 0, 1] -> b]"
    );
    // lexicographic is the default order; degree lexicographic ranks x^2 y^2 (degree 4) above x^3
    assert_eq!(ev("monomial_list(x^2*y^2 + x^3, [x, y], \"Lexicographic\") === monomial_list(x^2*y^2 + x^3, [x, y])"), "true");
    assert_eq!(ev("monomial_list(x^2*y^2 + x^3, [x, y], \"DegreeLexicographic\")"), "[x^2*y^2, x^3]");
    // lexicographic puts x^3 first; degree orders put the degree 4 term first; the sum of the terms is the input
    assert_eq!(ev("monomial_list(x^2*y^2 + x^3, [x, y])"), "[x^3, x^2*y^2]");
    assert_eq!(ev("expand(total(monomial_list(x^2*y + x*y^2 + x^3 + 1, [x, y, z], \"DegreeReverseLexicographic\")) - (x^2*y + x*y^2 + x^3 + 1))"), "0");
    // an order this function does not know leaves the call as it is
    assert_eq!(ev("head(monomial_list(x + y, [x, y], \"NoSuchOrder\"))"), "monomial_list");
}

#[test]
fn single_generator_reduction() {
    // a generator free of the variables generates the whole ring: every remainder is 0
    assert_eq!(ev("polynomial_reduction(x^4 + y^4 + z^4, gb, [x, y, z])"), "0");
    // a single generator is the list of it
    assert_eq!(ev("polynomial_reduction(x^3 + 1, x - 1, [x]) === polynomial_reduction(x^3 + 1, [x - 1], [x])"), "true");
    assert_eq!(ev("polynomial_reduction(x^3 + 1, x - 1, [x])"), "2");
}

#[test]
fn blocks_of_arrays_with_another_head() {
    assert_eq!(
        ev("block_map(f, h(h(1, 2, 3), h(4, 5, 6)), [2, 2], 1)"),
        "[[f(h(h(1, 2), h(4, 5))), f(h(h(2, 3), h(5, 6)))]]"
    );
    // the same blocks of the list form, with the head put back
    assert_eq!(
        ev("block_map(g, [[1, 2, 3], [4, 5, 6]], [2, 2], 1)"),
        "[[g([[1, 2], [4, 5]]), g([[2, 3], [5, 6]])]]"
    );
}

#[test]
fn partition_overhangs_by_whole_runs() {
    assert_eq!(
        ev("partition([a, b, c, d, e, f, g], 3, 3, [5, -5], x)"),
        "[[x, x, x], [x, a, b], [c, d, e], [f, g, x], [x, x, x]]"
    );
    // a place beyond the run is no longer refused, and the elements keep their order: dropping the padding gives the input
    assert_eq!(ev("select(flatten(partition([a, b, c, d, e, f, g], 3, 3, [5, -5], x)), t => t !== x)"), "[a, b, c, d, e, f, g]");
    assert_eq!(ev("head(partition([a, b, c], 3, 3, [0, 1], x))"), "partition");
}

#[test]
fn repeats_with_test_and_in_associations() {
    assert_eq!(ev("find_transient_repeat([1, 2, -3, 4, 3, -4, -3, 4], 3, same_test: &(abs(&1) == abs(&2)))"), "[[1, 2], [-3, 4]]");
    assert_eq!(ev("find_transient_repeat([1, 2, 3, 4.1, 3.2, 4.5, 3., 4.4], 3, same_test: &(floor(&1) == floor(&2)))"), "[[1, 2], [3, 4.1]]");
    // without a test the comparison is identity
    assert_eq!(ev("find_transient_repeat([1, 2, 3, 1, 2, 3, 1, 2, 3], 3)"), "[[], [1, 2, 3]]");
    assert_eq!(ev("find_repeat(assoc(a -> 1, b -> 2, c -> 1, d -> 2, e -> 1, f -> 2))"), "assoc(a -> 1, b -> 2)");
    // the block of the values, by the keys it starts with
    assert_eq!(ev("values(find_repeat(assoc(a -> 1, b -> 2, c -> 1, d -> 2, e -> 1, f -> 2))) === find_repeat([1, 2, 1, 2, 1, 2])"), "true");
}

#[test]
fn inner_of_vectors_with_another_head() {
    assert_eq!(ev("inner(s, f(1), f(2), t)"), "t(s(1, 2))");
    assert_eq!(ev("inner(mul, f(1, 2, 3), f(4, 5, 6)) === inner(mul, [1, 2, 3], [4, 5, 6])"), "true");
    assert_eq!(ev("inner(mul, [1, 2], [[1, 2], [3, 4]], add, 1) === dot([1, 2], [[1, 2], [3, 4]]) and inner(f, [[a, b], [c, d]], [[u, v], [w, x]], g, 1) === inner(f, transpose([[a, b], [c, d]]), [[u, v], [w, x]], g)"), "true");
}

#[test]
fn replace_at_inside_associations() {
    assert_eq!(ev("replace_at(assoc(\"a\" -> a, \"b\" -> a), a -> xx, \"a\")"), "assoc(\"a\" -> xx, \"b\" -> a)");
    assert_eq!(
        ev("replace_at(assoc(\"a\" -> [a, a], \"b\" -> [a, a]), a -> xx, [\"a\", all])"),
        "assoc(\"a\" -> [xx, xx], \"b\" -> [a, a])"
    );
    assert_eq!(ev("replace_at(assoc(k -> [1, 2]), 2 -> 9, [k, 2])"), "assoc(k -> [1, 9])");
}

#[test]
fn all_levels() {
    // `all` is levels 0 to infinity: the whole term and every part
    assert_eq!(ev("cases([a, [1, b], 2], x_ when is_number(x), all)"), "[1, 2]");
    assert_eq!(ev("cases([a, [1, b], 2], x_ when is_number(x), all) === cases([a, [1, b], 2], x_ when is_number(x), [0, inf])"), "true");
    assert_eq!(ev("map(f, [1, [2]], all)"), "f([f(1), f([f(2)])])");
}

#[test]
fn distance_matrix_of_nominal_and_mixed_features() {
    // columns in an association are the rows transposed; the nominal feature counts 1 for a mismatch: 346 + 1 = 347
    let d = ev("distance_matrix(assoc(\"age\" -> [32, 41, 30, 21], \"height\" -> [160, 183, 123, 175], \"gender\" -> [\"f\", \"f\", \"f\", \"m\"]))");
    assert_eq!(d, "[[0, 610^(1/2), 1373^(1/2), 347^(1/2)], [610^(1/2), 0, 61, 465^(1/2)], [1373^(1/2), 61, 0, 2786^(1/2)], [347^(1/2), 465^(1/2), 2786^(1/2), 0]]");
    assert_eq!(ev("distance_matrix(dataset(assoc(\"a\" -> [1, 2], \"b\" -> [\"x\", \"y\"]))) === distance_matrix([[1, \"x\"], [2, \"y\"]])"), "true");
    // only nominal features: the number of mismatches
    assert_eq!(
        ev("distance_matrix([[1, \"A\"], [2, \"A\"], [2, \"B\"], [1, \"B\"]], feature_types: assoc(1 -> \"Nominal\"))"),
        "[[0, 1, 2, 1], [1, 0, 1, 2], [2, 1, 0, 1], [1, 2, 1, 0]]"
    );
}

#[test]
fn structured_target_gives_the_ordinary_matrix() {
    assert_eq!(ev("lower_triangularize([[1, 2, 3], [4, 5, 6], [7, 8, 9]], target_structure: structured)"), "[[1, 0, 0], [4, 5, 0], [7, 8, 9]]");
    assert_eq!(ev("upper_triangularize([[1, 2, 3], [4, 5, 6], [7, 8, 9]], target_structure: structured)"), "[[1, 2, 3], [0, 5, 6], [0, 0, 9]]");
    assert_eq!(ev("toeplitz_matrix(4, target_structure: structured) === toeplitz_matrix(4)"), "true");
    assert_eq!(ev("hilbert_matrix(4, target_structure: structured) === hilbert_matrix(4)"), "true");
    assert_eq!(ev("rotation_matrix(pi/2, target_structure: structured) === rotation_matrix(pi/2)"), "true");
    assert_eq!(ev("identity_matrix(3, target_structure: structured) === identity_matrix(3)"), "true");
    assert_eq!(ev("diagonal_matrix([1, 2], target_structure: structured) === diagonal_matrix([1, 2])"), "true");
    // the factor is the one without the option: the product of its transpose and itself is the input
    assert_eq!(ev("let u = cholesky([[4, 2], [2, 3]], target_structure: structured); dot(transpose(u), u)"), "[[4, 2], [2, 3]]");
}
