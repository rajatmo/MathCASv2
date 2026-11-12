//! The functions of the language library on the assembled engine: each case is source text and the printed value.
use mc_api::{Engine, StopFlag};

fn run(cases: &[(&str, &str)]) {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    let mut bad = vec![];
    for (src, want) in cases {
        let got = e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"));
        if &got != want {
            bad.push(format!("{src}
   want {want}
   got  {got}"));
        }
    }
    assert!(bad.is_empty(), "{} of {} cases differ:
{}", bad.len(), cases.len(), bad.join("
"));
}

#[test]
fn elementwise_and_threaded() {
    run(&[
        ("elementwise(f)([a, b], [1, 2], [x, y])", "[f(a, 1, x), f(b, 2, y)]"),
        ("elementwise(f)([[a], [b]], [1, 2], [x, y])", "[[f(a, 1, x)], [f(b, 2, y)]]"),
        ("elementwise(f)()", "f()"),
        ("elementwise(f)([])", "[]"),
        ("elementwise(f)(x)", "f(x)"),
        ("elementwise(f)([a, b, c], [1, 2])", "failure(shape_mismatch)"),
        ("elementwise(f)([[1, 2, 3], [4, 5, 6]], [a, b])", "[[f(1, a), f(2, a), f(3, a)], [f(4, b), f(5, b), f(6, b)]]"),
        ("elementwise(f)([[1, 2, 3], [4, 5, 6]], threaded([a, b, c]))", "[[f(1, a), f(2, b), f(3, c)], [f(4, a), f(5, b), f(6, c)]]"),
        ("elementwise(f)([[1, 2], [3, 4]], threaded([x, y], 1))", "[[f(1, x), f(2, x)], [f(3, y), f(4, y)]]"),
        ("elementwise(f)(assoc(a -> 1, b -> 2), 3)", "assoc(a -> f(1, 3), b -> f(2, 3))"),
        ("elementwise(f)(threaded([x, y]), threaded([1, 2]))", "threaded([f(x, 1), f(y, 2)])"),
        ("elementwise(f)([[1, 2], [3, 4]], threaded([x, y, z]))", "failure(shape_mismatch)"),
        ("elementwise(f)([[1, 2], [3, 4]], threaded([x, y], 3))", "failure(threaded_level, 3)"),
        ("elementwise(f)([[[1, 2], [3, 4]], [[5, 6], [7, 8]]], threaded([[x, y], [z, w]], -2))", "[[[f(1, x), f(2, y)], [f(3, z), f(4, w)]], [[f(5, x), f(6, y)], [f(7, z), f(8, w)]]]"),
        ("elementwise(add)(threaded([1, 2]), threaded([[1, 2], [3, 4]]))", "threaded([[2, 4], [4, 6]])"),
    ]);
}

#[test]
fn join_across() {
    let l = r#"[assoc("k" -> 1, "v" -> "a"), assoc("k" -> 2, "v" -> "b")]"#;
    let r = r#"[assoc("k" -> 1, "w" -> "x"), assoc("k" -> 3, "w" -> "y")]"#;
    run(&[
        (&format!(r#"join_across({l}, {r}, "k")"#), r#"[assoc("k" -> 1, "v" -> "a", "w" -> "x")]"#),
        (&format!(r#"join_across({l}, {r}, "k", "Left")"#), r#"[assoc("k" -> 1, "v" -> "a", "w" -> "x"), assoc("k" -> 2, "v" -> "b", "w" -> missing("Unmatched"))]"#),
        (&format!(r#"join_across({l}, {r}, "k", "right")"#), r#"[assoc("k" -> 1, "v" -> "a", "w" -> "x"), assoc("k" -> 3, "v" -> missing("Unmatched"), "w" -> "y")]"#),
        (&format!(r#"length(join_across({l}, {r}, "k", "outer"))"#), "3"),
        (r#"join_across([assoc("k" -> 1, "v" -> "a")], [assoc("k" -> 1, "v" -> "x")], "k")"#, r#"[assoc("k" -> 1, "v" -> "a")]"#),
        (r#"join_across([assoc("k" -> 1, "v" -> "a")], [assoc("k" -> 1, "v" -> "x")], "k", key_collision_function: right)"#, r#"[assoc("k" -> 1, "v" -> "x")]"#),
        (r#"join_across([assoc("k" -> 1)], [assoc("j" -> 1)], "k" -> "j")"#, r#"[assoc("k" -> 1, "j" -> 1)]"#),
        (r#"join_across([assoc("k" -> 1, "m" -> 5, "v" -> "a")], [assoc("k" -> 1, "m" -> 5, "w" -> "x")], ["k", "m"])"#, r#"[assoc("k" -> 1, "m" -> 5, "v" -> "a", "w" -> "x")]"#),
        (r#"length(join_across([assoc("k" -> 1, "v" -> "a"), assoc("k" -> 1, "v" -> "b")], [assoc("k" -> 1, "w" -> "x"), assoc("k" -> 1, "w" -> "y")], "k"))"#, "4"),
        (r#"join_across([assoc(a -> 1, b -> 2)], [assoc(a -> 2, b -> 1)], [key(a) -> key(b), key(b) -> key(a)], "Left", key_collision_function: (x => [left(x), right(x)]))"#, "[assoc(left(a) -> 1, right(a) -> 2, left(b) -> 2, right(b) -> 1)]"),
        ("join_across([[1]], [assoc(a -> 1)], a)", "failure(argument, join_across, 1)"),
        (r#"join_across([assoc(a -> 1)], [assoc(a -> 1)], a, "sideways")"#, "failure(argument, join_across, 4)"),
    ]);
}

#[test]
fn boolean_functions() {
    run(&[
        ("boolean_function(30, 3)(true, false, true)", "false"),
        ("boolean_table(boolean_function(30, 3)(a, b, c), [a, b, c])", "[false, false, false, true, true, true, true, false]"),
        ("boolean_function(30, 3)(true, x, false)", "boolean_function(1, 1)(x)"),
        ("boolean_function(30, [x, y, z])", "x and not y and not z or not x and y or not x and z"),
        ("boolean_function([true, false, false, true])", "boolean_function(9, 2)"),
        ("boolean_function(5, 2)", "boolean_function(5, 2)"),
        ("boolean_function(16)", "boolean_function(16, 3)"),
        ("boolean_function([[false, false] -> true, [false, true] -> false, [true, false] -> true, [true, true] -> true], [x, y])", "x or not y"),
        ("boolean_function(16, 2)", "failure(argument, boolean_function, 1)"),
        ("boolean_function(0, 3)(a, b, c)", "false"),
        ("boolean_function(2^8 - 1, 3)(a, b, c)", "true"),
        ("boolean_convert(z and a, \"boolean_function\")", "boolean_function(8, 2)(a, z)"),
        ("boolean_counting_function([2, 3], 4)(true, true, false, true)", "true"),
        ("boolean_counting_function(2, 3)(a, b, c) === boolean_counting_function(2, 3)(a, b, c)", "true"),
        ("boolean_table(boolean_counting_function(2, 3)(a, b, c), [a, b, c])", "[false, true, true, true, true, true, true, true]"),
        ("boolean_table(boolean_counting_function([[1, 3]], 3)(a, b, c), [a, b, c])", "[true, false, false, true, false, true, true, false]"),
        ("boolean_convert(equivalent(x, y, z))", "x and y and z or not x and not y and not z"),
        ("boolean_convert(xor(x, y), \"CNF\")", "(not x or not y) and (x or y)"),
        ("boolean_convert(xor(x, y), nand)", "nand(nand(x, not y), nand(not x, y))"),
        ("boolean_convert(xor(x, y), \"nor\")", "nor(nor(not x, not y), nor(x, y))"),
        ("boolean_convert(xor(x, y, z), \"dnf\", x and (y or z))", "y and z"),
        ("boolean_convert(true)", "true"),
        ("boolean_convert(xor(a, b, c), \"ITE\")", "if a then if b then if c then true else false else if c then false else true else if b then if c then false else true else if c then true else false"),
        ("boolean_function(158, [a, b, c])", "a and not b and not c or not a and b or not a and c or b and c"),
        ("boolean_counting_function([2, 3], [a, b, c])", "a and b or a and c or b and c"),
        ("logical_expand(a or not a)", "true"),
        ("boolean_convert(a and not a)", "false"),
        ("boolean_convert(a or b, \"sideways\")", "failure(argument, boolean_convert, 2)"),
        ("boolean_convert(f(a) + 1)", "failure(argument, boolean_convert, 1)"),
        ("boolean_minimize((a and b) or (a and not b))", "a"),
        ("boolean_minimize(xor(a, b, c), \"CNF\")", "(not a or not b or c) and (not a or b or not c) and (a or not b or not c) and (a or b or c)"),
        ("boolean_minimize((a and b) or c, a)", "b or c"),
        ("boolean_minimize(implies(a, b), \"NAND\")", "nand(a, not b)"),
        ("boolean_minimize(xor(a, b), \"AND\")", "not (a and b) and not (not a and not b)"),
        ("logical_expand((a or b) and (a or c))", "a or b and c"),
        ("logical_expand(implies(a, b))", "not a or b"),
        ("logical_expand(not (a and (b or c)))", "not a or not b and not c"),
        ("logical_expand((x == 1 or x == 2) and (y == 3 or y == 4))", "x == 1 and y == 3 or x == 1 and y == 4 or x == 2 and y == 3 or x == 2 and y == 4"),
        ("logical_expand(a and not a)", "false"),
    ]);
}

/// Every named form is equivalent to the expression it was made from.
#[test]
fn boolean_forms_are_equivalent() {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    let forms = ["dnf", "cnf", "anf", "esop", "nand", "nor", "binary_nand", "binary_nor", "and", "or", "implies", "bff"];
    for src in ["boolean_counting_function([2, 3], 4)(a, b, c, d)", "xor(a, b, c)", "implies(a and b, c or d)", "equivalent(a, b, not c)", "a and (b or c)"] {
        for f in forms {
            let t = format!("is_tautology(equivalent({src}, boolean_convert({src}, \"{f}\")))");
            assert_eq!(e.eval(&t, &stop).unwrap().printed, "true", "{t}");
        }
    }
    // with a condition the form agrees where it holds
    let t = "is_tautology(implies(c, equivalent(xor(a, b, c), boolean_convert(xor(a, b, c), \"cnf\", c))))";
    assert_eq!(e.eval(t, &stop).unwrap().printed, "true");
}

#[test]
fn nest_graph_and_resampling() {
    run(&[
        ("edge_list(nest_graph(x => mod(x^2 + 1, 5), [0, 1, 2], 2))", "[directed_edge(0, 1), directed_edge(1, 2), directed_edge(2, 0)]"),
        ("vertex_list(nest_graph(x => [f(x), g(x)], x, 2))", "[x, f(x), g(x), f(f(x)), g(f(x)), f(g(x)), g(g(x))]"),
        ("edge_list(nest_graph(x => range(x, x + 1), 1, 2))", "[directed_edge(1, 1), directed_edge(1, 2), directed_edge(2, 2), directed_edge(2, 3)]"),
        ("edge_list(nest_graph(x => if x < 4 then x + 1 else x, 1))", "[directed_edge(1, 2)]"),
        ("edge_list(nest_graph(x => if x < 4 then [x + 1, x + 2] else [], 1, inf))", "[directed_edge(1, 2), directed_edge(1, 3), directed_edge(2, 3), directed_edge(2, 4), directed_edge(3, 4), directed_edge(3, 5)]"),
        ("edge_list(nest_graph(x => x + 1, 0, 0))", "[]"),
        ("edge_list(nest_graph(x => x + 1, 1, 2, directed_edges: false))", "[undirected_edge(1, 2), undirected_edge(2, 3)]"),
        ("array_resample([1, 2, 3, 4, 5], 9)", "[1, 3/2, 2, 5/2, 3, 7/2, 4, 9/2, 5]"),
        ("array_resample([1, 2, 3, 4, 5], 3)", "[1, 3, 5]"),
        ("array_resample([1, 2, 3], 4)", "[1, 5/3, 7/3, 3]"),
        ("array_resample([[1, 2, 3], [4, 5, 6]], 4)", "[[1, 4/3, 5/3, 2, 7/3, 8/3, 3], [2, 7/3, 8/3, 3, 10/3, 11/3, 4], [3, 10/3, 11/3, 4, 13/3, 14/3, 5], [4, 13/3, 14/3, 5, 16/3, 17/3, 6]]"),
        ("array_resample([1, 2, 3], 6, \"Bin\")", "[1, 5/4, 7/4, 9/4, 11/4, 3]"),
        ("array_resample([1, 2, 3], 9, \"Bin\", padding: 0)", "[2/3, 1, 4/3, 5/3, 2, 7/3, 8/3, 3, 2]"),
        ("array_resample([1, 2, 5], 6, \"Bin\", padding: \"periodic\")", "[2, 5/4, 7/4, 11/4, 17/4, 4]"),
        ("array_resample([1, 2, 5], 6, \"Bin\", padding: \"reflected\")", "[5/4, 5/4, 7/4, 11/4, 17/4, 17/4]"),
        ("array_resample(range(100), 5, \"Point\", [55, 63])", "[55, 57, 59, 61, 63]"),
        ("array_resample([1, 2, 3], 6, resampling: \"nearest\")", "[1, 1, 2, 2, 3, 3]"),
        ("array_resample([1, 2, 3], 5, resampling: \"nearest\")", "[1, 3/2, 2, 5/2, 3]"),
        ("array_resample([1, 2, 3], 5, resampling: \"nearest_left\")", "[1, 1, 2, 2, 3]"),
        ("array_resample([0, 2, 0, 1, 4, 1, 9, 4, 4], scaled(2), \"Point\")", "[0, 1, 2, 1, 0, 1/2, 1, 5/2, 4, 5/2, 1, 5, 9, 13/2, 4, 4, 4]"),
        ("array_resample([1, 2, 3, 4], 2)", "[1, 4]"),
        ("array_resample([1, 2, 3], 6, [\"Bin\", -1])", "[1, 3/2, 2, 5/2, 3, 3]"),
        ("array_resample([1, 2, 3], 6, [\"Bin\", 1])", "[1, 1, 3/2, 2, 5/2, 3]"),
        ("array_resample([0, 2, 0, 1, 4, 1, 9, 4, 4], scaled(2), \"Point\", [1, 5])", "[0, 1, 2, 1, 0, 1/2, 1, 5/2, 4]"),
        ("array_resample([0, 2, 0, 1, 4, 1, 9, 4, 4], scaled(2), \"Point\", [0, 1/2], data_range: [0, 1])", "[0, 1, 2, 1, 0, 1/2, 1, 5/2, 4]"),
        ("array_resample([1, 2, 3], 5, resampling: \"NearestLeft\")", "[1, 1, 2, 2, 3]"),
        ("map(x => round(1000000*x), array_resample([1, 3, 2, 5, 4], 9, resampling: \"cubic\"))", "[1000000, 2188467, 3000000, 2372595, 2000000, 3446152, 5000000, 4842795, 4000000]"),
        ("map(x => round(1000000*x), array_resample([1, 3, 2, 5, 4], 9, resampling: \"cubic\", padding: 0))", "[1000000, 2281215, 3000000, 2374528, 2000000, 3345671, 5000000, 5242786, 4000000]"),
        ("map(x => round(1000000*x), array_resample([1, 3, 2, 5, 4], 9, resampling: \"quadratic\"))", "[1000000, 2132468, 3000000, 2399495, 2000000, 3470563, 5000000, 4777129, 4000000]"),
        ("array_resample([1, 2, 5], 1)", "failure(argument, array_resample, 2)"),
        ("array_resample([], 3)", "failure(argument, array_resample, 1)"),
        ("array_resample([1, 2], 3, \"sideways\")", "failure(argument, array_resample, 3)"),
    ]);
}

#[test]
fn arguments_and_exceptions() {
    run(&[
        ("arguments_options(f(1, a -> 0), 1)", "[[1], [a -> 0]]"),
        ("arguments_options(f(1, 2), 1)", "failure(argument_count, f, 2, 1)"),
        ("arguments_options(f(1, 2, a -> 3), [1, 3])", "[[1, 2], [a -> 3]]"),
        ("arguments_options(f(1, 2, 3), [2, inf])", "[[1, 2, 3], []]"),
        ("arguments_options(f(1), [2, inf])", "failure(argument_count, f, 1, [2, inf])"),
        ("arguments_options(f(x, a -> 0, [b -> 5, [[c -> 17]]]), 1)", "[[x], [a -> 0, b -> 5, c -> 17]]"),
        ("arguments_options(f(x, a -> 5, y, b -> 7, c -> 11), [1, 4])", "[[x, a -> 5, y], [b -> 7, c -> 11]]"),
        ("arguments_options(f(1), -1)", "failure(argument, arguments_options, 2)"),
        ("arguments_options(f(2 + 2, 3*3, a -> 4^4), 2)", "[[4, 9], [a -> 256]]"),
        ("arguments_options(f(2 + 2, 3*3, a -> 4^4), 2, wrapper: quote)", "[quote(2 + 2, 3*3), quote(a -> 4^4)]"),
        ("arguments_options(f(1, a -> 2), [1, 2], assoc(\"OptionsMode\" -> none))", "[[1, a -> 2], []]"),
        ("rule fx(args___) := 7 when arguments_options(fx(1), 1) !== 0
arguments_options(fx(1, b -> 2), 1)", "[[1], [b -> 2]]"),
        ("check_arguments(f(1, a -> 0), 1)", "true"),
        ("check_arguments(f(), [0, 1])", "true"),
        ("check_arguments(f(1, 2), 1)", "false"),
        ("arguments_options(f(a, [], [], []), [1, 3], assoc(\"OptionsMode\" -> \"Shortest\"))", "[[a, [], []], []]"),
        ("arguments_options(f(1, b -> 2, a -> 3), [1, 2], assoc(\"OptionsMode\" -> \"Shortest\"))", "[[1, b -> 2], [a -> 3]]"),
        ("arguments_options(f(1, size: 2), [1, 2])", "[[1], [size -> 2]]"),
        ("register_exception_type(arg_error)", "assoc(exception_type -> arg_error, parent_exception_types -> [])"),
        ("register_exception_type(arg_type_error, arg_error)", "assoc(exception_type -> arg_type_error, parent_exception_types -> [arg_error])"),
        ("exception_types()", "[arg_error, arg_type_error]"),
        ("exception_tags(exception(arg_type_error))", "[arg_type_error, arg_error]"),
        ("exception(inv, a + b)", "exception(inv, assoc(payload -> a + b))"),
        ("is_exception(exception(inv)) and not is_exception(3)", "true"),
        ("catch_exception(1 + 2, arg_error)", "3"),
        ("catch_exception(1 + throw_exception(exception(arg_type_error, assoc(v -> 1))), arg_error)", "exception(arg_type_error, assoc(v -> 1))"),
        ("catch_exception(throw_exception(exception(arg_type_error)), arg_error, exception_type)", "arg_type_error"),
        ("catch_exception(throw_exception(exception(other)), arg_error, exception_type)", "failure(thrown, exception(other, assoc()), other)"),
        ("catch_exception(catch_exception(throw_exception(exception(other)), arg_error), other, exception_type)", "other"),
    ]);
}

#[test]
fn associations_and_lists() {
    run(&[
        ("key_union([assoc(a -> 1), assoc(b -> 2)], k => 0)", "[assoc(a -> 1, b -> 0), assoc(a -> 0, b -> 2)]"),
        ("key_union([assoc(a -> 1), assoc(b -> 2)])", "[assoc(a -> 1, b -> missing(\"KeyAbsent\", b)), assoc(a -> missing(\"KeyAbsent\", a), b -> 2)]"),
        ("key_intersection([assoc(a -> 1, b -> 2), assoc(b -> 3, c -> 4)])", "[assoc(b -> 2), assoc(b -> 3)]"),
        ("key_member_q(assoc(a -> 1), a) and key_free_q(assoc(a -> 1), b)", "true"),
        ("key_member_q([a -> 1], a)", "true"),
        ("key_member_q(assoc(\"x\" -> 1), _: Str)", "true"),
        ("map(key_free_q(a), [assoc(a -> 1), assoc(b -> 2)])", "[false, true]"),
        ("disjoint_q([1, 2], [3]) and intersecting_q([1, 2], [2, 3])", "true"),
        ("compose_list([f, g, h], x)", "[x, f(x), g(f(x)), h(g(f(x)))]"),
        ("longest_common_subsequence(\"abcdef\", \"xbcdy\")", "\"bcd\""),
        ("longest_common_subsequence([1, 2, 3, 4], [0, 2, 3, 5])", "[2, 3]"),
        ("longest_common_sequence(\"abcdef\", \"xbdy\")", "\"bd\""),
        ("longest_common_sequence([1, 3, 2, 5, 4], [3, 5, 2, 4])", "[3, 2, 4]"),
    ]);
}

/// The list is well formed: usage first in each description, names unique, a page for each (checked by mc-docs).
#[test]
fn functions_are_listed() {
    let mut seen = std::collections::HashSet::new();
    for (name, desc) in mc_langlib::FUNCTIONS {
        assert!(desc.starts_with(name), "{name}: the description starts with the usage");
        assert!(seen.insert(name), "{name} is listed twice");
    }
    assert!(mc_langlib::SOURCE.starts_with("#! mathcas 2"));
}

#[test]
fn threaded_operands_of_listable_calls() {
    run(&[
        ("[[1, 2], [3, 4], [5, 6]] + threaded([x, y])", "[[1 + x, 2 + y], [3 + x, 4 + y], [5 + x, 6 + y]]"),
        ("2*threaded([-1, 1])", "threaded([-2, 2])"),
        ("[[1, 2], [3, 4]] * threaded([x, y], 1)", "[[x, 2*x], [3*y, 4*y]]"),
        ("[[1, 2], [3, 4]] + threaded([x, y, z])", "failure(shape_mismatch)"),
        ("[[1, 2], [3, 4]] + threaded([x, y], 3)", "failure(threaded_level, 3)"),
        ("block(set_attributes(g, listable), g(threaded([a, b]), [[1, 2], [3, 4]]))", "[[g(a, 1), g(b, 2)], [g(a, 3), g(b, 4)]]"),
    ]);
}

#[test]
fn table_iterator_lists() {
    run(&[("table(k, [k, 1.5, 4])", "[1.5, 2.5, 3.5]"), ("table(k, [k, 2, 7, 2])", "[2, 4, 6]"), ("table(i + j, [i, 1, 2], [j, 1, 2])", "[[2, 3], [3, 4]]"), ("table(k, [k, 1/2, 3])", "[1/2, 3/2, 5/2]"), ("table(k, k, [1,2])", "[1, 2]")]);
}
