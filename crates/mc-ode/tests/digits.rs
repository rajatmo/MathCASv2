//! `digits:` above machine precision: the Taylor series method on balls, its error estimate (the printed digits
//! are correct) and the functions it supports. The references are the closed forms evaluated by `n` on balls.
mod common;
use common::{correct_digits, significant, Sys};

/// (source, closed form of the solution value, digits asked).
const CASES: &[(&str, &str, usize)] = &[
    // y = tan(t) near its pole
    (
        "n_dsolve_value([y'(t) == 1 + y(t)^2, y(0) == 0], y(3/2), [t, 0, 3/2], digits: 30)",
        "tan(3/2)",
        30,
    ),
    // the harmonic oscillator: sin(1), and cos(1) from the state's derivative
    (
        "n_dsolve_value([y''(t) == -y(t), y(0) == 0, y'(0) == 1], y(1), [t, 0, 1], digits: 50)",
        "sin(1)",
        50,
    ),
    (
        "n_dsolve_value([y''(t) == -y(t), y(0) == 0, y'(0) == 1], y'(1), [t, 0, 1], digits: 50)",
        "cos(1)",
        50,
    ),
    // y' = y: e at 100 digits, e^-1 backwards
    (
        "n_dsolve_value([y'(t) == y(t), y(0) == 1], y(1), [t, 0, 1], digits: 100)",
        "e",
        100,
    ),
    (
        "n_dsolve_value([y'(t) == y(t), y(0) == 1], y(-1), [t, 0, -1], digits: 40)",
        "exp(-1)",
        40,
    ),
    // y' = cos(t) y, y(0) = 1: exp(sin t)
    (
        "n_dsolve_value([y'(t) == cos(t)*y(t), y(0) == 1], y(2), [t, 0, 2], digits: 40)",
        "exp(sin(2))",
        40,
    ),
    // y' = y log(y), y(0) = e: exp(e^t)
    (
        "n_dsolve_value([y'(t) == y(t)*log(y(t)), y(0) == e], y(1/2), [t, 0, 1/2], digits: 30)",
        "exp(exp(1/2))",
        30,
    ),
    // y' = 1/(1 + t^2), y(0) = 0: atan(1) = pi/4
    (
        "n_dsolve_value([y'(t) == 1/(1 + t^2), y(0) == 0], y(1), [t, 0, 1], digits: 40)",
        "pi/4",
        40,
    ),
    // y' = cosh(t), y(0) = 0: sinh(1)
    (
        "n_dsolve_value([y'(t) == cosh(t), y(0) == 0], y(1), [t, 0, 1], digits: 30)",
        "sinh(1)",
        30,
    ),
];

#[test]
fn taylor_results_are_correct_to_the_printed_digits() {
    let mut s = Sys::new();
    for &(src, closed, d) in CASES {
        let want = s.run(&format!("n({closed}, {})", d + 20));
        let got = s.run(src);
        assert!(significant(&got) >= d, "{src}: only {} digits: {got}", significant(&got));
        assert!(correct_digits(&got, &want), "{src}: {got} vs {want}");
    }
}

#[test]
fn exact_solutions_and_powers() {
    let mut s = Sys::new();
    // y' = 1/(2 y), y(0) = 1: sqrt(1 + t), 2 at t = 3; y' = y^(3/2), y(0) = 1: 4/(2 - t)^2, 4 at t = 1
    for (src, want) in [
        ("n_dsolve_value([y'(t) == 1/(2*y(t)), y(0) == 1], y(3), [t, 0, 3], digits: 40)", 2.0),
        ("n_dsolve_value([y'(t) == y(t)^(3/2), y(0) == 1], y(1), [t, 0, 1], digits: 30)", 4.0),
        ("n_dsolve_value([y'(t) == sqrt(y(t)), y(0) == 1], y(2), [t, 0, 2], digits: 30)", 4.0),
    ] {
        let got = s.run(src);
        let digits: String = got.chars().filter(|c| c.is_ascii_digit()).collect();
        assert!(digits.len() >= 30, "{src}: {got}");
        let rest = &digits[1..];
        let ok = (digits.starts_with(&format!("{}", want as i64)) && rest.chars().take(28).all(|c| c == '0'))
            || (digits.starts_with(&format!("{}", want as i64 - 1)) && rest.chars().take(28).all(|c| c == '9'));
        assert!(ok, "{src}: {got}");
    }
}

#[test]
fn taylor_agrees_with_the_machine_methods() {
    let mut s = Sys::new();
    let eq = "[y'(t) == exp(-t)*sqrt(y(t)) + atan(t) - cosh(y(t)/10) + sinh(t/3)/(1 + y(t)^2), y(0) == 1]";
    let hi = s.run(&format!("n_dsolve_value({eq}, y(1), [t, 0, 1], digits: 25)"));
    let lo = common::number(&s.run(&format!("n_dsolve_value({eq}, y(1), [t, 0, 1], accuracy_goal: 13, precision_goal: 13)")));
    assert!((common::number(&hi) - lo).abs() < 1e-12, "{hi} {lo}");
    // the solution evaluates anywhere in its domain, on both sides of the initial point
    s.run("s = n_dsolve([y'(t) == y(t), y(0) == 1], y, [t, -1, 1], digits: 30)");
    for (at, closed) in [("1/3", "exp(1/3)"), ("-1/2", "exp(-1/2)")] {
        let got = s.run(&format!("rewrite(y({at}), s)"));
        let want = s.run(&format!("n({closed}, 50)"));
        assert!(significant(&got) >= 30 && correct_digits(&got, &want), "{got} vs {want}");
    }
    assert_eq!(s.run("rewrite(y(2), s)"), "failure(out_of_range, 2, [-1.0, 1.0])");
    // method: taylor alone works at machine digits
    let m = s.run("n_dsolve_value([y'(t) == y(t), y(0) == 1], y(1), [t, 0, 1], method: taylor)");
    assert!(correct_digits(&m, &s.run("n(e, 40)")), "{m}");
}
