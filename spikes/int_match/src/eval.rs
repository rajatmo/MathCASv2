//! The evaluator: spike 4's parallel core with spike 2's rule dispatch.
//!
//! From spike 4 (decisions 4.4 to 4.9), unchanged in substance: strict arguments form a group that the lazy
//! granularity rule may split; the right part of a split runs speculatively; only the task on the sequential frontier
//! performs effects, a speculative task that reaches one leaves a *hole* and the frontier fills holes in order; every
//! evaluation records a 64-bit read set, and a speculative part whose read set a definition on its left touched is
//! re-evaluated; the memo is spike 4's lossy seqlock, stamped with the start epoch and the read set.
//!
//! From spike 2 (decisions 2.1 to 2.9): innermost evaluation with AC canonicalisation, then argument-head rules of the
//! arguments' heads in argument order, then the head's rules (semantics 4.1 step 3.5, 6.5), each through its index in
//! rule order; read sets propagated through memo hits.
//!
//! New in the integration:
//! - **Conditions evaluate.** A condition receives a [`CondCx`] that lends it the task's context: it can allocate in
//!   the shared heap, read own values and call [`CondCx::eval`]. Matching is therefore re-entrant (condition, then
//!   evaluation, then dispatch, then matching), so dispatch scratch is per frame (a pool in the task's [`Cx`]).
//! - **Read sets through matching.** The reads a condition makes, directly or through the evaluations it starts
//!   (their rule-set lookups, own values, memo-hit read sets), accumulate in the task's read set, which is the read
//!   set of the evaluation whose dispatch ran the condition. A speculative condition that reaches an effect aborts the
//!   match; the abort leaves a hole like any other.
//! - **Definitions are ordered effects, enforced by types.** Between evaluations the session defines through
//!   `&mut self` methods ([`Eval::define`], [`Eval::set_own`]), which cannot overlap an evaluation (it borrows
//!   `&self`). During an evaluation only the frontier task defines (`define`, `defrule`), and it updates the rule
//!   indexes in place for concurrent readers ([`crate::index`]).
use crate::heap::{Heap, View, MAX_SYMS};
use crate::index::{Defs, IndexMode, Rhs, Rule, RuleSet, Scratch};
use crate::matcher::Matcher;
use crate::memo::{Memo, MemoKind};
use crate::pat::{compile, Pat};
use crate::{sym_bit, Term, IMM};
use parking_lot::Mutex;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::*};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Split {
    /// Never split: the sequential evaluator (the reference).
    Never,
    /// Split every group of two or more down to single elements (maximal speculation: the stress mode).
    Always,
    /// Spike 4's lazy rule (decision 4.4).
    Lazy,
}

