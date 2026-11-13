//! Round 39: Newton's method with a trust radius on functions that level off, uncertain numbers of real functions
//! evaluated through complex intermediates, comparisons of balls of very different magnitudes, the total parity
//! tests, threading of gcd and factor_integer, the factored form of Radon transforms, and the machine Barnes G
//! function.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn num(src: &str) -> f64 {
    let s = ev(src);
    s.trim().parse::<f64>().unwrap_or_else(|_| panic!("{src} gave {s}"))
}

/// `[re, im]` of the root that `find_root(<call>)` gives for the variable `v`.
fn root(call: &str, v: &str) -> (f64, f64) {
    let re = num(&format!("re(rewrite({v}, find_root({call})))"));
    let im = num(&format!("im_part(rewrite({v}, find_root({call})))"));
    (re, im)
}

#[test]
fn newton_stays_near_the_start_when_the_function_levels_off() {
    // the alternating harmonic number is complex off the integers ((-1)^n in its continuation); from n = 1 the
    // Newton steps go up into the half plane where it tends to log 2, and without a trust radius a step to a far
    // point of slightly smaller |f| led to the root near -48.25 + 0.55 i; the root reached is n = 2, H'(2) = 1/2
    let (a, b) = root("alternating_harmonic_number(n) == 0.5, [n, 1]", "n");
    assert!((a - 2.0).abs() < 1e-12 && b.abs() < 1e-12, "{a} {b}");
    assert_eq!(ev("chop(rewrite(n, find_root(alternating_harmonic_number(n) == 0.5, [n, 1])), 10^-6)"), "2.0");
    // H'(4) = 7/12 exactly, from n = 3.5
    let (a, b) = root("alternating_harmonic_number(n) == 7/12, [n, 3.5]", "n");
    assert!((a - 4.0).abs() < 1e-12 && b.abs() < 1e-12, "{a} {b}");
    // a complex root near 5: its residual at 30 digits is at the rounding level of the machine root
    let r = num("{ let z = rewrite(n, find_root(alternating_harmonic_number(n) == 0.8, [n, 5])); \
                 n(abs(alternating_harmonic_number(set_precision(z, inf)) - 4/5), 30) }");
    assert!(r < 1e-14, "{r}");
    let (a, b) = root("alternating_harmonic_number(n) == 0.8, [n, 5]", "n");
    assert!((a - 5.0).abs() < 0.1 && b.abs() < 0.1, "{a} {b}");
    // a real function that levels off: tanh(x) = 1/2 from x = 3 is atanh(1/2) = log(3)/2
    let x = num("rewrite(x, find_root(tanh(x) == 0.5, [x, 3]))");
    assert!((x - 3f64.ln() / 2.0).abs() < 1e-15, "{x}");
    // a linear function far from the start still converges (the radius doubles while the model is exact)
    let x = num("rewrite(x, find_root(x - 1e10, [x, 0]))");
    assert_eq!(x, 1e10);
}

