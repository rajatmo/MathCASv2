//! Single-writer, many-reader building blocks for definitions and their indexes (I2.4).
//!
//! Definitions are ordered effects (semantics 18.3): only the task on the sequential frontier, or the session between
//! evaluations, defines anything. So a structure here has at most one writer at a time while any number of tasks
//! read it, and is built so that:
//!
//! - readers take no lock and write no shared memory;
//! - an element, once published, never moves and is not freed before a safe point (`&mut self`), so a reader may hold
//!   `&T` for a whole evaluation;
//! - publication is one release store after the element is complete: a reader sees the state before an insertion or
//!   after it, never a partial element.
//!
//! Writers serialise on an internal mutex, uncontended by construction; it keeps the structures memory-safe even if
//! two writers ever raced.
//!
//! [`SymTable`] is the exception to "one writer": its elements are created on first use by any thread (they are
//! default values; a lost race frees the loser's segment), so evaluation tasks can record per-symbol measurements.
use parking_lot::Mutex;
use std::mem::MaybeUninit;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering::*};

const BASE_BITS: u32 = 4;
const BASE: usize = 1 << BASE_BITS;
/// Segments of doubling size: room for `BASE * (2^SEGS - 1)` elements.
const SEGS: usize = 28;

/// Segment and offset of element `i` (segment `k` holds `BASE << k` elements).
#[inline]
fn locate(i: usize) -> (usize, usize) {
    let j = i + BASE;
    let k = (usize::BITS - 1 - j.leading_zeros()) as usize - BASE_BITS as usize;
    (k, j - (BASE << k))
}

/// An append-only vector: segments of doubling size reached through a fixed directory, so elements never move.
pub(crate) struct AppendVec<T> {
    segs: [AtomicPtr<T>; SEGS],
    len: AtomicUsize,
    w: Mutex<()>,
}

// SAFETY: elements are shared by `&T` across threads (T: Sync) and moved in from the writer's thread (T: Send).
unsafe impl<T: Send + Sync> Sync for AppendVec<T> {}
// SAFETY: as above; the vector owns its elements.
unsafe impl<T: Send> Send for AppendVec<T> {}

impl<T> Default for AppendVec<T> {
    fn default() -> Self {
        AppendVec { segs: std::array::from_fn(|_| AtomicPtr::new(null_mut())), len: AtomicUsize::new(0), w: Mutex::new(()) }
    }
}

impl<T> AppendVec<T> {
    /// Number of published elements.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len.load(Acquire)
    }

    /// Append `v` and return its index (the publication point is the length store).
    pub(crate) fn push(&self, v: T) -> usize {
        let _g = self.w.lock();
        let i = self.len.load(Relaxed);
        let (k, off) = locate(i);
        assert!(k < SEGS, "append-only vector full");
        let mut seg = self.segs[k].load(Relaxed);
        if seg.is_null() {
            let b: Box<[MaybeUninit<T>]> = Box::new_uninit_slice(BASE << k);
            seg = Box::into_raw(b) as *mut T;
            self.segs[k].store(seg, Release);
        }
        // SAFETY: slot `i` is in bounds of segment `k`, unpublished (index >= len), and only this writer touches it.
        unsafe { seg.add(off).write(v) };
        self.len.store(i + 1, Release);
        i
    }

    /// The element at `i`, if published.
    #[inline]
    pub(crate) fn get(&self, i: usize) -> Option<&T> {
        if i >= self.len.load(Acquire) {
            return None;
        }
        let (k, off) = locate(i);
        let seg = self.segs[k].load(Acquire);
        // SAFETY: `i < len` (acquired), so the segment is installed and the element was written before publication;
        // published elements are never written again or freed while `&self` lives.
        Some(unsafe { &*seg.add(off) })
    }

    /// Every element, mutably (a safe point: `&mut self` proves there are no readers).
    pub(crate) fn for_each_mut(&mut self, mut f: impl FnMut(&mut T)) {
        let n = *self.len.get_mut();
        for i in 0..n {
            let (k, off) = locate(i);
            // SAFETY: element `i < len` is initialised; `&mut self` excludes every other access.
            f(unsafe { &mut *self.segs[k].load(Relaxed).add(off) });
        }
    }
}

