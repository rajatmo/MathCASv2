//! Composition and reversion of power series, coefficients of linear recurrences, polynomial tests and identities
//! in a variable. Each result is checked against an independent route: the series of the composed or inverse
//! function, substitution back into the input, or the recurrence run forward.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let stop = StopFlag::new();
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &stop) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn composition_of_series_follows_the_orders() {
    // (x + 2 x^2) at 3x + 4x^2: 3x + 22x^2 + 48x^3 + 32x^4, exact to the order 10 of the inner series
    assert_eq!(ev("compose_series(x + 2*x^2 + O(x^12), 3*x + 4*x^2 + O(x^10))"), "3*x + 22*x^2 + 48*x^3 + 32*x^4 + O(x^10)");
    // against the series of the composed functions
    assert_eq!(ev("compose_series(series(exp(x), [x, 0, 8]), series(sin(x), [x, 0, 8])) - series(exp(sin(x)), [x, 0, 8])"), "O(x^9)");
    assert_eq!(ev("compose_series(series(cos(x), [x, 0, 7]), series(x/(1 - x), [x, 0, 7])) - series(cos(x/(1 - x)), [x, 0, 7])"), "O(x^8)");
    // an inner series of valuation 2 raises the order of the result: f to O(x^4) at g = x^2 + ... gives O(x^8)
    assert_eq!(ev("compose_series(series(1/(1 - x), [x, 0, 3]), series(x^2 + x^3, [x, 0, 9])) - series(1/(1 - x^2 - x^3), [x, 0, 7])"), "O(x^8)");
    // at another point: log at 1 composed with 1 + 3x
    assert_eq!(ev("compose_series(series(log(x), [x, 1, 5]), 1 + 3*x + O(x^5)) - series(log(1 + 3*x), [x, 0, 4])"), "O(x^5)");
    // three series compose from the left
    assert_eq!(ev("compose_series(series(exp(x), [x, 0, 5]), series(sin(x), [x, 0, 5]), series(tan(x), [x, 0, 5])) - series(exp(sin(tan(x))), [x, 0, 5])"), "O(x^6)");
}

#[test]
fn composition_that_does_not_exist_stays_unevaluated() {
    assert_eq!(ev("compose_series(x + 2*x^2 + O(x^10), 1 + 3*x + O(x^5))"), "compose_series(x + 2*x^2 + O(x^10), 1 + 3*x + O(x^5))");
}

#[test]
fn reversion_inverts_the_series() {
    assert_eq!(ev("inverse_series(series(sin(x), [x, 0, 10]))"), "x + x^3/6 + 3*x^5/40 + 5*x^7/112 + 35*x^9/1152 + O(x^11)");
    // against the series of the inverse functions
    assert_eq!(ev("inverse_series(series(tan(x), [x, 0, 11])) - series(atan(x), [x, 0, 11])"), "O(x^12)");
    assert_eq!(ev("inverse_series(series(x*exp(x), [x, 0, 8])) - series(product_log(x), [x, 0, 8])"), "O(x^9)");
    // a constant term: the inverse is a series in y - 1, and composing back gives the identity
    assert_eq!(ev("inverse_series(1 + 2*x + 3*x^2 + O(x^5), y)"), "(y - 1)/2 - 3*(y - 1)^2/8 + 9*(y - 1)^3/16 - 135*(y - 1)^4/128 + O((y - 1)^5)");
    assert_eq!(ev("expand(normal(compose_series(1 + 2*x + 3*x^2 + O(x^5), inverse_series(1 + 2*x + 3*x^2 + O(x^5), x))) - x)"), "0");
}