#[test]
fn uncertain_values_of_real_functions_through_complex_intermediates() {
    // F3 with x = 0 is 2F1(3/2, 5; 1/2; y) = (1 + 9 y)/(1 - y)^6 (Euler's transformation): real at y = 2.1, its
    // derivative there is 9/(1 - y)^6 + 6 (1 + 9 y)/(1 - y)^7 (both at the binary value of the machine number 2.1)
    let at = |e: &str| ev(&format!("n(n(rewrite({e}, y -> set_precision(2.1, inf)), 30))"));
    let got = ev("appell_f3(1/2, 3/2, -4, 5, 1/2, 0, around(2.1, 0.01))");
    assert!(got.starts_with(&format!("around({}, ", at("(1 + 9*y)/(1 - y)^6"))), "{got}");
    let spread: f64 = got.trim_end_matches(')').rsplit(", ").next().unwrap().parse().unwrap();
    let want: f64 = at("abs(9/(1 - y)^6 + 6*(1 + 9*y)/(1 - y)^7)/100").parse().unwrap();
    assert!((spread - want).abs() < 1e-14 * want, "{spread} {want}");
    // the same function at y = 3: (1 + 27)/64 and |9/64 - 6*28/128| 0.01
    assert_eq!(ev("hypergeometric_2f1(3/2, 5, 1/2, around(3.0, 0.01))"), "around(0.4375, 0.01171875)");
    // Y(2, 1, theta, pi) = (1/4) sqrt(30/pi) sin(theta) cos(theta) is real; the spread is |cos(2 theta)| times the
    // factor, times 0.01
    let got = ev("spherical_harmonic_y(2, 1, around(2, 0.01), pi)");
    let want = format!("around({}, {})", ev("n(30^(1/2)*sin(2)*cos(2)/(4*pi^(1/2)))"), ev("n(30^(1/2)*abs(cos(4))/(4*pi^(1/2))/100)"));
    let close = ev(&format!("{{ let a = {got}; let b = {want}; [a, b] }}"));
    assert!(!got.contains("im"), "{got}");
    assert!(close.starts_with("[around(-0.2923332799642"), "{close}");
    // Y(3, 2, theta, pi/2) = -(1/4) sqrt(105/(2 pi)) sin(theta)^2 cos(theta): e^(2 i pi/2) = -1, real
    let got = ev("spherical_harmonic_y(3, 2, around(1.1, 0.02), pi/2)");
    assert!(!got.contains("im"), "{got}");
    let c = num("n(-105^(1/2)*sin(11/10)^2*cos(11/10)/(4*(2*pi)^(1/2)))");
    assert!(got.starts_with(&format!("around({}", &c.to_string()[..12])), "{got} {c}");
    // a value that is complex keeps its imaginary part: 2F1(1/2, 5/2; 2; 2.1) on its branch cut
    let got = ev("appell_f1(1/2, 1, 5/2, 3, 1, around(2.1, 0.01))");
    assert!(got.contains("-0.11516288504089") && got.contains("-0.85504555644632"), "{got}");
    // acosh on [-1, 1] is i acos(x): imaginary, with the spread |1/sqrt(1 - x^2)| dx
    let got = ev("acosh(around(0.9, 0.1))");
    let (im, spread) = (ev("n(acos(9/10))"), ev("n(1/(10*(1 - 81/100)^(1/2)))"));
    assert!(got.contains(&im[..15]) && got.contains(&spread[..15]), "{got} {im} {spread}");
}

#[test]
fn comparisons_of_balls_of_very_different_magnitudes() {
    // (the comparison itself: mc-arith tests/numbers.rs) the 30-digit values of Coulomb wave functions agree with
    // the 60-digit ones to their precision: the difference, a ball about 0 whose radius is about half the bound,
    // compares below the bound 10^-29 |b| whose midpoint is many binades larger
    assert_eq!(ev("{ let a = n(coulomb_f(3, 12/100, 7), 30); let b = n(coulomb_f(3, 12/100, 7), 60); abs(a - b) <= 10^-29*abs(b) }"), "true");
    assert_eq!(ev("{ let a = n(coulomb_g(3, 12/100, 7), 30); let b = n(coulomb_g(3, 12/100, 7), 60); abs(a - b) <= 10^-29*abs(b) }"), "true");
}

#[test]
fn parity_tests_are_total() {
    assert_eq!(ev("[is_even(x), is_even(\"a\"), is_even(2.0), is_even(2*pi), is_even(-4)]"), "[false, false, false, false, true]");
    assert_eq!(ev("[is_odd(x), is_odd(3.0), is_odd(3/2), is_odd(-3), is_odd(2^201 + 1)]"), "[false, false, false, true, true]");
    // as is_integer and is_prime
    assert_eq!(ev("[is_integer(2.0), is_prime(7.0), is_prime(x)]"), "[false, false, false]");
    assert_eq!(ev("count_if([1, 2.0, x, 4, 6], is_even)"), "2");
}

#[test]
fn threading_over_lists() {
    assert_eq!(ev("factor_integer([12, 15])"), ev("map(factor_integer, [12, 15])"));
    assert_eq!(ev("gcd([12, 18], 8)"), "[4, 2]");
    assert_eq!(ev("[gcd([12, 18], [8, 27]), lcm([4, 6], 8)]"), "[[4, 9], [8, 24]]");
    // the gcd of a list's elements
    assert_eq!(ev("apply(gcd, [12, 18, 8])"), "2");
}

