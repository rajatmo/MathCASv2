//! The hash-consed arena of spike 1 (`hc+imm+stamps`) with collection at safe points.
//!
//! Layout, as in spike 1: a term is a 32-bit handle; compound nodes live in `nodes` with their arguments in one shared
//! argument arena `args`; each node caches its hash and 64-bit symbol mask; normal forms are memoised per node id and
//! stamped with the definition epoch. Two changes, both needed by every collector:
//! - **Symbols are immediates** (decision 1.2 anticipated it): a symbol handle is its index with a tag, so symbol
//!   handles held anywhere (rule tables, workload structs) stay valid when a collector moves nodes, and symbols are
//!   never collected. Small integers shrink from 31 to 30 bits to make room for the tag.
//! - A node can be `Free` (mark-sweep keeps freed ids on a free list); viewing a freed node panics, which turns a
//!   handle the caller failed to root into a crash rather than a wrong answer.
//!
//! Collection modes ([`Mode`]):
//! - `MarkSweep` (A): mark from the roots with an explicit stack, free dead ids onto a free list and remove them from
//!   the interning table. Ids never change, so roots are only read. Dead argument slots are reclaimed either by
//!   compacting the argument arena alone once it is half dead (nodes keep their ids; only their `start` changes), or
//!   by free lists of argument ranges keyed by length.
//! - `Compact` (B): sliding compaction. Because terms are immutable and children are always interned before their
//!   parents, a child's id is always smaller than its parent's, and sliding preserves the order. So marking needs no
//!   stack (one descending pass), the new id of a node is a rank in the mark bitmap (no forwarding table), and the
//!   argument arena slides along with the nodes in the same ascending pass.
//! - `Gen` (C): the nodes allocated since the last minor collection form a young region at the tail of the arena,
//!   with their own interning table. A minor collection is the sliding compaction of the tail only: survivors move
//!   down and are inserted into the old table (promotion), the young table is cleared wholesale. Old nodes cannot
//!   point to young ones (immutability), so the only old-to-young references are roots and memo entries; memo writes
//!   of that kind are remembered (a write barrier on `memo_put`, never on term construction). The old region is
//!   compacted (as B) when it has doubled since the last major collection.
//!
//! The memo is weak: an entry survives a collection only if both its key and its value survive. Stamps, epochs and
//! the change log are not touched (a moved node keeps its mask, so a stamp means the same after the move).
//!
//! Every collection loop is bounded by the size of the arena, so a collection needs no interrupt polling (its pause
//! is what the driver measures).
use crate::{GcEvent, Store, View};
use hashbrown::HashTable;
use rustc_hash::FxHashMap;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Term(pub u32);

/// Immediate handles: bit 31 set. Bit 30 then tells a symbol (index in the low 30 bits) from a small integer (30-bit
/// two's complement in the low 30 bits).
const IMM: u32 = 1 << 31;
const SYMB: u32 = 1 << 30;
const PAYLOAD: u32 = SYMB - 1;
const IMM_MIN: i64 = -(1 << 29);
const IMM_MAX: i64 = (1 << 29) - 1;

impl Term {
    fn is_node(self) -> bool {
        self.0 & IMM == 0
    }
    fn idx(self) -> usize {
        self.0 as usize
    }
}

fn imm_value(t: Term) -> i64 {
    (((t.0 << 2) as i32) >> 2) as i64
}

#[derive(Clone, Copy)]
enum Kind {
    Int(i64),
    App {
        head: Term,
        start: u32,
        len: u32,
    },
    /// A freed id (mark-sweep only), waiting on the free list.
    Free,
}

#[derive(Clone, Copy)]
struct Node {
    kind: Kind,
    hash: u64,
    mask: u64,
}

const NODE: usize = std::mem::size_of::<Node>();
const SLOT: usize = std::mem::size_of::<Term>();

