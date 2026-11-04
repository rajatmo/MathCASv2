//! [`CoreEngine`]: the harness's [`Engine`] on the real core.
//!
//! One call of [`Engine::eval`] reads the whole source with `mc-syntax` (nothing runs if any statement has an
//! error), keeps the statements in the kernel's roots, and evaluates them one by one, each in its own
//! [`Session::step`] so that a safe point follows every statement. The value of the last statement is printed with
//! `mc-syntax`'s printer and `mc-arith`'s number text, fingerprinted structurally, and kept for
//! [`Engine::bind_last`].
//!
//! Names: every input goes through `mc-load`'s [`Loader`] first, which resolves modules and imports at read time
//! (semantics 1.2, package F) and loads the module files imports need; the evaluator sees plain statements on
//! qualified symbols. The engine starts by evaluating the prelude (`lib/prelude.mc`, embedded) unless
//! [`EngineConfig::prelude`] is off.
//!
//! The numeric compiler (`mc-jit`, package 1b) is installed with the libraries: `compile(f)` queues a compiled
//! specialisation, which the engine registers after the statement that made it (registration needs the kernel
//! exclusively), so it applies from the next statement on.
use crate::arith;
use crate::format::ArithNumbers;
use mc_api::names;
use mc_arith::num::StopScope;
use mc_bench::{Engine, EngineError, Evaluated, RaiseHook, StopFlag};
use mc_core::{Session, Store};
use mc_eval::{Control, EvalConfig, Kernel, Status};
use mc_load::{stdlib, LoadError, Loader, Module, Origin, SourceProvider};
use mc_syntax::Printer;
use mc_api::Sym;
use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::path::Path;
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
    /// evaluate the prelude when the engine starts (decision F.7)
    pub prelude: bool,
}

impl Default for EngineConfig {
    /// At most 4 threads (the shared-machine rule), the default heap, and the default evaluator: its frame limit is
    /// sized by memory and its native-callback limit by the pool's stacks (package 1a, replacing decision G.7).
    fn default() -> EngineConfig {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(4);
        EngineConfig { threads, heap: mc_core::Config::default(), eval: EvalConfig::default(), prelude: true }
    }
}

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
    /// the reader state: module table and session imports
    loader: Loader,
    /// names the libraries give a meaning without a definition (constants such as `pi`): known to name
    /// resolution like defined symbols (decision F.16)
    library_names: HashSet<Sym>,
    /// the numeric compiler: specialisations `compile` queued are registered after each statement (package 1b)
    jit: mc_jit::Jit,
}

/// The names of every installed library's functions and constants, and `inf`.
fn library_names(h: &mc_core::Heap) -> HashSet<Sym> {
    let lists = [
        mc_lists::FUNCTIONS,
        mc_assoc::FUNCTIONS,
        mc_numeric::FUNCTIONS,
        mc_numtheory::FUNCTIONS,
        mc_poly::FUNCTIONS,
        mc_jit::FUNCTIONS,
        mc_calculus::FUNCTIONS,
        mc_strings::FUNCTIONS,
        mc_roots::FUNCTIONS,
        mc_quad::FUNCTIONS,
        mc_simplify::FUNCTIONS,
        mc_random::FUNCTIONS,
    ];
    lists.iter().flat_map(|l| l.iter().map(|e| e.0)).chain(["inf"]).map(|n| h.sym(n)).collect()
}

/// What a script or input produced, in order: `print` output and the failures of top-level statements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// a line of `print` output
    Output(String),
    /// a top-level statement whose value is a `failure(...)`, printed
    Failure(String),
}

/// The result of [`CoreEngine::eval_in`] or [`CoreEngine::eval_file`].
#[derive(Clone, Debug)]
pub struct Script {
    /// output and failures in the order they happened
    pub events: Vec<Event>,
    /// the value of the last statement (its `output` holds the `print` output of the whole run)
    pub last: Evaluated,
    /// whether the last value is `null` (also when there are no statements)
    pub last_is_null: bool,
}

impl Script {
    /// Whether some top-level statement's value was a failure.
    pub fn failed(&self) -> bool {
        self.events.iter().any(|e| matches!(e, Event::Failure(_)))
    }
}

