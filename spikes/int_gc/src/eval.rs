//! A small rule language evaluated in parallel on a work-stealing pool (rayon), deterministically (semantics 18.1).
//!
//! Evaluation is innermost and left to right (semantics 4.1). The strict arguments of a call form a *group*; a group
//! may be split into tasks ([`Eval::range`]): the left part runs on the current task, the right part becomes a
//! stealable task that runs **speculatively**:
//!
//! - Only the task on the *sequential frontier* (everything before it in left-to-right order is complete) may
//!   perform an effect. The frontier is passed down to the leftmost part of every split, so exactly one task holds it
//!   at a time. A speculative task that reaches an effect *aborts* ([`Abort`]); the parent re-evaluates the aborted part
//!   after its left part is complete, now on the frontier. Aborting instead of blocking is what semantics 18.2 item 3
//!   asks ("waits until every argument to its left is complete") with no blocked worker: a blocked worker in a
//!   work-stealing pool can deadlock, because the work it waits for may sit below it on its own stack.
//! - Every evaluation records its *read set* (the symbols whose own values it consulted, as a 64-bit mask, semantics
//!   4.3 and decision 13.1). When the left part performed definitions, the right part's result is kept only if none of
//!   them touched its read set; otherwise it is discarded and re-evaluated (semantics 18.3).
//! - Random draws are effects on a *stream*. At top level the stream is the session's; each element of `ptable` and
//!   `pmap` gets its own stream, a function of the construct's seed (one draw from the enclosing stream) and the
//!   element's index (18.2 item 5).
//!
//! Granularity ([`Split`]) decides only *whether* a group is split, never the result.
use crate::heap::{Heap, View};
use crate::{Raw, SplitMix, Trace, T};
use parking_lot::Mutex;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::*};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Split {
    /// Never split: the sequential evaluator (the reference).
    Never,
    /// Split every group of two or more down to single elements (maximal speculation; a stress test).
    Always,
    /// Split while the static estimate (cached tree size) of a range reaches the grain.
    Static,
    /// Lazy splitting: at most every `poll` fuel units, split the rest of a range in two when its estimated cost
    /// (the larger of the static estimate and the measured cost so far extrapolated) reaches the grain *and* this
    /// worker's deque is empty (no work is waiting for a thief).
    Lazy,
    /// As `Lazy`, but a speculative task stops at the first effect instead of speculating past it.
    LazyStop,
}

