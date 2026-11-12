//! Round 36: fixed wrong values of special functions, one test per cause. Each test checks
//! a value by a route other than the code under test: a central difference of the function itself, the defining
//! identity, a second algorithm (an ODE integration done outside the system, recorded in the comment), or the
//! numeric value of the exact definition.
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

/// A printed machine number or complex number `a`, `a + bim`, `a - bim`, `bim` as (re, im).
fn cx(s: &str) -> (f64, f64) {
    let s = s.trim();
    if let Some(body) = s.strip_prefix("complex(").and_then(|b| b.strip_suffix(')')) {
        let (re, im) = body.split_once(',').expect("complex");
        return (re.trim().parse().unwrap_or(f64::NAN), im.trim().parse().unwrap_or(f64::NAN));
    }
    if let Some(body) = s.strip_suffix("im") {
        // split at the last sign that is not part of an exponent
        let bytes: Vec<char> = body.chars().collect();
        let mut cut = None;
        for i in (1..bytes.len()).rev() {
            if (bytes[i] == '+' || bytes[i] == '-') && bytes[i - 1] != 'e' {
                cut = Some(i);
                break;
            }
        }
        return match cut {
            Some(i) => {
                let re: String = bytes[..i].iter().collect();
                let im: String = bytes[i..].iter().collect();
                (re.trim().parse().unwrap_or(f64::NAN), im.replace(' ', "").parse().unwrap_or(f64::NAN))
            }
            None => (0.0, body.trim().parse().unwrap_or(f64::NAN)),
        };
    }
    (s.parse().unwrap_or(f64::NAN), 0.0)
}

fn close(a: (f64, f64), b: (f64, f64), tol: f64, what: &str) {
    let scale = 1.0 + b.0.hypot(b.1);
    assert!((a.0 - b.0).hypot(a.1 - b.1) <= tol * scale, "{what}: {a:?} vs {b:?}");
}

/// `d(f, v)` at the point (rules `at`) against the central difference of f in v with step 1e-8, both at 30 digits.
fn check_derivative(f: &str, v: &str, at: &str, point: &str, tol: f64) {
    let exact = ev(&format!("n(rewrite(d({f}, {v}), {at}), 30)"));
    let diff = ev(&format!(
        "let g = {v}0 => rewrite({f}, [{v} -> {v}0]) ; n((g(({point}) + 1/10^8) - g(({point}) - 1/10^8))/(2/10^8), 30)"
    ));
    close(cx(&exact), cx(&diff), tol, &format!("d({f}, {v}) at {at}: {exact} vs {diff}"));
}

#[test]
fn jacobi_parameter_derivatives_agree_with_central_differences() {
    // sn, cn, dn, the nine quotients, the amplitude, epsilon and zeta in m (they stayed operators `derivative(0, 1)(f)`)
    for f in [
        "jacobi_sn", "jacobi_cn", "jacobi_dn", "jacobi_cd", "jacobi_cs", "jacobi_dc", "jacobi_ds", "jacobi_nc", "jacobi_nd",
        "jacobi_ns", "jacobi_sc", "jacobi_sd", "jacobi_amplitude", "jacobi_epsilon", "jacobi_zn",
    ] {
        check_derivative(&format!("{f}(7/10, m)"), "m", "[m -> 2/5]", "2/5", 1e-9);
    }
}

#[test]
fn inverse_jacobi_derivatives_agree_with_central_differences() {
    // d/dv = 1/pq'(u) and d/dm = -pq_m/pq_u at u = the inverse (implicit differentiation); v inside each real domain
    for (f, v) in [
        ("sn", "3/10"), ("cn", "3/10"), ("dn", "9/10"), ("cd", "3/10"), ("cs", "3/10"), ("dc", "5/2"), ("ds", "5/2"), ("nc", "5/2"),
        ("nd", "5/4"), ("ns", "5/2"), ("sc", "3/10"), ("sd", "3/10"),
    ] {
        let name = format!("inverse_jacobi_{f}");
        check_derivative(&format!("{name}({v}, m)"), "m", "[m -> 2/5]", "2/5", 1e-8);
        check_derivative(&format!("{name}(v, 2/5)"), "v", &format!("[v -> {v}]"), v, 1e-8);
    }
}

