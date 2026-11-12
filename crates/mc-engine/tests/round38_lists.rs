//! Round 38: argument forms of list and structure functions: heads as parts, permuted transposes, ordering
//! functions and `up_to` counts, margins, negative steps, indexed table variables, sets of array heads, missing
//! elements at depth, sparse arrays in quantifiers.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn heads_as_parts() {
    assert_eq!(ev("map_all(f, [a, b, c], heads: true)"), "f(f(list)(f(a), f(b), f(c)))");
    assert_eq!(ev("map_apply(f, p(x)(q(y)), heads: true)"), "f(x)(f(y))");
    assert_eq!(ev("comap(h(a, b, c), x, heads: true)"), "h(x)(a(x), b(x), c(x))");
    assert_eq!(ev("comap_apply(f(g), [x], heads: true)"), "f(x)(g(x))");
    assert_eq!(ev("depth(h([[[a]]])(x, y), heads: true)"), "6");
    assert_eq!(ev("depth([f()(x), g(y)(x), h(g(y))(x)], heads: true)"), "5");
    // without heads the depth ignores them (an independent count: x and y are one level below)
    assert_eq!(ev("depth(h([[[a]]])(x, y))"), "2");
    assert_eq!(ev("replace_part(f(x, y), _ -> g, heads: true)"), "g(g, g)");
    assert_eq!(ev("replace_part(f(x, y), 0 -> g, heads: false)"), "f(x, y)");
    assert_eq!(ev("replace_part(f(x, y), 0 -> g)"), "g(x, y)");
    assert_eq!(ev("member_of(assoc(a -> [1], b -> [2]), list, 2, heads: true)"), "true");
    assert_eq!(ev("member_of(assoc(a -> [1], b -> [2]), list, 2)"), "false");
    // a heads option with another key declines
    assert_eq!(ev("map_all(f, [a], heads: true, foo: 1)"), "map_all(f, [a], heads: true, foo: 1)");
}

#[test]
fn transpose_with_a_permutation() {
    let a = "a3 = array(c, [2, 3, 2]); ";
    assert_eq!(
        ev(&format!("{a}transpose(a3, [1, 3, 2])")),
        "[[[c(1, 1, 1), c(1, 2, 1), c(1, 3, 1)], [c(1, 1, 2), c(1, 2, 2), c(1, 3, 2)]], [[c(2, 1, 1), c(2, 2, 1), c(2, 3, 1)], [c(2, 1, 2), c(2, 2, 2), c(2, 3, 2)]]]"
    );
    // part [i, j, k] of transpose(a, p) is part [j_p1, j_p2, j_p3] of a, checked element by element
    assert_eq!(ev(&format!("{a}t = transpose(a3, [3, 1, 2]); all_true(flatten(table(t[j, k, i] === a3[i, j, k], i, 2, j, 3, k, 2)), b => b)")), "true");
    assert_eq!(ev(&format!("{a}dimensions(transpose(a3, [3, 1, 2]))")), "[3, 2, 2]");
    // [2, 1] is the plain transpose; [1, 1] the diagonal
    assert_eq!(ev("m = [[1, 2, 3], [4, 5, 6]]; transpose(m, [2, 1]) == transpose(m)"), "true");
    assert_eq!(ev("transpose([[1, 2], [3, 4]], [1, 1])"), "[1, 4]");
    assert_eq!(ev("conjugate_transpose([[1 + im, 2]], [2, 1]) == [[1 - im], [2]]"), "true");
    assert_eq!(ev("conjugate_transpose([[1 + im, 2]], [2, 1]) == conjugate_transpose([[1 + im, 2]])"), "true");
    // a level that is not the target of any index declines
    assert_eq!(ev("transpose([[1, 2], [3, 4]], [3, 1])"), "transpose([[1, 2], [3, 4]], [3, 1])");
}

