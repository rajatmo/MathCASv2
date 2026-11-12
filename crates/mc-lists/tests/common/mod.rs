//! A session of the assembled core with `mc-lists` installed: source text in, printed result out.
#![allow(dead_code)]
use mc_core::Session;
use mc_eval::{EvalConfig, Kernel, Status};
use mc_syntax::Printer;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct Env {
    session: Session<Kernel>,
}

/// The engine's depth limit (decision G.7): debug frames are larger.
const MAX_DEPTH: u32 = if cfg!(debug_assertions) { 2_000 } else { 20_000 };

impl Env {
    pub fn new() -> Env {
        Env::with(1, mc_core::Config::default())
    }

    pub fn with(threads: usize, heap: mc_core::Config) -> Env {
        let stop = Arc::new(AtomicBool::new(false));
        let session = Session::with_state(heap, threads, |h| {
            let mut k = Kernel::new(h, EvalConfig { max_depth: MAX_DEPTH, ..EvalConfig::default() });
            mc_engine::arith::install(&mut k, h, &stop);
            mc_lists::install(&mut k, h, &stop);
            k
        });
        Env { session }
    }

    /// Evaluate the statements of `src` one per step; the printed value of the last, or the status of an
    /// evaluation that stopped.
    pub fn eval(&mut self, src: &str) -> String {
        self.eval_with(src, i64::MAX)
    }

    /// As [`Env::eval`], each statement with this fuel budget.
    pub fn eval_with(&mut self, src: &str, fuel: i64) -> String {
        let base = self.session.state.roots.len();
        let n = self.session.step(|h, k| {
            let ts = mc_syntax::read(h, src).unwrap_or_else(|d| panic!("{src}: {}", d[0].message));
            for &t in &ts {
                k.roots.push(t);
            }
            ts.len()
        });
        let mut out = String::new();
        for i in 0..n {
            let last = i + 1 == n;
            let r = self.session.step(|h, k| {
                let t = k.roots.get(h, base + i);
                let e = k.eval_with(h, t, fuel);
                if e.status != Status::Running {
                    return Some(format!("status {:?}", e.status));
                }
                last.then(|| Printer::with_numbers(h, Box::new(mc_engine::format::ArithNumbers)).expr(e.value))
            });
            if let Some(r) = r {
                out = r;
                if !last {
                    break;
                }
            }
        }
        self.session.state.roots.truncate(base);
        out
    }

    /// The evaluator's control block (interrupts).
    pub fn control(&self) -> Arc<mc_eval::Control> {
        self.session.state.control()
    }

    /// Run `f` on the heap and kernel in one step.
    pub fn step<R: Send>(&mut self, f: impl for<'h> FnOnce(&'h mc_core::Heap, &mut Kernel) -> R + Send) -> R {
        self.session.step(f)
    }
}

/// Evaluate each `(source, printed result)` pair in order in one session.
pub fn check(env: &mut Env, cases: &[(&str, &str)]) {
    for (src, want) in cases {
        assert_eq!(env.eval(src), *want, "{src}");
    }
}
