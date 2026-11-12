//! Round 23: bugs found where several crates meet; a test that still fails is kept ignored until its bug is fixed,
//! and removing the `ignore` is the proof of a fix.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Instant;

fn timed(src: &str) -> (String, f64) {
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

#[test]
fn x23_9_integrate_huge_power_times_exp_aborts() {
    let (v, _) = timed("time_constrained(integrate(x^(2^31)*exp(-x), x), 2)");
    assert!(!v.starts_with("error"), "{v}");
}

#[test]
fn x23_10_huge_exponent_limits() {
    for src in [
        "time_constrained(integrate(sqrt(x^(2^31)), x), 2)",
        "time_constrained(fourier_transform(exp(-u^(2^31)), u, x), 2)",
        "time_constrained(inverse_fourier_transform(exp(-u^(2^31) - v^2), [u, v], [x, y]), 2)",
    ] {
        let (_, t) = timed(src);
        assert!(t < 4.0, "{src}: took {t} s");
    }
}
