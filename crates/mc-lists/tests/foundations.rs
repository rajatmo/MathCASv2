//! Roadmap package 1A (predicates and structure) on source text: normal cases, edge cases and failures.
mod common;
use common::{check, Env};

#[test]
fn kind_predicates() {
    check(
        &mut Env::new(),
        &[
            ("is_integer(3)", "true"),
            ("is_integer(-2^100)", "true"),
            ("is_integer(3/2)", "false"),
            ("is_integer(3.0)", "false"),
            ("is_integer(n)", "false"),
            ("is_integer(\"3\")", "false"),
            ("is_rational(3)", "true"),
            ("is_rational(-3/7)", "true"),
            ("is_rational(0.5)", "false"),
            ("is_rational(1 + 2im)", "false"),
            ("is_number(2.5)", "true"),
            ("is_number(1/3 + 2im)", "true"),
            ("is_number(1.5~30)", "true"),
            ("is_number(x)", "false"),
            ("is_number(x + 1)", "false"),
            ("is_number([1])", "false"),
            ("is_exact(2^100)", "true"),
            ("is_exact(1/2 + im)", "true"),
            ("is_exact(0.5)", "false"),
            ("is_exact(0.5 + im)", "false"),
            ("is_exact(x)", "false"),
            ("is_inexact(0.5)", "true"),
            ("is_inexact(2 + 0.5im)", "true"),
            ("is_inexact(1.5~30)", "true"),
            ("is_inexact(3)", "false"),
            ("is_inexact(x)", "false"),
            ("is_machine_number(0.5)", "true"),
            ("is_machine_number(1.0 + 2.0im)", "true"),
            ("is_machine_number(1.5~30)", "false"),
            ("is_machine_number(1)", "false"),
            ("is_string(\"\")", "true"),
            ("is_string(s)", "false"),
            ("is_symbol(x)", "true"),
            ("is_symbol(true)", "true"),
            ("is_symbol(f(x))", "false"),
            ("is_symbol(\"x\")", "false"),
            ("is_atom(x)", "true"),
            ("is_atom(2^100)", "true"),
            ("is_atom(\"s\")", "true"),
            ("is_atom(f())", "false"),
            ("is_atom([])", "false"),
            ("is_atom(1 + 2im)", "true"),
            ("is_integer()", "is_integer()"),
            ("is_atom(x, y)", "is_atom(x, y)"),
        ],
    );
}

#[test]
fn shape_predicates() {
    check(
        &mut Env::new(),
        &[
            ("is_list([])", "true"),
            ("is_list([1, [2]])", "true"),
            ("is_list(f(1))", "false"),
            ("is_list(x)", "false"),
            ("is_vector([1, 2, x])", "true"),
            ("is_vector([])", "true"),
            ("is_vector([1, [2]])", "false"),
            ("is_vector(f(1, 2))", "false"),
            ("is_vector(3)", "false"),
            ("is_vector([1, 2, 3], is_integer)", "true"),
            ("is_vector([1, 2.0, 3], is_integer)", "false"),
            ("is_vector([], is_integer)", "true"),
            ("is_matrix([[1, 2], [3, 4]])", "true"),
            ("is_matrix([[]])", "true"),
            ("is_matrix([])", "false"),
            ("is_matrix([[1, 2], [3]])", "false"),
            ("is_matrix([[1, [2]], [3, 4]])", "false"),
            ("is_matrix([1, 2])", "false"),
            ("is_matrix([[1, 2], [3, x]], is_number)", "false"),
            ("is_matrix([[1, 2], [3, 4]], is_integer)", "true"),
            ("fn bad(k) = 1/0", "null"),
            ("is_vector([1], bad)", "failure(division_by_zero)"),
            ("is_matrix([[1]], bad)", "failure(division_by_zero)"),
        ],
    );
}

