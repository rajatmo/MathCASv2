//! A global allocator that keeps freed small blocks for reuse: [`CachingAlloc`].
//!
//! Evaluation allocates and frees very many small blocks (vectors of arguments, polynomial terms, big-integer limbs,
//! strings). The operating system's general heap takes a lock and searches its free structures for every one of them;
//! on Windows that was a large part of the time of short evaluations (a third to two thirds of it in measurements of
//! `together`, `series`, `integrate` and numerical special functions). This allocator sits on top of the system
//! allocator: a block of at most [`MAX_SMALL`] bytes (and alignment at most 16) is rounded up to one of a few size
//! classes, and when it is freed it is kept on a free list of its class for the next request of that class instead of
//! going back to the system.
//!
//! The free lists live in [`SLOTS`] caches. A thread takes a cache when it first allocates (round robin) and keeps it,
//! so the threads of an evaluation normally each have their own, and a thread that ends leaves its cache, with the
//! blocks in it, to the threads that come after it (sessions make new worker threads; nothing is lost). Each cache is
//! guarded by a lock that is only tried: a thread that finds it taken (two threads sharing a cache) takes the block
//! from the system allocator, or gives it back there, instead of waiting. A cache keeps at most [`SLOT_BYTES`] of
//! freed blocks; past that they go back to the system, so at most `SLOTS * SLOT_BYTES` bytes are held back. Larger
//! blocks and larger alignments go to the system allocator unchanged.
//!
//! Nothing here allocates, so the allocator never re-enters itself; the only thread-local state is the number of the
//! thread's cache (a plain integer, no destructor). Every block a class hands out was allocated from the system with
//! that class's size and alignment 16, and goes back to the system with the same layout.
//!
//! A binary installs it with
//! ```ignore
//! #[global_allocator]
//! static ALLOC: mc_core::alloc::CachingAlloc = mc_core::alloc::CachingAlloc;
//! ```
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, UnsafeCell};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The largest block served from the size classes, in bytes.
pub const MAX_SMALL: usize = 1024;
/// The alignment of every block of the size classes.
const ALIGN: usize = 16;
/// Classes of 16, 32, ..., 256 bytes, then of 320, 384, ..., 1024 bytes.
const CLASSES: usize = 16 + 12;
/// The number of caches.
pub const SLOTS: usize = 16;
/// The most bytes of freed blocks one cache keeps.
pub const SLOT_BYTES: usize = 4 << 20;

/// The class of a request of `size` bytes (`1..=MAX_SMALL`).
#[inline]
fn class_of(size: usize) -> usize {
    if size <= 256 {
        size.max(1).div_ceil(16) - 1
    } else {
        16 + (size - 257) / 64
    }
}

/// The block size of class `c`.
#[inline]
const fn class_size(c: usize) -> usize {
    if c < 16 {
        (c + 1) * 16
    } else {
        256 + (c - 15) * 64
    }
}

/// A freed block, linked through its first word.
struct Node {
    next: *mut Node,
}

/// One cache's free lists, by class, and the bytes they hold.
struct Lists {
    heads: [*mut Node; CLASSES],
    bytes: usize,
}

/// A cache: its lists behind a lock that is only tried. Aligned so that two caches never share a cache line.
#[repr(align(128))]
struct Slot {
    lock: AtomicBool,
    lists: UnsafeCell<Lists>,
}

// SAFETY: `lists` is only read or written while `lock` is held (see `try_with`).
unsafe impl Sync for Slot {}

impl Slot {
    const fn new() -> Slot {
        Slot { lock: AtomicBool::new(false), lists: UnsafeCell::new(Lists { heads: [std::ptr::null_mut(); CLASSES], bytes: 0 }) }
    }

    /// Run `f` on the lists if the lock is free; `None` when another thread holds it.
    #[inline]
    fn try_with<R>(&self, f: impl FnOnce(&mut Lists) -> R) -> Option<R> {
        if self.lock.load(Ordering::Relaxed) || self.lock.swap(true, Ordering::Acquire) {
            return None;
        }
        // SAFETY: the lock is held, so no other thread reads or writes the lists until it is released below.
        let r = f(unsafe { &mut *self.lists.get() });
        self.lock.store(false, Ordering::Release);
        Some(r)
    }
}

