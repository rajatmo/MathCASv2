//! Round 37: special-function inputs that had no answer. Each test carries an example input and
//! variations of its own (other numbers, complex or high-precision arguments, other orders), each checked by a route
//! other than the code under test: a closed-form identity, a second algorithm, or the defining equation.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate every statement in one fresh session (one worker thread, big stack); `TIMEOUT` after 120 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

/// A printed machine real.
fn num(s: &str) -> f64 {
    s.trim().parse().unwrap_or_else(|_| panic!("not a number: {s}"))
}

/// `abs(expr)` evaluated, as a machine real: the size of a difference that must vanish.
fn small(expr: &str, tol: f64) {
    let out = ev(&format!("n(abs({expr}))"));
    // a ball around 0 (every digit cancelled) prints as ball(0, r)
    let v = match out.strip_prefix("ball(0, ").and_then(|r| r.strip_suffix(')')) {
        Some(r) => num(r),
        None => num(&out),
    };
    assert!(v <= tol, "{expr}: {v} > {tol}");
}

// ---------------------------------------------------------------- numerical derivatives without a rule

/// The maximum of J_1(x) (the Anger function of integer order is the Bessel function), found through
/// the derivative. Checked by the zero of J_1' = (J_0 - J_2)/2, the known 1.8411837813406593.
#[test]
fn anger_maximum_by_find_root() {
    let x = num(&ev("rewrite(x, find_root(d(anger_j(1, x), x) == 0, [x, 2]))"));
    assert!((x - 1.841_183_781_340_659_3).abs() < 1e-12, "{x}");
    small(&format!("bessel_j(0, {x}) - bessel_j(2, {x})"), 1e-14);
}

/// The x-derivative rule of the Anger and Weber functions (DLMF 11.10.33-34) against the numerical derivative of
/// the function itself (a central difference at 40 digits).
#[test]
fn anger_weber_derivative_rule() {
    assert_eq!(ev("d(anger_j(n_u, z), z)"), "(1/2)*(anger_j(-1 + n_u, z) - anger_j(1 + n_u, z))");
    for f in ["anger_j", "weber_e"] {
        for (nu, z) in [("1/3", "5/2"), ("-7/4", "1/2 + 2*im")] {
            let rule = format!("rewrite(d({f}({nu}, x), x), x -> {z})");
            let diff = format!("({f}({nu}, {z} + 10^-12) - {f}({nu}, {z} - 10^-12))/(2*10^-12)");
            small(&format!("n({rule} - {diff}, 40)"), 1e-18);
        }
    }
}

/// The numerical derivative in a parameter (no rule): the order of the Bessel function against the closed form of
/// the order derivative at nu = 1/2 (DLMF 10.15.3: dJ_nu/dnu at 1/2 = sqrt(2/(pi x)) (Ci(2x) sin x - Si(2x) cos x)),
/// at machine precision, at 30 digits and at a second x.
#[test]
fn order_derivative_numerically() {
    let closed = |x: &str| format!("sqrt(2/(pi*{x}))*(cos_integral(2*{x})*sin({x}) - sin_integral(2*{x})*cos({x}))");
    // Bessel's order derivative (a rule of mc-calculus) is the independent check of the mixed partial below
    for x in ["2.0", "0.75"] {
        let v = ev(&format!("derivative(1, 0)(bessel_j)(0.5, {x})"));
        let c = ev(&format!("n({})", closed(x)));
        assert!((num(&v) - num(&c)).abs() < 1e-13, "{x}: {v} vs {c}");
    }
    // the parameter derivative of the Anger function has no rule: the numerical value against the integral
    // J_nu(x) = (1/pi) int_0^pi cos(nu t - x sin t) dt differentiated under the integral sign (DLMF 11.10.1)
    for (nu, x, nq, xq) in [("0.5", "2.0", "1/2", "2"), ("1.25", "3.5", "5/4", "7/2")] {
        let v = num(&ev(&format!("derivative(1, 0)(anger_j)({nu}, {x})")));
        let w = num(&ev(&format!("n(n_integrate(-t*sin({nq}*t - {xq}*sin(t))/pi, [t, 0, pi], working_precision: 30))")));
        assert!((v - w).abs() < 1e-13, "{nu}, {x}: {v} vs {w}");
    }
    let v = ev("n(derivative(1, 0)(anger_j)(1/2, 2), 30)");
    let w = ev("n_integrate(-t*sin(t/2 - 2*sin(t))/pi, [t, 0, pi], working_precision: 40)");
    small(&format!("{v} - ({w})"), 1e-28);
}

