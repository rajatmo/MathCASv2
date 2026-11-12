//! Round 38: functions at infinite arguments. A point at infinity in no direction gives no limit for the
//! oscillating and exponential functions; a periodic function at a real infinity gives the interval of its limit
//! points; the parts of complex_infinity.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn elementary_functions_at_complex_infinity_have_no_limit() {
    for f in ["sin", "cos", "tan", "sinh", "cosh", "tanh", "exp", "sech", "csch", "coth", "sinc"] {
        assert_eq!(ev(&format!("{f}(complex_infinity)")), "indeterminate", "{f}");
    }
    // independent route: along the real and the imaginary axis exp has different limits (inf and indeterminate)
    assert_eq!(ev("exp(inf)"), "inf");
    assert_eq!(ev("exp(-inf)"), "0");
    assert_eq!(ev("exp(im * inf)"), "indeterminate");
    // log still has the limit inf
    assert_eq!(ev("log(complex_infinity)"), "inf");
}

#[test]
fn parts_of_complex_infinity() {
    assert_eq!(ev("abs(complex_infinity)"), "inf");
    assert_eq!(ev("re(complex_infinity)"), "indeterminate");
    assert_eq!(ev("im_part(complex_infinity)"), "indeterminate");
    assert_eq!(ev("sign(complex_infinity)"), "indeterminate");
    assert_eq!(ev("conjugate(complex_infinity)"), "complex_infinity");
    assert_eq!(ev("arg(complex_infinity)"), "interval([-pi, pi])");
    // the directed infinities keep their values
    assert_eq!(ev("arg(-inf)"), "pi");
    assert_eq!(ev("sign(-inf)"), "-1");
}

#[test]
fn periodic_functions_at_real_infinity() {
    assert_eq!(ev("sin(inf)"), "interval([-1, 1])");
    assert_eq!(ev("cos(-inf)"), "interval([-1, 1])");
    assert_eq!(ev("tan(inf)"), "interval([-inf, inf])");
    assert_eq!(ev("sin(abs(complex_infinity))"), "interval([-1, 1])");
    assert_eq!(ev("fractional_part(inf)"), "interval([0, 1])");
    assert_eq!(ev("fractional_part(-inf)"), "interval([-1, 0])");
    // intervals a period long or unbounded cover the range; a short one does not
    assert_eq!(ev("sin(interval([-inf, inf]))"), "interval([-1, 1])");
    assert_eq!(ev("cos(interval([0, 7]))"), "interval([-1, 1])");
    assert_eq!(ev("tan(interval([0, inf]))"), "interval([-inf, inf])");
    // independent route: on [0, 1] sin is increasing, so the image is [0, sin 1]
    assert_eq!(ev("sin(interval([0, 1]))"), "interval([0, sin(1)])");
}

#[test]
fn conjugate_and_sign_of_a_multiple() {
    // documented: conjugate(-z), sign(-z)
    assert_eq!(ev("conjugate(-z)"), "-conjugate(z)");
    assert_eq!(ev("sign(-z)"), "-sign(z)");
    // variations: a complex coefficient, checked at a number by substitution
    assert_eq!(ev("conjugate(2 * im * x * y)"), "-2im*conjugate(x*y)");
    assert_eq!(ev("rewrite(conjugate(2 * im * z), z -> 3 + 4 * im) == conjugate(2 * im * (3 + 4 * im))"), "true");
    assert_eq!(ev("rewrite(sign((1 + im) * z), z -> 2 - im) == sign((1 + im) * (2 - im))"), "true");
    assert_eq!(ev("sign(-3 * pi)"), "-1");
}
