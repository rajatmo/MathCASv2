//! The heap: construction, inspection and the memo, all through `&self` from any number of threads.
//!
//! A new term is looked up in the old table (plain loads: the old region does not change until the next safe
//! point), then in the young table (lock-free probe, shard lock on a miss), and if absent stored in the young arena.
//! The old probe is skipped when any part of the term is young; this filter is exact, because old nodes never refer
//! to young ones (I1.1). Equal terms therefore get equal handles, whichever thread builds them.
use crate::gc::GcState;
use crate::handle::{is_imm, is_old, is_sym, is_young, small_bits, small_value, sym_bits, IMM, LOW, SMALL_MAX, SMALL_MIN, YOUNG};
use crate::hash::{app_hash, packed_hash, payload_hash, rat_hash, real_hash, small_int_hash, word_hash};
use crate::memo::{Item, OldMemo, SideMemo, YoungMemo};
use crate::node::{Kind, Node, Region, MAX_LEN};
use crate::old::OldRegion;
use crate::symbols::Symbols;
use crate::unpack::Unpacked;
use crate::young::{Body, YoungArena, YoungTable};
use mc_api::packed::{build_nested, int_words, pack_shape, real_words, words_ints, words_reals, Elem, MAX_RANK};
use mc_api::{names, Packed, Store, Sym, View, PACK_MIN, T};
use rayon::prelude::*;
use std::cell::Cell;

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
    /// the arguments of packed arrays viewed in this cycle (emptied at every collection)
    pub(crate) unpacked: Unpacked,
    /// the handle bits of the symbol `list`, the head of packed arrays
    pub(crate) list: u32,
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
        Kind::Packed => unreachable!("packed arrays are viewed through the heap"),
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

/// The `head` word of a packed node: bit 0 the leaf kind (set for integers), the rank above it.
#[inline]
fn packed_flags(ints: bool, rank: usize) -> u32 {
    (rank as u32) << 1 | ints as u32
}

/// Packed arrays of more leaves than this are unpacked on the pool (when viewed on a pool thread).
const PAR_UNPACK: usize = 1 << 14;

