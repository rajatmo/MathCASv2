//! Round 36: fixed wrong answers of linear algebra, lists and associations, one test per cause. Each value
//! is checked against a second route (an identity, the defining equation, a second function).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread with a big stack (one worker thread, fresh session); `TIMEOUT` after 60 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("thread");
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn a_slot_function_without_slots_ignores_its_arguments() {
    assert_eq!(ev("array(&(x), [2, 2])"), "[[x, x], [x, x]]");
    // second route: the same array from constant_array
    assert_eq!(ev("dims = [3, 2, 4]; constant_array(c, dims) === array(&(c), dims)"), "true");
    assert_eq!(ev("&(&(f(&1, &1)))(x)(y)"), "f(y, y)");
    assert_eq!(ev("map_at(&(x), [[a, b, c], [d, e, f]], [2, 1])"), "[[a, b, c], [x, e, f]]");
}

#[test]
fn position_of_an_association_walks_the_values_at_key_paths() {
    assert_eq!(ev("position(assoc(\"a\" -> [1, b]), _)"), "[[0], [key(\"a\"), 0], [key(\"a\"), 1], [key(\"a\"), 2], [key(\"a\")], []]");
    assert_eq!(
        ev("position(assoc(\"a\" -> 1, \"b\" -> 2, \"c\" -> 3, \"d\" -> 4), (t_pt1_: Int when is_prime(t_pt1)))"),
        "[[key(\"b\")], [key(\"c\")]]"
    );
    // the keys are no parts: a key is not found, the value at that key is
    assert_eq!(ev("position(assoc(\"a\" -> [1, b]), \"a\")"), "[]");
    assert_eq!(ev("first_position(assoc(\"a\" -> 1, \"b\" -> 2, \"c\" -> 3), 2)"), "[key(\"b\")]");
    // second route: each key path read back with the value lookup
    assert_eq!(ev("a = assoc(\"a\" -> [1, b], \"c\" -> 5); map(p => a[p[1][1]], position(a, 5))"), "[5]");
}

#[test]
fn map_indexed_of_an_association_gives_the_keys_as_positions() {
    assert_eq!(ev("map_indexed(f, assoc(a -> 1, b -> 2, c -> 3), [1])"), "assoc(a -> f(1, [key(a)]), b -> f(2, [key(b)]), c -> f(3, [key(c)]))");
    assert_eq!(
        ev("map_indexed(h, assoc(a -> assoc(b -> c), d -> [e]), [2])"),
        "assoc(a -> assoc(b -> h(c, [key(a), key(b)])), d -> [h(e, [key(d), 1])])"
    );
}

#[test]
fn select_gives_elements_indices_or_a_mask_by_its_rule() {
    assert_eq!(ev("select([1, 2, 4, 7, 6, 2], &(&1 > 2) -> \"Index\")"), "[3, 4, 5]");
    assert_eq!(ev("select(range(10), is_odd -> [\"Element\", \"Index\"])"), "assoc(\"Element\" -> [1, 3, 5, 7, 9], \"Index\" -> [1, 3, 5, 7, 9])");
    // second route: the elements at the indices are the selected elements; the mask marks the same places
    assert_eq!(
        ev("xs = [10, 25, 30, 45, 5]; f = &(&1 > 20); map(i => xs[i], select(xs, f -> \"Index\")) === select(xs, f)"),
        "true"
    );
    assert_eq!(ev("xs = [10, 25, 30, 45, 5]; f = &(&1 > 20); select(xs, f -> \"BitVectorMask\") === map(f, xs)"), "true");
    assert_eq!(ev("select_first([2, 3, 4, 5], is_odd -> [\"Element\", \"Index\"])"), "assoc(\"Element\" -> 3, \"Index\" -> 2)");
    assert_eq!(ev("select_first([2, 3, 4, 5], is_odd -> \"BitVectorMask\")"), "[false, true, false, false]");
}