#[test]
fn mellin_transform_of_a_shifted_step() {
    // the integral of x^(s - 1) e^-x over x > a is gamma(s, a)
    assert_eq!(ev("mellin_transform(exp(-x)*unit_step(x - 2), x, s)"), "gamma(s, 2)");
    assert_eq!(ev("mellin_transform(exp(-x)*unit_step(x - 3), x, s)"), "gamma(s, 3)");
    let v = num("n(mellin_transform(exp(-x)*unit_step(x - 2), x, 5/2))");
    let w = num("n_integrate(x^(3/2)*exp(-x), [x, 2, inf])");
    assert!((v - w).abs() < 1e-10, "{v} {w}");
}

#[test]
fn radon_transforms_keep_their_sign() {
    assert_eq!(
        ev("radon_transform((x^2 + 5*y^2)*exp(-x^2 - y^2), [x, y], [p, phi])"),
        "(1/2)*(3 + 2*cos(2*phi) + 6*p^2 + -4*cos(2*phi)*p^2)*exp(-p^2)*pi^(1/2)"
    );
    assert_eq!(ev("radon_transform((x^2 + y^2)*exp(-x^2 - y^2), [x, y], [p, phi])"), "(1/2)*(1 + 2*p^2)*exp(-p^2)*pi^(1/2)");
    // the same value as the line integral at a point
    let v = num("n(radon_transform((x^2 + 5*y^2)*exp(-x^2 - y^2), [x, y], [7/10, 1/3]))");
    let w = num("n_integrate(rewrite((x^2 + 5*y^2)*exp(-x^2 - y^2), [x -> 7/10*cos(1/3) - t*sin(1/3), y -> 7/10*sin(1/3) + t*cos(1/3)]), [t, -inf, inf])");
    assert!((v - w).abs() < 1e-9, "{v} {w}");
    // x^2 on the line: sqrt(pi) e^(-p^2) (p^2 cos^2 phi + sin^2 phi / 2), with its content 1/4 taken out
    let r = ev("radon_transform(x^2*exp(-x^2 - y^2), [x, y], [p, phi])");
    assert!(r.starts_with("(1/4)*(1 "), "{r}");
    assert_eq!(ev(&format!("simplify(expand_all(trig_expand({r} - sqrt(pi)*exp(-p^2)*(p^2*cos(phi)^2 + sin(phi)^2/2))))")), "0");
}

#[test]
fn machine_barnes_g_and_its_critical_points() {
    // G at machine reals against 30 digits: within an ulp
    for x in ["0.001", "0.5", "1.5", "2.7", "10.25", "19.9", "21.3", "28.0", "-0.5", "-2.999", "-3.0001", "-12.7"] {
        let rel = num(&format!("{{ let g = barnes_g({x}); let t = n(barnes_g(set_precision({x}, inf)), 30); n(abs(set_precision(g, inf) - t)/abs(t)) }}"));
        assert!(rel < 2.3e-16, "{x}: {rel}");
    }
    // G'(x) = G(x) ((x - 1) psi(x) - x + (1 + log(2 pi))/2), G > 0 on (0, 2): one root
    let crit = "(x - 1)*digamma(x) - x + (1 + log(2*pi))/2";
    assert_eq!(ev("length(solve(d(barnes_g(x), x) == 0 and (0 < x and x < 2), x, reals))"), "1");
    let s = num("rewrite(x, solve(d(barnes_g(x), x) == 0 and (0 < x and x < 2), x, reals)[1])");
    let x = num(&format!("n(rewrite(x, find_root({crit}, [x, 1.4], working_precision: 30)))"));
    assert!((s - x).abs() < 1e-14, "{s} {x}");
    // between -3 and -1: two (G has no zero there)
    assert_eq!(ev("length(solve(d(barnes_g(x), x) == 0 and (-3 < x and x < -1), x, reals))"), "2");
    for (k, start) in [(1, "-2.5"), (2, "-1.45")] {
        let s = num(&format!("rewrite(x, solve(d(barnes_g(x), x) == 0 and (-3 < x and x < -1), x, reals)[{k}])"));
        let r = num(&format!("n(rewrite(x, find_root({crit}, [x, {start}], working_precision: 30)))"));
        assert!((s - r).abs() < 1e-13, "{s} {r}");
    }
    // G(x) = 1/2 on (0, 2)
    let s = num("rewrite(x, solve(barnes_g(x) == 1/2 and (0 < x and x < 2), x, reals)[1])");
    let r = num("n(rewrite(x, find_root(barnes_g(x) == 1/2, [x, 0.4], working_precision: 30)))");
    assert!((s - r).abs() < 1e-14, "{s} {r}");
}