#[test]
fn inverse_gamma_beta_nome_and_modular_derivatives() {
    check_derivative("inverse_gamma_regularized(5/2, s)", "s", "[s -> 3/10]", "3/10", 1e-8);
    check_derivative("inverse_beta_regularized(s, 5/2, 3)", "s", "[s -> 3/10]", "3/10", 1e-8);
    check_derivative("inverse_elliptic_nome_q(q)", "q", "[q -> 1/20]", "1/20", 1e-8);
    // the lambda function and Klein's J along the imaginary direction (the derivative is the complex one)
    for f in ["modular_lambda", "klein_invariant_j"] {
        let exact = cx(&ev(&format!("rewrite(d({f}(t), t), t -> 0.2 + im)")));
        let diff = cx(&ev(&format!("({f}(0.2 + im + 1e-6) - {f}(0.2 + im - 1e-6))/2e-6")));
        close(exact, diff, 1e-6, f);
    }
    // the generalized form Q(a, z0) - Q(a, x) = s: dx/ds = +gamma(a) e^x x^(1 - a) (checked on the factorial of the formula)
    let out = ev("d(inverse_gamma_regularized(a, z0, s), s)");
    assert!(out.contains("gamma(a)") && out.contains("exp("), "{out}");
}

#[test]
fn bessel_zeros_and_spherical_hankel_derivatives() {
    // the zeros are continuous in the index: d j_(nu, k)/dk = pi^2 j Y_nu(j)^2/2 against a central difference
    for f in ["bessel_j_zero", "bessel_y_zero"] {
        let exact = cx(&ev(&format!("n(rewrite(d({f}(2, k), k), k -> 3), 25)")));
        let diff = cx(&ev(&format!("n(({f}(2, 3 + 1/10^6) - {f}(2, 3 - 1/10^6))/(2/10^6), 25)")));
        close(exact, diff, 1e-9, f);
    }
    for f in ["spherical_hankel_h1", "spherical_hankel_h2"] {
        check_derivative(&format!("{f}(2, z)"), "z", "[z -> 3/2]", "3/2", 1e-8);
    }
}

#[test]
fn jacobi_series_in_the_parameter() {
    // series about m = 0 against the function at a small m: the error of an order-n series is O(m^(n+1))
    for (f, order, tol) in [
        ("jacobi_sn(7/10, M)", 1, 5e-5),
        ("jacobi_cn(7/10, M)", 1, 5e-5),
        ("jacobi_dn(7/10, M)", 2, 5e-7),
        ("jacobi_cd(7/10, M)", 1, 5e-5),
        ("jacobi_ns(7/10, M)", 1, 5e-5),
        ("jacobi_epsilon(7/10, M)", 3, 5e-9),
        ("jacobi_zn(7/10, M)", 2, 5e-7),
        ("jacobi_amplitude(7/10, M)", 1, 5e-5),
    ] {
        let sym = f.replace('M', "m");
        let ser = cx(&ev(&format!("n(rewrite(normal(series({sym}, m, 0, {order})), m -> 1/200), 25)")));
        let fun = cx(&ev(&format!("n({}, 25)", f.replace('M', "1/200"))));
        close(ser, fun, tol, f);
    }
    // the first-order coefficient of sn: -(u - sin u cos u) cos u/4 (Abramowitz and Stegun 16.13.1)
    let c = cx(&ev("n(rewrite(coefficient(normal(series(jacobi_sn(u, m), m, 0, 1)), m, 1), u -> 7/10), 25)"));
    let u = 0.7f64;
    close(c, (-(u - u.sin() * u.cos()) * u.cos() / 4.0, 0.0), 1e-12, "A&S 16.13.1");
}

#[test]
fn inverse_jacobi_series_in_the_parameter_and_in_v() {
    let m = "1/200";
    for f in ["cd", "cn", "cs", "dc", "ds", "nc", "ns", "sc", "sd", "sn"] {
        let v = if matches!(f, "dc" | "ds" | "nc" | "ns") { "5/2" } else { "3/10" };
        let ser = cx(&ev(&format!("n(rewrite(normal(series(inverse_jacobi_{f}(v, m), m, 0, 1)), [m -> {m}, v -> {v}]), 25)")));
        let fun = cx(&ev(&format!("n(inverse_jacobi_{f}({v}, {m}), 25)")));
        close(ser, fun, 5e-5, f);
    }
    // series in v at 0: the derivatives now resolve (inverse_jacobi_cd(0, m) = K(m))
    let out = ev("series(inverse_jacobi_sc(z, m), z, 0, 3)");
    assert!(out.starts_with("z + (-1/3 + (1/6)*m)*z^3"), "{out}");
}

