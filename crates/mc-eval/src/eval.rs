//! The evaluator: spike 4's speculative parallel core with spike 2's rule dispatch, as integrated by I2, on
//! `mc-core`'s heap and sessions.
//!
//! **Evaluation** (semantics 4.1): innermost; the head is evaluated, then the arguments by the head's argument modes;
//! a failure argument is returned unless the head handles failures (10); associative-commutative heads are
//! canonicalised (7.3); a native implementation is called; then argument-head rules of the arguments' heads in
//! argument order, then the head's rules (6.5), each through its index in rule order; a replacement is evaluated
//! again (a loop that costs fuel).
//!
//! **Parallelism** (semantics 18, spike 4 decisions 4.4 to 4.9): the strict arguments of a call whose head performs
//! no effects, and the elements of `map`, form a group that the lazy granularity rule may split. The right part of
//! a split runs speculatively. Only the task on the sequential frontier performs effects: a speculative task that
//! reaches one leaves a *hole* and the frontier fills holes in order. Every evaluation records a 64-bit read set
//! (symbols whose values or rules it consulted, including through conditions and memo hits, I2.3); a speculative
//! part whose read set a definition on its left touched is re-evaluated.
//!
//! **The memo** is `mc-core`'s (A.1): an entry is stamped with the start epoch of the evaluation that made it and
//! its read set, which includes the symbol masks of the term and its normal form (the normal-form stamp of 4.3,
//! through [`Heap::mask`]). An entry is valid while no symbol of its read set was redefined since its epoch.
//!
//! **Limits** (semantics 10, 18.4, A.5) go through the [`Control`] block: see [`crate::ctl`] and decision F.3.
//!
//! **Conditions are pure** (6.1, A.6): an effect reached while a condition runs gives
//! `failure(effect_in_condition)` as the value of the term whose rule's condition reached it.
use crate::cells::Cells;
use crate::compiled::{compiled_of, set_compiled, Compiled};
use crate::conc::{AppendVec, SymTable};
use crate::ctl::{Control, Status, FUEL_BLOCK};
use crate::guard::{Guarded, PanicSlot};
use crate::defs::{Attributes, Defs, Hold, SymDefs};
use crate::index::{Rhs, Rule, RuleRhs, Scratch};
use crate::lang::{BlockState, Form};
use crate::machine::{Frame, SPLIT_NEST};
use crate::matcher::{Binds, Matcher};
use crate::native::{Native, NativeCx, Pattern};
use crate::order::{StructuralOrder, TermOrder};
use crate::pat::{compile, has_type, Canon, DefError, Pat};
use crate::subst::Scopes;
use mc_core::{Heap, Raw, Roots, Store, Sym, Trace, View, T};

// parallel runs of plain data for natives (roadmap item 16A): a child module, for the task state's private fields
#[path = "par.rs"]
pub(crate) mod par;
pub use par::{Budget, ParFn, Sub};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::*};
use std::sync::Arc;

/// Why an evaluation step stopped early. Never seen by users: [`Kernel::eval`] turns it into a value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Abort {
    /// A speculative task cannot go on (it reached an effect, exhausted its fuel share or its depth): it leaves a
    /// hole for the frontier.
    Hole,
    /// The control block's status is set (fuel on the frontier, depth on the frontier, or an interrupt): every task
    /// stops.
    Limit,
    /// An effect was reached inside a condition (A.6).
    EffectInCondition,
    /// An effect was reached on the frontier while a scratch scope was open: the native that opened it runs the
    /// evaluation again without one ([`NativeCx::scratch`], decision G.3). Never leaves that native.
    Scratch,
}

/// A result that may abort.
pub type Res<X> = Result<X, Abort>;

/// How groups of arguments and `map` elements are split into tasks (semantics 18.6: no semantic effect).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Split {
    /// Never split: the sequential evaluator.
    Never,
    /// Split every group of two or more down to single elements (maximal speculation: a stress mode).
    Always,
    /// Spike 4's lazy rule (decision 4.4), with the estimates of decision F.10.
    Lazy,
}

/// Evaluator settings.
#[derive(Clone, Copy, Debug)]
pub struct EvalConfig {
    /// the splitting rule
    pub split: Split,
    /// estimated fuel above which a group is worth splitting
    pub grain: u64,
    /// fuel between granularity checks of a group judged too small at first
    pub poll: u64,
    /// memoise normal forms
    pub memo: bool,
    /// default fuel budget of one top-level evaluation
    pub fuel: i64,
    /// deepest nesting of evaluations (frames of the evaluation stack) before `failure(limit, depth)`; the frames
    /// live on the heap, so this bounds memory, not the Rust stack (decision 1a.1)
    pub max_depth: u32,
    /// deepest nesting of native callbacks (natives, conditions and speculative groups evaluating terms through
    /// the evaluator) before `failure(limit, depth)`: the one nesting that still uses the Rust stack (decision 1a.2)
    pub max_nest: u32,
    /// most rewrites of one evaluation level (a rule whose result is rewritten again, `f(x_) := f(x + 1)`) before
    /// `failure(limit, iterations)` (decision p6core.11)
    pub max_iterations: u32,
    /// most bytes the young region may hold during one evaluation before `failure(limit, memory)`: nothing is
    /// collected until the evaluation ends, so a runaway loop would otherwise exhaust memory; 0 (the default):
    /// [`default_max_young`], read again at the start of every top-level evaluation (decision p6core.11)
    pub max_young: usize,
}

/// The default [`EvalConfig::max_iterations`].
pub const DEFAULT_MAX_ITERATIONS: u32 = 1 << 20;

/// The automatic [`EvalConfig::max_young`]: two fifths of the memory the process may use now (the smaller of the
/// physical memory and a limit set on the process; `MC_MEMORY_LIMIT_MB` overrides both), at most 8 GB; 2 GB when
/// neither is known.
pub fn default_max_young() -> usize {
    let cap = crate::membudget::process_cap().unwrap_or(5 << 30);
    (cap / 5 * 2).min(8 << 30)
}

/// Memory the default frame limit allows the evaluation stack of one task (decision 1a.1).
pub const DEFAULT_STACK_BYTES: usize = 1 << 28;

/// The default [`EvalConfig::max_depth`]: [`DEFAULT_STACK_BYTES`] of frames, counting two frames per level (an
/// evaluation level and a language form above it), about 1.8 million levels. Unoptimised builds, which are for
/// tests, stop runaway recursions sooner.
pub const DEFAULT_MAX_DEPTH: u32 = if cfg!(debug_assertions) {
    1 << 17
} else {
    (DEFAULT_STACK_BYTES / (2 * std::mem::size_of::<crate::machine::Frame<'static>>())) as u32
};

/// The default [`EvalConfig::max_nest`]: what the 64 MB stacks of a session's pool hold with room to spare
/// (unoptimised builds have frames several times larger).
pub const DEFAULT_MAX_NEST: u32 = if cfg!(debug_assertions) { 1_000 } else { 10_000 };

impl Default for EvalConfig {
    fn default() -> EvalConfig {
        EvalConfig {
            split: Split::Lazy,
            grain: 1 << 10,
            poll: 1 << 9,
            memo: true,
            fuel: i64::MAX,
            max_depth: DEFAULT_MAX_DEPTH,
            max_nest: DEFAULT_MAX_NEST,
            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_young: 0,
        }
    }
}

impl EvalConfig {
    /// The sequential reference: no splitting, no memo.
    pub fn reference() -> EvalConfig {
        EvalConfig { split: Split::Never, memo: false, ..EvalConfig::default() }
    }
}

/// Counters of one top-level evaluation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvalStats {
    /// groups split into a frontier and a speculative part
    pub splits: u64,
    /// elements re-evaluated by the frontier (holes filled, or speculative work invalidated by a definition)
    pub redos: u64,
    /// fuel charged to the budget (the frontier's work and the speculative work it kept)
    pub fuel: u64,
    /// holes left by speculative tasks
    pub holes: u64,
}

/// The result of a top-level evaluation.
#[derive(Clone, Copy, Debug)]
pub struct Evaluation<'h> {
    /// the normal form, or a failure term
    pub value: T<'h>,
    /// why the evaluation stopped, if it was stopped
    pub status: Status,
    /// counters
    pub stats: EvalStats,
}

/// Symbols the evaluator builds with (immediates: valid in the heap they were made in, across safe points).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Names {
    pub(crate) list: Sym,
    pub(crate) true_: Sym,
    pub(crate) false_: Sym,
    pub(crate) null: Sym,
    pub(crate) failure: Sym,
    pub(crate) effect_in_condition: Sym,
    pub(crate) no_match: Sym,
    /// `eval`: written directly in a held argument it forces that argument's evaluation (semantics 5)
    pub(crate) eval: Sym,
    /// `unevaluated(x)` as a strict argument passes `x` itself, unevaluated
    pub(crate) unevaluated: Sym,
    /// `threaded(v)`, `threaded(v, n)`: an operand spread over the levels of the array arguments of a listable call
    pub(crate) threaded: Sym,
    /// the term a `var` stands for: `local_cell(id)` (see [`crate::cells`])
    pub(crate) local_cell: Sym,
    /// the binding forms (hygienic substitution, closures)
    pub(crate) scopes: Scopes,
}

impl Names {
    fn new(h: &Heap) -> Names {
        use mc_api::names;
        Names {
            list: h.sym(names::LIST),
            true_: h.sym(names::TRUE),
            false_: h.sym(names::FALSE),
            null: h.sym(names::NULL),
            failure: h.sym(names::FAILURE),
            effect_in_condition: h.sym(names::failure::EFFECT_IN_CONDITION),
            no_match: h.sym(names::failure::NO_MATCH),
            eval: h.sym("eval"),
            unevaluated: h.sym("unevaluated"),
            threaded: h.sym("threaded"),
            local_cell: h.sym("local_cell"),
            scopes: Scopes::new(h),
        }
    }
}

/// The read-set bit of a symbol (spike 2's decision 2.7; the same bit as in [`Heap::mask`]).
#[inline]
pub(crate) fn bit(s: Sym) -> u64 {
    // as `mc_core::sym_mask_bit`: bit 63 belongs to `local_cell` alone ([`CELL_BIT`])
    1u64 << (s.0 % 63)
}

/// The read-set bit of mutable cells: the mask bit of `local_cell`, which no other symbol shares, so a term is
/// treated as referring to a cell only when it does (decision p6core.32).
pub(crate) const CELL_BIT: u64 = 1 << 63;

// The placeholder for an element a speculative task could not complete (a handle `mc-core` never hands out).
use mc_core::HOLE;

#[inline]
fn hole<'h>() -> T<'h> {
    T::from_raw(HOLE)
}

/// The evaluator's state across a session: definitions, built-ins, the definition epoch, outputs and the control
/// block. It is the session state ([`mc_core::Session`]) and a [`Trace`] root container: everything it keeps that
/// holds terms is traced at safe points.
///
/// Definitions between evaluations go through `&mut self` methods; an evaluation ([`Kernel::eval`]) also takes
/// `&mut self`, so at most one frontier exists and definitions made during it come from that frontier only (I2.5).
pub struct Kernel {
    pub(crate) defs: Defs,
    pub(crate) natives: Vec<Impl>,
    /// compiled specialisations ([`Kernel::register_compiled`]); a symbol's slot holds an index + 1
    pub(crate) compiled: AppendVec<Arc<dyn Compiled>>,
    /// the flag long-running library code polls ([`Kernel::stop_flag`])
    stop: Arc<AtomicBool>,
    /// learned fuel per evaluation, by head symbol (granularity)
    cost: SymTable<AtomicU32>,
    order: Box<dyn TermOrder>,
    pub(crate) ctl: Arc<Control>,
    /// the definition epoch: bumped by every definition
    epoch: AtomicU32,
    /// the last epoch at which a symbol with each read-set bit was defined
    last_change: [AtomicU32; 64],
    out: Mutex<Vec<Raw>>,
    /// the cells of mutable locals
    pub(crate) cells: Cells,
    pub(crate) names: Names,
    /// `call$`, under which the rules for calls with compound heads are kept (C9B2.4)
    call_sym: Option<Sym>,
    /// the symbols of assumption contexts, once [`crate::assume::install`] ran
    pub(crate) assume: Option<crate::assume::Syms>,
    pub(crate) cfg: EvalConfig,
    /// Terms the session keeps across evaluations (history, inputs); traced with the kernel.
    pub roots: Roots,
    /// the native a panic unwound through, for the guard ([`crate::guard`])
    pub(crate) panics: PanicSlot,
    /// the memory budget of the running evaluation ([`EvalConfig::max_young`])
    pub(crate) young_cap: usize,
    /// operator forms: `f(a..)(x..)` of these symbols is `f(x, a..)` or `f(a.., x..)` ([`Kernel::set_operator_form`])
    pub(crate) op_forms: rustc_hash::FxHashMap<Sym, OperatorForm>,
    /// the heads of associations and their entries, once a library declared them ([`Kernel::set_association_heads`]):
    /// listable functions thread over associations' values (decision io-core.1)
    pub(crate) assoc_heads: Option<(Sym, Sym)>,
    /// functions without the `listable` attribute that map over list arguments themselves: they thread over the
    /// values of associations as listable functions do ([`Kernel::set_maps_lists`])
    pub(crate) maps_lists: rustc_hash::FxHashSet<Sym>,
    /// heads of number-like values with their own arithmetic (quantities, uncertain numbers): sums and products of
    /// them go through the evaluator's `add` and `mul` ([`Kernel::set_value_head`])
    pub(crate) value_heads: rustc_hash::FxHashSet<Sym>,
    /// the measurements of `profile` and `timing(e, breakdown: true)` ([`crate::profile`])
    pub(crate) profiler: crate::profile::Profiler,
    /// set once a call `unevaluated(x)` was evaluated: only then do strict arguments get unwrapped
    pub(crate) uneval_seen: AtomicBool,
    /// the young-generation size at which `memory_constrained` stops the evaluation ([`crate::constrained`])
    pub(crate) mem_cap: std::sync::atomic::AtomicUsize,
    /// Extra flags a limit's watchdog raises with the interrupt (the engine registers the number kernels' stop flag,
    /// so one huge exact multiplication or gcd stops at `time_constrained`'s limit, engine B21-29).
    pub(crate) watch_flags: std::sync::Mutex<Vec<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
}