/// How mark-sweep reclaims dead argument slots.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArgPolicy {
    /// Compact the argument arena alone once dead slots outnumber live ones (no id changes).
    Compact,
    /// Free lists of argument ranges by exact length; never compacts.
    FreeLists,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Never collect (same layout, for the overhead comparison).
    None,
    /// A: non-moving mark-sweep.
    MarkSweep(ArgPolicy),
    /// B: sliding compaction of the whole arena.
    Compact,
    /// C: generational. A minor collection runs at a safe point once the young region holds `nursery` bytes (0: at
    /// every safe point); the old region is compacted when it has doubled.
    Gen { nursery: usize },
}

/// Lookup key for interning.
#[derive(Clone, Copy)]
enum Key<'a> {
    Int(i64),
    App(Term, &'a [Term]),
}

fn matches(nodes: &[Node], args: &[Term], hash: u64, key: Key, t: &Term) -> bool {
    let n = &nodes[t.idx()];
    if n.hash != hash {
        return false;
    }
    match (n.kind, key) {
        (Kind::Int(a), Key::Int(b)) => a == b,
        (Kind::App { head: h1, start, len }, Key::App(h2, a)) => {
            h1 == h2 && &args[start as usize..(start + len) as usize] == a
        }
        _ => false,
    }
}

const K: u64 = 0x9E37_79B9_7F4A_7C15;

fn mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(K)
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

pub struct GcStore {
    name: &'static str,
    mode: Mode,
    /// collect at every safe point (correctness testing)
    stress: bool,
    nodes: Vec<Node>,
    args: Vec<Term>,
    /// interning table (for C: the old region)
    table: HashTable<Term>,
    /// C: interning table of the young region, cleared at each minor collection
    young: HashTable<Term>,
    /// C: first node id and first argument slot of the young region
    young_lo: usize,
    young_args_lo: usize,
    /// A: freed node ids, and freed argument ranges by length (`FreeLists`)
    free: Vec<u32>,
    arg_free: FxHashMap<u32, Vec<u32>>,
    /// A: argument slots of dead nodes not yet reused or compacted away
    dead_args: usize,
    /// symbol hash and name, by symbol index
    syms: Vec<(u64, Box<str>)>,
    by_name: FxHashMap<Box<str>, u32>,
    /// normal-form memo: result handle and the epoch it was found at (0: none), indexed by node id
    memo: Vec<(u32, u32)>,
    /// C: old keys whose memo value is young (the remembered set of the write barrier)
    remembered: Vec<u32>,
    epoch: u32,
    changes: Vec<(u32, u64)>,
    /// bytes allocated since the last collection (C: since the last minor collection)
    alloc: usize,
    /// bytes live after the last collection (C: after the last major collection)
    live: usize,
    /// C: bytes promoted since the last major collection
    promoted: usize,
    /// no full collection before the heap has at least this many bytes to reclaim
    min_bytes: usize,
    log: Vec<GcEvent>,
    minors: usize,
}

impl GcStore {
    pub fn new(name: &'static str, mode: Mode, stress: bool) -> GcStore {
        GcStore {
            name,
            mode,
            stress,
            nodes: vec![],
            args: vec![],
            table: HashTable::new(),
            young: HashTable::new(),
            young_lo: 0,
            young_args_lo: 0,
            free: vec![],
            arg_free: FxHashMap::default(),
            dead_args: 0,
            syms: vec![],
            by_name: FxHashMap::default(),
            memo: vec![],
            remembered: vec![],
            epoch: 1,
            changes: vec![],
            alloc: 0,
            live: 0,
            promoted: 0,
            min_bytes: 4 << 20,
            log: vec![],
            minors: 0,
        }
    }

    fn gen(&self) -> bool {
        matches!(self.mode, Mode::Gen { .. })
    }

    /// Bytes reserved by the store: capacities of its arrays and tables (a hashbrown slot is the handle plus one
    /// control byte).
    pub fn reserved(&self) -> usize {
        self.nodes.capacity() * NODE
            + self.args.capacity() * SLOT
            + (self.table.capacity() + self.young.capacity()) * (SLOT + 1)
            + self.memo.capacity() * 8
            + (self.free.capacity() + self.remembered.capacity()) * 4
            + self.arg_free.values().map(|v| v.capacity() * 4).sum::<usize>()
    }

