//! A node arena that many threads append to at once.
//!
//! - Nodes live in fixed-size segments reached through a directory of atomic pointers, so a node never moves and a
//!   reader needs no lock: a segment is installed once (compare-and-swap) and never replaced.
//! - Ids are handed out in chunks of [`ID_CHUNK`] per thread, so the shared counter is touched once per chunk rather
//!   than once per node. Ids are therefore not dense and their values depend on the schedule: nothing observable may
//!   depend on an id's value (ordering is structural, semantics 8), only on id equality.
//! - Argument slices are bump-allocated from per-thread chunks; the chunks are owned by the arena and freed with it.
//! - A node is written completely before its id is published (through the interning table, with release ordering, or
//!   through a join), and never written again; readers therefore see it whole.
use crate::Term;
use parking_lot::Mutex;
use std::cell::Cell;
use std::mem::MaybeUninit;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering::*};

const SEG_BITS: u32 = 16;
const SEG_LEN: usize = 1 << SEG_BITS;
/// Ids are below 2^31: the top bit of a handle marks an immediate.
const MAX_SEGS: usize = 1 << (31 - SEG_BITS);
// The last segment is never allocated: its ids are free for placeholders (the evaluator's holes).
/// Ids reserved by a thread at a time (divides `SEG_LEN`, so a chunk never straddles segments).
const ID_CHUNK: u32 = 256;
/// Argument slots reserved by a thread at a time; longer slices get their own allocation.
const ARG_CHUNK: usize = 1 << 16;

/// Head tags of atoms (never valid ids, which are below 2^31).
pub const TAG_INT: Term = Term(u32::MAX);
pub const TAG_SYM: Term = Term(u32::MAX - 1);
pub const TAG_REAL: Term = Term(u32::MAX - 2);

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Node {
    /// Structural hash: computed from the children's hashes, never from ids, so it is the same in every store and
    /// for every schedule.
    pub hash: u64,
    /// Symbols contained (bit `index % 64`).
    pub mask: u64,
    /// Integer value, real bits, symbol index, or pointer to the argument slice.
    pub data: u64,
    /// The head of a compound term, or [`TAG_INT`] / [`TAG_SYM`] / [`TAG_REAL`].
    pub head: Term,
    pub len: u32,
    /// Tree size, saturating: the static cost estimate used for granularity.
    pub size: u32,
}

#[derive(Clone, Copy)]
struct Local {
    arena: u64,
    next: u32,
    end: u32,
    args: *mut Term,
    args_left: usize,
}

const NO_LOCAL: Local = Local { arena: 0, next: 0, end: 0, args: null_mut(), args_left: 0 };

thread_local! {
    /// This thread's id and argument chunks, for the arena whose id is `arena` (reset when another arena is used, so
    /// a stale chunk of a dropped arena is never touched: arena ids are never reused).
    static LOCAL: Cell<Local> = const { Cell::new(NO_LOCAL) };
}

static ARENA_IDS: AtomicU64 = AtomicU64::new(1);

pub struct Arena {
    id: u64,
    segs: Box<[AtomicPtr<Node>]>,
    next_chunk: AtomicU32,
    owned: Mutex<Vec<Box<[Term]>>>,
}

impl Default for Arena {
    fn default() -> Self {
        Arena::new()
    }
}

impl Arena {
    pub fn new() -> Arena {
        Arena {
            id: ARENA_IDS.fetch_add(1, Relaxed),
            segs: (0..MAX_SEGS).map(|_| AtomicPtr::new(null_mut())).collect(),
            next_chunk: AtomicU32::new(0),
            owned: Mutex::new(vec![]),
        }
    }

    fn with_local<R>(&self, f: impl FnOnce(&mut Local) -> R) -> R {
        LOCAL.with(|c| {
            let mut l = c.get();
            if l.arena != self.id {
                l = Local { arena: self.id, ..NO_LOCAL };
            }
            let r = f(&mut l);
            c.set(l);
            r
        })
    }

    fn ensure_seg(&self, s: usize) {
        if self.segs[s].load(Acquire).is_null() {
            let p = Box::into_raw(Box::<[Node]>::new_uninit_slice(SEG_LEN)) as *mut Node;
            if self.segs[s].compare_exchange(null_mut(), p, AcqRel, Acquire).is_err() {
                // another thread installed it first
                drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p as *mut MaybeUninit<Node>, SEG_LEN)) });
            }
        }
    }

    /// Store a node and return its id. The caller publishes the id.
    pub fn push(&self, n: Node) -> u32 {
        let id = self.with_local(|l| {
            if l.next == l.end {
                let c = self.next_chunk.fetch_add(1, Relaxed);
                let start = c.checked_mul(ID_CHUNK).filter(|&s| (s as usize) < (MAX_SEGS - 1) * SEG_LEN).expect("arena full");
                self.ensure_seg((start >> SEG_BITS) as usize);
                l.next = start;
                l.end = start + ID_CHUNK;
            }
            l.next += 1;
            l.next - 1
        });
        let seg = self.segs[(id >> SEG_BITS) as usize].load(Acquire);
        unsafe { seg.add(id as usize & (SEG_LEN - 1)).write(n) };
        id
    }

    /// The node with this id. `id` must have been returned by [`Arena::push`] of this arena and published to this
    /// thread (all ids reachable through the store's API are).
    #[inline]
    pub fn node(&self, id: u32) -> &Node {
        let seg = self.segs[(id >> SEG_BITS) as usize].load(Acquire);
        debug_assert!(!seg.is_null());
        unsafe { &*seg.add(id as usize & (SEG_LEN - 1)) }
    }

    /// Copy an argument slice into the arena; the pointer stays valid as long as the arena.
    pub fn alloc_args(&self, a: &[Term]) -> *const Term {
        if a.is_empty() {
            return NonNull::dangling().as_ptr();
        }
        if a.len() > ARG_CHUNK / 8 {
            let b: Box<[Term]> = a.into();
            let p = b.as_ptr();
            self.owned.lock().push(b);
            return p;
        }
        self.with_local(|l| {
            if l.args_left < a.len() {
                let mut b: Box<[Term]> = vec![Term(0); ARG_CHUNK].into_boxed_slice();
                l.args = b.as_mut_ptr();
                l.args_left = ARG_CHUNK;
                self.owned.lock().push(b);
            }
            let p = l.args;
            unsafe {
                std::ptr::copy_nonoverlapping(a.as_ptr(), p, a.len());
                l.args = p.add(a.len());
            }
            l.args_left -= a.len();
            p as *const Term
        })
    }

    /// The argument slice of a compound node.
    #[inline]
    pub fn args(&self, n: &Node) -> &[Term] {
        unsafe { std::slice::from_raw_parts(n.data as *const Term, n.len as usize) }
    }

    /// Bytes reserved by the arena (segments and argument chunks), for reporting.
    pub fn reserved_bytes(&self) -> usize {
        let segs = self.segs.iter().filter(|s| !s.load(Relaxed).is_null()).count();
        segs * SEG_LEN * std::mem::size_of::<Node>() + self.owned.lock().iter().map(|b| b.len() * 4).sum::<usize>()
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        for s in self.segs.iter() {
            let p = s.load(Relaxed);
            if !p.is_null() {
                drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p as *mut MaybeUninit<Node>, SEG_LEN)) });
            }
        }
    }
}
