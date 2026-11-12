//! Round 23: bugs found where several crates meet; a test that still fails is kept ignored until its bug is fixed,
//! and removing the `ignore` is the proof of a fix. Expected values: the same function of the exact argument at 40 digits.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Instant;

fn ev(src: &str) -> (String, f64) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let stop = StopFlag::new();
    let flag = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let dog = std::thread::spawn(move || {
        if rx.recv_timeout(std::time::Duration::from_secs(60)).is_err() {
            flag.raise();
        }
    });
    let t0 = Instant::now();
    let out = match e.eval(src, &stop) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    };
    let took = t0.elapsed().as_secs_f64();
    let _ = tx.send(());
    let _ = dog.join();
    (out, took)
}

fn num(s: &str) -> f64 {
    s.trim().replace(' ', "").parse::<f64>().unwrap_or_else(|_| panic!("not a real number: {s}"))
}

/// The machine value of `machine` against the 40-digit value of `exact`: relative error under `tol`.
fn agree(machine: &str, exact: &str, tol: f64) {
    let m = num(&ev(machine).0);
    let x = num(&ev(&format!("n({exact}, 40)")).0);
    let err = (m - x).abs() / x.abs().max(1e-300);
    assert!(err < tol, "{machine} = {m}, exact {x}, relative error {err:e}");
}

#[test]
fn x23_1_legendre_p_machine_degree() {
    agree("legendre_p(100, 0.5)", "legendre_p(100, 1/2)", 1e-9);
    agree("legendre_p(50, 0.9)", "legendre_p(50, 9/10)", 1e-9);
    agree("legendre_p(30, 0.9)", "legendre_p(30, 9/10)", 1e-12);
    agree("legendre_p(100, -1, 0.5)", "legendre_p(100, -1, 1/2)", 1e-9);
    agree("legendre_p(100, 3, 0.5)", "legendre_p(100, 3, 1/2)", 1e-9);
    agree("legendre_q(60, 0.5)", "legendre_q(60, 1/2)", 1e-12);
}

#[test]
fn x23_2_chebyshev_machine_degree() {
    agree("chebyshev_t(100, 0.9)", "chebyshev_t(100, 9/10)", 1e-9);
    agree("chebyshev_u(100, 0.9)", "chebyshev_u(100, 9/10)", 1e-9);
}

#[test]
fn x23_3_laguerre_jacobi_machine_degree() {
    agree("laguerre_l(60, 3.5)", "laguerre_l(60, 7/2)", 1e-9);
    agree("laguerre_l(100, 20.5)", "laguerre_l(100, 41/2)", 1e-9);
    agree("jacobi_p(80, 2, 3, 0.3)", "jacobi_p(80, 2, 3, 3/10)", 1e-9);
    agree("jacobi_p(50, 1/2, 1/3, 0.9)", "jacobi_p(50, 1/2, 1/3, 9/10)", 1e-12);
    // the value is complex (e^(i m phi)): compare the real and the imaginary part
    agree("re(spherical_harmonic_y(30, 5, 0.7, 0.4))", "re(spherical_harmonic_y(30, 5, 7/10, 2/5))", 1e-12);
    agree("re(-im*spherical_harmonic_y(30, 5, 0.7, 0.4))", "re(-im*spherical_harmonic_y(30, 5, 7/10, 2/5))", 1e-12);
}

#[test]
fn x23_4_gamma_regularized_huge_limit() {
    let (_, t) = ev("time_constrained(gamma_regularized(1.0e300, 1.0e300), 2)");
    assert!(t < 4.0, "took {t} s");
    let (_, t) = ev("time_constrained(gamma_regularized(2.0^1023, [2.0^1023, 1.0, 2.0]), 2)");
    assert!(t < 4.0, "took {t} s");
}

#[test]
fn x23_5_legendre_large_degree() {
    let (v, _) = ev("legendre_p(1000, 2, 0.5)");
    assert!(!v.contains("indeterminate"), "{v}");
    let (_, t) = ev("time_constrained(legendre_p(1000, -3, range(0.0, 1.0, 0.125)), 2)");
    assert!(t < 4.0, "took {t} s");
    let (_, t) = ev("time_constrained(legendre_p(1.0e300, sqrt(2)), 2)");
    assert!(t < 4.0, "took {t} s");
}

#[test]
fn x23_6_huge_arguments_limits() {
    for src in [
        "time_constrained(hypergeometric_0f1_regularized(2, 2.0^1023), 2)",
        "time_constrained(beta_regularized(interval([1.2, 2]), 2, 2^63), 2)",
    ] {
        let (_, t) = ev(src);
        assert!(t < 4.0, "{src}: took {t} s");
    }
}

#[test]
fn x23_7_hypergeometric_u_ball() {
    let (v, _) = ev("n(hypergeometric_u(5, 3, 1/100), 40)");
    assert!(!v.contains("ball("), "{v}");
}
