//! A mini evaluator (semantics 4.1), enough for the workloads: innermost, all arguments strict, symbols with own
//! values and rule sets, AC heads canonicalised on construction, argument-head rules (semantics 6.5), fuel and the
//! interrupt, and memoised normal forms with stamps (semantics 4.3, 4.4).
//!
//! Read-set tracking (decision 13.1) has three settings, so that its cost and its necessity can be measured:
//!
//! - [`Track::Off`]: the stamp covers the symbols of the term only (spike 1). Fast, and wrong as soon as a rule
//!   reads something outside the term: redefining a global that a condition read leaves a stale normal form.
//! - [`Track::Mask`]: every lookup of a symbol's own value or rules (by the evaluator or by a condition) ORs the
//!   symbol's bit into a 64-bit read mask, the same Bloom-style mask spike 1 keeps per term. A stamp is the epoch
//!   plus the term's mask OR its read mask; a memo hit ORs the stored mask into the caller's. Collisions only cause
//!   extra re-evaluation.
//! - [`Track::Exact`]: the read set is the exact list of symbol indices, collected on a stack, sorted and
//!   deduplicated per evaluated term, and stored in an arena; validity compares each symbol's last-change epoch.
//!
//! In every setting a global definition counter that has not moved since the stamp validates it at once.
//!
//! The evaluator is recursive in the depth of the term (the workloads are shallow); the real one needs an explicit
//! stack (decision 1.6).
use crate::index::{Dispatch, OrderBy, Rhs, Rule, RuleSet, Scratch};
use crate::matcher::{Binds, Env, Matcher};
use crate::pat::{compile, Pat};
use crate::store::{sym_bit, Store, Term, View};
use crate::{Limit, Stop};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Track {
    Off,
    Mask,
    Exact,
}

#[derive(Clone, Copy, Debug)]
pub struct Cfg {
    pub memo: bool,
    pub track: Track,
    pub dispatch: Dispatch,
    pub smart: bool,
    /// See [`Matcher::rest`].
    pub rest: bool,
    pub order: OrderBy,
}

impl Default for Cfg {
    fn default() -> Cfg {
        Cfg { memo: false, track: Track::Off, dispatch: Dispatch::Indexed, smart: true, rest: false, order: OrderBy::Index }
    }
}

#[derive(Default)]
pub struct SymData {
    pub own: Option<Term>,
    pub rules: Option<RuleSet>,
    /// Argument-head rules held by this symbol, per outer head (semantics 6.5).
    pub arg_rules: Vec<(Term, RuleSet)>,
    pub ac: bool,
    /// Epoch of the last change to this symbol's definitions.
    pub changed_at: u32,
}

#[derive(Clone, Copy, Default)]
struct Memo {
    res: u32,
    /// 0: no entry
    epoch: u32,
    /// Off and Mask: the symbols the stamp covers
    mask: u64,
    /// Exact: the read set in the arena
    rs_off: u32,
    rs_len: u32,
}

/// The reads of the evaluation in progress.
#[derive(Default)]
struct Reads {
    mask: Cell<u64>,
    list: RefCell<Vec<u32>>,
}

impl Reads {
    #[inline]
    fn record(&self, track: Track, memo: bool, i: u32) {
        if !memo {
            return;
        }
        match track {
            Track::Off => {}
            Track::Mask => self.mask.set(self.mask.get() | sym_bit(i)),
            Track::Exact => self.list.borrow_mut().push(i),
        }
    }
}

/// What conditions see: own values, with reads recorded.
struct EvalEnv<'a> {
    st: &'a Store,
    syms: &'a [SymData],
    reads: &'a Reads,
    cfg: Cfg,
}

impl Env for EvalEnv<'_> {
    fn st(&self) -> &Store {
        self.st
    }
    fn value(&self, sym: Term) -> Option<Term> {
        let i = self.st.sym_index(sym)?;
        self.reads.record(self.cfg.track, self.cfg.memo, i);
        self.syms.get(i as usize).and_then(|s| s.own)
    }
}

