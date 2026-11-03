//! Rule sets shared across threads: spike 2's indexes and rule order (decisions 2.4, 2.5) on the single-writer
//! structures of [`crate::conc`] (I2.4), keyed so that a moving collector never invalidates them (A.2).
//!
//! **Scheme.** A definition is an ordered effect (semantics 18.3), so a rule set has one writer at a time: the task
//! on the sequential frontier, or the session between evaluations. Readers are every task that dispatches on the
//! symbol, speculative ones included. The index is updated **in place by the one writer and read without locks or
//! shared writes**:
//!
//! - rules live in an append-only vector of rule pointers; a replacement (semantics 6.3: the same left side up to
//!   renaming) swaps the pointer and retires the old rule until a safe point;
//! - the discrimination tree is append-only: nodes and edges in append-only vectors, edge lookup through
//!   single-writer key maps, leaves in append-only linked lists; a new rule becomes visible with one release store;
//! - the AC key-set filter files rules in append-only lists under single-writer key maps;
//! - rule order is a rank per rule in an array that never changes for a published rule: a new rank goes into a gap,
//!   and when gaps run out the writer builds a renumbered array and swaps it in whole. A reader loads the array once
//!   per dispatch, after collecting candidates, so it sorts by one consistent order.
//!
//! A reader therefore sees each rule set before or after each definition. That is all it needs: a reader of `f`'s
//! rules records `f` in its read set, and a definition of `f` by the frontier bumps `f`'s change epoch, so any
//! speculative result computed while `f` was being redefined is discarded by read-set validation, and every memo
//! entry it wrote fails its stamp.
//!
//! **Keys (A.2).** An edge is keyed by its term's structural hash (stable across collections), with the kind and
//! arity; the edge record holds the term as a [`Raw`] handle, compared on lookup, so equal hashes of different terms
//! (collisions) get separate edges. The AC filter keys by hash as well and compares handles in its multiset test.
//! The handles are rewritten by tracing; no key is ever recomputed.
use crate::conc::{AppendVec, KeyMap};
use crate::matcher::{Binds, Matcher};
use crate::pat::{instance_of, same_up_to_renaming, AppInfo, Pat, Ty};
use mc_core::{Heap, Raw, Store, Sym, View, T};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering::*};
use std::sync::Arc;

/// A rule's right side: native code building the replacement from the bindings (the evaluator then evaluates it).
/// It may build terms in the shared heap; it must not capture terms (only symbols), since it outlives safe points.
pub type Rhs = Arc<dyn for<'h> Fn(&Binds<'h>, &'h Heap) -> T<'h> + Send + Sync>;

/// A rule's right side as stored.
pub(crate) enum RuleRhs {
    /// native code
    Native(Rhs),
    /// a term in which symbol `names[i]` stands for variable `vars[i]` (a sequence variable for the list of its
    /// terms): the bindings are substituted, and the evaluator evaluates the result
    Term { body: Raw, names: Box<[Sym]>, vars: Box<[u16]> },
}

impl RuleRhs {
    /// The replacement for these bindings.
    pub(crate) fn build<'h>(&self, b: &Binds<'h>, h: &'h Heap, nm: &crate::eval::Names) -> T<'h> {
        match self {
            RuleRhs::Native(f) => f(b, h),
            RuleRhs::Term { body, names, vars } => {
                let pairs: Vec<(Sym, T<'h>)> =
                    names.iter().zip(vars.iter()).map(|(&s, &v)| (s, crate::subst::value(h, b, v, nm.list))).collect();
                crate::subst::substitute(h, &nm.scopes, body.get(h), &pairs)
            }
        }
    }
}

/// A compiled rule.
pub struct Rule {
    pub(crate) lhs: Pat,
    pub(crate) rhs: RuleRhs,
    pub(crate) nvars: usize,
    /// AC rule sets: what the left side requires of the subject's arguments.
    req: Option<AcReq>,
}

impl Rule {
    pub(crate) fn new(lhs: Pat, rhs: RuleRhs) -> Rule {
        let nvars = lhs.n_vars();
        Rule { lhs, rhs, nvars, req: None }
    }

    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.lhs.trace(visit);
        if let RuleRhs::Term { body, .. } = &mut self.rhs {
            visit(body);
        }
        if let Some(r) = &mut self.req {
            r.keys.iter_mut().for_each(|k| visit(&mut k.term));
        }
    }
}