#[test]
fn quantifiers() {
    check(
        &mut Env::new(),
        &[
            ("all_true([1, 3, 5], is_integer)", "true"),
            ("all_true([1, 3.0, 5], is_integer)", "false"),
            ("all_true([], is_integer)", "true"),
            ("any_true([x, 2.0, \"s\"], is_number)", "true"),
            ("any_true([x, y], is_number)", "false"),
            ("any_true([], is_number)", "false"),
            ("none_true([x, y], is_number)", "true"),
            ("none_true([x, 1], is_number)", "false"),
            ("none_true([], is_number)", "true"),
            ("all_true(f(1, 2), is_integer)", "true"),
            ("fn bad(k) = 1/0", "null"),
            ("all_true([1], bad)", "failure(division_by_zero)"),
            // the test stops at the first deciding element: `bad` is never reached
            ("any_true([1, x], is_integer)", "true"),
            ("rule test(1) := true", "null"),
            ("any_true([1, 2], test)", "true"),
            ("all_true([1, 2], test)", "test(2)"),
            ("all_true(xs, is_integer)", "all_true(xs, is_integer)"),
            ("all_true(3, is_integer)", "true"),
        ],
    );
}

#[test]
fn searching() {
    check(
        &mut Env::new(),
        &[
            ("free_of(f(x, g(y)), y)", "false"),
            ("free_of(f(x, g(y)), z)", "true"),
            ("free_of(f(x, g(y)), g)", "false"),
            ("free_of(f(x, g(y)), g(_))", "false"),
            ("free_of(f(x, g(y)), h(_))", "true"),
            ("free_of(x, x)", "false"),
            ("free_of(1 + x^2, _: Int)", "false"),
            ("free_of([a, \"s\"], _: String)", "false"),
            ("free_of([a, b], _: String)", "true"),
            ("free_of(nest(f, x, 2000), y)", "true"),
            ("free_of(f(x), f(x_, y__))", "true"),
            ("free_of(f(x), f(k_, k__))", "failure(argument, free_of, 2)"),
            ("member_of([1, 2, 3], 2)", "true"),
            ("member_of([1, 2, 3], 4)", "false"),
            ("member_of([1, f(2)], f(_))", "true"),
            ("member_of([[1]], 1)", "false"),
            ("member_of([], _)", "false"),
            ("member_of(f(a, b), b)", "true"),
            ("member_of([1, 2.5], _: MachineReal)", "true"),
            ("member_of([1, 2], k_ when k > 1)", "true"),
            ("member_of([1, 2], k_ when k > 2)", "false"),
            ("member_of(3, 3)", "failure(argument, member_of, 1)"),
            ("member_of(xs, 3)", "member_of(xs, 3)"),
            ("position([a, b, a], a)", "[[1], [3]]"),
            ("position([a, [a]], a)", "[[1], [2, 1]]"),
            ("position([[a]], _)", "[[0], [1, 0], [1, 1], [1], []]"),
            ("position(f(x), f)", "[[0]]"),
            ("position([a, b], c)", "[]"),
            ("position(x, x)", "[[]]"),
            ("position([1, f(2), 3.5], _: Int)", "[[1], [2, 1]]"),
            ("xs = [p, [q, p]]", "null"),
            ("map(k => apply(part, prepend(k, xs)), position(xs, p))", "[p, p]"),
            ("select_first([1, 2, 3, 4], k => k > 2)", "3"),
            ("select_first([1, 2], k => k > 2)", "failure(not_found, select_first)"),
            ("select_first([1, 2], k => k > 2, none)", "none"),
            ("select_first([], is_integer, 0)", "0"),
            ("select_first([1, 2], k => 1/0)", "failure(division_by_zero)"),
            ("select_first(3, is_integer)", "failure(argument, select_first, 1)"),
        ],
    );
}

