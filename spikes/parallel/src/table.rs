//! Three concurrent interning tables. Each maps a structural hash to the ids of the stored terms with that hash, and
//! guarantees canonical ids: a term is looked up and, if absent, inserted under one critical section for its hash, so
//! two threads building equal terms always get the same id.
//!
//! - [`Locked`] (A): one hash table behind one lock: the baseline.
//! - [`Sharded`] (B): the table split into shards by hash bits, each with its own lock.
//! - [`Optimistic`] (C): sharded, but a lookup reads the shard's open-addressing array without any lock or write to
//!   shared memory; only a miss takes the shard lock (to re-check and insert). Hash-consing workloads mostly hit
//!   (rebuilding a term that exists), so the common case costs a few loads.
use crossbeam_utils::CachePadded;
use hashbrown::HashTable;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering::*};

pub trait Table: Send + Sync {
    const NAME: &'static str;
    fn new() -> Self;
    /// The id of the stored term with this `hash` for which `eq` holds, or the id `make` stores. `rehash` gives the
    /// hash of a stored id (for growing).
    fn intern(&self, hash: u64, eq: impl Fn(u32) -> bool, make: impl FnOnce() -> u32, rehash: impl Fn(u32) -> u64)
        -> u32;
    /// Number of distinct terms stored.
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Shard selection uses hash bits 25..32. Inside a shard, hashbrown (B) uses the low bits for the bucket (fewer than 25
/// unless a shard exceeds 2^25 buckets) and the top 7 for its tags; the optimistic table (C) uses the high 32 bits for
/// both its slot index and its tag. Neither loses entropy to the shard choice.
const SHARD_BITS: u32 = 7;
const SHARDS: usize = 1 << SHARD_BITS;

fn shard_of(hash: u64) -> usize {
    (hash >> 25) as usize & (SHARDS - 1)
}

// --------------------------------------------------------------------------------------------------------------
// A: one lock

pub struct Locked(Mutex<HashTable<u32>>);

impl Table for Locked {
    const NAME: &'static str = "A:locked";
    fn new() -> Self {
        Locked(Mutex::new(HashTable::new()))
    }
    fn intern(&self, hash: u64, eq: impl Fn(u32) -> bool, make: impl FnOnce() -> u32, rehash: impl Fn(u32) -> u64)
        -> u32 {
        let mut t = self.0.lock();
        if let Some(&id) = t.find(hash, |&id| eq(id)) {
            return id;
        }
        let id = make();
        t.insert_unique(hash, id, |&id| rehash(id));
        id
    }
    fn len(&self) -> usize {
        self.0.lock().len()
    }
}

// --------------------------------------------------------------------------------------------------------------
// B: sharded locks

pub struct Sharded(Box<[CachePadded<Mutex<HashTable<u32>>>]>);

impl Table for Sharded {
    const NAME: &'static str = "B:sharded";
    fn new() -> Self {
        Sharded((0..SHARDS).map(|_| CachePadded::new(Mutex::new(HashTable::new()))).collect())
    }
    fn intern(&self, hash: u64, eq: impl Fn(u32) -> bool, make: impl FnOnce() -> u32, rehash: impl Fn(u32) -> u64)
        -> u32 {
        let mut t = self.0[shard_of(hash)].lock();
        if let Some(&id) = t.find(hash, |&id| eq(id)) {
            return id;
        }
        let id = make();
        t.insert_unique(hash, id, |&id| rehash(id));
        id
    }
    fn len(&self) -> usize {
        self.0.iter().map(|s| s.lock().len()).sum()
    }
}

// --------------------------------------------------------------------------------------------------------------
// C: lock-free lookup, sharded locked insertion

/// An open-addressing array (linear probing). A slot holds `(hash >> 32) << 32 | (id + 1)`, 0 when empty; the probe
/// starts at the high hash bits, so growing re-places entries from the slot alone, without touching the nodes. Slots
/// only ever go from empty to full, and an array is never more than half full, so a lock-free probe always terminates
/// and sees a consistent prefix of the insertions.
struct Slots {
    mask: usize,
    s: Box<[AtomicU64]>,
}

impl Slots {
    fn new(cap: usize) -> Slots {
        Slots { mask: cap - 1, s: (0..cap).map(|_| AtomicU64::new(0)).collect() }
    }
    /// Probe from slot `i`: the id, or the first empty slot (where a probe under the lock can resume: slots before it
    /// were full and never change).
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
    /// Only under the shard lock.
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
    /// Arrays replaced by growth. Readers may still be probing them, so they are kept until the table is dropped
    /// (their total size is below the current array's); a real implementation frees them at the collector's safe
    /// points. Boxed: readers hold pointers to the `Slots` themselves, which must not move.
    #[allow(clippy::vec_box)]
    retired: Vec<Box<Slots>>,
}

struct Shard {
    cur: AtomicPtr<Slots>,
    w: Mutex<Writer>,
}

pub struct Optimistic(Box<[CachePadded<Shard>]>);

const C_INITIAL: usize = 1 << 10;

impl Table for Optimistic {
    const NAME: &'static str = "C:optimistic";
    fn new() -> Self {
        Optimistic(
            (0..SHARDS)
                .map(|_| {
                    CachePadded::new(Shard {
                        cur: AtomicPtr::new(Box::into_raw(Box::new(Slots::new(C_INITIAL)))),
                        w: Mutex::new(Writer { count: 0, retired: vec![] }),
                    })
                })
                .collect(),
        )
    }
    fn intern(&self, hash: u64, eq: impl Fn(u32) -> bool, make: impl FnOnce() -> u32, _: impl Fn(u32) -> u64) -> u32 {
        let sh = &self.0[shard_of(hash)];
        // fast path: no lock, no shared write
        let seen = sh.cur.load(Acquire);
        let slots = unsafe { &*seen };
        let empty = match slots.find_from(hash, slots.start(hash), &eq) {
            Ok(id) => return id,
            Err(i) => i,
        };
        let mut w = sh.w.lock();
        // under the lock the current array is the latest; re-check (another thread may have inserted it meanwhile),
        // resuming where the lock-free probe stopped if the array is the same one
        let cur = sh.cur.load(Relaxed);
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
            w.retired.push(unsafe { Box::from_raw(old) });
        } else {
            slots.place(entry);
        }
        w.count += 1;
        id
    }
    fn len(&self) -> usize {
        self.0.iter().map(|s| s.w.lock().count).sum()
    }
}

impl Drop for Optimistic {
    fn drop(&mut self) {
        for s in self.0.iter() {
            drop(unsafe { Box::from_raw(s.cur.load(Relaxed)) });
        }
    }
}
