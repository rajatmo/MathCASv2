//! Hash-consed terms in an arena (semantics 1.1, 4.3, 4.4).
//!
//! - A term is a 32-bit id; each distinct term is stored once, so `same` and `hash_term` are O(1).
//! - Compound nodes keep their arguments in one shared argument arena, so interning compares an argument slice of
//!   ids, never whole subterms.
//! - Each node caches a 64-bit mask of the symbols it contains (bit `symbol index % 64`, OR over the children).
//! - Memoised normal forms are stamped with the definition epoch at which they were found; a stamp stays valid unless
//!   a later redefinition touched a symbol whose bit is in the term's mask. With few redefinitions the check is a
//!   short scan of the change log, usually empty.
use super::{Store, View};
use hashbrown::HashTable;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Term(pub u32);

#[derive(Clone, Copy)]
enum Kind {
    Int(i64),
    /// The symbol's own term and its index in `names`.
    Sym(Term, u32),
    App { head: Term, start: u32, len: u32 },
}

#[derive(Clone, Copy)]
struct Node {
    kind: Kind,
    hash: u64,
    mask: u64,
}

/// Handles with this bit set are immediate small integers (31-bit two's complement in the low bits), never stored.
const IMM: u32 = 1 << 31;
const IMM_MIN: i64 = -(1 << 30);
const IMM_MAX: i64 = (1 << 30) - 1;

fn imm_value(t: Term) -> i64 {
    (((t.0 << 1) as i32) >> 1) as i64
}

pub struct HcStore {
    nodes: Vec<Node>,
    args: Vec<Term>,
    table: HashTable<Term>,
    names: Vec<Box<str>>,
    by_name: FxHashMap<Box<str>, Term>,
    /// Normal-form memo: result term and the epoch it was found at (0: none), indexed by term id.
    memo: Vec<(u32, u32)>,
    epoch: u32,
    /// (epoch at which the change happened, mask bit of the changed symbol)
    changes: Vec<(u32, u64)>,
    stamps: bool,
    /// small integers as immediates instead of interned nodes
    imm: bool,
}

const K: u64 = 0x9E37_79B9_7F4A_7C15;

fn mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(K)
}

impl HcStore {
    /// `stamps`: whether evaluation may use memoised normal forms.
    pub fn new(stamps: bool) -> HcStore {
        HcStore::with(stamps, false)
    }

