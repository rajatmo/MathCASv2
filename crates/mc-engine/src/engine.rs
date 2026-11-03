//! [`CoreEngine`]: the harness's [`Engine`] on the real core.
//!
//! One call of [`Engine::eval`] reads the whole source with `mc-syntax` (nothing runs if any statement has an
//! error), keeps the statements in the kernel's roots, and evaluates them one by one, each in its own
//! [`Session::step`] so that a safe point follows every statement. The value of the last statement is printed with
//! `mc-syntax`'s printer and `mc-arith`'s number text, fingerprinted structurally, and kept for
//! [`Engine::bind_last`].
//!
//! Names: `mc-syntax` lowers bare names to bare symbols and `M.f` to the symbol named `"M.f"`; module and import
//! resolution (semantics 1.2) is not done yet (decision G.3).
use crate::arith;
use crate::format::ArithNumbers;
use mc_api::names;
use mc_arith::num::StopScope;
use mc_bench::{Engine, EngineError, Evaluated, RaiseHook, StopFlag};
use mc_core::{Session, Store};
use mc_eval::{Control, EvalConfig, Kernel, Status};
use mc_syntax::{Diagnostic, Printer};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;

/// Settings of an engine.
#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    /// worker threads of the session (speculative parallel evaluation)
    pub threads: usize,
    /// the heap and its collector
    pub heap: mc_core::Config,
    /// the evaluator
    pub eval: EvalConfig,
}

impl Default for EngineConfig {
    /// At most 4 threads (the shared-machine rule), the default heap, and the default evaluator with a depth
    /// limit that fits the pool's stacks in this build (decision G.7).
    fn default() -> EngineConfig {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(4);
        let eval = EvalConfig { max_depth: MAX_DEPTH, ..EvalConfig::default() };
        EngineConfig { threads, heap: mc_core::Config::default(), eval }
    }
}

/// The evaluation depth limit: `mc-eval`'s default where its frames are small (optimised builds); unoptimised
/// frames are several times larger, and a rule's right side costs a few evaluation levels per call, so the 64 MB
/// stacks of the session's pool hold fewer levels.
const MAX_DEPTH: u32 = if cfg!(debug_assertions) { 2_000 } else { 20_000 };

/// Root slot of the last result; the statements of the running evaluation follow it.
const LAST: usize = 0;

/// The engine: a session of the evaluator with `mc-arith`'s arithmetic installed.
pub struct CoreEngine {
    session: Session<Kernel>,
    ctl: Arc<Control>,
    /// the flag `mc-arith` polls in long numeric loops
    arith_stop: Arc<AtomicBool>,
    /// forwards the caller's stop flag to `ctl` and `arith_stop` (decision G.5)
    hook: Option<RaiseHook>,
    /// whether a result is in the `LAST` root
    has_last: bool,
}

impl CoreEngine {
    /// An engine with the default settings.
    pub fn new() -> CoreEngine {
        CoreEngine::with_config(EngineConfig::default())
    }

    /// An engine with these settings.
    pub fn with_config(cfg: EngineConfig) -> CoreEngine {
        let arith_stop = Arc::new(AtomicBool::new(false));
        let session = Session::with_state(cfg.heap, cfg.threads, |h| {
            let mut k = Kernel::new(h, cfg.eval);
            arith::install(&mut k, h, &arith_stop);
            // the libraries poll the same stop flag as the arithmetic (decision G.5)
            mc_lists::install(&mut k, h, &arith_stop);
            mc_numeric::install_with_stop(&mut k, h, &arith_stop);
            // after any integer gcd: mc-poly's gcd also handles integers and rationals (K.5)
            mc_poly::install(&mut k, h, &arith_stop);
            k.roots.push(h.sym_term(h.sym(names::NULL)));
            k
        });
        let ctl = session.state.control();
        CoreEngine { session, ctl, arith_stop, hook: None, has_last: false }
    }