const K: u64 = 0x9E37_79B9_7F4A_7C15;

/// A nonzero 64-bit key-map code from a structural hash and a discriminator.
#[inline]
fn code(hash: u64, disc: u64) -> u64 {
    ((hash.rotate_left(5) ^ disc).wrapping_mul(K)) | 1
}

// ---------------------------------------------------------------------------------------------------------------
// Discrimination tree

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KeyKind {
    /// an atom (any non-compound term)
    Atom = 1,
    /// a compound term with this head and arity, its arguments following
    App = 2,
    /// a compound term with this head, arguments skipped (AC heads and sequence patterns)
    Head = 3,
}

/// A tree key during a query or an insertion.
#[derive(Clone, Copy)]
struct Key<'h> {
    kind: KeyKind,
    term: T<'h>,
    arity: u32,
}

impl<'h> Key<'h> {
    #[inline]
    fn code(self, h: &Heap) -> u64 {
        code(h.structural_hash(self.term), (self.kind as u64) << 32 | self.arity as u64)
    }
}

#[derive(Clone, Copy)]
enum QKey<'h> {
    K(Key<'h>),
    Star,
}

/// One entry of a flattened term or pattern: its key, the position after its subterm, and whether its arguments
/// follow (a subject is flattened only to the depth the tree had when the reader looked, decision I2.9).
#[derive(Clone, Copy)]
pub(crate) struct Flat<'h> {
    key: QKey<'h>,
    next: u32,
    expanded: bool,
}

#[derive(Default)]
struct TNode {
    star: AtomicU32,
    /// key code -> index + 1 of the first edge with that code
    edges: KeyMap,
    /// head of this node's leaf list (index + 1 into `Trie::leaves`; 0 = empty)
    leaves: AtomicU32,
}

/// An edge: its key (verified on lookup) and child; `next` chains edges whose codes collide.
struct Edge {
    kind: KeyKind,
    arity: u32,
    term: Raw,
    child: u32,
    next: u32,
}

struct Leaf {
    rule: u32,
    next: u32,
}

struct Trie {
    nodes: AppendVec<TNode>,
    edges: AppendVec<Edge>,
    leaves: AppendVec<Leaf>,
    /// Deepest key of any stored path; raised before the leaf of a deeper path is published.
    depth: AtomicU32,
}

impl Trie {
    fn new() -> Trie {
        let t = Trie { nodes: AppendVec::default(), edges: AppendVec::default(), leaves: AppendVec::default(), depth: AtomicU32::new(0) };
        t.nodes.push(TNode::default());
        t
    }

    #[inline]
    fn node(&self, n: u32) -> &TNode {
        self.nodes.get(n as usize).expect("tree node published before it is linked")
    }

    /// The child of `node` under `key`, verified by handle.
    #[inline]
    fn child(&self, h: &Heap, node: &TNode, key: Key<'_>) -> Option<u32> {
        let mut e = node.edges.get(key.code(h))?;
        let want = Raw::new(key.term);
        while e != 0 {
            let edge = self.edges.get(e as usize - 1).expect("edge published");
            if edge.kind == key.kind && edge.arity == key.arity && edge.term == want {
                return Some(edge.child);
            }
            e = edge.next;
        }
        None
    }

