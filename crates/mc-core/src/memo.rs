//! The memo of normal forms (semantics 4.4, 18.5): `key -> (value, start epoch, read set)`, weak (1b.5) and lossy.
//!
//! Each entry is a seqlock (spike 4's decision 4.8, with I2's release fence, A.8): a writer that finds the entry busy
//! gives up, and a reader that sees a write in progress or a torn entry misses; it never returns a wrong pair.
//! Readers write no shared memory.
//!
//! Layout (A.1): dense by term id, one memo per region.
//! - [`YoungMemo`]: segments parallel to the young arena's, allocated zeroed on first write.
//! - [`OldMemo`]: an array parallel to the old nodes. It grows at the minor collection (survivors' young entries are
//!   written at their new ids) and slides with the nodes at the major one.
//! - [`SideMemo`]: entries with an old key and a young value (the old memo never refers to a young term). A small
//!   direct-mapped table with dirty blocks, swept at every minor collection: the only remembered set.
//!
//! The collector's accessors (`load`, `store`) take `&self` but run only at safe points, when no evaluation task
//! exists; they use relaxed accesses, so disjoint entries can be written from the pool without `unsafe`.
use crate::segs::{ZeroSegs, Zeroable};
use std::sync::atomic::{fence, AtomicU32, AtomicU64, Ordering::*};

/// The key of an entry made empty by [`Entry::invalidate`] (never a stored handle: it would be the symbol 2^30 - 1,
/// and immediates are not memoised).
const NO_KEY: u32 = u32::MAX;

#[derive(Default)]
pub(crate) struct Entry {
    seq: AtomicU32,
    key: AtomicU32,
    val: AtomicU32,
    epoch: AtomicU32,
    deps: AtomicU64,
}

// SAFETY: atomics only; all zero is an empty entry (sequence 0 = never written).
unsafe impl Zeroable for Entry {}

/// One entry's contents.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Item {
    pub key: u32,
    pub val: u32,
    pub epoch: u32,
    pub deps: u64,
}

impl Entry {
    /// The entry, if it holds `key` and is not being written.
    #[inline]
    pub(crate) fn get(&self, key: u32) -> Option<Item> {
        let s1 = self.seq.load(Acquire);
        if s1 & 1 != 0 || s1 == 0 {
            return None;
        }
        let it = Item {
            key: self.key.load(Relaxed),
            val: self.val.load(Relaxed),
            epoch: self.epoch.load(Relaxed),
            deps: self.deps.load(Relaxed),
        };
        fence(Acquire);
        (self.seq.load(Relaxed) == s1 && it.key == key).then_some(it)
    }

    /// Write the entry, unless another writer holds it (the memo may forget).
    #[inline]
    pub(crate) fn put(&self, it: Item) {
        let s = self.seq.load(Relaxed);
        if s & 1 != 0 || self.seq.compare_exchange(s, s.wrapping_add(1), Acquire, Relaxed).is_err() {
            return;
        }
        // The data stores must not become visible before the odd sequence number, or a reader could take new data
        // with the old number: a release fence, as in a textbook seqlock writer (A.8; spike 4 relied on x86 order).
        fence(Release);
        self.key.store(it.key, Relaxed);
        self.val.store(it.val, Relaxed);
        self.epoch.store(it.epoch, Relaxed);
        self.deps.store(it.deps, Relaxed);
        // 0 means "never written"; skip it on wrap-around
        let next = s.wrapping_add(2);
        self.seq.store(if next == 0 { 2 } else { next }, Release);
    }

    /// Make the entry miss for every key (best effort, like `put`).
    #[inline]
    pub(crate) fn invalidate(&self) {
        self.put(Item { key: NO_KEY, val: 0, epoch: 0, deps: 0 });
    }

    /// Collector: the entry's contents, if valid. Only at a safe point.
    #[inline]
    pub(crate) fn load(&self) -> Option<Item> {
        let s = self.seq.load(Relaxed);
        let key = self.key.load(Relaxed);
        (s != 0 && s & 1 == 0 && key != NO_KEY).then(|| Item {
            key,
            val: self.val.load(Relaxed),
            epoch: self.epoch.load(Relaxed),
            deps: self.deps.load(Relaxed),
        })
    }

    /// Collector: overwrite the entry (`None` empties it). Only at a safe point, one task per entry.
    #[inline]
    pub(crate) fn store(&self, it: Option<Item>) {
        match it {
            Some(it) => {
                self.key.store(it.key, Relaxed);
                self.val.store(it.val, Relaxed);
                self.epoch.store(it.epoch, Relaxed);
                self.deps.store(it.deps, Relaxed);
                let s = self.seq.load(Relaxed);
                self.seq.store((s | 1).wrapping_add(1).max(2), Relaxed);
            }
            None => self.seq.store(0, Relaxed),
        }
    }
}

const YOUNG_SEG_BITS: u32 = 16;

/// The young memo: one entry per young index, in segments parallel to the young arena's (A.1).
pub(crate) struct YoungMemo(ZeroSegs<Entry>);

impl YoungMemo {
    pub(crate) fn new() -> YoungMemo {
        YoungMemo(ZeroSegs::new(YOUNG_SEG_BITS, 30))
    }