#[test]
fn partition_with_an_overhang_pads_a_matrix_in_both_directions() {
    let p = ev("partition([[11, 12, 13], [21, 22, 23], [31, 32, 33]], [2, 2], 1, 1, xx)");
    assert_eq!(
        p,
        "[[[[11, 12], [21, 22]], [[12, 13], [22, 23]], [[13, xx], [23, xx]]], [[[21, 22], [31, 32]], [[22, 23], [32, 33]], [[23, xx], [33, xx]]], [[[31, 32], [xx, xx]], [[32, 33], [xx, xx]], [[33, xx], [xx, xx]]]]"
    );
    // second route: the blocks (no overhang) of the matrix padded by hand with a row and a column of xx
    assert_eq!(
        ev("m = [[11, 12, 13, xx], [21, 22, 23, xx], [31, 32, 33, xx], [xx, xx, xx, xx]];             partition([[11, 12, 13], [21, 22, 23], [31, 32, 33]], [2, 2], 1, 1, xx) === partition(m, [2, 2], [1, 1])"),
        "true"
    );
    // cyclic padding when none is given: the matrix wraps around
    assert_eq!(
        ev("partition([[11, 12, 13], [21, 22, 23], [31, 32, 33]], [2, 2], 1, 1)[1][3]"),
        "[[13, 11], [23, 21]]"
    );
}

#[test]
fn cases_and_first_case_with_heads_see_the_head() {
    assert_eq!(ev("cases([a, 5, pi], _: Sym, heads: true)"), "[list, a, pi]");
    assert_eq!(ev("first_case([a, 5, pi], _: Sym, heads: true)"), "list");
    assert_eq!(ev("cases([1, 2, f(a, b)], _, [0, inf], heads: true)"), "[list, 1, 2, f, a, b, f(a, b), [1, 2, f(a, b)]]");
}

#[test]
fn sequence_split_takes_rules_and_plain_patterns_as_alternatives() {
    assert_eq!(ev("sequence_split([1, 2, 3, 4, 5, 6, 7], [[2, 3] -> \"X\", [6]])"), "[[1], \"X\", [4, 5], [7]]");
    // second route: splitting on the plain runs alone drops them
    assert_eq!(ev("sequence_split([1, 2, 3, 4, 5, 6, 7], [2, 3])"), "[[1], [4, 5, 6, 7]]");
}

#[test]
fn operator_forms_of_linear_solve_nearest_merge_nest_list() {
    // the solution satisfies the system
    assert_eq!(ev("m = [[1, 2], [3, 4]]; f = linear_solve(m); b = [5, 6]; [f(b), dot(m, f(b)) == b]"), "[[-4, 9/2], true]");
    assert_eq!(ev("merge(total)([assoc(a -> 1, b -> 2), assoc(a -> 3, b -> 1)])"), "assoc(a -> 4, b -> 3)");
    assert_eq!(ev("nest_list(sin, 4)(0.5) === nest_list(sin, 0.5, 4)"), "true");
    assert_eq!(ev("nearest([[-1, -1, 0] -> a, [1, 1, 0] -> c])([2, 0, 0])"), "[c]");
}

#[test]
fn fold_pair_stops_at_the_first_throw() {
    // 1, 1, 2, 6, 24, 120, 720, 5040: the first state above 1000
    assert_eq!(ev("catch(fold_pair(&([(if &1 > 1000 then throw(&1)), &1 * &2]), 1, range(10)))"), "5040");
    // second route: fold_pair_list finds the same state
    assert_eq!(ev("first(select(fold_pair_list(&([&1, &1 * &2]), 1, range(10)), &(&1 > 1000)))"), "5040");
}

#[test]
fn map_at_applies_twice_at_a_repeated_position_and_nothing_at_no_position() {
    assert_eq!(ev("map_at(f, [a, b, c, d], [[1], [3], [1]])"), "[f(f(a)), b, f(c), d]");
    assert_eq!(ev("map_at(f, h(a, b), [])"), "h(a, b)");
    assert_eq!(ev("delete(h(a, b), [])"), "h(a, b)");
}

