//! Values passing between functions (round 12, decisions io-core.*), checked through the whole engine: solutions
//! into `replace` and `table`, associations through the list functions and listable functions, tests and options
//! on the list functions, quantities and uncertain numbers in reductions, and failures passing through.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig, MAPS_LISTS};

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
fn solutions_flow_into_other_functions() {
    check(&[
        ("sols = solve(t^2 == 9, t)", "null"),
        ("sols", "[[t -> -3], [t -> 3]]"),
        ("replace(t, sols)", "[-3, 3]"),
        ("replace(t^2 + t, sols)", "[t + t^2, t + t^2]"),
        ("rewrite(t^2 + t, sols)", "[6, 12]"),
        ("table(replace(t, s), s, sols)", "[-3, 3]"),
        ("n(replace(t, solve(t^2 == 3, t)))", "[-1.7320508075688772, 1.7320508075688772]"),
        ("total(rewrite(t, sols))", "0"),
        ("map(s => rewrite(2*t, s), sols)", "[-6, 6]"),
        ("replace([t], [[t -> 1]], [1])", "[[1]]"),
    ]);
}

#[test]
fn associations_thread_through_listable_functions() {
    check(&[
        ("assoc(p -> 1, q -> 2) + 1", "assoc(p -> 2, q -> 3)"),
        ("assoc(p -> 1, q -> 2) + assoc(q -> 10, p -> 20)", "assoc(p -> 21, q -> 12)"),
        ("2*assoc(p -> 1, q -> 2)", "assoc(p -> 2, q -> 4)"),
        ("[1, 2]*assoc(p -> 1)", "[assoc(p -> 1), assoc(p -> 2)]"),
        ("assoc(p -> 1) + assoc(q -> 1)", "failure(shape_mismatch)"),
        ("sqrt(assoc(p -> 4, q -> 9))", "assoc(p -> 2, q -> 3)"),
        ("gamma(assoc(p -> 3, q -> 4))", "assoc(p -> 2, q -> 6)"),
        ("mod(assoc(p -> 7, q -> 8), 3)", "assoc(p -> 1, q -> 2)"),
        ("is_prime(assoc(p -> 7, q -> 8))", "assoc(p -> true, q -> false)"),
        ("member_of(attributes(sin), listable)", "true"),
        ("sin([0, 0])", "[0, 0]"),
    ]);
}

#[test]
fn associations_through_list_functions() {
    check(&[
        ("m = assoc(p -> 3, q -> 1, r -> 2, s -> 1)", "null"),
        ("total(m)", "7"),
        ("max(m)", "3"),
        ("min(m)", "1"),
        ("mean(m)", "7/4"),
        ("median(m)", "3/2"),
        ("count(m, 1)", "2"),
        ("cases(m, x_ when x > 1)", "[3, 2]"),
        ("all_true(m, is_integer)", "true"),
        ("any_true(m, x => x > 2)", "true"),
        ("none_true(m, is_even)", "false"),
        ("select_first(m, is_even)", "2"),
        ("unique(m)", "assoc(p -> 3, q -> 1, r -> 2)"),
        ("sort(m)", "assoc(q -> 1, s -> 1, r -> 2, p -> 3)"),
        ("reverse_sort(m)", "assoc(p -> 3, r -> 2, q -> 1, s -> 1)"),
        ("delete_cases(m, 1)", "assoc(p -> 3, r -> 2)"),
        ("take_largest(m, 2)", "assoc(p -> 3, r -> 2)"),
        ("max_by(m, x => -x, 2)", "assoc(q -> 1, s -> 1)"),
        ("catenate([m, [9]])", "[3, 1, 2, 1, 9]"),
        ("apply(f, m)", "f(3, 1, 2, 1)"),
        ("map_thread(f, [assoc(a -> 1, b -> 2), assoc(a -> 3, b -> 4)])", "assoc(a -> f(1, 3), b -> f(2, 4))"),
        ("keys([assoc(a -> 1), assoc(b -> 2)])", "[[a], [b]]"),
        ("values([assoc(a -> 1), [b -> 2]])", "[[1], [2]]"),
        ("key_drop([assoc(a -> 1, b -> 2), [a -> 3, c -> 4]], a)", "[assoc(b -> 2), assoc(c -> 4)]"),
        ("map_at(f, assoc(a -> 1, b -> [2, 3]), [b, 2])", "assoc(a -> 1, b -> [2, f(3)])"),
        ("map_at(f, assoc(a -> 1, b -> 2), a)", "assoc(a -> f(1), b -> 2)"),
        ("extract(assoc(a -> 1, b -> [2, 3]), [b, 1])", "2"),
        ("delete(assoc(a -> 1, b -> 2), b)", "assoc(a -> 1)"),
        ("apply(f, assoc(a -> [1, 2]), [1])", "assoc(a -> f(1, 2))"),
        ("map(f, assoc(a -> [1, 2]), [2])", "assoc(a -> [f(1), f(2)])"),
        ("level(assoc(a -> x, b -> assoc(c -> y)), [-1])", "[x, y]"),
        ("cases(assoc(a -> x, b -> assoc(c -> y)), _: Sym, inf)", "[x, y]"),
        ("rewrite(f(assoc(1 -> 2, 3 -> 4)), f(assoc(k_ -> v_, ___)) :> [k, v])", "[1, 2]"),
        ("is_assoc(assoc(a, b))", "false"),
    ]);
}

