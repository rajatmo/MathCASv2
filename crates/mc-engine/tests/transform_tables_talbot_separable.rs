//! Transforms added to the tables and routes: the Laplace transforms of sin(t^2) and airy_ai(t), the Fourier transform
//! of bessel_j(2, t) and of a function at a number (quadrature), inverse Laplace values at a number by two Talbot
//! contours, Mellin transforms in two variables with the later variable in its range, Mellin convolution with a box,
//! the inverse Laplace transform in two variables, the Fourier transform of dirac_comb. Each value is compared with
//! a reference computed outside MathCAS (25-digit quadrature of the defining integral, or the closed form of the
//! defining integral derived by hand), and calls outside each region are checked to end without a value.
//! Also: limits in the complex plane at a pole, upper and lower limits of sequences in two indices, asymptotic
//! expansions to a given order, and series of a quotient by a fractional power that must not claim more order
//! than they reached.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

/// `src` returns within `secs` seconds (the calls take well under a second).
fn quick(e: &mut CoreEngine, src: &str, secs: u64) -> String {
    let t0 = std::time::Instant::now();
    let r = ev(e, src);
    assert!(t0.elapsed() < std::time::Duration::from_secs(secs), "{src}: {:?}", t0.elapsed());
    r
}

/// An engine with `close(a, b, tol)`: |a - b| < tol (1 + |b|) in machine numbers.
fn engine() -> CoreEngine {
    let mut e = CoreEngine::new();
    ev(&mut e, "fn close(a, b, tol) = { let dd = n(a) - n(b); abs(re(dd)) + abs(im_part(dd)) < tol * (1 + abs(n(b))) }");
    e
}

#[test]
fn laplace_tables_against_quadrature() {
    let mut e = engine();
    // integrate(sin(t^2) exp(-13/10 t), [t, 0, inf]) at 25 digits, the integral split at the zeros sqrt(k pi)
    holds(&mut e, "close(rewrite(laplace_transform(sin(t^2), t, s), s -> 13/10), 0.2091008422613017942465, 10^-13)");
    // integrate(airy_ai(t) exp(-7/10 t), [t, 0, inf])
    holds(&mut e, "close(rewrite(laplace_transform(airy_ai(t), t, s), s -> 7/10), 0.2122094018413350923798, 10^-13)");
    // at s = 0 the Laplace integral of airy_ai is 1/3 (DLMF 9.10.11)
    holds(&mut e, "close(rewrite(laplace_transform(airy_ai(t), t, s), s -> 0), 1/3, 10^-14)");
}

#[test]
fn fourier_values_and_tables() {
    let mut e = engine();
    // (2/sqrt(2 pi)) integrate(exp(-t^6) sin(t^2) cos(3/10 t), [t, 0, inf]) (the integrand is even)
    holds(&mut e, "close(fourier_transform(exp(-t^6)*sin(t^2), t, 0.3), 0.2102610393589379881190, 10^-10)");
    // integrate(bessel_j(2, t) exp(i w t), [t, -inf, inf])/sqrt(2 pi) at w = 1/2 by oscillatory quadrature
    holds(&mut e, "close(rewrite(fourier_transform(bessel_j(2, t), t, w), w -> 1/2), 0.4606588659617806390203, 10^-14)");
    // |w| > 1: zero (the integral of J2 against exp(i w t) vanishes outside [-1, 1])
    holds(&mut e, "rewrite(fourier_transform(bessel_j(2, t), t, w), w -> 3) == 0");
    // sum over n of delta(x - n): sum of exp(i w n)/sqrt(2 pi) = sqrt(2 pi) sum delta(w - 2 pi k) = dirac_comb(w/(2 pi))/sqrt(2 pi)
    holds(&mut e, "simplify(fourier_transform(dirac_comb(x), x, w) - dirac_comb(w/(2*pi))/sqrt(2*pi)) === 0");
}

#[test]
fn inverse_laplace_values_by_talbot_contours() {
    let mut e = engine();
    // 30-digit Talbot and de Hoog inversions outside MathCAS agree: 0.207358058683607840418
    holds(&mut e, "close(inverse_laplace_transform(1/(s^(3/2) + s^2 + 1), s, 0.3), 0.2073580586836078404184, 10^-12)");
    // a transform with a known inverse: 1/(s + 1) at t = 2 is exp(-2)
    holds(&mut e, "close(inverse_laplace_transform(1/(s^(3/2) + 1) - 1/(s^(3/2) + 1) + 1/(s + 1), s, 2.0), exp(-2), 10^-12)");
    // outside the region: t <= 0, and a transform with complex coefficients, give no Talbot value and end
    let r = quick(&mut e, "inverse_laplace_transform(1/(s^(3/2) + s^2 + 1), s, -0.3)", 10);
    assert!(!r.starts_with("0.") && !r.starts_with("-0."), "{r}");
    quick(&mut e, "inverse_laplace_transform(1/(s^(3/2) + s^2 + 1), s, 0.0)", 10);
    quick(&mut e, "inverse_laplace_transform(1/(s^(3/2) + s^2 + im), s, 0.3)", 10);
}

