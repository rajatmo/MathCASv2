//! An engine session (with this crate installed by `mc-engine`) driven by source text, and readers of printed
//! results.
#![allow(dead_code)]
use mc_bench::{Engine, EngineError, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

pub struct Sys {
    engine: CoreEngine,
    stop: StopFlag,
}

impl Sys {
    pub fn new() -> Sys {
        Sys::with_threads(1, false)
    }

    /// An engine with `threads` threads, under the stress collector if `stress`.
    pub fn with_threads(threads: usize, stress: bool) -> Sys {
        let mut cfg = EngineConfig { threads, ..EngineConfig::default() };
        if stress {
            cfg.heap = mc_core::Config::stress();
        }
        Sys { engine: CoreEngine::with_config(cfg), stop: StopFlag::new() }
    }

    /// Evaluate `src`; the printed value of its last statement.
    pub fn run(&mut self, src: &str) -> String {
        match self.engine.eval(src, &self.stop) {
            Ok(e) => e.printed,
            Err(err) => panic!("{src}: {err}"),
        }
    }

    /// The machine real `src` prints.
    pub fn real(&mut self, src: &str) -> f64 {
        let out = self.run(src);
        out.parse().unwrap_or_else(|_| panic!("{src}: not a real: {out}"))
    }

    /// Evaluate `src`, raising the stop flag from another thread after `after`; the result and the time it took.
    pub fn interrupted(&mut self, src: &str, after: Duration) -> (Result<String, EngineError>, Duration) {
        let stop = self.stop.clone();
        let start = Instant::now();
        let t = std::thread::spawn(move || {
            while start.elapsed() < after {
                std::thread::yield_now();
            }
            stop.raise();
        });
        let r = self.engine.eval(src, &self.stop).map(|e| e.printed);
        let took = start.elapsed();
        t.join().unwrap();
        self.stop.clear();
        (r, took)
    }
}

/// Assert |got - want| <= tol * max(1, |want|).
pub fn close(src: &str, got: f64, want: f64, tol: f64) {
    assert!((got - want).abs() <= tol * want.abs().max(1.0), "{src}: {got} vs {want}");
}

/// The digits two decimal texts of numbers below 1 share (after the leading `0.`).
pub fn agreeing(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count().saturating_sub(2)
}

/// A reference value file of `bench/cases/L/ref`.
pub fn reference(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/cases/L/ref").join(name);
    std::fs::read_to_string(path).expect("reference file").trim().to_string()
}
