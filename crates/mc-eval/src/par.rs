//! Parallel portfolios and maps of plain data for natives (roadmap item 16A, roadmap 16A; semantics 18.1 and 18.4).
//!
//! The symbolic engines have independent work whose results are Rust values, not terms: the methods of a
//! portfolio (`integrate`, `d_solve`), the cases of a formula (`reduce`, the optimisation), the moves of one level
//! of the `simplify` search, the points of a numerical check. [`NativeCx::par_until`] runs such work on the
//! session's pool with the evaluator's speculative rules, so that **the result never depends on the number of
//! threads or on timing**:
//!
//! - **The sequential meaning.** `par_until(n, stop, f)` is `f(0), f(1), ...` in order, ending after the first
//!   value for which `stop` holds (or after `f(n - 1)`). [`NativeCx::par_map`] never stops; [`NativeCx::par_first`]
//!   stops at the first `Some`: the first verified answer *in the fixed order* is kept, never the first to finish.
//! - **Splitting** (as `mc-numfn`'s `Par` and the evaluator's groups do it): the elements are cut into at most
//!   four blocks per thread; the calling task computes the first block itself, every later block is a
//!   speculative task (its own [`Cx`], a fuel share, effects refused) that the pool's threads claim in order. With
//!   one thread, inside a scratch scope, or with `Split::Never` everything runs on the caller, in order: no
//!   speculative state, no slowdown.
//! - **Keeping speculative values.** After its own block the caller walks the later elements in order. An element
//!   computed speculatively is kept, and its fuel charged to the caller then, unless it left a hole (an effect, its
//!   share, a cancellation) or a definition made since the split touched what it read; otherwise the caller
//!   computes it again itself. So effects happen in order on the caller, and only the caller decides a limit
//!   failure (semantics 18.4, assembly decision A.5).
//! - **Cancellation.** When an element stops the run (`stop` holds), every later element is cancelled: a task
//!   working on one leaves a hole at its next fuel refill (at most [`crate::ctl::FUEL_BLOCK`] units later), and a
//!   block not yet started is skipped. Cancellation is inherited by nested work (a portfolio inside a cancelled
//!   method stops too). A cancelled element is never used: should the caller need it after all (the stopping
//!   value was discarded and computed again), it computes it itself.
//! - **Fuel, stops, interrupts.** Every task draws fuel; a time or memory limit and the interrupt stop every
//!   task at its next refill; a native's stop flag ([`NativeCx::stop_flag`]) is shared by every thread, but the
//!   number kernels poll it through a thread-local scope, which `f` must enter on the thread it runs on (each crate
//!   does it with its own `StopScope`).
//!
//! What `f` may do: anything a native may do (evaluate, build terms on the shared heap, match), with its own
//! thread-local state set up inside `f` (builders hold `Cell`s and are made per call of `f`). Values must not hold
//! cells allocated by `f` (none of the engines do). Time budgets inside `f` (a method's `Duration`) read the clock,
//! as they do sequentially: a run that reaches one is timing-dependent with any number of threads.
use super::{Abort, Cx, Eval, Res};
use crate::native::NativeCx;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU8, AtomicUsize, Ordering::*};

/// An element of a parallel run (`index`, on the task's context).
pub type ParFn<'f, 'h, R> = dyn Fn(&mut NativeCx<'_, '_, 'h>, usize) -> Res<R> + Sync + 'f;

/// Most blocks per pool thread.
const BLOCKS_PER_THREAD: usize = 4;

/// The cancellation state of one parallel run: elements above `cut` are not needed. Nodes are pooled for the
/// process (never freed), so a task's [`Cancel`] is a plain `Copy` reference.
pub(crate) struct Node {
    cut: AtomicUsize,
    /// the run this one is nested in, and the element of it this run belongs to
    parent: AtomicPtr<Node>,
    parent_at: AtomicU32,
}

static FREE: parking_lot::Mutex<Vec<&'static Node>> = parking_lot::Mutex::new(Vec::new());

impl Node {
    /// A node for a run started by a task with cancellation `outer`.
    fn acquire(outer: Cancel) -> &'static Node {
        let n = FREE.lock().pop().unwrap_or_else(|| {
            Box::leak(Box::new(Node { cut: AtomicUsize::new(usize::MAX), parent: AtomicPtr::new(std::ptr::null_mut()), parent_at: AtomicU32::new(0) }))
        });
        n.cut.store(usize::MAX, Relaxed);
        let (p, at) = match outer.0 {
            Some((p, at)) => (p as *const Node as *mut Node, at),
            None => (std::ptr::null_mut(), 0),
        };
        n.parent.store(p, Relaxed);
        n.parent_at.store(at, Relaxed);
        n
    }

    /// No element above `i` is needed.
    fn cut_at(&self, i: usize) {
        self.cut.fetch_min(i, AcqRel);
    }

    fn is_cut(&self, i: usize) -> bool {
        self.cut.load(Acquire) < i
    }
}