#[test]
fn ordering_functions_and_up_to() {
    assert_eq!(ev("ordering([2, 6, 1, 9, 1, 2, 3], [4, -1])"), "[6, 7, 2, 4]");
    assert_eq!(ev("ordering([2, 6, 1, 9, 2], up_to(6))"), "[3, 1, 5, 2, 4]");
    assert_eq!(ev("ordering([2, 6, 1, 9, 2], up_to(2)) == ordering([2, 6, 1, 9, 2], 2)"), "true");
    assert_eq!(ev("commonest([b, a, c, 2, a, b, 1, 2], up_to(6))"), "[b, a, c, 2, 1]");
    assert_eq!(ev("ordering_by([\"dog\", \"zebra\", \"donkey\", \"anteater\", \"fish\"], string_length, 3, greater)"), "[4, 3, 2]");
    assert_eq!(ev("reverse_sort_by([\"dog\", \"zebra\", \"donkey\", \"anteater\", \"fish\"], string_length, greater)"), "[\"dog\", \"fish\", \"zebra\", \"donkey\", \"anteater\"]");
    let w = "[\"abc\", \"def\", \"aga\", \"del\", \"two\", \"one\", \"One\"]";
    assert_eq!(ev(&format!("position_largest({w}, 3, alphabetic_order)")), "[[5], [7], [6]]");
    assert_eq!(ev(&format!("position_smallest({w}, 3, alphabetic_order)")), "[[1], [3], [2]]");
    // equal elements under the ordering form one group; under greater the largest is the smallest number
    assert_eq!(ev("position_largest([3, 1, 3, 2], 1, less)"), "[[3, 1]]");
    assert_eq!(ev("position_largest([3, 1, 3, 2], 1, greater)"), "[[2]]");
    let l = "[[3, 7], [2, 7, 2], [0, 1, 3, 4]]";
    assert_eq!(ev(&format!("take_largest({l}, 2, lexicographic_order)")), "[[3, 7], [2, 7, 2]]");
    assert_eq!(ev(&format!("take_smallest({l}, 2, lexicographic_order)")), "[[0, 1, 3, 4], [2, 7, 2]]");
    // by an order on numbers the result agrees with take_largest without one
    assert_eq!(ev("take_largest([5, 2, 9, 4], 2, numerical_order) == take_largest([5, 2, 9, 4], 2)"), "true");
    assert_eq!(ev("string_take(\"abcdefghijklm\", up_to(20))"), "\"abcdefghijklm\"");
    assert_eq!(ev("string_take(\"abc\", up_to(2))"), "\"ab\"");
    assert_eq!(ev("cases([1, \"f\", g, \"h\", \"j\"], x_ when is_string(x), 1, up_to(2))"), "[\"f\", \"h\"]");
}

#[test]
fn margins_steps_and_indexed_variables() {
    assert_eq!(ev("pad_left([a, b, c], 10, x, 2)"), "[x, x, x, x, x, a, b, c, x, x]");
    assert_eq!(ev("pad_right([a, b, c], 10, x, 2)"), "[x, x, a, b, c, x, x, x, x, x]");
    assert_eq!(ev("pad_left([1], 19, 0, 9) == pad_right([1], 19, 0, 9)"), "true");
    // a list too long for the room left is cut as pad_left cuts it
    assert_eq!(ev("pad_left([1, 2, 3, 4], 4, 0, 1)"), "[2, 3, 4, 0]");
    assert_eq!(ev("take(range(7), [-1, 1, -2])"), "[7, 5, 3, 1]");
    assert_eq!(ev("take(range(7), [-1, 1, -1]) == reverse(range(7))"), "true");
    assert_eq!(ev("drop(range(7), [7, 1, -3])"), "[2, 3, 5, 6]");
    assert_eq!(ev("table(factorial(a(x)), a(x), 6)"), "[1, 2, 6, 24, 120, 720]");
    assert_eq!(ev("table(x(1)^2 + x(2)^2, x(1), 3, x(2), 3) == table(i^2 + j^2, i, 3, j, 3)"), "true");
    assert_eq!(ev("table(i + j, [i, 2], j, 2)"), "[[2, 3], [3, 4]]");
    assert_eq!(ev("map_at(f, [[a, b, c], [d, e]], [all, 2])"), "[[a, f(b), c], [d, f(e)]]");
    assert_eq!(ev("replace_at([[a, a], [a, a]], a -> xx, [all, 2])"), "[[a, xx], [a, xx]]");
    assert_eq!(ev("replace_at([a, b, c, d, e], assoc(a -> b, b -> c, c -> a, d -> e, e -> d), all)"), "[b, c, a, e, d]");
    assert_eq!(ev("rotate_right(assoc(1 -> [a, b], 2 -> [b, c], 3 -> [c, d]), [1, 1])"), "assoc(3 -> [d, c], 1 -> [b, a], 2 -> [c, b])");
}

