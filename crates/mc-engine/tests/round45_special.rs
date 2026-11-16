//! Special functions in depth, second part. Each new exact form or enclosure is checked against an independent
//! numerical value: another algorithm for the same function (the machine and ball kernels against a closed form, the
//! Heun solver against a hypergeometric reduction, a series summed term by term), a defining relation (Wronskians,
//! round trips through an inverse function), or points sampled inside an enclosure at 30 digits.
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

/// Does the centered interval `r` (a variable holding a real or complex one) contain the value `v` (30 digits)?
fn inside(e: &mut CoreEngine, r: &str, v: &str) -> bool {
    let src = format!(
        "{{ let c = part({r}, 1); let w = part({r}, 2); let y = n({v}, 30); \
         abs(re(y) - re(c)) <= re(w) and abs(im_part(y) - im_part(c)) <= (if im_part(w) == 0 then re(w) else im_part(w)) }}"
    );
    let out = ev(e, &src);
    assert!(out == "true" || out == "false", "{src} gave {out}");
    out == "true"
}

#[test]
fn bessel_functions_of_half_integer_order_are_elementary() {
    let mut e = CoreEngine::new();
    let j = ev(&mut e, "bessel_j(3/2, x)");
    assert!(j.contains("sin(x)") && j.contains("cos(x)") && !j.contains("bessel"), "{j}");
    let i = ev(&mut e, "bessel_i(-5/2, x)");
    assert!(i.contains("sinh(x)") && i.contains("cosh(x)") && !i.contains("bessel"), "{i}");
    // every sign and kind against the ball kernels at a ball order (another algorithm: the series)
    for nu in ["7/2", "-7/2", "9/2", "-5/2", "21/2"] {
        for f in ["bessel_j", "bessel_y", "bessel_i"] {
            let exact = format!("{f}({nu}, 13/10)");
            let ball = format!("{f}(n({nu}, 30) + 10^-40, n(13/10, 30))");
            assert!(!ev(&mut e, &exact).contains(f), "{exact} not elementary");
            assert!(close(&mut e, &exact, &ball, 20), "{exact}");
        }
    }
    // the machine kernels at a negative argument's principal value are untouched; a symbolic large order stays
    assert!(ev(&mut e, "bessel_j(201/2, x)").contains("sin"));
    assert_eq!(ev(&mut e, "bessel_j(203/2, x)"), "bessel_j(203/2, x)");
}

#[test]
fn heun_functions_at_singular_points_and_as_hypergeometric_functions() {
    let mut e = CoreEngine::new();
    // the defining local solution does not exist or is not unique, or z is a singular point
    for src in [
        "heun_g(6/5, 9/10, -7/5, 3/25, 3/100, -1/5, 1)",
        "heun_g(6/5, 9/10, -7/5, 3/25, 3/100, -1/5, 6/5)",
        "heun_g(6/5, 9/10, -7/5, 3/25, -3, -1/5, 1/5)",
        "heun_g(0, 9/10, -7/5, 3/25, 3/100, -1/5, 1/5)",
        "heun_c(6/5, -7/5, 0, 3/100, -1/5, 1/5)",
        "heun_c(6/5, -7/5, 3/25, 3/100, -1/5, 1)",
        "heun_d(6/5, -7/5, 3/25, 3/100, -1/5, 0)",
        "heun_b(1, 2, -2, 1, 1, 1/2)",
    ] {
        assert_eq!(ev(&mut e, src), "indeterminate", "{src}");
    }
    // Maier's quadratic reduction against the Heun solver with q moved off the reduction by 10^-13
    let (al, be, ga) = ("(-0.6 + 0.9*im)", "(-0.7*im)", "(-0.18 - 0.03*im)");
    let de = format!("(1 + {al} + {be} - 2*{ga})");
    let red = format!("heun_g(2, {al}*{be}, {al}, {be}, {ga}, {de}, 0.3)");
    let solver = format!("heun_g(2, {al}*{be} + 10^-13, {al}, {be}, {ga}, {de}, 0.3)");
    assert!(close(&mut e, &red, "0.3528710781635568 - 0.36798511018752456*im", 13), "{red}");
    assert!(close(&mut e, &red, &solver, 10));
    assert!(close(&mut e, "heun_g(2, 3/35, 3/5, 1/7, 1/3, 113/105, 0.45)", "heun_g(2, 3/35 + 10^-13, 3/5, 1/7, 1/3, 113/105, 0.45)", 10));
    assert!(ev(&mut e, "heun_g(2, 3/35, 3/5, 1/7, 1/3, 113/105, z)").starts_with("hypergeometric_2f1"));
    // the biconfluent function with alpha = epsilon = 0 by Kummer's and the Bessel-type 0F1
    assert_eq!(ev(&mut e, "heun_b(q, 0, g, -1, 0, z)"), "hypergeometric_1f1(q, g, z)");
    assert!(close(&mut e, "heun_b(0.3, 0, 0.7, 2, 0, 0.4)", "heun_b(0.3, 0, 0.7, 2, 10^-14, 0.4)", 10));
    assert!(close(&mut e, "heun_b(0.3, 0, 0.7, 0, 0, 0.4)", "heun_b(0.3, 0, 0.7, 10^-14, 0, 0.4)", 10));
    assert_eq!(ev(&mut e, "hypergeometric_0f1(a, z) == heun_b(1, 0, a, 0, 0, z)"), "true");
}

