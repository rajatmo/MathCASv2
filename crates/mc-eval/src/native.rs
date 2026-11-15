//! The native-function interface (semantics 7.1): how built-ins, and later `mc-arith`'s arithmetic, plug into the
//! evaluator.
//!
//! A native is registered for a symbol with its [`Attributes`](crate::Attributes) ([`Kernel::register`](crate::Kernel::register)).
//! When a call `f(args...)` with that head is evaluated, the evaluator first evaluates the arguments by the
//! attributes' argument modes, returns a failure argument unless the native handles failures, canonicalises an AC
//! head, and then calls [`Native::call`] with the term and its arguments. The native returns:
//!
//! - [`Reply::Done`] with a normal form (it is not evaluated again);
//! - [`Reply::Again`] with a replacement, which the evaluator evaluates (a loop iteration that costs fuel);
//! - [`Reply::Decline`]: the rules of the symbol are tried next (4.1 step 3.5).
//!
//! Through [`NativeCx`] a native can charge fuel for its loops (semantics 7.1: every input-dependent loop must; the
//! charge also polls the interrupt), evaluate terms with its reads recorded, perform effects (refused speculatively
//! and inside conditions, then retried in order by the frontier), match compiled patterns, rewrite once with the
//! rules, and evaluate a group of elements with speculative parallelism (`map`). Natives are shared by every task
//! (`Sync`); one that keeps terms must hold them as [`Raw`] and visit them in [`Native::trace`].
//!
//! `mc-arith` plugs in by registering natives for `add`, `mul`, `pow` and the comparisons (strict, `add` and `mul`
//! AC), and by replacing the canonical order with [`Kernel::set_order`](crate::Kernel::set_order).
use crate::eval::{Cx, Eval, Res};
use crate::index::{Rhs, RuleRhs};
use crate::pat::{DefError, Pat};
use mc_core::{Heap, Raw, Store, Sym, View, T};

/// What a native call produced.
#[derive(Clone, Copy, Debug)]
pub enum Reply<'h> {
    /// the normal form
    Done(T<'h>),
    /// a replacement to evaluate
    Again(T<'h>),
    /// not applicable: try the rules
    Decline,
}

/// A native implementation of a symbol.
pub trait Native: Send + Sync {
    /// Evaluate the call `t` (with arguments `args`, already evaluated by the argument modes).
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>>;

    /// Visit every term handle the native keeps (a safe point).
    fn trace(&mut self, _visit: &mut dyn FnMut(&mut Raw)) {}

    /// Whether [`Native::call`] reads its term `t` (returns it, matches or rewrites it, takes its head from it).
    /// A native that reads only `args` returns `false`: the evaluator then calls it before building the call with
    /// the evaluated arguments, and builds that term only if the native declines (decision 1a.6), so arithmetic
    /// and comparisons in loops make no garbage calls. Its `t` is then the call as it was before its arguments
    /// were evaluated. Read once, at registration.
    fn reads_term(&self) -> bool {
        true
    }

    /// Whether the native is also called for *curried* calls `f(a..)(b..)`, `f(a..)(b..)(c..)`, ...: calls whose
    /// head is a compound term with this symbol as its innermost head. `t` is then the whole call, `args` its own
    /// (outermost) arguments; declining leaves the call as it was. Off by default: a native sees only calls with
    /// its symbol as the head. Read once, at registration.
    fn curried(&self) -> bool {
        false
    }

    /// The value of the call on the one argument `x`, a packed array (semantics 11), computed on its payload; `None`
    /// to take the ordinary path. A listable function is asked before its call is threaded over the elements, so a
    /// function that knows packed arrays is not called once per element (decision sp-core.3). The value must be
    /// what threading gives.
    fn packed<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>) -> Res<Option<T<'h>>> {
        let _ = (cx, x);
        Ok(None)
    }
}

/// A compiled pattern a native matches with ([`NativeCx::matches`]); made by
/// [`Kernel::compile_pattern`](crate::Kernel::compile_pattern). It holds terms: a native keeping one traces it.
#[derive(Clone)]
pub struct Pattern {
    pub(crate) pat: Pat,
    pub(crate) nvars: usize,
}

