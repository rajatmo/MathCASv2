//! Round 42: differences of gamma functions of one variable proved 0 by the multiplication formula and the
//! recurrence before the search of `simplify` and `full_simplify`. Each identity is also checked numerically at a
//! non-integer point at 30 digits, and an expression that is not 0 keeps its value.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn catalan_numbers_as_central_binomials() {
    let mut e = CoreEngine::new();
    ev(&mut e, "eq = catalan_number(n) == binomial(2*n, n) - binomial(2*n, n + 1)");
    assert_eq!(ev(&mut e, "full_simplify(function_expand(eq))"), "true");
    assert_eq!(ev(&mut e, "simplify(function_expand(eq))"), "true");
    // the same equation at n = 7/3, numerically
    assert_eq!(ev(&mut e, "f = function_expand(eq); abs(n(rewrite(f[1] - f[2], n -> 7/3), 30)) < 10^-25"), "true");
}

#[test]
fn duplication_and_triplication() {
    let mut e = CoreEngine::new();
    for d in [
        "gamma(2*n)/(gamma(n)*gamma(n + 1/2)) - 2^(2*n - 1)/sqrt(pi)",
        "gamma(3*n)/(gamma(n)*gamma(n + 1/3)*gamma(n + 2/3)) - 3^(3*n - 1/2)/(2*pi)",
        "gamma(2*n + 3) - 4^n*(2*n + 2)*(2*n + 1)*gamma(n + 1/2)*gamma(n + 1)/sqrt(pi)",
    ] {
        assert_eq!(ev(&mut e, &format!("full_simplify({d})")), "0", "{d}");
        assert_eq!(ev(&mut e, &format!("abs(n(rewrite({d}, n -> 13/7), 30)) < 10^-25")), "true", "{d}");
    }
    // not zero: the difference stays a number
    assert_eq!(ev(&mut e, "full_simplify(gamma(n + 2) - (n + 1)*n*gamma(n) + 1)"), "1");
    assert_eq!(ev(&mut e, "full_simplify(gamma(2*n)/(gamma(n)*gamma(n + 1/2)) - 2^(2*n)/sqrt(pi)) === 0"), "false");
}
