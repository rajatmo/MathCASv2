//! Round 41: certified root counts of functions that are not polynomials, on real intervals, on rectangles of the
//! plane (the argument principle) and on half-lines (sums of real powers). Each count is checked against an
//! independent route: the roots written explicitly, or the sign changes on a fine grid (all roots simple).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::f64::consts::PI;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// Sign changes of f on a grid of n steps over [a, b].
fn sign_changes(f: impl Fn(f64) -> f64, a: f64, b: f64, n: usize) -> usize {
    let mut last = f(a);
    let mut count = 0;
    for i in 1..=n {
        let v = f(a + (b - a) * i as f64 / n as f64);
        if v * last < 0.0 {
            count += 1;
        }
        if v != 0.0 {
            last = v;
        }
    }
    count
}

#[test]
fn rectangles() {
    // cos(x^3) = 0 where x^3 = pi/2 + k pi: the three cube roots of each, counted inside the square
    let inside = |re: f64, im: f64, s: f64| re.abs() <= s && im.abs() <= s;
    let mut n = 0;
    for k in -10i32..10 {
        let w = PI / 2.0 + k as f64 * PI;
        let (r, t) = (w.abs().cbrt(), if w < 0.0 { PI } else { 0.0 });
        for j in 0..3 {
            let a = (t + 2.0 * PI * j as f64) / 3.0;
            if inside(r * a.cos(), r * a.sin(), 2.0) {
                n += 1;
            }
        }
    }
    assert_eq!(n, 22);
    assert_eq!(ev("count_roots(cos(x^3), [x, -2 - 2im, 2 + 2im])"), "22");
    // cos(x^2) = 0 where x^2 = pi/2 + k pi: the two square roots
    let mut n2 = 0;
    for k in -10i32..10 {
        let w = PI / 2.0 + k as f64 * PI;
        let (re, im) = if w > 0.0 { (w.sqrt(), 0.0) } else { (0.0, (-w).sqrt()) };
        for s in [1.0, -1.0] {
            if inside(s * re, s * im, 3.0) {
                n2 += 1;
            }
        }
    }
    assert_eq!(ev("count_roots(cos(x^2), [x, -3 - 3im, 3 + 3im])"), n2.to_string());
    // exp(exp(z)) = 2: z = log(log 2 + 2 pi i k) + 2 pi i m
    let mut n3 = 0;
    for k in -10i32..=10 {
        let (wr, wi) = (2f64.ln(), 2.0 * PI * k as f64);
        let (lr, la) = ((wr * wr + wi * wi).sqrt().ln(), wi.atan2(wr));
        for m in -5i32..=5 {
            let im = la + 2.0 * PI * m as f64;
            if (-1.0..=3.0).contains(&lr) && (-10.0..=10.0).contains(&im) {
                n3 += 1;
            }
        }
    }
    assert_eq!(n3, 21);
    assert_eq!(ev("count_roots(exp(exp(z)) - 2, [z, -1 - 10im, 3 + 10im])"), "21");
    assert_eq!(ev("count_roots(-1 + cos(2 + sin(x)) - cos(2*x), [x, -3 - 3im, 3 + 3im])"), "12");
}

#[test]
fn rectangles_declined() {
    // a pole inside is counted around (tan = sin/cos with no common zero): no root of tan in 1..2, pi in 3..4
    assert_eq!(ev("count_roots(tan(z), [z, 1 - im/2, 2 + im/2])"), "0");
    assert_eq!(ev("count_roots(tan(z), [z, 3 - im/2, 4 + im/2])"), "1");
    // a zero on the boundary, a function that is not analytic: no count
    assert!(ev("count_roots(1/(z^2 + 4) - 1/5, [z, -1 - im/2, 1 + im/2])").starts_with("count_roots("));
    assert!(ev("count_roots(abs(z) - 1, [z, -2 - 2im, 2 + 2im])").starts_with("count_roots("));
    // analytic on the rectangle although tan has poles elsewhere: atan(2) = 1.107... lies outside
    assert_eq!(ev("count_roots(tan(z) - 2, [z, -1 - im/2, 1 + im/2])"), "0");
    assert_eq!(ev("count_roots(tan(z) - 2, [z, -1 - im/2, 3/2 + im/2])"), "1");
}