#[test]
fn array_heads_missing_and_sparse() {
    let na = "numeric_array([4, 5, 6], \"Integer8\")";
    assert_eq!(ev(&format!("dimensions([numeric_array([1, 2, 3], \"Integer8\"), {na}], allowed_heads: [list, numeric_array])")), "[2, 3]");
    assert_eq!(ev(&format!("dimensions([[1, 2, 3], {na}], allowed_heads: [list, numeric_array])")), "[2]");
    assert_eq!(ev(&format!("dimensions([[1, 2, 3], {na}], allowed_heads: list | numeric_array)")), "[2, 3]");
    assert_eq!(ev(&format!("array_depth([[1, 2, 3], {na}], allowed_heads: [list, numeric_array])")), "1");
    assert_eq!(ev("array_depth([byte_array([1, 2, 3]), numeric_array([1.0, 2.0, 3.0], \"Real64\")], allowed_heads: \"ListLike\" | byte_array | numeric_array)"), "2");
    assert_eq!(ev("dimensions([[1, 2], [3, 4]], allowed_heads: [list]) == dimensions([[1, 2], [3, 4]])"), "true");
    assert_eq!(ev("delete_missing([[a, 1], [b, 2], [c, missing()], [d, [missing()]]], 1, inf)"), "[[a, 1], [b, 2]]");
    assert_eq!(ev("delete_missing([[a, 1], [d, [missing()]]], 1, 1)"), "[[a, 1], [d, [missing()]]]");
    assert_eq!(ev("delete_missing([1, [2, missing()], missing()], 2)"), "[1, [2]]");
    assert_eq!(ev("delete_missing([f(x, missing(), y)], inf)"), "[f(x, missing(), y)]");
    assert_eq!(ev("all_true(sparse_array([1000 -> 2], [1000000000000]), is_even)"), "true");
    assert_eq!(ev("any_true(sparse_array([1000 -> 3], [1000000000000]), is_odd)"), "true");
    assert_eq!(ev("none_true(sparse_array([1000 -> 2], [1000000000000]), is_odd)"), "true");
    // the background counts: odd background, even explicit value
    assert_eq!(ev("all_true(sparse_array([1000 -> 2], [1000000000000], 1), is_even)"), "false");
    assert_eq!(ev("string_length([\"cat\", \"dog\", \"fish\", \"coelenterate\"])"), "[3, 3, 4, 12]");
    assert_eq!(ev("digit_sum(102341, mixed_radix([24, 60, 60]))"), "71");
    assert_eq!(ev("digit_sum(137, mixed_radix([3, 12])) == total(integer_digits(137, mixed_radix([3, 12])))"), "true");
    assert_eq!(ev("digit_sum(1084, mixed_radix([17, 15, 11]), -2)"), "14");
    assert_eq!(ev("integer_exponent(0)"), "inf");
    assert_eq!(ev("integer_exponent(0, 3)"), "inf");
}

#[test]
fn shapes_of_radius_zero() {
    // radius 0 holds only the offset 0: the centre of an odd array, nothing in an even one
    assert_eq!(ev("box_matrix(0, [3, 3])"), "[[0, 0, 0], [0, 1, 0], [0, 0, 0]]");
    assert_eq!(ev("box_matrix(0, [2, 2])"), "[[0, 0], [0, 0]]");
    assert_eq!(ev("disk_matrix(0, [3, 3]) == box_matrix(0, [3, 3])"), "true");
    // a zero radius in one dimension only: the middle row (column) of the box
    assert_eq!(ev("box_matrix([0, 1], [3, 3])"), "[[0, 0, 0], [1, 1, 1], [0, 0, 0]]");
    assert_eq!(ev("cross_matrix([1, 0], [3, 5])"), "[[0, 0, 1, 0, 0], [0, 0, 1, 0, 0], [0, 0, 1, 0, 0]]");
    assert_eq!(ev("box_matrix(1, [3, 3])"), "[[1, 1, 1], [1, 1, 1], [1, 1, 1]]");
}

#[test]
fn logarithm_to_a_base() {
    // log(b, x) is log_base(b, x); checked against log(x)/log(b) and b^result == x
    assert_eq!(ev("log(10.0, 5.0)"), "0.6989700043360186");
    assert_eq!(ev("abs(log(10.0, 5.0) - n(log(5) / log(10))) < 10^-15"), "true");
    assert_eq!(ev("log(2, 8)"), "3");
    assert_eq!(ev("log(2.0, 1024)"), "10.0");
    assert_eq!(ev("abs(10^log(10, 5.0) - 5) < 10^-14"), "true");
    assert_eq!(ev("log(b, x)"), "log(x)/log(b)");
    assert_eq!(ev("log(10)"), "log(10)");
}
