//! Batches of calls of one compiled expression on several threads (item N4b, semantics 18.2 (2)): the engine
//! under `mc_numfn::par`. An [`ExprFn`] is pure (it never draws) and immutable, so its calls may run at once on the
//! threads of the session's pool; each point writes only its own outputs, so the values do not depend on which
//! thread computed them.
//!
//! **Scheduling** (decision N4b.5): the points are cut into blocks that the calling thread and up to `threads - 1`
//! tasks of the current rayon pool claim one at a time (uneven costs balance). The calling thread never blocks:
//! when no block is left it charges the fuel of the blocks still running and yields until they end; a task that
//! starts after the blocks are gone returns at once.
//!
//! **Fuel and stops** (decision N4b.6): the loops' polls on other threads add their fuel to a shared counter; the
//! calling thread hands it to `charge` (in its own polls and while it waits), which charges the evaluation and
//! returns `false` to stop the batch. A stop raises the batch's stop word, which every compiled poll reads, and the
//! points not completed are reported as such; so is every point whose call declined. The caller evaluates those
//! with its sequential backend.
use crate::expr::{Call, ExprFn};
use crate::rt::PollFn;
use mc_eval::Control;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;

/// What one batch did.
#[derive(Clone, Debug)]
pub struct Batch {
    /// per point: whether the compiled call completed (its outputs are written)
    pub done: Vec<bool>,
    /// whether `charge` stopped the batch
    pub stopped: bool,
    /// the threads that took part (the calling one included)
    pub threads: usize,
}

/// The state shared by the threads of one batch.
struct Shared<'a> {
    code: &'a ExprFn,
    ctl: &'a Control,
    args: &'a [f64],
    ni: usize,
    no: usize,
    block: usize,
    nblocks: usize,
    next: AtomicUsize,
    active: AtomicUsize,
    fuel: AtomicU64,
    abort: AtomicBool,
    interrupt: &'a AtomicBool,
    status: &'a [AtomicU8],
    outs: &'a [Mutex<&'a mut [f64]>],
}

impl Shared<'_> {
    fn halted(&self) -> bool {
        self.abort.load(Relaxed) || self.interrupt.load(Relaxed)
    }

    fn stop(&self) {
        self.abort.store(true, Relaxed);
        self.ctl.stop.store(1, Relaxed);
    }

    /// Claim and compute blocks until none is left or the batch stops.
    fn work(&self, poll: &mut PollFn<'_>) {
        loop {
            // counted active before the claim, so that the waiting thread never misses a block in progress
            self.active.fetch_add(1, Relaxed);
            let b = self.next.fetch_add(1, Relaxed);
            if b >= self.nblocks || self.halted() {
                self.active.fetch_sub(1, Relaxed);
                return;
            }
            {
                let mut out = self.outs[b].lock().unwrap_or_else(|e| e.into_inner());
                for (i, o) in out.chunks_mut(self.no).enumerate() {
                    if self.halted() {
                        break;
                    }
                    let k = b * self.block + i;
                    let r = self.code.call_f64_detached(self.ctl, &self.args[k * self.ni..(k + 1) * self.ni], o, poll);
                    self.status[k].store((r == Call::Done) as u8, Relaxed);
                }
            }
            self.active.fetch_sub(1, Relaxed);
        }
    }
}

/// The threads of the current rayon pool (the session's, inside an evaluation).
pub fn pool_threads() -> usize {
    rayon::current_num_threads()
}

/// Evaluate `code` at the points in `args` (each `code.inputs()` machine reals) into `out` (each `code.outputs()`)
/// on up to `threads` threads of the current rayon pool. `ctl` is the batch's control block (its stop word is
/// cleared first and raised on a stop); `interrupt` is read by every thread before each point; `charge(k)` charges
/// `k` units of fuel to the evaluation on the calling thread and returns `false` to stop. The caller must have
/// checked [`ExprFn::current`]; the lengths must fit (else nothing is done).
pub fn run(
    code: &ExprFn,
    ctl: &Control,
    args: &[f64],
    out: &mut [f64],
    threads: usize,
    interrupt: &AtomicBool,
    charge: &mut dyn FnMut(u64) -> bool,
) -> Batch {
    let (ni, no) = (code.inputs(), code.outputs());
    let n = out.len().checked_div(no).unwrap_or(0);
    if no == 0 || out.len() != n * no || args.len() != n * ni {
        return Batch { done: vec![false; n], stopped: false, threads: 0 };
    }
    let threads = threads.max(1);
    let block = (n / (threads * 16)).max(16);
    let nblocks = n.div_ceil(block);
    let status: Vec<AtomicU8> = (0..n).map(|_| AtomicU8::new(0)).collect();
    let outs: Vec<Mutex<&mut [f64]>> = out.chunks_mut(block * no).map(Mutex::new).collect();
    ctl.stop.store(0, Relaxed);
    let sh = Shared {
        code,
        ctl,
        args,
        ni,
        no,
        block,
        nblocks,
        next: AtomicUsize::new(0),
        active: AtomicUsize::new(0),
        fuel: AtomicU64::new(0),
        abort: AtomicBool::new(false),
        interrupt,
        status: &status,
        outs: &outs,
    };
    let mut stopped = false;
    let helpers = threads.min(nblocks).saturating_sub(1);
    rayon::in_place_scope(|s| {
        for _ in 0..helpers {
            let sh = &sh;
            s.spawn(move |_| {
                let mut poll = |k: u64| {
                    sh.fuel.fetch_add(k, Relaxed);
                    !sh.halted()
                };
                sh.work(&mut poll);
            });
        }
        // the calling thread's charges: its own fuel and what the others spent since
        let mut charge_now = |k: u64, stopped: &mut bool| {
            let k = k + sh.fuel.swap(0, Relaxed);
            if !*stopped && !charge(k) {
                *stopped = true;
                sh.stop();
            }
            !*stopped
        };
        {
            let mut poll = |k: u64| charge_now(k, &mut stopped) && !sh.halted();
            sh.work(&mut poll);
        }
        while sh.active.load(Relaxed) > 0 {
            charge_now(0, &mut stopped);
            std::thread::yield_now();
        }
        charge_now(0, &mut stopped);
    });
    let done = status.iter().map(|s| s.load(Relaxed) == 1).collect();
    Batch { done, stopped, threads: helpers + 1 }
}
