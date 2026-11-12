//! Spike 4's interning table C (decision 4.1): 128 shards, each an open-addressing array probed without locks or
//! shared writes; only a miss takes the shard's lock to re-check and insert. Copied unchanged except that arrays
//! replaced by growth are freed at a safe point ([`Optimistic::reclaim`], which needs `&mut`, so the borrow checker
//! proves no reader is probing them).
use crossbeam_utils::CachePadded;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering::*};

const SHARD_BITS: u32 = 7;
const SHARDS: usize = 1 << SHARD_BITS;

fn shard_of(hash: u64) -> usize {
    (hash >> 25) as usize & (SHARDS - 1)
}

/// A slot holds `(hash >> 32) << 32 | (id + 1)`, 0 when empty. Slots only go from empty to full and an array is never
/// more than half full, so a lock-free probe terminates and sees a prefix of the insertions.
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
    #[allow(clippy::vec_box)]
    retired: Vec<Box<Slots>>,
}

struct Shard {
    cur: AtomicPtr<Slots>,
    w: Mutex<Writer>,
}

pub struct Optimistic(Box<[CachePadded<Shard>]>);

const C_INITIAL: usize = 1 << 10;

impl Default for Optimistic {
    fn default() -> Self {
        Optimistic::new()
    }
}

impl Optimistic {
    pub fn new() -> Self {
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

    /// The id of the stored term with this `hash` for which `eq` holds, or the id `make` stores.
    pub fn intern(&self, hash: u64, eq: impl Fn(u32) -> bool, make: impl FnOnce() -> u32) -> u32 {
        let sh = &self.0[shard_of(hash)];
        let seen = sh.cur.load(Acquire);
        // SAFETY: arrays are freed only by `reclaim` (`&mut self`) or drop, never while a reader holds `&self`.
        let slots = unsafe { &*seen };
        let empty = match slots.find_from(hash, slots.start(hash), &eq) {
            Ok(id) => return id,
            Err(i) => i,
        };
        let mut w = sh.w.lock();
        let cur = sh.cur.load(Relaxed);
        // SAFETY: as above.
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
            // SAFETY: `old` came from `Box::into_raw` and is no longer reachable from `cur`.
            w.retired.push(unsafe { Box::from_raw(old) });
        } else {
            slots.place(entry);
        }
        w.count += 1;
        id
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(|s| s.w.lock().count).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Free arrays replaced by growth. `&mut self`: no lock-free reader can be probing them.
    pub fn reclaim(&mut self) -> usize {
        let mut n = 0;
        for s in self.0.iter_mut() {
            let w = s.w.get_mut();
            n += w.retired.len();
            w.retired.clear();
        }
        n
    }
}

impl Drop for Optimistic {
    fn drop(&mut self) {
        for s in self.0.iter() {
            // SAFETY: the current array came from `Box::into_raw`; nothing else refers to it at drop.
            drop(unsafe { Box::from_raw(s.cur.load(Relaxed)) });
        }
    }
}
