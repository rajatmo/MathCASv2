//! Round 18 phase 9B: subset matching, `quote` and `unquote`, wrapped parts, ordering functions in
//! `sort`, properties of `take_largest_by`, `array_depth` heads (natives, on source text), and the prelude's new
//! functions (through the whole engine).
mod common;
use common::{check, Env};
use mc_api::{Engine, StopFlag};

#[test]
fn subsets_match_in_any_order() {
    check(
        &mut Env::new(),
        &[
            ("subset_cases([1, 2, 3, a, a, b, c], [a, _: Int])", "[[a, 1], [a, 2]]"),
            ("subset_cases([1, 2, 3, a, a, b, c], [a, i_: Int] :> f(i))", "[f(1), f(2)]"),
            ("subset_cases([a, b, c, 1, 2], [_: Sym, _: Int], overlaps: true)", "[[a, 1], [a, 2], [b, 1], [b, 2], [c, 1], [c, 2]]"),
            ("subset_cases([a, b, c, 1, 2], [_: Sym, _: Int])", "[[a, 1], [b, 2]]"),
            // runs: the largest subsets first
            ("subset_cases([1, a, 2, b, 3], [repeated(_: Int)], overlaps: true)", "[[1, 2, 3], [1, 2], [1, 3], [2, 3], [1], [2], [3]]"),
            ("subset_cases([a, a, b, b, b, c, c, d], [a] | [__])", "[[a], [a], [b, b, b, c, c, d]]"),
            ("subset_cases([1, 2, 3], [x_, y_] when x > y)", "[[2, 1]]"),
            ("subset_position([1, 2, 3, 4, 1, 3], [1, 3])", "[[1, 3], [1, 6], [5, 3], [5, 6]]"),
            ("subset_position([1, 2, 3, 4], [_, _])", "[[1, 2], [1, 3], [1, 4], [2, 3], [2, 4], [3, 4]]"),
            ("subset_position([1, 2, 3, 4], [_, 1])", "[[2, 1], [3, 1], [4, 1]]"),
            ("subset_position([a, b, c, 1, 2], [_: Sym, _: Int], overlaps: false)", "[[1, 4], [2, 5]]"),
            ("subset_replace([1, 2, 3, 4, 5, 6], [2, 4] -> x)", "[1, x, 3, 5, 6]"),
            ("subset_replace([1, 2, 3, a, a, b, c], [a, x_: Int] :> f(x))", "[3, f(1), f(2), b, c]"),
            ("subset_replace([a, b, a, b, a, b, a, b], [a, a] -> x, 1)", "[x, b, b, a, b, a, b]"),
            ("subset_cases(xs, [a])", "subset_cases(xs, [a])"),
            ("subset_cases([1, 2], 3)", "failure(argument, subset_cases, 2)"),
        ],
    );
}

#[test]
fn quote_holds_and_unquote_releases() {
    check(
        &mut Env::new(),
        &[
            ("quote(1 + 2)", "quote(1 + 2)"),
            ("unquote(quote(1 + 2))", "3"),
            ("unquote(f(quote(1 + 1), quote(a, b)))", "f(2, a, b)"),
            ("unquote(quote(quote(1 + 1)))", "quote(1 + 1)"),
            ("unquote(hold_complete(2 * 3))", "6"),
            ("length(quote(f(1 + 1, 2 + 2)))", "1"),
            ("extract(quote([1, 1 + 1, pi]), [1, 2], quote)", "quote(1 + 1)"),
            ("extract(hold_complete([1, 1 + 1, pi]), [[1, 2], [1, 3]], hold_complete)", "[hold_complete(1 + 1), hold_complete(pi)]"),
        ],
    );
}

