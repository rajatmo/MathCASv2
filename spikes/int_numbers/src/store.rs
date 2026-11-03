//! Spike 1b's generational store (decisions 1b.1 to 1b.7) with number payloads.
//!
//! A term is a 32-bit handle. Immediates (bit 31): symbols (bit 30) and 30-bit integers. Stored nodes are 32 bytes
//! (kind 16, hash 8, symbol mask 8) and kept in creation order, so a child's id is below its parent's.
//!
//! Number kinds (every value has exactly one representation, so hash-consing by kind is sound):
//! - an integer in [-2^29, 2^29) is an immediate;
//! - an integer whose magnitude fits one limb is a `Word` node (sign and limb inline in the node, no payload);
//! - a larger integer, and every ball, is a payload node: its words (integer: the limbs; ball: exponent, radius
//!   mantissa, radius exponent, then the midpoint mantissa's limbs; the sign sits in the node) live either in the
//!   **limb arena** (one `Vec<u64>` that slides with the nodes, like the argument arena) or in an out-of-arena
//!   **box** owned by the node and freed during the sliding pass when the node dies ([`Placement`]);
//! - a rational is a `Rat` node holding two integer terms, numerator and denominator, in canonical form.
//!
//! Hashing of payloads ([`Hashing`]): `Eager` hashes every word at construction and interns; `Sampled` hashes the
//! length and a fixed number of sampled words (O(1)) and interns, comparing all words on a hash match; `Lazy(t)`
//! does not intern payloads above t words at all: they are compared by value wherever identity is needed (inside the
//! interning table and in [`Heap::same`]), and their full hash is computed on first use and cached.
//!
//! Collection: `Gen` is spike 1b's scheme C (young region with its own table, promoted by sliding compaction once the
//! nursery holds `nursery` bytes; old region compacted when promotions have doubled it). The allocation counters
//! include payload bytes, so a few huge numbers trigger collections as a heap of many nodes would. The memo and
//! normal-form stamps of spike 1b are left out: numbers are their own normal forms and carry an empty symbol mask, so
//! payloads do not interact with them.
use crate::ball::{Ball, Float, Mag};
use crate::int::Int;
use crate::nat::{self, Algo, Nat};
use crate::rat::Rat;
use hashbrown::HashTable;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Term(pub u32);

const IMM: u32 = 1 << 31;
const SYMB: u32 = 1 << 30;
const PAYLOAD: u32 = SYMB - 1;
pub const IMM_MIN: i64 = -(1 << 29);
pub const IMM_MAX: i64 = (1 << 29) - 1;

impl Term {
    pub fn is_node(self) -> bool {
        self.0 & IMM == 0
    }
    pub fn is_imm_int(self) -> bool {
        self.0 & IMM != 0 && self.0 & SYMB == 0
    }
    fn idx(self) -> usize {
        self.0 as usize
    }
}

fn imm_value(t: Term) -> i64 {
    (((t.0 << 2) as i32) >> 2) as i64
}

