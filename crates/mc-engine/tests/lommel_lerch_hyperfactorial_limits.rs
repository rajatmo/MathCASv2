//! Values of the modified Lommel function T at 0 and by its machine kernel, Lerch's phi beyond the unit circle, the
//! hyperfactorial at quarter and half integers, the regularized beta function and the inverse Weierstrass function
//! at their special points, and limits of special functions at logarithmic points, along imaginary rays and at
//! infinity. Every value is checked by a route other than the code that gives it: a second algorithm, the defining
//! series, the forward function or values near the point.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

/// Evaluate every statement in one fresh session (one worker thread, big stack); `TIMEOUT` after 120 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &StopFlag::new()) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

/// A printed machine real.
fn num(s: &str) -> f64 {
    s.trim().parse().unwrap_or_else(|_| panic!("not a number: {s}"))
}

/// `abs(expr)` at 30 digits must be at most `tol` (a ball around 0 counts by its radius).
fn small(expr: &str, tol: f64) {
    let out = ev(&format!("n(n(abs({expr}), 30))"));
    let v = match out.strip_prefix("ball(0, ").and_then(|r| r.strip_suffix(')')) {
        Some(r) => num(r),
        None => num(&out),
    };
    assert!(v <= tol, "{expr}: {v} > {tol}");
}

// ---------------------------------------------------------------- modified Lommel function T

/// At an inexact 0 the value is the limit from the right (it did not return); the sign agrees with the values at
/// tiny positive z, both where K_nu dominates (mu + 1 > -nu) and where t does (mu + 1 < -|nu|).
#[test]
fn lommel_t2_at_zero() {
    let t0 = Instant::now();
    assert_eq!(ev("lommel_t2(2, 1/2, 0.0)"), "inf");
    assert!(num(&ev("lommel_t2(2, 1/2, 1.0e-200)")) > 1e90);
    assert_eq!(ev("lommel_t2(-5, 1/2, 0.0)"), "-inf");
    assert!(num(&ev("lommel_t2(-5, 1/2, 1.0e-10)")) < -1e30);
    assert_eq!(ev("lommel_t2(2, 1/2, 0)"), "lommel_t2(2, 1/2, 0)");
    assert!(t0.elapsed() < Duration::from_secs(30));
}

/// The double-double kernel against the ball path at 30 digits (another evaluation of the connection formula,
/// through K rather than I_(-nu)), and the ball path against the differential equation
/// z^2 w'' + z w' - (z^2 + nu^2) w = -z^(mu + 1) at the same point.
#[test]
fn lommel_t2_machine_kernel() {
    for (mu, nu, z, zx) in [
        ("4/3", "1/2", "1.5 + 2.0*im", "3/2 + 2*im"),
        ("2", "1/2", "2.5", "5/2"),
        ("3", "1/3", "-2.5", "-5/2"),
        ("5", "1/2", "7.9", "79/10"),
        ("1/2", "1/3", "0.25", "1/4"),
    ] {
        small(&format!("(lommel_t2({mu}, {nu}, {z}) - lommel_t2({mu}, {nu}, n({zx}, 30)))/lommel_t2({mu}, {nu}, n({zx}, 30))"), 1e-14);
    }
    // the derivatives by central differences of step 10^-8 at 30 digits (errors about 10^-16)
    let ode = "{ var z = n(3/2 + 2*im, 30); var h = n(10^-8, 30); var w0 = lommel_t2(4/3, 1/2, z); \
               var wp = lommel_t2(4/3, 1/2, z + h); var wm = lommel_t2(4/3, 1/2, z - h); \
               abs(z^2*(wp - 2*w0 + wm)/h^2 + z*(wp - wm)/(2*h) - (z^2 + 1/4)*w0 + z^(7/3))/abs(z^(7/3)) }";
    let v = num(&ev(&format!("n({ode})")));
    assert!(v < 1e-12, "{v}");
}

/// A plot's worth of values takes milliseconds each (the plots of T timed out).
#[test]
fn lommel_t2_plot_speed() {
    let t0 = Instant::now();
    let out = ev("length(table(lommel_t2(m, 1/2, x + 0.1*im), m, range(1, 5), x, range(-4, 8, 0.05)))");
    assert_eq!(out, "5");
    assert!(t0.elapsed() < Duration::from_secs(20), "{:?}", t0.elapsed());
}

// ---------------------------------------------------------------- Lerch phi beyond the unit circle

/// Near the circle (|z| = 0.992) the expansion in log z against the defining series summed to 9000 terms; at a = 1
/// against the polylogarithm, Phi(z, s, 1) = Li_s(z)/z (another algorithm); the value at z = 5 + i at machine
/// precision against its value at 30 digits.
#[test]
fn lerch_phi_log_expansion() {
    let direct = num(&ev("abs(lerch_phi(0.6 + 0.79*im, 0.5 + 0.2*im, 1.5 + 0.5*im) - total(table((0.6 + 0.79*im)^k/(k + 1.5 + 0.5*im)^(0.5 + 0.2*im), k, range(0, 9000))))"));
    assert!(direct < 1e-12, "{direct}");
    for z in ["2.0 + 0.5*im", "-2.0", "-1.5*im"] {
        let d = num(&ev(&format!("abs(lerch_phi({z}, 2.5, 1) - polylog(2.5, {z})/({z}))")));
        assert!(d < 1e-13, "{z}: {d}");
    }
    let v = ev("lerch_phi(5. + im, im, 2 + im)");
    assert!(!v.starts_with("lerch_phi"), "{v}");
    small("lerch_phi(5. + im, im, 2 + im) - (-0.58150174999043657531717 + 0.38476659358331604461253*im)", 1e-14);
    // on the cut z > 1 and at a positive integer s with a non-integer a nothing is given
    assert!(ev("lerch_phi(2.0, 1/2, 1/3)").starts_with("lerch_phi"));
}

