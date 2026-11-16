//! Names of pattern variables are not evaluated, so a symbol with a value in the session can still name
//! a pattern variable.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn pattern_names_with_values() {
    // x has a value; x_ still binds x in the rule (and the value of x is untouched)
    assert_eq!(ev("x = 3; rule_u = x_: Float :> n(round(100*x)/100); rewrite(123.5/7, rule_u)"), "17.64");
    assert_eq!(ev("x = 3; [rewrite(f(1.5, 2), f(x_, y_) :> x + y), x]"), "[3.5, 3]");
    // the same rule with a name that has no value: the same result
    assert_eq!(ev("x = 3; rewrite(g(4), g(x_) :> x^2)"), ev("rewrite(g(4), g(t_) :> t^2)"));
    // sequences and named patterns
    assert_eq!(ev("p = 5; rewrite(h(1, 2, 3), h(p__) :> [p])"), ev("rewrite(h(1, 2, 3), h(s__) :> [s])"));
    assert_eq!(ev("q = 2; rewrite(g(1, 2), g(q @ _, r_) :> q + 10*r)"), "21");
    assert_eq!(ev("u = 7; rewrite(k(), k(u___) :> [u])"), ev("rewrite(k(), k(w___) :> [w])"));
    // a definition with such a name
    assert_eq!(ev("z = 10; rule sq(z_) := z^2; sq(4)"), "16");
    // the rewritten floats of an expression become rationals that give the same machine numbers back
    assert_eq!(ev("x = 1; r = rewrite(1.4142135623730951*y + 0.1, x_: Float :> rationalize(x, 0)); [n(coefficient(r, y)) == 1.4142135623730951, rewrite(r, y -> 0)]"), "[true, 1/10]");
}
