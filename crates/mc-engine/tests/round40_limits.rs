//! Round 40: limits of difference quotients of re, im_part, conjugate, sign and arg along the real line (the
//! variable real) and over the complex plane (no limit when two directions give different limits).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn real_variable_parts() {
    assert_eq!(ev("limit((re(x + h) - re(x))/h, h -> 0, direction: reals)"), "1");
    assert_eq!(ev("limit((im_part(x + h) - im_part(x))/h, h -> 0, direction: reals)"), "0");
    assert_eq!(ev("limit((arg(1 + h) - arg(1))/h, h -> 0, direction: reals)"), "0");
    // variations: a multiple of h, an imaginary multiple, a point off the real axis
    assert_eq!(ev("limit((re(x + 3*h) - re(x))/h, h -> 0)"), "3");
    assert_eq!(ev("limit((im_part(z + (2 - 5*im)*h) - im_part(z))/h, h -> 0)"), "-5");
    assert_eq!(ev("limit((conjugate(w + im*h) - conjugate(w))/h, h -> 0) == -im"), "true");
    // d/dt arg(2 + i + t) at 0 = d/dt atan(1/(2 + t)) = -1/5, against a difference quotient at 30 digits
    assert_eq!(ev("limit((arg(2 + im + h) - arg(2 + im))/h, h -> 0)"), "-1/5");
    assert_eq!(ev("abs(n((arg(2 + im + 10^-12) - arg(2 + im))/10^-12, 30) + 1/5) < 10^-10"), "true");
}

#[test]
fn no_limit_over_the_plane() {
    for f in [
        "(re(x + h) - re(x))/h",
        "(im_part(x + h) - im_part(x))/h",
        "(conjugate(x + h) - conjugate(x))/h",
        "(sign(1 + h) - sign(1))/h",
        "(arg(1 + h) - arg(1))/h",
        "conjugate(h)/h",
        "(sign(3 - 2*im + h) - sign(3 - 2*im))/h",
    ] {
        let got = ev(&format!("limit({f}, h -> 0, direction: complexes)"));
        assert!(got.starts_with("failure(no_limit, limit,"), "{f}: {got}");
    }
    // along the real line and the imaginary axis the quotients differ: checked numerically
    assert_eq!(ev("t = 10^-9; [n((re(2 + t) - 2)/t), n((re(2 + im*t) - 2)/(im*t))]"), "[1.0, 0.0]");
    // the same limit along every ray (or none found): not decided here, unevaluated
    assert_eq!(ev("limit((exp(h) - 1)/h, h -> 0, direction: complexes)"), "limit((-1 + exp(h))/h, h -> 0, direction: complexes)");
}

#[test]
fn ranges_of_functions_constant_on_the_reals() {
    // im_part(x) is 0 for real x: its limits at the ends are 0 and a point inside takes the value
    assert_eq!(ev("function_range(im_part(x), x, y)"), "y == 0");
    assert_eq!(ev("function_range(im_part(x) + 3, x, y)"), "y == 3");
    assert_eq!(ev("function_range(re(x), x, y)"), "true");
    assert_eq!(ev("function_range(x^2, x, y)"), "y >= 0");
}
