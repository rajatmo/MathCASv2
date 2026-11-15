//! Round 42: characteristic values of the Mathieu equation at a complex q (the eigenvalue of Hill's matrix continued
//! from q = 0, refined on the Floquet condition), and the derivative of the theta functions at a nome near the unit
//! circle by the modular transformations. Checked by the Floquet condition on the solution of the equation, by
//! periodicity, by conjugate symmetry and by difference quotients of the theta functions.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn characteristic_values_at_a_complex_q() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "abs(n(mathieu_characteristic_a(pi/5, 5 - im)) - (-5.771184155452747 + 1.5541381999430361*im)) < 1e-12"), "true");
    // the Floquet condition w_I(pi) = cos(pi nu) (DLMF 28.2.16; w_I = ce/ce(0)) at 25 digits
    assert_eq!(
        ev(&mut e, "a = n(mathieu_characteristic_a(pi/5, 5 - im), 25); qq = n(5 - im, 25); abs(mathieu_c(a, qq, n(pi, 25))/mathieu_c(a, qq, 0) - cos(pi^2/5)) < 1e-15"),
        "true"
    );
    assert_eq!(
        ev(&mut e, "a = mathieu_characteristic_a(0.3, 2.0 + 1.5*im); abs(mathieu_c(a, 2.0 + 1.5*im, n(pi))/mathieu_c(a, 2.0 + 1.5*im, 0.0) - cos(0.3*pi)) < 1e-9"),
        "true"
    );
    // an even integer order: ce_0 has period pi, so its derivative vanishes at pi/2
    assert_eq!(ev(&mut e, "a = mathieu_characteristic_a(0, 1.0 + im); abs(mathieu_c_prime(a, 1.0 + im, n(pi)/2)) < 1e-9"), "true");
    // an odd order: ce_1 is odd about pi/2
    assert_eq!(ev(&mut e, "a = mathieu_characteristic_a(1, 0.5 - 2.0*im); abs(mathieu_c(a, 0.5 - 2.0*im, n(pi)/2)) < 1e-9"), "true");
    // conjugate q gives the conjugate value
    assert_eq!(
        ev(&mut e, "abs(mathieu_characteristic_a(0.7, 3.0 + im) - conjugate(mathieu_characteristic_a(0.7, 3.0 - im))) < 1e-12"),
        "true"
    );
    // b at an even order: se_2 has period pi, so it vanishes at pi/2
    assert_eq!(ev(&mut e, "b = mathieu_characteristic_b(2, 1.0 + 0.5*im); abs(mathieu_s(b, 1.0 + 0.5*im, n(pi)/2)) < 1e-9"), "true");
}

#[test]
fn theta_derivative_near_the_unit_circle() {
    let mut e = CoreEngine::new();
    for (k, z, q) in [(1, "3/10", "95/100"), (3, "7/10", "97/100*exp(im/5)"), (2, "1/5 + im/10", "-96/100"), (4, "11/10", "99/100")] {
        // a central difference of the function itself at 30 digits (step 1e-12, error of order 1e-24 times theta''')
        let src = format!(
            "h = 10^-12; zz = n({z}, 30); qq = n({q}, 30); dq = (elliptic_theta({k}, zz + h, qq) - elliptic_theta({k}, zz - h, qq))/(2*h); v = elliptic_theta_prime({k}, zz, qq); abs(v - dq) <= 1e-8*(1 + abs(dq))"
        );
        assert_eq!(ev(&mut e, &src), "true", "theta_{k}'({z}, {q})");
        // machine numbers give a number
        let m = ev(&mut e, &format!("is_number(elliptic_theta_prime({k}, n({z}), n({q})))"));
        assert_eq!(m, "true", "machine theta_{k}'({z}, {q})");
    }
}
