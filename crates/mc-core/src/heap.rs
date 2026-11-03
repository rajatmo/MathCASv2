//! The heap: construction, inspection and the memo, all through `&self` from any number of threads.
//!
//! A new term is looked up in the old table (plain loads: the old region does not change until the next safe
//! point), then in the young table (lock-free probe, shard lock on a miss), and if absent stored in the young arena.
//! The old probe is skipped when any part of the term is young; this filter is exact, because old nodes never refer
//! to young ones (I1.1). Equal terms therefore get equal handles, whichever thread builds them.
use crate::gc::GcState;
use crate::handle::{is_imm, is_old, is_sym, is_young, small_bits, small_value, sym_bits, IMM, LOW, SMALL_MAX, SMALL_MIN, YOUNG};
use crate::hash::{app_hash, payload_hash, rat_hash, real_hash, small_int_hash, word_hash};
use crate::memo::{Item, OldMemo, SideMemo, YoungMemo};
use crate::node::{Kind, Node, Region};
use crate::old::OldRegion;
use crate::symbols::Symbols;
use crate::young::{Body, YoungArena, YoungTable};
use mc_api::{Store, Sym, View, T};

/// Heap configuration.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Collect at safe points. Off, every term stays young (for references and measurements).
    pub collect: bool,
    /// A minor collection once the young region holds this many bytes, payloads included (1b.1, I3.3).
    pub nursery: usize,
    /// A major collection once the bytes promoted since the last one exceed the live size, and at least this.
    pub min_major: usize,
    /// Stress mode: a minor collection at every safe point with young terms, a major one every fourth, and
    /// [`Heap::verify`] before and after each.
    pub stress: bool,
    /// Run the copying phases of collections on the pool's workers when the safe point runs on a pool thread (I1.7).
    pub par_gc: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config { collect: true, nursery: 1 << 20, min_major: 4 << 20, stress: false, par_gc: true }
    }
}

impl Config {
    /// No collection.
    pub fn no_collect() -> Config {
        Config { collect: false, ..Config::default() }
    }
    /// Stress mode (see [`Config::stress`]).
    pub fn stress() -> Config {
        Config { stress: true, ..Config::default() }
    }
}

/// A memo hit: the normal form with its stamps (semantics 4.4; spike 4's start epoch and read set, 4.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoHit<'h> {
    /// the normal form
    pub value: T<'h>,
    /// the start epoch of the evaluation that made it
    pub epoch: u32,
    /// its read set
    pub deps: u64,
}

/// The store of MathCAS v2: a hash-consed arena of two regions, collected at safe points.
pub struct Heap {
    pub(crate) cfg: Config,
    pub(crate) syms: Symbols,
    pub(crate) old: OldRegion,
    pub(crate) young: YoungArena,
    pub(crate) ytable: YoungTable,
    pub(crate) ymemo: YoungMemo,
    pub(crate) omemo: OldMemo,
    pub(crate) side: SideMemo,
    pub(crate) gc: GcState,
}

impl Default for Heap {
    fn default() -> Heap {
        Heap::new(Config::default())
    }
}

/// Is `n` the node `proto` would make with this body?
#[inline]
pub(crate) fn same<R: Region>(r: &R, n: &Node, proto: &Node, body: Body) -> bool {
    n.hash == proto.hash
        && n.meta() == proto.meta()
        && n.head == proto.head
        && match body {
            Body::Inline => n.data == proto.data,
            Body::Args(a) => r.args(n) == a,
            Body::Words(w) => r.words(n) == w,
        }
}

