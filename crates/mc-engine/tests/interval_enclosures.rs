//! Interval images that enclose. `interval(...)` and `centered_interval(...)` results promise that every
//! value of the function over the argument lies inside. The sweep evaluates functions of the elementary, gamma,
//! Bessel, error-function, zeta and integral families on intervals that hold a pole, a branch point, an extremum or a
//! zero of the derivative, and checks every value at 41 equally spaced points, computed at 30 digits, against the
//! result (a sample outside is a defect; a call that stays unevaluated promises nothing). Complex centered intervals
//! are checked on a grid of the box.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The samples of `f` (a template in `X`) at 30 digits at the midpoints of 40 equal cells of [a, b] (so that no sample
/// falls on an exact pole) that lie outside `ri` (a real value outside every piece); `ri` must already be assigned.
fn outside(e: &mut CoreEngine, f: &str, a: &str, b: &str) -> String {
    let x = format!("({a} + ({b} - ({a})) * (k - 1/2) / 40)");
    let fx = f.replace('X', &x);
    ev(
        e,
        &format!(
            "vs = table(n({fx}, 30), [k, 1, 40]); \
             select(vs, v => if is_number(v) === true and is_real(v) === true then not(is_interval_member(ri, v)) else false)"
        ),
    )
}

/// `f` over interval([a, b]): the result, and the samples outside it (`[]` when every sample is inside or the call
/// stays).
fn check_interval(e: &mut CoreEngine, f: &str, a: &str, b: &str) -> (String, String) {
    let r = ev(e, &format!("ri = {}; ri", f.replace('X', &format!("interval([{a}, {b}])"))));
    if !r.starts_with("interval(") {
        return (r, "[]".into());
    }
    let bad = outside(e, f, a, b);
    (r, bad)
}

/// `f` over centered_interval(c, d): the result, and the samples outside it.
fn check_centered(e: &mut CoreEngine, f: &str, c: &str, d: &str) -> (String, String) {
    let r = ev(e, &format!("rc = {}; rc", f.replace('X', &format!("centered_interval({c}, {d})"))));
    if !r.starts_with("centered_interval(") {
        return (r, "[]".into());
    }
    ev(e, "ri = interval([rc[1] - rc[2], rc[1] + rc[2]]); ri");
    let bad = outside(e, f, &format!("{c} - {d}"), &format!("{c} + {d}"));
    (r, bad)
}

/// The functions of the sweep (templates in X) with the intervals [a, b] that hold a pole, a branch point, an
/// extremum or a zero of the derivative.
const SWEEP: &[(&str, &[(&str, &str)])] = &[
    ("sin(X)", &[("1", "2"), ("4", "5")]),
    ("cos(X)", &[("-1", "1"), ("3", "3.5")]),
    ("tan(X)", &[("1", "2"), ("0", "1.5")]),
    ("cot(X)", &[("-1", "1"), ("0.5", "3")]),
    ("sec(X)", &[("1", "2")]),
    ("csc(X)", &[("-1", "1"), ("1", "2")]),
    ("coth(X)", &[("-1", "1"), ("0", "2")]),
    ("sech(X)", &[("-1", "2")]),
    ("csch(X)", &[("0", "1")]),
    ("cosh(X)", &[("-1", "2")]),
    ("asin(X)", &[("-1", "1")]),
    ("acos(X)", &[("-1", "0.5")]),
    ("acot(X)", &[("0.5", "2")]),
    ("acosh(X)", &[("1", "3")]),
    ("atanh(X)", &[("-1", "0.5")]),
    ("acoth(X)", &[("1", "3")]),
    ("log(X)", &[("0", "2")]),
    ("sqrt(X)", &[("0", "2")]),
    ("sinc(X)", &[("-1", "10"), ("4", "5")]),
    ("gamma(X)", &[("-0.5", "0.5"), ("1", "2"), ("-1.9", "-1.1")]),
    ("log_gamma(X)", &[("0.5", "3")]),
    ("polygamma(X)", &[("-0.5", "0.5"), ("1.2", "2")]),
    ("polygamma(1, X)", &[("0.5", "2")]),
    ("factorial(X)", &[("-1.5", "-0.5"), ("0", "1")]),
    ("beta(2, X)", &[("-0.5", "0.5"), ("1.2", "2.1")]),
    ("bessel_j(2, X)", &[("-3", "3")]),
    ("bessel_j(0, X)", &[("2", "5")]),
    ("bessel_y(1, X)", &[("0", "2"), ("2", "6")]),
    ("bessel_i(1, X)", &[("-2", "2")]),
    ("bessel_k(0, X)", &[("0", "1")]),
    ("airy_ai(X)", &[("-3", "0")]),
    ("airy_bi(X)", &[("-2", "1")]),
    ("erf(X)", &[("-1", "2")]),
    ("erfc(X)", &[("-1", "2")]),
    ("erfi(X)", &[("-1", "1")]),
    ("dawson_f(X)", &[("0", "2")]),
    ("fresnel_s(X)", &[("0", "2")]),
    ("exp_integral_ei(X)", &[("-1", "1")]),
    ("log_integral(X)", &[("0.5", "1.5")]),
    ("sin_integral(X)", &[("-1", "5")]),
    ("cos_integral(X)", &[("0", "2")]),
    ("zeta(X)", &[("0.5", "1.5"), ("1.1", "1.2"), ("-3", "-1")]),
    ("product_log(X)", &[("-0.3", "1")]),
    ("elliptic_k(X)", &[("0", "1")]),
    ("hermite_h(3, X)", &[("-2", "2")]),
];

