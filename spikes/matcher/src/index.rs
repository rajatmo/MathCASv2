//! Rule sets: rule order (semantics 6.3), linear and indexed dispatch.
//!
//! - **Linear** dispatch tries every rule of the set in rule order.
//! - **Indexed** dispatch asks a term index for the rules that can match and tries only those, in rule order:
//!   - a **discrimination tree** (a trie over the preorder of the left side, with `*` for variables and for subterms
//!     the index does not look into) for heads without AC;
//!   - for AC heads, whose arguments have no fixed positions, a **key-set filter**: each rule lists the literal
//!     arguments and argument heads it requires, is filed under one of them, and is a candidate only if the subject
//!     has all of them and an admissible number of arguments.
//!
//! Rule order is kept by inserting each new rule just before the first rule it is an instance of (the first rule
//! more general than it), or at the end. By induction this keeps every rule before every rule more general than it,
//! and unrelated rules in definition order unless a more general rule forces otherwise. Finding that first more
//! general rule is a scan of all rules (**naive** ordering) or a generalisation query on the same index (**indexed**
//! ordering): the index returns every stored rule that could be more general, and the exact instance-of check runs on
//! those only.
use crate::matcher::{Binds, Matcher};
use crate::pat::{instance_of, same_up_to_renaming, Pat, Ty};
use crate::store::{Store, Term, View};
use crate::{Limit, Stop};
use rustc_hash::FxHashMap;
use std::rc::Rc;

/// The right side: a native closure building the replacement from the bindings (the evaluator evaluates it).
pub type Rhs = Rc<dyn Fn(&Binds, &mut Store) -> Term>;

