//! Round 43b: proofs for property answers. The sign of f, f' or f'' over the whole domain (also where f' is not
//! defined at an end, also on unbounded intervals by the limit at an end and the sign of the next derivative); facts
//! of the function table (inverse Jacobi functions, hypergeometric functions with positive parameter pairs, ...);
//! injectivity from the pieces of the domain and the intermediate value theorem; strict monotonicity of sums with
//! step functions; convexity of continuous functions with kinks; enclosures of functions of several intervals by the
//! monotonicity test; limits at the poles of tanh; the residue of zeta. Each answer is checked by an independent
//! route: values at 30 digits, the defining inequality at points, or a second formula.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// Is `src` true at 30 digits (a comparison of closed numeric terms)?
fn holds(e: &mut CoreEngine, src: &str) -> bool {
    ev(e, &format!("n({src}, 30)")) == "true" || ev(e, src) == "true"
}

fn real(e: &mut CoreEngine, src: &str) -> f64 {
    let s = ev(e, &format!("n({src})"));
    s.parse::<f64>().unwrap_or_else(|_| panic!("{src}: {s}"))
}

#[test]
fn signs_of_derivatives_up_to_the_ends_of_the_domain() {
    let mut e = CoreEngine::new();
    // acosh' = 1/sqrt(x^2 - 1) > 0 inside, acosh continuous at 1: strictly increasing on [1, inf)
    assert_eq!(ev(&mut e, "function_monotonicity([acosh(x), x >= 1], x, strict_inequalities: true)"), "1");
    assert_eq!(ev(&mut e, "function_injective(acosh(x), x)"), "true");
    assert_eq!(ev(&mut e, "function_sign([acosh(x), x > 1], x)"), "1");
    assert!(holds(&mut e, "0 < acosh(3/2) and acosh(3/2) < acosh(2) and acosh(2) < acosh(5)"));
    // variations: atanh on (-1, 1) (1/(1 - x^2) > 0), and acosh of an increasing argument
    assert_eq!(ev(&mut e, "function_injective(atanh(x), x)"), "true");
    assert_eq!(ev(&mut e, "function_monotonicity([acosh(2*x + 1), x >= 0], x, strict_inequalities: true)"), "1");
    assert!(holds(&mut e, "atanh(-9/10) < atanh(0) and atanh(0) < atanh(1/2)"));
}

#[test]
fn signs_on_unbounded_intervals_from_the_limit_at_an_end() {
    let mut e = CoreEngine::new();
    // erfc decreases to 0 at infinity: positive
    assert_eq!(ev(&mut e, "function_sign(erfc(x), x)"), "1");
    assert!(holds(&mut e, "erfc(5) > 0 and erfc(-5) < 2"));
    // variations: atan(x) + pi/2 increases from 0 at -inf; Shi(x) - x on x > 0 starts at 0 and its derivative
    // sinh(x)/x - 1 is positive there (itself 0 at 0 with a positive derivative)
    assert_eq!(ev(&mut e, "function_sign(atan(x) + pi/2, x)"), "1");
    assert_eq!(ev(&mut e, "function_sign([sinh_integral(x) - x, x > 0], x)"), "1");
    assert!(holds(&mut e, "atan(-100) + pi/2 > 0 and sinh_integral(1/10) - 1/10 > 0 and sinh_integral(3) - 3 > 0"));
    // sinh_integral increases on the line: its derivative sinh(x)/x is not defined at 0, where Shi is continuous
    assert_eq!(ev(&mut e, "function_monotonicity(sinh_integral(x), x)"), "1");
    assert!(holds(&mut e, "sinh_integral(-1) < sinh_integral(0) and sinh_integral(0) < sinh_integral(1/1000)"));
}

