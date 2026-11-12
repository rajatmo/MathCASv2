//! Startup cost: engine construction with and without the prelude, median of 20 each (item F requires the
//! prelude to cost under 10 ms in release). Run with
//! `cargo run --release --offline -p mc-engine --example startup`; prints `[k/N pct%]` progress.
use mc_api::Engine;
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

const RUNS: usize = 20;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn main() {
    let configs = [("without prelude", false), ("with prelude", true)];
    let total = RUNS * configs.len();
    let mut done = 0;
    let mut medians = vec![];
    for (name, prelude) in configs {
        let mut times = Vec::with_capacity(RUNS);
        for _ in 0..RUNS {
            let cfg = EngineConfig { prelude, ..EngineConfig::default() };
            let t = Instant::now();
            let e = CoreEngine::with_config(cfg);
            times.push(t.elapsed());
            drop(e);
            done += 1;
            if done % 10 == 0 {
                eprintln!("[{done}/{total} {}%]", 100 * done / total);
            }
        }
        let m = median(times);
        println!("{name}: median {:.2} ms over {RUNS} constructions", m.as_secs_f64() * 1e3);
        medians.push(m);
    }
    println!("prelude cost: {:.2} ms", (medians[1].as_secs_f64() - medians[0].as_secs_f64()) * 1e3);
    // the cost of one small input, reading and resolution included
    let mut e = CoreEngine::new();
    let stop = mc_api::StopFlag::new();
    let times: Vec<Duration> = (0..200)
        .map(|_| {
            let t = Instant::now();
            e.eval("1 + 1", &stop).expect("evaluates");
            t.elapsed()
        })
        .collect();
    println!("eval(\"1 + 1\"): median {:.1} us over 200", median(times).as_secs_f64() * 1e6);
}