#[test]
fn insert_into_an_association_before_a_key_with_a_list_or_into_a_value() {
    // key(k) names a key, an integer is a position
    assert_eq!(ev("insert(assoc(2 -> [a, b], 1 -> [c, d]), 3 -> [e, f], key(2))"), "assoc(3 -> [e, f], 2 -> [a, b], 1 -> [c, d])");
    assert_eq!(ev("insert(assoc(2 -> [a, b], 1 -> [c, d]), 3 -> [e, f], 2)"), "assoc(2 -> [a, b], 3 -> [e, f], 1 -> [c, d])");
    assert_eq!(ev("insert(assoc(a -> 1, b -> 2), [c -> 3, d -> 4], key(a))"), "assoc(c -> 3, d -> 4, a -> 1, b -> 2)");
    assert_eq!(ev("insert(assoc(1 -> [a, b], 2 -> [c, d]), x, [key(2), 1])"), "assoc(1 -> [a, b], 2 -> [x, c, d])");
    assert_eq!(ev("insert(assoc(1 -> [a, b], 2 -> [c, d]), x, [2, 1])"), "assoc(1 -> [a, b], 2 -> [x, c, d])");
}

#[test]
fn reverse_of_an_association_at_levels_keeps_the_keys() {
    assert_eq!(
        ev("reverse(assoc(a :> [1, 2], b -> [3, 4], c -> [5, 6]), [1, 2])"),
        "assoc(c -> [6, 5], b -> [4, 3], a :> [2, 1])"
    );
}

#[test]
fn extract_reads_key_paths_and_positions_of_an_association() {
    let a = "a = assoc(2 -> 1 + x^2, 3 -> y^2 - 1, 1 -> x + y + z);";
    assert_eq!(ev(&format!("{a} extract(a, [key(1)])")), "x + y + z");
    assert_eq!(ev(&format!("{a} extract(a, [1])")), "1 + x^2");
    assert_eq!(ev(&format!("{a} extract(a, [key(3), 1])")), "-1");
    // second route: the paths position gives read back with extract
    assert_eq!(ev("t = assoc(a -> 1, b -> 2); extract(t, position(t, _: Int))"), "[1, 2]");
}

#[test]
fn distance_matrix_follows_the_data_type() {
    // strings: the edit distance (insert, delete, substitute): abcd -> bcde is two edits
    assert_eq!(ev("distance_matrix([\"abcd\", \"bcde\", \"xyz\"])"), "[[0, 2, 4], [2, 0, 4], [4, 4, 0]]");
    // booleans: the Jaccard dissimilarity (places that differ over places with a true)
    assert_eq!(
        ev("distance_matrix([[true, false, true], [true, true, true], [true, false, false]])"),
        "[[0, 1/3, 1/2], [1/3, 0, 2/3], [1/2, 2/3, 0]]"
    );
    // a mixed row: a non-number adds 0 or 1 to the squared distance: sqrt(0.01 + 1)
    assert_eq!(ev("abs(distance_matrix([[1.2, \"A\"], [1.1, \"B\"]])[1][2] - sqrt(0.01 + 1)) < 1.0e-12"), "true");
    // numbers are unchanged
    assert_eq!(ev("distance_matrix([[1, 2], [4, 6]])"), "[[0, 5], [5, 0]]");
}

#[test]
fn pad_with_a_matrix_tiles_it_over_the_pad() {
    assert_eq!(
        ev("pad_left([[aa, bb], [cc]], [4, 4], [[x, y], [z]])"),
        "[[x, y, x, y], [z, z, z, z], [x, y, aa, bb], [z, z, z, cc]]"
    );
    assert_eq!(
        ev("pad_right([[aa, bb], [cc]], [4, 4], [[x, y], [z]])"),
        "[[aa, bb, x, y], [cc, z, z, z], [x, y, x, y], [z, z, z, z]]"
    );
}

