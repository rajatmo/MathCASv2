//! One-dimensional robustness (round 18, 9D): oscillatory infinite ranges by the zeros of the integrand and
//! Wynn's epsilon-algorithm, rough endpoints by the double-exponential rule, and paths through complex points.
mod common;
use common::Sys;

fn value(s: &mut Sys, src: &str) -> f64 {
    let out = s.run(src);
    out.parse::<f64>().unwrap_or_else(|_| panic!("{src}: {out}"))
}

#[test]
fn oscillatory_tails() {
    let mut s = Sys::new();
    let cases: &[(&str, f64)] = &[
        ("n_integrate(sin(x)/x, [x, 0, inf]) - pi/2", 1e-12),
        ("n_integrate(sin(x)/x, [x, -inf, inf]) - pi", 1e-12),
        ("n_integrate(cos(x)/x, [x, 1, inf]) + cos_integral(1)", 1e-13),
        ("n_integrate(cos(x)/sqrt(x), [x, 0, inf]) - sqrt(pi/2)", 1e-12),
        ("n_integrate(sin(x^2), [x, 0, inf]) - sqrt(pi/8)", 1e-12),
        ("n_integrate(bessel_j(0, x), [x, 0, inf]) - 1", 1e-12),
    ];
    for &(src, tol) in cases {
        let v = value(&mut s, src);
        assert!(v.abs() <= tol, "{src}: {v}");
    }
    // a complex chirp, part by part
    let out = s.run("n(abs(n_integrate(exp(im * x^2), [x, -inf, inf]) - sqrt(pi) * exp(im * pi / 4)))");
    assert!(out.parse::<f64>().unwrap() < 1e-12, "{out}");
}

#[test]
fn rough_endpoints() {
    let mut s = Sys::new();
    // x^x at 0 has an unbounded derivative: the Gauss-Kronrod estimate was off by 3e-11 and called converged
    let v = value(&mut s, "n_integrate(exp(-x^x), [x, 0, inf]) - n_integrate(exp(-x^x), [x, 0, inf], digits: 30)");
    assert!(v.abs() <= 1e-15, "{v}");
    let v = value(&mut s, "n_integrate(exp(-x^x), [x, 0, 1]) - n_integrate(exp(-x^x), [x, 0, 1], digits: 30)");
    assert!(v.abs() <= 1e-15, "{v}");
}

#[test]
fn complex_paths() {
    let mut s = Sys::new();
    // the ray to i inf: the integral of i sin(t)/t
    assert_eq!(s.run("re(n_integrate(sinh(x)/x, [x, 0, im * inf]))"), "0.0");
    let v = value(&mut s, "im_part(n_integrate(sinh(x)/x, [x, 0, im * inf])) - pi/2");
    assert!(v.abs() < 1e-12, "{v}");
    // a closed polygon around the pole -1/2: 2 pi i
    let v = value(&mut s, "im_part(n_integrate(1/(z + 1/2), [z, 1, im, -1, -im, 1])) - 2*pi");
    assert!(v.abs() < 1e-12, "{v}");
    let v = value(&mut s, "n(abs(n_integrate(sqrt(x), [x, im, 3 - im]) - (2/3)*((3 - im)^(3/2) - im^(3/2))))");
    assert!(v < 1e-13, "{v}");
}