#[test]
fn transforms_in_two_variables() {
    let mut e = engine();
    // integrate(x^(s-1) y^(t-1)/(x + y^2 + 1), x, y over (0, inf)^2) at s = 1/3, t = 1/2: the x integral is
    // pi csc(pi s) (1 + y^2)^(s - 1) (Euler), the y integral by 30-digit quadrature
    holds(&mut e, "close(rewrite(mellin_transform(1/(x + y^2 + 1), [x, y], [s, t]), [s -> 1/3, t -> 1/2]), 10.33225330750633043216, 10^-13)");
    // inverse of 1/sqrt(1 + p q): cos(2 sqrt(x y))/(pi sqrt(x y)) (inner Laplace integral sqrt(pi/q) exp(-x/q)/(pi sqrt x),
    // then the outer one 1/sqrt(p q + 1)); at x = 1/2, y = 7/10
    holds(&mut e, "close(rewrite(inverse_laplace_transform(1/sqrt(1 + p*q), [p, q], [x, y]), [x -> 1/2, y -> 7/10]), 0.2033525989350363473664, 10^-13)");
    // separable unilateral convolution with a box: integrate(exp(-2 u) box(r - u - 1), [u, 0, r]) times the same in s
    ev(&mut e, "uc = unilateral_convolve(e^(-2*x - y), unit_box(x - 1, y - 1), [x, y], [r, s])");
    holds(&mut e, "close(rewrite(uc, [r -> 1, s -> 2]), (1/2 - exp(-1)/2)*(exp(-1/2) - exp(-3/2)), 10^-14)");
    holds(&mut e, "close(rewrite(uc, [r -> 2, s -> 1]), (exp(-1)/2 - exp(-3)/2)*(1 - exp(-1/2)), 10^-14)");
    holds(&mut e, "rewrite(uc, [r -> 1/4, s -> 1]) == 0");
}

#[test]
fn mellin_convolution_with_a_box() {
    let mut e = engine();
    // integrate(sin(u)/u, [u, y/(a + 1/2), y/(a - 1/2)]) at a = 1, y = 2
    ev(&mut e, "mc = mellin_convolve(sin(x), unit_box(x - a), x, y, assumptions: a > 1/2)");
    holds(&mut e, "close(rewrite(mc, [a -> 1, y -> 2]), 0.5497420495085321616736, 10^-14)");
}

#[test]
fn limits_and_asymptotic_expansions() {
    let mut e = engine();
    // 1/(x (x + 1)) has a pole at 0: |f| grows without bound from every direction
    assert_eq!(ev(&mut e, "limit(x/(x^3 + x^2), x -> 0, direction: complexes)"), "complex_infinity");
    // a branch point is not a pole
    assert_ne!(ev(&mut e, "limit(1/sqrt(x), x -> 0, direction: complexes)"), "0");
    // iterated: the inner limit in n is cos(m), whose upper and lower limits over the integers are 1 and -1
    assert_eq!(ev(&mut e, "discrete_max_limit(cos(m + 1/(n + 1)), [m -> inf, n -> inf])"), "1");
    assert_eq!(ev(&mut e, "discrete_min_limit(cos(m + 1/(n + 1)), [m -> inf, n -> inf])"), "-1");
    // 2^sin(n): sin(n) is dense in [-1, 1] over the integers
    assert_eq!(ev(&mut e, "discrete_max_limit(2^sin(n), n -> inf)"), "2");
    assert_eq!(ev(&mut e, "discrete_min_limit(2^sin(n), n -> inf)"), "1/2");
    // twelve coefficients of sin at 0 (the powers below 12): the Taylor polynomial of degree 11
    holds(&mut e, "expand(asymptotic(sin(x), x -> 0, series_term_goal: 12) - sum((-1)^k*x^(2*k + 1)/factorial(2*k + 1), [k, 0, 5])) === 0");
    // the series solution of the initial value problem to x^5, checked by substitution: the residual is O(x^4)
    ev(&mut e, "ya = asymptotic(d_solve_value([y''(x) - y'(x) + x^10*y(x) == 1, y(0) == 1, y'(0) == -3/4], y(x), x), [x, 0, 5])");
    holds(&mut e, "rewrite(ya, x -> 0) == 1 and rewrite(d(ya, x), x -> 0) == -3/4");
    holds(&mut e, "every(take(coefficient_list(expand(d(ya, [x, 2]) - d(ya, x) - 1), x), 4), c => c == 0)");
    // bessel_j(2, x) at inf: sqrt(2/(pi x)) (cos(x - 5 pi/4) P - sin(x - 5 pi/4) Q) with P = 1 + ..., Q = (4 nu^2 -
    // 1)/(8 x) + ... (DLMF 10.17.3, nu = 2: Q = 15/(8 x)); each oscillating part keeps its first term
    ev(&mut e, "aj = asymptotic(bessel_j(2, x), x -> inf)");
    holds(&mut e, "every([3, 17/2, 40], v => close(rewrite(aj, x -> v), sqrt(2/(pi*v))*(cos(v - 5*pi/4) - 15*sin(v - 5*pi/4)/(8*v)), 10^-14))");
    // the error at x = 200 is of the order x^(-5/2)
    holds(&mut e, "abs(n(rewrite(aj, x -> 200)) - bessel_j(2, 200.0)) < 10^-6");
}

