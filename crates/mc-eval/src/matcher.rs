//! Matching (semantics 6.1, 6.2) by backtracking in continuation-passing style: `m(p, t, b, k)` tries every way `p`
//! matches `t` in the enumeration order, extending the bindings `b`, and calls the continuation `k` for each; it
//! stops at the first `k` that returns true. Bindings are undone on the way back, so a caller that wants the first
//! match copies them inside its continuation. This one shape handles nested AC patterns, sequence variables,
//! alternatives and conditions uniformly: a later failure resumes the enumeration of an earlier, nested match.
//!
//! **The pruning AC strategy** (spike 2, decision 2.1, after Eker 1995): before the search, a candidate set per
//! single pattern argument (literals by identity, fixed-head patterns by head and an independent match test, typed
//! variables by type, bound variables by identity), then a bipartite-matching (Hall) test that every single pattern
//! can get a distinct candidate; during the search, candidates only, a per-step test that every remaining single
//! pattern still has an unused candidate, the last sequence variable takes the rest without enumeration, and of
//! identical subject arguments only the first unused copy is tried. Every pruning step only removes branches without
//! solutions and the walk follows the enumeration order, so the first match is the one the order defines (the unit
//! tests compare it with a naive enumeration).
//!
//! The matcher runs inside an evaluation task and holds the task's context, lent to conditions one at a
//! time, so a condition can evaluate (I2.2, I2.3). A condition that aborts (a limit, an effect, a hole) stops the
//! whole enumeration at once and reports why, so no later alternative or rule is tried on a
//! half-checked match. Matching steps cost fuel (decision F.5) and poll the control block through it.
use crate::eval::{Abort, CondCx, Cx, Eval};
use crate::pat::{has_type, AppInfo, Cond, CondCode, Pat, Ty};
use mc_core::{Store, View, T};
use std::cell::{Cell, RefCell};

/// The value of a pattern variable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Val<'h> {
    /// not bound (yet)
    Unbound,
    /// a single term
    One(T<'h>),
    /// a sequence: offset and length in the bindings' sequence buffer
    Seq(u32, u32),
}

/// Pattern-variable bindings. Sequences live in one shared buffer used as a stack, so binding a sequence allocates
/// nothing once the buffer has grown.
#[derive(Clone, Default, Debug)]
pub struct Binds<'h> {
    vals: Vec<Val<'h>>,
    seq: Vec<T<'h>>,
    /// per sequence variable: the associative-commutative head whose arguments it took, if any (semantics 6.2:
    /// such a run stands for that head applied to it, not for a list)
    wrap: Vec<Option<T<'h>>>,
    /// while matching the arguments of an AC call: its head
    ac_head: Option<T<'h>>,
}

impl<'h> Binds<'h> {
    pub(crate) fn reset(&mut self, n: usize) {
        self.vals.clear();
        self.vals.resize(n, Val::Unbound);
        self.seq.clear();
        self.wrap.clear();
        self.wrap.resize(n, None);
        self.ac_head = None;
    }
    /// The value of variable `v`.
    pub fn get(&self, v: u16) -> Val<'h> {
        self.vals.get(v as usize).copied().unwrap_or(Val::Unbound)
    }
    /// The term bound to a single variable. Panics if `v` is not bound to one term.
    pub fn one(&self, v: u16) -> T<'h> {
        match self.get(v) {
            Val::One(t) => t,
            // invariant: right-hand sides read a variable by the kind its pattern binds (documented panic)
            other => panic!("variable {v} is not bound to one term: {other:?}"),
        }
    }
    /// The terms bound to a sequence variable. Panics if `v` is not bound to a sequence.
    pub fn seq(&self, v: u16) -> &[T<'h>] {
        match self.get(v) {
            Val::Seq(o, n) => &self.seq[o as usize..(o + n) as usize],
            // invariant: right-hand sides read a variable by the kind its pattern binds (documented panic)
            other => panic!("variable {v} is not bound to a sequence: {other:?}"),
        }
    }
    /// The terms of a sequence binding `Val::Seq(o, n)`.
    pub fn run(&self, o: u32, n: u32) -> &[T<'h>] {
        &self.seq[o as usize..(o + n) as usize]
    }
    /// For a sequence variable that took arguments of an associative-commutative call: that call's head. Its value
    /// is then the head applied to the run (`add(b, c)`), not a list (semantics 6.2).
    pub fn seq_head(&self, v: u16) -> Option<T<'h>> {
        self.wrap.get(v as usize).copied().flatten()
    }
    /// Number of variable slots.
    pub fn len(&self) -> usize {
        self.vals.len()
    }
    /// No variable slots?
    pub fn is_empty(&self) -> bool {
        self.vals.is_empty()
    }
    /// Copy without giving up `self`'s capacity.
    pub(crate) fn copy_from(&mut self, o: &Binds<'h>) {
        self.vals.clone_from(&o.vals);
        self.seq.clone_from(&o.seq);
        self.wrap.clone_from(&o.wrap);
        self.ac_head = o.ac_head;
    }
}

