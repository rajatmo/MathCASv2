//! Round 30:series of special functions through the `series` wrapper and the library expansions
//! (lib/special.mc), symbolic identities and special values, checked through an engine session.
mod common;
use common::{check, Sys};

#[test]
fn series_near_a_finite_point() {
    check(&[
        ("series(sin_integral(x), x, 0, 5)", "x - x^3/18 + x^5/600 + O(x^6)"),
        ("series(cos_integral(x), x, 0, 4)", "(euler_gamma + log(x)) - x^2/4 + x^4/96 + O(x^5)"),
        ("series(exp_integral_e(1, x), x, 0, 3)", "(-euler_gamma - log(x)) + x - x^2/4 + x^3/18 + O(x^4)"),
        ("series(elliptic_k(x), x, 0, 2)", "(1/2)*pi + (1/8)*pi*x + (9/128)*pi*x^2 + O(x^3)"),
        ("series(elliptic_nome_q(m), m, 0, 3)", "m/16 + m^2/32 + 21*m^3/1024 + O(m^4)"),
        ("series(elliptic_theta(4, 0, q), q, 0, 9)", "1 - 2*q + 2*q^4 - 2*q^9 + O(q^10)"),
        ("series(polylog(2, x), x, 0, 3)", "x + x^2/4 + x^3/9 + O(x^4)"),
        ("series(product_log(x), x, 0, 3)", "x - x^2 + 3*x^3/2 + O(x^4)"),
        ("series(bessel_k(0, x), x, 0, 2)", "(log(2) - euler_gamma - log(x)) + (1/4 + (-1/4)*euler_gamma + (-1/4)*log(x) + (1/4)*log(2))*x^2 + O(x^3)"),
        ("series(kelvin_ker(x), x, 0, 3)", "(log(2) - euler_gamma - log(x)) + (1/16)*pi*x^2 + O(x^4)"),
        ("series(weierstrass_sigma(x, [g2, g3]), x, 0, 7)", "x + (-1/240)*g2*x^5 + (-1/840)*g3*x^7 + O(x^8)"),
        ("series(polygamma(x), x, -1, 1)", "-(x + 1)^-1 + (1 - euler_gamma) + (1 + (1/6)*pi^2)*(x + 1) + O((x + 1)^2)"),
        ("series(jacobi_ns(z, m), z, 0, 1)", "z^-1 + (1/6 + (1/6)*m)*z + O(z^2)"),
        ("series(exp_integral_ei(x), x, 0, 2, assumptions: x < 0)", "(euler_gamma + log(-x)) + x + x^2/4 + O(x^3)"),
    ]);
}

#[test]
fn series_at_infinity() {
    check(&[
        ("series(harmonic_number(x), x, inf, 4)", "euler_gamma + log(x) + (1/2)/x + (-1/12)*x^-2 + (1/120)*x^-4 + O(x^-5)"),
        ("series(polygamma(z), z, inf, 2)", "log(z) + (-1/2)/z + (-1/12)*z^-2 + O(z^-3)"),
        ("series(hypergeometric_u(3, 2, x), x, inf, 5)", "x^-3 + -6*x^-4 + 36*x^-5 + O(x^-6)"),
        ("series(airy_ai_prime(x), x, inf, 2)", "((-1/2)*pi^(-1/2)*x^(1/4) + (-7/96)*pi^(-1/2)*x^(-5/4) + O(x^(-9/4)))*exp((-2/3)*x^(3/2))"),
        ("series(bessel_j_zero(n_u, x), x, inf, 1)", "pi*x + (-1/4)*pi + (1/2)*n_u*pi + ((1/8)/pi + (-1/2)*n_u^2/pi)/x + O(x^-2)"),
    ]);
}

#[test]
fn series_expansions_agree_with_values() {
    // the truncated expansion at a point near the expansion point against the function's value
    let mut s = Sys::new();
    s.run("fn chk(f, x0, xv) = abs(n(rewrite(normal(series(f, x, x0, 8)), x -> xv)) - n(rewrite(f, x -> xv)))");
    for (f, x0, xv, tol) in [
        ("kelvin_kei(x)", "0", "0.1", 1e-12),
        ("struve_h(1, x)", "0", "0.1", 1e-12),
        ("elliptic_nome_q(x)", "0", "0.1", 1e-10),
        ("coulomb_f(1, 2, x)", "0", "0.1", 1e-12),
        ("bessel_y(2, x)", "0", "0.1", 1e-10),
        ("exp_integral_e(3, x)", "0", "0.1", 1e-12),
        ("weierstrass_p(x, [1, 2])", "0", "0.1", 1e-10),
        ("digamma(x)", "-2", "-1.9", 1e-6),
        ("hypergeometric_u(1/3, 2, x)", "inf", "40.", 1e-9),
        ("log_gamma(x)", "inf", "10.", 1e-9),
        ("airy_ai_zero(x)", "inf", "10", 1e-9),
    ] {
        let v = s.run(&format!("chk({f}, {x0}, {xv})"));
        let d: f64 = v.parse().unwrap_or_else(|_| panic!("{f}: {v}"));
        assert!(d < tol, "{f} at {xv}: {d}");
    }
}