#[test]
fn kummer_functions_with_half_integer_parameters_are_bessel_functions() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "hypergeometric_1f1(3/2, 2, z)"), "(bessel_i(0, (1/2)*z) + bessel_i(1, (1/2)*z))*exp((1/2)*z)");
    // against the series summed term by term (exact rationals, 90 terms at |z| <= 2)
    for (a, b, z) in [("3/2", "2", "7/10"), ("5/2", "4", "-3/2"), ("5/2", "5", "2"), ("7/2", "7", "-1/3")] {
        let closed = format!("hypergeometric_1f1({a}, {b}, {z})");
        assert!(ev(&mut e, &closed).contains("bessel_i"), "{closed}");
        let series = format!("sum(pochhammer({a}, k)/pochhammer({b}, k)*({z})^k/factorial(k), [k, 0, 90])");
        assert!(close(&mut e, &closed, &series, 25), "{closed}");
    }
    assert_eq!(ev(&mut e, "hypergeometric_1f1(5/2, 4, 0)"), "1");
}

#[test]
fn values_the_soundness_pass_left() {
    let mut e = CoreEngine::new();
    // erf far out: within 2^-(p+20) of +-1, without e^(-x^2)
    assert!(ev(&mut e, "n(erf(10^8), 30)").starts_with("1.0000000000000000000000000000"));
    assert!(ev(&mut e, "n(erf(-10^8), 30)").starts_with("-1.0000000000000000000000000000"));
    assert!(close(&mut e, "erf(30)", "1 - erfc(30)", 28));
    // the Hurwitz zeta function at an arbitrary-precision negative integer: -B_2(4)/2 = -73/12
    assert!(close(&mut e, "zeta(n(-1, 40), n(4, 40))", "-73/12", 28));
    assert!(close(&mut e, "zeta(n(-3, 40), 5/2)", "-bernoulli_b(4, 5/2)/4", 28));
    // the Hankel functions are infinite at 0, so a series there is a Laurent series
    assert_eq!(ev(&mut e, "hankel_h1(1, 0)"), "complex_infinity");
    // J(tau + n) = J(tau): the symbolic rule against values at a point and its shifts
    assert_eq!(ev(&mut e, "klein_invariant_j(x + 1) == klein_invariant_j(x)"), "true");
    assert_eq!(ev(&mut e, "klein_invariant_j(t - 3)"), "klein_invariant_j(t)");
    assert!(close(&mut e, "klein_invariant_j(3/10 + 6/5*im + 2)", "klein_invariant_j(0.3 + 1.2*im)", 12));
    let s = ev(&mut e, "normal(series(hankel_h1(2, x), x, 0, 3) - series(bessel_j(2, x), x, 0, 3) - im*series(bessel_y(2, x), x, 0, 3))");
    assert_eq!(s, "0");
}