#[test]
fn intervals() {
    let f = |x: f64| 2.0 * x.exp().sin() - (PI * x).cos() - 0.5;
    assert_eq!(sign_changes(f, 0.0, 7.0, 4_000_000), 348);
    assert_eq!(ev("count_roots(-1/2 - cos(pi*x) + 2*sin(exp(x)), [x, 0, 7])"), "348");
    assert_eq!(ev("count_roots(-1/2 - cos(pi*x) + 2*sin(exp(x)), [x, 0, 5])"), sign_changes(f, 0.0, 5.0, 1_000_000).to_string());
    // roots at the ends count with their multiplicity: 0, pi, 2 pi, 3 pi; x^3 sin(x) has a root of order 4 at 0
    assert_eq!(ev("count_roots(sin(x), [x, 0, 10])"), "4");
    assert_eq!(ev("count_roots(x^3*sin(x), [x, 0, 10])"), "7");
    assert_eq!(ev("count_roots(sin(x), [x, -pi, pi])"), "3");
    assert_eq!(ev("count_roots(sin(x)^2, [x, 0, 3])"), "2");
    // a double root at an integer inside the interval, where the function is exactly 0, is counted twice; one at
    // a point that is not exact (pi) is not proved: no count
    assert_eq!(ev("count_roots(sin(x)^2, [x, -1, 1])"), "2");
    assert!(ev("count_roots(sin(x)^2, [x, -4, 4])").starts_with("count_roots("));
    // outside the real domain on part of the interval: no count
    assert!(ev("count_roots(sqrt(x) - 1/2, [x, -1, 1])").starts_with("count_roots("));
}

#[test]
fn sums_of_real_powers() {
    // x^1000000 - 2 x^777777 + 3 x^12345 + 9 x^67 - 10 on the reals: one negative root (one sign change of f(-x)) and
    // three positive ones, two of them within 3e-7 of each other near 1 + 2e-6 (values through logarithms)
    let g = |t: f64| {
        let l = t.ln_1p();
        -10.0 + 9.0 * (67.0 * l).exp() + 3.0 * (12345.0 * l).exp() - 2.0 * (777777.0 * l).exp() + (1e6 * l).exp()
    };
    assert_eq!(sign_changes(g, -0.01, 0.0, 2_000_000) + sign_changes(g, 0.0, 1e-5, 1_000_000), 3);
    assert_eq!(ev("count_roots(x^1000000 - 2*x^777777 + 3*x^12345 + 9*x^67 - 10, x)"), "4");
    assert_eq!(ev("count_roots(x^1000000 - 2*x^777777 + 3*x^12345 + 9*x^67 - 10, [x, 0, inf])"), "3");
    // real exponents
    let h = |x: f64| x.powf(PI) - x.powf(2f64.sqrt()) - 3f64.sqrt() * x + 2f64.cbrt();
    assert_eq!(sign_changes(h, 1e-9, 10.0, 1_000_000), 2);
    assert_eq!(ev("count_roots(x^pi - x^sqrt(2) - sqrt(3)*x + 2^(1/3), [x, 0, inf])"), "2");
    let k = |x: f64| 2.0 * x.powf(123451.0 / 67890.0) - x * x + 4.0 * x.sqrt() - 4.0 * x - 9.0 / 8.0;
    assert_eq!(sign_changes(k, 1e-9, 50.0, 5_000_000), 4);
    assert_eq!(ev("count_roots(2*x^(123451/67890) - x^2 + 4*sqrt(x) - 4*x - 9/8, [x, 0, inf])"), "4");
    let k2 = |x: f64| 2.0 * x.powf(1.8) - x * x + 4.0 * x.sqrt() - 4.0 * x - 9.0 / 8.0;
    assert_eq!(ev("count_roots(2*x^(9/5) - x^2 + 4*sqrt(x) - 4*x - 9/8, [x, 0, inf])"), sign_changes(k2, 1e-9, 1e4, 5_000_000).to_string());
}
