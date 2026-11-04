//! The young region: spike 4's concurrent store C used as the nursery (I1.1), with I3's number payloads.
//!
//! - [`YoungArena`]: nodes in fixed segments behind a directory of atomic pointers (never moved, read without
//!   locks). Ids are reserved in per-thread chunks of [`ID_CHUNK`]; argument slots and arena payload words are
//!   bump-allocated from per-thread **slabs** (A.4). Each thread also keeps a chunked list of the young nodes it
//!   made that own a boxed payload (A.3). Ids are not dense and not in global creation order (I1.2).
//! - [`YoungTable`]: 128 shards of open-addressing arrays probed without locks, a shard lock only on a miss. Arrays
//!   replaced by growth are retired and freed at the next safe point.
//!
//! At a minor collection (`&mut`, so no task is running) the arena is reset wholesale: its segments leave the
//! directory, the next cycle starts at the next segment boundary, and every thread's cached chunks are invalidated
//! by a generation number. A handle from a previous cycle finds a null segment and panics instead of reading a
//! reused node (no id reuse until the 2^29 young id space wraps, I1.5).
use crate::node::{box_bytes, box_owned, box_words, free_box, Node, Region, ARENA_MAX, NODE};
use parking_lot::Mutex;
use std::cell::Cell;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering::*};

const SEG_BITS: u32 = 16;
const SEG_LEN: usize = 1 << SEG_BITS;
/// Young indices are below 2^29 (the upper half of the 30-bit index space names scratch terms, see `scratch`); the
/// last segment is never allocated.
const MAX_SEGS: usize = 1 << (29 - SEG_BITS);
/// Ids a thread reserves at a time.
pub(crate) const ID_CHUNK: u32 = 256;
/// Words of a slab (64 KB): argument slots (two per word) and arena payloads. Requests above an eighth of a slab
/// get their own allocation.
const SLAB: usize = 1 << 13;
/// Entries of a box-list chunk.
const BOX_CHUNK: usize = 64;
/// An unused box-list slot (young indices are below 2^29).
const NO_ID: u32 = u32::MAX;
/// Segments and slabs kept for reuse after a reset (fresh pages cost more than the collection, 1b.6).
const SEG_POOL: usize = 4;
const SLAB_POOL: usize = 64;

/// A thread's cached chunks for one arena cycle.
#[derive(Clone, Copy)]
struct Local {
    /// arena id and generation the chunks belong to
    key: u64,
    next: u32,
    end: u32,
    slab: *mut u64,
    slab_left: usize,
    boxes: *mut u32,
    boxes_left: usize,
}

const NO_LOCAL: Local =
    Local { key: 0, next: 0, end: 0, slab: null_mut(), slab_left: 0, boxes: null_mut(), boxes_left: 0 };

thread_local! {
    static LOCAL: Cell<Local> = const { Cell::new(NO_LOCAL) };
}

static ARENA_IDS: AtomicU64 = AtomicU64::new(1);

struct SegPtr(*mut Node);
// SAFETY: a pooled segment is owned by the arena and touched only under its mutex or with `&mut`.
unsafe impl Send for SegPtr {}

/// Memory handed out by the arena in this cycle, besides node segments.
#[derive(Default)]
struct Chunks {
    /// slabs of this cycle, and free slabs for reuse
    slabs: Vec<Box<[u64]>>,
    free: Vec<Box<[u64]>>,
    /// oversized argument slices of this cycle
    big: Vec<Box<[u64]>>,
    /// box-list chunks of this cycle
    boxes: Vec<Box<[u32]>>,
}

pub(crate) struct YoungArena {
    id: u64,
    gen: u64,
    segs: Box<[AtomicPtr<Node>]>,
    next_chunk: AtomicU32,
    lo_chunk: u32,
    chunks: Mutex<Chunks>,
    /// bytes of slabs, oversized slices and boxed payloads of this cycle (the nursery trigger, I3.3)
    bytes: AtomicUsize,
    /// bytes of boxed payloads owned by young nodes
    boxed: AtomicUsize,
    pool: Mutex<Vec<SegPtr>>,
    installed: AtomicUsize,
}

fn alloc_seg() -> *mut Node {
    Box::into_raw(Box::<[Node]>::new_uninit_slice(SEG_LEN)) as *mut Node
}

fn free_seg(p: *mut Node) {
    // SAFETY: `p` came from `alloc_seg` (a boxed slice of SEG_LEN uninitialised nodes) and is dropped once.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p as *mut std::mem::MaybeUninit<Node>, SEG_LEN)) });
}

