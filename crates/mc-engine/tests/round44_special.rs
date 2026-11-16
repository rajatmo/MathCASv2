//! Round 44: special functions in depth. Each new method is checked by an independent route: a different
//! algorithm for the same value (the double-double machine kernels against the ball kernels, quadrature, the native
//! numerical evaluation against a new closed form), round trips through an inverse function, or the defining
//! relation.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// |a - b| < 10^-k (|a| + 1), both sides evaluated to 30 digits (machine numbers inside stay machine numbers).
fn close(e: &mut CoreEngine, a: &str, b: &str, k: u32) -> bool {
    let d = format!("(n({a}, 30) - n({b}, 30))");
    let src = format!("n(abs(re{d}) + abs(im_part{d})) < 10^-{k}*(1 + n(abs(n({a}, 30))))");
    let r = ev(e, &src);
    assert!(r == "true" || r == "false", "{src} gave {r}");
    r == "true"
}

#[test]
fn elliptic_f_at_complex_amplitudes_and_beyond_the_real_domain() {
    let mut e = CoreEngine::new();
    // F(12/5 | 3) at 50 digits, against the double-double machine path (another algorithm)
    let v = ev(&mut e, "re(n(elliptic_f(12/5, 3), 50))");
    assert!(v.starts_with("1.0010773804561062360796595863838358931497135904585"), "{v}");
    let v = ev(&mut e, "im_part(n(elliptic_f(12/5, 3), 50))");
    assert!(v.starts_with("-1.923166189504625353184144409335278171570687870309"), "{v}");
    assert!(close(&mut e, "elliptic_f(12/5, 3)", "elliptic_f(2.4, 3.0)", 14));
    // two variations: a real amplitude with m sin^2 > 1 and j = 0, and a negative amplitude with j = -1
    assert!(close(&mut e, "elliptic_f(1, 3)", "elliptic_f(1.0, 3.0)", 14));
    assert!(close(&mut e, "elliptic_f(-7/2, 5/2)", "elliptic_f(-3.5, 2.5)", 14));
    // complex amplitudes at 30 digits, against the machine complex path
    assert!(close(&mut e, "elliptic_f(1 + im, 1/2)", "elliptic_f(1.0 + 1.0*im, 0.5)", 14));
    assert!(close(&mut e, "elliptic_f(4 - 2*im, 3 + im)", "elliptic_f(4.0 - 2.0*im, 3.0 + 1.0*im)", 13));
    // on the line Re phi = pi/2 (where R_F's first argument is on its cut): the half-period form, and the round trip
    // dn(F(...)) = 1/2 of the inverse function
    ev(&mut e, "u = n(inverse_jacobi_dn(1/2, 1/3), 30)");
    let u = ev(&mut e, "re(u)");
    assert!(u.starts_with("1.73391688525793502508492"), "{u}");
    assert!(close(&mut e, "jacobi_dn(u, 1/3)", "1/2", 25));
    assert!(close(&mut e, "elliptic_f(pi/2 - im/3, 1/2)", "n_integrate(1/sqrt(1 - sin(t)^2/2), [t, 0, pi/2 - im/3])", 10));
    // the property that waited for it
    assert_eq!(ev(&mut e, "function_convexity(inverse_jacobi_dn(x, m), [x, m])"), "indeterminate");
}

#[test]
fn bessel_j_of_a_ball_order_is_a_tight_enclosure() {
    let mut e = CoreEngine::new();
    // three boxes; each result contains the values at the corners and the centre (30
    // digits) and is far narrower than the natural extension of the series was (width 1 for the first box)
    for (nu, x) in [("[0.5, 0.6]", "[2.4, 2.5]"), ("[1.2, 1.3]", "[0.7, 0.9]"), ("[0.0, 0.2]", "[1.0, 1.1]")] {
        ev(&mut e, &format!("r = bessel_j(interval({nu}), interval({x}))"));
        let r = ev(&mut e, "r");
        assert!(r.starts_with("interval(["), "{r}");
        let w = ev(&mut e, "max(r) - min(r) < 0.15");
        assert_eq!(w, "true", "{r}");
        for (a, b) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            let p = format!("bessel_j(set_precision({nu}[{}], 30), set_precision({x}[{}], 30))", a + 1, b + 1);
            assert_eq!(ev(&mut e, &format!("min(r) <= {p} and {p} <= max(r)")), "true", "{p} outside {r}");
        }
    }
    let r = ev(&mut e, "bessel_j(interval([0.5, 0.6]), interval([2.4, 2.5]))");
    assert_eq!(ev(&mut e, &format!("min({r}) > 0.143 and max({r}) < 0.63")), "true", "{r}");
}

