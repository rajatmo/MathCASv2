//! A session of the evaluator with `mc-arith`'s arithmetic (as the engine installs it) and this crate: read
//! source text, evaluate statement by statement, print the last value.
#![allow(dead_code)] // each test crate uses a part of these helpers
use mc_core::Session;
use mc_eval::{EvalConfig, Kernel};
use mc_syntax::Printer;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct Engine {
    session: Session<Kernel>,
    stop: Arc<AtomicBool>,
}

impl Engine {
    pub fn new() -> Engine {
        Engine::with_threads(1)
    }

    pub fn with_threads(threads: usize) -> Engine {
        let stop = Arc::new(AtomicBool::new(false));
        let session = Session::with_state(mc_core::Config::default(), threads, |h| {
            let depth = if cfg!(debug_assertions) { 2_000 } else { 20_000 };
            let mut k = Kernel::new(h, EvalConfig { max_depth: depth, ..EvalConfig::default() });
            mc_engine::arith::install(&mut k, h, &stop);
            mc_numeric::install_with_stop(&mut k, h, &stop);
            k
        });
        Engine { session, stop }
    }

    /// The flag the numeric loops poll and the evaluator's interrupt, as the engine raises them together.
    pub fn stopper(&self) -> (Arc<AtomicBool>, Arc<mc_eval::Control>) {
        (self.stop.clone(), self.session.state.control())
    }

    /// The printed value of the last statement of `src`.
    pub fn eval(&mut self, src: &str) -> String {
        let n = self.session.step(|h, k| {
            let terms = mc_syntax::read(h, src).unwrap_or_else(|d| panic!("syntax error in {src}: {}", d[0].message));
            for &t in &terms {
                k.roots.push(t);
            }
            terms.len()
        });
        let base = self.session.state.roots.len() - n;
        let mut out = String::new();
        for i in 0..n {
            let last = i + 1 == n;
            let printed = self.session.step(|h, k| {
                let t = k.roots.get(h, base + i);
                let e = k.eval(h, t);
                last.then(|| Printer::with_numbers(h, Box::new(mc_engine::format::ArithNumbers)).expr(e.value))
            });
            if let Some(p) = printed {
                out = p;
            }
        }
        self.session.state.roots.truncate(base);
        out
    }
}