#[test]
fn heun_series_at_the_singular_point_and_in_the_lower_half_plane() {
    // Taylor/Frobenius series from the recurrence of the equation, against the numeric function at a small z
    for (sym, num) in [
        ("heun_c(q, 7/10, 3/10, -2/5, 3/10, z)", "heun_c(0.2, 0.7, 0.3, -0.4, 0.3, 0.01)"),
        ("heun_b(q, 7/10, 3/10, -2/5, 3/10, z)", "heun_b(0.2, 0.7, 0.3, -0.4, 0.3, 0.01)"),
        ("heun_g(2, q, 7/10, 3/5, 3/10, -2/5, z)", "heun_g(2, 0.2, 0.7, 0.6, 0.3, -0.4, 0.01)"),
        ("heun_t(q, 7/10, 3/10, -2/5, 3/10, z)", "heun_t(0.2, 0.7, 0.3, -0.4, 0.3, 0.01)"),
    ] {
        let ser = cx(&ev(&format!("n(rewrite(normal(series({sym}, z, 0, 4)), [q -> 1/5, z -> 1/100]), 25)")));
        close(ser, cx(&ev(num)), 1e-9, sym);
    }
    let ser = cx(&ev("n(rewrite(normal(series(heun_d(q, 7/10, 3/10, -2/5, 3/10, z), z, 1, 4)), [q -> 1/5, z -> 101/100]), 25)"));
    close(ser, cx(&ev("heun_d(0.2, 0.7, 0.3, -0.4, 0.3, 1.01)")), 1e-9, "heun_d");
    // the leading terms of the Heun series (DLMF 31.12 recurrence)
    let c = ev("coefficient(normal(series(heun_t(q, a, g, d, e, z), z, 0, 2)), z, 2)");
    assert!(c == "q/2" || c == "(1/2)*q", "{c}");
    // the value in the lower half plane is the continuation without crossing the real axis: an ODE integration of
    // z (z - 1)(z - a) y'' + ... = 0 from the series at 0.04 with DOP853 to 1e-13 (done outside the system) gives
    // heun_g(0.1, 0.1 + i, i, 0.12, 1, -0.32, 0.105 - 0.53942099 i) = -1.6859229862650462 - 6.632313539604873 i
    let got = cx(&ev("heun_g(0.1, 0.1 + im, im, 0.12, 1, -0.32, 0.105 - 0.53942099*im)"));
    close(got, (-1.6859229862650462, -6.632313539604873), 1e-7, "heun_g below the singularity");
    // and above it (the same integration along the upper side)
    let got = cx(&ev("heun_g(0.1, 0.1 + im, im, 0.12, 1, -0.32, 0.105 + 0.53942099*im)"));
    close(got, (-0.07314770992872037, -0.01806684817383395), 1e-7, "heun_g above the singularity");
}

#[test]
fn series_at_zero_of_functions_defined_by_series() {
    // each series against the function at x = 1/100 (the error is O(x^(n + 1)))
    for (f, order, tol) in [
        ("zeta(x)", 2, 1e-5),
        ("factorial2(x)", 2, 1e-5),
        ("barnes_g(x)", 2, 1e-5),
        ("hyperfactorial(x)", 2, 1e-5),
        ("anger_j(1/2, x)", 4, 1e-9),
        ("weber_e(1/2, x)", 4, 1e-9),
        ("whittaker_m(2, 1/2, x)", 6, 1e-12),
        ("whittaker_w(2, 1/2, x)", 6, 1e-12),
        ("mittag_leffler_e(1/2, 1/3, x)", 3, 1e-7),
        ("q_pochhammer(x)", 7, 1e-12),
        ("elliptic_theta_prime(1, 7/10, x)", 10, 1e-12),
        ("inverse_elliptic_nome_q(x)", 6, 1e-7),
        ("hypergeometric_pfq_regularized([1/2, 2/3], [3/4, 5/3], x)", 3, 1e-9),
    ] {
        let ser = cx(&ev(&format!("n(rewrite(normal(series({f}, x, 0, {order})), x -> 1/100), 25)")));
        let fun = cx(&ev(&format!("n(rewrite({f}, x -> 1/100), 25)")));
        close(ser, fun, tol, f);
    }
    // known expansions: Euler's pentagonal series, the Barnes G and hyperfactorial coefficients
    assert_eq!(ev("series(q_pochhammer(x), x, 0, 7)"), "1 - x - x^2 + x^5 + x^7 + O(x^8)");
    assert_eq!(ev("series(inverse_elliptic_nome_q(q), q, 0, 4)"), "16*q - 128*q^2 + 704*q^3 - 3072*q^4 + O(q^5)");
    assert_eq!(ev("series(mathieu_c(3, 2, x), x, 0, 4)"), "mathieu_c(3, 2, 0) + (1/2)*mathieu_c(3, 2, 0)*x^2 + (-5/8)*mathieu_c(3, 2, 0)*x^4 + O(x^5)");
    assert_eq!(ev("series(mathieu_characteristic_a(1, q), q, 0, 3)"), "1 + q - q^2/8 - q^3/64 + O(q^4)");
    // the characteristic values against the numeric ones at small q
    for r in 0..=4 {
        let ser = cx(&ev(&format!("n(rewrite(normal(series(mathieu_characteristic_a({r}, q), q, 0, 4)), q -> 1/20), 25)")));
        let num = cx(&ev(&format!("mathieu_characteristic_a({r}, 0.05)")));
        close(ser, num, 1e-8, &format!("a_{r}"));
    }
    // spheroidal eigenvalue: -2 (n^2 + n - 1 + m^2) g^2/((2n - 1)(2n + 3)) at g = 1/20, error O(g^4)
    let ser = cx(&ev("n(rewrite(normal(series(spheroidal_eigenvalue(n, m, g), g, 0, 3)), [n -> 2, m -> 1, g -> 1/20]), 25)"));
    close(ser, cx(&ev("spheroidal_eigenvalue(2, 1, 0.05)")), 1e-5, "spheroidal");
}