/// Mixed and higher partial derivatives: d^2/(dnu dz) J_nu(z) at a complex point against the order derivative of
/// the z-rule (J_(nu-1) - J_(nu+1))/2 (the rule route of the order derivative), and the third z-derivative of the
/// Anger function against its rule.
#[test]
fn mixed_and_higher_partials() {
    small("derivative(1, 1)(bessel_j)(1.5, 2.0 + im) - (derivative(1, 0)(bessel_j)(0.5, 2.0 + im) - derivative(1, 0)(bessel_j)(2.5, 2.0 + im))/2", 1e-12);
    small("derivative(0, 3)(anger_j)(1/3, 2.0) - rewrite(d(anger_j(1/3, x), [x, 3]), x -> 2.0)", 1e-11);
}

/// Functions with no derivative rule at all: factorial2, riemann_siegel_z,
/// q_pochhammer, neville_theta_c; the critical points found by find_root are checked by a central difference of
/// the function at 40 digits (an independent quotient at a much smaller step).
#[test]
fn critical_points_of_functions_without_rules() {
    let cases = [
        ("factorial2(x)", "0.2"),
        ("riemann_siegel_z(x)", "20"),
        ("q_pochhammer(x)", "-0.2"),
        ("neville_theta_c(x, 1/4)", "7"),
        ("whittaker_w(3, 1/2, x)", "0.5"),
    ];
    for (f, x0) in cases {
        let x = ev(&format!("rewrite(x, find_root(d({f}, x) == 0, [x, {x0}]))"));
        let xv = num(&x);
        let g = |at: String| f.replace('x', &at);
        let quotient = format!("({} - {})/(2*10^-15)", g(format!("(set_precision({x}, 60) + 10^-15)")), g(format!("(set_precision({x}, 60) - 10^-15)")));
        let size = num(&ev(&format!("n(abs({}))", g(x.clone())))).max(1.0);
        let flat = ev(&format!("abs({quotient}) < {size}*10^-9"));
        assert_eq!(flat, "true", "{f} at {xv}: {}", ev(&quotient));
    }
}

/// No value where the function has none: a step function's index argument, an unknown function.
#[test]
fn numerical_derivative_declines() {
    assert!(ev("derivative(1)(prime)(10.5)").contains("prime"));
    assert!(ev("derivative(1)(moebius_mu)(3.0)").contains("moebius_mu"));
    assert!(ev("derivative(1)(g)(1.5)").contains("g'"));
}

// ---------------------------------------------------------------- degenerate Weierstrass lattices

/// The Weierstrass functions at invariants with discriminant 0. Checked by the differential equation
/// p'^2 = 4 p^3 - g2 p - g3 at a complex point, by zeta' = -p and sigma'/sigma = zeta (central differences), and by
/// the lattice algorithm at invariants moved by 10^-12 (a second algorithm).
#[test]
fn weierstrass_degenerate() {
    assert_eq!(ev("weierstrass_p(1, [12, -8])"), "1 + 3*csch(3^(1/2))^2");
    for (g2, g3) in [("12", "-8"), ("3", "1"), ("12", "8"), ("27/4", "-27/8")] {
        let u = "0.7 + 0.2*im";
        let p = format!("weierstrass_p({u}, [{g2}, {g3}])");
        let dp = format!("weierstrass_p_prime({u}, [{g2}, {g3}])");
        small(&format!("{dp}^2 - (4*{p}^3 - {g2}*{p} - ({g3}))"), 1e-9);
        let z = |v: &str| format!("weierstrass_zeta({v}, [{g2}, {g3}])");
        let s = |v: &str| format!("weierstrass_sigma({v}, [{g2}, {g3}])");
        let (a, b) = ("(0.7 + 0.2*im + 10^-6)", "(0.7 + 0.2*im - 10^-6)");
        small(&format!("({} - {})/(2*10^-6) + {p}", z(a), z(b)), 1e-6);
        small(&format!("(log({}) - log({}))/(2*10^-6) - {}", s(a), s(b), z(u)), 1e-6);
        let near = format!("weierstrass_p({u}, [{g2}, {g3} + 10^-7])");
        small(&format!("{p} - {near}"), 1e-4);
    }
    // around values: first-order propagation through the closed form
    let r = ev("weierstrass_p(around(2, 0.01), [12, -8])");
    assert!(r.starts_with("around(1.01178018848396"), "{r}");
}