impl Pattern {
    pub(crate) fn new(pat: Pat) -> Pattern {
        let nvars = pat.n_vars();
        Pattern { pat, nvars }
    }
    /// Visit the pattern's handles.
    pub fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.pat.trace(visit);
    }
}

/// The loop signals an iterating function reads in the values of its function or body ([`NativeCx::loop_signal`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoopSignal {
    /// `failure(break)`: the iteration ends; the result holds what was produced before it
    Break,
    /// `failure(continue)`: this element is skipped
    Continue,
}

/// Whether `v` is `failure(tag)` for the symbol `tag`.
fn is_signal(h: &Heap, failure: Sym, tag: Sym, v: T<'_>) -> bool {
    if h.packed(v).is_some() {
        return false;
    }
    match h.view(v) {
        View::App { head, args: &[k] } => head == h.sym_term(failure) && k == h.sym_term(tag),
        _ => false,
    }
}

/// The function computing element `i` of a group ([`NativeCx::group`]).
pub type ElemFn<'f, 'h> = dyn Fn(&mut NativeCx<'_, '_, 'h>, usize) -> Res<T<'h>> + Sync + 'f;

/// A native's access to the evaluation task that called it.
pub struct NativeCx<'a, 'k, 'h> {
    pub(crate) ev: &'a Eval<'k, 'h>,
    pub(crate) cx: &'a mut Cx<'h>,
}