#[test]
fn appell_rows_near_one_use_the_connection_formula() {
    let mut e = CoreEngine::new();
    // both variables near 1: 9 s before (the row Gauss functions at 0.95 went to the ball path); the machine value
    // against 20 digits from the exact arguments (the ball path)
    assert!(close(&mut e, "appell_f1(2.0, 1.5, 0.5, 3.0, 0.9, 0.95)", "appell_f1(2, 3/2, 1/2, 3, 9/10, 95/100)", 13));
    assert!(close(&mut e, "appell_f1(1.0, 0.3, 0.7, 2.5, 0.8, 0.99)", "appell_f1(1, 3/10, 7/10, 5/2, 8/10, 99/100)", 13));
    // F1(a; b1, b2; c; x, x) = 2F1(a, b1 + b2; c; x): a third route
    assert!(close(&mut e, "appell_f1(1.0, 0.3, 0.7, 2.5, 0.9, 0.9)", "hypergeometric_2f1(1, 1, 5/2, 9/10)", 13));
}

#[test]
fn asymptotic_series_at_infinity() {
    let mut e = CoreEngine::new();
    assert_eq!(
        ev(&mut e, "normal(series(erf(x), x, inf, 5))"),
        ev(&mut e, "1 + (-1/(pi^(1/2)*x) - 3/(4*pi^(1/2)*x^5) + 1/(2*pi^(1/2)*x^3))*exp(-x^2)")
    );
    assert_eq!(ev(&mut e, "normal(series(sin_integral(z), z, inf, 3))"), ev(&mut e, "(1/2)*pi + (-1/z + 2/z^3)*cos(z) - sin(z)/z^2"));
    assert_eq!(ev(&mut e, "normal(series(zeta(x), x, inf, 6))"), "1 + 2^-x + 3^-x + 4^-x + 5^-x + 6^-x");
    // each expansion against the function at a large argument: the error is of the order of the first omitted term
    let pairs = [
        ("erfc(x)", "8.0", 5, 9),
        ("fresnel_c(x)", "12.0", 5, 7),
        ("fresnel_s(x)", "12.0", 5, 7),
        ("cos_integral(x)", "30.0", 6, 6),
        ("sin_integral(x)", "30.0", 6, 6),
        ("cosh_integral(x)", "30.0", 6, 5),
        ("exp_integral_e(5/2, x)", "40.0", 6, 9),
        ("gamma_regularized(3/2, x)", "40.0", 6, 9),
        ("hankel_h1(2, x)", "60.0", 6, 10),
        ("hankel_h2(1/3, x)", "60.0", 6, 10),
        ("spherical_hankel_h1(3, x)", "5", 5, 20),
        ("whittaker_w(2, 1/2, x)", "5", 5, 20),
        ("polylog(2, x)", "50.0", 8, 13),
        ("polylog(3, x)", "50.0", 8, 13),
        ("lerch_phi(x, 1, 2)", "40.0", 8, 13),
    ];
    for (f, x0, order, digits) in pairs {
        let s = format!("rewrite(normal(series({f}, x, inf, {order})), x -> {x0})");
        let direct = format!("rewrite({f}, x -> {x0})");
        assert!(close(&mut e, &s, &direct, digits), "{f} at {x0}");
    }
}