impl<T> Drop for AppendVec<T> {
    fn drop(&mut self) {
        let n = *self.len.get_mut();
        for i in 0..n {
            let (k, off) = locate(i);
            // SAFETY: element `i < len` was initialised and is dropped once.
            unsafe { std::ptr::drop_in_place(self.segs[k].load(Relaxed).add(off)) };
        }
        for (k, s) in self.segs.iter().enumerate() {
            let p = s.load(Relaxed);
            if !p.is_null() {
                let slice = std::ptr::slice_from_raw_parts_mut(p as *mut MaybeUninit<T>, BASE << k);
                // SAFETY: allocated by `push` as a boxed slice of this length.
                drop(unsafe { Box::from_raw(slice) });
            }
        }
    }
}

/// SplitMix64's finaliser (probe start of [`KeyMap`]).
#[inline]
fn finalize(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A map from nonzero 64-bit keys to 32-bit values: open addressing, linear probing, at most half full. A slot's
/// value is written before its key (release), so a reader that finds the key sees the value; a value update is one
/// release store. Growth copies into a new array and swaps the pointer; the old array stays readable until a safe
/// point ([`KeyMap::reclaim`]) or drop.
#[derive(Default)]
pub(crate) struct KeyMap {
    cur: AtomicPtr<Slots>,
    w: Mutex<KmWriter>,
}

struct Slots {
    mask: usize,
    keys: Box<[AtomicU64]>,
    vals: Box<[AtomicU32]>,
}

#[derive(Default)]
struct KmWriter {
    count: usize,
    #[allow(clippy::vec_box)] // the boxes are the retired arrays themselves
    retired: Vec<Box<Slots>>,
}

impl Slots {
    fn new(cap: usize) -> Slots {
        Slots {
            mask: cap - 1,
            keys: (0..cap).map(|_| AtomicU64::new(0)).collect(),
            vals: (0..cap).map(|_| AtomicU32::new(0)).collect(),
        }
    }

    /// The slot holding `key`, or the empty slot where the probe ended (the table is at most half full).
    #[inline]
    fn find(&self, key: u64) -> Result<usize, usize> {
        let mut i = finalize(key) as usize & self.mask;
        loop {
            let k = self.keys[i].load(Acquire);
            if k == key {
                return Ok(i);
            }
            if k == 0 {
                return Err(i);
            }
            i = (i + 1) & self.mask;
        }
    }
}

impl KeyMap {
    /// The value under `key`.
    #[inline]
    pub(crate) fn get(&self, key: u64) -> Option<u32> {
        let p = self.cur.load(Acquire);
        if p.is_null() {
            return None;
        }
        // SAFETY: arrays are freed only by `reclaim` (`&mut self`) or drop.
        let s = unsafe { &*p };
        s.find(key).ok().map(|i| s.vals[i].load(Acquire))
    }

    /// Insert or update `key -> val` (`key` nonzero).
    pub(crate) fn set(&self, key: u64, val: u32) {
        debug_assert!(key != 0);
        let mut w = self.w.lock();
        let mut p = self.cur.load(Relaxed);
        if p.is_null() {
            p = Box::into_raw(Box::new(Slots::new(8)));
            self.cur.store(p, Release);
        }
        // SAFETY: as in `get`; the writer lock is held.
        let s = unsafe { &*p };
        match s.find(key) {
            Ok(i) => s.vals[i].store(val, Release),
            Err(i) => {
                if (w.count + 1) * 2 > s.keys.len() {
                    let g = Box::new(Slots::new(s.keys.len() * 2));
                    for j in 0..s.keys.len() {
                        let k = s.keys[j].load(Relaxed);
                        if k != 0 {
                            let e = g.find(k).unwrap_err();
                            g.vals[e].store(s.vals[j].load(Relaxed), Relaxed);
                            g.keys[e].store(k, Relaxed);
                        }
                    }
                    let e = g.find(key).unwrap_err();
                    g.vals[e].store(val, Relaxed);
                    g.keys[e].store(key, Relaxed);
                    // the swap publishes the whole grown array
                    let old = self.cur.swap(Box::into_raw(g), AcqRel);
                    // SAFETY: `old` came from `Box::into_raw` and is no longer reachable from `cur`.
                    w.retired.push(unsafe { Box::from_raw(old) });
                } else {
                    s.vals[i].store(val, Relaxed);
                    s.keys[i].store(key, Release);
                }
                w.count += 1;
            }
        }
    }

    /// Free arrays replaced by growth (a safe point: no reader can hold them). Returns how many.
    pub(crate) fn reclaim(&mut self) -> usize {
        let w = self.w.get_mut();
        let n = w.retired.len();
        w.retired.clear();
        n
    }
}

impl Drop for KeyMap {
    fn drop(&mut self) {
        let p = *self.cur.get_mut();
        if !p.is_null() {
            // SAFETY: from `Box::into_raw`, unreachable after drop.
            drop(unsafe { Box::from_raw(p) });
        }
    }
}

/// A table indexed by symbol, of default-initialised elements created on first use by any thread: segments of
/// doubling size behind a fixed directory, installed by compare-exchange (the loser of a race frees its segment).
/// Elements never move; reads take no lock. Symbol indices that were never touched read as absent.
pub(crate) struct SymTable<E: Default + Send + Sync> {
    segs: [AtomicPtr<E>; SEGS],
}

impl<E: Default + Send + Sync> Default for SymTable<E> {
    fn default() -> Self {
        SymTable { segs: std::array::from_fn(|_| AtomicPtr::new(null_mut())) }
    }
}

impl<E: Default + Send + Sync> SymTable<E> {
    /// The element of symbol index `i`, if its segment exists.
    #[inline]
    pub(crate) fn get(&self, i: u32) -> Option<&E> {
        let (k, off) = locate(i as usize);
        let p = self.segs.get(k)?.load(Acquire);
        // SAFETY: an installed segment holds `BASE << k` initialised elements and lives until drop.
        (!p.is_null()).then(|| unsafe { &*p.add(off) })
    }

    /// The element of symbol index `i`, creating its segment on first use.
    pub(crate) fn get_or_init(&self, i: u32) -> &E {
        let (k, off) = locate(i as usize);
        let d = &self.segs[k];
        let mut p = d.load(Acquire);
        if p.is_null() {
            let seg: Box<[E]> = (0..BASE << k).map(|_| E::default()).collect();
            let q = Box::into_raw(seg) as *mut E;
            match d.compare_exchange(null_mut(), q, AcqRel, Acquire) {
                Ok(_) => p = q,
                Err(cur) => {
                    // SAFETY: `q` was made above from a boxed slice of this length and never published.
                    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(q, BASE << k)) });
                    p = cur;
                }
            }
        }
        // SAFETY: as in `get`.
        unsafe { &*p.add(off) }
    }

    /// Every element of every installed segment, mutably (a safe point).
    pub(crate) fn for_each_mut(&mut self, mut f: impl FnMut(&mut E)) {
        for (k, d) in self.segs.iter_mut().enumerate() {
            let p = *d.get_mut();
            if !p.is_null() {
                // SAFETY: an installed segment of `BASE << k` elements; `&mut self` excludes readers.
                let seg = unsafe { std::slice::from_raw_parts_mut(p, BASE << k) };
                seg.iter_mut().for_each(&mut f);
            }
        }
    }
}

