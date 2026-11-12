//! Round 37: regressions found after combining the round's changes. Each test has the
//! reported input and variations of its own, checked by an independent route.
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

/// The mechanism: an option given as `name: value` reaches a library function through each way the library reads
/// options (a declared keyword, a positional `options(...)` operand read with `apply(list, o)` or by parts, a rest
/// argument, a rule pattern). The natives of the list operations that take options of their own (`apply`, `map`,
/// `cases`, ...) must not take an `options(...)` operand for their own options.
#[test]
fn options_reach_library_functions_every_way() {
    let out = session(&[
        "fn ot_kw(x; size = 1) = size",
        "fn ot_pos(x, o) = apply(list, o)",
        "fn ot_part(x, o) = o[1][2]",
        "fn ot_rest(x, rest...) = flatten(map(o => if not is_atom(o) and head(o) === options then apply(list, o) else [o], rest))",
        "fn ot_select(x, rest...) = select(flatten(map(o => apply(list, o), rest)), r => r[1] === size)",
        "rule ot_rule(x_, size: s_) := s",
        "ot_kw(0, size: 2)",
        "ot_pos(0, size: 2)",
        "ot_part(0, size: 2)",
        "ot_rest(0, size: 2, color: red)",
        "ot_select(0, size: 2, color: red)",
        "ot_rule(0, size: 2)",
        "ot_kw(0)",
    ]);
    assert_eq!(&out[6..], ["2", "[size -> 2]", "2", "[size -> 2, color -> red]", "[size -> 2]", "2", "1"]);
    // the natives with options of their own, given an options(...) term as their operand
    assert_eq!(ev("apply(list, options(a -> 1, b -> 2))"), "[a -> 1, b -> 2]");
    assert_eq!(ev("apply(g, options(a -> 1))"), "g(a -> 1)");
    assert_eq!(ev("leaf_count(options(a -> 1))"), "4");
    assert_eq!(ev("cases(options(a -> 1, b -> 2), (b -> v_) -> v)"), "[2]");
    assert_eq!(ev("count(options(a -> 1, b -> 2), _ -> _)"), "2");
    assert_eq!(ev("position(options(a -> 1, b -> 2), b -> _)"), "[[2]]");
    assert_eq!(ev("first_case(options(a -> 1, b -> 2), (b -> v_) -> v)"), "2");
    // and their own options still work
    assert_eq!(ev("apply(g, f(h(1), 2), [1], heads: true)"), "f(g(1), 2)");
    assert_eq!(ev("leaf_count(f(x, y), heads: false)"), "2");
}

#[test]
fn options_reach_n_roots_and_nest_graph() {
    // reported: precision_goal of n_roots (lib/calculus.mc cb_nr_goal) and directed_edges of nest_graph
    // (mc-langlib) were ignored
    assert_eq!(ev("precision(n_roots(x^2 - 2 == 0, x, precision_goal: 35)[1][2]) >= 35"), "true");
    assert_eq!(ev("edge_list(nest_graph(x => x + 1, 1, 2, directed_edges: false))"), "[undirected_edge(1, 2), undirected_edge(2, 3)]");
    // variations: another polynomial and goal, checked against the 50-digit root; another map and depth
    assert_eq!(ev("abs(n_roots(x^3 - 5 == 0, x, precision_goal: 40)[3][2] - n(5^(1/3), 50)) < 10^-38"), "true");
    assert_eq!(ev("precision(n_roots(x^3 - 5 == 0, x, precision_goal: 40)[3][2]) >= 40"), "true");
    assert_eq!(ev("length(edge_list(nest_graph(x => 2*x, 3, 4, directed_edges: false)))"), "4");
    assert_eq!(ev("head(edge_list(nest_graph(x => 2*x, 3, 4, directed_edges: false))[1])"), "undirected_edge");
    assert_eq!(ev("head(edge_list(nest_graph(x => 2*x, 3, 4))[1])"), "directed_edge");
}

#[test]
fn calls_given_settings_are_no_containers() {
    // accumulate of a time series: the settings of an unevaluated time series were added to its data; a call
    // whose last argument is options(...) has no elements, as before round 37
    assert_eq!(ev("accumulate(time_series(ts, temporal_regularity: true))"), "failure(argument, fold_list, 2)");
    // variations: fold and partition of a call with settings; the plain calls keep the round 37 forms
    assert_eq!(ev("fold(add, 0, g(1, 2, options(c -> 3)))"), "failure(argument, fold, 3)");
    assert_eq!(ev("partition(g(1, 2, 3, options(c -> 3)), 2)"), "failure(argument, partition, 1)");
    assert_eq!(ev("accumulate(f(a, b, c))"), "f(a, a + b, a + b + c)");
    assert_eq!(ev("partition(f(a, b, c, d), 2)"), "f(f(a, b), f(c, d))");
    // a list keeps its options-like elements
    assert_eq!(ev("fold_list(add, [1, 2, options(c -> 3)])"), "[1, 3, 3 + options(c -> 3)]");
}

