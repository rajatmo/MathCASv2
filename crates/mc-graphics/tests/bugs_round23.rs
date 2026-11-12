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
fn x23_8_deeply_nested_graphics_obeys_limit() {
    for f in ["graphics_grid", "panel"] {
        let (_, t) = timed(&format!("time_constrained({f}(nest(x => [x, 1], 1.5, 50000)), 2)"));
        assert!(t < 4.0, "{f}: took {t} s");
    }
}