impl<'h> NativeCx<'_, '_, 'h> {
    /// The shared heap.
    #[inline]
    pub fn heap(&self) -> &'h Heap {
        self.ev.heap
    }
    /// `true` or `false`.
    pub fn bool(&self, b: bool) -> T<'h> {
        let n = &self.ev.k.names;
        self.ev.sym(if b { n.true_ } else { n.false_ })
    }
    /// `null`
    pub fn null(&self) -> T<'h> {
        self.ev.sym(self.ev.k.names.null)
    }
    /// `list(items...)`
    pub fn list(&self, items: &[T<'h>]) -> T<'h> {
        self.ev.heap.app(self.ev.sym(self.ev.k.names.list), items)
    }
    /// Is `t` a list? Its elements.
    pub fn as_list(&self, t: T<'h>) -> Option<&'h [T<'h>]> {
        match self.ev.heap.view(t) {
            View::App { head, args } if head == self.ev.sym(self.ev.k.names.list) => Some(args),
            _ => None,
        }
    }
    /// Is `t` the symbol `true`?
    pub fn is_true(&self, t: T<'h>) -> bool {
        t == self.ev.sym(self.ev.k.names.true_)
    }
    /// Is `t` the symbol `false`?
    pub fn is_false(&self, t: T<'h>) -> bool {
        t == self.ev.sym(self.ev.k.names.false_)
    }
    /// `failure(tags...)`
    pub fn failure(&self, tags: &[&str]) -> T<'h> {
        self.ev.failure(tags)
    }
    /// The compound term `head(args)` in canonical form (AC heads flattened and sorted).
    pub fn make(&self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        self.ev.make(head, args)
    }
    /// Charge `n` units of fuel (and poll the interrupt). Every input-dependent loop of a native must. The work the
    /// kernels reported on this thread since the last charge ([`mc_api::work`]) is charged with it.
    pub fn tick(&mut self, n: u64) -> Res<()> {
        let w = self.ev.work_units(self.cx);
        self.ev.spend(self.cx, n + w)
    }
    /// The fuel this task has spent so far, the kernels' work not yet charged included: the clock of a method's
    /// budget (semantics 18.7). It counts work, never time, so the same calls give the same reading on every
    /// machine; only differences of two readings in one native call mean anything.
    pub fn fuel_spent(&self) -> u64 {
        self.cx.fuel + (self.cx.ops + mc_api::work::pending_ops()) / mc_api::work::OPS_PER_TICK
    }
    /// Evaluate `t` on this task: its reads enter the caller's read set.
    pub fn eval(&mut self, t: T<'h>) -> Res<T<'h>> {
        // an interrupt (or a limit) already requested: a native that swallowed the abort of an earlier step and goes on
        // to its next method stops here, instead of running on with the number kernels returning arbitrary values
        if self.ev.k.ctl.halted() {
            return Err(crate::eval::Abort::Limit);
        }
        self.ev.eval(self.cx, t)
    }
    /// Announce an effect: refused (an abort the evaluator handles) on a speculative task or inside a condition.
    /// Call it before performing any effect not offered below.
    pub fn effect(&mut self) -> Res<()> {
        self.ev.effect(self.cx)
    }
    /// Append `t` to the session output (an effect).
    pub fn emit(&mut self, t: T<'h>) -> Res<()> {
        self.ev.emit(self.cx, t)
    }
    /// `s = v` (an effect).
    pub fn set_own(&mut self, s: Sym, v: T<'h>) -> Res<()> {
        self.ev.set_own(self.cx, s, v)
    }
    /// `lhs := rhs`, attached to `attach` if given (an effect).
    pub fn define(&mut self, lhs: Pat, rhs: Rhs, attach: Option<Sym>) -> Res<Result<(), DefError>> {
        self.ev.define_rule(self.cx, lhs, RuleRhs::Native(rhs), attach)
    }
    /// `lhs := body` with a term right side in which symbol `names[i]` stands for pattern variable `vars[i]` (an
    /// effect).
    pub fn define_term(&mut self, lhs: Pat, body: T<'h>, names: &[Sym], vars: &[u16]) -> Res<Result<(), DefError>> {
        let rhs = RuleRhs::Term { body: Raw::new(body), names: names.into(), vars: vars.into(), src: None };
        self.ev.define_rule(self.cx, lhs, rhs, None)
    }
    /// Change the attributes of `s` (an effect).
    pub fn set_attributes(&mut self, s: Sym, a: crate::Attributes) -> Res<()> {
        self.ev.set_attributes(self.cx, s, a)
    }
    /// `unset(s)`: remove the own value, identity element and rules of `s` (an effect; natives and attributes
    /// stay). Whether there was anything to remove.
    pub fn unset(&mut self, s: Sym) -> Res<bool> {
        self.ev.unset(self.cx, s)
    }

    /// `unset(f(...))`: remove the rule whose left side is `lhs` up to renaming (an effect). Whether there was one.
    pub fn unset_rule(&mut self, lhs: Pat) -> Res<Result<bool, DefError>> {
        self.ev.unset_rule(self.cx, lhs)
    }

    /// The symbols with a definition of their own (an own value, rules, attributes or an identity element) and no
    /// native implementation, in symbol order: the user's symbols, for name patterns such as `protect("x*")`.
    pub fn user_symbols(&self) -> Vec<Sym> {
        let h = self.ev.heap;
        (0..h.sym_count() as u32).map(Sym).filter(|&s| self.ev.k.is_defined(s) && !self.ev.k.has_native(s)).collect()
    }

    /// Whether `s` has any definition: a native, an own value, attributes, an identity element or rules.
    pub fn is_defined(&mut self, s: Sym) -> bool {
        self.cx.reads |= crate::eval::bit(s);
        self.ev.k.is_defined(s)
    }

    /// Is `s` protected (its definitions may not change, decision r17fix.4)?
    pub fn protected(&self, s: Sym) -> bool {
        self.attributes(s).protected
    }

    /// The kernel's attributes of `s`.
    pub fn attributes(&self, s: Sym) -> crate::Attributes {
        self.ev.k.defs.attrs(s)
    }
    /// Whether `s` is listable or maps over lists itself (`Kernel::set_maps_lists`, decision io-core.1).
    pub fn maps_lists(&self, s: Sym) -> bool {
        self.ev.k.maps_lists(s)
    }
    /// Whether `t` is a call of a number-like value head with its own arithmetic (`Kernel::set_value_head`,
    /// decision io-core.5).
    pub fn is_value(&self, t: T<'h>) -> bool {
        let k = &self.ev.k;
        if k.value_heads.is_empty() {
            return false;
        }
        let h = self.ev.heap;
        match h.view(t) {
            mc_core::View::App { head, .. } => {
                matches!(h.view(head), mc_core::View::Sym(s) if k.value_heads.contains(&s))
            }
            _ => false,
        }
    }
    /// Whether a call of `s` on normal forms is its own normal form: `s` has no native, own value, identity,
    /// rules or attributes (the read is recorded). A native building `s(x)` from a normal form `x` may then skip
    /// evaluating it.
    pub fn is_inert(&mut self, s: Sym) -> bool {
        self.cx.reads |= crate::eval::bit(s);
        !self.ev.k.defs.has_meaning(s) && self.ev.k.defs.attrs(s) == crate::Attributes::default()
    }
    /// The own value of `s` (the read is recorded).
    pub fn value(&mut self, s: Sym) -> Option<T<'h>> {
        self.cx.reads |= crate::eval::bit(s);
        self.ev.k.defs.own(self.ev.heap, s)
    }
    /// The replacement the first matching rule gives for `t`, without evaluating it.
    pub fn rewrite_once(&mut self, t: T<'h>) -> Res<Option<T<'h>>> {
        self.ev.dispatch(self.cx, t)
    }
    /// Does `p` match `t`? (Conditions in it run on this task.)
    pub fn matches(&mut self, p: &Pattern, t: T<'h>) -> Res<bool> {
        self.ev.match_pattern(self.cx, p, t)
    }
    /// Compile a pattern once, to match many subjects with [`NativeCx::matches`] or [`NativeCx::match_binds`]
    /// (the same compilation and checks as a definition's left side; a pattern term lowers with
    /// [`crate::syntax::pattern`]). The pattern holds terms of this evaluation: keep it within the call.
    pub fn compile_pattern(&self, p: Pat) -> Result<Pattern, DefError> {
        self.ev.k.compile_pattern(self.ev.heap, p)
    }
    /// The first match of `p` against `t`: its bindings in `out` (by variable number). Conditions in it run on
    /// this task.
    pub fn match_binds(&mut self, p: &Pattern, t: T<'h>, out: &mut crate::Binds<'h>) -> Res<bool> {
        self.ev.match_binds(self.cx, p, t, out)
    }
    /// Compare two terms by the kernel's canonical order (semantics 8).
    pub fn cmp(&self, a: T<'h>, b: T<'h>) -> std::cmp::Ordering {
        self.ev.k.cmp(self.ev.heap, a, b)
    }
    /// Sort terms by the kernel's canonical order. Sorting is not interruptible: charge fuel for long inputs.
    pub fn sort(&self, v: &mut [T<'h>]) {
        self.ev.k.sort(self.ev.heap, v)
    }
    /// The stop flag long-running library code polls ([`Kernel::stop_flag`](crate::Kernel::stop_flag)).
    pub fn stop_flag(&self) -> &std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.ev.k.stop_flag_ref()
    }
    /// The control block of the evaluation (for worker threads to poll [`Control::should_stop`](crate::Control)).
    pub fn control(&self) -> std::sync::Arc<crate::Control> {
        self.ev.k.ctl.clone()
    }
    /// The bytes this evaluation may still allocate before its memory limit stops it: the session's memory budget,
    /// or the smaller cap of an enclosing `memory_constrained`, less what the evaluation has allocated so far. A
    /// builder whose result would not fit can refuse before allocating anything.
    pub fn memory_left(&self) -> usize {
        let k = &self.ev.k;
        k.young_cap.min(k.mem_cap.load(std::sync::atomic::Ordering::Relaxed)).saturating_sub(self.ev.heap.young_bytes())
    }
    /// Whether the evaluation should stop: the stop flag is raised, an interrupt was requested, or a limit fired.
    /// A cheap poll for loops between fuel charges.
    pub fn stopped(&self) -> bool {
        self.ev.k.stop_flag_ref().load(std::sync::atomic::Ordering::Relaxed) || self.ev.k.ctl.halted()
    }
    /// Give `head` a compiled specialisation during an evaluation (an effect of the frontier; see
    /// [`Kernel::register_compiled`](crate::Kernel::register_compiled)).
    pub fn register_compiled(&mut self, head: Sym, c: std::sync::Arc<dyn crate::Compiled>) -> Res<()> {
        self.ev.effect(self.cx)?;
        crate::compiled::set_compiled(&self.ev.k.defs, &self.ev.k.compiled, head, Some(c));
        Ok(())
    }
    /// The learned fuel per evaluation of calls with head `s` (granularity estimates).
    pub fn head_cost(&self, s: Sym) -> u64 {
        self.ev.k.head_cost(s)
    }
    /// Is `t` a function declared `effect` (a symbol, or a call whose head is one)?
    pub fn is_effect(&self, t: T<'h>) -> bool {
        let h = self.ev.heap;
        let s = match h.view(t) {
            View::Sym(s) => Some(s),
            View::App { head, .. } => match h.view(head) {
                View::Sym(s) => Some(s),
                _ => None,
            },
            _ => None,
        };
        s.is_some_and(|s| self.ev.k.defs.attrs(s).effect)
    }
    /// Run `f` in a scratch scope (decisions G.1 to G.3): the terms it makes that are not already shared are kept
    /// apart and dropped when it returns, so a native evaluating many times (an interpreted integrand, an ODE
    /// right-hand side) does not grow the heap until the statement ends. `f` returns plain data (machine numbers,
    /// arithmetic numbers): no handle made in the scope may outlive it.
    ///
    /// Inside the scope the evaluation does not split into parallel tasks and refuses effects: an effect makes `f`
    /// run again without a scope, which then performs it as usual (so `f` must be repeatable: it performed no
    /// effect before the refusal). Cells written in the scope are given shared copies of their values when it ends.
    /// Without a free scope (one is open on this task already, every slot is taken, or the heap's configuration
    /// turns them off) `f` simply runs. Results are the same either way.
    pub fn scratch<R: 'static>(&mut self, mut f: impl FnMut(&mut NativeCx<'_, '_, 'h>) -> Res<R>) -> Res<R> {
        let h = self.ev.heap;
        let Some(sc) = h.scratch() else { return f(self) };
        let outer = self.cx.scratch_cells.replace(vec![]);
        self.cx.serial += 1;
        let r = f(self);
        self.cx.serial -= 1;
        let written = std::mem::replace(&mut self.cx.scratch_cells, outer).unwrap_or_default();
        for id in written {
            if let Some(slot) = self.ev.k.cells.slot(id) {
                if let Some(v) = slot.get(h) {
                    if h.is_scratch(v) {
                        slot.set(sc.export(v));
                    }
                }
            }
        }
        drop(sc);
        match r {
            Err(crate::Abort::Scratch) => f(self),
            r => r,
        }
    }

    /// The elements `f(0) ... f(n-1)`, evaluated as a group: split into speculative tasks by the granularity rule,
    /// with effects in sequential order (semantics 18.2 item 4). `cost` estimates one element's fuel (0: unknown or
    /// effectful, which keeps the group sequential until measured).
    pub fn group(&mut self, n: usize, cost: u64, f: &ElemFn<'_, 'h>) -> Res<Vec<T<'h>>> {
        self.ev.group(self.cx, cost, n, &|ev: &Eval<'_, 'h>, cx: &mut Cx<'h>, i| f(&mut NativeCx { ev, cx }, i), None)
    }

    /// The elements `f(0) ... f(n-1)`, split over the session's worker threads at once (`parallel_table` and its
    /// kin, decision sp-core.4): the values, effects and failures are those of evaluating them in order.
    pub fn group_parallel(&mut self, n: usize, f: &ElemFn<'_, 'h>) -> Res<Vec<T<'h>>> {
        self.ev.group_parallel(self.cx, n, &|ev: &Eval<'_, 'h>, cx: &mut Cx<'h>, i| f(&mut NativeCx { ev, cx }, i), None)
    }

    /// What an iterating function does with a value its function or body gave (semantics 10): `failure(break)`
    /// (from `break()`) ends the iteration, `failure(continue)` (from `continue()`) skips the element; `None` for
    /// any other value, other failures included.
    pub fn loop_signal(&self, t: T<'h>) -> Option<LoopSignal> {
        let h = self.ev.heap;
        let failure = self.ev.sym(self.ev.k.names.failure);
        if h.mask(t) & h.mask(failure) == 0 || h.packed(t).is_some() {
            return None;
        }
        match h.view(t) {
            View::App { head, args: &[k] } if head == failure => match h.view(k) {
                View::Sym(s) if h.sym_name(s) == crate::nonlocal::BREAK => Some(LoopSignal::Break),
                View::Sym(s) if h.sym_name(s) == crate::nonlocal::CONTINUE => Some(LoopSignal::Continue),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether `t` is `failure(break)`.
    pub fn is_break(&self, t: T<'h>) -> bool {
        self.loop_signal(t) == Some(LoopSignal::Break)
    }

    /// Whether `t` is `failure(continue)`.
    pub fn is_continue(&self, t: T<'h>) -> bool {
        self.loop_signal(t) == Some(LoopSignal::Continue)
    }

    /// [`NativeCx::group`] read as the iterations of a loop: the values up to the first `failure(break)` (not
    /// included), no element after it evaluated on the frontier. Values `failure(continue)` stay in place for the
    /// caller to skip ([`NativeCx::without_continues`]), so the values stay aligned with the elements.
    pub fn group_loop(&mut self, n: usize, cost: u64, f: &ElemFn<'_, 'h>) -> Res<Vec<T<'h>>> {
        let (h, failure) = (self.ev.heap, self.ev.k.names.failure);
        let brk = h.sym(crate::nonlocal::BREAK);
        let stop = move |v: T<'h>| is_signal(h, failure, brk, v);
        let mut out =
            self.ev.group(self.cx, cost, n, &|ev: &Eval<'_, 'h>, cx: &mut Cx<'h>, i| f(&mut NativeCx { ev, cx }, i), Some(&stop))?;
        if out.last().is_some_and(|&v| stop(v)) {
            out.pop();
        }
        Ok(out)
    }

    /// [`NativeCx::group_parallel`] read as the iterations of a loop, like [`NativeCx::group_loop`]: the result is
    /// that of the serial loop (the frontier keeps the values in order and stops at the first `failure(break)`;
    /// speculative work past it is discarded, its effects never happen).
    pub fn group_parallel_loop(&mut self, n: usize, f: &ElemFn<'_, 'h>) -> Res<Vec<T<'h>>> {
        let (h, failure) = (self.ev.heap, self.ev.k.names.failure);
        let brk = h.sym(crate::nonlocal::BREAK);
        let stop = move |v: T<'h>| is_signal(h, failure, brk, v);
        let mut out =
            self.ev.group_parallel(self.cx, n, &|ev: &Eval<'_, 'h>, cx: &mut Cx<'h>, i| f(&mut NativeCx { ev, cx }, i), Some(&stop))?;
        if out.last().is_some_and(|&v| stop(v)) {
            out.pop();
        }
        Ok(out)
    }

    /// `vs` without the values `failure(continue)` (the skipped iterations of a loop).
    pub fn without_continues(&self, vs: Vec<T<'h>>) -> Vec<T<'h>> {
        if vs.iter().all(|&v| !self.is_continue(v)) {
            return vs;
        }
        vs.into_iter().filter(|&v| !self.is_continue(v)).collect()
    }

    /// Evaluate `t` with every call measured ([`crate::profile`]): the value and the measurements, one line per
    /// head symbol and kind of call. A profile inside a profiled evaluation measures nothing of its own (`None`):
    /// the outer one goes on recording.
    pub fn profiled(&mut self, t: T<'h>) -> Res<(T<'h>, Option<Vec<crate::profile::ProfileLine>>)> {
        let p = &self.ev.k.profiler;
        if p.active.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return Ok((self.eval(t)?, None));
        }
        p.reset();
        let (prof0, child0, depth) = (self.cx.prof, std::mem::take(&mut self.cx.prof_child), self.cx.prof_stack.len());
        self.cx.prof = true;
        let r = self.eval(t);
        // calls left open by an abort
        while self.cx.prof_stack.len() > depth {
            self.ev.prof_close(self.cx, 0);
        }
        self.cx.prof = prof0;
        self.cx.prof_child = child0;
        let lines = p.lines();
        p.active.store(false, std::sync::atomic::Ordering::Release);
        Ok((r?, Some(lines)))
    }

    /// An identity of the running task, the same for every call on it while it lives ([`Allowance`]).
    pub fn task_id(&self) -> usize {
        &*self.cx as *const Cx<'h> as usize
    }

    /// The number of worker threads evaluations may split over.
    pub fn threads(&self) -> usize {
        self.cx.threads as usize
    }
}

/// A method's budget of work (semantics 18.7): `limit` units of fuel from the reading of
/// [`NativeCx::fuel_spent`] when it was made. It replaces deadlines in wall-clock time, so a method gives up at the
/// same point of its work on every machine. It belongs to the task that made it: read on another task (a part of a
/// parallel group running on the same thread) it never runs out, as a deadline kept by a thread did not reach the
/// other threads.
#[derive(Clone, Copy, Debug)]
pub struct Allowance {
    start: u64,
    limit: u64,
    task: usize,
}

impl Allowance {
    /// `limit` units of fuel from now.
    pub fn new(cx: &NativeCx<'_, '_, '_>, limit: u64) -> Allowance {
        Allowance { start: cx.fuel_spent(), limit, task: cx.task_id() }
    }
    /// No limit.
    pub fn unlimited(cx: &NativeCx<'_, '_, '_>) -> Allowance {
        Allowance::new(cx, u64::MAX)
    }
    /// The units spent since it was made (0 on another task).
    pub fn spent(&self, cx: &NativeCx<'_, '_, '_>) -> u64 {
        if cx.task_id() != self.task {
            return 0;
        }
        cx.fuel_spent().saturating_sub(self.start)
    }
    /// The units left (0 once it is used up).
    pub fn left(&self, cx: &NativeCx<'_, '_, '_>) -> u64 {
        self.limit.saturating_sub(self.spent(cx))
    }
    /// Is it used up?
    pub fn expired(&self, cx: &NativeCx<'_, '_, '_>) -> bool {
        self.spent(cx) >= self.limit
    }
    /// The limit it was made with.
    pub fn limit(&self) -> u64 {
        self.limit
    }
    /// A budget of `limit` units from now, inside this one (at most what this one has left).
    pub fn within(&self, cx: &NativeCx<'_, '_, '_>, limit: u64) -> Allowance {
        Allowance::new(cx, limit.min(self.left(cx)))
    }
}

/// Microseconds of delay per block of fuel ([`set_test_slowdown`]).
static SLOWDOWN_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A test hook: delay every block of fuel by `us` microseconds of busy waiting, on every thread (0: off). It makes
/// the evaluation slower without changing its work, so a test can show that budgets of work give the same answers
/// on a slower machine.
pub fn set_test_slowdown(us: u64) {
    SLOWDOWN_US.store(us, std::sync::atomic::Ordering::Relaxed);
}

/// The delay of [`set_test_slowdown`], taken when a task draws a block of fuel.
#[inline]
pub(crate) fn slow_down() {
    let us = SLOWDOWN_US.load(std::sync::atomic::Ordering::Relaxed);
    if us > 0 {
        let end = std::time::Instant::now() + std::time::Duration::from_micros(us);
        while std::time::Instant::now() < end {
            std::hint::spin_loop();
        }
    }
}