#[test]
fn arranging() {
    check(
        &mut Env::new(),
        &[
            ("riffle([a, b, c], x)", "[a, x, b, x, c]"),
            ("riffle([a], x)", "[a]"),
            ("riffle([], x)", "[]"),
            ("riffle([a, b, c], [x, y])", "[a, x, b, y, c]"),
            ("riffle([a, b, c], [x, y, z])", "[a, x, b, y, c, z]"),
            ("riffle([a, b, c], [x])", "failure(shape_mismatch, riffle)"),
            ("riffle(a, x)", "riffle(a, x)"),
            ("riffle(1, x)", "failure(argument, riffle, 1)"),
            ("catenate([[a, b], [], [c]])", "[a, b, c]"),
            ("catenate([])", "[]"),
            ("catenate([[a], b])", "failure(argument, catenate, 1)"),
            ("partition([a, b, c, d, e], 2)", "[[a, b], [c, d]]"),
            ("partition([a, b, c, d, e], 2, 1)", "[[a, b], [b, c], [c, d], [d, e]]"),
            ("partition([a, b, c, d, e], 2, 3)", "[[a, b], [d, e]]"),
            ("partition([a, b], 3)", "[]"),
            ("partition([], 1)", "[]"),
            ("partition([a], 0)", "failure(argument, partition, 2)"),
            ("partition([a], 1, 0)", "failure(argument, partition, 3)"),
            ("partition([a], -1)", "failure(argument, partition, 2)"),
            ("partition([a], k)", "partition([a], k)"),
            ("rotate_left([a, b, c])", "[b, c, a]"),
            ("rotate_left([a, b, c], 5)", "[c, a, b]"),
            ("rotate_left([a, b, c], -1)", "[c, a, b]"),
            ("rotate_right([a, b, c])", "[c, a, b]"),
            ("rotate_right([a, b, c], 3)", "[a, b, c]"),
            ("rotate_right([], 2)", "[]"),
            ("rotate_left(f(a, b))", "f(b, a)"),
            ("pad_left([a, b], 4)", "[0, 0, a, b]"),
            ("pad_left([a, b, c], 2)", "[b, c]"),
            ("pad_right([a, b], 4, x)", "[a, b, x, x]"),
            ("pad_right([a, b, c], 2)", "[a, b]"),
            ("pad_right([a], 3, [0])", "[a, 0, 0]"),
            ("pad_right([a], 3, [[0]])", "[a, [0], [0]]"),
            ("pad_left([], 0)", "[]"),
            ("pad_left([a], -1)", "failure(argument, pad_left, 2)"),
            ("constant_array(0, 3)", "[0, 0, 0]"),
            ("constant_array(x, [2, 3])", "[[x, x, x], [x, x, x]]"),
            ("constant_array(x, 0)", "[]"),
            ("constant_array(x, [2, 0])", "[[], []]"),
            ("constant_array(x, [])", "failure(argument, constant_array, 2)"),
            ("constant_array(x, [2, -1])", "failure(argument, constant_array, 2)"),
            ("constant_array(x, [2, n])", "constant_array(x, [2, n])"),
            ("constant_array(x, [10^5, 10^5])", "failure(limit, memory, constant_array(x, [100000, 100000]))"),
            ("thread(f([a, b], [c, d]))", "[f(a, c), f(b, d)]"),
            ("thread(f([a, b], x))", "[f(a, x), f(b, x)]"),
            ("thread(f(x, y))", "f(x, y)"),
            ("thread(x)", "x"),
            ("thread(f([a, b], [c]))", "failure(shape_mismatch, thread)"),
            ("thread(g([1, 2], [3, 4]))", "[g(1, 3), g(2, 4)]"),
            ("thread(h([1, 2]))", "[h(1), h(2)]"),
            ("thread(imul([1, 2], [3, 4]))", "[3, 8]"),
            // an association is indexed by its rules (package 1B), not by position
            ("part(assoc(3 -> x, 1 -> y), 3)", "assoc(3 -> x, 1 -> y)[3]"),
            ("assoc(3 -> x, 1 -> y)[1]", "assoc(3 -> x, 1 -> y)[1]"),
            ("part(f(3 -> x, 1 -> y), 1)", "3 -> x"),
        ],
    );
}