impl Heap {
    /// An empty heap.
    pub fn new(cfg: Config) -> Heap {
        let syms = Symbols::new();
        let list = sym_bits(syms.intern(names::LIST).0);
        Heap {
            cfg,
            syms,
            old: OldRegion::new(),
            young: YoungArena::new(),
            ytable: YoungTable::new(),
            ymemo: YoungMemo::new(),
            omemo: OldMemo::default(),
            side: SideMemo::new(),
            gc: GcState::default(),
            unpacked: Unpacked::new(),
            list,
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

    /// Intern a payload node from words it takes over: a long payload becomes the node's box without a copy.
    fn intern_owned(&self, proto: Node, words: Vec<u64>) -> u32 {
        let raw = Box::into_raw(words.into_boxed_slice());
        // SAFETY: `raw` is a live boxed slice until it is either handed to the arena or freed below.
        let w: &[u64] = unsafe { &*raw };
        let free = || {
            // SAFETY: the box was not handed over; it is freed once.
            drop(unsafe { Box::from_raw(raw) })
        };
        if self.old.table.count > 0 {
            if let Some(id) = self.old.table.find(proto.hash, |id| same(&self.old, self.old.node(id), &proto, Body::Words(w))) {
                free();
                return id;
            }
        }
        let taken = Cell::new(false);
        let id = self.ytable.intern(
            proto.hash,
            |id| same(&self.young, self.young.node(id), &proto, Body::Words(w)),
            || {
                taken.set(true);
                // SAFETY: ownership of the box passes to the arena exactly once (`taken`).
                self.young.store_box(proto, unsafe { Box::from_raw(raw) })
            },
        );
        if !taken.get() {
            free();
        }
        YOUNG | id
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
    // Packed arrays (semantics 11, decisions N4a.1 to N4a.4)

    /// The payload words of a stored node, from its region.
    #[inline]
    fn words_of(&self, b: u32, n: &Node) -> &[u64] {
        if b & YOUNG != 0 {
            self.young.words(n)
        } else {
            self.old.words(n)
        }
    }

    /// A machine number as a leaf: its kind (true for an integer) and payload word.
    #[inline]
    fn leaf(&self, b: u32) -> Option<(bool, u64)> {
        if is_imm(b) {
            return (!is_sym(b)).then(|| (true, small_value(b) as u64));
        }
        let n = self.node(b);
        match n.kind() {
            Kind::Real => Some((false, n.data)),
            Kind::Word => {
                let neg = n.head != 0;
                if !neg && n.data <= i64::MAX as u64 {
                    Some((true, n.data))
                } else if neg && n.data <= 1 << 63 {
                    Some((true, (n.data as i64).wrapping_neg() as u64))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// The packed node of `words` (the dimensions, then the leaves).
    fn packed_node<'h>(&'h self, ints: bool, rank: usize, words: Vec<u64>) -> T<'h> {
        let (dims, data) = words.split_at(rank);
        let (hash, mask) = packed_hash(self.meta_bits(self.list), ints, dims, data);
        let node = Node::new(Kind::Packed, hash, mask, packed_flags(ints, rank), 0, words.len());
        T::from_raw(self.intern_owned(node, words))
    }

    /// `list(args)` as a packed array when the packing rule says so (`mc_api::packed`): leaves of one kind, rows
    /// that are packed arrays of one shape, or short compound rows that together reach the threshold.
    fn try_pack<'h>(&'h self, args: &[T<'h>]) -> Option<T<'h>> {
        let n = args.len();
        let first = args[0].raw();
        if let Some((ints, _)) = self.leaf(first) {
            if n < PACK_MIN || n + 1 > MAX_LEN {
                return None;
            }
            let mut words = Vec::with_capacity(n + 1);
            words.push(n as u64);
            for a in args {
                match self.leaf(a.raw()) {
                    Some((k, w)) if k == ints => words.push(w),
                    _ => return None,
                }
            }
            return Some(self.packed_node(ints, 1, words));
        }
        if is_imm(first) {
            return None;
        }
        let f = self.node(first);
        match f.kind() {
            Kind::Packed => {
                let rank = (f.head >> 1) as usize;
                let fw = self.words_of(first, f);
                let (dims, row) = (&fw[..rank], fw.len() - rank);
                if rank + 1 > MAX_RANK || n.checked_mul(row).is_none_or(|w| w + rank + 1 > MAX_LEN) {
                    return None;
                }
                let mut words = Vec::with_capacity(rank + 1 + n * row);
                words.push(n as u64);
                words.extend_from_slice(dims);
                for a in args {
                    let b = a.raw();
                    if is_imm(b) {
                        return None;
                    }
                    let x = self.node(b);
                    if x.kind() != Kind::Packed || x.head != f.head || x.len() != f.len() {
                        return None;
                    }
                    let xw = self.words_of(b, x);
                    if &xw[..rank] != dims {
                        return None;
                    }
                    words.extend_from_slice(&xw[rank..]);
                }
                Some(self.packed_node(f.head & 1 != 0, rank + 1, words))
            }
            Kind::App if f.head == self.list => {
                let (elem, dims, data) = pack_shape(self, args)?;
                if data.len() + dims.len() > MAX_LEN {
                    return None;
                }
                let rank = dims.len();
                let mut words = dims;
                words.extend_from_slice(&data);
                Some(self.packed_node(elem == Elem::Int, rank, words))
            }
            _ => None,
        }
    }

    /// May an array of these dimensions and `len` leaves be built packed directly? Panics unless the dimensions
    /// hold `len` leaves (as the contract's default does).
    fn packable(dims: &[usize], len: usize) -> bool {
        let product = dims.iter().try_fold(1usize, |a, &d| a.checked_mul(d));
        assert_eq!(product, Some(len), "the dimensions {dims:?} do not hold {len} leaves");
        (1..=MAX_RANK).contains(&dims.len()) && len >= PACK_MIN && len + dims.len() <= MAX_LEN
    }

    /// A packed array straight from its leaves' words.
    fn array_of<'h>(&'h self, ints: bool, dims: &[usize], data: &[u64]) -> T<'h> {
        let mut words = Vec::with_capacity(dims.len() + data.len());
        words.extend(dims.iter().map(|&d| d as u64));
        words.extend_from_slice(data);
        self.packed_node(ints, dims.len(), words)
    }

    /// The payload view of packed node `b`.
    #[inline]
    fn packed_view<'h>(&'h self, b: u32, n: &'h Node) -> Packed<'h> {
        let w = self.words_of(b, n);
        let (dims, data) = w.split_at((n.head >> 1) as usize);
        if n.head & 1 == 0 {
            Packed::Reals { dims, data: words_reals(data) }
        } else {
            Packed::Ints { dims, data: words_ints(data) }
        }
    }

    /// The arguments of packed node `b`: its leaves as terms (rank 1) or its rows as arrays, made on first use in
    /// this cycle and kept until the next collection (decision N4a.4).
    fn unpacked_args<'h>(&'h self, b: u32, n: &'h Node) -> &'h [T<'h>] {
        let a = self.unpacked.get_or_make(b, || {
            let p = self.packed_view(b, n);
            let par = p.count() >= PAR_UNPACK && rayon::current_thread_index().is_some();
            let dims = p.dims();
            if dims.len() == 1 {
                return match p {
                    Packed::Reals { data, .. } if par => {
                        data.par_iter().with_min_len(4096).map(|&v| self.real(v).raw()).collect()
                    }
                    Packed::Reals { data, .. } => data.iter().map(|&v| self.real(v).raw()).collect(),
                    Packed::Ints { data, .. } if par => {
                        data.par_iter().with_min_len(4096).map(|&v| self.int(v).raw()).collect()
                    }
                    Packed::Ints { data, .. } => data.iter().map(|&v| self.int(v).raw()).collect(),
                };
            }
            let sub: Vec<usize> = dims[1..].iter().map(|&d| d as usize).collect();
            let row = p.count() / dims[0] as usize;
            let make = |i: usize| match p {
                Packed::Reals { data, .. } => self.real_array(&sub, &data[i * row..(i + 1) * row]).raw(),
                Packed::Ints { data, .. } => self.int_array(&sub, &data[i * row..(i + 1) * row]).raw(),
            };
            if par {
                (0..dims[0] as usize).into_par_iter().map(make).collect()
            } else {
                (0..dims[0] as usize).map(make).collect()
            }
        });
        // SAFETY: `T` is a transparent `u32` plus a zero-sized brand; the slice lives until the next collection,
        // which needs `&mut self`.
        unsafe { std::slice::from_raw_parts(a.as_ptr() as *const T<'h>, a.len()) }
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
        self.unpacked.bytes()
            + self.old.reserved_bytes()
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
        if head.raw() == self.list && !args.is_empty() {
            if let Some(t) = self.try_pack(args) {
                return t;
            }
        }
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
            let n = self.young.node(b & LOW);
            if n.kind() == Kind::Packed {
                return View::App { head: T::from_raw(self.list), args: self.unpacked_args(b, n) };
            }
            node_view(&self.young, n)
        } else {
            let n = self.old.node(b);
            if n.kind() == Kind::Packed {
                return View::App { head: T::from_raw(self.list), args: self.unpacked_args(b, n) };
            }
            node_view(&self.old, n)
        }
    }

    #[inline]
    fn structural_hash(&self, t: T<'_>) -> u64 {
        self.meta_bits(t.raw()).0
    }

    #[inline]
    fn packed<'h>(&'h self, t: T<'h>) -> Option<Packed<'h>> {
        let b = t.raw();
        if is_imm(b) {
            return None;
        }
        let n = self.node(b);
        (n.kind() == Kind::Packed).then(|| self.packed_view(b, n))
    }

    fn real_array<'h>(&'h self, dims: &[usize], data: &[f64]) -> T<'h> {
        if Heap::packable(dims, data.len()) {
            self.array_of(false, dims, real_words(data))
        } else {
            build_nested(self, dims, data.len(), |i| self.real(data[i]))
        }
    }

    fn int_array<'h>(&'h self, dims: &[usize], data: &[i64]) -> T<'h> {
        if Heap::packable(dims, data.len()) {
            self.array_of(true, dims, int_words(data))
        } else {
            build_nested(self, dims, data.len(), |i| self.int(data[i]))
        }
    }
}

/// Is `b` an immediate integer handle? (Verification of canonical forms.)
pub(crate) fn is_small_int(b: u32) -> bool {
    b & IMM != 0 && !is_sym(b)
}
