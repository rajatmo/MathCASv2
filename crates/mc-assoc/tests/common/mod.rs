//! The assembled engine (with `mc-assoc` installed): source text in, printed result out.
#![allow(dead_code)]
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// An engine with the default settings, or with `threads` threads and the stress collector.
pub fn engine(threads: usize, stress: bool) -> CoreEngine {
    let mut cfg = EngineConfig { threads, ..EngineConfig::default() };
    if stress {
        cfg.heap = mc_core::Config::stress();
    }
    CoreEngine::with_config(cfg)
}

/// The printed value of `src`, or the error.
pub fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// Evaluate each `(source, printed result)` pair in order in one session.
pub fn check(e: &mut CoreEngine, cases: &[(&str, &str)]) {
    for (src, want) in cases {
        assert_eq!(eval(e, src), *want, "{src}");
    }
}

/// Print every result (for writing expectations).
pub fn show(e: &mut CoreEngine, srcs: &[&str]) {
    for src in srcs {
        println!("({src:?}, {:?}),", eval(e, src));
    }
}
