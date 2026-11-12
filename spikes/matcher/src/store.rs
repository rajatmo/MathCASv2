//! Hash-consed terms in an arena: spike 1's `hc+imm` store (decisions 1.1 to 1.3), made concrete (no `Store` trait:
//! this spike has one store), with three additions:
//!
//! - machine reals, so that patterns can test number kinds (semantics 16);
//! - the canonical order of semantics 8 ([`Store::cmp`]);
//! - canonicalisation of associative-commutative heads ([`Store::app_ac`]: flatten, then sort by the canonical order;
//!   semantics 7.3 without identity elements or number folding, which do not change the matching problem).
//!
//! Memoised normal forms moved to the evaluator, because their stamps now carry read sets (decision 13.1).
use hashbrown::HashTable;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Term(pub u32);

#[derive(Clone, Copy)]
enum Kind {
    Int(i64),
    Real(u64),
    /// The symbol's index in `names`.
    Sym(u32),
    App { head: Term, start: u32, len: u32 },
}

#[derive(Clone, Copy)]
struct Node {
    kind: Kind,
    hash: u64,
    /// Bit `symbol index % 64` for every symbol in the term (spike 1).
    mask: u64,
}

/// A read-only view of one term.
#[derive(Clone, Copy)]
pub enum View<'a> {
    Int(i64),
    Real(f64),
    Sym(u32),
    App(Term, &'a [Term]),
}

/// Handles with this bit set are immediate small integers (31-bit two's complement in the low bits), never stored.
const IMM: u32 = 1 << 31;
const IMM_MIN: i64 = -(1 << 30);
const IMM_MAX: i64 = (1 << 30) - 1;

fn imm_value(t: Term) -> i64 {
    (((t.0 << 1) as i32) >> 1) as i64
}

const K: u64 = 0x9E37_79B9_7F4A_7C15;

fn mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(K)
}