    /// `imm`: whether small integers are immediates (encoded in the handle) rather than interned nodes.
    pub fn with(stamps: bool, imm: bool) -> HcStore {
        HcStore {
            nodes: vec![],
            args: vec![],
            table: HashTable::new(),
            names: vec![],
            by_name: FxHashMap::default(),
            memo: vec![],
            epoch: 1,
            changes: vec![],
            stamps,
            imm,
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn arg_slots(&self) -> usize {
        self.args.len()
    }

    fn intern(&mut self, kind: Kind, hash: u64, mask: u64, new_args: Option<&[Term]>) -> Term {
        let nodes = &self.nodes;
        let args = &self.args;
        let eq = |t: &Term| {
            let n = &nodes[t.0 as usize];
            if n.hash != hash {
                return false;
            }
            match (n.kind, kind) {
                (Kind::Int(a), Kind::Int(b)) => a == b,
                (Kind::Sym(_, a), Kind::Sym(_, b)) => a == b,
                (Kind::App { head: h1, start, len }, Kind::App { head: h2, .. }) => {
                    h1 == h2 && &args[start as usize..(start + len) as usize] == new_args.unwrap()
                }
                _ => false,
            }
        };
        if let Some(t) = self.table.find(hash, eq) {
            return *t;
        }
        let id = Term(self.nodes.len() as u32);
        let kind = match kind {
            Kind::App { head, len, .. } => {
                let start = self.args.len() as u32;
                self.args.extend_from_slice(new_args.unwrap());
                Kind::App { head, start, len }
            }
            Kind::Sym(_, i) => Kind::Sym(id, i),
            k => k,
        };
        self.nodes.push(Node { kind, hash, mask });
        let nodes = &self.nodes;
        self.table.insert_unique(hash, id, |t| nodes[t.0 as usize].hash);
        id
    }

    fn node(&self, t: Term) -> &Node {
        &self.nodes[t.0 as usize]
    }

    /// Hash and symbol mask of any term, immediate or stored.
    fn meta(&self, t: Term) -> (u64, u64) {
        if t.0 & IMM != 0 {
            (mix(0x17, imm_value(t) as u64), 0)
        } else {
            let n = self.node(t);
            (n.hash, n.mask)
        }
    }

    fn stamp_valid(&self, stamp_epoch: u32, mask: u64) -> bool {
        // changes are appended in epoch order: scan only those after the stamp
        for &(e, m) in self.changes.iter().rev() {
            if e <= stamp_epoch {
                break;
            }
            if m & mask != 0 {
                return false;
            }
        }
        true
    }
}

impl Store for HcStore {
    type T = Term;
    type S = Term;

    fn name(&self) -> &'static str {
        match (self.imm, self.stamps) {
            (false, false) => "hc",
            (false, true) => "hc+stamps",
            (true, false) => "hc+imm",
            (true, true) => "hc+imm+stamps",
        }
    }
    fn sym(&mut self, name: &str) -> Term {
        if let Some(t) = self.by_name.get(name) {
            return *t;
        }
        let i = self.names.len() as u32;
        self.names.push(name.into());
        let hash = mix(0x51, super::str_hash(name));
        let t = self.intern(Kind::Sym(Term(0), i), hash, 1u64 << (i % 64), None);
        self.by_name.insert(name.into(), t);
        t
    }
    fn sym_term(&mut self, s: &Term) -> Term {
        *s
    }
    fn int(&mut self, v: i64) -> Term {
        if self.imm && (IMM_MIN..=IMM_MAX).contains(&v) {
            return Term(IMM | (v as u32 & !IMM));
        }
        self.intern(Kind::Int(v), mix(0x17, v as u64), 0, None)
    }
    fn app(&mut self, head: &Term, args: Vec<Term>) -> Term {
        let (hh, hm) = self.meta(*head);
        let mut hash = mix(0x33, hh);
        let mut mask = hm;
        for a in &args {
            let (ah, am) = self.meta(*a);
            hash = mix(hash, ah);
            mask |= am;
        }
        let kind = Kind::App { head: *head, start: 0, len: args.len() as u32 };
        self.intern(kind, hash, mask, Some(&args))
    }
    fn view<'a>(&'a self, t: &'a Term) -> View<'a, Term, Term> {
        if t.0 & IMM != 0 {
            return View::Int(imm_value(*t));
        }
        let n = &self.nodes[t.0 as usize];
        match &n.kind {
            Kind::Int(v) => View::Int(*v),
            Kind::Sym(own, _) => View::Sym(own),
            Kind::App { head, start, len } => View::App(head, &self.args[*start as usize..(*start + *len) as usize]),
        }
    }
    fn same(&self, a: &Term, b: &Term) -> bool {
        a == b
    }
    fn hash_term(&self, t: &Term) -> u64 {
        self.meta(*t).0
    }
    fn sym_is(&self, s: &Term, other: &Term) -> bool {
        s == other
    }
    fn sym_name(&self, s: &Term) -> String {
        match self.node(*s).kind {
            Kind::Sym(_, i) => self.names[i as usize].to_string(),
            _ => unreachable!("not a symbol"),
        }
    }

    fn has_stamps(&self) -> bool {
        self.stamps
    }
    fn memo_get(&self, t: &Term) -> Option<Term> {
        if !self.stamps {
            return None;
        }
        if t.0 & IMM != 0 {
            return Some(*t);
        }
        let &(r, e) = self.memo.get(t.0 as usize)?;
        if e == 0 {
            return None;
        }
        // the result depends on the symbols of the input term (rules are looked up by those symbols)
        if e == self.epoch || self.stamp_valid(e, self.meta(*t).1) {
            Some(Term(r))
        } else {
            None
        }
    }
    fn memo_put(&mut self, t: &Term, nf: &Term) {
        if !self.stamps || t.0 & IMM != 0 {
            return;
        }
        if self.memo.len() < self.nodes.len() {
            self.memo.resize(self.nodes.len(), (0, 0));
        }
        self.memo[t.0 as usize] = (nf.0, self.epoch);
        if nf.0 & IMM == 0 {
            self.memo[nf.0 as usize] = (nf.0, self.epoch);
        }
    }
    fn redefine(&mut self, s: &Term) {
        self.epoch += 1;
        let m = self.node(*s).mask;
        self.changes.push((self.epoch, m));
    }
}