#[test]
fn coefficient_of_a_monomial_is_the_multinomial_number() {
    // the coefficient of x y^2 z in (x + y + z)^4 is 4!/(1! 2! 1!) = 12; variations: a fourth variable (degree 4 leaves it out) and
    // other variables' factors in the coefficient
    assert_eq!(ev("coefficient(expand((x + y + z)^4), x*y^2*z)"), "12");
    assert_eq!(ev("coefficient(expand((x + y + z)^4), x*y^2*z) == multinomial(1, 2, 1)"), "true");
    assert_eq!(ev("coefficient(expand((x + y + z + w)^4), x*y^2*z)"), "12");
    assert_eq!(ev("coefficient(expand((x + y + z)^6), x^2*y^3*z) == factorial(6)/(factorial(2)*factorial(3))"), "true");
    assert_eq!(ev("coefficient(a*x*y^2*z + x^2*y^2*z + 3*x*y^2, x*y^2*z)"), "a");
}

#[test]
fn inverse_sine_transform_for_positive_arguments() {
    // the value is the same as in round 36 and right for t > 0 (the
    // transforms are for positive arguments; the result is not odd); checked against
    // sqrt(2/pi) times the integral at three points, and another frequency
    let out = session(&[
        "g = inverse_fourier_sin_transform(sin(w)/w^2, w, t)",
        "map(s => abs(n(rewrite(g, t -> s)) - n(sqrt(pi/2)*min(s, 1))) < 10^-12, [1/2, 2, 3/2])",
        "h = inverse_fourier_sin_transform(sin(2*w)/w^2, w, t)",
        "map(s => abs(n(rewrite(h, t -> s)) - n(sqrt(pi/2)*min(s, 2))) < 10^-12, [1/2, 3, 5/4])",
        "abs(n(rewrite(g, t -> 1/3)) - n(sqrt(2/pi)*n_integrate(sin(w)*sin(w/3)/w^2, [w, 0, inf]))) < 10^-6",
    ]);
    assert_eq!(out[1], "[true, true, true]");
    assert_eq!(out[3], "[true, true, true]");
    assert_eq!(out[4], "true");
}

#[test]
fn together_over_a_finite_field() {
    // the result is determined up to a unit of the field (here numerator and denominator
    // may carry a factor 3); checked by values at field elements; a variation with two fractions
    let out = session(&[
        "F = finite_field(17, 3)",
        "e = x * ((F(1) * x + F(246)) / (F(3) * x^2 + F(1771))) + F(4436) / (F(3) * x^2 + F(1771))",
        "r = together(e)",
        "map(k => rewrite(e, x -> F(k)) == rewrite(r, x -> F(k)), [5, 100, 2000])",
        "e2 = F(7)/(x + F(2)) + F(11)/(x + F(300))",
        "r2 = together(e2)",
        "map(k => rewrite(e2, x -> F(k)) == rewrite(r2, x -> F(k)), [5, 100, 2000])",
    ]);
    assert_eq!(out[3], "[true, true, true]");
    assert_eq!(out[6], "[true, true, true]");
}

#[test]
fn quintic_bijective_condition_is_sound() {
    // a quintic with parameters: the condition says f' > 0 everywhere (sufficient: f strictly increasing). Checked at
    // points inside, outside and across the boundary of the region; the boundary itself (f' with a double real
    // root, such as x^5) is left out by the strict conditions, which a conditional expression may do
    let out = session(&[
        "cond = function_bijective(x^5 + a * x^4 + b * x^3 + c * x^2 + d_u * x, x)[2]",
        "rewrite(cond, [a -> 0, b -> -10/3, c -> 0, d_u -> 5 + 1/1000])",
        "rewrite(cond, [a -> 0, b -> -10/3, c -> 0, d_u -> 5 - 1/1000])",
        "rewrite(cond, [a -> 1, b -> 2, c -> 3, d_u -> 4])",
        "length(n_solve(5*x^4 + 4*x^3 + 6*x^2 + 6*x + 4 == 0, x, reals))",
        "rewrite(cond, [a -> 0, b -> 0, c -> 0, d_u -> -1])",
    ]);
    assert_eq!(out[1], "true");
    assert_eq!(out[2], "false");
    // f' = 5x^4 + 4x^3 + 6x^2 + 6x + 4 has no real root exactly when the condition holds there
    assert_eq!(out[3] == "true", out[4] == "0", "{out:?}");
    assert_eq!(out[5], "false");
}

/// A library matrix builder asked for more than 2^26 entries declines before building anything (it used to run
/// for 98 s into the evaluation's memory limit); sizes under the limit are built as before.
#[test]
fn huge_library_matrices_decline_at_once() {
    let t = std::time::Instant::now();
    assert_eq!(ev("hankel_matrix(10000)"), "failure(limit, memory, [10000, 10000])");
    assert_eq!(ev("hankel_matrix(10000, target_structure: automatic)"), "failure(limit, memory, [10000, 10000])");
    assert_eq!(ev("toeplitz_matrix(9000)"), "failure(limit, memory, [9000, 9000])");
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
    assert_eq!(ev("hankel_matrix(4)"), "[[1, 2, 3, 4], [2, 3, 4, 0], [3, 4, 0, 0], [4, 0, 0, 0]]");
    assert_eq!(ev("dimensions(toeplitz_matrix(300))"), "[300, 300]");
    assert_eq!(ev("toeplitz_matrix([1, 2, 3])"), "[[1, 2, 3], [2, 1, 2], [3, 2, 1]]");
}