/// The continuation: called with the bindings of each match found; returning true stops the enumeration.
type K<'k, 'h> = &'k mut dyn FnMut(&mut Binds<'h>) -> bool;

/// Bitmask of the first `n` positions.
#[inline]
fn all(n: usize) -> u64 {
    if n == 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    }
}

/// AC argument sets are 64-bit masks: an AC subject with more arguments does not match any AC pattern (decision F.8,
/// a known gap).
pub(crate) const MAX_AC: usize = 64;
/// Most arguments of an AC pattern (candidate sets live on the stack).
pub(crate) const MAX_AC_PAT: usize = 32;
/// Matching steps between fuel charges; each charge is [`MATCH_FUEL`] units.
const MATCH_POLL: u32 = 1024;
/// Fuel charged per [`MATCH_POLL`] matching steps.
const MATCH_FUEL: u64 = 64;

/// A matcher bound to one evaluation task.
pub(crate) struct Matcher<'a, 'k, 'h> {
    ev: &'a Eval<'k, 'h>,
    /// The task's context, lent to conditions one at a time (the CPS frames never use it themselves).
    cx: RefCell<&'a mut Cx<'h>>,
    /// Set when a condition aborted or a limit fired: the enumeration unwinds and the caller reports it.
    stop: Cell<Option<Abort>>,
    ticks: Cell<u32>,
    /// The naive enumeration, as the model of the unit tests.
    #[cfg(test)]
    naive: bool,
}