/// The view of a stored node.
#[inline]
fn node_view<'h, R: Region>(r: &'h R, n: &'h Node) -> View<'h> {
    match n.kind() {
        Kind::App => {
            let a = r.args(n);
            // SAFETY: `T` is a transparent `u32` plus a zero-sized brand; the slots live for the borrow `'h`.
            let args = unsafe { std::slice::from_raw_parts(a.as_ptr() as *const T<'h>, a.len()) };
            View::App { head: T::from_raw(n.head), args }
        }
        Kind::Word => {
            let neg = n.head != 0;
            if !neg && n.data <= i64::MAX as u64 {
                View::Int(n.data as i64)
            } else if neg && n.data <= 1 << 63 {
                View::Int((n.data as i64).wrapping_neg())
            } else {
                View::BigInt { neg, mag: std::slice::from_ref(&n.data) }
            }
        }
        Kind::Rat => View::Rational { num: T::from_raw(n.head), den: T::from_raw(n.data as u32) },
        Kind::Real => View::Real(f64::from_bits(n.data)),
        Kind::Int => View::BigInt { neg: n.head != 0, mag: r.words(n) },
        Kind::Ball => View::Ball { neg: n.head != 0, words: r.words(n) },
        Kind::Str => {
            let w = r.words(n);
            // SAFETY: the words hold `head` bytes of UTF-8 (copied from a `&str`), followed by zero padding.
            View::Str(unsafe { std::str::from_utf8_unchecked(&words_as_bytes(w)[..n.head as usize]) })
        }
    }
}

fn words_as_bytes(w: &[u64]) -> &[u8] {
    // SAFETY: any initialised `u64` is eight initialised bytes; the borrow is the same.
    unsafe { std::slice::from_raw_parts(w.as_ptr() as *const u8, w.len() * 8) }
}

/// The kind tags payload hashes are seeded with.
const TAG_INT: u64 = 1;
const TAG_BALL: u64 = 2;
const TAG_STR: u64 = 3;

impl Heap {
    /// An empty heap.
    pub fn new(cfg: Config) -> Heap {
        Heap {
            cfg,
            syms: Symbols::new(),
            old: OldRegion::new(),
            young: YoungArena::new(),
            ytable: YoungTable::new(),
            ymemo: YoungMemo::new(),
            omemo: OldMemo::default(),
            side: SideMemo::new(),
            gc: GcState::default(),
        }
    }

    /// The heap's configuration.
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    // ----------------------------------------------------------------------------------------------------------
    // Nodes

    #[inline]
    pub(crate) fn node(&self, b: u32) -> &Node {
        if b & YOUNG != 0 {
            self.young.node(b & LOW)
        } else {
            self.old.node(b)
        }
    }

    /// Structural hash and symbol mask of any handle.
    #[inline]
    pub(crate) fn meta_bits(&self, b: u32) -> (u64, u64) {
        if is_imm(b) {
            if is_sym(b) {
                let i = b & LOW;
                (self.syms.hash(i), 1u64 << (i % 64))
            } else {
                (small_int_hash(small_value(b)), 0)
            }
        } else {
            let n = self.node(b);
            (n.hash, n.mask)
        }
    }

    fn intern(&self, proto: Node, body: Body, may_be_old: bool) -> u32 {
        if may_be_old && self.old.table.count > 0 {
            if let Some(id) = self.old.table.find(proto.hash, |id| same(&self.old, self.old.node(id), &proto, body)) {
                return id;
            }
        }
        YOUNG
            | self.ytable.intern(
                proto.hash,
                |id| same(&self.young, self.young.node(id), &proto, body),
                || self.young.store(proto, body),
            )
    }

    fn word<'h>(&'h self, neg: bool, mag: u64) -> T<'h> {
        let n = Node::new(Kind::Word, word_hash(neg, mag), 0, neg as u32, mag, 0);
        T::from_raw(self.intern(n, Body::Inline, true))
    }

    fn payload<'h>(&'h self, kind: Kind, flag: u32, words: &[u64]) -> T<'h> {
        let tag = match kind {
            Kind::Int => TAG_INT,
            Kind::Ball => TAG_BALL,
            _ => TAG_STR,
        };
        let n = Node::new(kind, payload_hash(tag, flag, words), 0, flag, 0, words.len());
        T::from_raw(self.intern(n, Body::Words(words), true))
    }

    // ----------------------------------------------------------------------------------------------------------
    // Inspection beyond the contract

    /// Structural hash and symbol mask (semantics 4.3: a normal-form stamp is checked against the mask).
    #[inline]
    pub fn meta(&self, t: T<'_>) -> (u64, u64) {
        self.meta_bits(t.raw())
    }

    /// The symbol mask of a term: bit `i % 64` for every symbol `i` it contains. Numbers and strings have none.
    #[inline]
    pub fn mask(&self, t: T<'_>) -> u64 {
        self.meta_bits(t.raw()).1
    }

    /// Number of symbols made so far.
    pub fn sym_count(&self) -> usize {
        self.syms.len()
    }

    // ----------------------------------------------------------------------------------------------------------
    // The memo

    /// The memoised normal form of `t` with its stamps, if an entry holds it. Immediates are not memoised.
    #[inline]
    pub fn memo_get<'h>(&'h self, t: T<'h>) -> Option<MemoHit<'h>> {
        let b = t.raw();
        let it = if is_young(b) {
            self.ymemo.entry(b & LOW)?.get(b)
        } else if is_old(b) {
            self.omemo.entry(b).and_then(|e| e.get(b)).or_else(|| self.side.get(b))
        } else {
            None
        }?;
        Some(MemoHit { value: T::from_raw(it.val), epoch: it.epoch, deps: it.deps })
    }

    /// Memoise `value` as the normal form of `key`, stamped with the evaluation's start epoch and read set. Lossy:
    /// a concurrent writer of the same entry wins. An entry with an old key and a young value goes to the side memo
    /// (and empties the old memo's entry), so the old memo never refers to a young term.
    #[inline]
    pub fn memo_put(&self, key: T<'_>, value: T<'_>, epoch: u32, deps: u64) {
        let (k, v) = (key.raw(), value.raw());
        let it = Item { key: k, val: v, epoch, deps };
        if is_young(k) {
            self.ymemo.entry_or_alloc(k & LOW).put(it);
        } else if is_old(k) {
            if is_young(v) {
                self.side.put(it);
                self.omemo.entry(k).expect("memo key outside the old region").invalidate();
            } else {
                self.omemo.entry(k).expect("memo key outside the old region").put(it);
            }
        }
    }

    // ----------------------------------------------------------------------------------------------------------
    // Accounting

    /// Distinct stored terms (old and young).
    pub fn stored(&mut self) -> usize {
        self.old.nodes.len() + self.ytable.len()
    }

    /// Old nodes.
    pub fn old_len(&self) -> usize {
        self.old.nodes.len()
    }

    /// Bytes reserved by the heap: both regions with their tables, boxed payloads, and the memos.
    pub fn reserved_bytes(&mut self) -> usize {
        self.old.reserved_bytes()
            + self.young.reserved_bytes()
            + self.young.boxed_bytes()
            + self.ytable.bytes()
            + self.ymemo.bytes()
            + self.omemo.bytes()
            + self.side.bytes()
    }

    /// Bytes of boxed payloads (old and young).
    pub fn boxed_bytes(&self) -> usize {
        self.old.boxed + self.young.boxed_bytes()
    }
}