#[test]
fn series_without_expansion_stays() {
    check(&[
        ("series(barnes_g(x), x, inf, 2)", "series(barnes_g(x), x, inf, 2)"),
        ("series(sin_integral(x), x, 1, 1)", "sin_integral(1) + sin(1)*(x - 1) + O((x - 1)^2)"),
    ]);
    // a global index variable does not reach the library's sums
    let mut s = Sys::new();
    s.run("j = 5");
    assert_eq!(s.run("series(sin_integral(x), x, 0, 3)"), "x - x^3/18 + O(x^4)");
}

#[test]
fn identities_and_special_values() {
    check(&[
        ("airy_ai(airy_ai_zero(1))", "0"),
        ("bessel_j(0, bessel_j_zero(0, 1))", "0"),
        ("zeta(zeta_zero(10))", "0"),
        ("anger_j(nu, 0)", "sinc(nu*pi)"),
        ("weber_e(1, 0)", "2/pi"),
        ("bessel_j(1, -z)", "-bessel_j(1, z)"),
        ("kelvin_ber(1, -x)", "-kelvin_ber(1, x)"),
        ("elliptic_f(-phi, m)", "-elliptic_f(phi, m)"),
        ("inverse_jacobi_sc(-v, m)", "-inverse_jacobi_sc(v, m)"),
        ("inverse_jacobi_cd(v, 0)", "acos(v)"),
        ("inverse_jacobi_nd(v, 1)", "acosh(v)"),
        ("inverse_jacobi_ns(v, inf)", "0"),
        ("inverse_jacobi_sc(inf, m)", "elliptic_k(m)"),
        ("jacobi_zn(u + elliptic_k(m), m)", "jacobi_zn(u, m) + -1*m*jacobi_cd(u, m)*jacobi_sn(u, m)"),
        ("neville_theta_n(im*elliptic_k(1 - m), m)", "0"),
        ("neville_theta_s(u, 1)", "sinh(u)"),
        ("modular_lambda(im)", "1/2"),
        ("inverse_elliptic_nome_q(e^(-pi))", "1/2"),
        ("weierstrass_p(weierstrass_half_period_w1([11, 7]), [11, 7])", "weierstrass_e1([11, 7])"),
        ("weierstrass_sigma(2*weierstrass_half_period_w2([g2, g3]), [g2, g3])", "0"),
        ("elliptic_exp(5, [0, 0])", "[1/25, -1/125]"),
        ("hermite_h(nu, 0)", "2^nu*pi^(1/2)/gamma((1/2)*(1 - nu))"),
        ("legendre_p(nu, 1)", "1"),
        ("lerch_phi(z, 0, 0)", "(1 - z)^-1"),
        ("lerch_phi(z, s, 1)", "polylog(s, z)/z"),
        ("polygamma(-1, z)", "log_gamma(z)"),
        ("heun_g(1, a*b, a, b, c, d, z)", "hypergeometric_2f1(a, b, c, z)"),
        ("hypergeometric_2f1(1/2, 1/2, 1, z)", "2*elliptic_k(z)/pi"),
        ("hypergeometric_2f1(1, 2, -3, x)", "complex_infinity"),
        ("hypergeometric_1f1(2, 1, x)", "(1 + x)*exp(x)"),
        ("whittaker_w(k, 1/2, 0)", "gamma(1 - k)^-1"),
        ("whittaker_w(2, 1/2, 0)", "0"),
        ("whittaker_m(1, 1, 0)", "0"),
        ("product_log(1, 0)", "-inf"),
        ("sin_integral(complex_infinity)", "indeterminate"),
    ]);
}

