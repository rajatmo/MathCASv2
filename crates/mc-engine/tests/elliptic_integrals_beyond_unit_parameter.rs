//! The incomplete integral of the second kind E(phi | m) and the complete integral of the third kind Pi(n | m) for a
//! parameter m > 1, where they are complex (m on its cut from below, as K(m) and F(phi | m)): each value is checked
//! against its defining integral, split where 1 - m sin^2 t changes sign (the real part is the integral up to there,
//! the imaginary part the integral of the other side with sqrt(1 - m sin^2 t) = i sqrt(m sin^2 t - 1)), and machine
//! arguments get the value of the exact ones.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

/// `v` (a 30-digit value) agrees with the two parts of the split integral (15-digit quadratures at 30 digits) to 10^-12.
fn split_check(e: &mut CoreEngine, v: &str, re_integrand: &str, im_integrand: &str, m: &str, phi: &str) {
    let src = format!(
        "{{ let v = n({v}, 30); let t0 = asin(1/sqrt({m})); \
         let r = n_integrate({re_integrand}, [t, 0, t0], working_precision: 30, precision_goal: 15); \
         let i = n_integrate({im_integrand}, [t, t0, {phi}], working_precision: 30, precision_goal: 15); \
         abs(re(v) - r) < 10^-12 and abs(im_part(v) - i) < 10^-12 }}"
    );
    holds(e, &src);
}

#[test]
fn incomplete_second_kind_beyond_one() {
    let mut e = CoreEngine::new();
    for (phi, m) in [("1", "3/2"), ("6/5", "2"), ("3/2", "5/4"), ("1/2", "5")] {
        split_check(
            &mut e,
            &format!("elliptic_e({phi}, {m})"),
            &format!("sqrt(1 - {m}*sin(t)^2)"),
            &format!("sqrt({m}*sin(t)^2 - 1)"),
            m,
            phi,
        );
    }
    // odd in phi, and quasi-periodic: E(phi + pi | m) = E(phi | m) + 2 E(m)
    holds(&mut e, "abs(n(elliptic_e(-1, 3/2), 30) + n(elliptic_e(1, 3/2), 30)) < 10^-25");
    holds(&mut e, "abs(n(elliptic_e(1 + pi, 3/2), 30) - n(elliptic_e(1, 3/2), 30) - 2*n(elliptic_e(3/2), 30)) < 10^-25");
    // below the turning point the value is real
    holds(&mut e, "im_part(n(elliptic_e(1/2, 3/2), 30)) == 0");
}

#[test]
fn complete_third_kind_beyond_one() {
    let mut e = CoreEngine::new();
    for (nn, m) in [("1/2", "3/2"), ("-2", "3"), ("0", "2"), ("9/10", "11/10")] {
        split_check(
            &mut e,
            &format!("elliptic_pi({nn}, {m})"),
            &format!("1/((1 - {nn}*sin(t)^2)*sqrt(1 - {m}*sin(t)^2))"),
            &format!("-1/((1 - {nn}*sin(t)^2)*sqrt({m}*sin(t)^2 - 1))"),
            m,
            "pi/2",
        );
    }
    // n = 0 is K(m)
    holds(&mut e, "abs(n(elliptic_pi(0, 3/2) - elliptic_k(3/2), 30)) < 10^-25");
}

#[test]
fn machine_arguments_beyond_one() {
    let mut e = CoreEngine::new();
    holds(&mut e, "abs(elliptic_e(1., 1.5) - n(elliptic_e(1, 3/2))) < 10^-14");
    holds(&mut e, "abs(elliptic_pi(0.5, 1.5) - n(elliptic_pi(1/2, 3/2))) < 10^-14");
}

/// The principal value of the integral of g over [a, b] with a simple pole at p inside: the two sides with
/// [p - 10^-12, p + 10^-12] left out (the error of the excision is of the order of its width times g's regular part).
fn pv_src(g: &str, a: &str, b: &str, p: &str) -> String {
    format!(
        "(n_integrate({g}, [t, {a}, ({p}) - 10^-12], working_precision: 30, precision_goal: 15) +          n_integrate({g}, [t, ({p}) + 10^-12, {b}], working_precision: 30, precision_goal: 15))"
    )
}

