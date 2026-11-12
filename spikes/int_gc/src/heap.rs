//! The heap: an old region (dense, immutable between safe points) and a concurrent young region (spike 4's store C),
//! collected by spike 1b's generational scheme at safe points.
//!
//! **Allocation (between safe points, `&self`, any number of threads).** A term is looked up in the old table
//! (plain loads: the old region does not change until the next safe point), then in the young table (lock-free
//! probe, shard lock on a miss), and if absent stored in the young arena. Canonical ids: equal terms get equal ids,
//! whichever thread builds them (the old table is fixed; the young table is canonical as in spike 4).
//!
//! **Minor collection (at a safe point, `&mut self`).** Young survivors are found from the roots (old nodes never
//! point to young ones: immutability; the old memo holds no young handle by construction) and copied to the end of the
//! old region, where they join the old table; the young arena, table and memo are reset wholesale. Per-thread id
//! chunks break the global child-before-parent order of the young region (a parent can get a smaller id than a child
//! built on another thread), so the stackless descending pass of spike 1b is not available there. Two orders
//! ([`Order`]) are compared:
//! - `Topo` (design D1): an iterative post-order traversal from the roots with an explicit stack; survivors get old
//!   ids in post-order (children first) through a forwarding table over the young indices. The old region keeps
//!   spike 1b's invariant (child id < parent id), so major collections stay stackless with renumbering by rank.
//! - `Chunk` (design D2): per-thread regions in chunk order. Marking is a descending pass over the young indices with
//!   an explicit stack only for out-of-order edges (child index above the parent's); survivors keep their relative
//!   order (renumbering by rank). Out-of-order edges then live on in the old region, whose major marking needs the
//!   same fallback.
//!
//! **Major collection.** Sliding compaction of the old region (spike 1b's B): marking (stackless in `Topo`), ranks as
//! new ids, arguments slide in the same pass, the old table is rebuilt from its slots (no node reads), the old memo
//! is rewritten in place (weak: an entry lives only while its key and value do).
use crate::memo::{Item, Memo};
use crate::young::{Node, YoungArena, YoungTable, NODE, TAG_INT};
use crate::{finalize, mix, str_hash, Raw, Sym, Trace, IMM, IMM_MAX, IMM_MIN, K, PAYLOAD, SYMB, T, YOUNG};
use crossbeam_utils::CachePadded;
use parking_lot::Mutex;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicU64, Ordering::*};
use std::time::{Duration, Instant};

const SLOT: usize = 4;
const MAX_SYMS: usize = 1 << 16;
const PAR_ARGS: usize = 1 << 15;
const PAR_CHUNK: usize = 1 << 13;
const MAX_THREADS: usize = 64;
const MIN_MEMO_BITS: u32 = 14;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    /// D1: topological promotion (post-order traversal, forwarding table).
    Topo,
    /// D2: per-thread regions in chunk order (descending pass, stack for out-of-order edges, ranks).
    Chunk,
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// collect at safe points (off: spike 4's store, everything stays young)
    pub collect: bool,
    /// minor collection once the young region holds this many bytes (decision 1b.1: about 1 MB)
    pub nursery: usize,
    pub order: Order,
    /// collect at every safe point (a major one every fourth) and verify the invariants before and after
    pub stress: bool,
    /// count young hits on nodes another worker created (what private per-thread tables would duplicate)
    pub instrument: bool,
    /// the old memo grows with the old region up to 2^old_memo_bits entries (from 2^MIN_MEMO_BITS)
    pub old_memo_bits: u32,
    pub young_memo_bits: u32,
    /// run the copying phases of collections on the pool's idle workers (marking stays sequential)
    pub par_gc: bool,
}

impl Config {
    pub fn none() -> Config {
        // without collection every term stays young: the young memo is spike 4's memo of 2^20 entries
        Config { collect: false, nursery: 1 << 20, order: Order::Topo, stress: false, instrument: false, old_memo_bits: 20, young_memo_bits: 20, par_gc: true }
    }
    pub fn gen(order: Order) -> Config {
        Config { collect: true, order, young_memo_bits: 16, ..Config::none() }
    }
}

/// One collection.
#[derive(Clone, Debug)]
pub struct GcEvent {
    /// `minor` or `major`
    pub kind: &'static str,
    pub pause: Duration,
    /// bytes reserved by the heap before and after
    pub before: usize,
    pub after: usize,
    /// old nodes after the collection
    pub live: usize,
    /// minor: young nodes promoted
    pub survivors: usize,
    /// edges among the marked nodes whose child index is above the parent's (they need the explicit stack in D2)
    pub out_of_order: usize,
    /// minor: edges among survivors between nodes of different workers (cross-region edges of per-thread nurseries)
    pub cross_owner: usize,
    /// young-table bytes freed (retired arrays and grown arrays)
    pub table_freed: usize,
    /// minor: mark, copy (with old-table insertion), roots and memo, reset (young table, arena, memo fit);
    /// major: mark, slide, table and memo, roots
    pub phases: [Duration; 4],
}

