//! Single-writer, many-reader structures for rule sets and their indexes (question 3).
//!
//! Definitions are ordered effects (semantics 18.3): only the task on the sequential frontier, or the session between
//! evaluations, defines anything. So a rule set has at most one writer at a time, while any number of speculative
//! tasks read it. These structures make that case cheap:
//!
//! - readers take no lock and write no shared memory (no reader counts, no reference counts on hot cache lines);
//! - an element, once published, never moves and is never freed before a safe point (`&mut self`), so a reader may
//!   hold `&T` for the whole evaluation;
//! - publication is one release store after the element is complete, so a reader sees either the state before an
//!   insertion or after it, never a partial element.
//!
//! Writers serialise on an internal mutex, uncontended by construction; it keeps the structures memory-safe even if
//! two writers ever raced.
use crate::finalize;
use parking_lot::Mutex;
use std::mem::MaybeUninit;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering::*};

const BASE_BITS: u32 = 4;
const BASE: usize = 1 << BASE_BITS;
const SEGS: usize = 28;

/// An append-only vector: segments of doubling size reached through a fixed directory, so elements never move.
pub struct AppendVec<T> {
    segs: [AtomicPtr<T>; SEGS],
    len: AtomicUsize,
    w: Mutex<()>,
}

// SAFETY: elements are shared by `&T` across threads (T: Sync) and moved in from the writer's thread (T: Send).
unsafe impl<T: Send + Sync> Sync for AppendVec<T> {}
unsafe impl<T: Send> Send for AppendVec<T> {}

#[inline]
fn locate(i: usize) -> (usize, usize) {
    let j = i + BASE;
    let k = (usize::BITS - 1 - j.leading_zeros()) as usize - BASE_BITS as usize;
    (k, j - (BASE << k))
}

impl<T> Default for AppendVec<T> {
    fn default() -> Self {
        AppendVec::new()
    }
}

impl<T> AppendVec<T> {
    pub fn new() -> Self {
        AppendVec { segs: std::array::from_fn(|_| AtomicPtr::new(null_mut())), len: AtomicUsize::new(0), w: Mutex::new(()) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len.load(Acquire)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append `v` and return its index (the publication point is the length store).
    pub fn push(&self, v: T) -> usize {
        let _g = self.w.lock();
        let i = self.len.load(Relaxed);
        let (k, off) = locate(i);
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

    /// Every element, mutably (a safe point: `&mut self` proves there are no readers).
    pub fn for_each_mut(&mut self, mut f: impl FnMut(&mut T)) {
        let n = *self.len.get_mut();
        for i in 0..n {
            let (k, off) = locate(i);
            // SAFETY: element `i < len` is initialised; `&mut self` excludes every other access.
            f(unsafe { &mut *self.segs[k].load(Relaxed).add(off) });
        }
    }

    /// The element at `i`, if published.
    #[inline]
    pub fn get(&self, i: usize) -> Option<&T> {
        if i >= self.len.load(Acquire) {
            return None;
        }
        let (k, off) = locate(i);
        let seg = self.segs[k].load(Acquire);
        // SAFETY: `i < len` (acquired), so the segment is installed and the element was written before publication;
        // published elements are never written again or freed while `&self` lives.
        Some(unsafe { &*seg.add(off) })
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

/// A map from nonzero 64-bit keys to 32-bit values: open addressing, linear probing, at most half full. A slot's
/// value is written before its key (release), so a reader that finds the key sees the value; a value update is one
/// release store. Growth copies into a new array and swaps the pointer; the old array stays readable until a safe
/// point ([`KeyMap::reclaim`]) or drop.
pub struct KeyMap {
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
    #[allow(clippy::vec_box)]
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
    /// The slot holding `key`, or the empty slot where the probe ended.
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

impl Default for KeyMap {
    fn default() -> Self {
        KeyMap::new()
    }
}

impl KeyMap {
    pub fn new() -> KeyMap {
        KeyMap { cur: AtomicPtr::new(null_mut()), w: Mutex::new(KmWriter::default()) }
    }

    #[inline]
    pub fn get(&self, key: u64) -> Option<u32> {
        let p = self.cur.load(Acquire);
        if p.is_null() {
            return None;
        }
        // SAFETY: arrays are freed only by `reclaim` (`&mut self`) or drop.
        let s = unsafe { &*p };
        s.find(key).ok().map(|i| s.vals[i].load(Acquire))
    }

    /// Insert or update `key -> val`.
    pub fn set(&self, key: u64, val: u32) {
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

    /// Free arrays replaced by growth (a safe point: no reader can hold them).
    pub fn reclaim(&mut self) -> usize {
        let w = self.w.get_mut();
        let n = w.retired.len();
        w.retired.clear();
        n
    }

    pub fn len(&self) -> usize {
        self.w.lock().count
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
