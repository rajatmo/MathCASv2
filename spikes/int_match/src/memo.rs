//! Concurrent memos of normal forms (semantics 4.4, 18.5). Each entry is a seqlock holding `key -> (normal form,
//! epoch, read set)`: a writer that finds the entry busy gives up (the memo may forget), and a reader that sees a
//! write in progress or a torn entry misses (it never returns a wrong pair). Readers do not write shared memory.
//!
//! - [`MemoKind::Hashed`]: spike 4's direct-mapped table (decision 4.8), `2^bits` entries addressed by a hash of the
//!   id. Lossy: colliding terms evict each other. Every access is a random access into the table.
//! - [`MemoKind::Dense`]: one entry per term id, in segments parallel to the arena's (2^16 ids each), allocated
//!   zeroed on the first write into the segment. Loses an entry only to a concurrent writer of the same term; ids
//!   are handed out in per-thread chunks, so terms built together sit together and access is local, like spike 2's
//!   dense memo.
use crate::Term;
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::null_mut;
use std::sync::atomic::{fence, AtomicPtr, AtomicU32, AtomicU64, Ordering::*};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemoKind {
    Hashed,
    Dense,
}

#[derive(Default)]
struct Entry {
    seq: AtomicU32,
    key: AtomicU32,
    val: AtomicU32,
    epoch: AtomicU32,
    deps: AtomicU64,
}

impl Entry {
    #[inline]
    fn get(&self, key: Term) -> Option<(Term, u32, u64)> {
        let s1 = self.seq.load(Acquire);
        if s1 & 1 != 0 || s1 == 0 {
            return None;
        }
        let (k, v, ep, d) = (self.key.load(Relaxed), self.val.load(Relaxed), self.epoch.load(Relaxed), self.deps.load(Relaxed));
        fence(Acquire);
        if self.seq.load(Relaxed) != s1 || k != key.0 {
            return None;
        }
        Some((Term(v), ep, d))
    }

    #[inline]
    fn put(&self, key: Term, val: Term, epoch: u32, deps: u64) {
        let s = self.seq.load(Relaxed);
        if s & 1 != 0 || self.seq.compare_exchange(s, s.wrapping_add(1), Acquire, Relaxed).is_err() {
            return;
        }
        // the data stores must not become visible before the odd sequence number (a reader that sees one of them
        // must then see the entry busy): a release fence, as in a textbook seqlock writer (spike 4 relied on x86
        // store order here)
        fence(Release);
        self.key.store(key.0, Relaxed);
        self.val.store(val.0, Relaxed);
        self.epoch.store(epoch, Relaxed);
        self.deps.store(deps, Relaxed);
        self.seq.store(s.wrapping_add(2), Release);
    }
}

const SEG_BITS: u32 = 16;
const SEG: usize = 1 << SEG_BITS;
const DIR: usize = 1 << (31 - SEG_BITS);

pub struct Memo {
    kind: MemoKind,
    e: Box<[Entry]>,
    shift: u32,
    segs: Box<[AtomicPtr<Entry>]>,
}

fn seg_layout() -> Layout {
    Layout::array::<Entry>(SEG).unwrap()
}

impl Memo {
    pub fn new(kind: MemoKind, bits: u32) -> Memo {
        match kind {
            MemoKind::Hashed => Memo {
                kind,
                e: (0..1usize << bits).map(|_| Entry::default()).collect(),
                shift: 64 - bits,
                segs: Box::new([]),
            },
            MemoKind::Dense => Memo {
                kind,
                e: Box::new([]),
                shift: 0,
                segs: (0..DIR).map(|_| AtomicPtr::new(null_mut())).collect(),
            },
        }
    }

    #[inline]
    fn hashed(&self, key: Term) -> &Entry {
        &self.e[((key.0 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> self.shift) as usize]
    }

    #[inline]
    fn dense(&self, key: Term, create: bool) -> Option<&Entry> {
        let s = &self.segs[(key.0 >> SEG_BITS) as usize & (DIR - 1)];
        let mut p = s.load(Acquire);
        if p.is_null() {
            if !create {
                return None;
            }
            // SAFETY: a nonzero-size layout; all-zero bytes are a valid `Entry` (atomics, never written = empty).
            let q = unsafe { alloc_zeroed(seg_layout()) } as *mut Entry;
            assert!(!q.is_null(), "memo segment allocation failed");
            match s.compare_exchange(null_mut(), q, AcqRel, Acquire) {
                Ok(_) => p = q,
                Err(cur) => {
                    // SAFETY: `q` was never published
                    unsafe { dealloc(q as *mut u8, seg_layout()) };
                    p = cur;
                }
            }
        }
        // SAFETY: `p` is an installed segment of `SEG` entries, freed only at drop
        Some(unsafe { &*p.add(key.0 as usize & (SEG - 1)) })
    }

    /// The memoised `(normal form, epoch, read set)` of `key`, if the entry holds it.
    #[inline]
    pub fn get(&self, key: Term) -> Option<(Term, u32, u64)> {
        match self.kind {
            MemoKind::Hashed => self.hashed(key).get(key),
            MemoKind::Dense => self.dense(key, false)?.get(key),
        }
    }

    #[inline]
    pub fn put(&self, key: Term, val: Term, epoch: u32, deps: u64) {
        match self.kind {
            MemoKind::Hashed => self.hashed(key).put(key, val, epoch, deps),
            MemoKind::Dense => {
                if let Some(e) = self.dense(key, true) {
                    e.put(key, val, epoch, deps)
                }
            }
        }
    }
}

impl Drop for Memo {
    fn drop(&mut self) {
        for s in self.segs.iter() {
            let p = s.load(Relaxed);
            if !p.is_null() {
                // SAFETY: allocated by `dense` with this layout; no reader remains at drop
                unsafe { dealloc(p as *mut u8, seg_layout()) };
            }
        }
    }
}
