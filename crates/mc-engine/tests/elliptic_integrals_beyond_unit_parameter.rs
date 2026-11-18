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

/// A characteristic of 1 or more with m > 1 has no value here: the call stays, at once (it used to run without end).
#[test]
fn characteristic_and_parameter_beyond_one_stay() {
    let mut e = CoreEngine::new();
    let t = std::time::Instant::now();
    assert_eq!(ev(&mut e, "elliptic_pi(2.0, 1.5)"), "elliptic_pi(2.0, 1.5)");
    assert_eq!(ev(&mut e, "elliptic_pi(3.0, 1.5)"), "elliptic_pi(3.0, 1.5)");
    assert_eq!(ev(&mut e, "elliptic_pi(2.0, 1.0)"), "elliptic_pi(2.0, 1.0)");
    assert!(t.elapsed().as_secs() < 30, "{:?}", t.elapsed());
}
