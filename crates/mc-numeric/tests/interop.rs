//! Values of this crate flowing into other functions (round 12): quantities, `around` and intervals through the
//! number functions, the special and number-theory functions, comparisons and units; exact, ball and machine
//! numbers mixed.
use mc_bench::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::new();
    let stop = StopFlag::new();
    for (src, want) in cases {
        let got = e.eval(src, &stop).unwrap_or_else(|err| panic!("{src}: {err}")).printed;
        assert_eq!(got, *want, "{src}");
    }
}

#[test]
fn quantities_through_functions() {
    check(&[
        (r#"sqrt(quantity(16, "Meters"^2))"#, r#"quantity(4, "Meters")"#),
        (r#"abs(quantity(-3, "Amperes"))"#, r#"quantity(3, "Amperes")"#),
        (r#"round(quantity(3.7, "Meters"))"#, r#"quantity(4, "Meters")"#),
        (r#"round(quantity(377, "Centimeters"), quantity(1, "Meters"))"#, r#"quantity(400, "Centimeters")"#),
        (r#"mod(quantity(8, "Feet"), quantity(3, "Feet"))"#, r#"quantity(2, "Feet")"#),
        (r#"quotient(quantity(2, "Meters"), quantity(30, "Centimeters"))"#, "6"),
        (r#"re(quantity(9.0 + 3.0*im, "Feet"))"#, r#"quantity(9.0, "Feet")"#),
        (r#"sign(quantity(-2, "Meters"))"#, "-1"),
        (r#"quantity([1, 2], "Meters")"#, r#"[quantity(1, "Meters"), quantity(2, "Meters")]"#),
        (r#"quantity(2, "Meters") + quantity(30, "Seconds")"#, "failure(incompatible_units)"),
    ]);
}

#[test]
fn quantity_comparisons() {
    check(&[
        (r#"quantity(3, "Feet") < quantity(1.1, "Meters")"#, "true"),
        (r#"quantity(16, "Ounces") == quantity(1, "Pounds")"#, "true"),
        (r#"quantity(3, "Meters") > quantity(200, "Centimeters")"#, "true"),
        (r#"quantity(3, "Percent") < 1/10"#, "true"),
        (r#"quantity(1, "Meters") < quantity(1, "Seconds")"#, r#"quantity(1, "Meters") < quantity(1, "Seconds")"#),
    ]);
}

#[test]
fn around_through_functions() {
    check(&[
        ("integer_part([1, around(0.2, 0.01)])", "[1, 0]"),
        ("floor(around(-1, 0.01))", "indeterminate"),
        ("unit_step(around(1/2, 0.01))", "1"),
        ("beta(around(1.2, 0.01), 1, 2)", "around(0.48, 0.0019999999999999996)"),
        ("around(1.0, 0.1) < 2", "true"),
        ("around(1.0, 0.1) > around(3.0, 0.5)", "false"),
        ("centered_interval(1, 0.1) < 2", "true"),
        (r#"around(quantity(-34.78, "Newtons"), scaled(0.04))"#, r#"quantity(around(-34.78, 1.3912), "Newtons")"#),
    ]);
}

#[test]
fn numeric_slopes_without_a_closed_derivative() {
    // functions whose derivative has no closed form propagate by a difference quotient
    let mut e = CoreEngine::new();
    let stop = StopFlag::new();
    let got = e.eval("factorial(around(2, 0.01))", &stop).unwrap().printed;
    let inner = got.strip_prefix("around(").and_then(|s| s.strip_suffix(')')).unwrap_or_else(|| panic!("{got}"));
    let (v, d) = inner.split_once(", ").unwrap();
    let (v, d): (f64, f64) = (v.parse().unwrap(), d.parse().unwrap());
    assert!((v - 2.0).abs() < 1e-12, "{got}");
    // d/dx x! at 2 = 2 (3/2 - euler_gamma)
    assert!((d - 0.01 * 2.0 * (1.5 - 0.5772156649015329)).abs() < 1e-10, "{got}");
}

#[test]
fn inexact_factorials_and_logs() {
    check(&[
        ("factorial(2.5)", "3.3233509704478426"),
        ("binomial(2.5, 2)", "1.875"),
        ("log(1/2)", "-log(2)"),
        ("log(1/8)", "-log(8)"),
    ]);
}