/// Returns the node to the pool when the run ends (every task that could see it has been joined).
struct Returned(&'static Node);

impl Drop for Returned {
    fn drop(&mut self) {
        FREE.lock().push(self.0);
    }
}

/// A task's cancellation: the element of the innermost parallel run it computes (`None`: not cancellable).
#[derive(Clone, Copy, Default)]
pub(crate) struct Cancel(Option<(&'static Node, u32)>);

impl Cancel {
    pub(crate) const NONE: Cancel = Cancel(None);

    fn at(node: &'static Node, i: usize) -> Cancel {
        Cancel(Some((node, i.min(u32::MAX as usize) as u32)))
    }

    /// Is this task's element, or the element of any run it is nested in, cancelled?
    #[inline]
    pub(crate) fn cancelled(self) -> bool {
        let mut c = self.0;
        // runs nest a few levels deep at most
        while let Some((n, i)) = c {
            if n.is_cut(i as usize) {
                return true;
            }
            let p = n.parent.load(Relaxed);
            // SAFETY: nodes are leaked (`&'static`); a parent pointer is either null or a node.
            c = (!p.is_null()).then(|| (unsafe { &*p }, n.parent_at.load(Relaxed)));
        }
        false
    }
}

/// One element computed by a speculative task: its value (`None`: skipped, cancelled before it started) and what
/// the caller needs to keep it.
struct Elem<R> {
    r: Option<Res<R>>,
    fuel: u64,
    reads: u64,
    volatile: u32,
    splits: u64,
    redos: u64,
    holes: u64,
}

impl<R> Default for Elem<R> {
    fn default() -> Self {
        Elem { r: None, fuel: 0, reads: 0, volatile: 0, splits: 0, redos: 0, holes: 0 }
    }
}

/// An element's slot: who computes it, and the speculative result once done.
struct Slot<R> {
    state: AtomicU8,
    elem: parking_lot::Mutex<Option<Elem<R>>>,
    done: parking_lot::Condvar,
}

/// Not started.
const PENDING: u8 = 0;
/// A speculative task computes it.
const RUNNING: u8 = 1;
/// Its speculative result is in the slot.
const DONE: u8 = 2;
/// The caller computes it itself.
const MINE: u8 = 3;

impl<'k, 'h> Eval<'k, 'h> {
    /// `f(0), f(1), ...` until `seq` (seeing the values in order) ends the run or `n` values: the parallel run
    /// described in the module documentation. `stop` is the one-value test the speculative tasks cancel by.
    pub(crate) fn until<R, S, F>(&self, cx: &mut Cx<'h>, n: usize, stop: &S, seq: &mut (dyn FnMut(&R) -> bool + Send), f: &F) -> Res<Vec<R>>
    where
        R: Send,
        S: Fn(&R) -> bool + Sync + ?Sized,
        F: Fn(&Self, &mut Cx<'h>, usize) -> Res<R> + Sync,
    {
        let mut out = Vec::with_capacity(n.min(64));
        if n < 2 || !self.may_split(cx) {
            for i in 0..n {
                let v = f(self, cx, i)?;
                let end = seq(&v);
                out.push(v);
                if end {
                    break;
                }
            }
            return Ok(out);
        }
        let blocks = n.min(BLOCKS_PER_THREAD * cx.threads.max(2) as usize);
        let size = n.div_ceil(blocks);
        let node = Node::acquire(cx.cancel);
        let _release = Returned(node);
        let slots: Vec<Slot<R>> = (0..n).map(|_| Slot { state: AtomicU8::new(PENDING), elem: parking_lot::Mutex::new(None), done: parking_lot::Condvar::new() }).collect();
        let epoch0 = cx.epoch;
        let task = cx.task;
        // cells: speculative tasks may read every cell allocated so far (as in the evaluator's groups)
        if cx.front {
            cx.split_born = self.k.cells.born();
        } else {
            cx.task = self.k.cells.task();
        }
        let mut rc = self.speculative(cx);
        cx.splits += 1;
        let first = size.min(n);
        let r = rayon::in_place_scope(|s| -> Res<()> {
            let slots = &slots;
            // the later blocks, offered to the pool (an idle thread takes them; with none, the caller computes
            // every element itself and the offer finds nothing left to do)
            s.spawn(move |_| mc_core::detached(|| self.chain(&mut rc, node, first, size, n, stop, f, slots)));
            for (i, slot) in slots.iter().enumerate() {
                let mine = i < first || slot.state.compare_exchange(PENDING, MINE, AcqRel, Acquire).is_ok();
                let kept = if mine {
                    None
                } else {
                    // a speculative task has it: wait for it, blocked (taking other jobs of the pool here could put
                    // work above this wait that waits on what is suspended below it; helping with later elements
                    // of this run delays seeing this one)
                    let mut g = slot.elem.lock();
                    while slot.state.load(Acquire) != DONE {
                        slot.done.wait(&mut g);
                    }
                    g.take()
                };
                let v = match kept {
                    Some(e) => {
                        cx.volatile += e.volatile;
                        cx.splits += e.splits;
                        cx.redos += e.redos;
                        cx.holes += e.holes;
                        match e.r {
                            Some(Ok(v)) if !self.k.changed(epoch0, cx.epoch, e.reads) => {
                                self.spend(cx, e.fuel).inspect_err(|_| node.cut_at(i))?;
                                cx.reads |= e.reads;
                                Ok(v)
                            }
                            _ => {
                                cx.redos += 1;
                                f(self, cx, i)
                            }
                        }
                    }
                    None => f(self, cx, i),
                };
                // a failure ends the run: nothing after it is needed
                let v = v.inspect_err(|_| node.cut_at(i))?;
                let end = seq(&v);
                out.push(v);
                if end {
                    node.cut_at(i);
                    break;
                }
            }
            Ok(())
        });
        cx.task = task;
        r?;
        Ok(out)
    }

    /// The speculative blocks from element `lo` on: this task computes the block at `lo` while the rest is offered
    /// to the pool.
    #[allow(clippy::too_many_arguments)]
    fn chain<R, S, F>(&self, rc: &mut Cx<'h>, node: &'static Node, lo: usize, size: usize, n: usize, stop: &S, f: &F, slots: &[Slot<R>])
    where
        R: Send,
        S: Fn(&R) -> bool + Sync + ?Sized,
        F: Fn(&Self, &mut Cx<'h>, usize) -> Res<R> + Sync,
    {
        if lo >= n || node.is_cut(lo) {
            return;
        }
        let hi = (lo + size).min(n);
        if hi < n {
            let mut next = self.speculative(rc);
            rayon::join(
                || self.block(rc, node, lo, hi, stop, f, slots),
                || mc_core::detached(|| self.chain(&mut next, node, hi, size, n, stop, f, slots)),
            );
        } else {
            self.block(rc, node, lo, hi, stop, f, slots);
        }
    }

    /// One speculative block: each element not taken by the caller on a fresh speculative context of `rc`, in
    /// order, until one stops or the run is cut.
    #[allow(clippy::too_many_arguments)]
    fn block<R, S, F>(&self, rc: &Cx<'h>, node: &'static Node, lo: usize, hi: usize, stop: &S, f: &F, slots: &[Slot<R>])
    where
        R: Send,
        S: Fn(&R) -> bool + Sync + ?Sized,
        F: Fn(&Self, &mut Cx<'h>, usize) -> Res<R> + Sync,
    {
        for (i, slot) in slots.iter().enumerate().take(hi).skip(lo) {
            if node.is_cut(i) {
                return;
            }
            self.one(rc, node, i, stop, f, slot);
        }
    }

    /// Element `i` on a fresh speculative context of `rc`, unless someone took it already; a value that stops the
    /// run cuts it there.
    fn one<R, S, F>(&self, rc: &Cx<'h>, node: &'static Node, i: usize, stop: &S, f: &F, slot: &Slot<R>)
    where
        R: Send,
        S: Fn(&R) -> bool + Sync + ?Sized,
        F: Fn(&Self, &mut Cx<'h>, usize) -> Res<R> + Sync,
    {
        if slot.state.compare_exchange(PENDING, RUNNING, AcqRel, Acquire).is_err() {
            return;
        }
        let mut ec = self.speculative(rc);
        ec.cancel = Cancel::at(node, i);
        let r = f(self, &mut ec, i);
        // a value finished after its element was cancelled may still be whole, but it is not needed now and the
        // caller recomputes it if that changes
        let r = if node.is_cut(i) { Err(Abort::Hole) } else { r };
        let end = matches!(&r, Ok(v) if stop(v));
        let hole = r.is_err();
        {
            let mut g = slot.elem.lock();
            *g = Some(Elem { r: Some(r), fuel: ec.fuel, reads: ec.reads, volatile: ec.volatile, splits: ec.splits, redos: ec.redos, holes: ec.holes + hole as u64 });
            slot.state.store(DONE, Release);
        }
        slot.done.notify_all();
        if end {
            node.cut_at(i);
        }
    }
}

impl<'h> NativeCx<'_, '_, 'h> {
    /// `f(0), f(1), ...` in order, ending after the first value for which `stop` holds (included) or after
    /// `f(n - 1)`; computed in parallel on the session's pool, with the same values, effects, failures and fuel as
    /// that sequential loop for any number of threads (see [`crate::eval`]'s `par` module documentation).
    pub fn par_until<R: Send>(&mut self, n: usize, stop: &(dyn Fn(&R) -> bool + Sync), f: &ParFn<'_, 'h, R>) -> Res<Vec<R>> {
        self.ev.until(self.cx, n, stop, &mut |r: &R| stop(r), &|ev: &Eval<'_, 'h>, cx: &mut Cx<'h>, i| f(&mut NativeCx { ev, cx }, i))
    }

    /// [`NativeCx::par_until`] with a stop that depends on the values so far: `scan` sees the values in order (on
    /// the calling task) and ends the run when it returns `true` (the value included); `stop` decides from one
    /// value alone, for cancelling later elements early, and must imply that `scan` would end the run there.
    /// (A check that needs three good points of twelve: `stop` is "this point refutes", `scan` counts.)
    pub fn par_scan<R: Send>(
        &mut self,
        n: usize,
        stop: &(dyn Fn(&R) -> bool + Sync),
        scan: &mut (dyn FnMut(&R) -> bool + Send),
        f: &ParFn<'_, 'h, R>,
    ) -> Res<Vec<R>> {
        self.ev.until(self.cx, n, stop, scan, &|ev: &Eval<'_, 'h>, cx: &mut Cx<'h>, i| f(&mut NativeCx { ev, cx }, i))
    }

    /// `[f(0), ..., f(n - 1)]`, computed in parallel ([`NativeCx::par_until`] without a stop).
    pub fn par_map<R: Send>(&mut self, n: usize, f: &ParFn<'_, 'h, R>) -> Res<Vec<R>> {
        self.par_until(n, &|_: &R| false, f)
    }

    /// The first `i` in `0..n` for which `f(i)` is `Some`, with its value: a portfolio whose methods run in
    /// parallel, the first answer **in the fixed order** kept (never the first to finish); later methods are
    /// cancelled once an earlier one answers.
    pub fn par_first<R: Send>(&mut self, n: usize, f: &ParFn<'_, 'h, Option<R>>) -> Res<Option<(usize, R)>> {
        let v = self.par_until(n, &|r: &Option<R>| r.is_some(), f)?;
        let i = v.len().wrapping_sub(1);
        Ok(v.into_iter().last().flatten().map(|r| (i, r)))
    }

    /// Whether [`NativeCx::par_until`] would split its work over threads here (more than one thread, no scratch
    /// scope, the evaluator splitting). Results never depend on it; callers may use it to size their work.
    pub fn parallel(&self) -> bool {
        self.ev.may_split(self.cx)
    }

    /// Whether this task's work was cancelled by a parallel run it belongs to (an earlier element stopped the run):
    /// a cheap poll for long loops between fuel charges, like [`NativeCx::stopped`]. A cancelled task's value is
    /// discarded whatever it is, so a loop may simply return early.
    pub fn cancelled(&self) -> bool {
        !self.cx.front && self.cx.cancel.cancelled()
    }
}

/// Plain-Rust parallel work of a native: work that evaluates nothing (the lifting of a decomposition, number
/// kernels), split over the session's pool without evaluation tasks. Made by [`NativeCx::budget`] on the calling
/// task; its elements poll and charge through a [`Sub`], and the caller charges what it keeps of their fuel to its
/// own task afterwards (by its meter), in order, so the fuel of a kept result is the same for any number of
/// threads.
///
/// What every element sees: the interrupt and the evaluation's limits (time, memory, fuel: the budget is capped by
/// the fuel the evaluation had left when it was made), the caller's stop flags and the caller's cancellation (an
/// element of a [`NativeCx::par_until`] run). A halted element returns whatever its code returns on a stop; the
/// caller decides (typically: charge what it kept, then compute a halted element again itself, which stops at once
/// if the stop was real).
pub struct Budget {
    ctl: std::sync::Arc<crate::ctl::Control>,
    flags: Vec<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    cancel: Cancel,
    cap: u64,
    used: std::sync::atomic::AtomicU64,
    threads: usize,
}

impl Budget {
    /// Whether [`Sub::until`] splits over threads (more than one thread and the evaluator splitting where the
    /// budget was made). Results never depend on it.
    pub fn parallel(&self) -> bool {
        self.threads > 1
    }

    /// The top of the work: a [`Sub`] not cancellable by any run of this budget.
    pub fn root(&self) -> Sub<'_> {
        Sub { b: self, cut: None, i: 0, parent: None, fuel: std::sync::atomic::AtomicU64::new(0) }
    }

    fn halted(&self) -> bool {
        self.ctl.halted() || self.cancel.cancelled() || self.flags.iter().any(|f| f.load(Relaxed))
    }
}

/// An element of plain parallel work ([`Budget`]): its polls and charges, and nested runs ([`Sub::until`]).
pub struct Sub<'a> {
    b: &'a Budget,
    /// the run this element belongs to (`None`: the root) and its index there
    cut: Option<&'a AtomicUsize>,
    i: usize,
    parent: Option<&'a Sub<'a>>,
    fuel: std::sync::atomic::AtomicU64,
}

impl<'a> Sub<'a> {
    /// Is this element no longer needed (an earlier element of its run, or of a run it is nested in, stopped it),
    /// or the whole work stopped (interrupt, limit, stop flag, the caller's cancellation)?
    pub fn halted(&self) -> bool {
        let mut s = Some(self);
        while let Some(x) = s {
            if x.cut.is_some_and(|c| c.load(Acquire) < x.i) {
                return true;
            }
            s = x.parent;
        }
        self.b.halted()
    }

    /// Charge `units` of work to this element; `false` when it must stop ([`Sub::halted`], or the budget's fuel
    /// cap reached by all elements together).
    pub fn charge(&self, units: u64) -> bool {
        self.fuel.fetch_add(units, Relaxed);
        let used = self.b.used.fetch_add(units, Relaxed) + units;
        used <= self.b.cap && !self.halted()
    }

    /// Add `units` kept from nested work (already counted in the budget's cap) to this element's fuel.
    pub fn settle(&self, units: u64) {
        self.fuel.fetch_add(units, Relaxed);
    }

    /// The fuel charged and settled to this element so far.
    pub fn fuel(&self) -> u64 {
        self.fuel.load(Relaxed)
    }

    /// Whether nested runs split over threads.
    pub fn parallel(&self) -> bool {
        self.b.parallel()
    }

    /// `f(0, sub), f(1, sub), ...` in order until `stop` holds of a value (included) or `n` values, each with its
    /// own [`Sub`] nested in this one: the values with each element's fuel. Elements after the stopping one are
    /// cancelled and not returned; the values are those of the sequential loop for any number of threads.
    pub fn until<R: Send>(&self, n: usize, stop: &(dyn Fn(&R) -> bool + Sync), f: &(dyn Fn(usize, &Sub<'_>) -> R + Sync)) -> Vec<(R, u64)> {
        let cut = AtomicUsize::new(usize::MAX);
        let mut slots: Vec<Option<(R, u64)>> = (0..n).map(|_| None).collect();
        let one = |i: usize, slot: &mut Option<(R, u64)>| {
            if cut.load(Acquire) < i {
                return;
            }
            let sub = Sub { b: self.b, cut: Some(&cut), i, parent: Some(self), fuel: std::sync::atomic::AtomicU64::new(0) };
            let r = f(i, &sub);
            if stop(&r) {
                cut.fetch_min(i, AcqRel);
            }
            *slot = Some((r, sub.fuel()));
        };
        if self.b.threads < 2 || n < 2 {
            for (i, s) in slots.iter_mut().enumerate() {
                one(i, s);
                if cut.load(Acquire) <= i {
                    break;
                }
            }
        } else {
            let blocks = n.min(BLOCKS_PER_THREAD * self.b.threads);
            let size = n.div_ceil(blocks);
            plain_chain(&one, 0, size, &mut slots);
        }
        let end = cut.load(Acquire);
        slots.into_iter().take(end.saturating_add(1)).map_while(|s| s).collect()
    }
}

/// The blocks of a plain run from element `lo` (`slots[0]`): this thread takes the block at `lo`, the rest is
/// offered to the pool.
fn plain_chain<R: Send>(one: &(dyn Fn(usize, &mut Option<R>) + Sync), lo: usize, size: usize, slots: &mut [Option<R>]) {
    let k = size.min(slots.len());
    let (block, rest) = slots.split_at_mut(k);
    let run = |block: &mut [Option<R>]| {
        for (j, s) in block.iter_mut().enumerate() {
            one(lo + j, s);
        }
    };
    if rest.is_empty() {
        run(block);
    } else {
        rayon::join(|| run(block), || mc_core::detached(|| plain_chain(one, lo + k, size, rest)));
    }
}

impl NativeCx<'_, '_, '_> {
    /// A [`Budget`] for plain-Rust parallel work of this native, polling `flags` besides the evaluation's own
    /// limits and interrupt. It holds no borrow of the task, so the native goes on charging its own meter.
    pub fn budget(&self, flags: &[&std::sync::Arc<std::sync::atomic::AtomicBool>]) -> Budget {
        let k = self.ev.k;
        let cap = if self.cx.front { k.ctl.remaining() } else { self.cx.share.saturating_add(self.cx.avail) };
        let threads = if self.ev.may_split(self.cx) { self.cx.threads.max(1) as usize } else { 1 };
        let mut fl: Vec<_> = flags.iter().map(|f| (*f).clone()).collect();
        fl.push(k.stop_flag_ref().clone());
        Budget { ctl: k.ctl.clone(), flags: fl, cancel: if self.cx.front { Cancel::NONE } else { self.cx.cancel }, cap: cap.saturating_add(self.cx.avail), used: std::sync::atomic::AtomicU64::new(0), threads }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_cut_and_nest() {
        let a = Node::acquire(Cancel::NONE);
        let _ra = Returned(a);
        let b = Node::acquire(Cancel::at(a, 3));
        let _rb = Returned(b);
        let c = Cancel::at(b, 5);
        assert!(!c.cancelled());
        b.cut_at(7);
        assert!(!c.cancelled());
        a.cut_at(2);
        assert!(c.cancelled(), "the outer run cancelled element 3");
        assert!(!Cancel::at(a, 2).cancelled() && !Cancel::at(a, 1).cancelled());
    }
}