impl<'a, 'k, 'h> Matcher<'a, 'k, 'h> {
    pub(crate) fn new(ev: &'a Eval<'k, 'h>, cx: &'a mut Cx<'h>) -> Self {
        Matcher {
            ev,
            cx: RefCell::new(cx),
            stop: Cell::new(None),
            ticks: Cell::new(0),
            #[cfg(test)]
            naive: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn naive(mut self) -> Self {
        self.naive = true;
        self
    }

    /// Why the last search stopped early, if it did.
    #[inline]
    pub(crate) fn stopped(&self) -> Option<Abort> {
        self.stop.get()
    }

    /// Record a read in the task's read set (rule-set lookups by dispatch).
    #[inline]
    pub(crate) fn read(&self, bits: u64) {
        self.cx.borrow_mut().reads |= bits;
    }

    /// One step; every [`MATCH_POLL`] steps charge fuel (which also polls the control block).
    #[inline]
    fn tick(&self) -> bool {
        if self.stop.get().is_some() {
            return true;
        }
        let t = self.ticks.get().wrapping_add(1);
        self.ticks.set(t);
        if t.is_multiple_of(MATCH_POLL) {
            if let Err(a) = self.ev.spend(&mut self.cx.borrow_mut(), MATCH_FUEL) {
                self.stop.set(Some(a));
                return true;
            }
        }
        false
    }

    /// Run a condition: `Some(holds)`, or `None` after an abort (recorded in `stop`). Effects inside it are refused
    /// (A.6): the context counts how deep in conditions it is.
    fn cond(&self, c: &Cond, b: &Binds<'h>) -> Option<bool> {
        let mut g = self.cx.borrow_mut();
        if g.in_cond == 0 {
            // cells allocated before this point are read-only inside the condition (crate::cells)
            g.cond_born = self.ev.k.cells.born();
        }
        g.in_cond += 1;
        let r = {
            let mut cc = CondCx { ev: self.ev, cx: &mut g, binds: b, vars: &c.vars };
            match &c.code {
                CondCode::Native(f) => f(&mut cc),
                CondCode::Term { body, names } => cc.term_condition(*body, names),
            }
        };
        g.in_cond -= 1;
        match r {
            Ok(v) => Some(v),
            Err(a) => {
                self.stop.set(Some(a));
                None
            }
        }
    }

    /// The first match of `p` against `t`, copied into `out`. `scratch` is a reusable work area. Returns false when
    /// there is no match or the search stopped (check [`Matcher::stopped`]).
    pub(crate) fn first(&self, p: &Pat, nvars: usize, t: T<'h>, scratch: &mut Binds<'h>, out: &mut Binds<'h>) -> bool {
        scratch.reset(nvars);
        let found = self.m(p, t, scratch, &mut |b| {
            out.copy_from(b);
            true
        });
        found && self.stop.get().is_none()
    }

    /// Every match of `p` against `t` in the enumeration order, each given to `f` (which returns true to stop).
    pub(crate) fn each(&self, p: &Pat, nvars: usize, t: T<'h>, scratch: &mut Binds<'h>, f: &mut dyn FnMut(&Binds<'h>) -> bool) {
        scratch.reset(nvars);
        self.m(p, t, scratch, &mut |b| f(b));
    }

    /// Does `p` match `t` at all?
    pub(crate) fn matches(&self, p: &Pat, nvars: usize, t: T<'h>, scratch: &mut Binds<'h>) -> bool {
        scratch.reset(nvars);
        self.m(p, t, scratch, &mut |_| true) && self.stop.get().is_none()
    }

    fn bind_one(&self, var: Option<u16>, t: T<'h>, b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let Some(v) = var else { return k(b) };
        match b.vals[v as usize] {
            Val::Unbound => {
                b.vals[v as usize] = Val::One(t);
                let r = k(b);
                b.vals[v as usize] = Val::Unbound;
                r
            }
            Val::One(u) => u == t && k(b),
            Val::Seq(..) => false,
        }
    }

    /// Bind a sequence variable to `ts` (an argument run, or the chosen arguments under an AC head).
    fn bind_seq(&self, var: Option<u16>, ts: &mut dyn Iterator<Item = T<'h>>, b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let Some(v) = var else { return k(b) };
        match b.vals[v as usize] {
            Val::Unbound => {
                let off = b.seq.len();
                b.seq.extend(ts);
                b.vals[v as usize] = Val::Seq(off as u32, (b.seq.len() - off) as u32);
                b.wrap[v as usize] = b.ac_head;
                let r = k(b);
                b.vals[v as usize] = Val::Unbound;
                b.wrap[v as usize] = None;
                b.seq.truncate(off);
                r
            }
            Val::Seq(o, n) => {
                let old = &b.seq[o as usize..(o + n) as usize];
                let mut i = 0;
                let mut same = true;
                for t in ts {
                    if i >= old.len() || old[i] != t {
                        same = false;
                        break;
                    }
                    i += 1;
                }
                same && i == old.len() && k(b)
            }
            Val::One(_) => false,
        }
    }

    /// Match `p` against `t`, calling `k` for each match.
    fn m(&self, p: &Pat, t: T<'h>, b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        if self.tick() {
            return true; // stop everything; callers check `stopped`
        }
        let h = self.ev.heap;
        match p {
            Pat::Blank { var, ty } => {
                if let Some(ty) = ty {
                    if !has_type(h, t, *ty) {
                        return false;
                    }
                }
                self.bind_one(*var, t, b, k)
            }
            Pat::Lit(l) => l.get(h) == t && k(b),
            Pat::App { info, args, .. } if info.curried => self.m_curried(args, info, t, b, k),
            Pat::App { head, args, info } => {
                let View::App { head: th, args: ts } = h.view(t) else {
                    return info.has_opt && self.m_unit(head.get(h), args, t, b, k);
                };
                if th != head.get(h) {
                    return info.has_opt && self.m_unit(head.get(h), args, t, b, k);
                }
                if !info.has_seq {
                    return if info.ac { self.m_ac(args, info, ts, b, k) } else { ts.len() == args.len() && self.m_fixed(args, ts, b, k) };
                }
                // sequence variables bound inside this call remember its head if it is AC; the continuation runs
                // outside the call, so it sees the outer head again
                let outer = b.ac_head;
                b.ac_head = if info.ac { Some(th) } else { None };
                let k = &mut |b: &mut Binds<'h>| {
                    let inner = b.ac_head;
                    b.ac_head = outer;
                    let r = k(b);
                    b.ac_head = inner;
                    r
                };
                let r = if info.ac {
                    self.m_ac(args, info, ts, b, k)
                } else if info.flat {
                    ts.len() >= info.min as usize && self.m_flat(th, args, ts, b, k)
                } else {
                    ts.len() >= info.min as usize && self.m_seq(args, ts, b, k)
                };
                b.ac_head = outer;
                r
            }
            Pat::Alt(ps) => ps.iter().any(|q| self.m(q, t, b, k)),
            Pat::Named(v, q) => self.m(q, t, b, &mut |b| self.bind_one(Some(*v), t, b, k)),
            Pat::Cond(q, c) => self.m(q, t, b, &mut |b| match self.cond(c, b) {
                Some(true) => k(b),
                Some(false) => false,
                None => true, // aborted: unwind
            }),
            Pat::KeyValue(ps) => match self.entries(t) {
                Some(es) if es.len() <= 64 => self.m_kv(ps, es, 0, b, k),
                _ => false,
            },
            Pat::Opt { p, .. } => self.m(p, t, b, k),
            // a run pattern outside an argument list takes the one term (validation keeps it in argument lists)
            Pat::Repeat { p, min, max, .. } => *min <= 1 && 1 <= *max && self.m(p, t, b, k),
            Pat::Seq { .. } => false,
        }
    }

    /// The entries of an association `assoc(k -> v, ...)` or of a list whose elements are all rules.
    fn entries(&self, t: T<'h>) -> Option<&'h [T<'h>]> {
        let h = self.ev.heap;
        if h.packed(t).is_some() {
            return None;
        }
        let View::App { head, args } = h.view(t) else { return None };
        let rule = |x: T<'h>| {
            h.packed(x).is_none()
                && matches!(h.view(x), View::App { head, args: [_, _] }
                    if head == h.sym_term(h.sym(mc_api::names::RULE)) || head == h.sym_term(h.sym(mc_api::names::RULE_DELAYED)))
        };
        if head == h.sym_term(h.sym("assoc")) || (head == h.sym_term(self.ev.k.names.list) && args.iter().all(|&x| rule(x))) {
            return Some(args);
        }
        None
    }

    /// The patterns `ps` matched by distinct entries of `es` (`used`: the entries taken), in any order.
    fn m_kv(&self, ps: &[Pat], es: &'h [T<'h>], used: u64, b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let Some((p, rest)) = ps.split_first() else { return k(b) };
        for (j, &e) in es.iter().enumerate() {
            if used & (1 << j) != 0 {
                continue;
            }
            if self.tick() {
                return true;
            }
            if self.m(p, e, b, &mut |b| self.m_kv(rest, es, used | (1 << j), b, k)) {
                return true;
            }
        }
        false
    }