impl Split {
    pub fn name(self) -> &'static str {
        match self {
            Split::Never => "never",
            Split::Always => "always",
            Split::Static => "static",
            Split::Lazy => "lazy",
            Split::LazyStop => "lazy-stop",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub split: Split,
    /// Minimum estimated cost (fuel) of a part worth a task.
    pub grain: u64,
    /// Fuel between two split decisions of one range (lazy mode).
    pub poll: u64,
    /// The evaluation's fuel budget (semantics 18.4: one budget drawn by every task).
    pub fuel_limit: u64,
}

impl Config {
    pub fn new(split: Split) -> Config {
        Config { split, grain: 1 << 10, poll: 1 << 9, fuel_limit: u64::MAX }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Abort {
    /// A speculative task reached a global effect (output, definition).
    Global,
    /// A speculative task reached a random draw on a stream it does not own.
    Stream,
    /// Fuel exhausted or interrupted: the whole evaluation stops (semantics 18.4).
    Limit,
}

pub type Res<T> = Result<T, Abort>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    None,
    Add,
    Mul,
    Inc,
    Dbl,
    Plus,
    D,
    Loop,
    LoopC,
    Walk,
    RWalk,
    Pr,
    Print,
    Define,
    Random,
    Map,
    PMap,
    PTable,
    Table,
    Stats,
}

const BUILTINS: &[(&str, Op)] = &[
    ("add", Op::Add),
    ("mul", Op::Mul),
    ("inc", Op::Inc),
    ("dbl", Op::Dbl),
    ("plus", Op::Plus),
    ("d", Op::D),
    ("loop", Op::Loop),
    ("loopc", Op::LoopC),
    ("walk", Op::Walk),
    ("rwalk", Op::RWalk),
    ("pr", Op::Pr),
    ("print", Op::Print),
    ("define", Op::Define),
    ("random", Op::Random),
    ("map", Op::Map),
    ("pmap", Op::PMap),
    ("ptable", Op::PTable),
    ("table", Op::Table),
    ("stats", Op::Stats),
];

/// Steps of `rwalk(i)`.
pub const WALK_LEN: i64 = 8;

/// Symbols the rules build with.
pub struct Syms {
    pub add: T<'static>,
    pub mul: T<'static>,
    pub pow: T<'static>,
    pub sin: T<'static>,
    pub cos: T<'static>,
    pub exp: T<'static>,
    pub log: T<'static>,
    pub x: T<'static>,
    pub y: T<'static>,
    pub list: T<'static>,
    /// The global read by `loopc`.
    pub c: T<'static>,
    pub lp: T<'static>,
    pub loopc: T<'static>,
    pub walk: T<'static>,
    pub random: T<'static>,
    pub print: T<'static>,
}

/// Per-task evaluation state. The frontier task's `Cx` is passed down by `&mut` along the leftmost path, so its
/// stream, epoch and counters advance in sequential order.
#[derive(Clone, Copy)]
pub struct Cx {
    /// May perform global effects (output, definitions).
    front: bool,
    /// May draw from `stream`.
    stream_front: bool,
    stream: SplitMix,
    /// The definition epoch this task sees.
    epoch: u32,
    /// Read set of the evaluation in progress.
    reads: u64,
    /// Effects performed (an evaluation with effects is not memoised).
    effects: u32,
    /// Fuel used by this task and the parts it joined: the measured cost for granularity.
    fuel: u64,
    unflushed: u64,
    threads: u32,
    pub splits: u64,
    pub redos: u64,
    /// The fuel level at which this task may next run the (lazy) split decision: one budget per task, so the
    /// decision costs at most one sample per `poll` units of work however many small groups the task meets.
    next_poll: u64,
}

impl Cx {
    fn speculative(&self) -> Cx {
        Cx {
            front: false,
            stream_front: false,
            reads: 0,
            effects: 0,
            fuel: 0,
            unflushed: 0,
            splits: 0,
            redos: 0,
            next_poll: 0,
            ..*self
        }
    }
}

enum Step<'h> {
    Done(T<'h>),
    /// A rule produced a replacement: evaluate it (a loop, not recursion, semantics 4.1 step 6).
    Again(T<'h>),
}

/// Placeholders for elements a speculative task could not complete: one reached a global effect (and may also
/// draw), the other only a random draw. Never valid handles: the young arena does not allocate its last segment.
pub const HOLE_G: T<'static> = T::from_bits(0x7FFF_FFFF);
pub const HOLE_S: T<'static> = T::from_bits(0x7FFF_FFFE);

fn hole(a: Abort) -> T<'static> {
    if a == Abort::Stream {
        HOLE_S
    } else {
        HOLE_G
    }
}

fn hole_bit(t: T<'_>) -> u8 {
    if t == HOLE_G {
        1
    } else if t == HOLE_S {
        2
    } else {
        0
    }
}

/// A group with holes cannot complete its call: report the abort the holes need.
fn incomplete(holes: u8) -> Res<()> {
    match holes {
        0 => Ok(()),
        h if h & 1 != 0 => Err(Abort::Global),
        _ => Err(Abort::Stream),
    }
}

const MAX_SYMS: usize = 1 << 16;
/// Definitions recorded for read-set validation; past this, stamps older than the current epoch are distrusted.
const MAX_EPOCHS: usize = 1 << 16;
const FLUSH: u64 = 1 << 12;

/// The session state of the evaluator: everything that outlives one evaluation (own values, definition epochs,
/// output log, session stream, learned head costs). A root container: own values and the output log hold handles.
pub struct Defs {
    pub s: Syms,
    ops: Box<[Op]>,
    /// Learned cost (fuel) of a call, by head symbol index: the granularity rule's prior.
    cost: Box<[AtomicU32]>,
    /// Mask bits of the symbols declared `effect`: global effects (output, definitions) and random draws.
    fx_global: u64,
    fx_draw: u64,
    /// Own values by symbol index: 0 = none, else `1 << 32 | handle`.
    values: Box<[AtomicU64]>,
    versions: Box<[AtomicU32]>,
    epoch: AtomicU32,
    /// Mask bit of the symbol defined at each epoch.
    changes: Box<[AtomicU64]>,
    out: Mutex<Vec<Raw>>,
    session: Mutex<SplitMix>,
    stop: AtomicBool,
    fuel: AtomicU64,
}

impl Defs {
    pub fn new(st: &Heap, seed: u64) -> Defs {
        let mut ops = vec![Op::None; MAX_SYMS].into_boxed_slice();
        for &(name, op) in BUILTINS {
            ops[st.sym(name).index() as usize] = op;
        }
        let sym = |n: &str| st.sym(n).t();
        let s = Syms {
            add: sym("add"),
            mul: sym("mul"),
            pow: sym("pow"),
            sin: sym("sin"),
            cos: sym("cos"),
            exp: sym("exp"),
            log: sym("log"),
            x: sym("x"),
            y: sym("y"),
            list: sym("list"),
            c: sym("c"),
            lp: sym("loop"),
            loopc: sym("loopc"),
            walk: sym("walk"),
            random: sym("random"),
            print: sym("print"),
        };
        Defs {
            s,
            ops,
            cost: (0..MAX_SYMS).map(|_| AtomicU32::new(0)).collect(),
            fx_global: ["print", "define", "pr"].iter().fold(0, |m, n| m | st.meta(sym(n)).1),
            fx_draw: ["random", "walk", "rwalk", "ptable", "pmap"].iter().fold(0, |m, n| m | st.meta(sym(n)).1),
            values: (0..MAX_SYMS).map(|_| AtomicU64::new(0)).collect(),
            versions: (0..MAX_SYMS).map(|_| AtomicU32::new(0)).collect(),
            epoch: AtomicU32::new(1),
            changes: (0..MAX_EPOCHS).map(|_| AtomicU64::new(0)).collect(),
            out: Mutex::new(vec![]),
            session: Mutex::new(SplitMix::new(seed)),
            stop: AtomicBool::new(false),
            fuel: AtomicU64::new(0),
        }
    }

