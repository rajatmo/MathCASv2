//! `reduce` of conditions with `max`/`min` of functions and with several functions of the unknown in different
//! atoms; integer roots of lacunary polynomials of huge degree, cut at the gaps of their exponents.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn max_and_min_of_sine_and_cosine() {
    let mut e = CoreEngine::new();
    assert_eq!(
        ev(&mut e, "reduce(max(sin(x), cos(x)) > 0 and 0 < x and x < 20, x)"),
        "0 < x and x < pi or (3/2)*pi < x and x < 3*pi or (7/2)*pi < x and x < 5*pi or (11/2)*pi < x and x < 20"
    );
    assert_eq!(
        ev(&mut e, "reduce(min(sin(x), cos(x)) > 0 and 0 < x and x < 20, x)"),
        "0 < x and x < (1/2)*pi or 2*pi < x and x < (5/2)*pi or 4*pi < x and x < (9/2)*pi or 6*pi < x and x < 20"
    );
    assert_eq!(ev(&mut e, "reduce(max(x^2, exp(x)) < 4, x)"), "-2 < x and x < log(4)");
}

#[test]
fn several_functions_in_different_atoms() {
    let mut e = CoreEngine::new();
    assert_eq!(
        ev(&mut e, "reduce((sin(x) > 0 or cos(x) > 0) and 0 < x and x < 20, x)"),
        "0 < x and x < pi or (3/2)*pi < x and x < 3*pi or (7/2)*pi < x and x < 5*pi or (11/2)*pi < x and x < 20"
    );
    assert_eq!(ev(&mut e, "reduce(exp(x) > 2 and sin(x) < 0 and -10 < x and x < 10, x)"), "pi < x and x < 2*pi or 3*pi < x and x < 10");
}

#[test]
fn totient_equations_over_the_integers() {
    let mut e = CoreEngine::new();
    // phi(m) >= sqrt(m/2) bounds the search
    assert_eq!(ev(&mut e, "solve(euler_phi(m) == 10, m, integers)"), "[[m -> -22], [m -> -11], [m -> 11], [m -> 22]]");
    assert_eq!(ev(&mut e, "solve(euler_phi(m) == 7, m, integers)"), "[]");
}

#[test]
fn integer_roots_of_lacunary_polynomials() {
    let mut e = CoreEngine::new();
    // (x^3 - 7 x^2 - 9 x + 63) (24 x - 25 x^12345 + x^999997)
    assert_eq!(
        ev(
            &mut e,
            "reduce(1512*x - 216*x^2 - 168*x^3 + 24*x^4 - 1575*x^12345 + 225*x^12346 + 175*x^12347 - 25*x^12348 + 63*x^999997 - 9*x^999998 - 7*x^999999 + x^1000000 == 0, x, integers)"
        ),
        "x == -3 or x == -1 or x == 0 or x == 1 or x == 3 or x == 7"
    );
    assert_eq!(ev(&mut e, "reduce(x^1000000 - 3*x^999999 + x - 3 == 0, x, integers)"), "x == -1 or x == 3");
    assert_eq!(ev(&mut e, "reduce(x^1000000 - 3*x^999999 + 2*x - 6 == 0 and x > 0, x, integers)"), "x == 3");
}