impl YoungArena {
    pub(crate) fn new() -> YoungArena {
        YoungArena {
            id: ARENA_IDS.fetch_add(1, Relaxed),
            gen: 0,
            segs: (0..MAX_SEGS).map(|_| AtomicPtr::new(null_mut())).collect(),
            next_chunk: AtomicU32::new(0),
            lo_chunk: 0,
            chunks: Mutex::new(Chunks::default()),
            bytes: AtomicUsize::new(0),
            boxed: AtomicUsize::new(0),
            pool: Mutex::new(vec![]),
            installed: AtomicUsize::new(0),
        }
    }

    #[inline]
    fn key(&self) -> u64 {
        self.id << 32 | (self.gen & 0xFFFF_FFFF)
    }

    fn with_local<R>(&self, f: impl FnOnce(&mut Local) -> R) -> R {
        LOCAL.with(|c| {
            let mut l = c.get();
            if l.key != self.key() {
                l = Local { key: self.key(), ..NO_LOCAL };
            }
            let r = f(&mut l);
            c.set(l);
            r
        })
    }

    fn ensure_seg(&self, s: usize) {
        if self.segs[s].load(Acquire).is_null() {
            let p = self.pool.lock().pop().map(|p| p.0).unwrap_or_else(alloc_seg);
            if self.segs[s].compare_exchange(null_mut(), p, AcqRel, Acquire).is_err() {
                self.pool.lock().push(SegPtr(p));
            } else {
                self.installed.fetch_add(1, Relaxed);
            }
        }
    }

    /// Store a node; returns its young index. The caller publishes it (through the table, with release ordering).
    fn push(&self, n: Node) -> u32 {
        let id = self.with_local(|l| {
            if l.next == l.end {
                let c = self.next_chunk.fetch_add(1, Relaxed);
                let start = c
                    .checked_mul(ID_CHUNK)
                    .filter(|&s| (s as usize) < (MAX_SEGS - 1) * SEG_LEN)
                    .expect("young id space full (one evaluation made more than 2^29 terms)");
                self.ensure_seg((start >> SEG_BITS) as usize);
                l.next = start;
                l.end = start + ID_CHUNK;
            }
            l.next += 1;
            l.next - 1
        });
        let seg = self.segs[(id >> SEG_BITS) as usize].load(Acquire);
        // SAFETY: the segment is installed and the slot belongs to this thread's chunk; written once, before
        // publication.
        unsafe { seg.add(id as usize & (SEG_LEN - 1)).write(n) };
        id
    }

    /// Room for `words` words from this thread's slab (or an allocation of its own when large).
    fn alloc_words(&self, words: usize) -> *mut u64 {
        if words == 0 {
            return NonNull::dangling().as_ptr();
        }
        if words > SLAB / 8 {
            let mut b = vec![0u64; words].into_boxed_slice();
            let p = b.as_mut_ptr();
            self.bytes.fetch_add(words * 8, Relaxed);
            self.chunks.lock().big.push(b);
            return p;
        }
        self.with_local(|l| {
            if l.slab_left < words {
                let mut g = self.chunks.lock();
                let mut b = g.free.pop().unwrap_or_else(|| vec![0u64; SLAB].into_boxed_slice());
                l.slab = b.as_mut_ptr();
                l.slab_left = SLAB;
                g.slabs.push(b);
                self.bytes.fetch_add(SLAB * 8, Relaxed);
            }
            let p = l.slab;
            // SAFETY: the slab has `slab_left >= words` words left and is owned by the arena until the next reset.
            l.slab = unsafe { p.add(words) };
            l.slab_left -= words;
            p
        })
    }

    /// Enter a young boxed node in this thread's box list (A.3).
    fn note_boxed(&self, id: u32, bytes: usize) {
        self.bytes.fetch_add(bytes, Relaxed);
        self.boxed.fetch_add(bytes, Relaxed);
        self.with_local(|l| {
            if l.boxes_left == 0 {
                let mut b = vec![NO_ID; BOX_CHUNK].into_boxed_slice();
                l.boxes = b.as_mut_ptr();
                l.boxes_left = BOX_CHUNK;
                self.chunks.lock().boxes.push(b);
            }
            // SAFETY: the chunk has `boxes_left > 0` slots left, is owned by the arena until the next reset, and only
            // this thread writes it; the collector reads it with `&mut`.
            unsafe {
                l.boxes.write(id);
                l.boxes = l.boxes.add(1);
            }
            l.boxes_left -= 1;
        })
    }

