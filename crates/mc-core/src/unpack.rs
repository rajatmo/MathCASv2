//! The arguments of packed arrays, made on demand (decision N4a.4).
//!
//! A packed node has no argument slots, but its view is the compound term `list(...)` it stands for, whose arguments
//! are handles. The first view of a packed node in a cycle builds them (interning the leaves, or the rows for a rank
//! above 1) and keeps them here, so later views in the same cycle return the same slice. The slices are a cache, not
//! part of the heap: the collector does not trace them, and every collection empties the cache (`&mut`, so no
//! borrowed slice is alive), because the handles they hold may be young or renumbered. Handles taken from a slice
//! are ordinary terms: one kept across a safe point must be rooted like any other.
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

const SHARD_BITS: u32 = 6;
const SHARDS: usize = 1 << SHARD_BITS;

type Shard = Mutex<FxHashMap<u32, Box<[u32]>>>;

/// Unpacked argument slices, by the handle bits of their packed node.
pub(crate) struct Unpacked {
    shards: Box<[Shard]>,
    /// bytes of the slices held (they count towards the nursery trigger)
    bytes: AtomicUsize,
}

fn shard_of(id: u32) -> usize {
    (id.wrapping_mul(0x9E37_79B9) >> (32 - SHARD_BITS)) as usize
}

impl Unpacked {
    pub(crate) fn new() -> Unpacked {
        Unpacked { shards: (0..SHARDS).map(|_| Mutex::new(FxHashMap::default())).collect(), bytes: AtomicUsize::new(0) }
    }

    /// The arguments of packed node `id`: the cached slice, or the one `make` builds (outside the lock: making them
    /// interns terms, possibly on other threads). When two threads race, the first slice stored wins; both are the
    /// same handles, since interning is canonical.
    pub(crate) fn get_or_make(&self, id: u32, make: impl FnOnce() -> Vec<u32>) -> &[u32] {
        let shard = &self.shards[shard_of(id)];
        if let Some(s) = shard.lock().get(&id) {
            let p: *const [u32] = &**s;
            // SAFETY: a slice's box is never moved or freed while `&self` is borrowed (only `clear`, with `&mut`,
            // drops boxes; rehashing the map moves the box pointers, not their contents).
            return unsafe { &*p };
        }
        let made = make().into_boxed_slice();
        let mut g = shard.lock();
        let s = g.entry(id).or_insert_with(|| {
            self.bytes.fetch_add(made.len() * 4, Relaxed);
            made
        });
        let p: *const [u32] = &**s;
        // SAFETY: as above.
        unsafe { &*p }
    }

    /// Bytes of the slices held.
    pub(crate) fn bytes(&self) -> usize {
        self.bytes.load(Relaxed)
    }

    /// Drop every slice (at a collection: the handles they hold may die or move).
    pub(crate) fn clear(&mut self) {
        for s in self.shards.iter_mut() {
            let m = s.get_mut();
            if !m.is_empty() {
                *m = FxHashMap::default();
            }
        }
        *self.bytes.get_mut() = 0;
    }
}
