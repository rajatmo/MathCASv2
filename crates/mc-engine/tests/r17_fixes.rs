//! Round 17 fixes: `unset` during an evaluation, `protected`, the `assumptions:` option replacing
//! the context, `simplify` on powers of one base and gamma ratios, symbolic `binomial`, and the extended list
//! functions; on source text.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate each `(source, printed result)` pair in order in one engine session (two threads).
fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let mut bad = vec![];
    for (src, want) in cases {
        let got = match e.eval(src, &StopFlag::new()) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        if got != *want {
            bad.push(format!("{src}
   got  {got}
   want {want}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("
"));
}

#[test]
fn unset_removes_values_and_rules_during_an_evaluation() {
    check(
        &[
            ("x = 5", "null"),
            ("{ unset(x); x + 1 }", "1 + x"),
            ("rule sq(y_) := y^2", "null"),
            ("[sq(3), { unset(sq); sq(3) }]", "[9, sq(3)]"),
            // one rule, matched up to the names of the pattern variables
            ("rule g(0) := 1", "null"),
            ("rule g(n_) := 10*n", "null"),
            ("[g(0), g(3)]", "[1, 30]"),
            ("unset(g(0))", "null"),
            ("g(0)", "0"),
            ("unset(g(m_))", "null"),
            ("g(3)", "g(3)"),
            ("unset(g(1))", "failure(not_defined, g(1))"),
            // attributes stay; several at once; definitions again after unset in the same block
            ("set_attributes(lf, listable)", "null"),
            ("rule lf(z_) := z + 1", "null"),
            ("{ unset(lf, x); lf(z_) := z - 1; lf([1, 2]) }", "[0, 1]"),
            ("attributes(lf)", "[listable]"),
            // memoised results that read the symbol are stale
            ("c1 = 2", "null"),
            ("[c1^2, { unset(c1); c1^2 }]", "[4, c1^2]"),
        ],
    );
}

#[test]
fn protected_symbols_keep_their_definitions() {
    check(
        &[
            ("rule area(r_) := pi*r^2", "null"),
            ("protect(area)", "[\"area\"]"),
            ("protect(area)", "[]"),
            ("attributes(area)", "[protected]"),
            ("rule area(r_) := 2*r", "failure(protected, area)"),
            ("area(2)", "4*pi"),
            ("unset(area)", "failure(protected, area)"),
            ("set_attributes(area, listable)", "failure(protected, area)"),
            ("lim = 10", "null"),
            ("protect([lim])", "[\"lim\"]"),
            ("lim = 20", "failure(protected, lim)"),
            ("lim", "10"),
            ("unprotect(\"lim\")", "[\"lim\"]"),
            ("{ lim = 20; lim }", "20"),
            ("clear_attributes(area, protected)", "null"),
            ("rule area(r_) := 2*r", "null"),
            ("area(2)", "4"),
        ],
    );
}

#[test]
fn the_assumptions_option_replaces_the_context() {
    check(
        &[
            ("assuming(x > 0, simplify(sqrt(x^2*y^2), assumptions: y < 0))", "-1*y*(x^2)^(1/2)"),
            ("assuming(x > 0, refine(sqrt(x^2*y^2), assumptions: y < 0))", "-1*y*(x^2)^(1/2)"),
            ("assuming(x > 0, refine(sqrt(x^2)))", "x"),
            ("assuming(x > 0, refine(sqrt(x^2), assumptions: true))", "(x^2)^(1/2)"),
            // positional facts extend
            ("assuming(x > 0, refine(sqrt(x^2*y^2), y < 0))", "-1*x*y"),
        ],
    );
}

#[test]
fn simplify_powers_and_gamma_ratios() {
    check(
        &[
            ("simplify(2*2^(1 + n) - 2^(2 + n))", "0"),
            ("simplify(2^n*2 - 2^(n + 1))", "0"),
            ("simplify(3^(n + 2) - 9*3^n)", "0"),
            ("simplify(2^(n + 1))", "2^(1 + n)"),
            ("simplify(gamma(m + 1)/gamma(m))", "m"),
            ("simplify(gamma(n + 2)/gamma(n + 1))", "1 + n"),
            ("simplify(factorial(n + 1)/factorial(n))", "1 + n"),
            ("simplify(gamma(z + 3)/gamma(z))", "z*(1 + z)*(2 + z)"),
            ("simplify(n*gamma(n)/gamma(n + 2))", "(1 + n)^-1"),
            ("[binomial(m, 0), binomial(m, 1), binomial(m, m), binomial(m, m - 1), binomial(m, 2)]", "[1, m, 1, m, binomial(m, 2)]"),
            // a tie in size (four leaves each): since round 19 (n12.4) the gamma form is taken
            ("simplify(x*gamma(x))", "gamma(1 + x)"),
            ("simplify(exp(log_gamma(x) + log_gamma(y)))", "gamma(x)*gamma(y)"),
            ("simplify(cosh(x) - sinh(x))", "exp(-x)"),
            ("function_expand(factorial_power(i, -3))", "(1 + i)^-1/(2 + i)/(3 + i)"),
            ("function_expand(chebyshev_t(n, x))", "cos(n*acos(x))"),
            ("function_expand(dirac_delta(x, y))", "dirac_delta(x)*dirac_delta(y)"),
            ("refine((a^b)^c, a >= 0 and element(b, reals))", "a^(b*c)"),
        ],
    );
}

#[test]
fn extended_list_functions() {
    check(
        &[
            ("[first([], x), first([a, b], x), last([], x), last([a, b], x)]", "[x, a, x, b]"),
            ("level([a, b, c], 1, heads: true)", "[list, a, b, c]"),
            ("level(p(x)(y), 2, heads: true)", "[p, x, p(x), y]"),
            ("level(1 + a + b^2, [-1], heads: true)", "[add, 1, a, pow, b, 2]"),
            ("position_largest([100, 200, 100, 200, 50], 3)", "[[4, 2], [3, 1], [5]]"),
            ("position_smallest([1, 2, 2, 3, 4], 5)", "[[1], [2, 3], [4], [5]]"),
            ("position_largest([100, 200, 100, 200, 50])", "[2, 4]"),
            ("position_largest([100, n(pi), 200, sqrt(3), 200, pi, 77])", "[3, 5]"),
            ("position_smallest(assoc(\"a\" -> 1, \"b\" -> 22, \"c\" -> 3, \"d\" -> inf), 2)", "[[1], [3]]"),
            ("power_range(10^3)", "[1, 10, 100, 1000]"),
            ("power_range(a, a^3, a)", "[a, a^2, a^3]"),
            ("count_distinct([3., 3.1, 3.11, 3.5, 3.51], (u, v) => abs(u - v) <= 0.01)", "3"),
            ("thread(f([a, b], [r, s], [u, v]), list, [1, -1, 2])", "[f(a, [r, s], u), f(b, [r, s], v)]"),
            ("thread(f([a, b], [r, s]), list, none)", "f([a, b], [r, s])"),
            ("array(f, 3, [0, 1])", "[f(0), f(1/2), f(1)]"),
            ("array(a, [2, 2], 1, add)", "a(1, 1) + a(1, 2) + a(2, 1) + a(2, 2)"),
            ("dimensions([[[[a, b]]]], 2)", "[1, 1]"),
            ("dimensions(f(f(1, 2), f(3, 4)), allowed_heads: list)", "[]"),
            ("comap(f(f(f(f))), x, [2])", "f(f(f(f)(x)))"),
            ("sort([\"cat\", \"fish\", \"catfish\", \"Cat\"])", "[\"cat\", \"Cat\", \"catfish\", \"fish\"]"),
            ("hash(\"abc\", \"MD5\")", "191415658344158766168031473277922803570"),
            ("hash(\"\", \"SHA256\", \"HexString\")", "\"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\""),
            ("hash(x^2 + 1) === hash(1 + x^2)", "true"),
            ("[a, b, c][span(up_to(4), 2, -1)]", "[c, b]"),
            ("range(10)[span(all, -1, 3)]", "[1, 4, 7, 10]"),
            ("take_largest([1, 3, 5, none, indeterminate, missing()], 2)", "[5, 3]"),
            ("take_largest([pi^e, exp(pi), 7*pi, -inf, 23], 3)", "[exp(pi), 23, pi^e]"),
            ("leaf_count(f(a, b)(x, y), heads: false)", "2"),
            ("[value_q(sin(2), method: \"TrialEvaluation\"), value_q(sin(2), method: \"SymbolDefinitionsPresent\")]", "[false, true]"),
            ("groupings(4, 2)", "[[[[1, 2], 3], 4], [1, [[2, 3], 4]], [[1, [2, 3]], 4], [1, [2, [3, 4]]], [[1, 2], [3, 4]]]"),
            ("groupings([a, b], [neg -> 1, f -> 2])", "[f(a, b), neg(f(a, b)), f(a, neg(b)), neg(f(a, neg(b))), f(neg(a), b), neg(f(neg(a), b)), f(neg(a), neg(b)), neg(f(neg(a), neg(b)))]"),
            ("groupings([a, a, a, a], [2, orderless])", "[[[[a, a], a], a], [[a, a], [a, a]]]"),
            ("table(length(groupings(n, 2)), n, 8)", "[1, 1, 2, 5, 14, 42, 132, 429]"),
        ],
    );
}