#[test]
fn ordering_functions_and_properties() {
    check(
        &mut Env::new(),
        &[
            // an ordering function's -1 puts the later element first; 1 and 0 keep the order
            ("sort([1, 3, 2], (a, b) => if a < b then -1 else 1)", "[3, 2, 1]"),
            ("ordered([3, 2, 2, 1], (a, b) => if a < b then -1 else if a == b then 0 else 1)", "true"),
            ("take_largest_by([-5, 2, 3, 4] -> \"Index\", x => x * x, 2)", "[1, 4]"),
            ("take_smallest_by([-5, 2, 3, 4] -> [\"Element\", \"Value\"], x => x * x, 2)", "[[2, 4], [3, 9]]"),
            ("take_largest_by([-5, 2] -> all, x => x * x, 1)", "[assoc(\"Element\" -> -5, \"Index\" -> 1, \"Value\" -> 25)]"),
            ("array_depth(f(f(1, 2, 3), f(4, 5, 6)), allowed_heads: list)", "0"),
            ("array_depth(f(g(1, 2), g(3, 4)), allowed_heads: all)", "2"),
            ("array_depth([[1, 2], [3, 4]], allowed_heads: \"ListLike\")", "2"),
        ],
    );
}

/// The prelude's functions of this round, through the whole engine.
#[test]
fn prelude_functions() {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    let cases: &[(&str, &str)] = &[
        ("subtract(10, 3)", "7"),
        ("select(range(6), greater_than(4))", "[5, 6]"),
        ("select(range(6), less_equal_than(2))", "[1, 2]"),
        ("[order(1, 2), order(b, a), order(x, x)]", "[1, -1, 0]"),
        ("[numerical_order(3/2, 1), numerical_order(3/2, 1.5), numerical_order(pi, e^2)]", "[-1, 0, 1]"),
        ("lexicographic_sort([[b, c], [b, b, a, a], [a, b, c], [a, b]])", "[[a, b], [a, b, c], [b, b, a, a], [b, c]]"),
        ("lexicographic_sort([\"A2\", \"a1\", \"A1\", \"a2\"])", "[\"a1\", \"a2\", \"A1\", \"A2\"]"),
        ("alphabetic_sort([\"cat\", \"fish\", \"catfish\", \"Cat\"])", "[\"cat\", \"Cat\", \"catfish\", \"fish\"]"),
        ("reverse_sort([\"cat\", \"fish\", \"catfish\", \"Cat\"], alphabetic_order(\"English\"))", "[\"fish\", \"catfish\", \"Cat\", \"cat\"]"),
        ("length(alphabet())", "26"),
        ("ordering_by(range(-5, 5), abs, -3)", "[10, 1, 11]"),
        ("split_by(tuples([1, 2], 2), [first, last])", "[[[[1, 1]], [[1, 2]]], [[[2, 1]], [[2, 2]]]]"),
        ("count_distinct_by([1, 2, 3, -2, -3], abs)", "3"),
        ("delete_adjacent_duplicates([1, 3, 5, 4, 3, 4, 1, 9, 9, 7, 8], greater)", "[1, 3, 5, 4, 9, 9, 8]"),
        ("delete_adjacent_duplicates(f(x, x, y, y, x))", "f(x, y, x)"),
        ("key_sort_by(assoc(2 -> a, 3 -> b, 1 -> c), k => mod(k, 3))", "assoc(3 -> b, 1 -> c, 2 -> a)"),
        ("hamming_distance(\"abc\", \"cba\")", "2"),
        ("unique_elements([[1, 2, 2, b, b, a], [4, 3, 2, 1]])", "[[b, a], [4, 3]]"),
        ("subset_map(reverse, [x1, x2, x3, x4], [2, 4])", "[x1, x4, x3, x2]"),
        ("subset_replace([a, x_: Int] :> f(x))([1, a, 2])", "[f(1), 2]"),
        ("boolean_table(x and y, [x, y])", "[true, false, false, false]"),
        ("[is_tautology(a or not a), is_tautology(a or b), is_satisfiable(a and not a), is_satisfiable(xor(a, b))]", "[true, false, false, true]"),
        ("[equivalent(a, b, a), equivalent(a, b, true), equivalent(a, true, false), equivalent(true, true)]", "[equivalent(a, b), a and b, false, true]"),
    ];
    for (src, want) in cases {
        let got = e.eval(src, &stop).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"));
        assert_eq!(&got, want, "{src}");
    }
}
