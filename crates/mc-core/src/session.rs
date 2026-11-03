//! Sessions: evaluations separated by safe points (I1.6).
//!
//! An evaluation is a closure that gets `&'h Heap` for a scope `'h` it cannot name outside (a higher-ranked
//! closure), so no branded handle can escape it: the result type is fixed outside the scope, and the session state
//! holds only [`Raw`](crate::Raw) handles in [`Trace`] containers. The whole evaluation runs on the session's thread
//! pool and may use scoped parallelism (`rayon::join`, parallel iterators) freely; detached tasks (`rayon::spawn`)
//! need `'static` closures, which cannot hold a branded handle. So every task of an evaluation has finished when the
//! closure returns, and the safe point that follows is a global barrier with no extra synchronisation.
use crate::roots::Trace;
use crate::{Config, Heap};
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::time::{Duration, Instant};

/// A heap, the session state that roots its terms, and the pool evaluations run on.
pub struct Session<St: Trace + Send> {
    /// the heap (between steps, `&mut` access is a safe point's)
    pub heap: Heap,
    /// the root set: everything the session keeps across steps
    pub state: St,
    pool: ThreadPool,
    /// time spent inside evaluations
    pub work_time: Duration,
    /// time spent in safe points
    pub safe_time: Duration,
    /// evaluations run
    pub steps: usize,
}

impl<St: Trace + Send> Session<St> {
    /// A session on a pool of `threads` workers (deep evaluations get 64 MB stacks).
    pub fn new(cfg: Config, threads: usize, state: St) -> Session<St> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads.max(1))
            .stack_size(64 << 20)
            .build()
            .expect("thread pool");
        Session { heap: Heap::new(cfg), state, pool, work_time: Duration::ZERO, safe_time: Duration::ZERO, steps: 0 }
    }

    /// Worker threads of the session's pool.
    pub fn threads(&self) -> usize {
        self.pool.current_num_threads()
    }

    /// One evaluation, then a safe point with the session state as the root set. Both run on the session's pool.
    pub fn step<R: Send>(&mut self, f: impl for<'h> FnOnce(&'h Heap, &mut St) -> R + Send) -> R {
        let Session { heap, state, pool, .. } = self;
        let (r, work, safe) = pool.install(|| {
            let t0 = Instant::now();
            let r = f(&*heap, &mut *state);
            let t1 = Instant::now();
            heap.safe_point(&mut [&mut *state]);
            (r, t1 - t0, t1.elapsed())
        });
        self.work_time += work;
        self.safe_time += safe;
        self.steps += 1;
        r
    }

    /// Read the heap and state without a safe point (inspection between evaluations), on the session's pool.
    pub fn read<R: Send>(&self, f: impl for<'h> FnOnce(&'h Heap, &St) -> R + Send) -> R
    where
        St: Sync,
    {
        let Session { heap, state, pool, .. } = self;
        pool.install(|| f(heap, state))
    }

    /// A full collection: afterwards the heap holds exactly the terms the state reaches.
    pub fn collect_full(&mut self) {
        let Session { heap, state, pool, .. } = self;
        pool.install(|| heap.collect_full(&mut [&mut *state]));
    }

    /// Check the heap's invariants (see [`Heap::verify`]).
    pub fn verify(&mut self) {
        let Session { heap, state, .. } = self;
        heap.verify(&mut [&mut *state]);
    }
}
