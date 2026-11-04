//! Key lookup in associations without a linear scan per lookup (decision 1B.6).
//!
//! An association is an ordinary hash-consed term, `assoc(rule(k1, v1), ...)`, so it carries no index of its own.
//! Instead the library keeps a small cache from association terms to hash maps `key -> position`, both identified
//! by their term handle. Terms are hash-consed, so a handle is the term's identity, and handles do not change
//! between two safe points: an entry is valid until the next one. The cache is emptied whenever the kernel is
//! traced, which happens at every collection, the only time handles move or are reused ([`Indexes::clear`], called
//! from the natives' `trace`). Lookups in the same association during one top-level evaluation (a loop, a `map`)
//! therefore build its index once, in linear time, and then cost a hash probe each; a later statement rebuilds it
//! once.
//!
//! Small associations ([`SMALL`] entries or fewer) are scanned instead: a scan of a few keys is cheaper than a
//! hash probe plus the lock.
use crate::ctx::{Lib, Meter};
use mc_core::T;
use mc_eval::{NativeCx, Res};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Associations with at most this many entries are scanned, not indexed.
pub(crate) const SMALL: usize = 16;
/// The most associations the cache keeps.
const SLOTS: usize = 16;
/// The most keys the cache keeps over all its associations (about 24 bytes each).
const MAX_KEYS: usize = 1 << 23;

/// The index of one association: key handle to 0-based entry position.
pub(crate) type KeyIndex = FxHashMap<u32, u32>;

struct Slot {
    /// the association's handle
    term: u32,
    index: Arc<KeyIndex>,
    /// the cache clock when last used (least recently used is evicted)
    used: u64,
}

#[derive(Default)]
struct Slots {
    slots: Vec<Slot>,
    clock: u64,
    keys: usize,
}

/// The cache of association indexes, shared by every task (a short lock per lookup).
#[derive(Default)]
pub(crate) struct Indexes(Mutex<Slots>);

impl Indexes {
    /// The cached index of association `a`.
    pub fn get(&self, a: T<'_>) -> Option<Arc<KeyIndex>> {
        let mut g = self.0.lock();
        g.clock += 1;
        let now = g.clock;
        let s = g.slots.iter_mut().find(|s| s.term == a.raw())?;
        s.used = now;
        Some(s.index.clone())
    }

    /// Whether `a` has a cached index (it is then an association in normal form).
    pub fn contains(&self, a: T<'_>) -> bool {
        self.0.lock().slots.iter().any(|s| s.term == a.raw())
    }

    /// Keep `index` as the index of association `a`, evicting the least recently used entries to make room.
    pub fn put(&self, a: T<'_>, index: Arc<KeyIndex>) {
        let n = index.len();
        if n > MAX_KEYS {
            return;
        }
        let mut g = self.0.lock();
        if g.slots.iter().any(|s| s.term == a.raw()) {
            return;
        }
        while g.slots.len() >= SLOTS || g.keys + n > MAX_KEYS {
            let Some((i, _)) = g.slots.iter().enumerate().min_by_key(|(_, s)| s.used) else { break };
            let old = g.slots.swap_remove(i);
            g.keys -= old.index.len();
        }
        g.clock += 1;
        let used = g.clock;
        g.keys += n;
        g.slots.push(Slot { term: a.raw(), index, used });
    }

    /// Forget every index: handles may move or be reused after this safe point.
    pub fn clear(&self) {
        let mut g = self.0.lock();
        g.slots.clear();
        g.keys = 0;
    }
}

/// The index of the entries of an association in normal form, built in linear time with fuel. A malformed entry
/// (impossible in a normal form) is left out; of repeated keys (equally impossible) the first is kept.
pub(crate) fn build<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, entries: &[T<'h>]) -> Res<KeyIndex> {
    let h = cx.heap();
    let mut m = KeyIndex::default();
    m.reserve(entries.len());
    let mut meter = Meter::default();
    for (i, &e) in entries.iter().enumerate() {
        meter.step(cx)?;
        if let Some((k, _)) = lib.entry(h, e) {
            m.entry(k.raw()).or_insert(i as u32);
        }
    }
    Ok(m)
}

/// The position of key `k` among the entries of the association `a` (a normal form with arguments `entries`).
/// `build`: build and cache the index if `a` has none (for lookups, which tend to repeat); otherwise a missing
/// index means a scan (for operations that copy the association anyway, such as `insert`).
pub(crate) fn position<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    a: T<'h>,
    entries: &[T<'h>],
    k: T<'h>,
    build_index: bool,
) -> Res<Option<usize>> {
    let h = cx.heap();
    if entries.len() > SMALL {
        let index = match lib.indexes.get(a) {
            Some(i) => Some(i),
            None if build_index => {
                let i = Arc::new(build(lib, cx, entries)?);
                lib.indexes.put(a, i.clone());
                Some(i)
            }
            None => None,
        };
        if let Some(i) = index {
            return Ok(i.get(&k.raw()).map(|&p| p as usize));
        }
    }
    let mut meter = Meter::default();
    for (i, &e) in entries.iter().enumerate() {
        meter.step(cx)?;
        if lib.key(h, e) == k {
            return Ok(Some(i));
        }
    }
    Ok(None)
}
