//! Round 38: the options trig: true and modulus: m of the fraction and expansion functions. Each test has an
//! example input, variations, and a check by another route: numeric values at points (the trigonometric forms) or
//! the cross product reduced mod m (the fractions over Z/m).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The two inputs evaluate to the same term (printing is not compared).
macro_rules! assert_same {
    ($a:expr, $b:expr) => {
        assert_eq!(ev(&format!("({}) === ({})", $a, $b)), "true", "{} gives {}", $a, ev($a))
    };
}

/// |a - b| at a few points of x and y (both expressions in x, y).
fn agree(a: &str, b: &str) -> bool {
    [(0.3, 1.1), (-0.7, 0.4), (1.9, -2.3)].iter().all(|(x, y)| {
        let v = ev(&format!("n(abs(rewrite(({a}) - ({b}), [x -> {x}, y -> {y}])))"));
        v.parse::<f64>().is_ok_and(|d| d < 1e-9)
    })
}

#[test]
fn trig_fractions_reduce_by_the_pythagorean_identity() {
    assert_same!("together(sin(x)^2 / cos(x) + cos(x), trig: true)", "cos(x)^-1");
    assert_same!("cancel(sin(2 * x) / sin(x), trig: true)", "2*cos(x)");
    assert_same!("together(1 / sin(x) + 1 / cos(x), trig: true)", "(cos(x) + sin(x))/(cos(x)*sin(x))");
    // variations: the hyperbolic identity, a higher power, a quotient of sums of angles
    assert_same!("together(tanh(x) * cosh(x)^3, trig: true)", "sinh(x)^3 + sinh(x)");
    let r = ev("together(cos(x)^4 / sin(x) + sin(x)^3 + cos(y)^2, trig: true)");
    assert!(agree(&r, "cos(x)^4 / sin(x) + sin(x)^3 + cos(y)^2"), "{r}");
    let r = ev("together(sin(x + y) / cos(x), trig: true)");
    assert!(agree(&r, "sin(x + y) / cos(x)") && !r.contains("x + y"), "{r}");
}

#[test]
fn trig_expansions_of_numerators_denominators_and_all() {
    assert_same!("expand(sin(x + y), trig: true)", "cos(x)*sin(y) + cos(y)*sin(x)");
    assert_same!("expand_numerator(sin(2 * x) / (x - 1)^2, trig: true)", "2*cos(x)*sin(x)/(-1 + x)^2");
    assert_same!("expand_denominator((a - b) / (sin(2 * x) + 1), trig: true)", "(a - b)/(1 + 2*cos(x)*sin(x))");
    let cases = [
        "(sin(2 * x) + x)^2",
        "(a - b)^2 / (sin(2 * x) + 1)",
        "(a - b) / (sin(x + y) + 1)",
        "cos(3 * x) / (1 + sin(x - y))",
    ];
    for c in cases {
        for f in ["expand", "expand_all", "expand_numerator", "expand_denominator"] {
            let r = ev(&format!("rewrite({f}({c}, trig: true), [a -> 2, b -> 1/3])"));
            assert!(agree(&r, &format!("rewrite({c}, [a -> 2, b -> 1/3])")), "{f} {c}: {r}");
        }
    }
    // the multiple angles are gone from the expanded parts
    assert!(!ev("expand_all((a - b) / (sin(x + y) + 1), trig: true)").contains("x + y"));
}

#[test]
fn trig_numerators_and_denominators() {
    assert_same!("numerator(tan(x), trig: true)", "sin(x)");
    assert_same!("numerator_denominator(tan(x), trig: true)", "[sin(x), cos(x)]");
    assert_eq!(
        ev("numerator_denominator([sin(x), cos(x), tan(x), csc(x), sec(x), cot(x)], trig: true)"),
        "[[sin(x), 1], [cos(x), 1], [sin(x), cos(x)], [1, sin(x)], [1, cos(x)], [cos(x), sin(x)]]"
    );
    // variations: hyperbolic, a product; the quotient is the function
    assert_same!("denominator(coth(2 * x), trig: true)", "sinh(2*x)");
    assert_same!("numerator_denominator(tan(x) * sec(y), trig: true)", "[sin(x), cos(x)*cos(y)]");
    assert!(agree("numerator(tan(x) * csc(y), trig: true) / denominator(tan(x) * csc(y), trig: true)", "tan(x) * csc(y)"));
}

#[test]
fn fractions_over_the_integers_mod_m() {
    assert_same!("together(5 * (x / (3 * x^2 - y^2)) + 3 * (y / (3 * x^2 - y^2)), modulus: 3)", "x/y^2");
    assert_same!("together((x - 1) * ((x - 2) / ((x - 3) * (x - 4) * (x - 5))), modulus: 2)", "(1 + x)^-1");
    // variations: another prime, a sum of fractions; checked by the cross product mod p
    for (e, p) in [("1 / x + 1 / (1 + x)", 2), ("x / (x + 1) + 1", 3), ("(x^2 - 1) / (x^3 + 2 * x + 3) + 1 / (x - 2)", 5), ("(x^2 + 3 * x + 2) / (x^2 + 4)", 5)] {
        let r = ev(&format!("together({e}, modulus: {p})"));
        let chk = ev(&format!("{{ var q = together({e}); polynomial_mod(expand(numerator(q) * denominator({r}) - numerator({r}) * denominator(q)), {p}) }}"));
        assert_eq!(chk, "0", "{e} mod {p}: {r}");
    }
    // the common factor over F_5: x^2 + 3x + 2 = (x + 1)(x + 2) and x^2 + 4 = (x + 1)(x + 4) mod 5
    assert_same!("together((x^2 + 3 * x + 2) / (x^2 + 4), modulus: 5)", "(2 + x)/(4 + x)");
}
