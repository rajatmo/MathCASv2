//! The concurrent hash-consed store: spike 1's `hc+imm` design (32-bit handles, 31-bit immediate integers, cached
//! hash and symbol mask per node) with every operation taking `&self`, so any number of threads can build and read
//! terms in one shared store. The interning table is a type parameter (see [`crate::table`]).
use crate::arena::{Arena, Node, TAG_INT, TAG_SYM};
use crate::table::Table;
use crate::{finalize, imm_value, mix, str_hash, Term, IMM, IMM_MAX, IMM_MIN, K};
use rayon::prelude::*;
use parking_lot::Mutex;
use rustc_hash::FxHashMap;

/// Nodes at least this wide hash their arguments in parallel (when called on a pool).
const PAR_ARGS: usize = 1 << 15;
const PAR_CHUNK: usize = 1 << 13;

/// `K^n` (wrapping).
fn kpow(mut n: usize) -> u64 {
    let (mut r, mut b) = (1u64, K);
    while n > 0 {
        if n & 1 != 0 {
            r = r.wrapping_mul(b);
        }
        b = b.wrapping_mul(b);
        n >>= 1;
    }
    r
}

pub enum View<'a> {
    Int(i64),
    /// A symbol, by index.
    Sym(u32),
    App(Term, &'a [Term]),
}

#[derive(Default)]
struct Syms {
    by_name: FxHashMap<Box<str>, Term>,
    names: Vec<Box<str>>,
}

pub struct Store<Tb: Table> {
    arena: Arena,
    table: Tb,
    syms: Mutex<Syms>,
}

impl<Tb: Table> Default for Store<Tb> {
    fn default() -> Self {
        Store::new()
    }
}

impl<Tb: Table> Store<Tb> {
    pub fn new() -> Self {
        Store { arena: Arena::new(), table: Tb::new(), syms: Mutex::new(Syms::default()) }
    }