    /// The output log, in order.
    pub fn output<'h>(&self, _st: &'h Heap) -> Vec<T<'h>> {
        self.out.lock().iter().map(|r| T::from_bits(r.0)).collect()
    }

    pub fn value_of<'h>(&self, _st: &'h Heap, s: crate::Sym) -> Option<T<'h>> {
        let v = self.values[s.index() as usize].load(Acquire);
        (v != 0).then(|| T::from_bits(v as u32))
    }

    /// Definitions made so far (one version bump each).
    pub fn version_of(&self, s: crate::Sym) -> u32 {
        self.versions[s.index() as usize].load(Relaxed)
    }
}

impl Trace for Defs {
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        for v in self.values.iter_mut() {
            let x = v.get_mut();
            if *x != 0 {
                let mut r = Raw(*x as u32);
                visit(&mut r);
                *x = 1 << 32 | r.0 as u64;
            }
        }
        for r in self.out.get_mut().iter_mut() {
            visit(r);
        }
    }
}

/// One evaluation's evaluator: shared borrows of the heap and the session state, so its handles are branded `'h`
/// and every task of the evaluation may hold them.
pub struct Eval<'h> {
    pub st: &'h Heap,
    pub d: &'h Defs,
    pub s: &'h Syms,
    pub cfg: Config,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RunStats {
    pub splits: u64,
    pub redos: u64,
    pub fuel: u64,
}

fn bit(i: u32) -> u64 {
    1u64 << (i % 64)
}