#[derive(Default)]
pub struct Store {
    nodes: Vec<Node>,
    args: Vec<Term>,
    table: HashTable<Term>,
    names: Vec<Box<str>>,
    sym_terms: Vec<Term>,
    by_name: FxHashMap<Box<str>, Term>,
}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    /// Number of stored (non-immediate) terms; term ids are below this.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn intern(&mut self, kind: Kind, hash: u64, mask: u64, new_args: &[Term]) -> Term {
        let nodes = &self.nodes;
        let args = &self.args;
        let eq = |t: &Term| {
            let n = &nodes[t.0 as usize];
            if n.hash != hash {
                return false;
            }
            match (n.kind, kind) {
                (Kind::Int(a), Kind::Int(b)) => a == b,
                (Kind::Real(a), Kind::Real(b)) => a == b,
                (Kind::Sym(a), Kind::Sym(b)) => a == b,
                (Kind::App { head: h1, start, len }, Kind::App { head: h2, .. }) => {
                    h1 == h2 && &args[start as usize..(start + len) as usize] == new_args
                }
                _ => false,
            }
        };
        if let Some(t) = self.table.find(hash, eq) {
            return *t;
        }
        let id = Term(self.nodes.len() as u32);
        assert!(id.0 < IMM, "term arena full");
        let kind = match kind {
            Kind::App { head, len, .. } => {
                let start = self.args.len() as u32;
                self.args.extend_from_slice(new_args);
                Kind::App { head, start, len }
            }
            k => k,
        };
        self.nodes.push(Node { kind, hash, mask });
        let nodes = &self.nodes;
        self.table.insert_unique(hash, id, |t| nodes[t.0 as usize].hash);
        id
    }

    #[inline]
    fn node(&self, t: Term) -> &Node {
        &self.nodes[t.0 as usize]
    }

    pub fn is_imm(t: Term) -> bool {
        t.0 & IMM != 0
    }

    /// Cached hash of any term.
    pub fn hash(&self, t: Term) -> u64 {
        if Store::is_imm(t) {
            mix(0x17, imm_value(t) as u64)
        } else {
            self.node(t).hash
        }
    }

    /// Cached symbol mask of any term.
    #[inline]
    pub fn mask(&self, t: Term) -> u64 {
        if Store::is_imm(t) {
            0
        } else {
            self.node(t).mask
        }
    }

    pub fn sym(&mut self, name: &str) -> Term {
        if let Some(t) = self.by_name.get(name) {
            return *t;
        }
        let i = self.names.len() as u32;
        self.names.push(name.into());
        let hash = mix(0x51, crate::str_hash(name));
        let t = self.intern(Kind::Sym(i), hash, sym_bit(i), &[]);
        self.sym_terms.push(t);
        self.by_name.insert(name.into(), t);
        t
    }

    pub fn int(&mut self, v: i64) -> Term {
        if (IMM_MIN..=IMM_MAX).contains(&v) {
            return Term(IMM | (v as u32 & !IMM));
        }
        self.intern(Kind::Int(v), mix(0x17, v as u64), 0, &[])
    }

    pub fn real(&mut self, v: f64) -> Term {
        self.intern(Kind::Real(v.to_bits()), mix(0x29, v.to_bits()), 0, &[])
    }

    /// A compound term, as given (no canonicalisation).
    pub fn app(&mut self, head: Term, args: &[Term]) -> Term {
        let mut hash = mix(0x33, self.hash(head));
        let mut mask = self.mask(head);
        for &a in args {
            hash = mix(hash, self.hash(a));
            mask |= self.mask(a);
        }
        let kind = Kind::App { head, start: 0, len: args.len() as u32 };
        self.intern(kind, hash, mask, args)
    }

    /// A compound term with an associative-commutative head: arguments with the same head are spliced in (they are
    /// canonical already, so one level suffices), then the arguments are sorted by the canonical order.
    pub fn app_ac(&mut self, head: Term, args: &[Term]) -> Term {
        let mut v = Vec::with_capacity(args.len());
        for &a in args {
            if self.head(a) == Some(head) {
                v.extend_from_slice(self.args(a));
            } else {
                v.push(a);
            }
        }
        v.sort_by(|a, b| self.cmp(*a, *b));
        self.app(head, &v)
    }

    #[inline]
    pub fn view(&self, t: Term) -> View<'_> {
        if Store::is_imm(t) {
            return View::Int(imm_value(t));
        }
        match self.node(t).kind {
            Kind::Int(v) => View::Int(v),
            Kind::Real(b) => View::Real(f64::from_bits(b)),
            Kind::Sym(i) => View::Sym(i),
            Kind::App { head, start, len } => View::App(head, &self.args[start as usize..(start + len) as usize]),
        }
    }

    #[inline]
    pub fn head(&self, t: Term) -> Option<Term> {
        if Store::is_imm(t) {
            return None;
        }
        match self.node(t).kind {
            Kind::App { head, .. } => Some(head),
            _ => None,
        }
    }

    /// The arguments of a compound term; empty for atoms.
    #[inline]
    pub fn args(&self, t: Term) -> &[Term] {
        match self.view(t) {
            View::App(_, a) => a,
            _ => &[],
        }
    }

    /// The index of a symbol term.
    #[inline]
    pub fn sym_index(&self, t: Term) -> Option<u32> {
        if Store::is_imm(t) {
            return None;
        }
        match self.node(t).kind {
            Kind::Sym(i) => Some(i),
            _ => None,
        }
    }

    /// The index of the head symbol of a compound term.
    #[inline]
    pub fn head_sym(&self, t: Term) -> Option<u32> {
        self.head(t).and_then(|h| self.sym_index(h))
    }

    pub fn sym_term(&self, i: u32) -> Term {
        self.sym_terms[i as usize]
    }

    pub fn name(&self, i: u32) -> &str {
        &self.names[i as usize]
    }

    pub fn n_syms(&self) -> usize {
        self.names.len()
    }

    pub fn as_int(&self, t: Term) -> Option<i64> {
        match self.view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }

    pub fn is_atom(&self, t: Term) -> bool {
        !matches!(self.view(t), View::App(..))
    }

    /// The canonical order of semantics 8: numbers (by value, exact before inexact at equal value), then symbols (by
    /// name, then index as a stand-in for the module path), then compound terms (by head, then arity, then argument by
    /// argument). Strings and complex numbers are not in this store.
    ///
    /// Recursive in the depth of the common prefix of the two terms; terms sorted under AC heads are shallow in the
    /// workloads, a real implementation needs an explicit stack here (decision 1.6).
    pub fn cmp(&self, a: Term, b: Term) -> Ordering {
        if a == b {
            return Ordering::Equal;
        }
        let rank = |v: &View| match v {
            View::Int(_) | View::Real(_) => 0,
            View::Sym(_) => 2,
            View::App(..) => 3,
        };
        let (va, vb) = (self.view(a), self.view(b));
        let (ra, rb) = (rank(&va), rank(&vb));
        if ra != rb {
            return ra.cmp(&rb);
        }
        match (va, vb) {
            (View::Int(x), View::Int(y)) => x.cmp(&y),
            (View::Real(x), View::Real(y)) => x.total_cmp(&y),
            (View::Int(x), View::Real(y)) => (x as f64).partial_cmp(&y).unwrap_or(Ordering::Less).then(Ordering::Less),
            (View::Real(x), View::Int(y)) => {
                x.partial_cmp(&(y as f64)).unwrap_or(Ordering::Greater).then(Ordering::Greater)
            }
            (View::Sym(i), View::Sym(j)) => self.names[i as usize].cmp(&self.names[j as usize]).then(i.cmp(&j)),
            (View::App(h1, a1), View::App(h2, a2)) => self.cmp(h1, h2).then(a1.len().cmp(&a2.len())).then_with(|| {
                for (x, y) in a1.iter().zip(a2) {
                    let c = self.cmp(*x, *y);
                    if c != Ordering::Equal {
                        return c;
                    }
                }
                Ordering::Equal
            }),
            _ => unreachable!("ranks are equal"),
        }
    }
}

/// The mask bit of a symbol index (spike 1: bit `index % 64`).
#[inline]
pub fn sym_bit(i: u32) -> u64 {
    1u64 << (i % 64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_ac() {
        let mut st = Store::new();
        let (add, a, b, c) = (st.sym("add"), st.sym("a"), st.sym("b"), st.sym("c"));
        let one = st.int(1);
        let x = st.app_ac(add, &[c, a]);
        let y = st.app_ac(add, &[b, x, one]);
        let z = st.app_ac(add, &[one, a, b, c]);
        assert_eq!(y, z);
        assert_eq!(st.args(z), &[one, a, b, c]);
        let h = st.real(1.0);
        assert_eq!(st.cmp(one, h), Ordering::Less);
        assert_eq!(st.cmp(h, one), Ordering::Greater);
    }
}
