//! Decompositions with real constant coefficients, integer optima over unbounded feasible sets, optimum values in
//! several variables as one root object, and formulas that are propositional tautologies.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))
}

/// The evaluation, which must end within `limit`.
fn within(src: &str, limit: Duration) -> String {
    let t = Instant::now();
    let r = ev(src);
    assert!(t.elapsed() < limit, "{src} took {:?}", t.elapsed());
    r
}

#[test]
fn constant_coefficients_as_parameters() {
    let s = Duration::from_secs(30);
    assert_eq!(within("cylindrical_decomposition(pi*x^2 < 1, [x])", s), "-pi^(-1/2) < x and x < pi^(-1/2)");
    assert_eq!(within("cylindrical_decomposition(x^2 + y^2 < pi and x > e, [x, y])", s), "false");
    assert_eq!(
        within("cylindrical_decomposition(e^pi * x^2 + pi^e * y^2 <= 77 and log(2) * x + log(3) * y == 2, [x, y])", s),
        "(-(77*exp(pi)*log(3)^4 + -4*exp(pi)*pi^e*log(3)^2 + 77*pi^e*log(2)^2*log(3)^2)^(1/2) + 2*log(2)*pi^e)/(exp(pi)*log(3)^2 + pi^e*log(2)^2) <= x and x <= (2*log(2)*pi^e + (77*exp(pi)*log(3)^4 + -4*exp(pi)*pi^e*log(3)^2 + 77*pi^e*log(2)^2*log(3)^2)^(1/2))/(exp(pi)*log(3)^2 + pi^e*log(2)^2) and y == (2 + -1*x*log(2))/log(3)"
    );
    let r = within("cylindrical_decomposition(sqrt(2) * x^2 + root(&(&1^5 - 2 * &1 - 11), 1) * y^2 < 1, [x, y])", s);
    assert!(r.starts_with("(-1/2)*2^(3/4) < x and x < (1/2)*2^(3/4) and "), "{r}");
    // a complex coefficient, a machine number, more constants than the bound: no answer, at once
    for src in [
        "cylindrical_decomposition(im*x^2 < 1, [x])",
        "cylindrical_decomposition(0.5*pi*x^2 < 1, [x])",
        "cylindrical_decomposition(pi*x + e*y + log(2)*z + log(3)*w + log(5) > 0 and x < 1, [x, y, z, w])",
    ] {
        assert!(within(src, s).starts_with("cylindrical_decomposition("), "{src}");
    }
}

#[test]
fn integer_optima_over_unbounded_sets() {
    let s = Duration::from_secs(30);
    assert_eq!(within("minimize([x^2 + x * y, x * y > 1], [x, y], integers)", s), "[3, [x -> -1, y -> -2]]");
    assert_eq!(within("maximize([-x^2 - x * y, x * y > 1], [x, y], integers)", s), "[-3, [x -> -1, y -> -2]]");
    assert_eq!(within("arg_min([x^2 + x * y, x * y > 1], [x, y], integers)", s), "[-1, -2]");
    assert_eq!(within("min_value([x^2 + y^2, x + y > 5/2], [x, y], integers)", s), "5");
    assert_eq!(within("minimize([x^2 - y, y < x], [x, y], integers)", s), "[1, [x -> 0, y -> -1]]");
    // a sublevel set too wide for the bound on the ranges: stays unevaluated
    assert!(within("minimize([x^2 + x*y + 1, x*y > 1 and x > 100], [x, y], integers)", s).starts_with("minimize("));
    // unbounded below: no feasible minimum is found, the sublevel ranges are unbounded
    assert!(within("minimize([x - y, x * y > 1], [x, y], integers)", s).starts_with("minimize("));
}

#[test]
fn optimum_values_in_several_variables_as_one_root() {
    let s = Duration::from_secs(60);
    assert_eq!(within("min_value([sqrt(x + y) + x, x * sqrt(y) >= 1], [x, y])", s), "root(&(-283 + -504*&1^2 + -288*&1^3 + -216*&1 + 4*&1^6 + 16*&1^4 + 16*&1^7), 1)");
    assert_eq!(within("max_value([-sqrt(x + y) - x, x * sqrt(y) >= 1], [x, y])", s), "-root(&(-283 + -504*&1^2 + -288*&1^3 + -216*&1 + 4*&1^6 + 16*&1^4 + 16*&1^7), 1)");
    // one variable: the objective at the root stays
    assert_eq!(
        within("min_value(x^4 + 2 * x^3 + 5 * x - 7, x)", s),
        "-7 + 2*root(&(5 + 4*&1^3 + 6*&1^2), 1)^3 + 5*root(&(5 + 4*&1^3 + 6*&1^2), 1) + root(&(5 + 4*&1^3 + 6*&1^2), 1)^4"
    );
}

#[test]
fn propositional_tautologies() {
    let s = Duration::from_secs(30);
    assert_eq!(within("resolve(exists(x, x > y) or not exists(x, x > y))", s), "true");
    assert_eq!(within("resolve(exists(x, x > y) and not exists(x, x > y))", s), "false");
    assert_eq!(within("resolve(implies(for_all(x, x^2 > y), for_all(x, x^2 > y)))", s), "true");
    assert_eq!(within("cylindrical_decomposition(xor(x > y, x > y), [x, y])", s), "false");
    // not a tautology: decided by the decomposition
    assert_eq!(within("resolve(exists(x, x^2 < y) or exists(x, x > y))", s), "true");
}