pub enum View<'h> {
    Int(i64),
    /// a symbol, by index
    Sym(u32),
    App(T<'h>, &'h [T<'h>]),
}

#[derive(Default)]
struct SymNames {
    by_name: FxHashMap<Box<str>, u32>,
    names: Vec<Box<str>>,
}

/// The interning table of the old region: open addressing, `(hash >> 32) << 32 | (id + 1)` per slot, at most half
/// full. Changed only at safe points; read by every thread between them with plain loads.
struct OldTable {
    s: Vec<u64>,
    count: usize,
}

impl OldTable {
    fn new(cap: usize) -> OldTable {
        OldTable { s: vec![0; cap.next_power_of_two().max(1 << 12)], count: 0 }
    }
    #[inline]
    fn find(&self, hash: u64, eq: impl Fn(u32) -> bool) -> Option<u32> {
        let mask = self.s.len() - 1;
        let tag = hash >> 32;
        let mut i = tag as usize & mask;
        loop {
            let e = self.s[i];
            if e == 0 {
                return None;
            }
            if e >> 32 == tag && eq((e as u32) - 1) {
                return Some((e as u32) - 1);
            }
            i = (i + 1) & mask;
        }
    }
    fn place(s: &mut [u64], e: u64) {
        let mask = s.len() - 1;
        let mut i = (e >> 32) as usize & mask;
        while s[i] != 0 {
            i = (i + 1) & mask;
        }
        s[i] = e;
    }
    fn insert(&mut self, hash: u64, id: u32) {
        if (self.count + 1) * 2 > self.s.len() {
            self.rebuild(self.s.len() * 2, Some);
        }
        Self::place(&mut self.s, (hash >> 32) << 32 | (id as u64 + 1));
        self.count += 1;
    }
    /// The slots viewed as atomics, for parallel placement (`&mut`: nothing else reads them meanwhile).
    fn atomic(s: &mut [u64]) -> &[AtomicU64] {
        // SAFETY: `AtomicU64` has the size and alignment of `u64`, and the exclusive borrow is held throughout.
        unsafe { std::slice::from_raw_parts(s.as_mut_ptr() as *const AtomicU64, s.len()) }
    }
    fn place_atomic(s: &[AtomicU64], e: u64) {
        let mask = s.len() - 1;
        let mut i = (e >> 32) as usize & mask;
        while s[i].compare_exchange(0, e, Relaxed, Relaxed).is_err() {
            i = (i + 1) & mask;
        }
    }
    /// Insert the nodes `ids` (just promoted) in parallel.
    fn insert_par(&mut self, ids: std::ops::Range<usize>, nodes: &[Node]) {
        self.reserve(ids.len());
        self.count += ids.len();
        let s = Self::atomic(&mut self.s);
        ids.into_par_iter().with_min_len(4096).for_each(|i| {
            Self::place_atomic(s, (nodes[i].hash >> 32) << 32 | (i as u64 + 1));
        });
    }
    /// `rebuild` on the pool.
    fn rebuild_par(&mut self, cap: usize, remap: impl Fn(u32) -> Option<u32> + Sync) {
        let cap = cap.next_power_of_two().max(1 << 12);
        let mut fresh = vec![0u64; cap];
        let f = Self::atomic(&mut fresh);
        let count: usize = self
            .s
            .par_iter()
            .with_min_len(4096)
            .map(|&e| match (e != 0).then(|| remap((e as u32) - 1)).flatten() {
                Some(id) => {
                    Self::place_atomic(f, (e >> 32) << 32 | (id as u64 + 1));
                    1
                }
                None => 0,
            })
            .sum();
        self.s = fresh;
        self.count = count;
    }
    /// Make room for `extra` more entries at once (one re-placement instead of several doublings).
    fn reserve(&mut self, extra: usize) {
        if (self.count + extra) * 2 > self.s.len() {
            self.rebuild(2 * (self.count + extra), Some);
        }
    }
    /// Re-place the surviving entries (renumbered by `remap`) into an array of `cap` slots, from the slots alone.
    fn rebuild(&mut self, cap: usize, remap: impl Fn(u32) -> Option<u32>) {
        let cap = cap.next_power_of_two().max(1 << 12);
        let mut fresh = vec![0u64; cap];
        let mut count = 0;
        for &e in &self.s {
            if e != 0 {
                if let Some(id) = remap((e as u32) - 1) {
                    Self::place(&mut fresh, (e >> 32) << 32 | (id as u64 + 1));
                    count += 1;
                }
            }
        }
        self.s = fresh;
        self.count = count;
    }
}

/// A raw pointer shared by the tasks of a parallel copy, each writing slots no other task writes.
#[derive(Clone, Copy)]
struct SyncPtr<X>(*mut X);
// SAFETY: used only for disjoint writes during a collection, joined before the arrays are read.
unsafe impl<X> Sync for SyncPtr<X> {}
unsafe impl<X> Send for SyncPtr<X> {}

impl<X> SyncPtr<X> {
    /// # Safety
    /// `i` is in bounds and no other task accesses slot `i` meanwhile.
    #[inline]
    unsafe fn at(self, i: usize) -> *mut X {
        unsafe { self.0.add(i) }
    }
}

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

#[inline]
fn imm_value(b: u32) -> i64 {
    (((b << 2) as i32) >> 2) as i64
}

#[inline]
fn is_young(b: u32) -> bool {
    b & (IMM | YOUNG) == YOUNG
}

#[inline]
fn is_old(b: u32) -> bool {
    b & (IMM | YOUNG) == 0
}

fn bit(bits: &[u64], i: usize) -> bool {
    bits[i >> 6] & (1 << (i & 63)) != 0
}

fn set(bits: &mut [u64], i: usize) -> bool {
    let w = &mut bits[i >> 6];
    let m = 1 << (i & 63);
    let was = *w & m != 0;
    *w |= m;
    !was
}

/// Ranks in a bitmap: the prefix count of each 64-bit word.
fn prefix(bits: &[u64]) -> Vec<u32> {
    let mut p = Vec::with_capacity(bits.len());
    let mut acc = 0u32;
    for w in bits {
        p.push(acc);
        acc += w.count_ones();
    }
    p
}

#[inline]
fn rank(bits: &[u64], pre: &[u32], j: usize) -> u32 {
    pre[j >> 6] + (bits[j >> 6] & ((1u64 << (j & 63)) - 1)).count_ones()
}

pub struct Heap {
    pub cfg: Config,
    nodes: Vec<Node>,
    args: Vec<u32>,
    table: OldTable,
    young: YoungArena,
    ytable: YoungTable,
    memo_old: Memo,
    memo_young: Memo,
    syms: Mutex<SymNames>,
    sym_hash: Box<[AtomicU64]>,
    /// bytes live after the last major collection, and promoted since
    live: usize,
    promoted: usize,
    min_bytes: usize,
    minors: usize,
    log: Vec<GcEvent>,
    cross_hits: Box<[CachePadded<AtomicU64>]>,
}

impl Heap {
    pub fn new(cfg: Config) -> Heap {
        Heap {
            cfg,
            nodes: vec![],
            args: vec![],
            table: OldTable::new(0),
            young: YoungArena::new(),
            ytable: YoungTable::new(),
            memo_old: Memo::new(if cfg.collect { MIN_MEMO_BITS } else { 4 }),
            memo_young: Memo::new(cfg.young_memo_bits),
            syms: Mutex::new(SymNames::default()),
            sym_hash: (0..MAX_SYMS).map(|_| AtomicU64::new(0)).collect(),
            live: 0,
            promoted: 0,
            min_bytes: 4 << 20,
            minors: 0,
            log: vec![],
            cross_hits: (0..MAX_THREADS).map(|_| CachePadded::new(AtomicU64::new(0))).collect(),
        }
    }

    // ----------------------------------------------------------------------------------------------------------
    // The concurrent interface (`&'h self`, handles branded `'h`)

    pub fn sym(&self, name: &str) -> Sym {
        let mut g = self.syms.lock();
        if let Some(&i) = g.by_name.get(name) {
            return Sym(i);
        }
        let i = g.names.len() as u32;
        assert!((i as usize) < MAX_SYMS, "too many symbols");
        self.sym_hash[i as usize].store(mix(0x51, str_hash(name)), Release);
        g.by_name.insert(name.into(), i);
        g.names.push(name.into());
        Sym(i)
    }

    pub fn sym_name(&self, i: u32) -> String {
        self.syms.lock().names[i as usize].to_string()
    }

    pub fn int<'h>(&'h self, v: i64) -> T<'h> {
        if (IMM_MIN..=IMM_MAX).contains(&v) {
            return T::from_bits(IMM | (v as u32 & PAYLOAD));
        }
        let n = Node { hash: mix(0x17, v as u64), mask: 0, data: v as u64, head: TAG_INT, len: 0, size: 1, owner: 0 };
        T::from_bits(self.intern(n, &[], true))
    }

    /// The polynomial hash of spike 4 (decision 4.3), combinable across a split; wide nodes hash in parallel.
    pub fn app<'h>(&'h self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        // SAFETY: `T` is a transparent `u32`.
        let raw: &[u32] = unsafe { std::slice::from_raw_parts(args.as_ptr() as *const u32, args.len()) };
        let (hh, hm, _) = self.meta_bits(head.0);
        let start = 0x33u64.wrapping_mul(K).wrapping_add(hh);
        let (hash, mask, size) = if raw.len() >= PAR_ARGS && rayon::current_thread_index().is_some() {
            let (p, m, sz, _) = raw
                .par_chunks(PAR_CHUNK)
                .map(|c| self.fold(c))
                // the combined length must be carried: spike 4 reset it to 0, which makes the hash depend on rayon's
                // reduction tree whenever a combined part is the right operand (equal wide terms, different ids)
                .reduce(|| (0, 0, 0, 0), |(h1, m1, s1, n1), (h2, m2, s2, n2)| {
                    (h1.wrapping_mul(kpow(n2)).wrapping_add(h2), m1 | m2, s1.saturating_add(s2), n1 + n2)
                });
            (start.wrapping_mul(kpow(raw.len())).wrapping_add(p), m, sz)
        } else {
            let (p, m, sz, _) = self.fold(raw);
            (start.wrapping_mul(kpow(raw.len())).wrapping_add(p), m, sz)
        };
        let n = Node { hash: finalize(hash), mask: mask | hm, data: 0, head: head.0, len: raw.len() as u32, size: size.saturating_add(1), owner: 0 };
        // Old nodes never refer to young ones, so a term with a young part cannot be in the old table: skip the probe.
        let young_part = is_young(head.0) || raw.iter().any(|&a| is_young(a));
        T::from_bits(self.intern(n, raw, !young_part))
    }

    fn fold(&self, c: &[u32]) -> (u64, u64, u32, usize) {
        let (mut h, mut m, mut sz) = (0u64, 0u64, 0u32);
        for &a in c {
            let (ah, am, asz) = self.meta_bits(a);
            h = h.wrapping_mul(K).wrapping_add(ah);
            m |= am;
            sz = sz.saturating_add(asz);
        }
        (h, m, sz, c.len())
    }

    #[inline]
    fn same_node(&self, n: &Node, proto: &Node, args: &[u32], young: bool) -> bool {
        n.hash == proto.hash
            && n.head == proto.head
            && n.len == proto.len
            && if proto.head == TAG_INT { n.data == proto.data } else { self.node_args(n, young) == args }
    }

    fn intern(&self, proto: Node, args: &[u32], may_be_old: bool) -> u32 {
        if may_be_old && self.table.count > 0 {
            if let Some(id) = self.table.find(proto.hash, |id| self.same_node(&self.nodes[id as usize], &proto, args, false)) {
                return id;
            }
        }
        let made = std::cell::Cell::new(false);
        let id = self.ytable.intern(
            proto.hash,
            |id| self.same_node(self.young.node(id), &proto, args, true),
            || {
                made.set(true);
                let mut n = proto;
                n.owner = rayon::current_thread_index().unwrap_or(MAX_THREADS - 1) as u32;
                if proto.head != TAG_INT {
                    n.data = self.young.alloc_args(args) as u64;
                }
                self.young.push(n)
            },
        );
        if self.cfg.instrument && !made.get() {
            let me = rayon::current_thread_index().unwrap_or(MAX_THREADS - 1);
            if self.young.node(id).owner as usize != me {
                self.cross_hits[me].fetch_add(1, Relaxed);
            }
        }
        YOUNG | id
    }

    #[inline]
    fn node_bits(&self, b: u32) -> &Node {
        if b & YOUNG != 0 {
            self.young.node(b & PAYLOAD)
        } else {
            &self.nodes[b as usize]
        }
    }

    #[inline]
    fn node_args(&self, n: &Node, young: bool) -> &[u32] {
        if young {
            self.young.args(n)
        } else {
            &self.args[n.data as usize..n.data as usize + n.len as usize]
        }
    }

    #[inline]
    fn meta_bits(&self, b: u32) -> (u64, u64, u32) {
        if b & IMM != 0 {
            if b & SYMB != 0 {
                let i = b & PAYLOAD;
                (self.sym_hash[i as usize].load(Relaxed), 1u64 << (i % 64), 1)
            } else {
                (mix(0x17, imm_value(b) as u64), 0, 1)
            }
        } else {
            let n = self.node_bits(b);
            (n.hash, n.mask, n.size)
        }
    }

    /// Hash, symbol mask and tree size.
    #[inline]
    pub fn meta(&self, t: T<'_>) -> (u64, u64, u32) {
        self.meta_bits(t.0)
    }

    #[inline]
    pub fn size(&self, t: T<'_>) -> u32 {
        self.meta_bits(t.0).2
    }

    #[inline]
    pub fn view<'h>(&'h self, t: T<'h>) -> View<'h> {
        let b = t.0;
        if b & IMM != 0 {
            return if b & SYMB != 0 { View::Sym(b & PAYLOAD) } else { View::Int(imm_value(b)) };
        }
        let young = b & YOUNG != 0;
        let n = self.node_bits(b);
        if n.head == TAG_INT {
            return View::Int(n.data as i64);
        }
        let a = self.node_args(n, young);
        // SAFETY: `T` is a transparent `u32`; the slice lives as long as the borrow of the heap.
        View::App(T::from_bits(n.head), unsafe { std::slice::from_raw_parts(a.as_ptr() as *const T<'h>, a.len()) })
    }

    #[inline]
    pub fn as_int(&self, t: T<'_>) -> Option<i64> {
        match self.view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }

    /// The memoised `(normal form, epoch, read set)` of `t`, if an entry holds it. Young keys live in the young
    /// memo; old keys in the old memo, or in the young one when their value is young.
    #[inline]
    pub fn memo_get<'h>(&'h self, t: T<'h>) -> Option<(T<'h>, u32, u64)> {
        let h = self.meta_bits(t.0).0;
        let r = if is_young(t.0) {
            self.memo_young.get(t.0, h)
        } else {
            self.memo_old.get(t.0, h).or_else(|| self.memo_young.get(t.0, h))
        };
        r.map(|(v, e, d)| (T::from_bits(v), e, d))
    }

    /// The write barrier of decision 1b.2, made structural: an entry that mentions a young term goes to the young
    /// memo, which the minor collection scans; the old memo never refers to a young term.
    #[inline]
    pub fn memo_put(&self, t: T<'_>, v: T<'_>, epoch: u32, deps: u64) {
        let h = self.meta_bits(t.0).0;
        if is_young(t.0) || is_young(v.0) {
            self.memo_young.put(t.0, h, v.0, epoch, deps);
        } else {
            self.memo_old.put(t.0, h, v.0, epoch, deps);
        }
    }

    /// A store-independent structural fingerprint (spikes 1 and 4), memoised per stored node. Iterative.
    pub fn fingerprint(&self, t: T<'_>) -> u64 {
        let mut done: FxHashMap<u32, u64> = FxHashMap::default();
        let mut names: FxHashMap<u32, u64> = FxHashMap::default();
        let mut name_hash = |i: u32| *names.entry(i).or_insert_with(|| str_hash(&self.sym_name(i)));
        enum F {
            Enter(u32),
            Exit(u32, u64, usize),
        }
        let mut work = vec![F::Enter(t.0)];
        let mut vals: Vec<u64> = vec![];
        while let Some(f) = work.pop() {
            match f {
                F::Enter(b) => {
                    if let Some(&h) = done.get(&b) {
                        vals.push(h);
                        continue;
                    }
                    match self.view(T::from_bits(b)) {
                        View::Int(v) => vals.push((v as u64).wrapping_mul(K) ^ 1),
                        View::Sym(i) => vals.push(name_hash(i) ^ 2),
                        View::App(h, args) => {
                            let View::Sym(hi) = self.view(h) else { unreachable!("heads are symbols") };
                            work.push(F::Exit(b, name_hash(hi), args.len()));
                            for &a in args.iter().rev() {
                                work.push(F::Enter(a.0));
                            }
                        }
                    }
                }
                F::Exit(b, hh, n) => {
                    let start = vals.len() - n;
                    let mut h = hh.wrapping_mul(K) ^ 3;
                    for v in vals.drain(start..) {
                        h = (h.rotate_left(5) ^ v).wrapping_mul(K);
                    }
                    done.insert(b, h);
                    vals.push(h);
                }
            }
        }
        vals.pop().unwrap()
    }

    // ----------------------------------------------------------------------------------------------------------
    // Reporting

    /// Bytes reserved: old arrays and table, young arena and table (with retired arrays), both memos.
    pub fn reserved(&mut self) -> usize {
        let (yc, yr) = self.ytable.bytes();
        self.nodes.capacity() * NODE
            + self.args.capacity() * SLOT
            + self.table.s.len() * 8
            + self.young.reserved_bytes()
            + yc
            + yr
            + self.memo_old.bytes()
            + self.memo_young.bytes()
    }

    pub fn old_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Distinct stored terms (old and young).
    pub fn stored(&mut self) -> usize {
        self.nodes.len() + self.ytable.len()
    }

    pub fn log(&self) -> &[GcEvent] {
        &self.log
    }

    /// Bytes of the young table: current arrays and arrays retired by growth (freed at the next safe point).
    pub fn young_table_bytes(&mut self) -> (usize, usize) {
        self.ytable.bytes()
    }

    pub fn cross_hits(&self) -> u64 {
        self.cross_hits.iter().map(|c| c.load(Relaxed)).sum()
    }

    // ----------------------------------------------------------------------------------------------------------
    // Safe points

    /// A safe point. `&mut self`: no evaluation task is running and no branded handle is alive. Every handle the
    /// caller keeps is in one of `roots`, which the collector may rewrite. `force` makes a minor collection due
    /// regardless of the nursery size (the major one still follows its trigger).
    pub fn safe_point(&mut self, roots: &mut [&mut dyn Trace], force: bool) {
        if !self.cfg.collect {
            return;
        }
        let stress = self.cfg.stress;
        let minor = !self.ytable.is_empty() && (force || stress || self.young.young_bytes() >= self.cfg.nursery);
        if minor {
            if stress {
                self.verify_young(roots);
            }
            self.minor(roots);
            self.minors += 1;
        }
        let major = minor && ((stress && self.minors.is_multiple_of(4)) || self.promoted >= self.min_bytes.max(self.live));
        if major {
            self.major(roots);
        }
        if stress && minor {
            self.verify_old(roots);
        }
    }

    /// Force a minor and a major collection (end of a workload: the live set alone remains).
    pub fn collect_full(&mut self, roots: &mut [&mut dyn Trace]) {
        if !self.cfg.collect {
            return;
        }
        if !self.ytable.is_empty() {
            self.minor(roots);
            self.minors += 1;
        }
        self.major(roots);
    }

    fn children(&self, n: &Node, young: bool) -> impl Iterator<Item = u32> + '_ {
        let a: &[u32] = if n.head == TAG_INT { &[] } else { self.node_args(n, young) };
        let head = if n.head == TAG_INT { None } else { Some(n.head) };
        head.into_iter().chain(a.iter().copied())
    }

    fn minor(&mut self, roots: &mut [&mut dyn Trace]) {
        let before = self.reserved();
        let t0 = Instant::now();
        let (lo, hi) = self.young.range();
        let n = (hi - lo) as usize;
        let old0 = self.nodes.len() as u32;
        let mut rootv: Vec<u32> = vec![];
        for r in roots.iter_mut() {
            r.trace(&mut |x: &mut Raw| {
                if is_young(x.0) {
                    rootv.push((x.0 & PAYLOAD) - lo);
                }
            });
        }
        // fwd[j]: 0 = dead, else new old id + 1
        let mut fwd = vec![0u32; n];
        let mut order: Vec<u32> = vec![];
        let mut out_of_order = 0usize;
        let young = &self.young;
        let local = |c: u32| -> Option<usize> { is_young(c).then(|| ((c & PAYLOAD) - lo) as usize) };
        match self.cfg.order {
            Order::Topo => {
                const VISITING: u32 = u32::MAX;
                // frames: (index, head, arguments, next child: 0 is the head)
                let mut stack: Vec<(u32, u32, &[u32], usize)> = vec![];
                let frame = |j: u32| {
                    let node = young.node(lo + j);
                    if node.head == TAG_INT { (j, 0, &[][..], 1) } else { (j, node.head, young.args(node), 0) }
                };
                for &r in &rootv {
                    if fwd[r as usize] != 0 {
                        continue;
                    }
                    fwd[r as usize] = VISITING;
                    stack.push(frame(r));
                    while let Some(top) = stack.last_mut() {
                        let (j, head, kids, k) = *top;
                        if k <= kids.len() {
                            top.3 += 1;
                            let c = if k == 0 { head } else { kids[k - 1] };
                            if let Some(cj) = local(c) {
                                if cj > j as usize {
                                    out_of_order += 1;
                                }
                                if fwd[cj] == 0 {
                                    fwd[cj] = VISITING;
                                    stack.push(frame(cj as u32));
                                }
                            }
                        } else {
                            stack.pop();
                            fwd[j as usize] = old0 + order.len() as u32 + 1;
                            order.push(j);
                        }
                    }
                }
            }
            Order::Chunk => {
                let mut bits = vec![0u64; n.div_ceil(64)];
                for &r in &rootv {
                    set(&mut bits, r as usize);
                }
                let mut stack: Vec<usize> = vec![];
                for j in (0..n).rev() {
                    if !bit(&bits, j) {
                        continue;
                    }
                    let node = young.node(lo + j as u32);
                    for c in self.children(node, true) {
                        let Some(cj) = local(c) else { continue };
                        if cj < j {
                            set(&mut bits, cj);
                        } else {
                            out_of_order += 1;
                            if set(&mut bits, cj) {
                                // the pass is already below cj: mark its young descendants with a stack
                                stack.push(cj);
                                while let Some(x) = stack.pop() {
                                    for y in self.children(young.node(lo + x as u32), true) {
                                        if let Some(yj) = local(y) {
                                            if set(&mut bits, yj) {
                                                stack.push(yj);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let pre = prefix(&bits);
                for (j, fw) in fwd.iter_mut().enumerate() {
                    if bit(&bits, j) {
                        *fw = old0 + rank(&bits, &pre, j) + 1;
                        order.push(j as u32);
                    }
                }
            }
        }
        let t_mark = Instant::now();
        // promotion: copy survivors in their new order, remapping children
        let remap = |c: u32| -> u32 {
            match local(c) {
                Some(cj) => {
                    debug_assert!(fwd[cj] != 0, "remapping a dead young term");
                    fwd[cj] - 1
                }
                None => c,
            }
        };
        let mut cross_owner = 0usize;
        let instrument = self.cfg.instrument;
        let mut moved_args = 0usize;
        if self.cfg.par_gc {
            // Destinations are known before copying (new id = old0 + position in `order`; argument offsets are a
            // prefix sum of the lengths), so the copy runs on the pool's idle workers, writing disjoint slots.
            let k = order.len();
            let a0 = self.args.len();
            let mut offs: Vec<usize> = Vec::with_capacity(k);
            let mut acc = a0;
            for &j in &order {
                offs.push(acc);
                let n = young.node(lo + j);
                if n.head != TAG_INT {
                    acc += n.len as usize;
                }
            }
            moved_args = acc - a0;
            self.nodes.resize(old0 as usize + k, Node { hash: 0, mask: 0, data: 0, head: TAG_INT, len: 0, size: 0, owner: 0 });
            self.args.resize(acc, 0);
            let (np, ap) = (SyncPtr(self.nodes.as_mut_ptr()), SyncPtr(self.args.as_mut_ptr()));
            cross_owner = order
                .par_iter()
                .enumerate()
                .with_min_len(1024)
                .map(|(r, &j)| {
                    let node = *young.node(lo + j);
                    let mut m = node;
                    let mut cross = 0;
                    if node.head != TAG_INT {
                        let a = young.args(&node);
                        for (x, &c) in a.iter().enumerate() {
                            if let Some(cj) = local(c).filter(|_| instrument) {
                                cross += (young.node(lo + cj as u32).owner != node.owner) as usize;
                            }
                            // SAFETY: slot offs[r] + x is written by this element only, within the resized array.
                            unsafe { *ap.at(offs[r] + x) = remap(c) };
                        }
                        m.head = remap(node.head);
                        m.data = offs[r] as u64;
                    }
                    // SAFETY: as above, node slot old0 + r.
                    unsafe { *np.at(old0 as usize + r) = m };
                    cross
                })
                .sum();
            self.table.insert_par(old0 as usize..old0 as usize + k, &self.nodes);
        } else {
            self.nodes.reserve(order.len());
            self.table.reserve(order.len());
            for &j in &order {
                let node = *young.node(lo + j);
                let mut m = node;
                if node.head != TAG_INT {
                    let a = young.args(&node);
                    let s = self.args.len();
                    for &c in a {
                        if let Some(cj) = local(c).filter(|_| instrument) {
                            if young.node(lo + cj as u32).owner != node.owner {
                                cross_owner += 1;
                            }
                        }
                        self.args.push(remap(c));
                    }
                    moved_args += a.len();
                    m.head = remap(node.head);
                    m.data = s as u64;
                }
                let id = self.nodes.len() as u32;
                debug_assert_eq!(id + 1, fwd[j as usize]);
                self.nodes.push(m);
                self.table.insert(m.hash, id);
            }
        }
        let t_copy = Instant::now();
        for r in roots.iter_mut() {
            r.trace(&mut |x: &mut Raw| x.0 = remap(x.0));
        }
        // the young memo: entries whose key and value survive move to the old memo (weak memo, 1b.5)
        {
            let (nodes, memo_old) = (&self.nodes, &mut self.memo_old);
            let alive = |c: u32| local(c).is_none_or(|cj| fwd[cj] != 0);
            self.memo_young.sweep(true, |it: &Item| {
                if alive(it.key) && alive(it.val) {
                    let (k, v) = (remap(it.key), remap(it.val));
                    let h = nodes[k as usize].hash;
                    memo_old.put_mut(h, Item { key: k, val: v, epoch: it.epoch, deps: it.deps });
                }
                None
            });
        }
        let t_memo = Instant::now();
        let table_freed = self.ytable.clear();
        self.young.reset();
        self.fit_memo();
        self.promoted += order.len() * NODE + moved_args * SLOT;
        let ev = GcEvent {
            kind: "minor",
            pause: t0.elapsed(),
            before,
            after: self.reserved(),
            live: self.nodes.len(),
            survivors: order.len(),
            out_of_order,
            cross_owner,
            table_freed,
            phases: [t_mark - t0, t_copy - t_mark, t_memo - t_copy, t_memo.elapsed()],
        };
        self.log.push(ev);
    }

    fn major(&mut self, roots: &mut [&mut dyn Trace]) {
        let before = self.reserved();
        let t0 = Instant::now();
        let n = self.nodes.len();
        let mut bits = vec![0u64; n.div_ceil(64)];
        for r in roots.iter_mut() {
            r.trace(&mut |x: &mut Raw| {
                assert!(!is_young(x.0), "a young root at a major collection");
                if is_old(x.0) {
                    set(&mut bits, x.0 as usize);
                }
            });
        }
        let mut out_of_order = 0usize;
        let topo = self.cfg.order == Order::Topo;
        let mut stack: Vec<usize> = vec![];
        for i in (0..n).rev() {
            if !bit(&bits, i) {
                continue;
            }
            let node = &self.nodes[i];
            for c in self.children(node, false) {
                if !is_old(c) {
                    continue;
                }
                let c = c as usize;
                if c < i {
                    set(&mut bits, c);
                    continue;
                }
                // only D2 has out-of-order edges in the old region
                assert!(!topo, "child after parent in the old region");
                out_of_order += 1;
                if set(&mut bits, c) {
                    stack.push(c);
                    while let Some(x) = stack.pop() {
                        for y in self.children(&self.nodes[x], false) {
                            if is_old(y) && set(&mut bits, y as usize) {
                                stack.push(y as usize);
                            }
                        }
                    }
                }
            }
        }
        let t_mark = Instant::now();
        let pre = prefix(&bits);
        let alive = |c: u32| !is_old(c) || bit(&bits, c as usize);
        let remap = |c: u32| if is_old(c) { rank(&bits, &pre, c as usize) } else { c };
        // when nothing died every rank is the identity: ids, tables and memo stay as they are
        let none_died = n == 0 || pre[pre.len() - 1] as usize + bits[bits.len() - 1].count_ones() as usize == n;
        let (mut dst, mut adst) = if none_died { (n, self.args.len()) } else { (0usize, 0usize) };
        // The slide stays sequential and in place: copying into fresh arrays on the pool (measured) nearly doubled
        // the peak heap and was slower on one thread. Table and memo are rebuilt on the pool.
        let par = self.cfg.par_gc && !none_died;
        // slide nodes and arguments down in ascending order (arguments were appended in node order)
        for i in (0..n).filter(|_| !none_died) {
            if !bit(&bits, i) {
                continue;
            }
            let mut node = self.nodes[i];
            if node.head != TAG_INT {
                let (s, l) = (node.data as usize, node.len as usize);
                debug_assert!(s >= adst);
                for k in 0..l {
                    self.args[adst + k] = remap(self.args[s + k]);
                }
                node.head = remap(node.head);
                node.data = adst as u64;
                adst += l;
            }
            self.nodes[dst] = node;
            dst += 1;
        }
        self.nodes.truncate(dst);
        self.args.truncate(adst);
        let t_slide = Instant::now();
        if !none_died {
            let cap = 2 * dst + self.min_bytes / NODE;
            let cap = cap.max(self.table.s.len() / 4);
            let remap_entry = |it: &Item| (alive(it.key) && alive(it.val)).then(|| (remap(it.key), remap(it.val)));
            if par {
                self.table.rebuild_par(cap, |id| alive(id).then(|| remap(id)));
                self.memo_old.sweep_par(remap_entry);
            } else {
                self.table.rebuild(cap, |id| alive(id).then(|| remap(id)));
                self.memo_old.sweep(false, remap_entry);
            }
        }
        let t_tab = Instant::now();
        if !none_died {
            for r in roots.iter_mut() {
                r.trace(&mut |x: &mut Raw| x.0 = remap(x.0));
            }
        }
        self.live = dst * NODE + adst * SLOT;
        self.promoted = 0;
        self.shrink();
        self.fit_memo();
        let ev = GcEvent {
            kind: "major",
            pause: t0.elapsed(),
            before,
            after: self.reserved(),
            live: dst,
            survivors: 0,
            out_of_order,
            cross_owner: 0,
            table_freed: 0,
            phases: [t_mark - t0, t_slide - t_mark, t_tab - t_slide, t_tab.elapsed()],
        };
        self.log.push(ev);
    }

    /// The old memo follows the old region: about one entry per old node, between 2^MIN_MEMO_BITS and
    /// 2^old_memo_bits. Resizing re-places entries by their key's hash (no node moves).
    fn fit_memo(&mut self) {
        let target = (self.nodes.len().max(1).next_power_of_two().trailing_zeros()).clamp(MIN_MEMO_BITS, self.cfg.old_memo_bits.max(MIN_MEMO_BITS));
        let cur = self.memo_old.capacity().trailing_zeros();
        if target > cur || target + 2 < cur {
            let nodes = &self.nodes;
            self.memo_old = self.memo_old.resized(target, |k| nodes[k as usize].hash);
        }
    }

    /// Decision 1b.6: keep about twice the live size plus the trigger slack.
    fn shrink(&mut self) {
        fn fit<X>(v: &mut Vec<X>, slack: usize) {
            let n = v.len();
            if v.capacity() > 4 * n + slack {
                v.shrink_to(2 * n + slack);
            }
        }
        let k = self.min_bytes;
        fit(&mut self.nodes, k / NODE);
        fit(&mut self.args, k / SLOT);
    }

    // ----------------------------------------------------------------------------------------------------------
    // Invariants (stress mode, tests)

    /// Before a minor collection: every young node is in the young table only and is found again by its content;
    /// its children are valid handles; roots are valid handles.
    pub fn verify_young(&mut self, roots: &mut [&mut dyn Trace]) {
        let mut ids = vec![];
        self.ytable.for_each(|id| ids.push(id));
        assert_eq!(ids.len(), self.ytable.len(), "young table count");
        for &id in &ids {
            assert!(self.young.valid(id), "young table entry outside the cycle");
            let n = *self.young.node(id);
            let a = if n.head == TAG_INT { &[][..] } else { self.young.args(&n) };
            let found = self.ytable_find(&n, a);
            assert_eq!(found, Some(id), "young node not found by its content");
            if self.table.count > 0 {
                assert!(
                    self.table.find(n.hash, |o| self.same_node(&self.nodes[o as usize], &n, a, false)).is_none(),
                    "a young node is also in the old table"
                );
            }
            for c in self.children(&n, true) {
                self.check_handle(c);
            }
        }
        for r in roots.iter_mut() {
            r.trace(&mut |x: &mut Raw| self.check_handle(x.0));
        }
    }

    fn ytable_find(&self, n: &Node, a: &[u32]) -> Option<u32> {
        self.ytable.find(n.hash, |id| self.same_node(self.young.node(id), n, a, true))
    }

    fn check_handle(&self, c: u32) {
        if c & IMM != 0 {
            return;
        }
        if is_young(c) {
            assert!(self.young.valid(c & PAYLOAD), "reference to a young id outside the current cycle");
        } else {
            assert!((c as usize) < self.nodes.len(), "reference to a moved-away old id");
        }
    }

    /// After a collection: the young region, table and memo are empty; every old node is in the old table exactly
    /// once and found again by its content; children and roots are immediates or old ids in range (no reference to
    /// a freed or moved-away id); in `Topo`, every child id is below its parent's; every memo entry has a live key
    /// and value and sits in the slot of its key's hash.
    pub fn verify_old(&mut self, roots: &mut [&mut dyn Trace]) {
        assert_eq!(self.ytable.len(), 0, "young table not empty after collection");
        assert_eq!(self.young.young_bytes(), 0, "young arena not reset");
        assert_eq!(self.table.count, self.nodes.len(), "old table out of step with the old region");
        let n = self.nodes.len();
        for i in 0..n {
            let node = self.nodes[i];
            let a = if node.head == TAG_INT { &[][..] } else { self.node_args(&node, false) };
            let found = self.table.find(node.hash, |o| self.same_node(&self.nodes[o as usize], &node, a, false));
            assert_eq!(found, Some(i as u32), "old node not found by its content");
            for c in self.children(&node, false) {
                assert!(!is_young(c), "old node refers to a young id");
                if is_old(c) {
                    assert!((c as usize) < n, "old node refers to a moved-away id");
                    if self.cfg.order == Order::Topo {
                        assert!((c as usize) < i, "child after parent in the old region");
                    }
                }
            }
        }
        let mut young_entries = 0;
        self.memo_young.for_each(|_, _| young_entries += 1);
        assert_eq!(young_entries, 0, "young memo not empty after collection");
        self.memo_old.for_each(|slot, it| {
            assert!(is_old(it.key) && (it.key as usize) < n, "memo key freed or moved away");
            assert!(it.val & IMM != 0 || (is_old(it.val) && (it.val as usize) < n), "memo value freed or moved away");
            assert_eq!(slot, self.memo_old.slot_of(self.nodes[it.key as usize].hash), "memo entry in the wrong slot");
        });
        for r in roots.iter_mut() {
            r.trace(&mut |x: &mut Raw| {
                assert!(x.0 & IMM != 0 || (is_old(x.0) && (x.0 as usize) < n), "root refers to a freed or moved-away id");
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Roots;

    /// A handle that was not rooted across a minor collection finds a removed segment and panics; it never reads
    /// another term (no id reuse).
    #[test]
    #[should_panic(expected = "use of a collected young term")]
    fn stale_young_handle_panics() {
        let mut h = Heap::new(Config::gen(Order::Topo));
        let f = h.sym("f");
        let bits = {
            let t = h.app(f.t(), &[h.int(1)]);
            t.bits()
        };
        let mut keep = Roots::new();
        h.safe_point(&mut [&mut keep], true);
        // forge the stale handle (outside the crate this cannot be written: `T::from_bits` is private)
        let stale: T = T::from_bits(bits);
        let _ = h.view(stale);
    }

    /// An old id beyond the region after a major collection is caught by the bounds check.
    #[test]
    #[should_panic]
    fn moved_away_old_handle_panics() {
        let mut h = Heap::new(Config::gen(Order::Topo));
        let f = h.sym("f");
        let mut keep = Roots::new();
        {
            for i in 0..100 {
                keep.push(h.app(f.t(), &[h.int(i)]));
            }
        }
        h.safe_point(&mut [&mut keep], true);
        let last = keep.get(&h, 99).bits();
        keep.clear();
        h.collect_full(&mut [&mut keep]);
        let _ = h.view(T::from_bits(last));
    }
}