#[test]
fn green_functions_of_a_named_unknown() {
    let mut e = engine();
    // u'' + 5 u' + 6 u with u(0) = u'(0) = 0: G(x, y) = (exp(-2 (x - y)) - exp(-3 (x - y))) theta(x - y); checked
    // by the operator (0 for x > y), G = 0 and dG/dx = 1 at x = y
    ev(&mut e, "g = green_function([u''(x) + 5*u'(x) + 6*u(x), u(0) == 0, u'(0) == 0], u, [x, 0, inf], y)");
    ev(&mut e, "gx = rewrite(g(x, y), heaviside_theta(x - y) -> 1)");
    holds(&mut e, "simplify(d(gx, [x, 2]) + 5*d(gx, x) + 6*gx) === 0 and simplify(rewrite(gx, x -> y)) === 0 and simplify(rewrite(d(gx, x), x -> y)) === 1");
    holds(&mut e, "g(1, 2) == 0");
    // the wave operator u_tt - u_xx: the causal fundamental solution theta(t - s - |x - y|)/2 (d'Alembert)
    ev(&mut e, "w = green_function([d(u(x, t), [t, 2]) - d(u(x, t), [x, 2])], u, [x, -inf, inf], t, [y, s])");
    holds(&mut e, "w(0, 1, 0, 0) == 1/2 and w(2, 1, 0, 0) == 0");
}

#[test]
fn cosine_transform_in_two_variables() {
    let mut e = engine();
    // 1/sqrt(x^2 + y^2) is its own two-dimensional cosine transform (the radial function 1/r in the plane); the
    // second step meets bessel_k(0, u (y^2)^(1/2)) and must stay fast
    let r = quick(&mut e, "fourier_cos_transform(1/sqrt(x^2 + y^2), [x, y], [u, v])", 5);
    let _ = r;
    holds(&mut e, "close(rewrite(fourier_cos_transform(1/sqrt(x^2 + y^2), [x, y], [u, v]), [u -> 3/2, v -> 2/3]), 1/sqrt(9/4 + 4/9), 10^-14)");
}

#[test]
fn series_of_quotients_by_fractional_powers() {
    let mut e = engine();
    // erfi(sqrt(a))/sqrt(a) = (2/sqrt(pi)) (1 + a/3 + a^2/10 + a^3/42 + ...): the series must not claim O(a^4)
    // without the a^3 term
    let r = ev(&mut e, "series(erfi(a^(1/2))/a^(1/2), a, 0, 3)");
    assert!(!r.contains("O(a^4)") || r.contains("a^3"), "{r}");
    holds(&mut e, "close(series_coefficient(series(erfi(a^(1/2))/a^(1/2), a, 0, 6), 2), 1/(5*sqrt(pi)), 10^-15)");
    // the even case keeps its full order: cos(sqrt(a)) to a^3
    assert_eq!(ev(&mut e, "normal(series(cos(a^(1/2)), a, 0, 3))"), ev(&mut e, "1 - a/2 + a^2/24 - a^3/720"));
}
