//! Inputs of numerics and linear algebra that used to stay unevaluated: inverse reciprocal functions at
//! complex_infinity, centered intervals of exact numbers, optimizers with integer entries and machine numbers,
//! a matrix power applied to a vector, matrices of machine numbers and constants, the principal components of the
//! correlation matrix, and the sum of a rational and a root object that cancels to 862927 digits. Each value is
//! checked by a route that does not share the code that computed it.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn inverse_reciprocal_functions_at_complex_infinity() {
    // f(z) = inverse(1/z) and 1/z -> 0 from every direction: inverse(0) where the inverse is continuous at 0
    assert_eq!(ev("[acoth(complex_infinity), acsch(complex_infinity), acot(complex_infinity), asec(complex_infinity), acsc(complex_infinity)]"), "[0, 0, 0, (1/2)*pi, 0]");
    // the values at large arguments in four directions agree (machine numbers, an independent evaluation)
    for (f, v) in [("acoth", 0.0), ("acsch", 0.0), ("asec", std::f64::consts::FRAC_PI_2)] {
        for z in ["1.0e12", "-1.0e12", "1.0e12*im", "-1.0e12*im", "1.0e12*(1 + im)"] {
            let got = ev(&format!("abs({f}({z}) - {v}) < 10^-11"));
            assert_eq!(got, "true", "{f}({z})");
        }
    }
    // asech: the cut of acosh through 0 gives i pi/2 from above and -i pi/2 from below
    assert_eq!(ev("asech(complex_infinity)"), "indeterminate");
    assert_eq!(ev("abs(asech(1.0e12*im) - asech(-1.0e12*im)) > 3"), "true");
}

#[test]
fn centered_intervals_of_exact_numbers_hold_them() {
    assert_eq!(ev("centered_interval(2, 10^(-10))"), "centered_interval(2.0, 1.0000000000000011e-10)");
    assert_eq!(
        ev("ci = centered_interval(2 + 3*im, 1 + im); [ci[1] == 2.0 + 3.0*im, ci[2] == 1.0 + 1.0*im]"),
        "[true, true]"
    );
    // a centre and a radius that are machine numbers exactly are kept as they are (no rounding to cover)
    assert_eq!(ev("centered_interval(0, 1/2)"), "centered_interval(0.0, 0.5)");
    assert_eq!(ev("interval_union(interval([-2, -1/2], [1, 2]), centered_interval(0, 1/2))"), "interval([-2, 1/2], [1, 2])");
    // with an exact interval the machine bounds of a centered interval are taken as the exact numbers they are: the
    // same set, still holding 1/3
    assert_eq!(
        ev("u = interval_union(interval([-2, -1/2], [1, 2]), centered_interval(0, 1/3)); [is_exact(u[2, 2]), u[2, 2] >= 1/3, u[2, 2] == centered_interval(0, 1/3)[2]]"),
        "[true, true, true]"
    );
    assert_eq!(ev("max(centered_interval(2, 1), centered_interval(1, 4))"), "centered_interval(3.0, 2.0)");
    // the machine interval holds the exact one: compared in exact arithmetic (the machine numbers as the rationals
    // they are)
    for (c, r) in [("2/3", "10^-10"), ("1", "1/10"), ("-7/3", "1/3"), ("123456789/1000", "10^-12")] {
        let t = format!(
            "{{ let ci = centered_interval({c}, {r}); let m = rationalize(ci[1], 0); let w = rationalize(ci[2], 0); \
             [m - w <= {c} - {r}, {c} + {r} <= m + w, w - ({r}) < 10^-12 * abs({c}) + 10^-300] }}"
        );
        assert_eq!(ev(&t), "[true, true, true]", "{c} {r}");
    }
    // a machine number: |x| eps; machine centre and radius stay as given
    assert_eq!(ev("centered_interval(9.87)[2] == 9.87*2^-52"), "true");
    assert_eq!(ev("centered_interval(2.0, 0.5)"), "centered_interval(2.0, 0.5)");
    // a complex ball: the box of its two parts
    assert_eq!(ev("ci = centered_interval(1.23~20 + 4.56~30*im); [re(ci[1]), im_part(ci[1])]"), "[1.23, 4.56]");
}

