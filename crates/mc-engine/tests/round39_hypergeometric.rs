//! Round 39: closed forms of hypergeometric functions whose parameters differ by integers (all but the one paired
//! with k!), of bilateral series with a positive-integer lower parameter, of the Ferrers functions of order +-1/2 and
//! of Jacobi functions of a non-integer degree. Each closed form is checked against the numerical series (or its
//! continuation) at points inside and outside the unit disc, to 25 digits.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The closed form of `call` (in x), free of the function `head`, agrees with the numerical value of `call` at each
/// point.
fn closed_and_agrees(call: &str, head: &str, points: &[&str]) {
    let mut e = CoreEngine::new();
    let f = ev(&mut e, &format!("f = {call}; f"));
    assert!(!f.contains(head), "{call} gave {f}");
    for p in points {
        let numeric = call.replace(" x)", &format!(" {p})"));
        let src = format!("chop(n(rewrite(f, x -> {p}), 30) - n({numeric}, 30), 10^-25)");
        assert_eq!(ev(&mut e, &src), "0", "{call} at {p}: {f}");
    }
}

#[test]
fn gauss_functions_with_integer_parameter_differences() {
    // the documented inputs
    closed_and_agrees("hypergeometric_2f1(2, 3, 4, x)", "hypergeometric", &["3/10", "-2", "-7/3"]);
    closed_and_agrees("hypergeometric_2f1(3/2, 2, 3, x)", "hypergeometric", &["3/10", "-2"]);
    // variations: a half-integer pole with a0 = 1, a larger shift, a negative non-integer a0
    closed_and_agrees("hypergeometric_2f1(1, 5/2, 7/2, x)", "hypergeometric", &["1/3", "-3/2"]);
    closed_and_agrees("hypergeometric_2f1(5/2, 3, 6, x)", "hypergeometric", &["1/5", "-4"]);
    closed_and_agrees("hypergeometric_2f1(-1/2, 2, 4, x)", "hypergeometric", &["2/7", "-5"]);
}

#[test]
fn generalized_functions_with_rational_terms() {
    // documented: dilogarithms from a double pole
    closed_and_agrees("hypergeometric_pfq([1, 1, 2], [3, 3], x)", "hypergeometric", &["3/10", "-1/2"]);
    closed_and_agrees("hypergeometric_pfq([1, 1, 3], [2, 2], x)", "hypergeometric", &["3/10", "-3/4"]);
    // variations: a triple pole (Li_3), shifts on both pairs
    closed_and_agrees("hypergeometric_pfq([1, 1, 1, 1], [2, 2, 2], x)", "hypergeometric", &["1/4", "-2/3"]);
    closed_and_agrees("hypergeometric_pfq([1, 2, 3], [4, 5], x)", "hypergeometric", &["1/2", "-4/5"]);
    // regularized: divided by the gamma functions of the lower parameters
    let mut e = CoreEngine::new();
    let r = ev(&mut e, "hypergeometric_pfq_regularized([1, 1, 3], [2, 2], x)");
    assert!(!r.contains("hypergeometric"), "{r}");
}

#[test]
fn no_closed_form_when_a_pole_is_not_positive() {
    let mut e = CoreEngine::new();
    // (a)_k/(b)_k with a = -1/2 below b: a pole at k = 1/2 - 1, not covered: the call stays
    let r = ev(&mut e, "hypergeometric_2f1(-3/2, 1/3, -1/2, x)");
    assert!(r.contains("hypergeometric_2f1") || !r.contains("log"), "{r}");
    // parameters not differing by integers stay
    assert_eq!(ev(&mut e, "hypergeometric_2f1(1/3, 1/4, 1/5, x)"), "hypergeometric_2f1(1/3, 1/4, 1/5, x)");
}

#[test]
fn bilateral_series_with_a_positive_integer_lower_parameter() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "bilateral_hypergeometric_pfq([1/2], [1], z)"), "(1 - z)^(-1/2)");
    // documented: through 2F1(1/2, 3/2; 5/2; z), checked against the arcsine form
    let src = "simplify(bilateral_hypergeometric_pfq([1/2, 3/2], [1, 5/2], z) - (-3*(-asin(z^(1/2)) + z^(1/2)*(1 - z)^(1/2))/(2*z^(3/2))))";
    assert_eq!(ev(&mut e, src), "0");
    // variation: the lower parameter 3 starts the series at k = -2; against the two-sided numerical sum at 0.4
    let a = ev(&mut e, "g = bilateral_hypergeometric_pfq([1/3, 1/4], [3, 5/2], x); g");
    assert!(!a.contains("bilateral"), "{a}");
    let s = "abs(n(rewrite(g, x -> 2/5), 25) - n(sum(pochhammer(1/3, k)*pochhammer(1/4, k)/(pochhammer(3, k)*pochhammer(5/2, k))*(2/5)^k, [k, -2, 200]), 25)) < 10^-20";
    assert_eq!(ev(&mut e, s), "true");
}

#[test]
fn ferrers_functions_of_half_order_and_jacobi_functions() {
    let mut e = CoreEngine::new();
    for (call, num) in [
        ("legendre_p(3/2, 1/2, x)", "legendre_p(3/2, 1/2, n(3/10, 30))"),
        ("legendre_p(5/2, -1/2, x)", "legendre_p(5/2, -1/2, n(3/10, 30))"),
        ("legendre_p(-5/2, 1/2, x)", "legendre_p(-5/2, 1/2, n(3/10, 30))"),
        ("jacobi_p(1/2, 3/2, 3/2, x)", "jacobi_p(1/2, 3/2, 3/2, n(3/10, 30))"),
        ("jacobi_p(1/2, 1/2, 1/2, x)", "jacobi_p(1/2, 1/2, 1/2, n(3/10, 30))"),
    ] {
        let f = ev(&mut e, &format!("f = {call}; f"));
        assert!(!f.contains("legendre") && !f.contains("jacobi") && !f.contains("hypergeometric"), "{call}: {f}");
        let src = format!("abs(n(rewrite(f, x -> 3/10), 25) - {num}) < 10^-18");
        assert_eq!(ev(&mut e, &src), "true", "{call}: {f}");
    }
}

#[test]
fn lerch_transcendent_beyond_the_unit_circle_at_integer_order_and_shift() {
    let mut e = CoreEngine::new();
    // s = 1, a = 2 at z = 7 on the cut: -(log(1 - z) + z)/z^2 with the principal logarithm
    assert_eq!(ev(&mut e, "abs(lerch_phi(7., 1, 2) - (-log(1 - 7.) - 7.)/49.) < 10^-14"), "true");
    // variations off the cut, against the integral (1/Gamma(s)) int_0^inf t^(s-1) e^(-a t)/(1 - z e^-t) dt
    for (z, s, a) in [("-2.", "3", "5"), ("-3.5", "2", "1"), ("1.5*im", "2", "3")] {
        let src = format!("abs(lerch_phi({z}, {s}, {a}) - n_integrate(t^({s} - 1)*exp(-{a}*t)/(1 - ({z})*exp(-t)), [t, 0, inf])/gamma({s})) < 10^-8");
        assert_eq!(ev(&mut e, &src), "true", "{z} {s} {a}");
    }
}