impl Store for Heap {
    fn sym(&self, name: &str) -> Sym {
        self.syms.intern(name)
    }

    fn sym_name(&self, s: Sym) -> String {
        self.syms.name(s)
    }

    fn sym_term<'h>(&'h self, s: Sym) -> T<'h> {
        T::from_raw(sym_bits(s.0))
    }

    fn int<'h>(&'h self, v: i64) -> T<'h> {
        if (SMALL_MIN..=SMALL_MAX).contains(&v) {
            T::from_raw(small_bits(v))
        } else {
            self.word(v < 0, v.unsigned_abs())
        }
    }

    /// The one canonicalising integer constructor (I3.1): trims leading zero limbs, then makes an immediate if the
    /// value fits 30 bits, a `Word` node if the magnitude is one limb, and a payload otherwise.
    fn big_int<'h>(&'h self, neg: bool, mag: &[u64]) -> T<'h> {
        let n = mag.iter().rposition(|&w| w != 0).map_or(0, |i| i + 1);
        match n {
            0 => T::from_raw(small_bits(0)),
            1 => {
                let m = mag[0];
                if !neg && m <= SMALL_MAX as u64 {
                    T::from_raw(small_bits(m as i64))
                } else if neg && m <= SMALL_MIN.unsigned_abs() {
                    T::from_raw(small_bits(-(m as i64)))
                } else {
                    self.word(neg, m)
                }
            }
            _ => self.payload(Kind::Int, neg as u32, &mag[..n]),
        }
    }

    fn rational<'h>(&'h self, num: T<'h>, den: T<'h>) -> T<'h> {
        debug_assert!(
            matches!(self.view(num), View::Int(v) if v != 0) || matches!(self.view(num), View::BigInt { .. }),
            "rational numerator must be a nonzero integer"
        );
        debug_assert!(
            matches!(self.view(den), View::Int(d) if d > 1) || matches!(self.view(den), View::BigInt { neg: false, .. }),
            "rational denominator must be an integer above 1"
        );
        let (n, d) = (num.raw(), den.raw());
        let hash = rat_hash(self.meta_bits(n).0, self.meta_bits(d).0);
        let node = Node::new(Kind::Rat, hash, 0, n, d as u64, 0);
        T::from_raw(self.intern(node, Body::Inline, !is_young(n) && !is_young(d)))
    }

    fn real<'h>(&'h self, v: f64) -> T<'h> {
        let bits = v.to_bits();
        T::from_raw(self.intern(Node::new(Kind::Real, real_hash(bits), 0, 0, bits, 0), Body::Inline, true))
    }

    fn ball<'h>(&'h self, neg: bool, words: &[u64]) -> T<'h> {
        debug_assert!(words.len() >= 3, "a ball has an exponent and a radius before its mantissa");
        self.payload(Kind::Ball, neg as u32, words)
    }

    fn string<'h>(&'h self, s: &str) -> T<'h> {
        assert!(s.len() < u32::MAX as usize, "string too long");
        let mut w = vec![0u64; s.len().div_ceil(8)];
        for (i, c) in s.as_bytes().chunks(8).enumerate() {
            let mut b = [0u8; 8];
            b[..c.len()].copy_from_slice(c);
            w[i] = u64::from_ne_bytes(b);
        }
        self.payload(Kind::Str, s.len() as u32, &w)
    }

    fn app<'h>(&'h self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        // SAFETY: `T` is a transparent `u32` plus a zero-sized brand.
        let raw: &[u32] = unsafe { std::slice::from_raw_parts(args.as_ptr() as *const u32, args.len()) };
        let (hash, mask) = app_hash(self.meta_bits(head.raw()), raw, |a| self.meta_bits(a));
        let node = Node::new(Kind::App, hash, mask, head.raw(), 0, raw.len());
        let young_part = is_young(head.raw()) || raw.iter().any(|&a| is_young(a));
        T::from_raw(self.intern(node, Body::Args(raw), !young_part))
    }

    #[inline]
    fn view<'h>(&'h self, t: T<'h>) -> View<'h> {
        let b = t.raw();
        if is_imm(b) {
            return if is_sym(b) { View::Sym(Sym(b & LOW)) } else { View::Int(small_value(b)) };
        }
        if b & YOUNG != 0 {
            node_view(&self.young, self.young.node(b & LOW))
        } else {
            node_view(&self.old, self.old.node(b))
        }
    }

    #[inline]
    fn structural_hash(&self, t: T<'_>) -> u64 {
        self.meta_bits(t.raw()).0
    }
}

/// Is `b` an immediate integer handle? (Verification of canonical forms.)
pub(crate) fn is_small_int(b: u32) -> bool {
    b & IMM != 0 && !is_sym(b)
}
