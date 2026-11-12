//! A concurrent, lossy memo of normal forms (semantics 4.4, 18.5): a direct-mapped cache of `key -> (normal form,
//! epoch, read set)`. Each entry is a seqlock: a writer that finds the entry busy gives up (the memo may forget), and
//! a reader that sees a write in progress or a torn entry misses (it never returns a wrong pair). Readers do not write
//! shared memory.
use crate::Term;
use std::sync::atomic::{fence, AtomicU32, AtomicU64, Ordering::*};

#[derive(Default)]
struct Entry {
    seq: AtomicU32,
    key: AtomicU32,
    val: AtomicU32,
    epoch: AtomicU32,
    deps: AtomicU64,
}

pub struct Memo {
    e: Box<[Entry]>,
    shift: u32,
}

impl Memo {
    pub fn new(bits: u32) -> Memo {
        Memo { e: (0..1usize << bits).map(|_| Entry::default()).collect(), shift: 64 - bits }
    }

    #[inline]
    fn slot(&self, key: Term) -> &Entry {
        &self.e[((key.0 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> self.shift) as usize]
    }

    /// The memoised `(normal form, epoch, read set)` of `key`, if the entry holds it.
    #[inline]
    pub fn get(&self, key: Term) -> Option<(Term, u32, u64)> {
        let e = self.slot(key);
        let s1 = e.seq.load(Acquire);
        if s1 & 1 != 0 || s1 == 0 {
            return None;
        }
        let (k, v, ep, d) = (e.key.load(Relaxed), e.val.load(Relaxed), e.epoch.load(Relaxed), e.deps.load(Relaxed));
        fence(Acquire);
        if e.seq.load(Relaxed) != s1 || k != key.0 {
            return None;
        }
        Some((Term(v), ep, d))
    }

    #[inline]
    pub fn put(&self, key: Term, val: Term, epoch: u32, deps: u64) {
        let e = self.slot(key);
        let s = e.seq.load(Relaxed);
        if s & 1 != 0 || e.seq.compare_exchange(s, s.wrapping_add(1), Acquire, Relaxed).is_err() {
            return;
        }
        e.key.store(key.0, Relaxed);
        e.val.store(val.0, Relaxed);
        e.epoch.store(epoch, Relaxed);
        e.deps.store(deps, Relaxed);
        // never wraps to 0 in practice (that would read as "never written": a miss, which is allowed)
        e.seq.store(s.wrapping_add(2), Release);
    }
}
