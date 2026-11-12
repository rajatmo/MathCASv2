//! The shared term heap: spike 4's concurrent store C (decision 4.1) with spike 2's additions (machine reals, the
//! canonical order of semantics 8, AC canonicalisation by flatten-and-sort).
//!
//! **The API that makes evaluating conditions safe** (question 1). Every operation that creates or reads terms takes
//! `&self`: interning, argument allocation and symbol lookup are interior-mutable and safe from any number of threads
//! (lock-free probes, sharded insertion locks, per-thread id and argument chunks). An evaluation, and every task and
//! condition inside it, holds the heap as a shared scope `&'h Heap` (the [`crate::eval::Eval`] borrows it for `'h`).
//! The only operation that may move, free or renumber anything, [`Heap::safe_point`], takes `&mut self`; it cannot
//! be called while any `&'h Heap` scope is alive, so the borrow checker proves that no condition, right side or
//! speculative task is running when it happens (decisions 1b.2, 1b.7). Spike 2 could not let a condition allocate:
//! its matcher held `&Store` while allocation needed `&mut Store`. Here allocation is `&self`, so the matcher, the
//! condition it calls, and the evaluator that condition calls all share one scope.
use crate::arena::{Arena, Node, TAG_INT, TAG_REAL, TAG_SYM};
use crate::table::Optimistic;
use crate::{finalize, imm_value, mix, str_hash, sym_bit, Term, IMM, IMM_MAX, IMM_MIN, K};
use parking_lot::Mutex;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::sync::atomic::{AtomicU32, Ordering::*};
use std::sync::OnceLock;

/// Symbols per heap (spike limit, as in spike 4).
pub const MAX_SYMS: usize = 1 << 16;
const PAR_ARGS: usize = 1 << 15;
const PAR_CHUNK: usize = 1 << 13;

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

#[derive(Clone, Copy)]
pub enum View<'a> {
    Int(i64),
    Real(f64),
    /// A symbol, by index.
    Sym(u32),
    App(Term, &'a [Term]),
}

pub struct Heap {
    arena: Arena,
    table: Optimistic,
    /// Symbol names by index: written once (under `by_name`'s lock), read without a lock by the canonical order and
    /// fingerprints.
    names: Box<[OnceLock<Box<str>>]>,
    sym_terms: Box<[AtomicU32]>,
    n_syms: AtomicU32,
    by_name: Mutex<FxHashMap<Box<str>, Term>>,
}

impl Default for Heap {
    fn default() -> Self {
        Heap::new()
    }
}

