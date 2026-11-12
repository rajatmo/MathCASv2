//! Round 27: tests of the fixes that the round 26 lists left without a test of their own, and of the
//! method behind them (values compared with the same function at higher precision or with an identity).
#![allow(dead_code)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread (fresh session, one worker): `None` when it did not finish in `secs` seconds.
fn ev_within(src: &str, secs: u64) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

fn ev(src: &str) -> String {
    ev_within(src, 60).unwrap_or_else(|| "TIMEOUT".to_string())
}

/// The language checks `abs(a - b) <= tol abs(b)` where `a` is the machine call and `b` the 25-digit value.
fn agrees(machine: &str, exact: &str, tol: &str) -> bool {
    ev(&format!("{{ let a = {machine}; let b = n({exact}, 25); abs(a - b) <= {tol}*abs(b) }}")) == "true"
}

#[test]
fn x26_16_sum_with_a_tiny_number_is_printed_with_its_precision() {
    // 1 - 10.0~25^-10^8 is 1 to 10^8 digits as a ball; printing it asked for every digit (2.8 GB)
    let out = ev_within("time_constrained(1 - 10.0~25^-100000000, 2)", 8).expect("finishes");
    assert!(out.len() < 200, "{}", &out[..out.len().min(80)]);
    let out = ev_within("time_constrained(cdf(exponential_distribution(3.0~25), 2^31), 2)", 8).expect("finishes");
    assert!(out.len() < 200);
}

#[test]
fn polynomials_of_high_degree_agree_with_their_exact_values() {
    // machine arguments: the recurrence, against the exact rational argument
    assert!(agrees("laguerre_l(300, 0.5, 3.)", "laguerre_l(300, 1/2, 3)", "1e-9"));
    assert!(agrees("gegenbauer_c(300, 2.5, 0.25)", "gegenbauer_c(300, 5/2, 1/4)", "1e-9"));
    assert!(agrees("jacobi_p(400, 0.5, 1.5, -0.25)", "jacobi_p(400, 1/2, 3/2, -1/4)", "1e-9"));
    // a degree beyond the exact recurrences (3000): machine
    let out = ev("laguerre_l(5000, 0, 1.5)");
    assert!(!out.contains("indeterminate") && !out.contains("nan"), "{out}");
    // the exact value at a 30-digit request has its 30 digits (it was a ball of radius 1e179)
    let out = ev("precision(n(jacobi_p(400, 1/2, 1/3, 1/7), 30)) >= 29");
    assert_eq!(out, "true");
}

#[test]
fn spherical_harmonics_of_high_degree_match_the_exact_value() {
    assert!(agrees("spherical_harmonic_y(40, 25, 0.7, 0.4)", "spherical_harmonic_y(40, 25, 7/10, 2/5)", "1e-11"));
    assert!(agrees("spherical_harmonic_y(40, -25, 0.7, 0.4)", "spherical_harmonic_y(40, -25, 7/10, 2/5)", "1e-11"));
}

#[test]
fn gegenbauer_parameter_minus_a_half_has_its_limit() {
    // C_n^(-1/2): the gamma ratio has a pole in the numerator and the denominator; n = 3: -(3/2... ) polynomial value
    let a = ev("gegenbauer_c(3, -1/2, 2.5)");
    assert!(a.contains("-6.5625"), "{a}");
    // a non-integer degree through the regularized 2F1: continuous in the parameter
    assert!(agrees("gegenbauer_c(2.5, -1/2, 3.2)", "gegenbauer_c(5/2, -1/2 + 1/10^12, 16/5)", "1e-6"));
}

#[test]
fn meijer_g_at_large_argument_has_the_cancellation_precision() {
    // G^{3,0}_{0,3}(z | 0, 1/3, 2/3) = 2 pi/sqrt(3) exp(-3 z^(1/3)); terms of size e^(3 z^(1/3)) cancel
    for z in ["1.0e4", "1.0e5", "1.0e6"] {
        assert!(agrees(&format!("meijer_g([[], []], [[0, 1/3, 2/3], []], {z})"), &format!("2*pi/sqrt(3)*exp(-3*({z})^(1/3))"), "1e-12"), "z = {z}");
    }
    // and with the parameters exact inside n at 30 digits
    let out = ev("abs(n(meijer_g([[], []], [[0, 1/3, 2/3], []], 10^6), 30) - 2*pi/sqrt(3)*exp(-3*100)) < 10^-155");
    assert_eq!(out, "true");
}

#[test]
fn zeta_far_up_the_imaginary_axis() {
    // exact arguments with a binary value: the machine value against the 25-digit one
    assert!(agrees("zeta(-873.5*im)", "zeta(-1747/2*im)", "1e-12"));
    assert!(agrees("zeta(2048.25*im)", "zeta(8193/4*im)", "1e-12"));
    assert!(agrees("zeta(0.25 + 300.5*im)", "zeta(1/4 + 601/2*im)", "1e-12"));
    // the 30-digit value at t = 10^5 within the time limit (it needed 150000 bits per term)
    let out = ev_within("n(zeta(1/2 + 100000*im), 30)", 20).expect("finishes");
    assert!(out.contains("1.0730320148577531321140762694920985"), "{out}");
}

#[test]
fn incomplete_gamma_at_a_large_imaginary_argument() {
    // Gamma(2, z) = (1 + z) exp(-z), z = 3000 i
    let out = ev("abs(gamma(2, 3000.*im) - (1 + 3000.*im)*exp(-3000.*im)) < 1e-9");
    assert_eq!(out, "true");
    // Gamma(a, z) + gamma_lower: the regularized function and E_nu from the same value
    let out = ev("abs(exp_integral_e(0.7, 9000.*im) - (9000.*im)^(-0.3)*gamma(0.3, 9000.*im)) < 1e-12");
    assert_eq!(out, "true");
}

#[test]
fn hypergeometric_2f1_beyond_the_unit_disc_with_a_degenerate_connection() {
    // b - a an integer: the connection formulas are singular; the exact parameters reach the evaluator as balls
    let out = ev_within("n(hypergeometric_2f1(1/3, 4/3, 5, 3), 30)", 10).expect("finishes");
    assert!(out.contains("1.253086604643616627660857698189"), "{out}");
    let out = ev("abs(hypergeometric_2f1(1/3, 4/3, 5, 3.) - n(hypergeometric_2f1(1/3, 4/3, 5, 3), 25)) < 1e-13");
    assert_eq!(out, "true");
}

#[test]
fn large_argument_values_that_took_seconds() {
    let t = std::time::Instant::now();
    // bessel_y on the negative axis: e^(-i nu pi) Y + 2 i cos(nu pi) J
    assert!(agrees("bessel_y(0.3, -5.5)", "bessel_y(3/10, -11/2)", "1e-12"));
    assert!(agrees("bessel_k(0.3, 40 + 3.*im)", "bessel_k(3/10, 40 + 3*im)", "1e-12"));
    assert!(agrees("struve_h(0.5, 100.)", "struve_h(1/2, 100)", "1e-12"));
    assert!(agrees("parabolic_cylinder_d(-0.5, 31.)", "parabolic_cylinder_d(-1/2, 31)", "1e-11"));
    assert!(agrees("hypergeometric_u(1/3, 1/2, -200.)", "hypergeometric_u(1/3, 1/2, -200)", "1e-12"));
    assert!(t.elapsed().as_secs() < 100);
    // overflow of ber: infinite with the sign of cos(alpha), alpha = x/sqrt 2 + (nu/2 - 1/8) pi
    let out = ev("kelvin_ber(2, 75069.946)");
    assert!(out.contains("inf"), "{out}");
}
