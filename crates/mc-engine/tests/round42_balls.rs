//! Round 42: proved images on wide intervals and centered intervals. Functions whose ball form was loose or missing
//! on a wide ball (no derivative in closed form, a constant left symbolic next to a ball, a series argument across
//! 1/2, a Hurwitz zeta of a ball) give enclosures again; a pole inside an interval gives the union of the two rays;
//! a ball reaching the singular point m = 1 of elliptic_k has no finite value. Every result is checked against the
//! values at 40 points of the interval computed at 30 digits (a sample outside is a defect).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The samples of `f` (a template in `X`) at 30 digits at the midpoints of 40 equal cells of [a, b] that lie outside
/// the interval `ri` (already assigned).
fn outside(e: &mut CoreEngine, f: &str, a: &str, b: &str) -> String {
    let x = format!("({a} + ({b} - ({a})) * (k - 1/2) / 40)");
    let fx = f.replace('X', &x);
    ev(
        e,
        &format!(
            "vs = table(n({fx}, 30), [k, 1, 40]); rn = n(ri, 40); \
             select(vs, v => if is_number(v) === true and is_real(v) === true then not(interval_member_q(rn, v)) else false)"
        ),
    )
}

/// `f` over interval([a, b]): the result (it must be an interval) and the samples outside it.
fn interval_image(e: &mut CoreEngine, f: &str, a: &str, b: &str) -> (String, String) {
    let r = ev(e, &format!("ri = {}; ri", f.replace('X', &format!("interval([{a}, {b}])"))));
    assert!(r.starts_with("interval("), "{f} on [{a}, {b}]: {r}");
    let bad = outside(e, f, a, b);
    (r, bad)
}

/// `f` over centered_interval(c, d): the result (it must be a centered interval) and the samples outside it.
fn centered_image(e: &mut CoreEngine, f: &str, c: &str, d: &str) -> (String, String) {
    let r = ev(e, &format!("rc = {}; rc", f.replace('X', &format!("centered_interval({c}, {d})"))));
    assert!(r.starts_with("centered_interval("), "{f} on {c} -+ {d}: {r}");
    ev(e, "ri = interval([rc[1] - rc[2], rc[1] + rc[2]]); ri");
    let bad = outside(e, f, &format!("{c} - {d}"), &format!("{c} + {d}"));
    (r, bad)
}

