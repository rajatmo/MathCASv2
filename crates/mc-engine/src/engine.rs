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
use crate::history;
use mc_api::names;
use mc_arith::num::StopScope;
use mc_bench::{Engine, EngineError, Evaluated, RaiseHook, StopFlag};
use mc_core::{Session, Store};
use mc_eval::{guard, Control, EvalConfig, Kernel, OperatorForm, Status};
use mc_load::{stdlib, LoadError, Loader, Module, Origin, SourceProvider};
use mc_syntax::Printer;
use mc_api::Sym;
use std::collections::{BTreeMap, HashSet};
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
    /// print values for reading: differences and divisions instead of `+ -c*x` and negative powers (the front
    /// ends' setting; the default prints text that reads back as the same unevaluated term, decision p6core.19)
    pub display: bool,
}

impl Default for EngineConfig {
    /// At most 4 threads (the shared-machine rule), the default heap, and the default evaluator: its frame limit is
    /// sized by memory and its native-callback limit by the pool's stacks (package 1a, replacing decision G.7).
    fn default() -> EngineConfig {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(4);
        EngineConfig { threads, heap: mc_core::Config::default(), eval: EvalConfig::default(), prelude: true, display: false }
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
    /// the line counter of numbered evaluations: the number the next statement gets (08-notebook 2.6)
    line: u64,
    /// [`EngineConfig::display`]
    display: bool,
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
        mc_ode::FUNCTIONS,
        mc_graphics::FUNCTIONS,
        mc_geom::FUNCTIONS,
        mc_plot::FUNCTIONS,
        mc_io::FUNCTIONS,
        mc_simplify::FUNCTIONS,
        mc_random::FUNCTIONS,
        mc_special::FUNCTIONS,
        mc_linalg::FUNCTIONS,
        mc_solve::FUNCTIONS,
        mc_dist::FUNCTIONS,
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

/// The functions without the `listable` attribute that map over list arguments themselves, elementwise (the
/// elementary, special and number-theory functions keep their own list fast paths): they also thread over the values
/// of associations, `sin(assoc(a -> x))` is `assoc(a -> sin(x))` (decision io-core.1). A test checks each one maps
/// lists elementwise.
pub const MAPS_LISTS: &[&str] = &[
        "abs", "abs_arg", "acos", "acosh", "acot", "acoth", "acsc", "acsch", "airy_ai", "airy_ai_prime", "airy_bi",
        "airy_bi_prime", "arg", "asec", "asech", "asin", "asinh", "atan", "atanh", "bessel_i", "bessel_i_scaled",
        "bessel_j", "bessel_k", "bessel_k_scaled", "bessel_y", "beta", "beta_regularized", "binomial", "ceiling",
        "chebyshev_t", "chebyshev_u", "chop", "clip", "conjugate", "cos", "cos_integral", "cosh", "cosh_integral",
        "cot", "coth", "csc", "csch", "dawson_f", "denominator", "digamma", "dirac_delta", "divisor_sigma",
        "elliptic_e", "elliptic_f", "elliptic_k", "elliptic_nome_q", "elliptic_pi", "erf", "erfc", "erfi",
        "euler_phi", "exp", "exp_integral_e", "exp_integral_ei", "expm1", "factorial", "factorial2",
        "factorial_power", "fibonacci", "floor", "fractional_part", "fresnel_c", "fresnel_s", "gamma",
        "gamma_regularized", "gegenbauer_c", "gudermannian", "hankel_h1", "hankel_h2", "harmonic_number",
        "heaviside_theta", "hermite_h", "hyperfactorial", "hypergeometric_0f1", "hypergeometric_0f1_regularized",
        "hypergeometric_1f1", "hypergeometric_2f1", "im_part", "integer_exponent", "integer_length", "integer_part",
        "inverse_erf", "inverse_erfc", "inverse_gudermannian", "inverse_jacobi_sn", "is_prime", "jacobi_cd",
        "jacobi_cn", "jacobi_cs", "jacobi_dc", "jacobi_dn", "jacobi_ds", "jacobi_nc", "jacobi_nd", "jacobi_ns",
        "jacobi_sc", "jacobi_sd", "jacobi_sn", "jacobi_symbol", "laguerre_l", "legendre_p", "legendre_q", "log",
        "log10", "log1p", "log2", "log_gamma", "log_integral", "lucas_l", "mod", "moebius_mu", "multinomial",
        "multiplicative_order", "next_prime", "numerator", "pochhammer", "polygamma", "polylog", "power_mod", "prime",
        "prime_pi", "primitive_root", "product_log", "quotient", "ramp", "rationalize", "re", "re_im", "real_abs",
        "real_sign", "round", "sec", "sech", "sign", "sin", "sin_integral", "sinc", "sinh", "sinh_integral", "sqrt",
        "subfactorial", "tan", "tanh", "unit_box", "unit_step", "unit_triangle", "zeta",
];

/// The functions with an operator form `f(a..)(x..)` and where its outer arguments go (decision p6core.2).
pub const OPERATOR_FORMS: &[(&str, OperatorForm)] = {
    use OperatorForm::{DataFirst as F, DataLast as L};
    &[
        ("select", F), ("append", F), ("prepend", F), ("cases", F), ("delete_cases", F), ("count", F),
        ("position", F), ("select_first", F), ("key_drop", F), ("key_take", F), ("key_exists", F), ("lookup", F),
        ("group_by", F), ("count_by", F), ("sort_by", F), ("max_by", F), ("min_by", F), ("all_true", F),
        ("any_true", F), ("none_true", F), ("member", F), ("member_of", F), ("free_of", F), ("rewrite", F),
        ("replace", F), ("replace_part", F), ("extract", F), ("insert", F), ("delete", F), ("key_select", F),
        ("match_q", F), ("string_replace", F), ("string_contains", F), ("string_starts", F), ("string_ends", F),
        ("string_split", F), ("string_cases", F), ("string_delete", F), ("gather_by", F), ("unique_by", F),
        ("take_largest", F), ("take_smallest", F), ("first_case", F),
        ("contains_any", F), ("contains_all", F), ("contains_none", F), ("key_member", F),
        ("map", L), ("apply", L), ("map_indexed", L), ("fold", L), ("fold_list", L), ("scan", L),
        ("key_map", L), ("key_value_map", L), ("assoc_map", L), ("map_thread", L),
        ("map_apply", L), ("comap", L), ("comap_apply", L), ("map_all", L), ("replace_at", F), ("contains_only", F),
        ("contains_exactly", F), ("take_largest_by", F),
        ("take_smallest_by", F), ("reverse_sort_by", F),
    ]
};

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
            mc_ode::install(&mut k, h, &arith_stop);
            mc_graphics::install(&mut k, h);
            mc_geom::install(&mut k, h);
            mc_plot::install(&mut k, h);
            mc_io::install(&mut k, h);
            mc_io::register_exporter("graphics", &["png", "svg", "pdf", "gif"], mc_plot::export);
            mc_io::register_exporter("graphics3d", &["png", "svg", "pdf", "gif"], mc_plot::export);
            // labeled and legended values around non-pictures: the value with its label
            mc_io::register_exporter("labeled", &["png", "svg", "pdf"], mc_plot::export);
            mc_io::register_exporter("legended", &["png", "svg", "pdf"], mc_plot::export);
            // a list of graphics or plots as an animated GIF
            mc_io::register_exporter("list", &["gif"], mc_plot::export);
            mc_simplify::install(&mut k, h, &arith_stop);
            mc_special::install(&mut k, h, &arith_stop);
            mc_linalg::install(&mut k, h, &arith_stop);
            mc_solve::install(&mut k, h, &arith_stop);
            // the session generator: fresh per engine, seeded from the operating system (decision RNG.2)
            let rng = mc_random::install(&mut k, h);
            // compiled functions draw from the same generator (package N4b)
            // invariant: `jit` was set two statements above, in the same closure
            jit.as_ref().expect("the compiler is installed").set_rng(rng.clone());
            // distributions draw their variates from the same generator (package 3D)
            mc_dist::install(&mut k, h, rng);
            // a list is data: a failure is kept as an element, not propagated to the whole list (semantics 10), so a
            // table with a few failed entries can still be filtered (`select(xs, x => not is_failure(x))`)
            let list = h.sym(names::LIST);
            let a = k.attributes(list);
            k.set_attributes(list, mc_eval::Attributes { handles_failures: true, ..a });
            // attributes as values (decision p6core.26)
            crate::attrs::install(&mut k, h);
            crate::compose::install(&mut k, h);
            // `exp(x)` is `e^x` (decision p6core.5)
            arith::canonical_exp(&mut k, h);
            // functions of one argument on packed arrays work on the payload (gap G48, decision sp-core.3)
            crate::packed::install(&mut k, h);
            // operator forms: `select(f)(xs)` is `select(xs, f)`, `map(f)(xs)` is `map(f, xs)` (decision p6core.2)
            for &(name, form) in OPERATOR_FORMS {
                k.set_operator_form(h.sym(name), form);
            }
            // elementwise functions thread over associations (decision io-core.1)
            for &name in MAPS_LISTS {
                k.set_maps_lists(h.sym(name));
            }
            // quantities and uncertain numbers keep their arithmetic in list reductions (decision io-core.5)
            for name in mc_numeric::uncertain::HEADS {
                k.set_value_head(h.sym(name));
            }
            // the notebook protocol's history (`out`) and library warnings (`message`)
            crate::history::install(&mut k, h);
            k.roots.push(h.sym_term(h.sym(names::NULL)));
            k
        });
        let ctl = session.state.control();
        let library_names = library_names(&session.heap);
        // invariant: the session's state closure always sets `jit`
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
            line: 1,
            display: cfg.display,
        };
        if cfg.prelude {
            let origin = Origin::embedded(stdlib::PRELUDE_ORIGIN);
            if let Err(err) = e.eval_in(stdlib::PRELUDE, &origin, &StopFlag::new()) {
                // invariant: the prelude is part of the binary and tested; failing here is a build defect, not an input's
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
        self.evaluate(Input::Text(src, origin), stop, None, &mut |_| {})
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
        let mut legacy = |s: Step<'_>| match s {
            Step::Event(e) => sink(e),
            Step::Report(_) => {}
        };
        self.evaluate(Input::Text(src, origin), stop, None, &mut legacy)
    }

    /// [`CoreEngine::eval_streaming`] with the statements numbered as in [`CoreEngine::eval_each`] (with
    /// `options.numbered`): `out(n)`, `%` and `%%` see the values of earlier statements and cells (gap G77).
    pub fn eval_streaming_with(
        &mut self,
        src: &str,
        origin: &Origin,
        stop: &StopFlag,
        options: EachOptions,
        sink: &mut dyn FnMut(&Event),
    ) -> Result<Script, RunError> {
        if !options.numbered {
            return self.eval_streaming(src, origin, stop, sink);
        }
        let suppressed = guard::catch(|| suppressed(src)).unwrap_or_default();
        let mut legacy = |s: Step<'_>| match s {
            Step::Event(e) => sink(e),
            Step::Report(_) => {}
        };
        let plan = Plan { numbered: true, suppressed, terms: vec![] };
        let r = self.evaluate(Input::Text(src, origin), stop, Some(plan), &mut legacy);
        if matches!(r, Err(RunError::Read(_))) {
            self.line += 1;
        }
        r
    }

    /// Evaluate `src` from `origin` as [`CoreEngine::eval_streaming`] does, reporting every top-level statement of
    /// `src` (08-notebook 5.2): a [`Report::Value`] for each statement whose value is shown (not `null`, which
    /// definitions give, and not followed by `;`), a [`Report::Failure`] for each failure, `print` output one
    /// report per call, warnings of library code as [`Report::Message`]. All reports of a statement are handed to
    /// `sink` when it finishes, before the next one starts.
    ///
    /// With [`EachOptions::numbered`], each statement of `src` takes the next number of the session's line counter
    /// ([`CoreEngine::line`]) when it starts, its reports carry it, and its value (unless `null`) is kept for
    /// `out(n)`. A read error then takes one number (nothing runs); an interrupted or stopped statement keeps its.
    /// Returns whether some statement's value was a failure.
    pub fn eval_each(
        &mut self,
        src: &str,
        origin: &Origin,
        stop: &StopFlag,
        options: EachOptions,
        sink: &mut dyn FnMut(Report),
    ) -> Result<bool, RunError> {
        // a panic in the parser is reported by the guarded read that follows
        let suppressed = guard::catch(|| suppressed(src)).unwrap_or_default();
        let mut each = |s: Step<'_>| match s {
            Step::Report(r) => sink(r),
            Step::Event(_) => {}
        };
        let plan = Plan { numbered: options.numbered, suppressed, terms: vec![] };
        let r = self.evaluate(Input::Text(src, origin), stop, Some(plan), &mut each);
        if options.numbered && matches!(r, Err(RunError::Read(_))) {
            self.line += 1;
        }
        r.map(|s| s.failed())
    }

    /// The line counter: the number the next statement of a numbered evaluation gets (1 in a fresh engine).
    pub fn line(&self) -> u64 {
        self.line
    }

    /// Evaluate the file at `path` as a script: the same as [`CoreEngine::eval_in`] on its text, from its
    /// directory. A module file evaluated again replaces the old definitions of its module (decision F.8).
    pub fn eval_file(&mut self, path: &Path, stop: &StopFlag) -> Result<Script, RunError> {
        self.evaluate(Input::File(path), stop, None, &mut |_| {})
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
        self.name_kinds().into_iter().map(|(n, _)| n).collect()
    }

    /// [`CoreEngine::names`] with what each name is (for colouring in editors): a module, a value (an own value,
    /// or a library constant), a function (a native or rules), or a symbol with only attributes.
    pub fn name_kinds(&mut self) -> Vec<(String, NameKind)> {
        let library = &self.library_names;
        let kind = |h: &mc_core::Heap, k: &Kernel, s: Sym| {
            if k.value_of(h, s).is_some() || (library.contains(&s) && !k.is_defined(s)) {
                NameKind::Value
            } else if k.has_native(s) || k.rule_count(s) > 0 {
                NameKind::Function
            } else {
                NameKind::Symbol
            }
        };
        let mut names: BTreeMap<String, NameKind> = self.session.step(|h, k| {
            (0..h.sym_count() as u32)
                .map(Sym)
                .filter(|&s| k.is_defined(s) || library.contains(&s))
                .map(|s| (h.sym_name(s), s))
                .filter(|(n, _)| is_identifier(n))
                .map(|(n, s)| (n, kind(h, k, s)))
                .collect()
        });
        let members: Vec<String> = self.loader.modules().flat_map(|m| m.public.iter().map(|p| m.qualified(p))).collect();
        let modules: Vec<String> = self.loader.modules().map(|m| m.name.clone()).collect();
        let kinds: Vec<NameKind> = self.session.step(|h, k| members.iter().map(|q| kind(h, k, h.sym(q))).collect());
        names.extend(members.into_iter().zip(kinds));
        for m in modules {
            names.insert(m, NameKind::Module);
        }
        names.into_iter().collect()
    }

    fn evaluate(
        &mut self,
        input: Input<'_>,
        stop: &StopFlag,
        mut plan: Option<Plan>,
        sink: &mut dyn FnMut(Step<'_>),
    ) -> Result<Script, RunError> {
        self.attach(stop);
        // withdraw an old request before looking at the flag: a raise after this check reaches the evaluation
        self.ctl.clear_interrupt();
        self.arith_stop.store(false, Relaxed);
        if stop.is_raised() {
            return Err(RunError::Engine(EngineError::Stopped));
        }
        let (loader, names) = (&mut self.loader, &self.library_names);
        let (n, ends) = self.session.step(|h, k| {
            k.clear_output();
            // the reader is guarded like an evaluation: a panic in it reports an internal error, nothing runs
            let read = guard::catch(|| {
                let known = |s| k.is_defined(s) || names.contains(&s);
                match input {
                    Input::Text(src, origin) => loader.read(h, src, origin, &known),
                    Input::File(path) => loader.read_file(h, path, &known),
                }
            })
            .map_err(|note| RunError::Engine(EngineError::Other(format!("internal error while reading: {}", note.text()))))?
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
            Ok((read.statements.len(), read.ends))
        })?;
        if let Some(p) = plan.as_mut() {
            p.lay_out(n, &ends, self.line);
        }
        let r = if n == 0 {
            let last = Evaluated { printed: names::NULL.into(), fingerprint: None, output: vec![] };
            Ok(Script { events: vec![], last, last_is_null: true })
        } else {
            self.run(n, stop, plan.as_ref(), sink).map_err(RunError::Engine)
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
    /// each statement, so that it stays in order with the failures of statements. With a `plan`
    /// ([`CoreEngine::eval_each`]) the statements of the input are numbered, their values kept for `out(n)` and
    /// reported one by one; `sink` gets both kinds of step, the events of scripts and the reports of `eval_each`.
    fn run(
        &mut self,
        n: usize,
        stop: &StopFlag,
        plan: Option<&Plan>,
        sink: &mut dyn FnMut(Step<'_>),
    ) -> Result<Script, EngineError> {
        let mut events = vec![];
        // the number in `out#line`
        let mut current: Option<u64> = None;
        for i in 1..=n {
            let last = i == n;
            let term = plan.map_or(TermPlan::default(), |p| p.terms[i - 1]);
            // a numbered statement takes its number when its first term starts
            let new_line = term.number.filter(|&nr| Some(nr) != current);
            if let Some(nr) = new_line {
                self.line = nr + 1;
                current = Some(nr);
            }
            let (arith_stop, jit, display) = (&self.arith_stop, &self.jit, self.display);
            let r = self.session.step(|h, k| {
                // the evaluation is guarded by the evaluator itself; this guard covers printing, rendering and the
                // compiler's registration
                guard::catch(|| -> Result<Finished, EngineError> {
                    if let Some(nr) = new_line {
                        k.set_own(h.sym(history::LINE), h.int(nr as i64));
                    }
                    let t = k.roots.get(h, i);
                    let assigned = assigned_symbol(h, t);
                    let e = k.eval(h, t);
                    // what `compile` queued takes effect from the next statement on
                    jit.flush(k);
                    match e.status {
                        Status::Interrupted => return Err(EngineError::Stopped),
                        Status::Fuel => return Err(EngineError::Limit("fuel".into())),
                        Status::Depth => return Err(EngineError::Limit("evaluation depth".into())),
                        Status::Memory => return Err(EngineError::Limit("memory".into())),
                        Status::Iterations => return Err(EngineError::Limit("iterations".into())),
                        // the statuses of compiled code are ordinary failures, already in the value
                        Status::Running | Status::Index | Status::DivZero => {}
                    }
                    let output = output(h, k, &|| stop.is_raised(), display);
                    k.clear_output();
                    let failure = is_failure(h, e.value);
                    let is_null = is_sym(h, e.value, names::NULL);
                    // the history: the value of every numbered statement but `null`, shown or not; an assignment
                    // `x = v` (whose value is `null`) records the value assigned (gap G86)
                    let recorded = if is_null { assigned.and_then(|s| k.value_of(h, s)) } else { Some(e.value) };
                    if let (Some(nr), Some(_), Some(v)) = (term.number, term.ends, recorded) {
                        k.set_own(h.sym(&history::entry(nr)), v);
                    }
                    let show = !failure && !is_null && term.ends == Some(true);
                    let mut done = Finished { output, fail: None, value: None, last: None };
                    if !last && !failure && !show {
                        return Ok(done);
                    }
                    // printing a huge number takes long: its decimal conversion polls the stop flag (decision G.6)
                    let _scope = StopScope::enter(Some(arith_stop));
                    let printed = shown(h, e.value, &|| stop.is_raised(), display);
                    done.fail = failure.then(|| printed.clone());
                    // `print(x)` returns x: a statement whose value its last output line already shows does not show it
                    // again (the scripts' rule, decision 9.5 of the library-gaps package)
                    // a graphics value is shown as its picture, like a plot (the value itself stays the graphics term)
                    let picture =
                        if (show || last) && !failure { mc_plot::render_to_files(h, e.value, &|| stop.is_raised()) } else { None };
                    let printed = match &picture {
                        Some((path, ..)) => format!("image(\"{path}\")"),
                        None => printed,
                    };
                    if show && !echoes(done.output.last(), &printed) {
                        let image = picture
                            .or_else(|| mc_plot::image_of(h, e.value))
                            .map(|(path, width, height)| Image { path, width, height });
                        done.value = Some((printed.clone(), image));
                    }
                    if last {
                        k.roots.set(LAST, e.value);
                        let fingerprint = Some(mc_core::shapes::fingerprint(h, e.value));
                        done.last = Some((printed, fingerprint, is_null));
                    }
                    Ok(done)
                })
                .unwrap_or_else(|note| Ok(internal(h, k, &note, last, display)))
            });
            // an interrupt can also end inside arithmetic, as `failure(interrupted, ...)` in a finished value
            if stop.is_raised() {
                return Err(EngineError::Stopped);
            }
            let Finished { output, fail, value, last } = r?;
            // the number of the statement the term belongs to
            let n = term.number;
            let start = events.len();
            for o in output {
                let (event, report) = match o {
                    Emitted::Text(s) => (Event::Output(s.clone()), Report::Output { n, text: s }),
                    Emitted::Message(tag, text) => {
                        (Event::Output(format!("{tag}: {text}")), Report::Message { n, tag, text })
                    }
                };
                events.push(event);
                sink(Step::Report(report));
            }
            if let Some(printed) = fail {
                events.push(Event::Failure(printed.clone()));
                sink(Step::Report(Report::Failure { n, printed }));
            }
            events[start..].iter().for_each(|e| sink(Step::Event(e)));
            if let Some((printed, image)) = value {
                sink(Step::Report(Report::Value { n, printed, image }));
            }
            if let Some((printed, fingerprint, last_is_null)) = last {
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
        // invariant: `run` is called with at least one statement and returns at the last
        unreachable!("run is called with at least one statement")
    }
}

/// What a statement whose evaluation, printing or rendering panicked hands back: `failure(internal, text)` as its
/// failure (and as the last value), with the output it made before the panic.
fn internal(h: &mc_core::Heap, k: &mut Kernel, note: &guard::PanicNote, last: bool, display: bool) -> Finished {
    let v = h.app(h.sym_term(h.sym(names::FAILURE)), &[h.sym_term(h.sym(guard::INTERNAL)), h.string(&note.text())]);
    // the output itself may be what failed to print
    let output = guard::catch(|| output(h, k, &|| false, display)).unwrap_or_default();
    k.clear_output();
    let printed = shown(h, v, &|| false, display);
    let last = last.then(|| {
        k.roots.set(LAST, v);
        (printed.clone(), Some(mc_core::shapes::fingerprint(h, v)), false)
    });
    Finished { output, fail: Some(printed), value: None, last }
}

/// What one statement's step hands back: its output, its failure printed, its shown value (printed, and the image
/// it names), and for the last statement its value printed, fingerprinted and whether it is `null`.
struct Finished {
    output: Vec<Emitted>,
    fail: Option<String>,
    value: Option<(String, Option<Image>)>,
    last: Option<(String, Option<u64>, bool)>,
}

/// One item of the session output.
enum Emitted {
    /// `print` output: a string as its text, another term printed
    Text(String),
    /// `message(tag, text)`
    Message(String, String),
}

/// What [`CoreEngine::run`] hands its sink: an event of scripts, or a report of [`CoreEngine::eval_each`].
enum Step<'a> {
    Event(&'a Event),
    Report(Report),
}

/// What a name is, for [`CoreEngine::name_kinds`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameKind {
    /// a native or a symbol with rules
    Function,
    /// a symbol with an own value, or a library constant
    Value,
    /// a loaded module
    Module,
    /// a defined symbol with neither (attributes only)
    Symbol,
}

impl NameKind {
    /// The kind's name in the kernel protocol.
    pub fn as_str(self) -> &'static str {
        match self {
            NameKind::Function => "function",
            NameKind::Value => "value",
            NameKind::Module => "module",
            NameKind::Symbol => "symbol",
        }
    }
}

/// Options of [`CoreEngine::eval_each`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EachOptions {
    /// number the statements with the session's line counter and keep their values for `out(n)`
    pub numbered: bool,
}

/// What [`CoreEngine::eval_each`] reports, in order. `n` is the statement's number in a numbered evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Report {
    /// the text of one `print` call (it may contain line ends)
    Output {
        /// the number of the statement that printed
        n: Option<u64>,
        /// the text
        text: String,
    },
    /// a warning of library code (`message(tag, text)`): the function's name and the text
    Message {
        /// the number of the statement that warned
        n: Option<u64>,
        /// who warns: a function's name
        tag: String,
        /// the warning
        text: String,
    },
    /// a statement's value is shown
    Value {
        /// the statement's number
        n: Option<u64>,
        /// the value in the concrete syntax
        printed: String,
        /// the picture the value names, when it is an image term of a readable PNG file
        image: Option<Image>,
    },
    /// a top-level statement's value is a `failure(...)`
    Failure {
        /// the statement's number
        n: Option<u64>,
        /// the failure in the concrete syntax
        printed: String,
    },
}

/// A picture a value names: `image("path")` of a PNG file and its size in pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// the file, as the value names it
    pub path: String,
    /// width in pixels
    pub width: u32,
    /// height in pixels
    pub height: u32,
}