/// Why [`CoreEngine::eval_in`] or [`CoreEngine::eval_file`] did not run to the end.
#[derive(Clone, Debug)]
pub enum RunError {
    /// a read, resolve or load error: nothing was evaluated
    Read(LoadError),
    /// the evaluation stopped (interrupt, limit)
    Engine(EngineError),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Read(e) => write!(f, "{e}"),
            RunError::Engine(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RunError {}

/// What one read takes its source from.
enum Input<'a> {
    Text(&'a str, &'a Origin),
    File(&'a Path),
}

impl CoreEngine {
    /// An engine with the default settings.
    pub fn new() -> CoreEngine {
        CoreEngine::with_config(EngineConfig::default())
    }

    /// An engine with these settings.
    pub fn with_config(cfg: EngineConfig) -> CoreEngine {
        let arith_stop = Arc::new(AtomicBool::new(false));
        let mut jit = None;
        let session = Session::with_state(cfg.heap, cfg.threads, |h| {
            let mut k = Kernel::new(h, cfg.eval);
            arith::install(&mut k, h, &arith_stop);
            // the libraries poll the same stop flag as the arithmetic (decision G.5)
            mc_lists::install(&mut k, h, &arith_stop);
            mc_assoc::install(&mut k, h);
            mc_numeric::install_with_stop(&mut k, h, &arith_stop);
            mc_numtheory::install(&mut k, h);
            // mc-poly owns `gcd` and `lcm` and sends exact numbers to mc-numtheory's half-gcd (decision 9.1): no two
            // libraries register one name, so the order of these calls does not matter
            mc_poly::install(&mut k, h, &arith_stop);
            mc_calculus::install(&mut k, h, &arith_stop);
            jit = Some(mc_jit::install(&mut k, h));
            mc_strings::install(&mut k, h);
            mc_roots::install(&mut k, h, &arith_stop);
            mc_quad::install(&mut k, h, &arith_stop);
            mc_simplify::install(&mut k, h, &arith_stop);
            // the session generator: fresh per engine, seeded from the operating system (decision RNG.2)
            let rng = mc_random::install(&mut k, h);
            // compiled functions draw from the same generator (package N4b)
            jit.as_ref().expect("the compiler is installed").set_rng(rng);
            k.roots.push(h.sym_term(h.sym(names::NULL)));
            k
        });
        let ctl = session.state.control();
        let library_names = library_names(&session.heap);
        let jit = jit.expect("the compiler is installed");
        let mut e = CoreEngine {
            session,
            ctl,
            arith_stop,
            hook: None,
            has_last: false,
            loader: Loader::default(),
            library_names,
            jit,
        };
        if cfg.prelude {
            let origin = Origin::embedded(stdlib::PRELUDE_ORIGIN);
            if let Err(err) = e.eval_in(stdlib::PRELUDE, &origin, &StopFlag::new()) {
                // the prelude is part of the binary and tested; failing here is a build defect
                panic!("the prelude does not load: {err}");
            }
            // the prelude's value is not a result of the session
            e.has_last = false;
        }
        e
    }

    /// Evaluate `src` from `origin` (a file name, `<repl>`, `<-e>`; its directory is where `import` looks for
    /// module files first). The whole text is read and resolved before any of it runs (decision F.6).
    pub fn eval_in(&mut self, src: &str, origin: &Origin, stop: &StopFlag) -> Result<Script, RunError> {
        self.evaluate(Input::Text(src, origin), stop, &mut |_| {})
    }

    /// [`CoreEngine::eval_in`], also handing every event to `sink` as soon as its statement has finished, before
    /// the next statement runs (the kernel protocol streams output this way).
    pub fn eval_streaming(
        &mut self,
        src: &str,
        origin: &Origin,
        stop: &StopFlag,
        sink: &mut dyn FnMut(&Event),
    ) -> Result<Script, RunError> {
        self.evaluate(Input::Text(src, origin), stop, sink)
    }

    /// Evaluate the file at `path` as a script: the same as [`CoreEngine::eval_in`] on its text, from its
    /// directory. A module file evaluated again replaces the old definitions of its module (decision F.8).
    pub fn eval_file(&mut self, path: &Path, stop: &StopFlag) -> Result<Script, RunError> {
        self.evaluate(Input::File(path), stop, &mut |_| {})
    }

    /// Read script and module files through `provider` (the file system by default; tests use
    /// [`mc_load::InMemory`]).
    pub fn set_sources(&mut self, provider: Box<dyn SourceProvider>) {
        self.loader.set_provider(provider);
    }

    /// The loaded modules, by name.
    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.loader.modules()
    }

    /// The numeric compiler of the session (package 1b).
    pub fn jit(&self) -> &mc_jit::Jit {
        &self.jit
    }

    /// Whether symbol `name` has a native implementation (for tests of the prelude).
    pub fn has_native(&mut self, name: &str) -> bool {
        self.session.step(|h, k| k.has_native(h.sym(name)))
    }

    /// The names a completion can offer, sorted: every defined symbol with a plain name (natives, the prelude,
    /// session definitions), the libraries' names without definitions (constants), the loaded modules and their
    /// public members as `M.f`. Qualified private members and names that are not identifiers are left out.
    pub fn names(&mut self) -> Vec<String> {
        let library = &self.library_names;
        let mut names: BTreeSet<String> = self.session.step(|h, k| {
            (0..h.sym_count() as u32)
                .map(Sym)
                .filter(|&s| k.is_defined(s) || library.contains(&s))
                .map(|s| h.sym_name(s))
                .filter(|n| is_identifier(n))
                .collect()
        });
        for m in self.loader.modules() {
            names.insert(m.name.clone());
            names.extend(m.public.iter().map(|p| m.qualified(p)));
        }
        names.into_iter().collect()
    }

    fn evaluate(
        &mut self,
        input: Input<'_>,
        stop: &StopFlag,
        sink: &mut dyn FnMut(&Event),
    ) -> Result<Script, RunError> {
        self.attach(stop);
        // withdraw an old request before looking at the flag: a raise after this check reaches the evaluation
        self.ctl.clear_interrupt();
        self.arith_stop.store(false, Relaxed);
        if stop.is_raised() {
            return Err(RunError::Engine(EngineError::Stopped));
        }
        let (loader, names) = (&mut self.loader, &self.library_names);
        let n = self.session.step(|h, k| {
            k.clear_output();
            let read = {
                let known = |s| k.is_defined(s) || names.contains(&s);
                match input {
                    Input::Text(src, origin) => loader.read(h, src, origin, &known),
                    Input::File(path) => loader.read_file(h, path, &known),
                }
            }
            .map_err(RunError::Read)?;
            // a module defined again: its old clauses go first (decision F.8)
            for name in &read.reloaded {
                k.clear(h.sym(name));
            }
            // and the argument-head rules it attached to other symbols (decision 9.6)
            for (holder, outer) in &read.detached {
                k.clear_attached(h.sym(holder), h.sym(outer));
            }
            for &t in &read.statements {
                k.roots.push(t);
            }
            Ok(read.statements.len())
        })?;
        let r = if n == 0 {
            let last = Evaluated { printed: names::NULL.into(), fingerprint: None, output: vec![] };
            Ok(Script { events: vec![], last, last_is_null: true })
        } else {
            self.run(n, stop, sink).map_err(RunError::Engine)
        };
        self.session.state.roots.truncate(LAST + 1);
        r
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

    /// Evaluate statements `1..=n` of the roots; the last one's value is the result. Output is collected after
    /// each statement, so that it stays in order with the failures of statements.
    fn run(&mut self, n: usize, stop: &StopFlag, sink: &mut dyn FnMut(&Event)) -> Result<Script, EngineError> {
        let mut events = vec![];
        for i in 1..=n {
            let last = i == n;
            let (arith_stop, jit) = (&self.arith_stop, &self.jit);
            let r = self.session.step(|h, k| {
                let t = k.roots.get(h, i);
                let e = k.eval(h, t);
                // what `compile` queued takes effect from the next statement on
                jit.flush(k);
                match e.status {
                    Status::Interrupted => return Err(EngineError::Stopped),
                    Status::Fuel => return Err(EngineError::Limit("fuel".into())),
                    Status::Depth => return Err(EngineError::Limit("evaluation depth".into())),
                    // the statuses of compiled code are ordinary failures, already in the value
                    Status::Running | Status::Index | Status::DivZero => {}
                }
                let output = output(h, k);
                k.clear_output();
                let failure = is_failure(h, e.value);
                if !last && !failure {
                    return Ok((output, None, None));
                }
                // printing a huge number takes long: its decimal conversion polls the stop flag (decision G.6)
                let _scope = StopScope::enter(Some(arith_stop));
                let printed = Printer::with_numbers(h, Box::new(ArithNumbers)).expr(e.value);
                let fail = failure.then(|| printed.clone());
                if !last {
                    return Ok((output, fail, None));
                }
                k.roots.set(LAST, e.value);
                let is_null = is_sym(h, e.value, names::NULL);
                let fingerprint = Some(mc_core::shapes::fingerprint(h, e.value));
                Ok((output, fail, Some((printed, fingerprint, is_null))))
            });
            // an interrupt can also end inside arithmetic, as `failure(interrupted, ...)` in a finished value
            if stop.is_raised() {
                return Err(EngineError::Stopped);
            }
            let (output, fail, value) = r?;
            let start = events.len();
            events.extend(output.into_iter().map(Event::Output));
            events.extend(fail.map(Event::Failure));
            events[start..].iter().for_each(&mut *sink);
            if let Some((printed, fingerprint, last_is_null)) = value {
                self.has_last = true;
                let output = events
                    .iter()
                    .filter_map(|e| match e {
                        Event::Output(s) => Some(s.clone()),
                        Event::Failure(_) => None,
                    })
                    .collect();
                let last = Evaluated { printed, fingerprint, output };
                return Ok(Script { events, last, last_is_null });
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

/// The session output since it was last cleared: strings as their text, other terms printed.
fn output(h: &mc_core::Heap, k: &Kernel) -> Vec<String> {
    k.output(h)
        .into_iter()
        .map(|t| match h.view(t) {
            mc_core::View::Str(s) => s.to_string(),
            _ => Printer::with_numbers(h, Box::new(ArithNumbers)).expr(t),
        })
        .collect()
}

/// Whether `name` is a plain identifier (a letter or `_`, then letters, digits, `_`): the names completion offers.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_') && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Whether `t` is the symbol named `name`.
fn is_sym(h: &mc_core::Heap, t: mc_core::T<'_>, name: &str) -> bool {
    matches!(h.view(t), mc_core::View::Sym(s) if h.sym_name(s) == name)
}

/// Whether `t` is `failure(...)`.
fn is_failure(h: &mc_core::Heap, t: mc_core::T<'_>) -> bool {
    match h.view(t) {
        mc_core::View::App { head, .. } => is_sym(h, head, names::FAILURE),
        _ => false,
    }
}

/// A read error as the harness's error: an error in the evaluated text itself is a syntax error at its offset (the
/// first diagnostic's message, with the others rendered after it); one in a module file it loaded is rendered with
/// its file and position.
fn engine_error(e: RunError) -> EngineError {
    match e {
        RunError::Engine(e) => e,
        RunError::Read(e) if e.in_input && !e.diagnostics.is_empty() => {
            let first = &e.diagnostics[0];
            let mut message = first.message.clone();
            for d in &e.diagnostics[1..] {
                message.push_str("\n  ");
                message.push_str(&d.render(&e.text));
            }
            EngineError::Syntax { message, offset: first.span.start as usize }
        }
        RunError::Read(e) => EngineError::Other(e.render()),
    }
}

impl Engine for CoreEngine {
    fn name(&self) -> String {
        format!("mathcas-v2 {}", env!("CARGO_PKG_VERSION"))
    }

    fn eval(&mut self, src: &str, stop: &StopFlag) -> Result<Evaluated, EngineError> {
        self.eval_in(src, &Origin::input(), stop).map(|r| r.last).map_err(engine_error)
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
