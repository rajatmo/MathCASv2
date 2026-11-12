//! Round 24: regression tests for the fixes without an earlier ignored test (D23-11, D23-22, D23-24,
//! D23-26, B23-23 hankel and jacobi ratios, B23-25, X23-4).
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

fn val(src: &str) -> String {
    ev(src).0
}

#[test]
fn d23_11_product_log_branch_minus_one_argument() {
    agree("re(product_log(-1.0))", "re(product_log(-1))", 1e-12);
    assert!(val("product_log(-1.0, -0.1)").starts_with("-3.5771520639572"));
    let v = val("n(product_log(-1))");
    assert!(v.contains("-0.3181315052047641") && v.contains("1.337235701430689"), "{v}");
}

#[test]
fn d23_26_kelvin_closed_forms_are_numbers() {
    assert_eq!(val("kelvin_ker(1e5)"), "0.0");
    assert!(!val("n(kelvin_ker(2, 5/2), 30)").contains("re("));
}

#[test]
fn d23_24_hermite_overflow_and_slow_cases() {
    assert!(val("hermite_h(1000, 0.5)").contains("-inf"));
    let (_, t) = ev("hermite_h(2.5, 100.0)");
    assert!(t < 5.0, "took {t} s");
}


#[test]
fn b23_25_jacobi_complex_is_fast() {
    for d in [12, 20, 30] {
        let (v, t) = ev(&format!("n(jacobi_sn(10 + im, 7/10), {d})"));
        assert!(t < 2.0 && !v.contains("ball("), "{d}: {v} in {t} s");
    }
}

#[test]
fn d23_22_jacobi_huge_argument() {
    let (v, t) = ev("n(jacobi_sn(10^20, 1/2), 20)");
    assert!(v.starts_with("-0.9627234515103996784"), "{v}");
    assert!(t < 5.0);
}

#[test]
fn x23_4_gamma_regularized_huge_limit() {
    let (_, t) = ev("time_constrained(gamma_regularized(1.0e300, 1.0e300), 2)");
    assert!(t < 4.0, "took {t} s");
    let (_, t) = ev("time_constrained(gamma_regularized(2.0^1023, [2.0^1023, 1.0, 2.0]), 2)");
    assert!(t < 4.0, "took {t} s");
}
#[test]
fn b23_23_hankel_and_jacobi_ratios_have_their_digits() {
    // the 30-digit value agrees with the 60-digit one to 27 digits (compared as printed digits)
    let digits = |s: String| s.trim_start_matches('-').replace('.', "").chars().take(27).collect::<String>();
    for (e30, e60) in [
        ("n(hankel_h2(1, 655/7 - 188/100*im), 30)", "n(hankel_h2(1, 655/7 - 188/100*im), 60)"),
        ("n(hankel_h2(3, 456/10 - 123/100*im), 30)", "n(hankel_h2(3, 456/10 - 123/100*im), 60)"),
        ("n(jacobi_cd(5 - 3*im, 1/3), 30)", "n(jacobi_cd(5 - 3*im, 1/3), 60)"),
    ] {
        for part in ["re", "re(-im*"] {
            let close = if part == "re" { ")" } else { "))" };
            let a = digits(val(&format!("{part}({e30}{close}")));
            let b = digits(val(&format!("{part}({e60}{close}")));
            assert_eq!(a, b, "{e30} {part}");
        }
    }
}