#[test]
fn centered_intervals_with_complex_parts_are_enclosed() {
    let mut e = CoreEngine::new();
    // each box sampled at its centre, its corners and the middles of its sides, at 30 digits
    let cases = [
        ("fresnel_c", "1 + im", "(1 + im)/10^6"),
        ("fresnel_s", "1/2 - im", "(1 + im)/10^4"),
        ("fresnel_f", "1 + im", "(1 + im)/10^6"),
        ("fresnel_g", "1 + im", "(1 + im)/10^6"),
        ("zeta", "2 - 5*im", "(1 + im)/100"),
        ("hankel_h1(1, &)", "2/3", "1/100"),
        ("hankel_h2(3/2, &)", "2 + im", "1/50"),
        ("spherical_hankel_h1(1/4, &)", "2", "1/100"),
        ("gamma(2/3, &)", "-3 + im", "(1 + im)/100"),
        ("gamma_regularized(2/5, &)", "-3 - im", "(1 + im)/100"),
        ("pochhammer(&, 3/5)", "5 + 3*im", "(1 + im)/1000"),
        ("subfactorial", "1/2", "1/100"),
        ("elliptic_k", "4", "1/100"),
        ("erf", "1/2 + im/2", "1/20"),
    ];
    for (f, c, r) in cases {
        let call = |arg: &str| if f.contains('&') { f.replace('&', arg) } else { format!("{f}({arg})") };
        ev(&mut e, &format!("res = {}", call(&format!("centered_interval({c}, {r})"))));
        let res = ev(&mut e, "res");
        assert!(res.starts_with("centered_interval("), "{f}: {res}");
        // a real radius of a real centre has no imaginary spread
        let real_centre = !c.contains("im");
        let (rx, ry) = (format!("re({r})"), if real_centre { "0".to_string() } else if r.contains("im") { format!("im_part({r})") } else { r.to_string() });
        for (sx, sy) in [(0, 0), (1, 1), (1, -1), (-1, 1), (-1, -1), (1, 0), (0, 1), (-1, 0), (0, -1)] {
            let p = format!("({c}) + ({sx})*{rx} + ({sy})*{ry}*im");
            assert!(inside(&mut e, "res", &call(&format!("set_precision({p}, 40)"))), "{f} at {p}: {res}");
        }
    }
    // tighter than the natural ball extension: the mean value form (radius of order |f'| r)
    ev(&mut e, "res = pochhammer(centered_interval(5 + 3*im, (1 + im)/1000), 3/5)");
    assert_eq!(ev(&mut e, "re(part(res, 2)) < 0.001"), "true");
    // a pole inside: no enclosure
    assert!(ev(&mut e, "gamma(centered_interval(im/10, 1/10))").starts_with("gamma(centered_interval("));
}

#[test]
fn ferrers_functions_of_any_order_on_balls_and_intervals() {
    let mut e = CoreEngine::new();
    // the Wronskian P Q' - P' Q = Gamma(nu + mu + 1)/(Gamma(nu - mu + 1)(1 - x^2)) (DLMF 14.2.4) checks the new
    // derivative in x and the ball form of Q together
    // (5/2, 1/2): nu + mu an integer, Q by P of the orders +-mu (DLMF 14.9.2)
    for (nu, mu, x) in [("1/2", "2/3", "3/10"), ("1/3", "-1/4", "-1/2"), ("5/2", "1/3", "7/10"), ("5/2", "1/2", "2/5")] {
        let p = |t: &str| format!("legendre_p({nu}, {mu}, {t})");
        let q = |t: &str| format!("legendre_q({nu}, {mu}, {t})");
        let w = format!(
            "(({}*d({}, t) - d({}, t)*{}) /. t -> {x})",
            p("t"), q("t"), p("t"), q("t")
        );
        let want = format!("gamma({nu} + {mu} + 1)/(gamma({nu} - {mu} + 1)*(1 - ({x})^2))");
        assert!(close(&mut e, &w, &want, 20), "{nu}, {mu}, {x}");
    }
    // the image of an interval holds the values at its points
    ev(&mut e, "r = legendre_q(1/2, 2/3, interval([0.4, 0.5]))");
    // (the machine ends 0.4 and 0.5 as their exact binary values)
    for x in ["set_precision(0.4, 40)", "9/20", "1/2", "43/100"] {
        let v = format!("n(legendre_q(1/2, 2/3, {x}), 25)");
        assert_eq!(ev(&mut e, &format!("min(r) <= {v} and {v} <= max(r)")), "true", "{x}");
    }
    // complex degree and order with nu + mu = 3, against 14.9.10 at a nearby order (mu moved by 10^-7; the division
    // by sin((nu + mu) pi) there costs 7 digits, the move about 6)
    let a = "legendre_q(1 - im, 2 + im, 0.5)";
    assert!(close(&mut e, a, "9.48659328396923 + 5.441832617637276*im", 12), "{}", ev(&mut e, a));
    assert!(close(&mut e, a, "legendre_q(1 - im, 2 + im + 10^-7, 0.5)", 4));
    // integer degree and order by the derivatives of Q_n, against the Wronskian (n + m)!/((n - m)! (1 - x^2))
    assert!(ev(&mut e, "legendre_q(2, 2, 0.5)").starts_with("4.06927215808"));
    for (n, m) in [("2", "1"), ("3", "2"), ("4", "3")] {
        let w = format!(
            "((legendre_p({n}, {m}, t)*d(legendre_q({n}, {m}, t), t) - d(legendre_p({n}, {m}, t), t)*legendre_q({n}, {m}, t)) /. t -> 1/3)"
        );
        let want = format!("factorial({n} + {m})/(factorial({n} - {m})*(1 - 1/9))");
        assert!(close(&mut e, &w, &want, 25), "{n}, {m}");
    }
}