    /// Every term of `ts` matching `q`, in order, the bindings shared (a variable binds one term in all).
    fn m_rep(&self, q: &Pat, ts: &[T<'h>], b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        match ts.split_first() {
            None => k(b),
            Some((&t, rest)) => self.m(q, t, b, &mut |b| self.m_rep(q, rest, b, k)),
        }
    }

    /// Arguments one to one (no sequence variables).
    fn m_fixed(&self, ps: &[Pat], ts: &[T<'h>], b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        match ps.split_first() {
            None => k(b),
            Some((p, rest)) => self.m(p, ts[0], b, &mut |b| self.m_fixed(rest, &ts[1..], b, k)),
        }
    }

    /// Arguments with sequence variables, left to right; a sequence variable tries the shortest run first.
    /// The arguments of a call of a flat (associative only) head `head`: each single argument pattern matches one
    /// argument or, in order, a run of two or more wrapped in `head` (`ff(x_, y_)` on `ff(1, 2, 3)` is `x = 1`,
    /// `y = ff(2, 3)`); the shortest runs first. Sequence, optional and repeated patterns match as in `m_seq`.
    fn m_flat(&self, head: T<'h>, ps: &[Pat], ts: &[T<'h>], b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let Some((p, rest)) = ps.split_first() else { return ts.is_empty() && k(b) };
        if p.is_seq() || matches!(p, Pat::Opt { .. }) || rest.iter().any(|q| q.is_seq() || matches!(q, Pat::Opt { .. })) {
            return self.m_seq(ps, ts, b, k);
        }
        if ts.len() < ps.len() {
            return false;
        }
        let most = ts.len() - rest.len();
        let lo = if rest.is_empty() { most } else { 1 };
        let h = self.ev.heap;
        for len in lo..=most {
            if self.tick() {
                return true;
            }
            let t = if len == 1 { ts[0] } else { h.app(head, &ts[..len]) };
            if self.m(p, t, b, &mut |b| self.m_flat(head, rest, &ts[len..], b, k)) {
                return true;
            }
        }
        false
    }

    fn m_seq(&self, ps: &[Pat], ts: &[T<'h>], b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let marked = |p: &Pat| matches!(p, Pat::Seq { mark: Some(_), .. } | Pat::Repeat { mark: Some(_), .. });
        if ps.iter().any(marked) && !ps.iter().any(|p| matches!(p, Pat::Opt { .. })) {
            let mut lens = vec![0; ps.len()];
            let order = marked_order(ps);
            return self.m_marked(ps, ts, &order, 0, 0, &mut lens, b, k);
        }
        let Some((p, rest)) = ps.split_first() else { return ts.is_empty() && k(b) };
        if let Pat::Opt { p: q, var, default } = p {
            if q.is_seq() {
                // an optional run: the run first (at least its minimum length), then absent (r18c14.1)
                let mut present: Vec<Pat> = Vec::with_capacity(ps.len());
                present.push((**q).clone());
                present.extend(rest.iter().cloned());
                if self.m_seq(&present, ts, b, k) {
                    return true;
                }
                if self.stop.get().is_some() {
                    return true;
                }
                // absent: the run of the one default term, as for a definition's optional run (C9B2.1)
                let d = default.get(self.ev.heap);
                return self.bind_seq(Some(*var), &mut std::iter::once(d), b, &mut |b| self.m_seq(rest, ts, b, k));
            }
            // present first, then absent with the variable bound to the default
            let rest_min: usize = rest.iter().map(Pat::min_len).sum();
            if ts.len() > rest_min && self.m(q, ts[0], b, &mut |b| self.m_seq(rest, &ts[1..], b, k)) {
                return true;
            }
            let d = default.get(self.ev.heap);
            return self.bind_one(Some(*var), d, b, &mut |b| self.m_seq(rest, ts, b, k));
        }
        if let Pat::Repeat { p: q, min, max, .. } = p {
            let rest_min: usize = rest.iter().map(Pat::min_len).sum();
            if ts.len() < rest_min + *min as usize {
                return false;
            }
            let most = ts.len() - rest_min;
            let hi = most.min(*max as usize);
            // with no sequence pattern after this one, only one length can work
            let lo = if rest.iter().any(Pat::is_seq) { *min as usize } else { most };
            for len in lo..=hi {
                if self.tick() {
                    return true;
                }
                if self.m_rep(q, &ts[..len], b, &mut |b| self.m_seq(rest, &ts[len..], b, k)) {
                    return true;
                }
            }
            return false;
        }
        if let Pat::Seq { var, min, ty, .. } = p {
            let rest_min: usize = rest.iter().map(Pat::min_len).sum();
            if ts.len() < rest_min + *min as usize {
                return false;
            }
            let longest = ts.len() - rest_min;
            // with no sequence variable after this one, only one length can work
            let lo = if rest.iter().any(Pat::is_seq) { *min as usize } else { longest };
            // a typed run: no longer than the arguments of that type from here
            let h = self.ev.heap;
            let max = match ty {
                Some(ty) => ts[..longest].iter().take_while(|&&t| has_type(h, t, *ty)).count(),
                None => longest,
            };
            for len in lo..=max {
                if self.tick() {
                    return true;
                }
                let run = &ts[..len];
                if self.bind_seq(*var, &mut run.iter().copied(), b, &mut |b| self.m_seq(rest, &ts[len..], b, k)) {
                    return true;
                }
            }
            false
        } else {
            !ts.is_empty() && self.m(p, ts[0], b, &mut |b| self.m_seq(rest, &ts[1..], b, k))
        }
    }

    /// Arguments with marked runs (`longest(xs__)`, `shortest(xs__)`, decision r30.4): the run lengths are chosen
    /// in decision order (`order`: the marked runs by priority, higher first and ties left to right, each trying
    /// its preferred lengths first; then the other patterns left to right, runs shortest first as in `m_seq`), and
    /// the arguments are matched on the slices those lengths give. The first complete match in that order is the
    /// result: a marked run is as long (short) as any match allows before an unmarked one is considered.
    #[allow(clippy::too_many_arguments)]
    fn m_marked(&self, ps: &[Pat], ts: &[T<'h>], order: &[usize], d: usize, used: usize, lens: &mut Vec<usize>, b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let n = ts.len();
        if d == order.len() {
            if used != n {
                return false;
            }
            let lens = lens.clone();
            return self.m_slices(ps, ts, &lens, b, k);
        }
        let i = order[d];
        let (lo, hi, long) = run_bounds(&ps[i], n);
        // what the patterns not chosen yet need and allow
        let (mut rest_lo, mut rest_hi) = (0usize, 0usize);
        for &j in &order[d + 1..] {
            let (l, h, _) = run_bounds(&ps[j], n);
            rest_lo += l;
            rest_hi = rest_hi.saturating_add(h);
        }
        let left = n - used;
        if left < rest_lo + lo {
            return false;
        }
        let first = lo.max(left.saturating_sub(rest_hi));
        let last = hi.min(left - rest_lo);
        if first > last {
            return false;
        }
        let mut try_len = |len: usize, b: &mut Binds<'h>, k: K<'_, 'h>| {
            lens[i] = len;
            self.m_marked(ps, ts, order, d + 1, used + len, lens, b, k)
        };
        if long {
            for len in (first..=last).rev() {
                if self.tick() || try_len(len, b, k) {
                    return true;
                }
            }
        } else {
            for len in first..=last {
                if self.tick() || try_len(len, b, k) {
                    return true;
                }
            }
        }
        false
    }

    /// The argument patterns `ps` against consecutive slices of `ts` of the lengths `lens`.
    fn m_slices(&self, ps: &[Pat], ts: &[T<'h>], lens: &[usize], b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let (Some((p, rest)), Some((&len, lrest))) = (ps.split_first(), lens.split_first()) else { return k(b) };
        let (run, tail) = ts.split_at(len);
        let h = self.ev.heap;
        match p {
            Pat::Seq { var, ty, .. } => {
                if let Some(ty) = ty {
                    if !run.iter().all(|&t| has_type(h, t, *ty)) {
                        return false;
                    }
                }
                self.bind_seq(*var, &mut run.iter().copied(), b, &mut |b| self.m_slices(rest, tail, lrest, b, k))
            }
            Pat::Repeat { p: q, .. } => self.m_rep(q, run, b, &mut |b| self.m_slices(rest, tail, lrest, b, k)),
            p => len == 1 && self.m(p, run[0], b, &mut |b| self.m_slices(rest, tail, lrest, b, k)),
        }
    }

    /// `hd(ps...)` against a term `t` that is not a call of `hd`, where some of `ps` are optional arguments: when
    /// every optional argument may be absent because its default is the unit of `hd` at its place (`add`: 0,
    /// `mul`: 1, a declared identity element, the exponent of `pow`: 1) and one single pattern `q` is left,
    /// `hd(q, unit, ...)` is the value of `q` itself, so `q` is matched against `t` with the optional variables
    /// bound to their defaults: `n_. * x_` matches `x` with `n = 1` (C9B2.2). A default that is not the unit
    /// does not match this way (`x + optional(c_, 5)` does not match `x`: `x + 5` is not `x`).
    fn m_unit(&self, hd: T<'h>, ps: &[Pat], t: T<'h>, b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let h = self.ev.heap;
        let View::Sym(s) = h.view(hd) else { return false };
        let mut q = None;
        for (i, p) in ps.iter().enumerate() {
            if !matches!(p, Pat::Opt { .. }) {
                if q.is_some() || p.is_seq() {
                    return false;
                }
                q = Some(i);
            }
        }
        let Some(qi) = q else { return false };
        let is = |n: &str| s == h.sym(n);
        let unit = if is("pow") {
            if ps.len() != 2 || qi != 0 {
                return false;
            }
            h.int(1)
        } else if is("add") {
            h.int(0)
        } else if is("mul") {
            h.int(1)
        } else {
            let Some(e) = self.ev.k.defs.get(s).and_then(|d| d.identity(h)) else { return false };
            e
        };
        if ps.iter().any(|p| matches!(p, Pat::Opt { default, .. } if default.get(h) != unit)) {
            return false;
        }
        self.bind_defaults(ps, b, &mut |b| self.m(&ps[qi], t, b, k))
    }

    /// `call$(hp, ps...)` against `t`: the evaluator's view `call$(head, args...)` argument by argument, or a call
    /// `head(args...)` with `hp` matched against its head and `ps` against its arguments (C9B2.4).
    fn m_curried(&self, ps: &[Pat], info: &AppInfo, t: T<'h>, b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let h = self.ev.heap;
        let View::App { head: th, args: ts } = h.view(t) else { return false };
        let Some((hp, rest)) = ps.split_first() else { return false };
        if th == h.sym_term(h.sym(crate::pat::CALL)) {
            return if info.has_seq { self.m_seq(ps, ts, b, k) } else { ts.len() == ps.len() && self.m_fixed(ps, ts, b, k) };
        }
        self.m(hp, th, b, &mut |b| {
            if info.has_seq {
                self.m_seq(rest, ts, b, k)
            } else {
                ts.len() == rest.len() && self.m_fixed(rest, ts, b, k)
            }
        })
    }

    /// Every optional argument among `ps` absent: its variable bound to its default.
    fn bind_defaults(&self, ps: &[Pat], b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        match ps.split_first() {
            None => k(b),
            Some((Pat::Opt { var, default, .. }, rest)) => {
                let d = default.get(self.ev.heap);
                self.bind_one(Some(*var), d, b, &mut |b| self.bind_defaults(rest, b, k))
            }
            Some((_, rest)) => self.bind_defaults(rest, b, k),
        }
    }

    /// Arguments of an AC head with optional arguments: the first one present (its pattern takes an argument
    /// like any single pattern), then absent (its variable bound to the default), each case matched with the
    /// remaining optional arguments decided the same way; the plain AC matcher runs on each resulting pattern list,
    /// put back in class order (C9B2.2).
    fn m_ac_opt(&self, ps: &[Pat], ts: &[T<'h>], b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let Some(i) = ps.iter().position(|p| matches!(p, Pat::Opt { .. })) else {
            let info = AppInfo {
                ac: true,
                flat: false,
                has_seq: ps.iter().any(Pat::is_seq),
                n_single: ps.iter().filter(|a| !a.is_seq()).count() as u16,
                min: ps.iter().map(Pat::min_len).sum::<usize>() as u16,
                has_opt: false,
                curried: false,
            };
            return self.m_ac(ps, &info, ts, b, k);
        };
        let Pat::Opt { p: q, var, default } = &ps[i] else { return false };
        let mut present: Vec<Pat> = ps.to_vec();
        present[i] = (**q).clone();
        present.sort_by_key(Pat::ac_class);
        if self.m_ac_opt(&present, ts, b, k) {
            return true;
        }
        if self.stop.get().is_some() {
            return true;
        }
        let mut absent: Vec<Pat> = ps.to_vec();
        absent.remove(i);
        let d = default.get(self.ev.heap);
        self.bind_one(Some(*var), d, b, &mut |b| self.m_ac_opt(&absent, ts, b, k))
    }

    /// Arguments of an AC head: `ts` is in canonical order, `ps` in class order.
    fn m_ac(&self, ps: &[Pat], info: &AppInfo, ts: &[T<'h>], b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        if info.has_opt {
            return self.m_ac_opt(ps, ts, b, k);
        }
        let n = ts.len();
        if n < info.min as usize || (!info.has_seq && n != info.n_single as usize) {
            return false;
        }
        if n > MAX_AC || ps.len() > MAX_AC_PAT {
            return false;
        }
        #[cfg(test)]
        if self.naive {
            return self.ac_naive(ps, ts, 0, b, k);
        }
        self.ac_smart_entry(ps, ts, b, k)
    }

    /// Enumerate subsets of the unused arguments for a sequence variable, in lexicographic order of their sorted
    /// positions with a prefix before its extensions ({} < {0} < {0,1} < {0,2} < {1} ...). `cands` is `Some` for
    /// the pruning strategy (it then skips duplicate copies and checks the remaining patterns' candidates).
    #[allow(clippy::too_many_arguments)]
    fn subsets(
        &self,
        var: Option<u16>,
        min: usize,
        rest: &[Pat],
        ts: &[T<'h>],
        used: u64,
        chosen: u64,
        from: usize,
        cands: Option<&[u64]>,
        allow: u64,
        b: &mut Binds<'h>,
        k: K<'_, 'h>,
    ) -> bool {
        if self.tick() {
            return true;
        }
        if chosen.count_ones() as usize >= min {
            let mut it = (0..ts.len()).filter(|j| chosen & (1 << j) != 0).map(|j| ts[j]);
            let ok = match cands {
                #[cfg(test)]
                None => self.bind_seq(var, &mut it, b, &mut |b| self.ac_naive(rest, ts, used | chosen, b, k)),
                #[cfg(not(test))]
                None => unreachable!("the naive strategy exists in tests only"),
                Some(c) => {
                    // `c` holds the candidates of this sequence variable followed by those of `rest`
                    let u = used | chosen;
                    let rc = &c[c.len() - rest.len()..];
                    self.feasible(rest, ts.len(), u, rc) && self.bind_seq(var, &mut it, b, &mut |b| self.ac_smart(rest, ts, rc, u, b, k))
                }
            };
            if ok {
                return true;
            }
        }
        for j in from..ts.len() {
            let taken = used | chosen;
            if taken & (1 << j) != 0 || allow & (1 << j) == 0 {
                continue;
            }
            // pruning: of identical arguments, take the first unused copy only (see ac_smart)
            if cands.is_some() && j > 0 && ts[j] == ts[j - 1] && taken & (1 << (j - 1)) == 0 {
                continue;
            }
            if self.subsets(var, min, rest, ts, used, chosen | (1 << j), j + 1, cands, allow, b, k) {
                return true;
            }
        }
        false
    }

    /// The arguments of type `ty` (every argument without a type).
    fn typed_mask(&self, ts: &[T<'h>], ty: Option<Ty>) -> u64 {
        let Some(ty) = ty else { return all(ts.len()) };
        let h = self.ev.heap;
        ts.iter().enumerate().filter(|(_, &t)| has_type(h, t, ty)).fold(0, |m, (j, _)| m | 1 << j)
    }

    #[cfg(test)]
    fn ac_naive(&self, ps: &[Pat], ts: &[T<'h>], used: u64, b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let Some((p, rest)) = ps.split_first() else { return used == all(ts.len()) && k(b) };
        if let Pat::Seq { var, min, ty, .. } = p {
            let allow = self.typed_mask(ts, *ty);
            return self.subsets(*var, *min as usize, rest, ts, used, 0, 0, None, allow, b, k);
        }
        for j in 0..ts.len() {
            if used & (1 << j) == 0 && self.m(p, ts[j], b, &mut |b| self.ac_naive(rest, ts, used | (1 << j), b, k)) {
                return true;
            }
        }
        false
    }

    fn ac_smart_entry(&self, ps: &[Pat], ts: &[T<'h>], b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let h = self.ev.heap;
        let n = ts.len();
        let mut cands = [0u64; MAX_AC_PAT];
        for (i, p) in ps.iter().enumerate() {
            cands[i] = all(n);
            if p.is_seq() {
                continue;
            }
            let mut c = 0u64;
            match p.strip() {
                Pat::Lit(l) => {
                    let l = l.get(h);
                    for (j, t) in ts.iter().enumerate() {
                        if *t == l {
                            c |= 1 << j;
                        }
                    }
                }
                Pat::Blank { var, ty } => {
                    let bound = var.map(|v| b.vals[v as usize]);
                    for (j, &t) in ts.iter().enumerate() {
                        let ok = match bound {
                            Some(Val::One(u)) => u == t,
                            Some(Val::Seq(..)) => false,
                            _ => ty.is_none_or(|ty| has_type(h, t, ty)),
                        };
                        if ok {
                            c |= 1 << j;
                        }
                    }
                }
                Pat::App { head, info, .. } => {
                    let head = head.get(h);
                    // the canonical order groups compound arguments by head: the same term as the previous one
                    // gets the same answer; with an optional argument a term of another head may match
                    for (j, &t) in ts.iter().enumerate() {
                        let ok = if j > 0 && ts[j - 1] == t {
                            c & (1 << (j - 1)) != 0
                        } else {
                            (info.has_opt || matches!(h.view(t), View::App { head: th, .. } if th == head))
                                && self.m(p, t, b, &mut |_| true)
                        };
                        if ok {
                            c |= 1 << j;
                        }
                    }
                    if self.stop.get().is_some() {
                        return true;
                    }
                }
                _ => c = all(n), // alternatives: no cheap filter
            }
            cands[i] = c;
            if c == 0 {
                return false;
            }
        }
        let cands = &cands[..ps.len()];
        let mut singles = [0u64; MAX_AC_PAT];
        let mut ns = 0;
        for (p, c) in ps.iter().zip(cands) {
            if !p.is_seq() {
                singles[ns] = *c;
                ns += 1;
            }
        }
        if !hall(&singles[..ns], all(n)) {
            return false;
        }
        self.ac_smart(ps, ts, cands, 0, b, k)
    }

    /// Can the remaining patterns still be satisfied with the unused arguments? Counts, and a candidate left for
    /// every single pattern.
    fn feasible(&self, ps: &[Pat], n: usize, used: u64, cands: &[u64]) -> bool {
        let free = all(n) & !used;
        let left = free.count_ones() as usize;
        let need: usize = ps.iter().map(Pat::min_len).sum();
        if left < need || (!ps.iter().any(Pat::is_seq) && left != ps.len()) {
            return false;
        }
        ps.iter().zip(cands).all(|(p, c)| p.is_seq() || c & free != 0)
    }

    fn ac_smart(&self, ps: &[Pat], ts: &[T<'h>], cands: &[u64], used: u64, b: &mut Binds<'h>, k: K<'_, 'h>) -> bool {
        let Some((p, rest)) = ps.split_first() else { return used == all(ts.len()) && k(b) };
        let n = ts.len();
        if let Pat::Seq { var, min, ty, .. } = p {
            let allow = self.typed_mask(ts, *ty);
            if rest.is_empty() {
                // the last pattern argument takes everything left: one subset, no enumeration
                let free = all(n) & !used;
                if (free.count_ones() as usize) < *min as usize || free & !allow != 0 {
                    return false;
                }
                let mut it = (0..n).filter(|j| free & (1 << j) != 0).map(|j| ts[j]);
                return self.bind_seq(*var, &mut it, b, k);
            }
            return self.subsets(*var, *min as usize, rest, ts, used, 0, 0, Some(cands), allow, b, k);
        }
        let mut c = cands[0] & !used;
        while c != 0 {
            let j = c.trailing_zeros() as usize;
            c &= c - 1;
            // Of identical arguments take the first unused copy: if a solution gave this pattern copy j while the
            // identical copy j-1 is unused here, swapping the two gives a solution that is earlier in the order.
            if j > 0 && ts[j] == ts[j - 1] && used & (1 << (j - 1)) == 0 {
                continue;
            }
            let u = used | (1 << j);
            if !self.feasible(rest, n, u, &cands[1..]) {
                continue;
            }
            if self.m(p, ts[j], b, &mut |b| self.ac_smart(rest, ts, &cands[1..], u, b, k)) {
                return true;
            }
        }
        false
    }
}

/// Is there a matching that gives each single pattern (candidate set `c[i]`) a distinct argument within `avail`?
/// Kuhn's augmenting paths; the sets are tiny (at most [`MAX_AC_PAT`] patterns over [`MAX_AC`] arguments).
/// The order in which `m_marked` chooses the lengths of argument patterns: marked runs by priority (higher first,
/// ties left to right), then the others left to right.
fn marked_order(ps: &[Pat]) -> Vec<usize> {
    let mark = |p: &Pat| match p {
        Pat::Seq { mark, .. } | Pat::Repeat { mark, .. } => *mark,
        _ => None,
    };
    let mut order: Vec<usize> = (0..ps.len()).collect();
    order.sort_by_key(|&i| (mark(&ps[i]).is_none(), std::cmp::Reverse(mark(&ps[i]).map_or(0, |m| m.1)), i));
    order
}

/// The least and most arguments an argument pattern takes among `n`, and whether its longest run is tried first.
fn run_bounds(p: &Pat, n: usize) -> (usize, usize, bool) {
    match p {
        Pat::Seq { min, mark, .. } => (*min as usize, n, mark.is_some_and(|m| m.0)),
        Pat::Repeat { min, max, mark, .. } => (*min as usize, (*max as usize).min(n), mark.is_some_and(|m| m.0)),
        _ => (1, 1, false),
    }
}

fn hall(c: &[u64], avail: u64) -> bool {
    fn augment(i: usize, c: &[u64], avail: u64, owner: &mut [u8; 64], seen: &mut u64) -> bool {
        let mut m = c[i] & avail & !*seen;
        while m != 0 {
            let j = m.trailing_zeros() as usize;
            m &= m - 1;
            *seen |= 1 << j;
            if owner[j] == u8::MAX || augment(owner[j] as usize, c, avail, owner, seen) {
                owner[j] = i as u8;
                return true;
            }
        }
        false
    }
    let mut owner = [u8::MAX; 64];
    (0..c.len()).all(|i| {
        let mut seen = 0;
        augment(i, c, avail, &mut owner, &mut seen)
    })
}

#[cfg(test)]
mod tests;