/// Where the arguments of the outer call of an operator form `f(a..)(x..)` go (decision p6core.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatorForm {
    /// `f(a..)(x)` is `f(x, a..)`: one argument, the data, first (`select(is_even)(xs)`, `append(x)(xs)`)
    DataFirst,
    /// `f(a..)(x..)` is `f(a.., x..)`: the arguments after the operator's (`map(f)(xs)`, `fold(f)(x, xs)`)
    DataLast,
}

/// The implementation in a symbol's native slot.
pub(crate) enum Impl {
    /// a native function, whether it reads its call term ([`Native::reads_term`]) and whether it takes curried
    /// calls ([`Native::curried`])
    Native { native: Box<dyn Native>, reads_term: bool, curried: bool, sym: Sym },
    /// a language form the evaluator runs as frames of its stack ([`crate::lang`])
    Form(Form),
}

impl Trace for Kernel {
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.defs.trace(visit);
        for n in self.natives.iter_mut() {
            if let Impl::Native { native: n, .. } = n {
                n.trace(visit);
            }
        }
        self.out.get_mut().iter_mut().for_each(&mut *visit);
        self.cells.trace(visit);
        self.roots.trace(visit);
    }
}

/// A session whose state is a [`Kernel`] (with the built-ins registered).
pub fn session(heap: mc_core::Config, threads: usize, cfg: EvalConfig) -> mc_core::Session<Kernel> {
    // the kernel names its symbols in the session's own heap
    mc_core::Session::with_state(heap, threads, |h| Kernel::new(h, cfg))
}

impl Kernel {
    /// A kernel for heap `h`, with the built-ins of [`crate::builtins`].
    pub fn new(h: &Heap, cfg: EvalConfig) -> Kernel {
        let mut k = Kernel {
            defs: Defs::default(),
            natives: vec![],
            compiled: AppendVec::default(),
            stop: Arc::new(AtomicBool::new(false)),
            cost: SymTable::default(),
            order: Box::new(StructuralOrder::default()),
            ctl: Arc::new(Control::default()),
            epoch: AtomicU32::new(1),
            last_change: std::array::from_fn(|_| AtomicU32::new(0)),
            out: Mutex::new(vec![]),
            cells: Cells::default(),
            names: Names::new(h),
            call_sym: Some(h.sym(crate::pat::CALL)),
            assume: None,
            cfg,
            roots: Roots::new(),
            panics: PanicSlot::default(),
            op_forms: Default::default(),
            assoc_heads: None,
            maps_lists: Default::default(),
            value_heads: Default::default(),
            profiler: Default::default(),
            young_cap: usize::MAX,
            uneval_seen: AtomicBool::new(false),
            mem_cap: std::sync::atomic::AtomicUsize::new(usize::MAX),
            watch_flags: std::sync::Mutex::new(Vec::new()),
        };
        crate::guard::install_hook();
        crate::builtins::register(&mut k, h);
        k
    }

    /// The evaluator settings.
    pub fn config(&self) -> &EvalConfig {
        &self.cfg
    }

    /// Change the evaluator settings (between evaluations).
    pub fn set_config(&mut self, cfg: EvalConfig) {
        self.cfg = cfg;
    }

    /// The control block: [`Control::interrupt`] stops the running evaluation from any thread.
    pub fn control(&self) -> Arc<Control> {
        self.ctl.clone()
    }

    // --- definitions between evaluations ---------------------------------------------------------------------

    /// Record a definition of symbols with read-set `bits` at the next epoch, around `define`.
    pub(crate) fn defined<R>(&self, from: u32, bits: u64, define: impl FnOnce() -> R) -> (R, u32) {
        let e = from.wrapping_add(1);
        let mut b = bits;
        while b != 0 {
            self.last_change[b.trailing_zeros() as usize].fetch_max(e, Release);
            b &= b - 1;
        }
        let r = define();
        self.epoch.store(e, Release);
        (r, e)
    }

    /// `s = v` (semantics 6.4).
    pub fn set_own(&mut self, s: Sym, v: T<'_>) {
        let e = *self.epoch.get_mut();
        self.defined(e, bit(s), || self.defs.set_own(s, v));
    }

    /// Set a symbol's attributes (argument modes, AC, effect, failure handling).
    pub fn set_attributes(&mut self, s: Sym, a: Attributes) {
        let e = *self.epoch.get_mut();
        self.defined(e, bit(s), || self.set_attrs(s, a));
    }

    fn set_attrs(&self, s: Sym, a: Attributes) {
        self.defs.set_attrs(s, a);
    }

    /// Give symbol `s` an operator form: a call `s(a..)(x..)` (whose head is a call of `s`) is rewritten to a call
    /// of `s` with the outer arguments placed by `form`. A native of `s` taking curried calls keeps them.
    pub fn set_operator_form(&mut self, s: Sym, form: OperatorForm) {
        self.op_forms.insert(s, form);
    }

    /// Declare the head of associations (`assoc(k -> v, ...)`) and of their entries (`rule`): a listable function
    /// then threads over the values of association arguments, as over the elements of lists (decision io-core.1).
    /// The library that implements associations calls it; the evaluator does not intern these names itself.
    pub fn set_association_heads(&mut self, assoc: Sym, rule: Sym) {
        self.assoc_heads = Some((assoc, rule));
    }

    /// Declare that the native of `s` maps over list arguments itself (elementwise) without the `listable` attribute
    /// (which would thread lists before the native's own fast paths): association arguments are then threaded by
    /// the evaluator, the values of each key in turn (decision io-core.1).
    pub fn set_maps_lists(&mut self, s: Sym) {
        self.maps_lists.insert(s);
    }

    /// Declare `s` the head of number-like values whose arithmetic the evaluator's `add`, `mul` and `pow` carry out
    /// (a library's own hook, such as quantities and uncertain numbers): list reductions that build sums and products
    /// directly hand terms with these heads to the evaluator (decision io-core.5).
    pub fn set_value_head(&mut self, s: Sym) {
        self.value_heads.insert(s);
    }

    /// Whether `s` is listable or maps over lists itself ([`Kernel::set_maps_lists`]).
    pub fn maps_lists(&self, s: Sym) -> bool {
        self.defs.attrs(s).listable || self.maps_lists.contains(&s)
    }

    /// The attributes of a symbol.
    pub fn attributes(&self, s: Sym) -> Attributes {
        self.defs.attrs(s)
    }

    /// Register a flag that a limit's watchdog (`time_constrained`) raises together with the interrupt and lowers
    /// when it withdraws it: the number kernels' stop flag (engine B21-29).
    pub fn watch_flag(&self, f: Arc<AtomicBool>) {
        self.watch_flags.lock().unwrap().push(f);
    }

    /// Give symbol `s` a native implementation (and its attributes). See [`Native`].
    pub fn register(&mut self, s: Sym, a: Attributes, native: Box<dyn Native>) {
        let reads_term = native.reads_term();
        let curried = native.curried();
        self.register_impl(s, a, Impl::Native { native, reads_term, curried, sym: s });
    }

    /// Make `s` a language form run as frames of the evaluation stack.
    pub(crate) fn register_form(&mut self, s: Sym, a: Attributes, form: Form) {
        self.register_impl(s, a, Impl::Form(form));
    }

    /// Replace the native of `s` by `wrap` of it (keeping its attributes); `false` when `s` has no native function.
    /// A wrapper that keeps terms must trace them and the wrapped native's.
    pub fn wrap_native(&mut self, s: Sym, wrap: impl FnOnce(Box<dyn Native>) -> Box<dyn Native>) -> bool {
        let Some(n) = self.defs.get(s).map(|sd| sd.native.load(Acquire)).filter(|&n| n != 0) else { return false };
        let slot = &mut self.natives[n as usize - 1];
        let Impl::Native { native, .. } = slot else { return false };
        let old = std::mem::replace(native, Box::new(crate::builtins::Declines));
        let new = wrap(old);
        let (reads_term, curried) = (new.reads_term(), new.curried());
        *slot = Impl::Native { native: new, reads_term, curried, sym: s };
        true
    }

    fn register_impl(&mut self, s: Sym, a: Attributes, imp: Impl) {
        let i = self.natives.len() as u32;
        self.natives.push(imp);
        self.set_attributes(s, a);
        let e = *self.epoch.get_mut();
        self.defined(e, bit(s), || self.defs.set_native(s, i));
    }

    /// Give calls with head `head` a compiled specialisation (see [`Compiled`]), replacing any earlier one. After
    /// the arguments are evaluated and the head's native (if any) declined, the evaluator offers the call to `c`
    /// before trying rules; `c` returns the value or declines. A specialisation must give the value the rules would
    /// give, so registering one is not a definition (memoised results stay valid); it is removed by
    /// [`Kernel::remove_compiled`] and by [`Kernel::clear`].
    pub fn register_compiled(&mut self, head: Sym, c: Arc<dyn Compiled>) {
        set_compiled(&self.defs, &self.compiled, head, Some(c));
    }

    /// Remove the compiled specialisation of `head`, if it has one.
    pub fn remove_compiled(&mut self, head: Sym) {
        set_compiled(&self.defs, &self.compiled, head, None);
    }

    /// The compiled specialisation of `head`, if it has one.
    pub fn compiled(&self, head: Sym) -> Option<Arc<dyn Compiled>> {
        compiled_of(&self.defs, &self.compiled, head).cloned()
    }

    /// The stop flag libraries poll in long loops (arithmetic on huge numbers, sorting): raised together with
    /// [`Control::interrupt`] by the session's owner, cleared by it before an evaluation. Natives reach it through
    /// [`NativeCx::stop_flag`]; a kernel starts with a flag of its own.
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    #[inline]
    pub(crate) fn stop_flag_ref(&self) -> &Arc<AtomicBool> {
        &self.stop
    }

    /// Use `flag` as the stop flag (an engine sharing one flag between the kernel and its libraries).
    pub fn set_stop_flag(&mut self, flag: Arc<AtomicBool>) {
        self.stop = flag;
    }