#[test]
fn take_largest_orders_quantities_in_a_common_unit_micrograms_included() {
    assert_eq!(
        ev("take_largest([quantity(1, \"Grams\"), quantity(2, \"Kilograms\"), quantity(3, \"Milligrams\"), quantity(4, \"Micrograms\")], 3)"),
        "[quantity(2, \"Kilograms\"), quantity(1, \"Grams\"), quantity(3, \"Milligrams\")]"
    );
    // second route: the values in grams (2 kg = 2000 g, 3 mg = 0.003 g, 4 ug = 0.000004 g)
    assert_eq!(ev("quantity(4, \"Micrograms\") < quantity(3, \"Milligrams\")"), "true");
}

#[test]
fn first_position_passes_heads_false_on() {
    assert_eq!(ev("first_position(x^2 + y^2, pow, heads: false)"), "missing(\"NotFound\")");
    // second route: with the heads the position of the head `pow` of x^2 is found
    assert_eq!(ev("position(x^2 + y^2, pow, heads: false)"), "[]");
}

#[test]
fn vector_angle_of_parallel_machine_vectors_is_zero_to_rounding() {
    // acos of the cosine gave 2.1e-8 for this pair
    assert_eq!(ev("vector_angle([1, 2], [0.1, 0.2]) < 1.0e-15"), "true");
    // second route: the cross product vanishes, and an ordinary angle agrees with the exact one to 1e-15
    assert_eq!(ev("abs(vector_angle([1., 2.], [3., 1.]) - pi/4) < 1.0e-15"), "true");
    assert_eq!(ev("vector_angle([1, 0], [1, 1])"), "(1/4)*pi");
}

#[test]
fn qr_full_completes_q_to_a_square_orthogonal_matrix() {
    // a singular 3 x 3 and a tall matrix: q is square and orthogonal, r has the zero rows, and q . r gives m back
    assert_eq!(
        ev("m = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]; t = qr(m, \"Full\"); [length(t[1][1]), simplify(dot(t[1], t[2])) == m, simplify(dot(transpose(t[1]), t[1])) == identity_matrix(3), t[2][3]]"),
        "[3, true, true, [0, 0, 0]]"
    );
    assert_eq!(
        ev("m = [[1, 2], [3, 4], [5, 6]]; t = qr(m, full); [dimensions(t[1]), dimensions(t[2]), simplify(dot(t[1], t[2])) == m]"),
        "[[3, 3], [3, 2], true]"
    );
}

#[test]
fn replace_part_reaches_the_values_of_a_nested_association() {
    assert_eq!(ev("replace_part([assoc(\"x\" -> 1, \"y\" -> 2)], [1, 1] -> g)"), "[assoc(\"x\" -> g, \"y\" -> 2)]");
    assert_eq!(ev("replace_part(assoc(\"x\" -> 1, \"y\" -> 2), [0] -> f)"), "f(1, 2)");
}

// ---- conventions: the defining property of each factorization holds ----

fn close(expr: &str) -> String {
    ev(&format!("{expr} < 1.0e-12"))
}