// ---------------------------------------------------------------- hyperfactorial

/// The closed forms against the numeric K-function of an inexact argument (exp(zeta'(-1, z + 1) - zeta'(-1)) by
/// log Gamma and log G).
#[test]
fn hyperfactorial_quarter_and_half_integers() {
    assert_eq!(ev("hyperfactorial(1/2)"), "(1/2)*2^(11/24)*exp(-1/8)*glaisher^(3/2)");
    assert_eq!(ev("hyperfactorial(-3/4)"), "exp(-3/32 + (1/4)*catalan/pi)*glaisher^(9/8)");
    for z in ["1/2", "-1/2", "-3/4", "-1/4", "1/4", "3/4", "13/4", "7/2", "21/2"] {
        small(&format!("(hyperfactorial({z}) - hyperfactorial(n({z}, 30)))/hyperfactorial(n({z}, 30))"), 1e-25);
    }
    // below -1 the cut: not given
    assert_eq!(ev("hyperfactorial(-5/4)"), "hyperfactorial(-5/4)");
}

// ---------------------------------------------------------------- regularized beta and inverse Weierstrass p

/// I_z(a, b) at the infinities for positive integers a, b: the sign of the polynomial far out.
#[test]
fn beta_regularized_at_infinities() {
    assert_eq!(ev("beta_regularized(inf, 1, 2)"), "-inf");
    assert_eq!(ev("beta_regularized(complex_infinity, 1, 3)"), "complex_infinity");
    for (a, b) in [(1, 2), (1, 3), (2, 3), (3, 2), (2, 2)] {
        for (pt, x) in [("inf", "1000"), ("-inf", "-1000")] {
            let v = ev(&format!("beta_regularized({pt}, {a}, {b})"));
            let far = num(&ev(&format!("n(beta_regularized({x}, {a}, {b}))")));
            let want = if far > 0.0 { "inf" } else { "-inf" };
            assert_eq!(v, want, "{pt} {a} {b}");
        }
    }
}

/// With g2 = g3 = 0, p(u) = 1/u^2: the inverse is p^(-1/2), complex infinity at 0.
#[test]
fn inverse_weierstrass_p_degenerate() {
    assert_eq!(ev("inverse_weierstrass_p(0, [0, 0])"), "complex_infinity");
    assert_eq!(ev("weierstrass_p(inverse_weierstrass_p(4, [0, 0]), [0, 0])"), "4");
    assert_eq!(ev("weierstrass_p(inverse_weierstrass_p(-3 + im, [0, 0]), [0, 0])"), "complex(-3, 1)");
}

// ---------------------------------------------------------------- limits

/// li at 1 and Ei at 0 tend to -inf from both sides (the values near the points fall like log|log x|).
#[test]
fn logarithmic_points() {
    assert_eq!(ev("limit(log_integral(x), x -> 1, direction: from_above)"), "-inf");
    assert_eq!(ev("limit(log_integral(x), x -> 1)"), "-inf");
    assert_eq!(ev("limit(exp_integral_ei(x), x -> 0)"), "-inf");
    assert!(num(&ev("n(log_integral(1 + 10^-12))")) < -26.0);
    assert!(num(&ev("n(log_integral(1 - 10^-12))")) < -26.0);
}

/// Along the imaginary axis: I_nu and M(a, b, z) with Re a > 0 and Re(a - b) < 0 tend to 0; values far out are small.
#[test]
fn imaginary_rays() {
    assert_eq!(ev("limit(bessel_i(n_u, x), x -> im*inf)"), "0");
    assert_eq!(ev("limit(hypergeometric_1f1(1/2, sqrt(2), x), x -> im*inf)"), "0");
    assert_eq!(ev("limit(hypergeometric_1f1_regularized(1/2, sqrt(2), x), x -> im*inf)"), "0");
    assert_eq!(ev("limit(1/x, x -> im*inf)"), "0");
    assert!(num(&ev("abs(bessel_i(2, 2000.0*im))")) < 0.02);
    assert!(num(&ev("abs(hypergeometric_1f1(0.5, 1.4142135623730951, -3000.0*im))")) < 0.05);
    // a parameter with Re a - b >= 0, and exp: no limit
    assert!(ev("limit(hypergeometric_1f1(2, 1, x), x -> im*inf)").starts_with("limit("));
    assert!(ev("limit(exp(x), x -> im*inf)").starts_with("limit("));
}

/// U(a, b, x) ~ x^(-a) at infinity.
#[test]
fn tricomi_u_at_infinity() {
    assert_eq!(ev("limit(hypergeometric_u(1/2, b, x), x -> inf)"), "0");
    assert_eq!(ev("limit(hypergeometric_u(-2, 3, x), x -> inf)"), "inf");
    assert_eq!(ev("limit(hypergeometric_u(0, 3, x), x -> inf)"), "1");
    assert!(ev("limit(hypergeometric_u(a, 3, x), x -> inf)").starts_with("limit("));
    assert!(num(&ev("n(hypergeometric_u(1/2, 3, 10^6))")) < 2e-3);
}

/// !x/x! -> 1/e: Q(x + 1, -1) -> 1; the ratio at x = 30 is 1/e to 30 digits (!n/n! = sum_(k<=n) (-1)^k/k!).
#[test]
fn subfactorial_over_factorial() {
    assert_eq!(ev("limit(subfactorial(x)/factorial(x), x -> inf)"), "exp(-1)");
    assert_eq!(ev("limit(3*subfactorial(x)/gamma(x + 1), x -> inf)"), "3/e");
    small("subfactorial(30)/factorial(30) - exp(-1)", 1e-30);
}