#[test]
fn function_expand_closed_forms_agree_with_the_functions() {
    let mut e = CoreEngine::new();
    // the forms
    assert_eq!(
        ev(&mut e, "function_expand(whittaker_m(k, m, x))"),
        ev(&mut e, "hypergeometric_1f1(1/2 + m - k, 1 + 2*m, x)*exp((-1/2)*x)*x^(1/2 + m)")
    );
    assert_eq!(ev(&mut e, "function_expand(subfactorial(x))"), ev(&mut e, "gamma(1 + x, -1)/e"));
    assert_eq!(ev(&mut e, "function_expand(polygamma(1, 1/4))"), ev(&mut e, "8*catalan + pi^2"));
    // every new rule: the expanded form at a point against the native value of the function (other algorithms)
    let cases = [
        ("whittaker_m(3/10, 7/10, x)", "7/10"),
        ("whittaker_m(2, -1/3, x)", "3/2"),
        ("whittaker_w(3/10, 7/10, x)", "7/10"),
        ("whittaker_w(-1/2, 1/2, x)", "2"),
        ("hankel_h1(1/3, x)", "3/2"),
        ("hankel_h2(2/5, x)", "5/2"),
        ("spherical_bessel_j(1/3, x)", "2"),
        ("spherical_bessel_y(5/2, x)", "3"),
        ("spherical_hankel_h1(1/2, x)", "3/2"),
        ("subfactorial(x)", "5/2"),
        ("inverse_elliptic_nome_q(x)", "3/10"),
        ("legendre_p(1/3, x)", "0"),
        ("legendre_q(1/3, x)", "0"),
        ("legendre_q(-7/4, x)", "0"),
        ("hypergeometric_0f1_regularized(5/2, x)", "7/10"),
        ("hypergeometric_0f1_regularized(-2, x)", "7/10"),
        ("spherical_harmonic_y(3, 0, x, 1)", "2/5"),
        ("anger_j(3, x)", "7/4"),
        ("weber_e(3, x)", "7/4"),
        ("weber_e(1, x)", "1/3"),
        ("zeta(-3, x)", "3/10"),
        ("gamma(9/2, x)", "13/10"),
        ("gamma(3/2, x)", "1/5"),
        ("alternating_factorial(x)", "29/20"),
        ("alternating_factorial(x)", "-13/10"),
    ];
    for (f, x0) in cases {
        let direct = format!("rewrite({f}, x -> n({x0}, 30))");
        let expanded = format!("rewrite(function_expand({f}), x -> {x0})");
        assert!(close(&mut e, &expanded, &direct, 18), "{f} at {x0}");
    }
    assert!(close(&mut e, "function_expand(elliptic_k(-1))", "elliptic_k(-1.0)", 14));
    assert!(close(&mut e, "function_expand(elliptic_k(1/2))", "elliptic_k(0.5)", 14));
    assert!(close(&mut e, "function_expand(polygamma(1, 3/4))", "polygamma(1, 0.75)", 14));
    // M with 1 + 2 mu a pole of the 1F1 form is left alone
    assert_eq!(ev(&mut e, "function_expand(whittaker_m(1/2, -1, x))"), "whittaker_m(1/2, -1, x)");
}

#[test]
fn parity_and_symmetric_parameters() {
    let mut e = CoreEngine::new();
    for (f, odd) in [
        ("jacobi_sn", true), ("jacobi_cn", false), ("jacobi_dn", false), ("jacobi_cd", false), ("jacobi_cs", true),
        ("jacobi_dc", false), ("jacobi_ds", true), ("jacobi_nc", false), ("jacobi_nd", false), ("jacobi_ns", true),
        ("jacobi_sc", true), ("jacobi_sd", true), ("neville_theta_s", true), ("neville_theta_c", false),
        ("neville_theta_d", false), ("neville_theta_n", false),
    ] {
        let s = if odd { "-" } else { "" };
        assert_eq!(ev(&mut e, &format!("{f}(-x, m) == {s}{f}(x, m)")), "true", "{f}");
        // the rule at an exact point against the machine value at the negative point
        assert!(close(&mut e, &format!("{f}(-3/10, 2/5)"), &format!("{f}(-0.3, 0.4)"), 13), "{f}");
    }
    assert_eq!(ev(&mut e, "carlson_rf(x, y, z) == carlson_rf(z, x, y)"), "true");
    assert_eq!(ev(&mut e, "carlson_rk(x, y) == carlson_rk(y, x)"), "true");
    assert_eq!(ev(&mut e, "hypergeometric_2f1(a, b, c, z) == hypergeometric_2f1(b, a, c, z)"), "true");
    assert_eq!(ev(&mut e, "hypergeometric_pfq([a1, a2], [b1, b2, b3], x) == hypergeometric_pfq([a2, a1], [b1, b3, b2], x)"), "true");
    // sorting never changes a value: a symbolic parameter substituted after the sort
    assert!(close(&mut e, "rewrite(hypergeometric_2f1(b, a, 3, 1/3), [a -> 1/2, b -> 2])", "hypergeometric_2f1(2.0, 0.5, 3.0, 1/3)", 13));
    assert!(close(&mut e, "rewrite(carlson_rf(z, x, 1), [x -> 2, z -> 3])", "carlson_rf(3.0, 2.0, 1.0)", 13));
    // Anger and Weber: J_nu(-z) = J_-nu(z), E_nu(-z) = -E_-nu(z), integer orders
    assert_eq!(ev(&mut e, "anger_j(0, -x) == anger_j(0, x)"), "true");
    assert_eq!(ev(&mut e, "anger_j(1, -x) == -anger_j(1, x)"), "true");
    assert!(close(&mut e, "anger_j(1/3, -2/3)", "anger_j(1/3, -0.6666666666666666)", 12));
    assert!(close(&mut e, "weber_e(1/3, -2/3)", "weber_e(1/3, -0.6666666666666666)", 12));
    assert!(close(&mut e, "weber_e(2, -1/3)", "weber_e(2, -0.3333333333333333)", 12));
    assert!(close(&mut e, "anger_j(-3, 5/7)", "anger_j(-3, 0.7142857142857143)", 12));
}