    pub fn name(&self) -> &'static str {
        Tb::NAME
    }

    /// Number of distinct stored terms.
    pub fn len(&self) -> usize {
        self.table.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn reserved_bytes(&self) -> usize {
        self.arena.reserved_bytes()
    }

    fn intern(&self, proto: Node, args: &[Term]) -> Term {
        let arena = &self.arena;
        let eq = |id: u32| {
            let n = arena.node(id);
            n.hash == proto.hash
                && n.head == proto.head
                && n.len == proto.len
                && if proto.head == TAG_INT || proto.head == TAG_SYM { n.data == proto.data } else { arena.args(n) == args }
        };
        let make = || {
            let mut n = proto;
            if proto.head != TAG_INT && proto.head != TAG_SYM {
                n.data = arena.alloc_args(args) as u64;
            }
            arena.push(n)
        };
        Term(self.table.intern(proto.hash, eq, make, |id| arena.node(id).hash))
    }

    pub fn sym(&self, name: &str) -> Term {
        let mut g = self.syms.lock();
        if let Some(t) = g.by_name.get(name) {
            return *t;
        }
        let i = g.names.len() as u32;
        let hash = mix(0x51, str_hash(name));
        let node = Node { hash, mask: 1u64 << (i % 64), data: i as u64, head: TAG_SYM, len: 0, size: 1 };
        let t = self.intern(node, &[]);
        g.by_name.insert(name.into(), t);
        g.names.push(name.into());
        t
    }

    pub fn sym_name(&self, index: u32) -> String {
        self.syms.lock().names[index as usize].to_string()
    }

    pub fn int(&self, v: i64) -> Term {
        if (IMM_MIN..=IMM_MAX).contains(&v) {
            return Term(IMM | (v as u32 & !IMM));
        }
        self.intern(Node { hash: mix(0x17, v as u64), mask: 0, data: v as u64, head: TAG_INT, len: 0, size: 1 }, &[])
    }

    /// The hash of a compound term is a polynomial in its parts, `((0x33 * K + head) * K + a1) * K + ... + an`
    /// (wrapping), then finalised. Unlike spike 1's sequential fold, the polynomial combines across a split
    /// (`H(xs ++ ys) = H(xs) * K^|ys| + H(ys)`), so the arguments of a wide node are hashed in parallel chunks with
    /// exactly the same result; the mask (OR) and size (sum) combine the same way.
    pub fn app(&self, head: Term, args: &[Term]) -> Term {
        let (hh, hm, _) = self.meta(head);
        let start = 0x33u64.wrapping_mul(K).wrapping_add(hh);
        let (hash, mask, size) = if args.len() >= PAR_ARGS && rayon::current_thread_index().is_some() {
            let (p, m, sz, _) = args
                .par_chunks(PAR_CHUNK)
                .map(|c| self.fold(c))
                .reduce(|| (0, 0, 0, 0), |(h1, m1, s1, _), (h2, m2, s2, n2)| {
                    (h1.wrapping_mul(kpow(n2)).wrapping_add(h2), m1 | m2, s1.saturating_add(s2), 0)
                });
            (start.wrapping_mul(kpow(args.len())).wrapping_add(p), m, sz)
        } else {
            let mut h = start;
            let (mut m, mut sz) = (0, 0u32);
            for &a in args {
                let (ah, am, asz) = self.meta(a);
                h = h.wrapping_mul(K).wrapping_add(ah);
                m |= am;
                sz = sz.saturating_add(asz);
            }
            (h, m, sz)
        };
        let node = Node {
            hash: finalize(hash),
            mask: mask | hm,
            data: 0,
            head,
            len: args.len() as u32,
            size: size.saturating_add(1),
        };
        self.intern(node, args)
    }

    /// The polynomial, mask and size of a chunk of arguments (`len` is filled by the caller where needed).
    fn fold(&self, c: &[Term]) -> (u64, u64, u32, usize) {
        let (mut h, mut m, mut sz) = (0u64, 0u64, 0u32);
        for &a in c {
            let (ah, am, asz) = self.meta(a);
            h = h.wrapping_mul(K).wrapping_add(ah);
            m |= am;
            sz = sz.saturating_add(asz);
        }
        (h, m, sz, c.len())
    }

    /// Hash, symbol mask and tree size of any term.
    #[inline]
    pub fn meta(&self, t: Term) -> (u64, u64, u32) {
        if t.0 & IMM != 0 {
            (mix(0x17, imm_value(t) as u64), 0, 1)
        } else {
            let n = self.arena.node(t.0);
            (n.hash, n.mask, n.size)
        }
    }

    #[inline]
    pub fn size(&self, t: Term) -> u32 {
        if t.0 & IMM != 0 {
            1
        } else {
            self.arena.node(t.0).size
        }
    }

    #[inline]
    pub fn view(&self, t: Term) -> View<'_> {
        if t.0 & IMM != 0 {
            return View::Int(imm_value(t));
        }
        let n = self.arena.node(t.0);
        match n.head {
            TAG_INT => View::Int(n.data as i64),
            TAG_SYM => View::Sym(n.data as u32),
            h => View::App(h, self.arena.args(n)),
        }
    }

    /// The head and arguments, for the spike-1 interface (returns references into the arena).
    pub fn parts(&self, t: Term) -> Option<(&Term, &[Term])> {
        if t.0 & IMM != 0 {
            return None;
        }
        let n = self.arena.node(t.0);
        match n.head {
            TAG_INT | TAG_SYM => None,
            _ => Some((&n.head, self.arena.args(n))),
        }
    }

    #[inline]
    pub fn as_int(&self, t: Term) -> Option<i64> {
        match self.view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }

    /// A store-independent structural fingerprint (the same function as spike 1's, so results of different stores
    /// compare), memoised per stored node so shared subterms are visited once. Iterative: deep terms do not
    /// overflow the stack.
    pub fn fingerprint(&self, t: Term) -> u64 {
        const K: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut done: FxHashMap<u32, u64> = FxHashMap::default();
        let mut names: FxHashMap<u32, u64> = FxHashMap::default();
        let mut name_hash = |i: u32| *names.entry(i).or_insert_with(|| str_hash(&self.sym_name(i)));
        enum F {
            Enter(Term),
            Exit(Term, u64, usize),
        }
        let mut work = vec![F::Enter(t)];
        let mut vals: Vec<u64> = vec![];
        while let Some(f) = work.pop() {
            match f {
                F::Enter(t) => {
                    if let Some(&h) = done.get(&t.0) {
                        vals.push(h);
                        continue;
                    }
                    match self.view(t) {
                        View::Int(v) => vals.push((v as u64).wrapping_mul(K) ^ 1),
                        View::Sym(i) => vals.push(name_hash(i) ^ 2),
                        View::App(h, args) => {
                            let View::Sym(hi) = self.view(h) else { unreachable!("heads are symbols") };
                            work.push(F::Exit(t, name_hash(hi), args.len()));
                            for &a in args.iter().rev() {
                                work.push(F::Enter(a));
                            }
                        }
                    }
                }
                F::Exit(t, hh, n) => {
                    let start = vals.len() - n;
                    let mut h = hh.wrapping_mul(K) ^ 3;
                    for v in vals.drain(start..) {
                        h = (h.rotate_left(5) ^ v).wrapping_mul(K);
                    }
                    done.insert(t.0, h);
                    vals.push(h);
                }
            }
        }
        vals.pop().unwrap()
    }
}

/// The concurrent store behind spike 1's sequential interface, so spike 1's workloads run on it unchanged.
pub struct Shared<'a, Tb: Table>(pub &'a Store<Tb>);

impl<Tb: Table> crate::seq::Store for Shared<'_, Tb> {
    type T = Term;
    type S = Term;
    fn name(&self) -> &'static str {
        Tb::NAME
    }
    fn sym(&mut self, name: &str) -> Term {
        self.0.sym(name)
    }
    fn sym_term(&mut self, s: &Term) -> Term {
        *s
    }
    fn int(&mut self, v: i64) -> Term {
        self.0.int(v)
    }
    fn app(&mut self, head: &Term, args: Vec<Term>) -> Term {
        self.0.app(*head, &args)
    }
    fn view<'a>(&'a self, t: &'a Term) -> crate::seq::View<'a, Term, Term> {
        if let Some((h, a)) = self.0.parts(*t) {
            return crate::seq::View::App(h, a);
        }
        match self.0.view(*t) {
            View::Int(v) => crate::seq::View::Int(v),
            // a symbol is its own handle
            _ => crate::seq::View::Sym(t),
        }
    }
    fn same(&self, a: &Term, b: &Term) -> bool {
        a == b
    }
    fn hash_term(&self, t: &Term) -> u64 {
        self.0.meta(*t).0
    }
    fn sym_is(&self, s: &Term, other: &Term) -> bool {
        s == other
    }
    fn sym_name(&self, s: &Term) -> String {
        match self.0.view(*s) {
            View::Sym(i) => self.0.sym_name(i),
            _ => unreachable!("not a symbol"),
        }
    }
}
