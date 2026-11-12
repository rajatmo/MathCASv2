//! Rule sets shared across threads (question 3): spike 2's indexes and rule order (decisions 2.4, 2.5) rebuilt on the
//! single-writer structures of [`crate::conc`].
//!
//! **Scheme.** A definition is an ordered effect (semantics 18.3), so a rule set has one writer at a time: the task
//! on the sequential frontier, or the session between evaluations. Readers are every task that dispatches on the
//! symbol, speculative ones included. The index is updated **in place by the one writer and read without locks or
//! shared writes**:
//!
//! - rules live in an append-only vector of rule pointers; a replacement (semantics 6.3: same left side up to
//!   renaming) swaps the pointer and retires the old rule until a safe point;
//! - the discrimination tree is append-only: nodes in an append-only vector, edges in single-writer key maps, leaves
//!   in append-only linked lists; a new rule becomes visible with one release store of a leaf;
//! - the AC key-set filter files rules in append-only lists under single-writer key maps;
//! - rule order is a rank per rule in an array that is never changed for a published rule: a new rule's rank is
//!   written into a gap before the rule is published, and when gaps run out the writer builds a renumbered array and
//!   swaps it in whole. A reader loads the array once per dispatch, after collecting candidates, so it always sorts
//!   by one consistent order.
//!
//! A reader therefore sees each rule set either before or after each definition. That is all it needs: a reader that
//! consults `f`'s rules records `f` in its read set, and a definition of `f` by the frontier bumps `f`'s change
//! epoch, so any speculative result computed while `f` was being redefined is discarded by read-set validation
//! (decision 4.5) and every memo entry it wrote fails its stamp check (decision 4.8).
//!
//! For comparison, [`IndexMode::Locked`] also takes a per-rule-set reader-writer lock around every index query.
use crate::conc::{AppendVec, KeyMap};
use crate::heap::{Heap, View};
use crate::matcher::{Binds, Matcher};
use crate::pat::{instance_of, same_up_to_renaming, AppInfo, Pat, Ty};
use crate::Term;
use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering::*};
use std::sync::{Arc, OnceLock};

/// The right side: a native closure building the replacement from the bindings (the evaluator evaluates it). It
/// receives the shared heap: building terms is `&self`.
pub type Rhs = Arc<dyn Fn(&Binds, &Heap) -> Term + Send + Sync>;

pub struct Rule {
    pub lhs: Pat,
    pub rhs: Rhs,
    pub nvars: usize,
    /// AC rule sets: what the left side requires of the subject's arguments.
    req: Option<AcReq>,
}