/// The least of c . x over integer points x in a box (enumeration), with the constraints given as a closure.
fn brute_min(c: (i64, i64), lo: i64, hi: i64, ok: impl Fn(i64, i64) -> bool) -> Option<(i64, i64, i64)> {
    let mut best: Option<(i64, i64, i64)> = None;
    for x in lo..=hi {
        for y in lo..=hi {
            if ok(x, y) {
                let v = c.0 * x + c.1 * y;
                if best.is_none_or(|b| v < b.0) {
                    best = Some((v, x, y));
                }
            }
        }
    }
    best
}

#[test]
fn optimizers_with_integer_entries() {
    // x1 - 0.5 >= 0, x2 >= 0, x1 + 2 x2 == 3 with x1 an integer: x1 = 1, x2 = 1 (x1 = 3, x2 = 0 costs 3)
    assert_eq!(ev("linear_optimization([1, 1], [[[1, 0], [0, 1]], [-0.5, 0]], [[[1, 2]], [-3]], [integers, reals])"), "[1, 1.0]");
    // both entries integers: compared with an enumeration of the integer points
    let got = ev("linear_optimization([-3, -2], [[[-1, -1], [-1, 0], [1, 0], [0, 1]], [9/2, 27/10, 0, 0]], [[], []], integers)");
    let (_, x, y) = brute_min((-3, -2), 0, 10, |x, y| 2 * (x + y) <= 9 && 10 * x <= 27).unwrap();
    assert_eq!(got, format!("[{x}, {y}]"));
    let got = ev("linear_optimization([5, 4], [[[1, 1], [2, -1], [1, 0], [0, 1]], [-7/2, 1/3, 0, 0]], [[], []], integers)");
    let (_, x, y) = brute_min((5, 4), 0, 10, |x, y| 2 * (x + y) >= 7 && 3 * (2 * x - y) + 1 >= 0).unwrap();
    assert_eq!(got, format!("[{x}, {y}]"));
    // a semidefinite program: [[x1, 1], [1, x2]] positive semidefinite (x1, x2 >= 0, x1 x2 >= 1), x1 an integer:
    // 2 x1 + 3/x1 over the positive integers is least at x1 = 1 (5 against 5.5 at 2)
    assert_eq!(ev("semidefinite_optimization([2, 3], [[[0, 1], [1, 0]], [[1, 0], [0, 0]], [[0, 0], [0, 1]]], [integers, reals])"), "[1, 1.0]");
    // a domain that is neither: unevaluated
    assert_eq!(ev("linear_optimization([1], [[[1]], [0]], [[], []], [primes])"), "linear_optimization([1], [[[1]], [0]], [[], []], [primes])");
}

#[test]
fn optimizers_with_machine_numbers_and_geometric_programs() {
    // |x + 2 y| on x + y == 1, x - y <= 2: x + 2 y = 2 - x with x <= 3/2, least 1/2 at x = 3/2
    assert_eq!(ev("convex_optimization(abs(x + 2*y), [x + y == 1., x - y <= 2], [x, y])"), "[x -> 1.5, y -> -0.5]");
    assert_eq!(ev("minimize([abs(x + 2*y), x + y == 1 and x - y <= 2], [x, y])"), "[1/2, [x -> 3/2, y -> -1/2]]");
    // the geometric program: 1/(w h) on 2 w + 2 h <= 1, h <= w/2 with positive variables; on the boundary
    // w + h = 1/2, h = w/2 gives w = 1/3, h = 1/6, where 1/(w h) = 18, and 1/(w (1/2 - w)) is least at w = 1/4 > 1/3
    // only beyond h <= w/2
    assert_eq!(ev("convex_optimization(1/(w*h), [2*w + 2*h <= 1, h <= (1/2)*w], [h, w], method: geometric_optimization)"), "[h -> 1/6, w -> 1/3]");
    assert_eq!(ev("1/(w*h) /. [h -> 1/6, w -> 1/3]"), "18");
}