#[test]
fn qr_svd_eigen_null_space_satisfy_their_defining_properties() {
    // qr(m) = [q, r]: m = q.r, q has orthonormal columns, r is upper triangular (the documented convention)
    let m = "m = [[1.2, 2.3, 3.4], [2.3, 4.5, 5.6], [3.2, 7.6, 6.5]]; t = qr(m); q = t[1]; r = t[2];";
    assert_eq!(close(&format!("{m} max(abs(flatten(dot(q, r) - m)))")), "true");
    assert_eq!(close(&format!("{m} max(abs(flatten(dot(transpose(q), q) - identity_matrix(3))))")), "true");
    assert_eq!(ev(&format!("{m} upper_triangular_matrix_q(map(row => map(x => if abs(x) < 1.0e-13 then 0 else x, row), r))")), "true");
    // svd(m) = [u, s, v]: m = u.s.v^T with orthonormal u and v and the singular values in decreasing order
    let svd = "m = [[0.5, 1], [2, 2.5]]; t = svd(m); u = t[1]; s = t[2]; v = t[3];";
    assert_eq!(close(&format!("{svd} max(abs(flatten(dot(u, s, transpose(v)) - m)))")), "true");
    assert_eq!(close(&format!("{svd} max(abs(flatten(dot(transpose(u), u) - identity_matrix(2))))")), "true");
    assert_eq!(close(&format!("{svd} max(abs(flatten(dot(transpose(v), v) - identity_matrix(2))))")), "true");
    assert_eq!(ev(&format!("{svd} s[1][1] >= s[2][2] and s[2][2] > 0")), "true");
    // eigensystem: m v = lambda v for each pair, unit eigenvectors
    let eig = "m = table(n(1 / (i + j + 1)), i, 3, j, 3); t = eigensystem(m); l = t[1]; w = t[2];";
    assert_eq!(close(&format!("{eig} max(map(k => max(abs(dot(m, w[k]) - l[k]*w[k])), range(3)))")), "true");
    assert_eq!(close(&format!("{eig} max(map(k => abs(norm(w[k]) - 1), range(3)))")), "true");
    // null_space: m.n = 0 for every basis vector, the basis has length n - rank, and the vectors are independent
    assert_eq!(
        ev("m = [[1, 2, 3, 4, 5], [2, 4, 6, 1, 3], [3, 6, 9, 5, 8]]; ns = null_space(m); [length(ns), 5 - rank(m), every(ns, v => every(dot(m, v), x => x == 0)), rank(ns)]"),
        "[3, 3, true, 3]"
    );
}

#[test]
fn range_space_spans_the_columns() {
    // every column of m is a combination of the basis (residual of the least-squares fit), and the basis has rank(m) vectors
    assert_eq!(
        ev("m = [[1.5, 4.75, -3.2], [-1.7, 6.7, -9.3], [4.9, -8.65, 15.4]]; b = range_space(m); [length(b), rank(m), max(map(c => norm(dot(transpose(b), least_squares(transpose(b), c)) - c), transpose(m))) < 1.0e-12]"),
        "[2, 2, true]"
    );
}

#[test]
fn recorded_conventions_with_a_second_route() {
    // interpolation: the default is the documented not-a-knot spline; the local cubic is the option
    assert_eq!(ev("interpolation([1, 2, 3, 5, 8, 5], interpolation_order: 3)(2.5)"), "2.4375");
    // inverse_erf iterated: each step inverts erf (erf(next) = previous)
    assert_eq!(
        ev("xs = fixed_point_list(inverse_erf, 0.5, 10); max(map(i => abs(erf(xs[i + 1]) - xs[i]), range(10))) < 1.0e-14"),
        "true"
    );
    // matrix_function of [[0, a], [b, 0]]: the two forms of the entries (sqrt(a b) against sqrt(a) sqrt(b)) agree
    assert_eq!(
        ev("a = 2.0; b = 3.0; s = sqrt(a*b); f = exp; \
            abs((f(-s) + f(s))/2 - (f(-sqrt(a)*sqrt(b)) + f(sqrt(a)*sqrt(b)))/2) < 1.0e-13 and \
            abs(-a*(f(-s) - f(s))/(2*s) - sqrt(a)*(f(sqrt(a)*sqrt(b)) - f(-sqrt(a)*sqrt(b)))/(2*sqrt(b))) < 1.0e-13"),
        "true"
    );
    // characteristic_polynomial is monic (det(x I - m)): det(m - x I) differs from it by (-1)^n
    assert_eq!(ev("m = [[1, 2, 3], [4, 5, 6], [7, 8, 10]]; expand(characteristic_polynomial(m, x) + det(m - x*identity_matrix(3)))"), "0");
    // a key lookup on an association is by key, positional access goes through values
    assert_eq!(ev("t = assoc(2 -> b, 1 -> c); [t[1], values(t)[1]]"), "[c, b]");
}