#[test]
fn facts_of_the_table() {
    let mut e = CoreEngine::new();
    // arccd(x|m) is the integral of ((1 - t^2)(1 - m t^2))^(-1/2) from x to 1: decreasing and nonnegative on [-1, 1]
    assert_eq!(ev(&mut e, "function_monotonicity([inverse_jacobi_cd(x, 1/3), -1 <= x and x <= 1], x)"), "-1");
    assert_eq!(ev(&mut e, "function_sign([inverse_jacobi_cd(x, 1/3), -1 <= x and x <= 1], x)"), "1");
    assert!(real(&mut e, "inverse_jacobi_cd(-0.9, 1/3)") > real(&mut e, "inverse_jacobi_cd(0.2, 1/3)"));
    // variations: a negative parameter, and m > 1 through the reciprocal modulus (arcdc(x|3) = 3^(-1/2) arccd(x|1/3))
    assert_eq!(ev(&mut e, "function_monotonicity([inverse_jacobi_cd(x, -2), -1 <= x and x <= 1], x)"), "-1");
    assert!(real(&mut e, "inverse_jacobi_cd(-0.5, -2)") > real(&mut e, "inverse_jacobi_cd(0.5, -2)"));
    assert_eq!(ev(&mut e, "function_monotonicity([inverse_jacobi_dc(x, 3), -1 <= x and x <= 1], x)"), "-1");
    let (a, b) = (real(&mut e, "inverse_jacobi_dc(0.3, 3)"), real(&mut e, "inverse_jacobi_cd(0.3, 1/3)/sqrt(3)"));
    assert!((a - b).abs() < 1e-12, "{a} {b}");
    // 3F3(1, 1, 1; 3, 3, 3; z) is a Laplace transform of a positive measure on [0, 1] (DLMF 16.5.2): positive,
    // increasing, convex on the line
    assert_eq!(ev(&mut e, "function_sign(hypergeometric_pfq([1, 1, 1], [3, 3, 3], z), z)"), "1");
    assert_eq!(ev(&mut e, "function_injective(hypergeometric_pfq([1, 1, 1], [3, 3, 3], z), z)"), "true");
    assert!(holds(&mut e, "hypergeometric_pfq([1, 1, 1], [3, 3, 3], -500) > 0"));
    // variations: 1F1(1/2; 3/2; x) (= sqrt(pi) erfi(sqrt(x))/(2 sqrt(x))) and 2F2(1, 2; 3, 5/2; x)
    assert_eq!(ev(&mut e, "function_monotonicity(hypergeometric_1f1(1/2, 3/2, x), x)"), "1");
    assert_eq!(ev(&mut e, "function_convexity(hypergeometric_pfq([1, 2], [3, 5/2], x), x)"), "1");
    assert!(holds(&mut e, "hypergeometric_1f1(1/2, 3/2, -10) > 0 and hypergeometric_1f1(1/2, 3/2, -10) < hypergeometric_1f1(1/2, 3/2, -9)"));
    // Neville's theta_d is positive for m < 1 (the product of DLMF 20.5.3)
    assert_eq!(ev(&mut e, "table(function_sign(neville_theta_d(x, 1/m), x), m, 4)"), "[1, 1, 1, 1]");
    assert_eq!(ev(&mut e, "function_sign(neville_theta_n(x, -3), x)"), "1");
    assert!(real(&mut e, "neville_theta_n(1.7, -3)") > 0.0);
}

#[test]
fn injective_by_pieces_and_by_values_taken_twice() {
    let mut e = CoreEngine::new();
    // csch: decreasing on x < 0 and x > 0 with the images (-inf, 0) and (0, inf)
    assert_eq!(ev(&mut e, "function_injective(csch(x), x)"), "true");
    // Ei takes every negative value on x < 0 and every real on x > 0
    assert_eq!(ev(&mut e, "function_injective(exp_integral_ei(x), x)"), "false");
    assert!(holds(&mut e, "exp_integral_ei(-1) < 0 and exp_integral_ei(3/10) < exp_integral_ei(-1) and exp_integral_ei(4/10) > exp_integral_ei(-1)"));
    // variations: 1/(x^3 - 8) (images (-inf, 0) and (0, inf)), li (values below 0 on both pieces), and the inverse
    // Gudermannian function, real on stretches of period 2 pi
    assert_eq!(ev(&mut e, "function_injective(1/(x^3 - 8), x)"), "true");
    assert_eq!(ev(&mut e, "function_injective(log_integral(x), x)"), "false");
    assert_eq!(ev(&mut e, "function_injective(inverse_gudermannian(x), x)"), "false");
    assert!(holds(&mut e, "log_integral(1/2) < 0 and log_integral(11/10) < log_integral(1/2)"));
    // sech is even: a turning point at 0
    assert_eq!(ev(&mut e, "function_injective(sech(x), x)"), "false");
    // ramp is 0 on x <= 0
    assert_eq!(ev(&mut e, "function_injective(ramp(x), x)"), "false");
    assert_eq!(ev(&mut e, "[ramp(-1), ramp(-2)]"), "[0, 0]");
}

#[test]
fn strict_monotonicity_and_convexity_with_steps_and_kinks() {
    let mut e = CoreEngine::new();
    // x + floor(x): a strictly increasing part plus a nondecreasing one
    assert_eq!(ev(&mut e, "function_monotonicity(x + floor(x), x, strict_inequalities: true)"), "1");
    assert!(holds(&mut e, "1/2 + floor(1/2) < 9/10 + floor(9/10) and 9/10 + floor(9/10) < 1 + floor(1)"));
    // variations: 2 x - ceiling(-x), and ramp(x) + 1 which is constant for x <= 0 (not strict)
    assert_eq!(ev(&mut e, "function_monotonicity(2*x - ceiling(-x), x, strict_inequalities: true)"), "1");
    assert_eq!(ev(&mut e, "function_monotonicity(ramp(x) + 1, x, strict_inequalities: true)"), "indeterminate");
    // |x|^3 = x^2 |x|: -x^3 and x^3 meet at 0 with slope 0, both convex (f'' >= 0, zero only at 0)
    assert_eq!(ev(&mut e, "function_convexity(x^2*abs(x), x, strict_inequalities: true)"), "1");
    assert!(holds(&mut e, "(abs(-1)^3 + abs(2)^3)/2 > abs(1/2)^3"));
    // variations: |x|^5 and |x|^3 + x^2 convex; x |x| neither (concave for x < 0)
    assert_eq!(ev(&mut e, "function_convexity(x^4*abs(x), x)"), "1");
    assert_eq!(ev(&mut e, "function_convexity(abs(x)^3 + x^2, x)"), "1");
    assert_eq!(ev(&mut e, "function_convexity(x*abs(x), x)"), "indeterminate");
}