static CACHES: [Slot; SLOTS] = [const { Slot::new() }; SLOTS];
/// The next cache to hand to a thread.
static NEXT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// The thread's cache, `usize::MAX` before its first allocation.
    static MINE: Cell<usize> = const { Cell::new(usize::MAX) };
}

/// The calling thread's cache (`None` while thread-local storage is gone, at the very end of a thread).
#[inline]
fn my_slot() -> Option<&'static Slot> {
    MINE.try_with(|m| {
        let mut k = m.get();
        if k == usize::MAX {
            k = NEXT.fetch_add(1, Ordering::Relaxed) % SLOTS;
            m.set(k);
        }
        &CACHES[k]
    })
    .ok()
}

/// The system allocator with freed small blocks kept for reuse (see the module documentation).
pub struct CachingAlloc;

/// The layout a block of class `c` has towards the system allocator.
#[inline]
fn system_layout(c: usize) -> Layout {
    // SAFETY: the size is a positive multiple of 16 and the alignment a power of two.
    unsafe { Layout::from_size_align_unchecked(class_size(c), ALIGN) }
}

#[inline]
fn small(layout: &Layout) -> bool {
    layout.size() <= MAX_SMALL && layout.align() <= ALIGN
}

// SAFETY: a small block is either taken from a free list of its class (a block allocated from the system with the
// class's layout and freed by the program, so no longer used by it) or allocated from the system with that layout; its
// size is at least the size asked for and its alignment 16. It is given back to a list of the same class (the class
// is worked out from the layout the program passes, the one it allocated with) or to the system with the class's
// layout. Other blocks go to the system allocator unchanged.
unsafe impl GlobalAlloc for CachingAlloc {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !small(&layout) {
            return System.alloc(layout);
        }
        let c = class_of(layout.size());
        let taken = my_slot().and_then(|s| {
            s.try_with(|l| {
                let head = l.heads[c];
                if !head.is_null() {
                    // SAFETY: a node on a list is a freed block of this class, at least 16 bytes, written by `dealloc`.
                    l.heads[c] = unsafe { (*head).next };
                    l.bytes -= class_size(c);
                }
                head
            })
        });
        match taken {
            Some(p) if !p.is_null() => p as *mut u8,
            _ => System.alloc(system_layout(c)),
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !small(&layout) {
            return System.alloc_zeroed(layout);
        }
        let p = self.alloc(layout);
        if !p.is_null() {
            std::ptr::write_bytes(p, 0, layout.size());
        }
        p
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !small(&layout) {
            return System.dealloc(ptr, layout);
        }
        let c = class_of(layout.size());
        let size = class_size(c);
        let node = ptr as *mut Node;
        let kept = my_slot().and_then(|s| {
            s.try_with(|l| {
                if l.bytes + size > SLOT_BYTES {
                    return false;
                }
                // SAFETY: the block is the program's no longer; it is at least 16 bytes and 16-aligned.
                unsafe { (*node).next = l.heads[c] };
                l.heads[c] = node;
                l.bytes += size;
                true
            })
        });
        if kept != Some(true) {
            System.dealloc(ptr, system_layout(c));
        }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let (old_small, new_small) = (small(&layout), small(&new_layout));
        if !old_small && !new_small {
            return System.realloc(ptr, layout, new_size);
        }
        if old_small && new_small && class_of(layout.size()) == class_of(new_size) {
            // the block is large enough already
            return ptr;
        }
        let new = self.alloc(new_layout);
        if !new.is_null() {
            std::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
            self.dealloc(ptr, layout);
        }
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_cover_every_small_size() {
        for size in 1..=MAX_SMALL {
            let c = class_of(size);
            assert!(c < CLASSES, "{size}");
            assert!(class_size(c) >= size, "{size}");
            if c > 0 {
                assert!(class_size(c - 1) < size, "{size}: a smaller class fits");
            }
            assert_eq!(class_size(c) % ALIGN, 0);
        }
        assert_eq!(class_size(CLASSES - 1), MAX_SMALL);
    }
}
