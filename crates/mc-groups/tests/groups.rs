//! Tests of mc-groups on the assembled core: worked examples from the literature and internal consistency
//! (orbit-stabiliser, Lagrange, class equation, orthogonality of characters, RS bijectivity).
use mc_api::{Engine, StopFlag};

fn ev(src: &str) -> String {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn check(cases: &[(&str, &str)]) {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    for (src, want) in cases {
        let got = e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"));
        assert_eq!(got, *want, "{src}");
    }
}

#[test]
fn permutations() {
    check(&[
        ("cycles([[3, 1, 2], [5, 4], [6]])", "cycles([[1, 2, 3], [4, 5]])"),
        ("cycles([[1, 2], [2, 3]])", "cycles([[1, 2], [2, 3]])"),
        // first (1 2), then (2 3): 1 -> 2 -> 3, 3 -> 2, 2 -> 1
        ("permutation_product(cycles([[1, 2]]), cycles([[2, 3]]))", "cycles([[1, 3, 2]])"),
        ("permutation_inverse(cycles([[1, 2, 3, 4]]))", "cycles([[1, 4, 3, 2]])"),
        ("permutation_power(cycles([[1, 2, 3, 4, 5, 6]]), 2)", "cycles([[1, 3, 5], [2, 4, 6]])"),
        ("permutation_power(cycles([[1, 2, 3]]), -1)", "cycles([[1, 3, 2]])"),
        ("permutation_order(cycles([[1, 2, 3], [4, 5]]))", "6"),
        ("[permutation_sign(cycles([[1, 2, 3], [4, 5]])), permutation_sign([2, 1, 3]), permutation_sign(cycles([]))]", "[-1, -1, 1]"),
        ("permutation_cycle_type(cycles([[1, 2, 3], [5, 6]]), 7)", "[3, 2, 1, 1]"),
        ("permutation_list(cycles([[1, 3]]), 4)", "[3, 2, 1, 4]"),
        ("permutation_cycles([3, 1, 2, 4])", "cycles([[1, 3, 2]])"),
        ("permutation_image(cycles([[1, 3, 2]]), [1, 2, 3, 4])", "[3, 1, 2, 4]"),
        ("permutation_support(cycles([[2, 5], [7, 9, 8]]))", "[2, 5, 7, 8, 9]"),
    ]);
}

#[test]
fn group_orders() {
    check(&[
        ("group_order(symmetric_group(10))", "3628800"),
        ("group_order(symmetric_group(30))", "265252859812191058636308480000000"),
        ("group_order(alternating_group(8))", "20160"),
        ("group_order(dihedral_group(7))", "14"),
        ("group_order(abelian_group([2, 4, 3]))", "24"),
        ("group_order(mathieu_group(11))", "7920"),
        ("group_order(mathieu_group(12))", "95040"),
        ("group_order(mathieu_group(21))", "20160"),
        ("group_order(mathieu_group(22))", "443520"),
        ("group_order(mathieu_group(23))", "10200960"),
        ("group_order(mathieu_group(24))", "244823040"),
        // Rubik's cube group (Singmaster's labelling of the 48 movable facets)
        ("group_order(permutation_group([cycles([[1,3,8,6],[2,5,7,4],[9,33,25,17],[10,34,26,18],[11,35,27,19]]), cycles([[9,11,16,14],[10,13,15,12],[1,17,41,40],[4,20,44,37],[6,22,46,35]]), cycles([[17,19,24,22],[18,21,23,20],[6,25,43,16],[7,28,42,13],[8,30,41,11]]), cycles([[25,27,32,30],[26,29,31,28],[3,38,43,19],[5,36,45,21],[8,33,48,24]]), cycles([[33,35,40,38],[34,37,39,36],[3,9,46,32],[2,12,47,29],[1,14,48,27]]), cycles([[41,43,48,46],[42,45,47,44],[14,22,30,38],[15,23,31,39],[16,24,32,40]])]))", "43252003274489856000"),
    ]);
}

#[test]
fn membership_orbits_stabilisers() {
    check(&[
        ("g = mathieu_group(12); null", "null"),
        ("[is_group_element(g, cycles([[1, 2]])), is_group_element(g, cycles([[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]]))]", "[false, true]"),
        ("group_orbit(dihedral_group(6), 2)", "[1, 2, 3, 4, 5, 6]"),
        ("group_orbits(permutation_group([cycles([[1, 2], [4, 5]])]), 6)", "[[1, 2], [3], [4, 5], [6]]"),
        // orbit-stabiliser: |G| = |orbit| |stabiliser|
        ("group_order(group_stabilizer(g, 1)) * length(group_orbit(g, 1)) == group_order(g)", "true"),
        ("group_order(group_stabilizer(g, [1, 2, 3, 4, 5]))", "1"),
        ("group_order(group_stabilizer(mathieu_group(24), [1, 2, 3, 4, 5]))", "48"),
        ("s = group_stabilizer_chain(symmetric_group(4)); [first(s), length(s[2]) > 0]", "[[1, 2, 3], true]"),
        ("group_order(group_set_stabilizer(symmetric_group(6), [1, 2]))", "48"),
        ("m = mathieu_group(11); group_order(group_set_stabilizer(m, [1, 2, 3, 4, 5])) == length(select(group_elements(m), p => sort(permutation_image(p, [1, 2, 3, 4, 5])) == [1, 2, 3, 4, 5]))", "true"),
    ]);
}

#[test]
fn subgroups_and_series() {
    check(&[
        ("s4 = symmetric_group(4); v4 = permutation_group([cycles([[1, 2], [3, 4]]), cycles([[1, 3], [2, 4]])]); null", "null"),
        ("[is_subgroup(s4, v4), is_normal_subgroup(s4, v4), subgroup_index(s4, v4)]", "[true, true, 6]"),
        ("length(right_coset_representatives(s4, v4))", "6"),
        ("length(left_coset_representatives(s4, group_stabilizer(s4, 4)))", "4"),
        ("group_order(group_normal_closure(s4, cycles([[1, 2, 3]])))", "12"),
        ("group_order(group_centralizer(s4, cycles([[1, 2], [3, 4]])))", "8"),
        ("group_order(group_centralizer(symmetric_group(8), cycles([[1, 2, 3], [4, 5, 6]])))", "36"),
        ("group_order(group_center(dihedral_group(8)))", "2"),
        ("group_order(group_center(mathieu_group(11)))", "1"),
        ("group_order(group_intersection(s4, permutation_group([cycles([[1, 2, 3, 4]])])))", "4"),
        ("map(group_order, derived_series(s4))", "[24, 12, 4, 1]"),
        ("map(group_order, lower_central_series(s4))", "[24, 12]"),
        ("map(group_order, lower_central_series(dihedral_group(8)))", "[16, 4, 2, 1]"),
        ("[is_solvable(s4), is_solvable(symmetric_group(5)), is_nilpotent(dihedral_group(8)), is_nilpotent(s4)]", "[true, false, true, false]"),
        ("[is_abelian_group(abelian_group([2, 2])), is_abelian_group(s4)]", "[true, false]"),
        ("map(group_order, composition_series(s4))", "[24, 12, 4, 2, 1]"),
        ("map(group_order, composition_series(symmetric_group(5)))", "[120, 60, 1]"),
        ("map(group_order, composition_series(cyclic_group(12)))", "[12, 4, 2, 1]"),
        ("[is_simple_group(alternating_group(5)), is_simple_group(mathieu_group(11)), is_simple_group(alternating_group(4)), is_simple_group(cyclic_group(7))]", "[true, true, false, true]"),
        ("abelian_invariants(abelian_group([4, 6]))", "[2, 3, 4]"),
        ("abelian_invariants(s4)", "[2]"),
    ]);
}

#[test]
fn transitivity_and_blocks() {
    check(&[
        ("[is_transitive(dihedral_group(6)), is_transitive(dihedral_group(6), 7), is_primitive(dihedral_group(6)), is_primitive(dihedral_group(7))]", "[true, false, false, true]"),
        ("group_blocks(dihedral_group(6))", "[[1, 4], [2, 5], [3, 6]]"),
        ("group_blocks(dihedral_group(6), [1, 3])", "[[1, 3, 5], [2, 4, 6]]"),
        ("[is_primitive(mathieu_group(24)), is_primitive(symmetric_group(5))]", "[true, true]"),
    ]);
}

#[test]
fn conjugacy() {
    check(&[
        ("[is_conjugate(symmetric_group(5), cycles([[1, 2, 3]]), cycles([[2, 4, 5]])), is_conjugate(symmetric_group(5), cycles([[1, 2, 3]]), cycles([[1, 2], [3, 4]]))]", "[true, false]"),
        // (1 2 3 4 5) and its square are not conjugate in A5
        ("is_conjugate(alternating_group(5), cycles([[1, 2, 3, 4, 5]]), cycles([[1, 3, 5, 2, 4]]))", "false"),
        ("c = conjugating_element(symmetric_group(6), cycles([[1, 2, 3], [4, 5]]), cycles([[2, 6, 4], [1, 3]])); permutation_product(permutation_inverse(c), cycles([[1, 2, 3], [4, 5]]), c)", "cycles([[1, 3], [2, 6, 4]])"),
        ("conjugacy_class_sizes(symmetric_group(4))", "[1, 3, 6, 8, 6]"),
        ("conjugacy_class_sizes(alternating_group(5))", "[1, 15, 20, 12, 12]"),
        ("total(conjugacy_class_sizes(mathieu_group(11)))", "7920"),
        ("length(conjugacy_class_representatives(mathieu_group(12)))", "15"),
        // the class equation through the random method (|M24| is past the enumeration limit)
        ("total(conjugacy_class_sizes(mathieu_group(24)))", "244823040"),
        ("length(conjugacy_class_sizes(mathieu_group(24)))", "26"),
        ("map(length, conjugacy_classes(cyclic_group(3)))", "[1, 1, 1]"),
    ]);
}

#[test]
fn character_tables() {
    check(&[
        ("character_table(symmetric_group(3))", "[[1, 1, 1], [1, -1, 1], [2, 0, -1]]"),
        ("character_table(symmetric_group(4))", "[[1, 1, 1, 1, 1], [1, 1, -1, 1, -1], [2, 2, 0, -1, 0], [3, -1, 1, 0, -1], [3, -1, -1, 0, 1]]"),
        // A5: the golden-ratio characters
        ("t = character_table(alternating_group(5)); map(first, t)", "[1, 3, 3, 4, 5]"),
        ("expand([t[2][4] + t[3][4], t[2][4]*t[3][4]])", "[1, -1]"),
        // first orthogonality: sum_k |C_k| chi(g_k) conj(chi'(g_k)) = |G| delta
        ("s = conjugacy_class_sizes(mathieu_group(11)); t = character_table(mathieu_group(11)); [length(t), map(first, t)]", "[10, [1, 10, 10, 10, 11, 16, 16, 44, 45, 55]]"),
        ("simplify(expand(total(s * t[6] * conjugate(t[7]))))", "0"),
        ("simplify(expand(total(s * t[6] * conjugate(t[6]))))", "7920"),
        ("map(first, character_table(dihedral_group(4)))", "[1, 1, 1, 1, 2]"),
    ]);
}

#[test]
fn finitely_presented() {
    check(&[
        ("f = finitely_presented_group([a, b], [a^2, b^3, table([a, b], 5)]); group_order(f)", "60"),
        ("group_order(finitely_presented_group([a, b], [a^2, b^3, table([a, b], 4)]))", "24"),
        // Coxeter's binary polyhedral group <2, 3, 5>
        // <x, y | x^2 = y^3 = (x y)^5> is a central extension of A5 with abelianisation Z/19 (det [[2, -3], [-5, -2]])
        ("p = finitely_presented_group([r, s], [[r, r] -> [s, s, s], [s, s, s] -> table([r, s], 5)]); [group_order(p), abelian_invariants(p)]", "[2280, [19]]"),
        // the binary icosahedral group <s, t | (s t)^2 = s^3 = t^5>
        ("group_order(finitely_presented_group([s, t], [table([s, t], 2) -> [s, s, s], [s, s, s] -> t^5]))", "120"),
        ("group_order(finitely_presented_group([a, b], [[a, b, a^-1, b^-1]]))", "inf"),
        ("subgroup_index(f, [a])", "30"),
        ("coset_table(finitely_presented_group([a], [a^3]))", "[[2, 3], [3, 1], [1, 2]]"),
        ("group_order(permutation_group(f, [b]))", "60"),
        ("is_transitive(permutation_group(f, [b]))", "true"),
        ("abelian_invariants(finitely_presented_group([a, b], [a^4, b^6, [a, b, a^-1, b^-1]]))", "[2, 3, 4]"),
        ("abelian_invariants(finitely_presented_group([a, b, c], [[a, a, b]]))", "[0, 0]"),
        ("knuth_bendix(finitely_presented_group([a], [a^3]))", "[[a^2] -> [a^-1], [a, a^-1] -> [], [a^-1, a] -> [], [a^-2] -> [a]]"),
        ("word_normal_form(finitely_presented_group([a, b], [a^2, b^2, table([a, b], 3)]), [a, b, a, b, a])", "[b]"),
        ("length(knuth_bendix(finitely_presented_group([a, b], [[a, b] -> [b, a]])))", "8"),
    ]);
}

#[test]
fn posets() {
    check(&[
        ("d = poset(divisors(12), (a, b) => mod(b, a) == 0); poset_moebius_mu(d, 1, 12)", "0"),
        ("poset_moebius_mu(d, 2, 12)", "1"),
        ("map(x => poset_moebius_mu(d, 1, x), divisors(12)) == map(moebius_mu, divisors(12))", "true"),
        // the Boolean lattice of {1, 2, 3}: mu(empty, full) = (-1)^3
        ("b = poset(subsets([1, 2, 3]), (s, t) => is_subset(s, t)); poset_moebius_mu(b, [], [1, 2, 3])", "-1"),
        ("is_lattice(b)", "true"),
        ("linear_extension_count(b)", "48"),
        ("linear_extensions(poset([u, v, w], [[u, v], [u, w]]))", "[[u, v, w], [u, w, v]]"),
        ("poset_covers(poset([1, 2, 3], [[1, 2], [2, 3], [1, 3]]))", "[[1, 2], [2, 3]]"),
        ("is_lattice(poset([1, 2, 3, 4], [[1, 3], [1, 4], [2, 3], [2, 4]]))", "false"),
        ("linear_extension_count(poset(range(12), []))", "479001600"),
    ]);
}

#[test]
fn matroids() {
    check(&[
        ("matroid_rank(linear_matroid([[1, 0, 1, 1], [0, 1, 1, 2]]))", "2"),
        ("matroid_bases(linear_matroid([[1, 0, 1, 2], [0, 1, 1, 2]]))", "[[1, 2], [1, 3], [1, 4], [2, 3], [2, 4]]"),
        ("matroid_circuits(linear_matroid([[1, 0, 1, 2], [0, 1, 1, 2]]))", "[[3, 4], [1, 2, 3], [1, 2, 4]]"),
        ("matroid_rank(uniform_matroid(2, 4), [1, 2, 3])", "2"),
        ("tutte_polynomial(uniform_matroid(2, 4), x, y)", "2*x + 2*y + x^2 + y^2"),
        // the triangle: x^2 + x + y; K4: the classical polynomial
        ("tutte_polynomial(graphic_matroid([[1, 2], [2, 3], [3, 1]]), x, y)", "x + y + x^2"),
        ("expand(tutte_polynomial(graphic_matroid(subsets([1, 2, 3, 4], [2])), x, y))", "2*x + 2*y + 3*x^2 + 3*y^2 + 4*x*y + x^3 + y^3"),
        // T(1, 1) counts spanning trees: 16 for K4
        ("tutte_polynomial(graphic_matroid(subsets([1, 2, 3, 4], [2])), 1, 1)", "16"),
        ("tutte_polynomial(graphic_matroid(subsets([1, 2, 3, 4], [2])), 2, 2) == 2^6", "true"),
        ("length(matroid_bases(matroid([[1, 2], [1, 3], [2, 3]])))", "3"),
    ]);
}

#[test]
fn tableaux() {
    check(&[
        ("robinson_schensted([3, 1, 4, 2, 5])", "[[[1, 2, 5], [3, 4]], [[1, 3, 5], [2, 4]]]"),
        ("inverse_robinson_schensted([[1, 2, 5], [3, 4]], [[1, 3, 5], [2, 4]])", "[3, 1, 4, 2, 5]"),
        // the inverse permutation exchanges P and Q
        ("w = [4, 7, 1, 3, 6, 2, 5]; reverse(robinson_schensted(w)) == robinson_schensted(permutation_list(permutation_inverse(permutation_cycles(w))))", "true"),
        ("hook_lengths([3, 2])", "[[4, 3, 1], [2, 1]]"),
        ("standard_young_tableaux_count([3, 2])", "5"),
        ("standard_young_tableaux([2, 2])", "[[[1, 2], [3, 4]], [[1, 3], [2, 4]]]"),
        ("standard_young_tableaux_count([10, 9, 8, 7, 6, 5, 4, 3, 2, 1])", "44261486084874072183645699204710400"),
        // sum over shapes of f^2 = n!
        ("total(map(s => standard_young_tableaux_count(s)^2, integer_partitions(8)))", "40320"),
        ("robinson_schensted([1, 2, 2, 1])", "[[[1, 1, 2], [2]], [[1, 2, 3], [4]]]"),
    ]);
}

#[test]
fn polya() {
    check(&[
        ("cycle_index_polynomial(cyclic_group(4), x)", "(1/4)*x(1)^4 + (1/4)*x(2)^2 + (1/2)*x(4)"),
        ("cycle_index_polynomial(symmetric_group(3), x)", "(1/6)*x(1)^3 + (1/3)*x(3) + (1/2)*x(1)*x(2)"),
        // necklaces of 6 beads in 2 colours up to rotation: 14; bracelets (with reflections): 13
        ("[polya_count(cyclic_group(6), 2), polya_count(dihedral_group(6), 2)]", "[14, 13]"),
        ("polya_count(cyclic_group(4), [r, b])", "b*r^3 + r*b^3 + 2*b^2*r^2 + b^4 + r^4"),
        ("polya_count(symmetric_group(12), 3)", "91"),
    ]);
}

#[test]
fn unevaluated_on_bad_input() {
    assert_eq!(ev("permutation_order(cycles([[0, 1]]))"), "permutation_order(cycles([[0, 1]]))");
    assert_eq!(ev("group_order(foo)"), "group_order(foo)");
}