    /// Compare two terms by the canonical order (semantics 8; the order [`Kernel::set_order`] installed).
    pub fn cmp<'h>(&self, h: &'h Heap, a: T<'h>, b: T<'h>) -> std::cmp::Ordering {
        self.order.cmp(h, a, b)
    }

    /// Sort terms by the canonical order.
    pub fn sort<'h>(&self, h: &'h Heap, v: &mut [T<'h>]) {
        self.order.sort(h, v)
    }

    /// Declare (or remove) the identity element of an associative and commutative symbol (semantics 7.3): its
    /// canonical form drops identity arguments, is the identity with no arguments and the argument with one.
    pub fn set_identity(&mut self, s: Sym, e: Option<T<'_>>) {
        let ep = *self.epoch.get_mut();
        self.defined(ep, bit(s), || self.defs.set_identity(s, e));
    }

    /// Replace the canonical order (normally by `mc-arith`'s). Terms already canonicalised keep their order.
    pub fn set_order(&mut self, order: Box<dyn TermOrder>) {
        self.order = order;
    }

    /// `lhs := rhs` (semantics 6.4), stored with the left side's head.
    pub fn define(&mut self, h: &Heap, lhs: Pat, rhs: Rhs) -> Result<(), DefError> {
        let e = *self.epoch.get_mut();
        self.define_rule(h, e, lhs, RuleRhs::Native(rhs), None).map(|_| ())
    }

    /// `lhs := rhs` stored with `attach`, the head of one of the left side's depth-one arguments (semantics 6.5).
    pub fn define_attached(&mut self, h: &Heap, lhs: Pat, rhs: Rhs, attach: Sym) -> Result<(), DefError> {
        let e = *self.epoch.get_mut();
        self.define_rule(h, e, lhs, RuleRhs::Native(rhs), Some(attach)).map(|_| ())
    }

    /// `lhs := body` with a term right side, in which symbol `names[i]` stands for pattern variable `vars[i]`
    /// (the shape `mc-syntax` lowers definitions to; see [`crate::syntax`]).
    pub fn define_term(&mut self, h: &Heap, lhs: Pat, body: T<'_>, names: &[Sym], vars: &[u16]) -> Result<(), DefError> {
        let e = *self.epoch.get_mut();
        let rhs = RuleRhs::Term { body: Raw::new(body), names: names.into(), vars: vars.into(), src: None };
        self.define_rule(h, e, lhs, rhs, None).map(|_| ())
    }

    /// Compile a pattern for a native's own matching ([`NativeCx::matches`]).
    pub fn compile_pattern(&self, h: &Heap, p: Pat) -> Result<Pattern, DefError> {
        let p = compile(h, p, &CanonOf(self))?;
        Ok(Pattern::new(p))
    }

    /// Compile and insert a rule at epoch `from + 1`; returns the new epoch. Called by the session or the frontier.
    fn define_rule(&self, h: &Heap, from: u32, lhs: Pat, rhs: RuleRhs, attach: Option<Sym>) -> Result<u32, DefError> {
        // a left side with a compound head is stored as a rule of `call$` (C9B2.4)
        let lhs = compile(h, crate::pat::curry_lit(h, lhs), &CanonOf(self))?;
        let (top_head, top_args): (T, Vec<T>) = match lhs.strip() {
            Pat::App { head, args, .. } => (head.get(h), args.iter().filter_map(|a| a.fixed_head(h).or_else(|| lit_sym(h, a))).collect()),
            Pat::Lit(t) => match h.view(t.get(h)) {
                View::App { head, args } => (head, args.iter().filter_map(|&a| attach_key(h, a)).collect()),
                _ => return Err(DefError::NotCompound),
            },
            _ => return Err(DefError::NotCompound),
        };
        let View::Sym(top) = h.view(top_head) else { return Err(DefError::NotCompound) };
        if let Some(s) = attach {
            if !top_args.contains(&h.sym_term(s)) {
                return Err(DefError::BadAttachment);
            }
        }
        let ac_top = self.defs.attrs(top).ac;
        // a rule for calls with a compound head is kept with the innermost head symbol of its head pattern, under
        // `call$` (`f(x_)(y_)` with `f`), so `f` is defined, `unset(f)` removes it and only calls rooted at `f`
        // read it; a head pattern without such a symbol (`g_(x_)`) keeps it in `call$`'s own rules (C9B2.4)
        if attach.is_none() && h.sym_name(top) == crate::pat::CALL {
            if let Some(root) = curried_root(h, &lhs) {
                let ((), e) = self.defined(from, bit(root), || {
                    self.defs.define(h, root, Some(top), false, Rule::new(lhs, rhs));
                });
                return Ok(e);
            }
        }
        let owner = attach.unwrap_or(top);
        let ((), e) = self.defined(from, bit(owner), || {
            self.defs.define(h, owner, attach.map(|_| top), ac_top, Rule::new(lhs, rhs));
        });
        Ok(e)
    }

    /// Remove every definition of `s` (own value, attributes, identity element, rules, including the
    /// argument-head rules `s` holds) except a native implementation, between evaluations. Used to reload a module
    /// without duplicating its clauses; counts as a definition of `s` (memoised results that read it are stale).
    pub fn clear(&mut self, s: Sym) {
        let e = *self.epoch.get_mut();
        let b = bit(s);
        let mut b2 = b;
        while b2 != 0 {
            self.last_change[b2.trailing_zeros() as usize].fetch_max(e.wrapping_add(1), Release);
            b2 &= b2 - 1;
        }
        self.defs.clear(s);
        *self.epoch.get_mut() = e.wrapping_add(1);
    }

    /// Whether `s` has any definition: a native, an own value, attributes, an identity element or rules.
    pub fn is_defined(&self, s: Sym) -> bool {
        self.defs.is_defined(s)
    }

    /// Whether `s` has a native implementation.
    pub fn has_native(&self, s: Sym) -> bool {
        self.defs.get(s).is_some_and(|d| d.native.load(Relaxed) != 0)
    }

    /// Free what definitions retired (also done by every collection). Returns how many allocations were freed.
    pub fn reclaim(&mut self) -> usize {
        self.defs.reclaim()
    }

    // --- inspection ------------------------------------------------------------------------------------------

    /// Everything printed so far, in order.
    pub fn output<'h>(&self, h: &'h Heap) -> Vec<T<'h>> {
        self.out.lock().iter().map(|r| r.get(h)).collect()
    }

    /// Forget the output.
    pub fn clear_output(&mut self) {
        self.out.get_mut().clear();
    }

    /// The own value of `s`.
    pub fn value_of<'h>(&self, h: &'h Heap, s: Sym) -> Option<T<'h>> {
        self.defs.own(h, s)
    }

    /// The version counter of `s` (semantics 1.2): bumped by every change to its definitions.
    pub fn version(&self, s: Sym) -> u32 {
        self.defs.version(s)
    }

    /// Number of rules `s` holds for itself.
    pub fn rule_count(&self, s: Sym) -> usize {
        let own = self.defs.get(s).and_then(SymDefs::rules).map_or(0, |r| r.len());
        // with the rules for calls whose head is a call of `s` (`s(x_)(y_) := ...`, C9B2.4)
        let curried = self.call_sym.and_then(|c| self.defs.get(s).filter(|d| d.has_arg_rules()).and_then(|d| d.arg_rules(c))).map_or(0, |r| r.len());
        own + curried
    }

    /// The rule ids of `s`'s own rules in rule order (ids are definition order; replacements keep the id).
    pub fn rule_order(&self, s: Sym) -> Vec<u32> {
        self.defs.get(s).and_then(SymDefs::rules).map_or(vec![], |r| r.order())
    }

    /// The definition epoch.
    pub fn epoch(&self) -> u32 {
        self.epoch.load(Acquire)
    }

    /// The compound term `head(args)` in canonical form (AC heads flattened and sorted, semantics 7.3).
    pub fn make<'h>(&self, h: &'h Heap, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        make(self, h, head, args)
    }

    /// `true` or `false`.
    pub fn bool<'h>(&self, h: &'h Heap, b: bool) -> T<'h> {
        h.sym_term(if b { self.names.true_ } else { self.names.false_ })
    }

    // --- evaluation ------------------------------------------------------------------------------------------

    /// Evaluate `t` as one top-level evaluation with the default fuel budget. Call it on the pool it should use
    /// (inside [`mc_core::Session::step`]); the safe point that follows the step is the barrier after it.
    pub fn eval<'h>(&mut self, h: &'h Heap, t: T<'h>) -> Evaluation<'h> {
        let fuel = self.cfg.fuel;
        self.eval_with(h, t, fuel)
    }

    /// Evaluate `t` with a fuel budget.
    ///
    /// A panic inside the evaluation (a native's, or the evaluator's own) does not unwind out of it: the value is
    /// then `failure(internal, "message (location)", f)` with `f` the innermost native the panic went through (left
    /// out when there is none) and the status [`Status::Running`] (see [`crate::guard`]).
    pub fn eval_with<'h>(&mut self, h: &'h Heap, t: T<'h>, fuel: i64) -> Evaluation<'h> {
        self.cells.begin();
        self.young_cap = match self.cfg.max_young {
            0 => default_max_young(),
            n => n,
        };
        let k: &Kernel = self;
        k.ctl.begin(fuel);
        // a record left by a panic some native caught itself
        k.panics.take();
        let ev = Eval { heap: h, k };
        let mut cx = Cx::frontier(k.epoch.load(Acquire), rayon::current_num_threads() as u32);
        let r = {
            let _guarded = Guarded::enter();
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ev.eval(&mut cx, t)))
        };
        let r = match r {
            Ok(r) => r,
            Err(payload) => {
                let value = k.panic_failure(h, &*payload);
                return Evaluation { value, status: Status::Running, stats: EvalStats::default() };
            }
        };
        let status = k.ctl.status();
        let value = match r {
            Ok(v) => v,
            Err(Abort::Limit) => ev.failure(status.failure_tags()),
            Err(Abort::EffectInCondition) => ev.failure(&[mc_api::names::failure::EFFECT_IN_CONDITION]),
            // the frontier never leaves a hole; should one reach the top, report a stop rather than a wrong value
            Err(Abort::Hole) => {
                debug_assert!(false, "a hole reached the top of an evaluation");
                ev.failure(&[mc_api::names::failure::INTERRUPTED])
            }
            // caught by the native that opened the scope
            Err(Abort::Scratch) => {
                debug_assert!(false, "a scratch abort reached the top of an evaluation");
                ev.failure(&[mc_api::names::failure::INTERRUPTED])
            }
        };
        Evaluation { value, status, stats: EvalStats { splits: cx.splits, redos: cx.redos, fuel: cx.fuel, holes: cx.holes } }
    }

    /// The value of an evaluation that panicked with `payload`: `failure(internal, text, f)`.
    fn panic_failure<'h>(&self, h: &'h Heap, payload: &(dyn std::any::Any + Send)) -> T<'h> {
        let (native, note) = match self.panics.take() {
            // the scope's note is the hook's for this panic when the messages agree
            Some((s, note)) if note.message == crate::guard::payload_text(payload) => (Some(s), note),
            Some((s, _)) => (Some(s), crate::guard::note_for(payload)),
            None => (None, crate::guard::note_for(payload)),
        };
        let mut args = vec![h.sym_term(h.sym(crate::guard::INTERNAL)), h.string(&note.text())];
        args.extend(native.map(|s| h.sym_term(s)));
        h.app(h.sym_term(self.names.failure), &args)
    }

    /// Did a definition after epoch `from` (up to `to`) touch read set `deps`? Conservative: a definition after `to`
    /// also counts.
    #[inline]
    pub(crate) fn changed(&self, from: u32, to: u32, deps: u64) -> bool {
        if from == to {
            return false;
        }
        if to < from {
            return true;
        }
        let mut d = deps;
        while d != 0 {
            if self.last_change[d.trailing_zeros() as usize].load(Acquire) > from {
                return true;
            }
            d &= d - 1;
        }
        false
    }

    #[inline]
    pub(crate) fn learn(&self, s: Sym, fuel: u64) {
        let c = self.cost.get_or_init(s.0);
        let old = c.load(Relaxed) as u64;
        let new = (old * 3 + fuel.min(u32::MAX as u64)) / 4;
        if new.abs_diff(old) > old / 8 {
            c.store(new as u32, Relaxed);
        }
    }

    #[inline]
    pub(crate) fn head_cost(&self, s: Sym) -> u64 {
        self.cost.get(s.0).map_or(0, |c| c.load(Relaxed) as u64)
    }
}

/// The head symbol of a compound term.
#[inline]
fn head_of<'h>(h: &'h Heap, t: T<'h>) -> Option<T<'h>> {
    // a packed array's head is `list`: viewing it would unpack every leaf (argument-head dispatch of any call
    // with a packed argument)
    if h.packed(t).is_some() {
        return Some(h.sym_term(h.sym(mc_api::names::LIST)));
    }
    match h.view(t) {
        View::App { head, .. } => Some(head),
        _ => None,
    }
}

