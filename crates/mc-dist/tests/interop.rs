//! Statistics and distributions with values of other crates (round 12): quantities, `around`, intervals and
//! distribution parameters with uncertainty.
use mc_api::{Engine, StopFlag};
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
fn statistics_of_quantities() {
    check(&[
        (r#"mean([quantity(1, "Meters"), quantity(300, "Centimeters")])"#, r#"quantity(2, "Meters")"#),
        (r#"variance([quantity(1, "Meters"), quantity(3, "Meters")])"#, r#"quantity(2, "Meters"^2)"#),
        (r#"std_dev([quantity(1.0, "Meters"), quantity(3.0, "Meters")])"#, r#"quantity(1.4142135623730951, "Meters")"#),
        (r#"median([quantity(1, "Meters"), quantity(3, "Meters"), quantity(2, "Meters")])"#, r#"quantity(2, "Meters")"#),
        (
            r#"quantile([quantity(1, "Meters"), quantity(3, "Meters"), quantity(2, "Meters")], [1/2, 1])"#,
            r#"[quantity(2, "Meters"), quantity(3, "Meters")]"#,
        ),
        (r#"skewness([quantity(1, "Meters"), quantity(2, "Meters"), quantity(3, "Meters")])"#, "0"),
    ]);
}

#[test]
fn statistics_of_uncertain_numbers() {
    check(&[
        ("mean([around(1.0, 0.1), around(2.0, 0.2)])", "around(1.5, 0.1118033988749895)"),
        ("mean([interval([1, 2]), interval([3, 5])])", "interval([2, 7/2])"),
        ("expectation(x, x, poisson_distribution(around(3.0, 0.1)))", "around(3.0, 0.1)"),
    ]);
}

#[test]
fn symbolic_forms() {
    check(&[
        ("pdf(chi_square_distribution(k), x)", "piecewise([[2^((-1/2)*k)*exp((-1/2)*x)*x^(-1 + (1/2)*k)/gamma((1/2)*k), x > 0]], 0)"),
        ("cdf(poisson_distribution(m), k)", "piecewise([[gamma_regularized(1 + floor(k), m), k >= 0]], 0)"),
        ("median(exponential_distribution(l))", "log(2)/l"),
        ("moment(exponential_distribution(l), r)", "factorial(r)*l^-r"),
        ("rewrite(moment(chi_square_distribution(4), r), r -> 2)", "24"),
        ("rewrite(moment(gamma_distribution(3, 2), r), r -> 3)", "480"),
        ("rewrite(moment(uniform_distribution([1, 3]), r), r -> 2)", "13/3"),
    ]);
}