#[test]
fn small_list_fixes_second_round() {
    assert_eq!(ev("array_pad([[], []], 1, x)"), "[[x, x], [x, x], [x, x], [x, x]]");
    assert_eq!(ev("array_pad([[], []], 1, \"Fixed\")"), "[[], [], [], []]");
    assert_eq!(
        ev("bin_lists([1.5 -> \"a\", 3 -> \"b\", 2.5 -> \"c\", 1 -> \"d\", 7 -> \"f\"], 2)"),
        "[[\"a\", \"d\"], [\"b\", \"c\"], [], [\"f\"]]"
    );
    assert_eq!(ev("array_reshape([10, 20, 30, 40, 50, 60, 70], [5, 3], \"Extrapolated\")"), "[[10, 20, 30], [40, 50, 60], [70, 80, 90], [100, 110, 120], [130, 140, 150]]");
    // a pair that is not one entry per coordinate pads below and above every coordinate
    assert_eq!(ev("coordinate_bounds([[-9, 1, 0], [8, 2, 3]], [0, 1])"), "[[-9, 9], [1, 3], [0, 4]]");
    // ragged numeric points: dynamic time warping, a distance (symmetric, zero on the diagonal)
    assert_eq!(
        ev("d = distance_matrix([[1.1, 1.2, 1.3, 1.4, 1.5], [1.2, 1.3], [1.4, 1.5, 1.6]]); [abs(d[1][2] - 0.4) < 1e-12, abs(d[1][3] - 0.7) < 1e-12, d[2][3] == d[3][2], d[1][1] == 0]"),
        "[true, true, true, true]"
    );
    // multi-level rotation: rows then columns; second route: two single rotations
    assert_eq!(ev("rotate_left([[a, b], [c, d]], [1, 1]) === rotate_left(map(r => rotate_left(r, 1), [[a, b], [c, d]]), 1)"), "true");
    assert_eq!(ev("take_list(array(&(&1*10 + &2), [6, 6]), [4, 2], [3, 2, 1])[1][2]"), "[[14, 15], [24, 25], [34, 35], [44, 45]]");
    // second route: the blocks flatten back to the matrix
    assert_eq!(ev("m = array(&(&1*10 + &2), [6, 6]); array_flatten(take_list(m, [4, 2], [3, 2, 1])) === m"), "true");
    assert_eq!(ev("orthogonalize([[1, 0], [1, 1.0e-10]], tolerance: 10^(-5))"), "[[1, 0]]");
    assert_eq!(ev("op = flatten_at(1); op([[1, 2], [3, 4]])"), "[1, 2, [3, 4]]");
}

#[test]
fn rotation_in_the_plane_of_two_vectors() {
    assert_eq!(ev("rotation_matrix(t, [[1, 0], [0, 1]])"), "[[cos(t), -sin(t)], [sin(t), cos(t)]]");
    assert_eq!(ev("rotation_matrix(t, [[1, 0, 0], [0, 0, 1]])"), "[[cos(t), 0, -sin(t)], [0, 1, 0], [sin(t), 0, cos(t)]]");
    // second route: orthogonal with determinant 1, numerically
    assert_eq!(
        ev("r = rotation_matrix(0.7, [[1, 2, 0], [0, 1, 3]]); [max(abs(flatten(dot(r, transpose(r)) - identity_matrix(3)))) < 1e-14, abs(det(r) - 1) < 1e-14]"),
        "[true, true]"
    );
}

#[test]
fn cellular_automaton_all_cells() {
    // 2 steps of a single cell reach 5 cells; every 5-cell row but two is reachable by some elementary rule
    assert_eq!(ev("cellular_automaton(110, [[1], 0], [2, all])"), "[[0, 0, 1, 0, 0], [0, 1, 1, 0, 0], [1, 1, 1, 0, 0]]");
    assert_eq!(
        ev("complement(tuples([0, 1], 5), table(last(cellular_automaton(i, [[1], 0], [2, all])), i, range(0, 255)))"),
        "[[0, 1, 1, 0, 1], [1, 0, 1, 1, 0]]"
    );
}
