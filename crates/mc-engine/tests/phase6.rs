//! Core semantics added or changed in phase 6 (decisions p6core.*), checked through the whole engine: operator
//! forms, negated comparisons, the canonical exponential, complex numbers as atoms, levels, the limits that stop
//! runaway evaluations, and the library gaps the user's notebooks met.
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
fn negated_comparisons() {
    check(&[
        ("not x > y", "x <= y"),
        ("not x < y", "x >= y"),
        ("not x >= y", "x < y"),
        ("not x <= y", "x > y"),
        ("not a == b", "a != b"),
        ("not a != b", "a == b"),
        ("not not a", "a"),
        ("not (not (not a))", "not a"),
        ("not f(x)", "not f(x)"),
        ("not 1 < 2", "false"),
    ]);
}

#[test]
fn operator_forms() {
    check(&[
        ("select(is_even)([1, 2, 4, 7, 6])", "[2, 4, 6]"),
        ("append([1, 2])([a, b])", "[a, b, [1, 2]]"),
        ("prepend(0)([a, b])", "[0, a, b]"),
        ("map(f)([a, b])", "[f(a), f(b)]"),
        ("fold(f)([a, b, c])", "f(f(a, b), c)"),
        ("fold(f)(x, [a, b])", "f(f(x, a), b)"),
        ("apply(f)([a, b])", "f(a, b)"),
        ("count_by(is_even)([1, 2, 3])", "[false -> 2, true -> 1]"),
        ("key_drop([\"a\"])(assoc(\"a\" -> 1, \"b\" -> 2))", "assoc(\"b\" -> 2)"),
        ("lookup(a)(assoc(a -> 1))", "1"),
        ("select_first(is_even)([1, 3, 4, 6])", "4"),
        ("rewrite(x -> 2)(x + y)", "2 + y"),
        // only one level of operator: the inner call stays as it is
        ("select(is_even)", "select(is_even)"),
    ]);
}

#[test]
fn the_exponential_has_one_form() {
    check(&[
        // one term, `pow(e, x)`, shown as a call of `exp`
        ("exp(x)", "exp(x)"),
        ("e^x", "exp(x)"),
        ("exp(x) === e^x", "true"),
        ("head(exp(x))", "pow"),
        ("exp(a)*exp(b)", "exp(a + b)"),
        ("exp(x)^2", "exp(2*x)"),
        ("1/exp(x)", "exp(-x)"),
        ("exp(1)", "e"),
        ("exp(0)", "1"),
        ("e^log(2)", "2"),
        ("e^(im*pi)", "-1"),
        ("exp(log(2))", "2"),
        ("simplify(exp(x) - e^x)", "0"),
        ("exp(2.0) == e^2.0", "true"),
    ]);
}

#[test]
fn complex_numbers_are_atoms() {
    check(&[
        ("length(3 + im)", "0"),
        ("depth(3 + im)", "1"),
        ("is_atom(1 + 2*im)", "true"),
        ("leaf_count([1/2, 1 + im])", "7"),
        ("leaf_count(sqrt(x))", "5"),
        ("depth(assoc(1 -> x + y))", "3"),
        ("leaf_count(assoc(a + b -> 1, c -> 2))", "3"),
        ("map(is_atom, [1/2, 1 + im])", "[true, true]"),
    ]);
}

#[test]
fn structure_functions() {
    check(&[
        ("most(1/b)", "b"),
        ("rest(1/b)", "-1"),
        ("all_true([2, x, 4, y], k => k < 10)", "x < 10 and y < 10"),
        ("any_true([12, x], k => k < 10)", "x < 10"),
        ("none_true([x, 1], f)", "not f(x) and not f(1)"),
        ("all_true([1, 20], k => k < 10)", "false"),
        ("split([a, a, b, a, c, c])", "[[a, a], [b], [a], [c, c]]"),
        ("split([1, 2, 4, 3, 5], (a, b) => b > a)", "[[1, 2, 4], [3, 5]]"),
        ("split(\"a b\")", "[\"a\", \"b\"]"),
        ("subdivide(0, 1, 4)", "[0, 1/4, 1/2, 3/4, 1]"),
        ("subdivide(2)", "[0, 1/2, 1]"),
        ("subdivide(1.0, 2, 2)", "[1.0, 1.5, 2.0]"),
        ("eval(1 + 1)", "2"),
        ("keys([a -> 1, b -> 2])", "[a, b]"),
        ("values([a -> 1, b :> 2])", "[1, 2]"),
        ("transpose(r(z))", "transpose(r(z))"),
        ("rewrite(a*t^4, [[t -> 1], [t -> 2]])", "[a, 16*a]"),
        ("x^3 /. solve(x^2 == 4, x)", "[-8, 8]"),
    ]);
}