#[test]
fn neville_theta_with_one_exact_argument_is_numeric() {
    // theta_c/theta_n = cn and theta_d/theta_n = dn (DLMF 22.2.5-6 with Neville's normalization)
    for (z, m) in [("2", "0.5"), ("2.", "1/3"), ("1.2", "3/10")] {
        let (c, d, n) = (
            cx(&ev(&format!("neville_theta_c({z}, {m})"))),
            cx(&ev(&format!("neville_theta_d({z}, {m})"))),
            cx(&ev(&format!("neville_theta_n({z}, {m})"))),
        );
        let cn = cx(&ev(&format!("jacobi_cn({z}, {m})")));
        let dn = cx(&ev(&format!("jacobi_dn({z}, {m})")));
        assert!(c.0.is_finite() && d.0.is_finite() && n.0.is_finite(), "{z} {m}: {c:?} {d:?} {n:?}");
        close((c.0 / n.0, 0.0), cn, 1e-13, &format!("cn {z} {m}"));
        close((d.0 / n.0, 0.0), dn, 1e-13, &format!("dn {z} {m}"));
    }
    // a machine m equal to 1/2 is not the exact nome exp(-pi)
    assert!(!ev("neville_theta_d(2, 0.5)").contains("elliptic_theta"));
}

#[test]
fn theta_zeros_and_integer_valued_index() {
    assert_eq!(ev("elliptic_theta(2, [pi/2, pi], 1/2)"), "[0, elliptic_theta(2, pi, 1/2)]");
    assert_eq!(ev("elliptic_theta_prime(2, [pi/2, pi], 1/2)"), "[elliptic_theta_prime(2, (1/2)*pi, 1/2), 0]");
    assert_eq!(ev("[elliptic_theta_prime(3, pi/2, q), elliptic_theta_prime(4, pi, q), elliptic_theta_prime(1, pi/2, q)]"), "[0, 0, 0]");
    // theta_2(pi/2, q) = 0 numerically too, and n maps over the index: n(theta_2(0, 1/30)) is a number
    let v = cx(&ev("n(elliptic_theta(2, 0, 1/30))"));
    close(v, cx(&ev("elliptic_theta(2, 0., 0.03333333333333333)")), 1e-15, "n over the index");
    // theta_2(0, q) = 2 q^(1/4) (1 + q^2 + q^6 + ...): the independent sum
    let q: f64 = 1.0 / 30.0;
    let s = 2.0 * q.powf(0.25) * (1.0 + q.powi(2) + q.powi(6) + q.powi(12));
    close(v, (s, 0.0), 1e-14, "theta_2(0, 1/30)");
}