#[test]
fn matrix_power_applied_to_a_vector() {
    assert_eq!(ev("matrix_power([[1, 1], [1, 0]], 10, [1, 0])"), "[89, 55]");
    assert_eq!(ev("m = [[2, -1, 0], [1, 3, 1], [0, 1, 1]]; v = [1, 2, -1]; [matrix_power(m, 5, v) == dot(m, dot(m, dot(m, dot(m, dot(m, v))))), matrix_power(m, -2, v) == linear_solve(m, linear_solve(m, v)), matrix_power(m, 7, v) == dot(matrix_power(m, 7), v)]"), "[true, true, true]");
    // a band matrix of size 1000: four products with the vector, no power of the matrix
    assert_eq!(ev("m = sparse_array(band([1, 2]) -> 2, [1000, 1000]); v = sparse_array([[1] -> 1, [-1] -> -1.5], [1000]); w = matrix_power(m, 4, v); [w[996], total(abs(w))]"), "[-24.0, 24.0]");
}

#[test]
fn matrices_of_machine_numbers_and_constants_are_numbers() {
    // the determinant by the rule of Sarrus in machine numbers, an independent formula
    let d = ev("det([[1. + im, 2, 3 - 2*im], [0, 4*pi, 5*im], [3, 0, 6]])");
    assert_eq!(ev(&format!("{{ let a = 1. + im; let p = 4*pi; abs(({d}) - (a*p*6 + 2*5*im*3 + 0 - (3 - 2*im)*p*3 - 0 - 0)) < 10^-12 }}")), "true");
    assert!(!d.contains("pi"), "{d}");
    // the inverse times the matrix is the identity
    assert_eq!(ev("m = [[1. + im, 2, 3 - 2*im], [0, 4*pi, 5*im], [e, 0, 6]]; max(abs(flatten(dot(inverse(m), m) - identity_matrix(3)))) < 10^-14"), "true");
}

#[test]
fn principal_components_of_the_correlation_matrix() {
    // standardized columns: the variances of the components are the eigenvalues of the correlation matrix, which
    // add up to the number of columns, and the components are uncorrelated
    let pc = "pc = principal_components([[1., 2., -1.], [2., 3., 2.], [4., 10., 9.], [4., 5., 6.]], method: correlation)";
    assert_eq!(ev(&format!("{pc}; abs(total(variance(pc)) - 3) < 10^-12")), "true");
    assert_eq!(ev(&format!("{pc}; max(abs(flatten(chop(covariance(pc)) - diagonal_matrix(variance(pc))))) < 10^-12")), "true");
    // the first column (up to its sign) against the value of the leading eigenvector of the
    // correlation matrix
    assert_eq!(ev(&format!("{pc}; abs(abs(pc[1, 1]) - 1.8196165781767486) < 10^-12")), "true");
}

#[test]
fn center_array_asked_as_sparse() {
    assert_eq!(ev("center_array([1, 2, 3], [5, 5], 0, sparse_array) == center_array([1, 2, 3], [5, 5], 0)"), "true");
}

#[test]
fn rational_plus_root_object_without_cancellation() {
    // r is the real root of x^1234567 + 5 x + 1 near -1/5, so r + 1/5 = -r^1234567/5 = (1/5)^1234568 (1 + O(10^-862920)):
    // the digits of 10^(-1234568 log10 5) at 60 digits (mpmath) are 9.99279297265720537349449270000309977261542943...
    // times 10^-862927
    let v = ev("r = root([&(1 + 5*&1 + &1^1234567), -0.2]); n(r + 1/5, 40)");
    assert!(v.starts_with("9.99279297265720537349449270000309977261"), "{v}");
    assert!(v.ends_with("e-862927"), "{v}");
    // a sum that does not cancel keeps its value
    assert_eq!(ev("r = root([&(1 + 5*&1 + &1^1234567), -0.2]); abs(n(r + 3, 20) - (3 + n(r, 20))) < 10^-18"), "true");
}