pub struct Evaluator {
    pub st: Store,
    pub syms: Vec<SymData>,
    pub cfg: Cfg,
    pub limit: Limit,
    /// Rule applications and own-value steps (fuel, semantics 4.2).
    pub steps: u64,
    pub fuel: u64,
    memo: Vec<Memo>,
    rs_arena: Vec<u32>,
    epoch: u32,
    /// (epoch, mask bit) of every definition change, in epoch order (spike 1).
    changes: Vec<(u32, u64)>,
    reads: Reads,
    scratch: Scratch,
    out: Binds,
    argbuf: Vec<Term>,
}

impl Evaluator {
    pub fn new(st: Store, cfg: Cfg, limit: Limit) -> Evaluator {
        Evaluator {
            st,
            syms: vec![],
            cfg,
            limit,
            steps: 0,
            fuel: u64::MAX,
            memo: vec![],
            rs_arena: vec![],
            epoch: 1,
            changes: vec![],
            reads: Reads::default(),
            scratch: Scratch::default(),
            out: Binds::default(),
            argbuf: vec![],
        }
    }

    fn sd(&mut self, s: Term) -> &mut SymData {
        let i = self.st.sym_index(s).expect("not a symbol") as usize;
        if self.syms.len() <= i {
            self.syms.resize_with(i + 1, SymData::default);
        }
        &mut self.syms[i]
    }

    fn changed(&mut self, s: Term) {
        self.epoch += 1;
        let e = self.epoch;
        let i = self.st.sym_index(s).unwrap();
        self.changes.push((e, sym_bit(i)));
        self.sd(s).changed_at = e;
    }

    pub fn set_ac(&mut self, s: Term) {
        self.sd(s).ac = true;
        self.changed(s);
    }

    /// `s = v` (semantics 6.4).
    pub fn set_own(&mut self, s: Term, v: Term) {
        self.sd(s).own = Some(v);
        self.changed(s);
    }

    pub fn is_ac(&self, h: Term) -> bool {
        self.st.sym_index(h).and_then(|i| self.syms.get(i as usize)).is_some_and(|s| s.ac)
    }

    /// Compile a pattern against the current AC flags.
    pub fn compile(&mut self, p: Pat) -> Pat {
        // compiling builds literal terms (mutable store), so collect the AC heads first
        let st = &self.st;
        let ac_heads: Vec<Term> =
            self.syms.iter().enumerate().filter(|(_, s)| s.ac).map(|(i, _)| st.sym_term(i as u32)).collect();
        compile(&mut self.st, p, &|h| ac_heads.contains(&h))
    }

    /// `lhs := rhs` (semantics 6.4). With `attach = Some(s)`, the rule is stored with `s`, which must be the head of
    /// one of the left side's arguments (semantics 6.5); otherwise with the left side's head.
    pub fn define(&mut self, lhs: Pat, rhs: Rhs, attach: Option<Term>) -> Result<(), Stop> {
        let lhs = self.compile(lhs);
        let nvars = lhs.n_vars();
        let st = &self.st;
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
        let (order, limit) = (self.cfg.order, &self.limit);
        let i = self.st.sym_index(owner).unwrap() as usize;
        if self.syms.len() <= i {
            self.syms.resize_with(i + 1, SymData::default);
        }
        let sd = &mut self.syms[i];
        let rs = if attach.is_some() {
            match sd.arg_rules.iter().position(|(h, _)| *h == top) {
                Some(p) => &mut sd.arg_rules[p].1,
                None => {
                    sd.arg_rules.push((top, RuleSet::new(ac_top)));
                    &mut sd.arg_rules.last_mut().unwrap().1
                }
            }
        } else {
            sd.rules.get_or_insert_with(|| RuleSet::new(ac_top))
        };
        rs.insert(&self.st, Rule { lhs, rhs, nvars }, order, limit)?;
        self.changed(owner);
        Ok(())
    }