/// How [`CoreEngine::eval_each`] treats the terms of one read.
struct Plan {
    numbered: bool,
    /// for each statement of the input: whether a `;` follows it
    suppressed: Vec<bool>,
    /// for each term to evaluate
    terms: Vec<TermPlan>,
}

#[derive(Clone, Copy, Default)]
struct TermPlan {
    /// the number of the input statement the term belongs to (numbered evaluations)
    number: Option<u64>,
    /// set on the last term of an input statement: whether its value is shown (no `;` after it)
    ends: Option<bool>,
}

impl Plan {
    /// Lay the plan over the `n` terms of a read whose input statements end at `ends` (indices of terms),
    /// numbering from `line`.
    fn lay_out(&mut self, n: usize, ends: &[usize], line: u64) {
        let mut terms = vec![TermPlan::default(); n];
        let mut start = 0;
        for (j, &end) in ends.iter().enumerate().filter(|&(_, &end)| end < n) {
            let number = self.numbered.then_some(line + j as u64);
            terms[start..=end].iter_mut().for_each(|t| t.number = number);
            terms[end].ends = Some(!self.suppressed.get(j).copied().unwrap_or(false));
            start = end + 1;
        }
        self.terms = terms;
    }
}

/// For each top-level statement of `src`: whether a `;` follows it on its line (its value is not shown).
fn suppressed(src: &str) -> Vec<bool> {
    let parsed = mc_syntax::parse(src);
    parsed
        .program
        .stmts
        .iter()
        .map(|s| {
            src.get(s.span.end as usize..).is_some_and(|rest| rest.trim_start_matches([' ', '\t']).starts_with(';'))
        })
        .collect()
}

