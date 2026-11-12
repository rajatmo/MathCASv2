//! Round 33: bugs found in round 32.
mod common;
use common::{number, Sys};
use std::time::Instant;

fn timed(s: &mut Sys, src: &str, secs: f64) -> String {
    let t = Instant::now();
    let out = s.run(src);
    assert!(t.elapsed().as_secs_f64() < secs, "{src} took {:?}", t.elapsed());
    out
}

fn re_im(s: &str) -> (f64, f64) {
    // "complex(a,b)" (the printed machine complex) or a real
    if let Some(body) = s.strip_prefix("complex(").and_then(|b| b.strip_suffix(')')) {
        let (re, im) = body.split_once(',').expect("complex");
        (number(re.trim()), number(im.trim()))
    } else {
        (number(s), 0.0)
    }
}

#[test]
fn mathieu_s_at_q_zero_is_sin_nu_z() {
    // DLMF 28.12: se_nu(z, 0) = sin(nu z) with nu = sqrt(a), so se' = nu cos(nu z) (the symbolic form had a factor 1/nu)
    let mut s = Sys::new();
    assert_eq!(s.run("mathieu_s(a, 0, z)"), "sin(z*a^(1/2))");
    assert_eq!(s.run("mathieu_s_prime(a, 0, z)"), "cos(z*a^(1/2))*a^(1/2)");
    // the machine value agrees with the exact form
    let got = number(&s.run("mathieu_s(2, 0, 1.0)"));
    assert!((got - 2f64.sqrt().sin()).abs() < 1e-15, "{got}");
    let got = number(&s.run("mathieu_s_prime(2, 0, 1.0)"));
    assert!((got - 2f64.sqrt() * 2f64.sqrt().cos()).abs() < 1e-15, "{got}");
}

#[test]
fn jacobi_complex_machine_values_agree_with_the_40_digit_values() {
    let mut s = Sys::new();
    // (machine call, exact call): the exact arguments are the doubles' decimal forms, 15 digits compared
    for (m, e) in [
        ("jacobi_cd(0.3 + 0.7*im, 1/3)", "n(jacobi_cd(3/10 + 7/10*im, 1/3), 30)"),
        ("jacobi_sn(2.5 - 3*im, 0.8)", "n(jacobi_sn(5/2 - 3*im, 4/5), 30)"),
        ("jacobi_dn(0.2 + 4*im, -3.0)", "n(jacobi_dn(1/5 + 4*im, -3), 30)"),
        ("jacobi_ns(1.5 + 2.5*im, 3)", "n(jacobi_ns(3/2 + 5/2*im, 3), 30)"),
        ("jacobi_cn(-7.5 + 0.1*im, 0.45)", "n(jacobi_cn(-15/2 + 1/10*im, 9/20), 30)"),
    ] {
        let (a, b) = (re_im(&s.run(m)), re_im(&s.run(e)));
        let scale = 1.0 + b.0.hypot(b.1);
        assert!((a.0 - b.0).hypot(a.1 - b.1) < 3e-15 * scale, "{m}: {a:?} {b:?}");
    }
}

#[test]
fn jacobi_complex_plots_are_fast() {
    // complex_plot3d of Jacobi functions took 12 to 67 s (6 to 35 ms per value on the ball path)
    let mut s = Sys::new();
    timed(&mut s, "complex_plot3d(jacobi_cd(z, 1/3), [z, -2 - 2*im, 2 + 2*im])", 5.0);
    timed(&mut s, "complex_plot3d(jacobi_ns(z, 3), [z, -2 - 2*im, 2 + 2*im])", 5.0);
    timed(&mut s, "complex_plot3d(inverse_jacobi_cd(z, 1/3), [z, -2 - 2*im, 2 + 2*im])", 5.0);
}

#[test]
fn inverse_jacobi_and_elliptic_k_beyond_one_are_fast() {
    // K(m) for m > 1 on the cut from below by the double-double AGM (was 23 ms on the ball path), and K' computed only
    // when a shift by 2iK' is tried
    let mut s = Sys::new();
    let k = re_im(&timed(&mut s, "elliptic_k(2.5)", 0.5));
    assert!((k.0 - 1.1242019597966657).abs() < 3e-16 && (k.1 + 1.2330149084392663).abs() < 3e-16, "{k:?}");
    timed(&mut s, "plot([inverse_jacobi_cd(-1, m), inverse_jacobi_cd(-1/2, m), inverse_jacobi_cd(0, m)], [m, -5, 5])", 3.0);
}

#[test]
fn appell_f1_with_c_equal_a_is_a_product() {
    // F1(a; b1, b2; a; x, y) = (1 - x)^-b1 (1 - y)^-b2 for every x, y (the plots with x = 4 took 6 s per point)
    let mut s = Sys::new();
    assert_eq!(s.run("appell_f1(a, b1, b2, a, x, y)"), "(1 - x)^-b1*(1 - y)^-b2");
    let got = number(&timed(&mut s, "appell_f1(1, 1, 1, 1, 4, 0.5)", 1.0));
    assert!((got + 2.0 / 3.0).abs() < 1e-15, "{got}");
    // an independent route: the double series at |x|, |y| < 1
    let got = number(&s.run("appell_f1(2, 1/2, 3/4, 2, 0.3, -0.2)"));
    assert!((got - 0.7f64.powf(-0.5) * 1.2f64.powf(-0.75)).abs() < 1e-15, "{got}");
}