#[test]
fn several_intervals_by_the_monotonicity_test() {
    let mut e = CoreEngine::new();
    // R_F decreases in each argument (its partials are -R_D/6 < 0): the image is the interval between the corners
    let r = ev(&mut e, "carlson_rf(centered_interval(3/2, 1/100), centered_interval(5/4, 1/100), centered_interval(7/6, 1/100))");
    assert!(r.starts_with("centered_interval("), "{r}");
    let (c, w) = (real(&mut e, &format!("{r}[1]")), real(&mut e, &format!("{r}[2]")));
    let (lo, hi) = (real(&mut e, "carlson_rf(151/100, 126/100, 706/600)"), real(&mut e, "carlson_rf(149/100, 124/100, 694/600)"));
    assert!(c - w <= lo && hi <= c + w && w < 0.00339, "{c} {w} {lo} {hi}");
    // variations: the Lerch transcendent (increasing in z, decreasing in a) and another box of R_F
    let r = ev(&mut e, "lerch_phi(interval([0.234, 0.235]), interval([0.345, 0.346]), interval([0.456, 0.457]))");
    assert!(r.starts_with("interval("), "{r}");
    let (a, b) = (real(&mut e, &format!("{r}[1, 1]")), real(&mut e, &format!("{r}[1, 2]")));
    for (z, s, q) in [(0.234, 0.345, 0.457), (0.235, 0.346, 0.456), (0.2345, 0.3455, 0.4565)] {
        let v = real(&mut e, &format!("lerch_phi({z}, {s}, {q})"));
        assert!(a <= v && v <= b, "{v} not in [{a}, {b}]");
    }
    assert!(b - a < 0.0036, "{a} {b}");
    let r = ev(&mut e, "carlson_rf(interval([1, 2]), interval([2, 3]), interval([3, 4]))");
    let (a, b) = (real(&mut e, &format!("{r}[1, 1]")), real(&mut e, &format!("{r}[1, 2]")));
    let (lo, hi) = (real(&mut e, "carlson_rf(2, 3, 4)"), real(&mut e, "carlson_rf(1, 2, 3)"));
    // (a wide box: the partials are not all proved on it at once, so the bound is a little looser)
    assert!(a <= lo && hi <= b && (b - a) < 1.3 * (hi - lo), "{a} {b} {lo} {hi}");
}

#[test]
fn limits_at_the_poles_of_tanh_and_the_residue_of_zeta() {
    let mut e = CoreEngine::new();
    // tanh has a simple pole at i pi/2 with residue 1: t tanh(i pi/2 + t) -> 1, and tanh itself has no limit
    assert_eq!(ev(&mut e, "limit(t*tanh(im*pi/2 + t), t -> 0)"), "1");
    let r = ev(&mut e, "limit(tanh(im*pi/2 + t), t -> 0)");
    assert!(!r.contains("tan((1/2)*pi)") && !r.starts_with("im*"), "{r}");
    assert!((real(&mut e, "re(1e-8*tanh(im*pi/2 + 1e-8))") - 1.0).abs() < 1e-6);
    // variations: the pole at 3 i pi/2, and t^2 tanh(i pi/2 + t) -> 0
    assert_eq!(ev(&mut e, "limit(t*tanh(3*im*pi/2 + t), t -> 0)"), "1");
    assert_eq!(ev(&mut e, "limit(t^2*tanh(im*pi/2 + t), t -> 0)"), "0");
    // zeta has a simple pole at 1 with residue 1 (DLMF 25.2.4)
    assert_eq!(ev(&mut e, "residue(zeta(x), [x, 1])"), "1");
    assert!((real(&mut e, "re(1e-6*zeta(1 + 1e-6))") - 1.0).abs() < 1e-5);
    // variations: zeta(2 x) at 1/2 (1/2), and a regular point (0)
    assert_eq!(ev(&mut e, "residue(zeta(2*x), [x, 1/2])"), "1/2");
    assert_eq!(ev(&mut e, "residue(zeta(x), [x, 2])"), "0");
}