#[test]
fn real_intervals_enclose_their_images() {
    let mut e = CoreEngine::new();
    let mut failures = vec![];
    for (f, intervals) in SWEEP {
        for (a, b) in *intervals {
            let (r, bad) = check_interval(&mut e, f, a, b);
            if bad != "[]" {
                failures.push(format!("{f} on [{a}, {b}] = {r}: outside {bad}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn reported_images_with_a_pole_or_an_extremum() {
    let mut e = CoreEngine::new();
    // a pole inside: the two rays on its sides    assert_eq!(ev(&mut e, "gamma(interval([-0.5, 0.5]))"), "interval([-inf, -3.5449077018110318], [1.7724538509055159, inf])");
    assert_eq!(ev(&mut e, "tan(interval([1, 2]))"), "interval([-inf, tan(2)], [tan(1), inf])");
    // the value at 0 is the least value: J_2(0) = 0 exactly
    let j = ev(&mut e, "j = bessel_j(2, interval([-3, 3])); j");
    assert!(j.starts_with("interval([0, 0.486091260585891"), "{j}");
    // independent check: J_2 increases on [0, 3] (its first maximum is at 3.054...), so the upper end is J_2(3)
    assert_eq!(ev(&mut e, "v = n(bessel_j(2, 3), 30); [v <= max(j), max(j) - v < 10^-15]"), "[true, true]");
    // variations: a pole inside for zeta and for the digamma function, a pole at an end (csch at 0), the maximum of
    // J_0 on [2, 5] at its end 2 and the minimum inside at the zero 3.8317... of J_1
    assert_eq!(ev(&mut e, "zeta(interval([0.5, 1.5]))"), "interval([-inf, inf])");
    assert_eq!(ev(&mut e, "polygamma(interval([-0.5, 0.5]))"), "interval([-inf, -1.9635100260214233], [0.03648997397857651, inf])");
    assert_eq!(ev(&mut e, "csch(interval([0, log(3)]))"), "interval([3/4, inf])");
    let r = ev(&mut e, "r = bessel_j(0, interval([2, 5])); r");
    assert!(r.starts_with("interval([-0.40275"), "{r}");
    // independent check: the minimum is J_0 at the first zero of J_1, inside the result and within 10^-12 of its end
    assert_eq!(ev(&mut e, "m = n(bessel_j(0, bessel_j_zero(1, 1)), 30); [min(r) <= m, m - min(r) < 10^-12]"), "[true, true]");
    // a step function that is not monotone takes a different value inside than at the ends (unit_box is 1 at 0)
    assert_eq!(ev(&mut e, "unit_box(centered_interval(2, 3))"), "unit_box(centered_interval(2.0, 3.0))");
    assert_eq!(ev(&mut e, "unit_box(interval([-1, 5]))"), "unit_box(interval([-1, 5]))");
    // monotone ones are the hull of the end values; a constant (arg on the positive reals) is a point
    assert_eq!(ev(&mut e, "[floor(interval([-1, 3/2])), floor(centered_interval(1.5, 0.4)), arg(interval([1, 3]))]"), "[interval([-1, 1]), 1, interval([0, 0])]");
    // centered intervals with a pole inside stay unevaluated
    assert_eq!(ev(&mut e, "gamma(centered_interval(0, 0.5))"), "gamma(centered_interval(0.0, 0.5))");
    assert_eq!(ev(&mut e, "tan(centered_interval(1.5, 0.5))"), "tan(centered_interval(1.5, 0.5))");
}

#[test]
fn centered_intervals_enclose_their_images() {
    let mut e = CoreEngine::new();
    let mut failures = vec![];
    for (f, c, d) in [
        ("bessel_j(2, X)", "0", "3"),
        ("sin(X)", "1.5", "0.5"),
        ("gamma(X)", "1.5", "0.1"),
        ("zeta(X)", "2", "1/100"),
        ("log(X) * sin(X)", "20", "10"),
        ("sinc(X)", "1", "1/100"),
        ("erf(X)", "0", "2"),
        ("cos(X)", "0", "0.25"),
    ] {
        let (r, bad) = check_centered(&mut e, f, c, d);
        if bad != "[]" {
            failures.push(format!("{f} on {c} -+ {d} = {r}: outside {bad}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // a machine point: the value with the rounding of the midpoint as the radius (not 0)
    assert_eq!(ev(&mut e, "r = sin(centered_interval(1.23)); r[2] > 0"), "true");
    assert_eq!(ev(&mut e, "abs(n(sin(123/100), 30) - r[1]) <= r[2]"), "true");
}

#[test]
fn complex_centered_intervals_enclose_their_boxes() {
    let mut e = CoreEngine::new();
    let mut failures = vec![];
    let funcs = ["sin", "cos", "tan", "sinh", "cosh", "tanh", "asin", "acos", "atan", "asinh", "acosh", "atanh", "log"];
    for f in funcs {
        for (c, d) in [("2 + 3*im", "(1 + im)/100"), ("0.3 - 0.4*im", "(5 + 2*im)/100")] {
            let r = ev(&mut e, &format!("rc = {f}(centered_interval({c}, {d})); rc"));
            assert!(r.starts_with("centered_interval("), "{f}({c} -+ {d}): {r}");
            // a 7 x 7 grid of the box, at 30 digits: every value inside the result's box
            let bad = ev(
                &mut e,
                &format!(
                    "c0 = {c}; d0 = {d}; \
                     pts = flatten(table(c0 + (2 i/6 - 1) re(d0) + (2 j/6 - 1) im_part(d0) im, [i, 0, 6], [j, 0, 6])); \
                     select(pts, z => {{ let v = n({f}(z), 30); \
                        not(abs(re(v) - re(rc[1])) <= re(rc[2]) and abs(im_part(v) - im_part(rc[1])) <= im_part(rc[2])) }})"
                ),
            );
            if bad != "[]" {
                failures.push(format!("{f}({c} -+ {d}) = {r}: outside at {bad}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // the reported input and two variations: the box of sin is tight to first order (|cos(2 + 3i)| (0.01 + 0.01) per
    // part, about 0.1342)
    let r = ev(&mut e, "rc = sin(centered_interval(2 + 3*im, (1 + im)/100)); [re(rc[2]) < 0.135, im_part(rc[2]) < 0.135]");
    assert_eq!(r, "[true, true]");
    // a box across a branch cut stays unevaluated (asin is cut along the real axis beyond 1)
    let r = ev(&mut e, "asin(centered_interval(2, 0.1 + 0.1*im))");
    assert!(r.starts_with("asin(centered_interval("), "{r}");
}
