//! Assumptions (semantics 13): `assuming`, `refine` (the refinement table), the decision functions, `simplify` and
//! `power_expand` inside a context, the memo never reusing a result across contexts, 1 against 4 threads, and the
//! refinements checked numerically at random points that satisfy their facts.
mod common;
use common::{check, number, Lcg, Sys};
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// The refinement table: `(input, result)`, each row with the facts it needs, and the same input without them (or
/// with facts too weak) left alone.
const TABLE: &[(&str, &str)] = &[
    // (x^a)^c
    ("refine(sqrt(x^2), x >= 0)", "x"),
    ("refine(sqrt(x^2), x > 0)", "x"),
    ("refine(sqrt(x^2), x <= 0)", "-x"),
    ("refine(sqrt(x^2), x < 0)", "-x"),
    ("refine(sqrt(x^2), element(x, reals))", "abs(x)"),
    ("refine(sqrt(x^2))", "(x^2)^(1/2)"),
    ("refine((x^2)^(3/2), x > 0)", "x^3"),
    ("refine((x^4)^(1/4), element(x, reals))", "abs(x)"),
    ("refine((x^a)^b, x > 0 and element([a, b], reals))", "x^(a*b)"),
    ("refine((x^a)^b, x > 0)", "(x^a)^b"),
    ("refine(sqrt(x^2 + 2*x + 1), x > -1)", "1 + x"),
    ("refine(sqrt(x^2 - 2*x + 1), x < 1)", "1 - x"),
    // products under a fractional power
    ("refine(sqrt(x^2*y^2), element([x, y], reals))", "abs(x)*abs(y)"),
    ("refine(sqrt(x*y), x > 0)", "x^(1/2)*y^(1/2)"),
    ("refine(sqrt(x*y))", "(x*y)^(1/2)"),
    ("refine(abs(x)^2, element(x, reals))", "x^2"),
    ("refine(abs(x)^2)", "abs(x)^2"),
    // (-1)^k
    ("refine((-1)^(2*n), element(n, integers))", "1"),
    ("refine((-1)^(2*n + 1), element(n, integers))", "-1"),
    ("refine((-1)^n, element(n, integers))", "(-1)^n"),
    ("refine((-1)^(3*n + 4*m + 2), element([m, n], integers))", "(-1)^n"),
    ("refine((-1)^(2*n))", "(-1)^(2*n)"),
    // abs and sign
    ("refine(abs(x), x > 0)", "x"),
    ("refine(abs(x), x <= 0)", "-x"),
    ("refine(abs(x - 1), x < 1)", "1 - x"),
    ("refine(abs(x*y), x > 0)", "x*abs(y)"),
    ("refine(abs(x*y), x > 0 and y < 0)", "-1*x*y"),
    ("refine(abs(x), element(x, reals))", "abs(x)"),
    ("refine(sign(x), x > 0)", "1"),
    ("refine(sign(x), x < 0)", "-1"),
    ("refine(sign(x), x == 0)", "0"),
    ("refine(sign(x*y), x > 0 and y < 0)", "-1"),
    ("refine(sign(x*y), x < 0)", "-sign(y)"),
    ("refine(sign(x))", "sign(x)"),
    // logarithms
    ("refine(log(x^3), x > 0)", "3*log(x)"),
    ("refine(log(x^a), x > 0 and element(a, reals))", "a*log(x)"),
    ("refine(log(x^3))", "log(x^3)"),
    ("refine(log(x^(1/2)))", "(1/2)*log(x)"),
    ("refine(log(2*x*y), x > 0)", "log(2) + log(x) + log(y)"),
    ("refine(log(x*y), x > 0 and y > 0)", "log(x) + log(y)"),
    ("refine(log(x*y))", "log(x*y)"),
    ("refine(log(exp(x)), element(x, reals))", "x"),
    ("refine(log(exp(x)))", "log(exp(x))"),
    ("refine(exp(log(x)))", "x"),
    // multiples of pi
    ("refine(sin(n*pi), element(n, integers))", "0"),
    ("refine(tan(3*k*pi), element(k, integers))", "0"),
    ("refine(cos(n*pi), element(n, integers))", "(-1)^n"),
    ("refine(cos(2*n*pi), element(n, integers))", "1"),
    ("refine(cos((2*n + 1)*pi/2), element(n, integers))", "0"),
    ("refine(sin((2*n + 1)*pi/2), element(n, integers))", "(-1)^n"),
    ("refine(sin(x + 2*n*pi), element(n, integers))", "sin(x)"),
    ("refine(cos(x + n*pi), element(n, integers))", "cos(x)*(-1)^n"),
    ("refine(tan(x - 3*n*pi), element(n, integers))", "tan(x)"),
    ("refine(sin(n*pi), element(n, reals))", "sin(n*pi)"),
    ("refine(cos(n*pi))", "cos(n*pi)"),
    // comparisons
    ("refine(x > 0, x > 1)", "true"),
    ("refine(x < 0, x > 1)", "false"),
    ("refine(x - 2 > 0, x > 3)", "true"),
    ("refine(y - x > 0, x < y)", "true"),
    ("refine(x == y, x < y)", "false"),
    ("refine(x^2 + 1 > 0, element(x, reals))", "true"),
    ("refine(x > 0, x > -1)", "x > 0"),
    // nothing to do, other shapes
    ("refine(x)", "x"),
    ("refine(5)", "5"),
    ("refine([sqrt(x^2), abs(y)], x > 0 and y < 0)", "[x, -y]"),
    ("refine(f(sqrt(x^2)), x > 0)", "f(x)"),
];