// ---------------------------------------------------------------- mathieu_c_prime

/// The maximum of ce through d(mathieu_c(3, 2, x), x) = mathieu_c_prime (a new member of the family,
/// round 37). Checked by a central difference of mathieu_c at 30 digits, at the example point, at a complex
/// point with complex parameters, and by the Mathieu equation (a central difference of mathieu_c_prime against
/// -(a - 2 q cos 2z) mathieu_c).
#[test]
fn mathieu_c_prime_values() {
    let x = num(&ev("find_root(d(mathieu_c(3, 2, x), x) == 0, [x, 0.5])[1, 2]"));
    assert!((x - 0.628_372_186_231_302_8).abs() < 1e-10, "{x}");
    let quotient = |a: &str, q: &str, z: &str| format!("(mathieu_c({a}, {q}, {z} + 10^-12) - mathieu_c({a}, {q}, {z} - 10^-12))/(2*10^-12)");
    small(&format!("n(mathieu_c_prime(3, 2, 1/2) - {}, 30)", quotient("3", "2", "1/2")), 1e-18);
    small(&format!("mathieu_c_prime(3, 2, {x})"), 1e-12);
    // complex parameters and argument (machine numbers against an exact-step quotient at 30 digits)
    let (a, q, z) = ("16/5", "3/2 + im", "2/5 + (3/10)*im");
    small(&format!("mathieu_c_prime(3.2, 1.5 + im, 0.4 + 0.3*im) - n({}, 30)", quotient(a, q, z)), 1e-12);
    // the equation: (c')' = -(a - 2 q cos 2z) c
    let dd = "(mathieu_c_prime(5/2, -1, 7/10 + 10^-12) - mathieu_c_prime(5/2, -1, 7/10 - 10^-12))/(2*10^-12)";
    small(&format!("n({dd} + (5/2 + 2*cos(7/5))*mathieu_c(5/2, -1, 7/10), 30)"), 1e-18);
    assert_eq!(ev("expand(d(mathieu_c(a, q, z), [z, 2]) + (a - 2*q*cos(2*z))*mathieu_c(a, q, z))"), "0");
    assert_eq!(ev("expand(mathieu_c_prime(a, 0, z) + sqrt(a)*sin(sqrt(a)*z))"), "0");
}

// ---------------------------------------------------------------- complex machine values for the contour plots

/// Relative difference of a machine value and the 30-digit ball value at the same point (the ball path: rigorous
/// series and connection formulas on balls, a separate implementation).
fn rel_to_balls(machine: &str, exact: &str) -> f64 {
    num(&ev(&format!("n(abs(({machine}) - n({exact}, 30))/abs(n({exact}, 30)))")))
}

/// 2F1 beyond |z| < 0.9 and the Pfaff disk by the connection formulas to 1/z, 1/(1 - z),
/// 1 - z (DLMF 15.8.2-4). Checked against the ball path, against Euler's transformation
/// F(a, b; c; z) = (1 - z)^(c - a - b) F(c - a, c - b; c; z), and the real z > 1 against its limit from below.
#[test]
fn hypergeometric_2f1_connection() {
    let pts = [("3.0 + 2.0*im", "3 + 2*im"), ("-4.0 + 0.5*im", "-4 + im/2"), ("1.3 - 0.4*im", "13/10 - 2*im/5"), ("0.9 + 0.1*im", "9/10 + im/10"), ("5.0", "5")];
    for (zm, zq) in pts {
        for (a, b, c, aq, bq, cq) in [("1", "1/2", "sqrt(2)", "1", "1/2", "sqrt(2)"), ("1/3 + im", "-2.5", "0.7", "1/3 + im", "-5/2", "7/10")] {
            let e = rel_to_balls(&format!("hypergeometric_2f1({a}, {b}, {c}, {zm})"), &format!("hypergeometric_2f1({aq}, {bq}, {cq}, {zq})"));
            assert!(e < 1e-12, "2F1({a}, {b}, {c}, {zm}): {e}");
        }
    }
    small("hypergeometric_2f1(0.3, 1.7, 2.2 + 0.5*im, 3.0 + 2.0*im) - (1 - (3.0 + 2.0*im))^(0.2 + 0.5*im)*hypergeometric_2f1(1.9 + 0.5*im, 0.5 + 0.5*im, 2.2 + 0.5*im, 3.0 + 2.0*im)", 1e-12);
    small("hypergeometric_2f1(1, 1/2, sqrt(2), 5.0) - hypergeometric_2f1(1, 1/2, sqrt(2), 5.0 - 1.0e-9*im)", 1e-8);
    let plot = ev("complex_contour_plot(re(hypergeometric_2f1(1, 1/2, sqrt(2), z)), [z, -5 - 5*im, 5 + 5*im], contours: 24)");
    assert!(plot.starts_with("graphics(") || plot.starts_with("image("), "{}", &plot[..plot.len().min(80)]);
}