#[test]
fn parts_by_position_and_pattern() {
    check(&[
        ("match_q(f(1), f(_: Int))", "true"),
        ("match_q([1, \"a\"], [_, _: Int])", "false"),
        ("head(f(x))", "f"),
        ("map(head, [1, 1/2, 2.5, \"s\", x, 1 + im, [1]])", "[Int, Rat, Float, Str, Sym, Complex, list]"),
        ("dimensions([[1, 2, 3], [4, 5, 6]])", "[2, 3]"),
        ("dimensions([[1, 2], [3]])", "[2]"),
        ("dimensions(f(g(a, b), g(c, d)))", "[2, 2]"),
        ("dimensions(x)", "[]"),
        ("array_depth([[1, 2], [3, 4]])", "2"),
        ("extract([[a, b], [c, d]], [2, 1])", "c"),
        ("extract([[a, b], [c, d]], [[1, 2], [2, 2]])", "[b, d]"),
        ("extract(f(x, y), 0)", "f"),
        ("replace_part([a, b, c], 2 -> x)", "[a, x, c]"),
        ("replace_part([[a, b], [c, d]], [[1, 1] -> x, [2, 2] -> y])", "[[x, b], [c, y]]"),
        ("delete([a, b, c, d], 2)", "[a, c, d]"),
        ("delete([a, b, c, d], [[1], [-1]])", "[b, c]"),
        ("map_at(f, [a, b, c], 2)", "[a, f(b), c]"),
        ("map_at(f, [[a, b], [c, d]], [[1, 2], [2, 1]])", "[[a, f(b)], [f(c), d]]"),
        ("level([[a, b], c], [2])", "[a, b]"),
        ("level([[a, b], c], 1)", "[[a, b], c]"),
        ("replace(x^2, x^2 -> y)", "y"),
        ("replace([x^2], x^2 -> y)", "[x^2]"),
        ("replace([x^2, x], x^2 -> y, [1])", "[y, x]"),
        ("delete_cases([1, a, 2, b], _: Int)", "[a, b]"),
        ("delete_cases([[1, a], [b, 2]], _: Int, [2])", "[[a], [b]]"),
        ("first_case([a, 2, b, 3], _: Int)", "2"),
        ("first_case([a, b], _: Int, none)", "none"),
        ("reverse_sort([3, 1, 2])", "[3, 2, 1]"),
        ("take_largest([3, 1, 4, 1, 5], 2)", "[5, 4]"),
        ("take_smallest([3, 1, 4, 1, 5], 2)", "[1, 1]"),
        ("full_form(x + y^2)", "add(x, pow(y, 2))"),
        ("full_form([1, a*b])", "list(1, mul(a, b))"),
        ("input_form(x + y^2)", "x + y^2"),
    ]);
}

#[test]
fn a_trailing_when_in_a_block_conditions_the_rule() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    assert_eq!(eval(&mut e, "rule f(x_) := { var u = x - 1; u^2 when u > 0 }"), "null");
    assert_eq!(eval(&mut e, "f(6)"), "25");
    assert_eq!(eval(&mut e, "f(0)"), "f(0)");
    assert_eq!(eval(&mut e, "f(a)"), "f(a)");
}