#[test]
fn identities_agree_with_values() {
    // each identity's right side at numbers against the function evaluated there directly
    let mut s = Sys::new();
    s.run("vals = [u -> 0.3, m -> 0.4, v -> 0.35, x -> 0.45, k -> 0.3]");
    s.run("fn d2(sy, nm) = abs(n(rewrite(sy, vals)) - n(nm))");
    for (sy, nm) in [
        ("jacobi_epsilon(u + elliptic_k(m), m)", "jacobi_epsilon(0.3 + n(elliptic_k(0.4)), 0.4)"),
        ("jacobi_zn(u + elliptic_k(m), m)", "jacobi_zn(0.3 + n(elliptic_k(0.4)), 0.4)"),
        ("inverse_jacobi_dc(1/v, 0)", "inverse_jacobi_dc(1/0.35, 0.)"),
        ("inverse_jacobi_dn(v + 0.5, 1)", "inverse_jacobi_dn(0.85, 1.)"),
        ("inverse_jacobi_cs(v, 0)", "inverse_jacobi_cs(0.35, 0.)"),
        ("gegenbauer_c(v, x)", "gegenbauer_c(0.35, 0.45)"),
        ("chebyshev_u(1/2, x)", "chebyshev_u(0.5, 0.45)"),
        ("weber_e(3/10, 0)", "weber_e(0.3, 0.)"),
        ("hypergeometric_1f1(1, 1/2, x)", "hypergeometric_1f1(1, 0.5, 0.45)"),
        ("hypergeometric_u(1/2, 1, x)", "hypergeometric_u(0.5, 1, 0.45)"),
        ("hypergeometric_2f1(1, 1/2, 3/2, x)", "hypergeometric_2f1(1, 0.5, 1.5, 0.45)"),
        ("lerch_phi(x, v, 1)", "lerch_phi(0.45, 0.35, 1.)"),
        ("polylog(v + 2, -1)", "polylog(2.35, -1.)"),
    ] {
        let d: f64 = s.run(&format!("d2({sy}, {nm})")).parse().unwrap_or_else(|_| panic!("{sy}"));
        assert!(d < 1e-12, "{sy}: {d}");
    }
}

#[test]
fn machine_values_the_natives_left() {
    // reference digits; each within a few ulps relative
    let mut s = Sys::new();
    for (src, want) in [
        ("jacobi_amplitude(4., 2/3)", 3.0836960187298788),
        ("legendre_q(1/3, 0.5)", -0.03995329475988959),
        ("inverse_erf(0.4, 0.2)", 0.6317759030550062),
        ("inverse_gamma_regularized(4, 3/4, 0.2)", 2.3324979233999876),
        ("inverse_beta_regularized(0.6, 0.2, 4, 2)", 0.7042384955144104),
        ("hypergeometric_pfq_regularized([1/3, 1/3, 1/3], [-2, -3], 0.5)", 15.062110268295829),
        ("hypergeometric_pfq_regularized([1., 13], [0, -3], 7)", 2.5945362780402128e14),
        ("re(jacobi_amplitude(0.2 + 0.1*im, 0.2*im))", 0.20036404855980616),
        ("im_part(inverse_jacobi_sn(0.2 + im, 0.1 - 0.3*im))", 0.8911019500147381),
        ("im_part(factorial_power(1 + im, im, 3.))", 1.0098175021356113),
        ("re(hypergeometric_pfq([im, im, im], [2, 2, 2], -1.*im))", 0.8700317745447872),
    ] {
        let v: f64 = s.run(src).parse().unwrap_or_else(|_| panic!("{src}"));
        assert!((v - want).abs() <= 1e-13 * want.abs(), "{src}: {v} vs {want}");
    }
}

#[test]
fn coulomb_machine_fast_path_agrees_with_balls() {
    // Steed's method in double-double against the 30-digit ball path, inside and outside the turning point
    let mut s = Sys::new();
    for (l, eta, rho) in [
        ("0", "3/10", "5/2"),
        ("0", "-3/10", "1/10"),
        ("0", "3", "1/2"),
        ("0", "3", "7"),
        ("0", "3", "15"),
        ("2", "1", "3"),
        ("3/2", "1/4", "6/5"),
        ("5", "-2", "1/3"),
        ("0", "1/2", "40"),
        ("1/3", "2", "9"),
        ("1", "10", "2"),
    ] {
        for f in ["coulomb_f", "coulomb_g"] {
            let fast: f64 = s.run(&format!("{f}(n({l}), n({eta}), n({rho}))")).parse().unwrap();
            let slow: f64 = s.run(&format!("n(n({f}({l}, {eta}, {rho}), 30))")).parse().unwrap();
            assert!((fast - slow).abs() <= 4e-16 * slow.abs(), "{f}({l}, {eta}, {rho}): {fast} vs {slow}");
        }
    }
    // a plot's worth of points is fast: 400 values well under a second
    let t = std::time::Instant::now();
    s.run("length(table(coulomb_g(0, 0.3, r) + coulomb_h2(0, 3, r), [r, 0.05, 10, 0.05]))");
    assert!(t.elapsed().as_secs_f64() < 1.0);
}
