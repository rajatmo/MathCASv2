//! An engine session (with this crate installed by `mc-engine`) driven by source text, and a parser for printed
//! lists of machine numbers.
#![allow(dead_code)]
use mc_api::{Engine, StopFlag};
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

    /// The numbers of the printed (possibly nested) list `src` evaluates to, flattened.
    pub fn numbers(&mut self, src: &str) -> Vec<f64> {
        let out = self.run(src);
        numbers(&out).unwrap_or_else(|| panic!("{src}: not a list of numbers: {out}"))
    }

    /// The one number `src` evaluates to.
    pub fn number(&mut self, src: &str) -> f64 {
        let out = self.run(src);
        number(&out).unwrap_or_else(|| panic!("{src}: not a number: {out}"))
    }
}

/// A printed number: an integer, a rational p/q, a machine real, `inf`, `-inf`.
pub fn number(s: &str) -> Option<f64> {
    let s = s.trim();
    match s {
        "inf" => return Some(f64::INFINITY),
        "-inf" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    if let Some((p, q)) = s.split_once('/') {
        return Some(p.trim().parse::<f64>().ok()? / q.trim().parse::<f64>().ok()?);
    }
    s.parse().ok()
}

/// The numbers of a printed list, flattened.
pub fn numbers(s: &str) -> Option<Vec<f64>> {
    s.replace(['[', ']'], " ").split(',').filter(|x| !x.trim().is_empty()).map(number).collect()
}

/// The relative difference of a value from a reference (absolute below `tiny`).
pub fn rel(v: f64, want: f64) -> f64 {
    if v == want {
        return 0.0;
    }
    let scale = want.abs().max(1e-300);
    (v - want).abs() / scale
}
