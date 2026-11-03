//! The young region: spike 4's concurrent store C (decision 4.1), made resettable.
//!
//! - [`YoungArena`]: nodes in fixed segments behind a directory of atomic pointers (never moved, read without locks);
//!   ids reserved in per-thread chunks of [`ID_CHUNK`], argument slices bump-allocated from per-thread chunks. Ids
//!   are not dense and not in global creation order (the conflict this spike resolves).
//! - [`YoungTable`]: 128 shards of open-addressing arrays probed without locks, a shard lock only on a miss; arrays
//!   replaced by growth are retired and freed at the next safe point (spike 4 kept them until the table was dropped).
//!
//! At a minor collection (`&mut`, so no task is running) the arena is reset wholesale: its segments leave the
//! directory (pooled for reuse), the next cycle starts at the next segment boundary, and every thread's cached
//! chunks are invalidated by a generation number. A handle into a previous cycle therefore finds a null segment and
//! panics instead of reading a reused node (no id reuse until the 2^30 young id space wraps).
use parking_lot::Mutex;
use std::cell::Cell;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering::*};

/// A stored node, in both regions. `data`: integer value, or the argument slice (young: a pointer; old: a start
/// index into the old argument array). `owner`: the worker that created it (instrumentation, in the padding).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Node {
    pub hash: u64,
    pub mask: u64,
    pub data: u64,
    /// head handle bits, or [`TAG_INT`]
    pub head: u32,
    pub len: u32,
    /// tree size, saturating (granularity estimate, spike 4)
    pub size: u32,
    pub owner: u32,
}

/// Head tag of a stored (large) integer.
pub const TAG_INT: u32 = u32::MAX;

pub const NODE: usize = std::mem::size_of::<Node>();

const SEG_BITS: u32 = 16;
const SEG_LEN: usize = 1 << SEG_BITS;
/// Young indices are below 2^30; the last segment is never allocated (its ids are the evaluator's holes).
const MAX_SEGS: usize = 1 << (30 - SEG_BITS);
pub const ID_CHUNK: u32 = 256;
/// Argument slots reserved by a thread at a time (64 KB); longer slices get their own allocation.
const ARG_CHUNK: usize = 1 << 14;
/// Segments and argument chunks kept for reuse after a reset (fresh pages cost more than the collection, 1b.6).
const SEG_POOL: usize = 4;
const ARG_POOL: usize = 64;

#[derive(Clone, Copy)]
struct Local {
    key: u64,
    next: u32,
    end: u32,
    args: *mut u32,
    args_left: usize,
}

const NO_LOCAL: Local = Local { key: 0, next: 0, end: 0, args: null_mut(), args_left: 0 };

thread_local! {
    static LOCAL: Cell<Local> = const { Cell::new(NO_LOCAL) };
}

static ARENA_IDS: AtomicU64 = AtomicU64::new(1);

struct SegPtr(*mut Node);
// SAFETY: a pooled segment is owned by the arena and touched only under its mutex or with `&mut`.
unsafe impl Send for SegPtr {}

#[derive(Default)]
struct ArgState {
    /// chunks handed out in this cycle
    owned: Vec<Box<[u32]>>,
    /// chunks free for reuse
    free: Vec<Box<[u32]>>,
    /// oversized slices of this cycle
    big: Vec<Box<[u32]>>,
}

pub struct YoungArena {
    id: u64,
    gen: u64,
    segs: Box<[AtomicPtr<Node>]>,
    next_chunk: AtomicU32,
    lo_chunk: u32,
    args: Mutex<ArgState>,
    arg_chunks: AtomicUsize,
    big_bytes: AtomicUsize,
    pool: Mutex<Vec<SegPtr>>,
    installed: AtomicUsize,
}

impl Default for YoungArena {
    fn default() -> Self {
        Self::new()
    }
}

fn alloc_seg() -> *mut Node {
    Box::into_raw(Box::<[Node]>::new_uninit_slice(SEG_LEN)) as *mut Node
}

fn free_seg(p: *mut Node) {
    // SAFETY: `p` came from `alloc_seg` (a boxed slice of SEG_LEN uninitialised nodes) and is dropped once.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p as *mut std::mem::MaybeUninit<Node>, SEG_LEN)) });
}