    /// Run `f` on the kernel between evaluations, for example to register more natives (a library's `install`).
    pub fn install(&mut self, f: impl for<'h> FnOnce(&mut Kernel, &'h mc_core::Heap) + Send) {
        self.session.step(|h, k| f(k, h));
    }

    /// Route raises of `stop` to the evaluator's interrupt and `mc-arith`'s flag.
    fn attach(&mut self, stop: &StopFlag) {
        if self.hook.as_ref().is_some_and(|h| h.flag().same(stop)) {
            return;
        }
        let (ctl, arith_stop) = (self.ctl.clone(), self.arith_stop.clone());
        self.hook = Some(stop.on_raise(move || {
            ctl.interrupt();
            arith_stop.store(true, Relaxed);
        }));
    }

    /// Evaluate statements `1..=n` of the roots; the last one's value is the result.
    fn run(&mut self, n: usize, stop: &StopFlag) -> Result<Evaluated, EngineError> {
        for i in 1..=n {
            let last = i == n;
            let arith_stop = &self.arith_stop;
            let r = self.session.step(|h, k| {
                let t = k.roots.get(h, i);
                let e = k.eval(h, t);
                match e.status {
                    Status::Interrupted => return Err(EngineError::Stopped),
                    Status::Fuel => return Err(EngineError::Limit("fuel".into())),
                    Status::Depth => return Err(EngineError::Limit("evaluation depth".into())),
                    // the statuses of compiled code are ordinary failures, already in the value
                    Status::Running | Status::Index | Status::DivZero => {}
                }
                if !last {
                    return Ok(None);
                }
                k.roots.set(LAST, e.value);
                // printing a huge number takes long: its decimal conversion polls the stop flag (decision G.6)
                let _scope = StopScope::enter(Some(arith_stop));
                let printed = Printer::with_numbers(h, Box::new(ArithNumbers)).expr(e.value);
                let output = output(h, k);
                Ok(Some(Evaluated { printed, fingerprint: Some(mc_core::shapes::fingerprint(h, e.value)), output }))
            });
            // an interrupt can also end inside arithmetic, as `failure(interrupted, ...)` in a finished value
            if stop.is_raised() {
                return Err(EngineError::Stopped);
            }
            if let Some(v) = r? {
                self.has_last = true;
                return Ok(v);
            }
        }
        unreachable!("run is called with at least one statement")
    }
}

impl Default for CoreEngine {
    fn default() -> CoreEngine {
        CoreEngine::new()
    }
}

/// The session output of this call to [`Engine::eval`] (it is cleared at the start of each): strings as their
/// text, other terms printed.
fn output(h: &mc_core::Heap, k: &Kernel) -> Vec<String> {
    k.output(h)
        .into_iter()
        .map(|t| match h.view(t) {
            mc_core::View::Str(s) => s.to_string(),
            _ => Printer::with_numbers(h, Box::new(ArithNumbers)).expr(t),
        })
        .collect()
}

/// The first diagnostic as the error (its message and offset; the caller renders the position), with the others
/// rendered after it.
fn syntax_error(diags: &[Diagnostic], src: &str) -> EngineError {
    let first = &diags[0];
    let mut message = first.message.clone();
    for d in &diags[1..] {
        message.push_str("\n  ");
        message.push_str(&d.render(src));
    }
    EngineError::Syntax { message, offset: first.span.start as usize }
}

impl Engine for CoreEngine {
    fn name(&self) -> String {
        format!("mathcas-v2 {}", env!("CARGO_PKG_VERSION"))
    }

    fn eval(&mut self, src: &str, stop: &StopFlag) -> Result<Evaluated, EngineError> {
        self.attach(stop);
        // withdraw an old request before looking at the flag: a raise after this check reaches the evaluation
        self.ctl.clear_interrupt();
        self.arith_stop.store(false, Relaxed);
        if stop.is_raised() {
            return Err(EngineError::Stopped);
        }
        let n = self.session.step(|h, k| {
            k.clear_output();
            let terms = mc_syntax::read(h, src).map_err(|d| syntax_error(&d, src))?;
            for &t in &terms {
                k.roots.push(t);
            }
            Ok(terms.len())
        })?;
        let r = if n == 0 {
            Ok(Evaluated { printed: names::NULL.into(), fingerprint: None, output: vec![] })
        } else {
            self.run(n, stop)
        };
        self.session.state.roots.truncate(LAST + 1);
        r
    }

    /// Settings: `memo` `on` or `off` (memoisation of normal forms, `EvalConfig::memo`).
    fn configure(&mut self, option: &str, value: &str) -> Result<(), EngineError> {
        let memo = match (option, value) {
            ("memo", "on") => true,
            ("memo", "off") => false,
            _ => return Err(EngineError::Other(format!("unknown engine setting {option}: {value}"))),
        };
        let k = &mut self.session.state;
        k.set_config(EvalConfig { memo, ..*k.config() });
        Ok(())
    }

    fn bind_last(&mut self, name: &str) -> Result<(), EngineError> {
        if !self.has_last {
            return Err(EngineError::Other("no result to bind yet".into()));
        }
        self.session.step(|h, k| {
            let v = k.roots.get(h, LAST);
            k.set_own(h.sym(name), v);
        });
        Ok(())
    }
}