#[test]
fn grouping() {
    check(
        &mut Env::new(),
        &[
            ("tally([a, b, a, c, a, b])", "[[a, 3], [b, 2], [c, 1]]"),
            ("tally([])", "[]"),
            ("tally([1, 1.0])", "[[1, 1], [1.0, 1]]"),
            ("gather([a, b, a, c, b])", "[[a, a], [b, b], [c]]"),
            ("gather([])", "[]"),
            ("gather_by([1, 2, 3, 4, 5], k => imod(k, 2))", "[[1, 3, 5], [2, 4]]"),
            ("gather_by([], f)", "[]"),
            ("gather_by([1, 2, 3, 4, 5], k => imod(k, 3))", "[[1, 4], [2, 5], [3]]"),
            ("gather_by([1, 2, 3, 4, 5], k => imod(k, 2), sum)", "[9, 6]"),
            ("gather_by([1, 2, 3], k => 0, length)", "[3]"),
            ("unique_by([1, -1, 2, -2, 3], k => k^2)", "[1, 2, 3]"),
            ("unique_by(f(1, -1, 2), k => k^2)", "f(1, 2)"),
            ("count_by([1, 2, 3, 4, 5], k => imod(k, 2))", "assoc(1 -> 3, 0 -> 2)"),
            ("count_by([], f)", "assoc()"),
            ("fn bad(k) = 1/0", "null"),
            ("gather_by([1], bad)", "failure(division_by_zero)"),
            ("gather_by([1], k => k, bad)", "failure(division_by_zero)"),
            ("group_by([1], f)", "group_by([1], f)"),
            ("tally(3)", "failure(argument, tally, 1)"),
            ("count_by(xs, f)", "count_by(xs, f)"),
        ],
    );
}

#[test]
fn ordering_by_keys() {
    check(
        &mut Env::new(),
        &[
            ("sort_by([3, -1, 2, -4], k => k^2)", "[-1, 2, 3, -4]"),
            ("sort_by([[b, 2], [a, 1], [c, 1]], last)", "[[a, 1], [c, 1], [b, 2]]"),
            ("sort_by([], f)", "[]"),
            ("sort_by(f(3, 1, 2), k => k)", "f(1, 2, 3)"),
            ("ordering([c, a, b])", "[2, 3, 1]"),
            ("ordering([c, a, b, a])", "[2, 4, 3, 1]"),
            ("ordering([c, a, b], 2)", "[2, 3]"),
            ("ordering([c, a, b], -1)", "[1]"),
            ("ordering([], 0)", "[]"),
            ("ordering([c, a, b], 4)", "failure(index, 4, 3)"),
            ("max_by([3, -5, 2], k => k^2)", "-5"),
            ("min_by([3, -5, 2], k => k^2)", "2"),
            ("max_by([-2, 2, 1], k => k^2)", "-2"),
            ("max_by([], f)", "failure(index, 1, 0)"),
            ("max_by([\"bb\", \"a\", \"ccc\"], string_length)", "\"ccc\""),
            ("fn bad(k) = 1/0", "null"),
            ("sort_by([1, 2], bad)", "failure(division_by_zero)"),
            ("max_by([1, 2], bad)", "failure(division_by_zero)"),
        ],
    );
}