    /// Store a new node with its body (argument slots or payload words); returns its young index. Runs under the
    /// young table's shard lock, once per distinct term.
    pub(crate) fn store(&self, mut n: Node, body: Body) -> u32 {
        match body {
            Body::Inline => self.push(n),
            Body::Args(a) => {
                let p = self.alloc_words(a.len().div_ceil(2)) as *mut u32;
                // SAFETY: room for `a.len()` slots was just reserved, and nothing else writes it.
                unsafe { std::ptr::copy_nonoverlapping(a.as_ptr(), p, a.len()) };
                n.data = p as u64;
                self.push(n)
            }
            Body::Words(w) if w.len() <= ARENA_MAX => {
                let p = self.alloc_words(w.len());
                // SAFETY: as above.
                unsafe { std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len()) };
                n.data = p as u64;
                self.push(n)
            }
            Body::Words(w) => {
                n.data = box_words(w);
                let id = self.push(n);
                self.note_boxed(id, box_bytes(w.len()));
                id
            }
        }
    }

    /// Store a new payload node whose words come in their own box: a long payload keeps the box (no copy), a short
    /// one is copied into the slab like any other.
    pub(crate) fn store_box(&self, mut n: Node, words: Box<[u64]>) -> u32 {
        if words.len() <= ARENA_MAX {
            return self.store(n, Body::Words(&words));
        }
        let len = words.len();
        n.data = box_owned(words);
        let id = self.push(n);
        self.note_boxed(id, box_bytes(len));
        id
    }

    /// The node at a young index. Panics on an index of a previous cycle (a handle that was not rooted across a
    /// safe point).
    #[inline]
    pub(crate) fn node(&self, id: u32) -> &Node {
        let seg = self.segs.get((id >> SEG_BITS) as usize).map_or(null_mut(), |s| s.load(Acquire));
        assert!(!seg.is_null(), "use of a collected young term {id} (a handle was not rooted across a safe point)");
        // SAFETY: published ids point at written nodes of an installed segment.
        unsafe { &*seg.add(id as usize & (SEG_LEN - 1)) }
    }

    /// The young indices of this cycle: `lo..hi`, with gaps (unused tails of chunks hold no node).
    pub(crate) fn range(&self) -> (u32, u32) {
        (self.lo_chunk * ID_CHUNK, self.next_chunk.load(Relaxed) * ID_CHUNK)
    }

    /// Is `id` an index of this cycle with an installed segment? (Verification.)
    pub(crate) fn valid(&self, id: u32) -> bool {
        let (lo, hi) = self.range();
        id >= lo && id < hi && !self.segs[(id >> SEG_BITS) as usize].load(Relaxed).is_null()
    }

    /// Bytes taken by this cycle (the nursery trigger): reserved id chunks, slabs, oversized slices and boxed
    /// payloads. It overstates the bytes in use by at most one chunk of each kind per thread (I1.9).
    pub(crate) fn young_bytes(&self) -> usize {
        let chunks = (self.next_chunk.load(Relaxed) - self.lo_chunk) as usize;
        chunks * ID_CHUNK as usize * NODE + self.bytes.load(Relaxed)
    }

    /// Bytes of boxed payloads owned by young nodes.
    pub(crate) fn boxed_bytes(&self) -> usize {
        self.boxed.load(Relaxed)
    }

    /// Bytes reserved by the arena (segments, slabs, slices, box lists), without boxed payloads.
    pub(crate) fn reserved_bytes(&mut self) -> usize {
        let segs = *self.installed.get_mut() + self.pool.get_mut().len();
        let g = self.chunks.get_mut();
        segs * SEG_LEN * NODE
            + (g.slabs.len() + g.free.len()) * SLAB * 8
            + g.big.iter().map(|b| b.len() * 8).sum::<usize>()
            + g.boxes.len() * BOX_CHUNK * 4
    }

    /// The young indices of every boxed node of this cycle (A.3). Only at a safe point.
    pub(crate) fn boxed_ids(&mut self) -> Vec<u32> {
        self.chunks.get_mut().boxes.iter().flat_map(|c| c.iter().copied()).filter(|&id| id != NO_ID).collect()
    }

    /// Start a new cycle: every node of this one is gone. Only at a safe point, after the collector has freed or
    /// taken over every boxed payload of the cycle.
    pub(crate) fn reset(&mut self) {
        let (lo, hi) = self.range();
        let mut pool = self.pool.lock();
        if hi > lo {
            for s in (lo as usize >> SEG_BITS)..=((hi as usize - 1) >> SEG_BITS) {
                let p = self.segs[s].swap(null_mut(), Relaxed);
                if !p.is_null() {
                    *self.installed.get_mut() -= 1;
                    if pool.len() < SEG_POOL {
                        pool.push(SegPtr(p));
                    } else {
                        free_seg(p);
                    }
                }
            }
        }
        drop(pool);
        // the next cycle starts at a fresh segment, so ids of this one are not reused (until the space wraps)
        let mut seg_end = (hi as usize).div_ceil(SEG_LEN);
        if seg_end >= MAX_SEGS - 2 {
            seg_end = 0;
        }
        self.lo_chunk = (seg_end * SEG_LEN) as u32 / ID_CHUNK;
        self.next_chunk.store(self.lo_chunk, Relaxed);
        self.gen += 1;
        let g = self.chunks.get_mut();
        for b in std::mem::take(&mut g.slabs) {
            if g.free.len() < SLAB_POOL {
                g.free.push(b);
            }
        }
        g.big.clear();
        g.boxes.clear();
        *self.bytes.get_mut() = 0;
        *self.boxed.get_mut() = 0;
    }
}