    pub fn rules(&self, s: Term) -> Option<&RuleSet> {
        self.st.sym_index(s).and_then(|i| self.syms.get(i as usize)).and_then(|d| d.rules.as_ref())
    }

    // --- memo ----------------------------------------------------------------------------------------------------

    fn stamp_valid(&self, e: &Memo) -> bool {
        if e.epoch == self.epoch {
            return true; // nothing was defined since: the fast path of semantics 4.3
        }
        match self.cfg.track {
            Track::Off | Track::Mask => {
                for &(ce, m) in self.changes.iter().rev() {
                    if ce <= e.epoch {
                        break;
                    }
                    if m & e.mask != 0 {
                        return false;
                    }
                }
                true
            }
            Track::Exact => self.rs_arena[e.rs_off as usize..(e.rs_off + e.rs_len) as usize]
                .iter()
                .all(|&i| self.syms.get(i as usize).is_none_or(|s| s.changed_at <= e.epoch)),
        }
    }

    fn memo_get(&self, t: Term) -> Option<Term> {
        if Store::is_imm(t) {
            return None;
        }
        let e = self.memo.get(t.0 as usize)?;
        if e.epoch == 0 || !self.stamp_valid(e) {
            return None;
        }
        // the caller's read set includes everything this result depended on
        match self.cfg.track {
            Track::Off => {}
            Track::Mask => self.reads.mask.set(self.reads.mask.get() | e.mask),
            Track::Exact => {
                self.reads.list.borrow_mut().extend_from_slice(&self.rs_arena[e.rs_off as usize..(e.rs_off + e.rs_len) as usize])
            }
        }
        Some(Term(e.res))
    }

    fn memo_put(&mut self, t: Term, r: Term, e: Memo) {
        for (k, res) in [(t, r), (r, r)] {
            if Store::is_imm(k) {
                continue;
            }
            if self.memo.len() <= k.0 as usize {
                self.memo.resize(self.st.len().max(k.0 as usize + 1), Memo::default());
            }
            self.memo[k.0 as usize] = Memo { res: res.0, ..e };
        }
    }

    // --- evaluation ----------------------------------------------------------------------------------------------

    /// Evaluate `t` to normal form.
    pub fn eval(&mut self, t: Term) -> Result<Term, Stop> {
        if !self.cfg.memo {
            return self.eval_inner(t);
        }
        if let Some(r) = self.memo_get(t) {
            return Ok(r);
        }
        let epoch = self.epoch;
        match self.cfg.track {
            Track::Off => {
                let r = self.eval_inner(t)?;
                let mask = self.st.mask(t) | self.st.mask(r);
                self.memo_put(t, r, Memo { epoch, mask, ..Memo::default() });
                Ok(r)
            }
            Track::Mask => {
                let saved = self.reads.mask.replace(0);
                let r = self.eval_inner(t)?;
                let mine = self.reads.mask.get() | self.st.mask(t) | self.st.mask(r);
                self.reads.mask.set(saved | mine);
                self.memo_put(t, r, Memo { epoch, mask: mine, ..Memo::default() });
                Ok(r)
            }
            Track::Exact => {
                let mark = self.reads.list.borrow().len();
                let r = self.eval_inner(t)?;
                let (off, len) = {
                    let mut list = self.reads.list.borrow_mut();
                    let seg = &mut list[mark..];
                    seg.sort_unstable();
                    let mut w = 0;
                    for i in 0..seg.len() {
                        if i == 0 || seg[i] != seg[w - 1] {
                            seg[w] = seg[i];
                            w += 1;
                        }
                    }
                    list.truncate(mark + w);
                    let off = self.rs_arena.len();
                    self.rs_arena.extend_from_slice(&list[mark..]);
                    (off as u32, w as u32)
                };
                self.memo_put(t, r, Memo { epoch, rs_off: off, rs_len: len, ..Memo::default() });
                Ok(r)
            }
        }
    }

    fn read(&self, i: u32) {
        self.reads.record(self.cfg.track, self.cfg.memo, i);
    }