/// Theta functions at complex machine arguments by their series. Checked against the ball path
/// at points with |Im u| up to 4, by Jacobi's identity theta_3^4 = theta_2^4 + theta_4^4 at u = 0, and by
/// theta_1'(0) = theta_2(0) theta_3(0) theta_4(0) (DLMF 20.4.6, 20.4.2) at a complex nome.
#[test]
fn theta_machine_series() {
    for k in 1..=4 {
        for (um, uq) in [("0.3 + 0.2*im", "3/10 + im/5"), ("-2.5 + 3.5*im", "-5/2 + 7*im/2"), ("1.1 - 4.0*im", "11/10 - 4*im")] {
            for f in ["elliptic_theta", "elliptic_theta_prime"] {
                let e = rel_to_balls(&format!("{f}({k}, {um}, 0.4 + 0.5*im)"), &format!("{f}({k}, {uq}, 2/5 + im/2)"));
                assert!(e < 1e-12, "{f}({k}, {um}): {e}");
            }
        }
    }
    let t = |k: u8| format!("elliptic_theta({k}, 0.0, 0.3 + 0.4*im)");
    small(&format!("{}^4 - {}^4 - {}^4", t(3), t(2), t(4)), 1e-13);
    small(&format!("elliptic_theta_prime(1, 0.0, 0.3 + 0.4*im) - {}*{}*{}", t(2), t(3), t(4)), 1e-13);
}

/// pFq at complex machine arguments (the series) and at complex balls (the rigorous series),
/// checked against the partial sum written out at 30 digits; 3F2 outside the unit disk stays unevaluated.
#[test]
fn pfq_complex() {
    let direct = "n(sum(pochhammer(1, k)^2/pochhammer(3, k)^3*(-39/4 + 13*im/4)^k/factorial(k), [k, 0, 150]), 30)";
    small(&format!("hypergeometric_pfq([1, 1], [3, 3, 3], -9.75 + 3.25*im) - {direct}"), 1e-14);
    small(&format!("n(hypergeometric_pfq([1, 1], [3, 3, 3], -39/4 + 13*im/4), 30) - {direct}"), 1e-28);
    let direct2 = "n(sum(pochhammer(1/2, k)*pochhammer(1/3, k)*pochhammer(1/4, k)/(pochhammer(1/5, k)*pochhammer(1/6, k))*(1/2 + im/3)^k/factorial(k), [k, 0, 400]), 30)";
    small(&format!("n(hypergeometric_pfq([1/2, 1/3, 1/4], [1/5, 1/6], 1/2 + im/3), 25) - {direct2}"), 1e-23);
    small(&format!("hypergeometric_pfq([0.5, 1/3, 1/4], [1/5, 1/6], 0.5 + 0.5*im/1.5) - {direct2}"), 1e-13);
    assert!(ev("n(hypergeometric_pfq([1/2, 1/3, 1/4], [1/5, 1/6], 2 + im/3), 25)").contains("hypergeometric_pfq"));
}

// ---------------------------------------------------------------- closed forms: Lommel, Mittag-Leffler