#[test]
fn jacobi_functions_at_the_quarter_periods() {
    // DLMF 22.4.3 (Table 22.4.3): values at K, K/2, iK', and the periods
    let mut s = Sys::new();
    assert_eq!(s.run("[jacobi_cd(0, m), jacobi_cd(elliptic_k(m)/2, m), jacobi_cd(elliptic_k(m), m)]"), "[1, (1 + (1 - m)^(1/2))^(-1/2), 0]");
    assert_eq!(s.run("[jacobi_sn(elliptic_k(m), m), jacobi_cn(elliptic_k(m), m), jacobi_dn(elliptic_k(m), m)]"), "[1, 0, (1 - m)^(1/2)]");
    assert_eq!(s.run("[jacobi_ns(im*elliptic_k(1 - m), m), jacobi_dc(im*elliptic_k(1 - m), m), jacobi_sc(im*elliptic_k(1 - m), m)]"), "[0, m^(1/2), 1im]");
    assert_eq!(s.run("jacobi_cd(x, m) == jacobi_cd(x + 4*elliptic_k(m), m)"), "true");
    assert_eq!(s.run("jacobi_sn(x + 2*im*elliptic_k(1 - m), m)"), "jacobi_sn(x, m)");
    assert_eq!(s.run("jacobi_sn(x + elliptic_k(m), m)"), "jacobi_cd(x, m)");
    assert_eq!(s.run("jacobi_dc(x - 5*elliptic_k(m), m)"), "jacobi_ns(x, m)");
    // an independent route: the reduction against the numeric value at every shift (a, b) of all twelve functions
    let src = "fs = [jacobi_sn, jacobi_cn, jacobi_dn, jacobi_cd, jacobi_sd, jacobi_nd, jacobi_dc, jacobi_nc, jacobi_sc, jacobi_ns, jacobi_ds, jacobi_cs]
        k = elliptic_k(0.3)
        kp = elliptic_k(0.7)
        worst = 0
        for i in range(1, 12) {
          f = fs[i]
          for a in range(-5, 6) {
            for b in range(-3, 4) {
              if a != 0 or b != 0 then {
                s1 = f(x + a*elliptic_k(m) + b*im*elliptic_k(1 - m), m)
                red = rewrite(s1, [x -> 0.37, m -> 0.3])
                dir = f(0.37 + a*k + b*im*kp, 0.3)
                worst = max(worst, abs(red - dir)/(1 + abs(dir)))
              }
            }
          }
        }
        worst";
    assert!(number(&timed(&mut s, src, 30.0)) < 1e-12);
}

#[test]
fn elliptic_incomplete_at_half_pi_and_zero() {
    // F(pi/2 | m) = K(m), E(pi/2 | m) = E(m), Pi(n; pi/2 | m) = Pi(n | m), Pi(n; 0 | m) = 0; inverse sn, cn at 1, -1, 0
    let mut s = Sys::new();
    assert_eq!(s.run("[elliptic_f(pi/2, m), elliptic_e(pi/2, m), elliptic_pi(n, 0, m), elliptic_pi(n, pi/2, m)]"), "[elliptic_k(m), elliptic_e(m), 0, elliptic_pi(n, m)]");
    assert_eq!(
        s.run("[inverse_jacobi_cd(0, m), inverse_jacobi_cn(-1, m), inverse_jacobi_ns(-1, m), inverse_jacobi_sn(1, m)]"),
        "[elliptic_k(m), 2*elliptic_k(m), -elliptic_k(m), elliptic_k(m)]"
    );
    // the values invert: cd(K(m)) = 0, cn(2 K) = -1 at m = 0.3
    assert!(number(&s.run("abs(jacobi_cd(elliptic_k(0.3), 0.3))")) < 1e-15);
    assert!((number(&s.run("jacobi_cn(2*elliptic_k(0.3), 0.3)")) + 1.0).abs() < 1e-15);
}

#[test]
fn elliptic_pi_at_complex_machine_arguments() {
    // expected values of elliptic_pi(-2, 1 - 2 im) and elliptic_pi(-1, 2.5 + im, 1/3 - im), checked by an independent
    // route: the integral of 1/((1 - n sin^2) sqrt(1 - m sin^2)) along the straight path from 0
    let mut s = Sys::new();
    let v = re_im(&s.run("elliptic_pi(-2, 1 - 2.0*im)"));
    assert!((v.0 - 0.7840492266613495).abs() < 1e-15 && (v.1 + 0.2584145161936538).abs() < 1e-15, "{v:?}");
    let v = re_im(&s.run("elliptic_pi(-1, 2.5 + 1.0*im, 1/3 - im)"));
    assert!((v.0 - 1.423782108151011).abs() < 1e-14 && (v.1 - 0.10216920379605804).abs() < 1e-14, "{v:?}");
    let w = re_im(&s.run("n_integrate(ph/((1 + sin(t*ph)^2)*sqrt(1 - (1/3 - im)*sin(t*ph)^2)), [t, 0, 1])".replace("ph", "(2.5 + 1.0*im)").as_str()));
    assert!((v.0 - w.0).abs() < 1e-13 && (v.1 - w.1).abs() < 1e-13, "{v:?} {w:?}");
    // the exact Pi(n | 0) = pi/(2 sqrt(1 - n))
    assert_eq!(s.run("elliptic_pi(-1, 0)"), "(1/4)*pi*2^(1/2)");
    // complete and incomplete values at real arguments agree with the real path
    let a = re_im(&s.run("elliptic_pi(0.3, 1.0, 0.3 + 0.0*im)")).0;
    let b = number(&s.run("elliptic_pi(0.3, 1.0, 0.3)"));
    assert!((a - b).abs() < 1e-15, "{a} {b}");
}
