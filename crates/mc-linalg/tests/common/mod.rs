//! An engine session (with this crate installed by `mc-engine`) driven by source text, and helpers to read printed
//! numbers back.
#![allow(dead_code)]
use mc_api::{Engine, EngineError, StopFlag};
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

    /// `src` must evaluate to `true`.
    pub fn holds(&mut self, src: &str) {
        assert_eq!(self.run(src), "true", "{src}");
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

/// Evaluate each `(source, printed result)` pair in one session.
pub fn check(cases: &[(&str, &str)]) {
    let mut s = Sys::new();
    for (src, want) in cases {
        assert_eq!(s.run(src), *want, "{src}");
    }
}