#[test]
fn combinatorics() {
    check(
        &mut Env::new(),
        &[
            ("subsets([a, b, c])", "[[], [a], [b], [c], [a, b], [a, c], [b, c], [a, b, c]]"),
            ("subsets([a, b, c], 1)", "[[], [a], [b], [c]]"),
            ("subsets([a, b, c], [2])", "[[a, b], [a, c], [b, c]]"),
            ("subsets([a, b, c, d], [2, 3])", "[[a, b], [a, c], [a, d], [b, c], [b, d], [c, d], [a, b, c], [a, b, d], [a, c, d], [b, c, d]]"),
            ("subsets([a, b], [3])", "[]"),
            ("subsets([])", "[[]]"),
            ("subsets([a, a])", "[[], [a], [a], [a, a]]"),
            ("length(subsets(range(20)))", "1048576"),
            ("subsets(range(30))", "failure(limit, memory, subsets([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30]))"),
            ("length(subsets(range(100), [2]))", "4950"),
            ("subsets([a], -1)", "failure(argument, subsets, 2)"),
            ("subsets([a], [1, 2, 3])", "failure(argument, subsets, 2)"),
            ("subsets([a], n)", "subsets([a], n)"),
            ("tuples([a, b], 2)", "[[a, a], [a, b], [b, a], [b, b]]"),
            ("tuples([a, b], 0)", "[[]]"),
            ("tuples([], 2)", "[]"),
            ("tuples([[a, b], [1, 2, 3]])", "[[a, 1], [a, 2], [a, 3], [b, 1], [b, 2], [b, 3]]"),
            ("tuples([[a], []])", "[]"),
            ("tuples([])", "[[]]"),
            ("tuples([a, b], 30)", "failure(limit, memory, tuples([a, b], 30))"),
            ("tuples([a], 10^9)", "failure(limit, memory, tuples([a], 1000000000))"),
            ("tuples([[a], b])", "failure(argument, tuples, 1)"),
            ("permutations([a, b, c])", "[[a, b, c], [a, c, b], [b, a, c], [b, c, a], [c, a, b], [c, b, a]]"),
            ("permutations([a, b, a])", "[[a, b, a], [a, a, b], [b, a, a]]"),
            ("permutations([a, b, c, a], [2])", "[[a, b], [a, c], [a, a], [b, a], [b, c], [c, a], [c, b]]"),
            ("permutations([a, b], 1)", "[[], [a], [b]]"),
            ("permutations([])", "[[]]"),
            ("permutations([a, a, a])", "[[a, a, a]]"),
            ("length(permutations(range(8)))", "40320"),
            ("length(permutations([1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2]))", "20"),
            ("permutations(range(12))", "failure(limit, memory, permutations([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]))"),
            ("outer(f, [a, b], [x, y])", "[[f(a, x), f(a, y)], [f(b, x), f(b, y)]]"),
            ("outer(f, [a, b])", "[f(a), f(b)]"),
            ("outer(imul, [1, 2], [1, 2, 3])", "[[1, 2, 3], [2, 4, 6]]"),
            ("outer(f, [a], [], [b])", "[[]]"),
            ("outer(f, [[1, 2]], [x])", "[[f([1, 2], x)]]"),
            ("outer(f)", "outer(f)"),
            ("outer(f, [a], 3)", "failure(argument, outer, 3)"),
            ("inner(f, [a, b], [x, y], g)", "g(f(a, x), f(b, y))"),
            ("inner(mul, [1, 2, 3], [4, 5, 6])", "32"),
            ("inner(mul, [[1, 2], [3, 4]], [[5, 6], [7, 8]])", "[[19, 22], [43, 50]]"),
            ("inner(f, [[a, b], [c, d]], [[x, y], [z, w]], g)", "[[g(f(a, x), f(b, z)), g(f(a, y), f(b, w))], [g(f(c, x), f(d, z)), g(f(c, y), f(d, w))]]"),
            ("inner(mul, [[1, 2], [3, 4]], [1, 1])", "[3, 7]"),
            ("inner(mul, [1, 1], [[1, 2], [3, 4]])", "[4, 6]"),
            ("inner(mul, [], [])", "0"),
            ("inner(mul, [1, 2], [1])", "failure(shape_mismatch, inner)"),
            ("inner(mul, [[1, 2], [3]], [1, 1])", "failure(shape_mismatch, inner)"),
            ("inner(mul, x, [1])", "inner(mul, x, [1])"),
        ],
    );
}