    fn step(&mut self) -> Result<(), Stop> {
        self.steps += 1;
        if self.steps > self.fuel || self.limit.tick() {
            return Err(Stop);
        }
        Ok(())
    }

    fn eval_inner(&mut self, mut t: Term) -> Result<Term, Stop> {
        loop {
            if self.limit.tick() {
                return Err(Stop);
            }
            let (h, n) = match self.st.view(t) {
                View::Int(_) | View::Real(_) => return Ok(t),
                View::Sym(i) => {
                    self.read(i);
                    match self.syms.get(i as usize).and_then(|s| s.own) {
                        Some(v) => {
                            self.step()?;
                            t = v;
                            continue;
                        }
                        None => return Ok(t),
                    }
                }
                View::App(h, args) => (h, args.len()),
            };
            // arguments, left to right, all strict
            let base = self.argbuf.len();
            let mut changed = false;
            for k in 0..n {
                let a = self.st.args(t)[k];
                let ea = match self.eval(a) {
                    Ok(v) => v,
                    Err(s) => {
                        self.argbuf.truncate(base);
                        return Err(s);
                    }
                };
                changed |= ea != a;
                self.argbuf.push(ea);
            }
            let hi = self.st.sym_index(h);
            if changed {
                let ac = hi.is_some_and(|i| self.syms.get(i as usize).is_some_and(|s| s.ac));
                t = if ac { self.st.app_ac(h, &self.argbuf[base..]) } else { self.st.app(h, &self.argbuf[base..]) };
            }
            self.argbuf.truncate(base);
            let Some(hi) = hi else { return Ok(t) };
            match self.dispatch(t, h, hi) {
                Some(rhs) => {
                    self.step()?;
                    t = rhs(&self.out, &mut self.st);
                }
                None => {
                    if self.limit.hit() {
                        return Err(Stop);
                    }
                    return Ok(t);
                }
            }
        }
    }

    /// Apply the first matching rule at the top of `t` once, without evaluating anything (R4: matching and
    /// dispatch only).
    pub fn rewrite_top(&mut self, t: Term) -> Result<Option<Term>, Stop> {
        let Some(h) = self.st.head(t) else { return Ok(None) };
        let Some(hi) = self.st.sym_index(h) else { return Ok(None) };
        match self.dispatch(t, h, hi) {
            Some(rhs) => Ok(Some(rhs(&self.out, &mut self.st))),
            None if self.limit.hit() => Err(Stop),
            None => Ok(None),
        }
    }

    /// Find the first rule for `t` (head `h`, symbol index `hi`): argument-head rules of the arguments' heads in
    /// argument order, then the head's rules (semantics 4.1 step 3.5, 6.5). Bindings go to `self.out`.
    fn dispatch(&mut self, t: Term, h: Term, hi: u32) -> Option<Rhs> {
        let Evaluator { st, syms, cfg, limit, reads, scratch, out, .. } = self;
        let env = EvalEnv { st, syms, reads, cfg: *cfg };
        let mt = Matcher::new(&env, cfg.smart, limit).with_rest(cfg.rest);
        let syms: &[SymData] = syms;
        for &a in st.args(t) {
            let Some(s) = st.head_sym(a) else { continue };
            // consulting the argument head's rules is a read even when it holds none for `h`
            reads.record(cfg.track, cfg.memo, s);
            let Some(sd) = syms.get(s as usize) else { continue };
            if sd.arg_rules.is_empty() {
                continue;
            }
            if let Some((_, rs)) = sd.arg_rules.iter().find(|(oh, _)| *oh == h) {
                if let Some(id) = rs.find(&mt, t, cfg.dispatch, scratch, out) {
                    return Some(rs.rules[id as usize].rhs.clone());
                }
                if limit.hit() {
                    return None;
                }
            }
        }
        reads.record(cfg.track, cfg.memo, hi);
        let rs = syms.get(hi as usize)?.rules.as_ref()?;
        let id = rs.find(&mt, t, cfg.dispatch, scratch, out)?;
        Some(rs.rules[id as usize].rhs.clone())
    }
}
