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
use crate::conc::SymTable;
use crate::ctl::{Control, Status, FUEL_BLOCK};
use crate::defs::{Attributes, Defs, Hold, SymDefs};
use crate::index::{Rhs, Rule, RuleRhs, Scratch};
use crate::matcher::{Binds, Matcher};
use crate::native::{Native, NativeCx, Pattern, Reply};
use crate::order::{StructuralOrder, TermOrder};
use crate::pat::{compile, Canon, DefError, Pat};
use mc_core::{Heap, Raw, Roots, Store, Sym, Trace, View, T};
use parking_lot::Mutex;
use std::borrow::Cow;
use std::sync::atomic::{AtomicU32, Ordering::*};
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
    /// deepest nesting of evaluations before `failure(limit, depth)` (decision F.11)
    pub max_depth: u32,
}

impl Default for EvalConfig {
    fn default() -> EvalConfig {
        EvalConfig { split: Split::Lazy, grain: 1 << 10, poll: 1 << 9, memo: true, fuel: i64::MAX, max_depth: 20_000 }
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
        }
    }
}

/// The read-set bit of a symbol (spike 2's decision 2.7; the same bit as in [`Heap::mask`]).
#[inline]
pub(crate) fn bit(s: Sym) -> u64 {
    1u64 << (s.0 % 64)
}

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
    natives: Vec<Box<dyn Native>>,
    /// learned fuel per evaluation, by head symbol (granularity)
    cost: SymTable<AtomicU32>,
    order: Box<dyn TermOrder>,
    ctl: Arc<Control>,
    /// the definition epoch: bumped by every definition
    epoch: AtomicU32,
    /// the last epoch at which a symbol with each read-set bit was defined
    last_change: [AtomicU32; 64],
    out: Mutex<Vec<Raw>>,
    pub(crate) names: Names,
    cfg: EvalConfig,
    /// Terms the session keeps across evaluations (history, inputs); traced with the kernel.
    pub roots: Roots,
}

impl Trace for Kernel {
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.defs.trace(visit);
        for n in self.natives.iter_mut() {
            n.trace(visit);
        }
        self.out.get_mut().iter_mut().for_each(&mut *visit);
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
            cost: SymTable::default(),
            order: Box::new(StructuralOrder::default()),
            ctl: Arc::new(Control::default()),
            epoch: AtomicU32::new(1),
            last_change: std::array::from_fn(|_| AtomicU32::new(0)),
            out: Mutex::new(vec![]),
            names: Names::new(h),
            cfg,
            roots: Roots::new(),
        };
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
    fn defined<R>(&self, from: u32, bits: u64, define: impl FnOnce() -> R) -> (R, u32) {
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

    /// The attributes of a symbol.
    pub fn attributes(&self, s: Sym) -> Attributes {
        self.defs.attrs(s)
    }

    /// Give symbol `s` a native implementation (and its attributes). See [`Native`].
    pub fn register(&mut self, s: Sym, a: Attributes, native: Box<dyn Native>) {
        let i = self.natives.len() as u32;
        self.natives.push(native);
        self.set_attributes(s, a);
        let e = *self.epoch.get_mut();
        self.defined(e, bit(s), || self.defs.set_native(s, i));
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
        let rhs = RuleRhs::Term { body: Raw::new(body), names: names.into(), vars: vars.into() };
        self.define_rule(h, e, lhs, rhs, None).map(|_| ())
    }

    /// Compile a pattern for a native's own matching ([`NativeCx::matches`]).
    pub fn compile_pattern(&self, h: &Heap, p: Pat) -> Result<Pattern, DefError> {
        let p = compile(h, p, &CanonOf(self))?;
        Ok(Pattern::new(p))
    }

    /// Compile and insert a rule at epoch `from + 1`; returns the new epoch. Called by the session or the frontier.
    fn define_rule(&self, h: &Heap, from: u32, lhs: Pat, rhs: RuleRhs, attach: Option<Sym>) -> Result<u32, DefError> {
        let lhs = compile(h, lhs, &CanonOf(self))?;
        let (top_head, top_args): (T, Vec<T>) = match lhs.strip() {
            Pat::App { head, args, .. } => (head.get(h), args.iter().filter_map(|a| a.fixed_head(h)).collect()),
            Pat::Lit(t) => match h.view(t.get(h)) {
                View::App { head, args } => (head, args.iter().filter_map(|&a| head_of(h, a)).collect()),
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
        let owner = attach.unwrap_or(top);
        let ((), e) = self.defined(from, bit(owner), || {
            self.defs.define(h, owner, attach.map(|_| top), ac_top, Rule::new(lhs, rhs));
        });
        Ok(e)
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
        self.defs.get(s).and_then(SymDefs::rules).map_or(0, |r| r.len())
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
    pub fn eval_with<'h>(&mut self, h: &'h Heap, t: T<'h>, fuel: i64) -> Evaluation<'h> {
        let k: &Kernel = self;
        k.ctl.begin(fuel);
        let ev = Eval { heap: h, k };
        let mut cx = Cx::frontier(k.epoch.load(Acquire), rayon::current_num_threads() as u32);
        let r = ev.eval(&mut cx, t);
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
        };
        Evaluation { value, status, stats: EvalStats { splits: cx.splits, redos: cx.redos, fuel: cx.fuel, holes: cx.holes } }
    }

    /// Did a definition after epoch `from` (up to `to`) touch read set `deps`? Conservative: a definition after `to`
    /// also counts.
    #[inline]
    fn changed(&self, from: u32, to: u32, deps: u64) -> bool {
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
    fn learn(&self, s: Sym, fuel: u64) {
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
    match h.view(t) {
        View::App { head, .. } => Some(head),
        _ => None,
    }
}

/// Compilation's view of the kernel.
struct CanonOf<'k>(&'k Kernel);

impl Canon for CanonOf<'_> {
    fn is_ac<'h>(&self, h: &'h Heap, head: T<'h>) -> bool {
        sym_of(h, head).is_some_and(|s| self.0.defs.attrs(s).ac)
    }
    fn make<'h>(&self, h: &'h Heap, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        make(self.0, h, head, args)
    }
}

