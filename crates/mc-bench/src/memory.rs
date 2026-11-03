//! Peak memory of the measured evaluation above the session's baseline (02-benchmarks section 1).
//!
//! The operating system's peak working set cannot be reset between repetitions, so the child measures heap bytes
//! with a counting global allocator instead (decision D.5). The benchmark binary installs it:
//!
//! ```ignore
//! #[global_allocator]
//! static ALLOC: mc_bench::memory::CountingAlloc = mc_bench::memory::CountingAlloc;
//! ```
//!
//! Memory an engine maps directly from the operating system bypasses the counter; the guard's process peak
//! (`proc_peak` in results) covers that case.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The system allocator with a running count of live bytes and their peak.
pub struct CountingAlloc;

impl CountingAlloc {
    fn grew(size: usize) {
        let now = CURRENT.fetch_add(size, Ordering::Relaxed) + size;
        PEAK.fetch_max(now, Ordering::Relaxed);
    }
    fn shrank(size: usize) {
        CURRENT.fetch_sub(size, Ordering::Relaxed);
    }
}

// SAFETY: every method forwards to `System` with the caller's arguments and only adds bookkeeping on success.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        INSTALLED.store(true, Ordering::Relaxed);
        let p = System.alloc(layout);
        if !p.is_null() {
            CountingAlloc::grew(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        INSTALLED.store(true, Ordering::Relaxed);
        let p = System.alloc_zeroed(layout);
        if !p.is_null() {
            CountingAlloc::grew(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        CountingAlloc::shrank(layout.size());
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            if new_size >= layout.size() {
                CountingAlloc::grew(new_size - layout.size());
            } else {
                CountingAlloc::shrank(layout.size() - new_size);
            }
        }
        p
    }
}

/// A peak measurement in progress: started before an evaluation, read after it.
pub struct PeakProbe {
    baseline: usize,
}

impl PeakProbe {
    /// Reset the peak to the current live bytes. Returns `None` when the counting allocator is not installed.
    pub fn start() -> Option<PeakProbe> {
        if !INSTALLED.load(Ordering::Relaxed) {
            return None;
        }
        let baseline = CURRENT.load(Ordering::Relaxed);
        PEAK.store(baseline, Ordering::Relaxed);
        Some(PeakProbe { baseline })
    }
    /// Peak live bytes since `start`, above the baseline.
    pub fn peak_above_baseline(&self) -> u64 {
        PEAK.load(Ordering::Relaxed).saturating_sub(self.baseline) as u64
    }
}