#[test]
fn refinement_table() {
    check(TABLE);
}

#[test]
fn facts_and_contexts() {
    check(&[
        // the exit cases
        ("assuming(x > 0, simplify(sqrt(x^2)))", "x"),
        ("simplify(sqrt(x^2))", "(x^2)^(1/2)"),
        // refine without facts reads the context
        ("assuming(x < 0, refine(sqrt(x^2)))", "-x"),
        // facts: and, lists, nested contexts, reversed sides, a variable holding facts
        ("assuming([x > 0, y < 0], refine(abs(x) + abs(y)))", "x - y"),
        ("assuming(x > 0, assuming(y < 0, refine(abs(x*y))))", "-1*x*y"),
        ("assuming(0 < x, refine(abs(x)))", "x"),
        ("facts = x > 0 and element(n, integers)", "null"),
        ("assuming(facts, refine(abs(x)*cos(n*pi)))", "x*(-1)^n"),
        ("refine(abs(x), [x > 0])", "x"),
        // domains
        ("refine(abs(x), element(x, positive_reals))", "x"),
        ("refine(abs(n), element(n, negative_integers))", "-n"),
        ("refine(sqrt(n^2), element(n, nonnegative_integers))", "n"),
        ("refine((-1)^(2*n), element(n, complexes))", "(-1)^(2*n)"),
        // order facts between expressions
        ("refine(abs(x - y), x < y)", "y - x"),
        ("refine(abs(y - x), x <= y)", "y - x"),
        // unknown facts prove nothing
        ("refine(abs(x), is_happy(x))", "abs(x)"),
        ("refine(abs(x), element(x, unknown_domain))", "abs(x)"),
        // the context ends with the body
        ("assuming(x > 0, refine(abs(x))) + refine(abs(x))", "x + abs(x)"),
        // malformed calls are left alone
        ("assuming(x > 0)", "assuming(x > 0)"),
        ("refine()", "refine()"),
        ("refine(a, b, c)", "refine(a, b, c)"),
        // power_expand: formal outside, reading the context inside
        ("power_expand(sqrt(x^2))", "x"),
        ("assuming(x > 0, power_expand(sqrt(x^2)))", "x"),
        ("assuming(x < 0, power_expand(sqrt(x^2)))", "-x"),
        ("assuming(x < 0, power_expand(log(x*y)))", "log(x*y)"),
        ("assuming(x > 0, power_expand(log(x*y)))", "log(x) + log(y)"),
        // simplify with refinement as a move
        ("assuming(x > -1, simplify(sqrt(x^2 + 2*x + 1)))", "1 + x"),
        ("assuming(element(x, reals), simplify(sqrt(x^2) - abs(x)))", "0"),
        ("assuming(element(n, integers), simplify(sin(x + 2*n*pi)^2 + cos(x)^2))", "1"),
    ]);
}