/// The symbol of a symbol term.
#[inline]
fn sym_of<'h>(h: &'h Heap, t: T<'h>) -> Option<Sym> {
    match h.view(t) {
        View::Sym(s) => Some(s),
        _ => None,
    }
}

/// A compound term in canonical form: under an associative and commutative head, arguments with the same head are
/// spliced in (one level: they are canonical already) and the arguments sorted by the canonical order.
fn make<'h>(k: &Kernel, h: &'h Heap, head: T<'h>, args: &[T<'h>]) -> T<'h> {
    if !sym_of(h, head).is_some_and(|s| k.defs.attrs(s).ac) {
        return h.app(head, args);
    }
    let mut v = Vec::with_capacity(args.len());
    for &a in args {
        match h.view(a) {
            View::App { head: g, args: inner } if g == head => v.extend_from_slice(inner),
            _ => v.push(a),
        }
    }
    k.order.sort(h, &mut v);
    h.app(head, &v)
}

/// Per-task evaluation state. The frontier's `Cx` is passed down by `&mut` along the leftmost path.
pub(crate) struct Cx<'h> {
    /// May perform effects (the task on the sequential frontier).
    front: bool,
    /// The definition epoch this task started from (stamps and validation use it).
    epoch: u32,
    /// Read set of the evaluation in progress (a 64-bit mask, decision 2.7).
    pub(crate) reads: u64,
    effects: u32,
    /// Fuel of the work this task keeps: on the frontier, what was charged to the budget.
    fuel: u64,
    /// Fuel in hand (drawn from the budget on the frontier, from the share otherwise).
    avail: u64,
    /// A speculative task's remaining share (decision F.3).
    share: u64,
    /// How many conditions this task is inside (effects are refused when nonzero, A.6).
    pub(crate) in_cond: u32,
    depth: u32,
    threads: u32,
    splits: u64,
    redos: u64,
    holes: u64,
    next_poll: u64,
    /// Dispatch scratch, one per active dispatch frame.
    #[allow(clippy::vec_box)] // boxed: popping and pushing a frame moves a pointer, not the buffers
    pool: Vec<Box<Scratch<'h>>>,
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
            depth: 0,
            threads,
            splits: 0,
            redos: 0,
            holes: 0,
            next_poll: 0,
            pool: vec![],
        }
    }
}

/// One evaluation's view: the heap and the kernel.
pub(crate) struct Eval<'k, 'h> {
    pub(crate) heap: &'h Heap,
    pub(crate) k: &'k Kernel,
}

