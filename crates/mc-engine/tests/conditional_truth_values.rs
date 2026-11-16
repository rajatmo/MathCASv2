//! Conditional truth values. A property that holds exactly when a condition on the parameters holds is
//! `conditional_expression(true, c)`; where a truth value is read (connectives, the test of `if`, `reduce`,
//! `resolve`, `find_instance`, `simplify` under facts) it is its condition. Also `reduce` of one variable over the
//! integers from the real solution set, and `element(x, integers)` conjuncts as the domain. The answers are
//! checked against the real solution sets and against membership at sample points.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn reduce_reads_a_conditional_property() {
    let mut e = CoreEngine::new();
    // E_n is meromorphic exactly for the integers n <= 0 (E_0(x) = e^-x/x)
    assert_eq!(ev(&mut e, "reduce(function_meromorphic(exp_integral_e(n, x), x))"), "element(n, integers) and n <= 0");
    assert_eq!(ev(&mut e, "reduce(function_meromorphic(polylog(a, z), z), a)"), "element(a, integers) and a <= 0");
    assert_eq!(ev(&mut e, "reduce(function_analytic(log(x^2 + a), x), a)"), "0 < a");
    assert_eq!(ev(&mut e, "resolve(function_analytic(log(x^2 + a), x))"), "a > 0");
    assert_eq!(ev(&mut e, "find_instance(conditional_expression(x^2 > 4, x > 0), x)"), "[[x -> 3]]");
}

#[test]
fn connectives_and_if_read_the_condition() {
    let mut e = CoreEngine::new();
    ev(&mut e, "c = function_analytic(log(x^2 + a), x)");
    assert_eq!(ev(&mut e, "c"), "conditional_expression(true, a > 0)");
    assert_eq!(ev(&mut e, "not(c)"), "a <= 0");
    assert_eq!(ev(&mut e, "c and b"), "a > 0 and b");
    assert_eq!(ev(&mut e, "c or b"), "a > 0 or b");
    assert_eq!(ev(&mut e, "if c then 1 else 2"), "if a > 0 then 1 else 2");
    // a value that is no truth value keeps the lifting
    assert_eq!(ev(&mut e, "conditional_expression(p, a > 0) and q"), "conditional_expression(p and q, a > 0)");
    // substituting a parameter decides it
    assert_eq!(ev(&mut e, "(c and b) /. a -> 2"), "b");
    assert_eq!(ev(&mut e, "not(c) /. a -> -1"), "true");
}

#[test]
fn simplify_decides_the_condition_under_facts() {
    let mut e = CoreEngine::new();
    ev(&mut e, "c = conditional_expression(true, a > 0)");
    assert_eq!(ev(&mut e, "simplify(c, assumptions: a > 1)"), "true");
    assert_eq!(ev(&mut e, "simplify(c, assumptions: a < -1)"), "false");
    assert_eq!(ev(&mut e, "assuming(a > 0, full_simplify(c))"), "true");
    assert_eq!(ev(&mut e, "simplify(c)"), "conditional_expression(true, a > 0)");
    assert_eq!(ev(&mut e, "simplify(conditional_expression(false, a != 0), assumptions: a > 1)"), "false");
    assert_eq!(ev(&mut e, "simplify(conditional_expression(sqrt(x^2), x < 0), assumptions: x < -1)"), "-x");
    assert_eq!(ev(&mut e, "simplify(conditional_expression(sqrt(x^2), x < 0))"), "conditional_expression(-x, x < 0)");
}

#[test]
fn the_condition_of_a_value_is_simplified_under_the_facts() {
    let mut e = CoreEngine::new();
    // the strip of a two-sided Laplace transform with a positive integer n: re(n) > 0 holds, re(s) > 0 stays
    ev(&mut e, "t = conditional_expression(s^-n, re(n) > 0 and re(s) > 0)");
    for f in ["simplify", "full_simplify", "refine"] {
        assert_eq!(ev(&mut e, &format!("{f}(t, element(n, positive_integers))")), "conditional_expression(s^-n, re(s) > 0)", "{f}");
        assert_eq!(ev(&mut e, &format!("{f}(t, assumptions: element(n, positive_integers))")), "conditional_expression(s^-n, re(s) > 0)", "{f}");
        assert_eq!(ev(&mut e, &format!("assuming(element(n, positive_integers), {f}(t))")), "conditional_expression(s^-n, re(s) > 0)", "{f}");
        // a list as the value, worked on under the condition; a truth value
        assert_eq!(ev(&mut e, &format!("{f}(conditional_expression([sqrt(x^2), 1], x > 0 and a > 0), x > 1)")), "conditional_expression([x, 1], a > 0)", "{f}");
        assert_eq!(ev(&mut e, &format!("{f}(conditional_expression(true, n > 0 and a > 0), n > 1)")), "conditional_expression(true, a > 0)", "{f}");
    }
    // facts that imply the whole condition give the value
    assert_eq!(ev(&mut e, "simplify(t, n > 0 and s > 0)"), "s^-n");
}

#[test]
fn reduce_over_the_integers_in_one_variable() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "reduce(x^2 < 5, x, integers)"), "x == -2 or x == -1 or x == 0 or x == 1 or x == 2");
    assert_eq!(ev(&mut e, "reduce(x > 1/2, x, integers)"), "element(x, integers) and x >= 1");
    assert_eq!(ev(&mut e, "reduce(x^2 > 2 and x < 3, x, integers)"), "element(x, integers) and x <= -2 or x == 2");
    assert_eq!(ev(&mut e, "reduce(x^2 == 3, x, integers)"), "false");
    // ends that are roots of a cubic (-1.879..., 0.347..., 1.532...)
    assert_eq!(ev(&mut e, "reduce(x^3 - 3*x + 1 > 0, x, integers)"), "x == -1 or x == 0 or element(x, integers) and x >= 2");
    assert_eq!(ev(&mut e, "reduce(x <= sqrt(10) and x > 0, x, integers)"), "x == 1 or x == 2 or x == 3");
    assert_eq!(ev(&mut e, "reduce(0 <= x and x <= 10^7, x, integers)"), "element(x, integers) and 0 <= x and x <= 10000000");
    assert_eq!(ev(&mut e, "reduce(element(n, integers))"), "element(n, integers)");
    assert_eq!(ev(&mut e, "reduce(x^2 == 4 and element(x, integers), x)"), "x == -2 or x == 2");
    // membership at sample points agrees with the answer
    assert_eq!(
        ev(&mut e, "r = reduce(x^2 - 7*x + 6 < 0 or x > 20, x, integers); table(r /. x -> k, [k, [0, 1, 2, 5, 6, 20, 21, 5/2]])"),
        "[false, false, true, true, false, false, true, false]"
    );
}