impl Default for CoreEngine {
    fn default() -> CoreEngine {
        CoreEngine::new()
    }
}

/// The printed form of a value: a dataset as a table (`mc_assoc::table_text`), anything else as its term. Printing
/// polls `stop` (a huge value's text ends in `…` once it is raised).
fn shown<'h>(h: &'h mc_core::Heap, t: mc_core::T<'h>, stop: &dyn Fn() -> bool, display: bool) -> String {
    let mut p = Printer::with_numbers(h, Box::new(ArithNumbers)).with_exp_calls();
    if display {
        p = p.with_display().with_short_forms();
    }
    let p = p.with_stop(stop);
    mc_assoc::table_text(h, t, &mut |x| p.expr(x)).unwrap_or_else(|| p.expr(t))
}

/// The session output since it was last cleared: strings as their text, warnings, other terms printed.
fn output(h: &mc_core::Heap, k: &Kernel, stop: &dyn Fn() -> bool, display: bool) -> Vec<Emitted> {
    k.output(h)
        .into_iter()
        .map(|t| match h.view(t) {
            mc_core::View::Str(s) => Emitted::Text(s.to_string()),
            _ => match history::as_message(h, t) {
                Some((tag, text)) => Emitted::Message(tag, text),
                None => Emitted::Text(shown(h, t, stop, display)),
            },
        })
        .collect()
}

/// Whether `output` (a statement's last output) is the text of `printed`: the same text, or a string's text when
/// `printed` is its literal.
fn echoes(output: Option<&Emitted>, printed: &str) -> bool {
    let Some(Emitted::Text(line)) = output else { return false };
    let mut literal = String::new();
    mc_syntax::string_literal(line, &mut literal);
    line == printed || literal == printed
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

/// The symbol an assignment statement `x = v` (`set` or `assign` of a symbol) gives a value.
fn assigned_symbol(h: &mc_core::Heap, t: mc_core::T<'_>) -> Option<mc_core::Sym> {
    use mc_core::{Store, View};
    let View::App { head, args: &[lhs, _] } = h.view(t) else { return None };
    let View::Sym(f) = h.view(head) else { return None };
    let View::Sym(x) = h.view(lhs) else { return None };
    matches!(h.sym_name(f).as_str(), "set" | "assign").then_some(x)
}