impl Rule {
    pub fn new(lhs: Pat, rhs: Rhs) -> Rule {
        let nvars = lhs.n_vars();
        Rule { lhs, rhs, nvars, req: None }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IndexMode {
    /// Readers take no lock (the scheme above).
    LockFree,
    /// Readers take a shared lock around each index query, the writer an exclusive one.
    Locked,
}

// ---------------------------------------------------------------------------------------------------------------
// Discrimination tree

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Key {
    Atom(Term),
    App(Term, u32),
    Head(Term),
}

impl Key {
    /// A nonzero 64-bit code for the key maps (arities are below 2^30 in this spike).
    #[inline]
    fn code(self) -> u64 {
        match self {
            Key::Atom(t) => 1 << 62 | t.0 as u64,
            Key::App(h, a) => 2 << 62 | (a as u64 & 0x3FFF_FFFF) << 32 | h.0 as u64,
            Key::Head(h) => 3 << 62 | h.0 as u64,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum QKey {
    K(Key),
    Star,
}

/// One entry of a flattened term or pattern: its key, the position after its subterm, and whether its arguments
/// follow (a subject is flattened only to the depth the tree had when the reader looked).
#[derive(Clone, Copy)]
pub struct Flat {
    key: QKey,
    next: u32,
    expanded: bool,
}

#[derive(Default)]
struct TNode {
    star: AtomicU32,
    edges: KeyMap,
    /// Head of this node's leaf list (index + 1 into `Trie::leaves`; 0 = empty).
    leaves: AtomicU32,
}

struct Leaf {
    rule: u32,
    next: u32,
}

struct Trie {
    nodes: AppendVec<TNode>,
    leaves: AppendVec<Leaf>,
    /// Deepest key of any stored path; raised before the leaf of a deeper path is published.
    depth: AtomicU32,
}

impl Trie {
    fn new() -> Trie {
        let t = Trie { nodes: AppendVec::new(), leaves: AppendVec::new(), depth: AtomicU32::new(0) };
        t.nodes.push(TNode::default());
        t
    }

    #[inline]
    fn node(&self, n: u32) -> &TNode {
        self.nodes.get(n as usize).expect("trie node published before it is linked")
    }

    /// Writer only.
    fn insert(&self, path: &[(QKey, usize)], id: u32) {
        let mut n = 0u32;
        for &(k, d) in path {
            self.depth.fetch_max(d as u32, Release);
            let node = self.node(n);
            let next = match k {
                QKey::Star => node.star.load(Relaxed),
                QKey::K(key) => node.edges.get(key.code()).unwrap_or(0),
            };
            n = if next != 0 {
                next
            } else {
                // the child is published (pushed) before it is linked
                let c = self.nodes.push(TNode::default()) as u32;
                match k {
                    QKey::Star => node.star.store(c, Release),
                    QKey::K(key) => node.edges.set(key.code(), c),
                }
                c
            };
        }
        let node = self.node(n);
        let l = self.leaves.push(Leaf { rule: id, next: node.leaves.load(Relaxed) }) as u32;
        // the publication point of the rule in this index
        node.leaves.store(l + 1, Release);
    }

    fn leaves_of(&self, node: &TNode, out: &mut Vec<u32>) {
        let mut l = node.leaves.load(Acquire);
        while l != 0 {
            let lf = self.leaves.get(l as usize - 1).expect("leaf published");
            out.push(lf.rule);
            l = lf.next;
        }
    }

    /// All rules on a path compatible with `q` (see spike 2). An unexpanded entry (the subject was flattened to an
    /// older, smaller depth) follows only `Head` and `*` edges: every rule published before that depth was read has
    /// no key below it, so only newer rules are missed, which is a "before" view of them.
    fn query(&self, q: &[Flat], out: &mut Vec<u32>, stack: &mut Vec<(u32, u32)>) {
        stack.clear();
        stack.push((0, 0));
        while let Some((n, pos)) = stack.pop() {
            let node = self.node(n);
            if pos as usize == q.len() {
                self.leaves_of(node, out);
                continue;
            }
            let e = q[pos as usize];
            let star = node.star.load(Acquire);
            if star != 0 {
                stack.push((star, e.next));
            }
            match e.key {
                QKey::K(Key::Atom(t)) => {
                    if let Some(c) = node.edges.get(Key::Atom(t).code()) {
                        stack.push((c, pos + 1));
                    }
                }
                QKey::K(Key::App(h, a)) => {
                    if e.expanded || a == 0 {
                        if let Some(c) = node.edges.get(Key::App(h, a).code()) {
                            stack.push((c, pos + 1));
                        }
                    }
                    if let Some(c) = node.edges.get(Key::Head(h).code()) {
                        stack.push((c, e.next));
                    }
                }
                QKey::K(Key::Head(h)) => {
                    if let Some(c) = node.edges.get(Key::Head(h).code()) {
                        stack.push((c, e.next));
                    }
                }
                QKey::Star => {}
            }
        }
    }

    fn reclaim(&mut self) -> usize {
        let mut n = 0;
        self.nodes.for_each_mut(|t| n += t.edges.reclaim());
        n
    }
}

fn pat_path(st: &Heap, p: &Pat, d: usize, out: &mut Vec<(QKey, usize)>) {
    match p.strip() {
        Pat::Lit(t) => term_path(st, *t, d, out),
        Pat::App { head, args, info } => {
            if info.ac || info.has_seq {
                out.push((QKey::K(Key::Head(*head)), d));
            } else {
                out.push((QKey::K(Key::App(*head, args.len() as u32)), d));
                for a in args {
                    pat_path(st, a, d + 1, out);
                }
            }
        }
        _ => out.push((QKey::Star, d)),
    }
}

fn term_path(st: &Heap, t: Term, d: usize, out: &mut Vec<(QKey, usize)>) {
    match st.view(t) {
        View::App(h, args) => {
            out.push((QKey::K(Key::App(h, args.len() as u32)), d));
            for &a in args {
                term_path(st, a, d + 1, out);
            }
        }
        _ => out.push((QKey::K(Key::Atom(t)), d)),
    }
}

fn term_flat(st: &Heap, t: Term, d: usize, depth: usize, out: &mut Vec<Flat>) {
    let i = out.len();
    match st.view(t) {
        View::App(h, args) => {
            let expanded = d < depth;
            out.push(Flat { key: QKey::K(Key::App(h, args.len() as u32)), next: 0, expanded });
            if expanded {
                for &a in args {
                    term_flat(st, a, d + 1, depth, out);
                }
            }
        }
        _ => out.push(Flat { key: QKey::K(Key::Atom(t)), next: 0, expanded: true }),
    }
    out[i].next = out.len() as u32;
}

fn pat_flat(st: &Heap, p: &Pat, out: &mut Vec<Flat>) {
    let i = out.len();
    match p.strip() {
        Pat::Lit(t) => {
            term_flat(st, *t, 0, usize::MAX, out);
            return;
        }
        Pat::App { head, args, info } => {
            if info.ac || info.has_seq {
                out.push(Flat { key: QKey::K(Key::Head(*head)), next: 0, expanded: true });
            } else {
                out.push(Flat { key: QKey::K(Key::App(*head, args.len() as u32)), next: 0, expanded: true });
                for a in args {
                    pat_flat(st, a, out);
                }
            }
        }
        _ => out.push(Flat { key: QKey::Star, next: 0, expanded: true }),
    }
    out[i].next = out.len() as u32;
}

// ---------------------------------------------------------------------------------------------------------------
// Key-set filter for AC heads

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum AcKey {
    Term(Term),
    Head(Term),
}

impl AcKey {
    #[inline]
    fn code(self) -> u64 {
        match self {
            AcKey::Term(t) => 1 << 62 | t.0 as u64,
            AcKey::Head(h) => 2 << 62 | h.0 as u64,
        }
    }
}

#[derive(Clone)]
struct AcReq {
    keys: Vec<AcKey>,
    min: usize,
    max: usize,
}

fn ac_req(st: &Heap, p: &Pat) -> AcReq {
    let (args, info) = match p.strip() {
        Pat::App { args, info, .. } => (args.clone(), *info),
        Pat::Lit(t) => {
            let a: Vec<Pat> = st.args(*t).iter().map(|&u| Pat::Lit(u)).collect();
            let n = a.len() as u16;
            (a, AppInfo { ac: true, has_seq: false, n_single: n, min: n })
        }
        _ => unreachable!("AC rule without an AC left side"),
    };
    let mut keys = vec![];
    for a in &args {
        match a.strip() {
            Pat::Lit(t) => keys.push(AcKey::Term(*t)),
            Pat::App { head, .. } | Pat::Blank { ty: Some(Ty::Head(head)), .. } => keys.push(AcKey::Head(*head)),
            _ => {}
        }
    }
    keys.sort();
    let max = if info.has_seq { usize::MAX } else { info.n_single as usize };
    AcReq { keys, min: info.min as usize, max }
}

fn present_keys(st: &Heap, args: &[Term], out: &mut Vec<AcKey>) {
    out.clear();
    for &a in args {
        out.push(AcKey::Term(a));
        if let Some(h) = st.head(a) {
            out.push(AcKey::Head(h));
        }
    }
    out.sort();
}

fn sub_multiset(need: &[AcKey], have: &[AcKey]) -> bool {
    let mut j = 0;
    for k in need {
        while j < have.len() && have[j] < *k {
            j += 1;
        }
        if j == have.len() || have[j] != *k {
            return false;
        }
        j += 1;
    }
    true
}

struct AcIndex {
    by_key: KeyMap,
    unkeyed: AtomicU32,
    lists: AppendVec<Leaf>,
}

impl AcIndex {
    fn new() -> AcIndex {
        AcIndex { by_key: KeyMap::new(), unkeyed: AtomicU32::new(0), lists: AppendVec::new() }
    }

    /// Writer only. Files the rule under its key with the fewest rules so far (spike 2).
    fn insert(&self, id: u32, req: &AcReq, counts: &mut FxHashMap<AcKey, usize>) {
        let best = req.keys.iter().min_by_key(|k| counts.get(k).copied().unwrap_or(0)).copied();
        match best {
            Some(k) => {
                *counts.entry(k).or_default() += 1;
                let head = self.by_key.get(k.code()).unwrap_or(0);
                let l = self.lists.push(Leaf { rule: id, next: head }) as u32;
                self.by_key.set(k.code(), l + 1);
            }
            None => {
                let l = self.lists.push(Leaf { rule: id, next: self.unkeyed.load(Relaxed) }) as u32;
                self.unkeyed.store(l + 1, Release);
            }
        }
    }

    fn walk(&self, mut l: u32, rs: &RuleSet, have: &[AcKey], n: Option<usize>, out: &mut Vec<u32>) {
        while l != 0 {
            let lf = self.lists.get(l as usize - 1).expect("list entry published");
            let r = rs.rule(lf.rule).req.as_ref().expect("AC rule");
            if n.is_none_or(|n| n >= r.min && n <= r.max) && sub_multiset(&r.keys, have) {
                out.push(lf.rule);
            }
            l = lf.next;
        }
    }

    fn query(&self, rs: &RuleSet, have: &[AcKey], n: Option<usize>, out: &mut Vec<u32>) {
        let mut last = None;
        for k in have {
            if last == Some(*k) {
                continue;
            }
            last = Some(*k);
            if let Some(l) = self.by_key.get(k.code()) {
                self.walk(l, rs, have, n, out);
            }
        }
        self.walk(self.unkeyed.load(Acquire), rs, have, n, out);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Rule sets

/// Ranks by rule id. A published rule's rank never changes within one array.
struct Ranks {
    r: Box<[AtomicU64]>,
}

impl Ranks {
    fn new(cap: usize) -> Ranks {
        Ranks { r: (0..cap).map(|_| AtomicU64::new(0)).collect() }
    }
}

/// Writer-side state (behind the writer lock; readers never touch it).
#[derive(Default)]
struct Writer {
    /// Rule ids in rule order.
    order: Vec<u32>,
    counts: FxHashMap<AcKey, usize>,
    #[allow(clippy::vec_box)]
    retired_rules: Vec<Box<Rule>>,
    #[allow(clippy::vec_box)]
    retired_ranks: Vec<Box<Ranks>>,
    renumbers: u32,
}

const GAP: u64 = 1 << 32;

/// Scratch buffers for one dispatch (one per active dispatch frame: conditions make dispatch re-entrant).
#[derive(Default)]
pub struct Scratch {
    pub binds: Binds,
    pub out: Binds,
    pub cands: Vec<u32>,
    ranked: Vec<(u64, u32)>,
    flat: Vec<Flat>,
    keys: Vec<AcKey>,
    stack: Vec<(u32, u32)>,
}

/// The rules of one symbol for one head (its own rules, or the argument-head rules it holds for one outer head).
pub struct RuleSet {
    ac_top: bool,
    mode: IndexMode,
    slots: AppendVec<AtomicPtr<Rule>>,
    ranks: AtomicPtr<Ranks>,
    trie: Trie,
    ac: AcIndex,
    w: Mutex<Writer>,
    lock: RwLock<()>,
}

impl RuleSet {
    pub fn new(ac_top: bool, mode: IndexMode) -> RuleSet {
        RuleSet {
            ac_top,
            mode,
            slots: AppendVec::new(),
            ranks: AtomicPtr::new(Box::into_raw(Box::new(Ranks::new(16)))),
            trie: Trie::new(),
            ac: AcIndex::new(),
            w: Mutex::new(Writer::default()),
            lock: RwLock::new(()),
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The current rule with this id. Valid for the whole `&self` borrow: replaced rules are retired, not freed,
    /// until [`RuleSet::reclaim`] (`&mut self`).
    #[inline]
    pub fn rule(&self, id: u32) -> &Rule {
        let p = self.slots.get(id as usize).expect("rule published").load(Acquire);
        // SAFETY: `p` came from `Box::into_raw`; it is freed only at a safe point (`&mut self`) or drop.
        unsafe { &*p }
    }

    #[inline]
    fn ranks(&self) -> &Ranks {
        // SAFETY: rank arrays are retired, not freed, until `reclaim` (`&mut self`) or drop.
        unsafe { &*self.ranks.load(Acquire) }
    }

    /// Insert a rule in rule order (semantics 6.3), through the index (decision 2.5); a rule with the same left side
    /// up to renaming replaces the old one. One writer at a time (the writer lock is uncontended by construction).
    pub fn insert(&self, st: &Heap, mut rule: Rule) {
        let mut w = self.w.lock();
        let _x = (self.mode == IndexMode::Locked).then(|| self.lock.write());
        if self.ac_top {
            rule.req = Some(ac_req(st, &rule.lhs));
        }
        let ranks = self.ranks();
        // the first rule (in rule order) that is more general than the new one
        let mut cands = vec![];
        self.general_candidates(st, &rule.lhs, &mut cands);
        cands.sort_by_key(|&id| ranks.r[id as usize].load(Relaxed));
        cands.dedup();
        let first = cands.into_iter().find(|&id| instance_of(st, &rule.lhs, &self.rule(id).lhs));
        if let Some(id) = first {
            if same_up_to_renaming(&self.rule(id).lhs, &rule.lhs) {
                let old = self.slots.get(id as usize).unwrap().swap(Box::into_raw(Box::new(rule)), AcqRel);
                // SAFETY: `old` came from `Box::into_raw` and is no longer reachable from the slot.
                w.retired_rules.push(unsafe { Box::from_raw(old) });
                return;
            }
        }
        let id = self.slots.len() as u32;
        let rank_of = |i: u32| ranks.r[i as usize].load(Relaxed);
        let (pos, lo, hi) = match first {
            None => {
                let lo = w.order.last().map_or(0, |&l| rank_of(l));
                (w.order.len(), lo, lo.saturating_add(2 * GAP))
            }
            Some(before) => {
                let pos = w.order.binary_search_by_key(&rank_of(before), |&i| rank_of(i)).expect("ranked rule");
                let lo = if pos == 0 { 0 } else { rank_of(w.order[pos - 1]) };
                (pos, lo, rank_of(before))
            }
        };
        w.order.insert(pos, id);
        let fits = (id as usize) < ranks.r.len();
        if hi - lo >= 2 && hi < u64::MAX / 2 && fits {
            // a gap: the new rank goes into the current array (no published rule's rank changes)
            ranks.r[id as usize].store(lo + (hi - lo) / 2, Relaxed);
        } else {
            // out of gaps or of room: a renumbered array, swapped in whole
            let cap = if fits { ranks.r.len() } else { (ranks.r.len() * 2).max(id as usize + 1) };
            let new = Box::new(Ranks::new(cap));
            for (i, &r) in w.order.iter().enumerate() {
                new.r[r as usize].store((i as u64 + 1) * GAP, Relaxed);
            }
            let old = self.ranks.swap(Box::into_raw(new), AcqRel);
            // SAFETY: as for rules.
            w.retired_ranks.push(unsafe { Box::from_raw(old) });
            w.renumbers += 1;
        }
        let req = rule.req.clone();
        let p = Box::into_raw(Box::new(rule));
        let got = self.slots.push(AtomicPtr::new(p));
        debug_assert_eq!(got, id as usize);
        // publication in the index (release); everything above happened before
        if let Some(req) = req {
            self.ac.insert(id, &req, &mut w.counts);
        } else {
            let mut path = vec![];
            // SAFETY: just published; the writer lock is held
            pat_path(st, &unsafe { &*p }.lhs, 0, &mut path);
            self.trie.insert(&path, id);
        }
    }

    /// Every rule that could be more general than a rule with left side `p` (a superset). Writer only.
    fn general_candidates(&self, st: &Heap, p: &Pat, out: &mut Vec<u32>) {
        if self.ac_top {
            let r = ac_req(st, p);
            let mut have = vec![];
            for k in &r.keys {
                have.push(*k);
                if let AcKey::Term(t) = k {
                    if let Some(h) = st.head(*t) {
                        have.push(AcKey::Head(h));
                    }
                }
            }
            have.sort();
            self.ac.query(self, &have, None, out);
        } else {
            let mut q = vec![];
            pat_flat(st, p, &mut q);
            self.trie.query(&q, out, &mut vec![]);
        }
    }

    /// The candidate rules for subject `t`, in rule order (into `s.cands`).
    pub fn candidates(&self, st: &Heap, t: Term, s: &mut Scratch) {
        let _g = (self.mode == IndexMode::Locked).then(|| self.lock.read());
        s.cands.clear();
        if self.ac_top {
            present_keys(st, st.args(t), &mut s.keys);
            self.ac.query(self, &s.keys, Some(st.args(t).len()), &mut s.cands);
        } else {
            s.flat.clear();
            let depth = self.trie.depth.load(Acquire) as usize;
            term_flat(st, t, 0, depth, &mut s.flat);
            self.trie.query(&s.flat, &mut s.cands, &mut s.stack);
        }
        if s.cands.len() > 1 {
            // one rank array for the whole sort, loaded after the candidates (so it ranks all of them)
            let ranks = self.ranks();
            s.ranked.clear();
            s.ranked.extend(s.cands.iter().map(|&id| (ranks.r[id as usize].load(Relaxed), id)));
            s.ranked.sort_unstable();
            s.ranked.dedup();
            s.cands.clear();
            s.cands.extend(s.ranked.iter().map(|&(_, id)| id));
        }
    }

    /// The first rule (in rule order) whose left side matches `t`, with its bindings in `s.out`. Stops at an abort
    /// (the caller checks [`Matcher::stopped`]).
    pub fn find(&self, mt: &Matcher, t: Term, s: &mut Scratch) -> Option<&Rule> {
        self.candidates(mt.ev.heap, t, s);
        let cands = std::mem::take(&mut s.cands);
        let mut found = None;
        for &id in &cands {
            let r = self.rule(id);
            if mt.first(&r.lhs, r.nvars, t, &mut s.binds, &mut s.out) {
                found = Some(r);
                break;
            }
            if mt.stopped().is_some() {
                break;
            }
        }
        s.cands = cands;
        found
    }

    /// A fingerprint of the rule order (for checks).
    pub fn order_fingerprint(&self) -> u64 {
        self.w.lock().order.iter().fold(99, |h, &id| crate::combine(h, id as u64))
    }

    /// How often the rank array was rebuilt.
    pub fn renumbers(&self) -> u32 {
        self.w.lock().renumbers
    }

    /// Free retired rules, rank arrays and key-map arrays (a safe point).
    pub fn reclaim(&mut self) -> usize {
        let w = self.w.get_mut();
        let n = w.retired_rules.len() + w.retired_ranks.len();
        w.retired_rules.clear();
        w.retired_ranks.clear();
        n + self.trie.reclaim() + self.ac.by_key.reclaim()
    }
}

impl Drop for RuleSet {
    fn drop(&mut self) {
        self.slots.for_each_mut(|p| {
            // SAFETY: each current rule came from `Box::into_raw` and is freed once.
            drop(unsafe { Box::from_raw(*p.get_mut()) });
        });
        // SAFETY: as above.
        drop(unsafe { Box::from_raw(*self.ranks.get_mut()) });
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Definitions of all symbols

/// One symbol's definitions. Own values and versions are atomics (spike 4); rule sets are created once and then
/// extended in place by their one writer.
pub struct SymDefs {
    /// 0 = none, else `1 << 32 | term`.
    own: AtomicU64,
    version: AtomicU32,
    ac: AtomicBool,
    rules: OnceLock<Box<RuleSet>>,
    /// Argument-head rules this symbol holds, per outer head (semantics 6.5): a prepend-only list.
    args: AtomicPtr<ArgNode>,
}

struct ArgNode {
    outer: Term,
    rs: RuleSet,
    next: *mut ArgNode,
}

impl Default for SymDefs {
    fn default() -> Self {
        SymDefs {
            own: AtomicU64::new(0),
            version: AtomicU32::new(0),
            ac: AtomicBool::new(false),
            rules: OnceLock::new(),
            args: AtomicPtr::new(null_mut()),
        }
    }
}

impl Drop for SymDefs {
    fn drop(&mut self) {
        let mut p = *self.args.get_mut();
        while !p.is_null() {
            // SAFETY: every node came from `Box::into_raw` and is reachable once.
            let b = unsafe { Box::from_raw(p) };
            p = b.next;
        }
    }
}

pub struct Defs {
    syms: Box<[SymDefs]>,
    /// Serialises writers (uncontended: only the frontier or the session defines).
    writer: Mutex<()>,
    pub mode: IndexMode,
}

impl Defs {
    pub fn new(n: usize, mode: IndexMode) -> Defs {
        Defs { syms: (0..n).map(|_| SymDefs::default()).collect(), writer: Mutex::new(()), mode }
    }

    #[inline]
    fn sd(&self, i: u32) -> Option<&SymDefs> {
        self.syms.get(i as usize)
    }

    #[inline]
    pub fn own(&self, i: u32) -> Option<Term> {
        let v = self.sd(i)?.own.load(Acquire);
        (v != 0).then_some(Term(v as u32))
    }

    pub fn set_own(&self, i: u32, v: Term) {
        let _w = self.writer.lock();
        let sd = &self.syms[i as usize];
        sd.own.store(1 << 32 | v.0 as u64, Release);
        sd.version.fetch_add(1, Relaxed);
    }

    #[inline]
    pub fn is_ac(&self, i: u32) -> bool {
        self.sd(i).is_some_and(|s| s.ac.load(Relaxed))
    }

    pub fn set_ac(&self, i: u32) {
        let _w = self.writer.lock();
        self.syms[i as usize].ac.store(true, Relaxed);
        self.syms[i as usize].version.fetch_add(1, Relaxed);
    }

    pub fn version(&self, i: u32) -> u32 {
        self.sd(i).map_or(0, |s| s.version.load(Relaxed))
    }

    #[inline]
    pub fn rules(&self, i: u32) -> Option<&RuleSet> {
        self.sd(i)?.rules.get().map(|b| &**b)
    }

    /// Whether symbol `i` holds any argument-head rules.
    #[inline]
    pub fn has_arg_rules(&self, i: u32) -> bool {
        self.sd(i).is_some_and(|s| !s.args.load(Acquire).is_null())
    }

    /// The argument-head rules symbol `i` holds for outer head `outer`.
    pub fn arg_rules(&self, i: u32, outer: Term) -> Option<&RuleSet> {
        let mut p = self.sd(i)?.args.load(Acquire);
        while !p.is_null() {
            // SAFETY: nodes are published complete (release) and freed only at drop.
            let n = unsafe { &*p };
            if n.outer == outer {
                return Some(&n.rs);
            }
            p = n.next;
        }
        None
    }

    /// Add `rule` to the rules `owner` holds: its own (`outer == None`) or its argument-head rules for `outer`.
    pub fn define(&self, st: &Heap, owner: u32, outer: Option<Term>, ac_top: bool, rule: Rule) {
        let _w = self.writer.lock();
        let sd = &self.syms[owner as usize];
        let rs = match outer {
            None => &**sd.rules.get_or_init(|| Box::new(RuleSet::new(ac_top, self.mode))),
            Some(o) => match self.arg_rules(owner, o) {
                Some(rs) => rs,
                None => {
                    let n = Box::into_raw(Box::new(ArgNode {
                        outer: o,
                        rs: RuleSet::new(ac_top, self.mode),
                        next: sd.args.load(Relaxed),
                    }));
                    sd.args.store(n, Release);
                    // SAFETY: just published; freed only at drop
                    &unsafe { &*n }.rs
                }
            },
        };
        rs.insert(st, rule);
        sd.version.fetch_add(1, Relaxed);
    }

    /// Free everything retired by definitions (a safe point).
    pub fn reclaim(&mut self) -> usize {
        let mut n = 0;
        for sd in self.syms.iter_mut() {
            if let Some(rs) = sd.rules.get_mut() {
                n += rs.reclaim();
            }
            let mut p = *sd.args.get_mut();
            while !p.is_null() {
                // SAFETY: `&mut self`: no reader; the node is live until drop
                let node = unsafe { &mut *p };
                n += node.rs.reclaim();
                p = node.next;
            }
        }
        n
    }
}
