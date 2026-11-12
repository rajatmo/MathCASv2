//! Round 37: regression tests for inputs that gave no answer. Each test has an example
//! input and variations of its own, checked by an independent route (another evaluation, an identity, a count).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// Evaluates every line in one session and returns the printed results.
fn session(lines: &[&str]) -> Vec<String> {
    let mut e = CoreEngine::new();
    lines
        .iter()
        .map(|s| e.eval(s, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}")))
        .collect()
}

#[test]
fn n_takes_a_real_number_of_digits() {
    // documented: n(e + pi, 23.4) and n(pi, machine_precision); a real precision is met by its ceiling
    assert_eq!(ev("n(e + pi, 23.4) === n(e + pi, 24)"), "true");
    assert!(ev("n(e + pi, 23.4)").starts_with("5.85987448204883847382"));
    assert!(ev("n(pi, machine_precision)").starts_with("3.141592653589793"));
    assert!(ev("n(pi, 2 * machine_precision)").starts_with("3.14159265358979323846264338327950"));
    // variations: a small real precision, a complex value, set_precision with a real precision
    assert_eq!(ev("n(exp(1 + im), 20.5) === n(exp(1 + im), 21)"), "true");
    // re(exp(1 + i)) = e cos 1
    assert_eq!(ev("abs(re(n(exp(1 + im), 20.5)) - n(e * cos(1), 30)) < 10^-20"), "true");
    assert_eq!(ev("precision(n(pi, 0.3)) >= 1"), "true");
    assert_eq!(ev("set_precision(a + 2 * b, machine_precision) === set_precision(a + 2 * b, 16)"), "true");
}

#[test]
fn n_with_an_accuracy_goal() {
    // documented: zero to 100 digits after the point (sqrt 2 + sqrt 3 = sqrt(5 + 2 sqrt 6)), exp(10) to 20
    assert_eq!(ev("abs(n(sqrt(2) + sqrt(3) - sqrt(5 + 2 * sqrt(6)), [inf, 100])) < 10^-100"), "true");
    assert_eq!(ev("accuracy(n(sqrt(2) + sqrt(3) - sqrt(5 + 2 * sqrt(6)), [inf, 100])) >= 100"), "true");
    assert_eq!(ev("accuracy(n(1 + 1 / golden_ratio - golden_ratio, [inf, 30])) >= 30"), "true");
    // exp(10) = 22026.4657948067165169579006452842443663535..., the absolute error below 10^-20
    assert!(ev("n(exp(10), [inf, 20])").starts_with("22026.46579480671651695790"));
    assert_eq!(ev("abs(n(exp(10), [inf, 20]) - n(exp(10), 50)) < 10^-20"), "true");
    // variations: a large value needs more significant digits, a small one fewer; a complex value
    assert_eq!(ev("abs(n(exp(100), [inf, 20]) - n(exp(100), 80)) < 10^-20"), "true");
    assert_eq!(ev("abs(n(exp(-30), [inf, 20]) - n(exp(-30), 30)) < 10^-20"), "true");
    assert_eq!(ev("precision(n(exp(-30), [inf, 20])) < precision(n(exp(-30), 20))"), "true");
    assert_eq!(ev("d = n(exp(1 + im), [inf, 25]) - n(exp(1 + im), 40); abs(re(d)) < 10^-25 and abs(im_part(d)) < 10^-25"), "true");
    // the precision goal of [p, a] is enough on its own; both inf is invalid
    assert_eq!(ev("n(pi, [20, inf]) === n(pi, 20)"), "true");
    assert_eq!(ev("n(pi, [inf, inf])"), "failure(invalid_argument, n(pi, [inf, inf]))");
}

#[test]
fn join_at_a_deeper_level() {
    assert_eq!(ev("join([[a, b], [c, d_u]], [[1, 2], [3, 4]], 2)"), "[[a, b, 1, 2], [c, d_u, 3, 4]]");
    assert_eq!(ev("join([[1], [5, 6]], [[2, 3], [7]], [[4], [8]], 2)"), "[[1, 2, 3, 4], [5, 6, 7, 8]]");
    assert_eq!(ev("join([[x]], [[1, 2], [3, 4]], 2)"), "[[x, 1, 2], [3, 4]]");
    // the level-2 join of matrices is the transpose of the level-1 join of their transposes
    assert_eq!(
        ev("m = [[1, 2], [3, 4]]; k = [[5], [6]]; join(m, k, 2) == transpose(join(transpose(m), transpose(k)))"),
        "true"
    );
    assert_eq!(ev("join([[[1]], [[2]]], [[[3]], [[4]]], 3)"), "[[[1, 3]], [[2, 4]]]");
    assert_eq!(ev("dimensions(join(identity_matrix(3), transpose([[1, 2, 3]]), 2))"), "[3, 4]");
}

#[test]
fn delete_elements_with_counts() {
    assert_eq!(ev("delete_elements([2, x, 1, x, a, y, x], 2 -> [x])"), "[2, 1, a, y, x]");
    assert_eq!(ev("delete_elements([2, x, 1, x, a, y, x], [1, 2] -> [x, y])"), "[2, 1, x, a, x]");
    assert_eq!(ev("delete_elements([1, 2, 2, 3, 3, 4], inf -> [2, 3])"), "[1, 4]");
    assert_eq!(ev("delete_elements([a, b, a, b, b, b, c], [1, 2] -> [b, b])"), "[a, a, b, c]");
    // inf -> es is delete_elements(xs, es); the counts of the result add up
    assert_eq!(ev("delete_elements(range(20), inf -> [3, 5]) == delete_elements(range(20), [3, 5])"), "true");
    assert_eq!(ev("length(delete_elements(table(mod(k, 3), k, 30), 4 -> [0, 1]))"), "22");
}

#[test]
fn size_ranges_with_a_step() {
    assert_eq!(ev("subsets([a, b, c], [3, 0, -1])"), "[[a, b, c], [a, b], [a, c], [b, c], [a], [b], [c], []]");
    // counted down, the same subsets as counted up: sorted, they agree
    assert_eq!(ev("sort(subsets(range(4), [4, 0, -1])) == sort(subsets(range(4)))"), "true");
    assert_eq!(ev("length(subsets(range(5), [0, 5, 2]))"), "16");
    // 1 + 10 + 5 subsets of sizes 0, 2, 4 of five; a range counted down from above the length
    assert_eq!(ev("map(length, subsets([a, b], [10, 0, -3]))"), "[1, 1]");
    assert_eq!(ev("length(permutations(range(4), [4, 0, -2]))"), "37");
    assert_eq!(ev("permutations(f(a, b, c))"), "[f(a, b, c), f(a, c, b), f(b, a, c), f(b, c, a), f(c, a, b), f(c, b, a)]");
}

#[test]
fn operations_keep_the_head_of_their_operand() {
    assert_eq!(ev("partition(f(a, b, c, d_u), 2)"), "f(f(a, b), f(c, d_u))");
    assert_eq!(ev("partition(f(a, b, c, d_u, e_u), 3, 1)"), "f(f(a, b, c), f(b, c, d_u), f(c, d_u, e_u))");
    assert_eq!(ev("block_map(f, h(1, 2, 3, 4), 2)"), "[f(h(1, 2)), f(h(3, 4))]");
    assert_eq!(ev("block_map(f, [1, 2, 3, 4, 5, 6], 3)"), "[f([1, 2, 3]), f([4, 5, 6])]");
    assert_eq!(ev("tuples(f(x, y, z), 2)"), "[f(x, x), f(x, y), f(x, z), f(y, x), f(y, y), f(y, z), f(z, x), f(z, y), f(z, z)]");
    assert_eq!(ev("length(tuples([0, 1], [3, 3]))"), "512");
    assert_eq!(ev("tuples([a, b], [2, 2])[16]"), "[[b, b], [b, b]]");
    assert_eq!(ev("fold(f, x, p(a, b, c, d_u))"), "f(f(f(f(x, a), b), c), d_u)");
    assert_eq!(ev("fold(add, 0, h(1, 2, 3)) == total(range(3))"), "true");
    assert_eq!(ev("fold_list(f, x, p(a, b))"), "p(x, f(x, a), f(f(x, a), b))");
    assert_eq!(ev("fold_list(f, [])"), "[]");
    assert_eq!(ev("symmetric_difference(f(1, 2, 3), f(2, 3, 4), f(3, 4, 5))"), "f(1, 3, 5)");
    assert_eq!(ev("pad_left(f(a, b, c), 8, x)"), "f(x, x, x, x, x, a, b, c)");
    assert_eq!(ev("pad_right(f(a, b, c), 5, x)"), "f(a, b, c, x, x)");
    // a negative length pads at the other end
    assert_eq!(ev("pad_left([a, b, c], -10) == pad_right([a, b, c], 10)"), "true");
    assert_eq!(ev("pad_right([a, b, c], -5, z)"), "[z, z, a, b, c]");
}

#[test]
fn transpose_of_a_vector_is_the_vector() {
    assert_eq!(ev("transpose([1.5, 2.2, 3.1])"), "[1.5, 2.2, 3.1]");
    assert_eq!(ev("conjugate_transpose([1.5, 2.2 * im, 3.1 + 4.4 * im]) == conjugate([1.5, 2.2 * im, 3.1 + 4.4 * im])"), "true");
    assert_eq!(ev("transpose([1, 2/3])"), "[1, 2/3]");
    let s = session(&["v = [1, 2, 3]", "transpose(transpose(v)) == v"]);
    assert_eq!(s[1], "true");
}

#[test]
fn characteristic_polynomial_of_a_pencil() {
    // documented: det(x m2 - m1) = -6 + 7 x - 2 x^2 (n = 2, the same as det(m1 - x m2))
    assert_eq!(ev("expand(characteristic_polynomial([[[1, 2], [5, 4]], [[4, 3], [6, 4]]], x) - (-6 + 7*x - 2*x^2))"), "0");
    // b = I gives the characteristic polynomial of the matrix
    assert_eq!(
        ev("m = [[1, 2, 0], [3, -1, 4], [0, 2, 5]]; expand(characteristic_polynomial([m, identity_matrix(3)], x) - characteristic_polynomial(m, x))"),
        "0"
    );
    // a singular b: degree 2 for n = 3, det(x b - a) = 1 + x - x^2
    assert_eq!(ev("expand(characteristic_polynomial([[[1, 1, 1], [1, 0, 1], [0, 0, 1]], [[0, 1, 1], [0, 1, 1], [1, 0, 0]]], x) - (1 + x - x^2))"), "0");
    // machine entries: the coefficients agree with det(x b - a) of the exact matrices
    assert_eq!(
        ev("a = [[1, 3/2, 2], [31/10, 2, 29/10], [3, 2, 1]]; b = [[13/10, 1/2, 11/10], [0, 3/2, 23/10], [1, 0, 1]]; \
            p = characteristic_polynomial(n([a, b]), x); q = det(x * b - a); \
            max(abs(coefficient_list(p, x) - coefficient_list(q, x))) < 10^-12"),
        "true"
    );
}

#[test]
fn eigenvalues_and_eigenvectors_of_a_pencil() {
    let s = session(&[
        "a = [[1, 1, 1], [1, 0, 1], [0, 0, 1]]; b = [[0, 1, 1], [0, 1, 1], [1, 0, 0]];",
        "eigenvalues([a, b])",
        "es = eigensystem([a, b]);",
        // every finite pair satisfies a v = lambda b v, the infinite one b v = 0
        "simplify(map(&(dot(a, es[2][&1]) - es[1][&1] * dot(b, es[2][&1])), [2, 3]))",
        "dot(b, es[2, 1])",
        "eigenvectors([a, b]) == es[2]",
    ]);
    assert_eq!(s[1], "[inf, 1/2 + (1/2)*5^(1/2), 1/2 + (-1/2)*5^(1/2)]");
    assert_eq!(s[3], "[[0, 0, 0], [0, 0, 0]]");
    assert_eq!(s[4], "[0, 0, 0]");
    assert_eq!(s[5], "true");
    // b = I: the eigenvalues of the matrix; a diagonal pencil: the ratios of the diagonals
    assert_eq!(ev("eigenvalues([[[1, 2], [3, 4]], identity_matrix(2)]) == eigenvalues([[1, 2], [3, 4]])"), "true");
    assert_eq!(ev("eigenvalues([[[2, 0, 0], [0, 9, 0], [0, 0, 5]], [[1, 0, 0], [0, 3, 0], [0, 0, 5]]])"), "[3, 2, 1]");
    // a cubic factor: three eigenvalues that are roots of det(x b - a)
    assert_eq!(
        ev("a = [[0, 1, 0], [0, 0, 1], [1, 1, 1]]; b = [[1, 0, 0], [0, 1, 0], [0, 0, 3]]; \
            chop(n(map(&(det(&1 * b - a)), eigenvalues([a, b])), 30))"),
        "[0, 0, 0]"
    );
}

#[test]
fn machine_eigenvalues_of_a_pencil() {
    // documented: eigenvalues([a, b]) = [-3.210040153039578, 1.6136689473871475, -0.6656977598648105]
    let s = session(&[
        "a = [[1., 1.5, 2.], [3.1, 2., 2.9], [3., 2., 1.]]; b = [[1.3, 0.5, 1.1], [0., 1.5, 2.3], [1., 0., 1.]];",
        "ls = eigenvalues([a, b]); vs = eigenvectors([a, b]);",
        // the residuals of a v = lambda b v, and the product of the eigenvalues is det(a)/det(b)
        "max(table(norm(dot(a, vs[k]) - ls[k] * dot(b, vs[k])), k, 3)) < 10^-12",
        "abs(apply(mul, ls) - det(a) / det(b)) < 10^-12",
        "abs(ls[1] + 3.210040153039578) < 10^-13",
        // a singular b in machine numbers declines
        "eigenvalues([a, [[1., 1.], [1., 1.]]])",
        "head(eigenvalues([a, [[1., 2., 3.], [2., 4., 6.], [0., 0., 1.]]]))",
    ]);
    assert_eq!(s[2], "true");
    assert_eq!(s[3], "true");
    assert_eq!(s[4], "true");
    assert_eq!(s[6], "eigenvalues");
    // complex entries
    assert_eq!(
        ev("a = [[1, im], [2, 3.]]; b = [[2., 0], [1, 1]]; ls = eigenvalues([a, b]); max(map(&(abs(det(a - &1 * b))), ls)) < 10^-12"),
        "true"
    );
}

#[test]
fn optimisation_over_a_cylinder() {
    // documented: the highest point of the cylinder of radius 1 from (-1, 0, 1) to (1, 0, -1) is 1 + 1/sqrt 2
    let s = session(&[
        "r = cylinder([[-1, 0, 1], [1, 0, -1]], 1);",
        "abs(find_max_value([z, element([x, y, z], r)], [x, y, z]) - n(1 + 1 / sqrt(2), 30)) < 10^-9",
        "abs(first(find_arg_max([z, element([x, y, z], r)], [x, y, z])) - n(-1 + 1 / sqrt(2), 30)) < 10^-8",
        // the lowest value of x + y + z on the unit cylinder about the z axis is -1 - sqrt 2
        "abs(find_min_value([x + y + z, element([x, y, z], cylinder())], [x, y, z]) + n(1 + sqrt(2), 30)) < 10^-9",
        // a point found is in the cylinder: its distance to the axis is at most the radius
        "p = find_arg_min([(x - 5)^2 + y^2 + z^2, element([x, y, z], cylinder([[0, 0, 0], [0, 0, 2]], 1/2))], [x, y, z]); abs(norm(most(p)) - 1/2) < 10^-6 and 0 <= p[3] and p[3] <= 2",
    ]);
    assert_eq!(&s[1..], &["true", "true", "true", "true"]);
}

#[test]
fn variance_of_weighted_data() {
    // documented: symbolic weights, (w1 w2 + w2 w3 + 4 w1 w3) / (w1 + w2 + w3)^2 = sum w (x - m)^2 / W
    assert_eq!(
        ev("v = variance(weighted_data([1, 2, 3], [w1, w2, w3])); m = (w1 + 2 w2 + 3 w3) / (w1 + w2 + w3); \
            simplify(v - (w1 (1 - m)^2 + w2 (2 - m)^2 + w3 (3 - m)^2) / (w1 + w2 + w3))"),
        "0"
    );
    // numbers: weights as repetitions (1 x 1, 2 x 2, 5 x 3) give the population variance of the repeated data
    assert_eq!(ev("variance(weighted_data([1, 2, 3], [1, 2, 5])) == mean((range(8) * 0 + [1, 2, 2, 3, 3, 3, 3, 3] - 5/2)^2)"), "true");
    assert_eq!(ev("variance(weighted_data([1, 2, 3], [1, 2, 5]))"), "1/2");
    assert_eq!(ev("abs(variance(weighted_data([1.5, 2.25, 3.0], [1, 2, 5])) - 0.28125) < 10^-15"), "true");
    // the standard deviation is its square root; a large machine value does not cancel
    assert_eq!(ev("std_dev(weighted_data([1, 2, 3], [1, 2, 5]))^2"), "1/2");
    assert_eq!(ev("abs(variance(weighted_data([10^8 + 1., 10^8 + 2., 10^8 + 3.], [1, 2, 5])) - 0.5) < 10^-6"), "true");
}

#[test]
fn total_at_levels_counted_from_the_leaves() {
    let s = session(&[
        "t = array(a, [2, 3, 4]);",
        "total(t, -2) === total(t, 2)",
        "total(t, [-2, -1]) === map(&(total(&1, 2)), t)",
        "total(t, [2, 3]) === total(t, [-2, -1])",
        "total(t, -1) === total(t, inf)",
        "dimensions(total(t, [-2, -1]))",
        // a numeric array: the sums agree with the explicit ones
        "u = table(i + 10 j + 100 k, i, 2, j, 3, k, 4); total(u, [2, 3]) == [total(flatten(u[1])), total(flatten(u[2]))]",
    ]);
    assert_eq!(&s[1..5], &["true", "true", "true", "true"]);
    assert_eq!(s[5], "[2]");
    assert_eq!(s[6], "true");
}

#[test]
fn apply_with_heads() {
    // documented: the head p(x) is a part of level 1
    assert_eq!(ev("apply(f, p(x)(q(y)), [1], heads: true)"), "f(x)(f(y))");
    assert_eq!(ev("apply(f, p(x)(q(y)(r(z))), [0, 1], heads: true)"), "f(f(r(z)))");
    assert_eq!(ev("apply(f, p(x)(q(y)(r(z))), [0, 1], heads: false) === apply(f, p(x)(q(y)(r(z))), [0, 1])"), "true");
    // without compound heads, heads: true is the plain apply
    assert_eq!(ev("apply(f, g(h(a), k(b)), [1], heads: true) === apply(f, g(h(a), k(b)), [1])"), "true");
    assert_eq!(ev("apply(f, g(a, b), heads: true)"), "f(a, b)");
}

#[test]
fn p_norms_for_any_order() {
    assert_eq!(ev("norm([x, y, z], p) === (abs(x)^p + abs(y)^p + abs(z)^p)^(1/p)"), "true");
    // documented: norm([1, 2, 3], 1.5) = 4.334622872113609; the exact order 3/2 gives the same value
    assert_eq!(ev("abs(norm([1, 2, 3], 1.5) - 4.334622872113609) < 10^-14"), "true");
    assert_eq!(ev("abs(n(norm([1, 2, 3], 3/2), 30) - n((1 + 2^(3/2) + 3^(3/2))^(2/3), 30)) < 10^-25"), "true");
    // p = 2.0 is the Euclidean norm; the p-norm tends to the largest modulus
    assert_eq!(ev("abs(norm([1, 2, 3], 2.0) - sqrt(14.)) < 10^-14"), "true");
    assert_eq!(ev("abs(norm([1, -7, 3], 200.) - 7) < 10^-2"), "true");
    // the Frobenius norm by its capitalized name, of a symbolic matrix; an order below 1 is not a norm
    assert_eq!(ev("norm([[a, b], [c, d]], \"Frobenius\") === norm(flatten([[a, b], [c, d]]))"), "true");
    assert_eq!(ev("norm([1, 2, 3], 1/2)"), "failure(argument, norm, 2)");
}

#[test]
fn rules_as_associations_in_merge_and_key_sort() {
    assert_eq!(ev("merge([a -> x, a -> y, b -> z], f)"), "assoc(a -> f([x, y]), b -> f([z]))");
    assert_eq!(ev("merge([assoc(a -> x), a -> y, [b -> z]], identity)"), "assoc(a -> [x, y], b -> [z])");
    // the same as with the associations written out; a later rule in one list replaces an earlier one
    assert_eq!(ev("merge([a -> 1, b -> 2, a -> 3], total) === merge([assoc(a -> 1), assoc(b -> 2), assoc(a -> 3)], total)"), "true");
    assert_eq!(ev("merge([[b -> z, b -> w]], f)"), "assoc(b -> f([w]))");
    assert_eq!(ev("key_sort([b -> y, c -> z, a -> x])"), "assoc(a -> x, b -> y, c -> z)");
    assert_eq!(ev("key_sort([3 -> c, 1 -> a, 2 -> b]) === key_sort(assoc(3 -> c, 1 -> a, 2 -> b))"), "true");
}

#[test]
fn alphabetic_orders_of_languages() {
    // documented: Slovak orders c, č, ..., h, ch; Serbian tailors only the Cyrillic script (root order for Latin)
    let s = session(&[
        "words = [\"čeština\", \"hebrejčina\", \"hebrejčina\", \"chorvátčina\"];",
        "sort(words, alphabetic_order(\"Slovak\"))",
        "sort(words, alphabetic_order(\"Serbian\")) === sort(words, alphabetic_order)",
        "is_ordered(words, alphabetic_order(\"Slovak\"))",
        "reverse_sort(words, alphabetic_order(\"Slovak\")) === reverse(sort(words, alphabetic_order(\"Slovak\")))",
        // variations: ch after every h-word, č after every c-word
        "alphabetic_sort([\"cha\", \"hz\", \"ci\", \"ča\", \"cz\"], \"Slovak\")",
        // Spanish: ñ after n; Swedish: å, ä (= æ), ö (= ø) after z, ü as y
        "key_sort(assoc(\"año\" -> 1, \"anulado\" -> 2, \"amigo\" -> 3), alphabetic_order(\"Spanish\"))",
        "alphabetic_sort([\"ña\", \"nz\", \"o\", \"n\"], \"Spanish\")",
        "alphabetic_sort([\"Encyclopædia\", \"Encyclopedia\", \"Encyclopaedia\"], \"Swedish\")",
        "alphabetic_sort([\"öl\", \"zebra\", \"åka\", \"ära\", \"yxa\", \"über\"], \"Swedish\")",
    ]);
    assert_eq!(s[1], "[\"čeština\", \"hebrejčina\", \"hebrejčina\", \"chorvátčina\"]");
    assert_eq!(s[2], "true");
    assert_eq!(s[3], "true");
    assert_eq!(s[4], "true");
    assert_eq!(s[5], "[\"ci\", \"cz\", \"ča\", \"hz\", \"cha\"]");
    assert_eq!(s[6], "assoc(\"amigo\" -> 3, \"anulado\" -> 2, \"año\" -> 1)");
    assert_eq!(s[7], "[\"n\", \"nz\", \"ña\", \"o\"]");
    assert_eq!(s[8], "[\"Encyclopaedia\", \"Encyclopedia\", \"Encyclopædia\"]");
    assert_eq!(s[9], "[\"über\", \"yxa\", \"zebra\", \"åka\", \"ära\", \"öl\"]");
}

#[test]
fn dimensions_with_any_heads() {
    assert_eq!(ev("dimensions(f(g(1, 2, 3), h(4, 5, 6)), allowed_heads: full)"), "[2, 3]");
    assert_eq!(ev("array_depth(f(g(1, 2, 3), h(4, 5, 6)), allowed_heads: full)"), "2");
    // ragged parts end the array; one head per level is not needed, but equal lengths are
    assert_eq!(ev("dimensions(f(g(1, 2, 3), h(4, 5)), allowed_heads: full)"), "[2]");
    assert_eq!(ev("dimensions(f(g(1, 2), g(3, 4)), allowed_heads: full) === dimensions(f(g(1, 2), g(3, 4)), allowed_heads: all)"), "true");
}

#[test]
fn solve_with_more_inverse_functions() {
    // documented: acsch(x) == log 2 has x = csch(log 2) = 4/3; asech(x) == log 2 has x = sech(log 2) = 4/5
    assert_eq!(ev("abs(n(rewrite(x, first(solve(acsch(x) - log(2) == 0, x))) - 4/3, 30)) < 10^-25"), "true");
    assert_eq!(ev("abs(n(rewrite(x, first(solve(asech(x) == log(2), x))) - 4/5, 30)) < 10^-25"), "true");
    // csch(x) == 1 with x > 0: asinh(1); sech(x) == 1/2 over the reals: two solutions, both satisfy the equation
    assert_eq!(ev("abs(n(rewrite(x, first(solve(csch(x) == 1 and 0 < x, x))) - asinh(1), 30)) < 10^-25"), "true");
    assert_eq!(ev("map(&(abs(n(sech(rewrite(x, &1)) - 1/2, 30)) < 10^-25), solve(sech(x) == 1/2, x, reals))"), "[true, true]");
    // the Gudermannian function and its inverse: gd(x) == 0.8 over the reals, inverse_gd(x) == 0.8; |v| >= pi/2 none
    assert_eq!(ev("abs(gudermannian(rewrite(x, first(solve(gudermannian(x) == 0.8, x, reals)))) - 0.8) < 10^-15"), "true");
    assert_eq!(ev("abs(inverse_gudermannian(rewrite(x, first(solve(inverse_gudermannian(x) == 0.8, x)))) - 0.8) < 10^-15"), "true");
    assert_eq!(ev("solve(gudermannian(x) == 2, x, reals)"), "[]");
    // logistic sigmoid: log 4 for 0.8; the complex solutions add 2 pi i c; no solution outside (0, 1) over the reals
    assert_eq!(ev("abs(rewrite(x, first(solve(logistic_sigmoid(x) == 0.8, x, reals))) - n(log(4))) < 10^-14"), "true");
    assert_eq!(ev("solve(logistic_sigmoid(x) == 3/2, x, reals)"), "[]");
    assert_eq!(ev("rewrite(x, first(solve(logistic_sigmoid(x) == 1/3, x, reals)))"), "-log(2)");
    assert_eq!(ev("1 / (1 + exp(log(2)))"), "1/3");
    // sign and real_sign are 0 only at 0; d(sech(x)) == 0 at 0
    assert_eq!(ev("solve(sign(x) == 0, x)"), "[[x -> 0]]");
    assert_eq!(ev("solve(real_sign(x) == 0, x)"), "[[x -> 0]]");
    assert_eq!(ev("solve(d(sech(x), x) == 0, x, reals)"), "[[x -> 0]]");
    assert_eq!(ev("solve(acsch(x) == 0, x)"), "[]");
}

#[test]
fn elimination_modulo_a_prime_and_with_a_tolerance() {
    // documented: m over GF(5) has rank 2 and the reduced form [[1, 0, 4], [0, 1, 4], [0, 0, 0]]
    let s = session(&[
        "m = [[1, 0, 4], [2, 0, 3], [2, 1, 2]];",
        "row_reduce(m, modulus: 5)",
        "rank(m, modulus: 5)",
        // det(m) = 5: singular modulo 5 only; the reduced form times m's row space: r = row_reduce(m, modulus: 5),
        // every row of m is a combination of r's rows modulo 5 (m[1] = r[1], m[2] = 2 r[1] + 0 r[2] ... checked by rank)
        "det(m)",
        "map(&(rank(m, modulus: &1)), [2, 3, 5, 7])",
        "rank(join(m, row_reduce(m, modulus: 5)), modulus: 5)",
        // a rational entry a/b is a b^-1: 1/2 = 4 modulo 7
        "row_reduce([[1/2, 1], [1, 2]], modulus: 7)",
        // not a prime: the call stays
        "rank(m, modulus: 4)",
    ]);
    assert_eq!(s[1], "[[1, 0, 4], [0, 1, 4], [0, 0, 0]]");
    assert_eq!(s[2], "2");
    assert_eq!(s[3], "5");
    assert_eq!(s[4], "[3, 3, 2, 3]");
    assert_eq!(s[5], "2");
    assert_eq!(s[6], "[[1, 2], [0, 0]]");
    assert_eq!(s[7], "rank([[1, 0, 4], [2, 0, 3], [2, 1, 2]], modulus: 4)");
    // documented: singular values 1.7, 1e-10, 1e-20: tolerance 0 counts all, 1e-8 only the first
    let s = session(&[
        "m = n([[1, 1, 1], [0, 10^(-10), 0], [0, 0, 10^(-20)]]);",
        "[rank(m, tolerance: 0), rank(m), rank(m, tolerance: 10^(-8)), rank(m, tolerance: 10^(-15))]",
        "row_reduce(m, tolerance: 0) == identity_matrix(3)",
        "row_reduce(m, tolerance: 10^(-5))",
    ]);
    assert_eq!(s[1], "[3, 2, 1, 2]");
    assert_eq!(s[2], "true");
    assert_eq!(s[3], "[[1.0, 1.0, 1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]]");
}

#[test]
fn dense_target_structures() {
    assert_eq!(ev("hankel_matrix(3, target_structure: \"Dense\") === hankel_matrix(3)"), "true");
    assert_eq!(ev("toeplitz_matrix(5, target_structure: automatic) === toeplitz_matrix(5)"), "true");
    assert_eq!(ev("lower_triangularize([[1, 2, 3], [4, 5, 6], [7, 8, 9]], target_structure: \"Dense\")"), "[[1, 0, 0], [4, 5, 0], [7, 8, 9]]");
    assert_eq!(ev("upper_triangularize([[1, 2], [3, 4]], 1, target_structure: \"Sparse\") === upper_triangularize([[1, 2], [3, 4]], 1)"), "true");
    assert_eq!(ev("head(hankel_matrix(3, target_structure: \"Symmetric\"))"), "hankel_matrix");
}

#[test]
fn subsequences_picked_by_position() {
    assert_eq!(ev("subsequences([a, b, c, d_u, e_u], [3], 2)"), "[[a, b, c], [b, c, d_u]]");
    assert_eq!(ev("subsequences(range(20), all, [181]) === [subsequences(range(20))[181]]"), "true");
    assert_eq!(ev("subsequences([a, b, c, d_u], all, [11, 1, -2]) === subsequences([a, b, c, d_u])[[11, 9, 7, 5, 3, 1]]"), "true");
    assert_eq!(ev("subsequences([a, b, c, d_u], all, up_to(20)) === subsequences([a, b, c, d_u])"), "true");
    assert_eq!(ev("subsequences([a, b, c, d_u], [2], [2, 3]) === take(subsequences([a, b, c, d_u], [2]), [2, 3])"), "true");
    // a position past the end: the call stays
    assert_eq!(ev("head(subsequences([a, b], all, 20))"), "subsequences");
}

#[test]
fn map_indexed_with_heads() {
    assert_eq!(ev("map_indexed(f, [a, b, c], heads: true)"), "f(list, [0])(f(a, [1]), f(b, [2]), f(c, [3]))");
    assert_eq!(ev("map_indexed(f, p(x)(a, b, c), inf, heads: true)"), "f(f(p, [0, 0])(f(x, [0, 1])), [0])(f(a, [1]), f(b, [2]), f(c, [3]))");
    // every position given is the part's position: extracting it from the original gives the part
    assert_eq!(ev("x = g(h(a), k)(u, v(w)); map_indexed(&(extract(x, &2) === &1), x, [2], heads: true)"), "true(true, true)(u, true(true))");
    assert_eq!(ev("map_indexed(f, [a, b], heads: false) === map_indexed(f, [a, b])"), "true");
}