impl YoungArena {
    pub fn new() -> YoungArena {
        YoungArena {
            id: ARENA_IDS.fetch_add(1, Relaxed),
            gen: 0,
            segs: (0..MAX_SEGS).map(|_| AtomicPtr::new(null_mut())).collect(),
            next_chunk: AtomicU32::new(0),
            lo_chunk: 0,
            args: Mutex::new(ArgState::default()),
            arg_chunks: AtomicUsize::new(0),
            big_bytes: AtomicUsize::new(0),
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
    pub fn push(&self, n: Node) -> u32 {
        let id = self.with_local(|l| {
            if l.next == l.end {
                let c = self.next_chunk.fetch_add(1, Relaxed);
                let start = c
                    .checked_mul(ID_CHUNK)
                    .filter(|&s| (s as usize) < (MAX_SEGS - 1) * SEG_LEN)
                    .expect("young id space full (one evaluation made more than 2^30 terms)");
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

    /// The node at a young index. Panics on an index of a previous cycle (a handle that was not rooted across a
    /// safe point).
    #[inline]
    pub fn node(&self, id: u32) -> &Node {
        let seg = self.segs[(id >> SEG_BITS) as usize].load(Acquire);
        assert!(!seg.is_null(), "use of a collected young term {id} (a handle was not rooted across a safe point)");
        // SAFETY: published ids point at written nodes of an installed segment.
        unsafe { &*seg.add(id as usize & (SEG_LEN - 1)) }
    }

    pub fn alloc_args(&self, a: &[u32]) -> *const u32 {
        if a.is_empty() {
            return NonNull::dangling().as_ptr();
        }
        if a.len() > ARG_CHUNK / 8 {
            let b: Box<[u32]> = a.into();
            let p = b.as_ptr();
            self.big_bytes.fetch_add(a.len() * 4, Relaxed);
            self.args.lock().big.push(b);
            return p;
        }
        self.with_local(|l| {
            if l.args_left < a.len() {
                let mut g = self.args.lock();
                let mut b = g.free.pop().unwrap_or_else(|| vec![0u32; ARG_CHUNK].into_boxed_slice());
                l.args = b.as_mut_ptr();
                l.args_left = ARG_CHUNK;
                g.owned.push(b);
                self.arg_chunks.fetch_add(1, Relaxed);
            }
            let p = l.args;
            // SAFETY: `p` has `args_left >= a.len()` slots left in a chunk owned by the arena until the next reset.
            unsafe {
                std::ptr::copy_nonoverlapping(a.as_ptr(), p, a.len());
                l.args = p.add(a.len());
            }
            l.args_left -= a.len();
            p as *const u32
        })
    }

    #[inline]
    pub fn args(&self, n: &Node) -> &[u32] {
        // SAFETY: `data` of a young compound node is a pointer to `len` slots owned by the arena for this cycle.
        unsafe { std::slice::from_raw_parts(n.data as *const u32, n.len as usize) }
    }

    /// The young indices of this cycle: `lo..hi` (with gaps: unused tails of chunks hold no node).
    pub fn range(&self) -> (u32, u32) {
        (self.lo_chunk * ID_CHUNK, self.next_chunk.load(Relaxed) * ID_CHUNK)
    }

    /// Is `id` an index of this cycle with an installed segment?
    pub fn valid(&self, id: u32) -> bool {
        let (lo, hi) = self.range();
        id >= lo && id < hi && !self.segs[(id >> SEG_BITS) as usize].load(Relaxed).is_null()
    }

    /// Bytes taken by this cycle (the nursery trigger): reserved id chunks and argument chunks, so it overstates
    /// the bytes in use by at most one chunk of each kind per thread.
    pub fn young_bytes(&self) -> usize {
        let chunks = (self.next_chunk.load(Relaxed) - self.lo_chunk) as usize;
        chunks * ID_CHUNK as usize * NODE + self.arg_chunks.load(Relaxed) * ARG_CHUNK * 4 + self.big_bytes.load(Relaxed)
    }

    pub fn reserved_bytes(&mut self) -> usize {
        let segs = *self.installed.get_mut() + self.pool.get_mut().len();
        let g = self.args.get_mut();
        segs * SEG_LEN * NODE + (g.owned.len() + g.free.len()) * ARG_CHUNK * 4 + g.big.iter().map(|b| b.len() * 4).sum::<usize>()
    }

    /// Start a new cycle: every node of this one is gone. Only at a safe point (`&mut`: no task is running).
    pub fn reset(&mut self) {
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
        let g = self.args.get_mut();
        let owned = std::mem::take(&mut g.owned);
        for b in owned {
            if g.free.len() < ARG_POOL {
                g.free.push(b);
            }
        }
        g.big.clear();
        self.arg_chunks.store(0, Relaxed);
        self.big_bytes.store(0, Relaxed);
    }
}

impl Drop for YoungArena {
    fn drop(&mut self) {
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

pub struct YoungTable(Box<[crossbeam_utils::CachePadded<Shard>]>);

impl Default for YoungTable {
    fn default() -> Self {
        Self::new()
    }
}

impl YoungTable {
    pub fn new() -> Self {
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
    pub fn intern(&self, hash: u64, eq: impl Fn(u32) -> bool, make: impl FnOnce() -> u32) -> u32 {
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

    /// A lock-free look-up only (verification, at safe points).
    pub fn find(&self, hash: u64, eq: impl Fn(u32) -> bool) -> Option<u32> {
        // SAFETY: as in `intern`.
        let slots = unsafe { &*self.0[shard_of(hash)].cur.load(Acquire) };
        slots.find_from(hash, slots.start(hash), &eq).ok()
    }

    pub fn len(&mut self) -> usize {
        self.0.iter_mut().map(|s| s.w.get_mut().count).sum()
    }

    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    /// Bytes in current and retired arrays.
    pub fn bytes(&mut self) -> (usize, usize) {
        let (mut cur, mut ret) = (0, 0);
        for s in self.0.iter_mut() {
            // SAFETY: as in `intern`.
            cur += unsafe { &**s.cur.get_mut() }.s.len() * 8;
            ret += s.w.get_mut().retired.iter().map(|r| r.s.len() * 8).sum::<usize>();
        }
        (cur, ret)
    }

    /// Every stored id (verification).
    pub fn for_each(&self, mut f: impl FnMut(u32)) {
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

    /// Empty the table and free every retired array. Only at a safe point. Returns the bytes freed.
    pub fn clear(&mut self) -> usize {
        let mut freed = 0;
        for s in self.0.iter_mut() {
            let w = s.w.get_mut();
            freed += w.retired.iter().map(|r| r.s.len() * 8).sum::<usize>();
            w.retired.clear();
            w.count = 0;
            let cur = s.cur.get_mut();
            // SAFETY: we hold `&mut`: no reader exists.
            let slots = unsafe { &mut **cur };
            if slots.s.len() > INITIAL {
                freed += slots.s.len() * 8;
                // SAFETY: `*cur` came from `Box::into_raw`; replaced before anyone can read it again.
                drop(unsafe { Box::from_raw(*cur) });
                *cur = Box::into_raw(Box::new(Slots::new(INITIAL)));
            } else {
                for e in slots.s.iter_mut() {
                    *e.get_mut() = 0;
                }
            }
        }
        freed
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