/// Where payload words live.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    /// all payloads in the limb arena
    Arena,
    /// all payloads in boxes owned by their nodes
    Boxed,
    /// payloads of at most this many words in the arena, larger ones boxed
    Hybrid(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hashing {
    Eager,
    Sampled,
    /// payloads above this many words are not interned
    Lazy(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gc {
    None,
    Gen { nursery: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub name: &'static str,
    pub place: Placement,
    pub hash: Hashing,
    pub gc: Gc,
    /// collect at every safe point (minor each time, major every fourth) and verify the invariants after each
    pub stress: bool,
    /// count boxed payload bytes towards the nursery trigger (the design; false only to measure what happens if
    /// the trigger sees nodes alone)
    pub count_boxed: bool,
}

/// Words of a ball payload before the mantissa: exponent, radius mantissa, radius exponent.
const BALL_HDR: usize = 3;

enum Kind {
    App { head: Term, start: u32, len: u32 },
    Word { neg: bool, mag: u64 },
    Rat { num: Term, den: Term },
    Arena { ball: bool, neg: bool, hashed: bool, start: u32, len: u32 },
    /// `Box<Vec>` on purpose: a thin pointer keeps the kind at 16 bytes and the node at 32 (a `Box<[u64]>` is a fat
    /// pointer and would make every node 40 bytes); the extra 24-byte allocation only exists for large payloads.
    #[allow(clippy::box_collection)]
    Boxed { ball: bool, neg: bool, hashed: bool, data: Box<Vec<u64>> },
    Dead,
}

struct Node {
    kind: Kind,
    hash: u64,
    mask: u64,
}

pub const NODE: usize = std::mem::size_of::<Node>();
const SLOT: usize = std::mem::size_of::<Term>();
/// accounted size of a box: the words plus the `Vec` header the node points to
fn boxed_size(v: &[u64]) -> usize {
    8 * v.len() + 24
}

/// A read-only view of one term.
pub enum View<'a> {
    Sym(u32),
    Int(IntRef<'a>),
    Rat(Term, Term),
    Ball(BallRef<'a>),
    App(Term, &'a [Term]),
}

#[derive(Clone, Copy)]
pub enum IntRef<'a> {
    Small(i64),
    Big { neg: bool, limbs: &'a [u64] },
}

#[derive(Clone, Copy)]
pub struct BallRef<'a> {
    pub neg: bool,
    pub man: &'a [u64],
    pub exp: i64,
    pub rad: Mag,
}

impl BallRef<'_> {
    pub fn to_ball(&self) -> Ball {
        Ball { mid: Float::new(self.neg, Nat::from_limbs(self.man.to_vec()), self.exp), rad: self.rad }
    }
}

#[derive(Clone, Copy)]
enum Key<'a> {
    App(Term, &'a [Term]),
    Word(bool, u64),
    Rat(Term, Term),
    Num { ball: bool, neg: bool, words: &'a [u64] },
}

/// One collection.
#[derive(Clone, Debug)]
pub struct GcEvent {
    pub kind: &'static str,
    pub pause: Duration,
    /// bytes reserved by the store after the collection (capacities, plus boxed payloads)
    pub after: usize,
    pub live_nodes: usize,
    /// payload bytes alive after the collection
    pub live_payload: usize,
    /// payload words copied by the sliding pass (arena placement)
    pub moved_words: usize,
    /// boxes freed by the sliding pass, and their bytes
    pub freed_boxes: usize,
    pub freed_bytes: usize,
}

const K: u64 = 0x9E37_79B9_7F4A_7C15;

fn mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(K)
}

/// Full hash of payload words: four independent lanes (the multiplications pipeline), then combined.
fn hash_words_full(words: &[u64]) -> u64 {
    let mut l = [0x243F_6A88_85A3_08D3u64, 0x1319_8A2E_0370_7344, 0xA409_3822_299F_31D0, 0x082E_FA98_EC4E_6C89];
    let (chunks, rest) = words.as_chunks::<4>();
    for c in chunks {
        for j in 0..4 {
            l[j] = (l[j] ^ c[j]).wrapping_mul(K).rotate_left(29);
        }
    }
    for (j, &w) in rest.iter().enumerate() {
        l[j] = (l[j] ^ w).wrapping_mul(K).rotate_left(29);
    }
    mix(mix(mix(l[0], l[1]), l[2]), l[3])
}

/// Sampled hash: the length, the lowest and highest four words and eight words at evenly spaced positions.
fn hash_words_sampled(words: &[u64]) -> u64 {
    let n = words.len();
    if n <= 16 {
        return hash_words_full(words);
    }
    let mut h = 0x5A5A_1234_9876_ABCDu64;
    for i in (0..4).chain(n - 4..n) {
        h = mix(h, words[i]);
    }
    for k in 1..=8 {
        h = mix(h, words[k * (n - 1) / 9]);
    }
    h
}

fn num_seed(ball: bool, neg: bool, len: usize) -> u64 {
    mix(0x4E55_4D42, (ball as u64) | (neg as u64) << 1 | (len as u64) << 2)
}

fn imm_hash(v: i64) -> u64 {
    mix(0x17, v as u64)
}

fn word_hash(neg: bool, mag: u64) -> u64 {
    mix(mix(0x18, neg as u64), mag)
}

fn bit(bits: &[u64], i: usize) -> bool {
    bits[i >> 6] & (1 << (i & 63)) != 0
}

fn set(bits: &mut [u64], i: usize) {
    bits[i >> 6] |= 1 << (i & 63);
}

fn trim(v: &mut Vec<u64>) {
    while v.last() == Some(&0) {
        v.pop();
    }
}

fn cmp_slices(a: &[u64], b: &[u64]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

/// Signed sum of two magnitudes with signs.
fn signed_add(na: bool, a: &[u64], nb: bool, b: &[u64]) -> (bool, Vec<u64>) {
    if na == nb {
        let mut r = nat::add_vec(a, b);
        trim(&mut r);
        return (na, r);
    }
    match cmp_slices(a, b) {
        Ordering::Equal => (false, vec![]),
        Ordering::Greater => {
            let mut r = a.to_vec();
            nat::sub_into(&mut r, b);
            trim(&mut r);
            (na, r)
        }
        Ordering::Less => {
            let mut r = b.to_vec();
            nat::sub_into(&mut r, a);
            trim(&mut r);
            (nb, r)
        }
    }
}

/// Read access to the arrays, shared by lookups and comparisons (split from the tables they run inside).
struct Arrays<'a> {
    nodes: &'a [Node],
    args: &'a [Term],
    limbs: &'a [u64],
    /// payloads above this many words are not interned (usize::MAX unless Lazy)
    lazy: usize,
}

impl<'a> Arrays<'a> {
    fn words(&self, k: &'a Kind) -> &'a [u64] {
        match k {
            Kind::Arena { start, len, .. } => &self.limbs[*start as usize..(*start + *len) as usize],
            Kind::Boxed { data, .. } => data,
            _ => &[],
        }
    }
    /// Is t a payload node that is not interned (only under Lazy)?
    fn uninterned(&self, t: Term) -> bool {
        self.lazy != usize::MAX
            && t.is_node()
            && match &self.nodes[t.idx()].kind {
                Kind::Arena { len, .. } => *len as usize > self.lazy,
                Kind::Boxed { data, .. } => data.len() > self.lazy,
                _ => false,
            }
    }
    /// Structural identity: handle equality, except for uninterned payloads, which compare by value.
    fn same(&self, a: Term, b: Term) -> bool {
        if a == b {
            return true;
        }
        if !(self.uninterned(a) && self.uninterned(b)) {
            return false;
        }
        let (ka, kb) = (&self.nodes[a.idx()].kind, &self.nodes[b.idx()].kind);
        let flags = |k: &Kind| match k {
            Kind::Arena { ball, neg, .. } | Kind::Boxed { ball, neg, .. } => (*ball, *neg),
            _ => unreachable!(),
        };
        flags(ka) == flags(kb) && self.words(ka) == self.words(kb)
    }
    fn matches(&self, hash: u64, key: Key, t: Term) -> bool {
        let n = &self.nodes[t.idx()];
        if n.hash != hash {
            return false;
        }
        let lazy = self.lazy != usize::MAX;
        match (&n.kind, key) {
            (Kind::App { head, start, len }, Key::App(h2, a)) => {
                let mine = &self.args[*start as usize..(*start + *len) as usize];
                if lazy {
                    mine.len() == a.len() && self.same(*head, h2) && mine.iter().zip(a).all(|(x, y)| self.same(*x, *y))
                } else {
                    *head == h2 && mine == a
                }
            }
            (Kind::Word { neg, mag }, Key::Word(n2, m2)) => *neg == n2 && *mag == m2,
            (Kind::Rat { num, den }, Key::Rat(n2, d2)) => {
                if lazy {
                    self.same(*num, n2) && self.same(*den, d2)
                } else {
                    *num == n2 && *den == d2
                }
            }
            (k @ (Kind::Arena { ball, neg, .. } | Kind::Boxed { ball, neg, .. }), Key::Num { ball: b2, neg: n2, words }) => {
                *ball == b2 && *neg == n2 && self.words(k) == words
            }
            _ => false,
        }
    }
}

pub struct Heap {
    cfg: Config,
    nodes: Vec<Node>,
    args: Vec<Term>,
    limbs: Vec<u64>,
    table: HashTable<Term>,
    young: HashTable<Term>,
    young_lo: usize,
    young_args_lo: usize,
    young_limbs_lo: usize,
    syms: Vec<(u64, Box<str>)>,
    by_name: FxHashMap<Box<str>, u32>,
    /// payload bytes held in boxes (live or not yet collected)
    boxed_bytes: usize,
    /// bytes allocated since the last minor collection (nodes, argument slots, payload words)
    alloc: usize,
    /// bytes live after the last major collection
    live: usize,
    promoted: usize,
    min_bytes: usize,
    log: Vec<GcEvent>,
    minors: usize,
    /// payload words hashed (statistics)
    pub hashed_words: u64,
}

impl Heap {
    pub fn new(cfg: Config) -> Heap {
        Heap {
            cfg,
            nodes: vec![],
            args: vec![],
            limbs: vec![],
            table: HashTable::new(),
            young: HashTable::new(),
            young_lo: 0,
            young_args_lo: 0,
            young_limbs_lo: 0,
            syms: vec![],
            by_name: FxHashMap::default(),
            boxed_bytes: 0,
            alloc: 0,
            live: 0,
            promoted: 0,
            min_bytes: 4 << 20,
            log: vec![],
            minors: 0,
            hashed_words: 0,
        }
    }

    pub fn name(&self) -> &'static str {
        self.cfg.name
    }

    fn lazy_limit(&self) -> usize {
        match self.cfg.hash {
            Hashing::Lazy(t) => t,
            _ => usize::MAX,
        }
    }

    fn arrays(&self) -> Arrays<'_> {
        Arrays { nodes: &self.nodes, args: &self.args, limbs: &self.limbs, lazy: self.lazy_limit() }
    }

    fn gen(&self) -> bool {
        matches!(self.cfg.gc, Gc::Gen { .. })
    }

    /// Bytes reserved by the store: array and table capacities plus boxed payloads.
    pub fn reserved(&self) -> usize {
        self.nodes.capacity() * NODE
            + self.args.capacity() * SLOT
            + self.limbs.capacity() * 8
            + (self.table.capacity() + self.young.capacity()) * (SLOT + 1)
            + self.boxed_bytes
    }

    pub fn gc_log(&self) -> &[GcEvent] {
        &self.log
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    // ---------------------------------------------------------------------------------------------- construction

    pub fn sym(&mut self, name: &str) -> Term {
        if let Some(&i) = self.by_name.get(name) {
            return Term(IMM | SYMB | i);
        }
        let i = self.syms.len() as u32;
        assert!(i < PAYLOAD, "too many symbols");
        self.syms.push((mix(0x51, crate::str_hash(name)), name.into()));
        self.by_name.insert(name.into(), i);
        Term(IMM | SYMB | i)
    }

    pub fn sym_name(&self, s: Term) -> &str {
        assert!(!s.is_node() && s.0 & SYMB != 0, "not a symbol");
        &self.syms[(s.0 & PAYLOAD) as usize].1
    }

    fn lookup(&self, key: Key, hash: u64) -> Option<Term> {
        let ar = self.arrays();
        if let Some(t) = self.table.find(hash, |t| ar.matches(hash, key, *t)) {
            return Some(*t);
        }
        if self.gen() {
            if let Some(t) = self.young.find(hash, |t| ar.matches(hash, key, *t)) {
                return Some(*t);
            }
        }
        None
    }

    fn push(&mut self, kind: Kind, hash: u64, mask: u64, intern: bool) -> Term {
        let id = self.nodes.len() as u32;
        assert!(id < IMM, "arena full");
        self.nodes.push(Node { kind, hash, mask });
        self.alloc += NODE;
        if intern {
            let nodes = &self.nodes;
            let table = if self.gen() { &mut self.young } else { &mut self.table };
            table.insert_unique(hash, Term(id), |t| nodes[t.idx()].hash);
        }
        Term(id)
    }

    /// Hash and symbol mask of any term. An uninterned payload whose hash is not cached yet is hashed here
    /// (without caching; [`Heap::ensure_hash`] caches).
    fn meta(&self, t: Term) -> (u64, u64) {
        if t.is_node() {
            let n = &self.nodes[t.idx()];
            match &n.kind {
                Kind::Arena { hashed: false, ball, neg, .. } | Kind::Boxed { hashed: false, ball, neg, .. } => {
                    let w = self.arrays().words(&n.kind);
                    (mix(num_seed(*ball, *neg, w.len()), hash_words_full(w)), 0)
                }
                _ => (n.hash, n.mask),
            }
        } else if t.0 & SYMB != 0 {
            let i = t.0 & PAYLOAD;
            (self.syms[i as usize].0, 1u64 << (i % 64))
        } else {
            (imm_hash(imm_value(t)), 0)
        }
    }

    /// Lazy mode: compute and cache the full hash of an uninterned payload (on first use as a child).
    fn ensure_hash(&mut self, t: Term) {
        if !t.is_node() {
            return;
        }
        let i = t.idx();
        let unhashed = matches!(self.nodes[i].kind, Kind::Arena { hashed: false, .. } | Kind::Boxed { hashed: false, .. });
        if unhashed {
            let h = self.meta(t).0;
            self.hashed_words += self.arrays().words(&self.nodes[i].kind).len() as u64;
            let n = &mut self.nodes[i];
            n.hash = h;
            match &mut n.kind {
                Kind::Arena { hashed, .. } | Kind::Boxed { hashed, .. } => *hashed = true,
                _ => {}
            }
        }
    }

    pub fn app(&mut self, head: Term, args: &[Term]) -> Term {
        if self.lazy_limit() != usize::MAX {
            self.ensure_hash(head);
            for &a in args {
                self.ensure_hash(a);
            }
        }
        let (hh, hm) = self.meta(head);
        let mut hash = mix(0x33, hh);
        let mut mask = hm;
        for &a in args {
            let (ah, am) = self.meta(a);
            hash = mix(hash, ah);
            mask |= am;
        }
        let key = Key::App(head, args);
        if let Some(t) = self.lookup(key, hash) {
            return t;
        }
        let start = self.args.len();
        self.args.extend_from_slice(args);
        self.alloc += args.len() * SLOT;
        self.push(Kind::App { head, start: start as u32, len: args.len() as u32 }, hash, mask, true)
    }

    /// A payload node from its words (integer limbs, or a ball's header and mantissa).
    fn payload(&mut self, ball: bool, neg: bool, words: Vec<u64>) -> Term {
        let n = words.len();
        if n > self.lazy_limit() {
            // not interned: no hash now, no lookup
            let kind = self.place(ball, neg, false, words);
            return self.push(kind, 0, 0, false);
        }
        let h = match self.cfg.hash {
            Hashing::Sampled => hash_words_sampled(&words),
            _ => hash_words_full(&words),
        };
        self.hashed_words += match self.cfg.hash {
            Hashing::Sampled => n.min(16) as u64,
            _ => n as u64,
        };
        let hash = mix(num_seed(ball, neg, n), h);
        if let Some(t) = self.lookup(Key::Num { ball, neg, words: &words }, hash) {
            return t;
        }
        let kind = self.place(ball, neg, true, words);
        self.push(kind, hash, 0, true)
    }

    fn place(&mut self, ball: bool, neg: bool, hashed: bool, words: Vec<u64>) -> Kind {
        let n = words.len();
        let in_arena = match self.cfg.place {
            Placement::Arena => true,
            Placement::Boxed => false,
            Placement::Hybrid(t) => n <= t,
        };
        if in_arena || self.cfg.count_boxed {
            self.alloc += 8 * n;
        }
        if in_arena {
            let start = self.limbs.len();
            assert!(start + n < u32::MAX as usize, "limb arena full");
            self.limbs.extend_from_slice(&words);
            Kind::Arena { ball, neg, hashed, start: start as u32, len: n as u32 }
        } else {
            self.boxed_bytes += boxed_size(&words);
            Kind::Boxed { ball, neg, hashed, data: Box::new(words) }
        }
    }

    /// The canonical integer term of an i64.
    pub fn int(&mut self, v: i64) -> Term {
        if (IMM_MIN..=IMM_MAX).contains(&v) {
            return Term(IMM | (v as u32 & PAYLOAD));
        }
        self.word(v < 0, v.unsigned_abs())
    }

    fn word(&mut self, neg: bool, mag: u64) -> Term {
        let hash = word_hash(neg, mag);
        if let Some(t) = self.lookup(Key::Word(neg, mag), hash) {
            return t;
        }
        self.push(Kind::Word { neg, mag }, hash, 0, true)
    }

    /// The canonical integer term of sign and magnitude limbs (any trailing zeros): an immediate if it fits, a
    /// `Word` if the magnitude is one limb, else a payload.
    pub fn int_parts(&mut self, neg: bool, mut limbs: Vec<u64>) -> Term {
        trim(&mut limbs);
        match limbs.len() {
            0 => Term(IMM),
            1 => {
                let m = limbs[0];
                if !neg && m <= IMM_MAX as u64 {
                    Term(IMM | m as u32)
                } else if neg && m <= (-IMM_MIN) as u64 {
                    self.int(-(m as i64))
                } else {
                    self.word(neg, m)
                }
            }
            _ => self.payload(false, neg, limbs),
        }
    }

    pub fn from_int(&mut self, x: &Int) -> Term {
        self.int_parts(x.neg, x.mag.limbs.clone())
    }

    /// Sign and magnitude of an integer term; an immediate's magnitude is placed in `tmp`.
    fn int_slices<'a>(&'a self, t: Term, tmp: &'a mut u64) -> (bool, &'a [u64]) {
        match self.view(t) {
            View::Int(IntRef::Small(v)) => {
                *tmp = v.unsigned_abs();
                (v < 0, if v == 0 { &[] } else { std::slice::from_ref(tmp) })
            }
            View::Int(IntRef::Big { neg, limbs }) => (neg, limbs),
            _ => panic!("not an integer term"),
        }
    }

    pub fn to_int(&self, t: Term) -> Int {
        let mut tmp = 0;
        let (neg, l) = self.int_slices(t, &mut tmp);
        Int::new(neg, Nat::from_limbs(l.to_vec()))
    }

    pub fn is_int(&self, t: Term) -> bool {
        matches!(self.view(t), View::Int(_))
    }

    /// The canonical term of a canonical rational (an integer if the denominator is one).
    pub fn from_rat(&mut self, r: &Rat) -> Term {
        debug_assert!(r.is_canonical());
        let num = self.from_int(&r.num);
        if crate::gcd::is_one(&r.den) {
            return num;
        }
        let den = self.int_parts(false, r.den.limbs.clone());
        self.rat_node(num, den)
    }

    fn rat_node(&mut self, num: Term, den: Term) -> Term {
        if self.lazy_limit() != usize::MAX {
            self.ensure_hash(num);
            self.ensure_hash(den);
        }
        let hash = mix(mix(0x22, self.meta(num).0), self.meta(den).0);
        if let Some(t) = self.lookup(Key::Rat(num, den), hash) {
            return t;
        }
        self.push(Kind::Rat { num, den }, hash, 0, true)
    }

    pub fn to_rat(&self, t: Term) -> Rat {
        match self.view(t) {
            View::Rat(n, d) => Rat { num: self.to_int(n), den: self.to_int(d).mag },
            View::Int(_) => Rat::from_int(self.to_int(t)),
            _ => panic!("not an exact number"),
        }
    }

    pub fn ball(&mut self, b: &Ball) -> Term {
        let mut w = Vec::with_capacity(BALL_HDR + b.mid.man.limbs.len());
        w.extend_from_slice(&[b.mid.exp as u64, b.rad.m, b.rad.e as u64]);
        w.extend_from_slice(&b.mid.man.limbs);
        self.payload(true, b.mid.neg, w)
    }

    pub fn to_ball(&self, t: Term) -> Ball {
        match self.view(t) {
            View::Ball(b) => b.to_ball(),
            _ => panic!("not a ball"),
        }
    }

    // ---------------------------------------------------------------------------------------------- arithmetic

    /// Exact sum (integers or rationals); results are canonical.
    pub fn add(&mut self, a: Term, b: Term) -> Term {
        if a.is_imm_int() && b.is_imm_int() {
            return self.int(imm_value(a) + imm_value(b));
        }
        if self.is_int(a) && self.is_int(b) {
            let (mut t1, mut t2) = (0, 0);
            let (na, la) = self.int_slices(a, &mut t1);
            let (nb, lb) = self.int_slices(b, &mut t2);
            let (neg, r) = signed_add(na, la, nb, lb);
            return self.int_parts(neg, r);
        }
        let r = self.to_rat(a).add(&self.to_rat(b));
        self.from_rat(&r)
    }

    pub fn neg(&mut self, a: Term) -> Term {
        if a.is_imm_int() {
            return self.int(-imm_value(a));
        }
        let r = self.to_rat(a).neg();
        self.from_rat(&r)
    }

    pub fn sub(&mut self, a: Term, b: Term) -> Term {
        let nb = self.neg(b);
        self.add(a, nb)
    }

    /// Exact product (integers or rationals); results are canonical.
    pub fn mul(&mut self, a: Term, b: Term) -> Term {
        if a.is_imm_int() && b.is_imm_int() {
            return self.int(imm_value(a) * imm_value(b));
        }
        if self.is_int(a) && self.is_int(b) {
            let (mut t1, mut t2) = (0, 0);
            let (na, la) = self.int_slices(a, &mut t1);
            let (nb, lb) = self.int_slices(b, &mut t2);
            let r = if std::ptr::eq(la, lb) && la.len() >= Algo::auto().ntt {
                crate::ntt::mul(la, lb)
            } else {
                nat::mul_slices(la, lb, Algo::auto())
            };
            return self.int_parts(na != nb, r);
        }
        let r = self.to_rat(a).mul(&self.to_rat(b));
        self.from_rat(&r)
    }

    /// The rational 1/k (or the integer for k = ±1).
    pub fn recip(&mut self, k: i64) -> Term {
        let r = Rat::new(Int::from_i64(1), Int::from_i64(k));
        self.from_rat(&r)
    }

    pub fn ball_add(&mut self, a: Term, b: Term, p: u64) -> Term {
        let r = self.ball_ref(a).to_ball().add(&self.ball_ref(b).to_ball(), p);
        self.ball(&r)
    }
    pub fn ball_mul(&mut self, a: Term, b: Term, p: u64) -> Term {
        let r = self.ball_ref(a).to_ball().mul(&self.ball_ref(b).to_ball(), p);
        self.ball(&r)
    }
    pub fn ball_div(&mut self, a: Term, b: Term, p: u64) -> Option<Term> {
        let r = self.ball_ref(a).to_ball().div(&self.ball_ref(b).to_ball(), p)?;
        Some(self.ball(&r))
    }
    fn ball_ref(&self, t: Term) -> BallRef<'_> {
        match self.view(t) {
            View::Ball(b) => b,
            _ => panic!("not a ball"),
        }
    }

    // ---------------------------------------------------------------------------------------------- access

    pub fn view(&self, t: Term) -> View<'_> {
        if !t.is_node() {
            return if t.0 & SYMB != 0 { View::Sym(t.0 & PAYLOAD) } else { View::Int(IntRef::Small(imm_value(t))) };
        }
        let n = &self.nodes[t.idx()];
        match &n.kind {
            Kind::App { head, start, len } => View::App(*head, &self.args[*start as usize..(*start + *len) as usize]),
            Kind::Word { neg, mag } => View::Int(IntRef::Big { neg: *neg, limbs: std::slice::from_ref(mag) }),
            Kind::Rat { num, den } => View::Rat(*num, *den),
            k @ (Kind::Arena { ball, neg, .. } | Kind::Boxed { ball, neg, .. }) => {
                let w = self.arrays().words(k);
                if *ball {
                    View::Ball(BallRef {
                        neg: *neg,
                        man: &w[BALL_HDR..],
                        exp: w[0] as i64,
                        rad: Mag { m: w[1], e: w[2] as i64 },
                    })
                } else {
                    View::Int(IntRef::Big { neg: *neg, limbs: w })
                }
            }
            Kind::Dead => panic!("use of collected term {} (a handle was not rooted across a safe point)", t.0),
        }
    }

    /// Structural identity (`===`): O(1) except between two uninterned payloads (Lazy), compared by value.
    pub fn same(&self, a: Term, b: Term) -> bool {
        self.arrays().same(a, b)
    }

    pub fn hash_term(&self, t: Term) -> u64 {
        self.meta(t).0
    }

    /// Store-independent fingerprint of a term (iterative, memoised per handle: terms are DAGs).
    pub fn fingerprint(&self, t: Term) -> u64 {
        let mut memo: FxHashMap<Term, u64> = FxHashMap::default();
        let mut stack = vec![(t, false)];
        // bounded: each handle is expanded once
        while let Some((u, done)) = stack.pop() {
            if memo.contains_key(&u) {
                continue;
            }
            let kids: Vec<Term> = match self.view(u) {
                View::App(h, a) => std::iter::once(h).chain(a.iter().copied()).collect(),
                View::Rat(n, d) => vec![n, d],
                _ => vec![],
            };
            if !done && !kids.is_empty() {
                stack.push((u, true));
                stack.extend(kids.iter().map(|&k| (k, false)));
                continue;
            }
            let v = match self.view(u) {
                View::Sym(_) => crate::fp::sym(self.sym_name(u)),
                View::Int(IntRef::Small(v)) => crate::fp::int(&Int::from_i64(v)),
                View::Int(IntRef::Big { neg, limbs }) => crate::fp::int_parts(neg, limbs),
                View::Rat(n, d) => crate::fp::rat_parts(memo[&n], memo[&d]),
                View::Ball(b) => crate::fp::ball_parts(b.neg, b.man, b.exp, b.rad.m, b.rad.e),
                View::App(h, a) => crate::fp::app(memo[&h], a.iter().map(|x| memo[x])),
            };
            memo.insert(u, v);
        }
        memo[&t]
    }

    // ---------------------------------------------------------------------------------------------- collection

    fn payload_bytes(&self, k: &Kind) -> usize {
        match k {
            Kind::Arena { len, .. } => 8 * *len as usize,
            Kind::Boxed { data, .. } => boxed_size(data),
            _ => 0,
        }
    }

    /// A safe point (spike 1b's contract): every handle still needed is in `roots`, which a collection rewrites.
    pub fn safe_point(&mut self, roots: &mut [&mut [Term]], force: bool) {
        let Gc::Gen { nursery } = self.cfg.gc else { return };
        let stress = self.cfg.stress;
        let minor = self.nodes.len() > self.young_lo && (force || stress || self.alloc >= nursery);
        if minor {
            self.compact(roots, true);
            self.minors += 1;
        }
        let major = minor && ((stress && self.minors.is_multiple_of(4)) || self.promoted >= self.min_bytes.max(self.live));
        if major {
            self.compact(roots, false);
        }
    }

    /// A full collection now (a minor one, then a major one), whatever the trigger says.
    pub fn collect_all(&mut self, roots: &mut [&mut [Term]]) {
        if !self.gen() {
            return;
        }
        if self.nodes.len() > self.young_lo {
            self.compact(roots, true);
            self.minors += 1;
        }
        self.compact(roots, false);
    }

    fn compact(&mut self, roots: &mut [&mut [Term]], minor: bool) {
        let t0 = Instant::now();
        let (lo, args_lo, limbs_lo) =
            if minor { (self.young_lo, self.young_args_lo, self.young_limbs_lo) } else { (0, 0, 0) };
        let n = self.nodes.len();
        let mut bits = vec![0u64; (n - lo).div_ceil(64)];
        for t in roots.iter().flat_map(|r| r.iter()) {
            if t.is_node() && t.idx() >= lo {
                set(&mut bits, t.idx() - lo);
            }
        }
        // children have smaller ids than parents: one descending pass, no stack
        for i in (lo..n).rev() {
            if !bit(&bits, i - lo) {
                continue;
            }
            let kids: [Option<Term>; 2] = match &self.nodes[i].kind {
                Kind::App { head, start, len } => {
                    for c in &self.args[*start as usize..(*start + *len) as usize] {
                        if c.is_node() && c.idx() >= lo {
                            debug_assert!(c.idx() < i, "child after parent");
                            set(&mut bits, c.idx() - lo);
                        }
                    }
                    [Some(*head), None]
                }
                Kind::Rat { num, den } => [Some(*num), Some(*den)],
                _ => [None, None],
            };
            for c in kids.into_iter().flatten() {
                if c.is_node() && c.idx() >= lo {
                    debug_assert!(c.idx() < i, "child after parent");
                    set(&mut bits, c.idx() - lo);
                }
            }
        }
        let mut prefix = Vec::with_capacity(bits.len());
        let mut acc = 0usize;
        for w in &bits {
            prefix.push(acc);
            acc += w.count_ones() as usize;
        }
        let alive = |t: Term| !t.is_node() || t.idx() < lo || bit(&bits, t.idx() - lo);
        let remap = |t: Term| -> Term {
            if !t.is_node() || t.idx() < lo {
                return t;
            }
            let j = t.idx() - lo;
            debug_assert!(bit(&bits, j), "remapping a dead term");
            let below = (bits[j >> 6] & ((1u64 << (j & 63)) - 1)).count_ones() as usize;
            Term((lo + prefix[j >> 6] + below) as u32)
        };
        // slide nodes, argument slots and arena payloads down in ascending order; dead boxes are freed here
        let (mut dst, mut adst, mut ldst) = (lo, args_lo, limbs_lo);
        let (mut moved, mut freed, mut freed_bytes, mut payload_live) = (0usize, 0usize, 0usize, 0usize);
        for i in lo..n {
            if !bit(&bits, i - lo) {
                if let Kind::Boxed { data, .. } = &self.nodes[i].kind {
                    freed += 1;
                    freed_bytes += boxed_size(data);
                }
                // finalisation: dropping the kind frees an out-of-arena payload
                self.nodes[i].kind = Kind::Dead;
                continue;
            }
            let mut node = std::mem::replace(&mut self.nodes[i], Node { kind: Kind::Dead, hash: 0, mask: 0 });
            match &mut node.kind {
                Kind::App { head, start, len } => {
                    let (s, l) = (*start as usize, *len as usize);
                    for k in 0..l {
                        self.args[adst + k] = remap(self.args[s + k]);
                    }
                    *head = remap(*head);
                    *start = adst as u32;
                    adst += l;
                }
                Kind::Rat { num, den } => {
                    *num = remap(*num);
                    *den = remap(*den);
                }
                Kind::Arena { start, len, .. } => {
                    let (s, l) = (*start as usize, *len as usize);
                    if s != ldst {
                        self.limbs.copy_within(s..s + l, ldst);
                        moved += l;
                    }
                    *start = ldst as u32;
                    ldst += l;
                    payload_live += 8 * l;
                }
                Kind::Boxed { data, .. } => payload_live += boxed_size(data),
                _ => {}
            }
            self.nodes[dst] = node;
            dst += 1;
        }
        self.nodes.truncate(dst);
        self.args.truncate(adst);
        self.limbs.truncate(ldst);
        self.boxed_bytes -= freed_bytes;
        for r in roots.iter_mut() {
            for t in r.iter_mut() {
                *t = remap(*t);
            }
        }
        let lazy = self.lazy_limit();
        if minor {
            self.young.clear();
            let ar = Arrays { nodes: &self.nodes, args: &self.args, limbs: &self.limbs, lazy };
            let nodes = &self.nodes;
            for j in lo..dst {
                if !ar.uninterned(Term(j as u32)) {
                    self.table.insert_unique(nodes[j].hash, Term(j as u32), |t| nodes[t.idx()].hash);
                }
            }
            self.promoted += (dst - lo) * NODE + (adst - args_lo) * SLOT + payload_live;
        } else {
            self.table.retain(|t| {
                if alive(*t) {
                    *t = remap(*t);
                    true
                } else {
                    false
                }
            });
            self.live = dst * NODE + adst * SLOT + 8 * ldst + self.boxed_bytes;
            self.promoted = 0;
        }
        self.young_lo = dst;
        self.young_args_lo = adst;
        self.young_limbs_lo = ldst;
        self.alloc = 0;
        self.shrink();
        if self.cfg.stress || cfg!(debug_assertions) {
            self.verify();
        }
        let live_payload = 8 * self.limbs.len() + self.boxed_bytes;
        self.log.push(GcEvent {
            kind: if minor { "minor" } else { "major" },
            pause: t0.elapsed(),
            after: self.reserved(),
            live_nodes: self.nodes.len(),
            live_payload,
            moved_words: moved,
            freed_boxes: freed,
            freed_bytes,
        });
    }

    fn shrink(&mut self) {
        fn fit<T>(v: &mut Vec<T>, slack: usize) {
            let n = v.len();
            if v.capacity() > 4 * n + slack {
                v.shrink_to(2 * n + slack);
            }
        }
        let k = self.min_bytes;
        fit(&mut self.nodes, k / NODE);
        fit(&mut self.args, k / SLOT);
        fit(&mut self.limbs, k / 8);
        let (nodes, len) = (&self.nodes, self.table.len());
        if self.table.capacity() > 4 * len + k / NODE {
            self.table.shrink_to(2 * len + k / NODE, |t| nodes[t.idx()].hash);
        }
    }

    /// Invariants (run after every collection in stress mode and in debug builds): every interned node is in
    /// exactly one table and found by its content; children precede parents and are not dead; arena payloads are
    /// contiguous in id order; boxed bytes add up; numbers are canonical.
    pub fn verify(&self) {
        let ar = self.arrays();
        let mut interned = 0;
        let mut next_limb = 0usize;
        let mut boxed = 0usize;
        for (i, n) in self.nodes.iter().enumerate() {
            let t = Term(i as u32);
            let child_ok = |c: Term| {
                !c.is_node() || (c.idx() < i && !matches!(self.nodes[c.idx()].kind, Kind::Dead))
            };
            let key = match &n.kind {
                Kind::Dead => panic!("dead node {i} left in the arena"),
                Kind::App { head, start, len } => {
                    let a = &self.args[*start as usize..(*start + *len) as usize];
                    assert!(child_ok(*head) && a.iter().all(|&c| child_ok(c)), "bad child of {i}");
                    Key::App(*head, a)
                }
                Kind::Word { neg, mag } => {
                    let v = if *neg { -(*mag as i128) } else { *mag as i128 };
                    assert!(v < IMM_MIN as i128 || v > IMM_MAX as i128, "word node holds an immediate value");
                    Key::Word(*neg, *mag)
                }
                Kind::Rat { num, den } => {
                    assert!(child_ok(*num) && child_ok(*den), "bad child of rational {i}");
                    let (nn, dd) = (self.to_int(*num), self.to_int(*den));
                    assert!(!nn.is_zero() && !dd.neg && dd.mag > Nat::from_u64(1), "rational not canonical");
                    if nn.mag.len() <= 64 && dd.mag.len() <= 64 {
                        assert!(crate::gcd::is_one(&crate::gcd::gcd(&nn.mag, &dd.mag)), "rational not reduced");
                    }
                    Key::Rat(*num, *den)
                }
                k @ (Kind::Arena { ball, neg, .. } | Kind::Boxed { ball, neg, .. }) => {
                    if let Kind::Arena { start, len, .. } = k {
                        assert_eq!(*start as usize, next_limb, "arena payloads not contiguous");
                        next_limb += *len as usize;
                    } else {
                        boxed += self.payload_bytes(k);
                    }
                    let w = ar.words(k);
                    if !*ball {
                        assert!(w.len() >= 2 && *w.last().unwrap() != 0, "integer payload not canonical");
                    }
                    Key::Num { ball: *ball, neg: *neg, words: w }
                }
            };
            if ar.uninterned(t) {
                continue;
            }
            interned += 1;
            let found = self
                .table
                .find(n.hash, |u| ar.matches(n.hash, key, *u))
                .or_else(|| self.young.find(n.hash, |u| ar.matches(n.hash, key, *u)));
            assert_eq!(found, Some(&t), "node {i} not found by its content");
        }
        assert_eq!(next_limb, self.limbs.len(), "limb arena has slack");
        assert_eq!(boxed, self.boxed_bytes, "boxed bytes out of step");
        assert_eq!(interned, self.table.len() + self.young.len(), "tables out of step with the arena");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configs() -> Vec<Config> {
        let mk = |name, place, hash| Config {
            name,
            place,
            hash,
            gc: Gc::Gen { nursery: 1 << 20 },
            stress: true,
            count_boxed: true,
        };
        vec![
            mk("arena", Placement::Arena, Hashing::Eager),
            mk("boxed", Placement::Boxed, Hashing::Eager),
            mk("hybrid", Placement::Hybrid(4), Hashing::Eager),
            mk("sampled", Placement::Hybrid(4), Hashing::Sampled),
            mk("lazy", Placement::Hybrid(4), Hashing::Lazy(4)),
        ]
    }

    #[test]
    fn node_is_32_bytes() {
        assert_eq!(NODE, 32);
    }

    #[test]
    fn integers_are_canonical() {
        for cfg in configs() {
            let mut h = Heap::new(cfg);
            // immediates at the edges, words beyond them
            assert!(h.int(IMM_MAX).is_imm_int() && h.int(IMM_MIN).is_imm_int());
            assert!(h.int(IMM_MAX + 1).is_node() && h.int(IMM_MIN - 1).is_node());
            assert_eq!(h.int(1 << 40), h.int(1 << 40));
            // promotion on overflow, demotion of results that fit
            let a = h.int(IMM_MAX);
            let s = h.add(a, a);
            assert!(s.is_node());
            assert_eq!(h.to_int(s), Int::from_i64(2 * IMM_MAX));
            let m = h.int(-IMM_MAX);
            let m2 = h.add(s, m);
            let back = h.add(m2, m);
            assert_eq!(back, h.int(0));
            // a big integer built two ways is the same term (or same() under Lazy)
            let two = h.int(2);
            let mut p = h.int(1);
            for _ in 0..300 {
                p = h.mul(p, two);
            }
            let q = h.int_parts(false, Nat::pow2(300).limbs);
            assert!(h.same(p, q), "{}", cfg.name);
            if !matches!(cfg.hash, Hashing::Lazy(_)) {
                assert_eq!(p, q);
            }
            let f = h.sym("f");
            let fp = h.app(f, &[p]);
            let fq = h.app(f, &[q]);
            assert_eq!(fp, fq, "compounds over equal numbers are one term ({})", cfg.name);
            // 2^300 - 2^300 + 7 demotes to the immediate 7
            let np = h.neg(q);
            let z = h.add(p, np);
            let seven = h.int(7);
            assert_eq!(h.add(z, seven), seven);
            // a two-limb product that shrinks back to a word and to an immediate by division-free means
            let big = h.int_parts(true, vec![0, 1]);
            assert_eq!(h.to_int(big), Int::new(true, Nat::from_limbs(vec![0, 1])));
            let pbig = h.int_parts(false, vec![0, 1]);
            assert_eq!(h.add(big, pbig), h.int(0));
        }
    }

    #[test]
    fn rationals_are_canonical() {
        for cfg in configs() {
            let mut h = Heap::new(cfg);
            let a = h.recip(6);
            let b = h.recip(3);
            let s = h.add(a, b);
            assert_eq!(s, h.recip(2));
            let two = h.int(2);
            assert_eq!(h.mul(s, two), h.int(1));
            let m = h.recip(-4);
            match h.view(m) {
                View::Rat(n, d) => {
                    assert_eq!(h.to_int(n), Int::from_i64(-1));
                    assert_eq!(h.to_int(d), Int::from_i64(4));
                }
                _ => panic!("1/-4 is not a rational term"),
            }
        }
    }

    #[test]
    fn collection_keeps_values_and_frees_payloads() {
        for cfg in configs() {
            let mut h = Heap::new(cfg);
            let three = h.int(3);
            let mut keep = vec![];
            let mut want = vec![];
            let mut x = h.int(1);
            for k in 0..200 {
                x = h.mul(x, three);
                let r = h.recip(k + 2);
                let s = h.add(x, r);
                if k % 7 == 0 {
                    keep.push(s);
                    want.push(h.fingerprint(s));
                }
                let mut cur = [x];
                h.safe_point(&mut [&mut keep, &mut cur], false);
                x = cur[0];
            }
            let got: Vec<u64> = keep.iter().map(|&t| h.fingerprint(t)).collect();
            assert_eq!(got, want, "{}", cfg.name);
            assert!(h.gc_log().iter().any(|e| e.kind == "major"));
            // drop everything: payloads must be gone
            h.collect_all(&mut []);
            assert_eq!(h.node_count(), 0);
            assert_eq!(h.boxed_bytes, 0);
            assert!(h.limbs.is_empty());
        }
    }
}