#[test]
fn tests_levels_and_options_on_list_functions() {
    check(&[
        ("table(0, 2, 3)", "[[0, 0, 0], [0, 0, 0]]"),
        ("table(i, 2, i, 2)", "[[1, 2], [1, 2]]"),
        ("all_true([[2, 4], [6]], is_even, 2)", "true"),
        ("any_true([[1, 3], [5, 6]], is_even, 2)", "true"),
        ("all_true(5, is_even)", "true"),
        ("select(range(10), is_odd, 2)", "[1, 3]"),
        ("unique([1, 2, 11, 12, 21], (a, b) => floor(a/10) == floor(b/10))", "[1, 11, 21]"),
        ("gather([1, 2, 11, 12], (a, b) => floor(a/10) == floor(b/10))", "[[1, 2], [11, 12]]"),
        ("tally([1, 2, 11], (a, b) => floor(a/10) == floor(b/10))", "[[1, 2], [11, 1]]"),
        ("union([3, -3, 1], same_test: (a, b) => abs(a) == abs(b))", "[-3, 1]"),
        ("intersection([1, 2, 3], [-2, -3], same_test: (a, b) => abs(a) == abs(b))", "[2, 3]"),
        ("complement([1, 2, 3], [-2], same_test: (a, b) => abs(a) == abs(b))", "[1, 3]"),
        ("fixed_point_list(x => x/2, 1.0, same_test: (a, b) => abs(a - b) < 0.2)", "[1.0, 0.5, 0.25, 0.125]"),
        ("fix_point(x => x/2, 1.0, same_test: (a, b) => abs(a - b) < 0.2)", "0.125"),
        ("fix_point(x => floor(x/2), 100)", "0"),
        ("nest_while(x => floor(x/2), 10, not_same, 2)", "0"),
        ("nest_while_list(x => mod(3*x, 7), 1, unequal, all)", "[1, 3, 2, 6, 4, 5, 1]"),
        ("nest_while(x => x + 1, 1, x => x < 5, 1, inf, -1)", "4"),
        ("nest_while_list(x => x + 1, 1, x => x < 3, 1, inf, 1)", "[1, 2, 3, 4]"),
        ("unequal(1, 2, 3)", "true"),
        ("unequal(1, 2, 1)", "false"),
        ("equal(2, 2, 2)", "true"),
        ("cases([1, f(2), [3]], _: Int, inf)", "[1, 2, 3]"),
        ("cases(range(10), _: Int, [1], 2)", "[1, 2]"),
        ("first_case([[a, 1], [2]], _: Int, none, [2])", "1"),
        ("position([a, [a]], a, [2])", "[[2, 1]]"),
        ("position(x^2, pow, heads: false)", "[]"),
        ("subsets([1, 2, 3], all, 3)", "[[], [1], [2]]"),
        ("subsets([1, 2, 3], all, [4])", "[[3]]"),
        ("total(subsets(a*b*c, [2]))", "a*b + a*c + b*c"),
        ("permutations([1, 2], all)", "[[], [1], [2], [1, 2], [2, 1]]"),
        ("flatten([1, [2, [3, [4]]]], inf)", "[1, 2, 3, 4]"),
        ("flatten(f(g(a, b), f(c)), inf, g)", "f(a, b, f(c))"),
        ("sort_by([\"bb\", \"a\", \"ccc\"], string_length, greater)", "[\"ccc\", \"bb\", \"a\"]"),
        ("ordering([3, 1, 2], all, greater)", "[1, 3, 2]"),
        ("reverse_sort([3, 1, 2], greater)", "[1, 2, 3]"),
        ("max_by([-3, 2, 1, 3], abs, 2)", "[-3, 3]"),
        ("to_string(x^2, input_form)", "\"x^2\""),
    ]);
}

