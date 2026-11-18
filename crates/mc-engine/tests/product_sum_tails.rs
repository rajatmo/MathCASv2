//! Products and sums with no closed form at infinity: the tail summed through Hurwitz zeta functions,
//! the constant from the terms up to 200 and the tail beyond. Values are checked against the products and sums
//! evaluated directly.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn products_without_closed_form() {
    let a = "asymptotic_product((k^(13/4) - 1)/(k^(13/4) + 1), [k, 2, n], [n, inf, 3])";
    // the constant to 30 digits from 800 factors and the tail: the same to machine precision
    assert_eq!(ev(&format!("expand({a}/(1 + 8/(9*n^(9/4))))")), "0.726803404967682");
    // at n = 80 the error is of the order of the first term left out, n^(-13/4)
    assert_eq!(
        ev(&format!("abs(rewrite({a}, n -> 80) - n_product((k^(13/4) - 1)/(k^(13/4) + 1), [k, 2, 80])) < 2*80^(-13/4)")),
        "true"
    );
    // variations: another power, another start
    let b = "asymptotic_product(1 - 1/(k^3 + 2), [k, 1, n], [n, inf, 4])";
    assert_eq!(
        ev(&format!("abs(rewrite({b}, n -> 60) - n_product(1 - 1/(k^3 + 2), [k, 1, 60])) < 60^(-5)")),
        "true"
    );
    // an indefinite product is expanded by the sum of its logarithm (asymptotic_indefinite_sums.rs), without constant
    assert_eq!(ev("expand(asymptotic_product((k^(13/4) - 1)/(k^(13/4) + 1), k, [k, inf, 3]) - 1 - 8/(9*k^(9/4)))"), "0");
}

#[test]
fn sums_without_closed_form() {
    let a = "asymptotic_sum(1/(k^2 + sqrt(k)), [k, 1, n], [n, inf, 3])";
    assert!(!ev(a).contains("failure"), "{}", ev(a));
    assert_eq!(
        ev(&format!("abs(rewrite({a}, n -> 100) - n_sum(1/(k^2 + sqrt(k)), [k, 1, 100])) < 10^-6")),
        "true"
    );
    // a tail that is not a sum of powers (a logarithm) stays declined
    assert!(ev("asymptotic_sum(log(k)/(k^2 + sqrt(k)), [k, 1, n], [n, inf, 3])").starts_with("failure("));
}