enum Step<'h> {
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
        if cx.front {
            let got = ctl.draw(FUEL_BLOCK);
            if got == 0 {
                ctl.fail(Status::Fuel);
                return Err(Abort::Limit);
            }
            cx.avail = got;
        } else {
            if cx.share == 0 {
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
        Cx { front: false, reads: 0, effects: 0, fuel: 0, avail: 0, share, splits: 0, redos: 0, holes: 0, next_poll: 0, pool: vec![], ..*cx }
    }

    /// Effects are allowed on the frontier, outside conditions (semantics 18.2, A.6).
    pub(crate) fn effect(&self, cx: &mut Cx<'h>) -> Res<()> {
        if cx.in_cond > 0 {
            return Err(Abort::EffectInCondition);
        }
        if !cx.front {
            return Err(Abort::Hole);
        }
        cx.effects += 1;
        Ok(())
    }

    // --- effects ---------------------------------------------------------------------------------------------

    /// Append to the output (an effect).
    pub(crate) fn emit(&self, cx: &mut Cx<'h>, t: T<'h>) -> Res<()> {
        self.effect(cx)?;
        self.k.out.lock().push(Raw::new(t));
        Ok(())
    }

    /// `s = v` during an evaluation (an effect of the frontier).
    pub(crate) fn set_own(&self, cx: &mut Cx<'h>, s: Sym, v: T<'h>) -> Res<()> {
        self.effect(cx)?;
        debug_assert_eq!(cx.epoch, self.k.epoch.load(Relaxed), "only the frontier defines");
        let ((), e) = self.k.defined(cx.epoch, bit(s), || self.k.defs.set_own(s, v));
        cx.epoch = e;
        Ok(())
    }

    /// Attributes changed during an evaluation (an effect of the frontier).
    pub(crate) fn set_attributes(&self, cx: &mut Cx<'h>, s: Sym, a: Attributes) -> Res<()> {
        self.effect(cx)?;
        let ((), e) = self.k.defined(cx.epoch, bit(s), || self.k.set_attrs(s, a));
        cx.epoch = e;
        Ok(())
    }

    /// A rule definition during an evaluation (an effect of the frontier): inserted into the live index while
    /// speculative tasks may be reading it.
    pub(crate) fn define_rule(&self, cx: &mut Cx<'h>, lhs: Pat, rhs: RuleRhs, attach: Option<Sym>) -> Res<Result<(), DefError>> {
        self.effect(cx)?;
        debug_assert_eq!(cx.epoch, self.k.epoch.load(Relaxed), "only the frontier defines");
        Ok(self.k.define_rule(self.heap, cx.epoch, lhs, rhs, attach).map(|e| cx.epoch = e))
    }

    // --- evaluation ------------------------------------------------------------------------------------------

    /// Evaluate `t` to normal form.
    pub(crate) fn eval(&self, cx: &mut Cx<'h>, t: T<'h>) -> Res<T<'h>> {
        let h = self.heap;
        match h.view(t) {
            View::App { .. } => {}
            View::Sym(s) => return self.eval_sym(cx, s, t),
            _ => return Ok(t),
        }
        self.tick(cx)?;
        let memo = self.k.cfg.memo;
        if memo {
            if let Some(hit) = h.memo_get(t) {
                // the epoch fast path, then the read-set check (semantics 4.3); a hit passes its read set on (2.7)
                if hit.epoch == cx.epoch || (hit.epoch < cx.epoch && !self.k.changed(hit.epoch, cx.epoch, hit.deps)) {
                    cx.reads |= hit.deps;
                    return Ok(hit.value);
                }
            }
        }
        if cx.depth >= self.k.cfg.max_depth {
            if cx.front {
                self.k.ctl.fail(Status::Depth);
                return Err(Abort::Limit);
            }
            return Err(Abort::Hole);
        }
        let (reads0, effects0, epoch0, fuel0) = (cx.reads, cx.effects, cx.epoch, cx.fuel);
        cx.reads = 0;
        cx.depth += 1;
        let r = self.eval_inner(cx, t);
        cx.depth -= 1;
        let mut deps = cx.reads;
        if let Ok(v) = r {
            // a normal form stays normal while none of its symbols (or the term's) gains a definition
            deps |= h.mask(t) | h.mask(v);
        }
        cx.reads = reads0 | deps;
        let v = r?;
        if let View::App { head, .. } = h.view(t) {
            if let Some(s) = sym_of(h, head) {
                self.k.learn(s, cx.fuel - fuel0);
            }
        }
        // results that involve effects are never memoised (semantics 9); stamped with the start epoch (4.8)
        if memo && cx.effects == effects0 {
            h.memo_put(t, v, epoch0, deps);
            if v != t {
                h.memo_put(v, v, epoch0, deps);
            }
        }
        Ok(v)
    }

    /// A symbol: its own value, evaluated, or itself. The read is recorded either way.
    fn eval_sym(&self, cx: &mut Cx<'h>, s: Sym, t: T<'h>) -> Res<T<'h>> {
        cx.reads |= bit(s);
        match self.k.defs.own(self.heap, s) {
            Some(v) if v != t => {
                self.tick(cx)?;
                self.eval(cx, v)
            }
            _ => Ok(t),
        }
    }

    fn eval_inner(&self, cx: &mut Cx<'h>, mut t: T<'h>) -> Res<T<'h>> {
        let h = self.heap;
        loop {
            let (head, args) = match h.view(t) {
                View::App { head, args } => (head, args),
                View::Sym(s) => return self.eval_sym(cx, s, t),
                _ => return Ok(t),
            };
            let hs = sym_of(h, head);
            let sd = hs.and_then(|s| self.k.defs.get(s));
            // 4.1 step 3.1: the head
            let hv = match hs {
                Some(s) => {
                    cx.reads |= bit(s);
                    match sd.and_then(|d| d.own(h)).filter(|&v| v != head) {
                        Some(v) => self.eval(cx, v)?,
                        None => head,
                    }
                }
                None => self.eval(cx, head)?,
            };
            if hv != head {
                self.tick(cx)?;
                t = h.app(hv, args);
                continue;
            }
            let attrs = sd.map_or_else(Attributes::default, SymDefs::attrs);
            // 3.2: the arguments, by the head's argument modes
            let vals = self.eval_args(cx, args, attrs)?;
            if !attrs.handles_failures {
                if let Some(f) = self.first_failure(&vals) {
                    return Ok(f);
                }
            }
            // 3.3: canonicalise
            if attrs.ac {
                t = self.make(head, &vals);
            } else if *vals != *args {
                t = h.app(head, &vals);
            }
            // 3.4: a native implementation
            if let Some(n) = sd.map(|d| d.native.load(Acquire)).filter(|&n| n != 0) {
                match self.call_native(cx, n - 1, t) {
                    Ok(Some(Step::Done(v))) => return Ok(v),
                    Ok(Some(Step::Again(u))) => {
                        self.tick(cx)?;
                        t = u;
                        continue;
                    }
                    Ok(None) => {}
                    Err(Abort::EffectInCondition) if cx.in_cond == 0 => return Ok(self.effect_failure()),
                    Err(e) => return Err(e),
                }
            }
            // 3.5: rules
            match self.dispatch(cx, t) {
                Ok(Some(u)) => {
                    self.tick(cx)?;
                    t = u;
                }
                Ok(None) => return Ok(t),
                Err(Abort::EffectInCondition) if cx.in_cond == 0 => return Ok(self.effect_failure()),
                Err(e) => return Err(e),
            }
        }
    }

    fn effect_failure(&self) -> T<'h> {
        let h = self.heap;
        h.app(self.sym(self.k.names.failure), &[self.sym(self.k.names.effect_in_condition)])
    }

    /// The first argument that is a failure (semantics 10).
    #[inline]
    fn first_failure(&self, vals: &[T<'h>]) -> Option<T<'h>> {
        let h = self.heap;
        let fb = bit(self.k.names.failure);
        let failure = self.sym(self.k.names.failure);
        vals.iter().copied().find(|&a| h.mask(a) & fb != 0 && matches!(h.view(a), View::App { head, .. } if head == failure))
    }

    fn call_native(&self, cx: &mut Cx<'h>, n: u32, t: T<'h>) -> Res<Option<Step<'h>>> {
        let native = &self.k.natives[n as usize];
        let View::App { args, .. } = self.heap.view(t) else { return Ok(None) };
        let mut ncx = NativeCx { ev: self, cx };
        Ok(match native.call(&mut ncx, t, args)? {
            Reply::Done(v) => Some(Step::Done(v)),
            Reply::Again(u) => Some(Step::Again(u)),
            Reply::Decline => None,
        })
    }

    /// The replacement from the first matching rule for `t`: argument-head rules of the arguments' heads in argument
    /// order, then the head's rules (semantics 4.1 step 3.5, 6.5). Consulting a symbol's rules is a read, even when
    /// it holds none (spike 2). Conditions run inside, on this task's context.
    pub(crate) fn dispatch(&self, cx: &mut Cx<'h>, t: T<'h>) -> Res<Option<T<'h>>> {
        let View::App { head, args } = self.heap.view(t) else { return Ok(None) };
        let Some(hs) = sym_of(self.heap, head) else { return Ok(None) };
        let mut sc = cx.pool.pop().unwrap_or_default();
        let r = self.dispatch_in(cx, t, hs, args, &mut sc);
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
                let Some(s) = head_of(h, a).and_then(|g| sym_of(h, g)) else { continue };
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
        Ok(found.map(|r| r.rhs.build(&sc.out, h, self.k.names.list)))
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

    // --- groups ----------------------------------------------------------------------------------------------

    #[inline]
    fn may_split(&self, cx: &Cx<'h>) -> bool {
        match self.k.cfg.split {
            Split::Never => false,
            Split::Lazy => cx.threads >= 2,
            Split::Always => true,
        }
    }

    /// Whether evaluating `a` can change it (a compound term, or a symbol with an own value).
    #[inline]
    fn needs_eval(&self, cx: &mut Cx<'h>, a: T<'h>) -> bool {
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
        match h.view(a) {
            View::App { head, args } => match sym_of(h, head) {
                Some(s) if self.k.defs.attrs(s).effect => 0,
                s => (args.len() as u64 + 1).max(s.map_or(0, |s| self.k.head_cost(s))),
            },
            _ => 1,
        }
    }

    fn eval_args<'a>(&self, cx: &mut Cx<'h>, args: &'a [T<'h>], attrs: Attributes) -> Res<Cow<'a, [T<'h>]>> {
        let hold = attrs.hold;
        if hold == Hold::All || !(0..args.len()).any(|i| !hold.holds(i) && self.needs_eval(cx, args[i])) {
            return Ok(Cow::Borrowed(args));
        }
        let mut out = args.to_vec();
        if args.len() >= 2 && !attrs.effect && self.may_split(cx) {
            let est = |i: usize| if hold.holds(i) { 0 } else { self.estimate(args[i]) };
            let k = args.len().min(8);
            let hint = (0..k).map(est).sum::<u64>() * args.len() as u64 / k as u64;
            let f = |ev: &Self, cx: &mut Cx<'h>, i: usize| if hold.holds(i) { Ok(args[i]) } else { ev.eval(cx, args[i]) };
            if self.range(cx, 0, args.len(), hint, &mut out, &mut vec![0; args.len()], &est, &f)? {
                return Err(Abort::Hole);
            }
        } else {
            for (i, o) in out.iter_mut().enumerate() {
                if !hold.holds(i) {
                    *o = self.eval(cx, *o)?;
                }
            }
        }
        Ok(Cow::Owned(out))
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
        } else if self.range(cx, 0, n, c.saturating_mul(n as u64), &mut out, &mut vec![0; n], &|_| c, f)? {
            return Err(Abort::Hole);
        }
        Ok(out)
    }

    /// Spike 4's range: evaluate elements `lo..hi` into `out`, splitting by the granularity rule; returns whether
    /// holes are left (only a speculative task leaves them). `fuels` receives each element's fuel, so the frontier
    /// can charge exactly the speculative work it keeps (decision F.3).
    #[allow(clippy::too_many_arguments)]
    fn range<E, F>(&self, cx: &mut Cx<'h>, lo: usize, hi: usize, hint: u64, out: &mut [T<'h>], fuels: &mut [u64], est: &E, f: &F) -> Res<bool>
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
            if cx.fuel >= next && hi - i >= 2 && self.should_split(cx, lo, i, hi, fuel0, &mut next, est) {
                let mid = i + (hi - i) / 2;
                let (left, right) = out[i - lo..].split_at_mut(mid - i);
                let (lf, rf) = fuels[i - lo..].split_at_mut(mid - i);
                let epoch0 = cx.epoch;
                let mut rc = self.speculative(cx);
                cx.splits += 1;
                let (rl, rr) = rayon::join(
                    || self.range(cx, i, mid, 0, left, lf, est, f),
                    || self.range(&mut rc, mid, hi, 0, right, rf, est, f),
                );
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
            match f(self, cx, i) {
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
        let list = self.ev.k.names.list;
        let pairs: Vec<(Sym, T<'h>)> =
            names.iter().zip(self.vars.iter()).map(|(&s, &v)| (s, crate::subst::value(h, self.binds, v, list))).collect();
        let t = crate::subst::substitute(h, body.get(h), &pairs);
        let r = self.eval(t)?;
        Ok(self.is_true(r))
    }
}