/// Are `params` the parameters of a slot function, the symbols `&1`, ..., `&n` (`n >= 1`) in order?
fn is_slot_params(h: &Heap, params: &[T<'_>]) -> bool {
    !params.is_empty()
        && params.iter().enumerate().all(|(i, &p)| matches!(h.view(p), View::Sym(s) if h.sym_name(s).strip_prefix('&') == Some(&(i + 1).to_string())))
}

/// The symbol an argument-head rule for argument `t` is stored with: the head of a compound argument, or a symbol
/// argument itself (`area(square) := ...` stored with `square`; semantics 6.5 extended to symbols, C9B.5).
/// The innermost head symbol of a curried left side `call$(head pattern, ...)`, if its head patterns end in a
/// symbol (`f(x_)(y_)`: `f`; `g_(x_)`: none).
fn curried_root(h: &Heap, p: &Pat) -> Option<Sym> {
    let mut p = p.strip();
    loop {
        match p {
            Pat::App { info, args, .. } if info.curried => p = args.first()?.strip(),
            Pat::App { head, .. } => return sym_of(h, head.get(h)),
            Pat::Lit(t) => {
                let mut t = t.get(h);
                while let View::App { head, .. } = h.view(t) {
                    t = head;
                }
                return sym_of(h, t);
            }
            _ => return None,
        }
    }
}

fn attach_key<'h>(h: &'h Heap, t: T<'h>) -> Option<T<'h>> {
    // (a packed array is never a symbol: test it first, as viewing it would unpack it)
    if h.packed(t).is_none() && matches!(h.view(t), View::Sym(_)) {
        return Some(t);
    }
    head_of(h, t)
}

/// A literal symbol argument pattern, as an attachment key.
fn lit_sym<'h>(h: &'h Heap, p: &Pat) -> Option<T<'h>> {
    match p.shape() {
        Pat::Lit(t) if matches!(h.view(t.get(h)), View::Sym(_)) => Some(t.get(h)),
        _ => None,
    }
}

/// Compilation's view of the kernel.
struct CanonOf<'k>(&'k Kernel);

impl Canon for CanonOf<'_> {
    fn is_ac<'h>(&self, h: &'h Heap, head: T<'h>) -> bool {
        sym_of(h, head).is_some_and(|s| self.0.defs.attrs(s).ac)
    }
    fn is_flat<'h>(&self, h: &'h Heap, head: T<'h>) -> bool {
        sym_of(h, head).is_some_and(|s| self.0.defs.attrs(s).flat)
    }
    fn make<'h>(&self, h: &'h Heap, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        make(self.0, h, head, args)
    }
}

/// The symbol of a symbol term.
#[inline]
pub(crate) fn sym_of<'h>(h: &'h Heap, t: T<'h>) -> Option<Sym> {
    match h.view(t) {
        View::Sym(s) => Some(s),
        _ => None,
    }
}

/// A compound term in canonical form: under an associative and commutative head, arguments with the same head are
/// spliced in (one level: they are canonical already) and the arguments sorted by the canonical order.
fn make<'h>(k: &Kernel, h: &'h Heap, head: T<'h>, args: &[T<'h>]) -> T<'h> {
    let Some(sd) = sym_of(h, head).and_then(|s| k.defs.get(s)).filter(|d| d.attrs().ac || d.attrs().flat) else {
        return h.app(head, args);
    };
    let ac = sd.attrs().ac;
    // the flattened arguments: on a fixed buffer when few (most sums and products), else in a vector
    let len: usize = args
        .iter()
        .map(|&a| match h.view(a) {
            View::App { head: g, args: inner } if g == head => inner.len(),
            _ => 1,
        })
        .sum();
    let mut buf = [head; 16];
    let mut big = vec![];
    let v: &mut [T<'h>] = if len <= buf.len() { &mut buf[..len] } else { big.resize(len, head); &mut big };
    let mut n = 0;
    for &a in args {
        match h.view(a) {
            View::App { head: g, args: inner } if g == head => {
                v[n..n + inner.len()].copy_from_slice(inner);
                n += inner.len();
            }
            _ => {
                v[n] = a;
                n += 1;
            }
        }
    }
    if !ac {
        return h.app(head, v);
    }
    k.order.sort(h, v);
    let Some(e) = sd.identity(h) else { return h.app(head, v) };
    let mut kept = 0;
    for i in 0..v.len() {
        if v[i] != e {
            v[kept] = v[i];
            kept += 1;
        }
    }
    match v[..kept] {
        [] => e,
        [a] => a,
        _ => h.app(head, &v[..kept]),
    }
}

/// Per-task evaluation state. The frontier's `Cx` is passed down by `&mut` along the leftmost path.
pub(crate) struct Cx<'h> {
    /// May perform effects (the task on the sequential frontier).
    pub(crate) front: bool,
    /// The definition epoch this task started from (stamps and validation use it).
    pub(crate) epoch: u32,
    /// Read set of the evaluation in progress (a 64-bit mask, decision 2.7).
    pub(crate) reads: u64,
    pub(crate) effects: u32,
    /// Fuel of the work this task keeps: on the frontier, what was charged to the budget.
    pub(crate) fuel: u64,
    /// Fuel in hand (drawn from the budget on the frontier, from the share otherwise).
    avail: u64,
    /// A speculative task's remaining share (decision F.3).
    share: u64,
    /// How many conditions this task is inside (effects are refused when nonzero, A.6).
    pub(crate) in_cond: u32,
    /// The cell allocation counter when the outermost condition was entered: older cells are read-only inside.
    pub(crate) cond_born: u64,
    /// The task's id for cell ownership (0: the frontier; see [`crate::cells`]).
    pub(crate) task: u64,
    /// Reads and allocations of cells: an evaluation that made any is not memoised.
    pub(crate) volatile: u32,
    /// Effects that may have stored a cell term somewhere outliving a block (its cells are then kept).
    pub(crate) escapes: u32,
    /// The cell allocation counter at the frontier's latest split: speculative tasks may read older cells.
    pub(crate) split_born: u64,
    /// Evaluation levels in progress on this task and the tasks it was split from (the frame limit).
    pub(crate) depth: u32,
    /// Nested drivers in progress on this task and the tasks it was split from (the native-callback limit).
    pub(crate) nest: u32,
    /// Scratch scopes open on this task ([NativeCx::scratch], decision G.3): groups are not split and effects are
    /// refused while one is.
    pub(crate) serial: u32,
    /// Cells written while a scratch scope is open: their values are copied out of it when it ends.
    pub(crate) scratch_cells: Option<Vec<i64>>,
    pub(crate) threads: u32,
    splits: u64,
    redos: u64,
    holes: u64,
    next_poll: u64,
    /// Dispatch scratch, one per active dispatch frame.
    #[allow(clippy::vec_box)] // boxed: popping and pushing a frame moves a pointer, not the buffers
    pool: Vec<Box<Scratch<'h>>>,
    /// The evaluation stack ([`crate::machine`]).
    pub(crate) stack: Vec<Frame<'h>>,
    /// The values of the arguments being evaluated, for the levels on the stack.
    pub(crate) vals: Vec<T<'h>>,
    /// Calls whose `unevaluated(x)` arguments were unwrapped: (level depth, the unwrapped call, the call as
    /// written); kept apart from the frames so that they stay small ([`crate::machine`])
    pub(crate) rewraps: Vec<(u32, T<'h>, T<'h>)>,
    /// Finished blocks' states, for reuse.
    #[allow(clippy::vec_box)] // boxed: a block frame holds a pointer, so the frames stay small
    pub(crate) blocks: Vec<Box<BlockState<'h>>>,
    /// The assumption context (`list(facts...)`, see [`crate::assume`]); `None` when empty. Copied into speculative
    /// tasks.
    pub(crate) assume: Option<T<'h>>,
    /// Calls are measured ([`crate::profile`]); inherited by speculative tasks.
    pub(crate) prof: bool,
    /// The inclusive nanoseconds of the profiled calls finished inside the current one.
    pub(crate) prof_child: u64,
    /// The rule calls in progress on this task.
    pub(crate) prof_stack: Vec<crate::profile::ProfFrame>,
    /// The element of a parallel run of a native this task computes ([`par`]): a cancelled one stops at its next
    /// refill. Inherited by the tasks split off it.
    pub(crate) cancel: par::Cancel,
}

impl<'h> Cx<'h> {
    pub(crate) fn frontier(epoch: u32, threads: u32) -> Cx<'h> {
        Cx {
            front: true,
            epoch,
            reads: 0,
            effects: 0,
            fuel: 0,
            avail: 0,
            share: 0,
            in_cond: 0,
            cond_born: 0,
            task: 0,
            volatile: 0,
            escapes: 0,
            split_born: 0,
            depth: 0,
            nest: 0,
            serial: 0,
            scratch_cells: None,
            threads,
            splits: 0,
            redos: 0,
            holes: 0,
            next_poll: 0,
            pool: vec![],
            stack: vec![],
            vals: vec![],
            rewraps: vec![],
            blocks: vec![],
            assume: None,
            prof: false,
            prof_child: 0,
            prof_stack: vec![],
            cancel: par::Cancel::NONE,
        }
    }
}

/// One evaluation's view: the heap and the kernel.
pub(crate) struct Eval<'k, 'h> {
    pub(crate) heap: &'h Heap,
    pub(crate) k: &'k Kernel,
}