#[test]
fn levels() {
    check(&[
        ("m = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]; total(m)", "[12, 15, 18]"),
        ("total(m, [2])", "[6, 15, 24]"),
        ("total(m, 2)", "45"),
        ("total(m, inf)", "45"),
        ("total([[1, 2, 3], [4, 5], [6]], [-1])", "[6, 9, 6]"),
        ("total(n(m))", "[12.0, 15.0, 18.0]"),
        ("total([1, 2, x])", "3 + x"),
        ("apply(f, [[a, b], [c], d], [1])", "[f(a, b), f(c), d]"),
        ("apply(f, [[a, b], [c]], [0, 1])", "f(f(a, b), f(c))"),
        ("map_indexed(f, [[a, b], [c]], [2])", "[[f(a, [1, 1]), f(b, [1, 2])], [f(c, [2, 1])]]"),
        ("count([[a, 1], [b, a]], a, inf)", "2"),
        ("map(f, [[a, b], [c]], [2])", "[[f(a), f(b)], [f(c)]]"),
        ("map(f, assoc(a -> 4, b -> 2))", "assoc(a -> f(4), b -> f(2))"),
        ("map(f, g(a, b))", "g(f(a), f(b))"),
        ("reverse(m, 2)", "[[3, 2, 1], [6, 5, 4], [9, 8, 7]]"),
        ("reverse(m, [1, 2])", "[[9, 8, 7], [6, 5, 4], [3, 2, 1]]"),
        ("take([[1, 2], [3, 4]], all, [1])", "[[1], [3]]"),
        ("take([a, b, c, d, e], [2, 4])", "[b, c, d]"),
        ("take([a, b, c, d, e], [1, -1, 2])", "[a, c, e]"),
        ("drop([a, b, c, d, e, f], [2, 4])", "[a, e, f]"),
        ("take([a, b, c], 5)", "failure(index, 5, 3)"),
        ("partition([[11, 12, 13], [21, 22, 23], [31, 32, 33]], [2, 2], 1)",
         "[[[[11, 12], [21, 22]], [[12, 13], [22, 23]]], [[[21, 22], [31, 32]], [[22, 23], [32, 33]]]]"),
        ("partition([a, b, c, d, e, f, g], 3, 3, 1)", "[[a, b, c], [d, e, f], [g, a, b]]"),
        ("partition([a, b, c, d, e, f, g], 3, 3, -1, [])", "[[a], [b, c, d], [e, f, g]]"),
        ("partition([a, b, c, d], 3, 1, [1, 1], x)", "[[a, b, c], [b, c, d], [c, d, x], [d, x, x]]"),
        ("pad_left([a, b, c], 7, [u, v])", "[v, u, v, u, a, b, c]"),
        ("pad_right([a, b, c], 7, [u, v])", "[a, b, c, v, u, v, u]"),
        ("pad_left([a, b], 4, [[u, v]])", "[[u, v], [u, v], a, b]"),
        ("pad_left([[a, b], [c]], [3, 4])", "[[0, 0, 0, 0], [0, 0, a, b], [0, 0, 0, c]]"),
        ("pad_right([[a, b, c], [d], []])", "[[a, b, c], [d, 0, 0], [0, 0, 0]]"),
    ]);
}

#[test]
fn runaway_evaluations_stop_with_a_limit() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    assert_eq!(eval(&mut e, "rule f(x_) := f(x + 1)"), "null");
    assert_eq!(eval(&mut e, "f(1)"), "error: limit: iterations");
    // the session goes on
    assert_eq!(eval(&mut e, "1 + 1"), "2");
    // a long but finite tail recursion is below the limit
    assert_eq!(eval(&mut e, "rule g(n_) := if n == 0 then done else g(n - 1)"), "null");
    assert_eq!(eval(&mut e, "g(100000)"), "done");
}

#[test]
fn the_memory_budget_stops_an_evaluation() {
    let mut cfg = EngineConfig { threads: 1, ..EngineConfig::default() };
    cfg.eval.max_young = 64 << 20;
    let mut e = CoreEngine::with_config(cfg);
    assert_eq!(eval(&mut e, "{ var s = 0; for k in range(10^7) { s = s + 1/k }; 1 }"), "error: limit: memory");
    assert_eq!(eval(&mut e, "2 + 2"), "4");
}