    /// Writer only.
    fn insert(&self, h: &Heap, path: &[(QKey<'_>, usize)], id: u32) {
        let mut n = 0u32;
        for &(k, d) in path {
            self.depth.fetch_max(d as u32, Release);
            let node = self.node(n);
            let next = match k {
                QKey::Star => node.star.load(Relaxed),
                QKey::K(key) => self.child(h, node, key).unwrap_or(0),
            };
            n = if next != 0 {
                next
            } else {
                // the child is published (pushed) before it is linked
                let c = self.nodes.push(TNode::default()) as u32;
                match k {
                    QKey::Star => node.star.store(c, Release),
                    QKey::K(key) => {
                        let code = key.code(h);
                        let head = node.edges.get(code).unwrap_or(0);
                        let e = self.edges.push(Edge { kind: key.kind, arity: key.arity, term: Raw::new(key.term), child: c, next: head });
                        node.edges.set(code, e as u32 + 1);
                    }
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

    /// All rules on a path compatible with `q` (spike 2). An unexpanded entry (the subject was flattened to an
    /// older, smaller depth) follows only `Head` and `*` edges: every rule published before that depth was read has
    /// no key below it, so only newer rules are missed, which is a "before" view of them.
    fn query(&self, h: &Heap, q: &[Flat<'_>], out: &mut Vec<u32>, stack: &mut Vec<(u32, u32)>) {
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
            let QKey::K(key) = e.key else { continue };
            match key.kind {
                KeyKind::Atom => {
                    if let Some(c) = self.child(h, node, key) {
                        stack.push((c, pos + 1));
                    }
                }
                KeyKind::App => {
                    if e.expanded || key.arity == 0 {
                        if let Some(c) = self.child(h, node, key) {
                            stack.push((c, pos + 1));
                        }
                    }
                    if let Some(c) = self.child(h, node, Key { kind: KeyKind::Head, arity: 0, ..key }) {
                        stack.push((c, e.next));
                    }
                }
                KeyKind::Head => {
                    if let Some(c) = self.child(h, node, key) {
                        stack.push((c, e.next));
                    }
                }
            }
        }
    }

    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.edges.for_each_mut(|e| visit(&mut e.term));
    }

    fn reclaim(&mut self) -> usize {
        let mut n = 0;
        self.nodes.for_each_mut(|t| n += t.edges.reclaim());
        n
    }
}

/// Flatten a term to `depth` (iteratively: terms may be arbitrarily deep). With `keys = true` the entries are
/// collected as an insertion path with depths instead.
fn term_flat<'h>(h: &'h Heap, t: T<'h>, d0: usize, depth: usize, out: &mut Vec<Flat<'h>>, path: Option<&mut Vec<(QKey<'h>, usize)>>) {
    enum Step<'h> {
        Enter(T<'h>, usize),
        Exit(usize),
    }
    let mut path = path;
    let mut stack = vec![Step::Enter(t, d0)];
    while let Some(s) = stack.pop() {
        match s {
            Step::Exit(i) => out[i].next = out.len() as u32,
            Step::Enter(u, d) => {
                let i = out.len();
                match h.view(u) {
                    View::App { head, args } => {
                        let expanded = d < depth;
                        let key = QKey::K(Key { kind: KeyKind::App, term: head, arity: args.len() as u32 });
                        out.push(Flat { key, next: 0, expanded });
                        if let Some(p) = path.as_deref_mut() {
                            p.push((key, d));
                        }
                        stack.push(Step::Exit(i));
                        if expanded {
                            stack.extend(args.iter().rev().map(|&a| Step::Enter(a, d + 1)));
                        }
                    }
                    _ => {
                        let key = QKey::K(Key { kind: KeyKind::Atom, term: u, arity: 0 });
                        out.push(Flat { key, next: i as u32 + 1, expanded: true });
                        if let Some(p) = path.as_deref_mut() {
                            p.push((key, d));
                        }
                    }
                }
            }
        }
    }
}

/// The insertion path of a pattern (recursive on pattern depth, which compilation bounds).
fn pat_path<'h>(h: &'h Heap, p: &Pat, d: usize, out: &mut Vec<(QKey<'h>, usize)>) {
    match p.strip() {
        Pat::Lit(t) => term_flat(h, t.get(h), d, usize::MAX, &mut vec![], Some(out)),
        Pat::App { head, args, info } => {
            let head = head.get(h);
            if info.ac || info.has_seq {
                out.push((QKey::K(Key { kind: KeyKind::Head, term: head, arity: 0 }), d));
            } else {
                out.push((QKey::K(Key { kind: KeyKind::App, term: head, arity: args.len() as u32 }), d));
                for a in args {
                    pat_path(h, a, d + 1, out);
                }
            }
        }
        _ => out.push((QKey::Star, d)),
    }
}

/// A pattern flattened as a query (for finding the rules that may be more general than a new one).
fn pat_flat<'h>(h: &'h Heap, p: &Pat, out: &mut Vec<Flat<'h>>) {
    let i = out.len();
    match p.strip() {
        Pat::Lit(t) => {
            term_flat(h, t.get(h), 0, usize::MAX, out, None);
            return;
        }
        Pat::App { head, args, info } => {
            let head = head.get(h);
            if info.ac || info.has_seq {
                out.push(Flat { key: QKey::K(Key { kind: KeyKind::Head, term: head, arity: 0 }), next: 0, expanded: true });
            } else {
                out.push(Flat { key: QKey::K(Key { kind: KeyKind::App, term: head, arity: args.len() as u32 }), next: 0, expanded: true });
                for a in args {
                    pat_flat(h, a, out);
                }
            }
        }
        _ => out.push(Flat { key: QKey::Star, next: 0, expanded: true }),
    }
    out[i].next = out.len() as u32;
}

// ---------------------------------------------------------------------------------------------------------------
// Key-set filter for AC heads

const AC_TERM: u8 = 1;
const AC_HEAD: u8 = 2;

/// A key a rule requires: an argument identical to `term` (`AC_TERM`) or with head `term` (`AC_HEAD`).
#[derive(Clone, Copy)]
struct AcKey {
    kind: u8,
    hash: u64,
    term: Raw,
}

/// A key present in a subject (or in a pattern, for the insertion query).
#[derive(Clone, Copy)]
pub(crate) struct PKey<'h> {
    kind: u8,
    hash: u64,
    term: T<'h>,
}

#[derive(Clone)]
struct AcReq {
    /// sorted by `(kind, hash)`
    keys: Vec<AcKey>,
    min: usize,
    max: usize,
}

fn ac_req(h: &Heap, p: &Pat) -> AcReq {
    let (args, info) = match p.strip() {
        Pat::App { args, info, .. } => (args.clone(), *info),
        Pat::Lit(t) => {
            let a: Vec<Pat> = match h.view(t.get(h)) {
                View::App { args, .. } => args.iter().map(|&u| Pat::Lit(Raw::new(u))).collect(),
                _ => vec![],
            };
            let n = a.len() as u16;
            (a, AppInfo { ac: true, has_seq: false, n_single: n, min: n })
        }
        _ => unreachable!("AC rule without a compound left side"),
    };
    let mut keys = vec![];
    for a in &args {
        match a.strip() {
            Pat::Lit(t) => keys.push(AcKey { kind: AC_TERM, hash: h.structural_hash(t.get(h)), term: *t }),
            Pat::App { head, .. } => keys.push(AcKey { kind: AC_HEAD, hash: h.structural_hash(head.get(h)), term: *head }),
            Pat::Blank { ty: Some(Ty::Head(s)), .. } => {
                let ht = h.sym_term(*s);
                keys.push(AcKey { kind: AC_HEAD, hash: h.structural_hash(ht), term: Raw::new(ht) })
            }
            _ => {}
        }
    }
    keys.sort_by_key(|k| (k.kind, k.hash));
    let max = if info.has_seq { usize::MAX } else { info.n_single as usize };
    AcReq { keys, min: info.min as usize, max }
}

/// The keys present in a subject's arguments, sorted by `(kind, hash)`.
fn present_keys<'h>(h: &'h Heap, args: &[T<'h>], out: &mut Vec<PKey<'h>>) {
    out.clear();
    for &a in args {
        out.push(PKey { kind: AC_TERM, hash: h.structural_hash(a), term: a });
        if let View::App { head, .. } = h.view(a) {
            out.push(PKey { kind: AC_HEAD, hash: h.structural_hash(head), term: head });
        }
    }
    out.sort_unstable_by_key(|k| (k.kind, k.hash));
}

/// Is the multiset `need` contained in `have`? Both sorted by `(kind, hash)`; within a run of equal `(kind, hash)`
/// the handles are compared (A.2: a hash hit is verified by handle).
fn sub_multiset(need: &[AcKey], have: &[PKey<'_>]) -> bool {
    let (mut i, mut j) = (0, 0);
    let mut taken: Vec<bool> = vec![];
    while i < need.len() {
        let key = (need[i].kind, need[i].hash);
        let i_end = i + need[i..].iter().take_while(|k| (k.kind, k.hash) == key).count();
        while j < have.len() && (have[j].kind, have[j].hash) < key {
            j += 1;
        }
        let j_end = j + have[j..].iter().take_while(|k| (k.kind, k.hash) == key).count();
        if j_end - j < i_end - i {
            return false;
        }
        taken.clear();
        taken.resize(j_end - j, false);
        for n in &need[i..i_end] {
            let Some(pos) = (j..j_end).find(|&q| !taken[q - j] && Raw::new(have[q].term) == n.term) else { return false };
            taken[pos - j] = true;
        }
        i = i_end;
        j = j_end;
    }
    true
}

#[derive(Default)]
struct AcIndex {
    /// key code -> index + 1 of the first entry of that key's list
    by_key: KeyMap,
    unkeyed: AtomicU32,
    lists: AppendVec<Leaf>,
}

impl AcIndex {
    /// Writer only. Files the rule under its key with the fewest rules so far (spike 2).
    fn insert(&self, id: u32, req: &AcReq, counts: &mut FxHashMap<u64, usize>) {
        let best = req.keys.iter().map(|k| code(k.hash, k.kind as u64)).min_by_key(|c| counts.get(c).copied().unwrap_or(0));
        match best {
            Some(c) => {
                *counts.entry(c).or_default() += 1;
                let head = self.by_key.get(c).unwrap_or(0);
                let l = self.lists.push(Leaf { rule: id, next: head }) as u32;
                self.by_key.set(c, l + 1);
            }
            None => {
                let l = self.lists.push(Leaf { rule: id, next: self.unkeyed.load(Relaxed) }) as u32;
                self.unkeyed.store(l + 1, Release);
            }
        }
    }

    fn walk(&self, mut l: u32, rs: &RuleSet, have: &[PKey<'_>], n: Option<usize>, out: &mut Vec<u32>) {
        while l != 0 {
            let lf = self.lists.get(l as usize - 1).expect("list entry published");
            let r = rs.rule(lf.rule).req.as_ref().expect("AC rule");
            if n.is_none_or(|n| n >= r.min && n <= r.max) && sub_multiset(&r.keys, have) {
                out.push(lf.rule);
            }
            l = lf.next;
        }
    }

    fn query(&self, rs: &RuleSet, have: &[PKey<'_>], n: Option<usize>, out: &mut Vec<u32>) {
        let mut last = None;
        for k in have {
            let c = code(k.hash, k.kind as u64);
            if last == Some(c) {
                continue;
            }
            last = Some(c);
            if let Some(l) = self.by_key.get(c) {
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
    counts: FxHashMap<u64, usize>,
    #[allow(clippy::vec_box)] // the boxes are the retired allocations readers may still point into
    retired_rules: Vec<Box<Rule>>,
    #[allow(clippy::vec_box)]
    retired_ranks: Vec<Box<Ranks>>,
}

const GAP: u64 = 1 << 32;

/// Scratch buffers for one dispatch (one per active dispatch frame: conditions make dispatch re-entrant).
#[derive(Default)]
pub(crate) struct Scratch<'h> {
    pub(crate) binds: Binds<'h>,
    pub(crate) out: Binds<'h>,
    cands: Vec<u32>,
    ranked: Vec<(u64, u32)>,
    flat: Vec<Flat<'h>>,
    keys: Vec<PKey<'h>>,
    stack: Vec<(u32, u32)>,
}

/// The rules of one symbol for one head (its own rules, or the argument-head rules it holds for one outer head).
pub(crate) struct RuleSet {
    ac_top: bool,
    slots: AppendVec<AtomicPtr<Rule>>,
    ranks: AtomicPtr<Ranks>,
    trie: Trie,
    ac: AcIndex,
    w: Mutex<Writer>,
}

impl RuleSet {
    pub(crate) fn new(ac_top: bool) -> RuleSet {
        RuleSet {
            ac_top,
            slots: AppendVec::default(),
            ranks: AtomicPtr::new(Box::into_raw(Box::new(Ranks::new(16)))),
            trie: Trie::new(),
            ac: AcIndex::default(),
            w: Mutex::new(Writer::default()),
        }
    }

    /// Number of rules.
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    /// The current rule with this id. Valid for the whole `&self` borrow: replaced rules are retired, not freed,
    /// until [`RuleSet::reclaim`] (`&mut self`).
    #[inline]
    pub(crate) fn rule(&self, id: u32) -> &Rule {
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
    pub(crate) fn insert(&self, h: &Heap, mut rule: Rule) {
        let mut w = self.w.lock();
        if self.ac_top {
            rule.req = Some(ac_req(h, &rule.lhs));
        }
        let ranks = self.ranks();
        // the first rule (in rule order) that is more general than the new one
        let mut cands = vec![];
        self.general_candidates(h, &rule.lhs, &mut cands);
        cands.sort_by_key(|&id| ranks.r[id as usize].load(Relaxed));
        cands.dedup();
        let first = cands.into_iter().find(|&id| instance_of(h, &rule.lhs, &self.rule(id).lhs));
        if let Some(id) = first {
            if same_up_to_renaming(&self.rule(id).lhs, &rule.lhs) {
                let old = self.slots.get(id as usize).expect("rule").swap(Box::into_raw(Box::new(rule)), AcqRel);
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
        }
        let req = rule.req.clone();
        let mut path = vec![];
        if req.is_none() {
            pat_path(h, &rule.lhs, 0, &mut path);
        }
        let got = self.slots.push(AtomicPtr::new(Box::into_raw(Box::new(rule))));
        debug_assert_eq!(got, id as usize);
        // publication in the index (release); everything above happened before
        match req {
            Some(req) => self.ac.insert(id, &req, &mut w.counts),
            None => self.trie.insert(h, &path, id),
        }
    }

    /// Every rule that could be more general than a rule with left side `p` (a superset). Writer only.
    fn general_candidates(&self, h: &Heap, p: &Pat, out: &mut Vec<u32>) {
        if self.ac_top {
            let r = ac_req(h, p);
            let mut have = vec![];
            for k in &r.keys {
                let t = k.term.get(h);
                have.push(PKey { kind: k.kind, hash: k.hash, term: t });
                if k.kind == AC_TERM {
                    if let View::App { head, .. } = h.view(t) {
                        have.push(PKey { kind: AC_HEAD, hash: h.structural_hash(head), term: head });
                    }
                }
            }
            have.sort_unstable_by_key(|k| (k.kind, k.hash));
            self.ac.query(self, &have, None, out);
        } else {
            let mut q = vec![];
            pat_flat(h, p, &mut q);
            self.trie.query(h, &q, out, &mut vec![]);
        }
    }

    /// The candidate rules for subject `t`, in rule order (into `s.cands`).
    fn candidates<'h>(&self, h: &'h Heap, t: T<'h>, s: &mut Scratch<'h>) {
        s.cands.clear();
        if self.ac_top {
            let View::App { args, .. } = h.view(t) else { return };
            present_keys(h, args, &mut s.keys);
            self.ac.query(self, &s.keys, Some(args.len()), &mut s.cands);
        } else {
            s.flat.clear();
            let depth = self.trie.depth.load(Acquire) as usize;
            term_flat(h, t, 0, depth, &mut s.flat, None);
            self.trie.query(h, &s.flat, &mut s.cands, &mut s.stack);
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
    pub(crate) fn find<'h>(&self, mt: &Matcher<'_, '_, 'h>, h: &'h Heap, t: T<'h>, s: &mut Scratch<'h>) -> Option<&Rule> {
        self.candidates(h, t, s);
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

    /// The rule ids in rule order (writer state; for inspection).
    pub(crate) fn order(&self) -> Vec<u32> {
        self.w.lock().order.clone()
    }

    /// The candidates for `t`, in rule order (for tests of the concurrent index).
    #[cfg(test)]
    pub(crate) fn candidate_ids<'h>(&self, h: &'h Heap, t: T<'h>) -> Vec<u32> {
        let mut s = Scratch::default();
        self.candidates(h, t, &mut s);
        s.cands
    }

    /// Visit every handle (a safe point).
    pub(crate) fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.slots.for_each_mut(|p| {
            // SAFETY: `&mut self`: no reader; the rule is live until replaced and retired.
            unsafe { &mut **p.get_mut() }.trace(visit)
        });
        self.trie.trace(visit);
    }

    /// Free retired rules, rank arrays and key-map arrays (a safe point). Returns how many.
    pub(crate) fn reclaim(&mut self) -> usize {
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

#[cfg(test)]
mod tests;