#[test]
fn quantities_and_uncertain_numbers_in_reductions() {
    check(&[
        ("total([around(1.0, 0.3), around(2.0, 0.4)])", "around(3.0, 0.5)"),
        ("total([interval([0, 1]), interval([2, 3])])", "interval([2, 4])"),
        ("total([quantity(1, \"m\"), quantity(2, \"m\")])", "quantity(3, \"m\")"),
        ("total([x, interval([0, 1])])", "interval([x, 1 + x])"),
        ("mean([around(1.0, 0.3), around(3.0, 0.4)])", "around(2.0, 0.25)"),
    ]);
}

#[test]
fn failures_pass_through() {
    check(&[
        ("total([1, failure(oops)])", "failure(oops)"),
        ("map(f, failure(oops))", "failure(oops)"),
        ("[1, failure(oops)]", "[1, failure(oops)]"),
        ("select([1, 2], x => failure(oops))", "failure(oops)"),
        ("unique([1, 2], (a, b) => failure(oops))", "failure(oops)"),
        ("union([1, 2], same_test: (a, b) => failure(oops))", "failure(oops)"),
        ("assoc(a -> failure(oops)) + 1", "failure(oops)"),
    ]);
}

/// Every function declared to map over lists maps a list elementwise, and an association by its values.
#[test]
fn maps_lists_functions_are_elementwise() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let mut bad = vec![];
    for name in MAPS_LISTS {
        let one = format!("{name}([2.5, 3.5]) === [{name}(2.5), {name}(3.5)]");
        let two = format!("{name}([2, 3], 5) === [{name}(2, 5), {name}(3, 5)]");
        let three = format!("{name}(5, [2, 3]) === [{name}(5, 2), {name}(5, 3)]");
        if ![one, two, three].iter().any(|src| eval(&mut e, src) == "true") {
            bad.push(format!("{name}: does not map lists"));
        }
        let a = format!("{name}(assoc(k -> 2.5)) === assoc(k -> {name}(2.5))");
        let b = format!("{name}(assoc(k -> 2), 5) === assoc(k -> {name}(2, 5))");
        let c = format!("{name}(5, assoc(k -> 2)) === assoc(k -> {name}(5, 2))");
        if ![a, b, c].iter().any(|src| eval(&mut e, src) == "true") {
            bad.push(format!("{name}: does not thread over associations"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Gap G65: a stored option list spliced in next to other options; later options win.
#[test]
fn stored_options_merge_with_the_options_after_them() {
    check(&[
        ("opts = options(frame -> true, grid_lines -> none);", "null"),
        ("g(1, opts, a: 2)", "g(1, frame: true, grid_lines: none, a: 2)"),
        ("g(1, options(a -> 1), options(b -> 2, a -> 3))", "g(1, b: 2, a: 3)"),
        ("g(1, opts)", "g(1, frame: true, grid_lines: none)"),
        ("head(plot(x, [x, 0, 1], opts, plot_range: all))", "image"),
    ]);
}

/// Gap G82: a stored option list may stand among named options (after some of them); it is merged in place.
#[test]
fn stored_options_may_follow_named_options() {
    check(&[
        ("opts = options(frame -> true, a -> 5);", "null"),
        ("g(1, b: 2, opts)", "g(1, b: 2, frame: true, a: 5)"),
        ("g(1, b: 2, opts, a: 3)", "g(1, b: 2, frame: true, a: 3)"),
        ("g(1, a: 2, opts, c: 3)", "g(1, frame: true, a: 5, c: 3)"),
        ("head(plot(x, [x, 0, 1], plot_style: red, opts, plot_range: all))", "image"),
    ]);
}

/// Gaps G67 and G76: display wrappers for numbers, and interpolations with many points shown short, in display
/// mode only (the default prints text that reads back).
#[test]
fn number_forms_and_interpolations_display_short() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, display: true, ..EngineConfig::default() });
    for (src, want) in [
        ("scientific_form(330808.9917147705)", "3.30809 × 10^5"),
        ("number_form(5.18e11)", "5.18 × 10^11"),
        ("number_form(330808.99)", "330809."),
        ("number_form(0.000123456789, 3)", "0.000123"),
        ("scientific_form([1.5, -0.002, 12])", "[1.5, -2 × 10^-3, 12]"),
        ("scientific_form(123456.789, 3)", "1.23 × 10^5"),
        ("f = interpolation(table([x, sin(x)], x, range(0, 25, 0.02)))", "null"),
        ("f", "interpolated([0.0, 25.0], <1251 points>)"),
        ("abs(f(1.0) - sin(1.0)) < 10^-6", "true"),
        ("interpolation([[0.0, 1.0], [1.0, 2.0]], method: linear)", "interpolated([0.0, 1.0], [1.0, 2.0], [0.0, 0.0])"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
    let mut plain = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    assert_eq!(eval(&mut plain, "scientific_form(1.5)"), "scientific_form(1.5)");
}