/// The Lommel functions s, S, t, T at mu = nu (Struve), s and S at mu = 0, -1 (Anger and Weber), S with a
/// terminating expansion. Checked against the series and connection-formula paths at a real and a complex point.
#[test]
fn lommel_closed_forms() {
    assert_eq!(ev("expand(lommel_s2(2, 5, z) - z*(1 + 24/z^2 + 576/z^4 + 9216/z^6))"), "0");
    assert!(ev("lommel_s1(n_u, n_u, z)").contains("struve_h"));
    let cases = [
        "lommel_s1(2/3, 2/3, z)", "lommel_s2(2/3, 2/3, z)", "lommel_t1(2/3, 2/3, z)", "lommel_t2(2/3, 2/3, z)", "lommel_t1(5/4, -5/4, z)",
        "lommel_s2(3/2, 3/2, z)", "lommel_s2(3/2, 1/2, z)", "lommel_s2(9/2, 1/2, z)",
        // mu = 0, -1 at a symbolic order (the order substituted afterwards)
        "lommel_s1(-1, n_u, z)", "lommel_s1(0, n_u, z)", "lommel_s2(-1, n_u, z)", "lommel_s2(0, n_u, z)",
    ];
    for c in cases {
        let closed = ev(c);
        assert!(!closed.starts_with("lommel"), "{c}: {closed}");
        for (zm, zq) in [("1.7", "17/10"), ("2.5 + 0.75*im", "5/2 + 3*im/4")] {
            for (nm, nq) in [("0.3333333333333333", "1/3"), ("1.4", "7/5")] {
                let numeric = c.replace("n_u", nm).replace('z', zm);
                small(&format!("rewrite({closed}, [n_u -> {nq}, z -> {zq}]) - ({numeric})"), 1e-12);
            }
        }
    }
    // a numeric order keeps the series (the faster and more accurate evaluation)
    assert_eq!(ev("lommel_s1(0, 1/3, z)"), "lommel_s1(0, 1/3, z)");
}

/// Mittag-Leffler E_(m,b) with integer m and b as a sum of m exponentials, E_(1,b) by the incomplete
/// gamma function; checked against the series at complex points (the numbers path sums the series itself).
#[test]
fn mittag_leffler_closed_forms() {
    small("n(mittag_leffler_e(2, 2, im*pi)) - sinh(sqrt(n(im*pi)))/sqrt(n(im*pi))", 1e-14);
    for (m, b) in [("4", "-1"), ("3", "2"), ("2", "0"), ("5", "3")] {
        let closed = ev(&format!("mittag_leffler_e({m}, {b}, z)"));
        assert!(!closed.starts_with("mittag"), "{m}, {b}: {closed}");
        small(&format!("rewrite({closed}, z -> 7/5 + 2*im/3) - mittag_leffler_e({m}, {b}, 1.4 + 0.6666666666666666*im)"), 1e-12);
    }
    let one = ev("mittag_leffler_e(1, b, x)");
    small(&format!("rewrite({one}, [b -> 5/2, x -> 3/2]) - mittag_leffler_e(1, 2.5, 1.5)"), 1e-13);
    small(&format!("rewrite({one}, [b -> 7/3, x -> 1/2 + im]) - mittag_leffler_e(1, 7/3, 0.5 + 1.0*im)"), 1e-13);
}

/// Integer relations of complex numbers (both parts at once): eta1 + eta2 + eta3 = 0, a
/// relation at 30 digits, and none for numbers without one.
#[test]
fn complex_integer_relations() {
    assert_eq!(ev("{ var g2 = 5; var g3 = 1.~20; find_integer_null_vector([weierstrass_eta1([g2, g3]), weierstrass_eta2([g2, g3]), weierstrass_eta3([g2, g3])]) }"), "[1, 1, 1]");
    assert_eq!(ev("{ var g2 = 5; var g3 = 1.~20; find_integer_null_vector([weierstrass_half_period_w1([g2, g3]), weierstrass_half_period_w2([g2, g3]), weierstrass_half_period_w3([g2, g3])]) }"), "[1, 1, 1]");
    assert_eq!(ev("find_integer_null_vector([n(1 + 2*im, 30), n(sqrt(2) + im, 30), n(1 - sqrt(2) + im, 30)])"), "[1, -1, -1]");
    assert!(ev("find_integer_null_vector([n(1 + 2*im, 30), n(sqrt(2) + pi*im, 30), n(e + im, 30)])").contains("failure"));
}

// ---------------------------------------------------------------- double-double machine paths

