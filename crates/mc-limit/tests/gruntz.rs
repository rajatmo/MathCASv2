//! Item 13C: Gruntz's algorithm behind `limit` (exp-log functions, special functions at infinity through their
//! asymptotic expansions, parameters, iterated and multivariate limits), `max_limit`, `min_limit`,
//! `discrete_limit`, `asymptotic`, and the `series` extensions (poles of gamma, branch points of asin and acos,
//! negative orders, lists, machine numbers, the leading-term form `series(f, x -> a)`).
mod common;
use common::{check, Sys};
use std::time::{Duration, Instant};

/// Every value was checked by hand or against the asymptotic expansion; the numeric plausibility check runs on
/// each finite one (decision 13C.4).
const CASES: &[(&str, &str)] = &[
    ("limit(x^n*exp(-x^(-2)), x -> 0)", "0"),
    ("limit((25^x - 5^x)/(4^x - 2^x), x -> 0)", "log(5)/log(2)"),
    ("limit(exp(x)*(exp(1/x - exp(-x)) - exp(1/x)), x -> inf)", "-1"),
    ("limit(exp(x)*(exp(1/x + exp(-x) + exp(-x^2)) - exp(1/x - exp(-exp(x)))), x -> inf)", "1"),
    ("limit(exp(exp(exp(x)))/exp(exp(exp(x - exp(-exp(x))))), x -> inf)", "inf"),
    ("limit(log(x)^2*exp(sqrt(log(x))*log(log(x))^2*exp(sqrt(log(log(x)))*log(log(log(x)))^3))/sqrt(x), x -> inf)", "0"),
    ("limit(2^x/3^x, x -> inf)", "0"),
    ("limit((3^x + 5^x)^(1/x), x -> inf)", "5"),
    ("limit(x*(gamma(x + 1/2)/gamma(x))^2 - x^2, x -> inf)", "-inf"),
    ("limit((x*(gamma(x + 1/2)/gamma(x))^2 - x^2 + x/4)*x, x -> inf)", "inf"),
    ("limit(erf(x)^x, x -> inf)", "1"),
    ("limit(x*exp(x^2)*erfc(x), x -> inf)", "pi^(-1/2)"),
    ("limit(log(gamma(x))/(x*log(x)), x -> inf)", "1"),
    ("limit(factorial(n)/n^n*exp(n)/sqrt(n), n -> inf)", "2^(1/2)*pi^(1/2)"),
    ("limit(exp_integral_ei(x)*x*exp(-x), x -> inf)", "1"),
    ("limit(bessel_i(0, x)*exp(-x)*sqrt(x), x -> inf)", "(1/2)*2^(1/2)*pi^(-1/2)"),
    ("limit(x^x - x, x -> 0, direction: from_above)", "1"),
    ("limit((x^(1/x) - 1)*x/log(x), x -> inf)", "1"),
    ("limit(product_log(x)/log(x), x -> inf)", "1"),
    ("limit(x*(atan(x) - pi/2), x -> inf)", "-1"),
    ("limit(tanh(x)^(exp(2*x)), x -> inf)", "exp(-2)"),
    ("limit(airy_ai(x)*x^(1/4)*exp(2/3*x^(3/2)), x -> inf)", "(1/2)*pi^(-1/2)"),
    ("limit((zeta(x) - 1)*2^x, x -> inf)", "1"),
    ("limit((x + 1)^(1/3) - x^(1/3), x -> inf)", "0"),
    ("limit(gamma(x)/x, x -> 0)", "inf"),
    ("limit(gamma(2*n + 1)/gamma(n + 1)^3, n -> inf)", "0"),
    ("limit(real_abs(x)/x, x -> 0, direction: from_above)", "1"),
    ("limit(real_abs(x)*tan(y)^2, [y -> pi/2, x -> 0])", "0"),
    ("limit(real_abs(x)*tan(y)^2, [x -> 0, y -> pi/2])", "inf"),
    ("limit(tan(x*y)/(x*y), [x, y] -> [0, 0])", "1"),
    ("max_limit(sin(x) - 1/x, x -> inf)", "1"),
    ("min_limit(sin(x) - 1/x, x -> inf)", "-1"),
    ("max_limit(1/x, x -> 0)", "inf"),
    ("min_limit(1/x, x -> 0)", "-inf"),
    ("max_limit(x/(x^3 + x^2), x -> 0, direction: from_below)", "-inf"),
    ("max_limit(y/(x + y), [y -> 0, x -> 0])", "1"),
    ("max_limit(y/(x + y), [x -> 0, y -> 0])", "0"),
    ("max_limit(3*cos(2*x) + 1, x -> inf)", "4"),
    ("min_limit(3*sin(x) + 4*cos(x), x -> inf)", "-5"),
    ("max_limit(a*x^2 + b*x + c, x -> 2)", "c + 2*b + 4*a"),
    ("max_limit(x*sin(x), x -> inf)", "inf"),
    ("discrete_limit(n/(n + 1), n -> inf)", "1"),
    ("discrete_limit((-5)^n/factorial(n), n -> inf)", "0"),
    ("discrete_limit(fibonacci(n)/lucas_l(n), n -> inf)", "(1/5)*5^(1/2)"),
    ("discrete_limit(fibonacci(n)/2^n, n -> inf)", "0"),
    ("discrete_limit(factorial(n)/3^n/exp(n), n -> inf)", "inf"),
    ("discrete_limit((-1)^n, n -> inf)", "failure(no_limit, discrete_limit, (-1)^n)"),
    ("discrete_limit(factorial_power(n, 2)/(5*n^2 + 1), n -> inf)", "1/5"),
    ("discrete_limit([n/(n + 2), exp(-n/(n + 1))], n -> inf)", "[1, exp(-1)]"),
    ("discrete_limit(n/(n + 2)/exp(m/(m + 1)), [m -> inf, n -> inf])", "exp(-1)"),
    ("asymptotic(sin(x), x -> 0)", "x"),
    ("asymptotic(x^4 + 5*x^3 + 3*x - 11, x -> inf)", "x^4"),
    ("asymptotic((3*x^3 + 5)/(x^2 - 1), x -> 1)", "4/(-1 + x)"),
    ("asymptotic(2^x/(1 + 4^x), x -> inf)", "2^-x"),
    ("asymptotic((x^2 + 3*x)*2^x, x -> inf)", "2^x*x^2"),
    ("asymptotic(gamma(x), x -> inf)", "2^(1/2)*exp(-x)*pi^(1/2)*x^(-1/2 + x)"),
    ("asymptotic(airy_ai(x), x -> inf)", "(1/2)*exp((-2/3)*x^(3/2))*pi^(-1/2)*x^(-1/4)"),
    ("asymptotic(gamma(sin(x) - x + x^3/6)^10, x -> 0)", "619173642240000000000*x^-50"),
    ("asymptotic(bessel_j(2, x), x -> 0)", "(1/8)*x^2"),
    ("asymptotic(5*x^2/(3*x^3 + 1)*log(x), x -> inf)", "(5/3)*log(x)/x"),
    ("series(gamma(sin(x) - x)^3, x -> 0)", "-216*x^-9 + O(x^-7)"),
    ("series(1/sin(x)^10, x, 0, -5)", "x^-10 + 5/(3*x^8) + 13/(9*x^6) + O(x^-4)"),
    ("series(exp(x)/x^10, x, 0, -5)", "x^-10 + x^-9 + 1/(2*x^8) + 1/(6*x^7) + 1/(24*x^6) + 1/(120*x^5) + O(x^-4)"),
    ("series(gamma(x), x, 0, 1)", "x^-1 - euler_gamma + ((1/12)*pi^2 + (1/2)*euler_gamma^2)*x + O(x^2)"),
    ("series(asin(x), x, 1, 1)", "(1/2)*pi + -1*2^(1/2)*(1 - x)^(1/2) + O((1 - x)^(3/2))"),
    ("series(acos(x), x, 1, 2)", "2^(1/2)*(1 - x)^(1/2) + (1/12)*2^(1/2)*(1 - x)^(3/2) + O((1 - x)^(5/2))"),
    ("series(sqrt(-x), x, 0, 1)", "(-x)^(1/2) + O((-x)^(3/2))"),
    ("series(sqrt(1 - x), x, 1, 2)", "(1 - x)^(1/2) + O((1 - x)^(5/2))"),
    ("series([sin(x), cos(x)], x, 0, 3)", "[x - x^3/6 + O(x^4), 1 - x^2/2 + O(x^4)]"),
    ("series(sin(2.5*x), x, 0, 5)", "2.5*x + -2.6041666666666665*x^3 + 0.8138020833333334*x^5 + O(x^6)"),
    ("series(asin(x), x, 1, 1, assumptions: x < 1)", "(1/2)*pi + 1im*2^(1/2)*(-1 + x)^(1/2) + O((-1 + x)^(3/2))"),
    ("series(acos(x), x, 1, 1, assumptions: x > 1)", "1im*2^(1/2)*(-1 + x)^(1/2) + O((-1 + x)^(3/2))"),
];

#[test]
fn gruntz_and_relatives() {
    check(CASES);
}

/// Undecided limits stay unevaluated and give up within their budget.
#[test]
fn undecided_within_budget() {
    let mut s = Sys::new();
    // decided since round 37: max_limit inf above min_limit -inf, so there is no limit
    let out = s.run("limit(sin(x)*exp(x), x -> inf)");
    assert!(out.starts_with("failure(no_limit"), "{out}");
    for src in [
        "limit(exp(a*x)/exp(x), x -> inf)",
        "max_limit(sin(x)*sin(x^2), x -> inf)",
        "discrete_limit(sin(n), n -> inf)",
    ] {
        let start = Instant::now();
        let out = s.run(src);
        assert!(out.contains("limit("), "{src}: {out}");
        assert!(start.elapsed() < Duration::from_secs(20), "{src}: {:?}", start.elapsed());
    }
}
