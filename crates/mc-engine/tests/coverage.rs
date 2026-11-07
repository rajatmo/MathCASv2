//! Round 14 (package cov-core): behaviours added for the reference examples and the gap queue, checked through the
//! whole engine: number-like values in `max`/`min`, sorting and `range` (G54-G56), datasets (G57, G63), iterated
//! and closed-form sums and products, `lookup`/`key_exists` on rules, `replace_part` with position patterns,
//! `take_largest` in numerical order, and the sort and `sort_by` tie rules.
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let mut bad = vec![];
    for (src, want) in cases {
        let got = eval(&mut e, src);
        if got != *want {
            bad.push(format!("{src}\n   got  {got}\n   want {want}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn values_in_max_min_sort_and_range() {
    check(&[
        ("max(interval([1, 3]), interval([-3, 5]))", "5"),
        ("min(interval([1, 3]), interval([-3, 5]))", "-3"),
        ("max([1, around(-1.1, 0.01)])", "1"),
        ("min([1, around(-1.1, 0.01)])", "around(-1.1, 0.01)"),
        ("max([quantity(3, \"Meters\"), quantity(200, \"Centimeters\")])", "quantity(3, \"Meters\")"),
        ("ordering([quantity(3, \"Feet\"), quantity(9, \"Inches\"), quantity(1, \"Meters\")])", "[2, 1, 3]"),
        (
            "sort([quantity(3, \"Feet\"), quantity(9, \"Inches\"), quantity(1, \"Meters\")])",
            "[quantity(9, \"Inches\"), quantity(3, \"Feet\"), quantity(1, \"Meters\")]",
        ),
        ("take_largest([quantity(1, \"Grams\"), quantity(2, \"Kilograms\")], 1)", "[quantity(2, \"Kilograms\")]"),
        ("range(quantity(3, \"Feet\"))", "[quantity(1, \"Feet\"), quantity(2, \"Feet\"), quantity(3, \"Feet\")]"),
        ("length(range(quantity(3, \"Ounces\"), quantity(1, \"Pounds\")))", "14"),
        // a value that does not compare leaves the call
        ("max([quantity(1, \"Meters\"), 5])", "max([quantity(1, \"Meters\"), 5])"),
    ]);
}

#[test]
fn datasets_of_single_values_and_columns_of_all_rows() {
    check(&[
        ("key_exists(dataset(assoc(\"a\" -> 1, \"b\" -> 2)), \"a\")", "true"),
        ("key_exists(dataset(assoc(\"a\" -> 1, \"b\" -> 2)), \"c\")", "false"),
        ("d = dataset([assoc(\"x\" -> 1, \"y\" -> 2), assoc(\"x\" -> 2, \"y\" -> 5)]); d[all, \"y\"]", "[2, 5]"),
        ("column([1, 2], center)", "column([1, 2], center)"),
    ]);
}

#[test]
fn sums_and_products() {
    check(&[
        ("product(f(i), [i, 1, 4, 2])", "f(1)*f(3)"),
        ("product(f(i, j), [i, 1, 3], [j, 1, i])", "f(1, 1)*f(2, 1)*f(2, 2)*f(3, 1)*f(3, 2)*f(3, 3)"),
        ("sum(f(i), [i, 1, 5, 2])", "f(1) + f(3) + f(5)"),
        ("sum(f(i, j), [i, [a, b]], [j, 1, 2])", "f(a, 1) + f(a, 2) + f(b, 1) + f(b, 2)"),
        ("product(i^2, [i, 1, n])", "factorial(n)^2"),
        ("product(i, [i, n])", "factorial(n)"),
        ("product(a^i, [i, n])", "a^((1/2)*n*(1 + n))"),
        ("product(a^i, [j, m], [i, j])", "a^((1/6)*m*(1 + m)*(2 + m))"),
        ("product(c, [i, 2, n])", "c^(-1 + n)"),
        ("product(i, [i, 3, n])", "pochhammer(3, -2 + n)"),
        ("product(f(i), [i, n])", "product(f(i), [i, n])"),
        // indefinite: S(k + 1) - S(k) = f(k)
        ("sum(i^3, i)", "(1/4)*i^2*(-1 + i)^2"),
        ("sum(i*j, i, j)", "(1/4)*i*j*(-1 + i)*(-1 + j)"),
        ("product(i, i)", "factorial(-1 + i)"),
        ("sum(f(i), i)", "sum(f(i), i)"),
    ]);
}

#[test]
fn lookups_in_rules_and_lists() {
    check(&[
        ("lookup(assoc(a -> 1, b -> 2), [a, b])", "[1, 2]"),
        ("lookup(assoc([0, 0] -> 1), [0, 0])", "1"),
        ("lookup([a -> 1, b -> 2], a)", "1"),
        ("lookup([[a -> 1, b -> 2], [a -> 3]], a)", "[1, 3]"),
        ("lookup([[a -> 1], [a -> 2, b -> 3], [b -> 4]], a, 0)", "[1, 2, 0]"),
        ("key_exists([a -> x, b -> y], a)", "true"),
        ("key_exists(\"a\" -> 1, \"b\")", "false"),
        ("map(key_exists(\"a\"), [\"a\" -> 1, \"b\" -> 2])", "[true, false]"),
    ]);
}

#[test]
fn replace_part_with_position_patterns() {
    check(&[
        ("replace_part([[a, b], [c, d]], [i_, i_] -> x)", "[[x, b], [c, x]]"),
        ("replace_part([a, b, c, d, e], 1 | 3 | 5 -> x)", "[x, b, x, d, x]"),
        ("replace_part([[a, b, c], [d], [f]], [1, _] -> x)", "[[x, x, x], [d], [f]]"),
        ("replace_part([[a, b, c], [d, e], [f]], [_, -1] -> x)", "[[a, b, x], [d, x], [x]]"),
        ("replace_part([[0, 0], [0, 0]], [i_, i_] :> f(i))", "[[f(1), 0], [0, f(2)]]"),
        ("replace_part([[0, 0, 0], [0, 0, 0], [0, 0, 0]], [___, 2, ___] -> x)", "[[0, x, 0], x, [0, x, 0]]"),
        ("replace_part(f(x, y), _ -> g)", "f(g, g)"),
    ]);
}

#[test]
fn ordering_rules() {
    check(&[
        ("l = [exp(pi), 7*pi, pi^e, 23]; take_largest(l, 3)", "[exp(pi), 23, pi^e]"),
        ("take_smallest(l, 2)", "[7*pi, pi^e]"),
        ("take_largest(l -> \"Index\", 2)", "[1, 4]"),
        ("take_largest(l -> [\"Element\", \"Index\"], 1)", "[[exp(pi), 1]]"),
        ("take_largest(l -> all, 1)", "[assoc(\"Element\" -> exp(pi), \"Index\" -> 1)]"),
        ("take_largest([x, 1], 1)", "[x]"),
        // a predicate sort swaps elements for which the predicate does not hold
        ("sort([\"aa\", \"abb\", \"ba\", \"b\", \"aaa\"], (a, b) => string_length(a) < string_length(b))", "[\"b\", \"ba\", \"aa\", \"aaa\", \"abb\"]"),
        // sort_by: equal keys in the canonical order of the elements
        ("sort_by([[1, 2, 3], [2, 3, 1], [3, 1, 2], [2, 2]], last)", "[[2, 3, 1], [2, 2], [3, 1, 2], [1, 2, 3]]"),
    ]);
}

#[test]
fn functional_forms() {
    check(&[
        ("map_apply(f, [[a, b], [c, d]])", "[f(a, b), f(c, d)]"),
        ("map_apply(f, [a, 1, [b, 2]])", "[a, 1, f(b, 2)]"),
        ("map_apply(f)([[1], [2, 3]])", "[f(1), f(2, 3)]"),
        ("comap([f, g], x)", "[f(x), g(x)]"),
        ("comap(f + g, x)", "f(x) + g(x)"),
        ("comap(assoc(\"a\" -> f), x)", "assoc(\"a\" -> f(x))"),
        ("comap_apply([add, mul], [2, 3, 4])", "[9, 24]"),
        ("map_all(f, [a, [b]])", "f([f(a), f([f(b)])])"),
        ("fold_pair_list((y, x) => [p(y, x), q(y, x)], u, [1, 2])", "[p(u, 1), p(q(u, 1), 2)]"),
        ("fold_pair_list(take_drop, [a, b, c, d, e, f], [1, 3, 2])", "[[a], [b, c, d], [e, f]]"),
        ("fold_pair(list, u, [1, 2, 3], last)", "3"),
        ("fold_pair(f, y0, [])", "fold_pair(f, y0, [])"),
        ("construct(f, a, b)", "f(a, b)"),
        ("identity([1])", "[1]"),
        ("hold_complete(1 + 1)", "hold_complete(1 + 1)"),
        ("[check(1 + 1, err), check(1/0, err)]", "[2, err]"),
        ("vv = 3; [value_q(vv), value_q(ww)]", "[true, false]"),
        ("[xor(true, false), xor(true, true), implies(false, x), implies(true, x), true_q(1)]", "[true, false, true, x, false]"),
    ]);
}

#[test]
fn list_tests_and_constructions() {
    check(&[
        ("[is_array([1, 2]), is_array([1, [2]]), is_array([[1, 2], [3]]), is_array([[1, 2], [3, 4]], 2)]", "[true, false, false, true]"),
        ("is_array([[1, 2], [3, 4]], 1 | 2, is_number)", "true"),
        ("subsequences([a, b, c])", "[[], [a], [b], [c], [a, b], [b, c], [a, b, c]]"),
        ("subsequences([a, b, c, d], [2])", "[[a, b], [b, c], [c, d]]"),
        ("[contains_any([b, a], [a, c]), contains_all([b, a], [a, c]), contains_none([b], [a]), contains_only([a, a], [a, b]), contains_exactly([a, b, a], [b, a])]", "[true, false, true, true, true]"),
        ("nearest([1, 2, 4, 8, 16, 32], 20, 3)", "[16, 8, 32]"),
        ("nearest([\"aaaa\", \"abaa\", \"bbbb\"], \"aaba\")", "[\"aaaa\"]"),
        ("pick([[a, b, c], [d, e, f]], [[1, 0, 0], [0, 1, 1]], 1)", "[[a], [e, f]]"),
        ("min_max([3, 1, 4], [1, 2])", "[0, 6]"),
        ("take_drop([a, b, c], up_to(5))", "[[a, b, c], []]"),
        ("power_range(1/2, 1/100, 1/3)", "[1/2, 1/6, 1/18, 1/54]"),
        ("first_position([[a, b], [c, a]], a)", "[1, 1]"),
        ("[count_distinct([1, 2, 1]), delete_elements([1, 2, 3, 1], [1]), position_largest([1, 3, 3])]", "[2, [2, 3], [[2], [3]]]"),
        ("symmetric_difference([1, 2, 3], [2, 4])", "[1, 3, 4]"),
        ("to_character_code(\"AB\")", "[65, 66]"),
        ("character_range(\"a\", \"c\")", "[\"a\", \"b\", \"c\"]"),
        ("partition([a, b, c, d, e], up_to(2))", "[[a, b], [c, d], [e]]"),
        ("replace_at([a, a, a], a -> x, -1)", "[a, a, x]"),
        ("replace_at([[a, a], [a, a]], a -> x, [2, 1])", "[[a, a], [x, a]]"),
        ("take_largest_by([-5, -2, 4, 3, 1, 9, 2, -4], abs, 4)", "[9, -5, 4, -4]"),
        ("take_smallest_by(assoc(a -> 5, b -> -1, c -> 2), abs, 2)", "assoc(b -> -1, c -> 2)"),
        ("[is_ordered([1, 4, 2]), is_ordered([4, 3, 1], greater)]", "[false, true]"),
        ("reverse_sort_by(range(-2, 2), abs)", "[2, -2, 1, -1, 0]"),
        ("block_map(total, range(6), 2)", "[3, 7, 11]"),
        ("replace_repeated(x^2 + y^2, [x -> y, y -> z])", "2*z^2"),
    ]);
}

#[test]
fn parts_groups_and_first_selections() {
    check(&[
        ("m = [[1, 2, 3], [4, 5, 6]]; part(m, all, 2)", "[2, 5]"),
        ("extract(m, [[all, 1], [all, 3]])", "[[1, 4], [3, 6]]"),
        ("group_by([[a, 10], [b, 20], [a, 30]], first -> last)", "assoc(a -> [10, 30], b -> [20])"),
        ("group_by([[a, 10], [b, 20], [a, 30]], first -> last, total)", "assoc(a -> 40, b -> 20)"),
        ("group_by(range(6), [is_prime, is_odd])", "assoc(false -> assoc(true -> [1], false -> [4, 6]), true -> assoc(false -> [2], true -> [3, 5]))"),
        ("select_first([2, 3, 4, 5], is_odd -> \"Index\")", "2"),
        ("select_first([2, 3, 4, 5], is_odd -> [\"Element\", \"Index\"])", "[3, 2]"),
        ("sort([b, a, inf, 0], less)", "[b, a, 0, inf]"),
    ]);
}