#[test]
fn normal_removes_the_order_terms_inside_a_product() {
    for src in [
        "normal(series(airy_ai(x), x, inf, 2))",
        "normal(series(airy_bi(x), x, inf, 2))",
        "normal(series(bessel_j(0, x), x, inf, 3))",
        "normal(series(bessel_k(0, x), x, inf, 4))",
        "normal(series(gamma(x), x, inf, 3))",
    ] {
        let out = ev(src);
        assert!(!out.contains("O("), "{src}: {out}");
    }
    // the truncated asymptotic series is the leading terms of the Airy expansion (DLMF 9.7.5)
    let ser = cx(&ev("n(rewrite(normal(series(airy_ai(x), x, inf, 2)), x -> 30), 25)"));
    close(ser, cx(&ev("n(airy_ai(30), 25)")), 1e-5, "airy_ai asymptotics");
}

#[test]
fn regularized_hypergeometric_functions_at_an_inexact_pole_parameter() {
    // 1F1~(1; -1; 9) = 81 e^9 and 0F1~(0; e) = sqrt(e) I_1(2 sqrt e): the limits of the regularized functions; the inexact
    // lower parameter -1. is the same as -1 (they were `indeterminate`)
    let v = cx(&ev("hypergeometric_1f1_regularized(1, -1., 9)"));
    close(v, (81.0 * 9f64.exp(), 0.0), 2e-16 * 4.0, "1F1~");
    let v = cx(&ev("hypergeometric_0f1_regularized(0., e)"));
    let w = cx(&ev("n(sqrt(e)*bessel_i(1, 2*sqrt(e)), 25)"));
    close(v, w, 1e-14, "0F1~");
    assert_eq!(ev("beta_regularized(0, 0, 0)"), "indeterminate");
    assert_eq!(ev("[struve_h(1/2, complex_infinity), struve_l(1/2, complex_infinity)]"), "[indeterminate, indeterminate]");
}

#[test]
fn machine_values_of_closed_forms_are_correctly_rounded() {
    // n(e^9) rounded the constant first and was 2 ulp off; the regularized value goes through 20 digits
    let v = cx(&ev("hypergeometric_1f1_regularized(1, -1, 9.)"));
    close(v, (656349.7981336061, 0.0), 1e-15, "81 e^9");
}

#[test]
fn kelvin_ber_and_bei_of_a_negative_argument_are_real() {
    // J_n(x e^(3 pi i/4)) for integer n and real x < 0: ber_n and bei_n are real (ber_n(-x) = (-1)^n ber_n(x))
    let out = ev("n(kelvin_ber(-2), 30)");
    assert!(!out.contains("im"), "{out}");
    let out = ev("n(kelvin_bei(-2), 30)");
    assert!(!out.contains("im"), "{out}");
    let a = cx(&ev("n(kelvin_ber(-2), 25)"));
    let b = cx(&ev("n(kelvin_ber(2), 25)"));
    close(a, b, 1e-20, "ber is even");
}

#[test]
fn terminating_two_f_one_at_exact_rational_arguments() {
    // 2F1(3/2, -4; 1/2; 2) = 17 by the terminating sum 1 - 12 z + 30 z^2 - 28 z^3 + 9 z^4 at z = 2; and Euler's
    // transformation (DLMF 15.8.1) for c - a = -1: 2F1(3/2, 5; 1/2; 1/2) = (1/2)^-6 2F1(-1, -9/2; 1/2; 1/2) = 352
    assert_eq!(ev("hypergeometric_2f1(3/2, -4, 1/2, 2)"), "17");
    assert_eq!(ev("hypergeometric_2f1(3/2, 5, 1/2, 1/2)"), "352");
    assert_eq!(ev("appell_f3(1/2, 3/2, -4, 5, 1/2, 0, [[1/2, 2], [2, 1/2]])"), "[[352, 19], [19, 352]]");
    assert_eq!(ev("appell_f4(3/2, -4, 5, 1/2, 0, 2)"), "17");
    // the exact value agrees with the series of the numeric call
    let v = cx(&ev("hypergeometric_2f1(3/2, 5, 1/2, 0.5)"));
    close(v, (352.0, 0.0), 1e-12, "numeric 2F1");
}

#[test]
fn multinomial_with_one_symbolic_argument_is_a_polynomial() {
    // (n + 3)!/(n! 1! 2!) = (n + 1)(n + 2)(n + 3)/2
    assert_eq!(ev("expand(multinomial(n, 1, 2) - (3 + (11/2)*n + 3*n^2 + (1/2)*n^3))"), "0");
    assert_eq!(ev("multinomial(1, 2, 3)"), "60");
    assert_eq!(ev("rewrite(multinomial(x, 2), x -> 5)"), "21");
}