impl<E: Default + Send + Sync> Drop for SymTable<E> {
    fn drop(&mut self) {
        for (k, d) in self.segs.iter_mut().enumerate() {
            let p = *d.get_mut();
            if !p.is_null() {
                // SAFETY: installed by `get_or_init` from a boxed slice of this length; dropped once.
                drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, BASE << k)) });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_vec_publishes_in_order() {
        let v = AppendVec::default();
        for i in 0..1000 {
            assert_eq!(v.push(i * 3), i);
        }
        assert_eq!(v.len(), 1000);
        assert_eq!(v.get(999), Some(&2997));
        assert_eq!(v.get(1000), None);
    }

    #[test]
    fn key_map_grows_and_retires() {
        let mut m = KeyMap::default();
        for k in 1..500u64 {
            m.set(k * 77, k as u32);
        }
        m.set(77, 9);
        assert_eq!(m.get(77), Some(9));
        assert_eq!(m.get(77 * 499), Some(499));
        assert_eq!(m.get(5), None);
        assert!(m.reclaim() > 0);
        assert_eq!(m.get(77 * 250), Some(250));
    }

    #[test]
    fn sym_table_is_lazy() {
        let t: SymTable<AtomicU32> = SymTable::default();
        assert!(t.get(100_000).is_none());
        t.get_or_init(100_000).store(5, Relaxed);
        assert_eq!(t.get(100_000).unwrap().load(Relaxed), 5);
        assert_eq!(t.get(100_001).unwrap().load(Relaxed), 0);
        std::thread::scope(|s| {
            for j in 0..4 {
                let t = &t;
                s.spawn(move || t.get_or_init(7 + j).fetch_add(1, Relaxed));
            }
        });
        assert_eq!((7..11).map(|i| t.get(i).unwrap().load(Relaxed)).sum::<u32>(), 4);
    }
}