#[test]
fn hypergeometric_pfq_of_intervals_and_the_inverse_nome() {
    let mut e = CoreEngine::new();
    ev(&mut e, "r = hypergeometric_pfq([1/2, 2/3, 3/4], [4/5, 5/6], interval([0.11, 0.12]))");
    assert!(ev(&mut e, "r").starts_with("interval(["));
    for x in ["set_precision(0.11, 30)", "23/200", "set_precision(0.12, 30)"] {
        let v = format!("n(hypergeometric_pfq([1/2, 2/3, 3/4], [4/5, 5/6], {x}), 30)");
        assert_eq!(ev(&mut e, &format!("min(r) <= {v} and {v} <= max(r)")), "true", "{x}");
    }
    // the inverse nome on real balls, negative nomes included: real, and the nome of the result is q again
    assert!(close(&mut e, "elliptic_nome_q(inverse_elliptic_nome_q(-1/2))", "-1/2", 25));
    assert!(close(&mut e, "elliptic_nome_q(inverse_elliptic_nome_q(7/10))", "7/10", 25));
    assert_eq!(ev(&mut e, "im_part(n(inverse_elliptic_nome_q(-1/2), 30))"), "0");
    ev(&mut e, "r = inverse_elliptic_nome_q(interval([-0.6, -0.4]))");
    for x in ["set_precision(-0.6, 30)", "-1/2", "set_precision(-0.4, 30)"] {
        let v = format!("n(inverse_elliptic_nome_q({x}), 30)");
        assert_eq!(ev(&mut e, &format!("min(r) <= {v} and {v} <= max(r)")), "true", "{x}");
    }
}

#[test]
fn zernike_polynomials_continued_off_the_integers() {
    let mut e = CoreEngine::new();
    // the integer case (3 r^3 - 2 r, 0 for odd n - m) from machine arguments
    assert!(close(&mut e, "zernike_r(3.0, 1.0, 0.5)", "3*(1/2)^3 - 2*(1/2)", 14));
    assert!(close(&mut e, "zernike_r(3.0, 2.0, 0.5)", "0", 14));
    // off the integers against the hypergeometric series summed term by term
    let series = "cos(-7/30*pi)*(1/7)^(2/3)*binomial(13/30, -7/30)*sum(pochhammer(7/30, k)*pochhammer(43/30, k)/pochhammer(5/3, k)*(1/49)^k/factorial(k), [k, 0, 40])";
    assert!(close(&mut e, "zernike_r(1/5, 2/3, n(1/7, 40))", series, 25));
    assert!(ev(&mut e, "zernike_r(-7, 1, 0.6)").starts_with("0.0143999999"));
}

#[test]
fn spherical_harmonics_of_any_order_and_terminating_series_without_cancellation() {
    let mut e = CoreEngine::new();
    // a machine value against the same function at 30 digits (exact arguments, the ball kernels)
    assert!(close(&mut e, "spherical_harmonic_y(1, 0.5, -5, pi/3)", "spherical_harmonic_y(1, n(1/2, 30), -5, pi/3)", 13));
    // a terminating 2F1 whose terms are 10^8 times its value: the machine sum lost 8 digits, the ball path does not
    assert!(close(&mut e, "spherical_harmonic_y(23.0, 5.0 - 1.0*im, pi/3, im + pi)", "spherical_harmonic_y(23, n(5 - im, 40), pi/3, im + pi)", 12));
    assert!(close(&mut e, "legendre_p(23, 5 - im, 0.5)", "legendre_p(23, 5 - im, n(1/2, 30))", 12));
    assert!(close(&mut e, "hypergeometric_2f1(-23, 24, -4 + im, 0.25)", "hypergeometric_2f1(-23, 24, -4 + im, 1/4)", 12));
}