fn step_lcg(x: i64) -> i64 {
    (x.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x3fff_ffff
}

impl<'h> Eval<'h> {
    pub fn new(st: &'h Heap, d: &'h Defs, cfg: Config) -> Self {
        Eval { st, d, s: &d.s, cfg }
    }

    /// Evaluate `t` as one top-level evaluation (on the frontier, drawing from the session stream). Call it inside
    /// the pool the evaluation should use.
    pub fn run(&self, t: T<'h>) -> (Res<T<'h>>, RunStats) {
        let d = self.d;
        d.stop.store(false, Relaxed);
        d.fuel.store(0, Relaxed);
        let mut cx = Cx {
            front: true,
            stream_front: true,
            stream: *d.session.lock(),
            epoch: d.epoch.load(Acquire),
            reads: 0,
            effects: 0,
            fuel: 0,
            unflushed: 0,
            threads: rayon::current_num_threads() as u32,
            splits: 0,
            redos: 0,
            next_poll: 0,
        };
        let r = self.eval(&mut cx, t);
        *d.session.lock() = cx.stream;
        let _ = self.flush(&mut cx);
        (r, RunStats { splits: cx.splits, redos: cx.redos, fuel: cx.fuel })
    }

    #[inline]
    fn value(&self, i: u32) -> Option<T<'h>> {
        let v = self.d.values.get(i as usize)?.load(Acquire);
        (v != 0).then_some(T::from_bits(v as u32))
    }

    #[inline]
    fn op(&self, h: T<'h>) -> Op {
        match self.st.view(h) {
            View::Sym(i) => self.d.ops.get(i as usize).copied().unwrap_or(Op::None),
            _ => Op::None,
        }
    }

    #[inline]
    fn tick(&self, cx: &mut Cx) -> Res<()> {
        cx.fuel += 1;
        cx.unflushed += 1;
        if cx.unflushed >= FLUSH {
            self.flush(cx)?;
        }
        Ok(())
    }

    /// Draw this task's fuel from the shared budget and poll the stop flag (in blocks, so the shared counter is
    /// touched once per `FLUSH` units).
    fn flush(&self, cx: &mut Cx) -> Res<()> {
        let u = std::mem::take(&mut cx.unflushed);
        if self.d.stop.load(Relaxed) {
            return Err(Abort::Limit);
        }
        if self.d.fuel.fetch_add(u, Relaxed).saturating_add(u) > self.cfg.fuel_limit {
            self.d.stop.store(true, Relaxed);
            return Err(Abort::Limit);
        }
        Ok(())
    }

    /// Did a definition between epochs `from` and `to` touch the read set `deps`?
    fn changed(&self, from: u32, to: u32, deps: u64) -> bool {
        if from == to {
            return false;
        }
        if to as usize >= MAX_EPOCHS || to < from {
            return true;
        }
        (from + 1..=to).any(|e| self.d.changes[e as usize].load(Relaxed) & deps != 0)
    }

    pub fn eval(&self, cx: &mut Cx, t: T<'h>) -> Res<T<'h>> {
        if t.is_imm() {
            return Ok(t);
        }
        self.tick(cx)?;
        if let Some((v, e, deps)) = self.st.memo_get(t) {
            // the session-counter fast path, then the read-set check (semantics 4.3)
            if e == cx.epoch || (e < cx.epoch && !self.changed(e, cx.epoch, deps)) {
                cx.reads |= deps;
                return Ok(v);
            }
        }
        let (reads0, effects0, epoch0, fuel0) = (cx.reads, cx.effects, cx.epoch, cx.fuel);
        cx.reads = 0;
        let r = self.eval_inner(cx, t);
        let deps = cx.reads;
        cx.reads |= reads0;
        let v = r?;
        self.learn(t, cx.fuel - fuel0);
        // results that involve effects are never memoised (semantics 9); a normal form is its own normal form
        // while none of its symbols gains a definition, hence its mask joins the read set
        if cx.effects == effects0 {
            let deps = deps | self.st.meta(v).1;
            self.st.memo_put(t, v, epoch0, deps);
            if v != t && !v.is_imm() {
                self.st.memo_put(v, v, epoch0, deps);
            }
        }
        Ok(v)
    }

    fn eval_inner(&self, cx: &mut Cx, mut t: T<'h>) -> Res<T<'h>> {
        loop {
            match self.st.view(t) {
                View::Int(_) => return Ok(t),
                View::Sym(i) => {
                    cx.reads |= bit(i);
                    return match self.value(i) {
                        Some(v) if v != t => self.eval(cx, v),
                        _ => Ok(t),
                    };
                }
                View::App(h, args) => {
                    let op = self.op(h);
                    if op == Op::Define && args.len() == 2 {
                        if let View::Sym(i) = self.st.view(args[0]) {
                            return self.define(cx, i, args[1]);
                        }
                    }
                    let vals = self.eval_args(cx, t, args)?;
                    match self.apply(cx, op, t, h, args, &vals)? {
                        Step::Done(v) => return Ok(v),
                        Step::Again(u) => {
                            self.tick(cx)?;
                            t = u;
                        }
                    }
                }
            }
        }
    }

    /// Whether groups may be split at all; with one worker the lazy rule takes the sequential loop.
    #[inline]
    fn may_split(&self, cx: &Cx) -> bool {
        match self.cfg.split {
            Split::Never => false,
            Split::Lazy | Split::LazyStop => cx.threads >= 2,
            _ => true,
        }
    }

    /// The learned cost of evaluating calls with `t`'s head, if `t` is a call.
    #[inline]
    fn head_cost(&self, t: T<'h>) -> u64 {
        match self.st.view(t) {
            View::App(h, _) => match self.st.view(h) {
                View::Sym(i) => self.d.cost.get(i as usize).map_or(0, |c| c.load(Relaxed) as u64),
                _ => 0,
            },
            _ => 0,
        }
    }

    /// Record the measured cost of evaluating `t` in its head's running estimate. Racy by design (a lost update only
    /// blurs a heuristic); written only when the estimate moves by more than an eighth, so popular heads do not
    /// bounce a cache line between workers.
    #[inline]
    fn learn(&self, t: T<'h>, fuel: u64) {
        let View::App(h, _) = self.st.view(t) else { return };
        let View::Sym(i) = self.st.view(h) else { return };
        let Some(c) = self.d.cost.get(i as usize) else { return };
        let old = c.load(Relaxed) as u64;
        let new = (old * 3 + fuel.min(u32::MAX as u64)) / 4;
        if new.abs_diff(old) > old / 8 {
            c.store(new as u32, Relaxed);
        }
    }

    /// The strict arguments of `t`.
    fn eval_args<'a>(&self, cx: &mut Cx, t: T<'h>, args: &'a [T<'h>]) -> Res<Cow<'a, [T<'h>]>> {
        // immediates are their own values: nothing to evaluate, split or copy
        if args.iter().all(|a| a.is_imm()) {
            return Ok(Cow::Borrowed(args));
        }
        let mut out = vec![HOLE_G; args.len()];
        if args.len() >= 2 && self.may_split(cx) {
            // static estimate: the tree size, or the learned cost of the head when that is larger (a small term can
            // be an expensive call); 0 for a term that contains a symbol declared `effect` (semantics 9), since a
            // speculative task would only reach the effect and leave a hole
            let est = |i: usize| {
                if self.st.meta(args[i]).1 & (self.d.fx_global | self.d.fx_draw) != 0 {
                    0
                } else {
                    (self.st.size(args[i]) as u64).max(self.head_cost(args[i]))
                }
            };
            let hint = self.st.size(t) as u64;
            let h = self.range(cx, 0, args.len(), hint, &mut out, &est, &|ev: &Self, cx: &mut Cx, i| ev.eval(cx, args[i]))?;
            incomplete(h)?;
        } else {
            for (o, &a) in out.iter_mut().zip(args) {
                *o = self.eval(cx, a)?;
            }
        }
        Ok(Cow::Owned(out))
    }

    /// Evaluate elements `lo..hi` of a group into `out` (which holds exactly those elements), splitting by the
    /// granularity rule. `f(cx, i)` evaluates element `i`; `est(i)` is its static cost estimate.
    ///
    /// Returns the kinds of *holes* left ([`HOLE_G`], [`HOLE_S`] bits; 0 when complete). Only a speculative task
    /// leaves holes: an element that reached an effect it may not perform is marked and the task carries on with the
    /// next element (speculating past the effect), unless it owns a stream, whose order a later draw would break.
    /// The task that can perform the effect fills the holes in order ([`Eval::fill`]).
    ///
    /// `hint` is an O(1) static estimate of the whole range (the parent's cached size): when it reaches the grain the
    /// decision runs at once, otherwise when the task's poll budget allows.
    #[allow(clippy::too_many_arguments)]
    fn range<E, F>(&self, cx: &mut Cx, lo: usize, hi: usize, hint: u64, out: &mut [T<'h>], est: &E, f: &F) -> Res<u8>
    where
        E: Fn(usize) -> u64 + Sync,
        F: Fn(&Self, &mut Cx, usize) -> Res<T<'h>> + Sync,
    {
        let fuel0 = cx.fuel;
        let mut holes = 0;
        // the fuel level at which to consider splitting next: the per-element cost of the loop is one comparison
        let mut next = match self.cfg.split {
            Split::Never => u64::MAX,
            Split::Lazy | Split::LazyStop if cx.threads < 2 => u64::MAX,
            Split::Lazy | Split::LazyStop if hint < self.cfg.grain => cx.next_poll,
            _ => cx.fuel,
        };
        for i in lo..hi {
            if cx.fuel >= next && hi - i >= 2 && self.should_split(cx, lo, i, hi, fuel0, &mut next, est) {
                let mid = i + (hi - i) / 2;
                let (left, right) = out[i - lo..].split_at_mut(mid - i);
                let epoch0 = cx.epoch;
                let mut rc = cx.speculative();
                cx.splits += 1;
                let (rl, rr) =
                    rayon::join(|| self.range(cx, i, mid, 0, left, est, f), || self.range(&mut rc, mid, hi, 0, right, est, f));
                cx.fuel += rc.fuel;
                cx.effects += rc.effects;
                cx.splits += rc.splits;
                cx.redos += rc.redos;
                // the right part's values are kept unless refreshed below, so its reads count
                cx.reads |= rc.reads;
                let (lh, rh) = (rl?, rr?);
                if lh != 0 {
                    // a speculative task: no definitions happened, and whoever can fill the holes does so, in order
                    return Ok(holes | lh | rh);
                }
                if rh == 0 && !self.changed(epoch0, cx.epoch, rc.reads) {
                    return Ok(holes);
                }
                // the right part reached effects, or read what the left part redefined: complete it in order, now
                // after everything to its left
                return Ok(holes | self.fill(cx, mid, right, epoch0, rc.reads, f)?);
            }
            match f(self, cx, i) {
                Ok(v) => out[i - lo] = v,
                Err(Abort::Limit) => return Err(Abort::Limit),
                Err(a) => {
                    let h = hole(a);
                    out[i - lo] = h;
                    holes |= hole_bit(h);
                    if cx.stream_front || self.cfg.split == Split::LazyStop {
                        for o in &mut out[i - lo + 1..] {
                            *o = HOLE_G;
                        }
                        return Ok(holes | hole_bit(HOLE_G));
                    }
                }
            }
        }
        Ok(holes)
    }

    /// Complete `out` (elements `lo..`) in order on this task: evaluate the holes this task may fill, and re-evaluate
    /// elements whose read set (`reads`, the union over the part) a definition since `epoch0` touched (the memo
    /// answers those it did not affect). Stops at the first hole it may not fill; returns the holes left.
    fn fill<F>(&self, cx: &mut Cx, lo: usize, out: &mut [T<'h>], epoch0: u32, reads: u64, f: &F) -> Res<u8>
    where
        F: Fn(&Self, &mut Cx, usize) -> Res<T<'h>> + Sync,
    {
        let left = |out: &[T<'h>]| out.iter().fold(0, |m, &t| m | hole_bit(t));
        for j in 0..out.len() {
            let cur = out[j];
            let is_hole = cur == HOLE_G || cur == HOLE_S;
            if !is_hole && !self.changed(epoch0, cx.epoch, reads) {
                continue;
            }
            if (cur == HOLE_G && !cx.front) || (cur == HOLE_S && !cx.stream_front) {
                return Ok(left(&out[j..]));
            }
            cx.redos += 1;
            let snapshot = cx.stream;
            match f(self, cx, lo + j) {
                Ok(v) => out[j] = v,
                Err(Abort::Limit) => return Err(Abort::Limit),
                Err(a) => {
                    // the element needs more than this task owns: undo its draws, leave it and the rest
                    cx.stream = snapshot;
                    out[j] = hole(a);
                    return Ok(left(&out[j..]));
                }
            }
        }
        Ok(0)
    }

    /// The granularity rule, called when the fuel reaches `next`; sets the next level.
    #[allow(clippy::too_many_arguments)]
    fn should_split<E: Fn(usize) -> u64>(
        &self,
        cx: &mut Cx,
        lo: usize,
        i: usize,
        hi: usize,
        fuel0: u64,
        next: &mut u64,
        est: &E,
    ) -> bool {
        // A sample of (up to) 8 elements: their static estimates, extrapolated to the rest, and the share of them
        // worth speculating on (an estimate of 0 marks an element that will reach an effect).
        let sample = || {
            let k = (hi - i).min(8);
            let (sum, pure) = (i..i + k).map(est).fold((0u64, 0u64), |(s, p), e| (s + e, p + (e > 0) as u64));
            (sum * (hi - i) as u64 / k as u64, pure, k as u64)
        };
        match self.cfg.split {
            Split::Never => false,
            Split::Always => true,
            Split::Static => {
                // decided once per range; the halves decide again
                *next = u64::MAX;
                sample().0 >= self.cfg.grain
            }
            Split::Lazy | Split::LazyStop => {
                cx.next_poll = cx.fuel + self.cfg.poll;
                *next = cx.next_poll;
                let done = (i - lo) as u64;
                let (stat, pure, k) = sample();
                let measured = (cx.fuel - fuel0).saturating_mul((hi - i) as u64).checked_div(done).unwrap_or(0);
                // only the elements worth speculating on count
                let measured = measured.saturating_mul(pure) / k;
                measured.max(stat) >= self.cfg.grain && rayon_core::current_thread_has_pending_tasks() == Some(false)
            }
        }
    }

    fn define(&self, cx: &mut Cx, i: u32, rhs: T<'h>) -> Res<T<'h>> {
        let v = self.eval(cx, rhs)?;
        if !cx.front {
            return Err(Abort::Global);
        }
        // only the frontier defines, so epochs advance in sequential order with one writer
        debug_assert_eq!(cx.epoch, self.d.epoch.load(Relaxed));
        let e = cx.epoch + 1;
        if (e as usize) < MAX_EPOCHS {
            self.d.changes[e as usize].store(bit(i), Relaxed);
        }
        self.d.values[i as usize].store(1 << 32 | v.bits() as u64, Release);
        self.d.versions[i as usize].fetch_add(1, Relaxed);
        self.d.epoch.store(e, Release);
        cx.epoch = e;
        cx.effects += 1;
        Ok(v)
    }

    fn draw(&self, cx: &mut Cx) -> Res<u64> {
        if !cx.stream_front {
            return Err(Abort::Stream);
        }
        cx.effects += 1;
        Ok(cx.stream.next_u64())
    }

    /// `f(x)` for `map`: arithmetic heads directly, without building the call.
    fn call1(&self, cx: &mut Cx, f: T<'h>, x: T<'h>) -> Res<T<'h>> {
        self.tick(cx)?;
        match (self.op(f), self.st.as_int(x)) {
            (Op::Inc, Some(v)) => Ok(self.st.int(v + 1)),
            (Op::Dbl, Some(v)) => Ok(self.st.int(v.wrapping_mul(2))),
            _ => self.eval(cx, self.st.app(f, &[x])),
        }
    }

    /// Element `i` of `ptable`/`pmap`, on its own stream.
    fn with_stream(&self, cx: &mut Cx, seed: u64, i: usize, g: impl FnOnce(&mut Cx) -> Res<T<'h>>) -> Res<T<'h>> {
        let saved = (cx.stream, cx.stream_front);
        cx.stream = SplitMix::split(seed, i as u64);
        cx.stream_front = true;
        let r = g(cx);
        (cx.stream, cx.stream_front) = saved;
        r
    }

    /// The elements `f(0..n)` of `map`, `pmap`, `ptable` or `table` as a list; `head` is the function applied (its
    /// learned cost is the static estimate of an element).
    /// `owned`: each element draws from its own stream (`ptable`, `pmap`), so only global effects stop speculation.
    fn group(
        &self,
        cx: &mut Cx,
        head: T<'h>,
        owned: bool,
        n: usize,
        f: &(impl Fn(&Self, &mut Cx, usize) -> Res<T<'h>> + Sync),
    ) -> Res<T<'h>> {
        let mut out = vec![HOLE_G; n];
        if !self.may_split(cx) {
            for (i, o) in out.iter_mut().enumerate() {
                *o = f(self, cx, i)?;
            }
        } else {
            let c = match self.st.view(head) {
                View::Sym(i) => self.d.cost.get(i as usize).map_or(0, |c| c.load(Relaxed) as u64),
                _ => 0,
            };
            let fx = if owned { self.d.fx_global } else { self.d.fx_global | self.d.fx_draw };
            let c = if self.st.meta(head).1 & fx != 0 { 0 } else { c.max(1) };
            let est = |_| c;
            incomplete(self.range(cx, 0, n, c.saturating_mul(n as u64), &mut out, &est, f)?)?;
        }
        Ok(self.st.app(self.s.list, &out))
    }

    fn list_items(&self, t: T<'h>) -> Option<&[T<'h>]> {
        match self.st.view(t) {
            View::App(h, a) if h == self.s.list => Some(a),
            _ => None,
        }
    }

    fn apply(&self, cx: &mut Cx, op: Op, t: T<'h>, h: T<'h>, args: &[T<'h>], v: &[T<'h>]) -> Res<Step<'h>> {
        let st = self.st;
        let int = |i: usize| v.get(i).and_then(|&x| st.as_int(x));
        let done = |x: T<'h>| Ok(Step::Done(x));
        match (op, v.len()) {
            (Op::Add, 2) => done(self.mk_add(v[0], v[1])),
            (Op::Mul, 2) => done(self.mk_mul(v[0], v[1])),
            (Op::Inc, 1) if int(0).is_some() => done(st.int(int(0).unwrap() + 1)),
            (Op::Dbl, 1) if int(0).is_some() => done(st.int(int(0).unwrap().wrapping_mul(2))),
            (Op::Plus, _) if v.iter().all(|&x| st.as_int(x).is_some()) => {
                done(st.int(v.iter().fold(0i64, |s, &x| s.wrapping_add(st.as_int(x).unwrap()))))
            }
            (Op::D, 1) => done(self.deriv(cx, v[0])?),
            (Op::Loop, 2) if int(0).is_some() && int(1).is_some() => {
                let (k, x) = (int(0).unwrap(), int(1).unwrap());
                if k <= 0 {
                    return done(v[1]);
                }
                Ok(Step::Again(st.app(self.s.lp, &[st.int(k - 1), st.int(step_lcg(x))])))
            }
            (Op::LoopC, 2) if int(0).is_some() => {
                let k = int(0).unwrap();
                if k <= 0 {
                    return done(v[1]);
                }
                let next = st.app(self.s.add, &[v[1], self.s.c]);
                Ok(Step::Again(st.app(self.s.loopc, &[st.int(k - 1), next])))
            }
            (Op::Walk, 2) if int(0).is_some() => {
                let k = int(0).unwrap();
                if k <= 0 {
                    return done(v[1]);
                }
                let r = st.app(self.s.random, &[st.int(3)]);
                let next = st.app(self.s.add, &[v[1], r]);
                Ok(Step::Again(st.app(self.s.walk, &[st.int(k - 1), next])))
            }
            (Op::RWalk, 1) => Ok(Step::Again(st.app(self.s.walk, &[st.int(WALK_LEN), v[0]]))),
            (Op::Pr, 1) => Ok(Step::Again(st.app(self.s.print, &[v[0]]))),
            (Op::Print, 1) => {
                if !cx.front {
                    return Err(Abort::Global);
                }
                cx.effects += 1;
                self.d.out.lock().push(v[0].raw());
                done(v[0])
            }
            (Op::Random, 1) if int(0).is_some_and(|n| n > 0) => {
                let n = int(0).unwrap() as u64;
                let r = self.draw(cx)?;
                done(st.int((r % n) as i64))
            }
            (Op::Map, 2) if self.list_items(v[1]).is_some() => {
                let xs = self.list_items(v[1]).unwrap();
                let f = v[0];
                done(self.group(cx, f, false, xs.len(), &|ev: &Self, cx: &mut Cx, i| ev.call1(cx, f, xs[i]))?)
            }
            (Op::PMap, 2) if self.list_items(v[1]).is_some() => {
                let xs = self.list_items(v[1]).unwrap();
                let (f, seed) = (v[0], self.draw(cx)?);
                done(self.group(cx, f, true, xs.len(), &|ev: &Self, cx: &mut Cx, i| {
                    ev.with_stream(cx, seed, i, |cx| ev.call1(cx, f, xs[i]))
                })?)
            }
            (Op::PTable, 2) if int(1).is_some_and(|n| n >= 0) => {
                let (f, n, seed) = (v[0], int(1).unwrap() as usize, self.draw(cx)?);
                done(self.group(cx, f, true, n, &|ev: &Self, cx: &mut Cx, i| {
                    ev.with_stream(cx, seed, i, |cx| ev.eval(cx, ev.st.app(f, &[ev.st.int(i as i64)])))
                })?)
            }
            (Op::Table, 2) if int(1).is_some_and(|n| n >= 0) => {
                let (f, n) = (v[0], int(1).unwrap() as usize);
                done(self.group(cx, f, false, n, &|ev: &Self, cx: &mut Cx, i| ev.eval(cx, ev.st.app(f, &[ev.st.int(i as i64)])))?)
            }
            (Op::Stats, 1) if self.list_items(v[0]).is_some_and(|a| a.iter().all(|&x| st.as_int(x).is_some())) => {
                let a: Vec<i64> = self.list_items(v[0]).unwrap().iter().map(|&x| st.as_int(x).unwrap()).collect();
                let sum = a.iter().fold(0i64, |s, &x| s.wrapping_add(x));
                let (mx, mn) = (a.iter().copied().max().unwrap_or(0), a.iter().copied().min().unwrap_or(0));
                let r = [st.int(sum), st.int(mx), st.int(mn), st.int(a.len() as i64)];
                done(st.app(self.s.list, &r))
            }
            // no rule: a constructor; unchanged arguments give the same term back without interning
            _ if std::ptr::eq(v, args) || v == args => done(t),
            _ => done(st.app(h, v)),
        }
    }

    fn mk_add(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        let st = self.st;
        match (st.as_int(a), st.as_int(b)) {
            (Some(0), _) => b,
            (_, Some(0)) => a,
            (Some(x), Some(y)) => st.int(x.wrapping_add(y)),
            _ => st.app(self.s.add, &[a, b]),
        }
    }

    fn mk_mul(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        let st = self.st;
        match (st.as_int(a), st.as_int(b)) {
            (Some(0), _) | (_, Some(0)) => st.int(0),
            (Some(1), _) => b,
            (_, Some(1)) => a,
            (Some(x), Some(y)) => st.int(x.wrapping_mul(y)),
            _ => st.app(self.s.mul, &[a, b]),
        }
    }

    /// d/dx, a native kernel (spike 1's R3 derivative), charging one fuel unit per node.
    fn deriv(&self, cx: &mut Cx, t: T<'h>) -> Res<T<'h>> {
        self.tick(cx)?;
        let st = self.st;
        let s = &self.s;
        let (h, a) = match st.view(t) {
            View::App(h, a) => (h, a),
            View::Sym(_) if t == s.x => return Ok(st.int(1)),
            _ => return Ok(st.int(0)),
        };
        Ok(if h == s.add && a.len() == 2 {
            let (da, db) = (self.deriv(cx, a[0])?, self.deriv(cx, a[1])?);
            self.mk_add(da, db)
        } else if h == s.mul && a.len() == 2 {
            let (da, db) = (self.deriv(cx, a[0])?, self.deriv(cx, a[1])?);
            let l = self.mk_mul(da, a[1]);
            let r = self.mk_mul(a[0], db);
            self.mk_add(l, r)
        } else if h == s.pow && a.len() == 2 && st.as_int(a[1]).is_some() {
            let n = st.as_int(a[1]).unwrap();
            let da = self.deriv(cx, a[0])?;
            let p = st.app(s.pow, &[a[0], st.int(n - 1)]);
            let m = self.mk_mul(st.int(n), p);
            self.mk_mul(m, da)
        } else if a.len() == 1 && [s.sin, s.cos, s.exp, s.log].contains(&h) {
            let da = self.deriv(cx, a[0])?;
            let outer = if h == s.sin {
                st.app(s.cos, &[a[0]])
            } else if h == s.cos {
                let sn = st.app(s.sin, &[a[0]]);
                self.mk_mul(st.int(-1), sn)
            } else if h == s.exp {
                t
            } else {
                st.app(s.pow, &[a[0], st.int(-1)])
            };
            self.mk_mul(outer, da)
        } else {
            st.int(0)
        })
    }
}