/// The q-product and the basic hypergeometric series in
/// double-double numbers. Checked against the ball path at complex points and by the q-binomial theorem
/// 1phi0(a; -; q, z) = (a z; q)_inf/(z; q)_inf (DLMF 17.2.37).
#[test]
fn q_functions_machine() {
    for (zm, zq) in [("-2.5 + 3.5*im", "-5/2 + 7*im/2"), ("3.9 - 3.9*im", "39/10 - 39*im/10"), ("0.2 + 0.1*im", "1/5 + im/10")] {
        for (f, g) in [
            (format!("q_pochhammer({zm}, 1/2)"), format!("q_pochhammer({zq}, 1/2)")),
            (format!("q_factorial({zm}, 1/2)"), format!("q_factorial({zq}, 1/2)")),
            (format!("q_hypergeometric_pfq([1/2], [1/3, 1/4], 3/5, {zm})"), format!("q_hypergeometric_pfq([1/2], [1/3, 1/4], 3/5, {zq})")),
        ] {
            let e = rel_to_balls(&f, &g);
            assert!(e < 1e-14, "{f}: {e}");
        }
    }
    small("q_hypergeometric_pfq([0.3 + 0.4*im], [], 0.6 - 0.2*im, 0.5 + 0.1*im) - q_pochhammer((0.3 + 0.4*im)*(0.5 + 0.1*im), 0.6 - 0.2*im)/q_pochhammer(0.5 + 0.1*im, 0.6 - 0.2*im)", 1e-14);
    assert_eq!(ev("q_hypergeometric_pfq([0.7], [1/3], 0.2, 5)"), "0.3720811428938267");
}

/// The lattice functions e_k, eta_k and the half-periods at machine invariants through the
/// double-double lattice. Checked against the ball path, by e1 + e2 + e3 = 0 and by Legendre's relation
/// eta1 omega3 - eta3 omega1 = i pi/2 (DLMF 23.2.14).
#[test]
fn lattice_machine() {
    for (gm, gq) in [("[2.0, 3.0]", "[2, 3]"), ("[-3.0, 5.0]", "[-3, 5]"), ("[1.0, -2.0]", "[1, -2]"), ("[3.0 + im, 0.5 - 2.0*im]", "[3 + im, 1/2 - 2*im]")] {
        for f in ["weierstrass_e1", "weierstrass_e3", "weierstrass_eta1", "weierstrass_eta2", "weierstrass_half_period_w1", "weierstrass_half_period_w3"] {
            small(&format!("{f}({gm}) - n({f}({gq}), 30)"), 1e-14);
        }
        small(&format!("weierstrass_e1({gm}) + weierstrass_e2({gm}) + weierstrass_e3({gm})"), 1e-14);
        let legendre = format!("weierstrass_eta1({gm})*weierstrass_half_period_w3({gm}) - weierstrass_eta3({gm})*weierstrass_half_period_w1({gm})");
        let v = ev(&format!("n(abs(abs({legendre}) - pi/2))"));
        assert!(num(&v) < 1e-13, "{gm}: {v}");
    }
}

/// s_(mu, nu) at machine arguments in double-double numbers, against the ball path.
#[test]
fn lommel_machine() {
    for (zm, zq) in [("1.7", "17/10"), ("-2.5 + 3.5*im", "-5/2 + 7*im/2"), ("3.9 - 3.9*im", "39/10 - 39*im/10"), ("-3.9", "-39/10")] {
        for (p, pq) in [("4/3, 1/2", "4/3, 1/2"), ("0.25 + 0.5*im, 2.0", "1/4 + im/2, 2")] {
            let e = rel_to_balls(&format!("lommel_s1({p}, {zm})"), &format!("lommel_s1({pq}, {zq})"));
            assert!(e < 1e-14, "lommel_s1({p}, {zm}): {e}");
        }
    }
}

/// Struve H and L at machine arguments as s_(nu,nu)/c and t_(nu,nu)/c in double-double numbers;
/// checked against the ball path (the series on balls) at real, negative and complex points.
#[test]
fn struve_machine() {
    for f in ["struve_h", "struve_l"] {
        for (nu, nq) in [("1", "1"), ("0.25", "1/4"), ("2.5", "5/2"), ("-0.75", "-3/4")] {
            for (zm, zq) in [("1.7", "17/10"), ("0.3 + 2.0*im", "3/10 + 2*im"), ("-3.2 - 1.1*im", "-16/5 - 11*im/10")] {
                let e = rel_to_balls(&format!("{f}({nu}, {zm})"), &format!("{f}(set_precision({nq}, 50), set_precision({zq}, 50))"));
                assert!(e < 1e-14, "{f}({nu}, {zm}): {e}");
            }
        }
    }
}