#[test]
fn docs_bugs_of_round_11() {
    check(&[
        // 1: symbolic steps
        ("range(0, pi, pi/4)", "[0, (1/4)*pi, (1/2)*pi, (3/4)*pi, pi]"),
        // 3: an association's values are tested
        ("select(assoc(\"a\" -> 3, \"b\" -> 8), v => v > 4)", "assoc(\"b\" -> 8)"),
        // 5: string and symbol types in patterns
        ("cases([\"id\", 4], _: Str)", "[\"id\"]"),
        ("position([1, x], _: Sym)", "[[0], [2]]"),
        ("cases([1, 2.5, 1/2, 1 + im], _: Float)", "[2.5]"),
        ("cases([1, 2.5, 1/2, 1 + im], _: Complex)", "[complex(1, 1)]"),
        // 6: failure patterns
        ("count([1, failure(a, f)], failure(___))", "1"),
        ("free_of([1, 2], failure(___))", "true"),
        // 7: sums of lists
        ("sum([[1, 2], [3, 4], [5, 6]])", "[9, 12]"),
        // 8: numeric constants
        ("max(pi, 3.2, sqrt(10))", "3.2"),
        ("min(e, 2.7, 11/4)", "2.7"),
        ("max(pi, sqrt(10))", "10^(1/2)"),
        // 10: a failure is assigned
        ("r1 = 1/0; is_failure(r1)", "true"),
        // 11: the default is evaluated only when the key is missing
        ("a1 = assoc(1 -> 2); lookup(a1, 1, 1/0)", "2"),
        ("lookup(a1, 3, 7)", "7"),
    ]);
}

#[test]
fn more_core_functions() {
    check(&[
        ("same(a, a, a)", "true"),
        ("same(a, a, b)", "false"),
        ("not_same(a, b, c)", "true"),
        ("not_same(a, b, a)", "false"),
        ("[a, b, c] == [d, e]", "false"),
        ("[1, 2] == [1, 2.0]", "true"),
        ("[a, b] != [a, c, d]", "true"),
        ("set_attributes(g1, listable); g1([1, 2])", "[g1(1), g1(2)]"),
        ("attributes(g1)", "[listable]"),
        ("clear_attributes(g1, listable); g1([1, 2])", "g1([1, 2])"),
        ("set_attributes(q1, hold_all); q1(1 + 1)", "q1(1 + 1)"),
        ("nest_while(n => n/2, 96, is_even)", "3"),
        ("nest_while_list(n => n/2, 96, is_even)", "[96, 48, 24, 12, 6, 3]"),
        ("nest_while(x => 2*x, 1, x => true, 1, 5)", "32"),
        ("fixed_point_list(x => (x + 2/x)/2, 1, 3)", "[1, 3/2, 17/12, 577/408]"),
        ("thread(log(x == y), equal)", "log(x) == log(y)"),
        ("array(f, 3, 0)", "[f(0), f(1), f(2)]"),
        ("array(f, [2, 2], [0, 1])", "[[f(0, 1), f(0, 2)], [f(1, 1), f(1, 2)]]"),
        ("fn g2(digits) = n(pi, digits: digits); g2(5) === n(pi, digits: 5)", "true"),
    ]);
}

#[test]
fn fix_point_stops_on_balls_that_agree() {
    check(&[("rule newton(f_, x0_) := { let fp = f'; fix_point(&(&1 - f(&1)/fp(&1)), x0) }; is_number(newton(cos, 1.~20))", "true")]);
}

#[test]
fn display_mode_prints_differences_and_divisions() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, display: true, ..EngineConfig::default() });
    for (src, want) in [
        ("4*a^2 - 12*b*c", "4*a^2 - 12*b*c"),
        ("x^-2*y", "y/x^2"),
        ("d(sin(x)/x, x)", "cos(x)/x - sin(x)/x^2"),
        ("1/(x*y)", "1/(x*y)"),
        ("a - (3/4)*x", "a - (3/4)*x"),
        ("to_string(4*a^2 - 12*b*c)", "\"4*a^2 - 12*b*c\""),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
    // the default prints text that reads back as the same term
    let mut plain = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    assert_eq!(eval(&mut plain, "x^-2*y"), "y*x^-2");
}