impl Split {
    pub fn name(self) -> &'static str {
        match self {
            Split::Never => "never",
            Split::Always => "always",
            Split::Lazy => "lazy",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub split: Split,
    pub grain: u64,
    pub poll: u64,
    pub fuel_limit: u64,
    /// Memoise normal forms (spike 2's R5 and R13 baselines run without).
    pub memo: bool,
    pub memo_bits: u32,
    /// Spike 4's hashed lossy table, or one entry per term id (decision I2.6).
    pub memo_kind: MemoKind,
    /// Spike 2's pruning AC strategy (decision 2.1); false: the naive strategy (tests only).
    pub smart: bool,
    pub index: IndexMode,
    /// Record the reads conditions make. False only to show what goes wrong without it.
    pub track_conds: bool,
}

impl Config {
    pub fn new(split: Split) -> Config {
        Config {
            split,
            grain: 1 << 10,
            poll: 1 << 9,
            fuel_limit: u64::MAX,
            memo: true,
            memo_bits: 20,
            memo_kind: MemoKind::Dense,
            smart: true,
            index: IndexMode::LockFree,
            track_conds: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Abort {
    /// A speculative task reached a global effect (output, definition).
    Global,
    /// Fuel exhausted or interrupted: the whole evaluation stops (semantics 18.4).
    Limit,
}

pub type Res<T> = Result<T, Abort>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    None,
    IAdd,
    IMul,
    IMod,
    Less,
    IsPrime,
    Plus,
    Loop,
    Print,
    Define,
    DefRule,
    Top,
    Cases,
    Map,
}

const BUILTINS: &[(&str, Op)] = &[
    ("iadd", Op::IAdd),
    ("imul", Op::IMul),
    ("imod", Op::IMod),
    ("less", Op::Less),
    ("is_prime", Op::IsPrime),
    ("plus", Op::Plus),
    ("loop", Op::Loop),
    ("print", Op::Print),
    ("define", Op::Define),
    ("defrule", Op::DefRule),
    ("top", Op::Top),
    ("cases", Op::Cases),
    ("map", Op::Map),
];

/// Symbols the evaluator builds with.
pub struct Syms {
    pub list: Term,
    pub true_: Term,
    pub false_: Term,
    pub none: Term,
    pub null: Term,
    pub lp: Term,
}

/// Per-task evaluation state. The frontier's `Cx` is passed down by `&mut` along the leftmost path.
pub struct Cx {
    /// May perform global effects.
    front: bool,
    /// The definition epoch this task started from (stamps and validation use it).
    epoch: u32,
    /// Read set of the evaluation in progress (a 64-bit mask, decision 2.7).
    pub reads: u64,
    effects: u32,
    fuel: u64,
    unflushed: u64,
    threads: u32,
    pub splits: u64,
    pub redos: u64,
    next_poll: u64,
    /// Dispatch scratch, one per active dispatch frame.
    #[allow(clippy::vec_box)] // boxed: popping and pushing a frame moves a pointer, not the buffers
    pool: Vec<Box<Scratch>>,
}

impl Cx {
    fn speculative(&self) -> Cx {
        Cx {
            front: false,
            epoch: self.epoch,
            reads: 0,
            effects: 0,
            fuel: 0,
            unflushed: 0,
            threads: self.threads,
            splits: 0,
            redos: 0,
            next_poll: 0,
            pool: vec![],
        }
    }
}

/// What a condition sees: the shared heap, own values and the evaluator, with every read entering the task's read
/// set (question 2).
pub struct CondCx<'a, 'h> {
    ev: &'a Eval<'h>,
    cx: &'a mut Cx,
}

impl<'a, 'h> CondCx<'a, 'h> {
    pub fn new(ev: &'a Eval<'h>, cx: &'a mut Cx) -> CondCx<'a, 'h> {
        CondCx { ev, cx }
    }
    /// The shared heap: allocating here is safe while other tasks match and evaluate.
    #[inline]
    pub fn heap(&self) -> &'h Heap {
        self.ev.heap
    }
    #[inline]
    pub fn int(&self, t: Term) -> Option<i64> {
        self.ev.heap.as_int(t)
    }
    /// The own value of `sym`; the read is recorded.
    pub fn value(&mut self, sym: Term) -> Option<Term> {
        let i = self.ev.heap.sym_index(sym)?;
        if self.ev.cfg.track_conds {
            self.cx.reads |= sym_bit(i);
        }
        self.ev.defs.own(i)
    }
    /// Evaluate `t` in the task's context: its reads (and the read sets of its memo hits) are recorded.
    pub fn eval(&mut self, t: Term) -> Res<Term> {
        let r0 = self.cx.reads;
        let r = self.ev.eval(self.cx, t);
        if !self.ev.cfg.track_conds {
            self.cx.reads = r0;
        }
        r
    }
    pub fn is_true(&self, t: Term) -> bool {
        t == self.ev.s.true_
    }
}

enum Step {
    Done(Term),
    Again(Term),
    Decline,
}

/// The placeholder for an element a speculative task could not complete (never a valid id: the arena does not
/// allocate its last segment).
pub const HOLE: Term = Term(0x7FFF_FFFF);

/// Definitions recorded for read-set validation; past this, older stamps are distrusted.
const MAX_EPOCHS: usize = 1 << 20;
const FLUSH: u64 = 1 << 12;

/// A rule that `defrule(k)` defines during an evaluation (the effect), prepared at setup.
pub struct RuleDef {
    pub lhs: Pat,
    pub rhs: Rhs,
    pub attach: Option<Term>,
}

pub struct Eval<'h> {
    pub heap: &'h Heap,
    pub defs: Defs,
    pub s: Syms,
    ops: Box<[Op]>,
    cost: Box<[AtomicU32]>,
    fx_global: u64,
    memo: Memo,
    epoch: AtomicU32,
    /// Mask bits of the symbol(s) defined at each epoch.
    changes: Box<[AtomicU64]>,
    out: Mutex<Vec<Term>>,
    stop: AtomicBool,
    fuel: AtomicU64,
    pub cfg: Config,
    /// Patterns (with their variable counts) for `cases(list, k)`.
    pub pats: Vec<(Pat, usize)>,
    /// Rules for `defrule(k)`.
    pub rule_table: Vec<RuleDef>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RunStats {
    pub splits: u64,
    pub redos: u64,
    pub fuel: u64,
}

fn step_lcg(x: i64) -> i64 {
    (x.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x3fff_ffff
}

/// Primality: trial division below 2^32 (at most 2^16 / 3 steps), deterministic Miller-Rabin above (bounded).
pub fn is_prime(n: i64) -> bool {
    if n < 2 {
        return false;
    }
    let n = n as u64;
    for p in [2u64, 3, 5, 7, 11, 13] {
        if n.is_multiple_of(p) {
            return n == p;
        }
    }
    if n < 1 << 32 {
        let mut d = 17u64;
        while d * d <= n {
            if n.is_multiple_of(d) || n.is_multiple_of(d + 2) {
                return false;
            }
            d += 6;
        }
        return true;
    }
    let mulm = |a: u64, b: u64| ((a as u128 * b as u128) % n as u128) as u64;
    let powm = |mut b: u64, mut e: u64| {
        let mut r = 1u64;
        while e > 0 {
            if e & 1 == 1 {
                r = mulm(r, b);
            }
            b = mulm(b, b);
            e >>= 1;
        }
        r
    };
    let (mut d, mut s) = (n - 1, 0);
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }
    'outer: for a in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        let mut x = powm(a, d);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mulm(x, x);
            if x == n - 1 {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

impl<'h> Eval<'h> {
    pub fn new(heap: &'h Heap, cfg: Config) -> Self {
        let mut ops = vec![Op::None; MAX_SYMS].into_boxed_slice();
        for &(name, op) in BUILTINS {
            let i = heap.sym_index(heap.sym(name)).unwrap();
            ops[i as usize] = op;
        }
        let s = Syms {
            list: heap.sym("list"),
            true_: heap.sym("true"),
            false_: heap.sym("false"),
            none: heap.sym("none"),
            null: heap.sym("null"),
            lp: heap.sym("loop"),
        };
        Eval {
            heap,
            defs: Defs::new(MAX_SYMS, cfg.index),
            s,
            ops,
            cost: (0..MAX_SYMS).map(|_| AtomicU32::new(0)).collect(),
            fx_global: ["print", "define", "defrule"].iter().fold(0, |m, n| m | heap.mask(heap.sym(n))),
            memo: Memo::new(cfg.memo_kind, cfg.memo_bits),
            epoch: AtomicU32::new(1),
            changes: (0..MAX_EPOCHS).map(|_| AtomicU64::new(0)).collect(),
            out: Mutex::new(vec![]),
            stop: AtomicBool::new(false),
            fuel: AtomicU64::new(0),
            cfg,
            pats: vec![],
            rule_table: vec![],
        }
    }

    // --- session-level definitions (between evaluations: `&mut self`) -------------------------------------------

    fn bump(&self, cx_epoch: u32, bits: u64) -> u32 {
        let e = cx_epoch + 1;
        if (e as usize) < MAX_EPOCHS {
            self.changes[e as usize].store(bits, Relaxed);
        }
        e
    }

    fn publish_epoch(&self, e: u32) {
        self.epoch.store(e, Release);
    }

    pub fn set_ac(&mut self, s: Term) {
        let i = self.heap.sym_index(s).expect("symbol");
        let e0 = *self.epoch.get_mut();
        let e = self.bump(e0, sym_bit(i));
        self.defs.set_ac(i);
        self.publish_epoch(e);
    }

    /// `s = v` (semantics 6.4), between evaluations.
    pub fn set_own(&mut self, s: Term, v: Term) {
        let i = self.heap.sym_index(s).expect("symbol");
        let e0 = *self.epoch.get_mut();
        let e = self.bump(e0, sym_bit(i));
        self.defs.set_own(i, v);
        self.publish_epoch(e);
    }

    /// `lhs := rhs` (semantics 6.4), between evaluations. With `attach = Some(s)` the rule is stored with `s`, the head
    /// of one of the left side's depth-one arguments (semantics 6.5).
    pub fn define(&mut self, lhs: Pat, rhs: Rhs, attach: Option<Term>) {
        let e0 = *self.epoch.get_mut();
        let owner = self.define_rule(lhs, rhs, attach);
        let e = self.bump(e0, sym_bit(owner));
        self.publish_epoch(e);
    }

    /// Compile and insert a rule; returns the owner's symbol index. Called by the session or by the frontier.
    fn define_rule(&self, lhs: Pat, rhs: Rhs, attach: Option<Term>) -> u32 {
        let st = self.heap;
        let lhs = self.compile(lhs);
        let top = match lhs.strip() {
            Pat::App { head, .. } => *head,
            Pat::Lit(t) => st.head(*t).expect("a rule's left side must be compound"),
            _ => panic!("a rule's left side must be compound"),
        };
        let ac_top = self.is_ac(top);
        let owner = attach.unwrap_or(top);
        if let Some(s) = attach {
            let ok = match lhs.strip() {
                Pat::App { args, .. } => args.iter().any(|a| a.fixed_head(st) == Some(s)),
                Pat::Lit(t) => st.args(*t).iter().any(|&a| st.head(a) == Some(s)),
                _ => false,
            };
            assert!(ok, "argument-head rules attach to the head of a depth-one argument");
        }
        let oi = st.sym_index(owner).expect("symbol");
        self.defs.define(st, oi, attach.map(|_| top), ac_top, Rule::new(lhs, rhs));
        oi
    }

    pub fn compile(&self, p: Pat) -> Pat {
        compile(self.heap, p, &|h| self.is_ac(h))
    }

    pub fn is_ac(&self, h: Term) -> bool {
        self.heap.sym_index(h).is_some_and(|i| self.defs.is_ac(i))
    }

    /// A safe point (`&mut self`: no evaluation is running): free what definitions retired.
    pub fn safe_point(&mut self) -> usize {
        self.defs.reclaim()
    }

    // --- inspection -----------------------------------------------------------------------------------------------

    pub fn output(&self) -> Vec<Term> {
        self.out.lock().clone()
    }
    pub fn value_of(&self, s: Term) -> Option<Term> {
        self.heap.sym_index(s).and_then(|i| self.defs.own(i))
    }
    pub fn version_of(&self, s: Term) -> u32 {
        self.heap.sym_index(s).map_or(0, |i| self.defs.version(i))
    }
    pub fn rules_of(&self, s: Term) -> Option<&RuleSet> {
        self.heap.sym_index(s).and_then(|i| self.defs.rules(i))
    }
    pub fn epoch(&self) -> u32 {
        self.epoch.load(Acquire)
    }
    pub fn stop_flag(&self) -> &AtomicBool {
        &self.stop
    }
    pub fn interrupt(&self) {
        self.stop.store(true, Relaxed);
    }

    // --- evaluation -----------------------------------------------------------------------------------------------

    /// Evaluate `t` as one top-level evaluation, on the frontier. Call it inside the pool it should use.
    pub fn run(&self, t: Term) -> (Res<Term>, RunStats) {
        let mut cx = self.top_cx(rayon::current_num_threads() as u32);
        let r = self.eval(&mut cx, t);
        let _ = self.flush(&mut cx);
        (r, RunStats { splits: cx.splits, redos: cx.redos, fuel: cx.fuel })
    }

    /// Apply the first matching rule at the top of `t` once, without evaluating anything (spike 2's
    /// `rewrite_top`), on the frontier of a fresh top-level context.
    pub fn rewrite_top(&self, t: Term) -> Res<Option<Term>> {
        let mut cx = self.top_cx(1);
        self.dispatch(&mut cx, t)
    }

    /// Run `f` with a fresh top-level (frontier) context: for matching outside an evaluation (tests).
    pub fn with_cx<R>(&self, f: impl FnOnce(&mut Cx) -> R) -> R {
        f(&mut self.top_cx(1))
    }

    fn top_cx(&self, threads: u32) -> Cx {
        Cx {
            front: true,
            epoch: self.epoch.load(Acquire),
            reads: 0,
            effects: 0,
            fuel: 0,
            unflushed: 0,
            threads,
            splits: 0,
            redos: 0,
            next_poll: 0,
            pool: vec![],
        }
    }

    #[inline]
    fn op(&self, h: Term) -> Op {
        match self.heap.view(h) {
            View::Sym(i) => self.ops.get(i as usize).copied().unwrap_or(Op::None),
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

    /// Draw this task's fuel from the shared budget and poll the stop flag (decision 4.9).
    fn flush(&self, cx: &mut Cx) -> Res<()> {
        let u = std::mem::take(&mut cx.unflushed);
        if self.stop.load(Relaxed) {
            return Err(Abort::Limit);
        }
        if self.fuel.fetch_add(u, Relaxed).saturating_add(u) > self.cfg.fuel_limit {
            self.stop.store(true, Relaxed);
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
        (from + 1..=to).any(|e| self.changes[e as usize].load(Relaxed) & deps != 0)
    }

    pub fn eval(&self, cx: &mut Cx, t: Term) -> Res<Term> {
        if t.0 & IMM != 0 {
            return Ok(t);
        }
        self.tick(cx)?;
        if self.cfg.memo {
            if let Some((v, e, deps)) = self.memo.get(t) {
                // the session-counter fast path, then the read-set check (semantics 4.3); a hit passes its read set
                // on (decision 2.7)
                if e == cx.epoch || (e < cx.epoch && !self.changed(e, cx.epoch, deps)) {
                    cx.reads |= deps;
                    return Ok(v);
                }
            }
        }
        let (reads0, effects0, epoch0, fuel0) = (cx.reads, cx.effects, cx.epoch, cx.fuel);
        cx.reads = 0;
        let r = self.eval_inner(cx, t);
        let mut deps = cx.reads;
        if let Ok(v) = r {
            // a normal form stays normal while none of its symbols (or the term's) gains a definition
            deps |= self.heap.mask(t) | self.heap.mask(v);
        }
        cx.reads = reads0 | deps;
        let v = r?;
        self.learn(t, cx.fuel - fuel0);
        // results that involve effects are never memoised (semantics 9); stamped with the start epoch (4.8)
        if self.cfg.memo && cx.effects == effects0 {
            self.memo.put(t, v, epoch0, deps);
            if v != t && v.0 & IMM == 0 {
                self.memo.put(v, v, epoch0, deps);
            }
        }
        Ok(v)
    }

    fn eval_inner(&self, cx: &mut Cx, mut t: Term) -> Res<Term> {
        let st = self.heap;
        loop {
            let (h, args) = match st.view(t) {
                View::Int(_) | View::Real(_) => return Ok(t),
                View::Sym(i) => {
                    cx.reads |= sym_bit(i);
                    return match self.defs.own(i) {
                        Some(v) if v != t => self.eval(cx, v),
                        _ => Ok(t),
                    };
                }
                View::App(h, args) => (h, args),
            };
            let op = self.op(h);
            // held arguments
            match (op, args.len()) {
                (Op::Define, 2) => {
                    if let Some(i) = st.sym_index(args[0]) {
                        return self.define_own(cx, i, args[1]);
                    }
                }
                (Op::Top, 1) => return Ok(self.dispatch(cx, args[0])?.unwrap_or(args[0])),
                (Op::Cases, 2) => {
                    if let Some(k) = st.as_int(args[1]).filter(|&k| (k as usize) < self.pats.len()) {
                        return self.cases(cx, args[0], k as usize);
                    }
                }
                _ => {}
            }
            let vals = self.eval_args(cx, t, args)?;
            if !(std::ptr::eq(&*vals, args) || *vals == *args) {
                t = if self.is_ac(h) { st.app_ac(h, &vals) } else { st.app(h, &vals) };
            }
            match self.apply(cx, op, t)? {
                Step::Done(v) => return Ok(v),
                Step::Again(u) => {
                    self.tick(cx)?;
                    t = u;
                    continue;
                }
                Step::Decline => {}
            }
            match self.dispatch(cx, t)? {
                Some(u) => {
                    self.tick(cx)?;
                    t = u;
                }
                None => return Ok(t),
            }
        }
    }

    /// The replacement from the first matching rule for `t`: argument-head rules of the arguments' heads in argument
    /// order, then the head's rules (semantics 4.1 step 3.5, 6.5). Consulting a symbol's rules is a read, even when
    /// it holds none (spike 2). Conditions run inside, on this task's context.
    pub fn dispatch(&self, cx: &mut Cx, t: Term) -> Res<Option<Term>> {
        let st = self.heap;
        let View::App(h, args) = st.view(t) else { return Ok(None) };
        let Some(hi) = st.sym_index(h) else { return Ok(None) };
        let mut sc = cx.pool.pop().unwrap_or_default();
        let r = self.dispatch_in(cx, t, h, hi, args, &mut sc);
        cx.pool.push(sc);
        r
    }

    fn dispatch_in(&self, cx: &mut Cx, t: Term, h: Term, hi: u32, args: &[Term], sc: &mut Scratch) -> Res<Option<Term>> {
        let st = self.heap;
        let found = {
            let mt = Matcher::new(self, cx, self.cfg.smart);
            let mut found = None;
            for &a in args {
                let Some(s) = st.head_sym(a) else { continue };
                mt.read(sym_bit(s));
                if !self.defs.has_arg_rules(s) {
                    continue;
                }
                if let Some(rs) = self.defs.arg_rules(s, h) {
                    found = rs.find(&mt, t, sc);
                    if let Some(a) = mt.stopped() {
                        return Err(a);
                    }
                    if found.is_some() {
                        break;
                    }
                }
            }
            if found.is_none() {
                mt.read(sym_bit(hi));
                if let Some(rs) = self.defs.rules(hi) {
                    found = rs.find(&mt, t, sc);
                    if let Some(a) = mt.stopped() {
                        return Err(a);
                    }
                }
            }
            found
        };
        Ok(found.map(|r| (r.rhs)(&sc.out, st)))
    }

    /// Does pattern `k` of the table match `t`? (`cases`.)
    fn match_pat(&self, cx: &mut Cx, k: usize, t: Term) -> Res<bool> {
        let (p, nv) = &self.pats[k];
        let mut sc = cx.pool.pop().unwrap_or_default();
        let r = {
            let mt = Matcher::new(self, cx, self.cfg.smart);
            let m = mt.matches(p, *nv, t, &mut sc.binds);
            match mt.stopped() {
                Some(a) => Err(a),
                None => Ok(m),
            }
        };
        cx.pool.push(sc);
        r
    }

    /// `cases(list, k)` with the list held: the elements that pattern `k` matches, selected in parallel.
    fn cases(&self, cx: &mut Cx, list: Term, k: usize) -> Res<Term> {
        let xs = self.heap.args(list);
        let none = self.s.none;
        let r = self.group_vec(cx, 1, xs.len(), &|ev: &Self, cx: &mut Cx, i| {
            ev.tick(cx)?;
            Ok(if ev.match_pat(cx, k, xs[i])? { xs[i] } else { none })
        })?;
        let sel: Vec<Term> = r.into_iter().filter(|&x| x != none).collect();
        Ok(self.heap.app(self.s.list, &sel))
    }

    #[inline]
    fn may_split(&self, cx: &Cx) -> bool {
        match self.cfg.split {
            Split::Never => false,
            Split::Lazy => cx.threads >= 2,
            Split::Always => true,
        }
    }

    #[inline]
    fn head_cost(&self, t: Term) -> u64 {
        match self.heap.head_sym(t) {
            Some(i) => self.cost.get(i as usize).map_or(0, |c| c.load(Relaxed) as u64),
            None => 0,
        }
    }

    #[inline]
    fn learn(&self, t: Term, fuel: u64) {
        let Some(i) = self.heap.head_sym(t) else { return };
        let Some(c) = self.cost.get(i as usize) else { return };
        let old = c.load(Relaxed) as u64;
        let new = (old * 3 + fuel.min(u32::MAX as u64)) / 4;
        if new.abs_diff(old) > old / 8 {
            c.store(new as u32, Relaxed);
        }
    }

    fn eval_args<'a>(&self, cx: &mut Cx, t: Term, args: &'a [Term]) -> Res<Cow<'a, [Term]>> {
        if args.iter().all(|a| a.0 & IMM != 0) {
            return Ok(Cow::Borrowed(args));
        }
        let mut out = vec![Term(0); args.len()];
        if args.len() >= 2 && self.may_split(cx) {
            let est = |i: usize| {
                if self.heap.mask(args[i]) & self.fx_global != 0 {
                    0
                } else {
                    (self.heap.size(args[i]) as u64).max(self.head_cost(args[i]))
                }
            };
            let hint = self.heap.size(t) as u64;
            let h = self.range(cx, 0, args.len(), hint, &mut out, &est, &|ev: &Self, cx: &mut Cx, i| ev.eval(cx, args[i]))?;
            if h {
                return Err(Abort::Global);
            }
        } else {
            for (o, &a) in out.iter_mut().zip(args) {
                *o = self.eval(cx, a)?;
            }
        }
        Ok(Cow::Owned(out))
    }

    /// Spike 4's range: evaluate elements `lo..hi` into `out`, splitting by the granularity rule; returns whether
    /// holes are left (only a speculative task leaves them).
    #[allow(clippy::too_many_arguments)]
    fn range<E, F>(&self, cx: &mut Cx, lo: usize, hi: usize, hint: u64, out: &mut [Term], est: &E, f: &F) -> Res<bool>
    where
        E: Fn(usize) -> u64 + Sync,
        F: Fn(&Self, &mut Cx, usize) -> Res<Term> + Sync,
    {
        let fuel0 = cx.fuel;
        let mut holes = false;
        let mut next = match self.cfg.split {
            Split::Never => u64::MAX,
            Split::Lazy if cx.threads < 2 => u64::MAX,
            Split::Lazy if hint < self.cfg.grain => cx.next_poll,
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
                cx.reads |= rc.reads;
                if cx.pool.len() < 8 {
                    cx.pool.append(&mut rc.pool);
                }
                let (lh, rh) = (rl?, rr?);
                if lh {
                    return Ok(true);
                }
                if !rh && !self.changed(epoch0, cx.epoch, rc.reads) {
                    return Ok(holes);
                }
                return Ok(holes | self.fill(cx, mid, right, epoch0, rc.reads, f)?);
            }
            match f(self, cx, i) {
                Ok(v) => out[i - lo] = v,
                Err(Abort::Limit) => return Err(Abort::Limit),
                Err(Abort::Global) => {
                    out[i - lo] = HOLE;
                    holes = true;
                }
            }
        }
        Ok(holes)
    }

    /// Complete `out` in order on this task: fill holes (if this task may) and re-evaluate elements whose read set a
    /// definition since `epoch0` touched (the memo answers the unaffected ones).
    #[allow(clippy::needless_range_loop)]
    fn fill<F>(&self, cx: &mut Cx, lo: usize, out: &mut [Term], epoch0: u32, reads: u64, f: &F) -> Res<bool>
    where
        F: Fn(&Self, &mut Cx, usize) -> Res<Term> + Sync,
    {
        for j in 0..out.len() {
            let is_hole = out[j] == HOLE;
            if !is_hole && !self.changed(epoch0, cx.epoch, reads) {
                continue;
            }
            if is_hole && !cx.front {
                return Ok(true);
            }
            cx.redos += 1;
            match f(self, cx, lo + j) {
                Ok(v) => out[j] = v,
                Err(Abort::Limit) => return Err(Abort::Limit),
                Err(Abort::Global) => {
                    out[j] = HOLE;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

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
        match self.cfg.split {
            Split::Never => false,
            Split::Always => true,
            Split::Lazy => {
                cx.next_poll = cx.fuel + self.cfg.poll;
                *next = cx.next_poll;
                let k = (hi - i).min(8);
                let (sum, pure) = (i..i + k).map(est).fold((0u64, 0u64), |(s, p), e| (s + e, p + (e > 0) as u64));
                let stat = sum * (hi - i) as u64 / k as u64;
                let done = (i - lo) as u64;
                let measured = (cx.fuel - fuel0).saturating_mul((hi - i) as u64).checked_div(done).unwrap_or(0);
                let measured = measured.saturating_mul(pure) / k as u64;
                measured.max(stat) >= self.cfg.grain && rayon_core::current_thread_has_pending_tasks() == Some(false)
            }
        }
    }

    /// `n` elements `f(0..n)` as a list, split by the granularity rule; `c` is the estimated cost of one element.
    fn group(&self, cx: &mut Cx, c: u64, n: usize, f: &(impl Fn(&Self, &mut Cx, usize) -> Res<Term> + Sync)) -> Res<Term> {
        Ok(self.heap.app(self.s.list, &self.group_vec(cx, c, n, f)?))
    }

    fn group_vec(&self, cx: &mut Cx, c: u64, n: usize, f: &(impl Fn(&Self, &mut Cx, usize) -> Res<Term> + Sync)) -> Res<Vec<Term>> {
        let mut out = vec![Term(0); n];
        if !self.may_split(cx) {
            for (i, o) in out.iter_mut().enumerate() {
                *o = f(self, cx, i)?;
            }
        } else {
            let est = |_| c;
            if self.range(cx, 0, n, c.saturating_mul(n as u64), &mut out, &est, f)? {
                return Err(Abort::Global);
            }
        }
        Ok(out)
    }

    // --- effects --------------------------------------------------------------------------------------------------

    /// `define(s, v)` during an evaluation: the frontier's effect.
    fn define_own(&self, cx: &mut Cx, i: u32, rhs: Term) -> Res<Term> {
        let v = self.eval(cx, rhs)?;
        if !cx.front {
            return Err(Abort::Global);
        }
        // only the frontier defines, so epochs advance in sequential order with one writer
        debug_assert_eq!(cx.epoch, self.epoch.load(Relaxed));
        let e = self.bump(cx.epoch, sym_bit(i));
        self.defs.set_own(i, v);
        self.publish_epoch(e);
        cx.epoch = e;
        cx.effects += 1;
        Ok(v)
    }

    /// `defrule(k)` during an evaluation: the frontier inserts rule `k` of the table into the live index while
    /// speculative tasks may be reading it.
    fn define_rule_effect(&self, cx: &mut Cx, k: usize) -> Res<Term> {
        if !cx.front {
            return Err(Abort::Global);
        }
        debug_assert_eq!(cx.epoch, self.epoch.load(Relaxed));
        let d = &self.rule_table[k];
        let owner = self.define_rule(d.lhs.clone(), d.rhs.clone(), d.attach);
        let e = self.bump(cx.epoch, sym_bit(owner));
        self.publish_epoch(e);
        cx.epoch = e;
        cx.effects += 1;
        Ok(self.s.null)
    }

    fn apply(&self, cx: &mut Cx, op: Op, t: Term) -> Res<Step> {
        let st = self.heap;
        let v = st.args(t);
        let int = |i: usize| v.get(i).and_then(|&x| st.as_int(x));
        let b = |x: bool| Ok(Step::Done(if x { self.s.true_ } else { self.s.false_ }));
        let done = |x: Term| Ok(Step::Done(x));
        match (op, v.len(), int(0), int(1)) {
            (Op::IAdd, 2, Some(x), Some(y)) => done(st.int(x.wrapping_add(y))),
            (Op::IMul, 2, Some(x), Some(y)) => done(st.int(x.wrapping_mul(y))),
            (Op::IMod, 2, Some(x), Some(m)) if m > 0 => done(st.int(x.rem_euclid(m))),
            (Op::Less, 2, Some(x), Some(y)) => b(x < y),
            (Op::IsPrime, 1, Some(x), _) => b(is_prime(x)),
            (Op::Plus, _, _, _) if v.iter().all(|&x| st.as_int(x).is_some()) => {
                done(st.int(v.iter().fold(0i64, |s, &x| s.wrapping_add(st.as_int(x).unwrap()))))
            }
            (Op::Loop, 2, Some(k), Some(x)) => {
                if k <= 0 {
                    return done(v[1]);
                }
                Ok(Step::Again(st.app(self.s.lp, &[st.int(k - 1), st.int(step_lcg(x))])))
            }
            (Op::Print, 1, _, _) => {
                if !cx.front {
                    return Err(Abort::Global);
                }
                cx.effects += 1;
                self.out.lock().push(v[0]);
                done(v[0])
            }
            (Op::DefRule, 1, Some(k), _) if (k as usize) < self.rule_table.len() => {
                Ok(Step::Done(self.define_rule_effect(cx, k as usize)?))
            }
            (Op::Map, 2, _, _) if st.head(v[1]) == Some(self.s.list) => {
                let (f, xs) = (v[0], st.args(v[1]));
                let c = st.sym_index(f).and_then(|i| self.cost.get(i as usize)).map_or(0, |c| c.load(Relaxed) as u64);
                let c = if st.mask(f) & self.fx_global != 0 { 0 } else { c.max(1) };
                done(self.group(cx, c, xs.len(), &|ev: &Self, cx: &mut Cx, i| {
                    ev.tick(cx)?;
                    ev.eval(cx, ev.heap.app(f, &[xs[i]]))
                })?)
            }
            _ => Ok(Step::Decline),
        }
    }
}
