//! An engine session driven by source text, and a parser for printed lists of numbers.
#![allow(dead_code)]
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

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

    /// The numbers of the flat list `src` prints.
    pub fn numbers(&mut self, src: &str) -> Vec<f64> {
        numbers(&self.run(src))
    }
}

/// Parse a printed flat list of numbers `[a, b, ...]`.
pub fn numbers(s: &str) -> Vec<f64> {
    let inner = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')).unwrap_or_else(|| panic!("not a list: {s}"));
    if inner.is_empty() {
        return vec![];
    }
    inner
        .split(", ")
        .map(|x| x.trim_end_matches('.').parse::<f64>().unwrap_or_else(|_| panic!("not a number: {x}")))
        .collect()
}
