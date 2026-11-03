//! A session of the assembled core with this crate's natives installed: read source text, evaluate statement by
//! statement (a safe point after each), print the last value. It mirrors `mc-engine`'s engine, which cannot take
//! extra natives yet.
#![allow(dead_code)]
use mc_api::names;
use mc_core::{Session, Store};
use mc_eval::{EvalConfig, Kernel, Status};
use mc_syntax::Printer;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// A session; statements are evaluated with unlimited fuel.
pub struct Nt {
    session: Session<Kernel>,
}

impl Nt {
    /// A session on 4 threads (the shared-machine rule).
    pub fn new() -> Nt {
        Nt::with_threads(4)
    }

    pub fn with_threads(threads: usize) -> Nt {
        let stop = Arc::new(AtomicBool::new(false));
        let session = Session::with_state(mc_core::Config::default(), threads, |h| {
            let cfg =
                EvalConfig { max_depth: if cfg!(debug_assertions) { 2_000 } else { 20_000 }, ..EvalConfig::default() };
            let mut k = Kernel::new(h, cfg);
            mc_engine::arith::install(&mut k, h, &stop);
            mc_numtheory::install(&mut k, h);
            k.roots.push(h.sym_term(h.sym(names::NULL)));
            k
        });
        Nt { session }
    }

    /// Evaluate the statements of `src`; the printed value of the last one (or the status when it stopped).
    pub fn eval(&mut self, src: &str) -> String {
        let n = self.session.step(|h, k| {
            let terms = mc_syntax::read(h, src).unwrap_or_else(|d| panic!("syntax error in {src:?}: {}", d[0].message));
            for &t in &terms {
                k.roots.push(t);
            }
            terms.len()
        });
        let mut out = String::from(names::NULL);
        for i in 1..=n {
            let last = i == n;
            let r = self.session.step(|h, k| {
                let t = k.roots.get(h, i);
                let e = k.eval(h, t);
                match e.status {
                    Status::Running | Status::Index | Status::DivZero => {}
                    s => return Some(format!("status {s:?}")),
                }
                last.then(|| Printer::with_numbers(h, Box::new(mc_engine::format::ArithNumbers)).expr(e.value))
            });
            if let Some(s) = r {
                out = s;
                if out.starts_with("status") {
                    break;
                }
            }
        }
        self.session.state.roots.truncate(1);
        out
    }
}

/// Evaluate each input in one session and compare the printed result.
pub fn check_all(cases: &[(&str, &str)]) {
    let mut nt = Nt::new();
    for (src, want) in cases {
        assert_eq!(nt.eval(src), *want, "{src}");
    }
}

impl Nt {
    /// Evaluate one statement with a fuel budget: the status and the printed value.
    pub fn eval_fuel(&mut self, src: &str, fuel: i64) -> (Status, String) {
        self.session.step(|h, k| {
            let t = mc_syntax::read(h, src).expect("one statement")[0];
            let e = k.eval_with(h, t, fuel);
            (e.status, Printer::with_numbers(h, Box::new(mc_engine::format::ArithNumbers)).expr(e.value))
        })
    }

    /// The control block (interrupts from other threads).
    pub fn control(&self) -> Arc<mc_eval::Control> {
        self.session.state.control()
    }
}
