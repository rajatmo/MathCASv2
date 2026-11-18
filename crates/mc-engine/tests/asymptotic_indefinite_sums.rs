//! asymptotic_sum and asymptotic_product of indefinite sums and products without a closed form: Euler-Maclaurin
//! in k -> inf, and term by term in a parameter -> inf. Each result is checked here by its difference (quotient)
//! against the summand (factor), and against a known value. Time-bounded.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn euler_maclaurin_for_indefinite_sums() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    let v = ev(&mut e, "ff = asymptotic_sum(polygamma(k)/(k + 1), k, [k, inf, 2]); ff");
    assert!(!v.contains("failure"), "{v}");
    assert_eq!(ev(&mut e, "expand(ff - (log(k)^2/2 + (3 + log(k))/(2*k) - (3 + 2*log(k))/(24*k^2)))"), "0");
    // F(k + 1) - F(k) - f at k = 10^4 is far below k^-3
    assert_eq!(ev(&mut e, "abs(n(rewrite(ff, k -> 10001) - rewrite(ff, k -> 10000) - polygamma(10000)/10001, 30)) < 10^-12"), "true");
    // a summand with a closed-form sum gives the same expansion as its closed form
    assert_eq!(ev(&mut e, "expand(asymptotic_sum(log(k)/k, k, [k, inf, 3]) - (log(k)^2/2 - log(k)/(2*k) + (1 - log(k))/(12*k^2)))"), "0");
    // exponentially small terms, and orders outside the bounds: no answer
    for src in [
        "asymptotic_sum(1/factorial(k), k, [k, inf, 2])",
        "asymptotic_sum(polygamma(k)/(k + 1), k, [k, inf, -1])",
        "asymptotic_sum(polygamma(k)/(k + 1), k, [k, inf, 31])",
    ] {
        assert!(ev(&mut e, src).starts_with("failure("), "{src}");
    }
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}

#[test]
fn indefinite_sums_in_a_parameter_and_products() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    // 1/(1 + m k^2) = sum_j (-1)^(j - 1) m^-j k^-2j, each summed as a polygamma of k
    let v = ev(&mut e, "ii = asymptotic_sum(1/(1 + m*k^2), k, [m, inf, 4]); ii");
    assert!(v.contains("polygamma(7, k)"), "{v}");
    assert_eq!(ev(&mut e, "expand(function_expand(normal(series(rewrite(ii, k -> k + 1) - ii - 1/(1 + m*k^2), m, inf, 4))))"), "0");
    // a series that vanishes to the order asked is an order term alone; its truncated sum is 0
    assert_eq!(ev(&mut e, "normal(series(difference_delta(ii, k) - 1/(1 + m*k^2), m, inf, 4))"), "0");
    assert_eq!(ev(&mut e, "[normal(series(1/m^6, m, inf, 4)), normal(O(x^3)), normal(1 + O(x^3))]"), "[0, 0, 1]");
    for m in ["1", "2", "3"] {
        assert!(!ev(&mut e, &format!("asymptotic_sum(1/(1 + m*k^2), k, [m, inf, {m}])")).contains("failure"), "order {m}");
    }
    // a factor tending to 1: P(k + 1)/P(k) = f to order 3
    let p = ev(&mut e, "pp = asymptotic_product((k^(13/4) - 1)/(k^(13/4) + 1), k, [k, inf, 3]); pp");
    assert!(!p.contains("failure"), "{p}");
    assert_eq!(ev(&mut e, "expand(pp - 1 - 8/(9*k^(9/4)))"), "0");
    assert_eq!(ev(&mut e, "abs(n(rewrite(pp, k -> 1001)/rewrite(pp, k -> 1000) - (1000^(13/4) - 1)/(1000^(13/4) + 1), 30)) < 10^-9"), "true");
    // a factor that does not tend to 1: no answer
    assert!(ev(&mut e, "asymptotic_product(2 + 1/k^(5/2), k, [k, inf, 4])").starts_with("failure("));
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}