/// A characteristic n > 1 beyond m > 1: 1 - n sin^2 t vanishes at sin^2 t = 1/n, before the turning point t0 when
/// n > m (in the real part), after it when 1 < n < m (in the imaginary part); the value is the Cauchy principal value
/// there, checked against the defining integral with the pole cut out symmetrically.
#[test]
fn complete_third_kind_principal_values_beyond_one() {
    let mut e = CoreEngine::new();
    for (nn, m) in [("3/2", "2"), ("3", "2"), ("5", "7/2"), ("11/10", "5")] {
        let re_g = format!("1/((1 - {nn}*sin(t)^2)*sqrt(1 - {m}*sin(t)^2))");
        let im_g = format!("-1/((1 - {nn}*sin(t)^2)*sqrt({m}*sin(t)^2 - 1))");
        let (t0, tp) = (format!("asin(1/sqrt({m}))"), format!("asin(1/sqrt({nn}))"));
        let pole_in_real = e.eval(&format!("{nn} > {m}"), &StopFlag::new()).map(|r| r.printed).unwrap() == "true";
        let (re_i, im_i) = if pole_in_real {
            (pv_src(&re_g, "0", &t0, &tp), format!("n_integrate({im_g}, [t, {t0}, pi/2], working_precision: 30, precision_goal: 15)"))
        } else {
            (format!("n_integrate({re_g}, [t, 0, {t0}], working_precision: 30, precision_goal: 15)"), pv_src(&im_g, &t0, "pi/2", &tp))
        };
        holds(&mut e, &format!("{{ let v = n(elliptic_pi({nn}, {m}), 30); abs(re(v) - {re_i}) < 10^-9 and abs(im_part(v) - {im_i}) < 10^-9 }}"));
    }
    // machine arguments: the value of the exact ones
    holds(&mut e, "abs(elliptic_pi(3.0, 2.0) - n(elliptic_pi(3, 2))) < 10^-14");
    holds(&mut e, "abs(elliptic_pi(1.5, 2.0) - n(elliptic_pi(3/2, 2))) < 10^-14");
}

/// The integral diverges at m = 1, at n = 1 and at n = m > 1: exact arguments give the pole failure, machine ones
/// stay unevaluated; every call on either side of each bound and at it ends at once (one of them used to run
/// without end).
#[test]
fn divergent_points_and_their_neighbours() {
    let mut e = CoreEngine::new();
    let t = std::time::Instant::now();
    for src in ["elliptic_pi(1/2, 1)", "elliptic_pi(3, 1)", "elliptic_pi(1, 1/2)", "elliptic_pi(1, 2)", "elliptic_pi(2, 2)", "elliptic_pi(7/2, 7/2)"] {
        assert_eq!(ev(&mut e, src), format!("failure(pole, {src})"));
    }
    // n = m < 1 is finite: E(m)/(1 - m)
    holds(&mut e, "abs(n(elliptic_pi(1/2, 1/2) - 2*elliptic_e(1/2), 30)) < 10^-25");
    for src in ["elliptic_pi(1.0, 1.5)", "elliptic_pi(1.5, 1.5)", "elliptic_pi(2.0, 1.0)", "elliptic_pi(1.0, 2.0)"] {
        assert_eq!(ev(&mut e, src), src);
    }
    // each side of n = 1, of n = m and of m = 1, as machine numbers and as 30-digit balls: finite values
    for (nn, m) in [("0.999999", "1.5"), ("1.000001", "1.5"), ("1.499999", "1.5"), ("1.500001", "1.5"), ("2.0", "1.000001"), ("2.0", "0.999999")] {
        holds(&mut e, &format!("is_number(elliptic_pi({nn}, {m}))"));
        holds(&mut e, &format!("is_number(n(elliptic_pi(rationalize({nn}, 0), rationalize({m}, 0)), 30))"));
    }
    // a ball across a bound gives no value (the sides differ)
    for src in ["elliptic_pi(centered_interval(3/2, 1/100), 3/2)", "elliptic_pi(centered_interval(1, 1/100), 2)"] {
        assert!(!ev(&mut e, src).starts_with("centered_interval"), "{src}");
    }
    assert!(t.elapsed().as_secs() < 60, "{:?}", t.elapsed());
}