#[test]
fn iteration() {
    check(
        &mut Env::new(),
        &[
            ("table(k^2, k, 4)", "[1, 4, 9, 16]"),
            ("table(k^2, k, [a, 3])", "[a^2, 9]"),
            ("table(k, k, 0)", "[]"),
            ("table(k, k, -2)", "[]"),
            ("table(x, 3)", "[x, x, x]"),
            ("table([i, j], i, 2, j, 3)", "[[[1, 1], [1, 2], [1, 3]], [[2, 1], [2, 2], [2, 3]]]"),
            ("table([i, j], i, 3, j, i)", "[[[1, 1]], [[2, 1], [2, 2]], [[3, 1], [3, 2], [3, 3]]]"),
            ("table(i + j, i, 2, j, range(i, 2))", "[[2, 3], [4]]"),
            ("k = 10", "null"),
            ("table(k, k, 3)", "[1, 2, 3]"),
            ("k", "10"),
            ("table(k, k, n)", "table(k, k, n)"),
            ("table(k, 1, 3)", "[[10, 10, 10]]"),
            ("table(x, 2, 3)", "[[x, x, x], [x, x, x]]"),
            ("table(k, k, \"s\")", "failure(argument, table, 3)"),
            ("table(k, k, 1/0)", "failure(division_by_zero)"),
            ("table(k, k, 10^9)", "failure(limit, memory, table(k, k, 10^9))"),
            ("table(k, k, 3, j)", "table(k, k, 3, j)"),
            ("{ let c = 5; table(c*k, k, 3) }", "[5, 10, 15]"),
            ("array(f, 3)", "[f(1), f(2), f(3)]"),
            ("array(f, [2, 2])", "[[f(1, 1), f(1, 2)], [f(2, 1), f(2, 2)]]"),
            ("array(imul, [2, 3])", "[[1, 2, 3], [2, 4, 6]]"),
            ("array(f, [0, 3])", "[]"),
            ("array(f, [2, 0])", "[[], []]"),
            ("array(f, [])", "failure(argument, array, 2)"),
            ("map_indexed(f, [a, b])", "[f(a, 1), f(b, 2)]"),
            ("map_indexed(f, [])", "[]"),
            ("map_indexed(f, 3)", "failure(argument, map_indexed, 2)"),
            ("map_thread(f, [[a, b], [c, d]])", "[f(a, c), f(b, d)]"),
            ("map_thread(iadd, [[1, 2], [3, 4], [5, 6]])", "[9, 12]"),
            ("map_thread(f, [])", "[]"),
            ("map_thread(f, [[a, b], [c]])", "failure(shape_mismatch, map_thread)"),
            ("map_thread(f, [[a], c])", "failure(argument, map_thread, 2)"),
            ("scan(print, [1, 2])", "null"),
            ("scan(f, [])", "null"),
            ("fn bad(k) = 1/0", "null"),
            ("scan(bad, [1, 2])", "failure(division_by_zero)"),
        ],
    );
}

/// The roadmap's package 1A targets, by their names here (`delete_duplicates_by` is `unique_by`, the `by` form of
/// `unique`; the list form of `group_by` is `gather_by`, `group_by` itself is package 1B's association form).
const TARGETS: &[&str] = &[
    "is_integer", "is_rational", "is_number", "is_exact", "is_inexact", "is_machine_number", "is_list", "is_vector",
    "is_matrix", "is_string", "is_symbol", "is_atom", "free_of", "member_of", "position", "all_true", "any_true",
    "none_true", "thread", "constant_array", "riffle", "catenate", "partition", "tally", "gather", "gather_by",
    "sort_by", "max_by", "min_by", "ordering", "rotate_left", "rotate_right", "pad_left", "pad_right",
    "unique_by", "subsets", "tuples", "permutations", "outer", "inner", "table", "array", "map_indexed", "map_thread",
    "scan", "select_first", "count_by",
];

#[test]
fn every_target_has_one_entry() {
    let names: Vec<&str> = mc_lists::FUNCTIONS.iter().map(|e| e.0).collect();
    for t in TARGETS {
        assert_eq!(names.iter().filter(|n| *n == t).count(), 1, "{t}");
    }
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "a name is listed twice");
    assert!(mc_lists::FUNCTIONS.iter().all(|e| !e.1.is_empty()));
    assert!(!names.contains(&"group_by"), "group_by is package 1B's");
}
