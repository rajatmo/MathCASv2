//! `resolve` over the algebraic numbers agrees with the reals (order relations) and the complexes (equations).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))
}

#[test]
fn sentences_over_the_algebraics() {
    assert_eq!(ev("resolve(exists([x, y], (1/2 <= x^2 + y^2 and x^2 + y^2 <= 1) and y == x^2), algebraics)"), "true");
    assert_eq!(ev("resolve(exists(x, x^2 == 2), algebraics)"), "true");
    assert_eq!(ev("resolve(exists(x, x^2 + 1 < 0), algebraics)"), "false");
    assert_eq!(ev("resolve(exists(x, x^2 + 1 == 0), algebraics)"), "true");
    // with a parameter: the same condition as over the reals
    assert_eq!(ev("resolve(exists(x, x^2 + a < 0), algebraics)"), ev("resolve(exists(x, x^2 + a < 0), reals)"));
}