    /// Nodes in use (not free).
    pub fn live_nodes(&self) -> usize {
        self.nodes.len() - self.free.len()
    }

    fn intern(&mut self, key: Key, hash: u64, mask: u64) -> Term {
        let (nodes, args) = (&self.nodes, &self.args);
        if let Some(t) = self.table.find(hash, |t| matches(nodes, args, hash, key, t)) {
            return *t;
        }
        if self.gen() {
            if let Some(t) = self.young.find(hash, |t| matches(nodes, args, hash, key, t)) {
                return *t;
            }
        }
        let kind = match key {
            Key::Int(v) => Kind::Int(v),
            Key::App(head, a) => {
                let len = a.len();
                let reused = match self.mode {
                    Mode::MarkSweep(ArgPolicy::FreeLists) if len > 0 => {
                        self.arg_free.get_mut(&(len as u32)).and_then(|v| v.pop())
                    }
                    _ => None,
                };
                let start = match reused {
                    Some(s) => {
                        self.args[s as usize..s as usize + len].copy_from_slice(a);
                        self.dead_args -= len;
                        s as usize
                    }
                    None => {
                        let s = self.args.len();
                        self.args.extend_from_slice(a);
                        s
                    }
                };
                self.alloc += len * SLOT;
                Kind::App { head, start: start as u32, len: len as u32 }
            }
        };
        let node = Node { kind, hash, mask };
        let id = match self.free.pop() {
            Some(i) => {
                self.nodes[i as usize] = node;
                i
            }
            None => {
                self.nodes.push(node);
                (self.nodes.len() - 1) as u32
            }
        };
        assert!(id < IMM, "arena full");
        self.alloc += NODE;
        let nodes = &self.nodes;
        let table = if self.gen() { &mut self.young } else { &mut self.table };
        table.insert_unique(hash, Term(id), |t| nodes[t.idx()].hash);
        Term(id)
    }

