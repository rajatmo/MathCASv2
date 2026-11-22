//! `resolve` of real sentences with functions that are not polynomial (decided through polynomial relaxations of
//! the functions' values, or instances of a quantifier), and of complex formulas with order relations and free
//! variables. Each expected value is proved here by hand in the comment and sampled numerically where it is a
//! claim about all points.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `src` gives `want` within `secs` seconds.
fn check(src: &str, want: &str, secs: u64) {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    let got = ev(&mut e, src);
    assert_eq!(got, want, "{src}");
    assert!(t.elapsed() < Duration::from_secs(secs), "{src} took {:?}", t.elapsed());
}

#[test]
fn transcendental_sentences_false() {
    // x >= 0 (x^sqrt(pi) is real only there): x^sqrt(pi) >= 0, -2 log(1/(1 + x^2)) >= 0, e^x >= 1, sum >= 1
    check("resolve(exists(x, x^sqrt(pi) - 2*log(1/(x^2 + 1)) + e^x < 1), reals)", "false", 30);
    // y = 0 makes the equation 0 == 1
    check("resolve(exists(x, for_all(y, e^x*y^3 + log(x)*y == 1 and x*y + e^x/x >= 2)), reals)", "false", 30);
    // |x| <= 1 and |y| <= 1: x = sin(..) y^3 - 2 <= -1, so x = -1, y = 0, and then 0 - (-1) == 2 fails
    check("resolve(exists([x, y], sin(x - exp(x))*y^3 - x == 2 and x^2 + y^2 <= 1), reals)", "false", 30);
    // e^x >= 1 + x everywhere
    check("resolve(exists(x, exp(x) < 1 + x), reals)", "false", 30);
}

#[test]
fn transcendental_sentences_true() {
    // on (-1, 0), x - cos(x) lies in (-2, -0.54), where sin is negative: the cubic has a real root
    check("resolve(for_all(x, -1 < x and x < 0, exists(y, sin(x - cos(x))*y^3 - x == 1)), reals)", "true", 30);
    // a monic cubic in y has a real root whatever its other coefficients
    check("resolve(for_all(x, -1 < x and x < 1, exists(y, y^3 - bessel_j(2, x + 2)*y - y - 3*x == -2)), reals)", "true", 30);
    // x = 0: 1 < 2
    check("resolve(exists(x, exp(x) < 2 + x), reals)", "true", 30);
}

#[test]
fn sin_of_x_minus_cos_negative_by_samples() {
    // the range claim behind the first true sentence, sampled at 2001 points of [-1, 0] at 30 digits
    let mut e = CoreEngine::new();
    let r = ev(&mut e, "max(table(n(sin(x - cos(x)), 30), [x, -1, 0, 1/2000])) < 0");
    assert_eq!(r, "true");
}

#[test]
fn undecided_sentences_stay() {
    // sin(x) - x + 1/10 decreases and is negative at 1, so the sentence is false; the relaxations (|sin| <= 1)
    // cannot see it: it stays, with no claim, quickly
    let mut e = CoreEngine::new();
    let t = Instant::now();
    let r = ev(&mut e, "resolve(exists(x, x > 1 and sin(x) > x - 1/10), reals)");
    assert!(r.starts_with("exists("), "{r}");
    assert!(t.elapsed() < Duration::from_secs(30));
    // free variables: not this method's
    let r = ev(&mut e, "resolve(exists(x, exp(x) < a), reals)");
    assert!(!r.is_empty());
}

#[test]
fn complex_order_relations_need_real_sides() {
    // x^2 < y over the complex numbers: both sides real; x^2 takes every real value
    check("resolve(exists(x, x^2 < y), complexes)", "element(y, reals)", 30);
    // y = i has no x (formerly answered true: x^2 = i - 1 makes x^2 - y real)
    check("resolve(for_all(y, exists(x, x^2 < y)), complexes)", "false", 30);
    check("resolve(exists(x, x < y), complexes)", "element(y, reals)", 30);
}

#[test]
fn function_form_by_instances() {
    // t = 0 and z = 0 satisfy the formula for every x and y
    check(
        "cylindrical_decomposition(exists([z, t], cylindrical_decomposition(t*x*y <= 1 + t^3 and z <= t*z^2 - y*z + 2*t*x, [x, y, z, t], \"Function\")), [x, y], \"Function\")",
        "true",
        60,
    );
}

/// `exists(z, element([x, y, z], cylinder(...)))`: the projection, checked at the points of a grid against the
/// projection computed here in machine numbers (the z of the cylinder of radius 2 about the axis from the origin
/// to (1, 1, 1) form an interval: 0 <= x + y + z <= 3 and the distance to the axis at most 2), points within
/// 10^-9 of the boundary skipped.
#[test]
fn projection_of_a_cylinder() {
    let mut e = CoreEngine::new();
    let r = ev(&mut e, "p = resolve(exists(z, element([x, y, z], cylinder([[0, 0, 0], [1, 1, 1]], 2))), reals)");
    assert!(!r.contains("exists") && !r.contains("cylinder"), "{r}");
    let mut checked = 0;
    for i in -12..=16 {
        for j in -12..=16 {
            let (x, y) = (i as f64 / 4.0, j as f64 / 4.0);
            // the z of the linear constraint
            let (l1, h1) = (-x - y, 3.0 - x - y);
            // (2/3) z^2 - (2/3)(x + y) z + c <= 0
            let (a, b, c) = (2.0 / 3.0, -2.0 / 3.0 * (x + y), x * x + y * y - (x + y) * (x + y) / 3.0 - 4.0);
            let disc = b * b - 4.0 * a * c;
            let truth = if disc < 0.0 {
                if disc > -1e-9 {
                    continue;
                }
                false
            } else {
                let (l2, h2) = ((-b - disc.sqrt()) / (2.0 * a), (-b + disc.sqrt()) / (2.0 * a));
                let gap = l1.max(l2) - h1.min(h2);
                if gap.abs() < 1e-9 || disc < 1e-9 {
                    continue;
                }
                gap < 0.0
            };
            let got = ev(&mut e, &format!("rewrite(p, [x -> {i}/4, y -> {j}/4])"));
            assert_eq!(got, truth.to_string(), "at ({x}, {y}): {r}");
            checked += 1;
        }
    }
    assert!(checked > 700, "{checked}");
}