pub(crate) enum Step<'h> {
    Done(T<'h>),
    Again(T<'h>),
}

impl<'k, 'h> Eval<'k, 'h> {
    /// `failure(tags...)`.
    pub(crate) fn failure(&self, tags: &[&str]) -> T<'h> {
        let h = self.heap;
        let args: Vec<T> = tags.iter().map(|t| h.sym_term(h.sym(t))).collect();
        h.app(h.sym_term(self.k.names.failure), &args)
    }

    #[inline]
    pub(crate) fn sym(&self, s: Sym) -> T<'h> {
        self.heap.sym_term(s)
    }

    #[inline]
    pub(crate) fn make(&self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        make(self.k, self.heap, head, args)
    }

    // --- fuel and limits -------------------------------------------------------------------------------------

    /// A new block of fuel: polls the control block; on the frontier draws from the budget (a failure there is the
    /// limit failure); a speculative task takes from its share (exhausting it leaves a hole).
    #[cold]
    fn refill(&self, cx: &mut Cx<'h>) -> Res<()> {
        let ctl = &*self.k.ctl;
        if ctl.halted() {
            return Err(Abort::Limit);
        }
        // the memory budget, polled with the fuel (decision p6core.11)
        if self.heap.young_bytes() > self.k.young_cap.min(self.k.mem_cap.load(Relaxed)) {
            ctl.fail(Status::Memory);
            return Err(Abort::Limit);
        }
        if cx.front {
            let got = ctl.draw(FUEL_BLOCK);
            if got == 0 {
                ctl.fail(Status::Fuel);
                return Err(Abort::Limit);
            }
            cx.avail = got;
        } else {
            // an exhausted share, or an element of a parallel run that is no longer needed (roadmap item 16A)
            if cx.share == 0 || cx.cancel.cancelled() {
                return Err(Abort::Hole);
            }
            let got = cx.share.min(FUEL_BLOCK);
            cx.share -= got;
            cx.avail = got;
        }
        Ok(())
    }

    /// One unit of fuel.
    #[inline]
    pub(crate) fn tick(&self, cx: &mut Cx<'h>) -> Res<()> {
        if cx.avail == 0 {
            self.refill(cx)?;
        }
        cx.avail -= 1;
        cx.fuel += 1;
        Ok(())
    }

    /// `n` units of fuel (natives' loops, matching).
    pub(crate) fn spend(&self, cx: &mut Cx<'h>, n: u64) -> Res<()> {
        let mut n = n;
        while n > 0 {
            if cx.avail == 0 {
                self.refill(cx)?;
            }
            let take = n.min(cx.avail);
            cx.avail -= take;
            cx.fuel += take;
            n -= take;
        }
        Ok(())
    }

    /// The context of a speculative task split off `cx`: its share is what `cx` could still spend.
    fn speculative(&self, cx: &Cx<'h>) -> Cx<'h> {
        let share = cx.avail + if cx.front { self.k.ctl.remaining() } else { cx.share };
        Cx {
            front: false,
            reads: 0,
            effects: 0,
            fuel: 0,
            avail: 0,
            share,
            task: self.k.cells.task(),
            volatile: 0,
            escapes: 0,
            splits: 0,
            redos: 0,
            holes: 0,
            next_poll: 0,
            pool: vec![],
            stack: vec![],
            vals: vec![],
            rewraps: vec![],
            blocks: vec![],
            scratch_cells: None,
            prof_child: 0,
            prof_stack: vec![],
            ..*cx
        }
    }

    /// Effects are allowed on the frontier, outside conditions (semantics 18.2, A.6).
    pub(crate) fn effect(&self, cx: &mut Cx<'h>) -> Res<()> {
        if cx.in_cond > 0 {
            return Err(Abort::EffectInCondition);
        }
        if !cx.front {
            return Err(Abort::Hole);
        }
        if cx.serial > 0 {
            // an effect could store terms of the scope where they outlive it (decision G.3)
            return Err(Abort::Scratch);
        }
        cx.effects += 1;
        Ok(())
    }

    // --- effects ---------------------------------------------------------------------------------------------

    /// Append to the output (an effect).
    pub(crate) fn emit(&self, cx: &mut Cx<'h>, t: T<'h>) -> Res<()> {
        self.effect(cx)?;
        self.note_store(cx, t);
        self.k.out.lock().push(Raw::new(t));
        Ok(())
    }

    /// `s = v` during an evaluation (an effect of the frontier).
    pub(crate) fn set_own(&self, cx: &mut Cx<'h>, s: Sym, v: T<'h>) -> Res<()> {
        self.effect(cx)?;
        self.note_store(cx, v);
        debug_assert_eq!(cx.epoch, self.k.epoch.load(Relaxed), "only the frontier defines");
        let ((), e) = self.k.defined(cx.epoch, bit(s), || self.k.defs.set_own(s, v));
        cx.epoch = e;
        Ok(())
    }

    /// The identity element changed during an evaluation (an effect of the frontier).
    pub(crate) fn set_identity(&self, cx: &mut Cx<'h>, s: Sym, v: Option<T<'h>>) -> Res<()> {
        self.effect(cx)?;
        if let Some(v) = v {
            self.note_store(cx, v);
        }
        let ((), e) = self.k.defined(cx.epoch, bit(s), || self.k.defs.set_identity(s, v));
        cx.epoch = e;
        Ok(())
    }

    /// `v` is stored where it outlives the current block: if it may hold a cell, the block's cells escape.
    #[inline]
    pub(crate) fn note_store(&self, cx: &mut Cx<'h>, v: T<'h>) {
        if self.heap.mask(v) & CELL_BIT != 0 {
            cx.escapes += 1;
        }
    }

    /// Attributes changed during an evaluation (an effect of the frontier).
    pub(crate) fn set_attributes(&self, cx: &mut Cx<'h>, s: Sym, a: Attributes) -> Res<()> {
        self.effect(cx)?;
        let ((), e) = self.k.defined(cx.epoch, bit(s), || self.k.set_attrs(s, a));
        cx.epoch = e;
        Ok(())
    }

    /// `unset(s)` during an evaluation (an effect of the frontier): the own value, identity element and rules of `s`
    /// removed ([`Defs::unset`](crate::defs::Defs)); whether there were any.
    pub(crate) fn unset(&self, cx: &mut Cx<'h>, s: Sym) -> Res<bool> {
        self.effect(cx)?;
        let (any, e) = self.k.defined(cx.epoch, bit(s), || self.k.defs.unset(s));
        cx.epoch = e;
        Ok(any)
    }

    /// `unset(f(...))` during an evaluation: the rule of `s` with left side `lhs` (up to renaming) removed; whether
    /// there was one.
    pub(crate) fn unset_rule(&self, cx: &mut Cx<'h>, lhs: Pat) -> Res<Result<bool, DefError>> {
        self.effect(cx)?;
        let h = self.heap;
        let lhs = match compile(h, crate::pat::curry_lit(h, lhs), &CanonOf(self.k)) {
            Ok(p) => p,
            Err(e) => return Ok(Err(e)),
        };
        let top = match lhs.strip() {
            Pat::App { head, .. } => head.get(h),
            Pat::Lit(t) => match h.view(t.get(h)) {
                View::App { head, .. } => head,
                _ => return Ok(Err(DefError::NotCompound)),
            },
            _ => return Ok(Err(DefError::NotCompound)),
        };
        let View::Sym(s) = h.view(top) else { return Ok(Err(DefError::NotCompound)) };
        let (any, e) = self.k.defined(cx.epoch, bit(s), || self.k.defs.unset_rule(h, s, &lhs));
        cx.epoch = e;
        Ok(Ok(any))
    }

    /// A rule definition during an evaluation (an effect of the frontier): inserted into the live index while
    /// speculative tasks may be reading it.
    pub(crate) fn define_rule(&self, cx: &mut Cx<'h>, lhs: Pat, rhs: RuleRhs, attach: Option<Sym>) -> Res<Result<(), DefError>> {
        self.effect(cx)?;
        // a rule's terms may hold cells: conservatively, every rule definition lets them escape
        cx.escapes += 1;
        debug_assert_eq!(cx.epoch, self.k.epoch.load(Relaxed), "only the frontier defines");
        Ok(self.k.define_rule(self.heap, cx.epoch, lhs, rhs, attach).map(|e| cx.epoch = e))
    }

    // --- evaluation ------------------------------------------------------------------------------------------

    /// The value the compiled specialisation of the call's head gives, if the head has one and it accepts the
    /// arguments ([`crate::compiled`]). Consulting it is a read of the head.
    #[inline]
    pub(crate) fn compiled(&self, cx: &mut Cx<'h>, t: T<'h>) -> Option<T<'h>> {
        let h = self.heap;
        let View::App { head, args } = h.view(t) else { return None };
        let s = sym_of(h, head)?;
        let c = compiled_of(&self.k.defs, &self.k.compiled, s)?;
        cx.reads |= bit(s);
        let _scope = crate::guard::NativeScope { sym: s, slot: &self.k.panics, ctl: &self.k.ctl };
        if !cx.prof {
            return c.call(&mut NativeCx { ev: self, cx }, args);
        }
        let (saved, start) = (std::mem::take(&mut cx.prof_child), std::time::Instant::now());
        let r = c.call(&mut NativeCx { ev: self, cx }, args);
        let total = crate::profile::nanos(start);
        if r.is_some() {
            self.k.profiler.record(s, crate::profile::CallKind::Compiled, total, total.saturating_sub(cx.prof_child));
        }
        cx.prof_child = saved + total;
        r
    }

    /// Close the profiled rule calls whose evaluation level (at depth `depth` or deeper) has finished.
    #[cold]
    pub(crate) fn prof_close(&self, cx: &mut Cx<'h>, depth: usize) {
        while let Some(f) = cx.prof_stack.last().copied().filter(|f| f.depth >= depth) {
            cx.prof_stack.pop();
            let total = crate::profile::nanos(f.start);
            self.k.profiler.record(f.sym, crate::profile::CallKind::Rules, total, total.saturating_sub(cx.prof_child));
            cx.prof_child = f.saved + total;
        }
    }

    /// The parameters and body of a closure `fn(list(params), body)`.
    #[inline]
    pub(crate) fn closure(&self, f: T<'h>) -> Option<(&'h [T<'h>], T<'h>)> {
        let h = self.heap;
        let View::App { head, args: &[ps, body] } = h.view(f) else { return None };
        if head != self.sym(self.k.names.scopes.fn_) {
            return None;
        }
        match h.view(ps) {
            View::App { head: l, args } if l == self.sym(self.k.names.list) => Some((args, body)),
            _ => None,
        }
    }

    /// The argument modes of a closure: its `held` parameters (a `held` `rest` parameter holds every argument from
    /// its position on). `None` if a parameter is malformed (the call gives `failure(no_match, ...)`).
    pub(crate) fn closure_hold(&self, params: &[T<'h>]) -> Option<Hold> {
        let sc = &self.k.names.scopes;
        let mut hold = 0u32;
        for (i, &p) in params.iter().enumerate() {
            let info = Param::of(self.heap, sc, p)?;
            if info.held {
                hold |= if info.rest { !0u32 << i.min(31) } else { 1u32 << i.min(31) };
            }
        }
        Some(Hold::from_mask(hold))
    }

    /// Apply closure `f` to its evaluated arguments `vals` (a failure among them is the result): bound to the
    /// parameters in order (`optional` defaults evaluated when an argument is missing, seeing the parameters before
    /// them; `typed` checked; `rest` taking the remaining arguments as a list) and substituted into the body
    /// hygienically. A call the parameters do not accept gives `failure(no_match, f, [args])`.
    pub(crate) fn bind_closure(&self, cx: &mut Cx<'h>, f: T<'h>, params: &[T<'h>], body: T<'h>, vals: &[T<'h>]) -> Res<Step<'h>> {
        let h = self.heap;
        let sc = &self.k.names.scopes;
        // a held parameter receives its argument as written, so a `failure(...)` term there is data, not a failed
        // argument (C9B.1)
        let hold = self.closure_hold(params).unwrap_or_default();
        if let Some(fail) = self.first_failure_strict(vals, hold) {
            return Ok(Step::Done(fail));
        }
        let mut pairs: Vec<(Sym, T<'h>)> = Vec::with_capacity(params.len());
        let mut j = 0;
        for &p in params {
            let Some(info) = Param::of(h, sc, p) else { return Ok(Step::Done(self.no_match(f, vals))) };
            if info.rest {
                pairs.push((info.name, h.call(self.k.names.list, &vals[j..])));
                j = vals.len();
                continue;
            }
            let v = if j < vals.len() {
                j += 1;
                vals[j - 1]
            } else if let Some(d) = info.default {
                // a default sees the parameters before it
                let d = crate::subst::substitute(h, sc, d, &pairs);
                self.eval(cx, d)?
            } else {
                return Ok(Step::Done(self.no_match(f, vals)));
            };
            if info.ty.is_some_and(|ty| !has_type(h, v, ty)) {
                return Ok(Step::Done(self.no_match(f, vals)));
            }
            pairs.push((info.name, v));
        }
        // a slot function `&(...)` (parameters `&1`, ..., `&n`) ignores arguments past `&n`: the shorthand is
        // passed to functions that hand over more than it uses, an index or a tag (C9B.9)
        if j < vals.len() && !is_slot_params(h, params) {
            return Ok(Step::Done(self.no_match(f, vals)));
        }
        Ok(Step::Again(crate::subst::substitute(h, sc, body, &pairs)))
    }

    /// `failure(no_match, f, [args])` (04 section 6.2).
    pub(crate) fn no_match(&self, f: T<'h>, args: &[T<'h>]) -> T<'h> {
        let h = self.heap;
        let nm = &self.k.names;
        h.call(nm.failure, &[self.sym(nm.no_match), f, h.call(nm.list, args)])
    }

    /// Is `t` a packed array (semantics 11) while `list` has no definitions? Then `t` is its own normal form: its
    /// head is the inert symbol `list` and its elements are numbers, so evaluating it would only unpack it (decision
    /// N4a.6). The read of `list` is recorded as evaluating the compound term would.
    #[inline]
    pub(crate) fn is_inert_packed(&self, cx: &mut Cx<'h>, t: T<'h>) -> bool {
        if self.heap.packed(t).is_none() {
            return false;
        }
        let list = self.k.names.list;
        cx.reads |= bit(list);
        !self.k.defs.has_meaning(list) && !self.list_holds()
    }

    /// Whether `list` holds its arguments or is listable (attributes that would change a packed array's evaluation).
    fn list_holds(&self) -> bool {
        let a = self.k.defs.attrs(self.k.names.list);
        a.hold != crate::defs::Hold::None || a.listable || a.ac
    }

    /// A listable `add` or `mul` over packed arrays computed on their payloads (`crate::packed`): the normal form, or
    /// `None` when the shortcut does not apply and ordinary threading follows.
    pub(crate) fn thread_packed(&self, cx: &mut Cx<'h>, head: T<'h>, vals: &[T<'h>]) -> Res<Option<T<'h>>> {
        let h = self.heap;
        let nm = &self.k.names;
        if !vals.iter().any(|&v| h.packed(v).is_some()) {
            return Ok(None);
        }
        // the heads are recognised by name: interning them here would shift symbol indices (and read-set bits)
        let op = match sym_of(h, head).map(|s| h.sym_name(s)).as_deref() {
            Some(mc_api::names::ADD) => crate::packed::Op::Add,
            Some(mc_api::names::MUL) => crate::packed::Op::Mul,
            Some(mc_api::names::POW) => crate::packed::Op::Pow,
            _ => return Ok(None),
        };
        if self.k.defs.has_meaning(nm.list) || self.list_holds() {
            return Ok(None);
        }
        cx.reads |= bit(nm.list);
        crate::packed::elementwise(self.heap, op, vals, &mut |n| self.spend(cx, n))
    }

    /// A listable call of a native on one packed array, computed by the native on the payload ([`Native::packed`]);
    /// `None` when the native does not take it (then the call is threaded).
    pub(crate) fn native_packed(&self, cx: &mut Cx<'h>, head: T<'h>, vals: &[T<'h>]) -> Res<Option<T<'h>>> {
        let h = self.heap;
        let &[x] = vals else { return Ok(None) };
        if h.packed(x).is_none() {
            return Ok(None);
        }
        let Some(s) = sym_of(h, head) else { return Ok(None) };
        let Some(n) = self.k.defs.get(s).map(|sd| sd.native.load(Acquire)).filter(|&n| n != 0) else { return Ok(None) };
        match &self.k.natives[n as usize - 1] {
            Impl::Native { native, .. } => native.packed(&mut NativeCx { ev: self, cx }, x),
            Impl::Form(_) => Ok(None),
        }
    }

    /// A listable call with list arguments: the list of calls on corresponding elements, the other arguments
    /// broadcast; `failure(shape_mismatch)` for lists of different lengths. `None` without list arguments.
    pub(crate) fn thread(&self, head: T<'h>, vals: &[T<'h>]) -> Option<T<'h>> {
        let h = self.heap;
        let list = self.sym(self.k.names.list);
        let as_list = |t: T<'h>| match h.view(t) {
            View::App { head, args } if head == list => Some(args),
            _ => None,
        };
        let mut n = None;
        for &v in vals {
            if let Some(xs) = as_list(v) {
                match n {
                    None => n = Some(xs.len()),
                    Some(m) if m != xs.len() => return Some(self.failure(&["shape_mismatch"])),
                    Some(_) => {}
                }
            }
        }
        let threaded = self.sym(self.k.names.threaded);
        let is_th = |t: T<'h>| matches!(h.view(t), View::App { head, args } if head == threaded && matches!(args.len(), 1 | 2));
        if vals.iter().any(|&v| is_th(v)) {
            return Some(self.thread_threaded(head, vals, threaded));
        }
        let Some(n) = n else { return self.thread_assoc(head, vals) };
        let mut parts = Vec::with_capacity(vals.len());
        let elems: Vec<T<'h>> = (0..n)
            .map(|i| {
                parts.clear();
                parts.extend(vals.iter().map(|&v| as_list(v).map_or(v, |xs| xs[i])));
                h.app(head, &parts)
            })
            .collect();
        Some(h.app(list, &elems))
    }

    /// A listable call with `threaded(v)` or `threaded(v, n)` arguments: each is spread over the levels of the array
    /// arguments (trailing levels by default, level `n` from the top for `n > 0`, `r + n + 1` for `n < 0`; over the
    /// longest threaded value when there are no arrays), then the call is made again with plain arrays. When there
    /// are no array arguments the result is threaded again. `failure(shape_mismatch)` or `failure(threaded_level, n)`
    /// when a value does not fit (the rules of `elementwise`, `lib/langlib.mc`).
    fn thread_threaded(&self, head: T<'h>, vals: &[T<'h>], threaded: T<'h>) -> T<'h> {
        let h = self.heap;
        let list = self.sym(self.k.names.list);
        let th_parts = |t: T<'h>| match h.view(t) {
            View::App { head: g, args } if g == threaded && matches!(args.len(), 1 | 2) => Some(args),
            _ => None,
        };
        let shape = |t: T<'h>| self.array_dims(t, list);
        let arrays: Vec<Vec<usize>> = vals.iter().filter(|&&v| matches!(h.view(v), View::App { head: g, .. } if g == list)).map(|&v| shape(v)).collect();
        let had_arrays = !arrays.is_empty();
        let shapes: Vec<Vec<usize>> = if had_arrays {
            arrays
        } else {
            vals.iter().filter_map(|&v| th_parts(v)).map(|a| shape(a[0])).collect()
        };
        let dims = shapes.into_iter().fold(Vec::new(), |best: Vec<usize>, d| if d.len() >= best.len() { d } else { best });
        let mut out = Vec::with_capacity(vals.len());
        for &v in vals {
            let Some(a) = th_parts(v) else {
                out.push(v);
                continue;
            };
            let vd = shape(a[0]);
            if vd.is_empty() {
                // a scalar acts at every level: it is broadcast like a plain argument (X26-12)
                out.push(a[0]);
                continue;
            }
            let r = dims.len() as i64;
            let nn = a.get(1).copied();
            let s = match nn.map(|t| h.view(t)) {
                None => r - vd.len() as i64 + 1,
                Some(View::Int(n)) if n > 0 => n,
                Some(View::Int(n)) => r + n + 1,
                Some(_) => 0,
            };
            if s < 1 || s + vd.len() as i64 - 1 > r {
                let mut args = vec![h.sym_term(h.sym("threaded_level"))];
                args.push(nn.unwrap_or_else(|| self.sym(self.k.names.null)));
                return h.app(h.sym_term(self.k.names.failure), &args);
            }
            let s = (s - 1) as usize;
            if dims[s..s + vd.len()] != vd[..] {
                return self.failure(&["shape_mismatch"]);
            }
            out.push(self.tile(a[0], &dims, 0, s, s + vd.len() - 1, list));
        }
        let call = h.app(head, &out);
        if had_arrays {
            call
        } else {
            h.app(threaded, &[call])
        }
    }

    /// The dimensions of a (possibly ragged) nested list: the longest common rectangular prefix.
    fn array_dims(&self, t: T<'h>, list: T<'h>) -> Vec<usize> {
        let h = self.heap;
        match h.view(t) {
            View::App { head, args } if head == list => {
                let mut d = vec![args.len()];
                let mut it = args.iter().map(|&x| self.array_dims(x, list));
                if let Some(first) = it.next() {
                    let mut common = first;
                    for o in it {
                        let k = common.iter().zip(&o).take_while(|(a, b)| a == b).count();
                        common.truncate(k);
                    }
                    d.extend(common);
                }
                d
            }
            _ => vec![],
        }
    }

    /// `v` spread over an array of dimensions `dims`: the element at `[i1, ..., ir]` is the one of `v` at levels `s..=e`.
    fn tile(&self, v: T<'h>, dims: &[usize], lvl: usize, s: usize, e: usize, list: T<'h>) -> T<'h> {
        let h = self.heap;
        if lvl >= dims.len() {
            return v;
        }
        if lvl < s || lvl > e {
            let inner = self.tile(v, dims, lvl + 1, s, e, list);
            return h.app(list, &vec![inner; dims[lvl]]);
        }
        match h.view(v) {
            View::App { head, args } if head == list => {
                let xs: Vec<T<'h>> = args.iter().map(|&x| self.tile(x, dims, lvl + 1, s, e, list)).collect();
                h.app(list, &xs)
            }
            _ => v,
        }
    }

    /// Whether a call of `head` on `vals` threads over associations although `head` is not listable: some argument
    /// is an association and `head` maps over lists itself ([`Kernel::set_maps_lists`]).
    /// A call whose last arguments are two or more `options(...)` terms: the call with them merged into one, in
    /// order (a later value of the same name wins where options are read; gap G65). `None` otherwise.
    pub(crate) fn merge_options(&self, head: T<'h>, vals: &[T<'h>]) -> Option<T<'h>> {
        let h = self.heap;
        let opts = self.sym(self.k.names.scopes.options);
        // a packed array is never an option list: its head is not read (viewing it would unpack it)
        let is_opts = |t: &T<'h>| h.packed(*t).is_none() && matches!(h.view(*t), View::App { head, .. } if head == opts);
        let n = vals.iter().rev().take_while(|t| is_opts(t)).count();
        if n < 2 || head == opts {
            return None;
        }
        let first = vals.len() - n;
        // the rules of all of them, the later ones replacing earlier rules of the same name in place
        let mut rules: Vec<T<'h>> = vec![];
        for t in &vals[first..] {
            let View::App { args, .. } = h.view(*t) else { continue };
            for &r in args {
                let name = |r: T<'h>| match h.view(r) {
                    View::App { args: &[k, _], .. } => Some(k),
                    _ => None,
                };
                match name(r).and_then(|k| rules.iter().position(|&q| name(q) == Some(k))) {
                    Some(i) => {
                        rules.remove(i);
                        rules.push(r);
                    }
                    None => rules.push(r),
                }
            }
        }
        let mut args = vals[..first].to_vec();
        args.push(h.app(opts, &rules));
        Some(h.app(head, &args))
    }

    pub(crate) fn maps_associations(&self, head: T<'h>, vals: &[T<'h>]) -> bool {
        let Some((assoc, _)) = self.k.assoc_heads else { return false };
        if self.k.maps_lists.is_empty() {
            return false;
        }
        let h = self.heap;
        let assoc = self.sym(assoc);
        // the head first, and a packed array is never an association (viewing it would unpack it)
        sym_of(h, head).is_some_and(|s| self.k.maps_lists.contains(&s))
            && vals.iter().any(|&v| h.packed(v).is_none() && matches!(h.view(v), View::App { head, .. } if head == assoc))
    }

    /// A listable call with association arguments and no list arguments: the association of the calls on the
    /// values of each key, the other arguments broadcast (decision io-core.1). The associations must have the same
    /// keys (in any order; the first one's order is kept), else `failure(shape_mismatch)`. `None` without
    /// association arguments.
    pub(crate) fn thread_assoc(&self, head: T<'h>, vals: &[T<'h>]) -> Option<T<'h>> {
        let (assoc, rule) = self.k.assoc_heads?;
        let h = self.heap;
        let (assoc, rule) = (self.sym(assoc), self.sym(rule));
        let entries = |t: T<'h>| match h.view(t) {
            View::App { head, args } if head == assoc => Some(args),
            _ => None,
        };
        let kv = |r: T<'h>| match h.view(r) {
            View::App { head, args: &[k, v] } if head == rule => Some((k, v)),
            _ => None,
        };
        let first = vals.iter().find_map(|&v| entries(v))?;
        let keys: Vec<T<'h>> = first.iter().map(|&r| kv(r).map(|(k, _)| k)).collect::<Option<_>>()?;
        // the value of each key in each association argument, aligned with `keys`
        let mut cols: Vec<Option<Vec<T<'h>>>> = Vec::with_capacity(vals.len());
        for &v in vals {
            let Some(es) = entries(v) else {
                cols.push(None);
                continue;
            };
            if es.len() != keys.len() {
                return Some(self.failure(&["shape_mismatch"]));
            }
            let pairs: Vec<(T<'h>, T<'h>)> = es.iter().map(|&r| kv(r)).collect::<Option<_>>()?;
            let mut col = Vec::with_capacity(keys.len());
            for (i, &k) in keys.iter().enumerate() {
                // the same order is the common case; otherwise a search (associations of a few keys)
                let v = if pairs[i].0 == k { Some(pairs[i].1) } else { pairs.iter().find(|p| p.0 == k).map(|p| p.1) };
                match v {
                    Some(v) => col.push(v),
                    None => return Some(self.failure(&["shape_mismatch"])),
                }
            }
            cols.push(Some(col));
        }
        let mut parts = Vec::with_capacity(vals.len());
        let out: Vec<T<'h>> = keys
            .iter()
            .enumerate()
            .map(|(i, &k)| {
                parts.clear();
                parts.extend(vals.iter().zip(&cols).map(|(&v, c)| c.as_ref().map_or(v, |c| c[i])));
                h.app(rule, &[k, h.app(head, &parts)])
            })
            .collect();
        Some(h.app(assoc, &out))
    }

    // --- cells (crate::cells) --------------------------------------------------------------------------------

    /// A new cell holding `v`: its id and the term `local_cell(id)` standing for it.
    pub(crate) fn alloc_cell(&self, cx: &mut Cx<'h>, v: T<'h>) -> (i64, T<'h>) {
        cx.volatile += 1;
        let id = self.k.cells.alloc(cx.front, cx.task, v);
        if let Some(c) = cx.scratch_cells.as_mut() {
            c.push(id);
        }
        (id, self.heap.call(self.k.names.local_cell, &[self.heap.int(id)]))
    }

    /// The value of cell `id`, if it is allocated (the read is recorded and makes the evaluation volatile).
    pub(crate) fn read_cell(&self, cx: &mut Cx<'h>, id: i64) -> Option<T<'h>> {
        cx.volatile += 1;
        cx.reads |= CELL_BIT;
        self.k.cells.slot(id)?.get(self.heap)
    }

    /// Write `v` to cell `id`; false when there is no such cell. Only the cell's owner writes (others leave a
    /// hole), and inside a condition only cells allocated in it.
    pub(crate) fn write_cell(&self, cx: &mut Cx<'h>, id: i64, v: T<'h>) -> Res<bool> {
        let Some(slot) = self.k.cells.slot(id) else { return Ok(false) };
        let born = slot.born.load(Relaxed);
        let owned = if id > 0 { cx.front } else { slot.owner.load(Relaxed) == cx.task };
        if !owned {
            // the frontier never meets a speculative cell; a speculative task cannot write the frontier's
            return if cx.front { Ok(false) } else { Err(Abort::Hole) };
        }
        if cx.in_cond > 0 && born <= cx.cond_born {
            return Err(Abort::EffectInCondition);
        }
        if cx.front && born <= cx.split_born {
            // a speculative task may have read it: speculative work that did is redone
            let ((), e) = self.k.defined(cx.epoch, CELL_BIT, || ());
            cx.epoch = e;
        }
        cx.volatile += 1;
        self.note_store(cx, v);
        slot.set(v);
        if let Some(c) = cx.scratch_cells.as_mut() {
            c.push(id);
        }
        Ok(true)
    }

    /// The ids of the cells `t` refers to that satisfy `pred` (the first one found).
    pub(crate) fn find_cell(&self, t: T<'h>, pred: impl Fn(i64) -> bool) -> Option<i64> {
        let h = self.heap;
        let lc = self.sym(self.k.names.local_cell);
        let b = h.mask(lc);
        if h.mask(t) & b == 0 {
            return None;
        }
        let mut seen = rustc_hash::FxHashSet::default();
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if h.mask(u) & b == 0 || !seen.insert(u) {
                continue;
            }
            if let View::App { head, args } = h.view(u) {
                if head == lc {
                    if let [a] = args {
                        if let View::Int(id) = h.view(*a) {
                            if pred(id) {
                                return Some(id);
                            }
                        }
                    }
                }
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        None
    }

    /// The end of a block that allocated `cells` (frontier cells; `escapes0` the escape count at its start): they
    /// are freed unless the block's value or an effect may hold them.
    pub(crate) fn end_block(&self, cx: &mut Cx<'h>, cells: &[i64], escapes0: u32, value: Option<T<'h>>) {
        if cells.is_empty() || !cx.front {
            return;
        }
        if cx.escapes == escapes0 && value.is_none_or(|v| self.find_cell(v, |id| cells.contains(&id)).is_none()) {
            self.k.cells.free(cells);
        }
    }

    pub(crate) fn effect_failure(&self) -> T<'h> {
        let h = self.heap;
        h.app(self.sym(self.k.names.failure), &[self.sym(self.k.names.effect_in_condition)])
    }

    /// The first argument in a strict position (not held by `hold`) that is a failure (semantics 10): a held
    /// argument is passed unevaluated, so a failure term in it is not a failed argument (`if c then failure(...)
    /// else x` must not fail when `c` is false; C9B.1).
    #[inline]
    pub(crate) fn first_failure_strict(&self, vals: &[T<'h>], hold: Hold) -> Option<T<'h>> {
        if hold == Hold::None {
            return self.first_failure(vals);
        }
        let h = self.heap;
        let fb = bit(self.k.names.failure);
        let failure = self.sym(self.k.names.failure);
        vals.iter().enumerate().filter(|&(i, _)| !hold.holds(i)).map(|(_, &a)| a).find(|&a| {
            h.mask(a) & fb != 0 && h.packed(a).is_none() && matches!(h.view(a), View::App { head, .. } if head == failure)
        })
    }

    /// The first argument that is a failure (semantics 10).
    #[inline]
    pub(crate) fn first_failure(&self, vals: &[T<'h>]) -> Option<T<'h>> {
        let h = self.heap;
        let fb = bit(self.k.names.failure);
        let failure = self.sym(self.k.names.failure);
        vals.iter()
            .copied()
            .find(|&a| h.mask(a) & fb != 0 && h.packed(a).is_none() && matches!(h.view(a), View::App { head, .. } if head == failure))
    }

    /// The replacement from the first matching rule for `t`: argument-head rules of the arguments' heads in argument
    /// order, then the head's rules (semantics 4.1 step 3.5, 6.5). Consulting a symbol's rules is a read, even when
    /// it holds none (spike 2). Conditions run inside, on this task's context.
    pub(crate) fn dispatch(&self, cx: &mut Cx<'h>, t: T<'h>) -> Res<Option<T<'h>>> {
        let View::App { head, args } = self.heap.view(t) else { return Ok(None) };
        let Some(hs) = sym_of(self.heap, head) else { return self.dispatch_curried(cx, head, args) };
        let mut sc = cx.pool.pop().unwrap_or_default();
        let r = self.dispatch_in(cx, t, hs, args, &mut sc);
        cx.pool.push(sc);
        r
    }

    /// A call `head(args...)` whose head is compound: the rules of `call$` (the definitions `f(x_)(y_) := ...`)
    /// tried on the view `call$(head, args...)` (C9B2.4).
    fn dispatch_curried(&self, cx: &mut Cx<'h>, head: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
        let h = self.heap;
        let call = h.sym(crate::pat::CALL);
        let defs = &self.k.defs;
        // the innermost head symbol's rules for such calls, then those of head patterns without a symbol
        let mut root = head;
        while let View::App { head: g, .. } = h.view(root) {
            root = g;
        }
        let root = sym_of(h, root);
        if let Some(r) = root {
            cx.reads |= bit(r);
        }
        cx.reads |= bit(call);
        let own = root.and_then(|r| defs.get(r)).filter(|d| d.has_arg_rules()).and_then(|d| d.arg_rules(call));
        let general = defs.get(call).and_then(SymDefs::rules);
        if own.is_none() && general.is_none() {
            return Ok(None);
        }
        let mut all = Vec::with_capacity(args.len() + 1);
        all.push(head);
        all.extend_from_slice(args);
        let view = h.call(call, &all);
        let mut sc = cx.pool.pop().unwrap_or_default();
        let r = {
            let mt = Matcher::new(self, cx);
            let mut found = None;
            for rs in [own, general].into_iter().flatten() {
                found = rs.find(&mt, h, view, &mut sc);
                if mt.stopped().is_some() || found.is_some() {
                    break;
                }
            }
            match mt.stopped() {
                Some(a) => Err(a),
                None => Ok(found.map(|r| r.rhs.build(&sc.out, h, &self.k.names))),
            }
        };
        cx.pool.push(sc);
        r
    }

    fn dispatch_in(&self, cx: &mut Cx<'h>, t: T<'h>, hs: Sym, args: &[T<'h>], sc: &mut Scratch<'h>) -> Res<Option<T<'h>>> {
        let h = self.heap;
        let defs = &self.k.defs;
        let found = {
            let mt = Matcher::new(self, cx);
            let mut found = None;
            for &a in args {
                let Some(s) = attach_key(h, a).and_then(|g| sym_of(h, g)) else { continue };
                mt.read(bit(s));
                let Some(rs) = defs.get(s).filter(|d| d.has_arg_rules()).and_then(|d| d.arg_rules(hs)) else { continue };
                found = rs.find(&mt, h, t, sc);
                if let Some(a) = mt.stopped() {
                    return Err(a);
                }
                if found.is_some() {
                    break;
                }
            }
            if found.is_none() {
                mt.read(bit(hs));
                if let Some(rs) = defs.get(hs).and_then(SymDefs::rules) {
                    found = rs.find(&mt, h, t, sc);
                    if let Some(a) = mt.stopped() {
                        return Err(a);
                    }
                }
            }
            found
        };
        Ok(found.map(|r| r.rhs.build(&sc.out, h, &self.k.names)))
    }

    /// Does compiled pattern `p` match `t`?
    pub(crate) fn match_pattern(&self, cx: &mut Cx<'h>, p: &Pattern, t: T<'h>) -> Res<bool> {
        let mut sc = cx.pool.pop().unwrap_or_default();
        let r = {
            let mt = Matcher::new(self, cx);
            let m = mt.matches(&p.pat, p.nvars, t, &mut sc.binds);
            match mt.stopped() {
                Some(a) => Err(a),
                None => Ok(m),
            }
        };
        cx.pool.push(sc);
        r
    }

    /// The first match of `p` against `t`, its bindings in `out`.
    pub(crate) fn match_binds(&self, cx: &mut Cx<'h>, p: &Pattern, t: T<'h>, out: &mut Binds<'h>) -> Res<bool> {
        let mut sc = cx.pool.pop().unwrap_or_default();
        let r = {
            let mt = Matcher::new(self, cx);
            let m = mt.first(&p.pat, p.nvars, t, &mut sc.binds, out);
            match mt.stopped() {
                Some(a) => Err(a),
                None => Ok(m),
            }
        };
        cx.pool.push(sc);
        r
    }

    /// The bindings of every match of `p` against `t`, in the enumeration order (semantics 6.2), at most `limit`
    /// of them (`replace_list`).
    pub(crate) fn match_all(&self, cx: &mut Cx<'h>, p: &Pattern, t: T<'h>, limit: usize) -> Res<Vec<Binds<'h>>> {
        let mut sc = cx.pool.pop().unwrap_or_default();
        let mut out: Vec<Binds<'h>> = vec![];
        let r = {
            let mt = Matcher::new(self, cx);
            if limit > 0 {
                mt.each(&p.pat, p.nvars, t, &mut sc.binds, &mut |b| {
                    out.push(b.clone());
                    out.len() >= limit
                });
            }
            match mt.stopped() {
                Some(a) => Err(a),
                None => Ok(()),
            }
        };
        cx.pool.push(sc);
        r.map(|()| out)
    }

    // --- groups ----------------------------------------------------------------------------------------------

    #[inline]
    pub(crate) fn may_split(&self, cx: &Cx<'h>) -> bool {
        if cx.serial > 0 {
            // a scratch scope's terms stay on its thread (decision G.2)
            return false;
        }
        match self.k.cfg.split {
            Split::Never => false,
            Split::Lazy => cx.threads >= 2 && cx.nest < SPLIT_NEST,
            Split::Always => cx.nest < SPLIT_NEST,
        }
    }

    /// Whether evaluating `a` can change it (a compound term, or a symbol with an own value).
    #[inline]
    pub(crate) fn needs_eval(&self, cx: &mut Cx<'h>, a: T<'h>) -> bool {
        if self.is_inert_packed(cx, a) {
            return false;
        }
        match self.heap.view(a) {
            View::App { .. } => true,
            View::Sym(s) => {
                cx.reads |= bit(s);
                self.k.defs.own(self.heap, s).is_some()
            }
            _ => false,
        }
    }

    /// The static cost estimate of evaluating `a` (decision F.10): 0 for a call of a function declared `effect`
    /// (it would only leave a hole), else the larger of its argument count plus one and its head's learned cost.
    #[inline]
    fn estimate(&self, a: T<'h>) -> u64 {
        let h = self.heap;
        if h.packed(a).is_some() {
            // a packed array is a normal form unless `list` has definitions: nothing to split
            return 1;
        }
        match h.view(a) {
            View::App { head, args } => match sym_of(h, head) {
                Some(s) if self.k.defs.attrs(s).effect => 0,
                s => (args.len() as u64 + 1).max(s.map_or(0, |s| self.k.head_cost(s))),
            },
            _ => 1,
        }
    }

    /// Whether these arguments may form a split group: two of them are evaluated, and under the lazy rule a pair
    /// (which splits only before its first element) has a static estimate reaching the grain; a longer group may
    /// also split later, by measured cost. Arguments that cannot split are evaluated on the stack without the
    /// group's bookkeeping.
    pub(crate) fn may_split_args(&self, args: &[T<'h>], hold: Hold) -> bool {
        // a group needs two evaluated arguments; a held one is passed as it is (and a held cell term would make a
        // speculative element a hole)
        if (0..args.len()).filter(|&i| !self.held(hold, i, args[i])).nth(1).is_none() {
            return false;
        }
        match (self.k.cfg.split, args) {
            (Split::Lazy, &[a, b]) => {
                let est = |i: usize, t: T<'h>| if self.held(hold, i, t) { 0 } else { self.estimate(t) };
                est(0, a) + est(1, b) >= self.k.cfg.grain
            }
            _ => true,
        }
    }

    /// Whether argument `i` of a call is passed unevaluated: its position is held and it is not `eval(x)` written
    /// there, the one escape from holding (semantics 5), which makes that argument strict.
    #[inline]
    pub(crate) fn held(&self, hold: Hold, i: usize, a: T<'h>) -> bool {
        hold.holds(i) && !self.forced(a)
    }

    /// Whether `a` is `eval(x)`: a held argument written so is evaluated.
    #[inline]
    pub(crate) fn forced(&self, a: T<'h>) -> bool {
        let h = self.heap;
        let s = self.k.names.eval;
        h.mask(a) & bit(s) != 0 && matches!(h.view(a), View::App { head, args } if args.len() == 1 && head == self.sym(s))
    }

    /// The strict arguments of a call as a group split by the granularity rule (the speculative path of argument
    /// evaluation; [`crate::machine`] evaluates them one at a time otherwise).
    pub(crate) fn split_args(&self, cx: &mut Cx<'h>, args: &'h [T<'h>], hold: Hold) -> Res<Vec<T<'h>>> {
        let mut out = args.to_vec();
        let est = |i: usize| if self.held(hold, i, args[i]) { 0 } else { self.estimate(args[i]) };
        let k = args.len().min(8);
        let hint = (0..k).map(est).sum::<u64>() * args.len() as u64 / k as u64;
        let f = |ev: &Self, cx: &mut Cx<'h>, i: usize| if ev.held(hold, i, args[i]) { Ok(args[i]) } else { ev.eval(cx, args[i]) };
        if self.range(cx, 0, args.len(), hint, 0, &mut out, &mut vec![0; args.len()], &est, &f)? {
            return Err(Abort::Hole);
        }
        Ok(out)
    }

    /// `n` elements `f(0..n)`, split by the granularity rule; `c` is the estimated cost of one element.
    pub(crate) fn group<F>(&self, cx: &mut Cx<'h>, c: u64, n: usize, f: &F) -> Res<Vec<T<'h>>>
    where
        F: Fn(&Self, &mut Cx<'h>, usize) -> Res<T<'h>> + Sync,
    {
        let mut out = vec![hole(); n];
        if !self.may_split(cx) {
            for (i, o) in out.iter_mut().enumerate() {
                *o = f(self, cx, i)?;
            }
        } else if self.range(cx, 0, n, c.saturating_mul(n as u64), 0, &mut out, &mut vec![0; n], &|_| c, f)? {
            return Err(Abort::Hole);
        }
        Ok(out)
    }

    /// `n` elements `f(0..n)` split at once into runs of about `n / (4 threads)` elements, whatever their
    /// estimated cost (`parallel_table`, `parallel_map`, `parallel: true`; decision sp-core.4). The runs are
    /// speculative tasks of the evaluator's ordinary splitting, so the result is the serial one: the frontier keeps
    /// a run's values in order, and evaluates again what a definition since the split may have changed or what
    /// could not run speculatively (an effect, a cell). Serial on one thread and inside a scratch scope.
    pub(crate) fn group_parallel<F>(&self, cx: &mut Cx<'h>, n: usize, f: &F) -> Res<Vec<T<'h>>>
    where
        F: Fn(&Self, &mut Cx<'h>, usize) -> Res<T<'h>> + Sync,
    {
        let mut out = vec![hole(); n];
        if cx.threads < 2 || cx.serial > 0 || cx.nest >= SPLIT_NEST {
            for (i, o) in out.iter_mut().enumerate() {
                *o = f(self, cx, i)?;
            }
            return Ok(out);
        }
        let run = n.div_ceil(4 * cx.threads as usize).max(1);
        if self.range(cx, 0, n, u64::MAX, run, &mut out, &mut vec![0; n], &|_| u64::MAX, f)? {
            return Err(Abort::Hole);
        }
        Ok(out)
    }

    /// Spike 4's range: evaluate elements `lo..hi` into `out`, splitting by the granularity rule; returns whether
    /// holes are left (only a speculative task leaves them). `fuels` receives each element's fuel, so the frontier
    /// can charge exactly the speculative work it keeps (decision F.3).
    #[allow(clippy::too_many_arguments)]
    fn range<E, F>(
        &self,
        cx: &mut Cx<'h>,
        lo: usize,
        hi: usize,
        hint: u64,
        force: usize,
        out: &mut [T<'h>],
        fuels: &mut [u64],
        est: &E,
        f: &F,
    ) -> Res<bool>
    where
        E: Fn(usize) -> u64 + Sync,
        F: Fn(&Self, &mut Cx<'h>, usize) -> Res<T<'h>> + Sync,
    {
        let cfg = &self.k.cfg;
        let fuel0 = cx.fuel;
        let mut holes = false;
        let mut next = match cfg.split {
            Split::Never => u64::MAX,
            Split::Lazy if cx.threads < 2 => u64::MAX,
            Split::Lazy if hint < cfg.grain => cx.next_poll,
            _ => cx.fuel,
        };
        for i in lo..hi {
            let split = if force > 0 {
                // a forced group splits off everything beyond the first run at once
                hi - i > force && i == lo
            } else {
                cx.fuel >= next && hi - i >= 2 && self.should_split(cx, lo, i, hi, fuel0, &mut next, est)
            };
            if split {
                let mid = i + (hi - i) / 2;
                let (left, right) = out[i - lo..].split_at_mut(mid - i);
                let (lf, rf) = fuels[i - lo..].split_at_mut(mid - i);
                let epoch0 = cx.epoch;
                let mut rc = self.speculative(cx);
                cx.splits += 1;
                // cells: the speculative part may read every cell allocated so far; a speculative left part runs
                // under a new id, so neither half writes the cells they share
                let task = cx.task;
                if cx.front {
                    cx.split_born = self.k.cells.born();
                } else {
                    cx.task = self.k.cells.task();
                }
                // the speculative part may run on a thread waiting inside a scratch scope: detached, its terms are
                // shared (decision G.2)
                let (rl, rr) = rayon::join(
                    || self.range(cx, i, mid, 0, force, left, lf, est, f),
                    || mc_core::detached(|| self.range(&mut rc, mid, hi, 0, force, right, rf, est, f)),
                );
                cx.task = task;
                cx.volatile += rc.volatile;
                cx.splits += rc.splits;
                cx.redos += rc.redos;
                cx.holes += rc.holes;
                cx.reads |= rc.reads;
                if cx.pool.len() < 8 {
                    cx.pool.append(&mut rc.pool);
                }
                let lh = rl?;
                // a speculative range fails only when every task must stop
                let rh = rr?;
                if !cx.front {
                    // a speculative parent keeps everything for now; the frontier above decides
                    cx.fuel += rc.fuel;
                    return Ok(holes | lh | rh);
                }
                debug_assert!(!lh, "the frontier leaves no holes");
                self.complete(cx, mid, right, rf, epoch0, rc.reads, f)?;
                return Ok(holes);
            }
            let f0 = cx.fuel;
            // a speculative cell never leaves its task (crate::cells): such a value is redone by the frontier
            let r = match f(self, cx, i) {
                Ok(v) if !cx.front && self.find_cell(v, |id| id < 0).is_some() => { Err(Abort::Hole) }
                r => r,
            };
            match r {
                Ok(v) => {
                    out[i - lo] = v;
                    fuels[i - lo] = cx.fuel - f0;
                }
                Err(Abort::Limit) => return Err(Abort::Limit),
                Err(e) if cx.front => return Err(e),
                Err(_) => {
                    // discarded: its fuel is not kept (only the frontier decides a limit, A.5)
                    out[i - lo] = hole();
                    fuels[i - lo] = 0;
                    cx.fuel = f0;
                    cx.holes += 1;
                    holes = true;
                }
            }
        }
        Ok(holes)
    }

    /// The frontier completes a speculative part in order: an element is kept, and its fuel charged, unless it is
    /// a hole or a definition since the split (including one made while completing this part) touched the part's
    /// read set; then it is evaluated again (the memo answers the unaffected parts).
    #[allow(clippy::too_many_arguments)]
    fn complete<F>(&self, cx: &mut Cx<'h>, lo: usize, out: &mut [T<'h>], fuels: &mut [u64], epoch0: u32, reads: u64, f: &F) -> Res<()>
    where
        F: Fn(&Self, &mut Cx<'h>, usize) -> Res<T<'h>> + Sync,
    {
        for (j, (o, fu)) in out.iter_mut().zip(fuels.iter_mut()).enumerate() {
            if o.raw() != HOLE && !self.k.changed(epoch0, cx.epoch, reads) {
                self.spend(cx, *fu)?;
                continue;
            }
            cx.redos += 1;
            let f0 = cx.fuel;
            *o = f(self, cx, lo + j)?;
            *fu = cx.fuel - f0;
        }
        Ok(())
    }

    /// Spike 4's lazy rule: split when the larger of the static estimate and the measured cost of the remaining
    /// elements reaches the grain, and this worker has no pending tasks of its own.
    #[allow(clippy::too_many_arguments)]
    fn should_split<E: Fn(usize) -> u64>(&self, cx: &mut Cx<'h>, lo: usize, i: usize, hi: usize, fuel0: u64, next: &mut u64, est: &E) -> bool {
        let cfg = &self.k.cfg;
        match cfg.split {
            Split::Never => false,
            Split::Always => true,
            Split::Lazy => {
                cx.next_poll = cx.fuel + cfg.poll;
                *next = cx.next_poll;
                let k = (hi - i).min(8);
                let (sum, pure) = (i..i + k).map(est).fold((0u64, 0u64), |(s, p), e| (s + e, p + (e > 0) as u64));
                let stat = sum * (hi - i) as u64 / k as u64;
                let done = (i - lo) as u64;
                let measured = (cx.fuel - fuel0).saturating_mul((hi - i) as u64).checked_div(done).unwrap_or(0);
                let measured = measured.saturating_mul(pure) / k as u64;
                measured.max(stat) >= cfg.grain && rayon_core::current_thread_has_pending_tasks() == Some(false)
            }
        }
    }
}

/// A closure parameter: `x`, with the wrappers `held`, `rest`, `typed(_, T)`, `optional(_, d)` (any order).
struct Param<'h> {
    name: Sym,
    held: bool,
    rest: bool,
    ty: Option<crate::pat::Ty>,
    default: Option<T<'h>>,
}

impl<'h> Param<'h> {
    fn of(h: &'h Heap, sc: &Scopes, mut p: T<'h>) -> Option<Param<'h>> {
        let mut info = Param { name: sc.fn_, held: false, rest: false, ty: None, default: None };
        // wrappers nest a few levels at most
        for _ in 0..8 {
            match h.view(p) {
                View::Sym(s) => {
                    info.name = s;
                    return Some(info);
                }
                View::App { head, args } => {
                    let w = sym_of(h, head)?;
                    match args {
                        [_] if w == sc.held => info.held = true,
                        [_] if w == sc.rest => info.rest = true,
                        [_] if w == sc.any => {}
                        // a type that is not one of the atomic types is not checked
                        [_, ty] if w == sc.typed => info.ty = crate::syntax::type_of(h, *ty).ok(),
                        [_, d] if w == sc.optional => info.default = Some(*d),
                        _ => return None,
                    }
                    p = args[0];
                }
                _ => return None,
            }
        }
        None
    }
}

/// What a condition sees: the shared heap, its declared variables, own values and the evaluator, with every read
/// entering the task's read set (I2.3). Effects are refused (A.6).
pub struct CondCx<'a, 'k, 'h> {
    pub(crate) ev: &'a Eval<'k, 'h>,
    pub(crate) cx: &'a mut Cx<'h>,
    pub(crate) binds: &'a Binds<'h>,
    pub(crate) vars: &'a [u16],
}

impl<'h> CondCx<'_, '_, 'h> {
    /// The shared heap: building terms here is safe while other tasks match and evaluate.
    #[inline]
    pub fn heap(&self) -> &'h Heap {
        self.ev.heap
    }
    /// The term bound to the `i`-th declared variable.
    #[inline]
    pub fn one(&self, i: usize) -> T<'h> {
        self.binds.one(self.vars[i])
    }
    /// The terms bound to the `i`-th declared variable, a sequence variable.
    #[inline]
    pub fn seq(&self, i: usize) -> &[T<'h>] {
        self.binds.seq(self.vars[i])
    }
    /// The binding of the `i`-th declared variable, of either kind (a variable that is a sequence when present
    /// and its default when absent, as `optional(xs__, d)` under an associative-commutative head, has either).
    #[inline]
    pub fn binding(&self, i: usize) -> crate::matcher::Val<'h> {
        self.binds.get(self.vars[i])
    }
    /// For the `i`-th declared variable bound to a run of arguments of an associative-commutative call: that
    /// call's head (the run stands for the head applied to it, semantics 6.2).
    #[inline]
    pub fn seq_head(&self, i: usize) -> Option<T<'h>> {
        self.binds.seq_head(self.vars[i])
    }
    /// The terms of the run starting at `o`, `n` long, of a sequence binding [`crate::matcher::Val::Seq`].
    #[inline]
    pub fn run(&self, o: u32, n: u32) -> &[T<'h>] {
        self.binds.run(o, n)
    }
    /// The value of a machine-integer term.
    #[inline]
    pub fn int(&self, t: T<'h>) -> Option<i64> {
        match self.ev.heap.view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }
    /// The own value of `s`; the read is recorded.
    pub fn value(&mut self, s: Sym) -> Option<T<'h>> {
        self.cx.reads |= bit(s);
        self.ev.k.defs.own(self.ev.heap, s)
    }
    /// Evaluate `t` in the task's context: its reads (and the read sets of its memo hits) are recorded. An effect
    /// reached inside gives [`Abort::EffectInCondition`].
    pub fn eval(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.ev.eval(self.cx, t)
    }
    /// Is `t` the symbol `true`?
    pub fn is_true(&self, t: T<'h>) -> bool {
        t == self.ev.sym(self.ev.k.names.true_)
    }

    /// A condition given as a term: the declared variables substituted for their symbols, evaluated, compared
    /// with `true`.
    pub(crate) fn term_condition(&mut self, body: Raw, names: &[Sym]) -> Res<bool> {
        let h = self.ev.heap;
        let nm = &self.ev.k.names;
        let pairs: Vec<(Sym, T<'h>)> =
            names.iter().zip(self.vars.iter()).map(|(&s, &v)| (s, crate::subst::value(h, self.binds, v, nm.list))).collect();
        let t = crate::subst::substitute(h, &nm.scopes, body.get(h), &pairs);
        let r = self.eval(t)?;
        Ok(self.is_true(r))
    }
}