#[test]
fn wide_intervals_of_special_functions_are_enclosed() {
    let mut e = CoreEngine::new();
    let mut failures = vec![];
    // the reported inputs, then variations: a wider interval, another parameter
    for (f, a, b) in [
        ("binomial(1/2, X)", "0.5", "0.6"),
        ("binomial(1/2, X)", "-0.4", "1.3"),
        ("catalan_number(X)", "0.5", "0.6"),
        ("catalan_number(X)", "1.5", "3"),
        ("factorial2(X)", "2.1", "2.2"),
        ("factorial2(X)", "0.5", "3.5"),
        ("harmonic_number(0.2, X)", "2.1", "2.2"),
        ("harmonic_number(3/2, X)", "1.5", "3"),
        ("harmonic_number(X, 2)", "0.5", "0.6"),
        ("riemann_siegel_z(X)", "1.234", "1.235"),
        ("riemann_siegel_z(X)", "14", "14.2"),
        ("whittaker_w(0.3, 0.4, X)", "0.5", "0.6"),
        ("whittaker_w(1, 2, X)", "1", "4"),
        ("whittaker_m(1/3, 1/2, X)", "0.5", "2"),
        ("log_barnes_g(X)", "0.111", "0.112"),
        ("log_barnes_g(X)", "0.5", "4"),
        ("fresnel_f(X)", "1.23", "1.24"),
        ("fresnel_g(X)", "0.5", "3"),
        ("fresnel_s(X)", "2.9", "3.1"),
    ] {
        let (r, bad) = interval_image(&mut e, f, a, b);
        if bad != "[]" {
            failures.push(format!("{f} on [{a}, {b}] = {r}: outside {bad}"));
        }
    }
    for (f, c, d) in [
        ("binomial(1, X)", "1/2", "1/100"),
        ("catalan_number(X)", "1", "1/100"),
        ("elliptic_theta(2, 1, X)", "1/4", "1/100"),
        ("elliptic_theta(3, 1, X)", "1/3", "1/10"),
        ("factorial2(X)", "3", "1/100"),
        ("harmonic_number(1/2, X)", "3/4", "1/1000"),
        ("harmonic_number(1/2, X)", "2", "1/2"),
        ("riemann_siegel_z(X)", "3/2", "1/1000"),
        ("whittaker_w(1, 2, X)", "3", "1/100"),
        ("fresnel_f(X)", "3", "1/100"),
        ("fresnel_f(X)", "1/2", "1/10"),
        ("log_barnes_g(X)", "1/2", "1/1000"),
        ("hypergeometric_2f1(1/2, 1/3, 1/4, X)", "1/2", "1/100"),
        ("hypergeometric_2f1(1/2, 1/3, 1/4, X)", "1/2", "1/5"),
        ("hypergeometric_2f1_regularized(1/2, 1/3, 1/4, X)", "1/2", "1/100"),
        ("hypergeometric_2f1(2, 3/2, 5/2, X)", "0.55", "0.1"),
    ] {
        let (r, bad) = centered_image(&mut e, f, c, d);
        if bad != "[]" {
            failures.push(format!("{f} on {c} -+ {d} = {r}: outside {bad}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // the images are tight where the derivative settles the cells: catalan_number increases on [0.5, 0.6], so the
    // image is [C(0.5), C(0.6)] (independently: the values at the ends from the gamma form, at 30 digits)
    let r = ev(&mut e, "r = catalan_number(interval([0.5, 0.6])); g = x => 4^x gamma(x + 1/2)/(sqrt(pi) gamma(x + 2)); \
        [abs(min(r) - n(g(1/2), 30)) < 10^-14, abs(max(r) - n(g(3/5), 30)) < 10^-14]");
    assert_eq!(r, "[true, true]");
}

#[test]
fn derivatives_of_the_counting_and_whittaker_functions() {
    let mut e = CoreEngine::new();
    // each derivative against a central difference at 30 digits
    for (f, x) in [
        ("catalan_number(x)", "7/10"),
        ("factorial2(x)", "13/10"),
        ("binomial(5/2, x)", "7/10"),
        ("binomial(x, 3/2)", "27/10"),
        ("log_barnes_g(x)", "13/10"),
        ("harmonic_number(x, 3/2)", "7/10"),
        ("whittaker_w(1/3, 1/2, x)", "13/10"),
        ("whittaker_m(1/3, 1/2, x)", "13/10"),
    ] {
        let r = ev(
            &mut e,
            &format!(
                "dd = d({f}, x); h = 10^-12; v = n(rewrite(dd, x -> {x}), 30); \
                 c = n((rewrite({f}, x -> {x} + h) - rewrite({f}, x -> {x} - h))/(2 h), 40); abs(v - c) < 10^-18"
            ),
        );
        assert_eq!(r, "true", "{f} at {x}");
    }
}

#[test]
fn a_pole_inside_gives_two_rays() {
    let mut e = CoreEngine::new();
    // acsch decreases on [-4, 0) to -inf and on (0, csch 2] from inf to 2
    assert_eq!(ev(&mut e, "acsch(interval([-4, csch(2)]))"), "interval([-inf, -acsch(4)], [2, inf])");
    // variations: tan across pi/2 (left unevaluated at the pole itself), gamma across 0
    assert_eq!(ev(&mut e, "tan(interval([1, 2]))"), "interval([-inf, tan(2)], [tan(1), inf])");
    assert_eq!(ev(&mut e, "tan(interval([1, pi/2]))"), "interval([tan(1), inf])");
    let g = ev(&mut e, "g = gamma(interval([-0.5, 0.5])); g");
    assert!(g.starts_with("interval([-inf, -3.544907701811"), "{g}");
    // independent check: the gap between the rays holds no value; gamma(-1/2) = -2 sqrt(pi) and gamma(1/2) = sqrt(pi)
    // are the inner ends (gamma decreases on [-1/2, 0) and on (0, 1/2])
    assert_eq!(ev(&mut e, "[interval_member_q(g, 0), interval_member_q(g, n(-2 sqrt(pi), 30)), interval_member_q(g, n(sqrt(pi), 30))]"), "[false, true, true]");
    let (_, bad) = interval_image(&mut e, "csch(X)", "-1", "1");
    assert_eq!(bad, "[]");
    let (_, bad) = interval_image(&mut e, "sec(X)", "1", "2");
    assert_eq!(bad, "[]");
}

#[test]
fn elliptic_k_reaching_one_is_unbounded() {
    let mut e = CoreEngine::new();
    // a ball containing m = 1: no finite value (it was 11.29 +- 2.26)
    let r = ev(&mut e, "x = n(1, 30) - (n(1/3, 3) - n(1/3, 30)); elliptic_k(x)");
    assert!(!r.contains("ball("), "{r}");
    // the image of [1/2, 1]: K increases to its logarithmic pole at 1
    assert_eq!(ev(&mut e, "elliptic_k(interval([1/2, 1]))"), "interval([elliptic_k(1/2), inf])");
    let r = ev(&mut e, "elliptic_k(interval([0, 1]))");
    assert!(r.starts_with("interval([1.57079632679489") && r.ends_with(", inf])"), "{r}");
    // independent check of a ball below 1: K(m) = pi/2 2F1(1/2, 1/2; 1; m) at 30 digits lies inside K of the ball
    // around 0.99
    let r = ev(&mut e, "y = centered_interval(n(99/100, 30) + (n(1/3, 3) - n(1/3, 30))); k = elliptic_k(y); \
        v = n(pi/2 hypergeometric_2f1(1/2, 1/2, 1, 99/100), 30); abs(v - k[1]) <= k[2]");
    assert_eq!(r, "true");
}

#[test]
fn centered_sums_widen_for_rounded_midpoints() {
    let mut e = CoreEngine::new();
    // 1/3 and -1/3 + 10^-10 are not machine numbers: the sum's radius must cover the rounding of both midpoints
    let r = ev(&mut e, "s = centered_interval(1/3, 10^-30) + centered_interval(-1/3 + 10^-10, 10^-30); abs(s[1] - 10^-10) <= s[2]");
    assert_eq!(r, "true");
    // variations: a product, and three terms
    assert_eq!(ev(&mut e, "p = centered_interval(1/3, 0) * centered_interval(3/7, 0); abs(p[1] - 1/7) <= p[2]"), "true");
    assert_eq!(ev(&mut e, "s = centered_interval(1/10, 0) + centered_interval(2/10, 0) + centered_interval(-3/10, 10^-40); abs(s[1]) <= s[2]"), "true");
}
