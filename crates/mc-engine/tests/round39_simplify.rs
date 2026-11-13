//! Round 39: full_simplify recognises at once a term that function_expand brings to 0 (and an equation it brings
//! to true), where the search over the gamma forms of the terms took long. The zero is checked numerically at a point.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn zeros_through_function_expand() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "full_simplify(beta(a, b) == beta(b, a))"), "true");
    assert_eq!(ev(&mut e, "full_simplify(beta(a, b) - beta(b, a))"), "0");
    // variations: a gamma ratio against beta, a binomial against its gamma form; each also zero at a point
    assert_eq!(ev(&mut e, "full_simplify(gamma(a)*gamma(b)/gamma(a + b) - beta(b, a))"), "0");
    assert_eq!(ev(&mut e, "full_simplify(binomial(n, k) - gamma(n + 1)/(gamma(k + 1)*gamma(n - k + 1)))"), "0");
    assert_eq!(ev(&mut e, "chop(n(rewrite(gamma(a)*gamma(b)/gamma(a + b) - beta(b, a), [a -> 13/10, b -> 27/10]), 30), 10^-25)"), "0");
    // a term that is not zero is simplified as before
    assert_eq!(ev(&mut e, "full_simplify(beta(a, b) - beta(a, c)) === 0"), "false");
}