pub struct Rule {
    pub lhs: Pat,
    pub rhs: Rhs,
    pub nvars: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dispatch {
    Linear,
    Indexed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OrderBy {
    /// Scan every rule for the first more general one.
    Scan,
    /// Query the index for candidates first.
    Index,
}

// ---------------------------------------------------------------------------------------------------------------
// Discrimination tree

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Key {
    /// An atom: integer, real or symbol term.
    Atom(Term),
    /// A compound with this head and arity whose arguments follow.
    App(Term, u32),
    /// A compound with this head, any arity, arguments not indexed.
    Head(Term),
}

#[derive(Clone, Copy, Debug)]
enum QKey {
    K(Key),
    Star,
}

/// One entry of a flattened term or pattern: its key, and the position just after its subterm.
#[derive(Clone, Copy)]
struct Flat {
    key: QKey,
    next: u32,
}

#[derive(Default)]
struct TNode {
    edges: FxHashMap<Key, u32>,
    /// 0: none (the root is never a child)
    star: u32,
    leaves: Vec<u32>,
}

struct Trie {
    nodes: Vec<TNode>,
    /// Deepest key of any stored path; subjects are flattened to this depth only.
    depth: usize,
}

impl Trie {
    fn new() -> Trie {
        Trie { nodes: vec![TNode::default()], depth: 0 }
    }

    fn insert(&mut self, path: &[(QKey, usize)], id: u32) {
        let mut n = 0usize;
        for &(k, d) in path {
            self.depth = self.depth.max(d);
            let next = match k {
                QKey::Star => self.nodes[n].star as usize,
                QKey::K(key) => self.nodes[n].edges.get(&key).map_or(0, |&c| c as usize),
            };
            n = if next != 0 {
                next
            } else {
                let c = self.nodes.len();
                self.nodes.push(TNode::default());
                match k {
                    QKey::Star => self.nodes[n].star = c as u32,
                    QKey::K(key) => {
                        self.nodes[n].edges.insert(key, c as u32);
                    }
                }
                c
            };
        }
        self.nodes[n].leaves.push(id);
    }

    /// All rules on a path compatible with `q`. For a subject term every key is an atom or `App`; for a pattern
    /// (generalisation query) `Head` and `Star` entries also occur and only stored keys at least as general follow
    /// them. Every trie node is reached by at most one path, so this visits each node at most once.
    fn query(&self, q: &[Flat], out: &mut Vec<u32>) {
        let mut stack = vec![(0usize, 0usize)];
        while let Some((n, pos)) = stack.pop() {
            let node = &self.nodes[n];
            if pos == q.len() {
                out.extend_from_slice(&node.leaves);
                continue;
            }
            let e = q[pos];
            if node.star != 0 {
                stack.push((node.star as usize, e.next as usize));
            }
            match e.key {
                QKey::K(Key::Atom(t)) => {
                    if let Some(&c) = node.edges.get(&Key::Atom(t)) {
                        stack.push((c as usize, pos + 1));
                    }
                }
                QKey::K(Key::App(h, a)) => {
                    if let Some(&c) = node.edges.get(&Key::App(h, a)) {
                        stack.push((c as usize, pos + 1));
                    }
                    if let Some(&c) = node.edges.get(&Key::Head(h)) {
                        stack.push((c as usize, e.next as usize));
                    }
                }
                QKey::K(Key::Head(h)) => {
                    if let Some(&c) = node.edges.get(&Key::Head(h)) {
                        stack.push((c as usize, e.next as usize));
                    }
                }
                QKey::Star => {}
            }
        }
    }
}

/// The index path of a pattern (preorder keys with their depths).
fn pat_path(st: &Store, p: &Pat, d: usize, out: &mut Vec<(QKey, usize)>) {
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

fn term_path(st: &Store, t: Term, d: usize, out: &mut Vec<(QKey, usize)>) {
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

/// Flatten a subject term down to `depth` (entries deeper than any stored key are never looked at).
fn term_flat(st: &Store, t: Term, d: usize, depth: usize, out: &mut Vec<Flat>) {
    let i = out.len();
    match st.view(t) {
        View::App(h, args) => {
            out.push(Flat { key: QKey::K(Key::App(h, args.len() as u32)), next: 0 });
            if d < depth {
                for &a in args {
                    term_flat(st, a, d + 1, depth, out);
                }
            }
        }
        _ => out.push(Flat { key: QKey::K(Key::Atom(t)), next: 0 }),
    }
    out[i].next = out.len() as u32;
}

/// Flatten a pattern for a generalisation query.
fn pat_flat(st: &Store, p: &Pat, out: &mut Vec<Flat>) {
    let i = out.len();
    match p.strip() {
        Pat::Lit(t) => {
            term_flat(st, *t, 0, usize::MAX, out);
            return;
        }
        Pat::App { head, args, info } => {
            if info.ac || info.has_seq {
                out.push(Flat { key: QKey::K(Key::Head(*head)), next: 0 });
            } else {
                out.push(Flat { key: QKey::K(Key::App(*head, args.len() as u32)), next: 0 });
                for a in args {
                    pat_flat(st, a, out);
                }
            }
        }
        _ => out.push(Flat { key: QKey::Star, next: 0 }),
    }
    out[i].next = out.len() as u32;
}

// ---------------------------------------------------------------------------------------------------------------
// Key-set filter for AC heads

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
enum AcKey {
    /// This literal argument.
    Term(Term),
    /// Some argument with this head.
    Head(Term),
}

struct AcReq {
    keys: Vec<AcKey>, // sorted, with repeats for multiplicity
    min: usize,
    max: usize,
}

#[derive(Default)]
struct AcIndex {
    by_key: FxHashMap<AcKey, Vec<u32>>,
    unkeyed: Vec<u32>,
    req: FxHashMap<u32, AcReq>,
}

/// What an AC pattern requires of the subject's arguments.
fn ac_req(st: &Store, p: &Pat) -> AcReq {
    let (args, info) = match p.strip() {
        Pat::App { args, info, .. } => (args.clone(), *info),
        Pat::Lit(t) => {
            let a: Vec<Pat> = st.args(*t).iter().map(|&u| Pat::Lit(u)).collect();
            let n = a.len() as u16;
            (a, crate::pat::AppInfo { ac: true, has_seq: false, n_single: n, min: n })
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

/// The keys present in a list of subject arguments (sorted, with repeats). A literal argument that is compound
/// also supplies its head.
fn present_keys(st: &Store, args: &[Term], out: &mut Vec<AcKey>) {
    out.clear();
    for &a in args {
        out.push(AcKey::Term(a));
        if let Some(h) = st.head(a) {
            out.push(AcKey::Head(h));
        }
    }
    out.sort();
}

/// Is the sorted multiset `need` contained in the sorted multiset `have`?
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

impl AcIndex {
    fn insert(&mut self, id: u32, req: AcReq) {
        // file under the key with the fewest rules so far, to keep the buckets that subjects hit small
        let best = req.keys.iter().min_by_key(|k| self.by_key.get(k).map_or(0, Vec::len)).copied();
        match best {
            Some(k) => self.by_key.entry(k).or_default().push(id),
            None => self.unkeyed.push(id),
        }
        self.req.insert(id, req);
    }

    /// Rules whose required keys are all in `have`; `n` is the subject's arity (None: generalisation query, no
    /// arity test).
    fn query(&self, have: &[AcKey], n: Option<usize>, out: &mut Vec<u32>) {
        let ok = |id: &u32| {
            let r = &self.req[id];
            n.is_none_or(|n| n >= r.min && n <= r.max) && sub_multiset(&r.keys, have)
        };
        let mut last = None;
        for k in have {
            if last == Some(*k) {
                continue;
            }
            last = Some(*k);
            if let Some(ids) = self.by_key.get(k) {
                out.extend(ids.iter().filter(|id| ok(id)));
            }
        }
        out.extend(self.unkeyed.iter().filter(|id| ok(id)));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Rule sets

/// The rules of one symbol for one head (the head's own rules, or the argument-head rules a symbol holds for one
/// outer head, semantics 6.5). All left sides have the same top head.
pub struct RuleSet {
    pub rules: Vec<Rule>,
    /// Rule ids in rule order.
    pub order: Vec<u32>,
    /// Rank of each rule id (increasing along `order`, with gaps so that most insertions renumber nothing).
    rank: Vec<u64>,
    ac_top: bool,
    trie: Trie,
    ac: AcIndex,
}

const GAP: u64 = 1 << 32;

/// Scratch buffers for dispatch, reused across calls.
#[derive(Default)]
pub struct Scratch {
    pub binds: Binds,
    pub cands: Vec<u32>,
    flat: Vec<Flat>,
    keys: Vec<AcKey>,
}

impl RuleSet {
    pub fn new(ac_top: bool) -> RuleSet {
        RuleSet { rules: vec![], order: vec![], rank: vec![], ac_top, trie: Trie::new(), ac: AcIndex::default() }
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Insert a rule in rule order (semantics 6.3); a rule identical up to renaming replaces the old one.
    pub fn insert(&mut self, st: &Store, rule: Rule, by: OrderBy, limit: &Limit) -> Result<(), Stop> {
        // the first rule (in rule order) that is more general than the new one
        let mut first: Option<u32> = None;
        match by {
            OrderBy::Scan => {
                for &id in &self.order {
                    if instance_of(st, &rule.lhs, &self.rules[id as usize].lhs, limit)? {
                        first = Some(id);
                        break;
                    }
                }
            }
            OrderBy::Index => {
                let mut cands = vec![];
                self.general_candidates(st, &rule.lhs, &mut cands);
                cands.sort_by_key(|&id| self.rank[id as usize]);
                cands.dedup();
                for id in cands {
                    if limit.tick() {
                        return Err(Stop);
                    }
                    if instance_of(st, &rule.lhs, &self.rules[id as usize].lhs, limit)? {
                        first = Some(id);
                        break;
                    }
                }
            }
        }
        if let Some(id) = first {
            if same_up_to_renaming(&self.rules[id as usize].lhs, &rule.lhs) {
                // the index path depends only on the left side, which is the same up to renaming
                self.rules[id as usize] = rule;
                return Ok(());
            }
        }
        let id = self.rules.len() as u32;
        if self.ac_top {
            self.ac.insert(id, ac_req(st, &rule.lhs));
        } else {
            let mut path = vec![];
            pat_path(st, &rule.lhs, 0, &mut path);
            self.trie.insert(&path, id);
        }
        self.rules.push(rule);
        match first {
            None => {
                let r = self.order.last().map_or(GAP, |&l| self.rank[l as usize] + GAP);
                self.rank.push(r);
                self.order.push(id);
            }
            Some(before) => {
                let pos = self.order.binary_search_by_key(&self.rank[before as usize], |&i| self.rank[i as usize]).unwrap();
                let lo = if pos == 0 { 0 } else { self.rank[self.order[pos - 1] as usize] };
                let hi = self.rank[before as usize];
                self.rank.push(0);
                self.order.insert(pos, id);
                if hi - lo >= 2 {
                    self.rank[id as usize] = lo + (hi - lo) / 2;
                } else {
                    // out of gaps: renumber (rare)
                    for (i, &r) in self.order.iter().enumerate() {
                        self.rank[r as usize] = (i as u64 + 1) * GAP;
                    }
                }
            }
        }
        Ok(())
    }

    /// Every rule that could be more general than a rule with left side `p` (a superset).
    fn general_candidates(&self, st: &Store, p: &Pat, out: &mut Vec<u32>) {
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
            self.ac.query(&have, None, out);
        } else {
            let mut q = vec![];
            pat_flat(st, p, &mut q);
            self.trie.query(&q, out);
        }
    }

    /// The candidate rules for subject `t`, in rule order.
    pub fn candidates(&self, st: &Store, t: Term, s: &mut Scratch) {
        s.cands.clear();
        if self.ac_top {
            present_keys(st, st.args(t), &mut s.keys);
            self.ac.query(&s.keys, Some(st.args(t).len()), &mut s.cands);
        } else {
            s.flat.clear();
            term_flat(st, t, 0, self.trie.depth, &mut s.flat);
            self.trie.query(&s.flat, &mut s.cands);
        }
        if s.cands.len() > 1 {
            s.cands.sort_unstable_by_key(|&id| self.rank[id as usize]);
            s.cands.dedup();
        }
    }

    /// The first rule (in rule order) whose left side matches `t`, with its bindings in `out`.
    pub fn find(&self, mt: &Matcher, t: Term, d: Dispatch, s: &mut Scratch, out: &mut Binds) -> Option<u32> {
        match d {
            Dispatch::Linear => {
                for &id in &self.order {
                    let r = &self.rules[id as usize];
                    if mt.first(&r.lhs, r.nvars, t, &mut s.binds, out) {
                        return Some(id);
                    }
                    if mt.limit.hit() {
                        return None;
                    }
                }
                None
            }
            Dispatch::Indexed => {
                self.candidates(mt.env.st(), t, s);
                let mut cands = std::mem::take(&mut s.cands);
                let mut found = None;
                for &id in &cands {
                    let r = &self.rules[id as usize];
                    if mt.first(&r.lhs, r.nvars, t, &mut s.binds, out) {
                        found = Some(id);
                        break;
                    }
                    if mt.limit.hit() {
                        break;
                    }
                }
                cands.clear();
                s.cands = cands;
                found
            }
        }
    }

    /// A fingerprint of the rule order (to check that both ordering methods agree).
    pub fn order_fingerprint(&self) -> u64 {
        self.order.iter().fold(99, |h, &id| crate::combine(h, id as u64))
    }
}
