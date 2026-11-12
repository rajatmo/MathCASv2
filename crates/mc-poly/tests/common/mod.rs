//! An evaluator session with `mc-arith`'s arithmetic and this crate's natives, driven by source text.
#![allow(dead_code)]
use mc_core::{Session, Store};
use mc_eval::{EvalConfig, Kernel};
use mc_syntax::Printer;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct Sys {
    session: Session<Kernel>,
}

impl Sys {
    pub fn new() -> Sys {
        let stop = Arc::new(AtomicBool::new(false));
        let session = Session::with_state(mc_core::Config::default(), 1, |h| {
            let cfg = EvalConfig { max_depth: 2000, ..EvalConfig::default() };
            let mut k = Kernel::new(h, cfg);
            mc_engine::arith::install(&mut k, h, &stop);
            mc_poly::install(&mut k, h, &stop);
            k
        });
        Sys { session }
    }

    /// Evaluate every statement of `src`; the printed value of the last one.
    pub fn run(&mut self, src: &str) -> String {
        let n = self.session.step(|h, k| {
            let terms = mc_syntax::read(h, src).unwrap_or_else(|d| panic!("syntax error in {src:?}: {}", d[0].message));
            k.roots.truncate(0);
            for &t in &terms {
                k.roots.push(t);
            }
            terms.len()
        });
        let mut out = String::new();
        for i in 0..n {
            let last = i + 1 == n;
            out = self.session.step(|h, k| {
                let t = k.roots.get(h, i);
                let e = k.eval(h, t);
                k.roots.set(i, e.value);
                if last {
                    Printer::with_numbers(h, Box::new(mc_engine::format::ArithNumbers)).expr(e.value)
                } else {
                    String::new()
                }
            });
        }
        out
    }
}

impl Sys {
    /// Give the symbol `name` the value that `build` constructs.
    pub fn define(&mut self, name: &str, build: impl for<'h> FnOnce(&'h mc_core::Heap) -> mc_api::T<'h> + Send) {
        self.session.step(|h, k| {
            let v = build(h);
            k.set_own(h.sym(name), v);
        });
    }
    /// Look at the value of the symbol `name`.
    pub fn inspect<R: Send>(
        &mut self,
        name: &str,
        f: impl for<'h> FnOnce(&'h mc_core::Heap, mc_api::T<'h>) -> R + Send,
    ) -> R {
        self.session.step(|h, k| {
            let v = k.value_of(h, h.sym(name)).expect("a defined symbol");
            f(h, v)
        })
    }
    /// The control block (to interrupt from another thread).
    pub fn control(&self) -> std::sync::Arc<mc_eval::Control> {
        self.session.state.control()
    }
}

/// Evaluate `src` in a fresh session.
pub fn eval(src: &str) -> String {
    Sys::new().run(src)
}