#[test]
fn decision_functions() {
    check(&[
        // outside every context: as before (numbers and closed numeric expressions)
        ("is_positive(pi)", "true"),
        ("is_positive(pi - 3)", "true"),
        ("is_positive(3 - pi)", "false"),
        ("is_negative(-2)", "true"),
        ("is_positive(0)", "false"),
        ("is_positive(x)", "is_positive(x)"),
        ("is_real(2)", "true"),
        ("is_real(1im)", "false"),
        ("is_real(x)", "is_real(x)"),
        ("is_integer(3)", "true"),
        ("is_integer(n)", "false"),
        ("is_nonnegative(0)", "true"),
        ("is_nonnegative(-1/2)", "false"),
        ("is_nonnegative(x^2)", "is_nonnegative(x^2)"),
        // inside contexts
        ("assuming(x > 0, is_positive(x))", "true"),
        ("assuming(x > 0, is_negative(x))", "false"),
        ("assuming(x > 0, is_positive(x^3 + 2*x))", "true"),
        ("assuming(x > 1, is_positive(log(x)))", "true"),
        ("assuming(0 < x and x < 1, is_negative(log(x)))", "true"),
        ("assuming(x > 3, is_positive(x - 2))", "true"),
        ("assuming(x > 0, is_positive(y))", "is_positive(y)"),
        ("assuming(element(x, reals), is_nonnegative(x^2))", "true"),
        ("assuming(element(x, reals), is_nonnegative(exp(x) + abs(y)))", "true"),
        ("assuming(element(x, reals), is_positive(x^2))", "is_positive(x^2)"),
        ("assuming(x != 0 and element(x, reals), is_positive(x^2))", "true"),
        ("assuming(x <= 0, is_positive(x))", "false"),
        ("assuming(x < 0, is_nonnegative(x))", "false"),
        ("assuming(element(x, reals), is_real(sin(x)*exp(x) + x^3))", "true"),
        ("assuming(x > 0, is_real(log(x)))", "true"),
        ("assuming(x > 0, is_real(sqrt(x)))", "true"),
        ("assuming(element(x, reals), is_real(sqrt(x)))", "is_real(x^(1/2))"),
        ("assuming(element(n, integers), is_integer(2*n + 1))", "true"),
        ("assuming(element([m, n], integers), is_integer(m*n^2 - 3))", "true"),
        ("assuming(element(n, integers), is_integer(n/2))", "false"),
        ("assuming(element(n, positive_integers), is_positive(n))", "true"),
        // a rule whose condition calls a decision function
        ("rule pos_part(u_) := u when is_positive(u)", "null"),
        ("pos_part(x)", "pos_part(x)"),
        ("assuming(x > 0, pos_part(x))", "x"),
        ("pos_part(x)", "pos_part(x)"),
    ]);
}

#[test]
fn no_result_is_reused_across_contexts() {
    check(&[
        ("fn f(u) = simplify(sqrt(u^2))", "null"),
        ("f(x)", "(x^2)^(1/2)"),
        ("assuming(x > 0, f(x))", "x"),
        ("assuming(x < 0, f(x))", "-x"),
        ("assuming(element(x, reals), f(x))", "abs(x)"),
        ("f(x)", "(x^2)^(1/2)"),
        ("assuming(x > 0, f(x))", "x"),
        ("[f(x), assuming(x < 0, f(x)), f(x)]", "[(x^2)^(1/2), -x, (x^2)^(1/2)]"),
        // a function declared to read the context
        ("@uses_assumptions fn g(u) = refine(abs(u))", "null"),
        ("g(x)", "abs(x)"),
        ("assuming(x < 0, g(x))", "-x"),
        ("g(x)", "abs(x)"),
        // the same context written differently is the same set of facts
        ("assuming(x > 0 and y > 0, refine(abs(x*y)))", "x*y"),
        ("assuming([y > 0, x > 0, y > 0], refine(abs(x*y)))", "x*y"),
    ]);
}

