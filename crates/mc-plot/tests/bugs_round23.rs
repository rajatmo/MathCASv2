//! Round 23: bugs found where several crates meet; a test that still fails is kept ignored until its bug is fixed,
//! and removing the `ignore` is the proof of a fix.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Instant;

#[test]
fn x23_12_date_list_plot_huge_date_component() {
    for src in [
        "time_constrained(date_list_plot([[[2010, 1, 4], 3.0], [[2010, 2^64, 5], 1.0]]), 2)",
        "time_constrained(date_list_plot([[[2010, 1, 4], 3.0], [[2^64, 1, 5], 1.0]]), 2)",
    ] {
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let stop = StopFlag::new();
        let flag = stop.clone();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let dog = std::thread::spawn(move || {
            if rx.recv_timeout(std::time::Duration::from_secs(30)).is_err() {
                flag.raise();
            }
        });
        let t0 = Instant::now();
        let _ = e.eval(src, &stop);
        let took = t0.elapsed().as_secs_f64();
        let _ = tx.send(());
        let _ = dog.join();
        assert!(took < 4.0, "{src}: took {took} s");
    }
}