impl Heap {
    pub fn new() -> Heap {
        Heap {
            arena: Arena::new(),
            table: Optimistic::new(),
            names: (0..MAX_SYMS).map(|_| OnceLock::new()).collect(),
            sym_terms: (0..MAX_SYMS).map(|_| AtomicU32::new(0)).collect(),
            n_syms: AtomicU32::new(0),
            by_name: Mutex::new(FxHashMap::default()),
        }
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

    /// A safe point between evaluations (decision 1b.7). `&mut self` proves that no evaluation scope is alive. This
    /// spike has no collector; it frees what the lock-free readers may have been using (replaced table arrays), which
    /// is the part of a collection that depends on exclusivity. Returns the number of arrays freed.
    pub fn safe_point(&mut self) -> usize {
        self.table.reclaim()
    }

    fn intern(&self, proto: Node, args: &[Term]) -> Term {
        let arena = &self.arena;
        let eq = |id: u32| {
            let n = arena.node(id);
            n.hash == proto.hash
                && n.head == proto.head
                && n.len == proto.len
                && if proto.head == TAG_INT || proto.head == TAG_SYM || proto.head == TAG_REAL {
                    n.data == proto.data
                } else {
                    arena.args(n) == args
                }
        };
        let make = || {
            let mut n = proto;
            if proto.head != TAG_INT && proto.head != TAG_SYM && proto.head != TAG_REAL {
                n.data = arena.alloc_args(args) as u64;
            }
            arena.push(n)
        };
        Term(self.table.intern(proto.hash, eq, make))
    }

    pub fn sym(&self, name: &str) -> Term {
        let mut g = self.by_name.lock();
        if let Some(t) = g.get(name) {
            return *t;
        }
        let i = self.n_syms.load(Relaxed);
        assert!((i as usize) < MAX_SYMS, "symbol table full");
        let _ = self.names[i as usize].set(name.into());
        let hash = mix(0x51, str_hash(name));
        let node = Node { hash, mask: sym_bit(i), data: i as u64, head: TAG_SYM, len: 0, size: 1 };
        let t = self.intern(node, &[]);
        self.sym_terms[i as usize].store(t.0, Release);
        self.n_syms.store(i + 1, Release);
        g.insert(name.into(), t);
        t
    }

    #[inline]
    pub fn name(&self, i: u32) -> &str {
        self.names.get(i as usize).and_then(|n| n.get()).map_or("?", |s| s)
    }

    pub fn sym_term(&self, i: u32) -> Term {
        Term(self.sym_terms[i as usize].load(Acquire))
    }

    pub fn n_syms(&self) -> usize {
        self.n_syms.load(Acquire) as usize
    }

    pub fn int(&self, v: i64) -> Term {
        if (IMM_MIN..=IMM_MAX).contains(&v) {
            return Term(IMM | (v as u32 & !IMM));
        }
        self.intern(Node { hash: mix(0x17, v as u64), mask: 0, data: v as u64, head: TAG_INT, len: 0, size: 1 }, &[])
    }

    pub fn real(&self, v: f64) -> Term {
        let b = v.to_bits();
        self.intern(Node { hash: mix(0x29, b), mask: 0, data: b, head: TAG_REAL, len: 0, size: 1 }, &[])
    }

    /// A compound term, as given. The hash is spike 4's polynomial (combinable across a split; decision 4.3). Fixed
    /// here: spike 4's parallel reduce returned length 0 for a combined chunk, so the hash of a node of 2^15 or more
    /// arguments depended on the reduction tree (the schedule) and one term could be interned twice.
    pub fn app(&self, head: Term, args: &[Term]) -> Term {
        let (hh, hm, _) = self.meta(head);
        let start = 0x33u64.wrapping_mul(K).wrapping_add(hh);
        let (hash, mask, size) = if args.len() >= PAR_ARGS && rayon::current_thread_index().is_some() {
            let (p, m, sz, _) = args.par_chunks(PAR_CHUNK).map(|c| self.fold(c)).reduce(
                || (0, 0, 0, 0),
                |(h1, m1, s1, n1), (h2, m2, s2, n2)| (h1.wrapping_mul(kpow(n2)).wrapping_add(h2), m1 | m2, s1.saturating_add(s2), n1 + n2),
            );
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
        let node = Node { hash: finalize(hash), mask: mask | hm, data: 0, head, len: args.len() as u32, size: size.saturating_add(1) };
        self.intern(node, args)
    }

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

    /// A compound term with an associative-commutative head: arguments with the same head are spliced in (one level:
    /// they are canonical already), then sorted by the canonical order (spike 2).
    pub fn app_ac(&self, head: Term, args: &[Term]) -> Term {
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

    /// Hash, symbol mask and tree size.
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
    pub fn mask(&self, t: Term) -> u64 {
        if t.0 & IMM != 0 {
            0
        } else {
            self.arena.node(t.0).mask
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
    pub fn is_imm(t: Term) -> bool {
        t.0 & IMM != 0
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
            TAG_REAL => View::Real(f64::from_bits(n.data)),
            h => View::App(h, self.arena.args(n)),
        }
    }

    #[inline]
    pub fn head(&self, t: Term) -> Option<Term> {
        if t.0 & IMM != 0 {
            return None;
        }
        match self.arena.node(t.0).head {
            TAG_INT | TAG_SYM | TAG_REAL => None,
            h => Some(h),
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

    #[inline]
    pub fn sym_index(&self, t: Term) -> Option<u32> {
        match self.view(t) {
            View::Sym(i) => Some(i),
            _ => None,
        }
    }

    #[inline]
    pub fn head_sym(&self, t: Term) -> Option<u32> {
        self.head(t).and_then(|h| self.sym_index(h))
    }

    #[inline]
    pub fn as_int(&self, t: Term) -> Option<i64> {
        match self.view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }

    /// The canonical order of semantics 8 (spike 2's): numbers, then symbols by name, then compound terms by head,
    /// arity and arguments. Recursive in the depth of the common prefix (AC arguments are shallow in the workloads).
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
            (View::Real(x), View::Int(y)) => x.partial_cmp(&(y as f64)).unwrap_or(Ordering::Greater).then(Ordering::Greater),
            (View::Sym(i), View::Sym(j)) => self.name(i).cmp(self.name(j)).then(i.cmp(&j)),
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

    /// A readable form (for test failures).
    pub fn show(&self, t: Term) -> String {
        match self.view(t) {
            View::Int(v) => v.to_string(),
            View::Real(r) => format!("{r:?}"),
            View::Sym(i) => self.name(i).to_string(),
            View::App(h, args) => {
                let a: Vec<String> = args.iter().map(|&u| self.show(u)).collect();
                format!("{}({})", self.show(h), a.join(", "))
            }
        }
    }
}