/// The cases of this file evaluated in one session per configuration.
fn session_outputs(threads: usize, memo: bool) -> Vec<String> {
    let mut cfg = EngineConfig { threads, ..EngineConfig::default() };
    cfg.eval.memo = memo;
    cfg.eval.split = if threads > 1 { mc_eval::Split::Always } else { mc_eval::Split::Lazy };
    let mut e = CoreEngine::with_config(cfg);
    let stop = StopFlag::new();
    let mut out = vec![];
    let srcs = TABLE.iter().map(|c| c.0.to_string()).chain([
        "assuming(x > 0, map(u => refine(sqrt(u^2)), [x, 2*x, x^3, x + 1, -x]))".to_string(),
        "assuming(element(n, integers), map(k => refine(cos(k*n*pi)), [1, 2, 3, 4, 5, 6]))".to_string(),
        "assuming(x > 0, [simplify(sqrt(x^2)), simplify(sqrt(y^2)), is_positive(x), is_positive(y)])".to_string(),
    ]);
    for src in srcs {
        out.push(e.eval(&src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error {err}")));
    }
    out
}

#[test]
fn one_and_four_threads_agree() {
    let one = session_outputs(1, true);
    assert_eq!(one[TABLE.len()], "[x, 2*x, x^3, 1 + x, x]");
    assert_eq!(one[TABLE.len() + 1], "[(-1)^n, 1, (-1)^n, 1, (-1)^n, 1]");
    assert_eq!(one[TABLE.len() + 2], "[x, (y^2)^(1/2), true, is_positive(y)]");
    for (threads, memo) in [(4, true), (4, false), (1, false)] {
        assert_eq!(session_outputs(threads, memo), one, "{threads} threads, memo {memo}");
    }
}

/// A variable and the interval its random values come from.
type Var = (&'static str, f64, f64);

/// Refinements against their inputs at random points satisfying the facts: `(input, facts, (variable, low,
/// high)...)`.
const NUMERIC: &[(&str, &str, &[Var])] = &[
    ("sqrt(x^2)", "x < 0", &[("x", -5.0, -0.1)]),
    ("sqrt(x^2)", "element(x, reals)", &[("x", -5.0, 5.0)]),
    ("sqrt(x^2 - 2*x + 1)", "x < 1", &[("x", -4.0, 0.9)]),
    ("(x^2)^(3/2)", "x > 0", &[("x", 0.1, 4.0)]),
    ("abs(x*y)", "x > 0 and y < 0", &[("x", 0.1, 4.0), ("y", -4.0, -0.1)]),
    ("sign(x*y)", "x < 0", &[("x", -4.0, -0.1), ("y", -4.0, 4.0)]),
    ("log(x^3)", "x > 0", &[("x", 0.1, 9.0)]),
    ("log(2*x*y)", "x > 0", &[("x", 0.1, 9.0), ("y", -9.0, 9.0)]),
    ("log(exp(x))", "element(x, reals)", &[("x", -9.0, 9.0)]),
    ("sqrt(x*y)", "x > 0", &[("x", 0.1, 9.0), ("y", -9.0, 9.0)]),
];

#[test]
fn refinements_keep_the_value_where_the_facts_hold() {
    let mut s = Sys::new();
    let mut r = Lcg(7);
    for (k, (e, facts, vars)) in NUMERIC.iter().enumerate() {
        let refined = s.run(&format!("refine({e}, {facts})"));
        assert_ne!(refined, s.run(e), "{e} under {facts} is refined");
        for _ in 0..4 {
            let rules: Vec<String> = vars.iter().map(|(v, lo, hi)| format!("{v} -> {}", r.point(*lo, *hi))).collect();
            let rules = rules.join(", ");
            let diff = s.run(&format!("n(abs(rewrite({e}, [{rules}]) - rewrite({refined}, [{rules}])))"));
            assert!(number(&diff) < 1e-12, "{e} -> {refined} at {rules}: {diff}");
        }
        eprintln!("[{}/{} {}%] {e}", k + 1, NUMERIC.len(), 100 * (k + 1) / NUMERIC.len());
    }
}

#[test]
fn interrupt_stops_a_long_refinement() {
    let mut s = Sys::new();
    s.run("p = expand((1 + x + y + z)^40)");
    for ms in [5, 50, 300] {
        let (r, took) = s.interrupted("assuming(x > 0 and y > 0, simplify(sqrt(p^2) + abs(p)))", std::time::Duration::from_millis(ms));
        assert!(took < std::time::Duration::from_millis(ms) + std::time::Duration::from_secs(5), "after {ms} ms: took {took:?}: {r:?}");
    }
    // the session is still usable, and no context is left behind
    assert_eq!(s.run("refine(abs(x))"), "abs(x)");
    assert_eq!(s.run("assuming(x > 0, refine(abs(x)))"), "x");
}
