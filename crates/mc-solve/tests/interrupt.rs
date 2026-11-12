//! Long reductions stop on an interrupt.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

#[test]
fn reduce_of_a_huge_degree_stops() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let stop = StopFlag::new();
    let flag = stop.clone();
    let start = Instant::now();
    let t = std::thread::spawn(move || {
        while start.elapsed() < Duration::from_millis(300) {
            std::thread::yield_now();
        }
        flag.raise();
    });
    let r = e.eval("reduce(x^1234567 + 5*x + 1 == 0, x, reals)", &stop);
    let took = start.elapsed();
    t.join().unwrap();
    assert!(took < Duration::from_secs(3), "took {took:?}: {:?}", r.map(|v| v.printed));
}