#[test]
fn reversion_of_a_series_starting_at_a_square_is_a_puiseux_series() {
    assert_eq!(ev("inverse_series(x^2 + 3*x^4 + O(x^8))"), "x^(1/2) + (-3/2)*x^(3/2) + (63/8)*x^(5/2) + O(x^(7/2))");
    // x sin x = y: x = y^(1/2) + y^(3/2)/12 + ..., and (x sin x) at that x is y to the order of the data: the value
    // at a small y agrees with the root of x sin x = y found numerically
    assert_eq!(ev("{ let s = normal(inverse_series(series(x*sin(x), [x, 0, 10]))); let y0 = 1/1000; let x0 = n(rewrite(s, x -> y0), 30); abs(x0*sin(x0) - y0) < 10^-14 }"), "true");
    // a cube: x^3 + x^4 = y, x = y^(1/3) - y^(2/3)/3 + ..., checked at y = 10^-6
    // a Puiseux series inverts back to the power series it came from
    assert_eq!(ev("inverse_series(inverse_series(x^2 + 3*x^4 + O(x^8)))"), "x^2 + 3*x^4 + O(x^8)");
    assert_eq!(ev("inverse_series(inverse_series(series(x*sin(x), [x, 0, 10]))) - series(x*sin(x), [x, 0, 10])"), "O(x^11)");
    assert_eq!(ev("inverse_series(x^(1/3) + x^(2/3) + O(x))"), "x^3 - 3*x^4 + O(x^5)");
    assert_eq!(ev("{ let s = normal(inverse_series(x^3 + x^4 + O(x^9))); let x0 = n(rewrite(s, x -> 10^-6), 30); abs(x0^3 + x0^4 - 10^-6) < 10^-17 }"), "true");
}

#[test]
fn linear_recurrences_are_found_and_checked() {
    assert_eq!(ev("find_linear_recurrence(table(fibonacci(k)*fibonacci(k - 1)^2, [k, 10]))"), "[3, 6, -3, -1]");
    // the recurrence run forward reproduces the list
    assert_eq!(ev("{ let xs = table(k^2*3^k - k, [k, 15]); let c = find_linear_recurrence(xs); linear_recurrence(c, take(xs, length(c)), 15) == xs }"), "true");
    assert_eq!(ev("{ let xs = table(lucas_l(2*k) + (-2)^k, [k, 14]); let c = find_linear_recurrence(xs); [length(c), linear_recurrence(c, take(xs, length(c)), 14) == xs] }"), "[3, true]");
    // entries in symbols: the recurrence x_n = 4 x_(n-1) + 3 x_(n-2) + 2 x_(n-3) + x_(n-4) from four free initial values
    assert_eq!(ev("{ var xs = [a, b, c, w]; for i in range(8) { xs = append(xs, expand(dot([4, 3, 2, 1], reverse(take(xs, -4))))) }; find_linear_recurrence(xs) }"), "[4, 3, 2, 1]");
    assert_eq!(ev("find_linear_recurrence(table(a^k + k*b^k, [k, 10]))"), "[a + 2*b, -b^2 + -2*a*b, a*b^2]");
    // too short to check, and an order bound below the order
    assert_eq!(ev("find_linear_recurrence([1, 2, 4, 9])"), "failure(no_recurrence)");
    assert_eq!(ev("find_linear_recurrence(table(fibonacci(k), [k, 10]), 1)"), "failure(no_recurrence)");
}

#[test]
fn polynomial_tests_respect_the_symbols_of_the_variables() {
    assert_eq!(ev("[polynomial_q(x^(3/2) + 1, sqrt(x)), polynomial_q(x^(3/5) + 1, sqrt(x)), polynomial_q(sec(x) + 1, cos(x))]"), "[true, false, false]");
    assert_eq!(ev("[polynomial_q(cos(x)^2 + a*cos(x), cos(x)), polynomial_q(x^2*y - y/2, [x, y]), polynomial_q(x^2 + exp(x), x)]"), "[true, true, false]");
    assert_eq!(ev("[polynomial_q(x^2 + y), polynomial_q(sqrt(x) + 1)]"), "[true, false]");
}

#[test]
fn solve_always_sets_every_coefficient_to_zero() {
    assert_eq!(ev("solve_always(1 + 2*x + 3*x^2 == a + b*x + c*x^2, x)"), "[[a -> 1, b -> 2, c -> 3]]");
    // substitution back gives an identity in x
    assert_eq!(ev("{ let ss = solve_always((a - 2*b + 1)*x^2 + (a - b^2 - c)*x == a^2 - b + 3*c - 1, x); map(r => simplify(rewrite((a - 2*b + 1)*x^2 + (a - b^2 - c)*x - (a^2 - b + 3*c - 1), r)), ss) }"), "[0, 0]");
    // the coefficients of a cubic from its roots are the elementary symmetric polynomials
    assert_eq!(ev("{ let ss = solve_always(x^3 + a*x^2 + b*x + c == (x - r)*(x - s)*(x - t), x); [length(ss), simplify(rewrite([a, b, c], ss[1]) - [-(r + s + t), r*s + r*t + s*t, -r*s*t])] }"), "[1, [0, 0, 0]]");
}