impl Region for YoungArena {
    #[inline]
    fn args<'a>(&'a self, n: &Node) -> &'a [u32] {
        // SAFETY: `data` of a young compound points at `len` slots of a slab or slice owned by the arena for this
        // cycle; the borrow of the arena ends before the reset (which needs `&mut`).
        unsafe { n.args_by_ptr() }
    }
    #[inline]
    fn words<'a>(&'a self, n: &Node) -> &'a [u64] {
        // SAFETY: a young payload points at `len` words of a slab (arena placement) or of its box; slabs live until
        // the reset, boxes until the collector frees the node (both need `&mut`).
        unsafe { n.words_by_ptr() }
    }
}

impl Drop for YoungArena {
    fn drop(&mut self) {
        for id in self.boxed_ids() {
            // SAFETY: a listed node of the current cycle owns its box; the arena is going away.
            unsafe { free_box(self.node(id)) };
        }
        for s in self.segs.iter() {
            let p = s.load(Relaxed);
            if !p.is_null() {
                free_seg(p);
            }
        }
        for p in self.pool.get_mut().drain(..) {
            free_seg(p.0);
        }
    }
}

/// What a new node stores besides itself.
#[derive(Clone, Copy)]
pub(crate) enum Body<'a> {
    /// nothing: the node's `data` is the value
    Inline,
    Args(&'a [u32]),
    Words(&'a [u64]),
}

// --------------------------------------------------------------------------------------------------------------
// The interning table of the young region (spike 4's table C).

const SHARD_BITS: u32 = 7;
const SHARDS: usize = 1 << SHARD_BITS;
/// 128 shards of 512 slots hold a 1 MB nursery (about 26 000 nodes) at half load without growing.
const INITIAL: usize = 1 << 9;

fn shard_of(hash: u64) -> usize {
    (hash >> 25) as usize & (SHARDS - 1)
}

/// An open-addressing array; a slot holds `(hash >> 32) << 32 | (id + 1)`, 0 when empty. At most half full; slots
/// only go from empty to full between safe points, so a lock-free probe terminates and sees a prefix of insertions.
struct Slots {
    mask: usize,
    s: Box<[AtomicU64]>,
}

impl Slots {
    fn new(cap: usize) -> Slots {
        Slots { mask: cap - 1, s: (0..cap).map(|_| AtomicU64::new(0)).collect() }
    }
    fn find_from(&self, hash: u64, mut i: usize, eq: &impl Fn(u32) -> bool) -> Result<u32, usize> {
        let tag = hash >> 32;
        loop {
            let e = self.s[i].load(Acquire);
            if e == 0 {
                return Err(i);
            }
            if e >> 32 == tag {
                let id = (e as u32) - 1;
                if eq(id) {
                    return Ok(id);
                }
            }
            i = (i + 1) & self.mask;
        }
    }
    fn start(&self, hash: u64) -> usize {
        (hash >> 32) as usize & self.mask
    }
    fn place(&self, e: u64) {
        let mut i = (e >> 32) as usize & self.mask;
        while self.s[i].load(Relaxed) != 0 {
            i = (i + 1) & self.mask;
        }
        self.s[i].store(e, Release);
    }
}

struct Writer {
    count: usize,
    /// Arrays replaced by growth: readers may still probe them until the next safe point, which frees them.
    #[allow(clippy::vec_box)]
    retired: Vec<Box<Slots>>,
}

struct Shard {
    cur: AtomicPtr<Slots>,
    w: Mutex<Writer>,
}

pub(crate) struct YoungTable(Box<[crossbeam_utils::CachePadded<Shard>]>);

impl YoungTable {
    pub(crate) fn new() -> Self {
        YoungTable(
            (0..SHARDS)
                .map(|_| {
                    crossbeam_utils::CachePadded::new(Shard {
                        cur: AtomicPtr::new(Box::into_raw(Box::new(Slots::new(INITIAL)))),
                        w: Mutex::new(Writer { count: 0, retired: vec![] }),
                    })
                })
                .collect(),
        )
    }

    /// The id of the stored term with this hash for which `eq` holds, or the id `make` stores (canonical: look-up
    /// and insertion of a hash happen under one critical section).
    pub(crate) fn intern(&self, hash: u64, eq: impl Fn(u32) -> bool, make: impl FnOnce() -> u32) -> u32 {
        let sh = &self.0[shard_of(hash)];
        let seen = sh.cur.load(Acquire);
        // SAFETY: arrays stay allocated until a safe point, and none can run while `&self` is borrowed.
        let slots = unsafe { &*seen };
        let empty = match slots.find_from(hash, slots.start(hash), &eq) {
            Ok(id) => return id,
            Err(i) => i,
        };
        let mut w = sh.w.lock();
        let cur = sh.cur.load(Relaxed);
        // SAFETY: as above; under the lock `cur` is the latest array.
        let slots = unsafe { &*cur };
        let from = if cur == seen { empty } else { slots.start(hash) };
        if let Ok(id) = slots.find_from(hash, from, &eq) {
            return id;
        }
        let id = make();
        let entry = (hash >> 32) << 32 | (id as u64 + 1);
        if (w.count + 1) * 2 > slots.s.len() {
            let grown = Box::new(Slots::new(slots.s.len() * 2));
            for e in slots.s.iter() {
                let e = e.load(Relaxed);
                if e != 0 {
                    grown.place(e);
                }
            }
            grown.place(entry);
            let old = sh.cur.swap(Box::into_raw(grown), AcqRel);
            // SAFETY: `old` was created by `Box::into_raw` and is no longer the current array.
            w.retired.push(unsafe { Box::from_raw(old) });
        } else {
            slots.place(entry);
        }
        w.count += 1;
        id
    }

    /// A lock-free look-up only (verification).
    pub(crate) fn find(&self, hash: u64, eq: impl Fn(u32) -> bool) -> Option<u32> {
        // SAFETY: as in `intern`.
        let slots = unsafe { &*self.0[shard_of(hash)].cur.load(Acquire) };
        slots.find_from(hash, slots.start(hash), &eq).ok()
    }

    pub(crate) fn len(&mut self) -> usize {
        self.0.iter_mut().map(|s| s.w.get_mut().count).sum()
    }

    pub(crate) fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    /// Bytes in current and retired arrays.
    pub(crate) fn bytes(&mut self) -> usize {
        let mut b = 0;
        for s in self.0.iter_mut() {
            // SAFETY: as in `intern`.
            b += unsafe { &**s.cur.get_mut() }.s.len() * 8;
            b += s.w.get_mut().retired.iter().map(|r| r.s.len() * 8).sum::<usize>();
        }
        b
    }

    /// Every stored id (verification).
    pub(crate) fn for_each(&self, mut f: impl FnMut(u32)) {
        for s in self.0.iter() {
            // SAFETY: as in `intern`.
            for e in unsafe { &*s.cur.load(Relaxed) }.s.iter() {
                let e = e.load(Relaxed);
                if e != 0 {
                    f((e as u32) - 1);
                }
            }
        }
    }

    /// Empty the table and free every retired array; the table returns to its initial size. Only at a safe point.
    pub(crate) fn clear(&mut self) {
        for s in self.0.iter_mut() {
            let w = s.w.get_mut();
            w.retired.clear();
            w.count = 0;
            let cur = s.cur.get_mut();
            // SAFETY: we hold `&mut`: no reader exists.
            let slots = unsafe { &mut **cur };
            if slots.s.len() > INITIAL {
                // SAFETY: `*cur` came from `Box::into_raw`; replaced before anyone can read it again.
                drop(unsafe { Box::from_raw(*cur) });
                *cur = Box::into_raw(Box::new(Slots::new(INITIAL)));
            } else {
                for e in slots.s.iter_mut() {
                    *e.get_mut() = 0;
                }
            }
        }
    }
}

impl Drop for YoungTable {
    fn drop(&mut self) {
        for s in self.0.iter_mut() {
            // SAFETY: created by `Box::into_raw`, dropped once.
            drop(unsafe { Box::from_raw(*s.cur.get_mut()) });
        }
    }
}
