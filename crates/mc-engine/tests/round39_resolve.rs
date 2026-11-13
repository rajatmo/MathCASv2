//! Round 39: resolve over a signed real domain (the sign condition on every variable, bound or free) and over the
//! booleans (each quantifier written out over true and false). Checked against resolve over the reals with the
//! conditions written by hand, and against the truth table.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn signed_real_domains() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "resolve(exists(x, y == sqrt(x)), positive_reals)"), "y > 0");
    assert_eq!(ev(&mut e, "resolve(exists(x, y == sqrt(x)), nonnegative_reals)"), "y >= 0");
    // variations: against the conditions written out
    assert_eq!(
        ev(&mut e, "resolve(exists(x, x^2 + y == 3), negative_reals) == resolve(y < 0 and exists(x, x < 0 and x^2 + y == 3), reals)"),
        "true"
    );
    assert_eq!(ev(&mut e, "resolve(for_all(x, x + y > 0), positive_reals)"), "y > 0");
}

#[test]
fn booleans() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "resolve(exists([p, q], p or q and not q), booleans)"), "true");
    // variations: a formula that is false for every value, and one with a free variable
    assert_eq!(ev(&mut e, "resolve(for_all(p, p and not p), booleans)"), "false");
    assert_eq!(ev(&mut e, "resolve(for_all(p, p or r), booleans)"), "r");
}
