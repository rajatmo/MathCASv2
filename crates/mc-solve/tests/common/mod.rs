//! An engine session (with this crate installed by `mc-engine`) driven by source text, and a small deterministic
//! random generator.
#![allow(dead_code)]
use mc_api::{Engine, EngineError, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

pub struct Sys {
    engine: CoreEngine,
    stop: StopFlag,
}

impl Sys {
    pub fn new() -> Sys {
        Sys { engine: CoreEngine::new(), stop: StopFlag::new() }
    }

    /// Evaluate `src`; the printed value of its last statement.
    pub fn run(&mut self, src: &str) -> String {
        match self.engine.eval(src, &self.stop) {
            Ok(e) => e.printed,
            Err(err) => panic!("{src}: {err}"),
        }
    }

    /// Evaluate `src` and parse the printed value as a machine real.
    pub fn number(&mut self, src: &str) -> f64 {
        let s = self.run(src);
        s.trim_end_matches('.').parse::<f64>().unwrap_or_else(|_| panic!("{src}: not a number: {s}"))
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
    let mut bad = vec![];
    for (src, want) in cases {
        let got = s.run(src);
        if got != *want {
            bad.push(format!("{src}
   got:  {got}
   want: {want}"));
        }
    }
    assert!(bad.is_empty(), "{} mismatches:
{}", bad.len(), bad.join("
"));
}

/// A 64-bit linear congruential generator: reproducible test inputs without a crate.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    /// Uniform in lo..=hi.
    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}
