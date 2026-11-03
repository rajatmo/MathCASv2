//! Lazily allocated, zero-initialised segments behind a directory of atomic pointers: an array indexed by a large id
//! space, of which only the touched segments exist. Readers never lock; a segment is installed by compare-exchange
//! and never moves until `clear` (which needs `&mut`, so no reader exists). Used for symbol hashes and the young memo.
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering::*};

/// Element types of [`ZeroSegs`].
///
/// # Safety
/// The all-zero bit pattern must be a valid value, and the type must have no drop glue.
pub(crate) unsafe trait Zeroable: Sync {}

// SAFETY: an atomic integer of zero is valid and has no drop glue.
unsafe impl Zeroable for AtomicU64 {}

pub(crate) struct ZeroSegs<E: Zeroable> {
    seg_bits: u32,
    dir: Box<[AtomicPtr<E>]>,
}

impl<E: Zeroable> ZeroSegs<E> {
    /// Room for indices below `2^index_bits`, in segments of `2^seg_bits` elements.
    pub(crate) fn new(seg_bits: u32, index_bits: u32) -> Self {
        ZeroSegs { seg_bits, dir: (0..1usize << (index_bits - seg_bits)).map(|_| AtomicPtr::new(null_mut())).collect() }
    }

    fn layout(&self) -> Layout {
        Layout::array::<E>(1 << self.seg_bits).expect("segment layout")
    }

    #[inline]
    fn slot(&self, p: *mut E, i: usize) -> &E {
        // SAFETY: `p` is an installed segment of 2^seg_bits elements, live until `clear` (which needs `&mut self`).
        unsafe { &*p.add(i & ((1 << self.seg_bits) - 1)) }
    }

    /// The element at `i`, if its segment exists (absent segments read as all zero).
    #[inline]
    pub(crate) fn get(&self, i: usize) -> Option<&E> {
        let p = self.dir.get(i >> self.seg_bits)?.load(Acquire);
        (!p.is_null()).then(|| self.slot(p, i))
    }

    /// The element at `i`, allocating its segment (zeroed) on first use. Panics beyond the index space.
    pub(crate) fn get_or_alloc(&self, i: usize) -> &E {
        let d = &self.dir[i >> self.seg_bits];
        let mut p = d.load(Acquire);
        if p.is_null() {
            let layout = self.layout();
            // SAFETY: a nonzero-size layout; all-zero bytes are a valid `E` (the `Zeroable` contract).
            let q = unsafe { alloc_zeroed(layout) } as *mut E;
            assert!(!q.is_null(), "segment allocation failed");
            match d.compare_exchange(null_mut(), q, AcqRel, Acquire) {
                Ok(_) => p = q,
                Err(cur) => {
                    // SAFETY: `q` was allocated above with this layout and never published.
                    unsafe { dealloc(q as *mut u8, layout) };
                    p = cur;
                }
            }
        }
        self.slot(p, i)
    }

    /// Free every segment (every element reads as zero again).
    pub(crate) fn clear(&mut self) {
        let layout = self.layout();
        for d in self.dir.iter_mut() {
            let p = std::mem::replace(d.get_mut(), null_mut());
            if !p.is_null() {
                // SAFETY: allocated by `get_or_alloc` with this layout; `&mut self` excludes readers.
                unsafe { dealloc(p as *mut u8, layout) };
            }
        }
    }

    /// Bytes held by installed segments.
    pub(crate) fn bytes(&self) -> usize {
        self.dir.iter().filter(|d| !d.load(Relaxed).is_null()).count() * self.layout().size()
    }
}

impl<E: Zeroable> Drop for ZeroSegs<E> {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_are_lazy_and_zeroed() {
        let mut s: ZeroSegs<AtomicU64> = ZeroSegs::new(4, 12);
        assert!(s.get(100).is_none());
        s.get_or_alloc(100).store(7, Relaxed);
        assert_eq!(s.get(100).unwrap().load(Relaxed), 7);
        assert_eq!(s.get(101).unwrap().load(Relaxed), 0);
        assert!(s.get(1 << 12).is_none(), "beyond the index space reads as absent");
        assert_eq!(s.bytes(), 16 * 8);
        s.clear();
        assert!(s.get(100).is_none());
    }
}
