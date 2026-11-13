//! Round 39: factoring options. trig: true factors a polynomial in sines and cosines through the Laurent polynomial
//! in exp(i w v); extension: e for factor_list, factor_square_free_list, together and cancel. Each result is checked
//! by multiplying out (or by values at a point) against the input.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The value of `a - b` at x = 0.3, y = 1.7 is 0 to 12 digits.
fn same_at_a_point(e: &mut CoreEngine, a: &str, b: &str) -> bool {
    ev(e, &format!("abs(n(rewrite(({a}) - ({b}), [x -> 3/10, y -> 17/10]), 20)) < 10^-12")) == "true"
}

#[test]
fn sums_of_sines_and_cosines_as_products() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "factor(sin(x) + sin(y), trig: true)"), "2*cos((-1/2)*y + (1/2)*x)*sin((1/2)*x + (1/2)*y)");
    assert_eq!(
        ev(&mut e, "factor_list(sin(x) + sin(y), trig: true)"),
        "[[2, 1], [cos((-1/2)*y + (1/2)*x), 1], [sin((1/2)*x + (1/2)*y), 1]]"
    );
    assert_eq!(ev(&mut e, "factor_square_free(1 - cos(2 * x), trig: true)"), "2*sin(x)^2");
    assert_eq!(ev(&mut e, "factor_square_free_list(1 - cos(2 * x), trig: true)"), "[[2, 1], [sin(x), 2]]");
    // variations: cosines, multiple angles, a power; each equal to the input at a point
    for p in ["cos(x) + cos(y)", "sin(2*x) + sin(4*x)", "cos(x) - cos(3*x)", "(sin(x) + sin(y))^2", "sin(x)*cos(y) + cos(x)*sin(y)"] {
        let f = ev(&mut e, &format!("factor({p}, trig: true)"));
        assert!(!f.contains("factor") && f.contains('*'), "{p}: {f}");
        assert!(same_at_a_point(&mut e, &f, p), "{p}: {f}");
    }
    // arguments with a constant term are not read: the call stays
    assert_eq!(ev(&mut e, "factor(sin(x + 1) + sin(x), trig: true)"), "factor(sin(x) + sin(1 + x), trig: true)");
}

#[test]
fn factor_lists_over_an_extension() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "factor_list(1 + x^2, extension: im)"), "[[1, 1], [-1im + x, 1], [1im + x, 1]]");
    assert_eq!(ev(&mut e, "factor_list(2 + 2 * sqrt(2) * x + x^2, extension: automatic)"), "[[1, 1], [x + 2^(1/2), 2]]");
    assert_eq!(ev(&mut e, "factor_square_free_list(x^2 + 2 * sqrt(2) * x + 2, extension: automatic)"), "[[1, 1], [x + 2^(1/2), 2]]");
    // variations: the product of the list is the polynomial
    for (p, ext) in [("x^4 - 2", "sqrt(2)"), ("x^4 + 1", "[sqrt(2), im]"), ("x^3 - 3", "3^(1/3)")] {
        let l = ev(&mut e, &format!("l = factor_list({p}, extension: {ext}); l"));
        assert!(l.starts_with("[["), "{p}: {l}");
        assert_eq!(ev(&mut e, &format!("expand(product(map(q => q[1]^q[2], l)) - ({p}))")), "0", "{p}: {l}");
    }
}

#[test]
fn fractions_in_lowest_terms_over_an_extension() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "together(x^2 / (x - sqrt(2)) - 2 / (x - sqrt(2)), extension: automatic)"), "x + 2^(1/2)");
    assert_eq!(ev(&mut e, "together((x^2 - 2) / (sqrt(2) + x), extension: automatic)"), "x - 2^(1/2)");
    assert_eq!(ev(&mut e, "cancel((x^2 - 2) / (sqrt(2) + x), extension: automatic)"), "x - 2^(1/2)");
    // variations: a sum that keeps a denominator, a cube root, an explicit field; equal to the input at a point
    for (p, ext) in [
        ("1 / (sqrt(2) + x) + 1 / (x^2 - 2)", "automatic"),
        ("(x^3 - 3) / (x - 3^(1/3))", "automatic"),
        ("(x^2 - 3) / (x^2 + 2*sqrt(3)*x + 3)", "sqrt(3)"),
    ] {
        let f = ev(&mut e, &format!("together({p}, extension: {ext})"));
        assert!(!f.contains("together"), "{p}: {f}");
        assert!(same_at_a_point(&mut e, &f, p), "{p}: {f}");
    }
    assert_eq!(ev(&mut e, "together((x^3 - 3) / (x - 3^(1/3)), extension: automatic)"), "x*3^(1/3) + 3^(2/3) + x^2");
}
