//! Round 42: Hankel transforms whose Mellin-Barnes integrand is balanced (a jump at the radius of the series),
//! double sums over a triangle by diagonals, finite sums with the pole above the range, and the optimization of step
//! functions on a bounded domain. Each value is compared with a direct evaluation.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn balanced_hankel_transforms() {
    // integrate(sin(r) bessel_j(0, s r), [r, 0, inf]) is 1/sqrt(1 - s^2) below s = 1 and 0 above
    let v = ev("hankel_transform(sin(r)/r, r, s)");
    assert!(v.contains("heaviside_theta"), "{v}");
    assert_eq!(ev("abs(hankel_transform(sin(r)/r, r, 0.23) - 1/sqrt(1 - 0.23^2)) < 10^-12"), "true");
    // variations: another frequency, below and above the jump at s = 2; the inverse transform is the same integral
    assert_eq!(ev("simplify(hankel_transform(sin(2*r)/r, r, 1) - 1/sqrt(3))"), "0");
    assert_eq!(ev("inverse_hankel_transform(sin(2*s)/s, s, 3)"), "0");
    assert_eq!(ev("abs(inverse_hankel_transform(sin(s)/s, s, 1/2) - 2/sqrt(3)) < 10^-12"), "true");
}

#[test]
fn sums_over_a_triangle() {
    let a = "sum((1/(m1^2*m2^3*(m1 + m2)^4))*boole(m1 + m2 <= k), [m1, 1, k], [m2, 1, k])";
    assert_eq!(
        ev(a),
        "3*multiple_harmonic_number(k, [7, 2], [1, 1]) + 6*multiple_harmonic_number(k, [8, 1], [1, 1]) + multiple_harmonic_number(k, [6, 3], [1, 1])"
    );
    // the closed form at k = 6 against the terms of the triangle added one by one
    assert_eq!(
        ev(&format!("abs(n(rewrite({a}, k -> 6)) - n(sum(sum(1/(m1^2*m2^3*(m1 + m2)^4), [m2, 1, 6 - m1]), [m1, 1, 5]))) < 10^-15")),
        "true"
    );
    // variations: other exponents and the strict form of the condition
    let b = "sum((1/(m1*m2*(m1 + m2)))*boole(m1 + m2 <= k), [m1, 1, k], [m2, 1, k])";
    assert_eq!(ev(b), "2*multiple_harmonic_number(k, [2, 1], [1, 1])");
    assert_eq!(ev(&format!("abs(n(rewrite({b}, k -> 7)) - n(sum(sum(1/(m1*m2*(m1 + m2)), [m2, 1, 7 - m1]), [m1, 1, 6]))) < 10^-14")), "true");
    let c = "sum((1/(m1^2*m2*(m1 + m2)))*boole(m1 + m2 < k + 1), [m1, 1, k], [m2, 1, k])";
    assert_eq!(ev(&format!("abs(n(rewrite({c}, k -> 8)) - n(sum(sum(1/(m1^2*m2*(m1 + m2)), [m2, 1, 8 - m1]), [m1, 1, 7]))) < 10^-14")), "true");
}

#[test]
fn sums_with_the_pole_above_the_range() {
    assert_eq!(ev("sum(1/(n - m)^3, [m, 1, n - 1])"), "harmonic_number(-1 + n, 3)");
    assert_eq!(ev("rewrite(sum(1/(m^2*(n - m)^3), [m, 1, n - 1]), n -> 6) - sum(1/(m^2*(6 - m)^3), [m, 1, 5])"), "0");
    assert_eq!(ev("rewrite(sum(1/(n + 2 - m)^2, [m, 1, n]), n -> 5) - sum(1/(7 - m)^2, [m, 1, 5])"), "0");
}

#[test]
fn step_functions_on_a_bounded_domain() {
    // x - floor(x + unit_step(x - 1)) on -4 <= x <= 4 takes -1 at 1, 2, 3, 4 (the rightmost point is given)
    assert_eq!(ev("minimize([x - floor(x + unit_step(x - 1)), ceiling(abs(x)) < 5], x)"), "[-1, [x -> 4]]");
    assert_eq!(ev("max_value([floor(x + unit_step(x - 1)) - x, ceiling(abs(x)) < 5], x)"), "1");
    // no value below -1 on a fine grid
    assert_eq!(ev("min(table(x - floor(x + unit_step(x - 1)), [x, -4, 4, 1/64])) >= -1"), "true");
    // variations: an infimum that is a limit and not attained (floor(x^2) - x tends to -1 as x -> 1 from below)
    assert_eq!(ev("min_value([floor(x^2) - x, 0 <= x and x <= 2], x)"), "-1");
    assert!(ev("minimize([floor(x^2) - x, 0 <= x and x <= 2], x)").starts_with("minimize("));
    assert_eq!(ev("min(table(floor(x^2) - x, [x, 0, 2, 1/128])) > -1"), "true");
    assert_eq!(ev("max_value([x - floor(x), 0 <= x and x <= 2], x)"), "1");
    assert_eq!(ev("minimize([round(2*x) - x, 0 <= x and x <= 3/2], x)"), "[-1/4, [x -> 1/4]]");
}