#[test]
fn closed_forms_at_exact_arguments() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "table(beta_regularized(z, 1, 2), z, range(1, 5))"), "[1, 0, -3, -8, -15]");
    assert_eq!(ev(&mut e, "beta_regularized(-z, 1, 1) == -beta_regularized(z, 1, 1)"), "true");
    let pairs = [
        ("beta_regularized(1/3, 2, 3)", "beta_regularized(0.3333333333333333, 2.0, 3.0)"),
        ("beta(2/5, 3, 2)", "beta(0.4, 3.0, 2.0)"),
        ("hypergeometric_u(1/3, 1/3, 2)", "hypergeometric_u(0.3333333333333333, 0.3333333333333333, 2.0)"),
        ("hypergeometric_u(1/2, 1/2, 1)", "hypergeometric_u(0.5, 0.5, 1.0)"),
        ("mittag_leffler_e(1/2, 3/4)", "mittag_leffler_e(0.5, 0.75)"),
        ("mittag_leffler_e(1/2, -2)", "mittag_leffler_e(0.5, -2.0)"),
        ("erf(2*im)", "erf(2.0*im)"),
        ("erfi(im*pi/2)", "erfi(1.5707963267948966*im)"),
        ("product_log(-pi/2)", "product_log(-1.5707963267948966)"),
        ("zeta(3, 5/2)", "zeta(3.0, 2.5)"),
        ("zeta(2, 3/2)", "zeta(2.0, 1.5)"),
        ("anger_j(2, 5/6)", "anger_j(2, 0.8333333333333334)"),
        ("weber_e(2, 5/6)", "weber_e(2, 0.8333333333333334)"),
        ("weber_e(0, 5/6)", "weber_e(0, 0.8333333333333334)"),
    ];
    for (a, b) in pairs {
        assert!(close(&mut e, a, b, 12), "{a}");
    }
    // inverses: the round trip through the function
    for (s, a, b) in [("1/3", 1, 2), ("2/3", 1, 4), ("1/5", 3, 1), ("1/2", 3, 3)] {
        assert!(close(&mut e, &format!("beta_regularized(inverse_beta_regularized({s}, {a}, {b}), {a}, {b})"), s, 25), "{s} {a} {b}");
    }
    assert_eq!(ev(&mut e, "inverse_gamma_regularized(1, 1/5)"), "log(5)");
    assert!(close(&mut e, "gamma_regularized(1, inverse_gamma_regularized(1, 2/7))", "2/7", 25));
    assert_eq!(ev(&mut e, "q_pochhammer(1)"), "0");
    // Kummer's transformation where it terminates: e^x times a polynomial, against the series at a point
    assert_eq!(ev(&mut e, "hypergeometric_1f1(3/2, 1/2, x)"), ev(&mut e, "(1 + 2*x)*exp(x)"));
    for (a, b) in [("5/2", "1/2"), ("7/3", "1/3"), ("4", "2")] {
        let f = format!("rewrite(hypergeometric_1f1({a}, {b}, x), x -> 7/10)");
        assert!(close(&mut e, &f, &format!("hypergeometric_1f1({a}, {b}, 0.7)"), 13), "{a} {b}");
    }
    // d/da gamma(a, z) in closed form, against a central difference at 40 digits
    assert!(close(
        &mut e,
        "rewrite(d(gamma(a, z), a), [a -> -29/20, z -> 1])",
        "(gamma(-29/20 + 10^-12, 1) - gamma(-29/20 - 10^-12, 1))/(2*10^-12)",
        18
    ));
    assert!(close(
        &mut e,
        "rewrite(d(gamma(a, z), a), [a -> 5/2, z -> 3/2])",
        "(gamma(5/2 + 10^-12, 3/2) - gamma(5/2 - 10^-12, 3/2))/(2*10^-12)",
        18
    ));
}