    /// Hash and symbol mask of any term, immediate or stored.
    fn meta(&self, t: Term) -> (u64, u64) {
        if t.is_node() {
            let n = &self.nodes[t.idx()];
            (n.hash, n.mask)
        } else if t.0 & SYMB != 0 {
            let i = t.0 & PAYLOAD;
            (self.syms[i as usize].0, 1u64 << (i % 64))
        } else {
            (mix(0x17, imm_value(t) as u64), 0)
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

    fn record(&mut self, kind: &'static str, t0: Instant, before: usize) {
        if cfg!(debug_assertions) || self.stress {
            self.verify();
        }
        let ev = GcEvent { kind, pause: t0.elapsed(), before, after: self.reserved(), live: self.live_nodes() };
        self.log.push(ev);
    }

    fn shrink(&mut self) {
        // Give memory back only beyond what the heap grows into again before the next collection (the trigger lets
        // it reach about twice the live size plus `min_bytes`). Shrinking further makes the arrays regrow into fresh
        // pages, and those page faults cost more than the collection itself (measured on S3).
        fn fit<T>(v: &mut Vec<T>, slack: usize) {
            let n = v.len();
            if v.capacity() > 4 * n + slack {
                v.shrink_to(2 * n + slack);
            }
        }
        let k = self.min_bytes;
        fit(&mut self.nodes, k / NODE);
        fit(&mut self.args, k / SLOT);
        fit(&mut self.memo, k / 8);
        let (nodes, len) = (&self.nodes, self.table.len());
        if self.table.capacity() > 4 * len + k / NODE {
            self.table.shrink_to(2 * len + k / NODE, |t| nodes[t.idx()].hash);
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // A: mark-sweep

    fn sweep(&mut self, roots: &mut [&mut [Term]]) {
        let (t0, before) = (Instant::now(), self.reserved());
        let n = self.nodes.len();
        let mut bits = vec![0u64; n.div_ceil(64)];
        // free-list reuse breaks the child-before-parent order, so marking needs an explicit stack
        let mut stack: Vec<u32> = vec![];
        for t in roots.iter().flat_map(|r| r.iter()) {
            if t.is_node() && set(&mut bits, t.idx()) {
                stack.push(t.0);
            }
        }
        while let Some(i) = stack.pop() {
            if let Kind::App { head, start, len } = self.nodes[i as usize].kind {
                let kids = std::iter::once(&head).chain(&self.args[start as usize..(start + len) as usize]);
                for c in kids {
                    if c.is_node() && set(&mut bits, c.idx()) {
                        stack.push(c.0);
                    }
                }
            }
        }
        let fl = self.mode == Mode::MarkSweep(ArgPolicy::FreeLists);
        let (mut live_nodes, mut live_args) = (0, 0);
        for i in 0..n {
            if bit(&bits, i) {
                live_nodes += 1;
                if let Kind::App { len, .. } = self.nodes[i].kind {
                    live_args += len as usize;
                }
                // weak memo: drop entries whose value dies
                if let Some(m) = self.memo.get_mut(i) {
                    let v = Term(m.0);
                    if m.1 != 0 && v.is_node() && !bit(&bits, v.idx()) {
                        *m = (0, 0);
                    }
                }
                continue;
            }
            match self.nodes[i].kind {
                Kind::Free => continue,
                Kind::App { start, len, .. } => {
                    if fl && len > 0 {
                        self.arg_free.entry(len).or_default().push(start);
                    }
                    self.dead_args += len as usize;
                }
                Kind::Int(_) => {}
            }
            self.nodes[i].kind = Kind::Free;
            self.free.push(i as u32);
            // the id will be reused: its memo entry must not outlive it
            if let Some(m) = self.memo.get_mut(i) {
                *m = (0, 0);
            }
        }
        self.table.retain(|t| bit(&bits, t.idx()));
        if !fl && self.dead_args >= live_args.max(1024) {
            self.compact_args(live_args);
        }
        self.shrink();
        self.live = live_nodes * NODE + live_args * SLOT;
        self.alloc = 0;
        self.record("sweep", t0, before);
    }

    /// Compact the argument arena alone: copy every live node's arguments into a fresh arena in id order and update
    /// its `start`. Node ids do not change, so nothing outside the store is affected.
    fn compact_args(&mut self, live_args: usize) {
        let mut fresh = Vec::with_capacity(2 * live_args + self.min_bytes / SLOT);
        for node in self.nodes.iter_mut() {
            if let Kind::App { start, len, .. } = &mut node.kind {
                let s = fresh.len() as u32;
                fresh.extend_from_slice(&self.args[*start as usize..(*start + *len) as usize]);
                *start = s;
            }
        }
        self.args = fresh;
        self.dead_args = 0;
        self.arg_free.clear();
    }

    // -----------------------------------------------------------------------------------------------------------
    // B and C: sliding compaction of the tail starting at `lo` (B: the whole arena; C minor: the young region)

    fn compact(&mut self, roots: &mut [&mut [Term]], minor: bool) {
        let (t0, before) = (Instant::now(), self.reserved());
        let (lo, args_lo) = if minor { (self.young_lo, self.young_args_lo) } else { (0, 0) };
        let n = self.nodes.len();
        let mut bits = vec![0u64; (n - lo).div_ceil(64)];
        for t in roots.iter().flat_map(|r| r.iter()) {
            if t.is_node() && t.idx() >= lo {
                set(&mut bits, t.idx() - lo);
            }
        }
        // Children have smaller ids than parents: one descending pass marks everything reachable, no stack.
        for i in (lo..n).rev() {
            if !bit(&bits, i - lo) {
                continue;
            }
            if let Kind::App { head, start, len } = self.nodes[i].kind {
                let kids = std::iter::once(&head).chain(&self.args[start as usize..(start + len) as usize]);
                for c in kids {
                    if c.is_node() && c.idx() >= lo {
                        debug_assert!(c.idx() < i, "child after parent");
                        set(&mut bits, c.idx() - lo);
                    }
                }
            }
        }
        // new id = lo + number of live nodes before it: a prefix count per bitmap word plus a popcount
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
        // slide nodes, their arguments and their memo entries down, in ascending order (destination <= source)
        let (mut dst, mut adst) = (lo, args_lo);
        let memo_len = self.memo.len();
        for i in lo..n {
            if !bit(&bits, i - lo) {
                continue;
            }
            let mut node = self.nodes[i];
            if let Kind::App { head, start, len } = node.kind {
                let (s, l) = (start as usize, len as usize);
                debug_assert!(s >= adst);
                for k in 0..l {
                    self.args[adst + k] = remap(self.args[s + k]);
                }
                node.kind = Kind::App { head: remap(head), start: adst as u32, len };
                adst += l;
            }
            self.nodes[dst] = node;
            if dst < memo_len {
                let (v, e) = if i < memo_len { self.memo[i] } else { (0, 0) };
                self.memo[dst] = if e != 0 && alive(Term(v)) { (remap(Term(v)).0, e) } else { (0, 0) };
            }
            dst += 1;
        }
        self.nodes.truncate(dst);
        self.args.truncate(adst);
        self.memo.truncate(dst);
        for r in roots.iter_mut() {
            for t in r.iter_mut() {
                *t = remap(*t);
            }
        }
        if minor {
            // old keys with young values (the write barrier's record); each key once, since remapping is not
            // idempotent
            self.remembered.sort_unstable();
            self.remembered.dedup();
            for &k in &self.remembered {
                if let Some(m) = self.memo.get_mut(k as usize) {
                    if m.1 != 0 {
                        *m = if alive(Term(m.0)) { (remap(Term(m.0)).0, m.1) } else { (0, 0) };
                    }
                }
            }
            self.remembered.clear();
            // promotion: survivors join the old table, the young table is reset wholesale
            self.young.clear();
            let nodes = &self.nodes;
            for j in lo..dst {
                self.table.insert_unique(nodes[j].hash, Term(j as u32), |t| nodes[t.idx()].hash);
            }
            self.promoted += (dst - lo) * NODE + (adst - args_lo) * SLOT;
        } else {
            // hashes are stored in the nodes and do not change: entries stay in their buckets, renumbered in place
            self.table.retain(|t| {
                if alive(*t) {
                    *t = remap(*t);
                    true
                } else {
                    false
                }
            });
            self.live = dst * NODE + adst * SLOT;
            self.promoted = 0;
        }
        self.young_lo = dst;
        self.young_args_lo = adst;
        self.alloc = 0;
        self.shrink();
        self.record(if minor { "minor" } else { "full" }, t0, before);
    }

    /// Check the store's invariants (debug builds and stress mode run it after every collection): every stored node
    /// is in exactly one interning table and is found again by its content, no live node or memo entry refers to a
    /// freed node.
    pub fn verify(&self) {
        let in_tables = self.table.len() + self.young.len();
        let stored = self.nodes.iter().filter(|n| !matches!(n.kind, Kind::Free)).count();
        assert_eq!(in_tables, stored, "interning tables out of step with the arena");
        let ok = |t: Term| !t.is_node() || !matches!(self.nodes[t.idx()].kind, Kind::Free);
        for (i, n) in self.nodes.iter().enumerate() {
            let key = match n.kind {
                Kind::Free => {
                    if let Some(m) = self.memo.get(i) {
                        assert_eq!(m.1, 0, "memo entry on a freed id");
                    }
                    continue;
                }
                Kind::Int(v) => Key::Int(v),
                Kind::App { head, start, len } => {
                    let a = &self.args[start as usize..(start + len) as usize];
                    assert!(ok(head) && a.iter().all(|&c| ok(c)), "live node refers to a freed node");
                    Key::App(head, a)
                }
            };
            let (nodes, args) = (&self.nodes, &self.args);
            let found = self
                .table
                .find(n.hash, |t| matches(nodes, args, n.hash, key, t))
                .or_else(|| self.young.find(n.hash, |t| matches(nodes, args, n.hash, key, t)));
            assert_eq!(found, Some(&Term(i as u32)), "node not found by its content");
            if let Some(&(v, e)) = self.memo.get(i) {
                assert!(e == 0 || ok(Term(v)), "memo value freed");
            }
        }
    }
}

impl Store for GcStore {
    type T = Term;
    type S = Term;

    fn name(&self) -> &'static str {
        self.name
    }
    fn sym(&mut self, name: &str) -> Term {
        if let Some(&i) = self.by_name.get(name) {
            return Term(IMM | SYMB | i);
        }
        let i = self.syms.len() as u32;
        assert!(i < PAYLOAD, "too many symbols");
        self.syms.push((mix(0x51, crate::str_hash(name)), name.into()));
        self.by_name.insert(name.into(), i);
        Term(IMM | SYMB | i)
    }
    fn sym_term(&mut self, s: &Term) -> Term {
        *s
    }
    fn int(&mut self, v: i64) -> Term {
        if (IMM_MIN..=IMM_MAX).contains(&v) {
            return Term(IMM | (v as u32 & PAYLOAD));
        }
        self.intern(Key::Int(v), mix(0x17, v as u64), 0)
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
        self.intern(Key::App(*head, &args), hash, mask)
    }
    fn view<'a>(&'a self, t: &'a Term) -> View<'a, Term, Term> {
        if !t.is_node() {
            return if t.0 & SYMB != 0 { View::Sym(t) } else { View::Int(imm_value(*t)) };
        }
        let n = &self.nodes[t.idx()];
        match &n.kind {
            Kind::Int(v) => View::Int(*v),
            Kind::App { head, start, len } => View::App(head, &self.args[*start as usize..(*start + *len) as usize]),
            Kind::Free => panic!("use of collected term {} (a handle was not rooted across a safe point)", t.0),
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
        assert!(!s.is_node() && s.0 & SYMB != 0, "not a symbol");
        self.syms[(s.0 & PAYLOAD) as usize].1.to_string()
    }

    fn has_stamps(&self) -> bool {
        true
    }
    fn memo_get(&self, t: &Term) -> Option<Term> {
        if !t.is_node() {
            // integers are normal; symbols are looked up (own values) every time
            return if t.0 & SYMB != 0 { None } else { Some(*t) };
        }
        let &(r, e) = self.memo.get(t.idx())?;
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
        if !t.is_node() {
            return;
        }
        if self.memo.len() < self.nodes.len() {
            self.memo.resize(self.nodes.len(), (0, 0));
        }
        self.memo[t.idx()] = (nf.0, self.epoch);
        if nf.is_node() {
            self.memo[nf.idx()] = (nf.0, self.epoch);
            // write barrier: an old key now refers to a young value
            if self.gen() && t.idx() < self.young_lo && nf.idx() >= self.young_lo {
                self.remembered.push(t.0);
            }
        }
    }
    fn redefine(&mut self, s: &Term) {
        self.epoch += 1;
        let m = self.meta(*s).1;
        self.changes.push((self.epoch, m));
    }

    fn safe_point(&mut self, roots: &mut [&mut [Term]], force: bool) {
        let due = force || self.stress || self.alloc >= self.min_bytes.max(self.live);
        match self.mode {
            Mode::None => {}
            Mode::MarkSweep(_) if due => self.sweep(roots),
            Mode::Compact if due => self.compact(roots, false),
            Mode::Gen { nursery } => {
                // A major collection always follows a minor one, so it starts with an empty young region and only
                // the old table needs renumbering.
                let minor = self.nodes.len() > self.young_lo && (force || self.stress || self.alloc >= nursery);
                if minor {
                    self.compact(roots, true);
                    self.minors += 1;
                }
                let major = minor
                    && ((self.stress && self.minors.is_multiple_of(4))
                        || self.promoted >= self.min_bytes.max(self.live));
                if major {
                    self.compact(roots, false);
                }
            }
            _ => {}
        }
    }
    fn gc_log(&self) -> &[GcEvent] {
        &self.log
    }
}
