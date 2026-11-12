//! Spike 4's lossy seqlock memo (decision 4.8): a direct-mapped cache of `key -> (normal form, epoch, read set)`. A
//! writer that finds an entry busy gives up; a reader that sees a write in progress or a torn entry misses. Readers
//! write nothing.
//!
//! Two changes for the collector:
//! - the slot is chosen by the key's **structural hash**, not its id, so renumbering a key (promotion, compaction)
//!   rewrites the entry in place instead of moving it;
//! - the collector's methods take `&mut self` (all tasks stopped) and use plain accesses.
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

/// One valid entry, as the collector sees it.
pub struct Item {
    pub key: u32,
    pub val: u32,
    pub epoch: u32,
    pub deps: u64,
}

impl Memo {
    pub fn new(bits: u32) -> Memo {
        Memo { e: (0..1usize << bits).map(|_| Entry::default()).collect(), shift: 64 - bits }
    }

    pub fn bytes(&self) -> usize {
        self.e.len() * std::mem::size_of::<Entry>()
    }

    #[inline]
    fn index(&self, hash: u64) -> usize {
        (hash.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> self.shift) as usize
    }

    #[inline]
    pub fn get(&self, key: u32, hash: u64) -> Option<(u32, u32, u64)> {
        let e = &self.e[self.index(hash)];
        let s1 = e.seq.load(Acquire);
        if s1 & 1 != 0 || s1 == 0 {
            return None;
        }
        let (k, v, ep, d) = (e.key.load(Relaxed), e.val.load(Relaxed), e.epoch.load(Relaxed), e.deps.load(Relaxed));
        fence(Acquire);
        if e.seq.load(Relaxed) != s1 || k != key {
            return None;
        }
        Some((v, ep, d))
    }

    #[inline]
    pub fn put(&self, key: u32, hash: u64, val: u32, epoch: u32, deps: u64) {
        let e = &self.e[self.index(hash)];
        let s = e.seq.load(Relaxed);
        if s & 1 != 0 || e.seq.compare_exchange(s, s.wrapping_add(1), Acquire, Relaxed).is_err() {
            return;
        }
        e.key.store(key, Relaxed);
        e.val.store(val, Relaxed);
        e.epoch.store(epoch, Relaxed);
        e.deps.store(deps, Relaxed);
        // 0 reads as "never written" (a miss, which is allowed), so skip it on wrap-around
        let next = s.wrapping_add(2);
        e.seq.store(if next == 0 { 2 } else { next }, Release);
    }

    /// Collector: store an entry (overwriting whatever the slot holds).
    pub fn put_mut(&mut self, hash: u64, it: Item) {
        let i = self.index(hash);
        let e = &mut self.e[i];
        *e.key.get_mut() = it.key;
        *e.val.get_mut() = it.val;
        *e.epoch.get_mut() = it.epoch;
        *e.deps.get_mut() = it.deps;
        let s = e.seq.get_mut();
        *s = (*s | 1).wrapping_add(1).max(2);
    }

    /// Collector: visit every valid entry; `f` returns the entry's new `(key, val)` or `None` to drop it. With
    /// `clear`, every entry is emptied after the visit (the young memo at a minor collection).
    pub fn sweep(&mut self, clear: bool, mut f: impl FnMut(&Item) -> Option<(u32, u32)>) {
        for e in self.e.iter_mut() {
            let s = *e.seq.get_mut();
            if s == 0 {
                continue;
            }
            if s & 1 == 0 {
                let it = Item { key: *e.key.get_mut(), val: *e.val.get_mut(), epoch: *e.epoch.get_mut(), deps: *e.deps.get_mut() };
                match f(&it) {
                    Some((k, v)) if !clear => {
                        *e.key.get_mut() = k;
                        *e.val.get_mut() = v;
                        continue;
                    }
                    _ => {}
                }
            }
            *e.seq.get_mut() = 0;
        }
    }

    /// `sweep` (without clearing) on the pool: entries are rewritten in place, independently.
    pub fn sweep_par(&mut self, f: impl Fn(&Item) -> Option<(u32, u32)> + Sync) {
        use rayon::prelude::*;
        self.e.par_iter_mut().with_min_len(4096).for_each(|e| {
            let s = *e.seq.get_mut();
            if s == 0 {
                return;
            }
            if s & 1 == 0 {
                let it = Item { key: *e.key.get_mut(), val: *e.val.get_mut(), epoch: *e.epoch.get_mut(), deps: *e.deps.get_mut() };
                if let Some((k, v)) = f(&it) {
                    *e.key.get_mut() = k;
                    *e.val.get_mut() = v;
                    return;
                }
            }
            *e.seq.get_mut() = 0;
        });
    }

    /// Collector: every valid entry with the index of its slot (verification).
    pub fn for_each(&self, mut f: impl FnMut(usize, &Item)) {
        for (i, e) in self.e.iter().enumerate() {
            let s = e.seq.load(Relaxed);
            if s != 0 && s & 1 == 0 {
                let it = Item { key: e.key.load(Relaxed), val: e.val.load(Relaxed), epoch: e.epoch.load(Relaxed), deps: e.deps.load(Relaxed) };
                f(i, &it);
            }
        }
    }

    pub fn capacity(&self) -> usize {
        self.e.len()
    }

    /// Collector: every valid entry moved to a memo of 2^bits entries (lossy where two land in one slot).
    pub fn resized(&mut self, bits: u32, hash_of: impl Fn(u32) -> u64) -> Memo {
        let mut m = Memo::new(bits);
        for e in self.e.iter_mut() {
            let s = *e.seq.get_mut();
            if s != 0 && s & 1 == 0 {
                let it = Item { key: *e.key.get_mut(), val: *e.val.get_mut(), epoch: *e.epoch.get_mut(), deps: *e.deps.get_mut() };
                m.put_mut(hash_of(it.key), it);
            }
        }
        m
    }

    /// The slot index of a hash (verification).
    pub fn slot_of(&self, hash: u64) -> usize {
        self.index(hash)
    }
}