    /// The entry of young index `i`, if its segment exists.
    #[inline]
    pub(crate) fn entry(&self, i: u32) -> Option<&Entry> {
        self.0.get(i as usize)
    }

    #[inline]
    pub(crate) fn entry_or_alloc(&self, i: u32) -> &Entry {
        self.0.get_or_alloc(i as usize)
    }

    /// Forget everything (end of a minor collection; the young ids are not reused).
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    pub(crate) fn bytes(&self) -> usize {
        self.0.bytes()
    }
}

/// The old memo: one entry per old node, at the node's index (A.1).
#[derive(Default)]
pub(crate) struct OldMemo(pub(crate) Vec<Entry>);

impl OldMemo {
    #[inline]
    pub(crate) fn entry(&self, i: u32) -> Option<&Entry> {
        self.0.get(i as usize)
    }

    pub(crate) fn bytes(&self) -> usize {
        self.0.capacity() * std::mem::size_of::<Entry>()
    }
}

/// Entries per dirty bit of the side memo.
const BLOCK: usize = 64;
const SIDE_MIN_BITS: u32 = 12;
const SIDE_MAX_BITS: u32 = 20;

/// Old keys with young values: direct-mapped by key, lossy. Writes mark their block dirty, so a minor collection
/// sweeps only the blocks written since the last one. The table is resized at collections to the number of entries
/// the last cycle left (between 2^12 and 2^20 entries).
pub(crate) struct SideMemo {
    e: Box<[Entry]>,
    dirty: Box<[AtomicU64]>,
    shift: u32,
}

impl SideMemo {
    pub(crate) fn new() -> SideMemo {
        SideMemo::with_bits(SIDE_MIN_BITS)
    }

    fn with_bits(bits: u32) -> SideMemo {
        let n = 1usize << bits;
        SideMemo {
            e: (0..n).map(|_| Entry::default()).collect(),
            dirty: (0..(n / BLOCK).div_ceil(64)).map(|_| AtomicU64::new(0)).collect(),
            shift: 64 - bits,
        }
    }

    #[inline]
    fn slot(&self, key: u32) -> usize {
        ((key as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> self.shift) as usize
    }

    #[inline]
    pub(crate) fn get(&self, key: u32) -> Option<Item> {
        self.e[self.slot(key)].get(key)
    }

    #[inline]
    pub(crate) fn put(&self, it: Item) {
        let i = self.slot(it.key);
        let (w, b) = ((i / BLOCK) / 64, 1u64 << ((i / BLOCK) % 64));
        if self.dirty[w].load(Relaxed) & b == 0 {
            self.dirty[w].fetch_or(b, Relaxed);
        }
        self.e[i].put(it);
    }

    /// Collector: visit every valid entry (in dirty blocks), then empty the table, resized to the load found.
    pub(crate) fn drain(&mut self, mut f: impl FnMut(Item)) {
        let mut found = 0usize;
        for (w, d) in self.dirty.iter_mut().enumerate() {
            let mut bits = std::mem::take(d.get_mut());
            while bits != 0 {
                let block = w * 64 + bits.trailing_zeros() as usize;
                bits &= bits - 1;
                for e in &self.e[block * BLOCK..(block + 1) * BLOCK] {
                    if let Some(it) = e.load() {
                        found += 1;
                        f(it);
                    }
                    e.store(None);
                }
            }
        }
        let bits = self.e.len().trailing_zeros();
        let want = if found * 2 > self.e.len() {
            bits + 1
        } else if found * 16 < self.e.len() {
            bits - 1
        } else {
            bits
        };
        let want = want.clamp(SIDE_MIN_BITS, SIDE_MAX_BITS);
        if want != bits {
            *self = SideMemo::with_bits(want);
        }
    }

    /// Every valid entry (verification).
    pub(crate) fn for_each(&self, mut f: impl FnMut(Item)) {
        for e in self.e.iter() {
            if let Some(it) = e.load() {
                f(it);
            }
        }
    }

    pub(crate) fn bytes(&self) -> usize {
        self.e.len() * std::mem::size_of::<Entry>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seqlock_entry() {
        let e = Entry::default();
        assert!(e.get(5).is_none() && e.load().is_none());
        let it = Item { key: 5, val: 9, epoch: 3, deps: 0xF0 };
        e.put(it);
        assert_eq!(e.get(5), Some(it));
        assert!(e.get(6).is_none(), "another key misses");
        e.invalidate();
        assert!(e.get(5).is_none() && e.load().is_none());
        e.store(Some(it));
        assert_eq!(e.get(5), Some(it));
        e.store(None);
        assert!(e.get(5).is_none());
    }

    #[test]
    fn side_memo_drains_dirty_blocks() {
        let mut s = SideMemo::new();
        for k in 0..100u32 {
            s.put(Item { key: k * 7, val: k, epoch: 1, deps: 0 });
        }
        let mut seen = vec![];
        s.drain(|it| seen.push(it.key));
        assert!(!seen.is_empty() && seen.len() <= 100);
        let mut left = 0;
        s.for_each(|_| left += 1);
        assert_eq!(left, 0);
    }
}
