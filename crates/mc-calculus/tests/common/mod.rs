//! An engine session (with this crate installed by `mc-engine`) driven by source text, and a small deterministic
//! random generator.
#![allow(dead_code)]
use mc_api::{Engine, EngineError, StopFlag};
use std::time::{Duration, Instant};
use mc_engine::{CoreEngine, EngineConfig};

pub struct Sys {
    engine: CoreEngine,
    stop: StopFlag,
}

impl Sys {
    pub fn new() -> Sys {
        Sys::with_threads(1)
    }

    /// An engine with `threads` threads.
    pub fn with_threads(threads: usize) -> Sys {
        let cfg = EngineConfig { threads, ..EngineConfig::default() };
        Sys { engine: CoreEngine::with_config(cfg), stop: StopFlag::new() }
    }

    /// Evaluate `src`; the printed value of its last statement.
    pub fn run(&mut self, src: &str) -> String {
        match self.engine.eval(src, &self.stop) {
            Ok(e) => e.printed,
            Err(err) => panic!("{src}: {err}"),
        }
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

/// A 64-bit linear congruential generator (Knuth's MMIX constants): reproducible test inputs without a crate.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    /// Uniform in 0..n.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// A rational in (lo, hi) with denominator 997, as source text.
    pub fn point(&mut self, lo: f64, hi: f64) -> String {
        let a = (lo * 997.0).ceil() as i64 + 1;
        let b = (hi * 997.0).floor() as i64 - 1;
        let k = a + self.below((b - a + 1) as u64) as i64;
        format!("({k}/997)")
    }
}

/// Parse a printed number (machine or ball digits).
pub fn number(s: &str) -> f64 {
    s.trim_end_matches('.').parse::<f64>().unwrap_or_else(|_| panic!("not a number: {s}"))
}

impl Sys {
    /// Check `integrate(f, x)` by differentiation: the result must contain no `integrate`, and `d(F, x) - f` must
    /// simplify to 0 or vanish numerically (to about 20 digits) at the given points (source text such as
    /// `"(3/7)"`, chosen inside the domain). `Ok` with the printed antiderivative, `Err` with the details.
    pub fn check_antiderivative(&mut self, f: &str, points: &[&str]) -> Result<String, String> {
        let shown = self.run(&format!("ff = {f}; FF = integrate(ff, x); FF"));
        if shown.contains("integrate(") {
            return Err(format!("integrate({f}, x) not found: {shown}"));
        }
        if self.run("simplify(d(FF, x) - ff)") == "0" {
            return Ok(shown);
        }
        for p in points {
            let out = self.run(&format!("n(1 + rewrite((d(FF, x) - ff)*10^20, [x -> {p}]), 30)"));
            let ok = out.trim_end_matches('.').parse::<f64>().is_ok_and(|v| (v - 1.0).abs() < 1.0);
            if !ok {
                return Err(format!("integrate({f}, x) = {shown}: d(F, x) - f at x = {p} is {out}"));
            }
        }
        Ok(shown)
    }
}
